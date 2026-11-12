use std::ffi::{OsStr, OsString};
use std::io;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

use rustix::fs::{self, AtFlags, FileType, Mode, OFlags};

use super::{
    DIRECTORY_FLAGS, READ_FILE_FLAGS, WRITE_NEW_FLAGS, capture, copy_data, observe, observe_fd,
    require_unchanged, sync_fd,
};
use crate::FileKind;
use crate::transfer::{
    Cancellation, MetadataTarget, Observation, Preservation, SyncOperands, SyncProgress,
    SyncRequest, TRANSFER_BUFFER_BYTES, into_basename,
};

const DIRECTORY_FOLLOW: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::CLOEXEC);
const SYNC_TEMP_PREFIX: &[u8] = b".xsh-sync-";

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct FileWork {
    name: OsString,
    observed: Observation,
}

struct Mapping {
    source: PathBuf,
    destination: PathBuf,
}

struct SyncSession<'a> {
    cwd: BorrowedFd<'a>,
    options: crate::transfer::SyncOptions,
    cancellation: &'a Cancellation,
    progress: SyncProgress,
    failed: bool,
}

pub(crate) fn sync(request: SyncRequest<'_>) -> io::Result<SyncProgress> {
    let SyncRequest {
        cwd,
        operands,
        options,
        cancellation,
    } = request;
    let mut session = SyncSession {
        cwd,
        options,
        cancellation,
        progress: SyncProgress::default(),
        failed: false,
    };
    match &operands {
        SyncOperands::Exact {
            source,
            destination,
        } => session.sync_exact(source, destination)?,
        SyncOperands::Into { directory, sources } => session.sync_into(directory, sources)?,
        SyncOperands::Contents { source, directory } => {
            session.sync_contents(source, directory)?;
        }
    }
    if cancellation.is_cancelled() {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "transfer cancelled",
        ));
    }
    if session.failed {
        return Err(io::Error::other("synchronization reported failures"));
    }
    Ok(session.progress)
}

impl SyncSession<'_> {
    fn sync_exact(&mut self, source: &Path, destination: &Path) -> io::Result<()> {
        validate_mappings(
            self.cwd,
            &[Mapping {
                source: source.to_path_buf(),
                destination: destination.to_path_buf(),
            }],
        )?;
        let source_parent = self.open_operand_parent(source)?;
        let source_name = operand_name(source)?;
        let destination_parent = self.open_operand_parent(destination)?;
        let destination_name = operand_name(destination)?;
        let mut pending = Vec::new();
        self.sync_pair(
            source_parent.as_fd(),
            source_name,
            destination_parent.as_fd(),
            destination_name,
            &mut pending,
        )?;
        self.flush_files(
            source_parent.as_fd(),
            destination_parent.as_fd(),
            pending,
        )
    }

    fn sync_into(&mut self, directory: &Path, sources: &[PathBuf]) -> io::Result<()> {
        let mut mappings = Vec::with_capacity(sources.len());
        for source in sources {
            let name = into_basename(source)?;
            let mut destination = directory.to_path_buf();
            destination.push(name);
            mappings.push(Mapping {
                source: source.clone(),
                destination,
            });
        }
        validate_mappings(self.cwd, &mappings)?;
        let destination_parent = self.open_directory(directory)?;
        let identity = observe_fd(destination_parent.as_fd())?.identity;
        for source in sources {
            if self.cancellation.is_cancelled() {
                return Ok(());
            }
            let source_parent = self.open_operand_parent(source)?;
            let source_name = into_basename(source)?;
            let mut pending = Vec::new();
            self.sync_pair(
                source_parent.as_fd(),
                source_name,
                destination_parent.as_fd(),
                source_name,
                &mut pending,
            )?;
            self.flush_files(
                source_parent.as_fd(),
                destination_parent.as_fd(),
                pending,
            )?;
        }
        if observe_fd(destination_parent.as_fd())?.identity != identity {
            return Err(io::Error::other("sync destination changed"));
        }
        Ok(())
    }

    fn sync_contents(&mut self, source: &Path, directory: &Path) -> io::Result<()> {
        validate_mappings(
            self.cwd,
            &[Mapping {
                source: source.to_path_buf(),
                destination: directory.to_path_buf(),
            }],
        )?;
        let source_parent = self.open_operand_parent(source)?;
        let source_name = operand_name(source)?;
        let destination_parent = self.open_directory(directory)?;
        let source_observed = observe(source_parent.as_fd(), source_name)?;
        if source_observed.identity.kind != FileKind::Directory {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "--contents source must be a directory",
            ));
        }
        let source_directory =
            open_directory_at(source_parent.as_fd(), source_name, source_observed)?;
        self.sync_contents_at(source_directory.as_fd(), destination_parent.as_fd())?;
        require_unchanged_fd(source_observed, source_directory.as_fd())
    }

    fn sync_pair(
        &mut self,
        source_parent: BorrowedFd<'_>,
        source_name: &OsStr,
        destination_parent: BorrowedFd<'_>,
        destination_name: &OsStr,
        pending: &mut Vec<FileWork>,
    ) -> io::Result<()> {
        if self.cancellation.is_cancelled() {
            return Ok(());
        }
        let source = observe(source_parent, source_name)?;
        let destination = match observe(destination_parent, destination_name) {
            Ok(observed) => Some(observed),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        self.progress.examined_files += 1;
        match source.identity.kind {
            FileKind::Directory => {
                let source_directory = open_directory_at(source_parent, source_name, source)?;
                match destination {
                    Some(destination) if destination.identity.kind == FileKind::Directory => {
                        let destination_directory = open_directory_at(
                            destination_parent,
                            destination_name,
                            destination,
                        )?;
                        self.sync_contents_at(
                            source_directory.as_fd(),
                            destination_directory.as_fd(),
                        )?;
                        if self.options.engine.durable && !self.options.dry_run {
                            sync_fd(destination_directory.as_fd())?;
                        }
                        require_unchanged_fd(source, source_directory.as_fd())
                    }
                    Some(replaced) => {
                        self.progress.transferred_files += 1;
                        if self.options.dry_run {
                            return Ok(());
                        }
                        remove_destination_entry(destination_parent, destination_name, replaced)?;
                        fs::mkdirat(
                            destination_parent,
                            destination_name,
                            Mode::from_raw_mode(0o700),
                        )?;
                        let created = observe(destination_parent, destination_name)?;
                        let destination_directory =
                            open_directory_at(destination_parent, destination_name, created)?;
                        self.sync_contents_at(
                            source_directory.as_fd(),
                            destination_directory.as_fd(),
                        )?;
                        let metadata = capture(MetadataTarget::Fd(source_directory.as_fd()))?;
                        super::apply(
                            MetadataTarget::Fd(destination_directory.as_fd()),
                            &metadata,
                            Preservation::Copy,
                        )?;
                        sync_fd(destination_directory.as_fd())?;
                        sync_fd(destination_parent)?;
                        require_unchanged_fd(source, source_directory.as_fd())
                    }
                    None => {
                        self.progress.transferred_files += 1;
                        if self.options.dry_run {
                            return Ok(());
                        }
                        fs::mkdirat(
                            destination_parent,
                            destination_name,
                            Mode::from_raw_mode(0o700),
                        )?;
                        let created = observe(destination_parent, destination_name)?;
                        let destination_directory =
                            open_directory_at(destination_parent, destination_name, created)?;
                        self.sync_contents_at(
                            source_directory.as_fd(),
                            destination_directory.as_fd(),
                        )?;
                        let metadata = capture(MetadataTarget::Fd(source_directory.as_fd()))?;
                        super::apply(
                            MetadataTarget::Fd(destination_directory.as_fd()),
                            &metadata,
                            Preservation::Copy,
                        )?;
                        sync_fd(destination_directory.as_fd())?;
                        sync_fd(destination_parent)?;
                        require_unchanged_fd(source, source_directory.as_fd())
                    }
                }
            }
            FileKind::File => {
                if let Some(destination) = destination
                    && destination.identity.kind == FileKind::File
                    && destination.size == source.size
                    && destination.mtime == source.mtime
                {
                    self.progress.skipped_files += 1;
                    return Ok(());
                }
                if let Some(replaced) = destination
                    && replaced.identity.kind != FileKind::File
                {
                    if self.options.dry_run {
                        self.progress.transferred_files += 1;
                        self.progress.logical_bytes += source.size;
                        return Ok(());
                    }
                    remove_destination_entry(destination_parent, destination_name, replaced)?;
                }
                self.progress.transferred_files += 1;
                self.progress.logical_bytes += source.size;
                if self.options.dry_run {
                    return Ok(());
                }
                pending.push(FileWork {
                    name: destination_name.to_os_string(),
                    observed: source,
                });
                Ok(())
            }
            FileKind::Symlink => {
                let target = fs::readlinkat(source_parent, source_name, Vec::new())?;
                if let Some(destination) = destination
                    && destination.identity.kind == FileKind::Symlink
                    && fs::readlinkat(destination_parent, destination_name, Vec::new())? == target
                {
                    self.progress.skipped_files += 1;
                    return Ok(());
                }
                self.progress.transferred_files += 1;
                if self.options.dry_run {
                    return Ok(());
                }
                if destination.is_some() {
                    fs::unlinkat(destination_parent, destination_name, AtFlags::empty())?;
                }
                fs::symlinkat(target.as_c_str(), destination_parent, destination_name)?;
                Ok(())
            }
            _ => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "unsupported special file type",
            )),
        }
    }

    fn sync_contents_at(
        &mut self,
        source: BorrowedFd<'_>,
        destination: BorrowedFd<'_>,
    ) -> io::Result<()> {
        let source_root = observe_fd(source)?;
        let mut names = Vec::new();
        for entry in fs::Dir::read_from(source)? {
            if self.cancellation.is_cancelled() {
                return Ok(());
            }
            let entry = entry?;
            let bytes = entry.file_name().to_bytes();
            if bytes == b"." || bytes == b".." {
                continue;
            }
            names.push(OsString::from_vec(bytes.to_vec()));
        }
        names.sort();
        if self.options.delete {
            let mut extras = Vec::new();
            for entry in fs::Dir::read_from(destination)? {
                let entry = entry?;
                let bytes = entry.file_name().to_bytes();
                if bytes == b"." || bytes == b".." {
                    continue;
                }
                if names
                    .binary_search_by(|name| name.as_bytes().cmp(bytes))
                    .is_err()
                {
                    extras.push(OsString::from_vec(bytes.to_vec()));
                }
            }
            extras.sort();
            for name in extras {
                if self.cancellation.is_cancelled() {
                    return Ok(());
                }
                self.delete_entry(destination, &name)?;
            }
        }
        let mut pending = Vec::new();
        for name in names {
            if self.cancellation.is_cancelled() {
                return Ok(());
            }
            let name = OsStr::from_bytes(name.as_bytes());
            self.sync_pair(source, name, destination, name, &mut pending)?;
        }
        self.flush_files(source, destination, pending)?;
        if !source_root.same_version(observe_fd(source)?) {
            return Err(io::Error::other("sync source directory changed"));
        }
        Ok(())
    }

    fn flush_files(
        &mut self,
        source: BorrowedFd<'_>,
        destination: BorrowedFd<'_>,
        pending: Vec<FileWork>,
    ) -> io::Result<()> {
        if pending.is_empty() || self.options.dry_run {
            return Ok(());
        }
        let jobs = super::worker_count(&self.options.engine)?.min(pending.len()).max(1);
        let queue = Mutex::new(pending.into_iter());
        let failure = Mutex::new(None::<io::Error>);
        let stop = AtomicBool::new(false);
        let streamed = Mutex::new(0_u64);
        std::thread::scope(|scope| {
            for _ in 0..jobs {
                let queue = &queue;
                let failure = &failure;
                let stop = &stop;
                let streamed = &streamed;
                let durable = self.options.engine.durable;
                let cancellation = self.cancellation;
                scope.spawn(move || {
                    let mut buffer = vec![0; TRANSFER_BUFFER_BYTES];
                    loop {
                        if cancellation.is_cancelled() || stop.load(Ordering::Acquire) {
                            break;
                        }
                        let work = queue.lock().unwrap().next();
                        let Some(work) = work else { break };
                        match copy_staged(
                            source,
                            &work.name,
                            destination,
                            &work.name,
                            work.observed,
                            &mut buffer,
                            durable,
                            cancellation,
                        ) {
                            Ok(stats) => *streamed.lock().unwrap() += stats.streamed_bytes,
                            Err(error) => {
                                stop.store(true, Ordering::Release);
                                let mut failure = failure.lock().unwrap();
                                if failure.is_none() {
                                    *failure = Some(error);
                                }
                            }
                        }
                    }
                });
            }
        });
        self.progress.streamed_bytes += streamed.into_inner().unwrap();
        if let Some(error) = failure.into_inner().unwrap() {
            return Err(error);
        }
        Ok(())
    }

    fn delete_entry(&mut self, parent: BorrowedFd<'_>, name: &OsStr) -> io::Result<()> {
        let observed = match observe(parent, name) {
            Ok(observed) => observed,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(self.deletion_failed(error)),
        };
        if observed.identity.kind == FileKind::Directory {
            let directory = match open_directory_at(parent, name, observed) {
                Ok(directory) => directory,
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
                Err(error) => return Err(self.deletion_failed(error)),
            };
            let mut children = Vec::new();
            for entry in fs::Dir::read_from(directory.as_fd())? {
                let entry = entry?;
                let bytes = entry.file_name().to_bytes();
                if bytes == b"." || bytes == b".." {
                    continue;
                }
                children.push(OsString::from_vec(bytes.to_vec()));
            }
            children.sort();
            for child in children {
                self.delete_entry(directory.as_fd(), &child)?;
            }
            self.progress.deleted_files += 1;
            if self.options.dry_run {
                return Ok(());
            }
            match fs::unlinkat(parent, name, AtFlags::REMOVEDIR) {
                Ok(()) => Ok(()),
                Err(error) if error == rustix::io::Errno::NOENT => {
                    self.progress.deleted_files -= 1;
                    Ok(())
                }
                Err(error) => Err(self.deletion_failed(error.into())),
            }
        } else {
            self.progress.deleted_files += 1;
            if self.options.dry_run {
                return Ok(());
            }
            match fs::unlinkat(parent, name, AtFlags::empty()) {
                Ok(()) => Ok(()),
                Err(error) if error == rustix::io::Errno::NOENT => {
                    self.progress.deleted_files -= 1;
                    Ok(())
                }
                Err(error) => Err(self.deletion_failed(error.into())),
            }
        }
    }

    fn deletion_failed(&mut self, error: io::Error) -> io::Error {
        self.failed = true;
        io::Error::new(error.kind(), format!("cannot delete: {error}"))
    }

    fn open_operand_parent(&self, path: &Path) -> io::Result<OwnedFd> {
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::openat(self.cwd, parent, DIRECTORY_FOLLOW, Mode::empty()).map_err(Into::into)
    }

    fn open_directory(&self, path: &Path) -> io::Result<OwnedFd> {
        let directory = fs::openat(self.cwd, path, DIRECTORY_FOLLOW, Mode::empty())?;
        if FileType::from_raw_mode(fs::fstat(directory.as_fd())?.st_mode) != FileType::Directory {
            return Err(io::Error::new(
                io::ErrorKind::NotADirectory,
                "sync destination is not a directory",
            ));
        }
        Ok(directory)
    }
}

#[allow(clippy::too_many_arguments)]
fn copy_staged(
    source_parent: BorrowedFd<'_>,
    source_name: &OsStr,
    destination_parent: BorrowedFd<'_>,
    destination_name: &OsStr,
    observed: Observation,
    buffer: &mut [u8],
    durable: bool,
    cancellation: &Cancellation,
) -> io::Result<crate::transfer::CopyStats> {
    cancellation.check()?;
    let source = fs::openat(source_parent, source_name, READ_FILE_FLAGS, Mode::empty())?;
    if !observed.same_version(observe_fd(source.as_fd())?) {
        return Err(io::Error::other("source changed while opening"));
    }
    let metadata = capture(MetadataTarget::Fd(source.as_fd()))?;
    let temp = OsString::from_vec(format!(
        "{}{}-{}",
        String::from_utf8_lossy(SYNC_TEMP_PREFIX),
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    )
    .into_bytes());
    let destination = fs::openat(
        destination_parent,
        &temp,
        WRITE_NEW_FLAGS,
        Mode::from_raw_mode(0o600),
    )?;
    let mut published = false;
    let result = (|| {
        let stats = copy_data(
            source.as_fd(),
            destination.as_fd(),
            observed.size,
            buffer,
            cancellation,
        )?;
        super::apply(
            MetadataTarget::Fd(destination.as_fd()),
            &metadata,
            Preservation::Copy,
        )?;
        if durable {
            sync_fd(destination.as_fd())?;
        }
        require_unchanged(source_parent, source_name, observed)?;
        fs::renameat(destination_parent, &temp, destination_parent, destination_name)?;
        published = true;
        if durable {
            sync_fd(destination_parent)?;
        }
        Ok(stats)
    })();
    match result {
        Ok(stats) => Ok(stats),
        Err(error) if published => Err(report_after_publication_error(error)),
        Err(error) => {
            let cleanup = fs::unlinkat(destination_parent, &temp, AtFlags::empty());
            Err(match cleanup {
                Ok(()) => error,
                Err(cleanup) if cleanup == rustix::io::Errno::NOENT => error,
                Err(cleanup) => io::Error::new(
                    error.kind(),
                    format!("{error}; cleaning the sync temporary also failed: {cleanup}"),
                ),
            })
        }
    }
}

fn report_after_publication_error(error: io::Error) -> io::Error {
    io::Error::new(
        error.kind(),
        format!("destination published but synchronization failed: {error}"),
    )
}

fn operand_name(path: &Path) -> io::Result<&OsStr> {
    path.file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "operand has no final name"))
}

fn open_directory_at(
    parent: BorrowedFd<'_>,
    name: &OsStr,
    expected: Observation,
) -> io::Result<OwnedFd> {
    let directory = fs::openat(parent, name, DIRECTORY_FLAGS, Mode::empty())?;
    if !expected.same_identity(observe_fd(directory.as_fd())?) {
        return Err(io::Error::other("directory identity changed"));
    }
    Ok(directory)
}

fn require_unchanged_fd(expected: Observation, fd: BorrowedFd<'_>) -> io::Result<()> {
    if expected.same_version(observe_fd(fd)?) {
        Ok(())
    } else {
        Err(io::Error::other("directory changed during synchronization"))
    }
}

fn validate_mappings(cwd: BorrowedFd<'_>, mappings: &[Mapping]) -> io::Result<()> {
    let mut entries = Vec::with_capacity(mappings.len());
    for mapping in mappings {
        let Ok(source) = establish_entry(cwd, &mapping.source) else {
            continue;
        };
        let destination = establish_entry(cwd, &mapping.destination).ok();
        match &destination {
            Some(destination) => {
                if same_entry(&source, destination)? {
                    return Err(overlap_error(&mapping.source));
                }
                if is_ancestor_of(&source, destination)?
                    || is_ancestor_of(destination, &source)?
                {
                    return Err(overlap_error(&mapping.source));
                }
            }
            None => {
                if let Some(ancestor) = nearest_existing_directory(cwd, &mapping.destination)
                    && is_ancestor_of(&source, &ancestor)?
                {
                    return Err(overlap_error(&mapping.source));
                }
            }
        }
        entries.push((mapping, source, destination));
    }
    for (index, (mapping, source, destination)) in entries.iter().enumerate() {
        for (other_index, (_other, other_source, other_destination)) in entries.iter().enumerate() {
            if index == other_index {
                continue;
            }
            if same_entry(source, other_source)? {
                return Err(mapping_error(&mapping.source, "source operand is repeated"));
            }
            if is_ancestor_of(source, other_source)? {
                return Err(mapping_error(
                    &mapping.source,
                    "source operands overlap by directory ancestry",
                ));
            }
            if let Some(other_destination) = other_destination {
                if same_entry(source, other_destination)? {
                    return Err(mapping_error(
                        &mapping.source,
                        "an output conflicts with another source operand",
                    ));
                }
                if is_ancestor_of(source, other_destination)? {
                    return Err(overlap_error(&mapping.source));
                }
            }
            if let Some(destination) = destination {
                if is_ancestor_of(destination, other_source)? {
                    return Err(mapping_error(
                        &mapping.destination,
                        "destination lies within a source directory",
                    ));
                }
                if let Some(other_destination) = other_destination {
                    if same_entry(destination, other_destination)? {
                        return Err(mapping_error(
                            &mapping.destination,
                            "multiple sources map to the same output",
                        ));
                    }
                    if is_ancestor_of(destination, other_destination)? {
                        return Err(mapping_error(
                            &mapping.destination,
                            "destination directories overlap by directory ancestry",
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

struct EstablishedEntry {
    observed: Observation,
    parent: OwnedFd,
    name: OsString,
}

fn establish_entry(cwd: BorrowedFd<'_>, path: &Path) -> io::Result<EstablishedEntry> {
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
    let parent = fs::openat(cwd, parent_path, DIRECTORY_FOLLOW, Mode::empty())?;
    let observed = observe(parent.as_fd(), name)?;
    Ok(EstablishedEntry {
        observed,
        parent,
        name: name.to_os_string(),
    })
}

fn same_entry(left: &EstablishedEntry, right: &EstablishedEntry) -> io::Result<bool> {
    Ok(left.name == right.name
        && observe_fd(left.parent.as_fd())?.identity == observe_fd(right.parent.as_fd())?.identity)
}

fn is_ancestor_of(ancestor: &EstablishedEntry, candidate: &EstablishedEntry) -> io::Result<bool> {
    if ancestor.observed.identity.kind != FileKind::Directory {
        return Ok(false);
    }
    if candidate.observed.identity == ancestor.observed.identity {
        return Ok(true);
    }
    let mut current = fs::openat(candidate.parent.as_fd(), c".", DIRECTORY_FLAGS, Mode::empty())?;
    loop {
        if observe_fd(current.as_fd())?.identity == ancestor.observed.identity {
            return Ok(true);
        }
        let parent = fs::openat(current.as_fd(), c"..", DIRECTORY_FLAGS, Mode::empty())?;
        let observed = observe_fd(current.as_fd())?;
        let parent_observed = observe_fd(parent.as_fd())?;
        if parent_observed.identity == observed.identity {
            return Ok(false);
        }
        current = parent;
    }
}

fn nearest_existing_directory(cwd: BorrowedFd<'_>, path: &Path) -> Option<EstablishedEntry> {
    let mut current = path.to_path_buf();
    loop {
        if let Ok(entry) = establish_entry(cwd, &current)
            && entry.observed.identity.kind == FileKind::Directory
        {
            return Some(entry);
        }
        let parent = current.parent()?.to_path_buf();
        if parent == current {
            return None;
        }
        current = parent;
    }
}

fn overlap_error(path: &Path) -> io::Error {
    mapping_error(path, "source and destination overlap by directory ancestry")
}

fn mapping_error(path: &Path, message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, format!("{path:?}: {message}"))
}

fn remove_destination_entry(
    parent: BorrowedFd<'_>,
    name: &OsStr,
    observed: Observation,
) -> io::Result<()> {
    if observed.identity.kind != FileKind::Directory {
        return fs::unlinkat(parent, name, AtFlags::empty()).map_err(Into::into);
    }
    remove_directory_entry(parent, name, observed)
}

fn remove_directory_entry(
    parent: BorrowedFd<'_>,
    name: &OsStr,
    observed: Observation,
) -> io::Result<()> {
    let directory = open_directory_at(parent, name, observed)?;
    let mut children = Vec::new();
    for entry in fs::Dir::read_from(directory.as_fd())? {
        let entry = entry?;
        let bytes = entry.file_name().to_bytes();
        if bytes == b"." || bytes == b".." {
            continue;
        }
        children.push(OsString::from_vec(bytes.to_vec()));
    }
    children.sort();
    for child in children {
        let child_name = OsStr::from_bytes(child.as_bytes());
        let child_observed = observe(directory.as_fd(), child_name)?;
        remove_destination_entry(directory.as_fd(), child_name, child_observed)?;
    }
    fs::unlinkat(parent, name, AtFlags::REMOVEDIR).map_err(Into::into)
}
