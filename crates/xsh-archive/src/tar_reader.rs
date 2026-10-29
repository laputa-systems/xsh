use crate::budget::Budget;
use crate::cursor::{
    BodyState, Counted, Cursor, Entry, Finish, SequentialBody, discard, read_exact_or_truncated,
    read_up_to,
};
use crate::entry::{EntryKind, EntryMeta};
use crate::error::{Error, Result};
use std::io::{self, Read};
use std::sync::Arc;
use tar::{EntryType, Header, PaxExtensions};

const BLOCK: usize = 512;
const MAX_EXTENSION: u64 = 1 << 20;

pub(crate) struct TarCursor {
    reader: Counted<Box<dyn Read + Send>>,
    state: BodyState,
    budget: Arc<Budget>,
    ended: bool,
}

impl TarCursor {
    pub(crate) fn new(reader: Box<dyn Read + Send>, budget: Arc<Budget>) -> Self {
        Self {
            reader: Counted::new(reader),
            state: BodyState {
                remaining: 0,
                padding: 0,
            },
            budget,
            ended: false,
        }
    }

    fn skip_body(&mut self) -> Result<()> {
        let count = self.state.remaining + self.state.padding;
        self.state.remaining = 0;
        self.state.padding = 0;
        discard(&mut self.reader, count, &self.budget)
    }

    fn read_extension(&mut self, size: u64, what: &str) -> Result<Vec<u8>> {
        if size > MAX_EXTENSION {
            return Err(Error::limit(format!(
                "{what} declares {size} bytes, over the {MAX_EXTENSION} byte header limit"
            )));
        }
        self.budget.charge_index(size)?;
        let mut body = vec![0_u8; size as usize];
        read_exact_or_truncated(&mut self.reader, &mut body, what)?;
        discard(&mut self.reader, padding(size), &self.budget)?;
        Ok(body)
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
                return Err(Error::corrupt("data follows the end-of-archive blocks")
                    .with_offset(self.reader.position()));
            }
        }
    }

    fn read_meta(&mut self) -> Result<Option<EntryMeta>> {
        let mut long_name: Option<Vec<u8>> = None;
        let mut long_link: Option<Vec<u8>> = None;
        let mut pax: Option<Pax> = None;
        loop {
            self.budget.checkpoint()?;
            let mut block = [0_u8; BLOCK];
            let filled = read_up_to(&mut self.reader, &mut block)?;
            if filled == 0 {
                return Err(Error::truncated("archive has no end-of-archive blocks")
                    .with_offset(self.reader.position()));
            }
            if filled < BLOCK {
                return Err(Error::truncated("archive ended inside a header")
                    .with_offset(self.reader.position()));
            }
            if block.iter().all(|byte| *byte == 0) {
                if long_name.is_some() || long_link.is_some() || pax.is_some() {
                    return Err(Error::corrupt("extension header has no member to describe"));
                }
                self.finish_trailer()?;
                return Ok(None);
            }
            let header_offset = self.reader.position() - BLOCK as u64;
            let mut header = Header::new_old();
            header.as_mut_bytes().copy_from_slice(&block);
            verify_checksum(&header, &block).map_err(|error| error.with_offset(header_offset))?;
            let flag = header.entry_type();
            let size = header
                .entry_size()
                .map_err(|error| Error::corrupt(error.to_string()).with_offset(header_offset))?;
            if flag.is_gnu_longname() {
                let body = self.read_extension(size, "a GNU long name")?;
                if long_name.replace(until_nul(body)).is_some() {
                    return Err(Error::corrupt("two long names describe one member"));
                }
                continue;
            }
            if flag.is_gnu_longlink() {
                let body = self.read_extension(size, "a GNU long link")?;
                if long_link.replace(until_nul(body)).is_some() {
                    return Err(Error::corrupt("two long links describe one member"));
                }
                continue;
            }
            if flag.is_pax_local_extensions() {
                let body = self.read_extension(size, "a pax header")?;
                if pax.replace(Pax::parse(&body)?).is_some() {
                    return Err(Error::corrupt("two pax headers describe one member"));
                }
                continue;
            }
            if flag.is_pax_global_extensions() {
                let total = size
                    .checked_add(padding(size))
                    .ok_or_else(|| Error::corrupt("pax global header size overflows"))?;
                discard(&mut self.reader, total, &self.budget)?;
                continue;
            }
            return meta_from(&header, size, long_name, long_link, pax, header_offset).map(Some);
        }
    }
}

impl Cursor for TarCursor {
    fn next_entry(&mut self) -> Result<Option<Entry<'_>>> {
        self.skip_body()?;
        if self.ended {
            return Ok(None);
        }
        let Some(meta) = self.read_meta()? else {
            self.ended = true;
            return Ok(None);
        };
        self.budget.count_entry()?;
        self.budget.charge_index(meta.index_bytes())?;
        let size = meta.size.unwrap_or(0);
        self.budget.declare_entry_size(size)?;
        self.state.remaining = size;
        self.state.padding = padding(size);
        Ok(Some(Entry::new(
            meta,
            SequentialBody {
                reader: &mut self.reader,
                state: &mut self.state,
                budget: &self.budget,
                delivered: 0,
            },
        )))
    }

    fn finish(&mut self) -> Result<Finish> {
        Ok(Finish {
            structure_complete: self.ended,
            checksum_verified: self.ended,
        })
    }
}

fn padding(size: u64) -> u64 {
    (BLOCK as u64 - size % BLOCK as u64) % BLOCK as u64
}

fn until_nul(mut bytes: Vec<u8>) -> Vec<u8> {
    if let Some(end) = bytes.iter().position(|byte| *byte == 0) {
        bytes.truncate(end);
    }
    bytes
}

fn verify_checksum(header: &Header, block: &[u8; BLOCK]) -> Result<()> {
    let stored = header
        .cksum()
        .map_err(|error| Error::from_read(&error).with_context("header checksum field"))?;
    let computed = block[..148]
        .iter()
        .chain(&block[156..])
        .map(|byte| u32::from(*byte))
        .sum::<u32>()
        + 8 * 32;
    if stored == computed {
        Ok(())
    } else {
        Err(Error::corrupt("header checksum mismatch"))
    }
}

#[derive(Default)]
struct Pax {
    path: Option<Vec<u8>>,
    link: Option<Vec<u8>>,
    size: Option<u64>,
    uid: Option<u64>,
    gid: Option<u64>,
    mtime: Option<i64>,
}

impl Pax {
    fn parse(body: &[u8]) -> Result<Self> {
        let mut pax = Self::default();
        for extension in PaxExtensions::new(body) {
            let extension = extension
                .map_err(|error| Error::from_read(&error).with_context("pax header"))?;
            let Ok(key) = extension.key() else { continue };
            let value = extension.value_bytes();
            match key {
                "path" => pax.path = Some(value.to_vec()),
                "linkpath" => pax.link = Some(value.to_vec()),
                "size" => pax.size = Some(parse_unsigned(value, "size")?),
                "uid" => pax.uid = Some(parse_unsigned(value, "uid")?),
                "gid" => pax.gid = Some(parse_unsigned(value, "gid")?),
                "mtime" => pax.mtime = Some(parse_seconds(value)?),
                _ => {}
            }
        }
        Ok(pax)
    }
}

fn parse_unsigned(value: &[u8], what: &str) -> Result<u64> {
    std::str::from_utf8(value)
        .ok()
        .and_then(|text| text.parse().ok())
        .ok_or_else(|| Error::corrupt(format!("pax {what} is not a number")))
}

fn parse_seconds(value: &[u8]) -> Result<i64> {
    let text = std::str::from_utf8(value)
        .map_err(|_| Error::corrupt("pax mtime is not a number"))?;
    let whole = text.split('.').next().unwrap_or("");
    whole
        .parse()
        .map_err(|_| Error::corrupt("pax mtime is not a number"))
}

fn kind_of(flag: EntryType, name: &[u8]) -> EntryKind {
    if flag.is_dir() {
        EntryKind::Directory
    } else if flag.is_file() || flag.is_contiguous() {
        if name.ends_with(b"/") {
            EntryKind::Directory
        } else {
            EntryKind::File
        }
    } else if flag.is_symlink() {
        EntryKind::Symlink
    } else if flag.is_hard_link() {
        EntryKind::Hardlink
    } else if flag.is_fifo() {
        EntryKind::Fifo
    } else if flag.is_character_special() {
        EntryKind::CharDevice
    } else if flag.is_block_special() {
        EntryKind::BlockDevice
    } else {
        EntryKind::Other
    }
}

fn meta_from(
    header: &Header,
    header_size: u64,
    long_name: Option<Vec<u8>>,
    long_link: Option<Vec<u8>>,
    pax: Option<Pax>,
    offset: u64,
) -> Result<EntryMeta> {
    let pax = pax.unwrap_or_default();
    let read = |error: io::Error| Error::corrupt(error.to_string()).with_offset(offset);
    let name = pax
        .path
        .or(long_name)
        .unwrap_or_else(|| header.path_bytes().into_owned());
    if name.is_empty() {
        return Err(Error::corrupt("member has an empty name").with_offset(offset));
    }
    let kind = kind_of(header.entry_type(), &name);
    let mut meta = EntryMeta::new(name, kind);
    meta.size = Some(pax.size.unwrap_or(header_size));
    let old = header.as_old();
    meta.mode = optional(header.mode(), &old.mode).map_err(read)?.and_then(|mode| u32::try_from(mode).ok());
    meta.modified = match pax.mtime {
        Some(seconds) => Some(seconds),
        None => optional(header.mtime(), &old.mtime)
            .map_err(read)?
            .map(|seconds| i64::try_from(seconds).unwrap_or(i64::MAX)),
    };
    meta.uid = match pax.uid {
        Some(uid) => u32::try_from(uid).ok(),
        None => optional(header.uid(), &old.uid).map_err(read)?.and_then(|uid| u32::try_from(uid).ok()),
    };
    meta.gid = match pax.gid {
        Some(gid) => u32::try_from(gid).ok(),
        None => optional(header.gid(), &old.gid).map_err(read)?.and_then(|gid| u32::try_from(gid).ok()),
    };
    if matches!(kind, EntryKind::Symlink | EntryKind::Hardlink) {
        meta.link = pax
            .link
            .or(long_link)
            .or_else(|| header.link_name_bytes().map(std::borrow::Cow::into_owned));
    }
    if matches!(kind, EntryKind::CharDevice | EntryKind::BlockDevice) {
        let major = header.device_major().map_err(read)?;
        let minor = header.device_minor().map_err(read)?;
        meta.device = major.zip(minor);
    }
    Ok(meta)
}

fn optional<T: Into<u64>>(parsed: io::Result<T>, field: &[u8]) -> io::Result<Option<u64>> {
    match parsed {
        Ok(value) => Ok(Some(value.into())),
        Err(_) if field.iter().all(|byte| matches!(byte, 0 | b' ')) => Ok(None),
        Err(error) => Err(error),
    }
}
