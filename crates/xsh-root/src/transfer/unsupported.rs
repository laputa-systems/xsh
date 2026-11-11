use std::io;
use std::os::fd::BorrowedFd;

use super::{Cancellation, CopyStats, Metadata, MetadataTarget, Preservation, TreeTransfer};

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
