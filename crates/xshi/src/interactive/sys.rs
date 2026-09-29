//! Platform-specific syscall wrappers.

use std::os::fd::{IntoRawFd, RawFd};

/// Create a pipe with O_CLOEXEC | O_NONBLOCK on both ends.
/// Used for the signal self-pipe.
/// Linux: 1 syscall. macOS: 5 syscalls (pipe + 4x fcntl).
pub fn pipe_nonblock_cloexec() -> Result<(RawFd, RawFd), std::io::Error> {
    let (read, write) = rustix::pipe::pipe()?;
    rustix::io::fcntl_setfd(&read, rustix::io::FdFlags::CLOEXEC)?;
    rustix::io::fcntl_setfd(&write, rustix::io::FdFlags::CLOEXEC)?;
    rustix::fs::fcntl_setfl(&read, rustix::fs::OFlags::NONBLOCK)?;
    rustix::fs::fcntl_setfl(&write, rustix::fs::OFlags::NONBLOCK)?;
    Ok((read.into_raw_fd(), write.into_raw_fd()))
}

