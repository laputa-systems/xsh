use crate::runtime::value::{RuntimeError, Value};
use crate::source::Span;
use diffy::DiffOptions;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

#[allow(clippy::single_call_fn)]
pub(crate) fn unified(
    original: PathBuf,
    modified: PathBuf,
    context: i64,
    span: Span,
) -> Result<Value, RuntimeError> {
    if context < 0 {
        return Err(RuntimeError::new("diff", "context cannot be negative").with_span(span));
    }
    let original_text =
        fs::read_to_string(&original).map_err(|error| diff_error("diff-read", error, span))?;
    let modified_text =
        fs::read_to_string(&modified).map_err(|error| diff_error("diff-read", error, span))?;
    let mut options = DiffOptions::new();
    options
        .set_context_len(context as usize)
        .set_original_filename(patch_filename(&original, "original", span)?)
        .set_modified_filename(patch_filename(&modified, "modified", span)?);
    let patch = options.create_patch(&original_text, &modified_text);
    Ok(Value::ok(Value::Record(
        crate::runtime::value::RecordMap::from([
            (
                Arc::from("files"),
                Value::Int((!patch.hunks().is_empty()) as i64),
            ),
            (Arc::from("hunks"), Value::Int(patch.hunks().len() as i64)),
            (Arc::from("text"), Value::Str(patch.to_string().into())),
        ]),
    )))
}

fn patch_filename(
    path: &std::path::Path,
    fallback: &str,
    span: Span,
) -> Result<String, RuntimeError> {
    let Some(name) = path.file_name() else {
        return Ok(fallback.to_string());
    };
    name.to_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| RuntimeError::new("diff-path", "path is not UTF-8").with_span(span))
}

fn diff_error(kind: &str, error: std::io::Error, span: Span) -> RuntimeError {
    RuntimeError::new(kind, error.to_string()).with_span(span)
}
