use super::{
    Evaluator, LoweredValue, NativeArgumentValues, RuntimeError, Span, archive_module,
    lowered_path_arg, lowered_path_list_arg, lowered_runtime_result, lowered_runtime_stream_result,
    lowered_unit_result,
};
use crate::modules::archive::{
    CreateRequest, ExtractRequest, LimitOverrides, Selection, SourceArg,
};
use crate::runtime::value::{StreamValue, Value};
use crate::symbol::Name;
use std::io::Read;
use xsh_archive as engine;
use xsh_registry::enums::{
    ARCHIVE_COLLISION, ARCHIVE_COMPRESSION, ARCHIVE_FORMAT, ARCHIVE_LINKS, ARCHIVE_MODES,
    ARCHIVE_OWNERSHIP, ARCHIVE_SPECIAL,
};

fn variant_arg<T>(
    value: Option<&LoweredValue>,
    enum_name: &'static str,
    default: Option<T>,
    parse: fn(&str) -> Option<T>,
    operation: &str,
    span: Span,
) -> Result<T, RuntimeError> {
    let Some(value) = value else {
        return default.ok_or_else(|| {
            RuntimeError::new("type-error", format!("{operation} requires a {enum_name} argument"))
                .with_span(span)
        });
    };
    match value {
        LoweredValue::Tag(tag) if tag.type_name == Name::intern(enum_name) => {
            parse(&tag.name).ok_or_else(|| {
                RuntimeError::new(
                    "type-error",
                    format!("{operation} does not accept {enum_name} variant {}", tag.name),
                )
                .with_span(span)
            })
        }
        other => Err(RuntimeError::new(
            "type-error",
            format!("{operation} expected {enum_name}, found {}", other.type_name()),
        )
        .with_span(span)),
    }
}

fn parse_format(name: &str) -> Option<engine::Format> {
    Some(match name {
        "Tar" => engine::Format::Tar,
        "Zip" => engine::Format::Zip,
        "Cpio" => engine::Format::Cpio,
        _ => return None,
    })
}

fn parse_compression(name: &str) -> Option<engine::Compression> {
    Some(match name {
        "Auto" => engine::Compression::Auto,
        "Plain" => engine::Compression::Plain,
        "Gzip" => engine::Compression::Codec(engine::Codec::Gzip),
        "Bzip2" => engine::Compression::Codec(engine::Codec::Bzip2),
        "Xz" => engine::Compression::Codec(engine::Codec::Xz),
        "Lzma" => engine::Compression::Codec(engine::Codec::Lzma),
        "Zstd" => engine::Compression::Codec(engine::Codec::Zstd),
        _ => return None,
    })
}

fn parse_collision(name: &str) -> Option<engine::Collision> {
    Some(match name {
        "Refuse" => engine::Collision::Refuse,
        "Replace" => engine::Collision::Replace,
        _ => return None,
    })
}

fn parse_links(name: &str) -> Option<engine::Links> {
    Some(match name {
        "Refuse" => engine::Links::Refuse,
        "Contained" => engine::Links::Contained,
        "Verbatim" => engine::Links::Verbatim,
        _ => return None,
    })
}

fn parse_special(name: &str) -> Option<engine::Special> {
    Some(match name {
        "Refuse" => engine::Special::Refuse,
        "Create" => engine::Special::Create,
        _ => return None,
    })
}

fn parse_modes(name: &str) -> Option<engine::Modes> {
    Some(match name {
        "Plain" => engine::Modes::Plain,
        "Exact" => engine::Modes::Exact,
        _ => return None,
    })
}

fn parse_ownership(name: &str) -> Option<engine::Ownership> {
    Some(match name {
        "Ignore" => engine::Ownership::Ignore,
        "Apply" => engine::Ownership::Apply,
        _ => return None,
    })
}

fn optional_unsigned(
    value: Option<&LoweredValue>,
    operation: &str,
    span: Span,
) -> Result<Option<u64>, RuntimeError> {
    match value {
        None | Some(LoweredValue::Null) => Ok(None),
        Some(LoweredValue::Int(value)) => u64::try_from(*value).map(Some).map_err(|_| {
            RuntimeError::new("type-error", format!("{operation} expected a nonnegative Int"))
                .with_span(span)
        }),
        Some(other) => Err(RuntimeError::new(
            "type-error",
            format!("{operation} expected Int?, found {}", other.type_name()),
        )
        .with_span(span)),
    }
}

fn optional_level(
    value: Option<&LoweredValue>,
    operation: &str,
    span: Span,
) -> Result<Option<i64>, RuntimeError> {
    match value {
        None | Some(LoweredValue::Null) => Ok(None),
        Some(LoweredValue::Int(value)) => Ok(Some(*value)),
        Some(other) => Err(RuntimeError::new(
            "type-error",
            format!("{operation} expected Int?, found {}", other.type_name()),
        )
        .with_span(span)),
    }
}

enum Input {
    Direct(SourceArg),
    Stream(StreamValue),
}

struct ChannelReader {
    chunks: crossbeam_channel::Receiver<Vec<u8>>,
    current: Vec<u8>,
    position: usize,
}

impl Read for ChannelReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        while self.position >= self.current.len() {
            match self.chunks.recv() {
                Ok(chunk) => {
                    self.current = chunk;
                    self.position = 0;
                }
                Err(_) => return Ok(0),
            }
        }
        let count = buffer.len().min(self.current.len() - self.position);
        buffer[..count].copy_from_slice(&self.current[self.position..self.position + count]);
        self.position += count;
        Ok(count)
    }
}

impl Evaluator {
    fn pump<T: Send + 'static>(
        &mut self,
        mut stream: StreamValue,
        span: Span,
        work: impl FnOnce(ChannelReader) -> Result<T, RuntimeError> + Send + 'static,
    ) -> Result<Result<T, RuntimeError>, RuntimeError> {
        let (sender, receiver) = crossbeam_channel::bounded::<Vec<u8>>(8);
        let worker = std::thread::Builder::new()
            .name("archive".to_string())
            .spawn(move || {
                work(ChannelReader {
                    chunks: receiver,
                    current: Vec::new(),
                    position: 0,
                })
            })
            .map_err(|error| RuntimeError::host("archive-worker", &error).with_span(span))?;
        let mut pull_error = None;
        let mut exhausted = false;
        loop {
            match self.stream_next(&mut stream, span) {
                Ok(Some(Value::Bytes(chunk))) => {
                    if sender.send(chunk).is_err() {
                        break;
                    }
                }
                Ok(Some(other)) => {
                    pull_error = Some(
                        RuntimeError::new(
                            "type-error",
                            format!("an archive stream must yield Bytes, found {}", other.type_name()),
                        )
                        .with_span(span),
                    );
                    break;
                }
                Ok(None) => {
                    exhausted = true;
                    break;
                }
                Err(error) => {
                    pull_error = Some(error);
                    break;
                }
            }
        }
        drop(sender);
        if !exhausted {
            let _ = self.stream_cancel(&mut stream, span);
        }
        let outcome = worker.join().map_err(|_| {
            RuntimeError::new("archive-worker", "the archive worker thread panicked").with_span(span)
        })?;
        match pull_error {
            Some(error) => Err(error),
            None => Ok(outcome),
        }
    }

    fn archive_source(
        &self,
        value: LoweredValue,
        operation: &str,
        span: Span,
    ) -> Result<Input, RuntimeError> {
        match value {
            LoweredValue::Path(path) => Ok(Input::Direct(SourceArg::Path(self.host_path(&path)))),
            LoweredValue::Bytes(bytes) => Ok(Input::Direct(SourceArg::Bytes(bytes.to_vec()))),
            LoweredValue::BytesView(view) => Ok(Input::Direct(SourceArg::Bytes(view.as_slice().to_vec()))),
            LoweredValue::Stream(stream) => Ok(Input::Stream(*stream)),
            other => Err(RuntimeError::new(
                "type-error",
                format!(
                    "{operation} expected a Path, Bytes or Stream[Bytes] source, found {}",
                    other.type_name()
                ),
            )
            .with_span(span)),
        }
    }

    fn archive_selection(
        &self,
        values: &NativeArgumentValues,
        format_index: usize,
        operation: &str,
        span: Span,
    ) -> Result<Selection, RuntimeError> {
        let members = match values.get(format_index + 2) {
            Some(list) => lowered_path_list_arg(list.clone(), operation, span)?,
            None => Vec::new(),
        };
        Ok(Selection {
            format: variant_arg(values.get(format_index), ARCHIVE_FORMAT, None, parse_format, operation, span)?,
            compression: variant_arg(
                values.get(format_index + 1),
                ARCHIVE_COMPRESSION,
                Some(engine::Compression::Auto),
                parse_compression,
                operation,
                span,
            )?,
            members,
        })
    }

    pub(super) fn eval_lowered_archive_list(
        &mut self,
        values: NativeArgumentValues,
        span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        let source = self.archive_source(values[0].clone(), "archive.list", span)?;
        let selection = self.archive_selection(&values, 1, "archive.list", span)?;
        let limits = LimitOverrides {
            max_output: optional_unsigned(values.get(4), "archive.list", span)?,
            max_entry: optional_unsigned(values.get(5), "archive.list", span)?,
            max_entries: optional_unsigned(values.get(6), "archive.list", span)?,
        };
        match source {
            Input::Direct(source) => lowered_runtime_stream_result(
                archive_module::list(source, selection, limits, span),
                span,
            ),
            Input::Stream(stream) => {
                let listed = self.pump(stream, span, move |reader| {
                    archive_module::list_all(
                        SourceArg::Reader(Box::new(reader)),
                        selection,
                        limits,
                        span,
                    )
                })?;
                let records = listed.and_then(|entries| archive_module::entry_records(&entries, span));
                lowered_runtime_stream_result(records.map(StreamValue::from_values), span)
            }
        }
    }

    pub(super) fn eval_lowered_archive_extract(
        &mut self,
        values: NativeArgumentValues,
        span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        const OP: &str = "archive.extract";
        let source = self.archive_source(values[0].clone(), OP, span)?;
        let dest = self.host_path(&lowered_path_arg(values[1].clone(), OP, span)?);
        let selection = self.archive_selection(&values, 2, OP, span)?;
        let strip_components = match optional_unsigned(values.get(5), OP, span)? {
            Some(count) => usize::try_from(count).map_err(|_| {
                RuntimeError::new("type-error", format!("{OP} strip_components is too large"))
                    .with_span(span)
            })?,
            None => 0,
        };
        let mut request = ExtractRequest {
            source: SourceArg::Bytes(Vec::new()),
            dest,
            selection,
            strip_components,
            collision: variant_arg(values.get(6), ARCHIVE_COLLISION, Some(engine::Collision::Refuse), parse_collision, OP, span)?,
            links: variant_arg(values.get(7), ARCHIVE_LINKS, Some(engine::Links::Contained), parse_links, OP, span)?,
            special: variant_arg(values.get(8), ARCHIVE_SPECIAL, Some(engine::Special::Refuse), parse_special, OP, span)?,
            modes: variant_arg(values.get(9), ARCHIVE_MODES, Some(engine::Modes::Plain), parse_modes, OP, span)?,
            ownership: variant_arg(values.get(10), ARCHIVE_OWNERSHIP, Some(engine::Ownership::Ignore), parse_ownership, OP, span)?,
            limits: LimitOverrides {
                max_output: optional_unsigned(values.get(11), OP, span)?,
                max_entry: optional_unsigned(values.get(12), OP, span)?,
                max_entries: optional_unsigned(values.get(13), OP, span)?,
            },
        };
        let result = match source {
            Input::Direct(source) => {
                request.source = source;
                archive_module::extract(request, span)
            }
            Input::Stream(stream) => self.pump(stream, span, move |reader| {
                request.source = SourceArg::Reader(Box::new(reader));
                archive_module::extract(request, span)
            })?,
        };
        Ok(lowered_unit_result(result))
    }

    pub(super) fn eval_lowered_archive_create(
        &mut self,
        values: NativeArgumentValues,
        span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        const OP: &str = "archive.create";
        let request = CreateRequest {
            dest: self.host_path(&lowered_path_arg(values[0].clone(), OP, span)?),
            root: self.host_path(&lowered_path_arg(values[1].clone(), OP, span)?),
            entries: lowered_path_list_arg(values[2].clone(), OP, span)?,
            format: variant_arg(values.get(3), ARCHIVE_FORMAT, None, parse_format, OP, span)?,
            compression: variant_arg(values.get(4), ARCHIVE_COMPRESSION, Some(engine::Compression::Auto), parse_compression, OP, span)?,
            level: optional_level(values.get(5), OP, span)?,
            collision: variant_arg(values.get(6), ARCHIVE_COLLISION, Some(engine::Collision::Refuse), parse_collision, OP, span)?,
        };
        Ok(lowered_unit_result(archive_module::create(request, span)))
    }

    pub(super) fn eval_lowered_archive_compress(
        &mut self,
        values: NativeArgumentValues,
        span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        const OP: &str = "archive.compress";
        let source = self.host_path(&lowered_path_arg(values[0].clone(), OP, span)?);
        let dest = self.host_path(&lowered_path_arg(values[1].clone(), OP, span)?);
        let compression = variant_arg(values.get(2), ARCHIVE_COMPRESSION, Some(engine::Compression::Auto), parse_compression, OP, span)?;
        let level = optional_level(values.get(3), OP, span)?;
        let collision = variant_arg(values.get(4), ARCHIVE_COLLISION, Some(engine::Collision::Refuse), parse_collision, OP, span)?;
        Ok(lowered_unit_result(archive_module::compress_file(
            source, dest, compression, level, collision, span,
        )))
    }

    pub(super) fn eval_lowered_archive_decompress(
        &mut self,
        values: NativeArgumentValues,
        span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        const OP: &str = "archive.decompress";
        let source = self.host_path(&lowered_path_arg(values[0].clone(), OP, span)?);
        let dest = self.host_path(&lowered_path_arg(values[1].clone(), OP, span)?);
        let compression = variant_arg(values.get(2), ARCHIVE_COMPRESSION, Some(engine::Compression::Auto), parse_compression, OP, span)?;
        let collision = variant_arg(values.get(3), ARCHIVE_COLLISION, Some(engine::Collision::Refuse), parse_collision, OP, span)?;
        Ok(lowered_unit_result(archive_module::decompress_file(
            source, dest, compression, collision, span,
        )))
    }

    pub(super) fn eval_lowered_archive_decompress_bytes(
        &mut self,
        values: NativeArgumentValues,
        span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        const OP: &str = "archive.decompress_bytes";
        let source = self.host_path(&lowered_path_arg(values[0].clone(), OP, span)?);
        let compression = variant_arg(values.get(1), ARCHIVE_COMPRESSION, Some(engine::Compression::Auto), parse_compression, OP, span)?;
        let max_bytes = optional_unsigned(values.get(2), OP, span)?;
        lowered_runtime_result(
            archive_module::decompress_bytes(source, compression, max_bytes, span).map(Value::Bytes),
            span,
        )
    }
}
