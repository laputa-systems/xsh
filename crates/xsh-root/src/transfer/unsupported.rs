use std::ffi::OsStr;
use std::io;
use std::os::fd::BorrowedFd;

use super::{
    Boundary, Cancellation, CopyStats, MergeRequest, Metadata, MetadataTarget, Observation,
    PathError, PolicyRequest, Preflight, Preservation, PublishRequest, TreeTransfer,
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

pub(super) fn observe(
    _parent: BorrowedFd<'_>,
    _name: &OsStr,
) -> io::Result<Observation> {
    Err(unsupported())
}

pub(super) fn observe_fd(_fd: BorrowedFd<'_>) -> io::Result<Observation> {
    Err(unsupported())
}

pub(super) fn preflight(
    _cwd: BorrowedFd<'_>,
    _request: &PolicyRequest<'_>,
) -> Result<Preflight, PathError> {
    Err(PathError::new(std::path::Path::new(""), unsupported()))
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
