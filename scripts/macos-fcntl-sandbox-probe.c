#include <sys/param.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <errno.h>
#include <unistd.h>

#if !defined(F_GETPATH_NOFIRMLINK)
#error "The macOS SDK must expose F_GETPATH_NOFIRMLINK"
#endif

int main(int argc, char **argv) {
    if (argc != 2) {
        fprintf(stderr, "usage: macos-fcntl-sandbox-probe DIRECTORY\n");
        return 2;
    }

    int fd = open(argv[1], O_RDONLY | O_DIRECTORY | O_CLOEXEC);
    if (fd < 0) {
        fprintf(stderr, "sandbox_probe open_error=%d\n", errno);
        return 3;
    }

    char visible[MAXPATHLEN];
    char nofirmlink[MAXPATHLEN];
    memset(visible, 0, sizeof(visible));
    memset(nofirmlink, 0, sizeof(nofirmlink));

    if (fcntl(fd, F_GETPATH, visible) != 0 ||
        memchr(visible, '\0', sizeof(visible)) == NULL) {
        fprintf(stderr, "sandbox_probe getpath_error=%d\n", errno);
        close(fd);
        return 4;
    }

    errno = 0;
    int nofirmlink_result = fcntl(fd, F_GETPATH_NOFIRMLINK, nofirmlink);
    int nofirmlink_errno = errno;
    printf("sandbox_probe getpath=success nofirmlink_result=%s nofirmlink_errno=%d\n",
           nofirmlink_result == 0 ? "success" : "denied_or_failed",
           nofirmlink_errno);

    close(fd);
    if (nofirmlink_result == 0) {
        return 5;
    }
    if (nofirmlink_errno != EPERM && nofirmlink_errno != EACCES) {
        return 6;
    }
    return 0;
}
