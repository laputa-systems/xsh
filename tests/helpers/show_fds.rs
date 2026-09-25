//! Describes non-standard descriptors inherited across `exec`.
//!
//! Integration tests use this in a child process while other descriptors are
//! live. It intentionally scans with `F_GETFD` instead of opening
//! `/proc/self/fd` or `/dev/fd`, which would create an observation descriptor
//! of its own. Socket ports identify live connections; on Apple systems,
//! character-device paths identify PTY masters.

use std::mem::{MaybeUninit, size_of};

fn main() {
    for fd in 3..1024 {
        let result = unsafe { libc::fcntl(fd, libc::F_GETFD) };
        if result != -1 {
            println!("{fd} {}", describe_fd(fd));
        }
    }
}

fn describe_fd(fd: i32) -> String {
    let mut stat = MaybeUninit::<libc::stat>::uninit();
    if unsafe { libc::fstat(fd, stat.as_mut_ptr()) } == -1 {
        return format!("fstat-error={}", std::io::Error::last_os_error());
    }
    let stat = unsafe { stat.assume_init() };
    let kind = match stat.st_mode & libc::S_IFMT {
        libc::S_IFSOCK => "socket",
        libc::S_IFIFO => "pipe",
        libc::S_IFREG => "file",
        libc::S_IFDIR => "directory",
        libc::S_IFCHR => "character-device",
        _ => "other",
    };
    if kind != "socket" {
        #[cfg(target_vendor = "apple")]
        return format!("{kind} rdev={} path={}", stat.st_rdev, descriptor_path(fd));
        #[cfg(not(target_vendor = "apple"))]
        return format!("{kind} rdev={}", stat.st_rdev);
    }

    let local = socket_address(fd, false);
    let peer = socket_address(fd, true);
    format!("socket local={local} peer={peer}")
}

#[cfg(target_vendor = "apple")]
fn descriptor_path(fd: i32) -> String {
    let mut path = [0_u8; 1024];
    if unsafe { libc::fcntl(fd, libc::F_GETPATH, path.as_mut_ptr()) } == -1 {
        return format!("error={}", std::io::Error::last_os_error());
    }
    let length = path.iter().position(|byte| *byte == 0).unwrap_or(path.len());
    String::from_utf8_lossy(&path[..length]).into_owned()
}

fn socket_address(fd: i32, peer: bool) -> String {
    let mut address = MaybeUninit::<libc::sockaddr_storage>::zeroed();
    let mut length = size_of::<libc::sockaddr_storage>() as libc::socklen_t;
    let result = unsafe {
        if peer {
            libc::getpeername(fd, address.as_mut_ptr().cast(), &mut length)
        } else {
            libc::getsockname(fd, address.as_mut_ptr().cast(), &mut length)
        }
    };
    if result == -1 {
        return format!("error={}", std::io::Error::last_os_error());
    }
    let address = unsafe { address.assume_init() };
    match i32::from(address.ss_family) {
        libc::AF_INET if (length as usize) >= size_of::<libc::sockaddr_in>() => {
            let address = unsafe {
                &*((&address as *const libc::sockaddr_storage).cast::<libc::sockaddr_in>())
            };
            format!("inet:{}", u16::from_be(address.sin_port))
        }
        libc::AF_INET6 if (length as usize) >= size_of::<libc::sockaddr_in6>() => {
            let address = unsafe {
                &*((&address as *const libc::sockaddr_storage).cast::<libc::sockaddr_in6>())
            };
            format!("inet6:{}", u16::from_be(address.sin6_port))
        }
        libc::AF_UNIX => "unix".to_string(),
        family => format!("family={family}"),
    }
}
