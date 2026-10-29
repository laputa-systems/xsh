use crate::runtime::value::RuntimeError;
use crate::source::Span;
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::Path;
use xsh_archive::{BUFFER_SIZE, Codec, Encode, Encoder, GzipHeader, SNIFF_LEN};

#[cfg(any(target_os = "linux", test))]
pub(crate) fn linux_module_reader(path: &Path) -> io::Result<Box<dyn Read>> {
    let file = File::open(path)?;
    let reader = BufReader::with_capacity(BUFFER_SIZE, file);
    let name = path.to_string_lossy();
    let codec = if name.ends_with(".gz") {
        Some(Codec::Gzip)
    } else if name.ends_with(".xz") {
        Some(Codec::Xz)
    } else if name.ends_with(".bz2") {
        Some(Codec::Bzip2)
    } else {
        None
    };
    Ok(match codec {
        Some(codec) => {
            let decoder = xsh_archive::decoder(codec, reader, xsh_archive::Limits::default().workspace_bytes)?;
            Box::new(decoder)
        }
        None => Box::new(reader),
    })
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
    let header = xsh_archive::gzip_header_name(BufReader::with_capacity(BUFFER_SIZE, file))
        .ok_or_else(|| RuntimeError::new("compression-header", "invalid gzip header").with_span(span))?;
    Ok(header.and_then(|name| {
        // Stored directory names cannot redirect the output outside its directory.
        let name = Path::new(std::ffi::OsStr::from_bytes(&name)).file_name()?;
        if name.is_empty() || name == "." || name == ".." { None }
        else { Some(std::path::PathBuf::from(name)) }
    }))
}

// Read stdin from its descriptor so all native I/O shares the same cursor.
struct StdinReader;

impl Read for StdinReader {
    fn read(&mut self, data: &mut [u8]) -> io::Result<usize> {
        crate::startup_stdio::check(0)?;
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
    let result = transform_io(request);
    result.map_err(|error| RuntimeError::host("compression-transform", &error).with_span(span))
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

fn transform_io(request: TransformRequest<'_>) -> io::Result<()> {
    if request.test && (request.destination.is_some() || !request.decode) {
        return Err(invalid("integrity testing requires decode and no destination"));
    }
    if request.pass_through && (!request.decode || request.test || request.destination.is_some()) {
        return Err(invalid("pass-through requires decoding to stdout without integrity testing"));
    }
    let formats = parse_formats(request.format, request.decode)?;
    let codec = formats[0];
    // The level only shapes encoding, so a detected format has no level to check.
    let encode = if formats.len() > 1 {
        None
    } else {
        Some(Encode::new(codec, Some(request.level)).map_err(io::Error::from)?)
    };
    let mut metadata = None;
    let input: Box<dyn Read + Send> = if let Some(path) = request.source {
        let file = File::open(path)?;
        metadata = Some(file.metadata()?);
        Box::new(file)
    } else {
        Box::new(StdinReader)
    };
    if let Some(destination) = request.destination
        && let Some(source) = request.source
    {
        use std::os::unix::fs::MetadataExt;
        let same_file = match fs::metadata(destination) {
            Ok(destination) => metadata.as_ref().is_some_and(|source| source.dev() == destination.dev() && source.ino() == destination.ino()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => false,
            Err(error) => return Err(error),
        };
        if source == destination || same_file {
            return Err(invalid("source and destination are the same file"));
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
        crate::startup_stdio::check(1)?;
        Box::new(io::stdout().lock())
    };
    let original_time = if request.decode {
        decode(input, output, &formats, request.pass_through)?
    } else {
        let mut encode = encode.expect("encoding names exactly one format");
        if request.metadata && codec == Codec::Gzip {
            use std::os::unix::ffi::OsStrExt;
            let name = request.source.and_then(Path::file_name).map(|name| name.as_bytes().to_vec()).unwrap_or_default();
            let mtime = metadata.as_ref().map(fs::Metadata::modified).transpose()?
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs().min(u32::MAX as u64) as u32).unwrap_or(0);
            encode = encode.with_gzip_header(GzipHeader { name, mtime });
        }
        if let Some(length) = metadata.as_ref().map(fs::Metadata::len) {
            encode = encode.with_size_hint(length);
        }
        write_encoded(input, output, &encode)?;
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

fn write_encoded<R: Read, W: Write>(mut input: R, output: W, encode: &Encode) -> io::Result<()> {
    let writer = BufWriter::with_capacity(BUFFER_SIZE, output);
    let mut encoder = Encoder::new(Some(encode), writer).map_err(io::Error::from)?;
    io::copy(&mut input, &mut encoder)?;
    encoder.finish().map_err(io::Error::from)?.flush()
}

// Candidate formats for one transform. A single name is that format; "auto"
// detects among every decodable format and a comma-separated list detects among
// the named ones. Detection only makes sense when decoding, because encoding has
// no input to inspect.
fn parse_formats(value: &str, decode: bool) -> io::Result<Vec<Codec>> {
    let formats = if value == "auto" {
        Codec::ALL.to_vec()
    } else {
        let mut formats = Vec::new();
        for name in value.split(',') {
            match codec_named(name) {
                Some(codec) => formats.push(codec),
                None if name == "auto" || name.is_empty() => return Err(invalid("explicit compression format required")),
                None => return Err(invalid("unsupported compression")),
            }
        }
        formats
    };
    if formats.len() > 1 && !decode {
        return Err(invalid("explicit compression format required"));
    }
    Ok(formats)
}

fn codec_named(name: &str) -> Option<Codec> {
    match name {
        "gz" | "gzip" => Some(Codec::Gzip),
        "bz2" | "bzip2" => Some(Codec::Bzip2),
        "xz" => Some(Codec::Xz),
        "lzma" => Some(Codec::Lzma),
        "zstd" | "zst" => Some(Codec::Zstd),
        _ => None,
    }
}

// Decodes `input`, choosing the codec from its first bytes when several formats
// are candidates. The peeked bytes are replayed in front of the stream, so a
// pipe or standard input is read once. Input that matches no candidate is copied
// through when `pass_through` is set (xz -dcf); otherwise a single named format
// is handed to its codec, which reports what is wrong, and several candidates
// are rejected as unrecognized.
fn decode<R: Read + Send + 'static, W: Write>(mut input: R, output: W, formats: &[Codec], pass_through: bool) -> io::Result<Option<u32>> {
    let mut writer = BufWriter::with_capacity(BUFFER_SIZE, output);
    let detected = formats.len() > 1;
    let mut prefix = Vec::new();
    let mut chosen = formats[0];
    if detected || pass_through {
        input.by_ref().take(SNIFF_LEN as u64).read_to_end(&mut prefix)?;
        match formats.iter().find(|format| format.recognizes(&prefix, detected)) {
            Some(format) => chosen = *format,
            None if pass_through => {
                writer.write_all(&prefix)?;
                io::copy(&mut input, &mut writer)?;
                writer.flush()?;
                return Ok(None);
            }
            None if detected && prefix.starts_with(b"LZIP") => {
                return Err(io::Error::new(io::ErrorKind::Unsupported, "lzip streams are not supported"));
            }
            None if detected => return Err(io::Error::new(io::ErrorKind::InvalidData, "File format not recognized")),
            None => {}
        }
    }
    let reader = BufReader::with_capacity(BUFFER_SIZE, io::Cursor::new(prefix).chain(input));
    let workspace = xsh_archive::Limits::default().workspace_bytes;
    let (mut reader, mtime) = xsh_archive::decoder_with_mtime(chosen, reader, workspace).map_err(io::Error::from)?;
    io::copy(&mut reader, &mut writer)?;
    writer.flush()?;
    Ok(mtime)
}
