//! The `cp` and `mv` builtins: ish's option grammar, operand mapping,
//! per-operand outcomes and diagnostics over XSH's internal transfer engine.

use std::ffi::{OsStr, OsString};
use std::io::{self, Write as _};
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::os::unix::ffi::OsStrExt as _;
use std::path::{Path, PathBuf};

use rustix::fs::{self as rfs, Mode, OFlags, RenameFlags};
use xsh_root::FileKind;
use xsh_root::transfer::{
    Action, CancelReason, Cancellation, Collision, EngineOptions, Jobs, LiveProgress, MergeRequest,
    Observation,
    Operands, Operation, PlannedOperand, PolicyOptions, PolicyRequest, Progress, Publication,
    PublishRequest, Removal, Retirement, Session, dry_run, into_basename, merge_tree,
    observe_entry, observe_fd, preflight, publish_tree, worker_count,
};

use super::foreground::{self, EventSink, Foreground, Sink, Streams};
use super::rmrf;

const DIRECTORY: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::CLOEXEC);

/// The paths a native file command resolves against.
pub(super) struct FileContext {
    /// The shell's working directory.
    pub(super) cwd: PathBuf,
    /// `HOME` from the command's environment.
    pub(super) home: Option<PathBuf>,
    /// `TMPDIR` from the command's environment, or `/tmp`.
    pub(super) scratch: PathBuf,
}

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

#[derive(Clone, Copy, Debug, Default)]
struct Options {
    collision: Collision,
    merge: bool,
    dry_run: bool,
    durable: bool,
    cross_file_systems: bool,
    jobs: Option<Jobs>,
}

struct Invocation {
    operands: Operands,
    options: Options,
    verbose: bool,
    no_progress: bool,
}

enum Parsed {
    Help,
    Run(Invocation),
}

#[derive(Debug)]
struct SyntaxError(String);

fn parse(arguments: &[String], operation: FileOperation) -> Result<Parsed, SyntaxError> {
    let mut options = Options::default();
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
            "--merge" => options.merge = true,
            "--replace" => {
                options.collision = Collision::Replace;
                replace_seen = true;
            }
            "-n" | "--no-clobber" => {
                options.collision = Collision::Skip;
                no_clobber_seen = true;
            }
            "--dry-run" => options.dry_run = true,
            "--durable" => options.durable = true,
            "--cross-file-systems" => options.cross_file_systems = true,
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
                options.jobs = Some(
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
                    source: operands.next().expect("two operands"),
                    destination: operands.next().expect("two operands"),
                }
            } else {
                let directory = operands.pop().expect("three or more operands");
                let mut sources = operands.into_iter();
                Operands::Into {
                    directory,
                    first: sources.next().expect("two or more sources"),
                    rest: sources.collect(),
                }
            }
        }
    };

    Ok(Parsed::Run(Invocation {
        operands,
        options,
        verbose,
        no_progress,
    }))
}

fn is_option(argument: &str) -> bool {
    argument.starts_with('-') && argument != "-"
}

fn usage(operation: FileOperation) -> String {
    let name = operation.name();
    format!(
        "usage: {name} [OPTIONS] SOURCE DESTINATION\n       {name} [OPTIONS] SOURCE... DIRECTORY\n       {name} [OPTIONS] --into DIRECTORY SOURCE...\n"
    )
}

/// The `--help` text of `cp` or `mv`.
pub(super) fn help(operation: FileOperation) -> String {
    let recursion = match operation {
        FileOperation::Copy => {
            "  -r, -R                   Accepted; directory recursion is automatic\n"
        }
        FileOperation::Move => "",
    };
    format!(
        "{}\n\nOPTIONS:\n{recursion}  --merge                   Merge directory contents\n  --replace                 Replace supported leaf targets\n  -n, --no-clobber           Skip existing targets\n  --dry-run                 Report planned actions\n  --durable                 Synchronize published copy data\n  --cross-file-systems      Traverse nested file systems\n  -j, --jobs N              Use N transfer workers (1..=32)\n  -v, --verbose              Report completed operations\n  --no-progress             Suppress terminal progress\n  -h, --help                 Show this help\n",
        usage(operation).trim_end(),
    )
}

/// What became of one source operand.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OperandState {
    Planned,
    Skipped,
    SameEntry,
    Copied,
    Moved,
    FailedBeforePublication,
    DestinationPublishedSourceRetained,
    DestinationPartiallyPublished,
    SourcePartiallyRetired,
    SourceRemovedSyncFailed,
    Cancelled,
}

#[derive(Debug)]
struct OperandOutcome {
    source: PathBuf,
    destination: PathBuf,
    state: OperandState,
}

#[derive(Debug)]
enum Event {
    Progress(Progress),
    Operand(OperandOutcome),
    Diagnostic { path: PathBuf, message: String },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Completion {
    Complete,
    Failed,
    InvalidRequest,
    Cancelled,
}

struct Outcome {
    progress: Progress,
    completion: Completion,
}

impl Outcome {
    fn status(&self) -> i32 {
        match self.completion {
            Completion::Complete => 0,
            Completion::Failed => 1,
            Completion::InvalidRequest => 2,
            Completion::Cancelled => 130,
        }
    }
}

struct Failure {
    state: OperandState,
    error: io::Error,
}

impl From<io::Error> for Failure {
    fn from(error: io::Error) -> Self {
        Self {
            state: OperandState::FailedBeforePublication,
            error,
        }
    }
}

impl From<rustix::io::Errno> for Failure {
    fn from(error: rustix::io::Errno) -> Self {
        io::Error::from(error).into()
    }
}

/// Runs `cp` or `mv` in the shell process and returns the shell status.
pub(super) fn run(
    operation: FileOperation,
    arguments: &[String],
    context: &FileContext,
    streams: &mut Streams<'_>,
) -> i32 {
    let status = run_command(operation, arguments, context, streams);
    foreground::exit_if_terminated(streams);
    status
}

fn run_command(
    operation: FileOperation,
    arguments: &[String],
    context: &FileContext,
    streams: &mut Streams<'_>,
) -> i32 {
    let name = operation.name();
    let invocation = match parse(arguments, operation) {
        Ok(Parsed::Help) => {
            return if streams.stdout.write_all(help(operation).as_bytes()).is_ok() {
                0
            } else {
                1
            };
        }
        Ok(Parsed::Run(invocation)) => invocation,
        Err(SyntaxError(message)) => {
            let text = format!("xshi: {name}: {message}\n{}", usage(operation));
            return if streams.stderr.write_all(text.as_bytes()).is_ok() {
                2
            } else {
                1
            };
        }
    };

    let mut foreground = match Foreground::start(streams.progress_terminal()) {
        Ok(foreground) => foreground,
        Err(error) => {
            let _ = writeln!(streams.stderr, "xshi: {name}: {error}");
            return 1;
        }
    };
    let cancellation = foreground.cancellation();
    let verbose = invocation.verbose;
    let no_progress = invocation.no_progress;
    let dry_run = invocation.options.dry_run;
    let live = LiveProgress::default();
    let reported = std::cell::Cell::new(Progress::default());
    let shown = std::cell::Cell::new(Progress::default());
    let mut retained_operands = 0;
    let stderr = &mut streams.stderr;
    let result = foreground.run(
        |cancellation, events| execute(operation, &invocation, context, cancellation, events, &live),
        |foreground, event| {
            if let Event::Progress(progress) = &event {
                let current = merge_progress(
                    merge_progress(reported.get(), *progress),
                    live.snapshot(),
                );
                reported.set(current);
                shown.set(current);
                return render_progress(foreground, operation, no_progress, current);
            }
            if let Event::Operand(outcome) = &event
                && matches!(
                    outcome.state,
                    OperandState::Cancelled
                        | OperandState::FailedBeforePublication
                        | OperandState::DestinationPublishedSourceRetained
                        | OperandState::DestinationPartiallyPublished
                        | OperandState::SourcePartiallyRetired
                )
            {
                retained_operands += 1;
            }
            render_event(stderr, operation, verbose, dry_run, event)
        },
        |foreground| {
            let current = merge_progress(reported.get(), live.snapshot());
            if current == shown.get() {
                return Ok(());
            }
            shown.set(current);
            foreground.note_activity();
            render_progress(foreground, operation, no_progress, current)
        },
    );
    let completed_transfers = merge_progress(reported.get(), live.snapshot()).completed_files;
    let finish = foreground.finish();
    drop(foreground);

    if cancellation.reason() == Some(CancelReason::User) {
        let _ = writeln!(
            streams.stderr,
            "xshi: {name}: cancelled: {completed_transfers} entries processed; {retained_operands} source operands unfinished"
        );
        foreground::cancel_line();
        return 130;
    }
    let outcome = match (result, finish) {
        (Ok(outcome), Ok(())) => outcome,
        (Err(error), Err(finish_error)) => {
            let _ = writeln!(
                streams.stderr,
                "xshi: {name}: {error}; foreground cleanup failed: {finish_error}"
            );
            return 1;
        }
        (Err(error), Ok(())) | (Ok(_), Err(error)) => {
            let _ = writeln!(streams.stderr, "xshi: {name}: {error}");
            return 1;
        }
    };
    if outcome.progress.skipped_files > 0
        && writeln!(
            streams.stderr,
            "xshi: {name}: skipped {} existing entries",
            outcome.progress.skipped_files
        )
        .is_err()
    {
        cancellation.cancel_for_output();
        return 1;
    }
    outcome.status()
}

/// The field-wise maximum of two snapshots of the same counters.
fn merge_progress(current: Progress, newer: Progress) -> Progress {
    Progress {
        discovered_files: current.discovered_files.max(newer.discovered_files),
        completed_files: current.completed_files.max(newer.completed_files),
        skipped_files: current.skipped_files.max(newer.skipped_files),
        failed_files: current.failed_files.max(newer.failed_files),
        logical_bytes: current.logical_bytes.max(newer.logical_bytes),
        streamed_bytes: current.streamed_bytes.max(newer.streamed_bytes),
        hole_bytes: current.hole_bytes.max(newer.hole_bytes),
        cloned_files: current.cloned_files.max(newer.cloned_files),
        synced_files: current.synced_files.max(newer.synced_files),
        synced_directories: current.synced_directories.max(newer.synced_directories),
    }
}

fn render_progress(
    foreground: &mut Foreground,
    operation: FileOperation,
    no_progress: bool,
    progress: Progress,
) -> io::Result<()> {
    if no_progress {
        return foreground.clear_progress();
    }
    let cloned = if progress.cloned_files > 0 {
        progress.hole_bytes
    } else {
        0
    };
    foreground.report_progress_line(&format!(
        "{}: {}/{} files, {} bytes streamed, {cloned} bytes cloned",
        operation.name(),
        progress.completed_files,
        progress.discovered_files,
        progress.streamed_bytes,
    ))
}

fn render_event(
    stderr: &mut Sink<'_>,
    operation: FileOperation,
    verbose: bool,
    dry_run: bool,
    event: Event,
) -> io::Result<()> {
    let label = operation.name();
    match event {
        Event::Progress(_) => Ok(()),
        Event::Diagnostic { path, message } => writeln!(
            stderr,
            "xshi: {label}: {}: {}",
            escape_bytes(path.as_os_str().as_bytes()),
            escape_bytes(message.as_bytes())
        ),
        Event::Operand(outcome) => {
            if verbose || dry_run {
                if let Some(message) = render_operand(operation, &outcome, dry_run) {
                    writeln!(stderr, "xshi: {label}: {message}")?;
                }
            } else if matches!(
                outcome.state,
                OperandState::DestinationPublishedSourceRetained
                    | OperandState::DestinationPartiallyPublished
                    | OperandState::SourcePartiallyRetired
                    | OperandState::SourceRemovedSyncFailed
                    | OperandState::Cancelled
            ) {
                writeln!(stderr, "xshi: {label}: {}", render_partial(&outcome))?;
            }
            Ok(())
        }
    }
}

fn render_operand(
    operation: FileOperation,
    outcome: &OperandOutcome,
    dry_run: bool,
) -> Option<String> {
    let source = escape_bytes(outcome.source.as_os_str().as_bytes());
    let destination = escape_bytes(outcome.destination.as_os_str().as_bytes());
    let action = match outcome.state {
        OperandState::Planned if dry_run => {
            return Some(format!(
                "would {} {source} to {destination}",
                operation.verb()
            ));
        }
        OperandState::Copied if operation == FileOperation::Copy => "copied",
        OperandState::Moved if operation == FileOperation::Move => "moved",
        OperandState::SameEntry => {
            return Some(format!("{source} and {destination} are the same entry"));
        }
        OperandState::Skipped => return Some(format!("skipped existing target {destination}")),
        OperandState::DestinationPublishedSourceRetained
        | OperandState::DestinationPartiallyPublished
        | OperandState::SourcePartiallyRetired
        | OperandState::SourceRemovedSyncFailed
        | OperandState::Cancelled => return Some(render_partial(outcome)),
        OperandState::Planned
        | OperandState::Copied
        | OperandState::Moved
        | OperandState::FailedBeforePublication => return None,
    };
    Some(format!("{action} {source} to {destination}"))
}

fn render_partial(outcome: &OperandOutcome) -> String {
    let source = escape_bytes(outcome.source.as_os_str().as_bytes());
    let destination = escape_bytes(outcome.destination.as_os_str().as_bytes());
    match outcome.state {
        OperandState::DestinationPublishedSourceRetained => {
            format!("published {destination}; retained source {source}")
        }
        OperandState::DestinationPartiallyPublished => {
            format!("partially published {destination}; retained source {source}")
        }
        OperandState::SourcePartiallyRetired => {
            format!("partially retired source {source}; inspect retained entries")
        }
        OperandState::SourceRemovedSyncFailed => {
            format!("removed source {source}; source parent sync failed")
        }
        OperandState::Cancelled => format!("cancelled; retained source {source}"),
        _ => String::new(),
    }
}

/// Escapes terminal controls, invalid bytes and backslashes in a rendered
/// path or message.
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

/// One operand path resolved to an open parent directory and final name.
struct Root {
    parent: OwnedFd,
    name: OsString,
    observed: Option<Observation>,
    path: PathBuf,
}

impl Root {
    fn establish(cwd: BorrowedFd<'_>, path: &Path, may_be_absent: bool) -> io::Result<Self> {
        let bytes = path.as_os_str().as_bytes();
        if bytes.is_empty() || bytes.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "path is empty or contains NUL",
            ));
        }
        let (parent_path, name) = match bytes.iter().rposition(|byte| *byte != b'/') {
            None => (Path::new("/"), OsStr::new(".")),
            Some(last) => {
                let stripped = &bytes[..=last];
                match stripped.iter().rposition(|byte| *byte == b'/') {
                    Some(0) => (Path::new("/"), OsStr::from_bytes(&stripped[1..])),
                    Some(slash) => (
                        Path::new(OsStr::from_bytes(&stripped[..slash])),
                        OsStr::from_bytes(&stripped[slash + 1..]),
                    ),
                    None => (Path::new("."), OsStr::from_bytes(stripped)),
                }
            }
        };
        let parent = rfs::openat(cwd, parent_path, DIRECTORY, Mode::empty())?;
        Self::from_parent(parent, name, path, may_be_absent)
    }

    fn establish_at(parent: BorrowedFd<'_>, name: &OsStr, path: &Path) -> io::Result<Self> {
        let bytes = name.as_bytes();
        if bytes.is_empty()
            || bytes.contains(&0)
            || bytes.contains(&b'/')
            || bytes == b"."
            || bytes == b".."
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "entry name is not a single path component",
            ));
        }
        let parent = rfs::openat(parent, ".", DIRECTORY, Mode::empty())?;
        Self::from_parent(parent, name, path, true)
    }

    fn from_parent(
        parent: OwnedFd,
        name: &OsStr,
        path: &Path,
        may_be_absent: bool,
    ) -> io::Result<Self> {
        let observed = match observe_entry(parent.as_fd(), name) {
            Ok(observed) => Some(observed),
            Err(error) if error.kind() == io::ErrorKind::NotFound && may_be_absent => None,
            Err(error) => return Err(error),
        };
        Ok(Self {
            parent,
            name: name.to_os_string(),
            observed,
            path: path.to_path_buf(),
        })
    }

    fn open_directory(&self) -> io::Result<OwnedFd> {
        let expected = self.observed.ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "directory root is absent")
        })?;
        if expected.identity.kind != FileKind::Directory {
            return Err(io::Error::new(
                io::ErrorKind::NotADirectory,
                "root is not a directory",
            ));
        }
        let directory = rfs::openat(
            &self.parent,
            &self.name,
            DIRECTORY.union(OFlags::NOFOLLOW),
            Mode::empty(),
        )?;
        if observe_fd(directory.as_fd())?.identity != expected.identity {
            return Err(io::Error::other("directory identity changed"));
        }
        Ok(directory)
    }
}

/// The shared inputs of one command, borrowed by the worker thread.
struct Job<'a> {
    operation: FileOperation,
    options: Options,
    context: &'a FileContext,
    cwd: BorrowedFd<'a>,
    cancellation: &'a Cancellation,
    events: &'a EventSink<Event>,
    live: &'a LiveProgress,
    track_renamed_trees: bool,
}

impl Job<'_> {
    fn emit(&self, event: Event) {
        if self.events.emit(event).is_err() {
            self.cancellation.cancel_for_output();
        }
    }

    fn engine(&self, durable: bool) -> EngineOptions {
        EngineOptions {
            cross_file_systems: self.options.cross_file_systems,
            jobs: self.options.jobs,
            durable,
        }
    }

    fn policy(&self) -> PolicyOptions {
        PolicyOptions {
            collision: self.options.collision,
            merge: self.options.merge,
            dry_run: self.options.dry_run,
        }
    }

    /// Opens the scratch directory a staged tree transfer keeps its records
    /// in, so a missing `TMPDIR` fails the operand before any mutation.
    fn require_scratch(&self) -> io::Result<()> {
        let scratch = if self.context.scratch.is_absolute() {
            self.context.scratch.clone()
        } else {
            self.context.cwd.join(&self.context.scratch)
        };
        rfs::open(&scratch, DIRECTORY, Mode::empty())?;
        Ok(())
    }
}

fn early_error(
    cancellation: &Cancellation,
    events: &EventSink<Event>,
    path: &Path,
    error: io::Error,
) -> Outcome {
    let mut completion = if error.kind() == io::ErrorKind::InvalidInput {
        Completion::InvalidRequest
    } else {
        Completion::Failed
    };
    let diagnostic = Event::Diagnostic {
        path: path.to_path_buf(),
        message: error.to_string(),
    };
    if events.emit(diagnostic).is_err() {
        cancellation.cancel_for_output();
    }
    if cancellation.reason() == Some(CancelReason::User) {
        completion = Completion::Cancelled;
    }
    Outcome {
        progress: Progress::default(),
        completion,
    }
}

fn execute(
    operation: FileOperation,
    invocation: &Invocation,
    context: &FileContext,
    cancellation: &Cancellation,
    events: &EventSink<Event>,
    live: &LiveProgress,
) -> Outcome {
    let options = invocation.options;
    let cwd = match rfs::open(&context.cwd, DIRECTORY, Mode::empty()) {
        Ok(cwd) => cwd,
        Err(error) => return early_error(cancellation, events, &context.cwd, error.into()),
    };
    let engine = EngineOptions {
        cross_file_systems: options.cross_file_systems,
        jobs: options.jobs,
        durable: options.durable,
    };
    if let Err(error) = worker_count(&engine) {
        return early_error(cancellation, events, &context.cwd, error);
    }
    if !options.dry_run
        && let Err((path, error)) = check_recovery_roots(&invocation.operands, cwd.as_fd())
    {
        return early_error(cancellation, events, &path, error);
    }
    let plan = match preflight(
        cwd.as_fd(),
        &PolicyRequest {
            operation: operation.operation(),
            operands: &invocation.operands,
            options: PolicyOptions {
                collision: options.collision,
                merge: options.merge,
                dry_run: options.dry_run,
            },
            home: context.home.as_deref(),
        },
    ) {
        Ok(plan) => plan,
        Err(error) => return early_error(cancellation, events, &error.path, error.error),
    };
    let job = Job {
        operation,
        options,
        context,
        cwd: cwd.as_fd(),
        cancellation,
        events,
        live,
        track_renamed_trees: options.merge
            || matches!(&invocation.operands, Operands::Into { rest, .. } if !rest.is_empty()),
    };
    let mut completion = Completion::Complete;
    let mut progress = Progress::default();
    for operand in &plan.operands {
        if cancellation.is_cancelled() {
            job.emit(Event::Operand(OperandOutcome {
                source: operand.source.clone(),
                destination: operand.destination.clone(),
                state: OperandState::Cancelled,
            }));
            continue;
        }
        let into = plan.into_directory.as_ref().map(AsFd::as_fd);
        let state = match execute_operand(&job, operand, into, &mut progress) {
            Ok(state) => state,
            Err(failure) => {
                completion = Completion::Failed;
                progress.failed_files += 1;
                let message = if cancellation.is_cancelled()
                    && failure.error.kind() == io::ErrorKind::Interrupted
                {
                    "operation cancelled".to_owned()
                } else {
                    failure.error.to_string()
                };
                job.emit(Event::Diagnostic {
                    path: operand.source.clone(),
                    message,
                });
                if cancellation.is_cancelled()
                    && failure.state == OperandState::FailedBeforePublication
                {
                    OperandState::Cancelled
                } else {
                    failure.state
                }
            }
        };
        job.emit(Event::Operand(OperandOutcome {
            source: operand.source.clone(),
            destination: operand.destination.clone(),
            state,
        }));
        job.emit(Event::Progress(progress));
    }
    match cancellation.reason() {
        Some(CancelReason::User) => completion = Completion::Cancelled,
        Some(CancelReason::Output) => completion = Completion::Failed,
        None => {}
    }
    Outcome {
        progress,
        completion,
    }
}

fn check_recovery_roots(operands: &Operands, cwd: BorrowedFd<'_>) -> Result<(), (PathBuf, io::Error)> {
    let check = |path: &Path| -> Result<(), (PathBuf, io::Error)> {
        let root = Root::establish(cwd, path, true).map_err(|error| (path.to_path_buf(), error))?;
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        Session::check_recovery(root.parent.as_fd())
            .map_err(|error| (parent.to_path_buf(), error))?;
        if root
            .observed
            .is_some_and(|observed| observed.identity.kind == FileKind::Directory)
        {
            let directory = root
                .open_directory()
                .map_err(|error| (path.to_path_buf(), error))?;
            Session::check_recovery(directory.as_fd())
                .map_err(|error| (path.to_path_buf(), error))?;
        }
        Ok(())
    };
    match operands {
        Operands::Exact {
            source,
            destination,
        } => {
            check(source)?;
            check(destination)
        }
        Operands::Into {
            directory,
            first,
            rest,
        } => {
            into_basename(first).map_err(|error| (first.clone(), error))?;
            for source in rest {
                into_basename(source).map_err(|error| (source.clone(), error))?;
            }
            check(directory)?;
            check(first)?;
            for source in rest {
                check(source)?;
            }
            Ok(())
        }
    }
}

fn execute_operand(
    job: &Job<'_>,
    operand: &PlannedOperand,
    into: Option<BorrowedFd<'_>>,
    progress: &mut Progress,
) -> Result<OperandState, Failure> {
    job.cancellation.check()?;
    match operand.action {
        Action::Skip => {
            progress.skipped_files += 1;
            return Ok(OperandState::Skipped);
        }
        Action::SameEntry => return Ok(OperandState::SameEntry),
        Action::Transfer | Action::Replace | Action::Merge => {}
    }
    let source = Root::establish(job.cwd, &operand.source, false)?;
    let destination = match into {
        Some(parent) => Root::establish_at(parent, &operand.destination_name, &operand.destination)?,
        None => Root::establish(job.cwd, &operand.destination, true)?,
    };
    if observe_fd(source.parent.as_fd())?.identity != operand.source_parent
        || observe_fd(destination.parent.as_fd())?.identity != operand.destination_parent
    {
        return Err(io::Error::other("operand parent identity changed after preflight").into());
    }
    let source_matches = source
        .observed
        .is_some_and(|observed| operand.source_observed.same_version(observed));
    let destination_matches = match operand.destination_observed {
        Some(expected) => destination
            .observed
            .is_some_and(|observed| expected.same_version(observed)),
        None => destination.observed.is_none(),
    };
    if !source_matches || !destination_matches {
        return Err(io::Error::other("operand changed after preflight").into());
    }
    if job.options.dry_run {
        return plan_dry_run(job, operand, &source, &destination, progress).map_err(Into::into);
    }
    if job.operation == FileOperation::Move && operand.action != Action::Merge {
        match rename_object(job, &source, &destination, operand.action, progress) {
            Ok(state) => return Ok(state),
            Err(failure) if failure.error.raw_os_error() == Some(libc::EXDEV) => {}
            Err(failure) => return Err(failure),
        }
    }
    let observed = operand.source_observed;
    if observed.identity.kind == FileKind::Directory {
        job.require_scratch()?;
    }
    let engine = job.engine(job.options.durable || job.operation == FileOperation::Move);
    if operand.action == Action::Merge {
        let expected_destination = operand
            .destination_observed
            .ok_or_else(|| io::Error::other("merge destination was not observed"))?;
        merge_tree(MergeRequest {
            source_parent: source.parent.as_fd(),
            source_name: &source.name,
            destination_parent: destination.parent.as_fd(),
            destination_name: &destination.name,
            expected_source: observed,
            expected_destination,
            options: job.policy(),
            engine,
            cancellation: job.cancellation,
            progress,
            live: Some(job.live),
        })?;
    } else {
        let expected_destination = if operand.action == Action::Replace {
            operand.destination_observed
        } else {
            None
        };
        publish_tree(
            PublishRequest {
                source_parent: source.parent.as_fd(),
                source_name: &source.name,
                destination_parent: destination.parent.as_fd(),
                destination_name: &destination.name,
                expected_source: observed,
                expected_destination,
                action: operand.action,
                engine,
                cancellation: job.cancellation,
                progress,
                live: Some(job.live),
            },
            |_| Ok(()),
        )?;
    }
    if job.operation == FileOperation::Copy {
        return Ok(OperandState::Copied);
    }
    retire_moved_source(job, &source, &destination, observed)
}

fn plan_dry_run(
    job: &Job<'_>,
    operand: &PlannedOperand,
    source: &Root,
    destination: &Root,
    progress: &mut Progress,
) -> io::Result<OperandState> {
    let mut plan = Vec::new();
    dry_run(
        source.parent.as_fd(),
        &source.name,
        destination.parent.as_fd(),
        &destination.name,
        &job.policy(),
        &job.engine(false),
        job.cancellation,
        &mut plan,
    )?;
    if operand.source_observed.identity.kind != FileKind::Directory {
        progress.discovered_files += 1;
        return Ok(OperandState::Planned);
    }
    for entry in plan.into_iter().skip(1) {
        progress.discovered_files += 1;
        if entry.action == Action::Skip {
            progress.skipped_files += 1;
        }
        let source_relative = entry.source.strip_prefix(&source.name).unwrap_or(&entry.source);
        let destination_relative = entry
            .destination
            .strip_prefix(&destination.name)
            .unwrap_or(&entry.destination);
        job.emit(Event::Operand(OperandOutcome {
            source: operand.source.join(source_relative),
            destination: operand.destination.join(destination_relative),
            state: if entry.action == Action::Skip {
                OperandState::Skipped
            } else {
                OperandState::Planned
            },
        }));
    }
    Ok(OperandState::Planned)
}

fn require_unchanged(parent: BorrowedFd<'_>, name: &OsStr, expected: Observation) -> io::Result<()> {
    if !expected.same_version(observe_entry(parent, name)?) {
        return Err(io::Error::other("source changed before rename"));
    }
    Ok(())
}

/// Moves an entry within one file system by renaming it, as ish does before
/// falling back to a staged copy on `EXDEV`.
fn rename_object(
    job: &Job<'_>,
    source: &Root,
    destination: &Root,
    action: Action,
    progress: &mut Progress,
) -> Result<OperandState, Failure> {
    job.cancellation.check()?;
    let observed = source
        .observed
        .ok_or_else(|| io::Error::other("rename source missing"))?;
    if observed.identity.kind == FileKind::Directory && job.track_renamed_trees {
        job.require_scratch()?;
    }
    require_unchanged(source.parent.as_fd(), &source.name, observed)?;
    match action {
        Action::Replace => {
            let expected = destination
                .observed
                .ok_or_else(|| io::Error::other("replacement target is absent"))?;
            replace_by_exchange(source, destination, observed, expected).map_err(|error| {
                let published = observe_entry(destination.parent.as_fd(), &destination.name)
                    .is_ok_and(|actual| actual.identity == observed.identity);
                Failure {
                    state: if published {
                        OperandState::SourcePartiallyRetired
                    } else {
                        OperandState::FailedBeforePublication
                    },
                    error,
                }
            })?;
        }
        Action::Transfer => rfs::renameat_with(
            &source.parent,
            &source.name,
            &destination.parent,
            &destination.name,
            RenameFlags::NOREPLACE,
        )?,
        Action::Merge | Action::Skip | Action::SameEntry => {
            return Err(io::Error::other("invalid rename action").into());
        }
    }
    let partial = |error| Failure {
        state: OperandState::SourcePartiallyRetired,
        error,
    };
    let actual = observe_entry(destination.parent.as_fd(), &destination.name).map_err(partial)?;
    let mut renamed = observed;
    renamed.ctime = actual.ctime;
    if !renamed.same_version(actual) {
        return Err(partial(io::Error::other("renamed object identity changed")));
    }
    progress.discovered_files += 1;
    progress.completed_files += 1;
    if job.options.durable {
        rfs::fsync(&destination.parent)
            .and_then(|()| rfs::fsync(&source.parent))
            .map_err(|error| Failure {
                state: OperandState::SourceRemovedSyncFailed,
                error: error.into(),
            })?;
    }
    Ok(OperandState::Moved)
}

/// Exchanges a moved leaf with the existing destination it replaces, then
/// retires the displaced destination through the journal.
fn replace_by_exchange(
    source: &Root,
    destination: &Root,
    observed: Observation,
    displaced: Observation,
) -> io::Result<()> {
    let session = Session::start(source.parent.as_fd())?;
    let request = Retirement::new(Vec::new(), source.name.clone(), displaced.identity)?;
    let publication = Publication::new(
        observed.identity,
        observe_fd(destination.parent.as_fd())?.identity,
        destination.name.clone(),
    )?;
    let result = session.remove_after_publication(
        source.parent.as_fd(),
        request,
        publication,
        || {
            rfs::renameat_with(
                &source.parent,
                &source.name,
                &destination.parent,
                &destination.name,
                RenameFlags::EXCHANGE,
            )
            .map_err(Into::into)
        },
        |_, _, _| Ok(()),
    );
    let finish = session.finish();
    match (result, finish) {
        (Ok(Removal::Removed), Ok(())) => Ok(()),
        (Ok(Removal::AlreadyAbsent | Removal::Changed), _) => Err(io::Error::other(
            "destination changed during replacement",
        )),
        (Err(error), _) => Err(error.error),
        (Ok(Removal::Removed), Err(error)) => Err(error),
    }
}

/// Retires a moved source once its copy is published at the destination.
fn retire_moved_source(
    job: &Job<'_>,
    source: &Root,
    destination: &Root,
    observed: Observation,
) -> Result<OperandState, Failure> {
    let retained = |error| Failure {
        state: OperandState::DestinationPublishedSourceRetained,
        error,
    };
    job.cancellation.check().map_err(retained)?;
    if observed.identity.kind == FileKind::Directory {
        return match rmrf::retire_moved_tree(
            source.parent.as_fd(),
            &source.name,
            observed.identity,
            destination.parent.as_fd(),
            &destination.name,
            job.cancellation,
        ) {
            Ok(()) => Ok(OperandState::Moved),
            Err(failure) => Err(Failure {
                state: if failure.removed == 0 {
                    OperandState::DestinationPublishedSourceRetained
                } else {
                    OperandState::SourcePartiallyRetired
                },
                error: failure.error,
            }),
        };
    }
    let session = Session::start(source.parent.as_fd()).map_err(retained)?;
    let request =
        Retirement::new(Vec::new(), source.name.clone(), observed.identity).map_err(retained)?;
    let result = session.remove(source.parent.as_fd(), request, |boundary, _, _| {
        if matches!(
            boundary,
            xsh_root::transfer::Boundary::BeforeCapture | xsh_root::transfer::Boundary::BeforeUnlink
        ) {
            job.cancellation.check()?;
        }
        Ok(())
    });
    let finish = session.finish();
    let path = source.path.display();
    match (result, finish) {
        (Ok(Removal::Removed), Ok(())) => Ok(OperandState::Moved),
        (Ok(Removal::Removed), Err(error)) => Err(Failure {
            state: OperandState::SourceRemovedSyncFailed,
            error,
        }),
        (Ok(Removal::AlreadyAbsent | Removal::Changed), _) => {
            Err(retained(io::Error::other("source changed before capture")))
        }
        (Err(error), _) if error.removed => Err(Failure {
            state: OperandState::SourceRemovedSyncFailed,
            error: io::Error::new(
                error.error.kind(),
                format!("source entry {path:?} removed; {}", error.error),
            ),
        }),
        (Err(error), _) => Err(retained(error.error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(arguments: &[&str]) -> Vec<String> {
        arguments.iter().map(|argument| (*argument).to_owned()).collect()
    }

    #[test]
    fn into_options_are_preserved() {
        let Ok(Parsed::Run(invocation)) = parse(
            &arguments(&[
                "--into",
                "target",
                "--merge",
                "--replace",
                "--dry-run",
                "--durable",
                "--cross-file-systems",
                "--jobs",
                "3",
                "-v",
                "--no-progress",
                "-r",
                "-R",
                "source",
                "second",
            ]),
            FileOperation::Copy,
        ) else {
            panic!("expected a transfer request");
        };
        assert!(invocation.options.merge);
        assert!(invocation.options.dry_run);
        assert!(invocation.options.durable);
        assert!(invocation.options.cross_file_systems);
        assert_eq!(invocation.options.collision, Collision::Replace);
        assert_eq!(invocation.options.jobs.map(Jobs::get), Some(3));
        assert!(invocation.verbose);
        assert!(invocation.no_progress);
        assert_eq!(
            invocation.operands,
            Operands::Into {
                directory: PathBuf::from("target"),
                first: PathBuf::from("source"),
                rest: vec![PathBuf::from("second")],
            }
        );
    }

    #[test]
    fn operand_mapping_follows_the_reference_grammar() {
        let Ok(Parsed::Run(exact)) = parse(&arguments(&["--", "-a", "-b"]), FileOperation::Move)
        else {
            panic!("expected an exact mapping");
        };
        assert_eq!(
            exact.operands,
            Operands::Exact {
                source: PathBuf::from("-a"),
                destination: PathBuf::from("-b"),
            }
        );
        let Ok(Parsed::Run(into)) = parse(&arguments(&["a", "b", "dir"]), FileOperation::Copy)
        else {
            panic!("expected an into mapping");
        };
        assert_eq!(
            into.operands,
            Operands::Into {
                directory: PathBuf::from("dir"),
                first: PathBuf::from("a"),
                rest: vec![PathBuf::from("b")],
            }
        );
    }

    #[test]
    fn invalid_options_and_mappings_are_syntax_errors() {
        for invalid in [
            &["--unknown", "source", "destination"][..],
            &["--jobs", "0", "source", "destination"],
            &["--jobs", "33", "source", "destination"],
            &["--jobs", "x", "source", "destination"],
            &["--replace", "-n", "source", "destination"],
            &["--help", "--unknown"],
            &[],
            &["source"],
            &["--into", "target"],
            &["--into", "a", "--into", "b", "c"],
        ] {
            assert!(
                parse(&arguments(invalid), FileOperation::Copy).is_err(),
                "{invalid:?}"
            );
        }
        assert!(parse(&arguments(&["-r", "a", "b"]), FileOperation::Move).is_err());
        assert!(matches!(
            parse(&arguments(&["--help"]), FileOperation::Copy),
            Ok(Parsed::Help)
        ));
    }

    #[test]
    fn rendered_text_matches_the_reference_wording() {
        assert_eq!(escape_bytes(b"line\n\x1b\xff\\name"), "line\\x0a\\x1b\\xff\\\\name");
        let outcome = |state| OperandOutcome {
            source: PathBuf::from("source/tree"),
            destination: PathBuf::from("target/tree"),
            state,
        };
        assert_eq!(
            render_operand(FileOperation::Copy, &outcome(OperandState::Planned), true).as_deref(),
            Some("would copy source/tree to target/tree")
        );
        assert_eq!(
            render_operand(FileOperation::Move, &outcome(OperandState::Moved), false).as_deref(),
            Some("moved source/tree to target/tree")
        );
        assert_eq!(
            render_partial(&outcome(OperandState::DestinationPublishedSourceRetained)),
            "published target/tree; retained source source/tree"
        );
        assert_eq!(
            render_partial(&outcome(OperandState::Cancelled)),
            "cancelled; retained source source/tree"
        );
        assert!(help(FileOperation::Copy).starts_with(
            "usage: cp [OPTIONS] SOURCE DESTINATION\n       cp [OPTIONS] SOURCE... DIRECTORY\n       cp [OPTIONS] --into DIRECTORY SOURCE...\n\nOPTIONS:\n  -r, -R"
        ));
        assert!(!help(FileOperation::Move).contains("-r, -R"));
    }
}
