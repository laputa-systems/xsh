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
        .map_err(|error| RuntimeError::host("hash-read", &error).with_span(span))?;
    digest_reader(algorithm, &mut file, span)
}

pub(crate) fn blake2b_file(
    path: &Path,
    output_length: i64,
    span: Span,
) -> Result<DigestValue, RuntimeError> {
    let mut file = open_hash_file(path, span)?;
    blake2b_reader(&mut file, output_length, span)
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

pub(crate) fn blake2b_bytes(
    bytes: &[u8],
    output_length: i64,
    span: Span,
) -> Result<DigestValue, RuntimeError> {
    let output_length = checked_blake2b_length(output_length, span)?;
    let mut state = Blake2b::new(output_length);
    state.update(bytes);
    Ok(DigestValue {
        algorithm: "blake2b".to_string(),
        bytes: state.finalize(),
    })
}

pub(crate) fn blake2b_reader(
    reader: &mut dyn Read,
    output_length: i64,
    span: Span,
) -> Result<DigestValue, RuntimeError> {
    let output_length = checked_blake2b_length(output_length, span)?;
    let mut state = Blake2b::new(output_length);
    read_stream(reader, span, |bytes| state.update(bytes))?;
    Ok(DigestValue {
        algorithm: "blake2b".to_string(),
        bytes: state.finalize(),
    })
}

pub(crate) fn cksum_bytes(bytes: &[u8]) -> (u32, u64) {
    let mut state = Cksum::default();
    state.update(bytes);
    state.finish()
}

pub(crate) fn cksum_reader(
    reader: &mut dyn Read,
    span: Span,
) -> Result<(u32, u64), RuntimeError> {
    let mut state = Cksum::default();
    read_stream(reader, span, |bytes| state.update(bytes))?;
    Ok(state.finish())
}

pub(crate) fn cksum_file(path: &Path, span: Span) -> Result<(u32, u64), RuntimeError> {
    let mut file = open_hash_file(path, span)?;
    cksum_reader(&mut file, span)
}

pub(crate) fn bsd_sum_bytes(bytes: &[u8]) -> (u16, u64) {
    let mut state = Sum::new(SumKind::Bsd);
    state.update(bytes);
    state.finish()
}

pub(crate) fn bsd_sum_reader(
    reader: &mut dyn Read,
    span: Span,
) -> Result<(u16, u64), RuntimeError> {
    let mut state = Sum::new(SumKind::Bsd);
    read_stream(reader, span, |bytes| state.update(bytes))?;
    Ok(state.finish())
}

pub(crate) fn bsd_sum_file(path: &Path, span: Span) -> Result<(u16, u64), RuntimeError> {
    let mut file = open_hash_file(path, span)?;
    bsd_sum_reader(&mut file, span)
}

pub(crate) fn sysv_sum_bytes(bytes: &[u8]) -> (u16, u64) {
    let mut state = Sum::new(SumKind::Sysv);
    state.update(bytes);
    state.finish()
}

pub(crate) fn sysv_sum_reader(
    reader: &mut dyn Read,
    span: Span,
) -> Result<(u16, u64), RuntimeError> {
    let mut state = Sum::new(SumKind::Sysv);
    read_stream(reader, span, |bytes| state.update(bytes))?;
    Ok(state.finish())
}

pub(crate) fn sysv_sum_file(path: &Path, span: Span) -> Result<(u16, u64), RuntimeError> {
    let mut file = open_hash_file(path, span)?;
    sysv_sum_reader(&mut file, span)
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

fn open_hash_file(path: &Path, span: Span) -> Result<std::fs::File, RuntimeError> {
    std::fs::File::open(path).map_err(|error| RuntimeError::host("hash-read", &error).with_span(span))
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

fn read_stream(
    reader: &mut dyn Read,
    span: Span,
    mut update: impl FnMut(&[u8]),
) -> Result<(), RuntimeError> {
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = match reader.read(&mut buffer) {
            Ok(count) => count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(RuntimeError::host("hash-read", &error).with_span(span)),
        };
        if count == 0 {
            return Ok(());
        }
        update(&buffer[..count]);
    }
}

fn checked_blake2b_length(output_length: i64, span: Span) -> Result<usize, RuntimeError> {
    if !(1..=64).contains(&output_length) {
        return Err(RuntimeError::new(
            "hash-blake2b",
            "output length must be between 1 and 64 bytes",
        )
        .with_span(span));
    }
    Ok(output_length as usize)
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

#[derive(Default)]
struct Cksum {
    crc: u32,
    length: u64,
}

impl Cksum {
    fn update(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.crc = cksum_update_byte(self.crc, *byte);
        }
        self.length = self.length.saturating_add(bytes.len() as u64);
    }

    fn finish(mut self) -> (u32, u64) {
        let mut length = self.length;
        while length != 0 {
            self.crc = cksum_update_byte(self.crc, length as u8);
            length >>= 8;
        }
        (!self.crc, self.length)
    }
}

fn cksum_update_byte(mut crc: u32, byte: u8) -> u32 {
    crc ^= u32::from(byte) << 24;
    for _ in 0..8 {
        let mask = 0_u32.wrapping_sub(crc >> 31);
        crc = (crc << 1) ^ (0x04c1_1db7 & mask);
    }
    crc
}

#[derive(Clone, Copy)]
enum SumKind {
    Bsd,
    Sysv,
}

struct Sum {
    kind: SumKind,
    value: u32,
    length: u64,
}

impl Sum {
    fn new(kind: SumKind) -> Self {
        Self {
            kind,
            value: 0,
            length: 0,
        }
    }

    fn update(&mut self, bytes: &[u8]) {
        for byte in bytes {
            match self.kind {
                SumKind::Bsd => {
                    self.value = (self.value >> 1) | ((self.value & 1) << 15);
                    self.value = self.value.wrapping_add(u32::from(*byte)) & 0xffff;
                }
                SumKind::Sysv => {
                    self.value += u32::from(*byte);
                    self.value = (self.value & 0xffff) + (self.value >> 16);
                }
            }
        }
        self.length = self.length.saturating_add(bytes.len() as u64);
    }

    fn finish(mut self) -> (u16, u64) {
        if matches!(self.kind, SumKind::Sysv) {
            self.value = (self.value & 0xffff) + (self.value >> 16);
        }
        let blocks = self.length / 1024 + u64::from(self.length % 1024 != 0);
        (self.value as u16, blocks)
    }
}

const BLAKE2B_IV: [u64; 8] = [
    0x6a09_e667_f3bc_c908,
    0xbb67_ae85_84ca_a73b,
    0x3c6e_f372_fe94_f82b,
    0xa54f_f53a_5f1d_36f1,
    0x510e_527f_ade6_82d1,
    0x9b05_688c_2b3e_6c1f,
    0x1f83_d9ab_fb41_bd6b,
    0x5be0_cd19_137e_2179,
];

const BLAKE2B_SIGMA: [[usize; 16]; 12] = [
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
    [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4],
    [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
    [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13],
    [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
    [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11],
    [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
    [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5],
    [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
];

struct Blake2b {
    h: [u64; 8],
    buffer: [u8; 128],
    buffered: usize,
    count: [u64; 2],
    output_length: usize,
}

impl Blake2b {
    fn new(output_length: usize) -> Self {
        let mut h = BLAKE2B_IV;
        h[0] ^= 0x0101_0000 ^ output_length as u64;
        Self {
            h,
            buffer: [0; 128],
            buffered: 0,
            count: [0; 2],
            output_length,
        }
    }

    fn update(&mut self, mut input: &[u8]) {
        if self.buffered != 0 {
            let needed = self.buffer.len() - self.buffered;
            if input.len() <= needed {
                self.buffer[self.buffered..self.buffered + input.len()].copy_from_slice(input);
                self.buffered += input.len();
                return;
            }
            self.buffer[self.buffered..].copy_from_slice(&input[..needed]);
            self.add_count(128);
            compress_blake2b(&mut self.h, &self.buffer, self.count, false);
            self.buffered = 0;
            input = &input[needed..];
        }

        while input.len() > self.buffer.len() {
            let block: &[u8; 128] = input[..128].try_into().expect("full BLAKE2b block");
            self.add_count(128);
            compress_blake2b(&mut self.h, block, self.count, false);
            input = &input[128..];
        }
        self.buffer[..input.len()].copy_from_slice(input);
        self.buffered = input.len();
    }

    fn finalize(mut self) -> Vec<u8> {
        self.add_count(self.buffered as u64);
        self.buffer[self.buffered..].fill(0);
        compress_blake2b(&mut self.h, &self.buffer, self.count, true);
        let mut output = Vec::with_capacity(64);
        for word in self.h {
            output.extend_from_slice(&word.to_le_bytes());
        }
        output.truncate(self.output_length);
        output
    }

    fn add_count(&mut self, amount: u64) {
        let (low, carry) = self.count[0].overflowing_add(amount);
        self.count[0] = low;
        self.count[1] = self.count[1].wrapping_add(u64::from(carry));
    }
}

fn compress_blake2b(h: &mut [u64; 8], block: &[u8; 128], count: [u64; 2], last: bool) {
    let mut message = [0_u64; 16];
    for (word, bytes) in message.iter_mut().zip(block.chunks_exact(8)) {
        *word = u64::from_le_bytes(bytes.try_into().expect("eight-byte word"));
    }

    let mut v = [0_u64; 16];
    v[..8].copy_from_slice(h);
    v[8..].copy_from_slice(&BLAKE2B_IV);
    v[12] ^= count[0];
    v[13] ^= count[1];
    if last {
        v[14] = !v[14];
    }

    for sigma in BLAKE2B_SIGMA {
        blake2b_mix(&mut v, 0, 4, 8, 12, message[sigma[0]], message[sigma[1]]);
        blake2b_mix(&mut v, 1, 5, 9, 13, message[sigma[2]], message[sigma[3]]);
        blake2b_mix(&mut v, 2, 6, 10, 14, message[sigma[4]], message[sigma[5]]);
        blake2b_mix(&mut v, 3, 7, 11, 15, message[sigma[6]], message[sigma[7]]);
        blake2b_mix(&mut v, 0, 5, 10, 15, message[sigma[8]], message[sigma[9]]);
        blake2b_mix(&mut v, 1, 6, 11, 12, message[sigma[10]], message[sigma[11]]);
        blake2b_mix(&mut v, 2, 7, 8, 13, message[sigma[12]], message[sigma[13]]);
        blake2b_mix(&mut v, 3, 4, 9, 14, message[sigma[14]], message[sigma[15]]);
    }

    for index in 0..8 {
        h[index] ^= v[index] ^ v[index + 8];
    }
}

#[allow(clippy::many_single_char_names)]
fn blake2b_mix(
    v: &mut [u64; 16],
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    x: u64,
    y: u64,
) {
    v[a] = v[a].wrapping_add(v[b]).wrapping_add(x);
    v[d] = (v[d] ^ v[a]).rotate_right(32);
    v[c] = v[c].wrapping_add(v[d]);
    v[b] = (v[b] ^ v[c]).rotate_right(24);
    v[a] = v[a].wrapping_add(v[b]).wrapping_add(y);
    v[d] = (v[d] ^ v[a]).rotate_right(16);
    v[c] = v[c].wrapping_add(v[d]);
    v[b] = (v[b] ^ v[c]).rotate_right(63);
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
    use super::{
        HashAlgorithm, bsd_sum_bytes, bsd_sum_reader, blake2b_bytes, blake2b_reader,
        cksum_bytes, cksum_reader, digest_bytes, digest_reader, sysv_sum_bytes,
        sysv_sum_reader,
    };
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

    #[test]
    fn file_digest_retries_interrupted_reads() {
        let data = b"digest after retry".to_vec();
        let span = Span::new(SourceId::new(0), 0, 0);
        let mut reader = InterruptOnce(Cursor::new(data.clone()), false);
        let digest = digest_reader(HashAlgorithm::Sha256, &mut reader, span)
            .expect("interrupted read retries");
        assert_eq!(digest, digest_bytes(HashAlgorithm::Sha256, &data));
    }

    #[test]
    fn blake2b_matches_known_variable_length_vectors() {
        let span = Span::new(SourceId::new(0), 0, 0);
        let empty = blake2b_bytes(b"", 64, span).expect("64-byte digest");
        assert_eq!(
            super::hex(&empty.bytes),
            "786a02f742015903c6c6fd852552d272912f4740e15847618a86e217f71f5419d25e1031afee585313896444934eb04b903a685b1448b755d56f701afe9be2ce"
        );
        assert_eq!(
            super::hex(&blake2b_bytes(b"abc", 1, span).expect("one-byte digest").bytes),
            "6b"
        );
        assert_eq!(
            super::hex(&blake2b_bytes(b"abc", 32, span).expect("32-byte digest").bytes),
            "bddd813c634239723171ef3fee98579b94964e3bb1cb3e427262c8c068d52319"
        );
        assert_eq!(
            blake2b_bytes(b"abc", 0, span)
                .expect_err("zero output size is invalid")
                .kind,
            "hash-blake2b"
        );
    }

    #[test]
    fn blake2b_streaming_handles_multiple_blocks_and_short_reads() {
        let data = (0..(256 * 1024 + 17))
            .map(|index| (index * 31) as u8)
            .collect::<Vec<_>>();
        let span = Span::new(SourceId::new(0), 0, 0);
        let mut reader = BoundedReader(Cursor::new(data.clone()));

        let streamed = blake2b_reader(&mut reader, 47, span).expect("stream BLAKE2b");
        let in_memory = blake2b_bytes(&data, 47, span).expect("BLAKE2b byte slice");

        assert_eq!(streamed, in_memory);
        assert_eq!(
            super::hex(&streamed.bytes),
            "c982dcc55cbe49dff7377fb845430908d86170615d5989b73ea5c6f693a99a7e6f0ec196893e53c563db85d9033e0a"
        );
    }

    #[test]
    fn cksum_matches_gnu_crc_and_includes_the_input_length() {
        assert_eq!(cksum_bytes(b"abc"), (1_219_131_554, 3));
        assert_eq!(cksum_bytes(b""), (u32::MAX, 0));

        let data = b"abc".to_vec();
        let span = Span::new(SourceId::new(0), 0, 0);
        let mut reader = InterruptOnce(Cursor::new(data), false);
        assert_eq!(
            cksum_reader(&mut reader, span).expect("stream cksum"),
            (1_219_131_554, 3)
        );
    }

    #[test]
    fn bsd_and_sysv_sum_match_gnu_and_count_1024_byte_blocks() {
        assert_eq!(bsd_sum_bytes(b"abc"), (16_556, 1));
        assert_eq!(sysv_sum_bytes(b"abc"), (294, 1));
        assert_eq!(bsd_sum_bytes(&vec![0; 1024]).1, 1);
        assert_eq!(sysv_sum_bytes(&vec![0; 1025]).1, 2);

        let repeated = (0..=255).cycle().take(2560).collect::<Vec<u8>>();
        assert_eq!(bsd_sum_bytes(&repeated), (5120, 3));
        assert_eq!(sysv_sum_bytes(&repeated), (64_260, 3));
        assert_eq!(bsd_sum_bytes(&vec![u8::MAX; 2049]), (33_532, 3));
        assert_eq!(sysv_sum_bytes(&vec![u8::MAX; 2049]), (63_750, 3));

        let data = b"abc".to_vec();
        let span = Span::new(SourceId::new(0), 0, 0);
        let mut bsd_reader = InterruptOnce(Cursor::new(data.clone()), false);
        let mut sysv_reader = InterruptOnce(Cursor::new(data), false);
        assert_eq!(
            bsd_sum_reader(&mut bsd_reader, span).expect("stream BSD sum"),
            (16_556, 1)
        );
        assert_eq!(
            sysv_sum_reader(&mut sysv_reader, span).expect("stream SysV sum"),
            (294, 1)
        );
    }
}
