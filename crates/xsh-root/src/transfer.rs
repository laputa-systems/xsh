//! Internal filesystem-transfer substrate: a metadata record, hardlink-group
//! preservation, cancellation, byte/hole progress, and a direct copy.
//!
//! Linux implements every operation. Other hosts return an unsupported error
//! at each entry point so callers stay portable. The API is a Rust boundary
//! only; it carries no script-visible contract.

use std::ffi::{OsStr, OsString};
use std::io;
use std::num::NonZeroUsize;
use std::os::fd::{BorrowedFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use crate::FileKind;

/// A byte buffer size for buffered transfers.
pub const TRANSFER_BUFFER_BYTES: usize = 256 * 1024;

/// The largest extended metadata capture one entry may allocate.
pub const METADATA_CAPTURE_BYTES: usize = 16 * 1024 * 1024;

/// The largest worker count a transfer may request.
pub const MAX_JOBS: usize = 32;

/// The number of descriptors the engine reserves outside the transfer budget.
pub const RESERVED_DESCRIPTORS: usize = 32;

/// The smallest descriptor budget that admits a transfer.
pub const MIN_DESCRIPTORS: usize = 16;

/// The largest descriptor budget a transfer claims.
pub const MAX_DESCRIPTORS: usize = 256;

/// The descriptors one worker holds while copying an entry.
pub const WORKER_DESCRIPTORS: usize = 6;

/// The in-process record budget reserved from the address space.
pub const RECORD_MEMORY_BYTES: usize = 64 * 1024 * 1024;

/// The address space one worker may hold: stack, copy buffer and metadata.
pub const WORKER_MEMORY_BYTES: usize =
    512 * 1024 + TRANSFER_BUFFER_BYTES + METADATA_CAPTURE_BYTES;

/// A requested number of transfer workers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Jobs(NonZeroUsize);

impl Jobs {
    /// Accepts a worker count in `1..=`[`MAX_JOBS`].
    pub fn new(count: usize) -> io::Result<Self> {
        match NonZeroUsize::new(count) {
            Some(count) if count.get() <= MAX_JOBS => Ok(Self(count)),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "jobs must be in 1..=32",
            )),
        }
    }

    /// The requested worker count.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0.get()
    }
}

/// Traversal, concurrency and durability choices for one engine operation.
#[derive(Clone, Copy, Debug, Default)]
pub struct EngineOptions {
    /// Descend into nested mount points instead of refusing them.
    pub cross_file_systems: bool,
    /// The requested worker count; the resource budget may refuse it.
    pub jobs: Option<Jobs>,
    /// Synchronize destination file contents and directory publication.
    pub durable: bool,
}

/// The kernel's mount identity for one open directory.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MountIdentity {
    /// The device that holds the mount.
    pub device: u64,
    /// The mount identifier reported by `statx`.
    pub mount_id: u64,
}

/// Reads the mount identity of an open directory through `statx`.
pub fn mount_id(fd: BorrowedFd<'_>) -> io::Result<MountIdentity> {
    platform::mount_identity(fd)
}

/// The number of workers the descriptor and address-space budgets admit.
///
/// An explicit [`EngineOptions::jobs`] above the budget is refused with
/// [`std::io::ErrorKind::InvalidInput`]; an implicit request that cannot admit
/// one worker fails operationally.
pub fn worker_count(options: &EngineOptions) -> io::Result<usize> {
    platform::worker_count(options)
}

/// A nanosecond-resolution file timestamp.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Timestamp {
    /// Whole seconds since the Unix epoch.
    pub seconds: i64,
    /// The subsecond part in nanoseconds.
    pub nanoseconds: u32,
}

/// The stable identity of one directory entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EntryIdentity {
    /// The device that holds the entry.
    pub device: u64,
    /// The inode number within that device.
    pub inode: u64,
    /// The object kind observed together with the identity.
    pub kind: FileKind,
}

impl PartialOrd for EntryIdentity {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for EntryIdentity {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.device, self.inode, kind_code(self.kind)).cmp(&(
            other.device,
            other.inode,
            kind_code(other.kind),
        ))
    }
}

/// One directory-entry observation used to revalidate a planned operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Observation {
    /// The stable identity of the observed entry.
    pub identity: EntryIdentity,
    /// The permission and set-id bits.
    pub mode: u32,
    /// The owning user.
    pub uid: u32,
    /// The owning group.
    pub gid: u32,
    /// The observed link count.
    pub links: u64,
    /// The logical length in bytes.
    pub size: u64,
    /// The access time.
    pub atime: Timestamp,
    /// The modification time.
    pub mtime: Timestamp,
    /// The inode change time.
    pub ctime: Timestamp,
}

impl Observation {
    /// Compares identity and every version field except access time.
    ///
    /// Reads update access time, so a comparison that must tolerate them
    /// excludes only that field.
    #[must_use]
    pub fn same_version(self, actual: Self) -> bool {
        let mut expected = self;
        expected.atime = actual.atime;
        expected == actual
    }

    /// Compares device, inode and kind without any version fields.
    #[must_use]
    pub fn same_identity(self, actual: Self) -> bool {
        self.identity == actual.identity
    }
}

fn kind_code(kind: FileKind) -> u8 {
    match kind {
        FileKind::File => 0,
        FileKind::Directory => 1,
        FileKind::Symlink => 2,
        FileKind::Fifo => 3,
        FileKind::CharDevice => 4,
        FileKind::BlockDevice => 5,
        FileKind::Socket => 6,
    }
}

#[cfg(target_os = "linux")]
fn kind_from_code(code: u8) -> io::Result<FileKind> {
    match code {
        0 => Ok(FileKind::File),
        1 => Ok(FileKind::Directory),
        2 => Ok(FileKind::Symlink),
        3 => Ok(FileKind::Fifo),
        4 => Ok(FileKind::CharDevice),
        5 => Ok(FileKind::BlockDevice),
        6 => Ok(FileKind::Socket),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unknown journal object kind",
        )),
    }
}

/// One extended attribute, preserved as raw bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Xattr {
    /// The attribute name.
    pub name: OsString,
    /// The attribute value without decoding.
    pub value: Vec<u8>,
}

/// A captured file metadata record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Metadata {
    /// The file kind observed at capture time.
    pub kind: FileKind,
    /// The permission and set-id bits.
    pub mode: u32,
    /// The owning user.
    pub uid: u32,
    /// The owning group.
    pub gid: u32,
    /// The observed link count.
    pub links: u64,
    /// The logical length in bytes.
    pub size: u64,
    /// The access time.
    pub atime: Timestamp,
    /// The modification time.
    pub mtime: Timestamp,
    /// The inode change time.
    pub ctime: Timestamp,
    /// Extended attributes other than the POSIX access ACL.
    pub xattrs: Vec<Xattr>,
    /// The raw `system.posix_acl_access` payload when the source has one.
    pub acl: Option<Vec<u8>>,
    /// The Linux inode flags, or zero when the host cannot report them.
    pub flags: u32,
}

impl Default for Metadata {
    fn default() -> Self {
        Self {
            kind: FileKind::File,
            mode: 0,
            uid: 0,
            gid: 0,
            links: 0,
            size: 0,
            atime: Timestamp::default(),
            mtime: Timestamp::default(),
            ctime: Timestamp::default(),
            xattrs: Vec::new(),
            acl: None,
            flags: 0,
        }
    }
}

/// Where a metadata record is captured from or applied to.
#[derive(Clone, Copy, Debug)]
pub enum MetadataTarget<'a> {
    /// An open file or directory descriptor.
    Fd(BorrowedFd<'a>),
    /// A symbolic link named in an open parent directory, accessed without
    /// following it.
    Symlink {
        /// The open parent directory.
        parent: BorrowedFd<'a>,
        /// The link's name within `parent`.
        name: &'a OsStr,
    },
}

/// How ownership is treated when metadata is applied.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Preservation {
    /// Keep the destination's owner. Privileged bits the new owner cannot hold
    /// are cleared and owner-bound capabilities are not copied.
    Copy,
    /// Make the destination's owner match the record before preserving
    /// everything.
    Move,
}

impl Metadata {
    /// Captures mode, ownership, timestamps, extended attributes, POSIX access
    /// ACL bytes and inode flags from `target`.
    pub fn capture(target: MetadataTarget<'_>) -> io::Result<Self> {
        platform::capture(target)
    }

    /// Applies this record to `target` with copy or move ownership rules.
    pub fn apply(
        &self,
        target: MetadataTarget<'_>,
        preservation: Preservation,
    ) -> io::Result<()> {
        platform::apply(target, self, preservation)
    }

    /// Applies only the inode flags that must be set after publication.
    ///
    /// Symlink targets cannot represent those flags on Linux and are refused
    /// with [`std::io::ErrorKind::Unsupported`].
    pub fn apply_final(&self, target: MetadataTarget<'_>) -> io::Result<()> {
        platform::apply_final(target, self)
    }
}

/// Why a transfer was cancelled.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancelReason {
    /// The user requested cancellation.
    User,
    /// The event sink or its output failed.
    Output,
}

/// A cooperative cancellation flag that workers poll.
#[derive(Clone, Debug, Default)]
pub struct Cancellation(Arc<AtomicU8>);

impl Cancellation {
    /// Requests user cancellation. It wins over an earlier output failure.
    pub fn cancel(&self) {
        self.0.store(2, Ordering::Release);
    }

    /// Requests cancellation after an output failure.
    pub fn cancel_for_output(&self) {
        let _ = self.0.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire);
    }

    /// Reports the first recorded reason.
    pub fn reason(&self) -> Option<CancelReason> {
        match self.0.load(Ordering::Acquire) {
            0 => None,
            1 => Some(CancelReason::Output),
            _ => Some(CancelReason::User),
        }
    }

    /// Reports whether any reason is recorded.
    pub fn is_cancelled(&self) -> bool {
        self.reason().is_some()
    }

    /// Fails with [`std::io::ErrorKind::Interrupted`] once cancelled.
    pub fn check(&self) -> io::Result<()> {
        if self.is_cancelled() {
            Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "transfer cancelled",
            ))
        } else {
            Ok(())
        }
    }
}

/// How the bytes of one file reached the destination.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CopyMethod {
    /// A reflink clone.
    Clone,
    /// Sparse extents moved by `copy_file_range`.
    #[default]
    Native,
    /// Bytes moved by read and write.
    Buffered,
}

/// Byte accounting for one copied file.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CopyStats {
    /// The strongest mechanism that contributed bytes.
    pub method: CopyMethod,
    /// The logical file length.
    pub logical_bytes: u64,
    /// Bytes materialized at the destination.
    pub streamed_bytes: u64,
    /// Bytes not materialized because they were sparse holes or reflink shares.
    pub hole_bytes: u64,
}

/// Accumulated transfer counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Progress {
    /// Entries found so far.
    pub discovered_files: u64,
    /// Entries fully transferred and finalized.
    pub completed_files: u64,
    /// Entries deliberately left alone.
    pub skipped_files: u64,
    /// Entries that failed.
    pub failed_files: u64,
    /// Logical bytes covered by copied files.
    pub logical_bytes: u64,
    /// Bytes materialized at the destination.
    pub streamed_bytes: u64,
    /// Bytes left as sparse holes or reflink shares.
    pub hole_bytes: u64,
    /// Files that used a reflink clone.
    pub cloned_files: u64,
    /// Regular files whose contents were synchronized in a durable transfer.
    pub synced_files: u64,
    /// Directories synchronized in a durable transfer.
    pub synced_directories: u64,
}

impl Progress {
    /// Adds one copied file's byte accounting.
    pub fn record(&mut self, stats: CopyStats) {
        self.logical_bytes = self.logical_bytes.saturating_add(stats.logical_bytes);
        self.streamed_bytes = self.streamed_bytes.saturating_add(stats.streamed_bytes);
        self.hole_bytes = self.hole_bytes.saturating_add(stats.hole_bytes);
        if stats.method == CopyMethod::Clone {
            self.cloned_files += 1;
        }
    }
}

/// One direct, non-staged transfer of a tree or leaf whose destination must be
/// absent.
pub struct TreeTransfer<'a> {
    /// The open directory holding the source entry.
    pub source_parent: BorrowedFd<'a>,
    /// The single source entry name within `source_parent`.
    pub source_name: &'a OsStr,
    /// The open parent directory for the destination.
    pub destination_parent: BorrowedFd<'a>,
    /// The single destination entry name within `destination_parent`.
    pub destination_name: &'a OsStr,
    /// Traversal and durability choices.
    pub engine: EngineOptions,
    /// Polled between entries and copy chunks.
    pub cancellation: &'a Cancellation,
    /// Updated as entries are discovered, completed and copied.
    pub progress: &'a mut Progress,
    /// Flags that must be applied only after publication.
    pub deferred_flags: &'a mut Vec<DeferredFlags>,
}

/// An inode flag set that must be applied after the object is published.
///
/// Linux refuses to rename or remove an object carrying these flags, so a
/// staged transfer records them and its caller applies them at the final name
/// with [`Metadata::apply_final`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeferredFlags {
    /// The entry's path relative to the transferred root, empty for the root.
    pub relative: PathBuf,
    /// The captured inode flags.
    pub flags: u32,
}


/// Copies regular-file bytes with the clone -> `copy_file_range` ->
/// read/write fallback chain.
///
/// The source must be seekable. Cancellation is checked before each kernel or
/// buffered operation.
pub fn copy_data(
    source: BorrowedFd<'_>,
    destination: BorrowedFd<'_>,
    length: u64,
    scratch: &mut [u8],
    cancellation: &Cancellation,
) -> io::Result<CopyStats> {
    platform::copy_data(source, destination, length, scratch, cancellation)
}

/// Transfers one entry and, when it is a directory, its tree.
///
/// Regular files and symbolic links keep their metadata; repeated
/// `(device, inode)` pairs become hardlinks in the destination tree. The
/// source tree is read through `O_NOFOLLOW` descriptors and every opened
/// object is checked against its captured identity.
pub fn transfer_tree(request: TreeTransfer<'_>) -> io::Result<()> {
    platform::transfer_tree(request)
}

/// Applies deferred inode flags to a published entry.
///
/// [`transfer_tree`] and [`publish_tree`] defer flags that Linux refuses to
/// carry through a rename. A staged publication applies them itself; a direct
/// caller applies the returned list at the final name with this function.
pub fn apply_deferred_flags(
    parent: BorrowedFd<'_>,
    name: &OsStr,
    deferred: &[DeferredFlags],
) -> io::Result<()> {
    platform::apply_deferred_flags(parent, name, deferred, false)
}

/// Observes one entry within an open parent without following a final
/// symbolic link.
pub fn observe_entry(parent: BorrowedFd<'_>, name: &OsStr) -> io::Result<Observation> {
    platform::observe(parent, name)
}

/// Observes an already-open file or directory descriptor.
pub fn observe_fd(fd: BorrowedFd<'_>) -> io::Result<Observation> {
    platform::observe_fd(fd)
}

/// Whether an operation copies or moves its sources.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Operation {
    /// Sources are retained.
    Copy,
    /// Transferred sources are retired after publication.
    Move,
}

/// What an existing destination permits.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Collision {
    /// An existing destination is an error.
    #[default]
    Error,
    /// An existing destination is left unchanged.
    Skip,
    /// A same-kind leaf destination is exchanged for the source.
    Replace,
}

/// The collision and merge choices that shape a plan.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PolicyOptions {
    /// The selected collision behavior.
    pub collision: Collision,
    /// Recurse into an existing destination directory.
    pub merge: bool,
    /// Plan only; application is the caller's decision.
    pub dry_run: bool,
}

/// The top-level operand spelling of one operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Operands {
    /// One source mapped to exactly one destination.
    Exact {
        /// The source operand.
        source: PathBuf,
        /// The destination operand.
        destination: PathBuf,
    },
    /// Sources mapped to children of an existing directory.
    Into {
        /// The destination container.
        directory: PathBuf,
        /// The first source operand.
        first: PathBuf,
        /// Remaining source operands.
        rest: Vec<PathBuf>,
    },
}

/// The action selected for one planned operand.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    /// Publish a new destination.
    Transfer,
    /// Exchange a verified leaf destination for the source.
    Replace,
    /// Recurse into an existing destination directory.
    Merge,
    /// Leave an existing destination unchanged.
    Skip,
    /// The source and destination name one entry.
    SameEntry,
}

/// What was observed at a planned destination.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    /// No entry exists at the destination name.
    Absent,
    /// The destination name resolves to the source entry itself.
    SameEntry,
    /// An entry exists at the destination name.
    Existing(Observation),
}

/// One source-to-destination mapping decided before mutation.
#[derive(Clone, Debug)]
pub struct PlannedOperand {
    /// The source operand as written.
    pub source: PathBuf,
    /// The mapped destination path.
    pub destination: PathBuf,
    /// The final destination name within the destination parent.
    pub destination_name: OsString,
    /// The observed identity of the source parent.
    pub source_parent: EntryIdentity,
    /// The observed identity of the destination parent.
    pub destination_parent: EntryIdentity,
    /// The source entry observation used for revalidation.
    pub source_observed: Observation,
    /// The destination entry observation, when one existed.
    pub destination_observed: Option<Observation>,
    /// The decision for this operand.
    pub action: Action,
}

/// A fully checked operation plan.
pub struct Preflight {
    /// The mappings in operand order.
    pub operands: Vec<PlannedOperand>,
    /// The held destination container for [`Operands::Into`].
    pub into_directory: Option<OwnedFd>,
}

/// A planning error attributed to one operand path.
#[derive(Debug)]
pub struct PathError {
    /// The operand path the error concerns.
    pub path: PathBuf,
    /// The underlying cause.
    pub error: io::Error,
}

impl PathError {
    /// Builds an attributed planning error.
    #[must_use]
    pub fn new(path: &Path, error: io::Error) -> Self {
        Self {
            path: path.to_path_buf(),
            error,
        }
    }
}

impl std::fmt::Display for PathError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:?}: {}", self.path, self.error)
    }
}

impl std::error::Error for PathError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

/// The inputs whose mappings and collision decisions are planned before any
/// namespace mutation.
pub struct PolicyRequest<'a> {
    /// The operation being planned.
    pub operation: Operation,
    /// The top-level operand spelling.
    pub operands: &'a Operands,
    /// The collision and merge choices.
    pub options: PolicyOptions,
    /// The home directory protected from a move.
    pub home: Option<&'a Path>,
}

/// Maps every operand and decides its action without mutating the namespace.
///
/// Duplicate outputs, repeated or overlapping sources, source/destination
/// ancestry conflicts and protected move roots are rejected before an operand
/// is transferred.
pub fn preflight(cwd: BorrowedFd<'_>, request: &PolicyRequest<'_>) -> Result<Preflight, PathError> {
    platform::preflight(cwd, request)
}

/// Decides one source against one observed destination.
pub fn decide(
    source: Observation,
    target: Target,
    options: &PolicyOptions,
) -> io::Result<Action> {
    let destination = match target {
        Target::Absent => return Ok(Action::Transfer),
        Target::SameEntry => return Ok(Action::SameEntry),
        Target::Existing(destination) => destination,
    };

    if options.merge
        && source.identity != destination.identity
        && source.identity.kind == FileKind::Directory
        && destination.identity.kind == FileKind::Directory
    {
        return Ok(Action::Merge);
    }

    if options.collision == Collision::Skip {
        return Ok(Action::Skip);
    }

    if source.identity == destination.identity {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "distinct names identify the same object",
        ));
    }

    if options.collision == Collision::Replace
        && matches!(
            (source.identity.kind, destination.identity.kind),
            (FileKind::File, FileKind::File) | (FileKind::Symlink, FileKind::Symlink)
        )
    {
        return Ok(Action::Replace);
    }

    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "destination exists and the selected policy does not permit replacement",
    ))
}

/// Returns the final name of a source used by [`Operands::Into`], rejecting
/// `.`, `..` and paths without an unambiguous basename.
pub fn into_basename(source: &Path) -> io::Result<&OsStr> {
    let bytes = source.as_os_str().as_bytes();
    if bytes.is_empty() || bytes.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "--into source has no unambiguous basename",
        ));
    }
    let Some(end) = bytes.iter().rposition(|byte| *byte != b'/').map(|index| index + 1) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "--into source has no unambiguous basename",
        ));
    };
    let trimmed = &bytes[..end];
    let basename = trimmed.rsplit(|byte| *byte == b'/').next().unwrap_or_default();
    if basename.is_empty() || basename == b"." || basename == b".." {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "--into source has no unambiguous basename",
        ));
    }
    Ok(OsStr::from_bytes(basename))
}

/// A point where a caller may inspect, fail or interrupt a journaled step.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Boundary {
    /// The session lock is held and any prior state has been recovered.
    SessionStarted,
    /// The unpublished staging object is recorded in the journal.
    StageRegistered,
    /// The complete tree exists inside the private staging directory.
    TransferComplete,
    /// Source and destination were revalidated; publication is next.
    BeforePublish,
    /// A replacement intent names both objects before the exchange.
    IntentPublished,
    /// The staged object occupies the destination name.
    Published,
    /// The displaced destination is about to be renamed into the journal.
    BeforeCapture,
    /// The displaced destination is held by the journal.
    Captured,
    /// The source parent sync after capture returned.
    CaptureParentSynced,
    /// The journal directory sync after capture returned.
    CaptureDirectorySynced,
    /// The captured entry is about to be unlinked.
    BeforeUnlink,
    /// The captured entry is unlinked.
    Unlinked,
    /// The journal directory sync after the unlink returned.
    RemovalDirectorySynced,
    /// The captured entry's parent is about to be synchronized.
    BeforeParentSync,
    /// The captured entry's parent sync returned.
    ParentSynced,
    /// The capture intent record is removed.
    IntentRemoved,
    /// The journal directory sync after removing the intent returned.
    IntentRemovalSynced,
    /// A capture was restored to its original name.
    Restored,
    /// The staging intent record is removed.
    StageCleared,
    /// The journal directory is gone.
    Finished,
}

/// The result of removing a captured entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Removal {
    /// The entry was unlinked.
    Removed,
    /// The original name was already absent.
    AlreadyAbsent,
    /// The source no longer matched the captured identity.
    Changed,
}

/// A failed removal that reports whether the entry was already unlinked.
#[derive(Debug)]
pub struct RemovalError {
    /// Whether the entry was unlinked before the failure.
    pub removed: bool,
    /// The cause of the failure.
    pub error: io::Error,
}

impl std::fmt::Display for RemovalError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(formatter)
    }
}

impl std::error::Error for RemovalError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

/// One verified ancestor from the recovery root to a retired entry's parent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Ancestor {
    /// The ancestor entry name within its parent.
    pub name: OsString,
    /// The observed ancestor identity.
    pub identity: EntryIdentity,
}

/// One entry to capture and remove under the journal.
#[derive(Clone, Debug)]
pub struct Retirement {
    parents: Vec<Ancestor>,
    name: OsString,
    expected: EntryIdentity,
}

impl Retirement {
    /// Builds a retirement request for one entry.
    ///
    /// Every name must be a single path component other than `.` or `..`.
    pub fn new(
        parents: Vec<Ancestor>,
        name: OsString,
        expected: EntryIdentity,
    ) -> io::Result<Self> {
        validate_name(name.as_bytes())?;
        for parent in &parents {
            validate_name(parent.name.as_bytes())?;
        }
        Ok(Self {
            parents,
            name,
            expected,
        })
    }
}

/// The two objects named by an atomic replacement.
#[derive(Clone, Debug)]
pub struct Publication {
    source: EntryIdentity,
    destination_parent: EntryIdentity,
    destination_name: OsString,
}

impl Publication {
    /// Builds a replacement publication record.
    ///
    /// `source` is the displaced object held at the staged name and
    /// `destination_name` is the live name it was exchanged from.
    pub fn new(
        source: EntryIdentity,
        destination_parent: EntryIdentity,
        destination_name: OsString,
    ) -> io::Result<Self> {
        validate_name(destination_name.as_bytes())?;
        Ok(Self {
            source,
            destination_parent,
            destination_name,
        })
    }
}

/// One staged publication of a source entry.
pub struct PublishRequest<'a> {
    /// The open directory holding the source entry.
    pub source_parent: BorrowedFd<'a>,
    /// The single source entry name within `source_parent`.
    pub source_name: &'a OsStr,
    /// The open parent directory for the destination.
    pub destination_parent: BorrowedFd<'a>,
    /// The single destination entry name within `destination_parent`.
    pub destination_name: &'a OsStr,
    /// The source observation that must still hold before publication.
    pub expected_source: Observation,
    /// The destination observation for [`Action::Replace`], or `None` for
    /// [`Action::Transfer`].
    pub expected_destination: Option<Observation>,
    /// [`Action::Transfer`] or [`Action::Replace`].
    pub action: Action,
    /// Traversal and durability choices.
    pub engine: EngineOptions,
    /// Polled between entries and copy chunks.
    pub cancellation: &'a Cancellation,
    /// Updated as entries are discovered, completed and copied.
    pub progress: &'a mut Progress,
}

/// Transfers one entry into a private staging directory beside the
/// destination, then publishes it with `RENAME_NOREPLACE` or
/// `RENAME_EXCHANGE`.
///
/// A replacement records both objects in the journal before the exchange and
/// retires the displaced object only after capturing and verifying it. On
/// failure the previous destination is never deleted outside the journal; an
/// unresolved journal is retained for recovery.
pub fn publish_tree(
    request: PublishRequest<'_>,
    authorize: impl FnMut(Boundary) -> io::Result<()>,
) -> io::Result<()> {
    platform::publish(request, authorize)
}

/// One copy-only merge of a source directory into an existing destination
/// directory.
pub struct MergeRequest<'a> {
    /// The open directory holding the source directory.
    pub source_parent: BorrowedFd<'a>,
    /// The single source directory name within `source_parent`.
    pub source_name: &'a OsStr,
    /// The open parent directory holding the destination.
    pub destination_parent: BorrowedFd<'a>,
    /// The single existing destination directory name.
    pub destination_name: &'a OsStr,
    /// The source observation that must still hold.
    pub expected_source: Observation,
    /// The destination observation that must still hold.
    pub expected_destination: Observation,
    /// The collision and merge choices applied to each child.
    pub options: PolicyOptions,
    /// Traversal and durability choices.
    pub engine: EngineOptions,
    /// Polled between entries and copy chunks.
    pub cancellation: &'a Cancellation,
    /// Updated as entries are discovered, completed and copied.
    pub progress: &'a mut Progress,
}

/// Recursively copies the contents of a source directory into an existing
/// destination directory.
///
/// Existing destination directories retain their metadata; newly created
/// entries are published through [`publish_tree`]. Source retirement is a
/// separate journaled step.
pub fn merge_tree(request: MergeRequest<'_>) -> io::Result<()> {
    platform::merge(request)
}

/// How a local sync maps its operands.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SyncOperands {
    /// One source mapped to exactly one destination.
    Exact {
        /// The source operand.
        source: PathBuf,
        /// The destination operand.
        destination: PathBuf,
    },
    /// Sources mapped to children of an existing directory.
    Into {
        /// The destination container.
        directory: PathBuf,
        /// The source operands.
        sources: Vec<PathBuf>,
    },
    /// The contents of one source directory synchronized into a directory.
    Contents {
        /// The source directory.
        source: PathBuf,
        /// The destination directory.
        directory: PathBuf,
    },
}

/// The choices that shape a local sync.
#[derive(Clone, Copy, Debug, Default)]
pub struct SyncOptions {
    /// Remove destination entries that the source does not name.
    pub delete: bool,
    /// Report the plan without mutating either tree.
    pub dry_run: bool,
    /// Traversal and durability choices.
    pub engine: EngineOptions,
}

/// One local synchronization request.
///
/// Paths resolve against [`SyncRequest::cwd`]; the destination parent must
/// already exist for [`SyncOperands::Exact`] and [`SyncOperands::Into`].
/// Unchanged regular files are skipped when kind, size and modification time
/// agree, and a differing destination is replaced through
/// [`publish_tree`].
pub struct SyncRequest<'a> {
    /// The resolved working directory for operand paths.
    pub cwd: BorrowedFd<'a>,
    /// The operand mapping.
    pub operands: SyncOperands,
    /// The synchronization choices.
    pub options: SyncOptions,
    /// Polled between entries and copy chunks.
    pub cancellation: &'a Cancellation,
}

/// Counters for one local sync.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SyncProgress {
    /// Entries examined in both trees.
    pub examined_files: u64,
    /// Entries written to the destination.
    pub transferred_files: u64,
    /// Entries left unchanged.
    pub skipped_files: u64,
    /// Destination entries removed by `--delete`.
    pub deleted_files: u64,
    /// Logical bytes covered by transferred files.
    pub logical_bytes: u64,
    /// Bytes materialized at the destination.
    pub streamed_bytes: u64,
    /// Bytes left as sparse holes or reflink shares.
    pub hole_bytes: u64,
}

/// Plans and performs one local sync.
///
/// A cancelled sync stops at the next entry boundary and leaves no staging
/// artifact. An interrupted replacement leaves the previous destination in
/// place.
pub fn sync_tree(request: SyncRequest<'_>) -> io::Result<SyncProgress> {
    platform::sync(request)
}

/// One planned action for a dry run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DryRunEntry {
    /// The path relative to the source operand.
    pub source: PathBuf,
    /// The path relative to the destination operand.
    pub destination: PathBuf,
    /// The action a real run would take.
    pub action: Action,
}

/// Plans a transfer, merge or replacement without mutating the namespace.
///
/// The plan observes each reached entry and applies [`PolicyOptions`] to it.
/// It creates no staging or journal state and probes no clone support.
pub fn dry_run(
    source_parent: BorrowedFd<'_>,
    source_name: &OsStr,
    destination_parent: BorrowedFd<'_>,
    destination_name: &OsStr,
    options: &PolicyOptions,
    engine: &EngineOptions,
    cancellation: &Cancellation,
    plan: &mut Vec<DryRunEntry>,
) -> io::Result<()> {
    platform::dry_run(
        source_parent,
        source_name,
        destination_parent,
        destination_name,
        options,
        engine,
        cancellation,
        plan,
    )
}

fn validate_name(name: &[u8]) -> io::Result<()> {
    if name.is_empty() || name == b"." || name == b".." || name.contains(&0) || name.contains(&b'/')
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "journal name is not one directory entry",
        ));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::journal::Session;
#[cfg(target_os = "linux")]
use linux as platform;

#[cfg(not(target_os = "linux"))]
mod unsupported;
#[cfg(not(target_os = "linux"))]
use unsupported as platform;
