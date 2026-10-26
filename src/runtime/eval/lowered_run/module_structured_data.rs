use super::{Evaluator, ControlFlow, LoweredValue, NativeArgumentValues, RuntimeError, RuntimeOp,
    Span, lowered_bool_arg_or, lowered_bytes_value, lowered_path_arg, lowered_result_err_value,
    lowered_result_ok, lowered_runtime_result, lowered_runtime_value, lowered_str_value, lowered_unit_result};
use crate::modules::{codec, fs};
use codec::Format;

#[derive(Clone, Copy)]
enum Operation { Decode, DecodeBytes, Encode, EncodeBytes, Read, Write }

impl Evaluator {
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
            _ => unreachable!("checked codec operation"),
        };
        let format = match fixed {
            Some(format) => format,
            None => {
                let name = lowered_str_value(&values[0]).ok_or_else(|| RuntimeError::new("type-error", "codec format expected Str").with_span(span))?;
                let parsed = Format::parse(name, span);
                values.remove(0);
                match parsed { Ok(format) => format, Err(error) => return Ok(ControlFlow::Continue(lowered_result_err_value(error))) }
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
                let overwrite = lowered_bool_arg_or(values.get(2).cloned(), false, "codec.write", span)?;
                let path = lowered_path_arg(values.remove(0), "codec.write", span)?;
                let value = super::super::json::borrowed_lowered(format, &values[0], span);
                match codec::encode_bytes(format, &value, false, span) {
                    Ok(bytes) => lowered_unit_result(if overwrite { fs::write_path(self.host_path(&path), &bytes, span) } else { fs::create_path(self.host_path(&path), &bytes, span) }),
                    Err(error) => lowered_result_err_value(error),
                }
            }
        };
        Ok(ControlFlow::Continue(result))
    }
}
