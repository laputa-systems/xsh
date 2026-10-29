#![allow(clippy::single_call_fn)]

mod archive;
mod budget;
mod compression;
mod cpio_reader;
mod create;
mod cursor;
mod entry;
mod error;
mod extract;
mod file;
mod input;
mod output;
mod tar_reader;
mod zip_reader;

pub use archive::Archive;
pub use budget::{Budget, Limits, Metered};
pub use compression::{
    BUFFER_SIZE, Codec, Compression, DEFAULT_LEVEL, Encode, Encoder, GzipHeader, SNIFF_LEN,
    decoder, decoder_with_mtime,
};
pub use create::{CreateOptions, create};
pub use cursor::{Completion, Cursor, Entry, Finish};
pub use entry::{EntryKind, EntryMeta, Format};
pub use error::{Error, ErrorKind, Result};
pub use extract::{
    Collision, Links, Modes, Ownership, Policy, Report, Special, extract, normalize, remove_tree,
};
pub use file::{compress_file, decompress_file, decompress_to_memory};
pub use input::{Input, ReadSeek, Source};
