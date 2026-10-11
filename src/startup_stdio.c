#include <errno.h>
#include <fcntl.h>

static unsigned int absent_standard_descriptors;

/* Rust reserves missing standard descriptors during runtime initialization.
 * Capture absence first so those reservations cannot become script streams. */
__attribute__((constructor)) static void capture_standard_descriptors(void) {
    for (int fd = 0; fd < 3; ++fd) {
        if (fcntl(fd, F_GETFD) == -1 && errno == EBADF)
            absent_standard_descriptors |= 1u << fd;
    }
}

unsigned int xsh_absent_standard_descriptors(void) {
    return absent_standard_descriptors;
}
