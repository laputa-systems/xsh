use crate::budget::Budget;
use crate::entry::EntryMeta;
use crate::error::{Error, Result};
use std::io::{self, Read};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Completion(u8);

impl Completion {
    pub const INSPECTED: Self = Self(1);
    pub const PAYLOAD_CONSUMED: Self = Self(2);
    pub const CHECKSUM_VERIFIED: Self = Self(4);
    pub const STRUCTURE_COMPLETE: Self = Self(8);
    pub const OUTER_COMPLETE: Self = Self(16);

    pub const fn none() -> Self {
        Self(0)
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    #[must_use]
    pub const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Finish {
    pub structure_complete: bool,
    pub checksum_verified: bool,
}

pub struct Entry<'a> {
    meta: EntryMeta,
    body: Box<dyn Read + 'a>,
}

impl<'a> Entry<'a> {
    pub fn new(meta: EntryMeta, body: impl Read + 'a) -> Self {
        Self {
            meta,
            body: Box::new(body),
        }
    }

    pub fn empty(meta: EntryMeta) -> Self {
        Self::new(meta, io::empty())
    }

    pub const fn meta(&self) -> &EntryMeta {
        &self.meta
    }
}

impl Read for Entry<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.body.read(buffer)
    }
}

pub trait Cursor: Send {
    fn next_entry(&mut self) -> Result<Option<Entry<'_>>>;
    fn finish(&mut self) -> Result<Finish>;
}

pub(crate) struct Counted<R> {
    inner: R,
    position: u64,
}

impl<R> Counted<R> {
    pub(crate) const fn new(inner: R) -> Self {
        Self { inner, position: 0 }
    }

    pub(crate) const fn position(&self) -> u64 {
        self.position
    }
}

impl<R: Read> Read for Counted<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let count = self.inner.read(buffer)?;
        self.position += count as u64;
        Ok(count)
    }
}

pub(crate) fn read_up_to<R: Read>(reader: &mut R, buffer: &mut [u8]) -> Result<usize> {
    let mut filled = 0;
    while filled < buffer.len() {
        match reader.read(&mut buffer[filled..]) {
            Ok(0) => break,
            Ok(count) => filled += count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(Error::from_read(&error)),
        }
    }
    Ok(filled)
}

pub(crate) fn read_exact_or_truncated<R: Read>(
    reader: &mut R,
    buffer: &mut [u8],
    what: &str,
) -> Result<()> {
    if read_up_to(reader, buffer)? == buffer.len() {
        Ok(())
    } else {
        Err(Error::truncated(format!("archive ended inside {what}")))
    }
}

pub(crate) fn discard<R: Read>(reader: &mut R, mut count: u64, budget: &Budget) -> Result<()> {
    let mut scratch = [0_u8; 8192];
    while count > 0 {
        budget.checkpoint()?;
        let want = usize::try_from(count.min(scratch.len() as u64)).unwrap_or(scratch.len());
        let read = read_up_to(reader, &mut scratch[..want])?;
        if read == 0 {
            return Err(Error::truncated("archive ended inside an entry body"));
        }
        count -= read as u64;
    }
    Ok(())
}

pub(crate) struct BodyState {
    pub(crate) remaining: u64,
    pub(crate) padding: u64,
}

pub(crate) struct SequentialBody<'a, R> {
    pub(crate) reader: &'a mut R,
    pub(crate) state: &'a mut BodyState,
    pub(crate) budget: &'a Arc<Budget>,
    pub(crate) delivered: u64,
}

impl<R: Read> Read for SequentialBody<'_, R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.state.remaining == 0 || buffer.is_empty() {
            return Ok(0);
        }
        self.budget.checkpoint()?;
        let want = usize::try_from(self.state.remaining.min(buffer.len() as u64))
            .unwrap_or(buffer.len());
        let count = self.reader.read(&mut buffer[..want])?;
        if count == 0 {
            return Err(Error::truncated("archive ended inside an entry body").into());
        }
        self.state.remaining -= count as u64;
        self.delivered += count as u64;
        Ok(count)
    }
}
