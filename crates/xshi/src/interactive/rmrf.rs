//! The `rmrf` builtin and the journaled tree deletion it shares with `mv`.
//!
//! A directory is indexed first; its contents are then retired bottom-up
//! through one journal session inside the directory, regular files and links
//! in batches per parent, and the emptied directory itself is removed last.

use std::ffi::{OsStr, OsString};
use std::io::{self, Write as _};
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};

use rustix::fs::{self as rfs, AtFlags, Mode, OFlags};
use rustix::io::Errno;
use xsh_root::FileKind;
use xsh_root::transfer::{
    Ancestor, Boundary, CancelReason, Cancellation, EntryIdentity, Removal, RemovalError,
    Retirement, Session, mount_id, observe_entry, observe_fd,
};

use super::foreground::{self, Foreground, Streams};
use super::fs_command::FileContext;

const DIRECTORY: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::CLOEXEC);
const BATCH_SIZE: usize = 256;
const SCAN_REPORT_EVERY: u64 = 1024;
const PROGRESS_BAR_WIDTH: usize = 30;

enum Command {
    Help,
    Run { paths: Vec<PathBuf> },
}

fn parse_args(arguments: &[String]) -> Result<Command, String> {
    let mut positional_only = false;
    let mut help = false;
    let mut paths = Vec::new();
    for argument in arguments {
        if !positional_only {
            match argument.as_str() {
                "--" => {
                    positional_only = true;
                    continue;
                }
                "-h" | "--help" => {
                    help = true;
                    continue;
                }
                "--force" | "--recursive" => continue,
                option if is_force_recursive_option(option) => continue,
                option if option.starts_with('-') => {
                    return Err(format!("unknown option: {option}"));
                }
                _ => {}
            }
        }
        paths.push(PathBuf::from(argument));
    }
    if help {
        return Ok(Command::Help);
    }
    if paths.is_empty() {
        return Err("requires at least one PATH".to_owned());
    }
    Ok(Command::Run { paths })
}

fn is_force_recursive_option(option: &str) -> bool {
    let bytes = option.as_bytes();
    bytes.len() > 1
        && bytes[0] == b'-'
        && bytes[1..]
            .iter()
            .all(|byte| matches!(byte, b'r' | b'R' | b'f'))
}

/// Runs `rmrf` and returns the shell status.
pub(super) fn run(
    name: &str,
    arguments: &[String],
    context: &FileContext,
    streams: &mut Streams<'_>,
) -> i32 {
    match parse_args(arguments) {
        Ok(Command::Help) => {
            if let Some(help) = super::builtin::help(name) {
                let _ = streams.stdout.write_all(help.as_bytes());
            }
            0
        }
        Ok(Command::Run { paths }) => {
            let progress_terminal = streams.progress_terminal();
            let status = match remove_paths(paths, context, progress_terminal) {
                Ok(()) => 0,
                Err(error) => {
                    let _ = writeln!(streams.stderr, "xshi: {name}: {error}");
                    if error.kind() == io::ErrorKind::Interrupted {
                        foreground::cancel_line();
                        130
                    } else {
                        1
                    }
                }
            };
            foreground::exit_if_terminated(streams);
            status
        }
        Err(error) => {
            let _ = writeln!(streams.stderr, "xshi: {name}: {error}");
            2
        }
    }
}

fn remove_paths(
    paths: Vec<PathBuf>,
    context: &FileContext,
    progress_terminal: bool,
) -> io::Result<()> {
    let mut foreground = Foreground::start(progress_terminal)?;
    let mut first_error = None;
    for path in paths {
        if foreground.cancellation().is_cancelled() {
            break;
        }
        if let Err(error) = remove_path(&path, context, &mut foreground)
            && first_error.is_none()
        {
            first_error = Some(error);
        }
        if let Err(error) = foreground.clear_progress() {
            first_error.get_or_insert(error);
            break;
        }
    }
    foreground.poll();
    let cancelled_by_user = foreground.cancellation().reason() == Some(CancelReason::User);
    let finish = foreground.finish();
    drop(foreground);
    if cancelled_by_user {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "operation cancelled",
        ));
    }
    if let Some(error) = first_error {
        return Err(error);
    }
    finish
}

fn resolve(context: &FileContext, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        context.cwd.join(path)
    }
}

fn remove_path(path: &Path, context: &FileContext, foreground: &mut Foreground) -> io::Result<()> {
    let (parent_path, name) = target_components(path)?;
    let resolved = resolve(context, path);
    let metadata = match std::fs::symlink_metadata(&resolved) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(path_error(path, error)),
    };
    let parent_path = resolve(context, &parent_path);
    if metadata.file_type().is_dir() {
        remove_directory(path, &resolved, &parent_path, &name, context, foreground)
    } else {
        let file_type = metadata.file_type();
        let kind = if file_type.is_symlink() {
            FileKind::Symlink
        } else if file_type.is_file() {
            FileKind::File
        } else {
            match observe_entry(open_parent(&parent_path)?.as_fd(), &name) {
                Ok(observed) => observed.identity.kind,
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
                Err(error) => return Err(path_error(path, error)),
            }
        };
        remove_leaf(
            path,
            &parent_path,
            &name,
            EntryIdentity {
                device: metadata.dev(),
                inode: metadata.ino(),
                kind,
            },
        )
    }
}

fn target_components(path: &Path) -> io::Result<(PathBuf, OsString)> {
    let bytes = path.as_os_str().as_bytes();
    let mut end = bytes.len();
    while end > 0 && bytes[end - 1] == b'/' {
        end -= 1;
    }
    if end == 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "refusing to remove the filesystem root",
        ));
    }
    let start = bytes[..end]
        .iter()
        .rposition(|byte| *byte == b'/')
        .map_or(0, |index| index + 1);
    let last = &bytes[start..end];
    if last == b"." || last == b".." {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("refusing to remove {}", path.display()),
        ));
    }
    let trimmed = Path::new(OsStr::from_bytes(&bytes[..end]));
    let name = trimmed
        .file_name()
        .expect("a non-empty final component has a name");
    let parent = trimmed
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    Ok((parent.to_path_buf(), name.to_os_string()))
}

fn open_parent(path: &Path) -> io::Result<OwnedFd> {
    rfs::open(path, DIRECTORY, Mode::empty()).map_err(|error| path_error(path, error.into()))
}

fn path_error(path: &Path, error: io::Error) -> io::Error {
    io::Error::new(error.kind(), format!("{}: {error}", path.display()))
}

fn sync_error(path: &Path, error: Errno) -> io::Error {
    let error = io::Error::from(error);
    io::Error::new(
        error.kind(),
        format!("{} was removed; synchronization failed: {error}", path.display()),
    )
}

fn changed(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        format!("{} changed before it could be removed", path.display()),
    )
}

fn stat_identity(parent: BorrowedFd<'_>, name: &OsStr) -> Result<EntryIdentity, Errno> {
    observe_entry(parent, name)
        .map(|observed| observed.identity)
        .map_err(|error| Errno::from_io_error(&error).unwrap_or(Errno::IO))
}

fn remove_leaf(
    path: &Path,
    parent_path: &Path,
    name: &OsStr,
    expected: EntryIdentity,
) -> io::Result<()> {
    let parent = open_parent(parent_path)?;
    match stat_identity(parent.as_fd(), name) {
        Ok(actual) if actual == expected => {}
        Ok(_) => return Err(changed(path)),
        Err(Errno::NOENT) => return Ok(()),
        Err(error) => return Err(path_error(path, error.into())),
    }
    match rfs::unlinkat(&parent, name, AtFlags::empty()) {
        Ok(()) => {}
        Err(Errno::NOENT) => return Ok(()),
        Err(error) => return Err(path_error(path, error.into())),
    }
    rfs::fsync(&parent).map_err(|error| sync_error(path, error))
}

fn remove_directory(
    path: &Path,
    resolved: &Path,
    parent_path: &Path,
    name: &OsStr,
    context: &FileContext,
    foreground: &mut Foreground,
) -> io::Result<()> {
    let root = open_root_no_follow(resolved).map_err(|error| describe_open_failure(path, resolved, error))?;
    let root_identity = observe_fd(root.as_fd())?.identity;
    let parent = open_parent(parent_path)?;
    if mount_id(root.as_fd())? != mount_id(parent.as_fd())? {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("refusing to remove {}: it is a mount point", path.display()),
        ));
    }
    if let Some(reason) = protected_root_reason(root_identity, &context.cwd, context.home.as_deref())? {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("refusing to remove {}: it is {reason}", path.display()),
        ));
    }
    let scanned = foreground.run(
        |cancellation, events| {
            Tree::scan(root.as_fd(), root_identity, cancellation, &mut |count| {
                let _ = events.emit(count);
            })
        },
        |foreground, count: u64| foreground.report_progress("Scanning...", count, None),
        |_| Ok(()),
    )?;
    foreground.clear_progress()?;
    let tree = match scanned {
        Ok(Scan::Complete(tree)) => tree,
        Ok(Scan::Incomplete) => {
            return Err(io::Error::other(format!(
                "refusing to remove {}: root scan is incomplete",
                path.display()
            )));
        }
        Err(error) => return Err(error),
    };
    let total = tree.nodes.len() - 1;
    if total > 0 {
        let deleted = foreground.run(
            |cancellation, events| {
                delete_contents(root.as_fd(), &tree, cancellation, &mut |completed| {
                    if completed.is_multiple_of(BATCH_SIZE) || completed == total {
                        let _ = events.emit(completed);
                    }
                })
            },
            |foreground, completed: usize| {
                foreground.report_progress_line(&deleting_progress(completed, total))
            },
            |_| Ok(()),
        );
        let (report, operation_error) = match deleted {
            Ok((report, error)) => (report, error),
            Err(error) => (DeletionReport::default(), Some(error)),
        };
        if let Some(error) = operation_error {
            return Err(report.with_operation_error(error));
        }
        foreground.clear_progress()?;
        if report.has_problems() {
            return Err(io::Error::other(report.failure_summary()));
        }
        if report.completed() != total {
            return Err(io::Error::other(format!(
                "deletion reported {} outcomes for {total} planned entries",
                report.completed()
            )));
        }
    }
    foreground.clear_progress()?;
    if foreground.cancellation().is_cancelled() {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "operation cancelled",
        ));
    }
    remove_root(root_identity, parent.as_fd(), name, path)
}

fn open_root_no_follow(path: &Path) -> io::Result<OwnedFd> {
    let bytes = path.as_os_str().as_bytes();
    let mut end = bytes.len();
    while end > 1 && bytes[end - 1] == b'/' {
        end -= 1;
    }
    Ok(rfs::open(
        Path::new(OsStr::from_bytes(&bytes[..end])),
        DIRECTORY.union(OFlags::NOFOLLOW),
        Mode::empty(),
    )?)
}

fn describe_open_failure(path: &Path, resolved: &Path, error: io::Error) -> io::Error {
    let bytes = resolved.as_os_str().as_bytes();
    let mut end = bytes.len();
    while end > 0 && bytes[end - 1] == b'/' {
        end -= 1;
    }
    let symlink = end > 0
        && std::fs::symlink_metadata(Path::new(OsStr::from_bytes(&bytes[..end])))
            .is_ok_and(|metadata| metadata.file_type().is_symlink());
    if symlink {
        return io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "{} is a symlink; name the directory it points to instead",
                path.display()
            ),
        );
    }
    path_error(path, error)
}

fn protected_root_reason(
    root: EntryIdentity,
    cwd: &Path,
    home: Option<&Path>,
) -> io::Result<Option<&'static str>> {
    let is_root = |candidate: &Path| -> io::Result<bool> {
        let metadata = std::fs::metadata(candidate).map_err(|error| {
            io::Error::new(error.kind(), format!("{}: {error}", candidate.display()))
        })?;
        Ok(metadata.dev() == root.device && metadata.ino() == root.inode)
    };
    if is_root(Path::new("/"))? {
        return Ok(Some("the filesystem root"));
    }
    if let Some(home) = home.filter(|home| !home.as_os_str().is_empty()) {
        match is_root(home) {
            Ok(true) => return Ok(Some("your home directory")),
            Ok(false) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    if is_root(cwd)? {
        return Ok(Some("the current directory"));
    }
    for parent in cwd.ancestors().skip(1) {
        if is_root(parent)? {
            return Ok(Some("a parent of the current directory"));
        }
    }
    Ok(None)
}

fn remove_root(
    expected: EntryIdentity,
    parent: BorrowedFd<'_>,
    name: &OsStr,
    path: &Path,
) -> io::Result<()> {
    match stat_identity(parent, name) {
        Ok(actual) if actual == expected => {}
        Ok(_) => return Err(changed(path)),
        Err(Errno::NOENT) => return Ok(()),
        Err(error) => return Err(path_error(path, error.into())),
    }
    match rfs::unlinkat(parent, name, AtFlags::REMOVEDIR) {
        Ok(()) => {}
        Err(Errno::NOENT) => return Ok(()),
        Err(error) => return Err(path_error(path, error.into())),
    }
    rfs::fsync(parent).map_err(|error| sync_error(path, error))
}

fn deleting_progress(completed: usize, total: usize) -> String {
    let fraction = if total == 0 {
        1.0
    } else {
        completed as f64 / total as f64
    };
    let filled = ((fraction * PROGRESS_BAR_WIDTH as f64) as usize).min(PROGRESS_BAR_WIDTH);
    format!(
        "Deleting [{}{}] {:>3}% {completed}/{total}",
        "#".repeat(filled),
        "-".repeat(PROGRESS_BAR_WIDTH - filled),
        (fraction * 100.0) as usize,
    )
}

/// One indexed entry; node 0 is the scanned directory itself.
struct Node {
    name: OsString,
    identity: EntryIdentity,
    size: u64,
    parent: usize,
    children: Vec<usize>,
}

/// The index of one directory tree taken before anything is removed.
struct Tree {
    nodes: Vec<Node>,
}

enum Scan {
    Complete(Tree),
    /// A subdirectory could not be read, crossed a mount, or the scan was
    /// cancelled, so the index does not cover the whole tree.
    Incomplete,
}

impl Tree {
    fn scan(
        root: BorrowedFd<'_>,
        identity: EntryIdentity,
        cancellation: &Cancellation,
        report: &mut dyn FnMut(u64),
    ) -> io::Result<Scan> {
        let mount = mount_id(root)?;
        let mut tree = Tree {
            nodes: vec![Node {
                name: OsString::new(),
                identity,
                size: 0,
                parent: usize::MAX,
                children: Vec::new(),
            }],
        };
        let mut scanned = 0_u64;
        let mut pending = vec![(0_usize, rfs::openat(root, ".", DIRECTORY, Mode::empty())?)];
        while let Some((index, directory)) = pending.pop() {
            if cancellation.is_cancelled() {
                return Ok(Scan::Incomplete);
            }
            if observe_fd(directory.as_fd())?.identity != tree.nodes[index].identity {
                return Err(io::Error::other(
                    "scan root identity changed after it was opened",
                ));
            }
            let Ok(entries) = rfs::Dir::read_from(&directory) else {
                return Ok(Scan::Incomplete);
            };
            for entry in entries {
                let Ok(entry) = entry else {
                    return Ok(Scan::Incomplete);
                };
                let name = entry.file_name().to_bytes();
                if name == b"." || name == b".." {
                    continue;
                }
                let name = OsStr::from_bytes(name).to_os_string();
                let Ok(observed) = observe_entry(directory.as_fd(), &name) else {
                    return Ok(Scan::Incomplete);
                };
                let child = tree.nodes.len();
                tree.nodes.push(Node {
                    name,
                    identity: observed.identity,
                    size: observed.size,
                    parent: index,
                    children: Vec::new(),
                });
                tree.nodes[index].children.push(child);
                scanned += 1;
                if scanned.is_multiple_of(SCAN_REPORT_EVERY) {
                    report(scanned);
                }
                if observed.identity.kind == FileKind::Directory {
                    let Ok(opened) = rfs::openat(
                        &directory,
                        &tree.nodes[child].name,
                        DIRECTORY.union(OFlags::NOFOLLOW),
                        Mode::empty(),
                    ) else {
                        return Ok(Scan::Incomplete);
                    };
                    if mount_id(opened.as_fd())? != mount {
                        return Ok(Scan::Incomplete);
                    }
                    pending.push((child, opened));
                }
            }
        }
        report(scanned);
        for index in 0..tree.nodes.len() {
            let mut children = std::mem::take(&mut tree.nodes[index].children);
            children.sort_unstable_by_key(|child| {
                let identity = tree.nodes[*child].identity;
                (identity.kind == FileKind::Directory, identity.inode)
            });
            tree.nodes[index].children = children;
        }
        Ok(Scan::Complete(tree))
    }

    fn is_directory(&self, node: usize) -> bool {
        self.nodes[node].identity.kind == FileKind::Directory
    }

    fn name(&self, node: usize) -> String {
        String::from_utf8_lossy(self.nodes[node].name.as_bytes()).into_owned()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OutcomeKind {
    Deleted,
    DeletedSyncFailed,
    AlreadyAbsent,
    Changed,
    Failed,
    Cancelled,
}

#[derive(Default)]
struct DeletionReport {
    deleted: usize,
    removed_sync_failed: usize,
    already_absent: usize,
    changed: usize,
    failed: usize,
    not_attempted: usize,
    first_problem: Option<String>,
}

impl DeletionReport {
    fn record(&mut self, name: String, kind: OutcomeKind, message: Option<String>) {
        let mut problem = |default: &str| {
            self.first_problem.get_or_insert_with(|| {
                let detail = message.clone().unwrap_or_else(|| default.to_owned());
                format!("{name}: {detail}")
            });
        };
        match kind {
            OutcomeKind::Deleted => self.deleted += 1,
            OutcomeKind::DeletedSyncFailed => {
                problem("entry removed; synchronization failed");
                self.removed_sync_failed += 1;
            }
            OutcomeKind::AlreadyAbsent => self.already_absent += 1,
            OutcomeKind::Changed => {
                problem("entry changed");
                self.changed += 1;
            }
            OutcomeKind::Failed => {
                problem("entry could not be removed");
                self.failed += 1;
            }
            OutcomeKind::Cancelled => self.not_attempted += 1,
        }
    }

    fn completed(&self) -> usize {
        self.deleted
            + self.removed_sync_failed
            + self.already_absent
            + self.changed
            + self.failed
            + self.not_attempted
    }

    fn has_problems(&self) -> bool {
        self.removed_sync_failed != 0
            || self.changed != 0
            || self.failed != 0
            || self.not_attempted != 0
    }

    fn with_operation_error(&self, error: io::Error) -> io::Error {
        if self.completed() == 0 {
            return error;
        }
        io::Error::new(
            error.kind(),
            format!("{}; {error}", self.failure_summary()),
        )
    }

    fn failure_summary(&self) -> String {
        let mut summary = format!(
            "Deleted {} · {} removed with synchronization failures · {} already absent · {} changed · {} failed · {} not attempted",
            self.deleted,
            self.removed_sync_failed,
            self.already_absent,
            self.changed,
            self.failed,
            self.not_attempted
        );
        if let Some(problem) = &self.first_problem {
            summary.push_str(&format!("; first problem: {problem}"));
        }
        summary
    }
}

/// The state of one bottom-up deletion of a tree's contents.
struct Deletion<'a> {
    tree: &'a Tree,
    session: Option<&'a Session>,
    mount: xsh_root::transfer::MountIdentity,
    cancellation: &'a Cancellation,
    recorded: Vec<bool>,
    report: DeletionReport,
    progress: &'a mut dyn FnMut(usize),
}

struct Frame {
    node: usize,
    next_child: usize,
    ancestry: Vec<Ancestor>,
}

impl Deletion<'_> {
    fn stopped(&self) -> bool {
        self.cancellation.is_cancelled()
            || self.session.is_some_and(Session::recovery_required)
    }

    fn record(&mut self, node: usize, kind: OutcomeKind, message: Option<String>) {
        if std::mem::replace(&mut self.recorded[node], true) {
            return;
        }
        let name = self.tree.name(node);
        self.report.record(name, kind, message);
        (self.progress)(self.report.completed());
    }

    fn record_below(&mut self, node: usize, kind: OutcomeKind, message: &str) {
        let mut stack = vec![node];
        while let Some(current) = stack.pop() {
            for child in self.tree.nodes[current].children.clone() {
                if self.tree.is_directory(child) {
                    stack.push(child);
                }
                self.record(child, kind, Some(message.to_owned()));
            }
        }
    }

    fn open_checked(&self, parent: BorrowedFd<'_>, node: usize) -> Result<OwnedFd, (OutcomeKind, String)> {
        let expected = self.tree.nodes[node].identity;
        let name = &self.tree.nodes[node].name;
        let directory = match rfs::openat(parent, name, DIRECTORY.union(OFlags::NOFOLLOW), Mode::empty()) {
            Ok(directory) => directory,
            Err(Errno::NOENT) => {
                return Err((
                    OutcomeKind::AlreadyAbsent,
                    "directory is already gone".to_owned(),
                ));
            }
            Err(error) => {
                if stat_identity(parent, name).is_ok_and(|found| found != expected) {
                    return Err((
                        OutcomeKind::Changed,
                        "directory was replaced during validation".to_owned(),
                    ));
                }
                return Err((OutcomeKind::Failed, io::Error::from(error).to_string()));
            }
        };
        let failed = |error: io::Error| (OutcomeKind::Failed, error.to_string());
        if observe_fd(directory.as_fd()).map_err(failed)?.identity != expected {
            return Err((OutcomeKind::Changed, "directory was replaced".to_owned()));
        }
        if mount_id(directory.as_fd()).map_err(failed)? != self.mount {
            return Err((
                OutcomeKind::Changed,
                "directory became a mount boundary".to_owned(),
            ));
        }
        Ok(directory)
    }

    fn open_parent_checked(&self, child: BorrowedFd<'_>, parent: usize) -> Result<OwnedFd, (OutcomeKind, String)> {
        let failed = |error: io::Error| (OutcomeKind::Failed, error.to_string());
        let directory = rfs::openat(child, "..", DIRECTORY, Mode::empty())
            .map_err(|error| failed(error.into()))?;
        if observe_fd(directory.as_fd()).map_err(failed)?.identity != self.tree.nodes[parent].identity {
            return Err((
                OutcomeKind::Changed,
                "parent directory changed during deletion".to_owned(),
            ));
        }
        if mount_id(directory.as_fd()).map_err(failed)? != self.mount {
            return Err((
                OutcomeKind::Changed,
                "parent became a mount boundary".to_owned(),
            ));
        }
        Ok(directory)
    }

    fn cancelled_boundary(&self, boundary: Boundary) -> io::Result<()> {
        if matches!(boundary, Boundary::BeforeCapture | Boundary::BeforeUnlink) && self.stopped() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "deletion cancelled",
            ));
        }
        Ok(())
    }

    fn delete_files_batch(&mut self, parent: BorrowedFd<'_>, ancestry: &[Ancestor], batch: &mut Vec<usize>) {
        let mut nodes = Vec::with_capacity(batch.len());
        let mut requests = Vec::with_capacity(batch.len());
        for node in batch.drain(..) {
            let entry = &self.tree.nodes[node];
            match Retirement::new(ancestry.to_vec(), entry.name.clone(), entry.identity) {
                Ok(request) => {
                    nodes.push(node);
                    requests.push(request);
                }
                Err(error) => self.record(node, OutcomeKind::Failed, Some(error.to_string())),
            }
        }
        if requests.is_empty() {
            return;
        }
        let Some(session) = self.session else {
            for node in nodes {
                if self.stopped() {
                    self.record(node, OutcomeKind::Cancelled, None);
                    continue;
                }
                let (kind, message) = direct_remove(parent, &self.tree.nodes[node]);
                self.record(node, kind, message);
            }
            return;
        };
        let results = session.remove_batch(parent, requests, |boundary| self.cancelled_boundary(boundary));
        let mut results = results.into_iter();
        for node in nodes {
            let result = results.next().unwrap_or_else(|| {
                Err(RemovalError {
                    removed: false,
                    error: io::Error::other("batch deletion result missing"),
                })
            });
            if matches!(&result, Err(error) if error.error.kind() == io::ErrorKind::Interrupted) {
                self.record(node, OutcomeKind::Cancelled, None);
            } else {
                let (kind, message) = removal_outcome(result);
                self.record(node, kind, message);
            }
        }
    }

    fn remove_directory(&mut self, parent: BorrowedFd<'_>, ancestry: &[Ancestor], node: usize) {
        let entry = &self.tree.nodes[node];
        match stat_identity(parent, &entry.name) {
            Ok(found) if found == entry.identity => {}
            Ok(_) => {
                return self.record(
                    node,
                    OutcomeKind::Changed,
                    Some("entry changed immediately before removal".to_owned()),
                );
            }
            Err(Errno::NOENT) => return self.record(node, OutcomeKind::AlreadyAbsent, None),
            Err(error) => {
                return self.record(node, OutcomeKind::Failed, Some(io::Error::from(error).to_string()));
            }
        }
        let Some(session) = self.session else {
            let (kind, message) = direct_remove(parent, entry);
            return self.record(node, kind, message);
        };
        let result = Retirement::new(ancestry.to_vec(), entry.name.clone(), entry.identity)
            .map_err(|error| RemovalError {
                removed: false,
                error,
            })
            .and_then(|request| {
                session.remove(parent, request, |boundary, _, _| self.cancelled_boundary(boundary))
            });
        let (kind, message) = removal_outcome(result);
        self.record(node, kind, message);
    }

    /// Removes everything below the root, files first in each directory and
    /// every directory after its contents.
    fn delete_contents(&mut self, root: BorrowedFd<'_>) -> io::Result<()> {
        let mut current = rfs::openat(root, ".", DIRECTORY, Mode::empty())?;
        let mut stack = vec![Frame {
            node: 0,
            next_child: 0,
            ancestry: Vec::new(),
        }];
        let mut batch = Vec::new();
        loop {
            if self.stopped() {
                return Ok(());
            }
            let frame = stack.last_mut().expect("directory frame exists");
            let child = self.tree.nodes[frame.node].children.get(frame.next_child).copied();
            if let Some(child) = child {
                frame.next_child += 1;
                if self.tree.is_directory(child) {
                    let ancestry = stack.last().expect("directory frame exists").ancestry.clone();
                    if !batch.is_empty() {
                        self.delete_files_batch(current.as_fd(), &ancestry, &mut batch);
                        if self.stopped() {
                            return Ok(());
                        }
                    }
                    match self.open_checked(current.as_fd(), child) {
                        Ok(directory) => {
                            let mut ancestry = ancestry;
                            ancestry.push(Ancestor {
                                name: self.tree.nodes[child].name.clone(),
                                identity: self.tree.nodes[child].identity,
                            });
                            current = directory;
                            stack.push(Frame {
                                node: child,
                                next_child: 0,
                                ancestry,
                            });
                        }
                        Err((kind, message)) => {
                            self.record(child, kind, Some(message.clone()));
                            let below = if kind == OutcomeKind::AlreadyAbsent {
                                OutcomeKind::AlreadyAbsent
                            } else {
                                OutcomeKind::Failed
                            };
                            self.record_below(child, below, &message);
                        }
                    }
                } else {
                    batch.push(child);
                    if batch.len() >= BATCH_SIZE {
                        let ancestry = stack.last().expect("directory frame exists").ancestry.clone();
                        self.delete_files_batch(current.as_fd(), &ancestry, &mut batch);
                        if self.stopped() {
                            return Ok(());
                        }
                    }
                }
                continue;
            }
            if !batch.is_empty() {
                let ancestry = stack.last().expect("directory frame exists").ancestry.clone();
                self.delete_files_batch(current.as_fd(), &ancestry, &mut batch);
                if self.stopped() {
                    return Ok(());
                }
            }
            let finished = stack.pop().expect("directory frame exists");
            let Some(parent) = stack.last() else {
                return Ok(());
            };
            match self.open_parent_checked(current.as_fd(), parent.node) {
                Ok(parent_fd) => {
                    let ancestry = parent.ancestry.clone();
                    self.remove_directory(parent_fd.as_fd(), &ancestry, finished.node);
                    current = parent_fd;
                }
                Err((kind, message)) => {
                    self.record(finished.node, kind, Some(message.clone()));
                    let selected = stack.get(1).map_or(finished.node, |frame| frame.node);
                    self.record(selected, OutcomeKind::Failed, Some(message.clone()));
                    self.record_below(selected, OutcomeKind::Failed, &message);
                    return Ok(());
                }
            }
        }
    }
}

fn direct_remove(parent: BorrowedFd<'_>, entry: &Node) -> (OutcomeKind, Option<String>) {
    match stat_identity(parent, &entry.name) {
        Ok(found) if found == entry.identity => {}
        Ok(_) => {
            return (
                OutcomeKind::Changed,
                Some("entry changed immediately before removal".to_owned()),
            );
        }
        Err(Errno::NOENT) => return (OutcomeKind::AlreadyAbsent, None),
        Err(error) => return (OutcomeKind::Failed, Some(io::Error::from(error).to_string())),
    }
    let flags = if entry.identity.kind == FileKind::Directory {
        AtFlags::REMOVEDIR
    } else {
        AtFlags::empty()
    };
    if let Err(error) = rfs::unlinkat(parent, &entry.name, flags) {
        if error == Errno::NOENT {
            return (OutcomeKind::AlreadyAbsent, None);
        }
        if error == Errno::NOTEMPTY {
            return (
                OutcomeKind::Failed,
                Some(format!(
                    "directory gained unindexed children: {}",
                    io::Error::from(error)
                )),
            );
        }
        return (OutcomeKind::Failed, Some(io::Error::from(error).to_string()));
    }
    match rfs::fsync(parent) {
        Ok(()) => (OutcomeKind::Deleted, None),
        Err(error) if io::Error::from(error).kind() == io::ErrorKind::Unsupported => {
            (OutcomeKind::Deleted, None)
        }
        Err(error) => (
            OutcomeKind::DeletedSyncFailed,
            Some(io::Error::from(error).to_string()),
        ),
    }
}

fn removal_outcome(result: Result<Removal, RemovalError>) -> (OutcomeKind, Option<String>) {
    match result {
        Ok(Removal::Removed) => (OutcomeKind::Deleted, None),
        Ok(Removal::AlreadyAbsent) => (OutcomeKind::AlreadyAbsent, None),
        Ok(Removal::Changed) => (OutcomeKind::Changed, None),
        Err(error) => (
            if error.removed {
                OutcomeKind::DeletedSyncFailed
            } else {
                OutcomeKind::Failed
            },
            Some(error.to_string()),
        ),
    }
}

/// Deletes every entry below `root` through one journal session inside it.
/// Returns the report together with an error that stopped the deletion.
fn delete_contents(
    root: BorrowedFd<'_>,
    tree: &Tree,
    cancellation: &Cancellation,
    progress: &mut dyn FnMut(usize),
) -> (DeletionReport, Option<io::Error>) {
    let stopped = |error: io::Error| {
        (
            DeletionReport::default(),
            Some(io::Error::new(error.kind(), format!("deletion stopped: {error}"))),
        )
    };
    let mount = match observe_fd(root).and_then(|observed| {
        if observed.identity == tree.nodes[0].identity {
            mount_id(root)
        } else {
            Err(io::Error::other("scan root identity changed before deletion"))
        }
    }) {
        Ok(mount) => mount,
        Err(error) => return stopped(error),
    };
    let session = match Session::start(root) {
        Ok(session) => Some(session),
        Err(error) if error.kind() == io::ErrorKind::Unsupported => None,
        Err(error) => return stopped(error),
    };
    let mut deletion = Deletion {
        tree,
        session: session.as_ref(),
        mount,
        cancellation,
        recorded: vec![false; tree.nodes.len()],
        report: DeletionReport::default(),
        progress,
    };
    deletion.recorded[0] = true;
    let walked = deletion.delete_contents(root);
    for node in 1..tree.nodes.len() {
        if !deletion.recorded[node] {
            deletion.record(node, OutcomeKind::Cancelled, None);
        }
    }
    let report = std::mem::take(&mut deletion.report);
    drop(deletion);
    let error = walked.err().or_else(|| {
        session
            .as_ref()
            .and_then(|session| session.finish().err())
    });
    (
        report,
        error.map(|error| io::Error::new(error.kind(), format!("deletion stopped: {error}"))),
    )
}

/// Why retiring a moved source tree stopped, and how many entries were
/// already removed.
pub(super) struct RetireFailure {
    pub(super) removed: usize,
    pub(super) error: io::Error,
}

/// Retires a source directory whose copy is published at the destination.
///
/// The source is indexed and every indexed entry must be present at the
/// destination with the same kind (and size, for regular files) before
/// anything is removed, so nothing that arrived during the copy is lost.
pub(super) fn retire_moved_tree(
    parent: BorrowedFd<'_>,
    name: &OsStr,
    expected: EntryIdentity,
    destination_parent: BorrowedFd<'_>,
    destination_name: &OsStr,
    cancellation: &Cancellation,
) -> Result<(), RetireFailure> {
    let retained = |error| RetireFailure { removed: 0, error };
    let root = rfs::openat(parent, name, DIRECTORY.union(OFlags::NOFOLLOW), Mode::empty())
        .map_err(|error| retained(error.into()))?;
    if observe_fd(root.as_fd()).map_err(retained)?.identity != expected {
        return Err(retained(io::Error::other("source changed before retirement")));
    }
    let tree = match Tree::scan(root.as_fd(), expected, cancellation, &mut |_| {}).map_err(retained)? {
        Scan::Complete(tree) => tree,
        Scan::Incomplete => {
            cancellation.check().map_err(retained)?;
            return Err(retained(io::Error::other("source tree could not be indexed for retirement")));
        }
    };
    let destination = rfs::openat(
        destination_parent,
        destination_name,
        DIRECTORY.union(OFlags::NOFOLLOW),
        Mode::empty(),
    )
    .map_err(|error| retained(error.into()))?;
    verify_published(&tree, 0, destination.as_fd()).map_err(retained)?;
    let (report, error) = delete_contents(root.as_fd(), &tree, cancellation, &mut |_| {});
    let removed = report.deleted + report.removed_sync_failed;
    if let Some(error) = error {
        return Err(RetireFailure {
            removed,
            error: report.with_operation_error(error),
        });
    }
    if report.has_problems() {
        return Err(RetireFailure {
            removed,
            error: io::Error::other(report.failure_summary()),
        });
    }
    cancellation.check().map_err(|error| RetireFailure { removed, error })?;
    remove_root(expected, parent, name, Path::new(name)).map_err(|error| RetireFailure {
        removed: removed + 1,
        error,
    })
}

fn verify_published(tree: &Tree, node: usize, destination: BorrowedFd<'_>) -> io::Result<()> {
    for &child in &tree.nodes[node].children {
        let entry = &tree.nodes[child];
        let published = observe_entry(destination, &entry.name).map_err(|_| {
            io::Error::other("source changed during transfer; source retained")
        })?;
        if published.identity.kind != entry.identity.kind
            || (entry.identity.kind == FileKind::File && published.size != entry.size)
        {
            return Err(io::Error::other(
                "source changed during transfer; source retained",
            ));
        }
        if entry.identity.kind == FileKind::Directory {
            let directory = rfs::openat(
                destination,
                &entry.name,
                DIRECTORY.union(OFlags::NOFOLLOW),
                Mode::empty(),
            )?;
            verify_published(tree, child, directory.as_fd())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(label: &str) -> PathBuf {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.work/tmp");
        std::fs::create_dir_all(&base).expect("create shared scratch");
        let path = base.join(format!(
            "xshi-rmrf-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock after epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).expect("create scratch");
        path
    }

    fn context(cwd: &Path, home: Option<&Path>) -> FileContext {
        FileContext {
            cwd: cwd.to_path_buf(),
            home: home.map(Path::to_path_buf),
            scratch: PathBuf::from("/tmp"),
        }
    }

    #[test]
    fn removes_directory_trees_across_journal_batches() {
        let base = scratch("batched");
        let root = base.join("root");
        let outside = base.join("outside");
        std::fs::create_dir_all(&outside).expect("create outside directory");
        std::fs::write(outside.join("kept"), b"kept").expect("write outside file");
        for branch in 0..2 {
            let directory = root.join(format!("branch-{branch}/nested"));
            std::fs::create_dir_all(&directory).expect("create branch");
            for index in 0..300 {
                std::fs::write(directory.join(format!("item-{index:03}")), b"content")
                    .expect("write branch file");
            }
        }
        std::os::unix::fs::symlink(outside.join("kept"), root.join("link")).expect("create link");
        let result = remove_paths(vec![root.clone()], &context(&base, None), false);
        assert!(result.is_ok(), "rmrf failed: {result:?}");
        assert!(!root.exists(), "rmrf kept the selected directory");
        assert!(outside.join("kept").is_file(), "symlink target was removed");
        std::fs::remove_dir_all(&base).expect("clean scratch");
    }

    #[test]
    fn removes_files_symlinks_and_ignores_missing_paths() {
        let base = scratch("mixed");
        let file = base.join("file");
        std::fs::write(&file, b"content").expect("write file");
        let target = base.join("target");
        std::fs::create_dir_all(&target).expect("create target");
        std::fs::write(target.join("inside"), b"content").expect("write inside");
        let link = base.join("link");
        std::os::unix::fs::symlink(&target, &link).expect("create link");
        let result = remove_paths(
            vec![file.clone(), link.clone(), base.join("missing")],
            &context(&base, None),
            false,
        );
        assert!(result.is_ok(), "rmrf failed: {result:?}");
        assert!(!file.exists());
        assert!(std::fs::symlink_metadata(&link).is_err());
        assert!(target.join("inside").is_file(), "symlink target was removed");
        std::fs::remove_dir_all(&base).expect("clean scratch");
    }

    #[test]
    fn protected_roots_are_refused_before_anything_is_removed() {
        let base = scratch("protected");
        let home = base.join("home");
        std::fs::create_dir_all(home.join("nested")).expect("create home");
        std::fs::write(home.join("nested/file"), b"content").expect("write file");
        let error = remove_paths(vec![home.clone()], &context(&base, Some(&home)), false)
            .expect_err("rmrf removed the home directory");
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(error.to_string().ends_with("it is your home directory"), "{error}");
        let error = remove_paths(vec![base.join("home/nested")], &context(&home.join("nested"), None), false)
            .expect_err("rmrf removed the current directory");
        assert!(error.to_string().ends_with("it is the current directory"), "{error}");
        assert!(home.join("nested/file").is_file(), "protected content was removed");
        for refused in ["/", "dir/.", "dir/.."] {
            assert_eq!(
                target_components(Path::new(refused)).expect_err("refused").kind(),
                io::ErrorKind::PermissionDenied
            );
        }
        std::fs::remove_dir_all(&base).expect("clean scratch");
    }

    #[test]
    fn arguments_accept_force_flags_and_separators() {
        let arguments: Vec<String> = ["-rf", "first", "--", "-second"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        let Ok(Command::Run { paths }) = parse_args(&arguments) else {
            panic!("paths were not parsed");
        };
        assert_eq!(paths, [PathBuf::from("first"), PathBuf::from("-second")]);
        assert!(matches!(parse_args(&["--help".to_owned()]), Ok(Command::Help)));
        assert_eq!(parse_args(&[]).err().as_deref(), Some("requires at least one PATH"));
        assert_eq!(
            parse_args(&["--yes".to_owned(), "path".to_owned()]).err().as_deref(),
            Some("unknown option: --yes")
        );
    }

    #[test]
    fn deleting_progress_renders_the_reference_bar() {
        assert_eq!(
            deleting_progress(3, 4),
            "Deleting [######################--------]  75% 3/4"
        );
    }
}
