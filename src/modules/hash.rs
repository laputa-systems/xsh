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

pub(crate) fn verify_hex(
    actual: &DigestValue,
    expected: &str,
    span: Span,
) -> Result<(), RuntimeError> {
    validate_expected_hex(
        actual.algorithm.as_str(),
        expected,
        actual.bytes.len(),
        span,
    )?;
    let actual_hex = digest_hex(actual);
    if actual_hex.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(RuntimeError::new(
            "checksum-mismatch",
            format!(
                "{} digest mismatch: expected {}, got {}",
                actual.algorithm, expected, actual_hex
            ),
        )
        .with_span(span))
    }
}

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
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|error| RuntimeError::new("hash-read", error.to_string()).with_span(span))?;
    Ok(digest_bytes(algorithm, &bytes))
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

fn validate_expected_hex(
    algorithm: &str,
    expected: &str,
    digest_len: usize,
    span: Span,
) -> Result<(), RuntimeError> {
    if expected.len() != digest_len * 2 {
        return Err(RuntimeError::new(
            "checksum-format",
            format!(
                "{} checksum must be {} hex characters",
                algorithm,
                digest_len * 2
            ),
        )
        .with_span(span));
    }
    if !expected.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(
            RuntimeError::new("checksum-format", "checksum must be hexadecimal").with_span(span),
        );
    }
    Ok(())
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
