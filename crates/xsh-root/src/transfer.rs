//! Internal filesystem-transfer substrate: a metadata record, hardlink-group
//! preservation, cancellation, byte/hole progress, and a direct copy.
//!
//! Linux implements every operation. Other hosts return an unsupported error
//! at each entry point so callers stay portable. The API is a Rust boundary
//! only; it carries no script-visible contract.

use std::ffi::{OsStr, OsString};
use std::io;
use std::os::fd::BorrowedFd;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use crate::FileKind;

/// A byte buffer size for buffered transfers.
pub const TRANSFER_BUFFER_BYTES: usize = 256 * 1024;

/// A nanosecond-resolution file timestamp.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Timestamp {
    /// Whole seconds since the Unix epoch.
    pub seconds: i64,
    /// The subsecond part in nanoseconds.
    pub nanoseconds: u32,
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
    /// Polled between entries and copy chunks.
    pub cancellation: &'a Cancellation,
    /// Updated as entries are discovered, completed and copied.
    pub progress: &'a mut Progress,
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

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as platform;

#[cfg(not(target_os = "linux"))]
mod unsupported;
#[cfg(not(target_os = "linux"))]
use unsupported as platform;
