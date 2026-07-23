use crate::modules::archive::policy::{
    archive_member_path, clean_archive_path, prepare_output_path, refuse_existing,
    validate_link_target,
};
use crate::runtime::value::{LiveStream, PathValue, RuntimeError, StreamValue, Value};
use crate::source::Span;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::{BUFFER_SIZE, archive_error, copy_exact, create_output_file};

pub(crate) fn cpio_list(path: PathBuf, span: Span) -> Result<StreamValue, RuntimeError> {
    let reader = BufReader::with_capacity(
        BUFFER_SIZE,
        File::open(path).map_err(|error| archive_error("archive-cpio-open", error, span))?,
    );
    Ok(StreamValue::from_live(
        "archive.cpio_list",
        CpioListStream { reader, span },
    ))
}

struct CpioListStream {
    reader: BufReader<File>,
    span: Span,
}

impl LiveStream for CpioListStream {
    fn next(&mut self, span: Span) -> Result<Option<Value>, RuntimeError> {
        let entry = read_cpio_entry(&mut self.reader, self.span)?;
        if entry.name == b"TRAILER!!!" {
            return Ok(None);
        }
        let record = cpio_entry_record(&entry, span)?;
        skip_cpio_data(&mut self.reader, entry.size, self.span)?;
        Ok(Some(record))
    }
}

pub(crate) fn cpio_extract(
    path: PathBuf,
    dest: PathBuf,
    overwrite: bool,
    span: Span,
) -> Result<(), RuntimeError> {
    let mut reader = BufReader::with_capacity(
        BUFFER_SIZE,
        File::open(path).map_err(|error| archive_error("archive-cpio-open", error, span))?,
    );
    fs::create_dir_all(&dest)
        .map_err(|error| archive_error("archive-cpio-extract", error, span))?;
    loop {
        let entry = read_cpio_entry(&mut reader, span)?;
        if entry.name == b"TRAILER!!!" {
            break;
        }
        extract_cpio_entry(&mut reader, &dest, entry, overwrite, span)?;
    }
    Ok(())
}

pub(crate) fn cpio_create(
    path: PathBuf,
    root: PathBuf,
    entries: Vec<PathValue>,
    overwrite: bool,
    span: Span,
) -> Result<(), RuntimeError> {
    if entries.is_empty() {
        return Err(
            RuntimeError::new("archive-cpio-create", "entries cannot be empty").with_span(span),
        );
    }
    let file = create_output_file(&path, overwrite, "archive-cpio-create", span)?;
    let mut writer = BufWriter::with_capacity(BUFFER_SIZE, file);
    let mut inode = 1_u32;
    for entry in entries {
        let archive_name = archive_member_path(&entry, span)?;
        let source = if archive_name.as_os_str().is_empty() {
            root.clone()
        } else {
            root.join(&archive_name)
        };
        append_cpio_entry(&mut writer, &source, &archive_name, &mut inode, span)?;
    }
    write_cpio_trailer(&mut writer, &mut inode, span)?;
    writer
        .flush()
        .map_err(|error| archive_error("archive-cpio-create", error, span))
}

#[derive(Clone, Debug)]
struct CpioEntry {
    name: Vec<u8>,
    mode: u32,
    mtime: u32,
    size: u64,
}

fn append_cpio_entry<W: Write>(
    writer: &mut W,
    source: &Path,
    archive_name: &Path,
    inode: &mut u32,
    span: Span,
) -> Result<(), RuntimeError> {
    let metadata = fs::symlink_metadata(source)
        .map_err(|error| archive_error("archive-cpio-create", error, span))?;
    append_cpio_entry_with_meta(writer, source, archive_name, &metadata, inode, span)
}

fn append_cpio_entry_with_meta<W: Write>(
    writer: &mut W,
    source: &Path,
    archive_name: &Path,
    metadata: &fs::Metadata,
    inode: &mut u32,
    span: Span,
) -> Result<(), RuntimeError> {
    if metadata.is_dir() && archive_name.as_os_str().is_empty() {
        let mut entries = fs::read_dir(source)
            .map_err(|error| archive_error("archive-cpio-create", error, span))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| archive_error("archive-cpio-create", error, span))?;
        entries.sort_unstable_by_key(|entry| entry.file_name());
        for entry in entries {
            let child_meta = entry
                .metadata()
                .map_err(|error| archive_error("archive-cpio-create", error, span))?;
            append_cpio_entry_with_meta(
                writer,
                &entry.path(),
                &PathBuf::from(entry.file_name()),
                &child_meta,
                inode,
                span,
            )?;
        }
        return Ok(());
    }

    let name = archive_name.as_os_str().as_bytes().to_vec();
    if name.is_empty() {
        return Err(RuntimeError::new("archive-cpio-create", "empty archive path").with_span(span));
    }
    let mode = cpio_mode(metadata);
    if metadata.is_dir() {
        write_cpio_header(writer, *inode, metadata, mode, 0, &name, span)?;
        *inode += 1;
        let mut entries = fs::read_dir(source)
            .map_err(|error| archive_error("archive-cpio-create", error, span))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| archive_error("archive-cpio-create", error, span))?;
        entries.sort_unstable_by_key(|entry| entry.file_name());
        for entry in entries {
            let child_meta = entry
                .metadata()
                .map_err(|error| archive_error("archive-cpio-create", error, span))?;
            append_cpio_entry_with_meta(
                writer,
                &entry.path(),
                &archive_name.join(entry.file_name()),
                &child_meta,
                inode,
                span,
            )?;
        }
        return Ok(());
    }
    if metadata.file_type().is_symlink() {
        let target = fs::read_link(source)
            .map_err(|error| archive_error("archive-cpio-create", error, span))?;
        let data = target.as_os_str().as_bytes();
        write_cpio_header(
            writer,
            *inode,
            metadata,
            mode,
            data.len() as u64,
            &name,
            span,
        )?;
        writer
            .write_all(data)
            .map_err(|error| archive_error("archive-cpio-create", error, span))?;
        write_padding(writer, data.len(), span, "archive-cpio-create")?;
        *inode += 1;
        return Ok(());
    }
    if metadata.is_file() {
        write_cpio_header(writer, *inode, metadata, mode, metadata.len(), &name, span)?;
        let mut input = File::open(source)
            .map_err(|error| archive_error("archive-cpio-create", error, span))?;
        io::copy(&mut input, &mut *writer)
            .map_err(|error| archive_error("archive-cpio-create", error, span))?;
        write_padding(writer, metadata.len() as usize, span, "archive-cpio-create")?;
        *inode += 1;
    }
    Ok(())
}

fn write_cpio_trailer<W: Write>(
    writer: &mut W,
    inode: &mut u32,
    span: Span,
) -> Result<(), RuntimeError> {
    write_cpio_header_raw(
        writer,
        *inode,
        0,
        0,
        0,
        1,
        0,
        0,
        b"TRAILER!!!",
        span,
        "archive-cpio-create",
    )
}

fn write_cpio_header<W: Write>(
    writer: &mut W,
    inode: u32,
    metadata: &fs::Metadata,
    mode: u32,
    size: u64,
    name: &[u8],
    span: Span,
) -> Result<(), RuntimeError> {
    write_cpio_header_raw(
        writer,
        inode,
        mode,
        metadata.uid(),
        metadata.gid(),
        1,
        metadata.mtime().max(0) as u32,
        size,
        name,
        span,
        "archive-cpio-create",
    )
}

#[allow(clippy::too_many_arguments)]
fn write_cpio_header_raw<W: Write>(
    writer: &mut W,
    inode: u32,
    mode: u32,
    uid: u32,
    gid: u32,
    nlink: u32,
    mtime: u32,
    size: u64,
    name: &[u8],
    span: Span,
    kind: &str,
) -> Result<(), RuntimeError> {
    if size > u32::MAX as u64 {
        return Err(RuntimeError::new(kind, "cpio entry too large").with_span(span));
    }
    let namesize = name
        .len()
        .checked_add(1)
        .ok_or_else(|| RuntimeError::new(kind, "cpio name too large").with_span(span))?;
    let header = format!(
        "070701{inode:08x}{mode:08x}{uid:08x}{gid:08x}{nlink:08x}{mtime:08x}{size:08x}{devmajor:08x}{devminor:08x}{rdevmajor:08x}{rdevminor:08x}{namesize:08x}{check:08x}",
        size = size as u32,
        devmajor = 0,
        devminor = 0,
        rdevmajor = 0,
        rdevminor = 0,
        check = 0,
    );
    writer
        .write_all(header.as_bytes())
        .and_then(|()| writer.write_all(name))
        .and_then(|()| writer.write_all(&[0]))
        .map_err(|error| archive_error(kind, error, span))?;
    write_padding(writer, 110 + namesize, span, kind)
}

fn read_cpio_entry<R: Read>(reader: &mut R, span: Span) -> Result<CpioEntry, RuntimeError> {
    let mut header = [0_u8; 110];
    reader
        .read_exact(&mut header)
        .map_err(|error| archive_error("archive-cpio-read", error, span))?;
    if &header[..6] != b"070701" {
        return Err(
            RuntimeError::new("archive-cpio-read", "unsupported cpio format").with_span(span),
        );
    }
    let mode = parse_cpio_hex(&header, 14, span)? as u32;
    let _uid = parse_cpio_hex(&header, 22, span)? as u32;
    let _gid = parse_cpio_hex(&header, 30, span)? as u32;
    let mtime = parse_cpio_hex(&header, 46, span)? as u32;
    let size = parse_cpio_hex(&header, 54, span)?;
    let namesize = parse_cpio_hex(&header, 94, span)? as usize;
    if namesize == 0 {
        return Err(RuntimeError::new("archive-cpio-read", "missing cpio name").with_span(span));
    }
    let mut name = vec![0_u8; namesize];
    reader
        .read_exact(&mut name)
        .map_err(|error| archive_error("archive-cpio-read", error, span))?;
    if name.pop() != Some(0) {
        return Err(
            RuntimeError::new("archive-cpio-read", "unterminated cpio name").with_span(span),
        );
    }
    skip_padding(reader, 110 + namesize, span)?;
    Ok(CpioEntry {
        name,
        mode,
        mtime,
        size,
    })
}

fn parse_cpio_hex(header: &[u8; 110], offset: usize, span: Span) -> Result<u64, RuntimeError> {
    let text = std::str::from_utf8(&header[offset..offset + 8]).map_err(|error| {
        RuntimeError::new("archive-cpio-read", error.to_string()).with_span(span)
    })?;
    u64::from_str_radix(text, 16)
        .map_err(|error| RuntimeError::new("archive-cpio-read", error.to_string()).with_span(span))
}

fn extract_cpio_entry<R: Read>(
    reader: &mut R,
    dest: &Path,
    entry: CpioEntry,
    overwrite: bool,
    span: Span,
) -> Result<(), RuntimeError> {
    let archive_path = PathBuf::from(OsString::from_vec(entry.name.clone()));
    let path = clean_archive_path(&archive_path, span)?;
    match cpio_kind(entry.mode) {
        "dir" => {
            let output = prepare_output_path(dest, &path, true, span)?;
            fs::create_dir_all(&output)
                .map_err(|error| archive_error("archive-cpio-extract", error, span))?;
            skip_cpio_data(reader, entry.size, span)
        }
        "symlink" => {
            let output = prepare_output_path(dest, &path, false, span)?;
            refuse_existing(&output, overwrite, span)?;
            let mut target = vec![0_u8; entry.size as usize];
            reader
                .read_exact(&mut target)
                .map_err(|error| archive_error("archive-cpio-extract", error, span))?;
            skip_padding(reader, entry.size as usize, span)?;
            let target = PathBuf::from(OsString::from_vec(target));
            validate_link_target(&path, &target, span)?;
            symlink(&target, output)
                .map_err(|error| archive_error("archive-cpio-extract", error, span))
        }
        "file" => {
            let output = prepare_output_path(dest, &path, false, span)?;
            refuse_existing(&output, overwrite, span)?;
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create(true)
                .create_new(!overwrite)
                .truncate(overwrite)
                .open(&output)
                .map_err(|error| archive_error("archive-cpio-extract", error, span))?;
            copy_exact(reader, &mut file, entry.size, span, "archive-cpio-extract")?;
            fs::set_permissions(&output, fs::Permissions::from_mode(entry.mode & 0o7777))
                .map_err(|error| archive_error("archive-cpio-extract", error, span))?;
            skip_padding(reader, entry.size as usize, span)
        }
        _ => skip_cpio_data(reader, entry.size, span),
    }
}

fn skip_cpio_data<R: Read>(reader: &mut R, size: u64, span: Span) -> Result<(), RuntimeError> {
    copy_exact(reader, &mut io::sink(), size, span, "archive-cpio-read")?;
    skip_padding(reader, size as usize, span)
}

fn cpio_entry_record(entry: &CpioEntry, span: Span) -> Result<Value, RuntimeError> {
    let path = PathValue::new(entry.name.clone()).map_err(|error| error.with_span(span))?;
    Ok(Value::Record(crate::runtime::value::RecordMap::from([
        (Arc::from("path"), Value::Path(path)),
        (Arc::from("kind"), Value::Str(cpio_kind(entry.mode).into())),
        (Arc::from("size"), Value::Int(entry.size as i64)),
        (Arc::from("mode"), Value::Int((entry.mode & 0o7777) as i64)),
        (Arc::from("modified"), Value::Int(entry.mtime as i64)),
        (Arc::from("link_name"), Value::Str("".into())),
    ])))
}

fn cpio_mode(metadata: &fs::Metadata) -> u32 {
    let file_type = if metadata.is_dir() {
        0o040000
    } else if metadata.file_type().is_symlink() {
        0o120000
    } else if metadata.is_file() {
        0o100000
    } else {
        0
    };
    file_type | (metadata.permissions().mode() & 0o7777)
}

fn cpio_kind(mode: u32) -> &'static str {
    match mode & 0o170000 {
        0o040000 => "dir",
        0o100000 => "file",
        0o120000 => "symlink",
        _ => "other",
    }
}

fn write_padding<W: Write>(
    writer: &mut W,
    len: usize,
    span: Span,
    kind: &str,
) -> Result<(), RuntimeError> {
    let padding = align4(len) - len;
    if padding > 0 {
        writer
            .write_all(&vec![0_u8; padding])
            .map_err(|error| archive_error(kind, error, span))?;
    }
    Ok(())
}

fn skip_padding<R: Read>(reader: &mut R, len: usize, span: Span) -> Result<(), RuntimeError> {
    let padding = align4(len) - len;
    if padding > 0 {
        let mut sink = io::sink();
        copy_exact(reader, &mut sink, padding as u64, span, "archive-cpio-read")?;
    }
    Ok(())
}

fn align4(len: usize) -> usize {
    (len + 3) & !3
}
