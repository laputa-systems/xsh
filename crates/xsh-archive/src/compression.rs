use crate::error::{Error, Result};
use bzip2::Compression as Bzip2Level;
use bzip2::bufread::MultiBzDecoder;
use bzip2::write::BzEncoder;
use flate2::Compression as GzipLevel;
use flate2::bufread::MultiGzDecoder;
use flate2::write::GzEncoder;
use lzma_rust2::{LzmaOptions, LzmaReader, LzmaWriter, XzOptions, XzReader, XzWriter};
use std::io::{self, BufRead, Read, Write};

pub const BUFFER_SIZE: usize = 64 * 1024;
pub const DEFAULT_LEVEL: i64 = 6;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Codec {
    Gzip,
    Bzip2,
    Xz,
    Lzma,
    Zstd,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Compression {
    Auto,
    Plain,
    Codec(Codec),
}

impl Codec {
    pub const ALL: [Self; 5] = [Self::Gzip, Self::Bzip2, Self::Xz, Self::Lzma, Self::Zstd];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Gzip => "gzip",
            Self::Bzip2 => "bzip2",
            Self::Xz => "xz",
            Self::Lzma => "lzma",
            Self::Zstd => "zstd",
        }
    }

    pub const fn level_range(self) -> (i64, i64) {
        match self {
            Self::Gzip | Self::Xz | Self::Lzma => (0, 9),
            Self::Bzip2 => (1, 9),
            Self::Zstd => (-131_072, 22),
        }
    }

    pub fn from_suffix(name: &[u8]) -> Option<Self> {
        const SUFFIXES: [(&[u8], Codec); 11] = [
            (b".gz", Codec::Gzip),
            (b".tgz", Codec::Gzip),
            (b".bz2", Codec::Bzip2),
            (b".tbz", Codec::Bzip2),
            (b".tbz2", Codec::Bzip2),
            (b".xz", Codec::Xz),
            (b".txz", Codec::Xz),
            (b".lzma", Codec::Lzma),
            (b".tlz", Codec::Lzma),
            (b".zst", Codec::Zstd),
            (b".tzst", Codec::Zstd),
        ];
        SUFFIXES
            .iter()
            .find(|(suffix, _)| name.ends_with(suffix))
            .map(|(_, codec)| *codec)
    }

    pub fn detect_magic(header: &[u8]) -> Option<Self> {
        [Self::Gzip, Self::Bzip2, Self::Xz, Self::Zstd]
            .into_iter()
            .find(|codec| codec.recognizes(header, true))
    }

    pub fn recognizes(self, header: &[u8], detected: bool) -> bool {
        match self {
            Self::Gzip => header.starts_with(&[0x1f, 0x8b]),
            Self::Bzip2 => header.starts_with(b"BZh"),
            Self::Xz => header.starts_with(&[0xfd, b'7', b'z', b'X', b'Z', 0]),
            Self::Lzma => lzma_header(header, detected),
            Self::Zstd => zstd_header(header),
        }
    }
}

pub const SNIFF_LEN: usize = 13;

fn lzma_header(header: &[u8], detected: bool) -> bool {
    if header.len() < SNIFF_LEN || header[0] >= 225 {
        return false;
    }
    let dictionary = u32::from_le_bytes([header[1], header[2], header[3], header[4]]);
    let standard_size = dictionary.is_power_of_two()
        || (dictionary.is_multiple_of(3) && (dictionary / 3).is_power_of_two());
    if detected && dictionary != u32::MAX && !standard_size {
        return false;
    }
    let size = u64::from_le_bytes(header[5..13].try_into().expect("eight size bytes"));
    size == u64::MAX || size < 1 << 38
}

fn zstd_header(header: &[u8]) -> bool {
    header.starts_with(&[0x28, 0xb5, 0x2f, 0xfd])
        || (header.len() >= 4
            && (0x50..=0x5f).contains(&header[0])
            && header[1..4] == [0x2a, 0x4d, 0x18])
}

impl Compression {
    pub fn resolve_read(self, peeked: &[u8], name: Option<&[u8]>) -> Option<Codec> {
        match self {
            Self::Plain => None,
            Self::Codec(codec) => Some(codec),
            Self::Auto => Codec::detect_magic(peeked)
                .or_else(|| name.and_then(Codec::from_suffix)),
        }
    }

    pub fn resolve_write(self, name: &[u8]) -> Option<Codec> {
        match self {
            Self::Plain => None,
            Self::Codec(codec) => Some(codec),
            Self::Auto => Codec::from_suffix(name),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GzipHeader {
    pub name: Vec<u8>,
    pub mtime: u32,
}

#[derive(Clone, Debug)]
pub struct Encode {
    codec: Codec,
    level: i32,
    gzip_header: Option<GzipHeader>,
    size_hint: Option<u64>,
}

impl Encode {
    pub fn new(codec: Codec, level: Option<i64>) -> Result<Self> {
        let level = level.unwrap_or(DEFAULT_LEVEL);
        let (low, high) = codec.level_range();
        if !(low..=high).contains(&level) {
            return Err(Error::options(format!(
                "{} level must be between {low} and {high}",
                codec.name()
            )));
        }
        Ok(Self {
            codec,
            level: level as i32,
            gzip_header: None,
            size_hint: None,
        })
    }

    #[must_use]
    pub fn with_gzip_header(mut self, header: GzipHeader) -> Self {
        self.gzip_header = Some(header);
        self
    }

    #[must_use]
    pub fn with_size_hint(mut self, size: u64) -> Self {
        self.size_hint = Some(size);
        self
    }

    pub const fn codec(&self) -> Codec {
        self.codec
    }
}

pub struct Encoder<W: Write> {
    inner: EncoderInner<W>,
}

enum EncoderInner<W: Write> {
    Plain(W),
    Gzip(GzEncoder<W>),
    Bzip2(BzEncoder<W>),
    Xz(Box<XzWriter<W>>),
    Lzma(Box<LzmaWriter<W>>),
    Zstd(zstd::stream::write::Encoder<'static, W>),
}

impl<W: Write> Encoder<W> {
    pub fn new(encode: Option<&Encode>, sink: W) -> Result<Self> {
        let Some(encode) = encode else {
            return Ok(Self {
                inner: EncoderInner::Plain(sink),
            });
        };
        let level = encode.level;
        let inner = match encode.codec {
            Codec::Gzip => {
                let level = GzipLevel::new(level as u32);
                let mut builder = flate2::GzBuilder::new();
                if let Some(header) = &encode.gzip_header {
                    builder = builder.filename(header.name.clone()).mtime(header.mtime);
                }
                EncoderInner::Gzip(builder.write(sink, level))
            }
            Codec::Bzip2 => EncoderInner::Bzip2(BzEncoder::new(sink, Bzip2Level::new(level as u32))),
            Codec::Xz => EncoderInner::Xz(Box::new(
                XzWriter::new(sink, XzOptions::with_preset(level as u32))
                    .map_err(|error| Error::from_host(&error))?,
            )),
            Codec::Lzma => EncoderInner::Lzma(Box::new(
                LzmaWriter::new_use_header(
                    sink,
                    &LzmaOptions::with_preset(level as u32),
                    encode.size_hint,
                )
                .map_err(|error| Error::from_host(&error))?,
            )),
            Codec::Zstd => {
                let mut encoder = zstd::stream::write::Encoder::new(sink, level)
                    .map_err(|error| Error::from_host(&error))?;
                encoder
                    .include_checksum(true)
                    .map_err(|error| Error::from_host(&error))?;
                EncoderInner::Zstd(encoder)
            }
        };
        Ok(Self { inner })
    }

    pub fn finish(self) -> Result<W> {
        let finished = match self.inner {
            EncoderInner::Plain(mut sink) => sink.flush().map(|()| sink),
            EncoderInner::Gzip(encoder) => encoder.finish(),
            EncoderInner::Bzip2(encoder) => encoder.finish(),
            EncoderInner::Xz(encoder) => encoder.finish(),
            EncoderInner::Lzma(encoder) => encoder.finish(),
            EncoderInner::Zstd(encoder) => encoder.finish(),
        };
        finished.map_err(|error| Error::from_host(&error).with_context("finishing compression"))
    }
}

impl<W: Write> Write for Encoder<W> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        match &mut self.inner {
            EncoderInner::Plain(writer) => writer.write(buffer),
            EncoderInner::Gzip(writer) => writer.write(buffer),
            EncoderInner::Bzip2(writer) => writer.write(buffer),
            EncoderInner::Xz(writer) => writer.write(buffer),
            EncoderInner::Lzma(writer) => writer.write(buffer),
            EncoderInner::Zstd(writer) => writer.write(buffer),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match &mut self.inner {
            EncoderInner::Plain(writer) => writer.flush(),
            EncoderInner::Gzip(writer) => writer.flush(),
            EncoderInner::Bzip2(writer) => writer.flush(),
            EncoderInner::Xz(writer) => writer.flush(),
            EncoderInner::Lzma(writer) => writer.flush(),
            EncoderInner::Zstd(writer) => writer.flush(),
        }
    }
}

pub fn decoder<R: BufRead + Send + 'static>(
    codec: Codec,
    reader: R,
    workspace_bytes: u64,
) -> Result<Box<dyn Read + Send>> {
    let host = |error: io::Error| Error::from_read(&error);
    let decoder: Box<dyn Read + Send> = match codec {
        Codec::Gzip => Box::new(MultiGzDecoder::new(reader)),
        Codec::Bzip2 => Box::new(MultiBzDecoder::new(reader)),
        Codec::Xz => Box::new(XzReader::new(reader, true)),
        Codec::Lzma => {
            let limit = u32::try_from(workspace_bytes / 1024).unwrap_or(u32::MAX);
            Box::new(LzmaReader::new_mem_limit(reader, limit, None).map_err(host)?)
        }
        Codec::Zstd => {
            let mut decoder = zstd::stream::read::Decoder::with_buffer(reader).map_err(host)?;
            let window_log = (u64::BITS - 1 - workspace_bytes.max(1).leading_zeros()).min(27);
            decoder.window_log_max(window_log).map_err(host)?;
            Box::new(decoder)
        }
    };
    Ok(decoder)
}

pub fn xz_dictionary_bytes(header: &[u8]) -> Option<u64> {
    const STREAM_HEADER: usize = 12;
    let block = header.get(STREAM_HEADER..)?;
    let size = (usize::from(*block.first()?) + 1) * 4;
    if block.len() < size {
        return None;
    }
    let flags = block[1];
    let mut cursor = 2;
    if flags & 0x40 != 0 {
        cursor = skip_varint(block, cursor)?;
    }
    if flags & 0x80 != 0 {
        cursor = skip_varint(block, cursor)?;
    }
    let filters = usize::from(flags & 3) + 1;
    for _ in 0..filters {
        let (id, next) = read_varint(block, cursor)?;
        let (properties, next) = read_varint(block, next)?;
        let end = next + usize::try_from(properties).ok()?;
        if id == 0x21 && properties == 1 {
            let byte = *block.get(next)?;
            if byte > 40 {
                return None;
            }
            return Some(if byte == 40 {
                u64::from(u32::MAX)
            } else {
                (2 | u64::from(byte & 1)) << (byte / 2 + 11)
            });
        }
        cursor = end;
    }
    None
}

fn read_varint(data: &[u8], mut cursor: usize) -> Option<(u64, usize)> {
    let mut value = 0_u64;
    for shift in (0..63).step_by(7) {
        let byte = *data.get(cursor)?;
        cursor += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some((value, cursor));
        }
    }
    None
}

fn skip_varint(data: &[u8], cursor: usize) -> Option<usize> {
    read_varint(data, cursor).map(|(_, next)| next)
}
