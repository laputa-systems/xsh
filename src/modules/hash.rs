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

fn hex(bytes: &[u8]) -> String {
    const TABLE: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(TABLE[(byte >> 4) as usize] as char);
        output.push(TABLE[(byte & 0x0f) as usize] as char);
    }
    output
}
