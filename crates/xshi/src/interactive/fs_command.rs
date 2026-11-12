use std::ffi::{OsStr, OsString};
use std::io::{self, IsTerminal as _, Write as _};
use std::os::fd::{AsFd, BorrowedFd};
use std::os::unix::ffi::OsStrExt as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::{Duration, Instant};

use xsh_root::transfer::{
    Action, Cancellation, Collision, EngineOptions, Jobs, MergeRequest, Observation, Operands,
    Operation, PolicyOptions, PolicyRequest, Preflight, Progress, PublishRequest, check_cancelled,
    into_basename, merge_tree, observe_entry, observe_fd, preflight, publish_tree, worker_count,
};

use super::rmrf::{self, FileContext};

pub(super) use super::rmrf::RMRF_HELP;

const COPY_HELP: &str = "Usage: {name} [OPTIONS] SOURCE DESTINATION
       {name} [OPTIONS] SOURCE... DIRECTORY
       {name} [OPTIONS] --into DIRECTORY SOURCE...

Copy objects and directory trees in the shell process.

OPTIONS:
  -r, -R                   Accepted; directory recursion is automatic
  --merge                   Merge directory contents
  --replace                 Replace supported leaf targets
  -n, --no-clobber           Skip existing targets
  --dry-run                 Report planned actions
  --durable                 Synchronize published copy data
  --cross-file-systems      Traverse nested file systems
  -j, --jobs N              Use N transfer workers (1..=32)
  -v, --verbose              Report completed operations
  --no-progress             Suppress terminal progress
  -h, --help                 Show this help
";

const MOVE_HELP: &str = "Usage: {name} [OPTIONS] SOURCE DESTINATION
       {name} [OPTIONS] SOURCE... DIRECTORY
       {name} [OPTIONS] --into DIRECTORY SOURCE...

Move objects and directory trees in the shell process.

OPTIONS:
  --merge                   Merge directory contents
  --replace                 Replace supported leaf targets
  -n, --no-clobber           Skip existing targets
  --dry-run                 Report planned actions
  --durable                 Synchronize published copy data
  --cross-file-systems      Traverse nested file systems
  -j, --jobs N              Use N transfer workers (1..=32)
  -v, --verbose              Report completed operations
  --no-progress             Suppress terminal progress
  -h, --help                 Show this help
";

pub(super) const PARENT_PROCESS_REFUSAL: &str =
    "requires the original shell process and cannot run in a forked context";

const PROGRESS_DELAY: Duration = Duration::from_millis(200);
const PROGRESS_INTERVAL: Duration = Duration::from_millis(50);
const PROGRESS_LINE_BYTES: usize = 64;

/// A parsed `cp` or `mv` invocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FileOperation {
    Copy,
    Move,
}

impl FileOperation {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Copy => "cp",
            Self::Move => "mv",
        }
    }

    fn verb(self) -> &'static str {
        match self {
            Self::Copy => "copy",
            Self::Move => "move",
        }
    }

    fn operation(self) -> Operation {
        match self {
            Self::Copy => Operation::Copy,
            Self::Move => Operation::Move,
        }
    }
}

pub(super) struct Invocation {
    operands: Operands,
    collision: Collision,
    merge: bool,
    dry_run: bool,
    durable: bool,
    cross_file_systems: bool,
    jobs: Option<Jobs>,
    verbose: bool,
    no_progress: bool,
}

pub(super) enum Parsed {
    Help,
    Run(Invocation),
}

#[derive(Debug)]
pub(super) struct SyntaxError(String);

pub(super) fn parse(arguments: &[String], operation: FileOperation) -> Result<Parsed, SyntaxError> {
    let mut collision = Collision::Error;
    let mut merge = false;
    let mut dry_run = false;
    let mut durable = false;
    let mut cross_file_systems = false;
    let mut jobs = None;
    let mut verbose = false;
    let mut no_progress = false;
    let mut into = None;
    let mut operands: Vec<PathBuf> = Vec::new();
    let mut options_enabled = true;
    let mut jobs_seen = false;
    let mut replace_seen = false;
    let mut no_clobber_seen = false;
    let mut help = false;
    let mut index = 0;

    while index < arguments.len() {
        let argument = arguments[index].as_str();
        index += 1;
        if options_enabled && argument == "--" {
            options_enabled = false;
            continue;
        }
        if options_enabled && matches!(argument, "-h" | "--help") {
            help = true;
            continue;
        }
        if !options_enabled || !is_option(argument) {
            operands.push(PathBuf::from(argument));
            continue;
        }

        match argument {
            "--into" => {
                if into.is_some() {
                    return Err(SyntaxError("--into may be used only once".to_owned()));
                }
                let Some(directory) = arguments.get(index) else {
                    return Err(SyntaxError("--into requires a directory".to_owned()));
                };
                into = Some(PathBuf::from(directory));
                index += 1;
            }
            "--merge" => merge = true,
            "--replace" => {
                collision = Collision::Replace;
                replace_seen = true;
            }
            "-n" | "--no-clobber" => {
                collision = Collision::Skip;
                no_clobber_seen = true;
            }
            "--dry-run" => dry_run = true,
            "--durable" => durable = true,
            "--cross-file-systems" => cross_file_systems = true,
            "-j" | "--jobs" => {
                if jobs_seen {
                    return Err(SyntaxError("--jobs may be set only once".to_owned()));
                }
                let Some(value) = arguments.get(index) else {
                    return Err(SyntaxError("--jobs requires a positive integer".to_owned()));
                };
                let count = value
                    .parse::<usize>()
                    .map_err(|_| SyntaxError("--jobs requires a positive integer".to_owned()))?;
                jobs = Some(
                    Jobs::new(count)
                        .map_err(|_| SyntaxError("--jobs must be in 1..=32".to_owned()))?,
                );
                jobs_seen = true;
                index += 1;
            }
            "-v" | "--verbose" => verbose = true,
            "--no-progress" => no_progress = true,
            "-r" | "-R" if operation == FileOperation::Copy => {}
            "-r" | "-R" => {
                return Err(SyntaxError(format!(
                    "{} does not accept recursive flags",
                    operation.name()
                )));
            }
            _ => {
                return Err(SyntaxError(format!(
                    "unknown option {}",
                    escape_bytes(argument.as_bytes())
                )));
            }
        }
    }

    if replace_seen && no_clobber_seen {
        return Err(SyntaxError(
            "--replace cannot be used with --no-clobber".to_owned(),
        ));
    }
    if help {
        return Ok(Parsed::Help);
    }

    let operands = match into {
        Some(directory) => {
            let mut sources = operands.into_iter();
            let Some(first) = sources.next() else {
                return Err(SyntaxError(
                    "--into requires at least one source operand".to_owned(),
                ));
            };
            Operands::Into {
                directory,
                first,
                rest: sources.collect(),
            }
        }
        None => {
            if operands.len() < 2 {
                return Err(SyntaxError(
                    "expected SOURCE DESTINATION, SOURCE... DIRECTORY, or --into DIRECTORY SOURCE..."
                        .to_owned(),
                ));
            }
            if operands.len() == 2 {
                let mut operands = operands.into_iter();
                Operands::Exact {
                    source: operands.next().unwrap(),
                    destination: operands.next().unwrap(),
                }
            } else {
                let directory = operands.pop().unwrap();
                let mut sources = operands.into_iter();
                Operands::Into {
                    directory,
                    first: sources.next().unwrap(),
                    rest: sources.collect(),
                }
            }
        }
    };

    Ok(Parsed::Run(Invocation {
        operands,
        collision,
        merge,
        dry_run,
        durable,
        cross_file_systems,
        jobs,
        verbose,
        no_progress,
    }))
}

fn is_option(argument: &str) -> bool {
    argument.as_bytes().starts_with(b"-") && argument != "-"
}

pub(super) fn help_text(operation: FileOperation, name: &str) -> String {
    let text = match operation {
        FileOperation::Copy => COPY_HELP,
        FileOperation::Move => MOVE_HELP,
    };
    text.replace("{name}", name)
}

fn usage(operation: FileOperation) -> String {
    let name = operation.name();
    format!(
        "usage: {name} [OPTIONS] SOURCE DESTINATION\n       {name} [OPTIONS] SOURCE... DIRECTORY\n       {name} [OPTIONS] --into DIRECTORY SOURCE...\n"
    )
}

/// Runs `cp` or `mv` in the shell process and returns the shell status.
pub(super) fn run(
    operation: FileOperation,
    arguments: &[String],
    context: &FileContext,
    stderr: &mut Vec<u8>,
) -> i32 {
    let invocation = match parse(arguments, operation) {
        Ok(Parsed::Help) => {
            stderr.extend_from_slice(help_text(operation, operation.name()).as_bytes());
            return 0;
        }
        Ok(Parsed::Run(invocation)) => invocation,
        Err(SyntaxError(message)) => {
            let _ = writeln!(
                stderr,
                "xshi: {}: {message}\n{}",
                operation.name(),
                usage(operation)
            );
            return 2;
        }
    };
    let mut foreground = match Foreground::install(invocation.verbose, !invocation.no_progress) {
        Ok(foreground) => foreground,
        Err(error) => {
            let _ = writeln!(stderr, "xshi: {}: {error}", operation.name());
            return 1;
        }
    };
    let result = execute(operation, &invocation, context, &mut foreground);
    let _ = foreground.finish();
    stderr.extend_from_slice(&foreground.take_diagnostics());
    if let Some(signal) = foreground.observed_interrupt() {
        if signal == libc::SIGTERM || signal == libc::SIGHUP {
            foreground.terminate(signal);
        }
        let _ = writeln!(
            stderr,
            "xshi: {}: cancelled: {} entries processed; {} source operands unfinished",
            operation.name(),
            foreground.progress().completed_files,
            foreground.unfinished_operands(),
        );
        return 130;
    }
    if foreground.output_failed() {
        let _ = writeln!(
            stderr,
            "xshi: {}: progress output failed",
            operation.name()
        );
        return 1;
    }
    match result {
        Ok(()) => {
            if foreground.progress().skipped_files > 0 {
                let _ = writeln!(
                    stderr,
                    "xshi: {}: skipped {} existing entries",
                    operation.name(),
                    foreground.progress().skipped_files
                );
            }
            0
        }
        Err(error) => {
            let _ = writeln!(stderr, "xshi: {}: {error}", operation.name());
            1
        }
    }
}

fn execute(
    operation: FileOperation,
    invocation: &Invocation,
    context: &FileContext,
    foreground: &mut Foreground,
) -> io::Result<()> {
    let engine = EngineOptions {
        cross_file_systems: invocation.cross_file_systems,
        jobs: invocation.jobs,
        durable: invocation.durable || operation == FileOperation::Move,
    };
    worker_count(&engine)?;
    let directory = open_directory(&context.cwd)?;
    let options = PolicyOptions {
        collision: invocation.collision,
        merge: invocation.merge,
        dry_run: invocation.dry_run,
    };
    let plan = preflight(
        directory.as_fd(),
        &PolicyRequest {
            operation: operation.operation(),
            operands: &invocation.operands,
            options,
            home: context.home.as_deref(),
        },
    )
    .map_err(path_error)?;
    if invocation.dry_run {
        for operand in &plan.operands {
            check_cancelled(foreground.cancellation())?;
            let line = match operand.action {
                Action::Skip => {
                    format!("skipped existing target {}", operand.destination.display())
                }
                Action::SameEntry => format!(
                    "{} and {} are the same entry",
                    operand.source.display(),
                    operand.destination.display()
                ),
                _ => format!(
                    "would {} {} to {}",
                    operation.verb(),
                    operand.source.display(),
                    operand.destination.display()
                ),
            };
            foreground.info(&format!("xshi: {}: {line}", operation.name()));
        }
        foreground.progress.discovered_files = plan.operands.len() as u64;
        return Ok(());
    }
    for index in 0..plan.operands.len() {
        check_cancelled(foreground.cancellation())?;
        match apply_operand(operation, &plan, index, &engine, options, context, foreground) {
            Ok(()) => foreground.complete_operand(),
            Err(error) => {
                if error.kind() != io::ErrorKind::Interrupted {
                    foreground.fail_operand();
                }
                return Err(error);
            }
        }
    }
    Ok(())
}

fn apply_operand(
    operation: FileOperation,
    plan: &Preflight,
    index: usize,
    engine: &EngineOptions,
    options: PolicyOptions,
    context: &FileContext,
    foreground: &mut Foreground,
) -> io::Result<()> {
    let operand = &plan.operands[index];
    let source_name = operand
        .source
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "source has no final name"))?;
    let source_parent = operand
        .source
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let source_file = open_directory(&context.cwd.join(source_parent))?;
    let destination_file: std::fs::File = match &plan.into_directory {
        Some(directory) => std::fs::File::from(directory.try_clone()?),
        None => {
            let parent = operand
                .destination
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            open_directory(&context.cwd.join(parent))?
        }
    };
    if observe_fd(source_file.as_fd())?.identity != operand.source_parent {
        return Err(io::Error::other("source parent changed after preflight"));
    }
    if observe_fd(destination_file.as_fd())?.identity != operand.destination_parent {
        return Err(io::Error::other(
            "destination parent changed after preflight",
        ));
    }
    let mut progress = Progress::default();
    let expected_destination = if operand.action == Action::Replace {
        operand.destination_observed
    } else {
        None
    };
    match operand.action {
        Action::Skip => {
            foreground.skip_operand();
            return Ok(());
        }
        Action::SameEntry => {
            foreground.skip_operand();
            foreground.info(&format!(
                "xshi: {}: {} and {} are the same entry",
                operation.name(),
                operand.source.display(),
                operand.destination.display()
            ));
            return Ok(());
        }
        Action::Merge => {
            merge_tree(MergeRequest {
                source_parent: source_file.as_fd(),
                source_name,
                destination_parent: destination_file.as_fd(),
                destination_name: &operand.destination_name,
                expected_source: operand.source_observed,
                expected_destination: operand
                    .destination_observed
                    .expect("merge destination was observed"),
                options,
                engine: *engine,
                cancellation: foreground.cancellation(),
                progress: &mut progress,
            })?;
            foreground.absorb(&progress);
            if operation == FileOperation::Move {
                retire_source(
                    source_file.as_fd(),
                    source_name,
                    operand.source_observed,
                    foreground,
                )?;
            }
        }
        Action::Transfer | Action::Replace => {
            publish_tree(
                PublishRequest {
                    source_parent: source_file.as_fd(),
                    source_name,
                    destination_parent: destination_file.as_fd(),
                    destination_name: &operand.destination_name,
                    expected_source: operand.source_observed,
                    expected_destination,
                    action: operand.action,
                    engine: *engine,
                    cancellation: foreground.cancellation(),
                    progress: &mut progress,
                },
                |_| Ok(()),
            )?;
            foreground.absorb(&progress);
            if operation == FileOperation::Move {
                retire_source(
                    source_file.as_fd(),
                    source_name,
                    operand.source_observed,
                    foreground,
                )?;
            }
        }
    }
    if foreground.verbose {
        let verb = match (operation, operand.action) {
            (FileOperation::Copy, _) => "copied",
            (FileOperation::Move, Action::Merge) => "merged",
            (FileOperation::Move, _) => "moved",
        };
        foreground.info(&format!(
            "xshi: {}: {verb} {} to {}",
            operation.name(),
            operand.source.display(),
            operand.destination.display()
        ));
    }
    Ok(())
}

/// Retires a moved source after its destination was published and verified.
fn retire_source(
    parent: BorrowedFd<'_>,
    name: &OsStr,
    expected: Observation,
    foreground: &mut Foreground,
) -> io::Result<()> {
    if expected.identity.kind == xsh_root::transfer::FileKind::Directory {
        let directory = xsh_root::transfer::open_entry_directory(parent, name)?;
        let plan = rmrf::plan_tree(
            name,
            expected.identity,
            directory.as_fd(),
            foreground.cancellation(),
        )?;
        if plan.was_cancelled() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "operation cancelled",
            ));
        }
        rmrf::retire_tree(&plan, parent, name, expected.identity)?;
    } else {
        rmrf::retire_leaf(parent, name, expected.identity)?;
    }
    Ok(())
}

fn path_error(error: xsh_root::transfer::PathError) -> io::Error {
    io::Error::new(
        error.error.kind(),
        format!("{}: {}", error.path.display(), error.error),
    )
}

pub(super) fn open_directory(path: &Path) -> io::Result<std::fs::File> {
    let flags = rustix::fs::OFlags::RDONLY
        .union(rustix::fs::OFlags::DIRECTORY)
        .union(rustix::fs::OFlags::CLOEXEC);
    let fd = rustix::fs::open(path, flags, rustix::fs::Mode::empty())?;
    Ok(std::fs::File::from(fd))
}

pub(super) fn escape_bytes(bytes: &[u8]) -> String {
    let mut escaped = String::with_capacity(bytes.len());
    for &byte in bytes {
        match byte {
            b' '..=b'~' if byte != b'\\' => escaped.push(char::from(byte)),
            b'\\' => escaped.push_str("\\\\"),
            _ => {
                use std::fmt::Write as _;
                let _ = write!(escaped, "\\x{byte:02x}");
            }
        }
    }
    escaped
}

pub(super) fn escape_os(text: &OsStr) -> String {
    escape_bytes(text.as_bytes())
}



static OBSERVED_SIGNAL: AtomicI32 = AtomicI32::new(0);

/// What a screen-facing command reports once its operation finishes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CancelReason {
    User,
    Output,
}

/// Terminal and signal state one foreground file command owns.
///
/// The process-wide dispositions xshi installed are remembered and restored on
/// drop. `SIGTSTP` stays ignored, so Ctrl+Z never suspends the shell process
/// while it is transferring. A `SIGINT` request maps to status 130; `SIGTERM`
/// and `SIGHUP` clean up and exit with `128 + signal`.
pub(super) struct Foreground {
    cancellation: Cancellation,
    progress: Progress,
    started: Instant,
    last_progress: Option<Instant>,
    progress_drawn: bool,
    progress_enabled: bool,
    progress_label: &'static str,
    verbose: bool,
    output_failure: bool,
    diagnostics: String,
    completed_operands: u64,
    unfinished_operands: u64,
    restored: RestoredDispositions,
}

struct RestoredDispositions {
    interrupt: libc::sigaction,
    terminate: libc::sigaction,
    hangup: libc::sigaction,
}

impl Foreground {
    pub(super) fn install(verbose: bool, progress_enabled: bool) -> io::Result<Self> {
        OBSERVED_SIGNAL.store(0, Ordering::Release);
        let restored = RestoredDispositions {
            interrupt: replace_disposition(libc::SIGINT)?,
            terminate: replace_disposition(libc::SIGTERM)?,
            hangup: replace_disposition(libc::SIGHUP)?,
        };
        Ok(Self {
            cancellation: Cancellation::default(),
            progress: Progress::default(),
            started: Instant::now(),
            last_progress: None,
            progress_drawn: false,
            progress_enabled,
            progress_label: "cp",
            verbose,
            output_failure: false,
            diagnostics: String::new(),
            completed_operands: 0,
            unfinished_operands: 0,
            restored,
        })
    }

    pub(super) fn cancellation(&self) -> &Cancellation {
        &self.cancellation
    }

    pub(super) fn cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    pub(super) fn verbose(&self) -> bool {
        self.verbose
    }

    pub(super) fn progress(&self) -> Progress {
        self.progress
    }

    pub(super) fn unfinished_operands(&self) -> u64 {
        self.unfinished_operands
    }

    pub(super) fn completed_files(&self) -> u64 {
        self.progress.completed_files
    }

    pub(super) fn output_failed(&self) -> bool {
        self.output_failure
    }

    pub(super) fn observed_interrupt(&self) -> Option<i32> {
        match OBSERVED_SIGNAL.load(Ordering::Acquire) {
            0 => None,
            signal => Some(signal),
        }
    }

    /// Restores the terminal and exits with the conventional signal status.
    pub(super) fn terminate(&self, signal: i32) -> ! {
        restore_termios();
        // SAFETY: `_exit` is async-signal-safe, never returns, and every owned
        // descriptor and journal session was already dropped or left for
        // recovery.
        unsafe { libc::_exit(128 + signal) }
    }

    fn complete_operand(&mut self) {
        self.completed_operands += 1;
    }

    /// Reports a journaled deletion counter line on a terminal.
    pub(super) fn report_deleting(&mut self, completed: usize, total: usize) {
        if !self.progress_enabled || !io::stderr().is_terminal() {
            return;
        }
        let fraction = if total == 0 { 1.0 } else { completed as f64 / total as f64 };
        let filled = ((fraction * 30.0) as usize).min(30);
        let line = format!(
            "Deleting [{}{}] {:>3}% {completed}/{total}",
            "#".repeat(filled),
            "-".repeat(30 - filled),
            (fraction * 100.0) as usize,
        );
        if write_progress_line(&line).is_err() {
            self.output_failure = true;
            self.cancellation.cancel_for_output();
            return;
        }
        self.progress_drawn = true;
    }

    fn fail_operand(&mut self) {
        self.unfinished_operands += 1;
    }

    fn skip_operand(&mut self) {
        self.progress.skipped_files += 1;
    }

    fn absorb(&mut self, progress: &Progress) {
        self.progress.discovered_files += progress.discovered_files;
        self.progress.completed_files += progress.completed_files;
        self.progress.skipped_files += progress.skipped_files;
        self.progress.failed_files += progress.failed_files;
        self.progress.logical_bytes += progress.logical_bytes;
        self.progress.streamed_bytes += progress.streamed_bytes;
        self.progress.hole_bytes += progress.hole_bytes;
        self.progress.cloned_files += progress.cloned_files;
        self.draw_progress();
    }

    /// Records one completed or planned operand for the caller's diagnostic
    /// stream, which the shell redirects.
    pub(super) fn info(&mut self, line: &str) {
        self.diagnostics.push_str(line);
        self.diagnostics.push('\n');
    }

    /// Takes the diagnostics recorded for the caller's stream.
    pub(super) fn take_diagnostics(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.diagnostics).into_bytes()
    }

    /// Draws the transfer progress line once terminal output is due.
    fn draw_progress(&mut self) {
        if !self.progress_enabled || !io::stderr().is_terminal() {
            return;
        }
        let now = Instant::now();
        if now.saturating_duration_since(self.started) < PROGRESS_DELAY
            || self
                .last_progress
                .is_some_and(|last| now.saturating_duration_since(last) < PROGRESS_INTERVAL)
        {
            return;
        }
        let line = format!(
            "{}: {}/{} files, {} bytes streamed, {} bytes cloned",
            self.progress_label,
            self.progress.completed_files,
            self.progress.discovered_files,
            self.progress.streamed_bytes,
            self.progress.logical_bytes,
        );
        if write_progress_line(&line).is_err() {
            self.output_failure = true;
            self.cancellation.cancel_for_output();
            return;
        }
        self.progress_drawn = true;
        self.last_progress = Some(now);
    }

    pub(super) fn set_progress_label(&mut self, label: &'static str) {
        self.progress_label = label;
    }

    pub(super) fn clear_progress(&mut self) -> io::Result<()> {
        if !self.progress_drawn || !io::stderr().is_terminal() {
            return Ok(());
        }
        self.progress_drawn = false;
        if let Err(error) = io::stderr().write_all(b"\r\x1b[2K").and_then(|()| io::stderr().flush())
        {
            self.output_failure = true;
            self.cancellation.cancel_for_output();
            return Err(error);
        }
        Ok(())
    }

    pub(super) fn finish(&mut self) -> io::Result<()> {
        self.clear_progress()
    }
}

impl Drop for Foreground {
    fn drop(&mut self) {
        self.clear_progress().ok();
        restore_disposition(libc::SIGINT, &self.restored.interrupt);
        restore_disposition(libc::SIGTERM, &self.restored.terminate);
        restore_disposition(libc::SIGHUP, &self.restored.hangup);
    }
}

fn write_progress_line(line: &str) -> io::Result<()> {
    let mut stderr = io::stderr().lock();
    stderr.write_all(b"\r\x1b[2K")?;
    stderr.write_all(truncate_bytes(line, PROGRESS_LINE_BYTES))?;
    stderr.flush()
}

fn truncate_bytes(text: &str, limit: usize) -> &[u8] {
    let bytes = text.as_bytes();
    let mut end = bytes.len().min(limit);
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    &bytes[..end]
}

fn replace_disposition(signal: i32) -> io::Result<libc::sigaction> {
    // SAFETY: both sigaction structs are zero-initialized plain data owned by
    // this frame, and the handler only stores an atomic.
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = foreground_signal as *const () as usize;
        action.sa_flags = libc::SA_RESTART;
        libc::sigemptyset(&mut action.sa_mask);
        let mut previous: libc::sigaction = std::mem::zeroed();
        if libc::sigaction(signal, &action, &mut previous) != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(previous)
    }
}

fn restore_disposition(signal: i32, previous: &libc::sigaction) {
    // SAFETY: `previous` was filled by sigaction for the same signal.
    unsafe {
        libc::sigaction(signal, previous, std::ptr::null_mut());
    }
}

extern "C" fn foreground_signal(signal: i32) {
    OBSERVED_SIGNAL.store(signal, Ordering::Release);
}

fn restore_termios() {
    if let Ok(termios) = super::term::save_termios() {
        // SAFETY: the shell owns standard input's terminal and the captured
        // termios came from the same descriptor.
        unsafe {
            let stdin = std::os::fd::BorrowedFd::borrow_raw(super::term::STDIN_FD);
            let _ = rustix::termios::tcsetattr(
                stdin,
                rustix::termios::OptionalActions::Now,
                &termios,
            );
        }
    }
}
