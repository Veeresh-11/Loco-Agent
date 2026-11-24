#include <sys/attr.h>
#include <sys/stat.h>
#include <sys/param.h>
#include <fcntl.h>
#include <stdint.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <errno.h>

static void dump_record(const unsigned char *record, size_t record_len) {
    uint32_t declared_len = 0;
    attribute_set_t returned;
    memcpy(&declared_len, record, sizeof(declared_len));
    if (declared_len != record_len || record_len < sizeof(uint32_t) + sizeof(attribute_set_t)) {
        fprintf(stderr, "invalid record length: declared=%u available=%zu\n", declared_len, record_len);
        exit(2);
    }

    memcpy(&returned, record + sizeof(uint32_t), sizeof(returned));
    size_t offset = sizeof(uint32_t) + sizeof(returned);
    uint32_t entry_error = 0;
    if (returned.commonattr & ATTR_CMN_ERROR) {
        if (offset + sizeof(entry_error) > record_len) {
            fprintf(stderr, "truncated ATTR_CMN_ERROR\n");
            exit(2);
        }
        memcpy(&entry_error, record + offset, sizeof(entry_error));
        offset += sizeof(entry_error);
    }

    char name[NAME_MAX * 3 + 1];
    name[0] = '\0';
    if (returned.commonattr & ATTR_CMN_NAME) {
        attrreference_t name_ref;
        if (offset + sizeof(name_ref) > record_len) {
            fprintf(stderr, "truncated ATTR_CMN_NAME reference\n");
            exit(2);
        }
        memcpy(&name_ref, record + offset, sizeof(name_ref));
        int64_t name_start = (int64_t)offset + (int64_t)name_ref.attr_dataoffset;
        if (name_start < 0 || name_ref.attr_length == 0 ||
            (uint64_t)name_start + name_ref.attr_length > record_len ||
            name_ref.attr_length > sizeof(name)) {
            fprintf(stderr, "invalid name reference: offset=%lld length=%u record=%zu\n",
                    (long long)name_start, name_ref.attr_length, record_len);
            exit(2);
        }
        const unsigned char *name_bytes = record + (size_t)name_start;
        if (name_bytes[name_ref.attr_length - 1] != '\0') {
            fprintf(stderr, "name is not NUL terminated\n");
            exit(2);
        }
        memcpy(name, name_bytes, name_ref.attr_length);
        offset += sizeof(name_ref);
    }

    uint32_t flags = 0;
    int has_flags = (returned.commonattr & ATTR_CMN_FLAGS) != 0;
    size_t flags_offset = offset;
    if (has_flags) {
        if (offset + sizeof(flags) > record_len) {
            fprintf(stderr, "truncated ATTR_CMN_FLAGS\n");
            exit(2);
        }
        memcpy(&flags, record + offset, sizeof(flags));
    }

    if (strcmp(name, "Users") == 0 || strcmp(name, "Applications") == 0 ||
        strcmp(name, "Library") == 0 || strcmp(name, "System") == 0) {
        printf("entry=%s record_len=%u returned_common=0x%08x error=%u ",
               name, declared_len, returned.commonattr, entry_error);
        if (has_flags) {
            printf("flags=0x%08x flags_offset=%zu ", flags, flags_offset);
        } else {
            printf("flags=NOT_RETURNED ");
        }
        printf("raw=");
        size_t dump_len = record_len < 64 ? record_len : 64;
        for (size_t i = 0; i < dump_len; ++i) {
            printf("%02x", record[i]);
        }
        printf("\n");
    }
}


static int capture_descriptor_paths(int fd, const char *label,
                                      char visible_path[MAXPATHLEN],
                                      char nofirmlink_path[MAXPATHLEN]) {
    memset(visible_path, 0, MAXPATHLEN);
    if (fcntl(fd, F_GETPATH, visible_path) != 0 ||
        memchr(visible_path, '\0', MAXPATHLEN) == NULL) {
        printf("fcntl_getpath label=%s error=%d\n", label, errno);
        return 1;
    }
    printf("fcntl_getpath label=%s path=%s\n", label, visible_path);

#if defined(F_GETPATH_NOFIRMLINK)
    memset(nofirmlink_path, 0, MAXPATHLEN);
    if (fcntl(fd, F_GETPATH_NOFIRMLINK, nofirmlink_path) != 0 ||
        memchr(nofirmlink_path, '\0', MAXPATHLEN) == NULL) {
        printf("fcntl_getpath_nofirmlink label=%s error=%d\n", label, errno);
        return 1;
    }
    printf("fcntl_getpath_nofirmlink label=%s path=%s\n",
           label, nofirmlink_path);
    return 0;
#else
    (void)nofirmlink_path;
    printf("fcntl_getpath_nofirmlink label=%s unavailable_in_sdk\n", label);
    return 2;
#endif
}

static void print_descriptor_paths(int fd, const char *label) {
    char visible_path[MAXPATHLEN];
    char nofirmlink_path[MAXPATHLEN];
    (void)capture_descriptor_paths(fd, label, visible_path, nofirmlink_path);
}

static int has_suffix(const char *value, const char *suffix) {
    size_t value_len = strlen(value);
    size_t suffix_len = strlen(suffix);
    return value_len >= suffix_len &&
           strcmp(value + value_len - suffix_len, suffix) == 0;
}

static void print_path_api_flags(int rootfd, const char *path) {
    // Compare path-based stat flags with getattrlist results. Firmlinks are not
    // POSIX symlinks, so AT_SYMLINK_NOFOLLOW is diagnostic only, not a boundary.
    struct stat path_stat;
    if (fstatat(rootfd, path, &path_stat, AT_SYMLINK_NOFOLLOW) == 0) {
        printf("fstatat_nofollow path=%s st_flags=0x%08x st_dev=%llu st_ino=%llu\n",
               path, (unsigned int)path_stat.st_flags,
               (unsigned long long)path_stat.st_dev,
               (unsigned long long)path_stat.st_ino);
    } else {
        printf("fstatat_nofollow path=%s error=%d\n", path, errno);
    }
    if (fstatat(rootfd, path, &path_stat, 0) == 0) {
        printf("fstatat_follow path=%s st_flags=0x%08x st_dev=%llu st_ino=%llu\n",
               path, (unsigned int)path_stat.st_flags,
               (unsigned long long)path_stat.st_dev,
               (unsigned long long)path_stat.st_ino);
    } else {
        printf("fstatat_follow path=%s error=%d\n", path, errno);
    }

    struct attrlist attrs;
    memset(&attrs, 0, sizeof(attrs));
    attrs.bitmapcount = ATTR_BIT_MAP_COUNT;
    attrs.commonattr = ATTR_CMN_RETURNED_ATTRS | ATTR_CMN_FLAGS;

    unsigned char buffer[256] __attribute__((aligned(8)));
    int result = getattrlistat(rootfd, path, &attrs, buffer, sizeof(buffer),
                               FSOPT_NOFOLLOW_ANY | FSOPT_RESOLVE_BENEATH);
    if (result != 0) {
        printf("getattrlistat path=%s options=NOFOLLOW_ANY|RESOLVE_BENEATH error=%d\n",
               path, errno);
        return;
    }

    uint32_t length = 0;
    attribute_set_t returned;
    memcpy(&length, buffer, sizeof(length));
    if (length < sizeof(length) + sizeof(returned) ||
        length > sizeof(buffer)) {
        printf("getattrlistat path=%s malformed_length=%u\n", path, length);
        return;
    }
    memcpy(&returned, buffer + sizeof(length), sizeof(returned));
    if ((returned.commonattr & ATTR_CMN_FLAGS) == 0 ||
        length < sizeof(length) + sizeof(returned) + sizeof(uint32_t)) {
        printf("getattrlistat path=%s flags=NOT_RETURNED returned=0x%08x\n",
               path, returned.commonattr);
        return;
    }
    uint32_t flags = 0;
    memcpy(&flags, buffer + sizeof(length) + sizeof(returned), sizeof(flags));
    printf("getattrlistat path=%s options=NOFOLLOW_ANY|RESOLVE_BENEATH flags=0x%08x returned=0x%08x\n",
           path, flags, returned.commonattr);

    int childfd = openat(rootfd, path, O_RDONLY | O_DIRECTORY | O_CLOEXEC | O_NOFOLLOW);
    if (childfd < 0) {
        printf("openat path=%s O_NOFOLLOW error=%d\n", path, errno);
        return;
    }
    print_descriptor_paths(childfd, path);

    struct stat handle_stat;
    if (fstat(childfd, &handle_stat) == 0) {
        printf("fstat_handle path=%s st_flags=0x%08x st_dev=%llu st_ino=%llu\n",
               path, (unsigned int)handle_stat.st_flags,
               (unsigned long long)handle_stat.st_dev,
               (unsigned long long)handle_stat.st_ino);
    } else {
        printf("fstat_handle path=%s error=%d\n", path, errno);
    }

    memset(&attrs, 0, sizeof(attrs));
    attrs.bitmapcount = ATTR_BIT_MAP_COUNT;
    attrs.commonattr = ATTR_CMN_RETURNED_ATTRS | ATTR_CMN_FLAGS;
    memset(buffer, 0, sizeof(buffer));
    result = fgetattrlist(childfd, &attrs, buffer, sizeof(buffer), 0);
    if (result != 0) {
        printf("fgetattrlist path=%s error=%d\n", path, errno);
    } else {
        memcpy(&length, buffer, sizeof(length));
        if (length >= sizeof(length) + sizeof(returned) + sizeof(uint32_t) &&
            length <= sizeof(buffer)) {
            memcpy(&returned, buffer + sizeof(length), sizeof(returned));
            memcpy(&flags, buffer + sizeof(length) + sizeof(returned), sizeof(flags));
            printf("fgetattrlist path=%s flags=0x%08x returned=0x%08x\n",
                   path, flags, returned.commonattr);
        } else {
            printf("fgetattrlist path=%s malformed_length=%u\n", path, length);
        }
    }
    close(childfd);
}

static int probe_renamed_open_directory(void) {
    char root[] = "/tmp/loco-fcntl-rename-XXXXXX";
    if (mkdtemp(root) == NULL) {
        printf("rename_probe setup_error=%d\\n", errno);
        return 1;
    }

    char original[PATH_MAX];
    char moved[PATH_MAX];
    if (snprintf(original, sizeof(original), "%s/before", root) >= (int)sizeof(original) ||
        snprintf(moved, sizeof(moved), "%s/after", root) >= (int)sizeof(moved)) {
        printf("rename_probe path_overflow\\n");
        rmdir(root);
        return 1;
    }
    if (mkdir(original, 0700) != 0) {
        printf("rename_probe mkdir_error=%d\\n", errno);
        rmdir(root);
        return 1;
    }

    int fd = open(original, O_RDONLY | O_DIRECTORY | O_CLOEXEC | O_NOFOLLOW);
    if (fd < 0) {
        printf("rename_probe open_error=%d\\n", errno);
        rmdir(original);
        rmdir(root);
        return 1;
    }

    struct stat opened_before;
    if (fstat(fd, &opened_before) != 0) {
        printf("rename_probe fstat_before_error=%d\\n", errno);
        close(fd);
        rmdir(original);
        rmdir(root);
        return 1;
    }

    char visible_path[MAXPATHLEN];
    char nofirmlink_path[MAXPATHLEN];
    if (capture_descriptor_paths(fd, "rename_before", visible_path,
                                 nofirmlink_path) != 0 ||
        !has_suffix(visible_path, "/before") ||
        !has_suffix(nofirmlink_path, "/before")) {
        printf("rename_probe initial_path_mismatch\n");
        close(fd);
        rmdir(original);
        rmdir(root);
        return 1;
    }

    if (rename(original, moved) != 0) {
        printf("rename_probe rename_error=%d\\n", errno);
        close(fd);
        rmdir(original);
        rmdir(root);
        return 1;
    }
    if (mkdir(original, 0700) != 0) {
        printf("rename_probe replacement_mkdir_error=%d\\n", errno);
        close(fd);
        rmdir(moved);
        rmdir(root);
        return 1;
    }

    struct stat opened_after;
    if (fstat(fd, &opened_after) != 0) {
        printf("rename_probe fstat_after_error=%d\\n", errno);
        close(fd);
        rmdir(original);
        rmdir(moved);
        rmdir(root);
        return 1;
    }
    printf("rename_probe retained_identity_same=%s\\n",
           opened_before.st_dev == opened_after.st_dev &&
           opened_before.st_ino == opened_after.st_ino ? "yes" : "no");
    print_descriptor_paths(fd, "rename_after_replacement");

    int replacement_fd = open(original, O_RDONLY | O_DIRECTORY | O_CLOEXEC | O_NOFOLLOW);
    if (replacement_fd < 0) {
        printf("rename_probe replacement_open_error=%d\\n", errno);
        close(fd);
        rmdir(original);
        rmdir(moved);
        rmdir(root);
        return 1;
    }
    if (capture_descriptor_paths(replacement_fd, "replacement_at_old_name",
                                 visible_path, nofirmlink_path) != 0 ||
        !has_suffix(visible_path, "/before") ||
        !has_suffix(nofirmlink_path, "/before")) {
        printf("rename_probe replacement_path_mismatch\n");
        close(replacement_fd);
        close(fd);
        rmdir(original);
        rmdir(moved);
        rmdir(root);
        return 1;
    }

    struct stat replacement_stat;
    if (fstat(replacement_fd, &replacement_stat) != 0) {
        printf("rename_probe replacement_fstat_error=%d\\n", errno);
        close(replacement_fd);
        close(fd);
        rmdir(original);
        rmdir(moved);
        rmdir(root);
        return 1;
    }
    printf("rename_probe replacement_identity_distinct=%s\\n",
           opened_after.st_dev != replacement_stat.st_dev ||
           opened_after.st_ino != replacement_stat.st_ino ? "yes" : "no");

    close(replacement_fd);
    close(fd);
    if (rmdir(original) != 0 || rmdir(moved) != 0 || rmdir(root) != 0) {
        printf("rename_probe cleanup_error=%d\\n", errno);
        return 1;
    }
    return 0;
}

int main(void) {
    struct attrlist attrs;
    memset(&attrs, 0, sizeof(attrs));
    attrs.bitmapcount = ATTR_BIT_MAP_COUNT;
    attrs.commonattr = ATTR_CMN_RETURNED_ATTRS | ATTR_CMN_NAME |
                       ATTR_CMN_FLAGS | ATTR_CMN_ERROR;

    int dirfd = open("/", O_RDONLY | O_CLOEXEC);
    if (dirfd < 0) {
        perror("open(/)");
        return 1;
    }

    print_descriptor_paths(dirfd, "/");

    unsigned char buffer[64 * 1024] __attribute__((aligned(8)));
    for (;;) {
        int count = getattrlistbulk(dirfd, &attrs, buffer, sizeof(buffer), 0);
        if (count < 0) {
            perror("getattrlistbulk(/)");
            close(dirfd);
            return 1;
        }
        if (count == 0) {
            break;
        }

        size_t cursor = 0;
        for (int i = 0; i < count; ++i) {
            if (cursor + sizeof(uint32_t) > sizeof(buffer)) {
                fprintf(stderr, "record header outside buffer\n");
                close(dirfd);
                return 2;
            }
            uint32_t record_len = 0;
            memcpy(&record_len, buffer + cursor, sizeof(record_len));
            if (record_len < sizeof(uint32_t) + sizeof(attribute_set_t) ||
                record_len > sizeof(buffer) - cursor) {
                fprintf(stderr, "invalid bulk record length=%u remaining=%zu\n",
                        record_len, sizeof(buffer) - cursor);
                close(dirfd);
                return 2;
            }
            dump_record(buffer + cursor, record_len);
            cursor += record_len;
        }
    }

    print_path_api_flags(dirfd, "Users");
    print_path_api_flags(dirfd, "Applications");
    print_path_api_flags(dirfd, "Library");

    if (probe_renamed_open_directory() != 0) {
        close(dirfd);
        return 3;
    }

    close(dirfd);
    return 0;
}
