#include <sys/attr.h>
#include <sys/stat.h>
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
    int64_t name_start = 0;
    uint32_t name_length = 0;
    if (returned.commonattr & ATTR_CMN_NAME) {
        attrreference_t name_ref;
        if (offset + sizeof(name_ref) > record_len) {
            fprintf(stderr, "truncated ATTR_CMN_NAME reference\n");
            exit(2);
        }
        memcpy(&name_ref, record + offset, sizeof(name_ref));
        size_t name_ref_end = offset + sizeof(name_ref);
        name_start = (int64_t)offset + (int64_t)name_ref.attr_dataoffset;
        name_length = name_ref.attr_length;
        size_t trailing_fixed_fields = 0;
        if (returned.commonattr & ATTR_CMN_FLAGS) {
            trailing_fixed_fields += sizeof(uint32_t);
        }
        if (returned.forkattr & ATTR_CMNEXT_NOFIRMLINKPATH) {
            trailing_fixed_fields += sizeof(attrreference_t);
        }
        size_t minimum_name_start = name_ref_end + trailing_fixed_fields;
        if (name_start < 0 || (uint64_t)name_start < minimum_name_start ||
            name_ref.attr_length < 2 ||
            (uint64_t)name_start + name_ref.attr_length > record_len ||
            name_ref.attr_length > sizeof(name)) {
            fprintf(stderr, "invalid name reference: offset=%lld length=%u record=%zu\n",
                    (long long)name_start, name_ref.attr_length, record_len);
            exit(2);
        }
        const unsigned char *name_bytes = record + (size_t)name_start;
        if (name_bytes[name_ref.attr_length - 1] != '\0' ||
            memchr(name_bytes, '\0', name_ref.attr_length - 1) != NULL) {
            fprintf(stderr, "name is not exactly NUL terminated\n");
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
        offset += sizeof(flags);
    }

    char nofirmlink_path[PATH_MAX + 1];
    nofirmlink_path[0] = '\0';
    int has_nofirmlink_path =
        (returned.forkattr & ATTR_CMNEXT_NOFIRMLINKPATH) != 0;
    if (has_nofirmlink_path) {
        attrreference_t path_ref;
        if (offset + sizeof(path_ref) > record_len) {
            fprintf(stderr, "truncated ATTR_CMNEXT_NOFIRMLINKPATH reference\n");
            exit(2);
        }
        memcpy(&path_ref, record + offset, sizeof(path_ref));
        size_t path_ref_end = offset + sizeof(path_ref);
        int64_t path_start = (int64_t)offset + (int64_t)path_ref.attr_dataoffset;
        if (path_start < 0 || (uint64_t)path_start < path_ref_end ||
            path_ref.attr_length < 2 ||
            path_ref.attr_length > sizeof(nofirmlink_path) ||
            (uint64_t)path_start + path_ref.attr_length > record_len) {
            fprintf(stderr, "invalid nofirmlink path reference: offset=%lld length=%u record=%zu\n",
                    (long long)path_start, path_ref.attr_length, record_len);
            exit(2);
        }
        const unsigned char *path_bytes = record + (size_t)path_start;
        if (path_bytes[0] != '/' || path_bytes[path_ref.attr_length - 1] != '\0' ||
            memchr(path_bytes, '\0', path_ref.attr_length - 1) != NULL) {
            fprintf(stderr, "nofirmlink path is not exactly NUL terminated\n");
            exit(2);
        }
        if ((returned.commonattr & ATTR_CMN_NAME) &&
            (uint64_t)name_start + name_length > (uint64_t)path_start) {
            fprintf(stderr, "name payload overlaps nofirmlink path payload\\n");
            exit(2);
        }
        memcpy(nofirmlink_path, path_bytes, path_ref.attr_length);
    }

    if (strcmp(name, "Users") == 0 || strcmp(name, "Applications") == 0 ||
        strcmp(name, "Library") == 0 || strcmp(name, "private") == 0 ||
        strcmp(name, "System") == 0 || strcmp(name, "usr") == 0) {
        printf("entry=%s record_len=%u returned_common=0x%08x error=%u ",
               name, declared_len, returned.commonattr, entry_error);
        if (has_flags) {
            printf("flags=0x%08x flags_offset=%zu ", flags, flags_offset);
        } else {
            printf("flags=NOT_RETURNED ");
        }
        if (has_nofirmlink_path) {
            printf("nofirmlink_path=%s ", nofirmlink_path);
        } else {
            printf("nofirmlink_path=NOT_RETURNED ");
        }
        printf("raw=");
        size_t dump_len = record_len < 64 ? record_len : 64;
        for (size_t i = 0; i < dump_len; ++i) {
            printf("%02x", record[i]);
        }
        printf("\n");
    }
}


static void print_path_api_flags(int rootfd, const char *path) {
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

int main(void) {
    struct attrlist attrs;
    memset(&attrs, 0, sizeof(attrs));
    attrs.bitmapcount = ATTR_BIT_MAP_COUNT;
    attrs.commonattr = ATTR_CMN_RETURNED_ATTRS | ATTR_CMN_NAME |
                       ATTR_CMN_FLAGS | ATTR_CMN_ERROR;
    attrs.forkattr = ATTR_CMNEXT_NOFIRMLINKPATH;

    int dirfd = open("/", O_RDONLY | O_CLOEXEC);
    if (dirfd < 0) {
        perror("open(/)");
        return 1;
    }

    unsigned char buffer[64 * 1024] __attribute__((aligned(8)));
    for (;;) {
        int count = getattrlistbulk(dirfd, &attrs, buffer, sizeof(buffer), FSOPT_ATTR_CMN_EXTENDED);
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
    print_path_api_flags(dirfd, "private");

    close(dirfd);
    return 0;
}
