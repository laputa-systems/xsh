//! Typed process-control primitives: the signal table, process groups and
//! sessions, scheduling priority, resource limits, and signal dispositions.
//! Every failure is a host error that carries its errno.

use super::{signal_info, signal_record, signal_table};
use crate::modules::RuntimeOp;
use crate::runtime::value::{PathValue, RecordMap, RuntimeError, Value};
use crate::source::Span;
use rustix::process::{self as rprocess, Pid, Resource, Rlimit, getrlimit, setrlimit};
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

/// The arguments of one native call. A slot is `None` when the caller omitted
/// the parameter, which is distinct from a supplied `null`.
pub(crate) struct Args<'a> {
    operation: String,
    values: &'a [Option<Value>],
    span: Span,
}

/// An optional integer parameter that tells omission, `null`, and a value apart.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Slot<T> {
    Omitted,
    Null,
    Value(T),
}

impl<'a> Args<'a> {
    pub(crate) fn new(op: RuntimeOp, values: &'a [Option<Value>], span: Span) -> Self {
        Self {
            operation: operation_label(op),
            values,
            span,
        }
    }

    pub(crate) fn span(&self) -> Span {
        self.span
    }

    fn type_error(&self, expected: &str, found: &Value) -> RuntimeError {
        RuntimeError::new(
            "type-error",
            format!(
                "{} expected {expected}, found {}",
                self.operation,
                found.type_name()
            ),
        )
        .with_span(self.span)
    }

    fn missing(&self) -> RuntimeError {
        RuntimeError::new("arity", format!("{} expected an argument", self.operation))
            .with_span(self.span)
    }

    fn get(&self, index: usize) -> Option<&Value> {
        self.values.get(index).and_then(Option::as_ref)
    }

    pub(crate) fn int(&self, index: usize) -> Result<i64, RuntimeError> {
        match self.get(index) {
            Some(Value::Int(value)) => Ok(*value),
            Some(other) => Err(self.type_error("Int", other)),
            None => Err(self.missing()),
        }
    }

    pub(crate) fn int_or(&self, index: usize, default: i64) -> Result<i64, RuntimeError> {
        match self.get(index) {
            Some(Value::Int(value)) => Ok(*value),
            Some(other) => Err(self.type_error("Int", other)),
            None => Ok(default),
        }
    }

    pub(crate) fn slot(&self, index: usize) -> Result<Slot<i64>, RuntimeError> {
        match self.get(index) {
            None => Ok(Slot::Omitted),
            Some(Value::Null) => Ok(Slot::Null),
            Some(Value::Int(value)) => Ok(Slot::Value(*value)),
            Some(other) => Err(self.type_error("Int or null", other)),
        }
    }

    pub(crate) fn str(&self, index: usize) -> Result<String, RuntimeError> {
        match self.get(index) {
            Some(Value::Str(value)) => Ok(value.to_string()),
            Some(other) => Err(self.type_error("Str", other)),
            None => Err(self.missing()),
        }
    }

    pub(crate) fn str_or(&self, index: usize, default: &str) -> Result<String, RuntimeError> {
        match self.get(index) {
            Some(Value::Str(value)) => Ok(value.to_string()),
            Some(other) => Err(self.type_error("Str", other)),
            None => Ok(default.to_string()),
        }
    }

    pub(crate) fn bool_or(&self, index: usize, default: bool) -> Result<bool, RuntimeError> {
        match self.get(index) {
            Some(Value::Bool(value)) => Ok(*value),
            Some(other) => Err(self.type_error("Bool", other)),
            None => Ok(default),
        }
    }

    pub(crate) fn path(&self, index: usize) -> Result<PathBuf, RuntimeError> {
        match self.get(index) {
            Some(Value::Path(value)) => Ok(path_buf(value)),
            Some(other) => Err(self.type_error("Path", other)),
            None => Err(self.missing()),
        }
    }

    pub(crate) fn path_opt(&self, index: usize) -> Result<Option<PathBuf>, RuntimeError> {
        match self.get(index) {
            Some(Value::Path(value)) => Ok(Some(path_buf(value))),
            Some(other) => Err(self.type_error("Path", other)),
            None => Ok(None),
        }
    }

    pub(crate) fn record(&self, index: usize) -> Result<&RecordMap, RuntimeError> {
        match self.get(index) {
            Some(Value::Record(value)) => Ok(value),
            Some(other) => Err(self.type_error("Record", other)),
            None => Err(self.missing()),
        }
    }
}

/// `ProcessSetGroupId` as `process.set_group_id`, for argument errors.
fn operation_label(op: RuntimeOp) -> String {
    let debug = format!("{op:?}");
    let mut label = String::with_capacity(debug.len() + 2);
    let mut module_done = false;
    for (index, character) in debug.chars().enumerate() {
        if character.is_ascii_uppercase() && index > 0 {
            label.push(if module_done { '_' } else { '.' });
            module_done = true;
        }
        label.push(character.to_ascii_lowercase());
    }
    label
}

fn path_buf(path: &PathValue) -> PathBuf {
    use std::os::unix::ffi::OsStringExt;
    PathBuf::from(std::ffi::OsString::from_vec(path.bytes.clone()))
}

pub(crate) fn key(name: &str) -> Arc<str> {
    Arc::from(name)
}

pub(crate) fn host_error(kind: &str, error: impl Into<io::Error>, span: Span) -> RuntimeError {
    RuntimeError::host(kind, &error.into()).with_span(span)
}

/// Prefixes a failure's message with the path it concerned.
pub(crate) fn name_error(shown: &str, mut error: RuntimeError) -> RuntimeError {
    if !error.message.contains(shown) {
        error.message = format!("{shown}: {}", error.message);
    }
    error
}

pub(crate) fn optional_int(value: Option<i64>) -> Value {
    value.map_or(Value::Null, Value::Int)
}

fn invalid(kind: &'static str, message: impl Into<String>, span: Span) -> RuntimeError {
    RuntimeError::new(kind, message.into()).with_span(span)
}

/// Whether `op` is one of the primitives in this file.
pub(crate) fn handles(op: RuntimeOp) -> bool {
    matches!(
        op,
        RuntimeOp::ProcessSignals
            | RuntimeOp::ProcessParentPid
            | RuntimeOp::ProcessGroupId
            | RuntimeOp::ProcessSetGroupId
            | RuntimeOp::ProcessSessionId
            | RuntimeOp::ProcessNewSession
            | RuntimeOp::ProcessKillGroup
            | RuntimeOp::ProcessPriority
            | RuntimeOp::ProcessSetPriority
            | RuntimeOp::ProcessNice
            | RuntimeOp::ProcessRlimit
            | RuntimeOp::ProcessRlimits
            | RuntimeOp::ProcessSetRlimit
            | RuntimeOp::ProcessSignalAction
            | RuntimeOp::ProcessSetSignalAction
    )
}

pub(crate) fn call(op: RuntimeOp, args: &Args<'_>) -> Result<Value, RuntimeError> {
    match op {
        RuntimeOp::ProcessSignals => Ok(signals()),
        RuntimeOp::ProcessParentPid => parent_pid(),
        RuntimeOp::ProcessGroupId => group_id(args),
        RuntimeOp::ProcessSetGroupId => set_group_id(args),
        RuntimeOp::ProcessSessionId => session_id(args),
        RuntimeOp::ProcessNewSession => new_session(args),
        RuntimeOp::ProcessKillGroup => kill_group(args),
        RuntimeOp::ProcessPriority => priority(args),
        RuntimeOp::ProcessSetPriority => set_priority(args),
        RuntimeOp::ProcessNice => nice(args),
        RuntimeOp::ProcessRlimit => rlimit(args),
        RuntimeOp::ProcessRlimits => rlimits(),
        RuntimeOp::ProcessSetRlimit => set_rlimit(args),
        RuntimeOp::ProcessSignalAction => signal_action(args),
        RuntimeOp::ProcessSetSignalAction => set_signal_action(args),
        _ => unreachable!("process primitive expected"),
    }
}

fn ok(value: Value) -> Result<Value, RuntimeError> {
    Ok(Value::ok(value))
}

fn signals() -> Value {
    Value::List(
        signal_table()
            .into_iter()
            .map(signal_record)
            .collect::<Vec<_>>(),
    )
}

/// A pid argument where `0` means the calling process (or its group).
fn pid_arg(pid: i64, operation: &str, span: Span) -> Result<Option<Pid>, RuntimeError> {
    if pid == 0 {
        return Ok(None);
    }
    positive_pid(pid).map(Some).ok_or_else(|| {
        invalid(
            "pid-range",
            format!("{operation}: pid is out of range"),
            span,
        )
    })
}

/// A process ID that names one process: `Pid::from_raw` also accepts
/// negative numbers, which the kernel reads as groups.
pub(crate) fn positive_pid(pid: i64) -> Option<Pid> {
    i32::try_from(pid)
        .ok()
        .filter(|pid| *pid > 0)
        .and_then(Pid::from_raw)
}

fn pid_value(pid: Pid) -> Value {
    Value::Int(pid.as_raw_nonzero().get() as i64)
}

fn parent_pid() -> Result<Value, RuntimeError> {
    // Only the kernel's own pid 0 and pid 1 have no parent.
    ok(Value::Int(
        rprocess::getppid().map_or(0, |pid| pid.as_raw_nonzero().get() as i64),
    ))
}

fn group_id(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let pid = pid_arg(args.int_or(0, 0)?, "process.group_id", args.span())?;
    match rprocess::getpgid(pid) {
        Ok(pgid) => ok(pid_value(pgid)),
        Err(error) => Err(host_error("process-group-id", error, args.span())),
    }
}

fn set_group_id(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let pid = pid_arg(args.int(0)?, "process.set_group_id", args.span())?;
    let pgid = pid_arg(args.int(1)?, "process.set_group_id", args.span())?;
    match rprocess::setpgid(pid, pgid) {
        Ok(()) => ok(Value::Unit),
        Err(error) => Err(host_error("process-set-group-id", error, args.span())),
    }
}

fn session_id(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let pid = pid_arg(args.int_or(0, 0)?, "process.session_id", args.span())?;
    match rprocess::getsid(pid) {
        Ok(sid) => ok(pid_value(sid)),
        Err(error) => Err(host_error("process-session-id", error, args.span())),
    }
}

fn new_session(args: &Args<'_>) -> Result<Value, RuntimeError> {
    match rprocess::setsid() {
        Ok(sid) => ok(pid_value(sid)),
        Err(error) => Err(host_error("process-new-session", error, args.span())),
    }
}

/// Maps a `kill(2)` failure onto the error kinds `process.kill` reports.
pub(crate) fn kill_error(error: rustix::io::Errno, span: Span) -> RuntimeError {
    let error = io::Error::from(error);
    let (kind, message) = match error.raw_os_error() {
        Some(libc::ESRCH) => ("process-missing", "process does not exist".to_string()),
        Some(libc::EPERM) => ("permission-denied", "permission denied".to_string()),
        Some(libc::EINVAL) => ("invalid-signal", "invalid signal".to_string()),
        _ => ("process-kill", error.to_string()),
    };
    RuntimeError::new(kind, message)
        .with_host_facet(&error)
        .with_span(span)
}

/// `kill(-pgid, signal)`: fails when the group is missing or any member refuses
/// the signal, where `unix.kill_process_group` swallows a missing group.
fn kill_group(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let span = args.span();
    let pgid = args.int(0)?;
    let Some(pgid) = positive_pid(pgid) else {
        return Err(invalid(
            "pid-range",
            "process group id must be a positive process id",
            span,
        ));
    };
    let signal = signal_info(&args.str_or(1, "TERM")?, span)?;
    let result = if signal.number == 0 {
        rprocess::test_kill_process_group(pgid)
    } else {
        let signal = std::num::NonZeroI32::new(signal.number)
            .map(|signal| unsafe { rprocess::Signal::from_raw_nonzero_unchecked(signal) })
            .ok_or_else(|| invalid("invalid-signal", "invalid signal", span))?;
        rprocess::kill_process_group(pgid, signal)
    };
    match result {
        Ok(()) => ok(Value::Unit),
        Err(error) => Err(kill_error(error, span)),
    }
}

#[derive(Clone, Copy)]
enum Which {
    Process,
    Group,
    User,
}

fn which(args: &Args<'_>, index: usize) -> Result<Which, RuntimeError> {
    match args.str_or(index, "process")?.as_str() {
        "process" => Ok(Which::Process),
        "group" => Ok(Which::Group),
        "user" => Ok(Which::User),
        other => Err(invalid(
            "invalid-argument",
            format!("which must be `process`, `group`, or `user`, found `{other}`"),
            args.span(),
        )),
    }
}

fn user_id(id: i64, span: Span) -> Result<rprocess::Uid, RuntimeError> {
    if id == 0 {
        // getpriority(2) reads 0 as the caller's real user.
        return Ok(rprocess::getuid());
    }
    u32::try_from(id)
        .ok()
        .filter(|id| *id != u32::MAX)
        .map(rprocess::Uid::from_raw)
        .ok_or_else(|| invalid("invalid-argument", "uid is out of range", span))
}

fn priority(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let span = args.span();
    let id = args.int_or(0, 0)?;
    let result = match which(args, 1)? {
        Which::Process => rprocess::getpriority_process(pid_arg(id, "process.priority", span)?),
        Which::Group => rprocess::getpriority_pgrp(pid_arg(id, "process.priority", span)?),
        Which::User => rprocess::getpriority_user(user_id(id, span)?),
    };
    match result {
        Ok(value) => ok(Value::Int(value as i64)),
        Err(error) => Err(host_error("process-priority", error, span)),
    }
}

fn priority_value(value: i64, span: Span) -> Result<i32, RuntimeError> {
    i32::try_from(value).map_err(|_| invalid("invalid-argument", "priority is out of range", span))
}

fn set_priority(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let span = args.span();
    let id = args.int(0)?;
    let value = priority_value(args.int(1)?, span)?;
    let result = match which(args, 2)? {
        Which::Process => {
            rprocess::setpriority_process(pid_arg(id, "process.set_priority", span)?, value)
        }
        Which::Group => {
            rprocess::setpriority_pgrp(pid_arg(id, "process.set_priority", span)?, value)
        }
        Which::User => rprocess::setpriority_user(user_id(id, span)?, value),
    };
    match result {
        Ok(()) => ok(Value::Unit),
        Err(error) => Err(host_error("process-set-priority", error, span)),
    }
}

/// `nice(2)`: adds `increment` to the calling process's niceness and returns
/// the new value; a negative increment needs privilege.
fn nice(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let span = args.span();
    let increment = priority_value(args.int(0)?, span)?;
    match rprocess::nice(increment) {
        Ok(value) => ok(Value::Int(value as i64)),
        Err(error) => Err(host_error("process-nice", error, span)),
    }
}

const RLIMIT_NAMES: &[(&str, Resource)] = &[
    ("cpu", Resource::Cpu),
    ("fsize", Resource::Fsize),
    ("data", Resource::Data),
    ("stack", Resource::Stack),
    ("core", Resource::Core),
    ("rss", Resource::Rss),
    ("nproc", Resource::Nproc),
    ("nofile", Resource::Nofile),
    ("memlock", Resource::Memlock),
    ("as", Resource::As),
    #[cfg(any(target_os = "linux", target_os = "android"))]
    ("locks", Resource::Locks),
    #[cfg(any(target_os = "linux", target_os = "android"))]
    ("sigpending", Resource::Sigpending),
    #[cfg(any(target_os = "linux", target_os = "android"))]
    ("msgqueue", Resource::Msgqueue),
    #[cfg(any(target_os = "linux", target_os = "android"))]
    ("nice", Resource::Nice),
    #[cfg(any(target_os = "linux", target_os = "android"))]
    ("rtprio", Resource::Rtprio),
    #[cfg(any(target_os = "linux", target_os = "android"))]
    ("rttime", Resource::Rttime),
];

fn resource_arg(args: &Args<'_>, index: usize) -> Result<(&'static str, Resource), RuntimeError> {
    let name = args.str(index)?;
    RLIMIT_NAMES
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .copied()
        .ok_or_else(|| {
            invalid(
                "invalid-argument",
                format!(
                    "unknown resource `{name}`; expected one of {}",
                    RLIMIT_NAMES
                        .iter()
                        .map(|(name, _)| *name)
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                args.span(),
            )
        })
}

/// `None` is unlimited; a finite value beyond `Int` saturates.
fn limit_value(limit: Option<u64>) -> Value {
    optional_int(limit.map(|limit| i64::try_from(limit).unwrap_or(i64::MAX)))
}

fn rlimit_record(name: &str, limit: Rlimit) -> Value {
    Value::Record(RecordMap::from([
        (key("resource"), Value::Str(name.into())),
        (key("soft"), limit_value(limit.current)),
        (key("hard"), limit_value(limit.maximum)),
    ]))
}

fn rlimit(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let (name, resource) = resource_arg(args, 0)?;
    ok(rlimit_record(name, getrlimit(resource)))
}

fn rlimits() -> Result<Value, RuntimeError> {
    ok(Value::List(
        RLIMIT_NAMES
            .iter()
            .map(|(name, resource)| rlimit_record(name, getrlimit(*resource)))
            .collect(),
    ))
}

/// Sets the soft and hard limits named by non-omitted arguments; `null` is
/// unlimited. Lowering the hard limit below an omitted soft one lowers the
/// soft one with it.
fn set_rlimit(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let span = args.span();
    let (_, resource) = resource_arg(args, 0)?;
    let mut limit = getrlimit(resource);
    let apply = |slot: Slot<i64>, target: &mut Option<u64>| match slot {
        Slot::Omitted => Ok(()),
        Slot::Null => {
            *target = None;
            Ok(())
        }
        Slot::Value(value) => u64::try_from(value)
            .map(|value| *target = Some(value))
            .map_err(|_| invalid("invalid-argument", "limit cannot be negative", span)),
    };
    let soft = args.slot(1)?;
    let hard = args.slot(2)?;
    apply(soft, &mut limit.current)?;
    apply(hard, &mut limit.maximum)?;
    if soft == Slot::Omitted
        && let Some(maximum) = limit.maximum
        && limit.current.is_none_or(|current| current > maximum)
    {
        limit.current = Some(maximum);
    }
    match setrlimit(resource, limit) {
        Ok(()) => ok(Value::Unit),
        Err(error) => Err(host_error("process-set-rlimit", error, span)),
    }
}

fn signal_number_arg(args: &Args<'_>, index: usize) -> Result<i32, RuntimeError> {
    let signal = signal_info(&args.str(index)?, args.span())?;
    if signal.number == 0 {
        return Err(invalid(
            "invalid-signal",
            "signal 0 has no disposition",
            args.span(),
        ));
    }
    Ok(signal.number)
}

fn signal_action(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let number = signal_number_arg(args, 0)?;
    let mut current: libc::sigaction = unsafe { std::mem::zeroed() };
    // SAFETY: a null new action only queries the current one into `current`.
    if unsafe { libc::sigaction(number, std::ptr::null(), &mut current) } != 0 {
        return Err(host_error(
            "process-signal-action",
            io::Error::last_os_error(),
            args.span(),
        ));
    }
    let action = if current.sa_sigaction == libc::SIG_IGN {
        "ignore"
    } else if current.sa_sigaction == libc::SIG_DFL {
        "default"
    } else {
        "handler"
    };
    ok(Value::Str(action.into()))
}

/// Ignores a signal or restores its default action in the running process. The
/// disposition survives `unix.exec`, so `ignore` before `exec` is how `nohup`
/// is built. The runtime's own handlers for `INT` and `TERM` are replaced.
fn set_signal_action(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let number = signal_number_arg(args, 0)?;
    let handler = match args.str(1)?.as_str() {
        "ignore" => libc::SIG_IGN,
        "default" => libc::SIG_DFL,
        other => {
            return Err(invalid(
                "invalid-argument",
                format!("action must be `ignore` or `default`, found `{other}`"),
                args.span(),
            ));
        }
    };
    let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
    action.sa_sigaction = handler;
    // SAFETY: `action` is fully initialised and the signal mask is emptied
    // before the call.
    let status = unsafe {
        libc::sigemptyset(&mut action.sa_mask);
        libc::sigaction(number, &action, std::ptr::null_mut())
    };
    if status == 0 {
        ok(Value::Unit)
    } else {
        Err(host_error(
            "process-set-signal-action",
            io::Error::last_os_error(),
            args.span(),
        ))
    }
}
