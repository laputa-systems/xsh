use crate::modules::codec::{config, Format};
use crate::runtime::value::{RecordMap, RuntimeError, Value};
use crate::source::Span;

pub fn decode(text: &str, span: Span) -> Result<Value, RuntimeError> {
    config::decode(Format::Ini, text, span)
}

pub(crate) fn encode(value: &RecordMap, span: Span) -> Result<String, RuntimeError> {
    config::encode(Format::Ini, &crate::runtime::eval::borrowed_codec_record(Format::Ini, value, span), span)
}
