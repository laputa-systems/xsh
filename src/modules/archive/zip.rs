use crate::modules::archive::policy::{
    clean_archive_path, prepare_output_path_with_kind, refuse_existing_with_kind,
};
use crate::runtime::process::path_bytes;
use crate::runtime::value::{PathValue, RuntimeError, Value};
use crate::source::Span;
use async_zip::{StoredZipEntry, base::read::mem::ZipFileReader};
use futures_lite::io::AsyncReadExt;
use std::fmt;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::{BUFFER_SIZE, archive_error, block_on_archive};

const ZIP_EXTRACT_KIND: &str = "archive-zip-extract";

pub(crate) fn zip_list(path: PathBuf, span: Span) -> Result<Vec<Value>, RuntimeError> {
    block_on_archive(span, zip_list_async(path, span))
}

async fn zip_list_async(path: PathBuf, span: Span) -> Result<Vec<Value>, RuntimeError> {
    let reader = zip_reader(path, "archive-zip-open", span).await?;
    reader
        .file()
        .entries()
        .iter()
        .map(|entry| zip_entry_record(entry, span))
        .collect()
}

pub(crate) fn zip_extract(
    path: PathBuf,
    dest: PathBuf,
    overwrite: bool,
    span: Span,
) -> Result<(), RuntimeError> {
    block_on_archive(span, zip_extract_async(path, dest, overwrite, span))
}

async fn zip_extract_async(
    path: PathBuf,
    dest: PathBuf,
    overwrite: bool,
    span: Span,
) -> Result<(), RuntimeError> {
    let reader = zip_reader(path, "archive-zip-open", span).await?;
    fs::create_dir_all(&dest).map_err(|error| archive_error(ZIP_EXTRACT_KIND, error, span))?;
    let plan = extraction_plan(&reader, &dest, overwrite, span)?;
    for output in &plan.dirs {
        fs::create_dir_all(output).map_err(|error| archive_error(ZIP_EXTRACT_KIND, error, span))?;
    }
    extract_files(reader, plan.files, span).await
}

async fn extract_files(
    reader: ZipFileReader,
    files: Vec<ZipFilePlan>,
    span: Span,
) -> Result<(), RuntimeError> {
    for file in files {
        extract_file(&reader, file, span).await?;
    }
    Ok(())
}

async fn extract_file(
    reader: &ZipFileReader,
    file: ZipFilePlan,
    span: Span,
) -> Result<(), RuntimeError> {
    let mut entry = reader
        .reader_without_entry(file.index)
        .await
        .map_err(|error| zip_runtime_error(ZIP_EXTRACT_KIND, error, span))?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .create_new(!file.overwrite)
        .truncate(file.overwrite)
        .open(&file.output)
        .map_err(|error| archive_error(ZIP_EXTRACT_KIND, error, span))?;
    let mut buffer = vec![0_u8; BUFFER_SIZE];
    loop {
        let count = entry
            .read(&mut buffer)
            .await
            .map_err(|error| zip_runtime_error(ZIP_EXTRACT_KIND, error, span))?;
        if count == 0 {
            break;
        }
        output
            .write_all(&buffer[..count])
            .map_err(|error| archive_error(ZIP_EXTRACT_KIND, error, span))?;
    }
    output
        .flush()
        .map_err(|error| archive_error(ZIP_EXTRACT_KIND, error, span))?;
    if file.mode != 0 {
        fs::set_permissions(&file.output, fs::Permissions::from_mode(file.mode & 0o7777))
            .map_err(|error| archive_error(ZIP_EXTRACT_KIND, error, span))?;
    }
    Ok(())
}

async fn zip_reader(
    path: PathBuf,
    kind: &str,
    span: Span,
) -> Result<ZipFileReader, RuntimeError> {
    let mut input = fs::File::open(path).map_err(|error| archive_error(kind, error, span))?;
    let mut data = Vec::new();
    input
        .read_to_end(&mut data)
        .map_err(|error| archive_error(kind, error, span))?;
    ZipFileReader::new(data)
        .await
        .map_err(|error| zip_runtime_error(kind, error, span))
}

#[derive(Debug)]
struct ExtractionPlan {
    dirs: Vec<PathBuf>,
    files: Vec<ZipFilePlan>,
}

#[derive(Debug)]
struct ZipFilePlan {
    index: usize,
    output: PathBuf,
    mode: u32,
    overwrite: bool,
}

fn extraction_plan(
    reader: &ZipFileReader,
    dest: &Path,
    overwrite: bool,
    span: Span,
) -> Result<ExtractionPlan, RuntimeError> {
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    for (index, entry) in reader.file().entries().iter().enumerate() {
        let path = zip_entry_path(entry, ZIP_EXTRACT_KIND, span)?;
        match zip_entry_kind(entry, ZIP_EXTRACT_KIND, span)? {
            "dir" => {
                let output =
                    prepare_output_path_with_kind(dest, &path, true, ZIP_EXTRACT_KIND, span)?;
                dirs.push(output);
            }
            "file" => {
                let output =
                    prepare_output_path_with_kind(dest, &path, false, ZIP_EXTRACT_KIND, span)?;
                refuse_existing_with_kind(&output, overwrite, ZIP_EXTRACT_KIND, span)?;
                files.push(ZipFilePlan {
                    index,
                    output,
                    mode: u32::from(entry.unix_permissions().unwrap_or_default()),
                    overwrite,
                });
            }
            _ => {}
        }
    }
    Ok(ExtractionPlan { dirs, files })
}

fn zip_entry_record(entry: &StoredZipEntry, span: Span) -> Result<Value, RuntimeError> {
    let path = zip_entry_path(entry, "archive-zip-list", span)?;
    let mode = entry.unix_permissions().unwrap_or_default() as i64;
    Ok(Value::Record(crate::runtime::value::RecordMap::from([
        (
            Arc::from("path"),
            Value::Path(PathValue::new(path_bytes(&path)).map_err(|error| error.with_span(span))?),
        ),
        (
            Arc::from("kind"),
            Value::Str(zip_entry_kind(entry, "archive-zip-list", span)?.into()),
        ),
        (
            Arc::from("size"),
            Value::Int(entry.uncompressed_size() as i64),
        ),
        (Arc::from("mode"), Value::Int(mode)),
        (Arc::from("modified"), Value::Int(0)),
        (Arc::from("link_name"), Value::Str("".into())),
    ])))
}

fn zip_entry_path(entry: &StoredZipEntry, kind: &str, span: Span) -> Result<PathBuf, RuntimeError> {
    let name = entry
        .filename()
        .as_str()
        .map_err(|error| zip_runtime_error(kind, error, span))?;
    clean_archive_path(Path::new(name), span)
}

fn zip_entry_kind(
    entry: &StoredZipEntry,
    kind: &str,
    span: Span,
) -> Result<&'static str, RuntimeError> {
    if entry
        .dir()
        .map_err(|error| zip_runtime_error(kind, error, span))?
    {
        return Ok("dir");
    }
    match entry.unix_permissions().unwrap_or_default() & 0o170000 {
        0o040000 => Ok("dir"),
        0o120000 => Ok("symlink"),
        0o100000 | 0 => Ok("file"),
        _ => Ok("other"),
    }
}

fn zip_runtime_error(kind: &str, error: impl fmt::Display, span: Span) -> RuntimeError {
    RuntimeError::new(kind, error.to_string()).with_span(span)
}
