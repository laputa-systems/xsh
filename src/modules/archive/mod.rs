#![allow(clippy::single_call_fn)]

use crate::runtime::value::{LiveStream, PathValue, RecordMap, RuntimeError, StreamValue, Value};
use crate::source::Span;
use crate::symbol::Name;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use xsh_archive as engine;

pub(crate) enum SourceArg {
    Path(PathBuf),
    Bytes(Vec<u8>),
    Reader(Box<dyn Read + Send>),
}

pub(crate) struct Selection {
    pub(crate) format: engine::Format,
    pub(crate) compression: engine::Compression,
    pub(crate) members: Vec<PathValue>,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct LimitOverrides {
    pub(crate) max_output: Option<u64>,
    pub(crate) max_entry: Option<u64>,
    pub(crate) max_entries: Option<u64>,
}

pub(crate) struct ExtractRequest {
    pub(crate) source: SourceArg,
    pub(crate) dest: PathBuf,
    pub(crate) selection: Selection,
    pub(crate) strip_components: usize,
    pub(crate) collision: engine::Collision,
    pub(crate) links: engine::Links,
    pub(crate) special: engine::Special,
    pub(crate) modes: engine::Modes,
    pub(crate) ownership: engine::Ownership,
    pub(crate) limits: LimitOverrides,
}

pub(crate) struct CreateRequest {
    pub(crate) dest: PathBuf,
    pub(crate) root: PathBuf,
    pub(crate) entries: Vec<PathValue>,
    pub(crate) format: engine::Format,
    pub(crate) compression: engine::Compression,
    pub(crate) level: Option<i64>,
    pub(crate) collision: engine::Collision,
}

fn budget(overrides: LimitOverrides) -> Arc<engine::Budget> {
    let defaults = engine::Limits::default();
    engine::Budget::new(
        engine::Limits {
            total_output: overrides.max_output.unwrap_or(defaults.total_output),
            entry_output: overrides.max_entry.unwrap_or(defaults.entry_output),
            entries: overrides.max_entries.unwrap_or(defaults.entries),
            ..defaults
        },
        || crate::runtime::process::cancellation_requested_signal().is_some(),
    )
}

fn members(values: &[PathValue]) -> Vec<Vec<u8>> {
    values.iter().map(|path| path.bytes.clone()).collect()
}

pub(crate) fn list(
    source: SourceArg,
    selection: Selection,
    limits: LimitOverrides,
    span: Span,
) -> Result<StreamValue, RuntimeError> {
    let budget = budget(limits);
    let filters = selection
        .members
        .iter()
        .map(|member| engine::normalize(&member.bytes))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| archive_error(&error, Some(selection.format), span))?;
    let archive = open(source, &selection, &budget, span)?;
    Ok(StreamValue::from_live(
        "archive.list",
        ListStream {
            archive,
            filters,
            format: selection.format,
            finished: false,
            span,
        },
    ))
}

pub(crate) fn list_all(
    source: SourceArg,
    selection: Selection,
    limits: LimitOverrides,
    span: Span,
) -> Result<Vec<engine::EntryMeta>, RuntimeError> {
    let budget = budget(limits);
    let filters = selection
        .members
        .iter()
        .map(|member| engine::normalize(&member.bytes))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| archive_error(&error, Some(selection.format), span))?;
    let mut archive = open(source, &selection, &budget, span)?;
    let fail = |error: engine::Error| archive_error(&error, Some(selection.format), span);
    let mut listed = Vec::new();
    while let Some(entry) = archive.next_entry().map_err(fail)? {
        let meta = entry.meta().clone();
        drop(entry);
        let components = engine::normalize(&meta.name).map_err(fail)?;
        if filters.is_empty() || filters.iter().any(|filter| components.starts_with(filter)) {
            listed.push(meta);
        }
    }
    archive.finish(false).map_err(fail)?;
    Ok(listed)
}

pub(crate) fn entry_records(
    entries: &[engine::EntryMeta],
    span: Span,
) -> Result<Vec<Value>, RuntimeError> {
    entries.iter().map(|meta| entry_record(meta, span)).collect()
}

pub(crate) fn extract(request: ExtractRequest, span: Span) -> Result<(), RuntimeError> {
    let format = request.selection.format;
    let budget = budget(request.limits);
    let mut archive = open(request.source, &request.selection, &budget, span)?;
    let policy = engine::Policy {
        collision: request.collision,
        links: request.links,
        special: request.special,
        modes: request.modes,
        ownership: request.ownership,
        strip_components: request.strip_components,
        members: members(&request.selection.members),
    };
    engine::extract(&mut archive, &request.dest, &policy, &budget)
        .map(drop)
        .map_err(|error| archive_error(&error, Some(format), span))
}

pub(crate) fn create(request: CreateRequest, span: Span) -> Result<(), RuntimeError> {
    let budget = budget(LimitOverrides::default());
    let options = engine::CreateOptions {
        format: request.format,
        compression: request.compression,
        level: request.level,
        collision: request.collision,
    };
    engine::create(
        &request.dest,
        &request.root,
        &members(&request.entries),
        &options,
        &budget,
    )
    .map_err(|error| archive_error(&error, Some(request.format), span))
}

pub(crate) fn compress_file(
    source: PathBuf,
    dest: PathBuf,
    compression: engine::Compression,
    level: Option<i64>,
    collision: engine::Collision,
    span: Span,
) -> Result<(), RuntimeError> {
    engine::compress_file(
        &source,
        &dest,
        compression,
        level,
        collision,
        &budget(LimitOverrides::default()),
    )
    .map_err(|error| archive_error(&error, None, span))
}

pub(crate) fn decompress_file(
    source: PathBuf,
    dest: PathBuf,
    compression: engine::Compression,
    collision: engine::Collision,
    span: Span,
) -> Result<(), RuntimeError> {
    engine::decompress_file(
        &source,
        &dest,
        compression,
        collision,
        &budget(LimitOverrides::default()),
    )
    .map_err(|error| archive_error(&error, None, span))
}

const DEFAULT_IN_MEMORY_LIMIT: u64 = 256 << 20;

pub(crate) fn decompress_bytes(
    source: PathBuf,
    compression: engine::Compression,
    max_bytes: Option<u64>,
    span: Span,
) -> Result<Vec<u8>, RuntimeError> {
    let limits = LimitOverrides {
        max_output: Some(max_bytes.unwrap_or(DEFAULT_IN_MEMORY_LIMIT)),
        ..LimitOverrides::default()
    };
    engine::decompress_to_memory(&source, compression, &budget(limits))
        .map_err(|error| archive_error(&error, None, span))
}

fn open(
    source: SourceArg,
    selection: &Selection,
    budget: &Arc<engine::Budget>,
    span: Span,
) -> Result<engine::Archive, RuntimeError> {
    let fail = |error: engine::Error| archive_error(&error, Some(selection.format), span);
    let source = match source {
        SourceArg::Path(path) => {
            let file = std::fs::File::open(&path)
                .map_err(|error| fail(engine::Error::from_open(&error)))?;
            let name = {
                use std::os::unix::ffi::OsStrExt;
                path.as_os_str().as_bytes().to_vec()
            };
            engine::Source::from_file(file, Some(name)).map_err(fail)?
        }
        SourceArg::Bytes(bytes) => engine::Source::from_bytes(bytes),
        SourceArg::Reader(reader) => engine::Source::from_reader(reader),
    };
    engine::Archive::open(
        source,
        selection.format,
        selection.compression,
        Arc::clone(budget),
    )
    .map_err(fail)
}

struct ListStream {
    archive: engine::Archive,
    filters: Vec<Vec<Vec<u8>>>,
    format: engine::Format,
    finished: bool,
    span: Span,
}

impl LiveStream for ListStream {
    fn next(&mut self, span: Span) -> Result<Option<Value>, RuntimeError> {
        if self.finished {
            return Ok(None);
        }
        let fail = |error: engine::Error| archive_error(&error, Some(self.format), self.span);
        loop {
            let Some(entry) = self.archive.next_entry().map_err(fail)? else {
                self.finished = true;
                self.archive.finish(false).map_err(fail)?;
                return Ok(None);
            };
            let meta = entry.meta().clone();
            drop(entry);
            if !self.selected(&meta.name).map_err(fail)? {
                continue;
            }
            return entry_record(&meta, span).map(Some);
        }
    }
}

impl ListStream {
    fn selected(&self, name: &[u8]) -> Result<bool, engine::Error> {
        if self.filters.is_empty() {
            return Ok(true);
        }
        let components = engine::normalize(name)?;
        Ok(self
            .filters
            .iter()
            .any(|filter| components.starts_with(filter)))
    }
}

pub(crate) fn tag(enum_name: &str, variant: &str) -> Value {
    Value::Tag {
        type_name: Name::intern(enum_name),
        name: Arc::from(variant),
        fields: Vec::new(),
        wire: None,
    }
}

fn entry_record(meta: &engine::EntryMeta, span: Span) -> Result<Value, RuntimeError> {
    let optional_uint = |value: Option<u64>| value.map_or(Value::Null, |value| Value::Int(value as i64));
    let path = PathValue::new(meta.name.clone()).map_err(|error| error.with_span(span))?;
    let link = match &meta.link {
        Some(link) => Value::Path(PathValue::new(link.clone()).map_err(|error| error.with_span(span))?),
        None => Value::Null,
    };
    Ok(Value::Record(RecordMap::from([
        (Arc::from("path"), Value::Path(path)),
        (
            Arc::from("kind"),
            tag(xsh_registry::enums::ARCHIVE_ENTRY_KIND, meta.kind.name()),
        ),
        (Arc::from("size"), optional_uint(meta.size)),
        (Arc::from("mode"), optional_uint(meta.mode.map(u64::from))),
        (
            Arc::from("modified"),
            meta.modified.map_or(Value::Null, Value::Int),
        ),
        (Arc::from("uid"), optional_uint(meta.uid.map(u64::from))),
        (Arc::from("gid"), optional_uint(meta.gid.map(u64::from))),
        (Arc::from("link_name"), link),
        (
            Arc::from("device_major"),
            optional_uint(meta.device.map(|(major, _)| u64::from(major))),
        ),
        (
            Arc::from("device_minor"),
            optional_uint(meta.device.map(|(_, minor)| u64::from(minor))),
        ),
    ])))
}

pub(crate) fn archive_error(
    error: &engine::Error,
    format: Option<engine::Format>,
    span: Span,
) -> RuntimeError {
    let variant = error.kind().name();
    let facets = match error.kind() {
        engine::ErrorKind::Open | engine::ErrorKind::Io => {
            let facet = error.io_kind().map_or(
                xsh_registry::errors::ErrorFacet::HostIo,
                xsh_registry::errors::ErrorFacet::of_host_io,
            );
            vec![facet.name().to_string()]
        }
        _ => xsh_registry::errors::ARCHIVE_ERROR_VARIANTS
            .iter()
            .find(|candidate| candidate.name == variant)
            .expect("archive error variant")
            .facets
            .iter()
            .map(|facet| facet.name().to_string())
            .collect(),
    };
    let entry = error
        .entry()
        .and_then(|bytes| PathValue::new(bytes.to_vec()).ok())
        .map_or(Value::Null, Value::Path);
    let payload = RecordMap::from([
        (Arc::from("message"), Value::Str(error.message().into())),
        (
            Arc::from("format"),
            format.map_or(Value::Null, |format| {
                tag(xsh_registry::enums::ARCHIVE_FORMAT, format_variant(format))
            }),
        ),
        (Arc::from("entry"), entry),
        (
            Arc::from("offset"),
            error.offset().map_or(Value::Null, |offset| Value::Int(offset as i64)),
        ),
    ]);
    RuntimeError::structured("ArchiveError", variant, payload, facets, error.to_string())
        .with_span(span)
}

fn format_variant(format: engine::Format) -> &'static str {
    match format {
        engine::Format::Tar => "Tar",
        engine::Format::Zip => "Zip",
        engine::Format::Cpio => "Cpio",
    }
}
