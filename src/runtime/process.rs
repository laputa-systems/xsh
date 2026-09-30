#![allow(clippy::single_call_fn)]

use crate::runtime::cgroup::{CgroupError, CgroupScope};
use crate::runtime::value::RunError;
use rustc_hash::FxHashMap;
use rustix::{event as revent, fs as rfs, io as rio, process as rprocess, stdio, termios};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io;
use std::num::NonZeroI32;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, RawFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const CAPTURE_LIMIT: usize = 16 * 1024 * 1024;
const CANCELLATION_GRACE: Duration = Duration::from_millis(150);
pub(crate) const WAIT_POLL: Duration = Duration::from_millis(10);
static PRIMARY_SIGNAL: AtomicI32 = AtomicI32::new(0);
static ESCALATION_SIGNAL: AtomicI32 = AtomicI32::new(0);

pub fn install_cancellation_signal_handlers() -> io::Result<SignalHandlerGuard> {
    SignalHandlerGuard::install_many(&[libc::SIGINT, libc::SIGTERM])
}

pub fn install_immediate_cancellation_signal_handlers() -> io::Result<SignalHandlerGuard> {
    SignalHandlerGuard::install_many_with(
        &[libc::SIGINT, libc::SIGTERM],
        handle_immediate_cancellation_signal as *const () as usize,
    )
}

pub fn install_interactive_signal_handlers() -> io::Result<SignalHandlerGuard> {
    SignalHandlerGuard::ignore_many(&[
        libc::SIGTSTP,
        libc::SIGTTIN,
        libc::SIGTTOU,
        libc::SIGQUIT,
        libc::SIGPIPE,
        libc::SIGINT,
    ])
}

pub fn initialize_interactive_process_group() -> io::Result<Option<InteractiveProcessGroupGuard>> {
    let fd = libc::STDIN_FILENO;
    if !termios::isatty(stdio::stdin()) {
        return Ok(None);
    }
    let previous = match termios::tcgetpgrp(stdio::stdin()) {
        Ok(pid) => pid.as_raw_pid(),
        Err(error) => {
            let error = io::Error::from(error);
            if matches!(error.raw_os_error(), Some(libc::ENOTTY)) {
                return Ok(None);
            }
            return Err(error);
        }
    };

    let pid = rprocess::getpid();
    let current = rprocess::getpgrp();
    if current != pid
        && let Err(error) = rprocess::setpgid(None, None)
    {
        let error = io::Error::from(error);
        if !is_benign_setpgid_race(&error) {
            return Err(error);
        }
    }
    let shell_pgid = rprocess::getpgrp().as_raw_pid();
    tcsetpgrp_ignoring_ttou(fd, shell_pgid)?;
    Ok(Some(InteractiveProcessGroupGuard {
        fd,
        previous,
        shell_pgid,
    }))
}

pub fn clear_cancellation_request() {
    PRIMARY_SIGNAL.store(0, Ordering::SeqCst);
    ESCALATION_SIGNAL.store(0, Ordering::SeqCst);
}

pub fn cancellation_requested_signal() -> Option<i32> {
    signal_snapshot().primary
}

pub fn cancellation_escalated_signal() -> Option<i32> {
    signal_snapshot().escalation
}

pub(crate) fn signal_snapshot() -> SignalSnapshot {
    SignalSnapshot {
        primary: match PRIMARY_SIGNAL.load(Ordering::SeqCst) {
            0 => None,
            signal => Some(signal),
        },
        escalation: match ESCALATION_SIGNAL.load(Ordering::SeqCst) {
            0 => None,
            signal => Some(signal),
        },
    }
}

pub(crate) fn install_hook_signal_handler(signal: i32) -> io::Result<SignalHandlerGuard> {
    SignalHandlerGuard::install_many(&[signal])
}

extern "C" fn handle_cancellation_signal(signal: i32) {
    record_signal(signal);
}

pub(crate) fn record_signal(signal: i32) {
    if PRIMARY_SIGNAL
        .compare_exchange(0, signal, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        let _ = ESCALATION_SIGNAL.compare_exchange(0, signal, Ordering::SeqCst, Ordering::SeqCst);
    }
}

extern "C" fn handle_immediate_cancellation_signal(signal: i32) {
    unsafe { libc::_exit((128 + signal).clamp(1, 255)) };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SignalSnapshot {
    pub primary: Option<i32>,
    pub escalation: Option<i32>,
}

pub struct SignalHandlerGuard {
    previous: Vec<(i32, libc::sigaction)>,
}

impl SignalHandlerGuard {
    fn install_many(signals: &[i32]) -> io::Result<Self> {
        Self::install_many_with(signals, handle_cancellation_signal as *const () as usize)
    }

    fn ignore_many(signals: &[i32]) -> io::Result<Self> {
        Self::install_many_with(signals, libc::SIG_IGN)
    }

    fn install_many_with(signals: &[i32], handler: usize) -> io::Result<Self> {
        let mut previous = Vec::new();
        for signal in signals {
            match install_signal_handler(*signal, handler) {
                Ok(old) => previous.push((*signal, old)),
                Err(error) => {
                    for (installed, old) in previous.into_iter().rev() {
                        restore_signal_handler(installed, &old);
                    }
                    return Err(error);
                }
            }
        }
        Ok(Self { previous })
    }
}

impl Drop for SignalHandlerGuard {
    fn drop(&mut self) {
        for (signal, old) in self.previous.iter().rev() {
            restore_signal_handler(*signal, old);
        }
    }
}

pub struct InteractiveProcessGroupGuard {
    fd: RawFd,
    previous: libc::pid_t,
    shell_pgid: libc::pid_t,
}

impl Drop for InteractiveProcessGroupGuard {
    fn drop(&mut self) {
        if self.previous > 0 && self.previous != self.shell_pgid {
            let _ = tcsetpgrp_ignoring_ttou(self.fd, self.previous);
        }
    }
}

pub enum CancellationDecision {
    Continue,
    Forward(i32),
    Escalate(i32),
}

pub trait CancellationPolicy {
    fn check_process_group(&mut self, group: ProcessGroup) -> CancellationDecision;

    fn process_group_finished(&mut self, _group: ProcessGroup) {}
}

struct DefaultCancellationPolicy;

impl CancellationPolicy for DefaultCancellationPolicy {
    fn check_process_group(&mut self, _group: ProcessGroup) -> CancellationDecision {
        let snapshot = signal_snapshot();
        if let Some(escalation) = snapshot.escalation {
            return CancellationDecision::Escalate(snapshot.primary.unwrap_or(escalation));
        }
        snapshot.primary.map_or(
            CancellationDecision::Continue,
            CancellationDecision::Forward,
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessStatus {
    pub success: bool,
    pub kind: ProcessStatusKind,
    pub code: Option<i32>,
    pub segments: Vec<ProcessSegmentStatus>,
}

impl ProcessStatus {
    pub fn exited(code: i32) -> Self {
        Self {
            success: code == 0,
            kind: ProcessStatusKind::Exit,
            code: Some(code),
            segments: vec![ProcessSegmentStatus {
                index: 0,
                target: Vec::new(),
                pid: None,
                success: code == 0,
                kind: ProcessSegmentStatusKind::Exit,
                code: Some(code),
                error_kind: None,
                error_message: None,
            }],
        }
    }

    pub fn signaled(signal: i32) -> Self {
        Self {
            success: false,
            kind: ProcessStatusKind::Signal,
            code: Some(signal),
            segments: vec![ProcessSegmentStatus {
                index: 0,
                target: Vec::new(),
                pid: None,
                success: false,
                kind: ProcessSegmentStatusKind::Signal,
                code: Some(signal),
                error_kind: None,
                error_message: None,
            }],
        }
    }

    pub fn from_segments(segments: Vec<ProcessSegmentStatus>) -> Self {
        let success = !segments.is_empty() && segments.iter().all(|segment| segment.success);
        let summary = segments
            .iter()
            .find(|segment| !segment.success)
            .or_else(|| segments.last());
        let (kind, code) = summary.map_or((ProcessStatusKind::Exec, None), |segment| {
            (segment.kind.into(), segment.code)
        });
        Self {
            success,
            kind,
            code,
            segments,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessStatusKind {
    Exit,
    Signal,
    Exec,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessSegmentStatus {
    pub index: usize,
    pub target: Vec<u8>,
    pub pid: Option<u32>,
    pub success: bool,
    pub kind: ProcessSegmentStatusKind,
    pub code: Option<i32>,
    pub error_kind: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessSegmentStatusKind {
    Exit,
    Signal,
    Exec,
}

impl From<ProcessSegmentStatusKind> for ProcessStatusKind {
    fn from(kind: ProcessSegmentStatusKind) -> Self {
        match kind {
            ProcessSegmentStatusKind::Exit => Self::Exit,
            ProcessSegmentStatusKind::Signal => Self::Signal,
            ProcessSegmentStatusKind::Exec => Self::Exec,
        }
    }
}

/// Validated ordinary process exit codes. Signals and execution failures never
/// enter this set, even when their shell-style status number would match.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AcceptedExitCodes([u64; 4]);

impl AcceptedExitCodes {
    pub fn new(codes: &[i64]) -> Result<Self, RunError> {
        if codes.is_empty() {
            return Err(RunError::new("accept-policy", "accepted exit codes must be a nonempty List[Int]"));
        }
        let mut bits = [0u64; 4];
        for code in codes {
            let code = u8::try_from(*code).map_err(|_| RunError::new("accept-policy", "accepted exit codes must be in 0..255"))?;
            let slot = &mut bits[usize::from(code / 64)];
            let mask = 1u64 << (code % 64);
            if *slot & mask != 0 {
                return Err(RunError::new("accept-policy", "accepted exit codes must not contain duplicates"));
            }
            *slot |= mask;
        }
        Ok(Self(bits))
    }

    fn accepts(self, segment: &ProcessSegmentStatus) -> bool {
        segment.kind == ProcessSegmentStatusKind::Exit && segment.code.and_then(|code| u8::try_from(code).ok())
            .is_some_and(|code| self.0[usize::from(code / 64)] & (1u64 << (code % 64)) != 0)
    }
}

pub(crate) fn rejected_segment<'a>(
    status: &'a ProcessStatus,
    policies: &[Option<AcceptedExitCodes>],
    require_zero: bool,
) -> Option<&'a ProcessSegmentStatus> {
    let require_zero = require_zero || policies.len() > 1 && policies.iter().any(Option::is_some);
    status.segments.iter().find(|segment| {
        policies.get(segment.index).copied().flatten()
            .map_or(require_zero && !segment.success, |policy| !policy.accepts(segment))
    })
}

pub(crate) fn completion_error(
    status: &ProcessStatus,
    policies: &[Option<AcceptedExitCodes>],
    require_zero: bool,
) -> Option<RunError> {
    let segment = rejected_segment(status, policies, require_zero)?;
    if policies.iter().all(Option::is_none) {
        return Some(RunError::from_status(status.clone()));
    }
    if segment.kind == ProcessSegmentStatusKind::Exit
        && policies.get(segment.index).is_some_and(Option::is_some)
    {
        return Some(RunError::new(
            if status.segments.len() > 1 { "pipeline-failure" } else { "unexpected-exit" },
            format!("pipeline segment {} `{}` exited with unaccepted status {}", segment.index,
                String::from_utf8_lossy(&segment.target), segment.code.unwrap_or_default()),
        ).with_status(status.clone()));
    }
    let mut error = RunError::from_status(ProcessStatus::from_segments(vec![segment.clone()]));
    if status.segments.len() > 1 && segment.error_kind.is_none() {
        error.kind = "pipeline-failure".to_string();
    }
    Some(error.with_status(status.clone()))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessInvocation {
    pub target: Vec<u8>,
    pub argv: Vec<Vec<u8>>,
    pub cwd: PathBuf,
    pub env: BTreeMap<Vec<u8>, Vec<u8>>,
    pub env_overlay: BTreeMap<Vec<u8>, Vec<u8>>,
    pub redirections: Vec<ProcessRedirection>,
    pub timeout: Option<Duration>,
    pub cpu_max: Option<i64>,
    pub accepted_exit_codes: Option<AcceptedExitCodes>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProcessRedirection {
    Input { bytes: Arc<[u8]> },
    File {
        stream: RedirectionStream,
        mode: FileRedirectionMode,
        path: PathBuf,
    },
    Dup {
        stream: RedirectionStream,
        fd: i32,
    },
    /// Makes `stream` a copy of the child's own descriptor `fd` (the shell's
    /// `2>&1`), resolved in the child at this point of the list: redirections
    /// before it have taken effect, those after it have not. The list up to
    /// the first `ChildDup` is applied by the parent as usual; from there on it
    /// is applied in the child, in order. `Dup` instead copies the descriptor
    /// as the parent process holds it.
    ChildDup {
        stream: RedirectionStream,
        fd: i32,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RedirectionStream {
    Stdin,
    Stdout,
    Stderr,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileRedirectionMode {
    Read,
    Write,
    Append,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessEnd {
    pub pid: Option<u32>,
    pub status: Option<ProcessStatus>,
    pub error: Option<RunError>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessOutput {
    pub end: ProcessEnd,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SpawnOptions {
    pub detach: bool,
    pub new_session: bool,
    pub ignore_hup: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpawnedProcess {
    pub pid: u32,
    pub target: Vec<u8>,
    pub argv: Vec<Vec<u8>>,
    pub options: SpawnOptions,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManagedStdio {
    Inherit,
    Null,
    Piped,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaitMode {
    Script,
    InteractiveForeground,
    Nonblocking,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ChildWaitOutcome {
    Exited(ProcessStatus),
    Signaled(ProcessStatus),
    Stopped { signal: i32 },
    StillRunning,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpawnManagedOptions {
    pub stdin: ManagedStdio,
    pub stdout: ManagedStdio,
    pub stderr: ManagedStdio,
    pub apply_redirections: bool,
    pub group: ProcessGroupConfig,
    pub reset_signals: bool,
    pub spawn: SpawnOptions,
}

impl SpawnManagedOptions {
    pub fn inherited_process_group() -> Self {
        Self {
            stdin: ManagedStdio::Inherit,
            stdout: ManagedStdio::Inherit,
            stderr: ManagedStdio::Inherit,
            apply_redirections: true,
            group: ProcessGroupConfig::NewRoot,
            reset_signals: true,
            spawn: SpawnOptions::default(),
        }
    }

    pub fn detached_command(options: SpawnOptions) -> Self {
        Self {
            stdin: ManagedStdio::Null,
            stdout: ManagedStdio::Null,
            stderr: ManagedStdio::Null,
            apply_redirections: false,
            group: ProcessGroupConfig::Inherit,
            reset_signals: true,
            spawn: options,
        }
    }
}

// The process owner retains one immutable payload and advances a bounded write
// at polling checkpoints, so capture reads and cancellation continue to run.
struct InputDelivery {
    writer: Option<std::process::ChildStdin>,
    bytes: Arc<[u8]>,
    offset: usize,
}

impl InputDelivery {
    fn start(child: &mut std::process::Child, invocation: &ProcessInvocation, group: ProcessGroup) -> Result<Option<Self>, RunError> {
        let Some(bytes) = invocation.redirections.iter().find_map(|item| match item {
            ProcessRedirection::Input { bytes } => Some(bytes.clone()), _ => None,
        }) else { return Ok(None); };
        let Some(writer) = child.stdin.take() else {
            group.kill(); let _ = child.kill(); let _ = child.wait();
            return Err(RunError::new("io", "missing Bytes input pipe"));
        };
        if let Err(error) = set_nonblocking(writer.as_fd()) {
            group.kill(); let _ = child.kill(); let _ = child.wait(); return Err(error);
        }
        Ok(Some(Self { writer: Some(writer), bytes, offset: 0 }))
    }

    fn pump(&mut self) -> Result<(), RunError> {
        let Some(writer) = self.writer.as_ref() else { return Ok(()); };
        if self.offset == self.bytes.len() { self.writer = None; return Ok(()); }
        let end = self.offset.saturating_add(65536).min(self.bytes.len());
        match rio::write(writer, &self.bytes[self.offset..end]) {
            Ok(0) => return Err(RunError::new("io", "stdin write made no progress")),
            Ok(count) => { self.offset += count; if self.offset == self.bytes.len() { self.writer = None; } }
            Err(rustix::io::Errno::PIPE) => self.writer = None,
            Err(rustix::io::Errno::AGAIN | rustix::io::Errno::INTR) => {}
            Err(error) => return Err(RunError::new("io", error.to_string())),
        }
        Ok(())
    }
}

fn feed_input(input: &mut Option<InputDelivery>) -> Result<(), RunError> {
    if let Some(delivery) = input.as_mut() {
        delivery.pump()?;
        if delivery.writer.is_none() { *input = None; }
    }
    Ok(())
}

#[allow(dead_code)]
pub struct ManagedChild {
    child: std::process::Child,
    pub pid: u32,
    pub pgid: libc::pid_t,
    pub target: Vec<u8>,
    pub argv: Vec<Vec<u8>>,
    pub cwd: PathBuf,
    pub env_overlay: BTreeMap<Vec<u8>, Vec<u8>>,
    pub deadline: Option<Instant>,
    pub detached: bool,
    pub consumed: bool,
    cgroup: CgroupScope,
    input: Option<InputDelivery>,
    input_error: Option<RunError>,
    accepted_exit_codes: Option<AcceptedExitCodes>,
}

impl ManagedChild {
    pub(crate) fn completion_error(&self, status: &ProcessStatus) -> Option<RunError> {
        completion_error(status, &[self.accepted_exit_codes], false)
    }

    pub fn process_group(&self) -> ProcessGroup {
        ProcessGroup { pgid: self.pgid }
    }
}

pub fn run_inherit(invocation: &ProcessInvocation) -> Result<ProcessEnd, RunError> {
    let mut policy = DefaultCancellationPolicy;
    run_inherit_with_policy(invocation, &mut policy)
}

pub fn run_inherit_with_policy(
    invocation: &ProcessInvocation,
    policy: &mut dyn CancellationPolicy,
) -> Result<ProcessEnd, RunError> {
    run_managed_with_policy(
        invocation,
        policy,
        SpawnManagedOptions::inherited_process_group(),
    )
}

/// Like `run_inherit_with_policy`, but the child's stdout/stderr are discarded
/// (sent to `/dev/null`). Used by `time.measure(.., quiet: true)` so benchmarking
/// a command does not flood the terminal with its output.
pub fn run_quiet_with_policy(
    invocation: &ProcessInvocation,
    policy: &mut dyn CancellationPolicy,
) -> Result<ProcessEnd, RunError> {
    let mut options = SpawnManagedOptions::inherited_process_group();
    options.stdout = ManagedStdio::Null;
    options.stderr = ManagedStdio::Null;
    options.apply_redirections = false;
    run_managed_with_policy(invocation, policy, options)
}

fn run_managed_with_policy(
    invocation: &ProcessInvocation,
    policy: &mut dyn CancellationPolicy,
    options: SpawnManagedOptions,
) -> Result<ProcessEnd, RunError> {
    let mut child = match spawn_managed(invocation, options) {
        Ok(child) => child,
        Err(error) if setup_error_is_hard(&error) => return Err(error),
        Err(error) => return Ok(process_end_from_exec_failure(0, &invocation.target, error)),
    };
    let pid = Some(child.pid);
    let group = child.process_group();
    let _foreground = ForegroundTerminal::take(group);
    let (outcome, cancellation) = wait_managed(&mut child, WaitMode::Script, policy)?;
    let status = match outcome {
        ChildWaitOutcome::Exited(status) | ChildWaitOutcome::Signaled(status) => status,
        ChildWaitOutcome::Stopped { .. } | ChildWaitOutcome::StillRunning => {
            ProcessStatus::signaled(libc::SIGTERM)
        }
    };
    if let Some(cancellation) = cancellation {
        return Err(cancellation.error(Some(status)));
    }
    Ok(ProcessEnd {
        pid,
        status: Some(status),
        error: None,
    })
}

pub fn run_pipeline_inherit(invocations: &[ProcessInvocation]) -> Result<ProcessEnd, RunError> {
    let mut policy = DefaultCancellationPolicy;
    run_pipeline_inherit_with_policy(invocations, &mut policy)
}

pub fn run_pipeline_inherit_with_policy(
    invocations: &[ProcessInvocation],
    policy: &mut dyn CancellationPolicy,
) -> Result<ProcessEnd, RunError> {
    if invocations.len() == 1 {
        return run_inherit_with_policy(&invocations[0], policy);
    }

    if invocations.iter().skip(1).any(|invocation| invocation.redirections.iter().any(|item| matches!(item, ProcessRedirection::Input { .. }))) {
        return Err(RunError::new("redirection", "Bytes input is only valid on the first byte pipeline segment"));
    }
    let mut children: Vec<StartedChild> = Vec::new();
    let mut previous_stdout: Option<ChildStdout> = None;
    let mut segment_statuses: Vec<Option<ProcessSegmentStatus>> = vec![None; invocations.len()];
    let mut process_group = None;
    let cgroup = CgroupScope::cpu_max(
        invocations
            .first()
            .and_then(|invocation| invocation.cpu_max),
        "xsh-pipeline",
    )
    .map_err(map_cgroup_error)?;

    for (index, invocation) in invocations.iter().enumerate() {
        let executable = match resolve_executable(invocation) {
            Ok(executable) => executable,
            Err(error) => {
                segment_statuses[index] =
                    Some(exec_failure_segment(index, &invocation.target, error));
                previous_stdout = None;
                break;
            }
        };

        let stdin = if index == 0 {
            Stdio::inherit()
        } else if let Some(stdout) = previous_stdout.take() {
            Stdio::from(stdout)
        } else {
            Stdio::null()
        };
        let stdout = if index + 1 == invocations.len() {
            Stdio::inherit()
        } else {
            Stdio::piped()
        };

        let group_config = process_group
            .map(|group: ProcessGroup| ProcessGroupConfig::Join(group.pgid))
            .unwrap_or(ProcessGroupConfig::NewRoot);
        let mut command = command_with_stdio(
            invocation,
            executable,
            stdin,
            stdout,
            Stdio::inherit(),
            group_config,
        )?;
        match command.spawn() {
            Ok(mut child) => {
                let group = process_group.unwrap_or_else(|| {
                    let group = ProcessGroup::from_child(&child);
                    process_group = Some(group);
                    group
                });
                assign_child_to_process_group(&child, group);
                if let Err(error) = cgroup.assign_pid(i64::from(child.id())) {
                    group.kill();
                    let _ = child.wait();
                    for started in &mut children {
                        let _ = started.child.wait();
                    }
                    return Err(map_cgroup_error(error));
                }
                let pid = Some(child.id());
                previous_stdout = if index + 1 == invocations.len() {
                    None
                } else {
                    child.stdout.take()
                };
                let input = match InputDelivery::start(&mut child, invocation, group) {
                    Ok(input) => input,
                    Err(error) => { group.kill(); for started in &mut children { let _ = started.child.wait(); } return Err(error); }
                };
                children.push(StartedChild {
                    input,
                    index,
                    target: invocation.target.clone(),
                    pid,
                    child,
                });
                if signal_snapshot().primary.is_some() {
                    break;
                }
            }
            Err(error) => {
                segment_statuses[index] = Some(exec_failure_segment(
                    index,
                    &invocation.target,
                    map_spawn_error(error),
                ));
                previous_stdout = None;
                break;
            }
        }
    }

    drop(previous_stdout);
    let cancellation = if let Some(group) = process_group {
        let _foreground = ForegroundTerminal::take(group);
        let deadline = invocations
            .iter()
            .filter_map(|invocation| invocation.timeout)
            .min()
            .map(|timeout| Instant::now() + timeout);
        wait_children(
            &mut children,
            group,
            &mut segment_statuses,
            deadline,
            policy,
        )?
    } else {
        None
    };

    let segments = segment_statuses.into_iter().flatten().collect();
    let status = ProcessStatus::from_segments(segments);
    if let Some(cancellation) = cancellation {
        return Err(cancellation.error(Some(status)));
    }
    Ok(ProcessEnd {
        pid: None,
        status: Some(status),
        error: None,
    })
}

pub fn run_capture(invocation: &ProcessInvocation) -> Result<ProcessOutput, RunError> {
    let mut policy = DefaultCancellationPolicy;
    run_capture_stdio(invocation, false, &mut policy)
}

pub fn run_capture_with_stderr(invocation: &ProcessInvocation) -> Result<ProcessOutput, RunError> {
    let mut policy = DefaultCancellationPolicy;
    run_capture_stdio(invocation, true, &mut policy)
}

pub fn run_capture_with_policy(
    invocation: &ProcessInvocation,
    policy: &mut dyn CancellationPolicy,
) -> Result<ProcessOutput, RunError> {
    run_capture_stdio(invocation, false, policy)
}

pub fn run_capture_with_stderr_policy(
    invocation: &ProcessInvocation,
    policy: &mut dyn CancellationPolicy,
) -> Result<ProcessOutput, RunError> {
    run_capture_stdio(invocation, true, policy)
}

fn run_capture_stdio(
    invocation: &ProcessInvocation,
    capture_stderr: bool,
    policy: &mut dyn CancellationPolicy,
) -> Result<ProcessOutput, RunError> {
    let executable = resolve_executable(invocation)?;
    let cgroup = CgroupScope::cpu_max(invocation.cpu_max, "xsh-run").map_err(map_cgroup_error)?;
    let stderr = if capture_stderr {
        Stdio::piped()
    } else {
        Stdio::inherit()
    };
    let mut child = command_with_stdio(
        invocation,
        executable,
        Stdio::inherit(),
        Stdio::piped(),
        stderr,
        ProcessGroupConfig::NewRoot,
    )?
    .spawn()
    .map_err(map_spawn_error)?;
    let pid = Some(child.id());
    let group = ProcessGroup::from_child(&child);
    assign_child_to_process_group(&child, group);
    if let Err(error) = cgroup.assign_pid(i64::from(child.id())) {
        group.kill();
        let _ = child.wait();
        return Err(map_cgroup_error(error));
    }
    let _foreground = ForegroundTerminal::take(group);
    let mut input = InputDelivery::start(&mut child, invocation, group)?;
    let stdout = child.stdout.take();
    let stderr = if capture_stderr {
        child.stderr.take()
    } else {
        None
    };
    let stdout_fd = stdout.as_ref().map(|stdout| stdout.as_raw_fd());
    let stderr_fd = stderr.as_ref().map(|stderr| stderr.as_raw_fd());
    if let Some(stdout) = stdout.as_ref() {
        set_nonblocking(stdout.as_fd())?;
    }
    if let Some(stderr) = stderr.as_ref() {
        set_nonblocking(stderr.as_fd())?;
    }
    let deadline = invocation.timeout.map(|timeout| Instant::now() + timeout);
    let mut captured_stdout = Vec::new();
    let mut captured_stderr = Vec::new();
    let mut cancellation = None;
    let mut capture_limit_hit = false;
    let mut buf = [0u8; 8192];

    let status = loop {
        if let Err(error) = feed_input(&mut input) {
            group.kill(); let _ = child.wait(); return Err(error);
        }
        if let Some(stdout_fd) = stdout_fd {
            drain_capture_fd(
                stdout_fd,
                &mut captured_stdout,
                &mut capture_limit_hit,
                group,
                &mut buf,
            );
        }
        if let Some(stderr_fd) = stderr_fd {
            drain_capture_fd(
                stderr_fd,
                &mut captured_stderr,
                &mut capture_limit_hit,
                group,
                &mut buf,
            );
        }

        if let Some(status) = child.try_wait().map_err(map_wait_error)? {
            if !capture_limit_hit {
                if let Some(stdout_fd) = stdout_fd {
                    drain_capture_fd(
                        stdout_fd,
                        &mut captured_stdout,
                        &mut capture_limit_hit,
                        group,
                        &mut buf,
                    );
                }
                if let Some(stderr_fd) = stderr_fd {
                    drain_capture_fd(
                        stderr_fd,
                        &mut captured_stderr,
                        &mut capture_limit_hit,
                        group,
                        &mut buf,
                    );
                }
            }
            break process_status(status, 0, &invocation.target, pid);
        }

        if timeout_elapsed(deadline) {
            group.kill();
            let status = child.wait().map_err(map_wait_error)?;
            let status = process_status(status, 0, &invocation.target, pid);
            return Err(RunError::new("timeout", "process timed out").with_status(status));
        }

        if !capture_limit_hit {
            check_cancellation(group, &mut cancellation, policy);
        }

        let mut pollfds = Vec::new();
        if let Some(writer) = input.as_ref().and_then(|input| input.writer.as_ref()) {
            pollfds.push(revent::PollFd::new(writer, revent::PollFlags::OUT));
        }
        if let Some(stdout_fd) = stdout_fd {
            let stdout = unsafe { BorrowedFd::borrow_raw(stdout_fd) };
            pollfds.push(revent::PollFd::from_borrowed_fd(
                stdout,
                revent::PollFlags::IN,
            ));
        }
        if let Some(stderr_fd) = stderr_fd {
            let stderr = unsafe { BorrowedFd::borrow_raw(stderr_fd) };
            pollfds.push(revent::PollFd::from_borrowed_fd(
                stderr,
                revent::PollFlags::IN,
            ));
        }
        let timeout = revent::Timespec::try_from(WAIT_POLL).expect("WAIT_POLL fits Timespec");
        if pollfds.is_empty() {
            std::thread::sleep(WAIT_POLL);
        } else {
            let _ = revent::poll(&mut pollfds, Some(&timeout));
        }
    };

    if capture_limit_hit && cancellation.is_none() {
        return Err(RunError::new(
            "capture-limit",
            "captured stdout exceeded the capture limit",
        ));
    }
    if let Some(cancellation) = cancellation {
        return Err(cancellation.error(Some(status)));
    }
    policy.process_group_finished(group);
    if let Some(error) = completion_error(&status, &[invocation.accepted_exit_codes], false) { return Err(error); }
    Ok(ProcessOutput {
        end: ProcessEnd {
            pid,
            status: Some(status),
            error: None,
        },
        stdout: captured_stdout,
        stderr: captured_stderr,
    })
}

fn drain_capture_fd(
    fd: RawFd,
    captured: &mut Vec<u8>,
    capture_limit_hit: &mut bool,
    group: ProcessGroup,
    buf: &mut [u8],
) {
    loop {
        let fd = unsafe { BorrowedFd::borrow_raw(fd) };
        let Ok(n) = rio::read(fd, &mut *buf) else {
            break;
        };
        if n == 0 {
            break;
        }
        if *capture_limit_hit {
            continue;
        }
        if captured.len() + n > CAPTURE_LIMIT {
            *capture_limit_hit = true;
            group.kill();
        } else {
            captured.extend_from_slice(&buf[..n]);
        }
    }
}

pub fn spawn_command(
    invocation: &ProcessInvocation,
    options: SpawnOptions,
) -> Result<SpawnedProcess, RunError> {
    let managed = spawn_managed(invocation, SpawnManagedOptions::detached_command(options))?;
    let pid = managed.pid;
    release_to_reaper(managed);
    Ok(SpawnedProcess {
        pid,
        target: invocation.target.clone(),
        argv: invocation.argv.clone(),
        options,
    })
}

pub fn spawn_managed(
    invocation: &ProcessInvocation,
    options: SpawnManagedOptions,
) -> Result<ManagedChild, RunError> {
    let executable = resolve_executable(invocation)?;
    let cgroup = CgroupScope::cpu_max(invocation.cpu_max, "xsh-spawn").map_err(map_cgroup_error)?;
    let mut command = command_with_managed_stdio(invocation, executable, options)?;
    let mut child = command.spawn().map_err(map_spawn_error)?;
    let pid = child.id();
    let group = match options.group {
        ProcessGroupConfig::Join(pgid) => ProcessGroup { pgid },
        ProcessGroupConfig::Inherit | ProcessGroupConfig::NewRoot => {
            ProcessGroup::from_child(&child)
        }
    };
    if options.group.parent_sets_group() {
        assign_child_to_process_group(&child, group);
    }
    if let Err(error) = cgroup.assign_pid(i64::from(pid)) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(map_cgroup_error(error));
    }
    let input = InputDelivery::start(&mut child, invocation, group)?;
    let mut managed = ManagedChild {
        child,
        pid,
        pgid: group.pgid,
        target: invocation.target.clone(),
        argv: invocation.argv.clone(),
        cwd: invocation.cwd.clone(),
        env_overlay: invocation.env_overlay.clone(),
        deadline: invocation.timeout.map(|timeout| Instant::now() + timeout),
        detached: options.spawn.detach || options.spawn.new_session,
        consumed: false,
        cgroup,
        input,
        input_error: None,
        accepted_exit_codes: invocation.accepted_exit_codes,
    };
    drive_managed_input(&mut managed)?;
    Ok(managed)
}

pub(crate) fn drive_managed_input(child: &mut ManagedChild) -> Result<(), RunError> {
    if let Some(error) = &child.input_error { return Err(error.clone()); }
    if let Err(error) = feed_input(&mut child.input) {
        child.process_group().kill(); let _ = child.child.wait(); child.consumed = true;
        child.input = None; child.input_error = Some(error.clone()); return Err(error);
    }
    Ok(())
}

/// An owned stdout cursor keeps input delivery, cancellation, and reaping with
/// the same child while consumers suspend between pulls.
pub(crate) struct ProcessStream {
    child: ManagedChild,
    stdout: Option<std::process::ChildStdout>,
    status: Option<ProcessStatus>,
    cancellation: Option<Cancellation>,
    captured: usize,
    _foreground: Option<ForegroundTerminal>,
}

impl ProcessStream {
    pub(crate) fn start(invocation: &ProcessInvocation) -> Result<Self, RunError> {
        let mut options = SpawnManagedOptions::inherited_process_group();
        options.stdout = ManagedStdio::Piped;
        let mut child = spawn_managed(invocation, options)?;
        let stdout = child.child.stdout.take();
        if let Some(stdout) = &stdout {
            if let Err(error) = set_nonblocking(stdout.as_fd()) {
                child.process_group().kill(); let _ = child.child.wait(); child.consumed = true;
                return Err(error);
            }
        }
        let foreground = ForegroundTerminal::take(child.process_group());
        Ok(Self { child, stdout, status: None, cancellation: None, captured: 0, _foreground: foreground })
    }

    pub(crate) fn next(&mut self, policy: &mut dyn CancellationPolicy) -> Result<Option<Vec<u8>>, RunError> {
        loop {
            drive_managed_input(&mut self.child)?;
            if let Some(stdout) = &self.stdout {
                let mut bytes = vec![0; 8192];
                match rio::read(stdout.as_fd(), &mut bytes) {
                    Ok(0) => self.stdout = None,
                    Ok(count) => {
                        self.captured += count;
                        if self.captured > CAPTURE_LIMIT { return Err(RunError::new("capture-limit", "streamed stdout exceeded the capture limit")); }
                        bytes.truncate(count); return Ok(Some(bytes));
                    }
                    Err(rustix::io::Errno::AGAIN | rustix::io::Errno::INTR) => {},
                    Err(error) => return Err(RunError::new("io", error.to_string())),
                }
            }
            if self.status.is_none() {
                match poll_managed(&mut self.child)? {
                    ChildWaitOutcome::Exited(status) | ChildWaitOutcome::Signaled(status) => {
                        self.child.consumed = true; self.status = Some(status);
                        policy.process_group_finished(self.child.process_group());
                    }
                    _ => {},
                }
            }
            if self.status.is_some() && self.stdout.is_none() {
                let status = self.status.as_ref().expect("completed stream child has status");
                if let Some(cancellation) = &self.cancellation { return Err(cancellation.error(Some(status.clone()))); }
                if let Some(error) = completion_error(status, &[self.child.accepted_exit_codes], false) { return Err(error); }
                return Ok(None);
            }
            if timeout_elapsed(self.child.deadline) {
                self.child.process_group().kill(); let status = self.child.child.wait().map_err(map_wait_error)?;
                self.child.consumed = true;
                let status = process_status(status, 0, &self.child.target, Some(self.child.pid)); self.status = Some(status.clone());
                return Err(RunError::new("timeout", "process timed out").with_status(status));
            }
            check_cancellation(self.child.process_group(), &mut self.cancellation, policy);
            std::thread::sleep(WAIT_POLL);
        }
    }

    pub(crate) fn process_group(&self) -> ProcessGroup { self.child.process_group() }

    pub(crate) fn end(&self, error: Option<RunError>) -> ProcessEnd {
        ProcessEnd { pid: Some(self.child.pid), status: self.status.clone().or_else(|| error.as_ref().and_then(|error| error.status.as_deref().cloned())), error }
    }

    pub(crate) fn cancel(&mut self) {
        // Descendants can retain stdout after the direct child has been reaped.
        // Closing that cursor must still terminate its owned process group.
        if self.stdout.is_some() || !self.child.consumed {
            self.child.process_group().kill();
        }
        if !self.child.consumed {
            if let Ok(status) = self.child.child.wait() { self.status = Some(process_status(status, 0, &self.child.target, Some(self.child.pid))); }
            self.child.consumed = true;
        }
        self.stdout = None; self.child.input = None;
    }
}

impl Drop for ProcessStream {
    fn drop(&mut self) { self.cancel(); }
}

#[allow(dead_code)]
pub fn poll_managed(child: &mut ManagedChild) -> Result<ChildWaitOutcome, RunError> {
    waitpid_managed(child, WaitMode::Nonblocking)
}

pub fn wait_managed(
    child: &mut ManagedChild,
    mode: WaitMode,
    policy: &mut dyn CancellationPolicy,
) -> Result<(ChildWaitOutcome, Option<Cancellation>), RunError> {
    let mut cancellation = None;
    loop {
        let outcome = waitpid_managed(child, mode)?;
        match outcome {
            ChildWaitOutcome::StillRunning => {}
            ChildWaitOutcome::Stopped { .. } if mode == WaitMode::Script => {}
            outcome => {
                if matches!(
                    outcome,
                    ChildWaitOutcome::Exited(_) | ChildWaitOutcome::Signaled(_)
                ) {
                    policy.process_group_finished(child.process_group());
                    child.consumed = true;
                }
                if cancellation.is_none()
                    && let ChildWaitOutcome::Exited(status) | ChildWaitOutcome::Signaled(status) = &outcome
                    && let Some(error) = child.completion_error(status)
                {
                    return Err(error);
                }
                return Ok((outcome, cancellation));
            }
        }
        if timeout_elapsed(child.deadline) {
            let group = child.process_group();
            group.kill();
            let outcome = waitpid_blocking_until_exit(child)?;
            child.consumed = true;
            let status = match outcome {
                ChildWaitOutcome::Exited(status) | ChildWaitOutcome::Signaled(status) => status,
                ChildWaitOutcome::Stopped { .. } | ChildWaitOutcome::StillRunning => {
                    ProcessStatus::signaled(libc::SIGKILL)
                }
            };
            return Err(RunError::new("timeout", "process timed out").with_status(status));
        }
        check_cancellation(child.process_group(), &mut cancellation, policy);
        std::thread::sleep(WAIT_POLL);
    }
}

#[allow(dead_code)]
pub fn cancel_managed(
    mut child: ManagedChild,
    signal: i32,
    kill_after: Duration,
) -> Result<Option<ProcessStatus>, RunError> {
    let group = child.process_group();
    group.signal(signal);
    let started = Instant::now();
    let mut killed = false;
    loop {
        match poll_managed(&mut child)? {
            ChildWaitOutcome::Exited(status) | ChildWaitOutcome::Signaled(status) => {
                if killed {
                    group.kill();
                }
                child.consumed = true;
                return Ok(Some(status));
            }
            ChildWaitOutcome::Stopped { .. } | ChildWaitOutcome::StillRunning => {}
        }
        if !killed && started.elapsed() >= kill_after {
            group.kill();
            killed = true;
        }
        std::thread::sleep(WAIT_POLL);
    }
}

pub fn release_to_reaper(mut child: ManagedChild) {
    std::thread::spawn(move || {
        let _ = waitpid_blocking_until_exit(&mut child);
    });
}

struct StartedChild {
    input: Option<InputDelivery>,
    index: usize,
    target: Vec<u8>,
    pid: Option<u32>,
    child: std::process::Child,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessGroup {
    pgid: libc::pid_t,
}

impl ProcessGroup {
    fn from_child(child: &std::process::Child) -> Self {
        Self {
            pgid: child.id() as libc::pid_t,
        }
    }

    pub fn from_pgid(pgid: libc::pid_t) -> Self {
        Self { pgid }
    }

    pub fn signal(self, signal: i32) {
        signal_process_group(self, signal);
    }

    pub fn kill(self) {
        signal_process_group(self, libc::SIGKILL);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessGroupConfig {
    Inherit,
    NewRoot,
    Join(libc::pid_t),
}

impl ProcessGroupConfig {
    fn parent_sets_group(self) -> bool {
        matches!(self, Self::NewRoot | Self::Join(_))
    }
}

pub struct ForegroundTerminal {
    fd: RawFd,
    previous: libc::pid_t,
}

impl ForegroundTerminal {
    pub fn take(group: ProcessGroup) -> Option<Self> {
        let fd = libc::STDIN_FILENO;
        if !termios::isatty(stdio::stdin()) {
            return None;
        }
        let previous = termios::tcgetpgrp(stdio::stdin()).ok()?.as_raw_pid();
        if previous == group.pgid {
            return None;
        }
        if tcsetpgrp_ignoring_ttou(fd, group.pgid).is_ok() {
            Some(Self { fd, previous })
        } else {
            None
        }
    }
}

impl Drop for ForegroundTerminal {
    fn drop(&mut self) {
        let _ = tcsetpgrp_ignoring_ttou(self.fd, self.previous);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Cancellation {
    signal: i32,
    started_at: Instant,
    killed: bool,
}

impl Cancellation {
    fn new(signal: i32, group: ProcessGroup) -> Self {
        group.signal(signal);
        Self {
            signal,
            started_at: Instant::now(),
            killed: false,
        }
    }

    fn tick(&mut self, group: ProcessGroup) {
        if !self.killed && self.started_at.elapsed() >= CANCELLATION_GRACE {
            group.kill();
            self.killed = true;
        }
    }

    pub fn error(self, status: Option<ProcessStatus>) -> RunError {
        RunError::canceled(self.signal, status)
    }
}

fn wait_children(
    children: &mut [StartedChild],
    group: ProcessGroup,
    segment_statuses: &mut [Option<ProcessSegmentStatus>],
    deadline: Option<Instant>,
    policy: &mut dyn CancellationPolicy,
) -> Result<Option<Cancellation>, RunError> {
    let mut cancellation = None;
    let mut remaining = children.len();
    while remaining > 0 {
        for started in children.iter_mut() {
            if segment_statuses[started.index].is_some() {
                continue;
            }
            if let Err(error) = feed_input(&mut started.input) {
                group.kill();
                for started in children.iter_mut() { let _ = started.child.wait(); }
                return Err(error);
            }
            if let Some(status) = started.child.try_wait().map_err(map_wait_error)? {
                started.input = None;
                segment_statuses[started.index] = Some(process_segment_status(
                    status,
                    started.index,
                    &started.target,
                    started.pid,
                ));
                remaining -= 1;
            }
        }
        if remaining == 0 {
            break;
        }
        if timeout_elapsed(deadline) {
            group.kill();
            for started in children.iter_mut() {
                if segment_statuses[started.index].is_none() {
                    let status = started.child.wait().map_err(map_wait_error)?;
                    segment_statuses[started.index] = Some(process_segment_status(
                        status,
                        started.index,
                        &started.target,
                        started.pid,
                    ));
                }
            }
            let status =
                ProcessStatus::from_segments(segment_statuses.iter().flatten().cloned().collect());
            return Err(RunError::new("timeout", "process pipeline timed out").with_status(status));
        }
        check_cancellation(group, &mut cancellation, policy);
        std::thread::sleep(WAIT_POLL);
    }
    policy.process_group_finished(group);
    Ok(cancellation)
}

fn waitpid_managed(child: &mut ManagedChild, mode: WaitMode) -> Result<ChildWaitOutcome, RunError> {
    drive_managed_input(child)?;
    if child.consumed {
        return Ok(ChildWaitOutcome::StillRunning);
    }
    let flags = match mode {
        WaitMode::Script => rprocess::WaitOptions::NOHANG,
        WaitMode::InteractiveForeground if child.input.is_some() => rprocess::WaitOptions::NOHANG | rprocess::WaitOptions::UNTRACED,
        WaitMode::InteractiveForeground => rprocess::WaitOptions::UNTRACED,
        WaitMode::Nonblocking => rprocess::WaitOptions::NOHANG | rprocess::WaitOptions::UNTRACED,
    };
    loop {
        let pid = child.child.id();
        let Some(pid) = rprocess::Pid::from_raw(pid as i32) else {
            return Err(RunError::new("wait", "child pid is out of range"));
        };
        match rprocess::waitpid(Some(pid), flags) {
            Ok(None) => return Ok(ChildWaitOutcome::StillRunning),
            Ok(Some((waited, status))) if waited == pid => {
                let outcome =
                    child_wait_outcome_from_raw(status.as_raw(), 0, &child.target, Some(child.pid));
                if matches!(
                    outcome,
                    ChildWaitOutcome::Exited(_) | ChildWaitOutcome::Signaled(_)
                ) {
                    child.consumed = true;
                    child.input = None;
                }
                return Ok(outcome);
            }
            Ok(Some((_waited, _status))) => {}
            Err(error) => {
                let error = io::Error::from(error);
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(map_wait_error(error));
            }
        }
    }
}

fn waitpid_blocking_until_exit(child: &mut ManagedChild) -> Result<ChildWaitOutcome, RunError> {
    loop {
        let mode = if child.input.is_some() { WaitMode::Script } else { WaitMode::InteractiveForeground };
        match waitpid_managed(child, mode)? {
            ChildWaitOutcome::Stopped { .. } | ChildWaitOutcome::StillRunning => { std::thread::sleep(WAIT_POLL); continue; },
            outcome => return Ok(outcome),
        }
    }
}

// POSIX wait-status decode. rustix exposes these as `WaitStatus` methods, but
// its only constructor is crate-private, so this raw-int path (exercised by the
// unit tests below, which synthesise statuses) reimplements the small macros.
const fn wifexited(status: i32) -> bool {
    status & 0x7f == 0
}
const fn wexitstatus(status: i32) -> i32 {
    (status >> 8) & 0xff
}
const fn wifsignaled(status: i32) -> bool {
    (((status & 0x7f) + 1) as i8 >> 1) > 0
}
const fn wtermsig(status: i32) -> i32 {
    status & 0x7f
}
const fn wifstopped(status: i32) -> bool {
    status & 0xff == 0x7f
}
const fn wstopsig(status: i32) -> i32 {
    (status >> 8) & 0xff
}

fn child_wait_outcome_from_raw(
    status: libc::c_int,
    index: usize,
    target: &[u8],
    pid: Option<u32>,
) -> ChildWaitOutcome {
    if wifexited(status) {
        return ChildWaitOutcome::Exited(ProcessStatus::from_segments(vec![
            ProcessSegmentStatus {
                index,
                target: target.to_vec(),
                pid,
                success: wexitstatus(status) == 0,
                kind: ProcessSegmentStatusKind::Exit,
                code: Some(wexitstatus(status)),
                error_kind: None,
                error_message: None,
            },
        ]));
    }
    if wifsignaled(status) {
        return ChildWaitOutcome::Signaled(ProcessStatus::from_segments(vec![
            ProcessSegmentStatus {
                index,
                target: target.to_vec(),
                pid,
                success: false,
                kind: ProcessSegmentStatusKind::Signal,
                code: Some(wtermsig(status)),
                error_kind: None,
                error_message: None,
            },
        ]));
    }
    if wifstopped(status) {
        return ChildWaitOutcome::Stopped {
            signal: wstopsig(status),
        };
    }
    ChildWaitOutcome::Signaled(ProcessStatus::from_segments(vec![ProcessSegmentStatus {
        index,
        target: target.to_vec(),
        pid,
        success: false,
        kind: ProcessSegmentStatusKind::Signal,
        code: None,
        error_kind: None,
        error_message: None,
    }]))
}

fn timeout_elapsed(deadline: Option<Instant>) -> bool {
    deadline.is_some_and(|deadline| Instant::now() >= deadline)
}

fn check_cancellation(
    group: ProcessGroup,
    cancellation: &mut Option<Cancellation>,
    policy: &mut dyn CancellationPolicy,
) {
    match policy.check_process_group(group) {
        CancellationDecision::Continue => {}
        CancellationDecision::Forward(signal) => {
            if cancellation.is_none() {
                *cancellation = Some(Cancellation::new(signal, group));
            }
        }
        CancellationDecision::Escalate(signal) => {
            group.kill();
            if cancellation.is_none() {
                *cancellation = Some(Cancellation {
                    signal,
                    started_at: Instant::now(),
                    killed: true,
                });
            } else if let Some(cancellation) = cancellation {
                cancellation.killed = true;
            }
        }
    }
    if let Some(cancellation) = cancellation {
        cancellation.tick(group);
    }
}

fn configure_process_group(command: &mut Command, group: ProcessGroupConfig) {
    unsafe {
        command.pre_exec(move || {
            reset_child_signal_handlers();
            match group {
                ProcessGroupConfig::Inherit => Ok(()),
                ProcessGroupConfig::NewRoot => {
                    rprocess::setpgid(None, None).map_err(io::Error::from)
                }
                ProcessGroupConfig::Join(pgid) => {
                    rprocess::setpgid(None, rprocess::Pid::from_raw(pgid)).map_err(io::Error::from)
                }
            }
        });
    }
}

fn assign_child_to_process_group(child: &std::process::Child, group: ProcessGroup) {
    if let Err(err) = rprocess::setpgid(
        rprocess::Pid::from_raw(child.id() as i32),
        rprocess::Pid::from_raw(group.pgid),
    ) {
        let error = io::Error::from(err);
        if !is_benign_setpgid_race(&error) {
            let _ = error;
        }
    }
}

fn is_benign_setpgid_race(error: &io::Error) -> bool {
    matches!(
        error.raw_os_error(),
        Some(libc::EACCES | libc::EPERM | libc::ESRCH)
    )
}

fn signal_process_group(group: ProcessGroup, signal: i32) {
    let Some(pgid) = rprocess::Pid::from_raw(group.pgid) else {
        return;
    };
    let Some(signal) = signal_from_i32(signal) else {
        return;
    };
    if let Err(error) = rprocess::kill_process_group(pgid, signal) {
        let error = io::Error::from(error);
        if !matches!(error.raw_os_error(), Some(libc::ESRCH)) {
            let _ = error;
        }
    }
}

fn tcsetpgrp_ignoring_ttou(fd: RawFd, pgid: libc::pid_t) -> io::Result<()> {
    let mut set = unsafe { std::mem::zeroed::<libc::sigset_t>() };
    let mut previous = unsafe { std::mem::zeroed::<libc::sigset_t>() };
    let masked = unsafe {
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, libc::SIGTTOU);
        libc::pthread_sigmask(libc::SIG_BLOCK, &set, &mut previous) == 0
    };
    let result = rprocess::Pid::from_raw(pgid)
        .ok_or_else(|| io::Error::from_raw_os_error(libc::EINVAL))
        .and_then(|pid| {
            let fd = unsafe { BorrowedFd::borrow_raw(fd) };
            termios::tcsetpgrp(fd, pid).map_err(io::Error::from)
        });
    if masked {
        unsafe {
            libc::pthread_sigmask(libc::SIG_SETMASK, &previous, std::ptr::null_mut());
        }
    }
    result
}

fn install_signal_handler(signal: i32, handler: usize) -> io::Result<libc::sigaction> {
    let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
    action.sa_sigaction = handler;
    action.sa_flags = 0;
    let mut old: libc::sigaction = unsafe { std::mem::zeroed() };
    unsafe {
        libc::sigemptyset(&mut action.sa_mask);
        if libc::sigaction(signal, &action, &mut old) == 0 {
            Ok(old)
        } else {
            Err(io::Error::last_os_error())
        }
    }
}

fn restore_signal_handler(signal: i32, old: &libc::sigaction) {
    unsafe {
        let _ = libc::sigaction(signal, old, std::ptr::null_mut());
    }
}

fn reset_child_signal_handlers() {
    unsafe {
        libc::signal(libc::SIGHUP, libc::SIG_DFL);
        libc::signal(libc::SIGINT, libc::SIG_DFL);
        libc::signal(libc::SIGQUIT, libc::SIG_DFL);
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
        libc::signal(libc::SIGTERM, libc::SIG_DFL);
        libc::signal(libc::SIGTSTP, libc::SIG_DFL);
        libc::signal(libc::SIGTTIN, libc::SIG_DFL);
        libc::signal(libc::SIGTTOU, libc::SIG_DFL);
        libc::signal(libc::SIGUSR1, libc::SIG_DFL);
        libc::signal(libc::SIGUSR2, libc::SIG_DFL);
        libc::signal(libc::SIGALRM, libc::SIG_DFL);
        libc::signal(libc::SIGXCPU, libc::SIG_DFL);
        libc::signal(libc::SIGXFSZ, libc::SIG_DFL);
    }
}

fn command_with_stdio(
    invocation: &ProcessInvocation,
    executable: PathBuf,
    stdin: Stdio,
    stdout: Stdio,
    stderr: Stdio,
    group: ProcessGroupConfig,
) -> Result<Command, RunError> {
    let mut command = Command::new(executable);
    command
        .args(
            invocation
                .argv
                .iter()
                .map(|arg| OsString::from_vec(arg.clone())),
        )
        .current_dir(&invocation.cwd)
        .stdin(stdin)
        .stdout(stdout)
        .stderr(stderr);
    command.env_clear();
    for (name, value) in &invocation.env {
        command.env(os_string_from_bytes(name), os_string_from_bytes(value));
    }
    configure_process_group(&mut command, group);
    apply_redirections(&mut command, &invocation.redirections)?;
    Ok(command)
}

fn command_with_managed_stdio(
    invocation: &ProcessInvocation,
    executable: PathBuf,
    options: SpawnManagedOptions,
) -> Result<Command, RunError> {
    let mut command = Command::new(executable);
    command
        .args(
            invocation
                .argv
                .iter()
                .map(|arg| OsString::from_vec(arg.clone())),
        )
        .current_dir(&invocation.cwd)
        .stdin(managed_stdio(options.stdin))
        .stdout(managed_stdio(options.stdout))
        .stderr(managed_stdio(options.stderr));
    command.env_clear();
    for (name, value) in &invocation.env {
        command.env(os_string_from_bytes(name), os_string_from_bytes(value));
    }
    configure_managed_child(&mut command, options);
    if options.apply_redirections {
        apply_redirections(&mut command, &invocation.redirections)?;
    } else if invocation.redirections.iter().any(|item| matches!(item, ProcessRedirection::Input { .. })) {
        validate_input_sources(&invocation.redirections)?;
        command.stdin(Stdio::piped());
    }
    Ok(command)
}

fn managed_stdio(stdio: ManagedStdio) -> Stdio {
    match stdio {
        ManagedStdio::Inherit => Stdio::inherit(),
        ManagedStdio::Null => Stdio::null(),
        ManagedStdio::Piped => Stdio::piped(),
    }
}

thread_local! {
    #[allow(clippy::type_complexity)]
    static EXECUTABLE_CACHE: RefCell<FxHashMap<(Vec<u8>, Vec<u8>), PathBuf>> =
        RefCell::new(FxHashMap::default());
}

pub fn resolve_executable(invocation: &ProcessInvocation) -> Result<PathBuf, RunError> {
    if invocation.target.contains(&b'/') {
        let path = path_from_target(&invocation.target);
        let executable = if path.is_absolute() {
            path
        } else {
            invocation.cwd.join(path)
        };
        validate_executable(&executable)?;
        return Ok(executable);
    }

    let path_bytes = invocation.env.get(b"PATH".as_slice());
    let cache_key = (
        path_bytes.map(Vec::as_slice).unwrap_or(&[]).to_vec(),
        invocation.target.clone(),
    );
    if let Some(cached) = EXECUTABLE_CACHE.with(|c| c.borrow().get(&cache_key).cloned()) {
        return Ok(cached);
    }

    let mut found_non_executable = false;
    if let Some(path_env) = path_bytes {
        let path_env = os_string_from_bytes(path_env);
        for dir in std::env::split_paths(&path_env) {
            let base = if dir.as_os_str().is_empty() {
                invocation.cwd.clone()
            } else if dir.is_absolute() {
                dir
            } else {
                invocation.cwd.join(dir)
            };
            let candidate = base.join(os_string_from_bytes(&invocation.target));
            match validate_executable(&candidate) {
                Ok(()) => {
                    EXECUTABLE_CACHE.with(|c| {
                        c.borrow_mut().insert(cache_key, candidate.clone());
                    });
                    return Ok(candidate);
                }
                Err(error) if error.kind == "not-executable" => found_non_executable = true,
                Err(error) if error.kind == "permission-denied" => found_non_executable = true,
                Err(_) => {}
            }
        }
    }

    if found_non_executable {
        Err(RunError::new(
            "not-executable",
            "executable was found but is not executable",
        ))
    } else {
        Err(RunError::new("not-found", "executable not found"))
    }
}

fn configure_managed_child(command: &mut Command, options: SpawnManagedOptions) {
    unsafe {
        command.pre_exec(move || {
            if options.reset_signals {
                reset_child_signal_handlers();
            }
            if options.spawn.ignore_hup {
                libc::signal(libc::SIGHUP, libc::SIG_IGN);
            }
            if options.spawn.new_session {
                rprocess::setsid().map_err(io::Error::from)?;
            } else {
                match options.group {
                    ProcessGroupConfig::Inherit if options.spawn.detach => {
                        rprocess::setpgid(None, None).map_err(io::Error::from)?;
                    }
                    ProcessGroupConfig::Inherit => {}
                    ProcessGroupConfig::NewRoot => {
                        rprocess::setpgid(None, None).map_err(io::Error::from)?;
                    }
                    ProcessGroupConfig::Join(pgid) => {
                        rprocess::setpgid(None, rprocess::Pid::from_raw(pgid))
                            .map_err(io::Error::from)?;
                    }
                }
            }
            Ok(())
        });
    }
}

fn validate_executable(path: &Path) -> Result<(), RunError> {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(RunError::new("not-found", "executable not found"));
        }
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
            return Err(RunError::new(
                "permission-denied",
                "permission denied resolving executable",
            ));
        }
        Err(error) => return Err(map_spawn_error(error)),
    };
    if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
        return Err(RunError::new(
            "not-executable",
            "path is not an executable file",
        ));
    }
    Ok(())
}

fn process_status(
    status: std::process::ExitStatus,
    index: usize,
    target: &[u8],
    pid: Option<u32>,
) -> ProcessStatus {
    ProcessStatus::from_segments(vec![process_segment_status(status, index, target, pid)])
}

fn process_segment_status(
    status: std::process::ExitStatus,
    index: usize,
    target: &[u8],
    pid: Option<u32>,
) -> ProcessSegmentStatus {
    if let Some(code) = status.code() {
        ProcessSegmentStatus {
            index,
            target: target.to_vec(),
            pid,
            success: code == 0,
            kind: ProcessSegmentStatusKind::Exit,
            code: Some(code),
            error_kind: None,
            error_message: None,
        }
    } else if let Some(signal) = status.signal() {
        ProcessSegmentStatus {
            index,
            target: target.to_vec(),
            pid,
            success: false,
            kind: ProcessSegmentStatusKind::Signal,
            code: Some(signal),
            error_kind: None,
            error_message: None,
        }
    } else {
        ProcessSegmentStatus {
            index,
            target: target.to_vec(),
            pid,
            success: false,
            kind: ProcessSegmentStatusKind::Signal,
            code: None,
            error_kind: None,
            error_message: None,
        }
    }
}

fn process_end_from_exec_failure(index: usize, target: &[u8], error: RunError) -> ProcessEnd {
    ProcessEnd {
        pid: None,
        status: Some(ProcessStatus::from_segments(vec![exec_failure_segment(
            index, target, error,
        )])),
        error: None,
    }
}

fn exec_failure_segment(index: usize, target: &[u8], error: RunError) -> ProcessSegmentStatus {
    ProcessSegmentStatus {
        index,
        target: target.to_vec(),
        pid: None,
        success: false,
        kind: ProcessSegmentStatusKind::Exec,
        code: None,
        error_kind: Some(error.kind),
        error_message: Some(error.message),
    }
}

fn validate_input_sources(redirections: &[ProcessRedirection]) -> Result<(), RunError> {
    if redirections.iter().any(|item| matches!(item, ProcessRedirection::Input { .. }))
        && redirections.iter().filter(|item| matches!(item, ProcessRedirection::Input { .. }
            | ProcessRedirection::File { stream: RedirectionStream::Stdin, .. }
            | ProcessRedirection::Dup { stream: RedirectionStream::Stdin, .. }
            | ProcessRedirection::ChildDup { stream: RedirectionStream::Stdin, .. })).count() != 1
    { return Err(RunError::new("redirection", "Bytes input cannot compete with another stdin source")); }
    Ok(())
}

fn apply_redirections(
    command: &mut Command,
    redirections: &[ProcessRedirection],
) -> Result<(), RunError> {
    validate_input_sources(redirections)?;
    // A `ChildDup` copies whatever its source descriptor is at that point in
    // the list, so it and everything after it must run in the child, in order,
    // on top of the streams the parent has already set up. Files are opened
    // here so a failure is still reported as a redirection error.
    let split = redirections
        .iter()
        .position(|redirection| matches!(redirection, ProcessRedirection::ChildDup { .. }))
        .unwrap_or(redirections.len());
    let (parent_side, child_side) = redirections.split_at(split);
    apply_parent_redirections(command, parent_side)?;
    for redirection in child_side {
        match redirection {
            ProcessRedirection::Input { .. } => { command.stdin(Stdio::piped()); }
            ProcessRedirection::File { stream, mode, path } => {
                let file = file_redirection_file(path, *mode)?;
                child_dup_owned(command, *stream, rustix::fd::OwnedFd::from(file));
            }
            ProcessRedirection::Dup { stream, fd } => {
                if *fd < 0 {
                    return Err(RunError::new(
                        "redirection",
                        "fd duplication target must be non-negative",
                    ));
                }
                let borrowed = unsafe { BorrowedFd::borrow_raw(*fd) };
                let owned = rio::fcntl_dupfd_cloexec(borrowed, 0)
                    .map_err(|error| map_redirection_error(error.into()))?;
                child_dup_owned(command, *stream, owned);
            }
            ProcessRedirection::ChildDup { stream, fd } => {
                child_dup(command, *stream, *fd)?;
            }
        }
    }
    Ok(())
}

fn apply_parent_redirections(
    command: &mut Command,
    redirections: &[ProcessRedirection],
) -> Result<(), RunError> {
    let mut index = 0;
    while index < redirections.len() {
        if let (
            ProcessRedirection::File {
                stream: first_stream,
                mode: first_mode,
                path: first_path,
            },
            Some(ProcessRedirection::File {
                stream: second_stream,
                mode: second_mode,
                path: second_path,
            }),
        ) = (&redirections[index], redirections.get(index + 1))
            && stdout_stderr_pair(*first_stream, *second_stream)
            && first_mode == second_mode
            && first_path == second_path
        {
            let file = file_redirection_file(first_path, *first_mode)?;
            let cloned = file.try_clone().map_err(map_redirection_error)?;
            apply_file_stdio(command, *first_stream, Stdio::from(file));
            apply_file_stdio(command, *second_stream, Stdio::from(cloned));
            index += 2;
            continue;
        }

        let redirection = &redirections[index];
        match redirection {
            ProcessRedirection::Input { .. } => { command.stdin(Stdio::piped()); }
            ProcessRedirection::File { stream, mode, path } => {
                let stdio = file_redirection(path, *mode)?;
                apply_file_stdio(command, *stream, stdio);
            }
            ProcessRedirection::Dup { stream, fd } => {
                let stdio = duplicate_fd(*fd)?;
                apply_file_stdio(command, *stream, stdio);
            }
            ProcessRedirection::ChildDup { .. } => {
                unreachable!("child-side redirections are applied by apply_redirections")
            }
        }
        index += 1;
    }
    Ok(())
}

fn stream_fd(stream: RedirectionStream) -> i32 {
    match stream {
        RedirectionStream::Stdin => libc::STDIN_FILENO,
        RedirectionStream::Stdout => libc::STDOUT_FILENO,
        RedirectionStream::Stderr => libc::STDERR_FILENO,
    }
}

/// Registers a `dup2(fd, stream)` to run in the child once its standard streams
/// are in place. Duplications run in the order they were registered.
fn child_dup(command: &mut Command, stream: RedirectionStream, fd: i32) -> Result<(), RunError> {
    use std::os::unix::process::CommandExt;
    if fd < 0 {
        return Err(RunError::new(
            "redirection",
            "fd duplication target must be non-negative",
        ));
    }
    // SAFETY: the closure only checks descriptor flags and duplicates stdio;
    // both descriptor operations are async-signal-safe.
    unsafe {
        command.pre_exec(move || duplicate_child_stdio(BorrowedFd::borrow_raw(fd), stream));
    }
    Ok(())
}

/// Like `child_dup`, for a descriptor the parent opened: the closure owns it
/// until the command is dropped, so it stays open across the spawn.
fn child_dup_owned(command: &mut Command, stream: RedirectionStream, source: rustix::fd::OwnedFd) {
    use std::os::fd::AsRawFd;
    use std::os::unix::process::CommandExt;
    let target = stream_fd(stream);
    // SAFETY: the closure only calls dup2 and fcntl, both async-signal-safe.
    unsafe {
        command.pre_exec(move || {
            let fd = source.as_raw_fd();
            if fd == target {
                // Already in place, but opened close-on-exec.
                let mut flags = rio::fcntl_getfd(&source)?;
                flags.remove(rio::FdFlags::CLOEXEC);
                rio::fcntl_setfd(&source, flags)?;
            } else {
                duplicate_child_stdio(source.as_fd(), stream)?;
            }
            Ok(())
        });
    }
}

fn duplicate_child_stdio(source: BorrowedFd<'_>, stream: RedirectionStream) -> io::Result<()> {
    // Validate even when source and target coincide: stdio's dup2 helpers
    // skip the syscall in that case, but a closed source must still fail.
    rio::fcntl_getfd(source)?;
    match stream {
        RedirectionStream::Stdin => stdio::dup2_stdin(source)?,
        RedirectionStream::Stdout => stdio::dup2_stdout(source)?,
        RedirectionStream::Stderr => stdio::dup2_stderr(source)?,
    }
    Ok(())
}

fn stdout_stderr_pair(first: RedirectionStream, second: RedirectionStream) -> bool {
    matches!(
        (first, second),
        (RedirectionStream::Stdout, RedirectionStream::Stderr)
            | (RedirectionStream::Stderr, RedirectionStream::Stdout)
    )
}

fn apply_file_stdio(command: &mut Command, stream: RedirectionStream, stdio: Stdio) {
    match stream {
        RedirectionStream::Stdin => {
            command.stdin(stdio);
        }
        RedirectionStream::Stdout => {
            command.stdout(stdio);
        }
        RedirectionStream::Stderr => {
            command.stderr(stdio);
        }
    }
}

fn file_redirection_file(
    path: &Path,
    mode: FileRedirectionMode,
) -> Result<std::fs::File, RunError> {
    let mut options = OpenOptions::new();
    match mode {
        FileRedirectionMode::Read => {
            options.read(true);
        }
        FileRedirectionMode::Write => {
            options.write(true).create(true).truncate(true);
        }
        FileRedirectionMode::Append => {
            options.write(true).create(true).append(true);
        }
    }
    options.open(path).map_err(map_redirection_error)
}

fn file_redirection(path: &Path, mode: FileRedirectionMode) -> Result<Stdio, RunError> {
    file_redirection_file(path, mode).map(Stdio::from)
}

fn duplicate_fd(fd: i32) -> Result<Stdio, RunError> {
    if fd < 0 {
        return Err(RunError::new(
            "redirection",
            "fd duplication target must be non-negative",
        ));
    }
    let borrowed = unsafe { BorrowedFd::borrow_raw(fd) };
    let duplicated = rio::fcntl_dupfd_cloexec(borrowed, 0)
        .map_err(|error| map_redirection_error(error.into()))?;
    Ok(Stdio::from(duplicated))
}

fn set_nonblocking(fd: BorrowedFd<'_>) -> Result<(), RunError> {
    let mut flags = rfs::fcntl_getfl(fd).map_err(|error| RunError::new("io", error.to_string()))?;
    flags.insert(rfs::OFlags::NONBLOCK);
    rfs::fcntl_setfl(fd, flags).map_err(|error| RunError::new("io", error.to_string()))
}

fn signal_from_i32(signal: i32) -> Option<rprocess::Signal> {
    NonZeroI32::new(signal)
        .map(|signal| unsafe { rprocess::Signal::from_raw_nonzero_unchecked(signal) })
}

fn path_from_target(target: &[u8]) -> PathBuf {
    PathBuf::from(os_string_from_bytes(target))
}

fn os_string_from_bytes(bytes: &[u8]) -> OsString {
    OsString::from_vec(bytes.to_vec())
}

fn map_spawn_error(error: io::Error) -> RunError {
    match error.raw_os_error() {
        Some(libc::ENOEXEC) => RunError::new("exec-format", "executable format error"),
        Some(libc::EACCES) => RunError::new("permission-denied", "permission denied"),
        Some(libc::ENOENT) => RunError::new("not-found", "executable not found"),
        _ => match error.kind() {
            io::ErrorKind::NotFound => RunError::new("not-found", "executable not found"),
            io::ErrorKind::PermissionDenied => {
                RunError::new("permission-denied", "permission denied")
            }
            _ => RunError::new("spawn", error.to_string()),
        },
    }
}

fn map_cgroup_error(error: CgroupError) -> RunError {
    RunError::new(error.kind, error.message)
}

fn setup_error_is_hard(error: &RunError) -> bool {
    matches!(
        error.kind.as_str(),
        "redirection" | "cgroup" | "unsupported-platform" | "io"
    )
}

fn map_wait_error(error: io::Error) -> RunError {
    RunError::new("io", error.to_string())
}

fn map_redirection_error(error: io::Error) -> RunError {
    RunError::new("redirection", error.to_string())
}

pub fn path_bytes(path: &Path) -> Vec<u8> {
    path.as_os_str().as_bytes().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_wait_status_distinguishes_exit_signal_and_stop() {
        let exited = child_wait_outcome_from_raw(7 << 8, 0, b"demo", Some(123));
        let ChildWaitOutcome::Exited(status) = exited else {
            panic!("expected exited outcome");
        };
        assert_eq!(status.kind, ProcessStatusKind::Exit);
        assert_eq!(status.code, Some(7));
        assert!(!status.success);
        assert_eq!(status.segments[0].target, b"demo");
        assert_eq!(status.segments[0].pid, Some(123));

        let signaled = child_wait_outcome_from_raw(libc::SIGTERM, 0, b"demo", Some(123));
        let ChildWaitOutcome::Signaled(status) = signaled else {
            panic!("expected signaled outcome");
        };
        assert_eq!(status.kind, ProcessStatusKind::Signal);
        assert_eq!(status.code, Some(libc::SIGTERM));
        assert!(!status.success);

        let stopped =
            child_wait_outcome_from_raw((libc::SIGTSTP << 8) | 0x7f, 0, b"demo", Some(123));
        assert_eq!(
            stopped,
            ChildWaitOutcome::Stopped {
                signal: libc::SIGTSTP
            }
        );
    }

    #[test]
    fn setpgid_race_filter_covers_exec_and_exit_races() {
        for code in [libc::EACCES, libc::EPERM, libc::ESRCH] {
            assert!(is_benign_setpgid_race(&io::Error::from_raw_os_error(code)));
        }
        assert!(!is_benign_setpgid_race(&io::Error::from_raw_os_error(
            libc::EINVAL
        )));
    }

    #[test]
    fn capture_allows_explicit_stdout_redirection() {
        let output_path = std::env::temp_dir().join(format!(
            "xsh-process-capture-redirection-{}",
            std::process::id()
        ));
        let env = std::env::vars_os()
            .map(|(name, value)| (name.as_bytes().to_vec(), value.as_bytes().to_vec()))
            .collect();
        let invocation = ProcessInvocation {
            target: b"sh".to_vec(),
            argv: vec![
                b"-c".to_vec(),
                b"printf redirected; printf captured >&2".to_vec(),
            ],
            cwd: std::env::current_dir().expect("current directory"),
            env,
            env_overlay: BTreeMap::new(),
            redirections: vec![ProcessRedirection::File {
                stream: RedirectionStream::Stdout,
                mode: FileRedirectionMode::Write,
                path: output_path.clone(),
            }],
            timeout: None,
            cpu_max: None,
            accepted_exit_codes: None,
        };

        let output = run_capture_with_stderr(&invocation).expect("capture redirected process");
        assert!(output.end.status.expect("process status").success);
        assert!(output.stdout.is_empty());
        assert_eq!(output.stderr, b"captured");
        assert_eq!(
            std::fs::read(&output_path).expect("read redirected output"),
            b"redirected"
        );
        std::fs::remove_file(output_path).expect("remove redirected output");
    }

    #[test]
    fn child_dup_follows_the_childs_own_redirections() {
        let output_path =
            std::env::temp_dir().join(format!("xsh-process-child-dup-{}", std::process::id()));
        let env = std::env::vars_os()
            .map(|(name, value)| (name.as_bytes().to_vec(), value.as_bytes().to_vec()))
            .collect();
        let dup_stderr_onto_stdout = ProcessRedirection::ChildDup {
            stream: RedirectionStream::Stderr,
            fd: 1,
        };
        let stdout_to_file = ProcessRedirection::File {
            stream: RedirectionStream::Stdout,
            mode: FileRedirectionMode::Write,
            path: output_path.clone(),
        };
        let run = |redirections: Vec<ProcessRedirection>| {
            let invocation = ProcessInvocation {
                target: b"sh".to_vec(),
                argv: vec![b"-c".to_vec(), b"printf out; printf err >&2".to_vec()],
                cwd: std::env::current_dir().expect("current directory"),
                env: BTreeMap::clone(&env),
                env_overlay: BTreeMap::new(),
                redirections,
                timeout: None,
                cpu_max: None,
            accepted_exit_codes: None,
            };
            run_capture_with_stderr(&invocation).expect("capture process")
        };

        // `> file 2>&1`: the duplicate sees the file, so both streams land in it.
        let output = run(vec![stdout_to_file.clone(), dup_stderr_onto_stdout.clone()]);
        assert!(output.stdout.is_empty() && output.stderr.is_empty());
        assert_eq!(std::fs::read(&output_path).expect("read output"), b"outerr");

        // `2>&1 > file`: the duplicate happened first, so stderr stays on the
        // original standard output.
        let output = run(vec![dup_stderr_onto_stdout, stdout_to_file]);
        assert_eq!(output.stdout, b"err");
        assert_eq!(std::fs::read(&output_path).expect("read output"), b"out");
        std::fs::remove_file(output_path).expect("remove output");
    }

    #[test]
    fn managed_cancel_reaps_process_group_child() {
        let env = std::env::vars_os()
            .map(|(name, value)| (name.as_bytes().to_vec(), value.as_bytes().to_vec()))
            .collect();
        let cwd = std::env::current_dir().expect("current directory");
        let invocation = ProcessInvocation {
            target: b"sh".to_vec(),
            argv: vec![b"-c".to_vec(), b"sleep 5".to_vec()],
            cwd: cwd.clone(),
            env,
            env_overlay: BTreeMap::new(),
            redirections: Vec::new(),
            timeout: None,
            cpu_max: None,
            accepted_exit_codes: None,
        };
        let options = SpawnManagedOptions {
            stdin: ManagedStdio::Null,
            stdout: ManagedStdio::Piped,
            stderr: ManagedStdio::Null,
            apply_redirections: false,
            group: ProcessGroupConfig::NewRoot,
            reset_signals: true,
            spawn: SpawnOptions::default(),
        };
        let child = spawn_managed(&invocation, options).expect("spawn managed child");

        assert_eq!(child.argv, invocation.argv);
        assert_eq!(child.cwd, cwd);
        assert!(child.env_overlay.is_empty());
        assert!(!child.detached);

        let status = cancel_managed(child, libc::SIGTERM, Duration::from_millis(10))
            .expect("cancel managed child")
            .expect("cancellation status");
        assert!(!status.success);
    }
    #[test]
    fn accepted_exit_codes_validate_bounds_and_keep_actual_status() {
        for invalid in [vec![], vec![-1], vec![256], vec![0, 0]] {
            assert!(AcceptedExitCodes::new(&invalid).is_err());
        }
        let accepted = AcceptedExitCodes::new(&[0, 1, 255]).unwrap();
        let status = ProcessStatus::exited(1);
        assert!(completion_error(&status, &[Some(accepted)], true).is_none());
        assert!(!status.success);
        assert_eq!(status.code, Some(1));
        let rejected_zero = completion_error(&ProcessStatus::exited(0), &[Some(AcceptedExitCodes::new(&[1]).unwrap())], false).unwrap();
        assert_eq!(rejected_zero.kind, "unexpected-exit");
        let signaled = ProcessStatus::signaled(libc::SIGTERM);
        let set = AcceptedExitCodes::new(&[0, 128 + i64::from(libc::SIGTERM)]).unwrap();
        assert_eq!(completion_error(&signaled, &[Some(set)], false).unwrap().kind, "signal");
    }

}
