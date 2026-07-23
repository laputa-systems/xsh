use crate::modules::archive::policy::{
    archive_member_filters, archive_member_path, archive_member_selected, archive_path_in,
    clean_archive_path, prepare_output_path, refuse_existing, strip_archive_path,
    validate_link_target,
};
use crate::modules::compression::{
    ArchiveWriter, BlockingAsyncIo, archive_reader, for_create as compression_for_create,
    parse as parse_compression,
};
use crate::runtime::process::path_bytes;
use crate::runtime::value::{LiveStream, PathValue, RuntimeError, StreamValue, Value};
use crate::source::Span;
use async_tar::{Archive, Builder, Entry, EntryType, Header};
use futures_lite::StreamExt;
use futures_lite::io::{AsyncRead, AsyncWrite, Cursor, copy, empty};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::{archive_error, block_on_archive};

pub(crate) fn tar_list(
    path: PathBuf,
    compression: &str,
    members: Vec<PathValue>,
    span: Span,
) -> Result<StreamValue, RuntimeError> {
    let reader = archive_reader(&path, parse_compression(compression, span)?, span)?;
    let archive = Archive::new(BlockingAsyncIo::new(reader));
    let entries = archive
        .entries()
        .map_err(|error| archive_error("archive-list", error, span))?;
    let filters = archive_member_filters(members, span)?;
    Ok(StreamValue::from_live(
        "archive.tar_list",
        TarListStream {
            entries,
            filters,
            span,
        },
    ))
}

type TarEntries = async_tar::Entries<BlockingAsyncIo<Box<dyn Read + Send>>>;

struct TarListStream {
    entries: TarEntries,
    filters: Vec<PathBuf>,
    span: Span,
}

impl LiveStream for TarListStream {
    fn next(&mut self, span: Span) -> Result<Option<Value>, RuntimeError> {
        loop {
            let entry = futures_lite::future::block_on(self.entries.next())
                .transpose()
                .map_err(|error| archive_error("archive-list", error, self.span))?;
            let Some(entry) = entry else {
                return Ok(None);
            };
            if entry.header().entry_type().is_pax_global_extensions() {
                continue;
            }
            let raw_path: PathBuf = entry
                .path()
                .map_err(|error| archive_error("archive-list", error, span))?
                .into_owned()
                .into();
            if !archive_member_selected(&raw_path, &self.filters) {
                continue;
            }
            return archive_entry_record(&entry, span).map(Some);
        }
    }
}

pub(crate) fn tar_extract(
    path: PathBuf,
    dest: PathBuf,
    strip_components: i64,
    compression: &str,
    overwrite: bool,
    members: Vec<PathValue>,
    span: Span,
) -> Result<(), RuntimeError> {
    block_on_archive(
        span,
        tar_extract_async(
            path,
            dest,
            strip_components,
            compression,
            overwrite,
            members,
            span,
        ),
    )
}

async fn tar_extract_async(
    path: PathBuf,
    dest: PathBuf,
    strip_components: i64,
    compression: &str,
    overwrite: bool,
    members: Vec<PathValue>,
    span: Span,
) -> Result<(), RuntimeError> {
    if strip_components < 0 {
        return Err(
            RuntimeError::new("archive-extract", "strip_components cannot be negative")
                .with_span(span),
        );
    }
    let reader = archive_reader(&path, parse_compression(compression, span)?, span)?;
    let archive = Archive::new(BlockingAsyncIo::new(reader));
    let filters = archive_member_filters(members, span)?;
    let mut entries = archive
        .entries()
        .map_err(|error| archive_error("archive-extract", error, span))?;
    fs::create_dir_all(&dest).map_err(|error| archive_error("archive-extract", error, span))?;
    while let Some(entry) = entries.next().await {
        let mut entry = entry.map_err(|error| archive_error("archive-extract", error, span))?;
        let raw_path: PathBuf = entry
            .path()
            .map_err(|error| archive_error("archive-extract", error, span))?
            .into_owned()
            .into();
        if !archive_member_selected(&raw_path, &filters) {
            continue;
        }
        let Some(path) = strip_archive_path(&raw_path, strip_components as usize, span)? else {
            continue;
        };
        extract_entry(&mut entry, &dest, &path, overwrite, span).await?;
    }
    Ok(())
}

pub(crate) fn tar_create(
    path: PathBuf,
    root: PathBuf,
    entries: Vec<PathValue>,
    compression: &str,
    overwrite: bool,
    span: Span,
) -> Result<(), RuntimeError> {
    block_on_archive(
        span,
        tar_create_async(path, root, entries, compression, overwrite, span),
    )
}

async fn tar_create_async(
    path: PathBuf,
    root: PathBuf,
    entries: Vec<PathValue>,
    compression: &str,
    overwrite: bool,
    span: Span,
) -> Result<(), RuntimeError> {
    if entries.is_empty() {
        return Err(RuntimeError::new("archive-create", "entries cannot be empty").with_span(span));
    }
    if path.exists() && !overwrite {
        return Err(RuntimeError::new("archive-create", "destination exists").with_span(span));
    }
    let compression = compression_for_create(&path, parse_compression(compression, span)?);
    let writer = ArchiveWriter::create(&path, compression, overwrite, span)?;
    let mut builder = Builder::new(BlockingAsyncIo::new(writer));
    builder.follow_symlinks(false);
    let mut create_entries = Vec::new();
    for entry in entries {
        let archive_name = archive_member_path(&entry, span)?;
        let source = if archive_name.as_os_str().is_empty() {
            root.clone()
        } else {
            root.join(&archive_name)
        };
        collect_create_entry(&source, &archive_name, span, &mut create_entries)?;
    }
    for entry in create_entries {
        append_create_entry(&mut builder, &entry, span).await?;
    }
    let writer = builder
        .into_inner()
        .await
        .map_err(|error| archive_error("archive-create", error, span))?;
    let writer = writer.into_inner();
    writer.finish(span)
}

#[derive(Debug)]
struct CreateEntry {
    source: PathBuf,
    archive_name: PathBuf,
}

fn collect_create_entry(
    source: &Path,
    archive_name: &Path,
    span: Span,
    entries: &mut Vec<CreateEntry>,
) -> Result<(), RuntimeError> {
    let metadata = fs::symlink_metadata(source)
        .map_err(|error| archive_error("archive-create", error, span))?;
    collect_create_entry_with_meta(source, archive_name, &metadata, span, entries)
}

fn collect_create_entry_with_meta(
    source: &Path,
    archive_name: &Path,
    metadata: &fs::Metadata,
    span: Span,
    entries: &mut Vec<CreateEntry>,
) -> Result<(), RuntimeError> {
    if metadata.is_dir() && archive_name.as_os_str().is_empty() {
        let mut dir_entries = fs::read_dir(source)
            .map_err(|error| archive_error("archive-create", error, span))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| archive_error("archive-create", error, span))?;
        dir_entries.sort_unstable_by_key(|entry| entry.file_name());
        for entry in dir_entries {
            let child_path = entry.path();
            let child_meta = fs::symlink_metadata(&child_path)
                .map_err(|error| archive_error("archive-create", error, span))?;
            collect_create_entry_with_meta(
                &child_path,
                &PathBuf::from(entry.file_name()),
                &child_meta,
                span,
                entries,
            )?;
        }
        return Ok(());
    }
    if metadata.is_dir() {
        entries.push(CreateEntry {
            source: source.to_path_buf(),
            archive_name: archive_name.to_path_buf(),
        });
        let mut dir_entries = fs::read_dir(source)
            .map_err(|error| archive_error("archive-create", error, span))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| archive_error("archive-create", error, span))?;
        dir_entries.sort_unstable_by_key(|entry| entry.file_name());
        for entry in dir_entries {
            let child_path = entry.path();
            let child_meta = fs::symlink_metadata(&child_path)
                .map_err(|error| archive_error("archive-create", error, span))?;
            collect_create_entry_with_meta(
                &child_path,
                &archive_name.join(entry.file_name()),
                &child_meta,
                span,
                entries,
            )?;
        }
        return Ok(());
    }
    entries.push(CreateEntry {
        source: source.to_path_buf(),
        archive_name: archive_name.to_path_buf(),
    });
    Ok(())
}

async fn append_create_entry<W: AsyncWrite + Unpin + Send + Sync>(
    builder: &mut Builder<W>,
    entry: &CreateEntry,
    span: Span,
) -> Result<(), RuntimeError> {
    let metadata = fs::symlink_metadata(&entry.source)
        .map_err(|error| archive_error("archive-create", error, span))?;
    let file_type = metadata.file_type();
    let mut header = Header::new_gnu();
    header.set_mtime(0);
    header.set_mode(metadata.mode());
    header.set_size(if file_type.is_file() { metadata.len() } else { 0 });
    header.set_entry_type(if file_type.is_dir() {
        EntryType::dir()
    } else if file_type.is_file() {
        EntryType::file()
    } else if file_type.is_symlink() {
        EntryType::symlink()
    } else if file_type.is_fifo() {
        EntryType::fifo()
    } else if file_type.is_char_device() {
        EntryType::character_special()
    } else if file_type.is_block_device() {
        EntryType::block_special()
    } else {
        EntryType::new(b' ')
    });

    if file_type.is_file() {
        let file = File::open(&entry.source)
            .map_err(|error| archive_error("archive-create", error, span))?;
        builder
            .append_data(&mut header, &entry.archive_name, BlockingAsyncIo::new(file))
            .await
            .map_err(|error| archive_error("archive-create", error, span))?;
        return Ok(());
    }

    if file_type.is_dir() {
        builder
            .append_data(&mut header, &entry.archive_name, empty())
            .await
            .map_err(|error| archive_error("archive-create", error, span))?;
        return Ok(());
    }

    if file_type.is_symlink() {
        let target = fs::read_link(&entry.source)
            .map_err(|error| archive_error("archive-create", error, span))?;
        append_link_name(builder, &mut header, &target, span).await?;
        builder
            .append_data(&mut header, &entry.archive_name, empty())
            .await
            .map_err(|error| archive_error("archive-create", error, span))?;
        return Ok(());
    }

    append_special_create_entry(
        builder,
        &entry.source,
        &entry.archive_name,
        &metadata,
        header,
        span,
    )
    .await
}

async fn append_link_name<W: AsyncWrite + Unpin + Send + Sync>(
    builder: &mut Builder<W>,
    header: &mut Header,
    target: &Path,
    span: Span,
) -> Result<(), RuntimeError> {
    if header.set_link_name(target).is_ok() {
        return Ok(());
    }

    let target = target.as_os_str().as_bytes();
    let mut long_link = Header::new_gnu();
    long_link
        .set_path("././@LongLink")
        .map_err(|error| archive_error("archive-create", error, span))?;
    long_link.set_mode(0o644);
    long_link.set_uid(0);
    long_link.set_gid(0);
    long_link.set_mtime(0);
    long_link.set_size(target.len() as u64 + 1);
    long_link.set_entry_type(EntryType::GNULongLink);
    long_link.set_cksum();
    let mut data = target.to_vec();
    data.push(0);
    builder
        .append(&long_link, Cursor::new(data))
        .await
        .map_err(|error| archive_error("archive-create", error, span))
}

async fn append_special_create_entry<W: AsyncWrite + Unpin + Send + Sync>(
    builder: &mut Builder<W>,
    source: &Path,
    archive_name: &Path,
    metadata: &fs::Metadata,
    mut header: Header,
    span: Span,
) -> Result<(), RuntimeError> {
    let file_type = metadata.file_type();
    if file_type.is_socket() {
        return Err(
            RuntimeError::new("archive-create", "socket cannot be archived").with_span(span),
        );
    }
    if file_type.is_char_device() || file_type.is_block_device() {
        let dev_id = metadata.rdev();
        let dev_major = ((dev_id >> 32) & 0xffff_f000) | ((dev_id >> 8) & 0x0000_0fff);
        let dev_minor = ((dev_id >> 12) & 0xffff_ff00) | (dev_id & 0x0000_00ff);
        header
            .set_device_major(dev_major as u32)
            .map_err(|error| archive_error("archive-create", error, span))?;
        header
            .set_device_minor(dev_minor as u32)
            .map_err(|error| archive_error("archive-create", error, span))?;
    } else if !file_type.is_fifo() {
        return Err(RuntimeError::new(
            "archive-create",
            format!("{} has unknown file type", source.display()),
        )
        .with_span(span));
    }
    builder
        .append_data(&mut header, archive_name, empty())
        .await
        .map_err(|error| archive_error("archive-create", error, span))
}

async fn extract_entry<R: AsyncRead + Unpin>(
    entry: &mut Entry<R>,
    dest: &Path,
    path: &Path,
    overwrite: bool,
    span: Span,
) -> Result<(), RuntimeError> {
    let entry_type = entry.header().entry_type();
    if entry_type.is_dir() {
        let output = prepare_output_path(dest, path, true, span)?;
        fs::create_dir_all(&output)
            .map_err(|error| archive_error("archive-extract", error, span))?;
        set_entry_mode(&output, entry, span)?;
        return Ok(());
    }
    if entry_type.is_symlink() {
        let output = prepare_output_path(dest, path, false, span)?;
        refuse_existing(&output, overwrite, span)?;
        let target: PathBuf = entry
            .link_name()
            .map_err(|error| archive_error("archive-extract", error, span))?
            .ok_or_else(|| {
                RuntimeError::new("archive-extract", "missing symlink target").with_span(span)
            })?
            .into_owned()
            .into();
        validate_link_target(path, &target, span)?;
        symlink(&target, &output).map_err(|error| archive_error("archive-extract", error, span))?;
        return Ok(());
    }
    if entry_type.is_hard_link() {
        let output = prepare_output_path(dest, path, false, span)?;
        refuse_existing(&output, overwrite, span)?;
        let target: PathBuf = entry
            .link_name()
            .map_err(|error| archive_error("archive-extract", error, span))?
            .ok_or_else(|| {
                RuntimeError::new("archive-extract", "missing hardlink target").with_span(span)
            })?
            .into_owned()
            .into();
        let target = clean_archive_path(&target, span)?;
        let target_output = archive_path_in(dest, &target, span)?;
        fs::hard_link(target_output, output)
            .map_err(|error| archive_error("archive-extract", error, span))?;
        return Ok(());
    }
    if entry_type.is_file() {
        let output = prepare_output_path(dest, path, false, span)?;
        refuse_existing(&output, overwrite, span)?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .create_new(!overwrite)
            .truncate(overwrite)
            .open(&output)
            .map_err(|error| archive_error("archive-extract", error, span))?;
        let mut async_output = BlockingAsyncIo::new(file);
        copy(&mut *entry, &mut async_output)
            .await
            .map_err(|error| archive_error("archive-extract", error, span))?;
        file = async_output.into_inner();
        file.flush()
            .map_err(|error| archive_error("archive-extract", error, span))?;
        set_entry_mode(&output, entry, span)?;
    }
    Ok(())
}

fn set_entry_mode<R: AsyncRead + Unpin>(
    output: &Path,
    entry: &Entry<R>,
    span: Span,
) -> Result<(), RuntimeError> {
    let mode = entry
        .header()
        .mode()
        .map_err(|error| archive_error("archive-extract", error, span))?;
    fs::set_permissions(output, fs::Permissions::from_mode(mode))
        .map_err(|error| archive_error("archive-extract", error, span))
}

fn archive_entry_record<R: AsyncRead + Unpin>(
    entry: &Entry<R>,
    span: Span,
) -> Result<Value, RuntimeError> {
    let header = entry.header();
    let path: PathBuf = entry
        .path()
        .map_err(|error| archive_error("archive-list", error, span))?
        .into_owned()
        .into();
    let link_name = entry
        .link_name()
        .map_err(|error| archive_error("archive-list", error, span))?
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut record = BTreeMap::new();
    record.insert(
        Arc::from("path"),
        Value::Path(PathValue::new(path_bytes(&path)).map_err(|error| error.with_span(span))?),
    );
    record.insert(
        Arc::from("kind"),
        Value::Str(entry_kind(header.entry_type()).into()),
    );
    record.insert(
        Arc::from("size"),
        Value::Int(header.size().unwrap_or_default() as i64),
    );
    record.insert(
        Arc::from("mode"),
        Value::Int(header.mode().unwrap_or_default() as i64),
    );
    record.insert(
        Arc::from("modified"),
        Value::Int(header.mtime().unwrap_or_default() as i64),
    );
    record.insert(Arc::from("link_name"), Value::Str(link_name.into()));
    Ok(Value::Record(crate::runtime::value::RecordMap::from(
        record,
    )))
}

fn entry_kind(kind: EntryType) -> &'static str {
    if kind.is_dir() {
        "dir"
    } else if kind.is_file() {
        "file"
    } else if kind.is_symlink() {
        "symlink"
    } else if kind.is_hard_link() {
        "hardlink"
    } else {
        "other"
    }
}
