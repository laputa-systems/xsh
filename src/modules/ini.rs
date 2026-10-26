use crate::modules::codec::{config, Format};
use crate::runtime::value::{RuntimeError, Value};
use crate::source::Span;

pub fn decode(text: &str, span: Span) -> Result<Value, RuntimeError> {
    config::decode(Format::Ini, text, span)
}
