#![allow(clippy::single_call_fn)]

mod cpio;
mod policy;
mod tar;
mod zip;

pub(crate) use cpio::{cpio_create, cpio_extract, cpio_list};
pub(crate) use tar::{tar_create, tar_extract, tar_list, tar_list_stream};
pub(crate) use zip::{zip_extract, zip_list};

use crate::modules::compression::{
    codec_reader, copy_compressed as copy_compressed_with_codec,
    for_create as compression_for_create, level as compression_level, parse as parse_compression,
};
use crate::runtime::value::RuntimeError;
use crate::source::Span;
use std::fs::{self, File};
use std::future::Future;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

const BUFFER_SIZE: usize = 64 * 1024;

fn block_on_archive<T>(
    _span: Span,
    future: impl Future<Output = Result<T, RuntimeError>>,
) -> Result<T, RuntimeError> {
    futures_lite::future::block_on(future)
}

pub(crate) fn compress_file(
    source: PathBuf,
    dest: PathBuf,
    format: &str,
    level: i64,
    overwrite: bool,
    span: Span,
) -> Result<(), RuntimeError> {
    let compression =
        compression_for_create(&dest, parse_compression(format, span)?).ok_or_else(|| {
            RuntimeError::new("archive-compression", "compression format required").with_span(span)
        })?;
    let level = compression_level(level, span)?;
    let input =
        File::open(&source).map_err(|error| archive_error("archive-compress", error, span))?;
    let input_len = input
        .metadata()
        .map_err(|error| archive_error("archive-compress", error, span))?
        .len();
    let output = create_output_file(&dest, overwrite, "archive-compress", span)?;
    copy_compressed_with_codec(input, output, compression, level, input_len, span)
}

pub(crate) fn decompress_file(
    source: PathBuf,
    dest: PathBuf,
    format: &str,
    overwrite: bool,
    span: Span,
) -> Result<(), RuntimeError> {
    let mut input = codec_reader(&source, parse_compression(format, span)?, span)?;
    let mut output = create_output_file(&dest, overwrite, "archive-decompress", span)?;
    io::copy(&mut input, &mut output)
        .map_err(|error| archive_error("archive-decompress", error, span))?;
    output
        .flush()
        .map_err(|error| archive_error("archive-decompress", error, span))
}

pub(crate) fn decompress_bytes(
    source: PathBuf,
    format: &str,
    span: Span,
) -> Result<Vec<u8>, RuntimeError> {
    let mut input = codec_reader(&source, parse_compression(format, span)?, span)?;
    let mut output = Vec::new();
    input
        .read_to_end(&mut output)
        .map_err(|error| archive_error("archive-decompress", error, span))?;
    Ok(output)
}

fn create_output_file(
    path: &Path,
    overwrite: bool,
    kind: &str,
    span: Span,
) -> Result<File, RuntimeError> {
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .create_new(!overwrite)
        .truncate(overwrite)
        .open(path)
        .map_err(|error| archive_error(kind, error, span))
}

fn copy_exact<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    size: u64,
    span: Span,
    kind: &str,
) -> Result<(), RuntimeError> {
    let mut limited = reader.take(size);
    io::copy(&mut limited, writer).map_err(|error| archive_error(kind, error, span))?;
    if limited.limit() != 0 {
        return Err(archive_error(
            kind,
            io::Error::new(io::ErrorKind::UnexpectedEof, "truncated archive entry"),
            span,
        ));
    }
    Ok(())
}

fn archive_error(kind: &str, error: io::Error, span: Span) -> RuntimeError {
    RuntimeError::new(kind, error.to_string()).with_span(span)
}
