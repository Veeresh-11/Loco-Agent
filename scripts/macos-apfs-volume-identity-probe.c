#include <sys/mount.h>
#include <sys/stat.h>
#include <fcntl.h>
#include <inttypes.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>
#include <errno.h>

static void inspect(const char *path) {
    int fd = open(path, O_RDONLY | O_CLOEXEC | O_DIRECTORY);
    if (fd < 0) {
        printf("volume_identity path=%s open_error=%d\n", path, errno);
        return;
    }

    struct stat st;
    struct statfs fs;
    if (fstat(fd, &st) != 0) {
        printf("volume_identity path=%s fstat_error=%d\n", path, errno);
        close(fd);
        return;
    }
    if (fstatfs(fd, &fs) != 0) {
        printf("volume_identity path=%s fstatfs_error=%d\n", path, errno);
        close(fd);
        return;
    }

    printf("volume_identity path=%s st_dev=%" PRIuMAX
           " st_ino=%" PRIuMAX " fsid=(%d,%d) fs_type=%s"
           " mount_on=%s mount_from=%s\n",
           path, (uintmax_t)st.st_dev, (uintmax_t)st.st_ino,
           fs.f_fsid.val[0], fs.f_fsid.val[1], fs.f_fstypename,
           fs.f_mntonname, fs.f_mntfromname);
    close(fd);
}

int main(void) {
    const char *paths[] = {
        "/",
        "/System",
        "/System/Volumes/Data",
        "/Users",
        "/Applications",
        "/Library",
        "/System/Volumes/Preboot"
    };
    printf("macOS APFS volume identity probe\n");
    for (size_t i = 0; i < sizeof(paths) / sizeof(paths[0]); ++i) {
        inspect(paths[i]);
    }
    return 0;
}
