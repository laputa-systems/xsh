use crate::budget::Budget;
use crate::compression::{BUFFER_SIZE, Compression, Encode, Encoder};
use crate::entry::Format;
use crate::error::{Error, Result};
use crate::extract::{Collision, normalize};
use crate::output::PendingOutput;
use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, BufWriter, Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tar::{EntryType, Header};
use xsh_root::{FileKind, Metadata, OpenOptions, Root};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CreateOptions {
    pub format: Format,
    pub compression: Compression,
    pub level: Option<i64>,
    pub collision: Collision,
}

type Components = Vec<Vec<u8>>;

pub fn create(
    dest: &Path,
    source_root: &Path,
    entries: &[Vec<u8>],
    options: &CreateOptions,
    budget: &Arc<Budget>,
) -> Result<()> {
    if entries.is_empty() {
        return Err(Error::options("entries cannot be empty"));
    }
    if options.format == Format::Zip {
        return Err(Error::unsupported("zip archives cannot be created"));
    }
    let roots = entries
        .iter()
        .map(|entry| normalize(entry))
        .collect::<Result<Vec<_>>>()?;
    let leaf = dest
        .file_name()
        .ok_or_else(|| Error::options("destination has no final name"))?;
    let codec = options.compression.resolve_write(leaf.as_bytes());
    let encode = match codec {
        Some(codec) => Some(Encode::new(codec, options.level)?),
        None if options.level.is_some() => {
            return Err(Error::options("a level needs a compression"));
        }
        None => None,
    };
    let source = Root::open(source_root).map_err(|error| Error::from_open(&error))?;
    let (pending, file) = PendingOutput::open(dest)?;
    let written = write_archive(
        file,
        encode.as_ref(),
        &source,
        &roots,
        options.format,
        pending.identity(),
        budget,
    );
    match written {
        Ok(()) => pending.publish(options.collision),
        Err(error) => Err(pending.discard(error)),
    }
}

fn write_archive(
    file: File,
    encode: Option<&Encode>,
    source: &Root,
    roots: &[Components],
    format: Format,
    identity: (u64, u64),
    budget: &Arc<Budget>,
) -> Result<()> {
    let sink = BufWriter::with_capacity(BUFFER_SIZE, file);
    let mut encoder = Encoder::new(encode, sink)?;
    match format {
        Format::Tar => write_tar(&mut encoder, source, roots, identity, budget)?,
        Format::Cpio => write_cpio(&mut encoder, source, roots, identity, budget)?,
        Format::Zip => unreachable!("zip creation is rejected before writing"),
    }
    let sink = encoder.finish()?;
    sink.into_inner()
        .map(drop)
        .map_err(|error| Error::from_host(error.error()))
}

fn to_path(components: &[Vec<u8>]) -> PathBuf {
    let mut path = PathBuf::new();
    for part in components {
        path.push(OsStr::from_bytes(part));
    }
    path
}

fn joined(components: &[Vec<u8>]) -> Vec<u8> {
    components.join(&b'/')
}

fn walk(
    source: &Root,
    roots: &[Components],
    identity: (u64, u64),
    budget: &Arc<Budget>,
    visit: &mut dyn FnMut(&Components, &Metadata) -> Result<()>,
) -> Result<()> {
    for root in roots {
        if root.is_empty() {
            let mut children = source
                .read_dir(".")
                .map_err(|error| Error::from_host(&error))?;
            children.sort_by(|left, right| left.name.cmp(&right.name));
            for child in children {
                let components = vec![child.name.as_bytes().to_vec()];
                walk_entry(source, &components, identity, budget, visit)?;
            }
        } else {
            walk_entry(source, root, identity, budget, visit)?;
        }
    }
    Ok(())
}

fn walk_entry(
    source: &Root,
    components: &Components,
    identity: (u64, u64),
    budget: &Arc<Budget>,
    visit: &mut dyn FnMut(&Components, &Metadata) -> Result<()>,
) -> Result<()> {
    budget.checkpoint()?;
    let path = to_path(components);
    let metadata = source
        .metadata(&path)
        .map_err(|error| Error::from_host(&error).with_entry(&joined(components)))?;
    if (metadata.device, metadata.inode) == identity {
        return Ok(());
    }
    budget.count_entry()?;
    visit(components, &metadata)?;
    if metadata.kind == FileKind::Directory {
        let mut children = source
            .read_dir(&path)
            .map_err(|error| Error::from_host(&error).with_entry(&joined(components)))?;
        children.sort_by(|left, right| left.name.cmp(&right.name));
        for child in children {
            let mut next = components.clone();
            next.push(child.name.as_bytes().to_vec());
            walk_entry(source, &next, identity, budget, visit)?;
        }
    }
    Ok(())
}

struct ExactBody<'a> {
    file: File,
    remaining: u64,
    budget: &'a Arc<Budget>,
}

impl Read for ExactBody<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.remaining == 0 {
            return Ok(0);
        }
        self.budget.checkpoint()?;
        let want = usize::try_from(self.remaining.min(buffer.len() as u64)).unwrap_or(buffer.len());
        let count = self.file.read(&mut buffer[..want])?;
        if count == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "a file shrank while it was being archived",
            ));
        }
        self.remaining -= count as u64;
        self.budget.charge_output(count as u64)?;
        Ok(count)
    }
}

fn open_source(source: &Root, components: &Components) -> Result<File> {
    source
        .open_with(
            to_path(components),
            OpenOptions::new().read(true).no_symlinks(true),
        )
        .map_err(|error| Error::from_host(&error).with_entry(&joined(components)))
}

fn write_tar<W: Write>(
    sink: &mut W,
    source: &Root,
    roots: &[Components],
    identity: (u64, u64),
    budget: &Arc<Budget>,
) -> Result<()> {
    let mut builder = tar::Builder::new(sink);
    builder.follow_symlinks(false);
    walk(source, roots, identity, budget, &mut |components, metadata| {
        let name = joined(components);
        let path = Path::new(OsStr::from_bytes(&name));
        let mut header = Header::new_gnu();
        header.set_mtime(0);
        header.set_uid(0);
        header.set_gid(0);
        header.set_mode(metadata.mode & 0o7777);
        let host = |error: io::Error| Error::from_read(&error).with_entry(&name);
        match metadata.kind {
            FileKind::File => {
                header.set_size(metadata.size);
                header.set_entry_type(EntryType::file());
                let body = ExactBody {
                    file: open_source(source, components)?,
                    remaining: metadata.size,
                    budget,
                };
                builder.append_data(&mut header, path, body).map_err(host)
            }
            FileKind::Directory => {
                header.set_size(0);
                header.set_entry_type(EntryType::dir());
                builder
                    .append_data(&mut header, path, io::empty())
                    .map_err(host)
            }
            FileKind::Symlink => {
                let target = source
                    .read_link(to_path(components))
                    .map_err(|error| Error::from_host(&error).with_entry(&name))?;
                header.set_size(0);
                header.set_entry_type(EntryType::symlink());
                builder
                    .append_link(&mut header, path, Path::new(&target))
                    .map_err(host)
            }
            FileKind::Fifo => {
                header.set_size(0);
                header.set_entry_type(EntryType::fifo());
                builder
                    .append_data(&mut header, path, io::empty())
                    .map_err(host)
            }
            FileKind::CharDevice | FileKind::BlockDevice => {
                header.set_size(0);
                header.set_entry_type(if metadata.kind == FileKind::CharDevice {
                    EntryType::character_special()
                } else {
                    EntryType::block_special()
                });
                header
                    .set_device_major(metadata.special_device.0)
                    .map_err(host)?;
                header
                    .set_device_minor(metadata.special_device.1)
                    .map_err(host)?;
                builder
                    .append_data(&mut header, path, io::empty())
                    .map_err(host)
            }
            FileKind::Socket => {
                Err(Error::unsupported("a socket cannot be archived").with_entry(&name))
            }
        }
    })?;
    builder.finish().map_err(|error| Error::from_host(&error))
}

fn write_cpio<W: Write>(
    sink: &mut W,
    source: &Root,
    roots: &[Components],
    identity: (u64, u64),
    budget: &Arc<Budget>,
) -> Result<()> {
    let mut inode = 1_u32;
    walk(source, roots, identity, budget, &mut |components, metadata| {
        let name = joined(components);
        let kind_bits = match metadata.kind {
            FileKind::File => 0o100_000,
            FileKind::Directory => 0o040_000,
            FileKind::Symlink => 0o120_000,
            FileKind::Fifo => 0o010_000,
            FileKind::CharDevice => 0o020_000,
            FileKind::BlockDevice => 0o060_000,
            FileKind::Socket => {
                return Err(Error::unsupported("a socket cannot be archived").with_entry(&name));
            }
        };
        let link;
        let (body_size, body): (u64, Option<&[u8]>) = match metadata.kind {
            FileKind::File => (metadata.size, None),
            FileKind::Symlink => {
                link = source
                    .read_link(to_path(components))
                    .map_err(|error| Error::from_host(&error).with_entry(&name))?;
                (link.as_bytes().len() as u64, Some(link.as_bytes()))
            }
            _ => (0, Some(&[])),
        };
        let header = CpioHeader {
            inode,
            mode: kind_bits | (metadata.mode & 0o7777),
            uid: metadata.uid,
            gid: metadata.gid,
            mtime: u32::try_from(metadata.modified.max(0)).unwrap_or(u32::MAX),
            size: body_size,
            rdev: metadata.special_device,
        };
        inode += 1;
        write_cpio_header(sink, &header, &name)?;
        match body {
            Some(bytes) => sink.write_all(bytes).map_err(|error| Error::from_host(&error))?,
            None => {
                let mut body = ExactBody {
                    file: open_source(source, components)?,
                    remaining: metadata.size,
                    budget,
                };
                io::copy(&mut body, sink).map_err(|error| Error::from_read(&error).with_entry(&name))?;
            }
        }
        pad(sink, body_size)
    })?;
    let trailer = CpioHeader {
        inode,
        mode: 0,
        uid: 0,
        gid: 0,
        mtime: 0,
        size: 0,
        rdev: (0, 0),
    };
    write_cpio_header(sink, &trailer, b"TRAILER!!!")?;
    sink.flush().map_err(|error| Error::from_host(&error))
}

struct CpioHeader {
    inode: u32,
    mode: u32,
    uid: u32,
    gid: u32,
    mtime: u32,
    size: u64,
    rdev: (u32, u32),
}

fn write_cpio_header<W: Write>(sink: &mut W, header: &CpioHeader, name: &[u8]) -> Result<()> {
    let size = u32::try_from(header.size).map_err(|_| {
        Error::limit("a cpio entry cannot exceed 4 GiB").with_entry(name)
    })?;
    let name_size = u32::try_from(name.len() + 1)
        .map_err(|_| Error::limit("a cpio name is too long").with_entry(name))?;
    let text = format!(
        "070701{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}",
        header.inode,
        header.mode,
        header.uid,
        header.gid,
        1,
        header.mtime,
        size,
        0,
        0,
        header.rdev.0,
        header.rdev.1,
        name_size,
        0,
    );
    sink.write_all(text.as_bytes())
        .and_then(|()| sink.write_all(name))
        .and_then(|()| sink.write_all(&[0]))
        .map_err(|error| Error::from_host(&error))?;
    pad(sink, 110 + u64::from(name_size))
}

fn pad<W: Write>(sink: &mut W, length: u64) -> Result<()> {
    let padding = ((4 - length % 4) % 4) as usize;
    sink.write_all(&[0_u8; 3][..padding])
        .map_err(|error| Error::from_host(&error))
}
