use std::error::Error as StdError;
use std::fmt;
use std::io;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ErrorKind {
    Open,
    Io,
    Corrupt,
    Truncated,
    Unsupported,
    NeedsSeek,
    Unsafe,
    Collision,
    Limit,
    Canceled,
    Options,
}

impl ErrorKind {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Io => "Io",
            Self::Corrupt => "Corrupt",
            Self::Truncated => "Truncated",
            Self::Unsupported => "Unsupported",
            Self::NeedsSeek => "NeedsSeek",
            Self::Unsafe => "Unsafe",
            Self::Collision => "Collision",
            Self::Limit => "Limit",
            Self::Canceled => "Canceled",
            Self::Options => "Options",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Error {
    kind: ErrorKind,
    message: String,
    entry: Option<Vec<u8>>,
    offset: Option<u64>,
    io_kind: Option<io::ErrorKind>,
}

impl Error {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            entry: None,
            offset: None,
            io_kind: None,
        }
    }

    pub fn corrupt(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Corrupt, message)
    }

    pub fn truncated(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Truncated, message)
    }

    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Unsupported, message)
    }

    pub fn unsafe_entry(entry: &[u8], message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Unsafe, message).with_entry(entry)
    }

    pub fn options(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Options, message)
    }

    pub fn limit(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Limit, message)
    }

    pub fn canceled() -> Self {
        Self::new(ErrorKind::Canceled, "operation canceled")
    }

    #[must_use]
    pub fn with_entry(mut self, entry: &[u8]) -> Self {
        self.entry = Some(entry.to_vec());
        self
    }

    #[must_use]
    pub fn with_offset(mut self, offset: u64) -> Self {
        self.offset = Some(offset);
        self
    }

    #[must_use]
    pub fn with_context(mut self, context: &str) -> Self {
        self.message = format!("{context}: {}", self.message);
        self
    }

    #[must_use]
    pub fn with_cleanup(mut self, cleanup: &Error) -> Self {
        self.message = format!("{}; cleanup failed: {}", self.message, cleanup.message);
        self
    }

    pub fn from_open(error: &io::Error) -> Self {
        Self {
            kind: ErrorKind::Open,
            message: error.to_string(),
            entry: None,
            offset: None,
            io_kind: Some(error.kind()),
        }
    }

    pub fn from_host(error: &io::Error) -> Self {
        Self {
            kind: ErrorKind::Io,
            message: error.to_string(),
            entry: None,
            offset: None,
            io_kind: Some(error.kind()),
        }
    }

    pub fn from_read(error: &io::Error) -> Self {
        if let Some(inner) = embedded(error) {
            return inner;
        }
        let kind = match error.kind() {
            io::ErrorKind::UnexpectedEof => ErrorKind::Truncated,
            io::ErrorKind::InvalidData | io::ErrorKind::InvalidInput => ErrorKind::Corrupt,
            _ => ErrorKind::Io,
        };
        Self {
            kind,
            message: error.to_string(),
            entry: None,
            offset: None,
            io_kind: Some(error.kind()),
        }
    }

    pub const fn kind(&self) -> ErrorKind {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn entry(&self) -> Option<&[u8]> {
        self.entry.as_deref()
    }

    pub const fn offset(&self) -> Option<u64> {
        self.offset
    }

    pub const fn io_kind(&self) -> Option<io::ErrorKind> {
        self.io_kind
    }
}

fn embedded(error: &io::Error) -> Option<Error> {
    let mut current: Option<&(dyn StdError + 'static)> = error.get_ref().map(|inner| inner as _);
    while let Some(candidate) = current {
        if let Some(found) = candidate.downcast_ref::<Error>() {
            return Some(found.clone());
        }
        current = candidate.source();
    }
    None
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.kind.name(), self.message)?;
        if let Some(offset) = self.offset {
            write!(formatter, " (offset {offset})")?;
        }
        Ok(())
    }
}

impl StdError for Error {}

impl From<Error> for io::Error {
    fn from(error: Error) -> Self {
        let kind = match error.kind {
            ErrorKind::Truncated => io::ErrorKind::UnexpectedEof,
            ErrorKind::Corrupt => io::ErrorKind::InvalidData,
            _ => io::ErrorKind::Other,
        };
        io::Error::new(kind, error)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
