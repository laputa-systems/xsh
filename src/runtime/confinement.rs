//! Kernel-enforced confinement a spawned child applies to itself before exec.
//!
//! The one supported profile is Landlock ABI 6 or newer plus `no_new_privs`:
//! the child may read and execute beneath the declared read paths, change
//! files beneath the declared write paths, and nothing else on the
//! filesystem; TCP bind and connect are denied unless allowed; signals and
//! abstract unix sockets cannot reach processes outside the child's domain;
//! and every descriptor above standard error is closed at exec. The ruleset
//! is built in the parent, so the child only makes three system calls.

use crate::runtime::value::RunError;
use std::fmt;
use std::os::fd::OwnedFd;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
pub enum FilesystemAuthority {
    Path(PathBuf),
    Root(Arc<OwnedFd>),
}

impl fmt::Debug for FilesystemAuthority {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Path(path) => formatter.debug_tuple("Path").field(path).finish(),
            Self::Root(_) => formatter.write_str("FsRoot"),
        }
    }
}

impl PartialEq for FilesystemAuthority {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Path(left), Self::Path(right)) => left == right,
            (Self::Root(left), Self::Root(right)) => Arc::ptr_eq(left, right),
            _ => false,
        }
    }
}

impl Eq for FilesystemAuthority {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Confinement {
    pub read: Vec<FilesystemAuthority>,
    pub write: Vec<FilesystemAuthority>,
    pub tcp: TcpAccess,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TcpAccess {
    Denied,
    Allowed,
}

pub const REQUIRED_LANDLOCK_ABI: i64 = 6;

/// A failure to build the confinement, reported before any fork.
pub const KIND: &str = "confinement";

fn failure(message: String) -> RunError {
    RunError::new(KIND, message)
}

/// Whether the result of probing the Landlock ABI version supports the
/// profile: the version, or the raw errno the probe failed with.
pub fn check_landlock_abi(probe: Result<i64, i32>) -> Result<(), RunError> {
    match probe {
        Ok(abi) if abi >= REQUIRED_LANDLOCK_ABI => Ok(()),
        Ok(abi) => Err(failure(format!(
            "confinement requires Landlock ABI {REQUIRED_LANDLOCK_ABI}, the kernel provides ABI {abi}"
        ))),
        Err(libc::ENOSYS) => Err(failure("confinement requires Landlock, which this kernel does not provide".into())),
        Err(libc::EOPNOTSUPP) => Err(failure("confinement requires Landlock, which is disabled on this kernel".into())),
        Err(errno) => Err(failure(format!(
            "cannot probe Landlock: {}",
            std::io::Error::from_raw_os_error(errno)
        ))),
    }
}

#[cfg(target_os = "linux")]
pub(crate) use linux::prepare;
#[cfg(not(target_os = "linux"))]
pub(crate) use unsupported::prepare;

#[cfg(target_os = "linux")]
mod linux {
    use super::{Confinement, FilesystemAuthority, TcpAccess, check_landlock_abi, failure};
    use crate::runtime::spawn::Command;
    use crate::runtime::value::RunError;
    use rustix::fs::{FileType, Mode, OFlags};
    use std::io;
    use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd};

    const ACCESS_FS_EXECUTE: u64 = 1 << 0;
    const ACCESS_FS_WRITE_FILE: u64 = 1 << 1;
    const ACCESS_FS_READ_FILE: u64 = 1 << 2;
    const ACCESS_FS_READ_DIR: u64 = 1 << 3;
    const ACCESS_FS_TRUNCATE: u64 = 1 << 14;
    const ACCESS_FS_IOCTL_DEV: u64 = 1 << 15;
    const ACCESS_FS_ABI_6: u64 = (1 << 16) - 1;
    const ACCESS_FS_FILE: u64 = ACCESS_FS_EXECUTE
        | ACCESS_FS_WRITE_FILE
        | ACCESS_FS_READ_FILE
        | ACCESS_FS_TRUNCATE
        | ACCESS_FS_IOCTL_DEV;
    const ACCESS_FS_READ: u64 = ACCESS_FS_EXECUTE | ACCESS_FS_READ_FILE | ACCESS_FS_READ_DIR;
    const ACCESS_NET_BIND_TCP: u64 = 1 << 0;
    const ACCESS_NET_CONNECT_TCP: u64 = 1 << 1;
    const SCOPE_ABSTRACT_UNIX_SOCKET: u64 = 1 << 0;
    const SCOPE_SIGNAL: u64 = 1 << 1;
    const CREATE_RULESET_VERSION: libc::c_uint = 1 << 0;
    const RULE_PATH_BENEATH: libc::c_int = 1;

    #[repr(C)]
    struct RulesetAttr {
        handled_access_fs: u64,
        handled_access_net: u64,
        scoped: u64,
    }

    // The kernel declares `struct landlock_path_beneath_attr` packed.
    #[repr(C, packed)]
    struct PathBeneathAttr {
        allowed_access: u64,
        parent_fd: i32,
    }

    fn probe_abi() -> Result<i64, i32> {
        // SAFETY: a null attribute with size 0 and the version flag only
        // queries the ABI version.
        let abi = unsafe {
            libc::syscall(
                libc::SYS_landlock_create_ruleset,
                std::ptr::null::<RulesetAttr>(),
                0usize,
                CREATE_RULESET_VERSION,
            )
        };
        if abi < 0 {
            Err(io::Error::last_os_error().raw_os_error().unwrap_or(libc::EINVAL))
        } else {
            Ok(abi)
        }
    }

    fn create_ruleset(tcp: TcpAccess) -> Result<OwnedFd, RunError> {
        let attr = RulesetAttr {
            handled_access_fs: ACCESS_FS_ABI_6,
            handled_access_net: match tcp {
                TcpAccess::Denied => ACCESS_NET_BIND_TCP | ACCESS_NET_CONNECT_TCP,
                TcpAccess::Allowed => 0,
            },
            scoped: SCOPE_ABSTRACT_UNIX_SOCKET | SCOPE_SIGNAL,
        };
        // SAFETY: `attr` is a valid ruleset attribute of the size passed.
        let fd = unsafe {
            libc::syscall(
                libc::SYS_landlock_create_ruleset,
                &attr as *const RulesetAttr,
                std::mem::size_of::<RulesetAttr>(),
                0 as libc::c_uint,
            )
        };
        if fd < 0 {
            return Err(failure(format!(
                "cannot create a Landlock ruleset: {}",
                io::Error::last_os_error()
            )));
        }
        // SAFETY: the kernel returned a new close-on-exec descriptor we own.
        Ok(unsafe { OwnedFd::from_raw_fd(fd as i32) })
    }

    fn allow_beneath(ruleset: &OwnedFd, authority: &FilesystemAuthority, access: u64) -> Result<(), RunError> {
        match authority {
            FilesystemAuthority::Path(path) => {
                let denied = |error: io::Error| failure(format!("cannot confine to {}: {error}", path.display()));
                let parent = rustix::fs::open(path, OFlags::PATH | OFlags::CLOEXEC, Mode::empty())
                    .map_err(|error| denied(error.into()))?;
                allow_descriptor(ruleset, parent.as_fd(), access).map_err(denied)
            }
            FilesystemAuthority::Root(root) => allow_descriptor(ruleset, root.as_fd(), access)
                .map_err(|error| failure(format!("cannot confine to held FsRoot: {error}"))),
        }
    }

    fn allow_descriptor(ruleset: &OwnedFd, parent: BorrowedFd<'_>, access: u64) -> io::Result<()> {
        let stat = rustix::fs::fstat(&parent)?;
        let allowed_access = if FileType::from_raw_mode(stat.st_mode) == FileType::Directory {
            access
        } else {
            access & ACCESS_FS_FILE
        };
        let attr = PathBeneathAttr { allowed_access, parent_fd: parent.as_raw_fd() };
        // SAFETY: `attr` is a valid path-beneath rule whose descriptor lives
        // until the call returns.
        let code = unsafe {
            libc::syscall(
                libc::SYS_landlock_add_rule,
                ruleset.as_raw_fd(),
                RULE_PATH_BENEATH,
                &attr as *const PathBeneathAttr,
                0 as libc::c_uint,
            )
        };
        if code != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Builds the ruleset in the parent and arranges for the child to enter
    /// it between fork and exec.
    pub(crate) fn prepare(confinement: &Confinement, command: &mut Command) -> Result<(), RunError> {
        check_landlock_abi(probe_abi())?;
        let ruleset = create_ruleset(confinement.tcp)?;
        for path in &confinement.read {
            allow_beneath(&ruleset, path, ACCESS_FS_READ)?;
        }
        for path in &confinement.write {
            allow_beneath(&ruleset, path, ACCESS_FS_ABI_6)?;
        }
        // SAFETY: the hook makes only raw system calls on a descriptor opened
        // here, and builds an `io::Error` from an errno, which does not allocate.
        unsafe {
            command.pre_exec(move || {
                rustix::thread::set_no_new_privs(true)?;
                if libc::syscall(libc::SYS_landlock_restrict_self, ruleset.as_raw_fd(), 0 as libc::c_uint) != 0 {
                    return Err(io::Error::last_os_error());
                }
                if libc::syscall(libc::SYS_close_range, 3 as libc::c_uint, libc::c_uint::MAX, libc::CLOSE_RANGE_CLOEXEC) != 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        Ok(())
    }
}

#[cfg(not(target_os = "linux"))]
mod unsupported {
    use super::Confinement;
    use crate::runtime::spawn::Command;
    use crate::runtime::value::RunError;

    pub(crate) fn prepare(_confinement: &Confinement, _command: &mut Command) -> Result<(), RunError> {
        Err(RunError::new("unsupported-platform", "confinement is only available on Linux"))
    }
}

#[cfg(test)]
mod tests {
    use super::{KIND, check_landlock_abi};

    #[test]
    fn the_profile_needs_landlock_abi_6_and_names_what_the_kernel_lacks() {
        assert!(check_landlock_abi(Ok(6)).is_ok());
        assert!(check_landlock_abi(Ok(10)).is_ok());
        for (probe, message) in [
            (Ok(5), "confinement requires Landlock ABI 6, the kernel provides ABI 5"),
            (Err(libc::ENOSYS), "confinement requires Landlock, which this kernel does not provide"),
            (Err(libc::EOPNOTSUPP), "confinement requires Landlock, which is disabled on this kernel"),
        ] {
            let error = check_landlock_abi(probe).expect_err("an unsupported kernel is refused");
            assert_eq!(error.kind, KIND);
            assert_eq!(error.message, message);
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn prepared_policy_keeps_directory_authority_after_all_source_descriptors_close() {
        use super::{Confinement, FilesystemAuthority, TcpAccess, prepare};
        use crate::runtime::spawn::{ChildStdio, Command};
        use std::sync::Arc;
        use xsh_root::Root;

        let directory = tempfile::tempdir().expect("directory");
        let parent = directory.path().join("parent");
        let selected = parent.join("selected");
        let retained = directory.path().join("retained");
        let outside = directory.path().join("outside");
        std::fs::create_dir_all(selected.join("manuals")).expect("selected directories");
        std::fs::create_dir_all(outside.join("selected/manuals")).expect("outside directories");
        std::fs::write(selected.join("input"), b"selected input").expect("selected input");
        std::fs::write(outside.join("selected/input"), b"outside input").expect("outside input");
        std::fs::write(outside.join("selected/manuals/sentinel"), b"unchanged").expect("sentinel");
        let root = Root::open(&selected).expect("root");
        let manuals = root.open_dir("manuals").expect("manual root");
        let held = |root: &Root| FilesystemAuthority::Root(Arc::new(
            rustix::io::fcntl_dupfd_cloexec(root, 3).expect("duplicate authority"),
        ));
        let mut read: Vec<_> = ["/bin", "/usr", "/lib", "/lib64"].into_iter()
            .filter(|path| std::path::Path::new(path).exists())
            .map(|path| FilesystemAuthority::Path(path.into())).collect();
        read.push(held(&root));
        let confinement = Confinement { read, write: vec![held(&manuals)], tcp: TcpAccess::Denied };
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "cat \"$RETAINED/input\" > \"$RETAINED/manuals/copy\" || exit 10; cat \"$ORIGINAL/input\" && exit 11; echo changed > \"$ORIGINAL/manuals/sentinel\" && exit 12; exit 0"])
            .env("PATH", "/usr/bin:/bin")
            .env("RETAINED", retained.join("selected"))
            .env("ORIGINAL", &selected)
            .stdout(ChildStdio::Null)
            .stderr(ChildStdio::Null);
        if let Err(error) = prepare(&confinement, &mut command) {
            assert!(error.message.starts_with("confinement requires Landlock"), "{error:?}");
            return;
        }
        drop(confinement);
        drop(manuals);
        drop(root);
        std::fs::rename(&parent, &retained).expect("move ancestor after policy installation");
        std::os::unix::fs::symlink(&outside, &parent).expect("replace ancestor");
        let status = command.spawn().expect("spawn").wait().expect("reap");
        assert!(status.success(), "{status}");
        assert_eq!(std::fs::read(retained.join("selected/manuals/copy")).expect("copy"), b"selected input");
        assert_eq!(std::fs::read(outside.join("selected/manuals/sentinel")).expect("sentinel"), b"unchanged");
    }
}
