use crate::budget::Budget;
use crate::cursor::{Cursor, Entry, Finish};
use crate::entry::{EntryKind, EntryMeta};
use crate::error::{Error, Result};
use flate2::read::DeflateDecoder;
use rawzip::time::ZipDateTimeKind;
use rawzip::{
    CompressionMethod, CreatorSystem, ReaderAt, ZipArchive, ZipArchiveEntryWayfinder,
    ZipFileHeaderRecord,
};
use std::io::{self, Read};
use std::sync::Arc;

const SYMLINK_LIMIT: u64 = 1 << 16;

struct Record {
    meta: EntryMeta,
    wayfinder: ZipArchiveEntryWayfinder,
    method: CompressionMethod,
}

#[derive(Default)]
struct Counters {
    started: u64,
    verified: u64,
}

pub(crate) struct ZipCursor<A> {
    archive: ZipArchive<A>,
    records: Vec<Record>,
    index: usize,
    budget: Arc<Budget>,
    counters: Counters,
}

impl<A: ReaderAt + Send> ZipCursor<A> {
    pub(crate) fn new(archive: ZipArchive<A>, budget: Arc<Budget>) -> Result<Self> {
        let expected = archive.entries_hint();
        let mut buffer = vec![0_u8; rawzip::RECOMMENDED_BUFFER_SIZE];
        let mut records = Vec::new();
        {
            let mut entries = archive.entries(&mut buffer);
            while let Some(record) = entries.next_entry().map_err(zip_error)? {
                budget.checkpoint()?;
                budget.count_entry()?;
                let record = describe(&record)?;
                budget.charge_index(record.meta.index_bytes() + 48)?;
                records.push(record);
            }
        }
        if records.len() as u64 != expected {
            return Err(Error::corrupt(
                "zip central directory entry count does not match its header",
            ));
        }
        Ok(Self {
            archive,
            records,
            index: 0,
            budget,
            counters: Counters::default(),
        })
    }
}

impl<A: ReaderAt + Send> Cursor for ZipCursor<A> {
    fn next_entry(&mut self) -> Result<Option<Entry<'_>>> {
        self.budget.checkpoint()?;
        let Some(record) = self.records.get(self.index) else {
            return Ok(None);
        };
        self.index += 1;
        let mut meta = record.meta.clone();
        let size = meta.size.unwrap_or(0);
        self.budget.declare_entry_size(size)?;
        let method = record.method;
        let wayfinder = record.wayfinder;
        if meta.kind == EntryKind::Symlink {
            if size > SYMLINK_LIMIT {
                return Err(Error::limit("zip symlink target is too long").with_entry(&meta.name));
            }
            let target = self.read_whole(wayfinder, method, size, &meta.name)?;
            self.budget.charge_index(target.len() as u64)?;
            meta.link = Some(target);
            return Ok(Some(Entry::empty(meta)));
        }
        if meta.kind != EntryKind::File {
            return Ok(Some(Entry::empty(meta)));
        }
        let body = self.open_body(wayfinder, method, size)?;
        Ok(Some(Entry::new(meta, body)))
    }

    fn finish(&mut self) -> Result<Finish> {
        Ok(Finish {
            structure_complete: self.index >= self.records.len(),
            checksum_verified: self.counters.started > 0
                && self.counters.started == self.counters.verified,
        })
    }
}

impl<A: ReaderAt + Send> ZipCursor<A> {
    fn open_body(
        &mut self,
        wayfinder: ZipArchiveEntryWayfinder,
        method: CompressionMethod,
        declared: u64,
    ) -> Result<ZipBody<'_>> {
        self.counters.started += 1;
        let entry = self.archive.get_entry(wayfinder).map_err(zip_error)?;
        let inner: Box<dyn Read + '_> = match method {
            CompressionMethod::STORE => Box::new(entry.verifying_reader(entry.reader())),
            CompressionMethod::DEFLATE => {
                Box::new(entry.verifying_reader(DeflateDecoder::new(entry.reader())))
            }
            other => Box::new(UnsupportedMethod(other.as_u16())),
        };
        Ok(ZipBody {
            inner,
            budget: &self.budget,
            counters: &mut self.counters,
            declared,
            delivered: 0,
            finished: false,
        })
    }

    fn read_whole(
        &mut self,
        wayfinder: ZipArchiveEntryWayfinder,
        method: CompressionMethod,
        declared: u64,
        name: &[u8],
    ) -> Result<Vec<u8>> {
        let mut body = self.open_body(wayfinder, method, declared)?;
        let mut target = Vec::new();
        body.read_to_end(&mut target)
            .map_err(|error| Error::from_read(&error).with_entry(name))?;
        Ok(target)
    }
}

struct ZipBody<'a> {
    inner: Box<dyn Read + 'a>,
    budget: &'a Arc<Budget>,
    counters: &'a mut Counters,
    declared: u64,
    delivered: u64,
    finished: bool,
}

impl Read for ZipBody<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.budget.checkpoint()?;
        let count = self.inner.read(buffer)?;
        if count == 0 {
            if !self.finished {
                self.finished = true;
                self.counters.verified += 1;
            }
            return Ok(0);
        }
        self.delivered += count as u64;
        if self.delivered > self.declared {
            return Err(Error::corrupt("zip entry is larger than its declared size").into());
        }
        self.budget.charge_output(count as u64)?;
        Ok(count)
    }
}

struct UnsupportedMethod(u16);

impl Read for UnsupportedMethod {
    fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
        Err(Error::unsupported(format!("zip compression method {}", self.0)).into())
    }
}

fn describe(record: &ZipFileHeaderRecord<'_>) -> Result<Record> {
    let name = record.file_path().as_bytes().to_vec();
    if name.is_empty() {
        return Err(Error::corrupt("zip member has an empty name"));
    }
    let creator = record.version_made_by().creator_system();
    let unix = creator == CreatorSystem::UNIX || creator == CreatorSystem::MACOS;
    let mode_bits = record.mode().value();
    let kind = if record.is_dir() {
        EntryKind::Directory
    } else if unix {
        match mode_bits & 0o170_000 {
            0o040_000 => EntryKind::Directory,
            0o120_000 => EntryKind::Symlink,
            0o020_000 => EntryKind::CharDevice,
            0o060_000 => EntryKind::BlockDevice,
            0o010_000 => EntryKind::Fifo,
            0o100_000 | 0 => EntryKind::File,
            _ => EntryKind::Other,
        }
    } else {
        EntryKind::File
    };
    let mut meta = EntryMeta::new(name, kind);
    meta.size = Some(record.uncompressed_size_hint());
    meta.mode = unix.then_some(mode_bits & 0o7777);
    meta.modified = match record.last_modified() {
        ZipDateTimeKind::Utc(time) => Some(time.to_unix()),
        ZipDateTimeKind::Local(_) => None,
    };
    Ok(Record {
        meta,
        wayfinder: record.wayfinder(),
        method: record.compression_method(),
    })
}

fn zip_error(error: rawzip::Error) -> Error {
    match error.kind() {
        rawzip::ErrorKind::IO(inner) => Error::from_read(inner),
        _ => Error::corrupt(error.to_string()),
    }
}
