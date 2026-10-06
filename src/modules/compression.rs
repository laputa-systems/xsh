use crate::runtime::value::RuntimeError;
use crate::source::Span;
use bzip2::Compression as Bzip2Compression;
use bzip2::bufread::MultiBzDecoder;
use bzip2::write::BzEncoder;
use flate2::Compression as GzipCompression;
use flate2::bufread::MultiGzDecoder;
use flate2::write::GzEncoder;
use lzma_rust2::XzReader;
use lzma_rust2::{LzmaOptions, LzmaReader, LzmaWriter, XzOptions, XzWriter};
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, BufWriter, Read, Write};
use std::path::Path;

const BUFFER_SIZE: usize = 64 * 1024;
const DEFAULT_LEVEL: u32 = 6;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Compression {
    Auto,
    Gz,
    Bz2,
    Xz,
    Lzma,
    Zstd,
}

pub(crate) fn parse(value: &str, span: Span) -> Result<Compression, RuntimeError> {
    match value {
        "auto" | "" => Ok(Compression::Auto),
        "gz" | "gzip" => Ok(Compression::Gz),
        "bz2" | "bzip2" => Ok(Compression::Bz2),
        "xz" => Ok(Compression::Xz),
        "lzma" => Ok(Compression::Lzma),
        "zstd" | "zst" => Ok(Compression::Zstd),
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
    match compression {
        Some(Compression::Auto) | None => Ok(Box::new(reader)),
        Some(compression) => decoded_reader(reader, compression)
            .map(|(reader, _)| reader)
            .map_err(|error| error_with_kind("archive-open", error, span)),
    }
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
    decoded_reader(reader, compression)
        .map(|(reader, _)| reader as Box<dyn Read>)
        .map_err(|error| error_with_kind("archive-open", error, span))
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
    input: File,
    output: File,
    compression: Compression,
    level: u32,
    input_len: u64,
    span: Span,
) -> Result<(), RuntimeError> {
    encode(input, output, compression, level as i32, Some(input_len), None)
        .map_err(|error| error_with_kind("archive-compress", error, span))
}

struct GzipMetadata {
    name: Vec<u8>,
    mtime: u32,
}

fn encode<R: Read, W: Write>(
    mut input: R,
    output: W,
    compression: Compression,
    level: i32,
    input_len: Option<u64>,
    metadata: Option<GzipMetadata>,
) -> io::Result<()> {
    let writer = BufWriter::with_capacity(BUFFER_SIZE, output);
    match compression {
        Compression::Gz => {
            let mut builder = flate2::GzBuilder::new();
            if let Some(metadata) = metadata {
                builder = builder.filename(metadata.name).mtime(metadata.mtime);
            }
            let mut writer = builder.write(writer, GzipCompression::new(level as u32));
            io::copy(&mut input, &mut writer)?;
            writer.finish()?.flush()
        }
        Compression::Bz2 => {
            if !(1..=9).contains(&level) {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, "bzip2 level must be between 1 and 9"));
            }
            let mut writer = BzEncoder::new(writer, Bzip2Compression::new(level as u32));
            io::copy(&mut input, &mut writer)?;
            writer.finish()?.flush()
        }
        Compression::Xz => {
            let mut writer = XzWriter::new(writer, XzOptions::with_preset(level as u32))?;
            io::copy(&mut input, &mut writer)?;
            writer.finish()?.flush()
        }
        Compression::Lzma => {
            let options = LzmaOptions::with_preset(level as u32);
            let mut writer = LzmaWriter::new_use_header(writer, &options, input_len)?;
            io::copy(&mut input, &mut writer)?;
            writer.finish()?.flush()
        }
        Compression::Zstd => {
            let mut writer = zstd::stream::write::Encoder::new(writer, level)?;
            writer.include_checksum(true)?;
            io::copy(&mut input, &mut writer)?;
            writer.finish()?.flush()
        }
        Compression::Auto => Err(io::Error::new(io::ErrorKind::InvalidInput, "compression format required")),
    }
}

pub(crate) struct TransformRequest<'a> {
    pub(crate) source: Option<&'a Path>,
    pub(crate) destination: Option<&'a Path>,
    pub(crate) format: &'a str,
    pub(crate) decode: bool,
    pub(crate) level: i64,
    pub(crate) test: bool,
    pub(crate) metadata: bool,
    pub(crate) overwrite: bool,
    pub(crate) pass_through: bool,
}

pub(crate) fn gzip_name(path: &Path, span: Span) -> Result<Option<std::path::PathBuf>, RuntimeError> {
    use std::os::unix::ffi::OsStrExt;
    let file = File::open(path).map_err(|error| RuntimeError::host("compression-header", &error).with_span(span))?;
    let decoder = flate2::bufread::GzDecoder::new(BufReader::with_capacity(BUFFER_SIZE, file));
    let header = decoder.header().ok_or_else(|| RuntimeError::new("compression-header", "invalid gzip header").with_span(span))?;
    Ok(header.filename().and_then(|name| {
        // Stored directory names cannot redirect the output outside its directory.
        let name = Path::new(std::ffi::OsStr::from_bytes(name)).file_name()?;
        if name.is_empty() || name == "." || name == ".." { None }
        else { Some(std::path::PathBuf::from(name)) }
    }))
}

// Read stdin from its descriptor so all native I/O shares the same cursor.
struct StdinReader;

impl Read for StdinReader {
    fn read(&mut self, data: &mut [u8]) -> io::Result<usize> {
        loop {
            match rustix::io::read(rustix::stdio::stdin(), &mut *data) {
                Ok(count) => return Ok(count),
                Err(rustix::io::Errno::INTR) => continue,
                Err(error) => return Err(error.into()),
            }
        }
    }
}

pub(crate) fn transform(request: TransformRequest<'_>, span: Span) -> Result<(), RuntimeError> {
    let result = transform_io(request, span);
    result.map_err(|error| RuntimeError::host("compression-transform", &error).with_span(span))
}

fn transform_io(request: TransformRequest<'_>, span: Span) -> io::Result<()> {
    if request.test && (request.destination.is_some() || !request.decode) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "integrity testing requires decode and no destination"));
    }
    if request.pass_through && (!request.decode || request.test || request.destination.is_some()) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "pass-through requires decoding to stdout without integrity testing"));
    }
    let compression = parse(request.format, span)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.message.to_string()))?;
    if compression == Compression::Auto {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "explicit compression format required"));
    }
    let level = if compression == Compression::Zstd {
        if !(-131072..=22).contains(&request.level) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "zstd level must be between -131072 and 22"));
        }
        request.level as i32
    } else {
        level(request.level, span)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.message.to_string()))? as i32
    };
    if !request.decode && compression == Compression::Bz2 && level == 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "bzip2 level must be between 1 and 9"));
    }
    let mut metadata = None;
    let input: Box<dyn Read + Send> = if let Some(path) = request.source {
        let file = File::open(path)?;
        metadata = Some(file.metadata()?);
        Box::new(file)
    } else {
        Box::new(StdinReader)
    };
    if let Some(destination) = request.destination {
        if let Some(source) = request.source {
            use std::os::unix::fs::MetadataExt;
            let same_file = match fs::metadata(destination) {
                Ok(destination) => metadata.as_ref().is_some_and(|source| source.dev() == destination.dev() && source.ino() == destination.ino()),
                Err(error) if error.kind() == io::ErrorKind::NotFound => false,
                Err(error) => return Err(error),
            };
            if source == destination || same_file {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, "source and destination are the same file"));
            }
        }
    }
    let mut temp = request.destination.map(|path| {
        use std::os::unix::fs::PermissionsExt;
        let parent = path.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
        tempfile::Builder::new().permissions(fs::Permissions::from_mode(0o666)).tempfile_in(parent)
    }).transpose()?;
    let output: Box<dyn Write + '_> = if request.test {
        Box::new(io::sink())
    } else if let Some(temp) = temp.as_mut() {
        Box::new(temp.as_file_mut())
    } else {
        Box::new(io::stdout().lock())
    };
    let original_time = if request.decode {
        decode(input, output, compression, request.pass_through)?
    } else {
        let header = if request.metadata && compression == Compression::Gz {
            use std::os::unix::ffi::OsStrExt;
            let name = request.source.and_then(Path::file_name).map(|name| name.as_bytes().to_vec()).unwrap_or_default();
            let mtime = metadata.as_ref().map(fs::Metadata::modified).transpose()?
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs().min(u32::MAX as u64) as u32).unwrap_or(0);
            Some(GzipMetadata { name, mtime })
        } else { None };
        encode(input, output, compression, level, metadata.as_ref().map(fs::Metadata::len), header)?;
        None
    };
    if let Some(temp) = temp {
        if let Some(metadata) = metadata {
            temp.as_file().set_permissions(metadata.permissions())?;
            let modified = if request.metadata {
                original_time.map(|seconds| std::time::UNIX_EPOCH + std::time::Duration::from_secs(seconds as u64))
                    .or(Some(metadata.modified()?))
            } else { Some(metadata.modified()?) };
            let mut times = fs::FileTimes::new().set_accessed(metadata.accessed()?);
            if let Some(modified) = modified { times = times.set_modified(modified); }
            temp.as_file().set_times(times)?;
        }
        temp.as_file().sync_all()?;
        if request.overwrite { temp.persist(request.destination.expect("destination owns temp")) }
        else { temp.persist_noclobber(request.destination.expect("destination owns temp")) }
            .map_err(|error| error.error)?;
    }
    Ok(())
}

fn decode<R: Read + Send + 'static, W: Write>(mut input: R, output: W, compression: Compression, pass_through: bool) -> io::Result<Option<u32>> {
    let mut writer = BufWriter::with_capacity(BUFFER_SIZE, output);
    let mut prefix = Vec::new();
    if pass_through {
        let size = match compression { Compression::Gz => 2, Compression::Bz2 => 3, Compression::Xz => 6, Compression::Lzma => 13, Compression::Zstd => 4, Compression::Auto => 0 };
        input.by_ref().take(size).read_to_end(&mut prefix)?;
        let packed = match compression {
            Compression::Gz => prefix.starts_with(&[0x1f, 0x8b]),
            Compression::Bz2 => prefix.starts_with(b"BZh"),
            Compression::Xz => prefix.starts_with(&[0xfd, b'7', b'z', b'X', b'Z', 0]),
            Compression::Lzma => prefix.len() == 13 && prefix[0] < 225,
            Compression::Zstd => zstd_header(&prefix),
            Compression::Auto => false,
        };
        if !packed {
            writer.write_all(&prefix)?;
            io::copy(&mut input, &mut writer)?;
            writer.flush()?;
            return Ok(None);
        }
    }
    let reader = BufReader::with_capacity(BUFFER_SIZE, io::Cursor::new(prefix).chain(input));
    let (mut reader, mtime) = decoded_reader(reader, compression)?;
    io::copy(&mut reader, &mut writer)?;
    writer.flush()?;
    Ok(mtime)
}

fn decoded_reader<R: BufRead + Send + 'static>(reader: R, compression: Compression) -> io::Result<(Box<dyn Read + Send>, Option<u32>)> {
    let mut mtime = None;
    let reader: Box<dyn Read + Send> = match compression {
        Compression::Gz => {
            let decoder = MultiGzDecoder::new(reader);
            mtime = decoder.header().map(flate2::GzHeader::mtime).filter(|time| *time != 0);
            Box::new(decoder)
        }
        Compression::Bz2 => Box::new(MultiBzDecoder::new(reader)),
        Compression::Xz => Box::new(XzReader::new(reader, true)),
        Compression::Lzma => Box::new(LzmaReader::new_mem_limit(reader, 256 * 1024, None)?),
        Compression::Zstd => Box::new(zstd::stream::read::Decoder::with_buffer(reader)?),
        Compression::Auto => return Err(io::Error::new(io::ErrorKind::InvalidInput, "compression format required")),
    };
    Ok((reader, mtime))
}

#[allow(clippy::large_enum_variant)]
pub(crate) enum ArchiveWriter {
    Plain(BufWriter<File>),
    Gz(GzEncoder<BufWriter<File>>),
    Bz2(BzEncoder<BufWriter<File>>),
    Xz(XzWriter<BufWriter<File>>),
    Lzma(LzmaWriter<BufWriter<File>>),
    Zstd(zstd::stream::write::Encoder<'static, BufWriter<File>>),
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
            Some(Compression::Zstd) => {
                let mut writer = zstd::stream::write::Encoder::new(writer, DEFAULT_LEVEL as i32)
                    .map_err(|error| error_with_kind("archive-create", error, span))?;
                writer.include_checksum(true).map_err(|error| error_with_kind("archive-create", error, span))?;
                Ok(Self::Zstd(writer))
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
            Self::Zstd(writer) => writer.finish().and_then(|mut writer| writer.flush())
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
            Self::Zstd(writer) => writer.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Plain(writer) => writer.flush(),
            Self::Gz(writer) => writer.flush(),
            Self::Bz2(writer) => writer.flush(),
            Self::Xz(writer) => writer.flush(),
            Self::Lzma(writer) => writer.flush(),
            Self::Zstd(writer) => writer.flush(),
        }
    }
}

fn zstd_header(header: &[u8]) -> bool {
    header.starts_with(&[0x28, 0xb5, 0x2f, 0xfd])
        || (header.len() >= 4 && (0x50..=0x5f).contains(&header[0]) && header[1..4] == [0x2a, 0x4d, 0x18])
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
    } else if zstd_header(header) {
        Ok(Some(Compression::Zstd))
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
    } else if name.ends_with(".zst") || name.ends_with(".tzst") {
        Some(Compression::Zstd)
    } else {
        None
    }
}

fn error_with_kind(kind: &str, error: impl ToString, span: Span) -> RuntimeError {
    RuntimeError::new(kind, error.to_string()).with_span(span)
}
