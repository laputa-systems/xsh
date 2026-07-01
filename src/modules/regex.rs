use crate::runtime::value::RuntimeError;
use crate::source::Span;

#[allow(clippy::single_call_fn)]
pub(crate) fn compile(pattern: &str, span: Span) -> Result<regex_lite::Regex, RuntimeError> {
    regex_lite::Regex::new(pattern)
        .map_err(|error| RuntimeError::new("regex-compile", error.to_string()).with_span(span))
}
