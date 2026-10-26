use super::{command_from_invocation, signal_info};
use crate::runtime::process::{ProcessInvocation, ProcessStatus};
use crate::runtime::value::RuntimeError;
use crate::source::Span;
use rustix::{io as rio, process as rp};
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

static ACTIVE: AtomicBool = AtomicBool::new(false);
static PENDING: AtomicU64 = AtomicU64::new(0);

extern "C" fn capture(signal: libc::c_int) {
    if (1..64).contains(&signal) {
        PENDING.fetch_or(1u64 << signal, Ordering::Relaxed);
    }
}

pub(crate) struct SignalCapture {
    previous: Vec<(i32, libc::sigaction)>,
    signals: Vec<(i32, String)>,
    previous_mask: Option<libc::sigset_t>,
}

impl SignalCapture {
    fn install(names: &[String], span: Span) -> Result<Self, RuntimeError> {
        let mut signals = Vec::new();
        for name in names {
            let info = signal_info(name, span)?;
            if !(1..64).contains(&info.number)
                || [libc::SIGKILL, libc::SIGSTOP, libc::SIGCHLD].contains(&info.number)
            {
                return Err(RuntimeError::new("invalid-signal", "signal cannot be captured").with_span(span));
            }
            if !signals.iter().any(|(number, _)| *number == info.number) {
                signals.push((info.number, info.name));
            }
        }
        if ACTIVE.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_err() {
            return Err(RuntimeError::new("unix-signal", "signal capture is already active").with_span(span));
        }
        PENDING.store(0, Ordering::Relaxed);
        let mut guard = Self { previous: Vec::new(), signals, previous_mask: None };
        for &(number, _) in &guard.signals {
            // SAFETY: sigaction structures are initialized before use and all pointers remain valid for the call.
            let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
            action.sa_sigaction = capture as *const () as usize;
            // SAFETY: sigemptyset initializes the mask in a live sigaction.
            unsafe { libc::sigemptyset(&mut action.sa_mask) };
            // SAFETY: zero is a valid initialization for the output sigaction storage.
            let mut old = unsafe { std::mem::zeroed() };
            // SAFETY: number is a validated catchable signal and both structures are live.
            if unsafe { libc::sigaction(number, &action, &mut old) } != 0 {
                return Err(host(io::Error::last_os_error(), span));
            }
            guard.previous.push((number, old));
        }
        Ok(guard)
    }

    fn restore(&mut self) -> io::Result<()> {
        while let Some((signal, old)) = self.previous.last() {
            // SAFETY: old came from sigaction for this signal and remains live.
            if unsafe { libc::sigaction(*signal, old, std::ptr::null_mut()) } != 0 {
                return Err(io::Error::last_os_error());
            }
            self.previous.pop();
        }
        Ok(())
    }

    fn restore_mask(&mut self) -> io::Result<()> {
        if let Some(mask) = self.previous_mask.as_ref() {
            // SAFETY: mask came from pthread_sigmask on the evaluator thread and remains initialized.
            let error = unsafe { libc::pthread_sigmask(libc::SIG_SETMASK, mask, std::ptr::null_mut()) };
            if error != 0 {
                return Err(io::Error::from_raw_os_error(error));
            }
            self.previous_mask = None;
        }
        Ok(())
    }

    pub(crate) fn take(&self) -> Vec<String> {
        let pending = PENDING.swap(0, Ordering::AcqRel);
        self.signals.iter().filter(|(number, _)| pending & (1u64 << number) != 0)
            .map(|(_, name)| name.clone()).collect()
    }

    fn forward(&self, pid: rp::Pid, span: Span) -> Result<(), RuntimeError> {
        let pending = PENDING.swap(0, Ordering::AcqRel);
        for &(number, _) in &self.signals {
            if pending & (1u64 << number) != 0 {
                // SAFETY: each number has been validated as a catchable signal.
                let signal = unsafe { rp::Signal::from_raw_unchecked(number) };
                match rp::kill_process(pid, signal) {
                    Ok(()) | Err(rio::Errno::SRCH) => {}
                    Err(error) => return Err(host(error.into(), span)),
                }
            }
        }
        Ok(())
    }
}

impl Drop for SignalCapture {
    fn drop(&mut self) {
        if let Err(error) = self.restore_mask() {
            eprintln!("unix-signal: cannot restore signal mask: {error}");
            std::process::abort();
        }
        if let Err(error) = self.restore() {
            eprintln!("unix-signal: cannot restore signal handlers: {error}");
            std::process::abort();
        }
        ACTIVE.store(false, Ordering::Release);
    }
}

fn host(error: io::Error, span: Span) -> RuntimeError {
    RuntimeError::host("unix-daemon", &error).with_span(span)
}

pub(crate) fn signal_setup(names: &[String], span: Span) -> Result<SignalCapture, RuntimeError> {
    let mut guard = SignalCapture::install(names, span)?;
    let mask = signal_mask(&guard.signals);
    // SAFETY: zero initialization provides writable storage for the previous mask.
    let mut previous = unsafe { std::mem::zeroed() };
    // SAFETY: mask is initialized and signal handlers are installed before pending signals are unblocked.
    let error = unsafe { libc::pthread_sigmask(libc::SIG_UNBLOCK, &mask, &mut previous) };
    if error != 0 {
        return Err(host(io::Error::from_raw_os_error(error), span));
    }
    guard.previous_mask = Some(previous);
    Ok(guard)
}

fn signal_mask(signals: &[(i32, String)]) -> libc::sigset_t {
    // SAFETY: sigemptyset initializes this storage before any use.
    let mut mask = unsafe { std::mem::zeroed() };
    // SAFETY: mask is live writable sigset_t storage and signal numbers were validated.
    unsafe {
        libc::sigemptyset(&mut mask);
        for (number, _) in signals {
            libc::sigaddset(&mut mask, *number);
        }
    }
    mask
}

pub(crate) fn daemon_start(
    invocation: &ProcessInvocation,
    names: &[String],
    span: Span,
) -> Result<Option<ProcessStatus>, RuntimeError> {
    if !invocation.redirections.is_empty() || invocation.timeout.is_some()
        || invocation.cpu_max.is_some() || invocation.namespaces.is_some()
        || invocation.accepted_exit_codes.is_some()
    {
        return Err(RuntimeError::new("unix-daemon", "daemon command requires inherited stdio and no timeout, CPU limit, namespace, or exit-code policy").with_span(span));
    }
    let _status_retention = crate::runtime::process::retain_child_status()
        .map_err(|error| host(error, span))?;
    let mut command = command_from_invocation(invocation, span)?;
    let mut capture = SignalCapture::install(names, span)?;
    let (reader, writer) = rustix::pipe::pipe_with(rustix::pipe::PipeFlags::CLOEXEC)
        .map_err(|error| host(error.into(), span))?;
    let writer_fd = writer.as_raw_fd();
    command.env("NOTIFY_FD", writer_fd.to_string());
    let mask = signal_mask(&capture.signals);
    // SAFETY: pre_exec performs only async-signal-safe descriptor and signal operations on live inherited descriptors.
    unsafe {
        command.pre_exec(move || {
            if libc::sigprocmask(libc::SIG_BLOCK, &mask, std::ptr::null_mut()) != 0 {
                return Err(io::Error::last_os_error());
            }
            let writer = std::os::fd::BorrowedFd::borrow_raw(writer_fd);
            rio::fcntl_setfd(writer, rio::FdFlags::empty()).map_err(io::Error::from)
        });
    }
    // SAFETY: both sigset_t pointers refer to valid storage. Blocking before fork prevents a pre-exec handler from consuming a child signal.
    let mut previous = unsafe { std::mem::zeroed() };
    // SAFETY: mask is initialized and previous is writable.
    let error = unsafe { libc::pthread_sigmask(libc::SIG_BLOCK, &mask, &mut previous) };
    if error != 0 {
        return Err(host(io::Error::from_raw_os_error(error), span));
    }
    capture.previous_mask = Some(previous);
    let spawned = command.spawn();
    let mask_restored = capture.restore_mask().map_err(|error| host(error, span));
    drop(writer);
    let mut child = spawned.map_err(|error| {
        if let Err(mask_error) = &mask_restored {
            eprintln!("unix-daemon: cannot restore startup signal mask: {}", mask_error.message);
        }
        host(error, span)
    })?;
    let pid = rp::Pid::from_raw(child.id() as i32).expect("spawn returns a positive pid");
    let result = mask_restored.and_then(|()| wait_ready(&reader, pid, &capture, span));
    let restored = capture.restore().map_err(|error| host(error, span));
    let result = match result {
        Err(error) if error.errno() == Some(i64::from(libc::ECHILD)) => {
            if let Err(cleanup) = restored {
                eprintln!("unix-daemon: cannot restore startup signal handlers: {}", cleanup.message);
            }
            return Err(error);
        }
        result => result,
    };
    let restored = restored.and_then(|()| capture.forward(pid, span));
    if result.as_ref().is_ok_and(|ready| *ready) && restored.is_ok() {
        return Ok(None);
    }
    if result.is_err() || restored.is_err() {
        match child.kill() {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::InvalidInput => {}
            Err(error) => eprintln!("unix-daemon: cannot terminate startup child: {error}"),
        }
    }
    let waited = child.wait().map_err(|error| host(error, span));
    if (result.is_err() || restored.is_err()) && let Err(error) = &waited {
        eprintln!("unix-daemon: cannot reap startup child: {}", error.message);
    }
    result?;
    restored?;
    let status = waited?;
    Ok(Some(match status.code() {
        Some(code) => ProcessStatus::exited(code),
        None => ProcessStatus::signaled(status.signal().expect("Unix exit has code or signal")),
    }))
}

fn wait_ready(
    reader: &std::os::fd::OwnedFd,
    pid: rp::Pid,
    capture: &SignalCapture,
    span: Span,
) -> Result<bool, RuntimeError> {
    let mut eof = false;
    loop {
        capture.forward(pid, span)?;
        let mut descriptors = [rustix::event::PollFd::new(reader, rustix::event::PollFlags::IN)];
        let timeout = rustix::time::Timespec { tv_sec: 0, tv_nsec: 10_000_000 };
        let polled = if eof {
            rustix::event::poll(&mut [], Some(&timeout))
        } else {
            rustix::event::poll(&mut descriptors, Some(&timeout))
        };
        match polled {
            Ok(0) | Err(rio::Errno::INTR) => {}
            Ok(_) => {
                let mut byte = [0];
                match rio::read(reader, &mut byte) {
                    Ok(1) => { capture.forward(pid, span)?; return Ok(true); }
                    Ok(_) => { eof = true; },
                    Err(rio::Errno::INTR) => continue,
                    Err(error) => return Err(host(error.into(), span)),
                }
            }
            Err(error) => return Err(host(error.into(), span)),
        }
        match rp::waitid(rp::WaitId::Pid(pid), rp::WaitIdOptions::NOHANG | rp::WaitIdOptions::EXITED | rp::WaitIdOptions::NOWAIT) {
            Ok(Some(_)) => return Ok(false),
            Ok(None) | Err(rio::Errno::INTR) => {}
            Err(error) => return Err(host(error.into(), span)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceId;
    use std::collections::BTreeMap;
    use std::process::Command;

    fn invocation(source: &str) -> ProcessInvocation {
        ProcessInvocation {
            target: b"/bin/sh".to_vec(),
            argv: vec![b"-c".to_vec(), source.as_bytes().to_vec()],
            cwd: std::env::current_dir().unwrap(),
            env: std::env::vars_os().map(|(key, value)| {
                use std::os::unix::ffi::OsStringExt;
                (key.into_vec(), value.into_vec())
            }).collect(),
            env_overlay: BTreeMap::new(),
            redirections: Vec::new(),
            timeout: None,
            cpu_max: None,
            accepted_exit_codes: None,
            namespaces: None,
        }
    }

    #[test]
    fn daemon_boundaries() {
        if std::env::var_os("XSH_DAEMON_BOUNDARY_TEST").is_none() {
            let output = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "modules::unix::daemon::tests::daemon_boundaries", "--nocapture"])
                .env("XSH_DAEMON_BOUNDARY_TEST", "1").output().unwrap();
            assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
            return;
        }
        let span = Span::new(SourceId::new(0), 0, 0);
        let signals = vec!["HUP".to_owned(), "TERM".to_owned()];
        match std::env::var("XSH_DAEMON_WORKER_TEST").ok().as_deref() {
            Some("fd-check") => {
                let fd = std::env::var("XSH_DAEMON_SAVED_FD").unwrap().parse::<i32>().unwrap();
                // SAFETY: fcntl queries a descriptor number and the kernel validates whether it is open.
                assert_eq!(unsafe { libc::fcntl(fd, libc::F_GETFD) }, -1);
                assert_eq!(io::Error::last_os_error().raw_os_error(), Some(libc::EBADF));
                return;
            }
            Some("worker") => {
                // SAFETY: getppid identifies the live daemon_start caller, whose scoped handlers capture both signals.
                unsafe {
                    assert_eq!(libc::kill(libc::getppid(), libc::SIGHUP), 0);
                    assert_eq!(libc::kill(libc::getppid(), libc::SIGTERM), 0);
                }
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
                loop {
                    // SAFETY: sigpending initializes this live storage before sigismember reads it.
                    let mut pending: libc::sigset_t = unsafe { std::mem::zeroed() };
                    // SAFETY: all pointers are live and both signal numbers are valid.
                    let received = unsafe {
                        assert_eq!(libc::sigpending(&mut pending), 0);
                        libc::sigismember(&pending, libc::SIGHUP) == 1
                            && libc::sigismember(&pending, libc::SIGTERM) == 1
                    };
                    if received { break; }
                    assert!(std::time::Instant::now() < deadline, "startup signals were not forwarded while blocked");
                    std::thread::yield_now();
                }
                let guard = signal_setup(&signals, span).unwrap();
                assert_eq!(guard.take(), signals);
                let fd = std::env::var("NOTIFY_FD").unwrap().parse::<i32>().unwrap();
                // SAFETY: NOTIFY_FD is the open writer owned by this worker until closed below.
                let borrowed = unsafe { std::os::fd::BorrowedFd::borrow_raw(fd) };
                let saved = rio::fcntl_dupfd_cloexec(borrowed, 128).unwrap();
                // SAFETY: the worker owns the inherited writer and closes it exactly once.
                unsafe { rio::close(fd) };
                let status = Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", "modules::unix::daemon::tests::daemon_boundaries", "--nocapture"])
                    .env("XSH_DAEMON_WORKER_TEST", "fd-check")
                    .env("XSH_DAEMON_SAVED_FD", saved.as_raw_fd().to_string())
                    .status().unwrap();
                assert!(status.success());
                assert_eq!(rio::write(&saved, b"x").unwrap(), 1);
                return;
            }
            None => {}
            Some(other) => panic!("unknown worker mode {other}"),
        }
        let mask = signal_mask(&[(libc::SIGHUP, "HUP".into()), (libc::SIGTERM, "TERM".into())]);
        // SAFETY: mask is initialized; this isolated subprocess owns its thread signal mask.
        assert_eq!(unsafe { libc::pthread_sigmask(libc::SIG_BLOCK, &mask, std::ptr::null_mut()) }, 0);
        // SAFETY: both catchable signals are blocked until capture is installed below.
        unsafe {
            assert_eq!(libc::raise(libc::SIGHUP), 0);
            assert_eq!(libc::raise(libc::SIGTERM), 0);
        }
        let guard = signal_setup(&signals, span).unwrap();
        assert_eq!(guard.take(), signals);
        assert!(guard.take().is_empty());
        drop(guard);
        // SAFETY: both signals were drained and restoring an empty mask returns this isolated test thread to normal delivery.
        assert_eq!(unsafe { libc::pthread_sigmask(libc::SIG_UNBLOCK, &mask, std::ptr::null_mut()) }, 0);
        // SAFETY: this isolated subprocess owns the signal disposition and uses initialized sigaction storage.
        let mut no_wait: libc::sigaction = unsafe { std::mem::zeroed() };
        no_wait.sa_sigaction = libc::SIG_DFL;
        no_wait.sa_flags = libc::SA_NOCLDWAIT;
        // SAFETY: sigemptyset initializes the mask before installing this disposition.
        unsafe { libc::sigemptyset(&mut no_wait.sa_mask) };
        // SAFETY: output storage remains live for the query and restoration below.
        let mut old_chld = unsafe { std::mem::zeroed() };
        // SAFETY: SIGCHLD is valid and both action structures remain initialized and live.
        assert_eq!(unsafe { libc::sigaction(libc::SIGCHLD, &no_wait, &mut old_chld) }, 0);
        let no_status = daemon_start(&invocation("exit 0"), &signals, span).unwrap_err();
        assert_eq!(no_status.errno(), Some(i64::from(libc::ECHILD)));
        assert!(!ACTIVE.load(Ordering::Acquire));
        crate::runtime::process::set_sigchld_action(&old_chld).unwrap();
        let mut unrelated = Command::new("/bin/sh").args(["-c", "exit 37"]).spawn().unwrap();
        let exited = daemon_start(&invocation("exit 23"), &signals, span).unwrap().unwrap();
        assert_eq!(exited.code, Some(23));
        assert_eq!(unrelated.wait().unwrap().code(), Some(37));
        assert!(daemon_start(&invocation("eval \"printf x >&$NOTIFY_FD\""), &signals, span).unwrap().is_none());
        let exited = daemon_start(&invocation("eval \"exec $NOTIFY_FD>&-\"; exit 41"), &signals, span).unwrap().unwrap();
        assert_eq!(exited.code, Some(41));
        let mut missing = invocation("exit 0");
        missing.target = b"/xsh-daemon-missing-executable".to_vec();
        assert!(daemon_start(&missing, &signals, span).is_err());
        let mut worker = invocation("");
        use std::os::unix::ffi::OsStrExt;
        worker.target = std::env::current_exe().unwrap().as_os_str().as_bytes().to_vec();
        worker.argv = ["--exact", "modules::unix::daemon::tests::daemon_boundaries", "--nocapture"].into_iter().map(|argument| argument.as_bytes().to_vec()).collect();
        worker.env.insert(b"XSH_DAEMON_WORKER_TEST".to_vec(), b"worker".to_vec());
        assert!(daemon_start(&worker, &signals, span).unwrap().is_none());
        assert!(!ACTIVE.load(Ordering::Acquire));
        // SAFETY: storage is writable and sigaction only queries the current process's disposition.
        let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
        // SAFETY: the output pointer is live and the action pointer is null for a query.
        assert_eq!(unsafe { libc::sigaction(libc::SIGTERM, std::ptr::null(), &mut action) }, 0);
        assert_ne!(action.sa_sigaction, capture as *const () as usize);
    }
}
