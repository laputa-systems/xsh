//! The foreground scope of a native file command.
//!
//! `cp`, `mv`, `rsync` and `rmrf` run inside the shell process, as ish runs
//! them. While one runs, this scope turns `SIGINT`, `SIGTERM` and `SIGHUP`
//! into cooperative cancellation, ignores `SIGTSTP` so Ctrl+Z cannot suspend
//! the shell, keeps `ISIG` on the terminal, runs the operation on a worker
//! thread, and draws a delayed, rate-limited progress line on the terminal.

use std::io::{self, IsTerminal as _, Write as _};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, SyncSender};
use std::time::{Duration, Instant};

use rustix::termios::{self, LocalModes, OptionalActions, Termios};
use xsh_root::transfer::Cancellation;

const POLL_INTERVAL: Duration = Duration::from_millis(10);
const PROGRESS_DELAY: Duration = Duration::from_millis(200);
const PROGRESS_INTERVAL: Duration = Duration::from_millis(50);
const BLOCKED_REPORT_AFTER: Duration = Duration::from_secs(30);
const EVENT_CAPACITY: usize = 256;

const INTERRUPT: u32 = 1;
const TERMINATE: u32 = 2;
const HANGUP: u32 = 4;

static PENDING: AtomicU32 = AtomicU32::new(0);
static TERMINATION: AtomicI32 = AtomicI32::new(0);
static LINE_CANCELLED: AtomicBool = AtomicBool::new(false);

/// Where one builtin stream ends up once redirections are applied.
pub(super) enum Sink<'a> {
    /// Collected for the shell, which writes it to its own stream after the
    /// command, as it does for every builtin.
    Buffer(&'a mut Vec<u8>),
    /// A redirection target, written as output arrives so a failing target
    /// stops the work.
    File(std::fs::File),
}

impl io::Write for Sink<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        match self {
            Self::Buffer(buffer) => buffer.write(bytes),
            Self::File(file) => file.write(bytes),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Buffer(_) => Ok(()),
            Self::File(file) => file.flush(),
        }
    }
}

/// The standard output and error of one native file command.
pub(super) struct Streams<'a> {
    pub(super) stdout: Sink<'a>,
    pub(super) stderr: Sink<'a>,
}

impl Streams<'_> {
    /// Whether progress may be drawn: diagnostics reach the shell's own
    /// standard error, and that is a terminal.
    pub(super) fn progress_terminal(&self) -> bool {
        matches!(self.stderr, Sink::Buffer(_)) && io::stderr().is_terminal()
    }
}

/// Starts a submitted line; a cancelled native command stops the rest of it.
pub(super) fn begin_line() {
    LINE_CANCELLED.store(false, Ordering::Release);
}

/// Whether a native command in the current line was cancelled by the user.
pub(super) fn line_cancelled() -> bool {
    LINE_CANCELLED.load(Ordering::Acquire)
}

/// Records a user cancellation that ends the current line with status 130.
pub(super) fn cancel_line() {
    LINE_CANCELLED.store(true, Ordering::Release);
}

/// Exits with `128 + signal` when `SIGTERM` or `SIGHUP` cancelled the command
/// that just finished. The command's buffered diagnostics are written first;
/// the terminal is already in the state the shell found it in, since the line
/// editor releases raw mode before any command runs.
pub(super) fn exit_if_terminated(streams: &mut Streams<'_>) {
    let signal = TERMINATION.swap(0, Ordering::AcqRel);
    if signal == 0 {
        return;
    }
    if let Sink::Buffer(buffer) = &mut streams.stderr {
        let _ = io::stderr().write_all(buffer);
        buffer.clear();
    }
    if let Sink::Buffer(buffer) = &mut streams.stdout {
        let _ = io::stdout().write_all(buffer);
        buffer.clear();
    }
    let _ = io::stdout().flush();
    let _ = io::stderr().flush();
    std::process::exit(128 + signal);
}

/// Sends worker events to the foreground thread.
pub(super) struct EventSink<E> {
    sender: SyncSender<E>,
}

impl<E> EventSink<E> {
    /// Queues one event, waiting while the foreground thread catches up.
    pub(super) fn emit(&self, event: E) -> io::Result<()> {
        self.sender.send(event).map_err(|_| {
            io::Error::new(
                io::ErrorKind::BrokenPipe,
                "foreground event receiver stopped",
            )
        })
    }
}

extern "C" fn record_signal(signal: libc::c_int) {
    let bit = match signal {
        libc::SIGINT => INTERRUPT,
        libc::SIGTERM => TERMINATE,
        libc::SIGHUP => HANGUP,
        _ => 0,
    };
    PENDING.fetch_or(bit, Ordering::AcqRel);
}

fn replace_disposition(
    signal: libc::c_int,
    handler: libc::sighandler_t,
    flags: libc::c_int,
) -> io::Result<libc::sigaction> {
    // SAFETY: both sigaction values are zero-initialized plain data owned by
    // this frame, and the installed handler only performs an atomic store.
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = handler;
        action.sa_flags = flags;
        if libc::sigemptyset(&mut action.sa_mask) != 0 {
            return Err(io::Error::last_os_error());
        }
        let mut previous: libc::sigaction = std::mem::zeroed();
        if libc::sigaction(signal, &action, &mut previous) != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(previous)
    }
}

fn restore_disposition(signal: libc::c_int, previous: &libc::sigaction) {
    // SAFETY: `previous` was filled by sigaction for the same signal.
    unsafe {
        libc::sigaction(signal, previous, std::ptr::null_mut());
    }
}

fn current_disposition(signal: libc::c_int) -> io::Result<libc::sighandler_t> {
    // SAFETY: the out parameter is zero-initialized plain data owned by this
    // frame and sigaction only reads the current disposition.
    unsafe {
        let mut current: libc::sigaction = std::mem::zeroed();
        if libc::sigaction(signal, std::ptr::null(), &mut current) != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(current.sa_sigaction)
    }
}

/// The signal dispositions one command replaced, restored on drop.
struct Signals {
    installed: Vec<(libc::c_int, libc::sigaction)>,
}

impl Signals {
    fn install() -> io::Result<Self> {
        PENDING.store(0, Ordering::Release);
        let mut signals = Self {
            installed: Vec::with_capacity(4),
        };
        let handler = record_signal as *const () as libc::sighandler_t;
        signals.replace(libc::SIGINT, handler, libc::SA_RESTART)?;
        signals.replace(libc::SIGTSTP, libc::SIG_IGN, 0)?;
        for signal in [libc::SIGTERM, libc::SIGHUP] {
            if current_disposition(signal)? != libc::SIG_IGN {
                signals.replace(signal, handler, libc::SA_RESTART)?;
            }
        }
        Ok(signals)
    }

    fn replace(
        &mut self,
        signal: libc::c_int,
        handler: libc::sighandler_t,
        flags: libc::c_int,
    ) -> io::Result<()> {
        let previous = replace_disposition(signal, handler, flags)?;
        self.installed.push((signal, previous));
        Ok(())
    }
}

impl Drop for Signals {
    fn drop(&mut self) {
        for (signal, previous) in self.installed.iter().rev() {
            restore_disposition(*signal, previous);
        }
        let pending = PENDING.swap(0, Ordering::AcqRel);
        for (bit, signal) in [(TERMINATE, libc::SIGTERM), (HANGUP, libc::SIGHUP)] {
            if pending & bit != 0 {
                let _ =
                    TERMINATION.compare_exchange(0, signal, Ordering::AcqRel, Ordering::Acquire);
            }
        }
    }
}

/// Keeps `ISIG` set on the terminal so Ctrl+C reaches the shell as `SIGINT`.
struct TerminalMode {
    original: Option<Termios>,
}

impl TerminalMode {
    fn enable() -> io::Result<Self> {
        let stdin = rustix::stdio::stdin();
        if !termios::isatty(stdin) {
            return Ok(Self { original: None });
        }
        let original = termios::tcgetattr(stdin)?;
        let mut active = original.clone();
        active.local_modes.insert(LocalModes::ISIG);
        termios::tcsetattr(stdin, OptionalActions::Drain, &active)?;
        Ok(Self {
            original: Some(original),
        })
    }
}

impl Drop for TerminalMode {
    fn drop(&mut self) {
        if let Some(original) = &self.original {
            let _ = termios::tcsetattr(rustix::stdio::stdin(), OptionalActions::Now, original);
        }
    }
}

/// The foreground state of one native file command.
pub(super) struct Foreground {
    cancellation: Cancellation,
    _signals: Signals,
    _terminal: TerminalMode,
    progress_terminal: bool,
    started: Instant,
    last_activity: Instant,
    watchdog_reported: bool,
    last_progress: Option<Instant>,
    pending_line: Option<String>,
    progress_drawn: bool,
    output_error: Option<io::Error>,
}

impl Foreground {
    /// Installs the signal and terminal scope. Progress is drawn only when
    /// `progress_terminal` is set.
    pub(super) fn start(progress_terminal: bool) -> io::Result<Self> {
        let signals = Signals::install()?;
        let terminal = TerminalMode::enable()?;
        let now = Instant::now();
        Ok(Self {
            cancellation: Cancellation::default(),
            _signals: signals,
            _terminal: terminal,
            progress_terminal,
            started: now,
            last_activity: now,
            watchdog_reported: false,
            last_progress: None,
            pending_line: None,
            progress_drawn: false,
            output_error: None,
        })
    }

    pub(super) fn cancellation(&self) -> Cancellation {
        self.cancellation.clone()
    }

    /// Turns received signals into cancellation requests.
    pub(super) fn poll(&mut self) {
        let pending = PENDING.swap(0, Ordering::AcqRel);
        if pending & INTERRUPT != 0 {
            self.cancellation.cancel();
        }
        for (bit, signal) in [(TERMINATE, libc::SIGTERM), (HANGUP, libc::SIGHUP)] {
            if pending & bit != 0 {
                self.cancellation.cancel();
                let _ =
                    TERMINATION.compare_exchange(0, signal, Ordering::AcqRel, Ordering::Acquire);
            }
        }
    }

    /// Draws `line` once the progress delay has passed and the previous line
    /// is old enough; an early line is kept and drawn when it becomes due.
    pub(super) fn report_progress_line(&mut self, line: &str) -> io::Result<()> {
        if !self.progress_terminal {
            return Ok(());
        }
        if self.started.elapsed() < PROGRESS_DELAY
            || self
                .last_progress
                .is_some_and(|last| last.elapsed() < PROGRESS_INTERVAL)
        {
            self.pending_line = Some(line.to_owned());
            return Ok(());
        }
        self.pending_line = None;
        self.draw(line)
    }

    /// Draws a counter line, `LABEL COMPLETED[/TOTAL]`.
    pub(super) fn report_progress(
        &mut self,
        label: &str,
        completed: u64,
        total: Option<u64>,
    ) -> io::Result<()> {
        let line = match total {
            Some(total) => format!("{label} {completed}/{total}"),
            None => format!("{label} {completed}"),
        };
        self.report_progress_line(&line)
    }

    fn draw(&mut self, line: &str) -> io::Result<()> {
        let result = write!(io::stderr(), "\r\x1b[2K{line}").and_then(|()| io::stderr().flush());
        match result {
            Ok(()) => {
                self.last_progress = Some(Instant::now());
                self.progress_drawn = true;
                Ok(())
            }
            Err(error) => {
                self.note_output_error(error);
                Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "foreground progress output failed",
                ))
            }
        }
    }

    fn replay_progress(&mut self) {
        if self.output_error.is_some()
            || self.started.elapsed() < PROGRESS_DELAY
            || self
                .last_progress
                .is_some_and(|last| last.elapsed() < PROGRESS_INTERVAL)
        {
            return;
        }
        if let Some(line) = self.pending_line.take() {
            let _ = self.draw(&line);
        }
    }

    /// Erases a drawn progress line.
    pub(super) fn clear_progress(&mut self) -> io::Result<()> {
        self.pending_line = None;
        if !self.progress_drawn {
            return Ok(());
        }
        self.progress_drawn = false;
        let result = write!(io::stderr(), "\r\x1b[2K").and_then(|()| io::stderr().flush());
        result.map_err(|error| {
            self.note_output_error(error);
            io::Error::new(
                io::ErrorKind::BrokenPipe,
                "foreground progress output failed",
            )
        })
    }

    /// Records that the operation advanced, for the stalled-work report.
    pub(super) fn note_activity(&mut self) {
        self.last_activity = Instant::now();
        self.watchdog_reported = false;
    }

    /// Runs `operation` on a worker thread and delivers its events to
    /// `handle` on this thread, polling signals in between and calling `tick`
    /// whenever no event arrived for a poll interval. A failing handler
    /// cancels the operation for output and its error is returned once the
    /// worker has stopped.
    pub(super) fn run<T, E, F, H, K>(
        &mut self,
        operation: F,
        mut handle: H,
        mut tick: K,
    ) -> io::Result<T>
    where
        T: Send,
        E: Send,
        F: FnOnce(&Cancellation, &EventSink<E>) -> T + Send,
        H: FnMut(&mut Self, E) -> io::Result<()>,
        K: FnMut(&mut Self) -> io::Result<()>,
    {
        let (sender, receiver) = mpsc::sync_channel(EVENT_CAPACITY);
        let cancellation = self.cancellation.clone();
        let mut output_error = None;
        let result = std::thread::scope(|scope| -> io::Result<T> {
            let sink = EventSink { sender };
            let worker = std::thread::Builder::new()
                .name("xshi-foreground".to_owned())
                .spawn_scoped(scope, move || operation(&cancellation, &sink))?;
            loop {
                self.poll();
                match receiver.recv_timeout(POLL_INTERVAL) {
                    Ok(event) => {
                        self.note_activity();
                        if let Err(error) = handle(self, event) {
                            self.cancellation.cancel_for_output();
                            let stored = self.output_error.take();
                            output_error.get_or_insert(stored.unwrap_or(error));
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => {
                        if output_error.is_none()
                            && let Err(error) = tick(self)
                        {
                            self.cancellation.cancel_for_output();
                            let stored = self.output_error.take();
                            output_error.get_or_insert(stored.unwrap_or(error));
                        }
                        self.check_watchdog();
                        self.replay_progress();
                    }
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
            self.poll();
            worker
                .join()
                .map_err(|_| io::Error::other("foreground operation worker panicked"))
        });
        if let Err(error) = self.clear_progress() {
            let stored = self.output_error.take();
            output_error.get_or_insert(stored.unwrap_or(error));
        }
        let value = result?;
        match output_error.or_else(|| self.output_error.take()) {
            Some(error) => Err(error),
            None => Ok(value),
        }
    }

    /// Clears progress and reports any output failure recorded so far.
    pub(super) fn finish(&mut self) -> io::Result<()> {
        let clear = self.clear_progress();
        match (clear, self.output_error.take()) {
            (Err(error), _) | (Ok(()), Some(error)) => Err(error),
            (Ok(()), None) => Ok(()),
        }
    }

    fn note_output_error(&mut self, error: io::Error) {
        self.cancellation.cancel_for_output();
        self.output_error.get_or_insert(error);
    }

    fn check_watchdog(&mut self) {
        if self.watchdog_reported || self.last_activity.elapsed() < BLOCKED_REPORT_AFTER {
            return;
        }
        self.watchdog_reported = true;
        if let Err(error) = writeln!(
            io::stderr(),
            "xshi: foreground operation has made no progress for 30 seconds; waiting for safe completion"
        )
        .and_then(|()| io::stderr().flush())
        {
            self.note_output_error(error);
        }
    }
}

impl Drop for Foreground {
    fn drop(&mut self) {
        let _ = self.clear_progress();
    }
}
