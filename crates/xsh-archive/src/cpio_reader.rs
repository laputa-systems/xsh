use crate::budget::Budget;
use crate::cursor::{
    BodyState, Counted, Cursor, Entry, Finish, SequentialBody, discard, read_exact_or_truncated,
    read_up_to,
};
use crate::entry::{EntryKind, EntryMeta};
use crate::error::{Error, Result};
use std::io::{self, Read};
use std::sync::Arc;

const HEADER: usize = 110;
const MAX_NAME: u64 = 1 << 16;
const MAX_LINK: u64 = 1 << 16;
const TRAILER: &[u8] = b"TRAILER!!!";

pub(crate) struct CpioCursor {
    reader: Counted<Box<dyn Read + Send>>,
    state: BodyState,
    budget: Arc<Budget>,
    ended: bool,
    checked_entries: u64,
    verified_entries: u64,
}

impl CpioCursor {
    pub(crate) fn new(reader: Box<dyn Read + Send>, budget: Arc<Budget>) -> Self {
        Self {
            reader: Counted::new(reader),
            state: BodyState {
                remaining: 0,
                padding: 0,
            },
            budget,
            ended: false,
            checked_entries: 0,
            verified_entries: 0,
        }
    }

    fn finish_trailer(&mut self) -> Result<()> {
        let mut scratch = [0_u8; 8192];
        loop {
            self.budget.checkpoint()?;
            let count = read_up_to(&mut self.reader, &mut scratch)?;
            if count == 0 {
                return Ok(());
            }
            if scratch[..count].iter().any(|byte| *byte != 0) {
                return Err(Error::corrupt("data follows the cpio trailer")
                    .with_offset(self.reader.position()));
            }
        }
    }

    fn read_meta(&mut self) -> Result<Option<(EntryMeta, Option<u32>)>> {
        self.budget.checkpoint()?;
        let offset = self.reader.position();
        let mut header = [0_u8; HEADER];
        let filled = read_up_to(&mut self.reader, &mut header)?;
        if filled == 0 {
            return Err(Error::truncated("cpio archive has no trailer").with_offset(offset));
        }
        if filled < HEADER {
            return Err(Error::truncated("cpio archive ended inside a header").with_offset(offset));
        }
        let checked = match &header[..6] {
            b"070701" => false,
            b"070702" => true,
            _ => {
                return Err(Error::unsupported("cpio magic is not newc (070701 or 070702)")
                    .with_offset(offset));
            }
        };
        let field = |index: usize| hex_field(&header, 6 + index * 8, offset);
        let mode = field(1)?;
        let uid = field(2)?;
        let gid = field(3)?;
        let nlink = field(4)?;
        let mtime = field(5)?;
        let size = field(6)?;
        let rdev_major = field(9)?;
        let rdev_minor = field(10)?;
        let name_size = u64::from(field(11)?);
        let check = field(12)?;
        if name_size == 0 {
            return Err(Error::corrupt("cpio entry has no name").with_offset(offset));
        }
        if name_size > MAX_NAME {
            return Err(Error::limit(format!(
                "cpio name declares {name_size} bytes, over the {MAX_NAME} byte limit"
            )));
        }
        self.budget.charge_index(name_size)?;
        let mut name = vec![0_u8; name_size as usize];
        read_exact_or_truncated(&mut self.reader, &mut name, "a cpio name")?;
        if name.pop() != Some(0) || name.contains(&0) {
            return Err(Error::corrupt("cpio name is not NUL terminated").with_offset(offset));
        }
        discard(
            &mut self.reader,
            align4(HEADER as u64 + name_size) - (HEADER as u64 + name_size),
            &self.budget,
        )?;
        if name == TRAILER {
            return Ok(None);
        }
        let mut meta = EntryMeta::new(name, kind_of(mode));
        meta.mode = Some(mode & 0o7777);
        meta.modified = Some(i64::from(mtime));
        meta.uid = Some(uid);
        meta.gid = Some(gid);
        meta.size = Some(u64::from(size));
        meta.link_count = Some(nlink);
        if matches!(meta.kind, EntryKind::CharDevice | EntryKind::BlockDevice) {
            meta.device = Some((rdev_major, rdev_minor));
        }
        Ok(Some((meta, checked.then_some(check))))
    }
}

impl Cursor for CpioCursor {
    fn next_entry(&mut self) -> Result<Option<Entry<'_>>> {
        let leftover = self.state.remaining + self.state.padding;
        self.state.remaining = 0;
        self.state.padding = 0;
        discard(&mut self.reader, leftover, &self.budget)?;
        if self.ended {
            return Ok(None);
        }
        let Some((mut meta, check)) = self.read_meta()? else {
            self.ended = true;
            self.finish_trailer()?;
            return Ok(None);
        };
        self.budget.count_entry()?;
        let size = meta.size.unwrap_or(0);
        self.budget.declare_entry_size(size)?;
        if meta.kind == EntryKind::Symlink {
            if size > MAX_LINK {
                return Err(Error::limit("cpio symlink target is too long")
                    .with_entry(&meta.name));
            }
            self.budget.charge_index(size)?;
            let mut target = vec![0_u8; size as usize];
            read_exact_or_truncated(&mut self.reader, &mut target, "a cpio symlink target")?;
            discard(&mut self.reader, align4(size) - size, &self.budget)?;
            if let Some(expected) = check {
                self.checked_entries += 1;
                if byte_sum(&target) != expected {
                    return Err(Error::corrupt("cpio checksum mismatch").with_entry(&meta.name));
                }
                self.verified_entries += 1;
            }
            meta.link = Some(target);
            self.budget.charge_index(meta.index_bytes())?;
            return Ok(Some(Entry::empty(meta)));
        }
        self.budget.charge_index(meta.index_bytes())?;
        self.state.remaining = size;
        self.state.padding = align4(size) - size;
        let body = SequentialBody {
            reader: &mut self.reader,
            state: &mut self.state,
            budget: &self.budget,
            delivered: 0,
        };
        match check {
            None => Ok(Some(Entry::new(meta, body))),
            Some(expected) => {
                self.checked_entries += 1;
                Ok(Some(Entry::new(
                    meta,
                    CheckedBody {
                        inner: body,
                        sum: 0,
                        expected,
                        verified: &mut self.verified_entries,
                        done: false,
                    },
                )))
            }
        }
    }

    fn finish(&mut self) -> Result<Finish> {
        Ok(Finish {
            structure_complete: self.ended,
            checksum_verified: self.ended
                && self.checked_entries > 0
                && self.checked_entries == self.verified_entries,
        })
    }
}

struct CheckedBody<'a, R> {
    inner: SequentialBody<'a, R>,
    sum: u32,
    expected: u32,
    verified: &'a mut u64,
    done: bool,
}

impl<R: Read> Read for CheckedBody<'_, R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let count = self.inner.read(buffer)?;
        self.sum = self.sum.wrapping_add(byte_sum(&buffer[..count]));
        if self.inner.state.remaining == 0 && !self.done {
            self.done = true;
            if self.sum != self.expected {
                return Err(Error::corrupt("cpio checksum mismatch").into());
            }
            *self.verified += 1;
        }
        Ok(count)
    }
}

fn byte_sum(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .fold(0_u32, |sum, byte| sum.wrapping_add(u32::from(*byte)))
}

fn align4(length: u64) -> u64 {
    (length + 3) & !3
}

fn hex_field(header: &[u8; HEADER], start: usize, offset: u64) -> Result<u32> {
    let text = std::str::from_utf8(&header[start..start + 8])
        .map_err(|_| Error::corrupt("cpio header field is not hexadecimal").with_offset(offset))?;
    u32::from_str_radix(text, 16)
        .map_err(|_| Error::corrupt("cpio header field is not hexadecimal").with_offset(offset))
}

fn kind_of(mode: u32) -> EntryKind {
    match mode & 0o170_000 {
        0o100_000 => EntryKind::File,
        0o040_000 => EntryKind::Directory,
        0o120_000 => EntryKind::Symlink,
        0o020_000 => EntryKind::CharDevice,
        0o060_000 => EntryKind::BlockDevice,
        0o010_000 => EntryKind::Fifo,
        _ => EntryKind::Other,
    }
}
