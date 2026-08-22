use std::ffi::CStr;
use std::fs::File;
use std::io;
use std::os::fd::{AsRawFd, BorrowedFd, FromRawFd, OwnedFd};

// `O_RESOLVE_BENEATH` is present in the macOS 26 SDK's `sys/fcntl.h` as
// `0x00001000`. libc 0.2.186 does not expose the Darwin constant yet.
const O_RESOLVE_BENEATH: libc::c_int = 0x0000_1000;

pub(crate) fn open_root(path: &CStr) -> io::Result<OwnedFd> {
    // SAFETY: `path` is a NUL-terminated C string. The flags require a
    // directory descriptor and the returned descriptor is adopted once.
    let fd = unsafe {
        libc::open(
            path.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    };
    root_from_fd(fd)
}

pub(crate) fn open_dir(dirfd: BorrowedFd<'_>, path: &CStr) -> io::Result<OwnedFd> {
    let fd = openat(
        dirfd,
        path,
        libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
        0,
    );
    root_from_fd(fd)
}

pub(crate) fn open_file(
    dirfd: BorrowedFd<'_>,
    path: &CStr,
    flags: libc::c_int,
    mode: libc::mode_t,
) -> io::Result<File> {
    file_from_fd(openat(dirfd, path, flags | libc::O_CLOEXEC, mode))
}

fn openat(
    dirfd: BorrowedFd<'_>,
    path: &CStr,
    flags: libc::c_int,
    mode: libc::mode_t,
) -> libc::c_int {
    // SAFETY: `path` is a NUL-terminated C string, `dirfd` remains open for
    // this call, and `mode` is only observed by the kernel with `O_CREAT`.
    unsafe {
        libc::openat(
            dirfd.as_raw_fd(),
            path.as_ptr(),
            flags | O_RESOLVE_BENEATH,
            libc::c_uint::from(mode),
        )
    }
}

fn file_from_fd(fd: libc::c_int) -> io::Result<File> {
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: a nonnegative descriptor returned by this module's opening
    // syscall is newly owned here and has not been wrapped elsewhere.
    let fd = unsafe { OwnedFd::from_raw_fd(fd) };
    Ok(File::from(fd))
}

fn root_from_fd(fd: libc::c_int) -> io::Result<OwnedFd> {
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: a nonnegative descriptor returned by this module's root-open
    // syscall is newly owned here and has not been wrapped elsewhere.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}
