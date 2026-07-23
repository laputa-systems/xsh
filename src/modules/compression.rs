use crate::runtime::value::RuntimeError;
use crate::source::Span;
use bzip2::Compression as Bzip2Compression;
use bzip2::bufread::MultiBzDecoder;
use bzip2::write::BzEncoder;
use flate2::Compression as GzipCompression;
use flate2::bufread::MultiGzDecoder;
use flate2::write::GzEncoder;
use futures_lite::io::{AsyncRead, AsyncWrite};
#[cfg(any(target_os = "linux", test))]
use lzma_rust2::XzReader;
use lzma_rust2::{LzmaOptions, LzmaReader, LzmaWriter, XzOptions, XzReaderMt, XzWriter};
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, BufWriter, Read, Write};
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};

const BUFFER_SIZE: usize = 64 * 1024;
const DEFAULT_LEVEL: u32 = 6;

fn xz_worker_count() -> u32 {
    std::thread::available_parallelism()
        .map_or(1, |count| count.get().min(u32::MAX as usize) as u32)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Compression {
    Auto,
    Gz,
    Bz2,
    Xz,
    Lzma,
}

pub(crate) struct BlockingAsyncIo<T> {
    inner: T,
}

impl<T> BlockingAsyncIo<T> {
    pub(crate) fn new(inner: T) -> Self {
        Self { inner }
    }

    pub(crate) fn into_inner(self) -> T {
        self.inner
    }
}

impl<T: Read + Unpin> AsyncRead for BlockingAsyncIo<T> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(self.inner.read(buf))
    }
}

impl<T: Write + Unpin> AsyncWrite for BlockingAsyncIo<T> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(self.inner.write(buf))
    }

    fn poll_flush(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(self.inner.flush())
    }

    fn poll_close(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(self.inner.flush())
    }
}

pub(crate) fn parse(value: &str, span: Span) -> Result<Compression, RuntimeError> {
    match value {
        "auto" | "" => Ok(Compression::Auto),
        "gz" | "gzip" => Ok(Compression::Gz),
        "bz2" | "bzip2" => Ok(Compression::Bz2),
        "xz" => Ok(Compression::Xz),
        "lzma" => Ok(Compression::Lzma),
        _ => {
            Err(RuntimeError::new("archive-compression", "unsupported compression").with_span(span))
        }
    }
}

pub(crate) fn for_create(path: &Path, compression: Compression) -> Option<Compression> {
    match compression {
        Compression::Auto => from_extension(path),
        mode => Some(mode),
    }
}

pub(crate) fn level(level: i64, span: Span) -> Result<u32, RuntimeError> {
    if (0..=9).contains(&level) {
        Ok(level as u32)
    } else {
        Err(
            RuntimeError::new("archive-compression", "level must be between 0 and 9")
                .with_span(span),
        )
    }
}

pub(crate) fn archive_reader(
    path: &Path,
    compression: Compression,
    span: Span,
) -> Result<Box<dyn Read + Send>, RuntimeError> {
    let file = File::open(path).map_err(|error| error_with_kind("archive-open", error, span))?;
    let mut reader = BufReader::with_capacity(BUFFER_SIZE, file);
    let compression = match compression {
        Compression::Auto => detect(&mut reader, span)?.or_else(|| from_extension(path)),
        mode => Some(mode),
    };
    Ok(match compression {
        Some(Compression::Gz) => Box::new(MultiGzDecoder::new(reader)),
        Some(Compression::Bz2) => Box::new(MultiBzDecoder::new(reader)),
        Some(Compression::Xz) => Box::new(
            XzReaderMt::new(reader, true, xz_worker_count())
                .map_err(|error| error_with_kind("archive-open", error, span))?,
        ),
        Some(Compression::Lzma) => Box::new(
            LzmaReader::new_mem_limit(reader, u32::MAX, None)
                .map_err(|error| error_with_kind("archive-open", error, span))?,
        ),
        Some(Compression::Auto) | None => Box::new(reader),
    })
}

pub(crate) fn codec_reader(
    path: &Path,
    compression: Compression,
    span: Span,
) -> Result<Box<dyn Read>, RuntimeError> {
    let file = File::open(path).map_err(|error| error_with_kind("archive-open", error, span))?;
    let mut reader = BufReader::with_capacity(BUFFER_SIZE, file);
    let compression = match compression {
        Compression::Auto => detect(&mut reader, span)?
            .or_else(|| from_extension(path))
            .ok_or_else(|| {
                RuntimeError::new("archive-compression", "compression format required")
                    .with_span(span)
            })?,
        mode => mode,
    };
    Ok(match compression {
        Compression::Gz => Box::new(MultiGzDecoder::new(reader)),
        Compression::Bz2 => Box::new(MultiBzDecoder::new(reader)),
        Compression::Xz => Box::new(
            XzReaderMt::new(reader, true, xz_worker_count())
                .map_err(|error| error_with_kind("archive-open", error, span))?,
        ),
        Compression::Lzma => Box::new(
            LzmaReader::new_mem_limit(reader, u32::MAX, None)
                .map_err(|error| error_with_kind("archive-open", error, span))?,
        ),
        Compression::Auto => unreachable!("auto resolved above"),
    })
}

#[cfg(any(target_os = "linux", test))]
pub(crate) fn linux_module_reader(path: &Path) -> io::Result<Box<dyn Read>> {
    let file = File::open(path)?;
    let reader = BufReader::with_capacity(BUFFER_SIZE, file);
    let name = path.to_string_lossy();
    Ok(if name.ends_with(".gz") {
        Box::new(flate2::bufread::GzDecoder::new(reader))
    } else if name.ends_with(".xz") {
        Box::new(XzReader::new(reader, false))
    } else if name.ends_with(".bz2") {
        Box::new(bzip2::bufread::BzDecoder::new(reader))
    } else {
        Box::new(reader)
    })
}

pub(crate) fn copy_compressed(
    mut input: File,
    output: File,
    compression: Compression,
    level: u32,
    input_len: u64,
    span: Span,
) -> Result<(), RuntimeError> {
    let writer = BufWriter::with_capacity(BUFFER_SIZE, output);
    match compression {
        Compression::Gz => {
            let mut writer = GzEncoder::new(writer, GzipCompression::new(level));
            io::copy(&mut input, &mut writer)
                .map_err(|error| error_with_kind("archive-compress", error, span))?;
            writer
                .try_finish()
                .map_err(|error| error_with_kind("archive-compress", error, span))
        }
        Compression::Bz2 => {
            let mut writer = BzEncoder::new(writer, Bzip2Compression::new(level));
            io::copy(&mut input, &mut writer)
                .map_err(|error| error_with_kind("archive-compress", error, span))?;
            writer
                .try_finish()
                .map_err(|error| error_with_kind("archive-compress", error, span))
        }
        Compression::Xz => {
            let mut writer = XzWriter::new(writer, XzOptions::with_preset(level))
                .map_err(|error| error_with_kind("archive-compress", error, span))?;
            io::copy(&mut input, &mut writer)
                .map_err(|error| error_with_kind("archive-compress", error, span))?;
            writer
                .finish()
                .map(|_| ())
                .map_err(|error| error_with_kind("archive-compress", error, span))
        }
        Compression::Lzma => {
            let options = LzmaOptions::with_preset(level);
            let mut writer = LzmaWriter::new_use_header(writer, &options, Some(input_len))
                .map_err(|error| error_with_kind("archive-compress", error, span))?;
            io::copy(&mut input, &mut writer)
                .map_err(|error| error_with_kind("archive-compress", error, span))?;
            writer
                .finish()
                .map(|_| ())
                .map_err(|error| error_with_kind("archive-compress", error, span))
        }
        Compression::Auto => unreachable!("auto resolved before compression"),
    }
}

#[allow(clippy::large_enum_variant)]
pub(crate) enum ArchiveWriter {
    Plain(BufWriter<File>),
    Gz(GzEncoder<BufWriter<File>>),
    Bz2(BzEncoder<BufWriter<File>>),
    Xz(XzWriter<BufWriter<File>>),
    Lzma(LzmaWriter<BufWriter<File>>),
}

impl ArchiveWriter {
    pub(crate) fn create(
        path: &Path,
        compression: Option<Compression>,
        overwrite: bool,
        span: Span,
    ) -> Result<Self, RuntimeError> {
        let file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .create_new(!overwrite)
            .truncate(overwrite)
            .open(path)
            .map_err(|error| error_with_kind("archive-create", error, span))?;
        let writer = BufWriter::with_capacity(BUFFER_SIZE, file);
        match compression {
            Some(Compression::Gz) => Ok(Self::Gz(GzEncoder::new(
                writer,
                GzipCompression::new(DEFAULT_LEVEL),
            ))),
            Some(Compression::Bz2) => Ok(Self::Bz2(BzEncoder::new(
                writer,
                Bzip2Compression::new(DEFAULT_LEVEL),
            ))),
            Some(Compression::Xz) => XzWriter::new(writer, XzOptions::with_preset(DEFAULT_LEVEL))
                .map(Self::Xz)
                .map_err(|error| error_with_kind("archive-create", error, span)),
            Some(Compression::Lzma) => {
                LzmaWriter::new_use_header(writer, &LzmaOptions::with_preset(DEFAULT_LEVEL), None)
                    .map(Self::Lzma)
                    .map_err(|error| error_with_kind("archive-create", error, span))
            }
            Some(Compression::Auto) | None => Ok(Self::Plain(writer)),
        }
    }

    pub(crate) fn finish(self, span: Span) -> Result<(), RuntimeError> {
        match self {
            Self::Plain(mut writer) => writer
                .flush()
                .map_err(|error| error_with_kind("archive-create", error, span)),
            Self::Gz(mut writer) => writer
                .try_finish()
                .map_err(|error| error_with_kind("archive-create", error, span)),
            Self::Bz2(mut writer) => writer
                .try_finish()
                .map_err(|error| error_with_kind("archive-create", error, span)),
            Self::Xz(writer) => writer
                .finish()
                .map(|_| ())
                .map_err(|error| error_with_kind("archive-create", error, span)),
            Self::Lzma(writer) => writer
                .finish()
                .map(|_| ())
                .map_err(|error| error_with_kind("archive-create", error, span)),
        }
    }
}

impl Write for ArchiveWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::Plain(writer) => writer.write(buf),
            Self::Gz(writer) => writer.write(buf),
            Self::Bz2(writer) => writer.write(buf),
            Self::Xz(writer) => writer.write(buf),
            Self::Lzma(writer) => writer.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Plain(writer) => writer.flush(),
            Self::Gz(writer) => writer.flush(),
            Self::Bz2(writer) => writer.flush(),
            Self::Xz(writer) => writer.flush(),
            Self::Lzma(writer) => writer.flush(),
        }
    }
}

fn detect<R: BufRead>(reader: &mut R, span: Span) -> Result<Option<Compression>, RuntimeError> {
    let header = reader
        .fill_buf()
        .map_err(|error| error_with_kind("archive-read", error, span))?;
    if header.starts_with(&[0x1f, 0x8b]) {
        Ok(Some(Compression::Gz))
    } else if header.starts_with(b"BZh") {
        Ok(Some(Compression::Bz2))
    } else if header.starts_with(&[0xfd, b'7', b'z', b'X', b'Z', 0x00]) {
        Ok(Some(Compression::Xz))
    } else {
        Ok(None)
    }
}

fn from_extension(path: &Path) -> Option<Compression> {
    let name = path.to_string_lossy();
    if name.ends_with(".gz") || name.ends_with(".tgz") {
        Some(Compression::Gz)
    } else if name.ends_with(".bz2") || name.ends_with(".tbz") || name.ends_with(".tbz2") {
        Some(Compression::Bz2)
    } else if name.ends_with(".xz") || name.ends_with(".txz") {
        Some(Compression::Xz)
    } else if name.ends_with(".lzma") || name.ends_with(".tlz") {
        Some(Compression::Lzma)
    } else {
        None
    }
}

fn error_with_kind(kind: &str, error: impl ToString, span: Span) -> RuntimeError {
    RuntimeError::new(kind, error.to_string()).with_span(span)
}
