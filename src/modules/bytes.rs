#![allow(clippy::single_call_fn)]

use crate::runtime::value::{RuntimeError, Value};
use crate::source::Span;
use data_encoding::{BASE32, BASE32_NOPAD, BASE64, BASE64_NOPAD};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::Arc;

pub(crate) fn len(bytes: &[u8]) -> i64 {
    bytes.len() as i64
}

pub(crate) fn slice(
    bytes: Vec<u8>,
    offset: i64,
    length: Option<i64>,
    span: Span,
) -> Result<Vec<u8>, RuntimeError> {
    if offset < 0 {
        return Err(RuntimeError::new("bytes-slice", "offset cannot be negative").with_span(span));
    }
    let offset = offset as usize;
    if offset > bytes.len() {
        return Err(
            RuntimeError::new("bytes-slice", "offset is past end of byte data").with_span(span),
        );
    }
    let end = match length {
        Some(length) if length < 0 => {
            return Err(
                RuntimeError::new("bytes-slice", "length cannot be negative").with_span(span),
            );
        }
        Some(length) => offset.saturating_add(length as usize).min(bytes.len()),
        None => bytes.len(),
    };
    Ok(bytes[offset..end].to_vec())
}

pub(crate) fn chunks(bytes: Vec<u8>, size: i64, span: Span) -> Result<Vec<Value>, RuntimeError> {
    if size <= 0 {
        return Err(
            RuntimeError::new("bytes-chunks", "chunk size must be positive").with_span(span),
        );
    }
    Ok(bytes
        .chunks(size as usize)
        .map(|chunk| Value::Bytes(chunk.to_vec()))
        .collect())
}

pub(crate) fn compare_record(left: &[u8], right: &[u8]) -> Value {
    let mut line = 1_i64;
    let shared_len = left.len().min(right.len());
    for index in 0..shared_len {
        if left[index] != right[index] {
            return compare_fields(
                false,
                index as i64 + 1,
                line,
                i64::from(left[index]),
                i64::from(right[index]),
            );
        }
        if left[index] == b'\n' {
            line += 1;
        }
    }

    if left.len() == right.len() {
        return compare_fields(true, 0, 0, -1, -1);
    }

    compare_fields(
        false,
        shared_len as i64 + 1,
        line,
        left.get(shared_len).map_or(-1, |byte| i64::from(*byte)),
        right.get(shared_len).map_or(-1, |byte| i64::from(*byte)),
    )
}

pub(crate) fn dump(bytes: &[u8], format: &str, span: Span) -> Result<String, RuntimeError> {
    match format {
        "canonical" => Ok(canonical_dump(bytes)),
        "hex-u8" => Ok(radix_dump(bytes, 16)),
        "octal-u8" => Ok(radix_dump(bytes, 8)),
        _ => Err(RuntimeError::new(
            "bytes-dump",
            "dump format must be canonical, hex-u8, or octal-u8",
        )
        .with_span(span)),
    }
}

pub(crate) fn strings(bytes: &[u8], min_len: i64, span: Span) -> Result<Vec<Value>, RuntimeError> {
    if min_len <= 0 {
        return Err(RuntimeError::new("bytes-strings", "min_len must be positive").with_span(span));
    }
    let min_len = min_len as usize;
    let mut output = Vec::new();
    let mut current = Vec::new();
    for byte in bytes {
        if byte.is_ascii_graphic() || *byte == b' ' {
            current.push(*byte);
        } else {
            push_string_run(&mut output, &mut current, min_len);
        }
    }
    push_string_run(&mut output, &mut current, min_len);
    Ok(output)
}

pub(crate) fn zero(length: i64, span: Span) -> Result<Vec<u8>, RuntimeError> {
    if length < 0 {
        return Err(RuntimeError::new("bytes-zero", "length cannot be negative").with_span(span));
    }
    Ok(vec![0_u8; length as usize])
}

pub(crate) fn from_ints(values: Vec<i64>, span: Span) -> Result<Vec<u8>, RuntimeError> {
    let mut bytes = Vec::with_capacity(values.len());
    for value in values {
        if !(0..=255).contains(&value) {
            return Err(RuntimeError::new(
                "bytes-from-ints",
                "byte values must be between 0 and 255",
            )
            .with_span(span));
        }
        bytes.push(value as u8);
    }
    Ok(bytes)
}

pub(crate) fn from_text(text: &str) -> Vec<u8> {
    text.as_bytes().to_vec()
}

pub(crate) fn human(size: i64) -> String {
    if size < 0 {
        return "-".to_string();
    }

    let mut amount = size as f64;
    let mut unit_index = 0usize;
    let units = ["", "K", "M", "G", "T", "P", "E"];

    while amount >= 1024.0 && unit_index + 1 < units.len() {
        amount /= 1024.0;
        unit_index += 1;
    }

    let unit = units[unit_index];
    if unit.is_empty() {
        return format!("{amount:.0}");
    }

    if amount < 10.0 {
        return format!("{amount:.1}{unit}");
    }

    format!("{amount:.0}{unit}")
}

pub(crate) fn concat(chunks: Vec<Vec<u8>>) -> Vec<u8> {
    let len = chunks.iter().map(Vec::len).sum();
    let mut out = Vec::with_capacity(len);
    for chunk in chunks {
        out.extend_from_slice(&chunk);
    }
    out
}

pub(crate) fn pack_int_le(value: i64, width: i64, span: Span) -> Result<Vec<u8>, RuntimeError> {
    pack_int(value, width, true, span)
}

pub(crate) fn pack_int_be(value: i64, width: i64, span: Span) -> Result<Vec<u8>, RuntimeError> {
    pack_int(value, width, false, span)
}

pub(crate) fn unpack_int_le(
    bytes: &[u8],
    offset: i64,
    width: i64,
    span: Span,
) -> Result<i64, RuntimeError> {
    unpack_int(bytes, offset, width, true, span)
}

pub(crate) fn unpack_int_be(
    bytes: &[u8],
    offset: i64,
    width: i64,
    span: Span,
) -> Result<i64, RuntimeError> {
    unpack_int(bytes, offset, width, false, span)
}

pub(crate) fn read_at(
    path: PathBuf,
    offset: i64,
    length: i64,
    span: Span,
) -> Result<Vec<u8>, RuntimeError> {
    if offset < 0 {
        return Err(
            RuntimeError::new("bytes-read-at", "offset cannot be negative").with_span(span),
        );
    }
    if length < 0 {
        return Err(
            RuntimeError::new("bytes-read-at", "length cannot be negative").with_span(span),
        );
    }
    let mut file = std::fs::File::open(&path)
        .map_err(|error| RuntimeError::new("bytes-read-at", error.to_string()).with_span(span))?;
    file.seek(SeekFrom::Start(offset as u64))
        .map_err(|error| RuntimeError::new("bytes-read-at", error.to_string()).with_span(span))?;
    let mut data = vec![0_u8; length as usize];
    file.read_exact(&mut data)
        .map_err(|error| RuntimeError::new("bytes-read-at", error.to_string()).with_span(span))?;
    Ok(data)
}

pub(crate) fn write_at(
    path: PathBuf,
    offset: i64,
    data: &[u8],
    create: bool,
    span: Span,
) -> Result<i64, RuntimeError> {
    if offset < 0 {
        return Err(
            RuntimeError::new("bytes-write-at", "offset cannot be negative").with_span(span),
        );
    }
    if let Ok(metadata) = std::fs::symlink_metadata(&path)
        && metadata.file_type().is_symlink()
    {
        return Err(RuntimeError::new("bytes-write-at", "path is a symlink").with_span(span));
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(create)
        .open(&path)
        .map_err(|error| RuntimeError::new("bytes-write-at", error.to_string()).with_span(span))?;
    file.seek(SeekFrom::Start(offset as u64))
        .map_err(|error| RuntimeError::new("bytes-write-at", error.to_string()).with_span(span))?;
    file.write_all(data)
        .map_err(|error| RuntimeError::new("bytes-write-at", error.to_string()).with_span(span))?;
    Ok(data.len() as i64)
}

pub(crate) fn zero_at(
    path: PathBuf,
    offset: i64,
    length: i64,
    create: bool,
    span: Span,
) -> Result<i64, RuntimeError> {
    if length < 0 {
        return Err(
            RuntimeError::new("bytes-zero-at", "length cannot be negative").with_span(span),
        );
    }
    let data = vec![0_u8; length as usize];
    write_at(path, offset, &data, create, span)
        .map_err(|error| RuntimeError::new("bytes-zero-at", error.message).with_span(span))
}

fn pack_int(
    value: i64,
    width: i64,
    little_endian: bool,
    span: Span,
) -> Result<Vec<u8>, RuntimeError> {
    let width = checked_int_width(width, "bytes-pack", span)?;
    let max = if width == 8 {
        i64::MAX
    } else {
        (1_i64 << (width * 8)) - 1
    };
    if !(0..=max).contains(&value) {
        return Err(
            RuntimeError::new("bytes-pack", "value does not fit in requested width")
                .with_span(span),
        );
    }
    let bytes = if little_endian {
        (value as u64).to_le_bytes()
    } else {
        (value as u64).to_be_bytes()
    };
    if little_endian {
        Ok(bytes[..width].to_vec())
    } else {
        Ok(bytes[8 - width..].to_vec())
    }
}

fn unpack_int(
    bytes: &[u8],
    offset: i64,
    width: i64,
    little_endian: bool,
    span: Span,
) -> Result<i64, RuntimeError> {
    if offset < 0 {
        return Err(RuntimeError::new("bytes-unpack", "offset cannot be negative").with_span(span));
    }
    let width = checked_int_width(width, "bytes-unpack", span)?;
    let offset = offset as usize;
    let end = offset.saturating_add(width);
    if end > bytes.len() {
        return Err(RuntimeError::new(
            "bytes-unpack",
            "requested integer extends past end of byte data",
        )
        .with_span(span));
    }
    let mut raw = [0_u8; 8];
    if little_endian {
        raw[..width].copy_from_slice(&bytes[offset..end]);
        Ok(u64::from_le_bytes(raw) as i64)
    } else {
        raw[8 - width..].copy_from_slice(&bytes[offset..end]);
        Ok(u64::from_be_bytes(raw) as i64)
    }
}

fn checked_int_width(width: i64, kind: &str, span: Span) -> Result<usize, RuntimeError> {
    match width {
        1 | 2 | 4 | 8 => Ok(width as usize),
        _ => Err(RuntimeError::new(kind, "width must be 1, 2, 4, or 8").with_span(span)),
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn copy_blocks(
    source: PathBuf,
    dest: PathBuf,
    block_size: i64,
    count: Option<i64>,
    skip: i64,
    seek: i64,
    overwrite: bool,
    span: Span,
) -> Result<Value, RuntimeError> {
    if block_size <= 0 {
        return Err(RuntimeError::new("bytes-copy", "block_size must be positive").with_span(span));
    }
    if let Some(count) = count
        && count < 0
    {
        return Err(RuntimeError::new("bytes-copy", "count cannot be negative").with_span(span));
    }
    if skip < 0 {
        return Err(RuntimeError::new("bytes-copy", "skip cannot be negative").with_span(span));
    }
    if seek < 0 {
        return Err(RuntimeError::new("bytes-copy", "seek cannot be negative").with_span(span));
    }

    let source_metadata = std::fs::symlink_metadata(&source)
        .map_err(|error| RuntimeError::new("bytes-copy", error.to_string()).with_span(span))?;
    if !source_metadata.file_type().is_file() {
        return Err(
            RuntimeError::new("bytes-copy", "source is not a regular file").with_span(span),
        );
    }
    if let Ok(dest_metadata) = std::fs::symlink_metadata(&dest) {
        if dest_metadata.file_type().is_symlink() {
            return Err(RuntimeError::new("bytes-copy", "destination is a symlink").with_span(span));
        }
        if !overwrite {
            return Err(RuntimeError::new("bytes-copy", "destination exists").with_span(span));
        }
    }

    let mut input = std::fs::File::open(&source)
        .map_err(|error| RuntimeError::new("bytes-copy", error.to_string()).with_span(span))?;
    input
        .seek(SeekFrom::Start(
            (skip as u64).saturating_mul(block_size as u64),
        ))
        .map_err(|error| RuntimeError::new("bytes-copy", error.to_string()).with_span(span))?;

    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .create_new(!overwrite)
        .truncate(overwrite)
        .open(&dest)
        .map_err(|error| RuntimeError::new("bytes-copy", error.to_string()).with_span(span))?;
    output
        .seek(SeekFrom::Start(
            (seek as u64).saturating_mul(block_size as u64),
        ))
        .map_err(|error| RuntimeError::new("bytes-copy", error.to_string()).with_span(span))?;

    let mut buffer = vec![0_u8; block_size as usize];
    let mut copied = 0_i64;
    let mut blocks = 0_i64;
    loop {
        if count.is_some_and(|limit| blocks >= limit) {
            break;
        }
        let read = input
            .read(&mut buffer)
            .map_err(|error| RuntimeError::new("bytes-copy", error.to_string()).with_span(span))?;
        if read == 0 {
            break;
        }
        output
            .write_all(&buffer[..read])
            .map_err(|error| RuntimeError::new("bytes-copy", error.to_string()).with_span(span))?;
        copied += read as i64;
        blocks += 1;
        if read < buffer.len() {
            break;
        }
    }

    Ok(Value::Record(crate::runtime::value::RecordMap::from([
        (Arc::from("bytes"), Value::Int(copied)),
        (Arc::from("blocks"), Value::Int(blocks)),
    ])))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn copy_file(
    source: PathBuf,
    dest: PathBuf,
    source_offset: i64,
    dest_offset: i64,
    length: Option<i64>,
    create: bool,
    truncate: bool,
    span: Span,
) -> Result<Value, RuntimeError> {
    if source_offset < 0 {
        return Err(
            RuntimeError::new("bytes-copy-file", "source_offset cannot be negative")
                .with_span(span),
        );
    }
    if dest_offset < 0 {
        return Err(
            RuntimeError::new("bytes-copy-file", "dest_offset cannot be negative").with_span(span),
        );
    }
    if let Some(length) = length
        && length < 0
    {
        return Err(
            RuntimeError::new("bytes-copy-file", "length cannot be negative").with_span(span),
        );
    }

    let source_metadata = std::fs::symlink_metadata(&source)
        .map_err(|error| RuntimeError::new("bytes-copy-file", error.to_string()).with_span(span))?;
    if !source_metadata.file_type().is_file() {
        return Err(
            RuntimeError::new("bytes-copy-file", "source is not a regular file").with_span(span),
        );
    }
    if let Ok(dest_metadata) = std::fs::symlink_metadata(&dest)
        && dest_metadata.file_type().is_symlink()
    {
        return Err(
            RuntimeError::new("bytes-copy-file", "destination is a symlink").with_span(span),
        );
    }

    let mut input = std::fs::File::open(&source)
        .map_err(|error| RuntimeError::new("bytes-copy-file", error.to_string()).with_span(span))?;
    input
        .seek(SeekFrom::Start(source_offset as u64))
        .map_err(|error| RuntimeError::new("bytes-copy-file", error.to_string()).with_span(span))?;

    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create(create)
        .truncate(truncate)
        .open(&dest)
        .map_err(|error| RuntimeError::new("bytes-copy-file", error.to_string()).with_span(span))?;
    output
        .seek(SeekFrom::Start(dest_offset as u64))
        .map_err(|error| RuntimeError::new("bytes-copy-file", error.to_string()).with_span(span))?;

    let copied = match length {
        Some(length) => std::io::copy(&mut input.take(length as u64), &mut output),
        None => std::io::copy(&mut input, &mut output),
    }
    .map_err(|error| RuntimeError::new("bytes-copy-file", error.to_string()).with_span(span))?;

    Ok(Value::Record(crate::runtime::value::RecordMap::from([
        (Arc::from("bytes"), Value::Int(copied as i64)),
        (
            Arc::from("blocks"),
            Value::Int(((copied as i64).saturating_add(511)) / 512),
        ),
    ])))
}

fn compare_fields(equal: bool, byte: i64, line: i64, left: i64, right: i64) -> Value {
    Value::Record(crate::runtime::value::RecordMap::from([
        (Arc::from("equal"), Value::Bool(equal)),
        (Arc::from("byte"), Value::Int(byte)),
        (Arc::from("line"), Value::Int(line)),
        (Arc::from("left"), Value::Int(left)),
        (Arc::from("right"), Value::Int(right)),
    ]))
}

pub(crate) fn base32_encode(bytes: &[u8]) -> String {
    BASE32.encode(bytes)
}

pub(crate) fn base32_decode(text: &str) -> Result<Vec<u8>, String> {
    let data = text
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .map(|byte| byte.to_ascii_uppercase())
        .collect::<Vec<_>>();
    if data.contains(&b'=') {
        BASE32
            .decode(&data)
            .map_err(|error| format!("invalid base32: {error}"))
    } else {
        BASE32_NOPAD
            .decode(&data)
            .map_err(|error| format!("invalid base32: {error}"))
    }
}

pub(crate) fn base64_encode(bytes: &[u8]) -> String {
    BASE64.encode(bytes)
}

pub(crate) fn base64_decode(text: &str) -> Result<Vec<u8>, String> {
    let data = text
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect::<Vec<_>>();
    if data.contains(&b'=') {
        BASE64
            .decode(&data)
            .map_err(|error| format!("invalid base64: {error}"))
    } else {
        BASE64_NOPAD
            .decode(&data)
            .map_err(|error| format!("invalid base64: {error}"))
    }
}

fn canonical_dump(bytes: &[u8]) -> String {
    let mut output = String::new();
    for (index, chunk) in bytes.chunks(16).enumerate() {
        let offset = index * 16;
        output.push_str(&format!("{offset:08x}  "));
        for column in 0..16 {
            if column == 8 {
                output.push(' ');
            }
            match chunk.get(column) {
                Some(byte) => output.push_str(&format!("{byte:02x} ")),
                None => output.push_str("   "),
            }
        }
        output.push(' ');
        output.push('|');
        for byte in chunk {
            if byte.is_ascii_graphic() || *byte == b' ' {
                output.push(char::from(*byte));
            } else {
                output.push('.');
            }
        }
        output.push_str("|\n");
    }
    output.push_str(&format!("{:08x}", bytes.len()));
    output
}

fn radix_dump(bytes: &[u8], radix: u32) -> String {
    let mut output = String::new();
    for (index, chunk) in bytes.chunks(16).enumerate() {
        if index > 0 {
            output.push('\n');
        }
        output.push_str(&format!("{:07o}", index * 16));
        for byte in chunk {
            if radix == 16 {
                output.push_str(&format!(" {byte:02x}"));
            } else {
                output.push_str(&format!(" {byte:03o}"));
            }
        }
    }
    if bytes.is_empty() {
        output.push_str("0000000");
    }
    output
}

fn push_string_run(output: &mut Vec<Value>, current: &mut Vec<u8>, min_len: usize) {
    if current.len() >= min_len {
        let text = String::from_utf8(std::mem::take(current)).expect("ASCII printable run");
        output.push(Value::Str(text.into()));
    } else {
        current.clear();
    }
}
