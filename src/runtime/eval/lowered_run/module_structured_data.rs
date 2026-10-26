use super::{Evaluator, ControlFlow, LoweredValue, NativeArgumentValues, RuntimeError, RuntimeOp,
    Span, lowered_bool_arg_or, lowered_bytes_value, lowered_path_arg, lowered_result_err_value,
    lowered_result_ok, lowered_runtime_result, lowered_runtime_value, lowered_str_value, lowered_unit_result};
use crate::modules::{codec, fs};
use codec::Format;

#[derive(Clone, Copy)]
enum Operation { Decode, DecodeBytes, Encode, EncodeBytes, Read, Write }

fn codec_input_error(mut error: RuntimeError, input_path: Option<&crate::runtime::value::PathValue>) -> RuntimeError {
    if error.family == "CodecError" && let Some(path) = input_path {
        error.payload.insert("input_path".into(), crate::runtime::value::Value::Path(path.clone()));
    }
    error
}

fn sequence_text(value: LoweredValue, format: Format, span: Span) -> Result<std::sync::Arc<str>, RuntimeError> {
    let text = lowered_str_value(&value).ok_or_else(|| RuntimeError::new("type-error", "codec.decode_all expected Str").with_span(span))?;
    if text.len() > codec::INPUT_LIMIT { return Err(codec::failure("Limit", format.identifier(), "codec input exceeds 64 MiB", span)); }
    Ok(match value { LoweredValue::Str(text) => text, LoweredValue::StrView(text) => text.as_str().into(), _ => unreachable!("checked sequence text") })
}

fn sequence_bytes(value: LoweredValue, format: Format, span: Span) -> Result<std::sync::Arc<[u8]>, RuntimeError> {
    let bytes = lowered_bytes_value(&value).ok_or_else(|| RuntimeError::new("type-error", "codec.decode_all_bytes expected Bytes").with_span(span))?;
    if bytes.len() > codec::INPUT_LIMIT { return Err(codec::failure("Limit", format.identifier(), "codec input exceeds 64 MiB", span)); }
    Ok(match value { LoweredValue::Bytes(bytes) => bytes, LoweredValue::BytesView(bytes) => bytes.as_slice().into(), _ => unreachable!("checked sequence bytes") })
}

fn csv_columns(value: &LoweredValue, span: Span) -> Result<Vec<String>, RuntimeError> {
    let values = match value { LoweredValue::List(values) => values.as_slice(), LoweredValue::SharedList(values) => values.as_slice(), _ => return Err(RuntimeError::new("type-error", "csv columns expected List[Str]").with_span(span)) };
    let limit = || codec::failure("Limit", "csv", "CSV columns exceed codec container or scalar limits", span);
    if values.len() > 1_000_000 { return Err(limit()); }
    let mut bytes = 0usize;
    for value in values {
        let text = lowered_str_value(value).ok_or_else(|| RuntimeError::new("type-error", "csv column expected Str").with_span(span))?;
        if text.len() > 16 * 1024 * 1024 { return Err(limit()); }
        bytes = bytes.checked_add(text.len()).filter(|bytes| *bytes <= codec::OUTPUT_LIMIT).ok_or_else(limit)?;
    }
    Ok(values.iter().map(|value| lowered_str_value(value).expect("checked CSV column").to_owned()).collect())
}

impl Evaluator {
    pub(super) fn eval_lowered_codec_schema(&mut self, op: RuntimeOp, format: LoweredValue, input: LoweredValue, check: &super::super::LoweredTypeCheck, span: Span) -> Result<LoweredValue, RuntimeError> {
        let identifier = lowered_str_value(&format).ok_or_else(|| RuntimeError::new("type-error", "typed codec format expected Str").with_span(span))?;
        let context_path = if op == RuntimeOp::CodecReadAs { Some(lowered_path_arg(input.clone(), "codec.read_as", span)?) } else { None };
        let contextualize = |error: RuntimeError| codec_input_error(error, context_path.as_ref());
        let format = match Format::parse(identifier, span) { Ok(format) => format, Err(error) => return Ok(lowered_result_err_value(contextualize(error))) };
        if !format.supports_lexical() { return Ok(lowered_result_err_value(contextualize(codec::failure("UnsupportedFormat", identifier, "typed lexical decoding requires CSV, TSV, or INI", span)))); }
        let result = if op == RuntimeOp::CodecDecodeAs {
            let text = lowered_str_value(&input).ok_or_else(|| RuntimeError::new("type-error", "typed codec input expected Str").with_span(span))?;
            super::super::require::decode_lexical(self, format, text, check, span)
        } else {
            let path = lowered_path_arg(input, "codec.read_as", span)?;
            let result = (|| {
                let bytes = codec::read_input(&self.host_path(&path), span)?;
                if bytes.len() > codec::INPUT_LIMIT { return Err(codec::failure("Limit", format.identifier(), "codec input exceeds 64 MiB", span)); }
                let text = std::str::from_utf8(&bytes).map_err(|error| {
                    let mut failure = codec::failure("Encoding", format.identifier(), "invalid UTF-8", span);
                    failure.payload.insert("offset".into(), crate::runtime::value::Value::Int(error.valid_up_to() as i64));
                    failure
                })?;
                super::super::require::decode_lexical(self, format, text, check, span)
            })();
            result.map_err(|error| codec_input_error(error, Some(&path)))
        };
        Ok(match result { Ok(value) => lowered_result_ok(value), Err(error) => lowered_result_err_value(error) })
    }

    pub(super) fn eval_lowered_csv_values(&mut self, op: RuntimeOp, mut values: NativeArgumentValues, span: Span) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        use codec::table;
        let write = op == RuntimeOp::CsvWrite;
        let encode = write || op == RuntimeOp::CsvEncode;
        let context_path = if op == RuntimeOp::CsvRead { Some(lowered_path_arg(values[0].clone(), "csv.read", span)?) } else { None };
        let option = if write { 2 } else { 1 };
        let headers = lowered_bool_arg_or(values.get(option).cloned(), true, "csv", span)?;
        let delimiter = values.get(option + 1).and_then(lowered_str_value).unwrap_or(",");
        let delimiter = match table::parse_delimiter(delimiter, span) { Ok(delimiter) => delimiter, Err(error) => return Ok(ControlFlow::Continue(lowered_result_err_value(codec_input_error(error, context_path.as_ref())))) };
        let text = if encode {
            let columns = match values.get(option + 2) {
                None | Some(LoweredValue::Null) => None,
                Some(value) => match csv_columns(value, span) { Ok(columns) => Some(columns), Err(error) => return Ok(ControlFlow::Continue(lowered_result_err_value(error))) },
            };
            let escape_formulas = lowered_bool_arg_or(values.get(option + 3).cloned(), false, "csv", span)?;
            let value = super::super::json::borrowed_lowered(Format::Csv, &values[usize::from(write)], span);
            table::encode(Format::Csv, &value, table::EncodeOptions { headers, delimiter, columns, escape_formulas }, span)
        } else {
            let input_path = context_path;
            let bytes = match &input_path { Some(path) => Some(codec::read_input(&self.host_path(path), span)), None => None };
            let result = (|| {
                let owned;
                let text = match bytes { Some(bytes) => {
                    owned = bytes?;
                    if owned.len() > codec::INPUT_LIMIT { return Err(codec::failure("Limit", "csv", "codec input exceeds 64 MiB", span)); }
                    std::str::from_utf8(&owned).map_err(|error| {
                        let mut failure = codec::failure("Encoding", "csv", "invalid UTF-8", span);
                        failure.payload.insert("offset".into(), crate::runtime::value::Value::Int(error.valid_up_to() as i64)); failure
                    })?
                }, None => lowered_str_value(&values[0]).ok_or_else(|| RuntimeError::new("type-error", "csv.decode expected Str").with_span(span))? };
                let mut out = None::<codec::Decoded>;
                { let mut driver = deser::de::DeserializeDriver::new(&mut out);
                    table::drive(Format::Csv, text, table::DecodeOptions { headers, delimiter }, &mut driver, span)?;
                }
                Ok(out.expect("CSV document sink").0)
            })().map_err(|error| codec_input_error(error, input_path.as_ref()));
            return Ok(ControlFlow::Continue(lowered_runtime_result(result, span)?));
        };
        let result = if write {
            let overwrite = lowered_bool_arg_or(values.get(option + 4).cloned(), false, "csv.write", span)?;
            let path = lowered_path_arg(values.remove(0), "csv.write", span)?;
            match text { Ok(text) => lowered_unit_result(if overwrite { fs::write_path(self.host_path(&path), text.as_bytes(), span) } else { fs::create_path(self.host_path(&path), text.as_bytes(), span) }), Err(error) => lowered_result_err_value(error) }
        } else { match text { Ok(text) => lowered_result_ok(LoweredValue::Str(text.into())), Err(error) => lowered_result_err_value(error) } };
        Ok(ControlFlow::Continue(result))
    }

    pub(super) fn eval_lowered_codec_sequence(&mut self, op: RuntimeOp, mut values: NativeArgumentValues, span: Span) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let input_path = if op == RuntimeOp::CodecReadAll { Some(lowered_path_arg(values[1].clone(), "codec.read_all", span)?) } else { None };
        let identifier = lowered_str_value(&values[0]).ok_or_else(|| RuntimeError::new("type-error", "codec format expected Str").with_span(span))?;
        let format = match Format::parse(identifier, span) {
            Ok(format) => format,
            Err(error) => return Ok(ControlFlow::Continue(lowered_result_err_value(codec_input_error(error, input_path.as_ref())))),
        };
        let text_only = matches!(op, RuntimeOp::CodecDecodeAll | RuntimeOp::CodecEncodeAll);
        let result = (|| -> Result<LoweredValue, RuntimeError> {
            if !format.supports_sequence() || (text_only && format.is_binary()) {
                return Err(codec::failure("UnsupportedFormat", format.identifier(), "format does not support this sequence operation", span));
            }
            match op {
                RuntimeOp::CodecDecodeAll => {
                    let text = sequence_text(values.remove(1), format, span)?;
                    codec::sequence::decode_text(format, text, span).map(|stream| LoweredValue::Stream(Box::new(stream)))
                }
                RuntimeOp::CodecDecodeAllBytes => {
                    let bytes = sequence_bytes(values.remove(1), format, span)?;
                    codec::sequence::decode_bytes(format, bytes, span).map(|stream| LoweredValue::Stream(Box::new(stream)))
                }
                RuntimeOp::CodecReadAll => {
                    let path = input_path.as_ref().expect("file sequence path");
                    let file = codec::open_input(&self.host_path(path), span)?;
                    codec::sequence::read_all(format, file, path.clone(), span).map(|stream| LoweredValue::Stream(Box::new(stream)))
                }
                RuntimeOp::CodecEncodeAll | RuntimeOp::CodecEncodeAllBytes => {
                    let items = match &values[1] {
                        LoweredValue::List(items) => items.as_slice(),
                        LoweredValue::SharedList(items) => items.as_slice(),
                        _ => return Err(RuntimeError::new("type-error", "codec sequence encoding expected List[Any]").with_span(span)),
                    };
                    if items.len() > 1_000_000 { return Err(codec::failure("Limit", format.identifier(), "codec sequence exceeds 1000000 items", span)); }
                    let bytes = codec::sequence::encode_all(format, items.iter().map(|value| super::super::json::borrowed_lowered(format, value, span)), span)?;
                    Ok(if op == RuntimeOp::CodecEncodeAll {
                        LoweredValue::Str(String::from_utf8(bytes).expect("text sequence serializer UTF-8").into())
                    } else { LoweredValue::Bytes(bytes.into()) })
                }
                _ => unreachable!("checked codec sequence operation"),
            }
        })().map_err(|error| codec_input_error(error, input_path.as_ref()));
        Ok(ControlFlow::Continue(match result { Ok(value) => lowered_result_ok(value), Err(error) => lowered_result_err_value(error) }))
    }

    pub(super) fn eval_lowered_codec_values(&mut self, op: RuntimeOp, mut values: NativeArgumentValues, span: Span) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        if op == RuntimeOp::CodecFormats {
            return Ok(ControlFlow::Continue(lowered_runtime_value(codec::formats(), span)?));
        }
        let (fixed, operation) = match op {
            RuntimeOp::CodecDecode => (None, Operation::Decode),
            RuntimeOp::CodecDecodeBytes => (None, Operation::DecodeBytes),
            RuntimeOp::CodecEncode => (None, Operation::Encode),
            RuntimeOp::CodecEncodeBytes => (None, Operation::EncodeBytes),
            RuntimeOp::CodecRead => (None, Operation::Read),
            RuntimeOp::CodecWrite => (None, Operation::Write),
            RuntimeOp::YamlDecode => (Some(Format::Yaml), Operation::Decode),
            RuntimeOp::YamlEncode => (Some(Format::Yaml), Operation::Encode),
            RuntimeOp::YamlRead => (Some(Format::Yaml), Operation::Read),
            RuntimeOp::YamlWrite => (Some(Format::Yaml), Operation::Write),
            RuntimeOp::TomlDecode => (Some(Format::Toml), Operation::Decode),
            RuntimeOp::TomlEncode => (Some(Format::Toml), Operation::Encode),
            RuntimeOp::TomlRead => (Some(Format::Toml), Operation::Read),
            RuntimeOp::TomlWrite => (Some(Format::Toml), Operation::Write),
            RuntimeOp::IniDecode => (Some(Format::Ini), Operation::Decode),
            RuntimeOp::IniEncode => (Some(Format::Ini), Operation::Encode),
            RuntimeOp::IniRead => (Some(Format::Ini), Operation::Read),
            RuntimeOp::IniWrite => (Some(Format::Ini), Operation::Write),
            _ => unreachable!("checked codec operation"),
        };
        let context_path = if matches!(operation, Operation::Read) { Some(lowered_path_arg(values[usize::from(fixed.is_none())].clone(), "codec.read", span)?) } else { None };
        let format = match fixed {
            Some(format) => format,
            None => {
                let name = lowered_str_value(&values[0]).ok_or_else(|| RuntimeError::new("type-error", "codec format expected Str").with_span(span))?;
                let parsed = Format::parse(name, span);
                values.remove(0);
                match parsed { Ok(format) => format, Err(error) => return Ok(ControlFlow::Continue(lowered_result_err_value(codec_input_error(error, context_path.as_ref())))) }
            }
        };
        let result = match operation {
            Operation::Decode => {
                let text = lowered_str_value(&values[0]).ok_or_else(|| RuntimeError::new("type-error", "codec.decode expected Str").with_span(span))?;
                lowered_runtime_result(codec::decode_text(format, text, span), span)?
            }
            Operation::DecodeBytes => {
                let bytes = lowered_bytes_value(&values[0]).ok_or_else(|| RuntimeError::new("type-error", "codec.decode_bytes expected Bytes").with_span(span))?;
                lowered_runtime_result(codec::decode_bytes(format, bytes, span), span)?
            }
            Operation::Encode => {
                let value = super::super::json::borrowed_lowered(format, &values[0], span);
                match codec::encode_text(format, &value, span) {
                    Ok(text) => lowered_result_ok(LoweredValue::Str(text.into())),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            Operation::EncodeBytes => {
                let canonical = lowered_bool_arg_or(values.get(1).cloned(), false, "codec.encode_bytes", span)?;
                let value = super::super::json::borrowed_lowered(format, &values[0], span);
                match codec::encode_bytes(format, &value, canonical, span) {
                    Ok(bytes) => lowered_result_ok(LoweredValue::Bytes(bytes.into())),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            Operation::Read => {
                let path = lowered_path_arg(values.remove(0), "codec.read", span)?;
                lowered_runtime_result(codec::read(format, &self.host_path(&path), &path, span), span)?
            }
            Operation::Write => {
                let overwrite = lowered_bool_arg_or(values.get(2).cloned(), op == RuntimeOp::IniWrite, "codec.write", span)?;
                let path = lowered_path_arg(values.remove(0), "codec.write", span)?;
                let value = super::super::json::borrowed_lowered(format, &values[0], span);
                match codec::encode_bytes(format, &value, false, span) {
                    Ok(bytes) => lowered_unit_result(if overwrite { fs::write_path(self.host_path(&path), &bytes, span) } else { fs::create_path(self.host_path(&path), &bytes, span).map_err(|error| {
                        if op == RuntimeOp::IniWrite && error.errno() == Some(i64::from(libc::EEXIST)) {
                            RuntimeError::new("ini-write", "destination exists").with_span(span)
                        } else { error }
                    }) }),
                    Err(error) => lowered_result_err_value(error),
                }
            }
        };
        Ok(ControlFlow::Continue(result))
    }
}
