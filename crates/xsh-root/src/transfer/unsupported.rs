use std::ffi::OsStr;
use std::io;
use std::os::fd::BorrowedFd;
use std::path::Path;

use super::{
    Boundary, Cancellation, CopyStats, DryRunEntry, EngineOptions, MergeRequest, Metadata,
    MetadataTarget, MountIdentity, Observation, PathError, PolicyOptions, PolicyRequest, Preflight,
    Preservation, PublishRequest, SyncProgress, SyncRequest, TreeTransfer,
};

fn unsupported() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "filesystem transfer requires Linux",
    )
}

pub(super) fn capture(_target: MetadataTarget<'_>) -> io::Result<Metadata> {
    Err(unsupported())
}

pub(super) fn apply(
    _target: MetadataTarget<'_>,
    _metadata: &Metadata,
    _preservation: Preservation,
) -> io::Result<()> {
    Err(unsupported())
}

pub(super) fn apply_final(_target: MetadataTarget<'_>, _metadata: &Metadata) -> io::Result<()> {
    Err(unsupported())
}

pub(super) fn copy_data(
    _source: BorrowedFd<'_>,
    _destination: BorrowedFd<'_>,
    _length: u64,
    _scratch: &mut [u8],
    _cancellation: &Cancellation,
) -> io::Result<CopyStats> {
    Err(unsupported())
}

pub(super) fn transfer_tree(_request: TreeTransfer<'_>) -> io::Result<()> {
    Err(unsupported())
}

pub(super) fn apply_deferred_flags(
    _parent: BorrowedFd<'_>,
    _name: &OsStr,
    _deferred: &[super::DeferredFlags],
    _durable: bool,
) -> io::Result<()> {
    Err(unsupported())
}

pub(super) fn observe(_parent: BorrowedFd<'_>, _name: &OsStr) -> io::Result<Observation> {
    Err(unsupported())
}

pub(super) fn observe_fd(_fd: BorrowedFd<'_>) -> io::Result<Observation> {
    Err(unsupported())
}

pub(super) fn mount_identity(_fd: BorrowedFd<'_>) -> io::Result<MountIdentity> {
    Err(unsupported())
}

pub(super) fn worker_count(_options: &EngineOptions) -> io::Result<usize> {
    Err(unsupported())
}

pub(super) fn preflight(
    _cwd: BorrowedFd<'_>,
    _request: &PolicyRequest<'_>,
) -> Result<Preflight, PathError> {
    Err(PathError::new(Path::new(""), unsupported()))
}

pub(super) fn publish(
    _request: PublishRequest<'_>,
    _authorize: impl FnMut(Boundary) -> io::Result<()>,
) -> io::Result<()> {
    Err(unsupported())
}

pub(super) fn merge(_request: MergeRequest<'_>) -> io::Result<()> {
    Err(unsupported())
}

pub(super) fn sync(_request: SyncRequest<'_>) -> io::Result<SyncProgress> {
    Err(unsupported())
}

pub(super) fn dry_run(
    _source_parent: BorrowedFd<'_>,
    _source_name: &OsStr,
    _destination_parent: BorrowedFd<'_>,
    _destination_name: &OsStr,
    _options: &PolicyOptions,
    _engine: &EngineOptions,
    _cancellation: &Cancellation,
    _plan: &mut Vec<DryRunEntry>,
) -> io::Result<()> {
    Err(unsupported())
}
