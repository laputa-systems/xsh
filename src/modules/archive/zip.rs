use crate::modules::archive::policy::{
    clean_archive_path, prepare_output_path_with_kind, refuse_existing_with_kind,
};
use crate::runtime::process::path_bytes;
use crate::runtime::value::{LiveStream, PathValue, RuntimeError, StreamValue, Value};
use crate::source::Span;
use flate2::read::DeflateDecoder;
use rawzip::{
    CompressionMethod, FileReader, ZipArchive, ZipArchiveEntryWayfinder, ZipFileHeaderRecord,
};
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::{BUFFER_SIZE, archive_error};

const ZIP_EXTRACT_KIND: &str = "archive-zip-extract";

pub(crate) fn zip_list(path: PathBuf, span: Span) -> Result<StreamValue, RuntimeError> {
    let reader = zip_reader(&path, "archive-zip-open", span)?;
    let entries = zip_entries(&reader, "archive-zip-open", span)?;
    Ok(StreamValue::from_live(
        "archive.zip_list",
        ZipListStream { entries, index: 0 },
    ))
}

struct ZipListStream {
    entries: Vec<ZipEntry>,
    index: usize,
}

impl LiveStream for ZipListStream {
    fn next(&mut self, span: Span) -> Result<Option<Value>, RuntimeError> {
        let Some(entry) = self.entries.get(self.index) else {
            return Ok(None);
        };
        self.index += 1;
        zip_entry_record(entry, span).map(Some)
    }
}

pub(crate) fn zip_extract(
    path: PathBuf,
    dest: PathBuf,
    overwrite: bool,
    span: Span,
) -> Result<(), RuntimeError> {
    let reader = zip_reader(&path, "archive-zip-open", span)?;
    let entries = zip_entries(&reader, "archive-zip-open", span)?;
    fs::create_dir_all(&dest).map_err(|error| archive_error(ZIP_EXTRACT_KIND, error, span))?;
    let plan = extraction_plan(&entries, &dest, overwrite, span)?;
    for output in &plan.dirs {
        fs::create_dir_all(output).map_err(|error| archive_error(ZIP_EXTRACT_KIND, error, span))?;
    }
    extract_files(&reader, plan.files, span)
}

fn extract_files(
    reader: &ZipArchive<FileReader>,
    files: Vec<ZipFilePlan>,
    span: Span,
) -> Result<(), RuntimeError> {
    for file in files {
        extract_file(reader, file, span)?;
    }
    Ok(())
}

fn extract_file(
    reader: &ZipArchive<FileReader>,
    file: ZipFilePlan,
    span: Span,
) -> Result<(), RuntimeError> {
    let entry = reader
        .get_entry(file.wayfinder)
        .map_err(|error| zip_runtime_error(ZIP_EXTRACT_KIND, error, span))?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .create_new(!file.overwrite)
        .truncate(file.overwrite)
        .open(&file.output)
        .map_err(|error| archive_error(ZIP_EXTRACT_KIND, error, span))?;
    copy_entry(&entry, file.compression, &mut output, span)?;
    output
        .flush()
        .map_err(|error| archive_error(ZIP_EXTRACT_KIND, error, span))?;
    if file.mode != 0 {
        fs::set_permissions(&file.output, fs::Permissions::from_mode(file.mode & 0o7777))
            .map_err(|error| archive_error(ZIP_EXTRACT_KIND, error, span))?;
    }
    Ok(())
}

fn copy_entry(
    entry: &rawzip::ZipEntry<'_, FileReader>,
    compression: CompressionMethod,
    output: &mut fs::File,
    span: Span,
) -> Result<(), RuntimeError> {
    match compression {
        CompressionMethod::STORE => {
            let mut input = entry.verifying_reader(entry.reader());
            io::copy(&mut input, output)
        }
        CompressionMethod::DEFLATE => {
            let mut input = entry.verifying_reader(DeflateDecoder::new(entry.reader()));
            io::copy(&mut input, output)
        }
        method => {
            return Err(RuntimeError::new(
                ZIP_EXTRACT_KIND,
                format!("unsupported zip compression method {}", method.as_u16()),
            )
            .with_span(span));
        }
    }
    .map(|_| ())
    .map_err(|error| archive_error(ZIP_EXTRACT_KIND, error, span))
}

fn zip_reader(path: &Path, kind: &str, span: Span) -> Result<ZipArchive<FileReader>, RuntimeError> {
    let input = fs::File::open(path).map_err(|error| archive_error(kind, error, span))?;
    let mut buffer = vec![0_u8; BUFFER_SIZE];
    ZipArchive::from_file(input, &mut buffer).map_err(|error| zip_runtime_error(kind, error, span))
}

fn zip_entries(
    reader: &ZipArchive<FileReader>,
    kind: &str,
    span: Span,
) -> Result<Vec<ZipEntry>, RuntimeError> {
    let expected = reader.entries_hint();
    let mut buffer = vec![0_u8; BUFFER_SIZE];
    let mut entries = reader.entries(&mut buffer);
    let mut output = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .map_err(|error| zip_runtime_error(kind, error, span))?
    {
        output.push(zip_entry(&entry, kind, span)?);
    }
    if output.len() as u64 != expected {
        return Err(
            RuntimeError::new(kind, "zip central directory entry count mismatch").with_span(span),
        );
    }
    Ok(output)
}

#[derive(Debug)]
struct ZipEntry {
    path: PathBuf,
    kind: &'static str,
    size: u64,
    mode: u32,
    wayfinder: ZipArchiveEntryWayfinder,
    compression: CompressionMethod,
}

fn zip_entry(
    entry: &ZipFileHeaderRecord<'_>,
    kind: &str,
    span: Span,
) -> Result<ZipEntry, RuntimeError> {
    Ok(ZipEntry {
        path: zip_entry_path(entry, kind, span)?,
        kind: zip_entry_kind(entry),
        size: entry.uncompressed_size_hint(),
        mode: entry.mode().value(),
        wayfinder: entry.wayfinder(),
        compression: entry.compression_method(),
    })
}

#[derive(Debug)]
struct ExtractionPlan {
    dirs: Vec<PathBuf>,
    files: Vec<ZipFilePlan>,
}

#[derive(Debug)]
struct ZipFilePlan {
    wayfinder: ZipArchiveEntryWayfinder,
    compression: CompressionMethod,
    output: PathBuf,
    mode: u32,
    overwrite: bool,
}

fn extraction_plan(
    entries: &[ZipEntry],
    dest: &Path,
    overwrite: bool,
    span: Span,
) -> Result<ExtractionPlan, RuntimeError> {
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    for entry in entries {
        match entry.kind {
            "dir" => {
                let output =
                    prepare_output_path_with_kind(dest, &entry.path, true, ZIP_EXTRACT_KIND, span)?;
                dirs.push(output);
            }
            "file" => {
                let output = prepare_output_path_with_kind(
                    dest,
                    &entry.path,
                    false,
                    ZIP_EXTRACT_KIND,
                    span,
                )?;
                refuse_existing_with_kind(&output, overwrite, ZIP_EXTRACT_KIND, span)?;
                files.push(ZipFilePlan {
                    wayfinder: entry.wayfinder,
                    compression: entry.compression,
                    output,
                    mode: entry.mode,
                    overwrite,
                });
            }
            _ => {}
        }
    }
    Ok(ExtractionPlan { dirs, files })
}

fn zip_entry_record(entry: &ZipEntry, span: Span) -> Result<Value, RuntimeError> {
    let mode = entry.mode as i64;
    Ok(Value::Record(crate::runtime::value::RecordMap::from([
        (
            Arc::from("path"),
            Value::Path(
                PathValue::new(path_bytes(&entry.path)).map_err(|error| error.with_span(span))?,
            ),
        ),
        (Arc::from("kind"), Value::Str(entry.kind.into())),
        (Arc::from("size"), Value::Int(entry.size as i64)),
        (Arc::from("mode"), Value::Int(mode)),
        (Arc::from("modified"), Value::Int(0)),
        (Arc::from("link_name"), Value::Str("".into())),
    ])))
}

fn zip_entry_path(
    entry: &ZipFileHeaderRecord<'_>,
    kind: &str,
    span: Span,
) -> Result<PathBuf, RuntimeError> {
    let path = entry.file_path();
    let name = std::str::from_utf8(path.as_bytes())
        .map_err(|error| zip_runtime_error(kind, error, span))?;
    clean_archive_path(Path::new(name), span)
}

fn zip_entry_kind(entry: &ZipFileHeaderRecord<'_>) -> &'static str {
    if entry.is_dir() {
        return "dir";
    }
    match entry.mode().value() & 0o170000 {
        0o040000 => "dir",
        0o120000 => "symlink",
        0o100000 | 0 => "file",
        _ => "other",
    }
}

fn zip_runtime_error(kind: &str, error: impl fmt::Display, span: Span) -> RuntimeError {
    RuntimeError::new(kind, error.to_string()).with_span(span)
}
