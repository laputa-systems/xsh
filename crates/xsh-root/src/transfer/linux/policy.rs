use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::io;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use rustix::fs::{self, Mode, OFlags};

use crate::FileKind;
use crate::transfer::{
    Action, EntryIdentity, Observation, Operation, PathError, PlannedOperand, PolicyRequest,
    Preflight, Target, decide, into_basename,
};

use super::{mount_identity, observe, observe_fd};

const PREFLIGHT_MEMORY_BYTES: usize = 64 * 1024 * 1024;
const DIRECTORY_FLAGS: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::CLOEXEC);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Requirement {
    Existing,
    MayBeAbsent,
}

struct EntryRoot {
    parent: OwnedFd,
    name: OsString,
    observed: Option<Observation>,
}

impl EntryRoot {
    fn establish(
        cwd: BorrowedFd<'_>,
        path: &Path,
        requirement: Requirement,
    ) -> io::Result<Self> {
        let bytes = path.as_os_str().as_bytes();
        if bytes.is_empty() || bytes.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "path is empty or contains NUL",
            ));
        }
        let last = bytes.iter().rposition(|byte| *byte != b'/');
        let (parent_path, name) = match last {
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
        let parent = fs::openat(cwd, parent_path, DIRECTORY_FLAGS, Mode::empty())?;
        Self::from_parent(parent, name, requirement)
    }

    fn establish_at(
        parent: BorrowedFd<'_>,
        name: &OsStr,
        requirement: Requirement,
    ) -> io::Result<Self> {
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
        let parent = fs::openat(parent, ".", DIRECTORY_FLAGS, Mode::empty())?;
        Self::from_parent(parent, name, requirement)
    }

    fn from_parent(
        parent: OwnedFd,
        name: &OsStr,
        requirement: Requirement,
    ) -> io::Result<Self> {
        let observed = match observe(parent.as_fd(), name) {
            Ok(observed) => Some(observed),
            Err(error)
                if error.kind() == io::ErrorKind::NotFound
                    && requirement == Requirement::MayBeAbsent =>
            {
                None
            }
            Err(error) => return Err(error),
        };
        Ok(Self {
            parent,
            name: name.to_os_string(),
            observed,
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
        let directory = fs::openat(
            &self.parent,
            &self.name,
            DIRECTORY_FLAGS.union(OFlags::NOFOLLOW),
            Mode::empty(),
        )?;
        if !expected.same_identity(observe_fd(directory.as_fd())?) {
            return Err(io::Error::other("directory identity changed"));
        }
        Ok(directory)
    }

    fn open_directory_following_final(&self) -> io::Result<OwnedFd> {
        let expected = self.observed.ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "directory root is absent")
        })?;
        if !matches!(
            expected.identity.kind,
            FileKind::Directory | FileKind::Symlink
        ) {
            return Err(io::Error::new(
                io::ErrorKind::NotADirectory,
                "root is not a directory",
            ));
        }
        let directory = fs::openat(&self.parent, &self.name, DIRECTORY_FLAGS, Mode::empty())?;
        let opened = observe_fd(directory.as_fd())?;
        if expected.identity.kind == FileKind::Directory {
            if !expected.same_identity(opened) {
                return Err(io::Error::other("directory identity changed"));
            }
        } else if !expected.same_identity(observe(self.parent.as_fd(), &self.name)?) {
            return Err(io::Error::other("directory symlink identity changed"));
        }
        Ok(directory)
    }

    fn parent_identity(&self) -> io::Result<EntryIdentity> {
        Ok(observe_fd(self.parent.as_fd())?.identity)
    }

    fn is_mount_root(&self) -> io::Result<bool> {
        if self
            .observed
            .is_none_or(|observed| observed.identity.kind != FileKind::Directory)
        {
            return Ok(false);
        }
        let directory = self.open_directory()?;
        let mount = mount_identity(directory.as_fd())?;
        let parent = mount_identity(self.parent.as_fd())?;
        Ok(mount != parent)
    }

    fn same_entry(&self, other: &Self) -> io::Result<bool> {
        Ok(self.name == other.name && self.parent_identity()? == other.parent_identity()?)
    }

    fn is_ancestor_of(&self, candidate: &Self) -> io::Result<bool> {
        let Some(ancestor) = self.observed else {
            return Ok(false);
        };
        if ancestor.identity.kind != FileKind::Directory {
            return Ok(false);
        }
        if candidate
            .observed
            .is_some_and(|observed| observed.identity == ancestor.identity)
        {
            return Ok(true);
        }
        candidate.parent_chain_contains(|identity| identity == ancestor.identity)
    }

    fn is_ancestor_of_fd(&self, candidate: BorrowedFd<'_>) -> io::Result<bool> {
        let Some(ancestor) = self.observed else {
            return Ok(false);
        };
        if ancestor.identity.kind != FileKind::Directory {
            return Ok(false);
        }
        if observe_fd(candidate)?.identity == ancestor.identity {
            return Ok(true);
        }
        directory_chain_contains(candidate, |identity| identity == ancestor.identity)
    }

    fn parent_chain_contains(
        &self,
        predicate: impl FnMut(EntryIdentity) -> bool,
    ) -> io::Result<bool> {
        directory_chain_contains(self.parent.as_fd(), predicate)
    }
}

fn directory_chain_contains(
    start: BorrowedFd<'_>,
    mut predicate: impl FnMut(EntryIdentity) -> bool,
) -> io::Result<bool> {
    let mut current = fs::openat(start, ".", DIRECTORY_FLAGS, Mode::empty())?;
    loop {
        let current_identity = observe_fd(current.as_fd())?.identity;
        if predicate(current_identity) {
            return Ok(true);
        }
        let parent = fs::openat(current.as_fd(), "..", DIRECTORY_FLAGS, Mode::empty())?;
        let parent_identity = observe_fd(parent.as_fd())?.identity;
        if parent_identity == current_identity {
            return Ok(false);
        }
        current = parent;
    }
}

struct Mapping {
    source: PathBuf,
    destination: PathBuf,
    destination_name: Option<OsString>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct EntryKey {
    parent: EntryIdentity,
    name: OsString,
}

pub(crate) fn preflight(
    cwd: BorrowedFd<'_>,
    request: &PolicyRequest<'_>,
) -> Result<Preflight, PathError> {
    let (mappings, into_directory) = mappings(request, cwd)?;
    let mut operands = Vec::with_capacity(mappings.len());
    let mut outputs = BTreeMap::new();
    let mut source_entries = BTreeMap::new();

    for mapping in mappings {
        let source = mapping.source;
        let destination = mapping.destination;
        let source_root = EntryRoot::establish(cwd, &source, Requirement::Existing)
            .map_err(|error| PathError::new(&source, error))?;
        let source_observed = source_root.observed.ok_or_else(|| {
            PathError::new(&source, io::Error::other("source disappeared during preflight"))
        })?;
        let destination_root = establish_destination(
            cwd,
            into_directory.as_ref(),
            &destination,
            mapping.destination_name.as_deref(),
            Requirement::MayBeAbsent,
        )
        .map_err(|error| PathError::new(&destination, error))?;
        let source_parent = source_root
            .parent_identity()
            .map_err(|error| PathError::new(&source, error))?;
        let destination_parent = destination_root
            .parent_identity()
            .map_err(|error| PathError::new(&destination, error))?;
        let source_key = EntryKey {
            parent: source_parent,
            name: source_root.name.clone(),
        };
        let destination_key = EntryKey {
            parent: destination_parent,
            name: destination_root.name.clone(),
        };

        let index = operands.len();
        if source_entries.insert(source_key, index).is_some() {
            return Err(PathError::new(
                &source,
                io::Error::new(io::ErrorKind::InvalidInput, "source operand is repeated"),
            ));
        }
        if outputs.insert(destination_key, index).is_some() {
            return Err(PathError::new(
                &destination,
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "multiple sources map to the same output",
                ),
            ));
        }

        operands.push(PlannedOperand {
            source,
            destination,
            destination_name: destination_root.name.clone(),
            source_parent,
            destination_parent,
            source_observed,
            destination_observed: destination_root.observed,
            action: Action::Transfer,
        });
    }

    if let Some(source_index) = source_entries.iter().find_map(|(source, source_index)| {
        outputs
            .get(source)
            .is_some_and(|destination_index| destination_index != source_index)
            .then_some(*source_index)
    }) {
        return Err(PathError::new(
            &operands[source_index].source,
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "an output conflicts with another source operand",
            ),
        ));
    }

    validate_ancestry(cwd, into_directory.as_ref(), &operands)?;
    validate_move_roots(request, cwd, &operands)?;

    for operand in &mut operands {
        let source = reestablish(
            cwd,
            &operand.source,
            Requirement::Existing,
            operand.source_parent,
            Some(operand.source_observed),
        )
        .map_err(|error| PathError::new(&operand.source, error))?;
        let destination = reestablish_destination(cwd, into_directory.as_ref(), operand)
            .map_err(|error| PathError::new(&operand.destination, error))?;
        let target = if source
            .same_entry(&destination)
            .map_err(|error| PathError::new(&operand.source, error))?
        {
            Target::SameEntry
        } else if let Some(observed) = operand.destination_observed {
            Target::Existing(observed)
        } else {
            Target::Absent
        };
        operand.action = decide(operand.source_observed, target, &request.options)
            .map_err(|error| PathError::new(&operand.destination, error))?;
    }

    Ok(Preflight {
        operands,
        into_directory,
    })
}

fn mappings(
    request: &PolicyRequest<'_>,
    cwd: BorrowedFd<'_>,
) -> Result<(Vec<Mapping>, Option<OwnedFd>), PathError> {
    match request.operands {
        crate::transfer::Operands::Exact {
            source,
            destination,
        } => {
            let mut mappings = Vec::with_capacity(1);
            let mut memory_bytes = 0;
            let destination = exact_destination(request, cwd, source, destination);
            push_mapping(
                &mut mappings,
                &mut memory_bytes,
                source.clone(),
                destination,
                None,
            )
            .map_err(|error| PathError::new(source, error))?;
            Ok((mappings, None))
        }
        crate::transfer::Operands::Into {
            directory,
            first,
            rest,
        } => {
            let into_root = EntryRoot::establish(cwd, directory, Requirement::Existing)
                .map_err(|error| PathError::new(directory, error))?;
            let into_directory = into_root
                .open_directory_following_final()
                .map_err(|error| PathError::new(directory, error))?;
            let sources = std::iter::once(first).chain(rest.iter());
            let mut mappings = Vec::new();
            let mut memory_bytes = 0;
            for source in sources {
                let name = into_basename(source).map_err(|error| PathError::new(source, error))?;
                let mut destination = directory.clone();
                destination.push(name);
                push_mapping(
                    &mut mappings,
                    &mut memory_bytes,
                    source.clone(),
                    destination,
                    Some(name.to_os_string()),
                )
                .map_err(|error| PathError::new(source, error))?;
            }
            Ok((mappings, Some(into_directory)))
        }
    }
}

fn exact_destination(
    request: &PolicyRequest<'_>,
    cwd: BorrowedFd<'_>,
    source: &Path,
    destination: &Path,
) -> PathBuf {
    let Ok(destination_root) = EntryRoot::establish(cwd, destination, Requirement::Existing) else {
        return destination.to_path_buf();
    };
    if !destination_root
        .observed
        .is_some_and(|observed| observed.identity.kind == FileKind::Directory)
    {
        return destination.to_path_buf();
    }
    let Ok(name) = into_basename(source) else {
        return destination.to_path_buf();
    };
    if request.options.merge {
        let Ok(source_root) = EntryRoot::establish(cwd, source, Requirement::Existing) else {
            return destination.to_path_buf();
        };
        if source_root
            .observed
            .is_some_and(|observed| observed.identity.kind == FileKind::Directory)
        {
            return destination.to_path_buf();
        }
    }
    let mut rewritten = destination.to_path_buf();
    rewritten.push(name);
    rewritten
}

fn push_mapping(
    mappings: &mut Vec<Mapping>,
    memory_bytes: &mut usize,
    source: PathBuf,
    destination: PathBuf,
    destination_name: Option<OsString>,
) -> io::Result<()> {
    let fixed_bytes = std::mem::size_of::<PlannedOperand>()
        .checked_add(std::mem::size_of::<Mapping>())
        .and_then(|bytes| bytes.checked_add(std::mem::size_of::<EntryKey>() * 2))
        .and_then(|bytes| bytes.checked_add(std::mem::size_of::<usize>() * 8))
        .ok_or_else(|| io::Error::other("preflight mapping budget overflow"))?;
    let source_bytes = source.as_os_str().as_bytes().len().checked_mul(2);
    let destination_bytes = destination.as_os_str().as_bytes().len().checked_mul(3);
    *memory_bytes = memory_bytes
        .checked_add(fixed_bytes)
        .and_then(|size| size.checked_add(source_bytes?))
        .and_then(|size| size.checked_add(destination_bytes?))
        .ok_or_else(|| io::Error::other("preflight mapping budget overflow"))?;
    if *memory_bytes > PREFLIGHT_MEMORY_BYTES {
        return Err(io::Error::other(
            "preflight mappings exceed the operation record budget",
        ));
    }
    mappings.push(Mapping {
        source,
        destination,
        destination_name,
    });
    Ok(())
}

fn establish_destination(
    cwd: BorrowedFd<'_>,
    into_directory: Option<&OwnedFd>,
    path: &Path,
    name: Option<&OsStr>,
    requirement: Requirement,
) -> io::Result<EntryRoot> {
    match (into_directory, name) {
        (Some(parent), Some(name)) => {
            EntryRoot::establish_at(parent.as_fd(), name, requirement)
        }
        (None, None) => EntryRoot::establish(cwd, path, requirement),
        _ => Err(io::Error::other(
            "mapping parent and destination name disagree",
        )),
    }
}

fn validate_ancestry(
    cwd: BorrowedFd<'_>,
    into_directory: Option<&OwnedFd>,
    operands: &[PlannedOperand],
) -> Result<(), PathError> {
    let mut source_directories: BTreeMap<EntryIdentity, Vec<usize>> = BTreeMap::new();
    let mut destination_directories: BTreeMap<EntryIdentity, Vec<usize>> = BTreeMap::new();

    for (index, operand) in operands.iter().enumerate() {
        if operand.source_observed.identity.kind == FileKind::Directory {
            source_directories
                .entry(operand.source_observed.identity)
                .or_default()
                .push(index);
        }
        if let Some(destination) = operand.destination_observed
            && destination.identity.kind == FileKind::Directory
        {
            destination_directories
                .entry(destination.identity)
                .or_default()
                .push(index);
        }
    }

    for (identity, source_indices) in &source_directories {
        let Some(destination_indices) = destination_directories.get(identity) else {
            continue;
        };
        for source_index in source_indices {
            for destination_index in destination_indices {
                if source_index != destination_index {
                    let path = &operands[*source_index].source;
                    return Err(PathError::new(
                        path,
                        io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "source and destination identify the same directory",
                        ),
                    ));
                }
                let operand = &operands[*source_index];
                let source = reestablish(
                    cwd,
                    &operand.source,
                    Requirement::Existing,
                    operand.source_parent,
                    Some(operand.source_observed),
                )
                .map_err(|error| PathError::new(&operand.source, error))?;
                let destination = reestablish_destination(cwd, into_directory, operand)
                    .map_err(|error| PathError::new(&operand.destination, error))?;
                if !source
                    .same_entry(&destination)
                    .map_err(|error| PathError::new(&operand.source, error))?
                {
                    return Err(PathError::new(
                        &operand.source,
                        io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "source and destination identify the same directory",
                        ),
                    ));
                }
            }
        }
    }

    for (index, operand) in operands.iter().enumerate() {
        let source = reestablish(
            cwd,
            &operand.source,
            Requirement::Existing,
            operand.source_parent,
            Some(operand.source_observed),
        )
        .map_err(|error| PathError::new(&operand.source, error))?;
        let destination = reestablish_destination(cwd, into_directory, operand)
            .map_err(|error| PathError::new(&operand.destination, error))?;
        let source_key =
            entry_key(&source).map_err(|error| PathError::new(&operand.source, error))?;
        let destination_key =
            entry_key(&destination).map_err(|error| PathError::new(&operand.destination, error))?;
        let same_mapping_entry = source_key == destination_key;

        if operand.source_observed.identity.kind == FileKind::Directory
            && source_directories
                .get(&operand.source_observed.identity)
                .is_some_and(|indices| indices.len() > 1)
        {
            return Err(PathError::new(
                &operand.source,
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "source directory operands overlap",
                ),
            ));
        }

        if source
            .parent_chain_contains(|identity| {
                source_directories
                    .get(&identity)
                    .is_some_and(|indices| indices.iter().any(|other| *other != index))
            })
            .map_err(|error| PathError::new(&operand.source, error))?
        {
            return Err(PathError::new(
                &operand.source,
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "source operands overlap by directory ancestry",
                ),
            ));
        }

        if source
            .parent_chain_contains(|identity| {
                destination_directories.get(&identity).is_some_and(|indices| {
                    indices
                        .iter()
                        .any(|other| *other != index || !same_mapping_entry)
                })
            })
            .map_err(|error| PathError::new(&operand.source, error))?
        {
            return Err(PathError::new(
                &operand.source,
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "source and destination overlap by directory ancestry",
                ),
            ));
        }

        if !same_mapping_entry
            && operand.source_observed.identity.kind == FileKind::Directory
            && destination
                .observed
                .is_some_and(|observed| observed.identity == operand.source_observed.identity)
        {
            return Err(PathError::new(
                &operand.source,
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "source and destination identify the same directory",
                ),
            ));
        }

        if destination
            .parent_chain_contains(|identity| {
                source_directories.get(&identity).is_some_and(|indices| {
                    indices
                        .iter()
                        .any(|other| *other != index || !same_mapping_entry)
                })
            })
            .map_err(|error| PathError::new(&operand.destination, error))?
        {
            return Err(PathError::new(
                &operand.destination,
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "destination lies within a source directory",
                ),
            ));
        }

        if let Some(destination) = destination
            .observed
            .filter(|observed| observed.identity.kind == FileKind::Directory)
            && destination_directories
                .get(&destination.identity)
                .is_some_and(|indices| indices.iter().any(|other| *other != index))
        {
            return Err(PathError::new(
                &operand.destination,
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "destination directory is mapped more than once",
                ),
            ));
        }
    }

    for operand in operands {
        let source = reestablish(
            cwd,
            &operand.source,
            Requirement::Existing,
            operand.source_parent,
            Some(operand.source_observed),
        )
        .map_err(|error| PathError::new(&operand.source, error))?;
        let destination = reestablish_destination(cwd, into_directory, operand)
            .map_err(|error| PathError::new(&operand.destination, error))?;
        let source_key =
            entry_key(&source).map_err(|error| PathError::new(&operand.source, error))?;
        let destination_key =
            entry_key(&destination).map_err(|error| PathError::new(&operand.destination, error))?;
        if source_key == destination_key {
            continue;
        }
        if source
            .is_ancestor_of(&destination)
            .map_err(|error| PathError::new(&operand.source, error))?
            || destination
                .is_ancestor_of(&source)
                .map_err(|error| PathError::new(&operand.destination, error))?
        {
            return Err(PathError::new(
                &operand.source,
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "source and destination overlap by directory ancestry",
                ),
            ));
        }
    }

    Ok(())
}

fn validate_move_roots(
    request: &PolicyRequest<'_>,
    cwd: BorrowedFd<'_>,
    operands: &[PlannedOperand],
) -> Result<(), PathError> {
    if request.operation != Operation::Move {
        return Ok(());
    }
    let home = request
        .home
        .map(|path| {
            EntryRoot::establish(cwd, path, Requirement::Existing)
                .and_then(|root| root.open_directory_following_final())
                .map_err(|error| PathError::new(path, error))
        })
        .transpose()?;
    let home_identity = home
        .as_ref()
        .map(|directory| {
            observe_fd(directory.as_fd())
                .map(|observed| observed.identity)
                .map_err(|error| {
                    PathError::new(
                        request.home.unwrap_or_else(|| Path::new(".")),
                        error,
                    )
                })
        })
        .transpose()?;
    for operand in operands {
        if operand.source_observed.identity.kind != FileKind::Directory {
            continue;
        }
        let source = reestablish(
            cwd,
            &operand.source,
            Requirement::Existing,
            operand.source_parent,
            Some(operand.source_observed),
        )
        .map_err(|error| PathError::new(&operand.source, error))?;
        if source
            .parent_identity()
            .map_err(|error| PathError::new(&operand.source, error))?
            == operand.destination_parent
            && source.name == operand.destination_name
        {
            continue;
        }
        let parent_identity = source
            .parent_identity()
            .map_err(|error| PathError::new(&operand.source, error))?;
        let is_mount_root = source
            .is_mount_root()
            .map_err(|error| PathError::new(&operand.source, error))?;
        let protects_cwd = source
            .is_ancestor_of_fd(cwd)
            .map_err(|error| PathError::new(&operand.source, error))?;
        let protects_home =
            home_identity.is_some_and(|identity| identity == operand.source_observed.identity);
        if operand.source_observed.identity == parent_identity
            || is_mount_root
            || protects_cwd
            || protects_home
        {
            return Err(PathError::new(
                &operand.source,
                io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "move source is a protected filesystem or shell root",
                ),
            ));
        }
    }
    Ok(())
}

fn reestablish(
    cwd: BorrowedFd<'_>,
    path: &Path,
    requirement: Requirement,
    expected_parent: EntryIdentity,
    expected_observed: Option<Observation>,
) -> io::Result<EntryRoot> {
    let root = EntryRoot::establish(cwd, path, requirement)?;
    if root.parent_identity()? != expected_parent
        || !same_observation(expected_observed, root.observed)
    {
        return Err(io::Error::other("path identity changed during preflight"));
    }
    Ok(root)
}

fn entry_key(root: &EntryRoot) -> io::Result<EntryKey> {
    Ok(EntryKey {
        parent: root.parent_identity()?,
        name: root.name.clone(),
    })
}

fn reestablish_destination(
    cwd: BorrowedFd<'_>,
    into_directory: Option<&OwnedFd>,
    operand: &PlannedOperand,
) -> io::Result<EntryRoot> {
    let root = establish_destination(
        cwd,
        into_directory,
        &operand.destination,
        into_directory.map(|_| operand.destination_name.as_os_str()),
        Requirement::MayBeAbsent,
    )?;
    if root.parent_identity()? != operand.destination_parent
        || !same_observation(operand.destination_observed, root.observed)
    {
        return Err(io::Error::other(
            "destination identity changed during preflight",
        ));
    }
    Ok(root)
}

fn same_observation(expected: Option<Observation>, actual: Option<Observation>) -> bool {
    match (expected, actual) {
        (Some(expected), Some(actual)) => expected.same_version(actual),
        (None, None) => true,
        _ => false,
    }
}
