#![allow(clippy::single_call_fn)]

use crate::modules::process::{list_processes, signal_info};
use crate::runtime::cgroup::{CgroupError, CgroupScope};
use crate::runtime::process::{
    ProcessInvocation, ProcessSegmentStatus, ProcessSegmentStatusKind, ProcessStatus,
    resolve_executable,
};
use crate::runtime::value::{RuntimeError, Value};
use crate::source::Span;
use rustix::fd::BorrowedFd;
use rustix::{fs as rfs, io as rio, pipe as rpipe, process as rprocess, stdio, termios};
use std::ffi::{CString, OsString};
use std::fs::File;
use std::io;
use std::num::NonZeroI32;
use std::os::fd::{AsRawFd, IntoRawFd, RawFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::{Duration, Instant};

const LINUX_COMM_LIMIT: usize = 15;
const WAIT_POLL: Duration = Duration::from_millis(100);
static PENDING_SIGNAL: AtomicI32 = AtomicI32::new(0);

#[derive(Clone, Debug, Eq, PartialEq)]
struct ProcessTarget {
    pid: i64,
    command: String,
    argv0: String,
}

pub(crate) fn spawn_process_group(
    invocation: &ProcessInvocation,
    notify: bool,
    span: Span,
) -> Result<Value, RuntimeError> {
    match spawn_process_group_native(invocation, notify, span) {
        Ok(child) => Ok(spawn_record_from_native(child)),
        Err(error) => Ok(Value::err(Value::Error(Box::new(error)))),
    }
}

pub(crate) fn spawn_logged_process_group(
    invocation: &ProcessInvocation,
    logger: &ProcessInvocation,
    span: Span,
) -> Result<Value, RuntimeError> {
    match spawn_logged_process_group_native(invocation, logger, span) {
        Ok(child) => Ok(logged_spawn_record_from_native(child)),
        Err(error) => Ok(Value::err(Value::Error(Box::new(error)))),
    }
}

pub(crate) fn spawn_with_tty(
    invocation: &ProcessInvocation,
    tty: &str,
    span: Span,
) -> Result<Value, RuntimeError> {
    match spawn_with_tty_native(invocation, tty, span) {
        Ok(child) => Ok(spawn_record_from_native(child)),
        Err(error) => Ok(Value::err(Value::Error(Box::new(error)))),
    }
}

pub(crate) fn kill_process_group(pid: i64, signal: i32, span: Span) -> Result<Value, RuntimeError> {
    match kill_process_group_native(pid, signal, span) {
        Ok(()) => Ok(ok_unit()),
        Err(error) => Ok(Value::err(Value::Error(Box::new(error)))),
    }
}

pub(crate) fn pid1_setup_native(
    signals: &[String],
    subreaper: bool,
    allow_non_pid1: bool,
    span: Span,
) -> Result<(), RuntimeError> {
    let pid = rprocess::getpid().as_raw_nonzero().get();
    if pid != 1 && !allow_non_pid1 {
        return Err(RuntimeError::new("pid1-required", "PID must be 1").with_span(span));
    }

    let mut signal_numbers = Vec::new();
    for signal in signals {
        let info = signal_info(signal, span)?;
        signal_numbers.push(info.number);
    }
    if !signal_numbers.contains(&libc::SIGCHLD) {
        signal_numbers.push(libc::SIGCHLD);
    }

    for signal in signal_numbers {
        if let Err(error) = install_signal_handler(signal) {
            return Err(RuntimeError::new("unix-signal", error.to_string()).with_span(span));
        }
    }

    if subreaper && let Err(error) = enable_child_subreaper() {
        return Err(RuntimeError::new("unix-subreaper", error.to_string()).with_span(span));
    }

    Ok(())
}

// Wait for the next PID 1 event. With no `deadline`, this is a single bounded
// poll: it checks for pending children/signals, sleeps one `WAIT_POLL` grain,
// re-checks, and otherwise returns a `Poll` event (historical behavior). With a
// `deadline`, it instead blocks across multiple grains until a child or signal
// arrives or the deadline elapses, returning a `Timeout` event in the latter
// case — so a supervisor can sleep until its next scheduled action.
pub(crate) fn wait_pid1_event_native(
    deadline: Option<Instant>,
    span: Span,
) -> Result<Pid1Event, RuntimeError> {
    loop {
        let children = drain_child_events_native(span)?;
        if !children.is_empty() {
            return Ok(Pid1Event::children(children));
        }
        if let Some(signal) = take_signal() {
            return Ok(Pid1Event::signal(signal_name(signal)));
        }

        match deadline {
            None => {
                std::thread::sleep(WAIT_POLL);
                let children = drain_child_events_native(span)?;
                if !children.is_empty() {
                    return Ok(Pid1Event::children(children));
                }
                if let Some(signal) = take_signal() {
                    return Ok(Pid1Event::signal(signal_name(signal)));
                }
                return Ok(Pid1Event::poll());
            }
            Some(deadline) => {
                let now = Instant::now();
                if now >= deadline {
                    return Ok(Pid1Event::timeout());
                }
                std::thread::sleep(std::cmp::min(WAIT_POLL, deadline - now));
            }
        }
    }
}

pub(crate) fn reap_child_events(span: Span) -> Result<Value, RuntimeError> {
    Ok(Value::ok(Value::List(
        drain_child_events_native(span)?
            .into_iter()
            .map(child_event_record)
            .collect(),
    )))
}

pub(crate) fn spawn_process_group_native(
    invocation: &ProcessInvocation,
    notify: bool,
    span: Span,
) -> Result<SpawnedChild, RuntimeError> {
    spawn_child(invocation, None, notify, span)
}

pub(crate) fn spawn_process_group_with_stdio_native(
    invocation: &ProcessInvocation,
    stdout: File,
    stderr: File,
    notify: bool,
    span: Span,
) -> Result<SpawnedChild, RuntimeError> {
    let mut command = command_from_invocation(invocation, span)?;
    let cgroup = invocation_cgroup(invocation, "xsh-unix", span)?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    let notify_pipe = begin_notify_pipe(&mut command, notify)
        .map_err(|error| RuntimeError::new("unix-notify", error.to_string()).with_span(span))?;
    let notify_write = notify_pipe.as_ref().map(|(_, write)| write.as_raw_fd());
    configure_init_child(&mut command, None, notify_write);
    let child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return Err(RuntimeError::new("unix-spawn", error.to_string()).with_span(span));
        }
    };
    assign_cgroup_pid(&cgroup, child.id() as i64, span)?;
    Ok(SpawnedChild {
        pid: child.id() as i64,
        command: String::from_utf8_lossy(&invocation.target).into_owned(),
        argv: spawn_argv(&invocation.target, &invocation.argv),
        detach: true,
        new_session: false,
        ignore_hup: true,
        notify_fd: finish_notify_pipe(notify_pipe),
    })
}

pub(crate) fn spawn_logged_process_group_native(
    invocation: &ProcessInvocation,
    logger: &ProcessInvocation,
    span: Span,
) -> Result<LoggedSpawnedChild, RuntimeError> {
    let mut command = command_from_invocation(invocation, span)?;
    let mut logger_command = command_from_invocation(logger, span)?;
    let cgroup = invocation_cgroup(invocation, "xsh-unix", span)?;
    let (reader, writer) = pipe_files().map_err(|error| {
        RuntimeError::new("unix-spawn-log-pipe", error.to_string()).with_span(span)
    })?;
    let stdout = duplicate_file(&writer).map_err(|error| {
        RuntimeError::new("unix-spawn-log-pipe", error.to_string()).with_span(span)
    })?;
    let stderr = duplicate_file(&writer).map_err(|error| {
        RuntimeError::new("unix-spawn-log-pipe", error.to_string()).with_span(span)
    })?;
    drop(writer);

    command.stdin(Stdio::null());
    command.stdout(Stdio::from(stdout));
    command.stderr(Stdio::from(stderr));
    configure_init_child(&mut command, None, None);
    let child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return Err(RuntimeError::new("unix-spawn", error.to_string()).with_span(span));
        }
    };

    let pid = child.id() as i64;
    assign_cgroup_pid(&cgroup, pid, span)?;
    logger_command.stdin(Stdio::from(reader));
    logger_command.stdout(Stdio::null());
    logger_command.stderr(Stdio::null());
    configure_logger_child(&mut logger_command, child.id() as libc::pid_t);
    let logger_child = match logger_command.spawn() {
        Ok(child) => child,
        Err(error) => {
            if let Some(pid) = rprocess::Pid::from_raw(child.id() as i32) {
                let _ = rprocess::kill_process_group(pid, rprocess::Signal::TERM);
            }
            return Err(RuntimeError::new("unix-spawn-logger", error.to_string()).with_span(span));
        }
    };
    assign_cgroup_pid(&cgroup, logger_child.id() as i64, span)?;

    Ok(LoggedSpawnedChild {
        pid,
        log_pid: logger_child.id() as i64,
        command: String::from_utf8_lossy(&invocation.target).into_owned(),
        argv: spawn_argv(&invocation.target, &invocation.argv),
        detach: true,
        new_session: false,
        ignore_hup: true,
    })
}

pub(crate) fn spawn_with_tty_native(
    invocation: &ProcessInvocation,
    tty: &str,
    span: Span,
) -> Result<SpawnedChild, RuntimeError> {
    let tty_path = tty_path(tty, invocation);
    let tty_c = cstring_path(&tty_path, "unix-spawn-tty", span)?;
    spawn_child(invocation, Some(tty_c), false, span)
}

pub(crate) fn kill_process_group_native(
    pid: i64,
    signal: i32,
    span: Span,
) -> Result<(), RuntimeError> {
    if !(1..=i32::MAX as i64).contains(&pid) {
        return Err(
            RuntimeError::new("pid-range", "pid must be a positive process id").with_span(span),
        );
    }
    let _ = signal_process_group(pid, signal, span)?;
    Ok(())
}

fn signal_process_group(pid: i64, signal: i32, span: Span) -> Result<bool, RuntimeError> {
    let Some(pid) = rprocess::Pid::from_raw(pid as i32) else {
        return Ok(false);
    };
    if signal == 0 {
        return match rprocess::test_kill_process_group(pid) {
            Ok(()) => Ok(true),
            Err(error) if error == rio::Errno::SRCH => Ok(false),
            Err(error) if error == rio::Errno::PERM => match rprocess::test_kill_process(pid) {
                Ok(()) => Ok(true),
                Err(kill_error) if kill_error == rio::Errno::SRCH => Ok(false),
                Err(kill_error) => Err(RuntimeError::new(
                    "unix-kill-process-group",
                    io::Error::from(kill_error).to_string(),
                )
                .with_span(span)),
            },
            Err(error) => Err(RuntimeError::new(
                "unix-kill-process-group",
                io::Error::from(error).to_string(),
            )
            .with_span(span)),
        };
    }
    let Some(signal) = signal_from_i32(signal) else {
        return Err(RuntimeError::new("invalid-signal", "invalid signal").with_span(span));
    };
    match rprocess::kill_process_group(pid, signal) {
        Ok(()) => Ok(true),
        Err(error) => {
            let error = io::Error::from(error);
            if error.raw_os_error() == Some(rio::Errno::SRCH.raw_os_error()) {
                Ok(false)
            } else if error.raw_os_error() == Some(rio::Errno::PERM.raw_os_error()) {
                match rprocess::kill_process(pid, signal) {
                    Ok(()) => Ok(true),
                    Err(kill_error) => {
                        let kill_error = io::Error::from(kill_error);
                        if kill_error.raw_os_error() == Some(rio::Errno::SRCH.raw_os_error()) {
                            Ok(false)
                        } else {
                            Err(RuntimeError::new(
                                "unix-kill-process-group",
                                kill_error.to_string(),
                            )
                            .with_span(span))
                        }
                    }
                }
            } else {
                Err(RuntimeError::new("unix-kill-process-group", error.to_string()).with_span(span))
            }
        }
    }
}

fn process_group_exists(pid: i64) -> bool {
    let Some(pid) = rprocess::Pid::from_raw(pid as i32) else {
        return false;
    };
    match rprocess::test_kill_process_group(pid) {
        Ok(()) => true,
        Err(error) => error == rio::Errno::PERM,
    }
}

fn remaining_process_groups(groups: &[i64]) -> Vec<i64> {
    groups
        .iter()
        .copied()
        .filter(|pid| process_group_exists(*pid))
        .collect()
}

pub(crate) fn shutdown_process_groups_native(
    groups: &[i64],
    term_timeout: Duration,
    kill_timeout: Duration,
    span: Span,
) -> Result<Pid1Shutdown, RuntimeError> {
    for pid in groups {
        if !(1..=i32::MAX as i64).contains(pid) {
            return Err(
                RuntimeError::new("pid-range", "pid must be a positive process id").with_span(span),
            );
        }
    }

    let mut term_sent = 0_i64;
    for pid in groups {
        if signal_process_group(*pid, libc::SIGTERM, span)? {
            term_sent += 1;
        }
    }
    std::thread::sleep(term_timeout);

    let mut reaped = drain_child_events_native(span)?;
    let remaining_after_term = remaining_process_groups(groups);
    let mut kill_sent = 0_i64;
    for pid in &remaining_after_term {
        if signal_process_group(*pid, libc::SIGKILL, span)? {
            kill_sent += 1;
        }
    }
    std::thread::sleep(kill_timeout);
    reaped.extend(drain_child_events_native(span)?);

    Ok(Pid1Shutdown {
        term_sent,
        kill_sent,
        reaped,
        remaining: remaining_process_groups(groups),
    })
}

pub(crate) fn exec(invocation: &ProcessInvocation, span: Span) -> Result<Value, RuntimeError> {
    let mut command = match command_from_invocation(invocation, span) {
        Ok(command) => command,
        Err(error) => return Ok(Value::err(Value::Error(Box::new(error)))),
    };
    let error = command.exec();
    Ok(io_error("unix-exec", error, span))
}

pub(crate) fn set_hostname(hostname: &str, span: Span) -> Result<Value, RuntimeError> {
    match rustix::system::sethostname(hostname.as_bytes()) {
        Ok(()) => Ok(ok_unit()),
        Err(e) => Ok(io_error("unix-set-hostname", io::Error::from(e), span)),
    }
}

pub(crate) fn uptime_seconds(span: Span) -> Result<Value, RuntimeError> {
    uptime_seconds_impl(span)
}

pub(crate) fn tty(span: Span) -> Result<Value, RuntimeError> {
    match termios::ttyname(stdio::stdin(), Vec::new()) {
        Ok(cstr) => {
            let bytes = cstr.to_bytes();
            if bytes.is_empty() {
                return Ok(error_value("unix-tty", "stdin is not a tty", span));
            }
            Ok(Value::ok(Value::Str(
                String::from_utf8_lossy(bytes).as_ref().into(),
            )))
        }
        Err(e) => Ok(io_error("unix-tty", io::Error::from(e), span)),
    }
}

pub(crate) fn id(span: Span) -> Result<Value, RuntimeError> {
    let groups = match supplementary_groups(span) {
        Ok(groups) => groups,
        Err(error) => return Ok(Value::err(Value::Error(Box::new(error)))),
    };
    Ok(Value::ok(Value::Record(
        crate::runtime::value::RecordMap::from([
            (
                Arc::from("uid"),
                Value::Int(rprocess::getuid().as_raw() as i64),
            ),
            (
                Arc::from("euid"),
                Value::Int(rprocess::geteuid().as_raw() as i64),
            ),
            (
                Arc::from("gid"),
                Value::Int(rprocess::getgid().as_raw() as i64),
            ),
            (
                Arc::from("egid"),
                Value::Int(rprocess::getegid().as_raw() as i64),
            ),
            (Arc::from("groups"), Value::List(groups)),
        ]),
    )))
}

pub(crate) fn tty_attrs(fd: i64, span: Span) -> Result<Value, RuntimeError> {
    let fd = match raw_fd_arg(fd, "unix-tty-attrs", span) {
        Ok(fd) => fd,
        Err(error) => return Ok(Value::err(Value::Error(Box::new(error)))),
    };
    let borrowed = unsafe { BorrowedFd::borrow_raw(fd) };
    let attrs = match termios::tcgetattr(borrowed) {
        Ok(a) => a,
        Err(e) => return Ok(io_error("unix-tty-attrs", io::Error::from(e), span)),
    };
    Ok(Value::ok(tty_attrs_record(&attrs)))
}

pub(crate) fn set_tty_attrs(
    record: &crate::runtime::value::RecordMap,
    fd: i64,
    span: Span,
) -> Result<Value, RuntimeError> {
    let fd = match raw_fd_arg(fd, "unix-tty-attrs", span) {
        Ok(fd) => fd,
        Err(error) => return Ok(Value::err(Value::Error(Box::new(error)))),
    };
    let borrowed = unsafe { BorrowedFd::borrow_raw(fd) };
    let mut attrs = match termios::tcgetattr(borrowed) {
        Ok(a) => a,
        Err(e) => return Ok(io_error("unix-tty-attrs", io::Error::from(e), span)),
    };
    attrs.input_modes =
        termios::InputModes::from_bits_retain(
            record_uint(record, "iflag", "unix-tty-attrs", span)? as _,
        );
    attrs.output_modes =
        termios::OutputModes::from_bits_retain(
            record_uint(record, "oflag", "unix-tty-attrs", span)? as _,
        );
    attrs.control_modes = termios::ControlModes::from_bits_retain(record_uint(
        record,
        "cflag",
        "unix-tty-attrs",
        span,
    )? as _);
    attrs.local_modes =
        termios::LocalModes::from_bits_retain(
            record_uint(record, "lflag", "unix-tty-attrs", span)? as _,
        );
    if let Some(chars) = record.get("control_chars") {
        let chars = match chars {
            Value::List(items) => items,
            value => {
                return Err(RuntimeError::new(
                    "type-error",
                    format!(
                        "control_chars expected List[Int], found {}",
                        value.type_name()
                    ),
                )
                .with_span(span));
            }
        };
        let nccs = std::mem::size_of_val(&attrs.special_codes);
        for (index, value) in chars.iter().enumerate().take(nccs) {
            let Value::Int(value) = value else {
                return Err(
                    RuntimeError::new("type-error", "control_chars expected List[Int]")
                        .with_span(span),
                );
            };
            if !(0..=u8::MAX as i64).contains(value) {
                return Err(RuntimeError::new(
                    "unix-tty-attrs",
                    "control character values must be between 0 and 255",
                )
                .with_span(span));
            }
            // SpecialCodes wraps [u8; NCCS] with layout verified by rustix's own static checks.
            unsafe {
                *(&mut attrs.special_codes as *mut _ as *mut u8).add(index) = *value as u8;
            }
        }
    }
    let ispeed = record_uint(record, "ispeed", "unix-tty-attrs", span)? as u32;
    let ospeed = record_uint(record, "ospeed", "unix-tty-attrs", span)? as u32;
    if attrs.set_input_speed(ispeed).is_err() || attrs.set_output_speed(ospeed).is_err() {
        return Ok(io_error(
            "unix-tty-attrs",
            io::Error::from(rustix::io::Errno::INVAL),
            span,
        ));
    }
    match termios::tcsetattr(borrowed, termios::OptionalActions::Now, &attrs) {
        Ok(()) => Ok(ok_unit()),
        Err(e) => Ok(io_error("unix-tty-attrs", io::Error::from(e), span)),
    }
}

pub(crate) fn kill_all(name: &str, signal: &str, span: Span) -> Result<Value, RuntimeError> {
    let name = name.trim();
    if name.is_empty() {
        return Ok(error_value(
            "unix-kill-all",
            "process name cannot be empty",
            span,
        ));
    }

    let signal = match signal_info(signal, span) {
        Ok(signal) => signal,
        Err(error) => return Ok(Value::err(Value::Error(Box::new(error)))),
    };
    let processes = match list_processes(span) {
        Ok(processes) => processes,
        Err(error) => return Ok(Value::err(Value::Error(Box::new(error)))),
    };
    let targets = matching_targets(
        processes.iter(),
        name,
        rprocess::getpid().as_raw_pid() as i64,
    );
    let mut signaled = 0_i64;

    for target in &targets {
        let Some(pid) = rprocess::Pid::from_raw(target.pid as i32) else {
            continue;
        };
        let Some(signal) = signal_from_i32(signal.number) else {
            return Ok(error_value("invalid-signal", "invalid signal", span));
        };
        match rprocess::kill_process(pid, signal) {
            Ok(()) => signaled += 1,
            Err(error) => {
                let error = io::Error::from(error);
                match error.raw_os_error() {
                    Some(libc::ESRCH) => {}
                    Some(libc::EPERM) => {
                        return Ok(error_value("permission-denied", "permission denied", span));
                    }
                    Some(libc::EINVAL) => {
                        return Ok(error_value("invalid-signal", "invalid signal", span));
                    }
                    _ => return Ok(error_value("unix-kill-all", error.to_string(), span)),
                }
            }
        }
    }

    if signaled == 0 {
        return Ok(error_value(
            "process-missing",
            "no matching process was signaled",
            span,
        ));
    }

    Ok(Value::ok(Value::Record(
        crate::runtime::value::RecordMap::from([
            (Arc::from("matched"), Value::Int(targets.len() as i64)),
            (Arc::from("signaled"), Value::Int(signaled)),
        ]),
    )))
}

extern "C" fn handle_signal(signal: libc::c_int) {
    if signal != libc::SIGCHLD {
        PENDING_SIGNAL.store(signal, Ordering::Relaxed);
    }
}

fn install_signal_handler(signal: libc::c_int) -> io::Result<()> {
    let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
    action.sa_sigaction = handle_signal as *const () as usize;
    action.sa_flags = 0;
    unsafe {
        libc::sigemptyset(&mut action.sa_mask);
        if libc::sigaction(signal, &action, std::ptr::null_mut()) == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
}

fn take_signal() -> Option<libc::c_int> {
    match PENDING_SIGNAL.swap(0, Ordering::Relaxed) {
        0 => None,
        signal => Some(signal),
    }
}

fn signal_name(signal: libc::c_int) -> &'static str {
    match signal {
        libc::SIGHUP => "HUP",
        libc::SIGTERM => "TERM",
        libc::SIGUSR1 => "USR1",
        libc::SIGUSR2 => "USR2",
        libc::SIGINT => "INT",
        _ => "",
    }
}

#[cfg(target_os = "linux")]
fn enable_child_subreaper() -> io::Result<()> {
    // Passing our own (nonzero) pid sets PR_SET_CHILD_SUBREAPER, marking this
    // process as the subreaper for its descendants.
    rprocess::set_child_subreaper(Some(rprocess::getpid())).map_err(io::Error::from)
}

#[cfg(not(target_os = "linux"))]
fn enable_child_subreaper() -> io::Result<()> {
    Ok(())
}

#[derive(Clone, Debug)]
pub(crate) struct ChildEvent {
    pub(crate) pid: i64,
    pub(crate) status: ProcessStatus,
}

#[derive(Clone, Debug)]
pub(crate) struct Pid1Event {
    pub(crate) kind: Pid1EventKind,
    pub(crate) signal: String,
    pub(crate) children: Vec<ChildEvent>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Pid1EventKind {
    Signal,
    Children,
    Poll,
    Timeout,
}

impl Pid1Event {
    fn signal(signal: &str) -> Self {
        Self {
            kind: Pid1EventKind::Signal,
            signal: signal.to_string(),
            children: Vec::new(),
        }
    }

    fn children(children: Vec<ChildEvent>) -> Self {
        Self {
            kind: Pid1EventKind::Children,
            signal: String::new(),
            children,
        }
    }

    fn poll() -> Self {
        Self {
            kind: Pid1EventKind::Poll,
            signal: String::new(),
            children: Vec::new(),
        }
    }

    fn timeout() -> Self {
        Self {
            kind: Pid1EventKind::Timeout,
            signal: String::new(),
            children: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SpawnedChild {
    pub(crate) pid: i64,
    pub(crate) command: String,
    pub(crate) argv: Vec<String>,
    pub(crate) detach: bool,
    pub(crate) new_session: bool,
    pub(crate) ignore_hup: bool,
    // Read end of the readiness pipe when spawned with `notify: true`, else -1.
    // The child inherits the write end as the fd named by the `NOTIFY_FD` env
    // var and writes a byte when ready; poll this fd with `unix.notify_ready`.
    pub(crate) notify_fd: i64,
}

#[derive(Clone, Debug)]
pub(crate) struct LoggedSpawnedChild {
    pub(crate) pid: i64,
    pub(crate) log_pid: i64,
    pub(crate) command: String,
    pub(crate) argv: Vec<String>,
    pub(crate) detach: bool,
    pub(crate) new_session: bool,
    pub(crate) ignore_hup: bool,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) struct Pid1Shutdown {
    pub(crate) term_sent: i64,
    pub(crate) kill_sent: i64,
    pub(crate) reaped: Vec<ChildEvent>,
    pub(crate) remaining: Vec<i64>,
}

fn wait_one_child_event(span: Span) -> Result<Option<ChildEvent>, RuntimeError> {
    let Some((pid, status)) = (match rprocess::wait(rprocess::WaitOptions::NOHANG) {
        Ok(wait) => wait,
        Err(error) => {
            let error = io::Error::from(error);
            if error.raw_os_error() == Some(libc::ECHILD) {
                return Ok(None);
            }
            return Err(RuntimeError::new("unix-waitpid", error.to_string()).with_span(span));
        }
    }) else {
        return Ok(None);
    };
    let pid = pid.as_raw_pid() as i64;
    Ok(Some(ChildEvent {
        pid,
        status: child_status_from_wait(pid, status),
    }))
}

pub(crate) fn drain_child_events_native(span: Span) -> Result<Vec<ChildEvent>, RuntimeError> {
    let mut events = Vec::new();
    loop {
        match wait_one_child_event(span)? {
            Some(event) => events.push(event),
            None => return Ok(events),
        }
    }
}

fn child_status_from_wait(pid: i64, status: rprocess::WaitStatus) -> ProcessStatus {
    let (success, kind, code) = if status.exited() {
        let code = status.exit_status().unwrap_or(0);
        (code == 0, ProcessSegmentStatusKind::Exit, Some(code))
    } else if status.signaled() {
        (
            false,
            ProcessSegmentStatusKind::Signal,
            status.terminating_signal(),
        )
    } else {
        (false, ProcessSegmentStatusKind::Signal, None)
    };
    ProcessStatus::from_segments(vec![ProcessSegmentStatus {
        index: 0,
        target: Vec::new(),
        pid: Some(pid as u32),
        success,
        kind,
        code,
        error_kind: None,
        error_message: None,
    }])
}

fn child_event_record(event: ChildEvent) -> Value {
    Value::Record(crate::runtime::value::RecordMap::from([
        (Arc::from("pid"), Value::Int(event.pid)),
        (Arc::from("status"), Value::Status(event.status)),
    ]))
}

fn spawn_child(
    invocation: &ProcessInvocation,
    tty: Option<CString>,
    notify: bool,
    span: Span,
) -> Result<SpawnedChild, RuntimeError> {
    let mut command = command_from_invocation(invocation, span)?;
    let cgroup = invocation_cgroup(invocation, "xsh-unix", span)?;
    let new_session = tty.is_some();
    if tty.is_none() {
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
    }
    let notify_pipe = begin_notify_pipe(&mut command, notify)
        .map_err(|error| RuntimeError::new("unix-notify", error.to_string()).with_span(span))?;
    let notify_write = notify_pipe.as_ref().map(|(_, write)| write.as_raw_fd());
    configure_init_child(&mut command, tty, notify_write);
    let child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return Err(RuntimeError::new("unix-spawn", error.to_string()).with_span(span));
        }
    };
    assign_cgroup_pid(&cgroup, child.id() as i64, span)?;
    Ok(SpawnedChild {
        pid: child.id() as i64,
        command: String::from_utf8_lossy(&invocation.target).into_owned(),
        argv: spawn_argv(&invocation.target, &invocation.argv),
        detach: true,
        new_session,
        ignore_hup: true,
        notify_fd: finish_notify_pipe(notify_pipe),
    })
}

fn command_from_invocation(
    invocation: &ProcessInvocation,
    span: Span,
) -> Result<Command, RuntimeError> {
    let executable = resolve_executable(invocation)
        .map_err(|error| RuntimeError::new(error.kind, error.message).with_span(span))?;
    let mut command = Command::new(executable);
    command
        .args(
            invocation
                .argv
                .iter()
                .map(|arg| OsString::from_vec(arg.clone())),
        )
        .current_dir(&invocation.cwd);
    command.env_clear();
    for (name, value) in &invocation.env {
        command.env(os_string_from_bytes(name), os_string_from_bytes(value));
    }
    Ok(command)
}

fn invocation_cgroup(
    invocation: &ProcessInvocation,
    prefix: &str,
    span: Span,
) -> Result<CgroupScope, RuntimeError> {
    CgroupScope::cpu_max(invocation.cpu_max, prefix)
        .map_err(|error| cgroup_runtime_error(error, span))
}

fn assign_cgroup_pid(scope: &CgroupScope, pid: i64, span: Span) -> Result<(), RuntimeError> {
    scope
        .assign_pid(pid)
        .map_err(|error| cgroup_runtime_error(error, span))
}

fn cgroup_runtime_error(error: CgroupError, span: Span) -> RuntimeError {
    RuntimeError::new(error.kind, error.message).with_span(span)
}

fn configure_init_child(
    command: &mut Command,
    tty: Option<CString>,
    notify_write_fd: Option<RawFd>,
) {
    unsafe {
        command.pre_exec(move || {
            reset_child_signal_handlers();
            libc::signal(libc::SIGHUP, libc::SIG_IGN);
            // Clear CLOEXEC on the notify write end so it survives exec; the
            // child finds its number in NOTIFY_FD. Async-signal-safe.
            if let Some(fd) = notify_write_fd {
                let fd = BorrowedFd::borrow_raw(fd);
                let _ = rio::fcntl_setfd(fd, rio::FdFlags::empty());
            }
            match &tty {
                Some(tty) => configure_controlling_tty(tty),
                None => create_process_group(),
            }
        });
    }
}

fn configure_logger_child(command: &mut Command, service_pid: libc::pid_t) {
    unsafe {
        command.pre_exec(move || {
            reset_child_signal_handlers();
            libc::signal(libc::SIGHUP, libc::SIG_IGN);
            rprocess::setpgid(None, rprocess::Pid::from_raw(service_pid)).map_err(io::Error::from)
        });
    }
}

fn reset_child_signal_handlers() {
    unsafe {
        libc::signal(libc::SIGHUP, libc::SIG_DFL);
        libc::signal(libc::SIGINT, libc::SIG_DFL);
        libc::signal(libc::SIGTERM, libc::SIG_DFL);
        libc::signal(libc::SIGUSR1, libc::SIG_DFL);
        libc::signal(libc::SIGUSR2, libc::SIG_DFL);
    }
}

fn configure_controlling_tty(tty: &CString) -> io::Result<()> {
    if let Err(error) = rprocess::setsid() {
        let error = io::Error::from(error);
        if error.raw_os_error() != Some(libc::EPERM) {
            return Err(error);
        }
    }
    let fd = rfs::open(
        tty.as_c_str(),
        rfs::OFlags::RDWR | rfs::OFlags::CLOEXEC,
        rfs::Mode::empty(),
    )
    .map_err(io::Error::from)?;
    stdio::dup2_stdin(&fd).map_err(io::Error::from)?;
    stdio::dup2_stdout(&fd).map_err(io::Error::from)?;
    stdio::dup2_stderr(&fd).map_err(io::Error::from)?;

    let raw_fd = fd.as_raw_fd();
    if termios::isatty(&fd) {
        rprocess::ioctl_tiocsctty(&fd).map_err(io::Error::from)?;
    }
    if raw_fd <= libc::STDERR_FILENO {
        std::mem::forget(fd);
    }
    Ok(())
}

fn pipe_files() -> io::Result<(File, File)> {
    let (read, write) = rpipe::pipe().map_err(io::Error::from)?;
    set_cloexec(&read)?;
    set_cloexec(&write)?;
    Ok((File::from(read), File::from(write)))
}

fn set_nonblocking(fd: RawFd) {
    unsafe {
        let fd = BorrowedFd::borrow_raw(fd);
        if let Ok(mut flags) = rfs::fcntl_getfl(fd) {
            flags.insert(rfs::OFlags::NONBLOCK);
            let _ = rfs::fcntl_setfl(fd, flags);
        }
    }
}

// Set up a readiness pipe for a child when `notify` is true. The write end's fd
// number is published to the child as `NOTIFY_FD` (sd_notify convention); the
// child's pre_exec hook clears its CLOEXEC so it survives exec. Returns the pipe
// files; the caller passes the write fd to `configure_init_child` and finishes
// with `finish_notify_pipe` after spawn.
fn begin_notify_pipe(command: &mut Command, notify: bool) -> io::Result<Option<(File, File)>> {
    if !notify {
        return Ok(None);
    }
    let (read, write) = pipe_files()?;
    command.env("NOTIFY_FD", write.as_raw_fd().to_string());
    Ok(Some((read, write)))
}

// After spawn: the parent drops the write end and keeps the non-blocking read
// end, transferring its ownership out as a raw fd the caller polls with
// `notify_ready_native` and releases with `notify_close_native`.
fn finish_notify_pipe(pipe: Option<(File, File)>) -> i64 {
    match pipe {
        Some((read, write)) => {
            drop(write);
            let fd = read.into_raw_fd();
            set_nonblocking(fd);
            fd as i64
        }
        None => -1,
    }
}

pub(crate) fn notify_ready_native(fd: i64, span: Span) -> Result<bool, RuntimeError> {
    if fd < 0 {
        return Ok(false);
    }
    let fd = unsafe { BorrowedFd::borrow_raw(fd as RawFd) };
    let mut ready = false;
    let mut buf = [0u8; 256];
    loop {
        match rio::read(fd, &mut buf) {
            Ok(n) if n > 0 => {
                ready = true;
                continue;
            }
            Ok(_) => break, // writer closed without (further) notifying
            Err(rio::Errno::INTR) => continue,
            Err(error) if error == rio::Errno::AGAIN || error == rio::Errno::WOULDBLOCK => break,
            Err(error) => {
                return Err(RuntimeError::new("unix-notify", error.to_string()).with_span(span));
            }
        }
    }
    Ok(ready)
}

pub(crate) fn notify_close_native(fd: i64, _span: Span) -> Result<(), RuntimeError> {
    if fd < 0 {
        return Ok(());
    }
    unsafe { rio::close(fd as RawFd) };
    Ok(())
}

fn set_cloexec<Fd: std::os::fd::AsFd>(fd: Fd) -> io::Result<()> {
    let mut flags = rio::fcntl_getfd(&fd).map_err(io::Error::from)?;
    flags.insert(rio::FdFlags::CLOEXEC);
    rio::fcntl_setfd(fd, flags).map_err(io::Error::from)
}

fn duplicate_file(file: &File) -> io::Result<File> {
    rio::fcntl_dupfd_cloexec(file, 0)
        .map(File::from)
        .map_err(io::Error::from)
}

fn signal_from_i32(signal: i32) -> Option<rprocess::Signal> {
    NonZeroI32::new(signal)
        .map(|signal| unsafe { rprocess::Signal::from_raw_nonzero_unchecked(signal) })
}

fn create_process_group() -> io::Result<()> {
    rprocess::setpgid(None, None).map_err(io::Error::from)
}

fn tty_path(id: &str, invocation: &ProcessInvocation) -> PathBuf {
    if id.contains('/') {
        return PathBuf::from(id);
    }
    invocation
        .env
        .get(b"XSH_UNIX_TTY_DIR".as_slice())
        .or_else(|| invocation.env.get(b"SEED_INIT_TTY_DIR".as_slice()))
        .map(|value| PathBuf::from(os_string_from_bytes(value)))
        .unwrap_or_else(|| PathBuf::from("/dev"))
        .join(id)
}

fn spawn_argv(target: &[u8], argv: &[Vec<u8>]) -> Vec<String> {
    std::iter::once(target)
        .chain(argv.iter().map(Vec::as_slice))
        .map(|item| String::from_utf8_lossy(item).into_owned())
        .collect()
}

fn spawn_record_from_native(child: SpawnedChild) -> Value {
    let argv_values = child
        .argv
        .into_iter()
        .map(|item| Value::Str(item.into()))
        .collect::<Vec<_>>();
    Value::ok(Value::Record(crate::runtime::value::RecordMap::from([
        (Arc::from("pid"), Value::Int(child.pid)),
        (Arc::from("command"), Value::Str(child.command.into())),
        (Arc::from("argv"), Value::List(argv_values)),
        (Arc::from("detach"), Value::Bool(child.detach)),
        (Arc::from("new_session"), Value::Bool(child.new_session)),
        (Arc::from("ignore_hup"), Value::Bool(child.ignore_hup)),
        (Arc::from("notify_fd"), Value::Int(child.notify_fd)),
    ])))
}

fn logged_spawn_record_from_native(child: LoggedSpawnedChild) -> Value {
    let argv_values = child
        .argv
        .into_iter()
        .map(|item| Value::Str(item.into()))
        .collect::<Vec<_>>();
    Value::ok(Value::Record(crate::runtime::value::RecordMap::from([
        (Arc::from("pid"), Value::Int(child.pid)),
        (Arc::from("log_pid"), Value::Int(child.log_pid)),
        (Arc::from("command"), Value::Str(child.command.into())),
        (Arc::from("argv"), Value::List(argv_values)),
        (Arc::from("detach"), Value::Bool(child.detach)),
        (Arc::from("new_session"), Value::Bool(child.new_session)),
        (Arc::from("ignore_hup"), Value::Bool(child.ignore_hup)),
    ])))
}

fn os_string_from_bytes(bytes: &[u8]) -> OsString {
    OsString::from_vec(bytes.to_vec())
}

#[cfg(target_os = "linux")]
fn uptime_seconds_impl(span: Span) -> Result<Value, RuntimeError> {
    let text = match std::fs::read_to_string("/proc/uptime") {
        Ok(text) => text,
        Err(error) => return Ok(io_error("unix-uptime", error, span)),
    };
    let seconds = text
        .split_whitespace()
        .next()
        .and_then(|field| field.split('.').next())
        .and_then(|field| field.parse::<i64>().ok())
        .unwrap_or(0);
    Ok(Value::ok(Value::Int(seconds)))
}

#[cfg(target_os = "macos")]
fn uptime_seconds_impl(span: Span) -> Result<Value, RuntimeError> {
    let mut boot_time: libc::timeval = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<libc::timeval>();
    let mut mib = [libc::CTL_KERN, libc::KERN_BOOTTIME];
    let rc = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as libc::c_uint,
            &mut boot_time as *mut _ as *mut libc::c_void,
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 {
        return Ok(io_error("unix-uptime", io::Error::last_os_error(), span));
    }
    let now = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => duration.as_secs() as i64,
        Err(error) => return Ok(error_value("unix-uptime", error.to_string(), span)),
    };
    Ok(Value::ok(Value::Int(
        now.saturating_sub(boot_time.tv_sec as i64),
    )))
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn uptime_seconds_impl(span: Span) -> Result<Value, RuntimeError> {
    Ok(error_value(
        "unix-unsupported",
        "unix.uptime_seconds is only implemented on Linux and macOS",
        span,
    ))
}

fn matching_targets<'a>(
    processes: impl Iterator<Item = &'a Value>,
    name: &str,
    self_pid: i64,
) -> Vec<ProcessTarget> {
    processes
        .filter_map(ProcessTarget::from_value)
        .filter(|process| process.pid > 1 && process.pid != self_pid && process.matches_name(name))
        .collect()
}

impl ProcessTarget {
    fn from_value(value: &Value) -> Option<Self> {
        let fields = match value {
            Value::Record(fields) => fields,
            _ => return None,
        };
        Some(Self {
            pid: int_field(fields, "pid")?,
            command: str_field(fields, "command"),
            argv0: str_field(fields, "argv0"),
        })
    }

    fn matches_name(&self, target: &str) -> bool {
        self.command == target
            || basename(&self.argv0) == target
            || (self.command.len() == LINUX_COMM_LIMIT && target.starts_with(&self.command))
    }
}

fn int_field(fields: &crate::runtime::value::RecordMap, name: &str) -> Option<i64> {
    match fields.get(name) {
        Some(Value::Int(value)) => Some(*value),
        _ => None,
    }
}

fn supplementary_groups(span: Span) -> Result<Vec<Value>, RuntimeError> {
    let mut gids = rprocess::getgroups().map_err(|e| {
        RuntimeError::new("unix-id", io::Error::from(e).to_string()).with_span(span)
    })?;
    let primary_raw = rprocess::getgid().as_raw();
    if !gids.iter().any(|g| g.as_raw() == primary_raw) {
        gids.push(rprocess::getgid());
    }
    gids.sort_unstable_by_key(|g| g.as_raw());
    gids.dedup_by_key(|g| g.as_raw());
    Ok(gids
        .into_iter()
        .map(|gid| {
            let raw = gid.as_raw();
            Value::Record(crate::runtime::value::RecordMap::from([
                (Arc::from("gid"), Value::Int(raw as i64)),
                (Arc::from("name"), Value::Str(group_name(raw).into())),
            ]))
        })
        .collect())
}

fn group_name(gid: libc::gid_t) -> String {
    let group = unsafe { libc::getgrgid(gid) };
    if group.is_null() {
        return gid.to_string();
    }
    let name = unsafe { std::ffi::CStr::from_ptr((*group).gr_name) };
    name.to_string_lossy().into_owned()
}

fn raw_fd_arg(fd: i64, kind: &str, span: Span) -> Result<libc::c_int, RuntimeError> {
    if (0..=libc::c_int::MAX as i64).contains(&fd) {
        Ok(fd as libc::c_int)
    } else {
        Err(RuntimeError::new(kind, "fd must be non-negative").with_span(span))
    }
}

fn tty_attrs_record(attrs: &termios::Termios) -> Value {
    // SpecialCodes wraps [u8; NCCS] with layout verified by rustix's own static checks.
    let nccs = std::mem::size_of_val(&attrs.special_codes);
    let control_chars: Vec<Value> =
        unsafe { std::slice::from_raw_parts(&attrs.special_codes as *const _ as *const u8, nccs) }
            .iter()
            .map(|&b| Value::Int(b as i64))
            .collect();
    Value::Record(crate::runtime::value::RecordMap::from([
        (
            Arc::from("iflag"),
            Value::Int(attrs.input_modes.bits() as i64),
        ),
        (
            Arc::from("oflag"),
            Value::Int(attrs.output_modes.bits() as i64),
        ),
        (
            Arc::from("cflag"),
            Value::Int(attrs.control_modes.bits() as i64),
        ),
        (
            Arc::from("lflag"),
            Value::Int(attrs.local_modes.bits() as i64),
        ),
        (Arc::from("ispeed"), Value::Int(attrs.input_speed() as i64)),
        (Arc::from("ospeed"), Value::Int(attrs.output_speed() as i64)),
        (
            Arc::from("echo"),
            Value::Bool(attrs.local_modes.contains(termios::LocalModes::ECHO)),
        ),
        (
            Arc::from("raw"),
            Value::Bool(
                !attrs.local_modes.intersects(
                    termios::LocalModes::ICANON
                        | termios::LocalModes::ECHO
                        | termios::LocalModes::ISIG,
                ) && !attrs
                    .input_modes
                    .intersects(termios::InputModes::ICRNL | termios::InputModes::IXON),
            ),
        ),
        (
            Arc::from("crnl"),
            Value::Bool(attrs.input_modes.contains(termios::InputModes::ICRNL)),
        ),
        (Arc::from("control_chars"), Value::List(control_chars)),
    ]))
}

fn record_uint(
    record: &crate::runtime::value::RecordMap,
    name: &str,
    kind: &'static str,
    span: Span,
) -> Result<u64, RuntimeError> {
    match record.get(name) {
        Some(Value::Int(value)) if *value >= 0 => Ok(*value as u64),
        Some(Value::Int(_)) => {
            Err(RuntimeError::new(kind, format!("{name} cannot be negative")).with_span(span))
        }
        Some(value) => Err(RuntimeError::new(
            "type-error",
            format!("{name} expected Int, found {}", value.type_name()),
        )
        .with_span(span)),
        None => Err(RuntimeError::new(kind, format!("missing `{name}` field")).with_span(span)),
    }
}

fn str_field(fields: &crate::runtime::value::RecordMap, name: &str) -> String {
    match fields.get(name) {
        Some(Value::Str(value)) => value.to_string(),
        _ => String::new(),
    }
}

fn basename(path: &str) -> &str {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
}

fn cstring_path(path: &Path, kind: &str, span: Span) -> Result<CString, RuntimeError> {
    CString::new(path.as_os_str().as_bytes())
        .map_err(|_| RuntimeError::new(kind, "path contains NUL").with_span(span))
}

fn ok_unit() -> Value {
    Value::ok(Value::Unit)
}

fn io_error(kind: &str, error: io::Error, span: Span) -> Value {
    error_value(kind, error.to_string(), span)
}

fn error_value(kind: &str, message: impl Into<String>, span: Span) -> Value {
    Value::err(Value::Error(Box::new(
        RuntimeError::new(kind, message.into()).with_span(span),
    )))
}

#[cfg(test)]
mod tests {
    use super::{Arc, Value, matching_targets};

    fn process(pid: i64, command: &str, argv0: &str) -> Value {
        Value::Record(crate::runtime::value::RecordMap::from([
            (Arc::from("pid"), Value::Int(pid)),
            (Arc::from("command"), Value::Str(command.into())),
            (Arc::from("argv0"), Value::Str(argv0.into())),
        ]))
    }

    #[test]
    fn matches_exact_process_name_and_argv0_basename() {
        let records = [
            process(10, "match-killall-target", "./match-killall-target"),
            process(11, "helper", "/usr/bin/helper"),
        ];

        let matches = matching_targets(records.iter(), "match-killall-target", 99);

        assert_eq!(
            matches.iter().map(|item| item.pid).collect::<Vec<_>>(),
            [10]
        );
    }

    #[test]
    fn does_not_match_later_shell_argv_tokens() {
        let records = [process(10, "sh", "sh")];

        let matches = matching_targets(records.iter(), "wrapper-killall-target", 99);

        assert!(matches.is_empty());
    }

    #[test]
    fn matches_linux_comm_truncation_only_as_prefix() {
        let records = [
            process(10, "match-killall-t", "sh"),
            process(11, "worker-killall-", "sh"),
        ];

        let matches = matching_targets(records.iter(), "match-killall-target", 99);

        assert_eq!(
            matches.iter().map(|item| item.pid).collect::<Vec<_>>(),
            [10]
        );
    }

    #[test]
    fn skips_self_and_pid1() {
        let records = [
            process(1, "target", "target"),
            process(10, "target", "target"),
            process(11, "target", "target"),
        ];

        let matches = matching_targets(records.iter(), "target", 10);

        assert_eq!(
            matches.iter().map(|item| item.pid).collect::<Vec<_>>(),
            [11]
        );
    }
}
