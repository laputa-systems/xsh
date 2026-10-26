use crate::modules::codec::{config, Format};
use crate::runtime::value::{RuntimeError, Value};
use crate::source::Span;

pub fn decode(text: &str, span: Span) -> Result<Value, RuntimeError> {
    let symbols = crate::symbol::SymbolOwner::new();
    let _scope = symbols.enter();
    config::decode(Format::Ini, text, span)
}
