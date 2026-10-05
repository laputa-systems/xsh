#![allow(clippy::single_call_fn)]

use crate::modules::bytes::base64_encode;
use crate::runtime::value::{DigestValue, RuntimeError, Value};
use crate::source::Span;
use md5::Digest as _;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;

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
) -> Result<DigestValue, RuntimeError, Value> {
    let mut file = std::fs::File::open(path)
        .map_err(|error| RuntimeError::host("hash-read", &error).with_span(span))?;
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
) -> Result<DigestValue, RuntimeError, Value> {
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
        let count = match reader.read(&mut buffer) {
            Ok(count) => count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => {
                return Err(RuntimeError::host("hash-read", &error).with_span(span));
            }
        };
        if count == 0 {
            return Ok(digest.finalize().to_vec());
        }
        digest.update(&buffer[..count]);
    }
}

pub(crate) fn named_digest_file(
    algorithm: &str, length: i64, path: &Path, span: Span,
) -> Result<DigestValue, RuntimeError> {
    let mut file = std::fs::File::open(path)
        .map_err(|error| RuntimeError::host("hash-read", &error).with_span(span))?;
    named_digest_reader(algorithm, length, &mut file, span)
}

pub(crate) fn named_digest_reader(
    algorithm: &str, length: i64, reader: &mut dyn Read, span: Span,
) -> Result<DigestValue, RuntimeError> {
    if !matches!(algorithm, "md5" | "sha1" | "sha224" | "sha256" | "sha384" | "sha512" | "blake2b") {
        return Err(RuntimeError::new("hash-algorithm", "unsupported digest algorithm").with_span(span));
    }
    let (bytes, _, _) = hash_reader(algorithm, length, reader, span)?;
    Ok(DigestValue { algorithm: algorithm.to_string(), bytes })
}

pub(crate) fn checksum_file(
    algorithm: &str, path: &Path, span: Span,
) -> Result<Value, RuntimeError> {
    let mut file = std::fs::File::open(path)
        .map_err(|error| RuntimeError::host("hash-read", &error).with_span(span))?;
    checksum_reader(algorithm, &mut file, span)
}

pub(crate) fn checksum_reader(
    algorithm: &str, reader: &mut dyn Read, span: Span,
) -> Result<Value, RuntimeError> {
    if !matches!(algorithm, "crc" | "bsd" | "sysv") {
        return Err(RuntimeError::new("hash-algorithm", "unsupported numeric checksum algorithm").with_span(span));
    }
    let (_, checksum, size) = hash_reader(algorithm, 512, reader, span)?;
    Ok(Value::Record(crate::runtime::value::RecordMap::from([
        (Arc::from("checksum"), Value::Int(i64::from(checksum))),
        (Arc::from("size"), Value::Int(size)),
    ])))
}

// The counter counts content bytes, while the final CRC also incorporates the
// little-endian byte representation of the content length required by POSIX.
fn hash_reader(
    algorithm: &str, length: i64, reader: &mut dyn Read, span: Span,
) -> Result<(Vec<u8>, u32, i64), RuntimeError> {
    let mut digest: Option<Box<dyn md5::digest::DynDigest>> = match algorithm {
        "md5" => Some(Box::new(md5::Md5::new())),
        "sha1" => Some(Box::new(sha1::Sha1::new())),
        "sha224" => Some(Box::new(sha2::Sha224::new())),
        "sha256" => Some(Box::new(sha2::Sha256::new())),
        "sha384" => Some(Box::new(sha2::Sha384::new())),
        "sha512" => Some(Box::new(sha2::Sha512::new())),
        "blake2b" | "crc" | "bsd" | "sysv" => None,
        _ => return Err(RuntimeError::new("hash-algorithm", "unsupported checksum algorithm").with_span(span)),
    };
    if algorithm == "blake2b" && (!(8..=512).contains(&length) || length % 8 != 0) {
        return Err(RuntimeError::new("hash-length", "BLAKE2b length must be a multiple of 8 between 8 and 512 bits").with_span(span));
    }
    let mut blake = Blake2b::new(if algorithm == "blake2b" { length as usize / 8 } else { 64 });
    let mut size = 0_i64;
    let mut checksum = 0_u32;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = match reader.read(&mut buffer) {
            Ok(count) => count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(RuntimeError::host("hash-read", &error).with_span(span)),
        };
        if count == 0 { break; }
        size = size.checked_add(count as i64).ok_or_else(|| RuntimeError::new("hash-size", "input length exceeds Int range").with_span(span))?;
        let chunk = &buffer[..count];
        if let Some(digest) = digest.as_mut() { digest.update(chunk); }
        match algorithm {
            "blake2b" => blake.update(chunk),
            "crc" => for byte in chunk { checksum = posix_crc_byte(checksum, *byte); },
            "bsd" => for byte in chunk { checksum = ((checksum >> 1) | ((checksum & 1) << 15)).wrapping_add(u32::from(*byte)) & 0xffff; },
            "sysv" => for byte in chunk { checksum = checksum.wrapping_add(u32::from(*byte)); },
            _ => {},
        }
    }
    if algorithm == "crc" {
        let mut count = size as u64;
        while count != 0 { checksum = posix_crc_byte(checksum, count as u8); count >>= 8; }
        checksum = !checksum;
    } else if algorithm == "sysv" {
        checksum = (checksum & 0xffff) + (checksum >> 16);
        checksum = (checksum & 0xffff) + (checksum >> 16);
    }
    let bytes = if let Some(digest) = digest { digest.finalize().to_vec() }
        else if algorithm == "blake2b" { blake.finish() }
        else { Vec::new() };
    Ok((bytes, checksum, size))
}

fn posix_crc_byte(mut crc: u32, byte: u8) -> u32 {
    crc ^= u32::from(byte) << 24;
    for _ in 0..8 {
        let high = crc >> 31;
        crc = (crc << 1) ^ (0x04c1_1db7 & 0_u32.wrapping_sub(high));
    }
    crc
}

const BLAKE_IV: [u64; 8] = [
    0x6a09e667f3bcc908, 0xbb67ae8584caa73b, 0x3c6ef372fe94f82b, 0xa54ff53a5f1d36f1,
    0x510e527fade682d1, 0x9b05688c2b3e6c1f, 0x1f83d9abfb41bd6b, 0x5be0cd19137e2179,
];
const BLAKE_SIGMA: [[usize; 16]; 10] = [
    [0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15],
    [14,10,4,8,9,15,13,6,1,12,0,2,11,7,5,3],
    [11,8,12,0,5,2,15,13,10,14,3,6,7,1,9,4],
    [7,9,3,1,13,12,11,14,2,6,5,10,4,0,15,8],
    [9,0,5,7,2,4,10,15,14,1,11,12,6,8,3,13],
    [2,12,6,10,0,11,8,3,4,13,7,5,15,14,1,9],
    [12,5,1,15,14,13,4,10,0,7,6,3,9,2,8,11],
    [13,11,7,14,12,1,3,9,5,0,15,4,8,6,2,10],
    [6,15,14,9,11,3,0,8,12,2,13,7,1,4,10,5],
    [10,2,8,4,7,6,1,5,15,11,9,14,3,12,13,0],
];

// Sequential unkeyed BLAKE2b keeps the last block pending because its final
// flag changes compression even when the input ends at a block boundary.
struct Blake2b {
    state: [u64; 8], block: [u8; 128], used: usize, count: u128, length: usize,
}
impl Blake2b {
    fn new(length: usize) -> Self {
        let mut state = BLAKE_IV;
        state[0] ^= 0x0101_0000 ^ length as u64;
        Self { state, block: [0;128], used: 0, count: 0, length }
    }
    fn update(&mut self, mut bytes: &[u8]) {
        while !bytes.is_empty() {
            if self.used == 128 {
                self.count += 128;
                self.compress(false);
                self.used = 0;
            }
            let count = bytes.len().min(128 - self.used);
            self.block[self.used..self.used + count].copy_from_slice(&bytes[..count]);
            self.used += count;
            bytes = &bytes[count..];
        }
    }
    fn finish(mut self) -> Vec<u8> {
        self.count += self.used as u128;
        self.block[self.used..].fill(0);
        self.compress(true);
        let mut bytes = Vec::with_capacity(64);
        for word in self.state { bytes.extend_from_slice(&word.to_le_bytes()); }
        bytes.truncate(self.length);
        bytes
    }
    fn compress(&mut self, last: bool) {
        let mut words = [0_u64;16];
        for (word, bytes) in words.iter_mut().zip(self.block.chunks_exact(8)) {
            *word = u64::from_le_bytes(bytes.try_into().expect("eight byte word"));
        }
        let mut v = [0_u64;16];
        v[..8].copy_from_slice(&self.state);
        v[8..].copy_from_slice(&BLAKE_IV);
        v[12] ^= self.count as u64;
        v[13] ^= (self.count >> 64) as u64;
        if last { v[14] = !v[14]; }
        for round in 0..12 {
            let s = BLAKE_SIGMA[round % 10];
            for (a,b,c,d,x,y) in [
                (0,4,8,12,0,1), (1,5,9,13,2,3), (2,6,10,14,4,5), (3,7,11,15,6,7),
                (0,5,10,15,8,9), (1,6,11,12,10,11), (2,7,8,13,12,13), (3,4,9,14,14,15),
            ] {
                v[a] = v[a].wrapping_add(v[b]).wrapping_add(words[s[x]]);
                v[d] = (v[d] ^ v[a]).rotate_right(32);
                v[c] = v[c].wrapping_add(v[d]);
                v[b] = (v[b] ^ v[c]).rotate_right(24);
                v[a] = v[a].wrapping_add(v[b]).wrapping_add(words[s[y]]);
                v[d] = (v[d] ^ v[a]).rotate_right(16);
                v[c] = v[c].wrapping_add(v[d]);
                v[b] = (v[b] ^ v[c]).rotate_right(63);
            }
        }
        for i in 0..8 { self.state[i] ^= v[i] ^ v[i + 8]; }
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
    use super::{HashAlgorithm, digest_bytes, digest_reader};
    use crate::source::{SourceId, Span};
    use std::io::{self, Cursor, Read};

    struct BoundedReader(Cursor<Vec<u8>>);

    struct InterruptOnce(Cursor<Vec<u8>>, bool);

    impl Read for BoundedReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if buffer.len() > 64 * 1024 {
                return Err(io::Error::other("digest requested an unbounded read"));
            }
            self.0.read(buffer)
        }
    }

    impl Read for InterruptOnce {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if !self.1 {
                self.1 = true;
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "retry this read",
                ));
            }
            self.0.read(buffer)
        }
    }

    #[test]
    fn file_digests_read_in_bounded_chunks_with_byte_parity() {
        let data = (0..(256 * 1024 + 17))
            .map(|index| index as u8)
            .collect::<Vec<_>>();
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

    // A host reader can enforce buffer bounds and arbitrary read boundaries.
    #[test]
    fn named_digests_preserve_chunk_boundaries_and_bounded_reads() {
        let span = Span::new(SourceId::new(0), 0, 0);
        for size in [0, 1, 127, 128, 129, 65536, 65537, 262161] {
            let data = (0..size).map(|i| i as u8).collect::<Vec<_>>();
            for algorithm in ["md5", "sha1", "sha224", "sha256", "sha384", "sha512", "blake2b"] {
                let expected = super::named_digest_reader(algorithm, 512, &mut Cursor::new(data.clone()), span).expect("digest");
                let actual = super::named_digest_reader(algorithm, 512, &mut BoundedReader(Cursor::new(data.clone())), span).expect("bounded digest");
                assert_eq!(actual, expected);
                let actual = super::named_digest_reader(algorithm, 512, &mut InterruptOnce(Cursor::new(data.clone()), false), span).expect("retry digest");
                assert_eq!(actual, expected);
            }
        }
    }

    #[test]
    fn file_digest_retries_interrupted_reads() {
        let data = b"digest after retry".to_vec();
        let span = Span::new(SourceId::new(0), 0, 0);
        let mut reader = InterruptOnce(Cursor::new(data.clone()), false);
        let digest = digest_reader(HashAlgorithm::Sha256, &mut reader, span)
            .expect("interrupted read retries");
        assert_eq!(digest, digest_bytes(HashAlgorithm::Sha256, &data));
    }
}
