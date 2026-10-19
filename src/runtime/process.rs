#![allow(clippy::single_call_fn)]

use crate::runtime::cgroup::{CgroupError, CgroupScope};
use crate::runtime::value::{ErrorContext, RunError, Value};
use rustc_hash::FxHashMap;
use rustix::{event as revent, fs as rfs, io as rio, process as rprocess, stdio, termios};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io;
use std::num::NonZeroI32;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd, RawFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, Weak};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::{Duration, Instant};

pub const CAPTURE_LIMIT: usize = 16 * 1024 * 1024;
const CANCELLATION_GRACE: Duration = Duration::from_millis(150);
pub(crate) const WAIT_POLL: Duration = Duration::from_millis(10);
static PRIMARY_SIGNAL: AtomicI32 = AtomicI32::new(0);
static ESCALATION_SIGNAL: AtomicI32 = AtomicI32::new(0);
static CHILD_STATUS_AUTHORITY: OnceLock<Mutex<ChildStatusAuthority>> = OnceLock::new();

/// Whether SIGPIPE is ignored as far as scripts and the processes they start
/// are concerned. The runtime itself always ignores SIGPIPE so that a closed
/// pipe surfaces as a write error, which makes the process's real disposition
/// useless for either purpose. This flag holds the disposition the process was
/// started with until a script sets another with `process.set_signal_action`.
static SIGPIPE_IGNORED: AtomicBool = AtomicBool::new(true);

// std sets SIGPIPE to SIG_IGN before `main`, so the disposition the process
// inherited can only be read from a constructor. Other platforms keep the
// initial `true`, which is what std leaves behind.
#[cfg(target_os = "linux")]
extern "C" fn record_startup_sigpipe_disposition() {
    let mut current: libc::sigaction = unsafe { std::mem::zeroed() };
    if unsafe { libc::sigaction(libc::SIGPIPE, std::ptr::null(), &mut current) } == 0 {
        SIGPIPE_IGNORED.store(current.sa_sigaction == libc::SIG_IGN, Ordering::Relaxed);
    }
}

#[cfg(target_os = "linux")]
#[used]
#[unsafe(link_section = ".init_array")]
static RECORD_STARTUP_SIGPIPE_DISPOSITION: extern "C" fn() = record_startup_sigpipe_disposition;

pub(crate) fn sigpipe_ignored() -> bool {
    SIGPIPE_IGNORED.load(Ordering::Relaxed)
}

pub(crate) fn set_sigpipe_ignored(ignored: bool) {
    SIGPIPE_IGNORED.store(ignored, Ordering::Relaxed);
}

/// Gives a child the SIGPIPE disposition scripts see. Async-signal-safe, so it
/// may run between fork and exec.
pub(crate) fn apply_sigpipe_disposition_to_child() {
    let disposition = if sigpipe_ignored() { libc::SIG_IGN } else { libc::SIG_DFL };
    unsafe {
        libc::signal(libc::SIGPIPE, disposition);
    }
}

pub fn install_cancellation_signal_handlers() -> io::Result<SignalHandlerGuard> {
    restore_default_sigchld()?;
    SignalHandlerGuard::install_many_preserving_ignored(&[libc::SIGINT, libc::SIGTERM])
}

/// An ignored SIGCHLD makes the kernel discard children's exit statuses, so
/// every wait fails with ECHILD and the runtime cannot observe the processes
/// it starts. A parent that ignores SIGCHLD (`trap '' CHLD`) is inherited
/// across exec, so the runtime takes back the default action, which also
/// keeps what its children inherit well defined.
fn restore_default_sigchld() -> io::Result<()> {
    let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
    action.sa_sigaction = libc::SIG_DFL;
    unsafe {
        libc::sigemptyset(&mut action.sa_mask);
    }
    set_sigchld_action(&action)
}

pub(crate) fn set_sigchld_action(action: &libc::sigaction) -> io::Result<()> {
    change_process_signal_action(libc::SIGCHLD, action, None)
}

fn begin_timed_child_launch(
) -> io::Result<MutexGuard<'static, ChildStatusAuthority>> {
    let mut authority = child_status_authority();
    authority.refresh_status_retention()?;
    if !authority.retains_status {
        return Err(io::Error::from_raw_os_error(libc::ECHILD));
    }
    Ok(authority)
}

fn signal_action_retains_child_status(action: &libc::sigaction) -> bool {
    action.sa_sigaction != libc::SIG_IGN && action.sa_flags & libc::SA_NOCLDWAIT == 0
}

fn child_status_authority() -> MutexGuard<'static, ChildStatusAuthority> {
    CHILD_STATUS_AUTHORITY
        .get_or_init(|| Mutex::new(ChildStatusAuthority::new()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub(crate) fn wait_any_child_nohang(
) -> io::Result<Option<(rprocess::Pid, rprocess::WaitStatus)>> {
    let mut authority = child_status_authority();
    authority.refresh_status_retention()?;
    let waited = rprocess::wait(rprocess::WaitOptions::NOHANG).map_err(io::Error::from)?;
    if let Some((pid, _)) = &waited {
        authority.invalidate_pid(pid.as_raw_pid() as u32);
    }
    Ok(waited)
}

struct ChildStatusAuthority {
    epoch: u64,
    retains_status: bool,
    identities: FxHashMap<u32, Weak<AtomicBool>>,
}

impl ChildStatusAuthority {
    fn new() -> Self {
        Self {
            epoch: 0,
            retains_status: true,
            identities: FxHashMap::default(),
        }
    }

    fn update_status_retention(&mut self, retains_status: bool) {
        if self.retains_status && !retains_status {
            self.epoch = self.epoch.wrapping_add(1);
            for identity in self.identities.values().filter_map(Weak::upgrade) {
                identity.store(false, Ordering::Release);
            }
            self.identities.clear();
        }
        self.retains_status = retains_status;
    }

    fn register(&mut self, pid: u32) -> TimedChildIdentity {
        self.identities.retain(|_, identity| identity.strong_count() != 0);
        self.invalidate_pid(pid);
        let valid = Arc::new(AtomicBool::new(true));
        self.identities.insert(pid, Arc::downgrade(&valid));
        TimedChildIdentity {
            pid,
            epoch: self.epoch,
            valid,
        }
    }

    fn is_valid(&self, identity: &TimedChildIdentity) -> bool {
        self.retains_status
            && self.epoch == identity.epoch
            && identity.valid.load(Ordering::Acquire)
    }

    fn refresh_status_retention(&mut self) -> io::Result<()> {
        let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
        if unsafe { libc::sigaction(libc::SIGCHLD, std::ptr::null(), &mut action) } != 0 {
            return Err(io::Error::last_os_error());
        }
        self.update_status_retention(signal_action_retains_child_status(&action));
        Ok(())
    }

    fn invalidate_pid(&mut self, pid: u32) {
        if let Some(identity) = self.identities.remove(&pid).and_then(|value| value.upgrade()) {
            identity.store(false, Ordering::Release);
        }
    }

    fn invalidate_identity(&mut self, identity: &TimedChildIdentity) {
        let matches = self
            .identities
            .get(&identity.pid)
            .and_then(Weak::upgrade)
            .is_some_and(|active| Arc::ptr_eq(&active, &identity.valid));
        if matches {
            self.identities.remove(&identity.pid);
        }
        identity.valid.store(false, Ordering::Release);
    }
}

#[derive(Clone)]
pub(crate) struct TimedChildIdentity {
    pid: u32,
    epoch: u64,
    valid: Arc<AtomicBool>,
}

impl TimedChildIdentity {
    fn signal_group_checked(&self, group: ProcessGroup, signal: i32) -> Result<bool, RunError> {
        let mut authority = child_status_authority();
        authority
            .refresh_status_retention()
            .map_err(map_wait_error)?;
        if !authority.is_valid(self) {
            return Ok(false);
        }
        signal_process_group_checked(group, signal)?;
        Ok(true)
    }

    fn signal_process_checked(&self, signal: i32) -> Result<bool, RunError> {
        let mut authority = child_status_authority();
        authority
            .refresh_status_retention()
            .map_err(map_wait_error)?;
        if !authority.is_valid(self) {
            return Ok(false);
        }
        signal_process_checked(self.pid, signal)?;
        Ok(true)
    }

    pub(crate) fn signal_group(&self, group: ProcessGroup, signal: i32) -> bool {
        let mut authority = child_status_authority();
        if authority.refresh_status_retention().is_err() {
            return false;
        }
        if !authority.is_valid(self) {
            return false;
        }
        group.signal(signal);
        true
    }

    fn is_valid(&self) -> bool {
        let mut authority = child_status_authority();
        authority.refresh_status_retention().is_ok() && authority.is_valid(self)
    }

    fn observe(&self) -> Result<ChildIdentityObservation, RunError> {
        let pid = rprocess::Pid::from_raw(self.pid as i32)
            .ok_or_else(|| RunError::new("io", "child pid is out of range"))?;
        let mut authority = child_status_authority();
        authority
            .refresh_status_retention()
            .map_err(map_wait_error)?;
        if !authority.is_valid(self) {
            return Ok(ChildIdentityObservation::Lost);
        }
        loop {
            match rprocess::waitid(
                rprocess::WaitId::Pid(pid),
                rprocess::WaitIdOptions::NOHANG
                    | rprocess::WaitIdOptions::EXITED
                    | rprocess::WaitIdOptions::NOWAIT,
            ) {
                Ok(Some(status)) => {
                    return Ok(if status.exited() || status.killed() || status.dumped() {
                        ChildIdentityObservation::Exited
                    } else {
                        ChildIdentityObservation::Running
                    });
                }
                Ok(None) => return Ok(ChildIdentityObservation::Running),
                Err(rustix::io::Errno::INTR) => {}
                Err(rustix::io::Errno::CHILD) => {
                    authority.invalidate_identity(self);
                    return Ok(ChildIdentityObservation::Lost);
                }
                Err(error) => return Err(map_wait_error(io::Error::from(error))),
            }
        }
    }

    fn waitpid(
        &self,
        flags: rprocess::WaitOptions,
    ) -> Result<Option<(rprocess::Pid, rprocess::WaitStatus)>, RunError> {
        let pid = rprocess::Pid::from_raw(self.pid as i32)
            .ok_or_else(|| RunError::new("io", "child pid is out of range"))?;
        let mut authority = child_status_authority();
        authority
            .refresh_status_retention()
            .map_err(map_wait_error)?;
        if !authority.is_valid(self) {
            return Err(map_wait_error(io::Error::from_raw_os_error(libc::ECHILD)));
        }
        loop {
            match rprocess::waitpid(Some(pid), flags) {
                Ok(None) => return Ok(None),
                Ok(Some((waited, status))) if waited == pid => {
                    let raw = status.as_raw();
                    if wifexited(raw) || wifsignaled(raw) {
                        authority.invalidate_identity(self);
                    }
                    return Ok(Some((waited, status)));
                }
                Ok(Some((_waited, _status))) => {}
                Err(rustix::io::Errno::INTR) => {}
                Err(rustix::io::Errno::CHILD) => {
                    authority.invalidate_identity(self);
                    return Err(map_wait_error(io::Error::from_raw_os_error(libc::ECHILD)));
                }
                Err(error) => return Err(map_wait_error(io::Error::from(error))),
            }
        }
    }
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

/// Clears prior signal state before the owner starts an independent cancellation epoch.
/// Call this before installing handlers or preparing a new run, never after a
/// handler may have recorded a signal for the active run.
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

    // Exec preserves ignored dispositions, so the CLI must not replace one
    // while installing handlers for cancellation signals.
    fn install_many_preserving_ignored(signals: &[i32]) -> io::Result<Self> {
        let mut previous = Vec::new();
        for signal in signals {
            let mut current: libc::sigaction = unsafe { std::mem::zeroed() };
            let result = unsafe { libc::sigaction(*signal, std::ptr::null(), &mut current) };
            if result != 0 {
                for (installed, old) in previous.into_iter().rev() {
                    restore_signal_handler(installed, &old);
                }
                return Err(io::Error::last_os_error());
            }
            if current.sa_sigaction == libc::SIG_IGN {
                continue;
            }
            match install_signal_handler(*signal, handle_cancellation_signal as *const () as usize) {
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

    /// Runs before a child that shares this process's stdout or stderr starts
    /// or is waited on, so output the caller buffered appears first.
    fn before_shared_stdio(&mut self) {}
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
        let summary =
            first_rejected(&segments, |segment| !segment.success).or_else(|| segments.last());
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
            return Err(RunError::new(
                "accept-policy",
                "accepted exit codes must be a nonempty List[Int]",
            ));
        }
        let mut bits = [0u64; 4];
        for code in codes {
            let code = u8::try_from(*code).map_err(|_| {
                RunError::new("accept-policy", "accepted exit codes must be in 0..255")
            })?;
            let slot = &mut bits[usize::from(code / 64)];
            let mask = 1u64 << (code % 64);
            if *slot & mask != 0 {
                return Err(RunError::new(
                    "accept-policy",
                    "accepted exit codes must not contain duplicates",
                ));
            }
            *slot |= mask;
        }
        Ok(Self(bits))
    }

    fn accepts(self, segment: &ProcessSegmentStatus) -> bool {
        segment.kind == ProcessSegmentStatusKind::Exit
            && segment
                .code
                .and_then(|code| u8::try_from(code).ok())
                .is_some_and(|code| self.0[usize::from(code / 64)] & (1u64 << (code % 64)) != 0)
    }
}

impl ProcessSegmentStatus {
    pub(crate) fn is_sigpipe(&self) -> bool {
        self.kind == ProcessSegmentStatusKind::Signal && self.code == Some(libc::SIGPIPE)
    }
}

/// Pipefail with the `cmd | head` exception: an upstream segment killed by
/// `SIGPIPE` is not a failure when some later segment exited and was accepted
/// (code 0, or a code in its own `--accept` list), because that later segment
/// chose to stop reading.
fn tolerate_upstream_sigpipe(
    segments: &mut [ProcessSegmentStatus],
    invocations: &[ProcessInvocation],
) {
    let downstream_accepted = |segment: &ProcessSegmentStatus| {
        segment.kind == ProcessSegmentStatusKind::Exit
            && invocations
                .get(segment.index)
                .and_then(|invocation| invocation.accepted_exit_codes)
                .map_or(segment.code == Some(0), |policy| policy.accepts(segment))
    };
    for position in 0..segments.len() {
        if segments[position].is_sigpipe()
            && segments[position + 1..].iter().any(downstream_accepted)
        {
            segments[position].success = true;
        }
    }
}

/// The segment a pipeline failure names: the first rejected segment that was
/// not merely killed by `SIGPIPE`, else the first rejected segment.
fn first_rejected(
    segments: &[ProcessSegmentStatus],
    rejected: impl Fn(&ProcessSegmentStatus) -> bool,
) -> Option<&ProcessSegmentStatus> {
    segments
        .iter()
        .find(|segment| rejected(segment) && !segment.is_sigpipe())
        .or_else(|| segments.iter().find(|segment| rejected(segment)))
}

pub(crate) fn failed_segment(status: &ProcessStatus) -> Option<&ProcessSegmentStatus> {
    first_rejected(&status.segments, |segment| !segment.success)
}

pub(crate) fn rejected_segment<'a>(
    status: &'a ProcessStatus,
    policies: &[Option<AcceptedExitCodes>],
    require_zero: bool,
) -> Option<&'a ProcessSegmentStatus> {
    let require_zero = require_zero || policies.len() > 1 && policies.iter().any(Option::is_some);
    first_rejected(&status.segments, |segment| {
        if segment.success && segment.is_sigpipe() {
            return false;
        }
        policies
            .get(segment.index)
            .copied()
            .flatten()
            .map_or(require_zero && !segment.success, |policy| {
                !policy.accepts(segment)
            })
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
        return Some(
            RunError::new(
                if status.segments.len() > 1 {
                    "pipeline-failure"
                } else {
                    "unexpected-exit"
                },
                format!(
                    "{} exited with unaccepted status {}",
                    crate::runtime::value::run_error_segment_label(
                        segment,
                        status.segments.len() > 1
                    ),
                    segment.code.unwrap_or_default()
                ),
            )
            .with_status(status.clone()),
        );
    }
    let (kind, message) =
        crate::runtime::value::run_error_segment_summary(segment, status.segments.len() > 1);
    Some(RunError::new(kind, message).with_status(status.clone()))
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
    /// Namespace changes the child makes before it executes the command.
    pub namespaces: Option<crate::runtime::namespace::NamespaceEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProcessRedirection {
    Input {
        bytes: Arc<[u8]>,
    },
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
    /// Keep the child in the caller's process group instead of giving it its
    /// own, so a terminal's foreground group still reaches it. Signals then
    /// address the child's pid alone: the group also holds the caller.
    pub same_group: bool,
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
    fn start(
        child: &mut std::process::Child,
        invocation: &ProcessInvocation,
        group: ProcessGroup,
    ) -> Result<Option<Self>, RunError> {
        match Self::start_stream(child, invocation) {
            Ok(input) => Ok(input),
            Err(error) => {
                group.kill();
                let _ = child.kill();
                let _ = child.wait();
                Err(error)
            }
        }
    }

    fn start_stream(
        child: &mut std::process::Child,
        invocation: &ProcessInvocation,
    ) -> Result<Option<Self>, RunError> {
        let Some(bytes) = invocation.redirections.iter().find_map(|item| match item {
            ProcessRedirection::Input { bytes } => Some(bytes.clone()),
            _ => None,
        }) else {
            return Ok(None);
        };
        let Some(writer) = child.stdin.take() else {
            return Err(RunError::new("io", "missing Bytes input pipe"));
        };
        set_nonblocking(writer.as_fd())?;
        Ok(Some(Self {
            writer: Some(writer),
            bytes,
            offset: 0,
        }))
    }

    fn pump(&mut self) -> Result<(), RunError> {
        let Some(writer) = self.writer.as_ref() else {
            return Ok(());
        };
        if self.offset == self.bytes.len() {
            self.writer = None;
            return Ok(());
        }
        let end = self.offset.saturating_add(65536).min(self.bytes.len());
        match rio::write(writer, &self.bytes[self.offset..end]) {
            Ok(0) => return Err(RunError::new("io", "stdin write made no progress")),
            Ok(count) => {
                self.offset += count;
                if self.offset == self.bytes.len() {
                    self.writer = None;
                }
            }
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
        if delivery.writer.is_none() {
            *input = None;
        }
    }
    Ok(())
}

#[allow(dead_code)]
pub struct ManagedChild {
    child: std::process::Child,
    pub pid: u32,
    group: ProcessGroup,
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
        self.group
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
    policy.before_shared_stdio();
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
    policy.before_shared_stdio();
    let mut started = start_pipeline(invocations, false, None)?;
    let cancellation = if let Some(group) = started.group {
        let _foreground = ForegroundTerminal::take(group);
        wait_children(
            &mut started.children,
            group,
            &mut started.segment_statuses,
            pipeline_deadline(invocations),
            policy,
        )?
    } else {
        None
    };
    let status = started.status(invocations);
    if let Some(cancellation) = cancellation {
        return Err(cancellation.error(Some(status)));
    }
    Ok(ProcessEnd {
        pid: None,
        status: Some(status),
        error: None,
    })
}

struct StartedPipeline {
    children: Vec<StartedChild>,
    segment_statuses: Vec<Option<ProcessSegmentStatus>>,
    group: Option<ProcessGroup>,
    /// The last segment's stdout when the pipeline captures it.
    stdout: Option<ChildStdout>,
    _cgroup: CgroupScope,
}

impl StartedPipeline {
    fn status(&self, invocations: &[ProcessInvocation]) -> ProcessStatus {
        let mut segments: Vec<_> = self.segment_statuses.iter().flatten().cloned().collect();
        tolerate_upstream_sigpipe(&mut segments, invocations);
        ProcessStatus::from_segments(segments)
    }
}

fn pipeline_deadline(invocations: &[ProcessInvocation]) -> Option<Instant> {
    invocations
        .iter()
        .filter_map(|invocation| invocation.timeout)
        .min()
        .map(|timeout| Instant::now() + timeout)
}

/// Spawns every segment into one process group, stopping at the first setup
/// failure (recorded as that segment's status). `stderr` is a shared pipe
/// every segment writes to, or `None` to inherit.
fn start_pipeline(
    invocations: &[ProcessInvocation],
    capture_stdout: bool,
    stderr: Option<&io::PipeWriter>,
) -> Result<StartedPipeline, RunError> {
    if invocations.iter().skip(1).any(|invocation| {
        invocation
            .redirections
            .iter()
            .any(|item| matches!(item, ProcessRedirection::Input { .. }))
    }) {
        return Err(RunError::new(
            "redirection",
            "Bytes input is only valid on the first byte pipeline segment",
        ));
    }
    let mut children: Vec<StartedChild> = Vec::new();
    let mut previous_stdout: Option<ChildStdout> = None;
    let mut last_stdout = None;
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
        let last = index + 1 == invocations.len();
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
        let stdout = if last && !capture_stdout {
            Stdio::inherit()
        } else {
            Stdio::piped()
        };
        let stderr_stdio = match stderr {
            Some(writer) => Stdio::from(writer.try_clone().map_err(map_spawn_error)?),
            None => Stdio::inherit(),
        };

        let group_config = process_group
            .map(|group: ProcessGroup| ProcessGroupConfig::Join(group.pgid))
            .unwrap_or(ProcessGroupConfig::NewRoot);
        let mut command = command_with_stdio(
            invocation,
            executable,
            stdin,
            stdout,
            stderr_stdio,
            group_config,
            (if index == 0 { 1 } else { 0 })
                | (if last && !capture_stdout { 2 } else { 0 })
                | (if stderr.is_none() { 4 } else { 0 }),
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
                if last {
                    last_stdout = child.stdout.take();
                } else {
                    previous_stdout = child.stdout.take();
                }
                let input = match InputDelivery::start(&mut child, invocation, group) {
                    Ok(input) => input,
                    Err(error) => {
                        group.kill();
                        for started in &mut children {
                            let _ = started.child.wait();
                        }
                        return Err(error);
                    }
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
    Ok(StartedPipeline {
        children,
        segment_statuses,
        group: process_group,
        stdout: last_stdout,
        _cgroup: cgroup,
    })
}

/// Runs a byte pipeline capturing the last segment's stdout and, when
/// `capture_stderr` is set, every segment's stderr into one buffer. Each
/// buffer has the single-command capture limit.
pub fn run_pipeline_capture_with_policy(
    invocations: &[ProcessInvocation],
    capture_stderr: bool,
    policy: &mut dyn CancellationPolicy,
) -> Result<ProcessOutput, RunError> {
    policy.before_shared_stdio();
    let (stderr_reader, stderr_writer) = if capture_stderr {
        let (reader, writer) = io::pipe().map_err(map_spawn_error)?;
        (Some(reader), Some(writer))
    } else {
        (None, None)
    };
    let mut started = start_pipeline(invocations, true, stderr_writer.as_ref())?;
    drop(stderr_writer);
    let stdout = started.stdout.take();
    let stdout_fd = stdout.as_ref().map(AsRawFd::as_raw_fd);
    let stderr_fd = stderr_reader.as_ref().map(AsRawFd::as_raw_fd);
    if let Some(stdout) = stdout.as_ref() {
        set_nonblocking(stdout.as_fd())?;
    }
    if let Some(stderr) = stderr_reader.as_ref() {
        set_nonblocking(stderr.as_fd())?;
    }
    let mut captured_stdout = Vec::new();
    let mut captured_stderr = Vec::new();
    let mut cancellation = None;
    let mut capture_limit_hit = false;
    let mut buf = [0u8; 8192];
    if let Some(group) = started.group {
        let _foreground = ForegroundTerminal::take(group);
        let deadline = pipeline_deadline(invocations);
        loop {
            let mut remaining = 0;
            for child in &mut started.children {
                if started.segment_statuses[child.index].is_some() {
                    continue;
                }
                if let Err(error) = feed_input(&mut child.input) {
                    group.kill();
                    for child in &mut started.children {
                        let _ = child.child.wait();
                    }
                    return Err(error);
                }
                match child.child.try_wait().map_err(map_wait_error)? {
                    Some(status) => {
                        child.input = None;
                        started.segment_statuses[child.index] = Some(process_segment_status(
                            status,
                            child.index,
                            &child.target,
                            child.pid,
                        ));
                    }
                    None => remaining += 1,
                }
            }
            if let Some(fd) = stdout_fd {
                drain_capture_fd(
                    fd,
                    &mut captured_stdout,
                    &mut capture_limit_hit,
                    group,
                    &mut buf,
                );
            }
            if let Some(fd) = stderr_fd {
                drain_capture_fd(
                    fd,
                    &mut captured_stderr,
                    &mut capture_limit_hit,
                    group,
                    &mut buf,
                );
            }
            if remaining == 0 {
                break;
            }
            if timeout_elapsed(deadline) {
                group.kill();
                for child in &mut started.children {
                    if started.segment_statuses[child.index].is_none() {
                        let status = child.child.wait().map_err(map_wait_error)?;
                        started.segment_statuses[child.index] = Some(process_segment_status(
                            status,
                            child.index,
                            &child.target,
                            child.pid,
                        ));
                    }
                }
                let status = started.status(invocations);
                return Err(
                    RunError::new("timeout", "process pipeline timed out").with_status(status)
                );
            }
            if !capture_limit_hit {
                check_cancellation(group, &mut cancellation, policy);
            }
            let mut pollfds = Vec::new();
            for fd in [stdout_fd, stderr_fd].into_iter().flatten() {
                let fd = unsafe { BorrowedFd::borrow_raw(fd) };
                pollfds.push(revent::PollFd::from_borrowed_fd(fd, revent::PollFlags::IN));
            }
            let timeout = revent::Timespec::try_from(WAIT_POLL).expect("WAIT_POLL fits Timespec");
            let _ = revent::poll(&mut pollfds, Some(&timeout));
        }
        policy.process_group_finished(group);
    }
    let status = started.status(invocations);
    if capture_limit_hit && cancellation.is_none() {
        return Err(RunError::new(
            "capture-limit",
            "captured pipeline output exceeded the capture limit",
        )
        .with_status(status));
    }
    if let Some(cancellation) = cancellation {
        return Err(cancellation.error(Some(status)));
    }
    let policies = invocations
        .iter()
        .map(|invocation| invocation.accepted_exit_codes)
        .collect::<Vec<_>>();
    if let Some(error) = completion_error(&status, &policies, false) {
        return Err(error);
    }
    Ok(ProcessOutput {
        end: ProcessEnd {
            pid: None,
            status: Some(status),
            error: None,
        },
        stdout: captured_stdout,
        stderr: captured_stderr,
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
    policy.before_shared_stdio();
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
        1 | if capture_stderr { 0 } else { 4 },
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
            group.kill();
            let _ = child.wait();
            return Err(error);
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
    if let Some(error) = completion_error(&status, &[invocation.accepted_exit_codes], false) {
        return Err(error);
    }
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
    spawn_managed_inner(invocation, options, false).map(|(child, _, _)| child)
}

fn spawn_stream_managed(
    invocation: &ProcessInvocation,
    options: SpawnManagedOptions,
) -> Result<(
    ManagedChild,
    Option<ProcessTimeoutWatch>,
    Option<TimedChildIdentity>,
), RunError> {
    spawn_managed_inner(invocation, options, true)
}

fn spawn_managed_inner(
    invocation: &ProcessInvocation,
    options: SpawnManagedOptions,
    watch_stream_timeout: bool,
) -> Result<(
    ManagedChild,
    Option<ProcessTimeoutWatch>,
    Option<TimedChildIdentity>,
), RunError> {
    if options.spawn.same_group && (options.spawn.detach || options.spawn.new_session) {
        return Err(RunError::new(
            "spawn-options",
            "same_group conflicts with detach and new_session, which leave the caller's group",
        ));
    }
    let executable = resolve_executable(invocation)?;
    let cgroup = CgroupScope::cpu_max(invocation.cpu_max, "xsh-spawn").map_err(map_cgroup_error)?;
    let mut command = command_with_managed_stdio(invocation, executable, options)?;
    let timed_stream = watch_stream_timeout && invocation.timeout.is_some();
    let mut launch_authority = if timed_stream {
        Some(begin_timed_child_launch().map_err(map_wait_error)?)
    } else {
        None
    };
    let spawn_deadline = if watch_stream_timeout {
        invocation.timeout.map(|timeout| Instant::now() + timeout)
    } else {
        None
    };
    let mut child = command.spawn().map_err(map_spawn_error)?;
    let pid = child.id();
    let timed_identity = launch_authority
        .as_mut()
        .map(|authority| authority.register(pid));
    drop(launch_authority);
    let group = match options.group {
        ProcessGroupConfig::Join(pgid) => ProcessGroup::from_pgid(pgid),
        ProcessGroupConfig::NewRoot => ProcessGroup::from_child(&child),
        ProcessGroupConfig::Inherit => ProcessGroup::from_child(&child).signaling_pid_only(),
    };
    if options.group.parent_sets_group() {
        assign_child_to_process_group(&child, group);
    }
    let mut timeout_watch = None;
    if let (Some(deadline), Some(identity)) = (spawn_deadline, timed_identity.as_ref()) {
        let stdout = child.stdout.as_ref().map(AsFd::as_fd);
        match ProcessTimeoutWatch::start(group, stdout, deadline, identity.clone()) {
            Ok(watch) => timeout_watch = Some(watch),
            Err(error) => {
                let mut no_watch = None;
                let error = cleanup_stream_spawn_failure(
                    error,
                    &mut child,
                    pid,
                    group,
                    timed_identity.as_ref(),
                    &mut no_watch,
                );
                return Err(error);
            }
        }
    }
    if let Err(error) = cgroup.assign_pid(i64::from(pid)) {
        let error = cleanup_stream_spawn_failure(
            map_cgroup_error(error),
            &mut child,
            pid,
            group,
            timed_identity.as_ref(),
            &mut timeout_watch,
        );
        return Err(error);
    }
    let input = match InputDelivery::start_stream(&mut child, invocation) {
        Ok(input) => input,
        Err(error) => {
            let error = cleanup_stream_spawn_failure(
                error,
                &mut child,
                pid,
                group,
                timed_identity.as_ref(),
                &mut timeout_watch,
            );
            return Err(error);
        }
    };
    let mut managed = ManagedChild {
        child,
        pid,
        group,
        target: invocation.target.clone(),
        argv: invocation.argv.clone(),
        cwd: invocation.cwd.clone(),
        env_overlay: invocation.env_overlay.clone(),
        deadline: if watch_stream_timeout {
            spawn_deadline
        } else {
            invocation.timeout.map(|timeout| Instant::now() + timeout)
        },
        detached: options.spawn.detach || options.spawn.new_session,
        consumed: false,
        cgroup,
        input,
        input_error: None,
        accepted_exit_codes: invocation.accepted_exit_codes,
    };
    if watch_stream_timeout {
        drive_stream_managed_input(
            &mut managed,
            timed_identity.as_ref(),
            &mut timeout_watch,
        )?;
    } else {
        drive_managed_input(&mut managed)?;
    }
    Ok((managed, timeout_watch, timed_identity))
}

pub(crate) fn drive_managed_input(child: &mut ManagedChild) -> Result<(), RunError> {
    if let Some(error) = &child.input_error {
        return Err(error.clone());
    }
    if let Err(error) = feed_input(&mut child.input) {
        child.process_group().kill();
        let _ = child.child.wait();
        child.consumed = true;
        child.input = None;
        child.input_error = Some(error.clone());
        return Err(error);
    }
    Ok(())
}

fn drive_stream_managed_input(
    child: &mut ManagedChild,
    identity: Option<&TimedChildIdentity>,
    timeout_watch: &mut Option<ProcessTimeoutWatch>,
) -> Result<(), RunError> {
    if let Some(error) = &child.input_error {
        return Err(error.clone());
    }
    if let Err(error) = feed_input(&mut child.input) {
        let outcome = stop_timeout_watch(timeout_watch);
        let mut primary = outcome
            .as_ref()
            .and_then(timeout_outcome_error)
            .map_or(error.clone(), |cleanup| with_cleanup_error(error.clone(), cleanup));
        let mut identity_lost = outcome
            .as_ref()
            .is_some_and(timeout_outcome_identity_lost);
        let group = child.process_group();
        if let Err(cleanup) = terminate_stream_child(
            &mut child.child,
            child.pid,
            group,
            identity,
            &mut child.consumed,
            &mut identity_lost,
        ) {
            primary = with_cleanup_error(primary, cleanup);
        }
        child.input = None;
        child.input_error = Some(error.clone());
        return Err(primary);
    }
    Ok(())
}

fn timeout_outcome_error(outcome: &ProcessTimeoutOutcome) -> Option<RunError> {
    match outcome {
        ProcessTimeoutOutcome::TimedOut {
            signal_error: Some(error),
            ..
        } => Some(error.clone()),
        ProcessTimeoutOutcome::Failed(failure) => Some(failure.error.clone()),
        ProcessTimeoutOutcome::Completed
        | ProcessTimeoutOutcome::TimedOut {
            signal_error: None,
            ..
        }
        | ProcessTimeoutOutcome::Stopped => None,
    }
}

fn timeout_outcome_identity_lost(outcome: &ProcessTimeoutOutcome) -> bool {
    matches!(
        outcome,
        ProcessTimeoutOutcome::Failed(ProcessTimeoutFailure {
            child_identity_lost: true,
            ..
        }) | ProcessTimeoutOutcome::TimedOut {
            child_identity_lost: true,
            ..
        }
    )
}

fn cleanup_stream_spawn_failure(
    mut primary: RunError,
    child: &mut std::process::Child,
    pid: u32,
    group: ProcessGroup,
    identity: Option<&TimedChildIdentity>,
    timeout_watch: &mut Option<ProcessTimeoutWatch>,
) -> RunError {
    let outcome = stop_timeout_watch(timeout_watch);
    if let Some(outcome) = &outcome
        && let Some(cleanup) = timeout_outcome_error(outcome)
    {
        primary = with_cleanup_error(primary, cleanup);
    }
    let mut consumed = false;
    let mut identity_lost = outcome
        .as_ref()
        .is_some_and(timeout_outcome_identity_lost);
    if let Err(cleanup) =
        terminate_stream_child(
            child,
            pid,
            group,
            identity,
            &mut consumed,
            &mut identity_lost,
        )
    {
        primary = with_cleanup_error(primary, cleanup);
    }
    primary
}

pub(crate) fn with_cleanup_error(mut primary: RunError, cleanup: RunError) -> RunError {
    primary.contexts.push(ErrorContext {
        kind: cleanup.kind.clone(),
        message: Some(cleanup.message.clone()),
        span: cleanup.span,
    });
    let previous_cause = primary.cause.take();
    let cleanup = Value::RunError(Box::new(cleanup));
    let cause = previous_cause.map_or(cleanup.clone(), |previous| {
        append_error_cause(previous.as_value().clone(), cleanup)
    });
    let Value::RunError(primary) = Value::RunError(Box::new(primary))
        .with_error_cause(cause)
        .expect("process cleanup errors can be attached as causes")
    else {
        unreachable!("attaching a process cleanup cause preserves the outer error")
    };
    *primary
}

pub(crate) fn append_error_cause(mut value: Value, suffix: Value) -> Value {
    let mut chain = Vec::new();
    loop {
        let cause = match &mut value {
            Value::Error(error) => error.cause.take(),
            Value::RunError(error) => error.cause.take(),
            _ => None,
        };
        chain.push(value);
        let Some(cause) = cause else {
            break;
        };
        value = cause.as_value().clone();
    }
    let tail = chain.pop().expect("a process error has a cause chain");
    chain.push(
        tail.with_error_cause(suffix)
            .expect("error causes contain only errors"),
    );
    let mut rebuilt = None;
    for value in chain.into_iter().rev() {
        rebuilt = Some(match rebuilt {
            Some(cause) => value
                .with_error_cause(cause)
                .expect("error causes contain only errors"),
            None => value,
        });
    }
    rebuilt.expect("a cleanup error has a cause chain")
}

fn stop_timeout_watch(watch: &mut Option<ProcessTimeoutWatch>) -> Option<ProcessTimeoutOutcome> {
    watch.take().map(|mut watch| watch.stop_and_join())
}

#[derive(Clone, Copy)]
enum ProcessTimeoutControl {
    Monitoring,
    StopRequested,
    Finished,
}

enum ProcessTimeoutOutcome {
    Completed,
    TimedOut {
        signal_error: Option<RunError>,
        child_identity_lost: bool,
    },
    Failed(ProcessTimeoutFailure),
    Stopped,
}

enum ProcessStreamTimeout {
    Watching(ProcessTimeoutWatch),
    TimedOut(Option<RunError>),
}

struct ProcessTimeoutFailure {
    error: RunError,
    child_identity_lost: bool,
}

enum ChildIdentityObservation {
    Running,
    Exited,
    Lost,
}

struct ProcessTimeoutWatch {
    control: Arc<Mutex<ProcessTimeoutControl>>,
    worker: Option<std::thread::JoinHandle<ProcessTimeoutOutcome>>,
    identity: TimedChildIdentity,
    group: ProcessGroup,
}

impl ProcessTimeoutWatch {
    fn start(
        group: ProcessGroup,
        stdout: Option<BorrowedFd<'_>>,
        deadline: Instant,
        identity: TimedChildIdentity,
    ) -> Result<Self, RunError> {
        let stdout = stdout
            .map(|stdout| rio::fcntl_dupfd_cloexec(stdout, 0))
            .transpose()
            .map_err(|error| RunError::new("io", error.to_string()))?;
        let control = Arc::new(Mutex::new(ProcessTimeoutControl::Monitoring));
        let worker_control = Arc::clone(&control);
        let watch_identity = identity.clone();
        let worker = std::thread::Builder::new()
            .name("xsh-process-timeout".to_string())
            .spawn(move || {
                match catch_unwind(AssertUnwindSafe(|| {
                    process_timeout_watch_loop(
                        group,
                        stdout,
                        deadline,
                        identity.clone(),
                        Arc::clone(&worker_control),
                    )
                })) {
                    Ok(outcome) => outcome,
                    Err(_) => fail_process_timeout_watch(
                        &identity,
                        group,
                        &worker_control,
                        RunError::new("io", "process timeout monitor panicked"),
                    ),
                }
            })
            .map_err(map_wait_error)?;
        Ok(Self {
            control,
            worker: Some(worker),
            identity: watch_identity,
            group,
        })
    }

    fn is_finished(&self) -> bool {
        self.worker.as_ref().is_some_and(std::thread::JoinHandle::is_finished)
    }

    fn request_stop(&self) {
        let mut control = self
            .control
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if matches!(*control, ProcessTimeoutControl::Monitoring) {
            *control = ProcessTimeoutControl::StopRequested;
        }
    }

    fn finish(&mut self) -> ProcessTimeoutOutcome {
        let Some(worker) = self.worker.take() else {
            return ProcessTimeoutOutcome::Failed(ProcessTimeoutFailure {
                error: RunError::new("io", "process timeout monitor was already joined"),
                child_identity_lost: false,
            });
        };
        match worker.join() {
            Ok(outcome) => outcome,
            Err(_) => fail_process_timeout_watch(
                &self.identity,
                self.group,
                &self.control,
                RunError::new("io", "process timeout monitor panicked"),
            ),
        }
    }

    fn try_finish(&mut self) -> Option<ProcessTimeoutOutcome> {
        if !self.is_finished() {
            return None;
        }
        Some(self.finish())
    }

    fn stop_and_join(&mut self) -> ProcessTimeoutOutcome {
        self.request_stop();
        self.finish()
    }
}

impl Drop for ProcessTimeoutWatch {
    fn drop(&mut self) {
        if self.worker.is_some() {
            let _ = self.stop_and_join();
        }
    }
}

fn process_timeout_watch_loop(
    group: ProcessGroup,
    stdout: Option<OwnedFd>,
    deadline: Instant,
    identity: TimedChildIdentity,
    control: Arc<Mutex<ProcessTimeoutControl>>,
) -> ProcessTimeoutOutcome {
    let mut stdout_closed = stdout.is_none();
    loop {
        if process_timeout_stop_requested(&control) {
            return ProcessTimeoutOutcome::Stopped;
        }
        let child_exited = match identity.observe() {
            Ok(ChildIdentityObservation::Exited) => true,
            Ok(ChildIdentityObservation::Running) => false,
            Ok(ChildIdentityObservation::Lost) => {
                return finish_process_timeout_watch(
                    &control,
                    ProcessTimeoutOutcome::Failed(ProcessTimeoutFailure {
                        error: RunError::new("io", "child process identity was lost"),
                        child_identity_lost: true,
                    }),
                );
            }
            Err(error) => {
                return fail_process_timeout_watch(&identity, group, &control, error);
            }
        };
        if child_exited && stdout_closed && Instant::now() < deadline {
            return finish_process_timeout_watch(&control, ProcessTimeoutOutcome::Completed);
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            if !claim_process_timeout_watch(&control) {
                return ProcessTimeoutOutcome::Stopped;
            }
            let (signal_error, child_identity_lost) = match identity
                .signal_group_checked(group, libc::SIGKILL)
            {
                Ok(true) => (None, false),
                Ok(false) => (
                    Some(RunError::new("io", "child process identity was lost")),
                    true,
                ),
                Err(error) => match identity.signal_process_checked(libc::SIGKILL) {
                    Ok(true) => (Some(error), false),
                    Ok(false) => (
                        Some(with_cleanup_error(
                            error,
                            RunError::new("io", "child process identity was lost"),
                        )),
                        true,
                    ),
                    Err(cleanup_error) => {
                        (Some(with_cleanup_error(error, cleanup_error)), false)
                    }
                },
            };
            return ProcessTimeoutOutcome::TimedOut {
                signal_error,
                child_identity_lost,
            };
        }
        let poll_for = remaining.min(WAIT_POLL);
        if !stdout_closed {
            let Some(stdout) = stdout.as_ref() else {
                stdout_closed = true;
                continue;
            };
            let timeout =
                revent::Timespec::try_from(poll_for).expect("WAIT_POLL fits Timespec");
            let mut pollfds = [revent::PollFd::new(stdout, revent::PollFlags::HUP)];
            match revent::poll(&mut pollfds, Some(&timeout)) {
                Ok(_) => {
                    let events = pollfds[0].revents();
                    if events.contains(revent::PollFlags::NVAL) {
                        return fail_process_timeout_watch(
                            &identity,
                            group,
                            &control,
                            RunError::new(
                                "io",
                                "process stdout monitor received an invalid descriptor",
                            ),
                        );
                    }
                    if events.contains(revent::PollFlags::ERR) {
                        return fail_process_timeout_watch(
                            &identity,
                            group,
                            &control,
                            RunError::new(
                                "io",
                                "process stdout monitor received a poll error",
                            ),
                        );
                    }
                    stdout_closed = events.contains(revent::PollFlags::HUP);
                }
                Err(rustix::io::Errno::INTR) => {}
                Err(error) => {
                    return fail_process_timeout_watch(
                        &identity,
                        group,
                        &control,
                        map_wait_error(io::Error::from(error)),
                    );
                }
            }
        } else {
            std::thread::sleep(poll_for);
        }
    }
}

fn fail_process_timeout_watch(
    identity: &TimedChildIdentity,
    group: ProcessGroup,
    control: &Mutex<ProcessTimeoutControl>,
    mut error: RunError,
) -> ProcessTimeoutOutcome {
    let mut child_identity_lost = false;
    match identity.signal_group_checked(group, libc::SIGKILL) {
        Ok(true) => {}
        Ok(false) => {
            child_identity_lost = true;
            error = with_cleanup_error(
                error,
                RunError::new("io", "child process identity was lost"),
            );
        }
        Err(group_error) => match identity.signal_process_checked(libc::SIGKILL) {
            Ok(true) => error = with_cleanup_error(error, group_error),
            Ok(false) => {
                child_identity_lost = true;
                error = with_cleanup_error(
                    with_cleanup_error(
                        error,
                        group_error,
                    ),
                    RunError::new("io", "child process identity was lost"),
                );
            }
            Err(child_error) => {
                error = with_cleanup_error(
                    with_cleanup_error(error, group_error),
                    child_error,
                );
            }
        },
    }
    finish_process_timeout_watch(
        control,
        ProcessTimeoutOutcome::Failed(ProcessTimeoutFailure {
            error,
            child_identity_lost,
        }),
    )
}

fn process_timeout_stop_requested(control: &Mutex<ProcessTimeoutControl>) -> bool {
    let mut control = control
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if matches!(*control, ProcessTimeoutControl::StopRequested) {
        *control = ProcessTimeoutControl::Finished;
        true
    } else {
        false
    }
}

fn claim_process_timeout_watch(control: &Mutex<ProcessTimeoutControl>) -> bool {
    let mut control = control
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    match *control {
        ProcessTimeoutControl::Monitoring => {
            *control = ProcessTimeoutControl::Finished;
            true
        }
        ProcessTimeoutControl::StopRequested => {
            *control = ProcessTimeoutControl::Finished;
            false
        }
        ProcessTimeoutControl::Finished => false,
    }
}

fn finish_process_timeout_watch(
    control: &Mutex<ProcessTimeoutControl>,
    outcome: ProcessTimeoutOutcome,
) -> ProcessTimeoutOutcome {
    if matches!(&outcome, ProcessTimeoutOutcome::Failed(_)) {
        let mut control = control
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *control = ProcessTimeoutControl::Finished;
        return outcome;
    }
    if claim_process_timeout_watch(control) {
        outcome
    } else {
        ProcessTimeoutOutcome::Stopped
    }
}

fn process_exited_without_reaping(pid: u32) -> Result<ChildIdentityObservation, RunError> {
    let pid = rprocess::Pid::from_raw(pid as i32)
        .ok_or_else(|| RunError::new("io", "child pid is out of range"))?;
    loop {
        match rprocess::waitid(
            rprocess::WaitId::Pid(pid),
            rprocess::WaitIdOptions::NOHANG
                | rprocess::WaitIdOptions::EXITED
                | rprocess::WaitIdOptions::NOWAIT,
        ) {
            Ok(Some(status)) => {
                return Ok(if status.exited() || status.killed() || status.dumped() {
                    ChildIdentityObservation::Exited
                } else {
                    ChildIdentityObservation::Running
                });
            }
            Ok(None) => return Ok(ChildIdentityObservation::Running),
            Err(rustix::io::Errno::INTR) => {}
            Err(rustix::io::Errno::CHILD) => return Ok(ChildIdentityObservation::Lost),
            Err(error) => return Err(map_wait_error(io::Error::from(error))),
        }
    }
}

fn signal_process_group_checked(group: ProcessGroup, signal: i32) -> Result<(), RunError> {
    let pgid = rprocess::Pid::from_raw(group.pgid)
        .ok_or_else(|| RunError::new("io", "process group id is invalid"))?;
    let signal = signal_from_i32(signal)
        .ok_or_else(|| RunError::new("io", "process signal is invalid"))?;
    let result = if group.pid_only {
        rprocess::kill_process(pgid, signal)
    } else {
        rprocess::kill_process_group(pgid, signal)
    };
    if let Err(error) = result {
        let error = io::Error::from(error);
        if !matches!(error.raw_os_error(), Some(libc::ESRCH)) {
            return Err(map_wait_error(error));
        }
    }
    Ok(())
}

fn signal_process_checked(pid: u32, signal: i32) -> Result<(), RunError> {
    let pid = rprocess::Pid::from_raw(pid as i32)
        .ok_or_else(|| RunError::new("io", "process id is invalid"))?;
    let signal = signal_from_i32(signal)
        .ok_or_else(|| RunError::new("io", "process signal is invalid"))?;
    if let Err(error) = rprocess::kill_process(pid, signal) {
        let error = io::Error::from(error);
        if !matches!(error.raw_os_error(), Some(libc::ESRCH)) {
            return Err(map_wait_error(error));
        }
    }
    Ok(())
}

fn terminate_stream_child(
    child: &mut std::process::Child,
    pid: u32,
    group: ProcessGroup,
    identity: Option<&TimedChildIdentity>,
    consumed: &mut bool,
    identity_lost: &mut bool,
) -> Result<Option<std::process::ExitStatus>, RunError> {
    if *identity_lost {
        *consumed = true;
        return Ok(None);
    }
    let mut failure = None;
    if !*consumed {
        let observation = match identity {
            Some(identity) => identity.observe(),
            None => process_exited_without_reaping(pid),
        };
        match observation {
            Ok(ChildIdentityObservation::Running | ChildIdentityObservation::Exited) => {}
            Ok(ChildIdentityObservation::Lost) => {
                *identity_lost = true;
                *consumed = true;
                return Err(map_wait_error(io::Error::from_raw_os_error(libc::ECHILD)));
            }
            Err(error) => {
                failure = Some(error);
            }
        }
    }
    let group_signal = match identity {
        Some(identity) => identity.signal_group_checked(group, libc::SIGKILL),
        None => signal_process_group_checked(group, libc::SIGKILL).map(|()| true),
    };
    match group_signal {
        Ok(true) => {}
        Ok(false) => {
            *identity_lost = true;
            *consumed = true;
            let error = map_wait_error(io::Error::from_raw_os_error(libc::ECHILD));
            return Err(failure.map_or(error.clone(), |primary| {
                with_cleanup_error(primary, error)
            }));
        }
        Err(error) => {
            failure = Some(match failure.take() {
                Some(primary) => with_cleanup_error(primary, error),
                None => error,
            });
        }
    }
    if failure.is_some() && !*consumed {
        let direct_signal = match identity {
            Some(identity) => identity.signal_process_checked(libc::SIGKILL),
            None => signal_process_checked(pid, libc::SIGKILL).map(|()| true),
        };
        match direct_signal {
            Ok(true) => {}
            Ok(false) => {
                *identity_lost = true;
                *consumed = true;
                let error = map_wait_error(io::Error::from_raw_os_error(libc::ECHILD));
                return Err(failure.map_or(error.clone(), |primary| {
                    with_cleanup_error(primary, error)
                }));
            }
            Err(error) => {
                failure = Some(match failure.take() {
                    Some(primary) => with_cleanup_error(primary, error),
                    None => error,
                });
            }
        }
    }
    let status = if *consumed {
        None
    } else {
        let waited = if let Some(identity) = identity {
            loop {
                match identity.waitpid(rprocess::WaitOptions::NOHANG) {
                    Ok(Some((_, status)))
                        if wifexited(status.as_raw()) || wifsignaled(status.as_raw()) =>
                    {
                        break Ok(<std::process::ExitStatus as ExitStatusExt>::from_raw(
                            status.as_raw(),
                        ));
                    }
                    Ok(Some(_)) | Ok(None) => std::thread::sleep(WAIT_POLL),
                    Err(error) => break Err(error),
                }
            }
        } else {
            child.wait().map_err(map_wait_error)
        };
        match waited {
            Ok(status) => {
                *consumed = true;
                Some(status)
            }
            Err(error) => {
                *identity_lost = identity.is_some_and(|identity| !identity.is_valid());
                *consumed = true;
                failure = Some(match failure.take() {
                    Some(primary) => with_cleanup_error(primary, error),
                    None => error,
                });
                None
            }
        }
    };
    if let Some(error) = failure {
        Err(error)
    } else {
        Ok(status)
    }
}

pub(crate) struct ProcessStream {
    child: ManagedChild,
    identity: Option<TimedChildIdentity>,
    stdout: Option<std::process::ChildStdout>,
    timeout: Option<ProcessStreamTimeout>,
    status: Option<ProcessStatus>,
    cancellation: Option<Cancellation>,
    captured: usize,
    _foreground: Option<ForegroundTerminal>,
}

impl ProcessStream {
    pub(crate) fn start(invocation: &ProcessInvocation) -> Result<Self, RunError> {
        let mut options = SpawnManagedOptions::inherited_process_group();
        options.stdout = ManagedStdio::Piped;
        let (mut child, mut timeout_watch, identity) = spawn_stream_managed(invocation, options)?;
        let stdout = child.child.stdout.take();
        if let Some(stdout) = &stdout
            && let Err(error) = set_nonblocking(stdout.as_fd())
        {
            let outcome = stop_timeout_watch(&mut timeout_watch);
            let mut primary = outcome
                .as_ref()
                .and_then(timeout_outcome_error)
                .map_or(error.clone(), |cleanup| with_cleanup_error(error.clone(), cleanup));
            let mut identity_lost = outcome
                .as_ref()
                .is_some_and(timeout_outcome_identity_lost);
            let group = child.process_group();
            if let Err(cleanup) = terminate_stream_child(
                &mut child.child,
                child.pid,
                group,
                identity.as_ref(),
                &mut child.consumed,
                &mut identity_lost,
            ) {
                primary = with_cleanup_error(primary, cleanup);
            }
            return Err(primary);
        }
        let foreground = ForegroundTerminal::take(child.process_group());
        Ok(Self {
            child,
            identity,
            stdout,
            timeout: timeout_watch.map(ProcessStreamTimeout::Watching),
            status: None,
            cancellation: None,
            captured: 0,
            _foreground: foreground,
        })
    }

    pub(crate) fn next(
        &mut self,
        policy: &mut dyn CancellationPolicy,
    ) -> Result<Option<Vec<u8>>, RunError> {
        loop {
            self.update_timeout_watch()?;
            if let Some(error) = &self.child.input_error {
                return Err(error.clone());
            }
            if let Err(error) = feed_input(&mut self.child.input) {
                self.child.input = None;
                self.child.input_error = Some(error.clone());
                return Err(error);
            }
            if let Some(stdout) = &self.stdout {
                let mut bytes = vec![0; 8192];
                match rio::read(stdout.as_fd(), &mut bytes) {
                    Ok(0) => self.stdout = None,
                    Ok(count) => {
                        self.captured += count;
                        if self.captured > CAPTURE_LIMIT {
                            return Err(RunError::new(
                                "capture-limit",
                                "streamed stdout exceeded the capture limit",
                            ));
                        }
                        bytes.truncate(count);
                        return Ok(Some(bytes));
                    }
                    Err(rustix::io::Errno::AGAIN | rustix::io::Errno::INTR) => {}
                    Err(error) => return Err(RunError::new("io", error.to_string())),
                }
            }
            if self.status.is_none()
                && !matches!(self.timeout.as_ref(), Some(ProcessStreamTimeout::Watching(_)))
            {
                match poll_stream_child(&mut self.child, self.identity.as_ref())? {
                    ChildWaitOutcome::Exited(status) | ChildWaitOutcome::Signaled(status) => {
                        self.child.consumed = true;
                        self.status = Some(status);
                        policy.process_group_finished(self.child.process_group());
                    }
                    _ => {}
                }
            }
            if let Some(status) = &self.status
                && self.stdout.is_none()
            {
                if self.is_timed_out() {
                    return Err(self.timeout_error(Some(status.clone())));
                }
                if let Some(cancellation) = &self.cancellation {
                    return Err(cancellation.error(Some(status.clone())));
                }
                if let Some(error) =
                    completion_error(status, &[self.child.accepted_exit_codes], true)
                {
                    return Err(error);
                }
                return Ok(None);
            }
            if self.is_timed_out() {
                self.stdout = None;
                if self.identity.as_ref().is_some_and(|identity| !identity.is_valid()) {
                    self.child.consumed = true;
                    return Err(self.timeout_error(None));
                }
                return match self.reap_timeout_child() {
                    Ok(status) => Err(self.timeout_error(Some(status))),
                    Err(cleanup) => {
                        self.child.consumed = true;
                        Err(with_cleanup_error(self.timeout_error(None), cleanup))
                    }
                };
            }
            check_cancellation_with_identity(
                self.child.process_group(),
                self.identity.as_ref(),
                &mut self.cancellation,
                policy,
            );
            std::thread::sleep(WAIT_POLL);
        }
    }

    fn update_timeout_watch(&mut self) -> Result<(), RunError> {
        let outcome = match self.timeout.as_mut() {
            Some(ProcessStreamTimeout::Watching(watch)) => watch.try_finish(),
            Some(ProcessStreamTimeout::TimedOut(_)) | None => None,
        };
        let Some(outcome) = outcome else {
            return Ok(());
        };
        match outcome {
            ProcessTimeoutOutcome::Completed => {
                self.timeout = None;
                Ok(())
            }
            ProcessTimeoutOutcome::TimedOut { signal_error, .. } => {
                self.timeout = Some(ProcessStreamTimeout::TimedOut(signal_error));
                Ok(())
            }
            ProcessTimeoutOutcome::Failed(failure) => {
                self.timeout = None;
                Err(self.cleanup_after_timeout_watch_failure(
                    failure.error,
                    failure.child_identity_lost,
                ))
            }
            ProcessTimeoutOutcome::Stopped => {
                self.timeout = None;
                Err(self.cleanup_after_timeout_watch_failure(RunError::new(
                    "io",
                    "process timeout monitor stopped before the stream ended",
                ), false))
            }
        }
    }

    fn cleanup_after_timeout_watch_failure(
        &mut self,
        mut error: RunError,
        mut identity_lost: bool,
    ) -> RunError {
        self.stdout = None;
        let group = self.child.process_group();
        match terminate_stream_child(
            &mut self.child.child,
            self.child.pid,
            group,
            self.identity.as_ref(),
            &mut self.child.consumed,
            &mut identity_lost,
        ) {
            Ok(Some(status)) => {
                let status = process_status(status, 0, &self.child.target, Some(self.child.pid));
                self.status = Some(status.clone());
                error = error.with_status(status);
            }
            Ok(None) => {}
            Err(cleanup_error) => error = with_cleanup_error(error, cleanup_error),
        }
        self.child.input = None;
        error
    }

    fn reap_timeout_child(&mut self) -> Result<ProcessStatus, RunError> {
        if let Some(status) = &self.status {
            return Ok(status.clone());
        }
        let status = if let Some(identity) = &self.identity {
            loop {
                match identity.waitpid(rprocess::WaitOptions::NOHANG)? {
                    Some((_, status)) if wifexited(status.as_raw()) || wifsignaled(status.as_raw()) => {
                        break <std::process::ExitStatus as ExitStatusExt>::from_raw(
                            status.as_raw(),
                        );
                    }
                    Some(_) | None => std::thread::sleep(WAIT_POLL),
                }
            }
        } else {
            self.child.child.wait().map_err(map_wait_error)?
        };
        self.child.consumed = true;
        let status = process_status(status, 0, &self.child.target, Some(self.child.pid));
        self.status = Some(status.clone());
        Ok(status)
    }

    fn timeout_error(&self, status: Option<ProcessStatus>) -> RunError {
        let error = status.map_or_else(
            || RunError::new("timeout", "process timed out"),
            |status| RunError::new("timeout", "process timed out").with_status(status),
        );
        self.timeout_signal_error()
            .map_or(error.clone(), |cleanup| with_cleanup_error(error, cleanup.clone()))
    }

    fn timeout_signal_error(&self) -> Option<&RunError> {
        match self.timeout.as_ref() {
            Some(ProcessStreamTimeout::TimedOut(error)) => error.as_ref(),
            Some(ProcessStreamTimeout::Watching(_)) | None => None,
        }
    }

    fn is_timed_out(&self) -> bool {
        matches!(self.timeout.as_ref(), Some(ProcessStreamTimeout::TimedOut(_)))
    }

    pub(crate) fn process_group(&self) -> ProcessGroup {
        self.child.process_group()
    }

    pub(crate) fn timed_child_identity(&self) -> Option<TimedChildIdentity> {
        self.identity.clone()
    }

    pub(crate) fn end(&self, error: Option<RunError>) -> ProcessEnd {
        ProcessEnd {
            pid: Some(self.child.pid),
            status: self.status.clone().or_else(|| {
                error
                    .as_ref()
                    .and_then(|error| error.status.as_deref().cloned())
            }),
            error,
        }
    }

    pub(crate) fn cancel(
        &mut self,
        reported_error: Option<&RunError>,
    ) -> Result<(), RunError> {
        let mut failure = None;
        let mut identity_lost = false;
        if let Some(timeout) = self.timeout.take() {
            match timeout {
                ProcessStreamTimeout::Watching(mut timeout_watch) => {
                    match timeout_watch.stop_and_join() {
                        ProcessTimeoutOutcome::TimedOut { signal_error, .. } => {
                            self.timeout = Some(ProcessStreamTimeout::TimedOut(signal_error));
                        }
                        ProcessTimeoutOutcome::Failed(watch_failure) => {
                            identity_lost = watch_failure.child_identity_lost;
                            failure = Some(watch_failure.error);
                        }
                        ProcessTimeoutOutcome::Completed | ProcessTimeoutOutcome::Stopped => {}
                    }
                }
                ProcessStreamTimeout::TimedOut(signal_error) => {
                    self.timeout = Some(ProcessStreamTimeout::TimedOut(signal_error));
                }
            }
        }
        if self.stdout.is_some() || !self.child.consumed {
            let group = self.child.process_group();
            match terminate_stream_child(
                &mut self.child.child,
                self.child.pid,
                group,
                self.identity.as_ref(),
                &mut self.child.consumed,
                &mut identity_lost,
            ) {
                Ok(Some(status)) => {
                    self.status = Some(process_status(
                        status,
                        0,
                        &self.child.target,
                        Some(self.child.pid),
                    ));
                }
                Ok(None) => {}
                Err(error) => {
                    failure = Some(match failure.take() {
                        Some(primary) => with_cleanup_error(primary, error),
                        None => error,
                    });
                }
            }
        }
        self.stdout = None;
        self.child.input = None;
        if self.is_timed_out()
            && !reported_error.is_some_and(|error| error.kind == "timeout")
        {
            let timeout = self.timeout_error(self.status.clone());
            failure = Some(match failure.take() {
                Some(cleanup) => with_cleanup_error(timeout, cleanup),
                None => timeout,
            });
        }
        match failure {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

impl Drop for ProcessStream {
    fn drop(&mut self) {
        let _ = self.cancel(None);
    }
}

#[allow(dead_code)]
pub fn poll_managed(child: &mut ManagedChild) -> Result<ChildWaitOutcome, RunError> {
    waitpid_managed(child, WaitMode::Nonblocking)
}

fn poll_stream_child(
    child: &mut ManagedChild,
    identity: Option<&TimedChildIdentity>,
) -> Result<ChildWaitOutcome, RunError> {
    let Some(identity) = identity else {
        return poll_managed(child);
    };
    if child.consumed {
        return Ok(ChildWaitOutcome::StillRunning);
    }
    let flags = rprocess::WaitOptions::NOHANG | rprocess::WaitOptions::UNTRACED;
    loop {
        match identity.waitpid(flags)? {
            None => return Ok(ChildWaitOutcome::StillRunning),
            Some((waited, status)) if waited.as_raw_pid() as u32 == child.pid => {
                let outcome = child_wait_outcome_from_raw(
                    status.as_raw(),
                    0,
                    &child.target,
                    Some(child.pid),
                );
                if matches!(
                    outcome,
                    ChildWaitOutcome::Exited(_) | ChildWaitOutcome::Signaled(_)
                ) {
                    child.consumed = true;
                    child.input = None;
                }
                return Ok(outcome);
            }
            Some((_waited, _status)) => {}
        }
    }
}

pub fn wait_managed(
    child: &mut ManagedChild,
    mode: WaitMode,
    policy: &mut dyn CancellationPolicy,
) -> Result<(ChildWaitOutcome, Option<Cancellation>), RunError> {
    policy.before_shared_stdio();
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
                    && let ChildWaitOutcome::Exited(status) | ChildWaitOutcome::Signaled(status) =
                        &outcome
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
    /// The member is not the group's leader and shares the caller's group, so
    /// `pgid` holds the member's pid and signals must not be sent to the group.
    pid_only: bool,
}

impl ProcessGroup {
    fn from_child(child: &std::process::Child) -> Self {
        Self {
            pgid: child.id() as libc::pid_t,
            pid_only: false,
        }
    }

    fn signaling_pid_only(self) -> Self {
        Self {
            pid_only: true,
            ..self
        }
    }

    pub fn from_pgid(pgid: libc::pid_t) -> Self {
        Self {
            pgid,
            pid_only: false,
        }
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
        if group.pid_only || previous == group.pgid {
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
    fn new(
        signal: i32,
        group: ProcessGroup,
        identity: Option<&TimedChildIdentity>,
    ) -> Self {
        signal_process_group_owned(group, signal, identity);
        Self {
            signal,
            started_at: Instant::now(),
            killed: false,
        }
    }

    fn tick(&mut self, group: ProcessGroup, identity: Option<&TimedChildIdentity>) {
        if !self.killed && self.started_at.elapsed() >= CANCELLATION_GRACE {
            signal_process_group_owned(group, libc::SIGKILL, identity);
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
                for started in children.iter_mut() {
                    let _ = started.child.wait();
                }
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
        WaitMode::InteractiveForeground if child.input.is_some() => {
            rprocess::WaitOptions::NOHANG | rprocess::WaitOptions::UNTRACED
        }
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
        let mode = if child.input.is_some() {
            WaitMode::Script
        } else {
            WaitMode::InteractiveForeground
        };
        match waitpid_managed(child, mode)? {
            ChildWaitOutcome::Stopped { .. } | ChildWaitOutcome::StillRunning => {
                std::thread::sleep(WAIT_POLL);
                continue;
            }
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
    check_cancellation_with_identity(group, None, cancellation, policy);
}

fn check_cancellation_with_identity(
    group: ProcessGroup,
    identity: Option<&TimedChildIdentity>,
    cancellation: &mut Option<Cancellation>,
    policy: &mut dyn CancellationPolicy,
) {
    match policy.check_process_group(group) {
        CancellationDecision::Continue => {}
        CancellationDecision::Forward(signal) => {
            if cancellation.is_none() {
                *cancellation = Some(Cancellation::new(signal, group, identity));
            }
        }
        CancellationDecision::Escalate(signal) => {
            signal_process_group_owned(group, libc::SIGKILL, identity);
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
        cancellation.tick(group, identity);
    }
}

fn signal_process_group_owned(
    group: ProcessGroup,
    signal: i32,
    identity: Option<&TimedChildIdentity>,
) -> bool {
    match identity {
        Some(identity) => identity.signal_group(group, signal),
        None => {
            group.signal(signal);
            true
        }
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
    let result = if group.pid_only {
        rprocess::kill_process(pgid, signal)
    } else {
        rprocess::kill_process_group(pgid, signal)
    };
    if let Err(error) = result {
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
    }
    change_process_signal_action(signal, &action, Some(&mut old))?;
    Ok(old)
}

fn restore_signal_handler(signal: i32, old: &libc::sigaction) {
    let _ = change_process_signal_action(signal, old, None);
}

fn change_process_signal_action(
    signal: i32,
    action: &libc::sigaction,
    old: Option<&mut libc::sigaction>,
) -> io::Result<()> {
    let mut previous: libc::sigaction = unsafe { std::mem::zeroed() };
    if signal == libc::SIGCHLD {
        let mut authority = child_status_authority();
        if unsafe { libc::sigaction(signal, action, &mut previous) } != 0 {
            return Err(io::Error::last_os_error());
        }
        authority.update_status_retention(signal_action_retains_child_status(&previous));
        authority.update_status_retention(signal_action_retains_child_status(action));
    } else if unsafe { libc::sigaction(signal, action, &mut previous) } != 0 {
        return Err(io::Error::last_os_error());
    }
    if let Some(old) = old {
        *old = previous;
    }
    Ok(())
}

fn reset_child_signal_handlers() {
    unsafe {
        libc::signal(libc::SIGHUP, libc::SIG_DFL);
        libc::signal(libc::SIGINT, libc::SIG_DFL);
        libc::signal(libc::SIGQUIT, libc::SIG_DFL);
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
    apply_sigpipe_disposition_to_child();
}

fn command_with_stdio(
    invocation: &ProcessInvocation,
    executable: PathBuf,
    stdin: Stdio,
    stdout: Stdio,
    stderr: Stdio,
    group: ProcessGroupConfig,
    inherited: u8,
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
    inherit_standard_streams(&mut command, inherited, &invocation.redirections);
    apply_redirections(&mut command, &invocation.redirections)?;
    if let Some(entry) = &invocation.namespaces {
        crate::runtime::namespace::prepare(entry, &mut command)?;
    }
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
    let mut inherited = (if options.stdin == ManagedStdio::Inherit { 1 } else { 0 })
        | (if options.stdout == ManagedStdio::Inherit { 2 } else { 0 })
        | (if options.stderr == ManagedStdio::Inherit { 4 } else { 0 });
    if !options.apply_redirections && invocation.redirections.iter()
        .any(|item| matches!(item, ProcessRedirection::Input { .. }))
    {
        inherited &= !1;
    }
    inherit_standard_streams(&mut command, inherited,
        if options.apply_redirections { &invocation.redirections } else { &[] });
    if options.apply_redirections {
        apply_redirections(&mut command, &invocation.redirections)?;
    } else if invocation
        .redirections
        .iter()
        .any(|item| matches!(item, ProcessRedirection::Input { .. }))
    {
        validate_input_sources(&invocation.redirections)?;
        command.stdin(Stdio::piped());
    }
    if let Some(entry) = &invocation.namespaces {
        crate::runtime::namespace::prepare(entry, &mut command)?;
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

pub(crate) fn validate_input_sources(redirections: &[ProcessRedirection]) -> Result<(), RunError> {
    if redirections
        .iter()
        .any(|item| matches!(item, ProcessRedirection::Input { .. }))
        && redirections
            .iter()
            .filter(|item| {
                matches!(
                    item,
                    ProcessRedirection::Input { .. }
                        | ProcessRedirection::File {
                            stream: RedirectionStream::Stdin,
                            ..
                        }
                        | ProcessRedirection::Dup {
                            stream: RedirectionStream::Stdin,
                            ..
                        }
                        | ProcessRedirection::ChildDup {
                            stream: RedirectionStream::Stdin,
                            ..
                        }
                )
            })
            .count()
            != 1
    {
        return Err(RunError::new(
            "redirection",
            "Bytes input cannot compete with another stdin source",
        ));
    }
    Ok(())
}

pub(crate) fn inherit_standard_streams(
    command: &mut Command,
    mut inherited: u8,
    redirections: &[ProcessRedirection],
) {
    for redirection in redirections {
        let stream = match redirection {
            ProcessRedirection::Input { .. } => RedirectionStream::Stdin,
            ProcessRedirection::File { stream, .. } | ProcessRedirection::Dup { stream, .. } => *stream,
            ProcessRedirection::ChildDup { .. } => break,
        };
        inherited &= !(1 << stream_fd(stream));
    }
    crate::startup_stdio::inherit(command, inherited);
}

pub(crate) fn apply_redirections(
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
            ProcessRedirection::Input { .. } => {
                command.stdin(Stdio::piped());
            }
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
                crate::startup_stdio::check(*fd).map_err(map_redirection_error)?;
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
            ProcessRedirection::Input { .. } => {
                command.stdin(Stdio::piped());
            }
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
    // The runner opens redirections before spawning the child; a session
    // leader must not acquire a controlling terminal from a PTY slave.
    options.custom_flags(libc::O_NOCTTY);
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
    crate::startup_stdio::check(fd).map_err(map_redirection_error)?;
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
    if let Some(failure) = error
        .raw_os_error()
        .and_then(crate::runtime::namespace::step_failure)
    {
        return failure;
    }
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
    fn process_cleanup_errors_keep_their_order_and_nested_contexts() {
        let Value::RunError(primary) = Value::RunError(Box::new(RunError::new("io", "read")))
            .with_error_cause(Value::RunError(Box::new(RunError::new("spawn", "origin"))))
            .unwrap()
        else {
            panic!("a process error remains a process error")
        };
        let group_failure = RunError::new("io", "group signal").with_context(ErrorContext {
            kind: "group-kill".to_string(),
            message: Some("process group signal failed".to_string()),
            span: None,
        });
        let fallback_failure = RunError::new("io", "child signal").with_context(ErrorContext {
            kind: "child-kill".to_string(),
            message: Some("direct child signal failed".to_string()),
            span: None,
        });

        let combined = with_cleanup_error(
            with_cleanup_error(*primary, group_failure),
            fallback_failure,
        );
        assert_eq!(combined.contexts.len(), 2);
        let origin = combined.cause.as_ref().unwrap().as_value();
        assert_eq!(origin.error_kind(), Some("spawn"));
        let group_failure = origin.error_cause().unwrap().as_value();
        assert_eq!(group_failure.error_kind(), Some("io"));
        let Value::RunError(group_failure) = group_failure else {
            panic!("group failure remains a process error")
        };
        assert_eq!(group_failure.message, "group signal");
        assert_eq!(group_failure.contexts[0].kind, "group-kill");
        let fallback_failure = group_failure.cause.as_ref().unwrap().as_value();
        let Value::RunError(fallback_failure) = fallback_failure else {
            panic!("fallback failure remains a process error")
        };
        assert_eq!(fallback_failure.message, "child signal");
        assert_eq!(fallback_failure.contexts[0].kind, "child-kill");

        assert_eq!(map_wait_error(io::Error::from_raw_os_error(libc::ECHILD)).variant_name(), "Io");
        assert!(map_wait_error(io::Error::from_raw_os_error(libc::ECHILD))
            .facets()
            .iter()
            .any(|facet| facet == "HostIo"));
        assert_eq!(RunError::new("timeout", "deadline").variant_name(), "Timeout");
    }

    #[test]
    fn process_timeout_stop_request_preserves_worker_failure() {
        let control = Mutex::new(ProcessTimeoutControl::StopRequested);
        let error = with_cleanup_error(
            RunError::new("io", "monitor failed"),
            RunError::new("io", "cleanup failed"),
        );

        let ProcessTimeoutOutcome::Failed(failure) = finish_process_timeout_watch(
            &control,
            ProcessTimeoutOutcome::Failed(ProcessTimeoutFailure {
                error,
                child_identity_lost: false,
            }),
        ) else {
            panic!("a worker failure remains observable after a stop request")
        };

        assert_eq!(failure.error.message, "monitor failed");
        let cleanup = failure.error.cause.as_ref().unwrap().as_value();
        let Value::RunError(cleanup) = cleanup else {
            panic!("the cleanup cause remains a process error")
        };
        assert_eq!(cleanup.message, "cleanup failed");
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
            namespaces: None,
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
                namespaces: None,
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
            namespaces: None,
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
        let rejected_zero = completion_error(
            &ProcessStatus::exited(0),
            &[Some(AcceptedExitCodes::new(&[1]).unwrap())],
            false,
        )
        .unwrap();
        assert_eq!(rejected_zero.kind, "unexpected-exit");
        let signaled = ProcessStatus::signaled(libc::SIGTERM);
        let set = AcceptedExitCodes::new(&[0, 128 + i64::from(libc::SIGTERM)]).unwrap();
        assert_eq!(
            completion_error(&signaled, &[Some(set)], false)
                .unwrap()
                .kind,
            "signal"
        );
    }
}
