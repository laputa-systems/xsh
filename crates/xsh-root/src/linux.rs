use std::ffi::CStr;
use std::fs::File;
use std::io;
use std::mem::{size_of, zeroed};
use std::os::fd::{AsRawFd, BorrowedFd, FromRawFd, OwnedFd};

pub(crate) fn open_root(path: &CStr) -> io::Result<OwnedFd> {
    // SAFETY: `path` is a NUL-terminated C string. The flags require a
    // directory descriptor and the returned descriptor is adopted once.
    let fd = unsafe {
        libc::open(
            path.as_ptr(),
            libc::O_PATH | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    };
    root_from_fd(fd)
}

pub(crate) fn open_dir(dirfd: BorrowedFd<'_>, path: &CStr) -> io::Result<OwnedFd> {
    let fd = openat2(
        dirfd,
        path,
        libc::O_PATH | libc::O_DIRECTORY | libc::O_CLOEXEC,
        0,
    )?;
    root_from_fd(fd)
}

pub(crate) fn open_file(
    dirfd: BorrowedFd<'_>,
    path: &CStr,
    flags: libc::c_int,
    mode: libc::mode_t,
) -> io::Result<File> {
    let fd = openat2(dirfd, path, flags | libc::O_CLOEXEC, mode)?;
    file_from_fd(fd)
}

fn openat2(
    dirfd: BorrowedFd<'_>,
    path: &CStr,
    flags: libc::c_int,
    mode: libc::mode_t,
) -> io::Result<libc::c_int> {
    // SAFETY: all-zero is a valid `open_how` baseline; every ABI field is then
    // initialized below before the kernel reads the structure.
    let mut how: libc::open_how = unsafe { zeroed() };
    how.flags = flags as u64;
    how.mode = if flags & libc::O_CREAT != 0 {
        mode as u64
    } else {
        0
    };
    how.resolve = libc::RESOLVE_BENEATH | libc::RESOLVE_NO_MAGICLINKS;
    // SAFETY: `path` points to a NUL-terminated string, `how` is a valid
    // `open_how` with its exact ABI size, and `dirfd` remains borrowed for the
    // duration of this direct syscall.
    let fd = unsafe {
        libc::syscall(
            libc::SYS_openat2,
            dirfd.as_raw_fd(),
            path.as_ptr(),
            &how,
            size_of::<libc::open_how>(),
        )
    };
    if fd < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(fd as libc::c_int)
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
