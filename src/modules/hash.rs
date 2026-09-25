#![allow(clippy::single_call_fn)]

use crate::modules::bytes::base64_encode;
use crate::runtime::value::{DigestValue, RuntimeError};
use crate::source::Span;
use md5::Digest as _;
use std::io::Read;
use std::path::Path;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HashAlgorithm {
    Md5,
    Sha1,
    Sha256,
    Sha512,
}

impl HashAlgorithm {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Md5 => "md5",
            Self::Sha1 => "sha1",
            Self::Sha256 => "sha256",
            Self::Sha512 => "sha512",
        }
    }
}

pub(crate) fn digest_bytes(algorithm: HashAlgorithm, bytes: &[u8]) -> DigestValue {
    match algorithm {
        HashAlgorithm::Md5 => {
            let mut digest = md5::Md5::new();
            digest.update(bytes);
            digest_value(algorithm, digest.finalize().as_slice())
        }
        HashAlgorithm::Sha1 => {
            let mut digest = sha1::Sha1::new();
            digest.update(bytes);
            digest_value(algorithm, digest.finalize().as_slice())
        }
        HashAlgorithm::Sha256 => {
            let mut digest = sha2::Sha256::new();
            digest.update(bytes);
            digest_value(algorithm, digest.finalize().as_slice())
        }
        HashAlgorithm::Sha512 => {
            let mut digest = sha2::Sha512::new();
            digest.update(bytes);
            digest_value(algorithm, digest.finalize().as_slice())
        }
    }
}

pub(crate) fn digest_file(
    algorithm: HashAlgorithm,
    path: &Path,
    span: Span,
) -> Result<DigestValue, RuntimeError> {
    let mut file = std::fs::File::open(path)
        .map_err(|error| RuntimeError::new("hash-read", error.to_string()).with_span(span))?;
    digest_reader(algorithm, &mut file, span)
}

pub(crate) fn digest_hex(digest: &DigestValue) -> String {
    hex(&digest.bytes)
}

pub(crate) fn digest_base64(digest: &DigestValue) -> String {
    base64_encode(&digest.bytes)
}

pub(crate) fn crc32(bytes: &[u8]) -> i64 {
    crc32_with_polynomial(bytes, 0xedb8_8320) as i64
}

pub(crate) fn crc32c(bytes: &[u8]) -> i64 {
    crc32_with_polynomial(bytes, 0x82f6_3b78) as i64
}

// GNU checksum lines prefer a double-space separator anywhere in the line to
// a space-star pair. The separator's second byte sets `binary`; a leading
// star in the path is stripped independently, and every trailing CR is ignored.
pub(crate) fn parse_check_line(line: &str, span: Span) -> Result<CheckLine, RuntimeError> {
    let line = line.trim_end_matches('\r');
    let separator = line.find("  ").or_else(|| line.find(" *")).ok_or_else(|| {
        RuntimeError::new(
            "checksum-line",
            "expected `<hex>  <path>` or `<hex> *<path>`",
        )
        .with_span(span)
    })?;
    let hex = &line[..separator];
    let marker = line.as_bytes().get(separator + 1).copied().unwrap_or(b' ');
    let path = &line[separator + 2..];
    let path = path.strip_prefix('*').unwrap_or(path);
    if hex.is_empty() || path.is_empty() {
        return Err(
            RuntimeError::new("checksum-line", "checksum line is incomplete").with_span(span),
        );
    }
    if !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(
            RuntimeError::new("checksum-line", "checksum is not hexadecimal").with_span(span),
        );
    }
    Ok(CheckLine {
        hex: hex.to_ascii_lowercase(),
        path: path.to_string(),
        binary: marker == b'*',
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckLine {
    pub(crate) hex: String,
    pub(crate) path: String,
    pub(crate) binary: bool,
}

fn digest_reader(
    algorithm: HashAlgorithm,
    reader: &mut dyn Read,
    span: Span,
) -> Result<DigestValue, RuntimeError> {
    let bytes = match algorithm {
        HashAlgorithm::Md5 => digest_stream::<md5::Md5>(reader, span)?,
        HashAlgorithm::Sha1 => digest_stream::<sha1::Sha1>(reader, span)?,
        HashAlgorithm::Sha256 => digest_stream::<sha2::Sha256>(reader, span)?,
        HashAlgorithm::Sha512 => digest_stream::<sha2::Sha512>(reader, span)?,
    };
    Ok(digest_value(algorithm, &bytes))
}

fn digest_stream<D: md5::Digest + Default>(
    reader: &mut dyn Read,
    span: Span,
) -> Result<Vec<u8>, RuntimeError> {
    let mut digest = D::default();
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|error| RuntimeError::new("hash-read", error.to_string()).with_span(span))?;
        if count == 0 {
            return Ok(digest.finalize().to_vec());
        }
        digest.update(&buffer[..count]);
    }
}

fn digest_value(algorithm: HashAlgorithm, bytes: &[u8]) -> DigestValue {
    DigestValue {
        algorithm: algorithm.name().to_string(),
        bytes: bytes.to_vec(),
    }
}

fn crc32_with_polynomial(bytes: &[u8], polynomial: u32) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0_u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (polynomial & mask);
        }
    }
    !crc
}

fn hex(bytes: &[u8]) -> String {
    const TABLE: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(TABLE[(byte >> 4) as usize] as char);
        output.push(TABLE[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{digest_bytes, digest_reader, HashAlgorithm};
    use crate::source::{SourceId, Span};
    use std::io::{self, Cursor, Read};

    struct BoundedReader(Cursor<Vec<u8>>);

    impl Read for BoundedReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if buffer.len() > 64 * 1024 {
                return Err(io::Error::other("digest requested an unbounded read"));
            }
            self.0.read(buffer)
        }
    }

    #[test]
    fn file_digests_read_in_bounded_chunks_with_byte_parity() {
        let data = (0..(256 * 1024 + 17)).map(|index| index as u8).collect::<Vec<_>>();
        let span = Span::new(SourceId::new(0), 0, 0);
        for algorithm in [
            HashAlgorithm::Md5,
            HashAlgorithm::Sha1,
            HashAlgorithm::Sha256,
            HashAlgorithm::Sha512,
        ] {
            let mut reader = BoundedReader(Cursor::new(data.clone()));
            let file = digest_reader(algorithm, &mut reader, span).expect("bounded file read");
            assert_eq!(file, digest_bytes(algorithm, &data));
        }
    }
}
