#![allow(clippy::single_call_fn)]

mod archive;
mod budget;
mod compression;
mod cpio_reader;
mod cursor;
mod entry;
mod error;
mod input;
mod tar_reader;
mod zip_reader;

pub use archive::Archive;
pub use budget::{Budget, Limits, Metered};
pub use compression::{
    BUFFER_SIZE, Codec, Compression, DEFAULT_LEVEL, Encode, Encoder, GzipHeader, SNIFF_LEN,
    decoder,
};
pub use cursor::{Completion, Cursor, Entry, Finish};
pub use entry::{EntryKind, EntryMeta, Format};
pub use error::{Error, ErrorKind, Result};
pub use input::{Input, ReadSeek, Source};
