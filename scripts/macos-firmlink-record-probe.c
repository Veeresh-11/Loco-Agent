#include <sys/attr.h>
#include <sys/stat.h>
#include <sys/param.h>
#include <sys/mount.h>
#include <fcntl.h>
#include <stdint.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <errno.h>
#include <pthread.h>
#include <sched.h>

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


static int same_fsid(fsid_t left, fsid_t right) {
    return left.val[0] == right.val[0] && left.val[1] == right.val[1];
}

static void print_fsid(const char *label, fsid_t value) {
    printf("%s=%d:%d", label, value.val[0], value.val[1]);
}

/*
 * getattrlistbulk documents that a firmlink directory entry's attributes are
 * those of the entry, not the target. Compare that metadata with the parent
 * directory and the opened target descriptor to test whether FSID/PARENTID
 * can identify a firmlink before opening its target.
 */
static int probe_bulk_entry_identity(void) {
    int rootfd = open("/", O_RDONLY | O_DIRECTORY | O_CLOEXEC);
    if (rootfd < 0) {
        printf("bulk_identity root_open_error=%d\n", errno);
        return 1;
    }

    struct stat root_stat;
    struct statfs root_fs;
    if (fstat(rootfd, &root_stat) != 0 || fstatfs(rootfd, &root_fs) != 0) {
        printf("bulk_identity root_metadata_error=%d\n", errno);
        close(rootfd);
        return 1;
    }

    struct attrlist attrs;
    memset(&attrs, 0, sizeof(attrs));
    attrs.bitmapcount = ATTR_BIT_MAP_COUNT;
    attrs.commonattr = ATTR_CMN_RETURNED_ATTRS | ATTR_CMN_NAME |
                       ATTR_CMN_FSID | ATTR_CMN_FLAGS |
                       ATTR_CMN_FILEID | ATTR_CMN_PARENTID |
                       ATTR_CMN_ERROR;

    unsigned char buffer[64 * 1024] __attribute__((aligned(8)));
    for (;;) {
        int count = getattrlistbulk(rootfd, &attrs, buffer, sizeof(buffer), 0);
        if (count < 0) {
            printf("bulk_identity getattrlistbulk_error=%d\n", errno);
            close(rootfd);
            return 1;
        }
        if (count == 0) {
            break;
        }

        size_t cursor = 0;
        for (int i = 0; i < count; ++i) {
            if (cursor + sizeof(uint32_t) + sizeof(attribute_set_t) > sizeof(buffer)) {
                printf("bulk_identity record_header_out_of_bounds\n");
                close(rootfd);
                return 1;
            }

            uint32_t record_len = 0;
            memcpy(&record_len, buffer + cursor, sizeof(record_len));
            if (record_len < sizeof(uint32_t) + sizeof(attribute_set_t) ||
                record_len > sizeof(buffer) - cursor) {
                printf("bulk_identity invalid_record_length=%u\n", record_len);
                close(rootfd);
                return 1;
            }

            const unsigned char *record = buffer + cursor;
            attribute_set_t returned;
            memcpy(&returned, record + sizeof(record_len), sizeof(returned));
            size_t offset = sizeof(record_len) + sizeof(returned);
            uint32_t entry_error = 0;
            if (returned.commonattr & ATTR_CMN_ERROR) {
                if (offset + sizeof(entry_error) > record_len) {
                    printf("bulk_identity truncated_error\n");
                    close(rootfd);
                    return 1;
                }
                memcpy(&entry_error, record + offset, sizeof(entry_error));
                offset += sizeof(entry_error);
            }
            if (entry_error != 0) {
                cursor += record_len;
                continue;
            }

            if (!(returned.commonattr & ATTR_CMN_NAME)) {
                printf("bulk_identity missing_name_attr\n");
                close(rootfd);
                return 1;
            }
            attrreference_t name_ref;
            if (offset + sizeof(name_ref) > record_len) {
                printf("bulk_identity truncated_name_ref\n");
                close(rootfd);
                return 1;
            }
            memcpy(&name_ref, record + offset, sizeof(name_ref));
            int64_t name_start = (int64_t)offset + (int64_t)name_ref.attr_dataoffset;
            if (name_start < 0 || name_ref.attr_length == 0 ||
                (uint64_t)name_start + name_ref.attr_length > record_len ||
                name_ref.attr_length > NAME_MAX + 1) {
                printf("bulk_identity invalid_name_ref\n");
                close(rootfd);
                return 1;
            }
            char name[NAME_MAX + 1];
            memcpy(name, record + (size_t)name_start, name_ref.attr_length);
            if (name[name_ref.attr_length - 1] != '\0') {
                printf("bulk_identity nonterminated_name\n");
                close(rootfd);
                return 1;
            }
            offset += sizeof(name_ref);

            fsid_t entry_fsid;
            uint32_t flags = 0;
            uint64_t fileid = 0;
            uint64_t parentid = 0;
            if (!(returned.commonattr & ATTR_CMN_FSID) ||
                !(returned.commonattr & ATTR_CMN_FLAGS) ||
                !(returned.commonattr & ATTR_CMN_FILEID) ||
                !(returned.commonattr & ATTR_CMN_PARENTID) ||
                offset + sizeof(entry_fsid) + sizeof(flags) +
                        sizeof(fileid) + sizeof(parentid) > record_len) {
                printf("bulk_identity missing_or_truncated_metadata name=%s\n", name);
                close(rootfd);
                return 1;
            }
            memcpy(&entry_fsid, record + offset, sizeof(entry_fsid));
            offset += sizeof(entry_fsid);
            memcpy(&flags, record + offset, sizeof(flags));
            offset += sizeof(flags);
            memcpy(&fileid, record + offset, sizeof(fileid));
            offset += sizeof(fileid);
            memcpy(&parentid, record + offset, sizeof(parentid));

            if (strcmp(name, "Users") == 0 ||
                strcmp(name, "Applications") == 0 ||
                strcmp(name, "Library") == 0) {
                int childfd = openat(rootfd, name,
                                     O_RDONLY | O_DIRECTORY | O_CLOEXEC | O_NOFOLLOW);
                if (childfd < 0) {
                    printf("bulk_identity name=%s open_target_error=%d\n", name, errno);
                    close(rootfd);
                    return 1;
                }
                struct stat child_stat;
                struct statfs child_fs;
                if (fstat(childfd, &child_stat) != 0 ||
                    fstatfs(childfd, &child_fs) != 0) {
                    printf("bulk_identity name=%s target_metadata_error=%d\n", name, errno);
                    close(childfd);
                    close(rootfd);
                    return 1;
                }

                printf("bulk_identity name=%s root_ino=%llu entry_fileid=%llu "
                       "entry_parentid=%llu target_ino=%llu flags=0x%08x ",
                       name, (unsigned long long)root_stat.st_ino,
                       (unsigned long long)fileid,
                       (unsigned long long)parentid,
                       (unsigned long long)child_stat.st_ino, flags);
                print_fsid("root_fsid", root_fs.f_fsid);
                printf(" ");
                print_fsid("entry_fsid", entry_fsid);
                printf(" ");
                print_fsid("target_fsid", child_fs.f_fsid);
                printf(" parent_matches_root=%s entry_fsid_matches_root=%s "
                       "entry_fsid_matches_target=%s\n",
                       parentid == (uint64_t)root_stat.st_ino ? "yes" : "no",
                       same_fsid(entry_fsid, root_fs.f_fsid) ? "yes" : "no",
                       same_fsid(entry_fsid, child_fs.f_fsid) ? "yes" : "no");
                close(childfd);
            }

            cursor += record_len;
        }
    }

    close(rootfd);
    return 0;
}

#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
static int probe_extended_nofirmlink_path_at(int rootfd, const char *path) {
    struct attrlist attrs;
    memset(&attrs, 0, sizeof(attrs));
    attrs.bitmapcount = ATTR_BIT_MAP_COUNT;
    attrs.commonattr = ATTR_CMN_RETURNED_ATTRS | ATTR_CMN_FLAGS;
    attrs.forkattr = ATTR_CMNEXT_NOFIRMLINKPATH;

    unsigned char buffer[PATH_MAX + 512] __attribute__((aligned(8)));
    memset(buffer, 0, sizeof(buffer));
    if (getattrlistat(rootfd, path, &attrs, buffer, sizeof(buffer),
                      FSOPT_ATTR_CMN_EXTENDED | FSOPT_RESOLVE_BENEATH) != 0) {
        printf("extended_nofirmlink path=%s error=%d\n", path, errno);
        return 1;
    }

    uint32_t record_len = 0;
    attribute_set_t returned;
    memcpy(&record_len, buffer, sizeof(record_len));
    memcpy(&returned, buffer + sizeof(record_len), sizeof(returned));
    size_t offset = sizeof(record_len) + sizeof(returned);
    if (!(returned.commonattr & ATTR_CMN_FLAGS) ||
        !(returned.forkattr & ATTR_CMNEXT_NOFIRMLINKPATH) ||
        record_len < offset + sizeof(uint32_t) + sizeof(attrreference_t) ||
        record_len > sizeof(buffer)) {
        printf("extended_nofirmlink path=%s missing_attrs returned_common=0x%08x "
               "returned_extended=0x%08x length=%u\n",
               path, returned.commonattr, returned.forkattr, record_len);
        return 1;
    }

    uint32_t flags = 0;
    memcpy(&flags, buffer + offset, sizeof(flags));
    offset += sizeof(flags);
    attrreference_t path_ref;
    memcpy(&path_ref, buffer + offset, sizeof(path_ref));
    int64_t path_start = (int64_t)offset + (int64_t)path_ref.attr_dataoffset;
    if (path_ref.attr_length == 0 ||
        path_start < 0 ||
        (uint64_t)path_start + path_ref.attr_length > record_len ||
        path_ref.attr_length > PATH_MAX ||
        buffer[path_start + path_ref.attr_length - 1] != '\0') {
        printf("extended_nofirmlink path=%s invalid_path_ref\n", path);
        return 1;
    }

    printf("extended_nofirmlink path=%s flags=0x%08x resolved_path=%s "
           "path_bytes=%u\n",
           path, flags, (char *)(buffer + (size_t)path_start),
           path_ref.attr_length);
    return 0;
}

static int probe_getdirentriesattr_firmlink_flags(void) {
    int dirfd = open("/", O_RDONLY | O_DIRECTORY | O_CLOEXEC);
    if (dirfd < 0) {
        printf("getdirentriesattr root_open_error=%d\n", errno);
        return 1;
    }

    struct attrlist attrs;
    memset(&attrs, 0, sizeof(attrs));
    attrs.bitmapcount = ATTR_BIT_MAP_COUNT;
    attrs.commonattr = ATTR_CMN_RETURNED_ATTRS | ATTR_CMN_NAME | ATTR_CMN_FLAGS;

    unsigned char buffer[64 * 1024] __attribute__((aligned(8)));
    unsigned int basep = 0;
    unsigned int state = 0;
    unsigned int count = 128;
    int done = 0;
    int found = 0;
    do {
        count = 128;
        done = getdirentriesattr(dirfd, &attrs, buffer, sizeof(buffer),
                                 &count, &basep, &state, 0);
        if (done < 0) {
            int error = errno;
            printf("getdirentriesattr error=%d%s\n", error,
                   error == ENOTSUP ? " unsupported_on_volume" : "");
            close(dirfd);
            return error == ENOTSUP ? 0 : 1;
        }

        size_t cursor = 0;
        for (unsigned int i = 0; i < count; ++i) {
            if (cursor + sizeof(uint32_t) + sizeof(attribute_set_t) > sizeof(buffer)) {
                printf("getdirentriesattr record_header_out_of_bounds\n");
                close(dirfd);
                return 1;
            }
            uint32_t record_len = 0;
            memcpy(&record_len, buffer + cursor, sizeof(record_len));
            if (record_len < sizeof(uint32_t) + sizeof(attribute_set_t) ||
                record_len > sizeof(buffer) - cursor) {
                printf("getdirentriesattr invalid_record_length=%u\n", record_len);
                close(dirfd);
                return 1;
            }

            const unsigned char *record = buffer + cursor;
            attribute_set_t returned;
            memcpy(&returned, record + sizeof(record_len), sizeof(returned));
            size_t offset = sizeof(record_len) + sizeof(returned);
            if (!(returned.commonattr & ATTR_CMN_NAME) ||
                !(returned.commonattr & ATTR_CMN_FLAGS) ||
                offset + sizeof(attrreference_t) + sizeof(uint32_t) > record_len) {
                printf("getdirentriesattr required_attrs_missing returned=0x%08x\n",
                       returned.commonattr);
                close(dirfd);
                return 1;
            }

            attrreference_t name_ref;
            memcpy(&name_ref, record + offset, sizeof(name_ref));
            int64_t name_start = (int64_t)offset + (int64_t)name_ref.attr_dataoffset;
            if (name_start < 0 || name_ref.attr_length == 0 ||
                (uint64_t)name_start + name_ref.attr_length > record_len ||
                name_ref.attr_length > NAME_MAX + 1) {
                printf("getdirentriesattr invalid_name_ref\n");
                close(dirfd);
                return 1;
            }
            char name[NAME_MAX + 1];
            memcpy(name, record + (size_t)name_start, name_ref.attr_length);
            if (name[name_ref.attr_length - 1] != '\0') {
                printf("getdirentriesattr nonterminated_name\n");
                close(dirfd);
                return 1;
            }

            uint32_t flags = 0;
            memcpy(&flags, record + offset + sizeof(name_ref), sizeof(flags));
            if (strcmp(name, "Users") == 0 ||
                strcmp(name, "Applications") == 0 ||
                strcmp(name, "Library") == 0) {
                printf("getdirentriesattr name=%s flags=0x%08x firmlink=%s\n",
                       name, flags,
                       (flags & SF_FIRMLINK) != 0 ? "yes" : "no");
                found++;
            }
            cursor += record_len;
        }
    } while (!done && count > 0);

    printf("getdirentriesattr completed=%s mapped_entries_found=%d\n",
           done ? "yes" : "no", found);
    close(dirfd);
    return found == 3 ? 0 : 1;
}
#pragma clang diagnostic pop

static int capture_descriptor_paths(int fd, const char *label,
                                      char visible_path[MAXPATHLEN],
                                      char nofirmlink_path[MAXPATHLEN]) {
    memset(visible_path, 0, MAXPATHLEN);
    if (fcntl(fd, F_GETPATH, visible_path) != 0 ||
        memchr(visible_path, '\0', MAXPATHLEN) == NULL) {
        if (label != NULL) printf("fcntl_getpath label=%s error=%d\n", label, errno);
        return 1;
    }
    if (label != NULL) printf("fcntl_getpath label=%s path=%s\n", label, visible_path);

#if defined(F_GETPATH_NOFIRMLINK)
    memset(nofirmlink_path, 0, MAXPATHLEN);
    if (fcntl(fd, F_GETPATH_NOFIRMLINK, nofirmlink_path) != 0 ||
        memchr(nofirmlink_path, '\0', MAXPATHLEN) == NULL) {
        if (label != NULL) printf("fcntl_getpath_nofirmlink label=%s error=%d\n", label, errno);
        return 1;
    }
    if (label != NULL) printf("fcntl_getpath_nofirmlink label=%s path=%s\n",
                              label, nofirmlink_path);
    return 0;
#else
    (void)nofirmlink_path;
    if (label != NULL) printf("fcntl_getpath_nofirmlink label=%s unavailable_in_sdk\n", label);
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

static int same_fsid(fsid_t left, fsid_t right);
static void print_fsid(const char *label, fsid_t value);

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
    attrs.commonattr = ATTR_CMN_RETURNED_ATTRS | ATTR_CMN_FSID | ATTR_CMN_FLAGS |
                       ATTR_CMN_FILEID | ATTR_CMN_PARENTID;
    attrs.dirattr = ATTR_DIR_MOUNTSTATUS;

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
    size_t identity_offset = sizeof(length) + sizeof(returned);
    fsid_t entry_fsid;
    uint32_t flags = 0;
    uint64_t entry_fileid = 0;
    uint64_t entry_parentid = 0;
    if ((returned.commonattr & (ATTR_CMN_FSID | ATTR_CMN_FLAGS |
                                ATTR_CMN_FILEID | ATTR_CMN_PARENTID)) !=
        (ATTR_CMN_FSID | ATTR_CMN_FLAGS | ATTR_CMN_FILEID | ATTR_CMN_PARENTID) ||
        length < identity_offset + sizeof(entry_fsid) + sizeof(flags) +
                 sizeof(entry_fileid) + sizeof(entry_parentid)) {
        printf("getattrlistat path=%s identity_attrs_missing returned=0x%08x length=%u\n",
               path, returned.commonattr, length);
        return;
    }
    memcpy(&entry_fsid, buffer + identity_offset, sizeof(entry_fsid));
    identity_offset += sizeof(entry_fsid);
    memcpy(&flags, buffer + identity_offset, sizeof(flags));
    identity_offset += sizeof(flags);
    memcpy(&entry_fileid, buffer + identity_offset, sizeof(entry_fileid));
    identity_offset += sizeof(entry_fileid);
    memcpy(&entry_parentid, buffer + identity_offset, sizeof(entry_parentid));
    identity_offset += sizeof(entry_parentid);
    uint32_t dir_mountstatus = 0;
    if (returned.dirattr & ATTR_DIR_MOUNTSTATUS) {
        if (length < identity_offset + sizeof(dir_mountstatus)) {
            printf("getattrlistat path=%s truncated_dir_mountstatus length=%u\n", path, length);
            return;
        }
        memcpy(&dir_mountstatus, buffer + identity_offset, sizeof(dir_mountstatus));
    }
    printf("getattrlistat path=%s options=NOFOLLOW_ANY|RESOLVE_BENEATH flags=0x%08x "
           "dir_mountstatus=0x%08x returned_common=0x%08x returned_dir=0x%08x\n",
           path, flags, dir_mountstatus, returned.commonattr, returned.dirattr);

    struct stat root_stat;
    struct statfs root_fs;
    if (fstat(rootfd, &root_stat) == 0 && fstatfs(rootfd, &root_fs) == 0) {
        printf("getattrlistat_entry_metadata path=%s root_ino=%llu entry_fileid=%llu "
               "entry_parentid=%llu flags=0x%08x ",
               path, (unsigned long long)root_stat.st_ino,
               (unsigned long long)entry_fileid,
               (unsigned long long)entry_parentid, flags);
        print_fsid("root_fsid", root_fs.f_fsid);
        printf(" ");
        print_fsid("entry_fsid", entry_fsid);
        printf(" parent_matches_root=%s\n",
               entry_parentid == (uint64_t)root_stat.st_ino ? "yes" : "no");
    }

    int childfd = openat(rootfd, path, O_RDONLY | O_DIRECTORY | O_CLOEXEC | O_NOFOLLOW);
    if (childfd < 0) {
        printf("openat path=%s O_NOFOLLOW error=%d\n", path, errno);
        return;
    }
    print_descriptor_paths(childfd, path);

    struct stat handle_stat;
    struct statfs child_fs;
    if (fstat(childfd, &handle_stat) == 0 && fstatfs(childfd, &child_fs) == 0) {
        printf("getattrlistat_target_metadata path=%s target_ino=%llu ",
               path, (unsigned long long)handle_stat.st_ino);
        print_fsid("target_fsid", child_fs.f_fsid);
        printf(" entry_fsid_matches_target=%s\n",
               same_fsid(entry_fsid, child_fs.f_fsid) ? "yes" : "no");
    }
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

struct rename_race_context {
    char left[MAXPATHLEN];
    char right[MAXPATHLEN];
    int iterations;
    int writer_error;
    int writer_errno;
};

static void *rename_race_writer(void *opaque) {
    struct rename_race_context *context = opaque;
    for (int i = 0; i < context->iterations; ++i) {
        if (rename(context->left, context->right) != 0) {
            context->writer_error = 1;
            context->writer_errno = errno;
            return NULL;
        }
        sched_yield();
        if (rename(context->right, context->left) != 0) {
            context->writer_error = 1;
            context->writer_errno = errno;
            return NULL;
        }
        sched_yield();
    }
    return NULL;
}

static int has_either_suffix(const char *value, const char *left_suffix,
                             const char *right_suffix) {
    return has_suffix(value, left_suffix) || has_suffix(value, right_suffix);
}

static int probe_concurrent_rename_paths(void) {
    struct rename_race_context context;
    memset(&context, 0, sizeof(context));
    context.iterations = 2000;

    char root[] = "/tmp/loco-fcntl-race-XXXXXX";
    if (mkdtemp(root) == NULL) {
        printf("rename_race setup_error=%d\n", errno);
        return 1;
    }
    if (snprintf(context.left, sizeof(context.left), "%s/left", root) >=
            (int)sizeof(context.left) ||
        snprintf(context.right, sizeof(context.right), "%s/right", root) >=
            (int)sizeof(context.right)) {
        printf("rename_race path_overflow\n");
        rmdir(root);
        return 1;
    }
    if (mkdir(context.left, 0700) != 0) {
        printf("rename_race mkdir_error=%d\n", errno);
        rmdir(root);
        return 1;
    }

    int fd = open(context.left, O_RDONLY | O_DIRECTORY | O_CLOEXEC | O_NOFOLLOW);
    if (fd < 0) {
        printf("rename_race open_error=%d\n", errno);
        rmdir(context.left);
        rmdir(root);
        return 1;
    }

    struct stat before;
    if (fstat(fd, &before) != 0) {
        printf("rename_race fstat_before_error=%d\n", errno);
        close(fd);
        rmdir(context.left);
        rmdir(root);
        return 1;
    }

    pthread_t writer;
    int thread_error = pthread_create(&writer, NULL, rename_race_writer, &context);
    if (thread_error != 0) {
        printf("rename_race pthread_create_error=%d\n", thread_error);
        close(fd);
        rmdir(context.left);
        rmdir(root);
        return 1;
    }

    int path_errors = 0;
    int unexpected_paths = 0;
    char visible_path[MAXPATHLEN];
    char nofirmlink_path[MAXPATHLEN];
    for (int i = 0; i < context.iterations * 2; ++i) {
        if (capture_descriptor_paths(fd, NULL, visible_path, nofirmlink_path) != 0) {
            ++path_errors;
        } else if (!has_either_suffix(visible_path, "/left", "/right") ||
                   !has_either_suffix(nofirmlink_path, "/left", "/right")) {
            ++unexpected_paths;
        }
        sched_yield();
    }

    int join_error = pthread_join(writer, NULL);
    struct stat after;
    int fstat_error = fstat(fd, &after);
    int identity_same = fstat_error == 0 &&
                        before.st_dev == after.st_dev &&
                        before.st_ino == after.st_ino;
    printf("rename_race iterations=%d path_errors=%d unexpected_paths=%d "
           "writer_error=%d writer_errno=%d join_error=%d identity_same=%s\n",
           context.iterations * 2, path_errors, unexpected_paths,
           context.writer_error, context.writer_errno, join_error,
           identity_same ? "yes" : "no");

    close(fd);
    int cleanup_error = 0;
    if (rmdir(context.left) != 0 && errno != ENOENT) cleanup_error = errno;
    if (rmdir(context.right) != 0 && errno != ENOENT && cleanup_error == 0) cleanup_error = errno;
    if (rmdir(root) != 0 && cleanup_error == 0) cleanup_error = errno;

    return path_errors == 0 && unexpected_paths == 0 &&
           context.writer_error == 0 && join_error == 0 &&
           identity_same && cleanup_error == 0 ? 0 : 1;
}

static int probe_renamed_open_directory(void) {
    char root[] = "/tmp/loco-fcntl-rename-XXXXXX";
    if (mkdtemp(root) == NULL) {
        printf("rename_probe setup_error=%d\n", errno);
        return 1;
    }

    char original[MAXPATHLEN];
    char moved[MAXPATHLEN];
    if (snprintf(original, sizeof(original), "%s/before", root) >= (int)sizeof(original) ||
        snprintf(moved, sizeof(moved), "%s/after", root) >= (int)sizeof(moved)) {
        printf("rename_probe path_overflow\n");
        rmdir(root);
        return 1;
    }
    if (mkdir(original, 0700) != 0) {
        printf("rename_probe mkdir_error=%d\n", errno);
        rmdir(root);
        return 1;
    }

    int fd = open(original, O_RDONLY | O_DIRECTORY | O_CLOEXEC | O_NOFOLLOW);
    if (fd < 0) {
        printf("rename_probe open_error=%d\n", errno);
        rmdir(original);
        rmdir(root);
        return 1;
    }

    struct stat opened_before;
    if (fstat(fd, &opened_before) != 0) {
        printf("rename_probe fstat_before_error=%d\n", errno);
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
        printf("rename_probe rename_error=%d\n", errno);
        close(fd);
        rmdir(original);
        rmdir(root);
        return 1;
    }
    if (mkdir(original, 0700) != 0) {
        printf("rename_probe replacement_mkdir_error=%d\n", errno);
        close(fd);
        rmdir(moved);
        rmdir(root);
        return 1;
    }

    struct stat opened_after;
    if (fstat(fd, &opened_after) != 0) {
        printf("rename_probe fstat_after_error=%d\n", errno);
        close(fd);
        rmdir(original);
        rmdir(moved);
        rmdir(root);
        return 1;
    }
    int retained_identity_same =
        opened_before.st_dev == opened_after.st_dev &&
        opened_before.st_ino == opened_after.st_ino;
    printf("rename_probe retained_identity_same=%s\n",
           retained_identity_same ? "yes" : "no");
    if (!retained_identity_same ||
        capture_descriptor_paths(fd, "rename_after_replacement", visible_path,
                                 nofirmlink_path) != 0 ||
        !has_suffix(visible_path, "/after") ||
        !has_suffix(nofirmlink_path, "/after")) {
        printf("rename_probe retained_path_not_updated\n");
        close(fd);
        rmdir(original);
        rmdir(moved);
        rmdir(root);
        return 1;
    }

    int replacement_fd = open(original, O_RDONLY | O_DIRECTORY | O_CLOEXEC | O_NOFOLLOW);
    if (replacement_fd < 0) {
        printf("rename_probe replacement_open_error=%d\n", errno);
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
        printf("rename_probe replacement_fstat_error=%d\n", errno);
        close(replacement_fd);
        close(fd);
        rmdir(original);
        rmdir(moved);
        rmdir(root);
        return 1;
    }
    int replacement_identity_distinct =
        opened_after.st_dev != replacement_stat.st_dev ||
        opened_after.st_ino != replacement_stat.st_ino;
    printf("rename_probe replacement_identity_distinct=%s\n",
           replacement_identity_distinct ? "yes" : "no");
    if (!replacement_identity_distinct) {
        close(replacement_fd);
        close(fd);
        rmdir(original);
        rmdir(moved);
        rmdir(root);
        return 1;
    }

    close(replacement_fd);
    close(fd);
    if (rmdir(original) != 0 || rmdir(moved) != 0 || rmdir(root) != 0) {
        printf("rename_probe cleanup_error=%d\n", errno);
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

    if (probe_renamed_open_directory() != 0 ||
        probe_concurrent_rename_paths() != 0) {
        close(dirfd);
        return 3;
    }

    close(dirfd);
    if (probe_bulk_entry_identity() != 0) {
        return 4;
    }
    if (probe_extended_nofirmlink_path_at(dirfd, "Users") != 0 ||
        probe_extended_nofirmlink_path_at(dirfd, "Applications") != 0 ||
        probe_extended_nofirmlink_path_at(dirfd, "Library") != 0) {
        return 5;
    }
    if (probe_getdirentriesattr_firmlink_flags() != 0) {
        return 5;
    }
    return 0;
}
