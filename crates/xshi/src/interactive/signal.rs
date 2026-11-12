use super::sys;
use std::os::fd::RawFd;
use std::sync::atomic::{AtomicI32, Ordering};
use xsh::process::{ForegroundTerminal, ProcessGroup};

// SAFETY: These are written once in init() before any signal can fire,
// then only read from handler() (async-signal context) and read_signal()
// (main thread). The shell is single-threaded, so no data race.
static mut PIPE_WRITE: RawFd = -1;
static mut PIPE_READ: RawFd = -1;

static FOREGROUND_JOB_GROUP: AtomicI32 = AtomicI32::new(0);

/// The terminal handed to a foreground job. While it is held, SIGTERM and
/// SIGHUP end the job's process group before the shell exits.
pub struct ForegroundJob {
    _terminal: Option<ForegroundTerminal>,
}

impl ForegroundJob {
    pub fn take(group: ProcessGroup) -> Self {
        let terminal = ForegroundTerminal::take(group);
        if terminal.is_some() {
            FOREGROUND_JOB_GROUP.store(group.id(), Ordering::SeqCst);
        }
        Self {
            _terminal: terminal,
        }
    }
}

impl Drop for ForegroundJob {
    fn drop(&mut self) {
        FOREGROUND_JOB_GROUP.store(0, Ordering::SeqCst);
    }
}

/// Kills and reaps the foreground job's process group and takes the terminal
/// back. Async-signal-safe.
pub(super) fn end_foreground_job() {
    let group = FOREGROUND_JOB_GROUP.swap(0, Ordering::SeqCst);
    // SAFETY: getpgrp, kill, waitpid, and tcsetpgrp are async-signal-safe, and
    // the group is a job this shell started in its own process group.
    unsafe {
        let shell = libc::getpgrp();
        if group <= 0 || group == shell {
            return;
        }
        libc::kill(-group, libc::SIGKILL);
        loop {
            if libc::waitpid(-group, std::ptr::null_mut(), 0) > 0 {
                continue;
            }
            if std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            break;
        }
        libc::tcsetpgrp(libc::STDIN_FILENO, shell);
    }
}

/// Creates the signal self-pipe and installs the SIGWINCH handler.
/// Returns the read-end fd for polling.
pub fn init() -> RawFd {
    let (read_fd, write_fd) =
        sys::pipe_nonblock_cloexec().expect("pipe() failed for signal self-pipe");

    // SAFETY: Called once at startup before any signals are installed.
    // Single-threaded — no concurrent access.
    unsafe {
        PIPE_READ = read_fd;
        PIPE_WRITE = write_fd;
    }

    // SIGCHLD does not need to wake the input poller: external jobs are waited
    // synchronously. SIGINT, SIGTSTP, SIGTTIN, SIGTTOU, SIGQUIT, and SIGPIPE
    // are ignored process-wide by `install_interactive_signal_handlers`, so
    // only the terminal-resize notification is routed through the self-pipe.
    install_handler(rustix::process::Signal::WINCH);

    // SAFETY: PIPE_READ was set above, single-threaded.
    unsafe { PIPE_READ }
}

/// Routes SIGINT through the editor's own event loop while the line editor
/// owns the terminal. The process-wide disposition ignores SIGINT so external
/// jobs can handle it; the editor restores that disposition when it hands the
/// terminal back.
pub fn install_interrupt_handler() -> Option<libc::sigaction> {
    // SAFETY: the action is zero-initialized plain data, the handler is
    // async-signal-safe, and `previous` is owned by the caller.
    unsafe {
        let mut previous: libc::sigaction = std::mem::zeroed();
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = handler as *const () as usize;
        action.sa_flags = 0;
        libc::sigemptyset(&mut action.sa_mask);
        if libc::sigaction(libc::SIGINT, &action, &mut previous) != 0 {
            return None;
        }
        Some(previous)
    }
}

pub fn restore_interrupt_handler(previous: Option<libc::sigaction>) {
    if let Some(previous) = previous {
        // SAFETY: `previous` was filled by sigaction for SIGINT.
        unsafe {
            libc::sigaction(libc::SIGINT, &previous, std::ptr::null_mut());
        }
    }
}

/// Read one signal byte from the self-pipe. Returns the signal number or None.
pub fn read_signal() -> Option<i32> {
    let mut byte = 0u8;
    let fd = unsafe { std::os::fd::BorrowedFd::borrow_raw(PIPE_READ) };
    rustix::io::read(fd, std::slice::from_mut(&mut byte))
        .ok()
        .filter(|&n| n == 1)
        .map(|_| byte as i32)
}

fn install_handler(sig: rustix::process::Signal) {
    #[cfg(target_os = "linux")]
    unsafe {
        // The Rustix kernel_sigaction trampoline segfaults when a signal is
        // delivered to a statically linked musl binary.
        // let action = rustix::runtime::KernelSigaction {
        //     sa_handler_kernel: Some(handler),
        //     sa_flags: rustix::runtime::KernelSigactionFlags::RESTART,
        //     sa_restorer: None,
        //     sa_mask: rustix::runtime::KernelSigSet::empty(),
        // };
        // let _ = rustix::runtime::kernel_sigaction(sig, Some(action));
        let _ = libc::signal(sig.as_raw(), handler as *const () as usize);
    }

    #[cfg(target_os = "macos")]
    unsafe {
        darwin_sigaction(sig.as_raw(), handler as *const () as usize, 0x0002);
    }
}

unsafe extern "C" fn handler(sig: i32) {
    let byte = sig as u8;
    // SAFETY: write() is async-signal-safe. PIPE_WRITE is valid (set in init).
    // O_NONBLOCK ensures we never block in the handler. If the pipe is full,
    // the signal was already pending — dropping the byte is safe.
    unsafe {
        let _ = libc::write(PIPE_WRITE, &byte as *const u8 as *const libc::c_void, 1);
    }
}

#[cfg(target_os = "macos")]
#[repr(C)]
struct DarwinSigaction {
    sa_sigaction: usize,
    sa_mask: u32,
    sa_flags: core::ffi::c_int,
}

#[cfg(target_os = "macos")]
unsafe fn darwin_sigaction(signal: i32, handler: usize, flags: core::ffi::c_int) {
    unsafe extern "C" {
        fn sigaction(
            signal: core::ffi::c_int,
            action: *const DarwinSigaction,
            old_action: *mut DarwinSigaction,
        ) -> core::ffi::c_int;
    }
    let action = DarwinSigaction {
        sa_sigaction: handler,
        sa_mask: 0,
        sa_flags: flags,
    };
    let _ = unsafe { sigaction(signal, &action, std::ptr::null_mut()) };
}
