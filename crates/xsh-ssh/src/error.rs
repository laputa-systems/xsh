use std::fmt;

use oxish_proto::{ProtoError, crypto::CryptoError};

/// Whether a request may have reached the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Certainty {
    /// The request was never submitted; retrying is safe.
    NotSubmitted,
    /// The request may have reached the server; retrying is unsafe.
    Submitted,
}

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
    /// The private key file could not be used.
    KeyFile {
        /// Why the key was rejected.
        detail: String,
    },
    /// The private key file is encrypted.
    EncryptedKey,
    /// The server rejected authentication.
    Authentication {
        /// The server's failure detail.
        detail: String,
    },
    /// A channel operation failed.
    Channel {
        /// Why the channel operation failed.
        detail: String,
    },
    /// A request was never submitted and the transport failed.
    NotSubmitted {
        /// The underlying failure.
        detail: String,
    },
    /// A request may have reached the server and the transport failed.
    Submitted {
        /// The underlying failure.
        detail: String,
    },
    /// The peer violated the SSH protocol.
    Protocol {
        /// Why the protocol state was rejected.
        detail: String,
    },
    /// The operation exceeded its deadline.
    Timeout {
        /// Whether the request may have reached the server.
        certainty: Certainty,
    },
    /// The caller cancelled the operation.
    Cancelled {
        /// Whether the request may have reached the server.
        certainty: Certainty,
    },
    /// The server refused the request; the command did not run.
    Rejected {
        /// The refused request.
        request: &'static str,
    },
    /// A captured output stream exceeded its limit.
    OutputLimit {
        /// The stream that exceeded the limit.
        stream: &'static str,
    },
    /// The processor lacks a feature the cryptographic provider requires.
    UnsupportedHardware {
        /// The missing feature.
        feature: &'static str,
    },
    /// An earlier failure left the connection in an unknown state.
    Unusable,
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
            Self::KeyFile { detail } => write!(formatter, "SSH key file: {detail}"),
            Self::EncryptedKey => write!(formatter, "encrypted SSH keys are unsupported"),
            Self::Authentication { detail } => write!(formatter, "authentication failed: {detail}"),
            Self::Channel { detail } => write!(formatter, "SSH channel failed: {detail}"),
            Self::NotSubmitted { detail } => {
                write!(formatter, "SSH transport failed before submission: {detail}")
            }
            Self::Submitted { detail } => {
                write!(formatter, "SSH transport failed after submission: {detail}")
            }
            Self::Protocol { detail } => write!(formatter, "SSH protocol error: {detail}"),
            Self::Timeout {
                certainty: Certainty::NotSubmitted,
            } => write!(formatter, "SSH operation timed out before submission"),
            Self::Timeout {
                certainty: Certainty::Submitted,
            } => write!(formatter, "SSH operation timed out after submission"),
            Self::Cancelled {
                certainty: Certainty::NotSubmitted,
            } => write!(formatter, "SSH operation cancelled before submission"),
            Self::Cancelled {
                certainty: Certainty::Submitted,
            } => write!(formatter, "SSH operation cancelled after submission"),
            Self::Rejected { request } => write!(formatter, "server rejected the {request} request"),
            Self::OutputLimit { stream } => {
                write!(formatter, "{stream} exceeded the capture limit")
            }
            Self::UnsupportedHardware { feature } => {
                write!(formatter, "this processor lacks {feature}, which the SSH cryptography requires")
            }
            Self::Unusable => write!(
                formatter,
                "SSH connection is unusable after an earlier failure"
            ),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::NotSubmitted {
            detail: error.to_string(),
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

impl Error {
    /// Whether the failed request may have reached the server.
    pub fn certainty(&self) -> Certainty {
        match self {
            Self::Submitted { .. } | Self::OutputLimit { .. } => Certainty::Submitted,
            Self::Timeout { certainty } | Self::Cancelled { certainty } => *certainty,
            _ => Certainty::NotSubmitted,
        }
    }

    pub(crate) fn after_submission(self) -> Self {
        match self {
            Self::Submitted { .. }
            | Self::OutputLimit { .. }
            | Self::Timeout { .. }
            | Self::Cancelled { .. }
            | Self::Rejected { .. }
            | Self::Unusable => self,
            Self::NotSubmitted { detail } => Self::Submitted { detail },
            other => Self::Submitted {
                detail: other.to_string(),
            },
        }
    }
}
