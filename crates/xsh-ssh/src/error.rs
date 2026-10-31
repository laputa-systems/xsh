use std::fmt;

use oxish_proto::{ProtoError, crypto::CryptoError};

/// A failure to establish or use an SSH connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// A host-key pin is absent or not a `SHA256:` fingerprint.
    InvalidHostKeyPin {
        /// Why the pin was rejected.
        detail: String,
    },
    /// The server presented a key whose fingerprint differs from the pin.
    HostKeyMismatch {
        /// The pinned fingerprint.
        expected: String,
        /// The fingerprint the server presented.
        actual: String,
    },
    /// The host key or its signature could not be verified.
    HostKey {
        /// Why verification failed.
        detail: String,
    },
    /// The peer and client have no algorithm in common.
    UnsupportedAlgorithm {
        /// The algorithm category.
        category: &'static str,
        /// The algorithm the client or peer required.
        name: String,
    },
    /// The server did not negotiate strict key exchange.
    StrictKexUnsupported,
    /// The peer violated the SSH protocol.
    Protocol {
        /// Why the protocol state was rejected.
        detail: String,
    },
    /// The connection ended before the operation completed.
    Io {
        /// The underlying failure.
        detail: String,
    },
    /// The operation exceeded its deadline.
    Timeout,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidHostKeyPin { detail } => write!(formatter, "invalid host key pin: {detail}"),
            Self::HostKeyMismatch { expected, actual } => {
                write!(formatter, "host key mismatch: expected {expected}, got {actual}")
            }
            Self::HostKey { detail } => write!(formatter, "host key verification failed: {detail}"),
            Self::UnsupportedAlgorithm { category, name } => {
                write!(formatter, "no supported {category} algorithm: {name}")
            }
            Self::StrictKexUnsupported => write!(formatter, "server does not support strict key exchange"),
            Self::Protocol { detail } => write!(formatter, "SSH protocol error: {detail}"),
            Self::Io { detail } => write!(formatter, "SSH I/O error: {detail}"),
            Self::Timeout => write!(formatter, "SSH operation timed out"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        match error.kind() {
            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => Self::Timeout,
            _ => Self::Io {
                detail: error.to_string(),
            },
        }
    }
}

impl From<ProtoError> for Error {
    fn from(error: ProtoError) -> Self {
        Self::Protocol {
            detail: error.to_string(),
        }
    }
}

impl From<CryptoError> for Error {
    fn from(error: CryptoError) -> Self {
        Self::HostKey {
            detail: error.to_string(),
        }
    }
}
