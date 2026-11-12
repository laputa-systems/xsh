use std::ffi::{OsStr, OsString};
use std::io;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::os::unix::ffi::OsStrExt as _;
use std::path::{Path, PathBuf};

use xsh_root::transfer::{
    Ancestor, Cancellation, EntryIdentity, Removal, Retirement, RetirementGroup, Session,
    observe_entry,
};

use super::fs_command::Foreground;

pub(super) const RMRF_HELP: &str =
    "Usage: {name} [--] PATH...\nPermanently remove each PATH, including directory trees. Missing paths are ignored.";

pub(super) const RMRF_USAGE: &str = "Usage: rmrf [--] PATH...\n";

const DIRECTORY_FLAGS: rustix::fs::OFlags = rustix::fs::OFlags::RDONLY
    .union(rustix::fs::OFlags::DIRECTORY)
    .union(rustix::fs::OFlags::NOFOLLOW)
    .union(rustix::fs::OFlags::CLOEXEC);

const BAR_WIDTH: usize = 30;

/// Where a builtin resolves operands and which roots it protects.
pub(super) struct FileContext {
    pub(super) cwd: PathBuf,
    pub(super) home: Option<PathBuf>,
}

pub(super) enum Command {
    Help,
    Run { paths: Vec<String> },
}

pub(super) fn parse_args(arguments: &[String]) -> Result<Command, String> {
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
                option if option.len() > 1 && option.starts_with('-') => {
                    return Err(format!("unknown option: {argument}"));
                }
                _ => {}
            }
        }
        paths.push(argument.clone());
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

/// One entry scheduled for journaled retirement.
pub(super) struct PlannedEntry {
    name: OsString,
    identity: EntryIdentity,
}

/// One directory's retirement group, ordered after every descendant group.
pub(super) struct PlannedGroup {
    ancestors: Vec<Ancestor>,
    entries: Vec<PlannedEntry>,
}

/// A directory-tree retirement plan, descendants first and every parent last.
pub(super) struct Plan {
    groups: Vec<PlannedGroup>,
    cancelled: bool,
}

impl Plan {
    pub(super) fn entry_count(&self) -> usize {
        self.groups.iter().map(|group| group.entries.len()).sum()
    }

    pub(super) fn was_cancelled(&self) -> bool {
        self.cancelled
    }
}

/// Builds one journaled retirement group per directory reached from
/// `directory`, whose parent is `parent` and whose ancestry under the recovery
/// root is `ancestors`.
pub(super) fn plan_tree(
    root_name: &OsStr,
    root_identity: EntryIdentity,
    directory: BorrowedFd<'_>,
    cancellation: &Cancellation,
) -> io::Result<Plan> {
    let mut groups = Vec::new();
    let mut cancelled = false;
    let ancestors = vec![Ancestor {
        name: root_name.to_os_string(),
        identity: root_identity,
    }];
    collect(&ancestors, directory, cancellation, &mut groups, &mut cancelled)?;
    Ok(Plan { groups, cancelled })
}

fn collect(
    ancestors: &[Ancestor],
    directory: BorrowedFd<'_>,
    cancellation: &Cancellation,
    groups: &mut Vec<PlannedGroup>,
    cancelled: &mut bool,
) -> io::Result<()> {
    let mut entries = Vec::new();
    for name in directory_entries(directory)? {
        if cancellation.is_cancelled() {
            *cancelled = true;
            break;
        }
        let observed = observe_entry(directory, &name)?;
        if observed.identity.kind == xsh_root::transfer::FileKind::Directory {
            let child = xsh_root::transfer::open_entry_directory(directory, &name)?;
            let mut child_ancestors = ancestors.to_vec();
            child_ancestors.push(Ancestor {
                name: name.clone(),
                identity: observed.identity,
            });
            collect(
                &child_ancestors,
                child.as_fd(),
                cancellation,
                groups,
                cancelled,
            )?;
        }
        entries.push(PlannedEntry {
            name,
            identity: observed.identity,
        });
    }
    if !*cancelled {
        groups.push(PlannedGroup {
            ancestors: ancestors.to_vec(),
            entries,
        });
    }
    Ok(())
}

fn directory_entries(directory: BorrowedFd<'_>) -> io::Result<Vec<OsString>> {
    let mut names = Vec::new();
    for entry in rustix::fs::Dir::read_from(directory)? {
        let entry = entry?;
        let bytes = entry.file_name().to_bytes();
        if bytes != b"." && bytes != b".." {
            names.push(OsString::from(OsStr::from_bytes(bytes)));
        }
    }
    names.sort();
    Ok(names)
}

/// Retires every planned group and the tree root through one journal session.
pub(super) fn retire_tree(
    plan: &Plan,
    parent: BorrowedFd<'_>,
    root_name: &OsStr,
    root_identity: EntryIdentity,
) -> io::Result<()> {
    let session = Session::start(parent)?;
    for group in &plan.groups {
        retire_group(&session, group)?;
    }
    let request = Retirement::new(Vec::new(), root_name.to_os_string(), root_identity)?;
    let removal = session
        .remove(parent, request, |_, _, _| Ok(()))
        .map_err(|error| io::Error::new(error.error.kind(), error.error.to_string()))?;
    match removal {
        Removal::Removed | Removal::AlreadyAbsent => session.finish(),
        Removal::Changed => Err(io::Error::other("entry changed before it could be removed")),
    }
}

fn retire_group(session: &Session, group: &PlannedGroup) -> io::Result<()> {
    let entries = group
        .entries
        .iter()
        .map(|entry| {
            Retirement::new(group.ancestors.clone(), entry.name.clone(), entry.identity)
        })
        .collect::<io::Result<Vec<_>>>()?;
    session.retire_batch(
        &RetirementGroup {
            ancestors: group.ancestors.clone(),
            entries,
        },
        |_, _, _| Ok(()),
    )?;
    Ok(())
}

/// Captures and removes one leaf entry through its own journal session.
pub(super) fn retire_leaf(
    parent: BorrowedFd<'_>,
    name: &OsStr,
    expected: EntryIdentity,
) -> io::Result<()> {
    let session = Session::start(parent)?;
    let request = Retirement::new(Vec::new(), name.to_os_string(), expected)?;
    let removal = session
        .remove(parent, request, |_, _, _| Ok(()))
        .map_err(|error| io::Error::new(error.error.kind(), error.error.to_string()))?;
    match removal {
        Removal::Removed | Removal::AlreadyAbsent => session.finish(),
        Removal::Changed => Err(io::Error::other("entry changed before it could be removed")),
    }
}

/// Removes each operand and reports the first failure.
pub(super) fn run(
    arguments: &[String],
    context: &FileContext,
    foreground: &mut Foreground,
    stderr: &mut Vec<u8>,
) -> i32 {
    let paths = match parse_args(arguments) {
        Ok(Command::Help) => {
            stderr.extend_from_slice(RMRF_HELP.replace("{name}", "rmrf").as_bytes());
            return 0;
        }
        Ok(Command::Run { paths }) => paths,
        Err(error) => {
            let _ = io::Write::write_all(
                stderr,
                format!("xshi: rmrf: {error}\n{RMRF_USAGE}").as_bytes(),
            );
            return 2;
        }
    };
    foreground.set_progress_label("rmrf");
    let mut status = 0;
    for path in paths {
        if foreground.observed_interrupt().is_some() {
            break;
        }
        match remove_path(&path, context, foreground) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => {
                if status == 0 {
                    status = 1;
                }
                let _ = io::Write::write_all(
                    stderr,
                    format!("xshi: rmrf: {path}: {error}\n").as_bytes(),
                );
            }
        }
        let _ = foreground.clear_progress();
    }
    if foreground.observed_interrupt().is_some() {
        return 130;
    }
    status
}

fn retire_root(
    plan: &Plan,
    parent: BorrowedFd<'_>,
    name: &OsStr,
    identity: EntryIdentity,
) -> io::Result<()> {
    retire_tree(plan, parent, name, identity)
}

fn remove_path(path: &str, context: &FileContext, foreground: &mut Foreground) -> io::Result<()> {
    let relative = Path::new(path);
    let absolute = if relative.is_absolute() {
        relative.to_path_buf()
    } else {
        context.cwd.join(relative)
    };
    let Some((parent_path, name)) = split_target(&absolute) else {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "refusing to remove the filesystem root",
        ));
    };
    let parent = open_directory(&parent_path)?;
    let observed = match observe_entry(parent.as_fd(), &name) {
        Ok(observed) => observed,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if observed.identity.kind == xsh_root::transfer::FileKind::Directory {
        if let Some(reason) = protected_reason(observed.identity, context) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("refusing to remove {path}: it is {reason}"),
            ));
        }
        let directory = xsh_root::transfer::open_entry_directory(parent.as_fd(), &name)?;
        let mount = xsh_root::transfer::mount_id(parent.as_fd())?;
        if xsh_root::transfer::mount_id(directory.as_fd())? != mount {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("refusing to remove {path}: it is a mount point"),
            ));
        }
        let plan = plan_tree(
            &name,
            observed.identity,
            directory.as_fd(),
            foreground.cancellation(),
        )?;
        if plan.was_cancelled() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "operation cancelled",
            ));
        }
        retire_root(&plan, parent.as_fd(), &name, observed.identity)?;
        foreground.clear_progress()?;
        return Ok(());
    }
    retire_leaf(parent.as_fd(), &name, observed.identity)?;
    foreground.clear_progress()?;
    Ok(())
}

fn split_target(path: &Path) -> Option<(PathBuf, OsString)> {
    let bytes = path.as_os_str().as_bytes();
    let mut end = bytes.len();
    while end > 0 && bytes[end - 1] == b'/' {
        end -= 1;
    }
    if end == 0 {
        return None;
    }
    let trimmed = Path::new(OsStr::from_bytes(&bytes[..end]));
    let name = trimmed.file_name()?.to_os_string();
    if name == "." || name == ".." {
        return None;
    }
    let parent = trimmed
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    Some((parent.to_path_buf(), name))
}

fn open_directory(path: &Path) -> io::Result<OwnedFd> {
    Ok(rustix::fs::open(path, DIRECTORY_FLAGS, rustix::fs::Mode::empty())?)
}

fn protected_reason(identity: EntryIdentity, context: &FileContext) -> Option<&'static str> {
    let identity_of = |path: &Path| -> Option<EntryIdentity> {
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            context.cwd.join(path)
        };
        let (parent, name) = split_target(&path)?;
        let directory = open_directory(&parent).ok()?;
        observe_entry(directory.as_fd(), &name)
            .ok()
            .map(|observed| observed.identity)
    };
    if identity_of(Path::new("/")).is_some_and(|found| found == identity) {
        return Some("the filesystem root");
    }
    if let Some(home) = context.home.as_deref()
        && !home.as_os_str().is_empty()
        && identity_of(home).is_some_and(|found| found == identity)
    {
        return Some("your home directory");
    }
    if identity_of(&context.cwd).is_some_and(|found| found == identity) {
        return Some("the current directory");
    }
    for ancestor in context.cwd.ancestors().skip(1) {
        if identity_of(ancestor).is_some_and(|found| found == identity) {
            return Some("a parent of the current directory");
        }
    }
    None
}

/// Renders the `rmrf` deletion progress line on a terminal.
pub(super) fn deleting_progress(completed: usize, total: usize) -> String {
    let fraction = if total == 0 {
        1.0
    } else {
        completed as f64 / total as f64
    };
    let filled = ((fraction * BAR_WIDTH as f64) as usize).min(BAR_WIDTH);
    format!(
        "Deleting [{}{}] {:>3}% {completed}/{total}",
        "#".repeat(filled),
        "-".repeat(BAR_WIDTH - filled),
        (fraction * 100.0) as usize,
    )
}
