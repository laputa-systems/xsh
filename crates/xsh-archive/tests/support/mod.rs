#![allow(dead_code)]

pub mod specs;

use rawzip::time::UtcDateTime;
use rawzip::{CompressionMethod, ZipArchiveWriter};
use std::ffi::OsStr;
use std::io::{self, Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::sync::Arc;
use xsh_archive::{
    Archive, Budget, Compression, Completion, EntryMeta, ErrorKind, Format, Limits, Source,
};

pub fn budget() -> Arc<Budget> {
    Budget::uncancelable(Limits::default())
}

pub fn budget_with(limits: Limits) -> Arc<Budget> {
    Budget::uncancelable(limits)
}

pub fn tar_with(entries: &[(&[u8], &[u8])]) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    for (name, body) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(body.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(1_700_000_000);
        header.set_entry_type(tar::EntryType::Regular);
        builder
            .append_data(&mut header, OsStr::from_bytes(name), *body)
            .unwrap();
    }
    builder.into_inner().unwrap()
}

pub fn tar_raw_header(name: &[u8], flag: u8, size: u64, link: &[u8]) -> [u8; 512] {
    let mut block = [0_u8; 512];
    block[..name.len()].copy_from_slice(name);
    block[100..107].copy_from_slice(b"0000644");
    block[108..115].copy_from_slice(b"0000000");
    block[116..123].copy_from_slice(b"0000000");
    block[124..135].copy_from_slice(format!("{size:011o}").as_bytes());
    block[136..147].copy_from_slice(b"14000000000");
    block[156] = flag;
    block[157..157 + link.len()].copy_from_slice(link);
    block[257..263].copy_from_slice(b"ustar\0");
    block[263..265].copy_from_slice(b"00");
    seal(&mut block);
    block
}

pub fn seal(block: &mut [u8; 512]) {
    block[148..156].copy_from_slice(b"        ");
    let sum: u32 = block.iter().map(|byte| u32::from(*byte)).sum();
    block[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
}

pub fn pad512(mut bytes: Vec<u8>) -> Vec<u8> {
    let rest = bytes.len() % 512;
    if rest != 0 {
        bytes.resize(bytes.len() + 512 - rest, 0);
    }
    bytes
}

pub fn cpio_entry(name: &[u8], mode: u32, body: &[u8], nlink: u32, check: Option<u32>) -> Vec<u8> {
    let magic: &[u8] = if check.is_some() { b"070702" } else { b"070701" };
    let mut out = magic.to_vec();
    for value in [
        1, mode, 1000, 1000, nlink, 1_700_000_000, body.len() as u32, 0, 0, 0, 0,
        name.len() as u32 + 1, check.unwrap_or(0),
    ] {
        out.extend_from_slice(format!("{value:08x}").as_bytes());
    }
    out.extend_from_slice(name);
    out.push(0);
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
    out.extend_from_slice(body);
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
    out
}

pub fn cpio_trailer() -> Vec<u8> {
    cpio_entry(b"TRAILER!!!", 0, b"", 1, None)
}

pub fn cpio_with(entries: &[(&[u8], &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    for (name, body) in entries {
        out.extend(cpio_entry(name, 0o100_644, body, 1, None));
    }
    out.extend(cpio_trailer());
    pad512(out)
}

pub fn zip_with(entries: &[(&str, &[u8], i64)]) -> Vec<u8> {
    let mut output = Vec::new();
    let mut archive = ZipArchiveWriter::new(&mut output);
    for (name, body, modified) in entries {
        let (mut file, config) = archive
            .new_file(name)
            .compression_method(CompressionMethod::DEFLATE)
            .last_modified(UtcDateTime::from_unix(*modified))
            .unix_permissions(0o640)
            .start()
            .unwrap();
        let encoder = flate2::write::DeflateEncoder::new(&mut file, flate2::Compression::fast());
        let mut writer = config.wrap(encoder);
        writer.write_all(body).unwrap();
        let (encoder, description) = writer.finish().unwrap();
        encoder.finish().unwrap();
        file.finish(description).unwrap();
    }
    archive.finish().unwrap();
    output
}

pub fn open(bytes: Vec<u8>, format: Format, compression: Compression) -> Result<Archive, xsh_archive::Error> {
    Archive::open(Source::from_bytes(bytes), format, compression, budget())
}

pub fn open_stream(bytes: Vec<u8>, format: Format, compression: Compression) -> Result<Archive, xsh_archive::Error> {
    Archive::open(Source::from_reader(OneByte::new(bytes)), format, compression, budget())
}

pub struct OneByte {
    bytes: Vec<u8>,
    position: usize,
}

impl OneByte {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self { bytes, position: 0 }
    }
}

impl Read for OneByte {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.position >= self.bytes.len() || buffer.is_empty() {
            return Ok(0);
        }
        buffer[0] = self.bytes[self.position];
        self.position += 1;
        Ok(1)
    }
}

pub struct Listing {
    pub entries: Vec<EntryMeta>,
    pub completion: Completion,
}

pub fn list(archive: &mut Archive) -> Result<Listing, xsh_archive::Error> {
    let mut entries = Vec::new();
    while let Some(entry) = archive.next_entry()? {
        entries.push(entry.meta().clone());
    }
    let completion = archive.finish(false)?;
    Ok(Listing { entries, completion })
}

pub fn read_all(archive: &mut Archive) -> Result<Vec<(EntryMeta, Vec<u8>)>, xsh_archive::Error> {
    let mut entries = Vec::new();
    while let Some(mut entry) = archive.next_entry()? {
        let mut body = Vec::new();
        entry
            .read_to_end(&mut body)
            .map_err(|error| xsh_archive::Error::from_read(&error))?;
        entries.push((entry.meta().clone(), body));
    }
    archive.finish(true)?;
    Ok(entries)
}

pub fn kind_of(result: Result<impl Sized, xsh_archive::Error>) -> ErrorKind {
    match result {
        Ok(_) => panic!("expected an error"),
        Err(error) => error.kind(),
    }
}
