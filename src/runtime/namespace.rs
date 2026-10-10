//! Namespace changes a spawned child makes between fork and exec.
//!
//! Scripts run on a worker thread of a multithreaded process, and the kernel
//! refuses `unshare(CLONE_NEWUSER)` and `setns` into a user namespace to any
//! process with more than one thread. A forked child has exactly one, so every
//! namespace change a script asks for is made there, and the command is
//! executed from the same child. Whatever the child does between fork and exec
//! must be async-signal-safe: everything it needs is prepared in the parent.

use crate::runtime::value::RunError;

/// A kind of namespace, named as the kernel names it under `/proc/PID/ns`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NamespaceKind {
    Mount,
    Uts,
    Ipc,
    Net,
    Pid,
    User,
    Cgroup,
    Time,
}

impl NamespaceKind {
    pub const NAMES: [&'static str; 8] = ["mnt", "uts", "ipc", "net", "pid", "user", "cgroup", "time"];

    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "mnt" => Self::Mount,
            "uts" => Self::Uts,
            "ipc" => Self::Ipc,
            "net" => Self::Net,
            "pid" => Self::Pid,
            "user" => Self::User,
            "cgroup" => Self::Cgroup,
            "time" => Self::Time,
            _ => return None,
        })
    }

    #[cfg(target_os = "linux")]
    fn clone_flag(self) -> libc::c_int {
        // CLONE_NEWTIME is not named by every libc release.
        const CLONE_NEWTIME: libc::c_int = 0x0000_0080;
        match self {
            Self::Mount => libc::CLONE_NEWNS,
            Self::Uts => libc::CLONE_NEWUTS,
            Self::Ipc => libc::CLONE_NEWIPC,
            Self::Net => libc::CLONE_NEWNET,
            Self::Pid => libc::CLONE_NEWPID,
            Self::User => libc::CLONE_NEWUSER,
            Self::Cgroup => libc::CLONE_NEWCGROUP,
            Self::Time => CLONE_NEWTIME,
        }
    }
}

/// How the mount tree is marked after a new mount namespace is created.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Propagation {
    #[default]
    Unchanged,
    Private,
    Shared,
    Slave,
}

impl Propagation {
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "unchanged" => Self::Unchanged,
            "private" => Self::Private,
            "shared" => Self::Shared,
            "slave" => Self::Slave,
            _ => return None,
        })
    }

    #[cfg(target_os = "linux")]
    fn mount_flag(self) -> Option<rustix::mount::MountPropagationFlags> {
        use rustix::mount::MountPropagationFlags as Flags;
        match self {
            Self::Unchanged => None,
            Self::Private => Some(Flags::PRIVATE),
            Self::Shared => Some(Flags::SHARED),
            Self::Slave => Some(Flags::DOWNSTREAM),
        }
    }
}

/// The ordered namespace work of one child, applied as: unshare the listed
/// kinds, join the namespace files, map the caller to root, fork so the
/// command is the first process of a new pid namespace, set mount propagation,
/// mount a fresh procfs, enter the root and working directories, then change
/// credentials.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NamespaceEntry {
    pub unshare: Vec<NamespaceKind>,
    /// Namespace files (`/proc/PID/ns/net`), opened before any change so that
    /// a path resolves in the caller's mount namespace, and entered in order.
    pub join: Vec<std::path::PathBuf>,
    pub map_root_user: bool,
    pub propagation: Propagation,
    pub mount_proc: Option<std::path::PathBuf>,
    pub fork: bool,
    /// Directories opened before any change, like `join`.
    pub root: Option<std::path::PathBuf>,
    pub cwd: Option<std::path::PathBuf>,
    /// Drop the supplementary groups before credentials change.
    pub drop_groups: bool,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
}

/// The failing child step is carried to the parent inside the OS error
/// number the spawn reports: the step in the bits above the 12 an errno uses.
const STEP_SHIFT: u32 = 12;
const ERRNO_MASK: i32 = (1 << STEP_SHIFT) - 1;
const STEP_KIND_PREFIX: &str = "namespace-step-";
/// Kind of the failure to open a file the entry names, before any fork.
const OPEN_KIND: &str = "namespace-open";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum Step {
    Unshare = 1,
    Join,
    SetgroupsFile,
    UidMap,
    GidMap,
    Fork,
    Propagation,
    MountProc,
    RootDirectory,
    Chroot,
    WorkingDirectory,
    Setgroups,
    Setgid,
    Setuid,
}

impl Step {
    const ALL: [Step; 14] = [
        Step::Unshare,
        Step::Join,
        Step::SetgroupsFile,
        Step::UidMap,
        Step::GidMap,
        Step::Fork,
        Step::Propagation,
        Step::MountProc,
        Step::RootDirectory,
        Step::Chroot,
        Step::WorkingDirectory,
        Step::Setgroups,
        Step::Setgid,
        Step::Setuid,
    ];

    fn from_code(code: u8) -> Option<Self> {
        Self::ALL.into_iter().find(|step| *step as u8 == code)
    }
}

impl NamespaceEntry {
    /// The namespace step failure a completed run reports as an exec failure
    /// in its status, worded as the util-linux tools word it.
    pub fn status_failure(&self, status: &crate::runtime::process::ProcessStatus) -> Option<RunError> {
        status.segments.iter().find_map(|segment| {
            let kind = segment.error_kind.as_deref()?;
            let message = segment.error_message.clone().unwrap_or_default();
            if kind == OPEN_KIND {
                return Some(RunError::new("spawn", message));
            }
            kind.starts_with(STEP_KIND_PREFIX)
                .then(|| self.describe_failure(RunError::new(kind, message)))
        })
    }

    /// The failure a child step reported, worded as the util-linux tools word
    /// it, or `error` unchanged when it is not a namespace step.
    pub fn describe_failure(&self, error: RunError) -> RunError {
        let Some(code) = error
            .kind
            .strip_prefix(STEP_KIND_PREFIX)
            .and_then(|code| code.parse::<u8>().ok())
            .and_then(Step::from_code)
        else {
            return error;
        };
        let what = match code {
            Step::Unshare => "unshare failed".to_string(),
            Step::Join => "reassociate to namespaces failed".to_string(),
            Step::SetgroupsFile => "write failed /proc/self/setgroups".to_string(),
            Step::UidMap => "write failed /proc/self/uid_map".to_string(),
            Step::GidMap => "write failed /proc/self/gid_map".to_string(),
            Step::Fork => "fork failed".to_string(),
            Step::Propagation => "cannot change root filesystem propagation".to_string(),
            Step::MountProc => format!(
                "mount {} failed",
                self.mount_proc
                    .as_deref()
                    .unwrap_or_else(|| std::path::Path::new("/proc"))
                    .display()
            ),
            Step::RootDirectory => "change directory by root file descriptor failed".to_string(),
            Step::Chroot => "chroot failed".to_string(),
            Step::WorkingDirectory => {
                "change directory by working directory file descriptor failed".to_string()
            }
            Step::Setgroups => "setgroups failed".to_string(),
            Step::Setgid => "setgid() failed".to_string(),
            Step::Setuid => "setuid() failed".to_string(),
        };
        RunError::new("spawn", format!("{what}: {}", error.message))
    }
}

/// The spawn failure a namespace step reported, or `None` when `raw` is an
/// ordinary OS error number. The step travels in the kind and the real error
/// text in the message until `NamespaceEntry::describe_failure` words them.
pub fn step_failure(raw: i32) -> Option<RunError> {
    let code = raw >> STEP_SHIFT;
    if raw < 0 || code == 0 {
        return None;
    }
    let step = u8::try_from(code).ok().and_then(Step::from_code)?;
    let error = std::io::Error::from_raw_os_error(raw & ERRNO_MASK);
    Some(RunError::new(
        format!("{STEP_KIND_PREFIX}{}", step as u8),
        strerror(&error),
    ))
}

/// The `strerror` text of an I/O error, without Rust's ` (os error N)`.
fn strerror(error: &std::io::Error) -> String {
    let text = error.to_string();
    match text.find(" (os error ") {
        Some(at) => text[..at].to_string(),
        None => text,
    }
}

#[cfg(target_os = "linux")]
pub use linux::prepare;
#[cfg(not(target_os = "linux"))]
pub use unsupported::prepare;

#[cfg(target_os = "linux")]
mod linux {
    use super::{ERRNO_MASK, NamespaceEntry, STEP_SHIFT, Step, strerror};
    use crate::runtime::value::RunError;
    use rustix::fs::{Mode, OFlags};
    use rustix::io::Errno;
    use rustix::mount::{MountFlags, MountPropagationFlags};
    use rustix::process::{self as rprocess, Pid, Signal, WaitOptions};
    use std::ffi::CString;
    use std::io;
    use rustix::thread::UnshareFlags;
    use std::os::fd::{AsFd, OwnedFd};
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::process::CommandExt;
    use std::path::Path;
    use std::process::Command;

    /// Opens what the child needs in the caller's namespaces and arranges for
    /// the child to apply `entry` between fork and exec.
    pub fn prepare(entry: &NamespaceEntry, command: &mut Command) -> Result<(), RunError> {
        let plan = ChildPlan::open(entry)?;
        // SAFETY: `ChildPlan::apply` calls only async-signal-safe functions and
        // allocates nothing; every buffer and descriptor it uses was created
        // here, before the fork.
        unsafe {
            command.pre_exec(move || plan.apply());
        }
        Ok(())
    }

    struct ChildPlan {
        unshare_flags: libc::c_int,
        join: Vec<OwnedFd>,
        user_map: Option<UserMap>,
        fork: bool,
        propagation: Option<MountPropagationFlags>,
        mount_proc: Option<CString>,
        root: Option<OwnedFd>,
        cwd: Option<OwnedFd>,
        drop_groups: bool,
        uid: Option<u32>,
        gid: Option<u32>,
    }

    struct UserMap {
        uid: Vec<u8>,
        gid: Vec<u8>,
    }

    fn open_error(path: &Path, error: io::Error) -> RunError {
        RunError::new(
            super::OPEN_KIND,
            format!("cannot open {}: {}", path.display(), strerror(&error)),
        )
    }

    fn cstring(path: &Path) -> Result<CString, RunError> {
        CString::new(path.as_os_str().as_bytes())
            .map_err(|_| RunError::new("spawn", format!("{}: path contains NUL", path.display())))
    }

    fn open_fd(path: &Path, flags: OFlags) -> Result<OwnedFd, RunError> {
        let c_path = cstring(path)?;
        rustix::fs::open(c_path.as_c_str(), flags | OFlags::CLOEXEC, Mode::empty())
            .map_err(|errno| open_error(path, io::Error::from(errno)))
    }

    impl ChildPlan {
        fn open(entry: &NamespaceEntry) -> Result<Self, RunError> {
            // The directories open first, then the namespace files, so the
            // first failure reported is the first one a user asked for.
            let root = match &entry.root {
                Some(path) => Some(open_fd(path, OFlags::RDONLY)?),
                None => None,
            };
            let cwd = match &entry.cwd {
                Some(path) => Some(open_fd(path, OFlags::RDONLY)?),
                None => None,
            };
            let mut join = Vec::with_capacity(entry.join.len());
            for path in &entry.join {
                join.push(open_fd(path, OFlags::RDONLY)?);
            }
            let mut unshare_flags = 0;
            for kind in &entry.unshare {
                unshare_flags |= kind.clone_flag();
            }
            // Read before the user namespace exists, where the effective
            // identity still names the caller's own.
            let user_map = entry.map_root_user.then(|| UserMap {
                uid: format!("0 {} 1\n", rprocess::geteuid().as_raw()).into_bytes(),
                gid: format!("0 {} 1\n", rprocess::getegid().as_raw()).into_bytes(),
            });
            let mount_proc = match &entry.mount_proc {
                Some(path) => Some(cstring(path)?),
                None => None,
            };
            // Propagation applies to a mount namespace this child creates.
            let propagation = if entry.unshare.contains(&super::NamespaceKind::Mount) {
                entry.propagation.mount_flag()
            } else {
                None
            };
            Ok(Self {
                unshare_flags,
                join,
                user_map,
                fork: entry.fork,
                propagation,
                mount_proc,
                root,
                cwd,
                drop_groups: entry.drop_groups,
                uid: entry.uid,
                gid: entry.gid,
            })
        }

        fn apply(&self) -> io::Result<()> {
            // SAFETY: each call below is a raw system call or an
            // async-signal-safe libc wrapper over valid, preallocated data.
            unsafe {
                if self.unshare_flags != 0
                    && let Err(errno) = rustix::thread::unshare_unsafe(
                        UnshareFlags::from_bits_retain(self.unshare_flags as u32),
                    )
                {
                    return fail_with(Step::Unshare, errno);
                }
                for fd in &self.join {
                    if let Err(errno) = rustix::thread::move_into_link_name_space(fd.as_fd(), None) {
                        return fail_with(Step::Join, errno);
                    }
                }
                if let Some(map) = &self.user_map {
                    write_file(c"/proc/self/setgroups", b"deny", Step::SetgroupsFile)?;
                    write_file(c"/proc/self/uid_map", &map.uid, Step::UidMap)?;
                    write_file(c"/proc/self/gid_map", &map.gid, Step::GidMap)?;
                }
                if self.fork {
                    fork_and_relay()?;
                }
                if let Some(flag) = self.propagation
                    && let Err(errno) = rustix::mount::mount_change(
                        c"/",
                        MountPropagationFlags::REC | flag,
                    )
                {
                    return fail_with(Step::Propagation, errno);
                }
                if let Some(target) = &self.mount_proc
                    && let Err(errno) = rustix::mount::mount(
                        c"proc",
                        target.as_c_str(),
                        c"proc",
                        MountFlags::NOSUID | MountFlags::NOEXEC | MountFlags::NODEV,
                        None,
                    )
                {
                    return fail_with(Step::MountProc, errno);
                }
                if let Some(root) = &self.root {
                    if let Err(errno) = rprocess::fchdir(root) {
                        return fail_with(Step::RootDirectory, errno);
                    }
                    if let Err(errno) = rprocess::chroot(c".") {
                        return fail_with(Step::Chroot, errno);
                    }
                }
                if let Some(cwd) = &self.cwd
                    && let Err(errno) = rprocess::fchdir(cwd)
                {
                    return fail_with(Step::WorkingDirectory, errno);
                }
                if self.drop_groups && libc::setgroups(0, std::ptr::null()) != 0 {
                    return fail(Step::Setgroups);
                }
                if let Some(gid) = self.gid
                    && libc::setgid(gid) != 0
                {
                    return fail(Step::Setgid);
                }
                if let Some(uid) = self.uid
                    && libc::setuid(uid) != 0
                {
                    return fail(Step::Setuid);
                }
            }
            Ok(())
        }
    }

    /// Reports `step` with the error number of the failed call, in the
    /// encoding `decode_step_error` reads.
    fn fail(step: Step) -> io::Result<()> {
        let errno = io::Error::last_os_error().raw_os_error().unwrap_or(libc::EINVAL);
        Err(io::Error::from_raw_os_error(
            ((step as i32) << STEP_SHIFT) | (errno & ERRNO_MASK),
        ))
    }

    /// As `fail`, for a call that returned its error number instead of
    /// leaving it in the C library's errno.
    fn fail_with(step: Step, errno: Errno) -> io::Result<()> {
        Err(step_error(step, errno))
    }

    fn step_error(step: Step, errno: Errno) -> io::Error {
        io::Error::from_raw_os_error(
            ((step as i32) << STEP_SHIFT) | (errno.raw_os_error() & ERRNO_MASK),
        )
    }

    fn write_file(path: &std::ffi::CStr, bytes: &[u8], step: Step) -> io::Result<()> {
        let file = rustix::fs::open(path, OFlags::WRONLY | OFlags::CLOEXEC, Mode::empty())
            .map_err(|errno| step_error(step, errno))?;
        match rustix::io::write(&file, bytes) {
            Ok(written) if written == bytes.len() => Ok(()),
            Ok(_) => fail_with(step, Errno::IO),
            Err(errno) => fail_with(step, errno),
        }
    }

    /// Forks once more so the command, not this process, is the first child of
    /// a pid namespace this process created or joined. The parent half never
    /// returns: it waits and ends the way the command did, a signal death
    /// being repeated on itself so the caller sees the same status.
    unsafe fn fork_and_relay() -> io::Result<()> {
        // SAFETY: fork has no preconditions; this process is single-threaded.
        let pid = unsafe { libc::fork() };
        if pid < 0 {
            return fail(Step::Fork);
        }
        if pid == 0 {
            return Ok(());
        }
        // SAFETY: only raw system calls on this process's own descriptors and
        // status. The spawn machinery of the caller waits for the exec of the
        // command by watching a close-on-exec pipe, which this process holds
        // open until it exits unless every such descriptor is closed here.
        unsafe {
            if libc::syscall(libc::SYS_close_range, 3, u32::MAX, 0) != 0 {
                for fd in 3..4096 {
                    // A descriptor that is not open is the common case here.
                    rustix::io::close(fd);
                }
            }
            // The child was just forked, so its pid is a positive process id.
            let child = Pid::from_raw_unchecked(pid);
            let status = loop {
                match rprocess::waitpid(Some(child), WaitOptions::empty()) {
                    Ok(Some((_, status))) => break status,
                    Err(Errno::INTR) => continue,
                    Ok(None) | Err(_) => libc::_exit(1),
                }
            };
            if let Some(signal) = status.terminating_signal() {
                libc::signal(signal, libc::SIG_DFL);
                let mut set: libc::sigset_t = std::mem::zeroed();
                libc::sigemptyset(&mut set);
                libc::sigaddset(&mut set, signal);
                libc::sigprocmask(libc::SIG_UNBLOCK, &set, std::ptr::null_mut());
                // The signal number came from the kernel's wait status.
                let _ = rprocess::kill_process(
                    rprocess::getpid(),
                    Signal::from_raw_unchecked(signal),
                );
                libc::_exit(128 + signal);
            }
            libc::_exit(status.exit_status().unwrap_or(1));
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod unsupported {
    use super::NamespaceEntry;
    use crate::runtime::value::RunError;
    use std::process::Command;

    pub fn prepare(_entry: &NamespaceEntry, _command: &mut Command) -> Result<(), RunError> {
        Err(RunError::new(
            "unsupported-platform",
            "namespaces are only available on Linux",
        ))
    }
}
