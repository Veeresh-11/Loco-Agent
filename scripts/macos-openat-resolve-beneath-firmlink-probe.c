#include <sys/types.h>
#include <sys/stat.h>
#include <sys/param.h>
#include <fcntl.h>
#include <unistd.h>
#include <errno.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// Keep ordinary controls strict: only ENOTCAPABLE may classify a mapped open as blocked.

#ifndef O_RESOLVE_BENEATH
#error "This probe requires the SDK's O_RESOLVE_BENEATH definition"
#endif

#ifndef F_GETPATH_NOFIRMLINK
#define F_GETPATH_NOFIRMLINK 102
#endif

static int print_descriptor_path(int fd, char *buffer, size_t size) {
    memset(buffer, 0, size);
    if (fcntl(fd, F_GETPATH_NOFIRMLINK, buffer) != 0) {
        return -1;
    }
    return 0;
}

static int probe_openat_beneath(const char *source_path, int is_firmlink_mapping) {
    char parent_path[PATH_MAX];
    char leaf[NAME_MAX + 1];
    char resolved[PATH_MAX];
    size_t length = strlen(source_path);
    if (length == 0 || length >= sizeof(parent_path)) {
        printf("resolve_beneath source=%s invalid_path\n", source_path);
        return 1;
    }

    memcpy(parent_path, source_path, length + 1);
    char *slash = strrchr(parent_path, '/');
    if (slash == NULL || (slash == parent_path && slash[1] == '\0')) {
        printf("resolve_beneath source=%s invalid_parent\n", source_path);
        return 1;
    }
    const char *name = slash + 1;
    if (strlen(name) == 0 || strlen(name) > NAME_MAX) {
        printf("resolve_beneath source=%s invalid_leaf\n", source_path);
        return 1;
    }
    strlcpy(leaf, name, sizeof(leaf));
    if (slash == parent_path) {
        slash[1] = '\0';
    } else {
        *slash = '\0';
    }

    int parent_fd = open(parent_path, O_RDONLY | O_DIRECTORY | O_CLOEXEC);
    if (parent_fd < 0) {
        printf("resolve_beneath source=%s parent_error=%d\n", source_path, errno);
        return 1;
    }

    // Establish that this exact mapping is ordinarily traversable before
    // attributing any failure to O_RESOLVE_BENEATH.
    errno = 0;
    int baseline_fd = openat(parent_fd, leaf,
        O_RDONLY | O_DIRECTORY | O_CLOEXEC | O_NOFOLLOW);
    int baseline_error = errno;
    if (baseline_fd < 0) {
        printf("resolve_beneath source=%s baseline=unavailable errno=%d (%s)\n",
               source_path, baseline_error, strerror(baseline_error));
        close(parent_fd);
        return 1;
    }
    if (print_descriptor_path(baseline_fd, resolved, sizeof(resolved)) != 0) {
        printf("resolve_beneath source=%s baseline=path_unavailable errno=%d\n",
               source_path, errno);
        close(baseline_fd);
        close(parent_fd);
        return 1;
    }
    printf("resolve_beneath source=%s baseline=opened nofirmlink_path=%s\n",
           source_path, resolved);
    close(baseline_fd);

    errno = 0;
    int child_fd = openat(parent_fd, leaf,
        O_RDONLY | O_DIRECTORY | O_CLOEXEC | O_NOFOLLOW | O_RESOLVE_BENEATH);
    int open_error = errno;
    if (child_fd < 0) {
        if (open_error != ENOTCAPABLE) {
            printf("resolve_beneath source=%s beneath=inconclusive errno=%d (%s)\n",
                   source_path, open_error, strerror(open_error));
            close(parent_fd);
            return 1;
        }
        if (!is_firmlink_mapping) {
            printf("resolve_beneath source=%s control=unexpectedly_blocked errno=%d (%s)\n",
                   source_path, open_error, strerror(open_error));
            close(parent_fd);
            return 1;
        }
        printf("resolve_beneath source=%s beneath=blocked errno=%d (%s)\n",
               source_path, open_error, strerror(open_error));
        close(parent_fd);
        return 0;
    }

    if (print_descriptor_path(child_fd, resolved, sizeof(resolved)) != 0) {
        printf("resolve_beneath source=%s beneath=path_unavailable errno=%d\n",
               source_path, errno);
        close(child_fd);
        close(parent_fd);
        return 1;
    }
    printf("resolve_beneath source=%s beneath=opened nofirmlink_path=%s\n",
           source_path, resolved);
    close(child_fd);
    close(parent_fd);
    return 0;
}
static int verify_flag_rejects_parent_escape(void) {
    char template[] = "/tmp/loco-resolve-beneath-XXXXXX";
    char *root = mkdtemp(template);
    if (root == NULL) {
        printf("resolve_beneath control=parent_escape setup_error=%d\n", errno);
        return 1;
    }
    char child[PATH_MAX];
    if (snprintf(child, sizeof(child), "%s/child", root) >= (int)sizeof(child) ||
        mkdir(child, 0700) != 0) {
        printf("resolve_beneath control=parent_escape mkdir_error=%d\n", errno);
        rmdir(root);
        return 1;
    }
    int root_fd = open(child, O_RDONLY | O_DIRECTORY | O_CLOEXEC);
    if (root_fd < 0) {
        printf("resolve_beneath control=parent_escape open_error=%d\n", errno);
        rmdir(child);
        rmdir(root);
        return 1;
    }
    errno = 0;
    int escaped = openat(root_fd, "..", O_RDONLY | O_DIRECTORY | O_CLOEXEC | O_RESOLVE_BENEATH);
    int error = errno;
    if (escaped >= 0) {
        close(escaped);
        printf("resolve_beneath control=parent_escape unexpected_success\n");
        close(root_fd);
        rmdir(child);
        rmdir(root);
        return 1;
    }
    printf("resolve_beneath control=parent_escape blocked errno=%d (%s)\n",
           error, strerror(error));
    close(root_fd);
    rmdir(child);
    rmdir(root);
    return error == ENOTCAPABLE ? 0 : 1;
}

int main(void) {
    if (verify_flag_rejects_parent_escape() != 0) {
        return 1;
    }

    const char *controls[] = {"/System", "/usr"};
    for (size_t i = 0; i < sizeof(controls) / sizeof(controls[0]); ++i) {
        if (probe_openat_beneath(controls[i], 0) != 0) {
            return 2;
        }
    }

    const char *root_mappings[] = {"/Users", "/Applications", "/Library", "/private"};
    for (size_t i = 0; i < sizeof(root_mappings) / sizeof(root_mappings[0]); ++i) {
        if (probe_openat_beneath(root_mappings[i], 1) != 0) {
            return 3;
        }
    }

    FILE *map = fopen("/usr/share/firmlinks", "r");
    if (map == NULL) {
        printf("resolve_beneath map_open_error=%d\n", errno);
        return 4;
    }
    char line[PATH_MAX * 2];
    char source[PATH_MAX];
    char target[PATH_MAX];
    const char prefix[] = "/System/Library/";
    size_t nested_count = 0;
    while (fgets(line, sizeof(line), map) != NULL) {
        if (sscanf(line, "%1023s %1023s", source, target) != 2) {
            continue;
        }
        if (strncmp(source, prefix, sizeof(prefix) - 1) != 0) {
            continue;
        }
        if (probe_openat_beneath(source, 1) != 0) {
            fclose(map);
            return 5;
        }
        nested_count++;
    }
    fclose(map);
    printf("resolve_beneath nested_entries_tested=%zu\n", nested_count);
    return nested_count > 0 ? 0 : 6;
}
