//! Lowered value operations and method dispatch.
//!
//! Pure functions over `LoweredValue`/`LoweredType` (no `Evaluator`/`self`),
//! split out of the monolithic `eval.rs`. The IR types live in the parent
//! module and are imported via `super::`.

use super::{
    LoweredReturnKind, LoweredStatsValue, LoweredStrPredicate, LoweredTagValue, LoweredType,
    LoweredValue, add_error_context, bytes_contains, bytes_find, format_duration,
    lowered_bytes_view_value, lowered_inline_stats_field_value, lowered_record_vec_get,
    lowered_stats_field_value, lowered_str_view_value, normalize_path_value, path_parent,
    path_text_field, path_value_from_pathbuf, path_with_ext, pathbuf_from_path_value,
};
use crate::runtime::process::{ProcessStatus, ProcessStatusKind};
use crate::runtime::value::{
    ErrorContext, PathValue, RecordMap, RegexValue, ResultValue, RuntimeError, Value,
    error_constructor,
};
use crate::source::Span;
use crate::symbol::Name;
use crate::syntax::node::{AssignOp, BinaryOp};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::Arc;

pub(super) fn lowered_binary_op(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::Eq
            | BinaryOp::Ne
            | BinaryOp::Lt
            | BinaryOp::Le
            | BinaryOp::Gt
            | BinaryOp::Ge
            | BinaryOp::Or
            | BinaryOp::And
            | BinaryOp::Add
            | BinaryOp::Sub
            | BinaryOp::Mul
            | BinaryOp::Div
            | BinaryOp::Rem
            | BinaryOp::In
            | BinaryOp::NotIn
    )
}

pub(super) fn lowered_binary_value(
    op: BinaryOp,
    left: LoweredValue,
    right: LoweredValue,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    if op == BinaryOp::Eq {
        return Ok(LoweredValue::Bool(left == right));
    }
    if op == BinaryOp::Ne {
        return Ok(LoweredValue::Bool(left != right));
    }
    if let (Some(left_text), Some(right_text)) =
        (lowered_str_value(&left), lowered_str_value(&right))
    {
        return match op {
            BinaryOp::Add => {
                let mut text = left_text.to_string();
                text.push_str(right_text);
                Ok(LoweredValue::Str(text.into()))
            }
            BinaryOp::Lt => Ok(LoweredValue::Bool(left_text < right_text)),
            BinaryOp::Le => Ok(LoweredValue::Bool(left_text <= right_text)),
            BinaryOp::Gt => Ok(LoweredValue::Bool(left_text > right_text)),
            BinaryOp::Ge => Ok(LoweredValue::Bool(left_text >= right_text)),
            BinaryOp::In => Ok(LoweredValue::Bool(bytes_contains(
                right_text.as_bytes(),
                left_text.as_bytes(),
            ))),
            BinaryOp::NotIn => Ok(LoweredValue::Bool(!bytes_contains(
                right_text.as_bytes(),
                left_text.as_bytes(),
            ))),
            _ => Err(
                RuntimeError::new("type-error", "invalid lowered binary operation").with_span(span),
            ),
        };
    }
    if let Some(left_text) = lowered_str_value(&left)
        && let LoweredValue::Path(path) = &right
    {
        return match op {
            BinaryOp::In => Ok(LoweredValue::Bool(bytes_contains(
                path.display().as_bytes(),
                left_text.as_bytes(),
            ))),
            BinaryOp::NotIn => Ok(LoweredValue::Bool(!bytes_contains(
                path.display().as_bytes(),
                left_text.as_bytes(),
            ))),
            _ => Err(
                RuntimeError::new("type-error", "invalid lowered binary operation").with_span(span),
            ),
        };
    }
    match (op, left, right) {
        (BinaryOp::Add, LoweredValue::Float(left), LoweredValue::Float(right)) => Ok(
            LoweredValue::Float(crate::runtime::value::FloatValue::new(left.0 + right.0)),
        ),
        (BinaryOp::Sub, LoweredValue::Float(left), LoweredValue::Float(right)) => Ok(
            LoweredValue::Float(crate::runtime::value::FloatValue::new(left.0 - right.0)),
        ),
        (BinaryOp::Sub, LoweredValue::Int(0), LoweredValue::Float(right)) => Ok(
            LoweredValue::Float(crate::runtime::value::FloatValue::new(-right.0)),
        ),
        (BinaryOp::Mul, LoweredValue::Float(left), LoweredValue::Float(right)) => Ok(
            LoweredValue::Float(crate::runtime::value::FloatValue::new(left.0 * right.0)),
        ),
        (BinaryOp::Div, LoweredValue::Float(left), LoweredValue::Float(right)) => Ok(
            LoweredValue::Float(crate::runtime::value::FloatValue::new(left.0 / right.0)),
        ),
        (BinaryOp::Lt, LoweredValue::Float(left), LoweredValue::Float(right)) => {
            Ok(LoweredValue::Bool(left.0 < right.0))
        }
        (BinaryOp::Le, LoweredValue::Float(left), LoweredValue::Float(right)) => {
            Ok(LoweredValue::Bool(left.0 <= right.0))
        }
        (BinaryOp::Gt, LoweredValue::Float(left), LoweredValue::Float(right)) => {
            Ok(LoweredValue::Bool(left.0 > right.0))
        }
        (BinaryOp::Ge, LoweredValue::Float(left), LoweredValue::Float(right)) => {
            Ok(LoweredValue::Bool(left.0 >= right.0))
        }
        (BinaryOp::Lt, LoweredValue::Int(left), LoweredValue::Int(right)) => {
            Ok(LoweredValue::Bool(left < right))
        }
        (BinaryOp::Le, LoweredValue::Int(left), LoweredValue::Int(right)) => {
            Ok(LoweredValue::Bool(left <= right))
        }
        (BinaryOp::Gt, LoweredValue::Int(left), LoweredValue::Int(right)) => {
            Ok(LoweredValue::Bool(left > right))
        }
        (BinaryOp::Ge, LoweredValue::Int(left), LoweredValue::Int(right)) => {
            Ok(LoweredValue::Bool(left >= right))
        }
        (BinaryOp::Add, LoweredValue::Int(left), LoweredValue::Int(right)) => {
            Ok(LoweredValue::Int(left + right))
        }
        (BinaryOp::Sub, LoweredValue::Int(left), LoweredValue::Int(right)) => {
            Ok(LoweredValue::Int(left - right))
        }
        (BinaryOp::Mul, LoweredValue::Int(left), LoweredValue::Int(right)) => {
            Ok(LoweredValue::Int(left * right))
        }
        (BinaryOp::Div, LoweredValue::Int(left), LoweredValue::Int(right)) => {
            Ok(LoweredValue::Int(left / right))
        }
        (BinaryOp::Rem, LoweredValue::Int(left), LoweredValue::Int(right)) => {
            Ok(LoweredValue::Int(left % right))
        }
        (BinaryOp::In, left, LoweredValue::List(items)) => {
            Ok(LoweredValue::Bool(items.contains(&left)))
        }
        (BinaryOp::In, left, LoweredValue::SharedList(items)) => {
            Ok(LoweredValue::Bool(items.contains(&left)))
        }
        (BinaryOp::NotIn, left, LoweredValue::List(items)) => {
            Ok(LoweredValue::Bool(!items.contains(&left)))
        }
        (BinaryOp::NotIn, left, LoweredValue::SharedList(items)) => {
            Ok(LoweredValue::Bool(!items.contains(&left)))
        }
        _ => {
            Err(RuntimeError::new("type-error", "invalid lowered binary operation").with_span(span))
        }
    }
}

pub(super) fn lowered_bool_value(value: LoweredValue, span: Span) -> Result<bool, RuntimeError> {
    match value {
        LoweredValue::Bool(value) => Ok(value),
        LoweredValue::Status(status) => Ok(status.success),
        _ => {
            Err(RuntimeError::new("type-error", "lowered expression expected Bool").with_span(span))
        }
    }
}

pub(super) fn lowered_str_value(value: &LoweredValue) -> Option<&str> {
    match value {
        LoweredValue::Str(text) => Some(text),
        LoweredValue::StrView(view) => Some(view.as_str()),
        _ => None,
    }
}

pub(super) fn lowered_str_parts(value: &LoweredValue) -> Option<(Arc<str>, usize, usize)> {
    match value {
        LoweredValue::Str(text) => Some((text.clone(), 0, text.len())),
        LoweredValue::StrView(view) => Some((view.text.clone(), view.start(), view.end())),
        _ => None,
    }
}

pub(super) fn lowered_bytes_value(value: &LoweredValue) -> Option<&[u8]> {
    match value {
        LoweredValue::Bytes(bytes) => Some(bytes),
        LoweredValue::BytesView(view) => Some(view.as_slice()),
        _ => None,
    }
}

pub(super) fn lowered_bytes_parts(value: &LoweredValue) -> Option<(Arc<[u8]>, usize, usize)> {
    match value {
        LoweredValue::Bytes(bytes) => Some((bytes.clone(), 0, bytes.len())),
        LoweredValue::BytesView(view) => Some((view.bytes.clone(), view.start(), view.end())),
        _ => None,
    }
}

pub(super) fn lowered_bytes_arg<'a>(
    value: &'a LoweredValue,
    method: &str,
    span: Span,
) -> Result<&'a [u8], RuntimeError> {
    lowered_bytes_value(value).ok_or_else(|| {
        RuntimeError::new(
            "type-error",
            format!("{method} expected Bytes, found {}", value.type_name()),
        )
        .with_span(span)
    })
}

pub(super) fn lowered_str_arg<'a>(
    value: &'a LoweredValue,
    method: &str,
    span: Span,
) -> Result<&'a str, RuntimeError> {
    lowered_str_value(value).ok_or_else(|| {
        RuntimeError::new("type-error", format!("{method} expected Str")).with_span(span)
    })
}

pub(super) fn lowered_trim_str_value(
    value: &LoweredValue,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    let Some((text, start, end)) = lowered_str_parts(value) else {
        return Err(RuntimeError::new("type-error", "trim expected Str").with_span(span));
    };
    let slice = &text[start..end];
    let trimmed = slice.trim();
    let trim_start = trimmed.as_ptr() as usize - slice.as_ptr() as usize;
    let trim_len = trimmed.len();
    Ok(lowered_str_view_value(
        text,
        start + trim_start,
        start + trim_start + trim_len,
    ))
}

pub(super) fn lowered_trim_is_empty_value(
    value: &LoweredValue,
    span: Span,
) -> Result<bool, RuntimeError> {
    if let Some(bytes) = lowered_bytes_value(value) {
        return Ok(crate::runtime::text_bytes::trim_bytes(bytes).is_empty());
    }
    let Some(text) = lowered_str_value(value) else {
        return Err(RuntimeError::new("type-error", "trim expected Str").with_span(span));
    };
    if text.is_ascii() {
        return Ok(text.bytes().all(|byte| byte.is_ascii_whitespace()));
    }
    Ok(text.chars().all(char::is_whitespace))
}

pub(super) fn ascii_trim_start(text: &str) -> &str {
    let start = text
        .bytes()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(text.len());
    &text[start..]
}

pub(super) fn ascii_trim_end(text: &str) -> &str {
    let end = text
        .bytes()
        .rposition(|byte| !byte.is_ascii_whitespace())
        .map_or(0, |index| index + 1);
    &text[..end]
}

pub(super) fn lowered_str_byte_len_value(
    value: &LoweredValue,
    span: Span,
) -> Result<i64, RuntimeError> {
    if let Some(bytes) = lowered_bytes_value(value) {
        return Ok(bytes.len() as i64);
    }
    match lowered_str_value(value) {
        Some(text) => Ok(text.len() as i64),
        None => Err(RuntimeError::new(
            "type-error",
            format!("byte_len expected Str, found {}", value.type_name()),
        )
        .with_span(span)),
    }
}

pub(super) fn lowered_str_byte_at_value(
    value: &LoweredValue,
    index: i64,
    default: i64,
    span: Span,
) -> Result<i64, RuntimeError> {
    if let Some(bytes) = lowered_bytes_value(value) {
        return Ok(bytes
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .copied()
            .map(i64::from)
            .unwrap_or(default));
    }
    match lowered_str_value(value) {
        Some(text) => Ok(text
            .as_bytes()
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .copied()
            .map(i64::from)
            .unwrap_or(default)),
        None => Err(RuntimeError::new(
            "type-error",
            format!("byte_at expected Str, found {}", value.type_name()),
        )
        .with_span(span)),
    }
}

pub(super) fn lowered_str_count_lines_value(
    value: &LoweredValue,
    span: Span,
) -> Result<i64, RuntimeError> {
    if let Some(bytes) = lowered_bytes_value(value) {
        return Ok(crate::runtime::text_bytes::count_lines_bytes(bytes) as i64);
    }
    let text = lowered_str_arg(value, "count_lines", span)?;
    Ok(lowered_str_count_lines_text(text))
}

pub(super) fn lowered_str_count_lines_text(text: &str) -> i64 {
    crate::runtime::text_bytes::count_lines(text) as i64
}

pub(super) fn lowered_str_predicate_value(
    value: &LoweredValue,
    predicate: LoweredStrPredicate,
    needle: &LoweredValue,
    span: Span,
) -> Result<bool, RuntimeError> {
    if let Some(bytes) = lowered_bytes_value(value) {
        let needle = lowered_bytes_arg(needle, "string predicate", span)?;
        return Ok(match predicate {
            LoweredStrPredicate::StartsWith => bytes.starts_with(needle),
            LoweredStrPredicate::EndsWith => bytes.ends_with(needle),
        });
    }
    let needle = lowered_str_arg(needle, "string predicate", span)?;
    lowered_str_predicate_text(value, predicate, needle.as_bytes(), span)
}

pub(super) fn lowered_str_predicate_text(
    value: &LoweredValue,
    predicate: LoweredStrPredicate,
    needle: &[u8],
    span: Span,
) -> Result<bool, RuntimeError> {
    // Byte-level comparison is equivalent to the `Str` operation: for a `Str`
    // receiver the needle holds the UTF-8 bytes of the original `Str` literal.
    let bytes = lowered_bytes_value(value)
        .or_else(|| lowered_str_value(value).map(str::as_bytes))
        .ok_or_else(|| {
            RuntimeError::new(
                "type-error",
                format!("string predicate expected Str, found {}", value.type_name()),
            )
            .with_span(span)
        })?;
    Ok(match predicate {
        LoweredStrPredicate::StartsWith => bytes.starts_with(needle),
        LoweredStrPredicate::EndsWith => bytes.ends_with(needle),
    })
}

pub(super) fn lowered_trim_str_predicate_value(
    value: &LoweredValue,
    predicate: LoweredStrPredicate,
    needle: &[u8],
    span: Span,
) -> Result<bool, RuntimeError> {
    if let Some(bytes) = lowered_bytes_value(value) {
        // `Bytes` uses full trim then a byte prefix/suffix check, which is the
        // exact `trim().starts_with(...)` / `.ends_with(...)` semantic.
        let trimmed = crate::runtime::text_bytes::trim_bytes(bytes);
        return Ok(match predicate {
            LoweredStrPredicate::StartsWith => trimmed.starts_with(needle),
            LoweredStrPredicate::EndsWith => trimmed.ends_with(needle),
        });
    }
    let text = lowered_str_value(value).ok_or_else(|| {
        RuntimeError::new(
            "type-error",
            format!("trim expected Str, found {}", value.type_name()),
        )
        .with_span(span)
    })?;
    // For `Str`, trimming only the relevant end is a valid shortcut for a
    // prefix/suffix check; compare on bytes so the needle type stays uniform.
    Ok(match predicate {
        LoweredStrPredicate::StartsWith if text.is_ascii() => {
            ascii_trim_start(text).as_bytes().starts_with(needle)
        }
        LoweredStrPredicate::StartsWith => text.trim_start().as_bytes().starts_with(needle),
        LoweredStrPredicate::EndsWith if text.is_ascii() => {
            ascii_trim_end(text).as_bytes().ends_with(needle)
        }
        LoweredStrPredicate::EndsWith => text.trim_end().as_bytes().ends_with(needle),
    })
}

pub(super) fn lowered_contains_value(
    receiver: &LoweredValue,
    needle: &LoweredValue,
    span: Span,
) -> Result<bool, RuntimeError> {
    match receiver {
        LoweredValue::Str(_) | LoweredValue::StrView(_) => {
            let needle = lowered_str_arg(needle, "contains", span)?;
            let text = lowered_str_value(receiver).expect("checked lowered string");
            Ok(bytes_contains(text.as_bytes(), needle.as_bytes()))
        }
        LoweredValue::List(items) => Ok(items.iter().any(|item| item == needle)),
        LoweredValue::SharedList(items) => Ok(items.iter().any(|item| item == needle)),
        _ => lowered_method_value(receiver.clone(), "contains", vec![needle.clone()], span)
            .and_then(|value| match value {
                LoweredValue::Bool(value) => Ok(value),
                _ => Err(
                    RuntimeError::new("type-error", "contains expected Bool result")
                        .with_span(span),
                ),
            }),
    }
}

pub(super) fn compare_lowered_sort_keys(left: &LoweredValue, right: &LoweredValue) -> Ordering {
    if let (Some(left), Some(right)) = (lowered_str_value(left), lowered_str_value(right)) {
        return left.cmp(right);
    }
    match (left, right) {
        (LoweredValue::Int(left), LoweredValue::Int(right)) => left.cmp(right),
        (LoweredValue::Bool(left), LoweredValue::Bool(right)) => left.cmp(right),
        (LoweredValue::Path(left), LoweredValue::Path(right)) => left.bytes.cmp(&right.bytes),
        _ => left.type_name().cmp(right.type_name()),
    }
}

pub(super) fn lowered_find_text_bytes(text: &str, needle: &str, start: i64) -> i64 {
    let Ok(start) = usize::try_from(start) else {
        return -1;
    };
    let haystack = text.as_bytes();
    let needle = needle.as_bytes();
    if start > haystack.len() {
        return -1;
    }
    if needle.is_empty() {
        return start as i64;
    }
    bytes_find(&haystack[start..], needle)
        .map(|offset| (start + offset) as i64)
        .unwrap_or(-1)
}

pub(super) fn lowered_byte_slice_text(
    text: &str,
    offset: i64,
    length: Option<i64>,
    span: Span,
) -> Result<Arc<str>, RuntimeError> {
    if offset < 0 {
        return Err(
            RuntimeError::new("text-byte-slice", "offset cannot be negative").with_span(span),
        );
    }
    let offset = offset as usize;
    if offset > text.len() {
        return Err(
            RuntimeError::new("text-byte-slice", "offset is past end of text").with_span(span),
        );
    }
    let end = match length {
        Some(length) if length < 0 => {
            return Err(
                RuntimeError::new("text-byte-slice", "length cannot be negative").with_span(span),
            );
        }
        Some(length) => offset.saturating_add(length as usize).min(text.len()),
        None => text.len(),
    };
    if !text.is_char_boundary(offset) || !text.is_char_boundary(end) {
        return Err(
            RuntimeError::new("text-byte-slice", "slice must align to UTF-8 boundaries")
                .with_span(span),
        );
    }
    Ok(text[offset..end].into())
}

pub(super) fn lowered_join_list(
    items: &[LoweredValue],
    args: &[LoweredValue],
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    let separator = match args {
        [] => "",
        [separator] => lowered_str_arg(separator, "join", span)?,
        _ => {
            return Err(
                RuntimeError::new("arity", "join expected 0 or 1 arguments").with_span(span)
            );
        }
    };
    let mut parts = Vec::with_capacity(items.len());
    for item in items {
        if let Some(part) = lowered_str_value(item) {
            parts.push(part);
        } else {
            return Err(RuntimeError::new("type-error", "join expected List[Str]").with_span(span));
        }
    }
    Ok(LoweredValue::Str(parts.join(separator).into()))
}

pub(super) fn lowered_assign_value(
    op: AssignOp,
    current: LoweredValue,
    value: LoweredValue,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    match op {
        AssignOp::Set => Ok(value),
        AssignOp::Add => lowered_binary_value(BinaryOp::Add, current, value, span),
        AssignOp::Sub => lowered_binary_value(BinaryOp::Sub, current, value, span),
        AssignOp::Mul => lowered_binary_value(BinaryOp::Mul, current, value, span),
        AssignOp::Div => lowered_binary_value(BinaryOp::Div, current, value, span),
        AssignOp::Rem => lowered_binary_value(BinaryOp::Rem, current, value, span),
    }
}

pub(super) fn lowered_return_value(
    kind: LoweredReturnKind,
    value: LoweredValue,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    match (kind, value) {
        (LoweredReturnKind::Plain(kind), value) if lowered_value_matches(kind, &value) => Ok(value),
        (LoweredReturnKind::Result(kind), LoweredValue::ResultOk(value))
            if lowered_value_matches(kind, &value) =>
        {
            Ok(LoweredValue::ResultOk(value))
        }
        (LoweredReturnKind::Result(kind), value) if lowered_value_matches(kind, &value) => {
            Ok(LoweredValue::ResultOk(Box::new(value)))
        }
        (LoweredReturnKind::Result(_), LoweredValue::ResultErr(value)) => {
            Ok(LoweredValue::ResultErr(value))
        }
        _ => Err(RuntimeError::new("type-error", "lowered return type mismatch").with_span(span)),
    }
}

pub(super) fn lowered_value_matches(kind: LoweredType, value: &LoweredValue) -> bool {
    matches!(
        (kind, value),
        (LoweredType::Any, _)
            | (LoweredType::Unit, LoweredValue::Unit)
            | (LoweredType::Int, LoweredValue::Int(_))
            | (LoweredType::Float, LoweredValue::Float(_))
            | (LoweredType::Duration, LoweredValue::Duration(_))
            | (LoweredType::Bool, LoweredValue::Bool(_))
            | (LoweredType::Str, LoweredValue::Str(_))
            | (LoweredType::Str, LoweredValue::StrView(_))
            | (LoweredType::Bytes, LoweredValue::Bytes(_))
            | (LoweredType::Bytes, LoweredValue::BytesView(_))
            | (LoweredType::Digest, LoweredValue::Digest(_))
            | (LoweredType::Regex, LoweredValue::Regex(_))
            | (LoweredType::Status, LoweredValue::Status(_))
            | (LoweredType::Path, LoweredValue::Path(_))
            | (LoweredType::Command, LoweredValue::Command(_))
            | (LoweredType::ProcessHandle, LoweredValue::ProcessHandle(_))
            | (LoweredType::Stream, LoweredValue::Stream(_))
            | (LoweredType::Pure, LoweredValue::Pure(_))
            | (LoweredType::Proc, LoweredValue::Proc(_))
            | (LoweredType::Error, LoweredValue::Error(_))
            | (LoweredType::Record, LoweredValue::Record(_))
            | (LoweredType::Record, LoweredValue::RecordVec(_))
            | (LoweredType::Record, LoweredValue::Stats { .. })
            | (LoweredType::Record, LoweredValue::StatsBlob(_))
            | (LoweredType::Record, LoweredValue::FsEntry(_))
            | (LoweredType::Module, LoweredValue::Module(_))
            | (LoweredType::List, LoweredValue::List(_))
            | (LoweredType::List, LoweredValue::SharedList(_))
            | (LoweredType::Map, LoweredValue::Map(_))
            | (LoweredType::Tag, LoweredValue::Tag(_))
            | (LoweredType::Result, LoweredValue::ResultOk(_))
            | (LoweredType::Result, LoweredValue::ResultErr(_))
    )
}

pub(super) fn lowered_type_name(kind: LoweredType) -> &'static str {
    match kind {
        LoweredType::Any => "Any",
        LoweredType::Unit => "Unit",
        LoweredType::Int => "Int",
        LoweredType::Float => "Float",
        LoweredType::Duration => "Duration",
        LoweredType::Bool => "Bool",
        LoweredType::Str => "Str",
        LoweredType::Bytes => "Bytes",
        LoweredType::Digest => "Digest",
        LoweredType::Regex => "Regex",
        LoweredType::Status => "Status",
        LoweredType::Path => "Path",
        LoweredType::Command => "Command",
        LoweredType::ProcessHandle => "ProcessHandle",
        LoweredType::Stream => "Stream",
        LoweredType::Pure => "Pure",
        LoweredType::Proc => "Proc",
        LoweredType::Error => "Error",
        LoweredType::Record => "Record",
        LoweredType::Module => "Module",
        LoweredType::List => "List",
        LoweredType::Map => "Map",
        LoweredType::Tag => "Tag",
        LoweredType::Result => "Result",
    }
}

pub(super) fn lowered_value_from_runtime(value: &Value, kind: LoweredType) -> Option<LoweredValue> {
    match (kind, value) {
        (LoweredType::Any, _) => lowered_value_from_runtime_any(value),
        (_, Value::Result(ResultValue::Ok(value))) if kind != LoweredType::Result => {
            lowered_value_from_runtime(value, kind)
        }
        (_, Value::Null) => Some(LoweredValue::Null),
        (LoweredType::Unit, Value::Unit) => Some(LoweredValue::Unit),
        (LoweredType::Int, Value::Int(value)) => Some(LoweredValue::Int(*value)),
        (LoweredType::Float, Value::Float(value)) => Some(LoweredValue::Float(*value)),
        (LoweredType::Duration, Value::Duration(value)) => {
            Some(LoweredValue::Duration(value.clone()))
        }
        (LoweredType::Bool, Value::Bool(value)) => Some(LoweredValue::Bool(*value)),
        (LoweredType::Str, Value::Str(value)) => Some(LoweredValue::Str(value.clone())),
        (LoweredType::Bytes, Value::Bytes(value)) => {
            Some(LoweredValue::Bytes(value.as_slice().into()))
        }
        (LoweredType::Digest, Value::Digest(value)) => Some(LoweredValue::Digest(value.clone())),
        (LoweredType::Regex, Value::Regex(value)) => {
            Some(LoweredValue::Regex(Box::new(value.clone())))
        }
        (LoweredType::Status, Value::Status(value)) => {
            Some(LoweredValue::Status(Box::new(value.clone())))
        }
        (LoweredType::Path, Value::Path(value)) => Some(LoweredValue::Path(value.clone())),
        (LoweredType::Command, Value::Command(value)) => {
            Some(LoweredValue::Command(Box::new((**value).clone())))
        }
        (LoweredType::ProcessHandle, Value::ProcessHandle(value)) => {
            Some(LoweredValue::ProcessHandle(value.clone()))
        }
        (LoweredType::Stream, Value::Stream(value)) => Some(LoweredValue::Stream(value.clone())),
        (LoweredType::Pure, Value::Pure(value)) => Some(LoweredValue::Pure(*value)),
        (LoweredType::Proc, Value::Proc(value)) => Some(LoweredValue::Proc(*value)),
        (LoweredType::Error, Value::Error(_)) => Some(LoweredValue::Error(Box::new(value.clone()))),
        (LoweredType::Record, Value::Record(value)) => lowered_record_from_runtime(value),
        (LoweredType::Record, Value::FsEntry(value)) => Some(LoweredValue::FsEntry(value.clone())),
        (LoweredType::Module, Value::Module(value)) => lowered_module_from_runtime(value),
        (LoweredType::List, Value::List(value)) => lowered_list_from_runtime(value),
        (LoweredType::Map, Value::Map(value)) => lowered_map_from_runtime(value),
        (LoweredType::Tag, Value::Tag { name, fields }) => {
            Some(LoweredValue::Tag(Box::new(LoweredTagValue {
                name: name.clone(),
                fields: fields
                    .iter()
                    .map(lowered_value_from_runtime_any)
                    .collect::<Option<Vec<_>>>()?,
            })))
        }
        (LoweredType::Result, Value::Result(value)) => lowered_result_from_runtime(value),
        _ => None,
    }
}

pub(super) fn lowered_value_from_runtime_any(value: &Value) -> Option<LoweredValue> {
    match value {
        Value::Null => Some(LoweredValue::Null),
        Value::Unit => Some(LoweredValue::Unit),
        Value::Int(value) => Some(LoweredValue::Int(*value)),
        Value::Float(value) => Some(LoweredValue::Float(*value)),
        Value::Duration(value) => Some(LoweredValue::Duration(value.clone())),
        Value::Bool(value) => Some(LoweredValue::Bool(*value)),
        Value::Str(value) => Some(LoweredValue::Str(value.clone())),
        Value::Bytes(value) => Some(LoweredValue::Bytes(value.as_slice().into())),
        Value::Digest(value) => Some(LoweredValue::Digest(value.clone())),
        Value::Regex(value) => Some(LoweredValue::Regex(Box::new(value.clone()))),
        Value::Status(value) => Some(LoweredValue::Status(Box::new(value.clone()))),
        Value::Path(value) => Some(LoweredValue::Path(value.clone())),
        Value::FsEntry(value) => Some(LoweredValue::FsEntry(value.clone())),
        Value::Command(value) => Some(LoweredValue::Command(Box::new((**value).clone()))),
        Value::ProcessHandle(value) => Some(LoweredValue::ProcessHandle(value.clone())),
        Value::Stream(value) => Some(LoweredValue::Stream(value.clone())),
        Value::Pure(value) => Some(LoweredValue::Pure(*value)),
        Value::Proc(value) => Some(LoweredValue::Proc(*value)),
        Value::Error(_) | Value::RunError(_) => Some(LoweredValue::Error(Box::new(value.clone()))),
        Value::Record(value) => lowered_record_from_runtime(value),
        Value::Module(value) => lowered_module_from_runtime(value),
        Value::List(value) => lowered_list_from_runtime(value),
        Value::Map(value) => lowered_map_from_runtime(value),
        Value::Tag { name, fields } => Some(LoweredValue::Tag(Box::new(LoweredTagValue {
            name: name.clone(),
            fields: fields
                .iter()
                .map(lowered_value_from_runtime_any)
                .collect::<Option<Vec<_>>>()?,
        }))),
        Value::Result(value) => lowered_result_from_runtime(value),
        _ => None,
    }
}

fn lowered_runtime_any(value: Value, span: Span) -> Result<LoweredValue, RuntimeError> {
    lowered_value_from_runtime_any(&value).ok_or_else(|| {
        RuntimeError::new(
            "type-error",
            format!("cannot lower runtime value {}", value.type_name()),
        )
        .with_span(span)
    })
}

fn lowered_runtime_list(values: Vec<Value>, span: Span) -> Result<LoweredValue, RuntimeError> {
    lowered_runtime_any(Value::List(values), span)
}

fn lowered_result_from_runtime(value: &ResultValue) -> Option<LoweredValue> {
    match value {
        ResultValue::Ok(value) => Some(LoweredValue::ResultOk(Box::new(
            lowered_value_from_runtime_any(value)?,
        ))),
        ResultValue::Err(value) => Some(LoweredValue::ResultErr(value.clone())),
    }
}

pub(super) fn lowered_record_from_runtime(value: &RecordMap) -> Option<LoweredValue> {
    let mut record = BTreeMap::new();
    for (key, value) in value {
        record.insert(key.clone(), lowered_value_from_runtime_any(value)?);
    }
    Some(LoweredValue::Record(record))
}

pub(super) fn lowered_module_from_runtime(value: &RecordMap) -> Option<LoweredValue> {
    let mut module = BTreeMap::new();
    for (key, value) in value {
        module.insert(key.clone(), lowered_value_from_runtime_any(value)?);
    }
    Some(LoweredValue::Module(module))
}

pub(super) fn lowered_list_from_runtime(value: &[Value]) -> Option<LoweredValue> {
    let mut items = Vec::with_capacity(value.len());
    for item in value {
        items.push(lowered_value_from_runtime_any(item)?);
    }
    Some(LoweredValue::List(items))
}

pub(super) fn lowered_map_from_runtime(value: &BTreeMap<String, Value>) -> Option<LoweredValue> {
    let mut map = BTreeMap::new();
    for (key, value) in value {
        map.insert(key.clone(), lowered_value_from_runtime_any(value)?);
    }
    Some(LoweredValue::Map(map))
}

pub(super) fn push_lowered_display(
    output: &mut String,
    value: &LoweredValue,
    span: Span,
) -> Result<(), RuntimeError> {
    match value {
        LoweredValue::Int(value) => output.push_str(&value.to_string()),
        LoweredValue::Float(value) => output.push_str(&value.format()),
        LoweredValue::Duration(value) => output.push_str(&format_duration(value.millis)),
        LoweredValue::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
        LoweredValue::Str(value) => output.push_str(value),
        LoweredValue::StrView(value) => output.push_str(value.as_str()),
        LoweredValue::Path(value) => output.push_str(&value.display()),
        LoweredValue::Status(value) => output.push_str(&format!("{:?}", value.kind).to_lowercase()),
        LoweredValue::Error(value) => match value.as_ref() {
            Value::Error(error) => output.push_str(&error.message),
            _ => output.push_str("error"),
        },
        LoweredValue::ResultOk(value) => push_lowered_display(output, value, span)?,
        LoweredValue::ResultErr(value) => {
            output.push_str(value.error_message().unwrap_or("error"));
        }
        value => {
            return Err(RuntimeError::new(
                "display-conversion",
                format!("cannot display {}", value.type_name()),
            )
            .with_span(span));
        }
    }
    Ok(())
}

pub(super) fn lowered_method_value(
    receiver: LoweredValue,
    name: &str,
    args: Vec<LoweredValue>,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    if matches!(receiver, LoweredValue::Str(_) | LoweredValue::StrView(_)) {
        return lowered_str_method_value(&receiver, name, args, span);
    }
    if matches!(
        receiver,
        LoweredValue::Bytes(_) | LoweredValue::BytesView(_)
    ) {
        return lowered_bytes_method_value(&receiver, name, args, span);
    }
    match receiver {
        LoweredValue::Int(value) => lowered_int_method_value(value, name, args, span),
        LoweredValue::Float(value) => lowered_float_method_value(value, name, args, span),
        LoweredValue::Digest(digest) => lowered_digest_method_value(digest, name, args, span),
        LoweredValue::Regex(regex) => lowered_regex_method_value(*regex, name, args, span),
        LoweredValue::Status(status) => lowered_status_method_value(*status, name, args, span),
        LoweredValue::Path(path) => lowered_path_method_value(path, name, args, span),
        LoweredValue::FsEntry(entry) => {
            let record = entry
                .to_record_map()
                .map_err(|error| error.with_span(span))?;
            lowered_record_method_value(
                record
                    .into_iter()
                    .filter_map(|(key, value)| {
                        lowered_value_from_runtime_any(&value).map(|value| (key, value))
                    })
                    .collect(),
                name,
                args,
                span,
            )
        }
        LoweredValue::Record(record) | LoweredValue::Module(record) => {
            lowered_record_method_value(record, name, args, span)
        }
        LoweredValue::RecordVec(record) => {
            lowered_record_vec_method_value(&record, name, args, span)
        }
        LoweredValue::Stats {
            blanks,
            code,
            comments,
        } => lowered_inline_stats_method_value(blanks, code, comments, name, args, span),
        LoweredValue::StatsBlob(stats) => lowered_stats_method_value(&stats, name, args, span),
        LoweredValue::List(items) => lowered_list_method_value(items, name, args, span),
        LoweredValue::SharedList(items) => {
            if let Some(value) =
                lowered_list_method_ref(items.as_slice(), name, args.clone(), span)?
            {
                Ok(value)
            } else {
                lowered_list_method_value(items.as_ref().clone(), name, args, span)
            }
        }
        LoweredValue::Map(map) => lowered_map_method_value(map, name, args, span),
        LoweredValue::ResultOk(value) => {
            lowered_result_method_value(LoweredValue::ResultOk(value), name, args, span)
        }
        LoweredValue::ResultErr(value) => {
            lowered_result_method_value(LoweredValue::ResultErr(value), name, args, span)
        }
        _ => Err(RuntimeError::new("type-error", "unsupported lowered method").with_span(span)),
    }
}

fn lowered_int_method_value(
    value: i64,
    name: &str,
    args: Vec<LoweredValue>,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    match name {
        "float" if args.is_empty() => Ok(LoweredValue::Float(
            crate::runtime::value::FloatValue::new(value as f64),
        )),
        _ => Err(
            RuntimeError::new("unsupported-call", "unsupported lowered Int method").with_span(span),
        ),
    }
}

fn lowered_float_to_int_result(value: f64, span: Span) -> LoweredValue {
    if !value.is_finite()
        || !(-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&value)
    {
        return LoweredValue::ResultErr(Box::new(Value::Error(Box::new(
            RuntimeError::new(
                "float-conversion",
                "Float value cannot be represented as Int",
            )
            .with_span(span),
        ))));
    }
    LoweredValue::ResultOk(Box::new(LoweredValue::Int(value as i64)))
}

fn lowered_float_method_value(
    value: crate::runtime::value::FloatValue,
    name: &str,
    args: Vec<LoweredValue>,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    match name {
        "floor" if args.is_empty() => Ok(lowered_float_to_int_result(value.0.floor(), span)),
        "ceil" if args.is_empty() => Ok(lowered_float_to_int_result(value.0.ceil(), span)),
        "round" if args.is_empty() => Ok(lowered_float_to_int_result(value.0.round(), span)),
        "format" if args.is_empty() || args.len() == 1 => {
            let precision = match args.first() {
                Some(LoweredValue::Int(value)) => *value,
                Some(other) => {
                    return Err(RuntimeError::new(
                        "type-error",
                        format!("format precision expected Int, found {}", other.type_name()),
                    )
                    .with_span(span));
                }
                None => 6,
            };
            if !(0..=100).contains(&precision) {
                return Err(RuntimeError::new(
                    "float-format",
                    "precision must be between 0 and 100",
                )
                .with_span(span));
            }
            if !value.0.is_finite() {
                return Ok(LoweredValue::Str(value.format().into()));
            }
            Ok(LoweredValue::Str(
                format!("{:.*}", precision as usize, value.0).into(),
            ))
        }
        "sqrt" if args.is_empty() => Ok(LoweredValue::Float(
            crate::runtime::value::FloatValue::new(value.0.sqrt()),
        )),
        "pow" if args.len() == 1 => {
            let LoweredValue::Float(exp) = args[0] else {
                return Err(RuntimeError::new("type-error", "pow expected Float").with_span(span));
            };
            Ok(LoweredValue::Float(crate::runtime::value::FloatValue::new(
                value.0.powf(exp.0),
            )))
        }
        "exp" if args.is_empty() => Ok(LoweredValue::Float(
            crate::runtime::value::FloatValue::new(value.0.exp()),
        )),
        "ln" if args.is_empty() => Ok(LoweredValue::Float(crate::runtime::value::FloatValue::new(
            value.0.ln(),
        ))),
        "log" if args.len() == 1 => {
            let LoweredValue::Float(base) = args[0] else {
                return Err(RuntimeError::new("type-error", "log expected Float").with_span(span));
            };
            Ok(LoweredValue::Float(crate::runtime::value::FloatValue::new(
                value.0.log(base.0),
            )))
        }
        "sin" if args.is_empty() => Ok(LoweredValue::Float(
            crate::runtime::value::FloatValue::new(value.0.sin()),
        )),
        "cos" if args.is_empty() => Ok(LoweredValue::Float(
            crate::runtime::value::FloatValue::new(value.0.cos()),
        )),
        "tan" if args.is_empty() => Ok(LoweredValue::Float(
            crate::runtime::value::FloatValue::new(value.0.tan()),
        )),
        "abs" if args.is_empty() => Ok(LoweredValue::Float(
            crate::runtime::value::FloatValue::new(value.0.abs()),
        )),
        _ => Err(
            RuntimeError::new("unsupported-call", "unsupported lowered Float method")
                .with_span(span),
        ),
    }
}

pub(super) fn lowered_str_method_value(
    text: &LoweredValue,
    name: &str,
    args: Vec<LoweredValue>,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    let text_value = lowered_str_arg(text, name, span)?;
    match name {
        "trim" if args.is_empty() => lowered_trim_str_value(text, span),
        "lower" if args.is_empty() => Ok(LoweredValue::Str(
            crate::modules::text::lower_text(text_value).into(),
        )),
        "upper" if args.is_empty() => Ok(LoweredValue::Str(
            crate::modules::text::upper_text(text_value).into(),
        )),
        "reverse" if args.is_empty() => Ok(LoweredValue::Str(
            text_value.chars().rev().collect::<String>().into(),
        )),
        "lines" if args.is_empty() => Ok(LoweredValue::List(
            text_value
                .lines()
                .map(|line| LoweredValue::Str(line.into()))
                .collect(),
        )),
        "words" | "fields" if args.is_empty() => Ok(LoweredValue::List(
            text_value
                .split_whitespace()
                .map(|word| LoweredValue::Str(word.into()))
                .collect(),
        )),
        "fields" if args.len() == 1 => {
            let delimiter = lowered_str_arg(&args[0], "fields", span)?;
            lowered_runtime_list(
                crate::modules::text::fields_text(text_value, delimiter),
                span,
            )
        }
        "split" if args.len() == 1 => {
            let separator = lowered_str_arg(&args[0], "split", span)?;
            lowered_runtime_list(crate::modules::text::split_text(text_value, separator), span)
        }
        "wrap" if args.len() == 1 => {
            let LoweredValue::Int(width) = args[0] else {
                return Err(RuntimeError::new("type-error", "wrap expected Int").with_span(span));
            };
            lowered_runtime_list(
                crate::modules::text::wrap_text(text_value, width, span)?,
                span,
            )
        }
        "replace" if args.len() == 2 => {
            let from = lowered_str_arg(&args[0], "replace", span)?;
            let to = lowered_str_arg(&args[1], "replace", span)?;
            Ok(LoweredValue::Str(text_value.replace(from, to).into()))
        }
        "translate" if args.len() == 2 => {
            let from = lowered_str_arg(&args[0], "translate", span)?;
            let to = lowered_str_arg(&args[1], "translate", span)?;
            Ok(LoweredValue::Str(
                crate::modules::text::translate_text(text_value, from, to).into(),
            ))
        }
        "delete" if args.len() == 1 => {
            let chars = lowered_str_arg(&args[0], "delete", span)?;
            Ok(LoweredValue::Str(
                crate::modules::text::delete_text(text_value, chars).into(),
            ))
        }
        "squeeze" if args.is_empty() || args.len() == 1 => {
            let chars = match args.first() {
                Some(value) => lowered_str_arg(value, "squeeze", span)?,
                None => "",
            };
            Ok(LoweredValue::Str(
                crate::modules::text::squeeze_text(text_value, chars).into(),
            ))
        }
        "parse_int" if args.is_empty() => {
            match crate::modules::text::parse_int_text(text_value, span) {
                Ok(value) => Ok(LoweredValue::ResultOk(Box::new(LoweredValue::Int(value)))),
                Err(error) => Ok(LoweredValue::ResultErr(Box::new(Value::Error(Box::new(
                    error,
                ))))),
            }
        }
        "parse_float" if args.is_empty() => {
            match crate::modules::text::parse_float_text(text_value, span) {
                Ok(value) => Ok(LoweredValue::ResultOk(Box::new(LoweredValue::Float(
                    crate::runtime::value::FloatValue::new(value),
                )))),
                Err(error) => Ok(LoweredValue::ResultErr(Box::new(Value::Error(Box::new(
                    error,
                ))))),
            }
        }
        "base64_decode" if args.is_empty() => {
            match crate::modules::bytes::base64_decode(text_value) {
                Ok(bytes) => Ok(LoweredValue::ResultOk(Box::new(LoweredValue::Bytes(
                    bytes.into(),
                )))),
                Err(message) => Ok(lowered_result_err("invalid-base64", message)),
            }
        }
        "base32_decode" if args.is_empty() => {
            match crate::modules::bytes::base32_decode(text_value) {
                Ok(bytes) => Ok(LoweredValue::ResultOk(Box::new(LoweredValue::Bytes(
                    bytes.into(),
                )))),
                Err(message) => Ok(lowered_result_err("invalid-base32", message)),
            }
        }
        "count_lines" if args.is_empty() => {
            Ok(LoweredValue::Int(lowered_str_count_lines_text(text_value)))
        }
        "count_words" if args.is_empty() => Ok(LoweredValue::Int(
            text_value.split_whitespace().count() as i64,
        )),
        "count_chars" if args.is_empty() => {
            Ok(LoweredValue::Int(text_value.chars().count() as i64))
        }
        "count_bytes" if args.is_empty() => Ok(LoweredValue::Int(text_value.len() as i64)),
        "byte_len" if args.is_empty() => Ok(LoweredValue::Int(text_value.len() as i64)),
        "byte_at" if args.len() == 1 || args.len() == 2 => {
            let LoweredValue::Int(index) = &args[0] else {
                return Err(RuntimeError::new("type-error", "byte_at expected Int").with_span(span));
            };
            let default = match args.get(1) {
                Some(LoweredValue::Int(value)) => *value,
                Some(_) => {
                    return Err(
                        RuntimeError::new("type-error", "byte_at default expected Int")
                            .with_span(span),
                    );
                }
                None => -1,
            };
            Ok(LoweredValue::Int(
                text_value
                    .as_bytes()
                    .get(usize::try_from(*index).unwrap_or(usize::MAX))
                    .copied()
                    .map(i64::from)
                    .unwrap_or(default),
            ))
        }
        "byte_slice" if args.len() == 1 || args.len() == 2 => {
            let LoweredValue::Int(offset) = &args[0] else {
                return Err(
                    RuntimeError::new("type-error", "byte_slice expected Int").with_span(span)
                );
            };
            let length = match args.get(1) {
                Some(LoweredValue::Int(value)) => Some(*value),
                Some(_) => {
                    return Err(
                        RuntimeError::new("type-error", "byte_slice length expected Int")
                            .with_span(span),
                    );
                }
                None => None,
            };
            Ok(LoweredValue::Str(lowered_byte_slice_text(
                text_value, *offset, length, span,
            )?))
        }
        "find" if args.len() == 1 || args.len() == 2 => {
            let needle = lowered_str_arg(&args[0], "find", span)?;
            let start = match args.get(1) {
                Some(LoweredValue::Int(value)) => *value,
                Some(_) => {
                    return Err(
                        RuntimeError::new("type-error", "find start expected Int").with_span(span)
                    );
                }
                None => 0,
            };
            Ok(LoweredValue::Int(lowered_find_text_bytes(
                text_value, needle, start,
            )))
        }
        "starts_with" if args.len() == 1 => {
            let prefix = lowered_str_arg(&args[0], "starts_with", span)?;
            Ok(LoweredValue::Bool(text_value.starts_with(prefix)))
        }
        "ends_with" if args.len() == 1 => {
            let suffix = lowered_str_arg(&args[0], "ends_with", span)?;
            Ok(LoweredValue::Bool(text_value.ends_with(suffix)))
        }
        "contains" if args.len() == 1 => {
            let needle = lowered_str_arg(&args[0], "contains", span)?;
            Ok(LoweredValue::Bool(bytes_contains(
                text_value.as_bytes(),
                needle.as_bytes(),
            )))
        }
        _ => Err(
            RuntimeError::new("unsupported-call", "unsupported lowered Str method").with_span(span),
        ),
    }
}

pub(super) fn lowered_trim_bytes_value(
    value: &LoweredValue,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    let (bytes, start, end) = lowered_bytes_parts(value).ok_or_else(|| {
        RuntimeError::new(
            "type-error",
            format!("trim expected Bytes, found {}", value.type_name()),
        )
        .with_span(span)
    })?;
    let slice = &bytes[start..end];
    let trimmed = crate::runtime::text_bytes::trim_bytes(slice);
    // `trimmed` is a subslice of `slice`, so the pointer offset is in-bounds.
    let leading = trimmed.as_ptr() as usize - slice.as_ptr() as usize;
    let view_start = start + leading;
    let view_end = view_start + trimmed.len();
    Ok(lowered_bytes_view_value(bytes, view_start, view_end))
}

pub(super) fn lowered_bytes_lines(
    bytes: &Arc<[u8]>,
    start: usize,
    end: usize,
) -> Vec<LoweredValue> {
    let mut lines = Vec::new();
    let mut cursor = start;
    while cursor < end {
        let newline = memchr::memchr(b'\n', &bytes[cursor..end]).map(|offset| cursor + offset);
        let line_end = newline.unwrap_or(end);
        let view_end = if line_end > cursor && bytes[line_end - 1] == b'\r' {
            line_end - 1
        } else {
            line_end
        };
        lines.push(lowered_bytes_view_value(bytes.clone(), cursor, view_end));
        let Some(newline) = newline else {
            break;
        };
        cursor = newline + 1;
    }
    lines
}

pub(super) fn lowered_bytes_method_value(
    receiver: &LoweredValue,
    name: &str,
    args: Vec<LoweredValue>,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    let bytes = lowered_bytes_arg(receiver, name, span)?;
    match name {
        "trim" if args.is_empty() => lowered_trim_bytes_value(receiver, span),
        "lines" if args.is_empty() => {
            let (arc, start, end) =
                lowered_bytes_parts(receiver).expect("checked lowered bytes value");
            Ok(LoweredValue::List(lowered_bytes_lines(&arc, start, end)))
        }
        "count_lines" if args.is_empty() => Ok(LoweredValue::Int(
            crate::runtime::text_bytes::count_lines_bytes(bytes) as i64,
        )),
        "len" if args.is_empty() => Ok(LoweredValue::Int(crate::modules::bytes::len(bytes))),
        "dump" if args.is_empty() || args.len() == 1 => {
            let format = match args.first() {
                Some(value) => lowered_str_arg(value, "dump", span)?,
                None => "canonical",
            };
            Ok(LoweredValue::Str(
                crate::modules::bytes::dump(bytes, format, span)?.into(),
            ))
        }
        "strings" if args.is_empty() || args.len() == 1 => {
            let min_len = match args.first() {
                Some(LoweredValue::Int(value)) => *value,
                Some(_) => {
                    return Err(
                        RuntimeError::new("type-error", "strings min_len expected Int")
                            .with_span(span),
                    );
                }
                None => 4,
            };
            lowered_runtime_list(crate::modules::bytes::strings(bytes, min_len, span)?, span)
        }
        "chunks" if args.len() == 1 => {
            let LoweredValue::Int(size) = args[0] else {
                return Err(RuntimeError::new("type-error", "chunks expected Int").with_span(span));
            };
            lowered_runtime_list(
                crate::modules::bytes::chunks(bytes.to_vec(), size, span)?,
                span,
            )
        }
        "compare" if args.len() == 1 => {
            let right = lowered_bytes_arg(&args[0], "compare", span)?;
            lowered_runtime_any(crate::modules::bytes::compare_record(bytes, right), span)
        }
        "starts_with" if args.len() == 1 => {
            let prefix = lowered_bytes_arg(&args[0], "starts_with", span)?;
            Ok(LoweredValue::Bool(bytes.starts_with(prefix)))
        }
        "ends_with" if args.len() == 1 => {
            let suffix = lowered_bytes_arg(&args[0], "ends_with", span)?;
            Ok(LoweredValue::Bool(bytes.ends_with(suffix)))
        }
        "contains" if args.len() == 1 => {
            let needle = lowered_bytes_arg(&args[0], "contains", span)?;
            Ok(LoweredValue::Bool(bytes_contains(bytes, needle)))
        }
        "lower" if args.is_empty() => Ok(LoweredValue::Bytes(bytes.to_ascii_lowercase().into())),
        "base64" if args.is_empty() => Ok(LoweredValue::Str(
            crate::modules::bytes::base64_encode(bytes).into(),
        )),
        "base32" if args.is_empty() => Ok(LoweredValue::Str(
            crate::modules::bytes::base32_encode(bytes).into(),
        )),
        "md5" if args.is_empty() => Ok(LoweredValue::Digest(Box::new(
            crate::modules::hash::digest_bytes(crate::modules::hash::HashAlgorithm::Md5, bytes),
        ))),
        "sha1" if args.is_empty() => Ok(LoweredValue::Digest(Box::new(
            crate::modules::hash::digest_bytes(crate::modules::hash::HashAlgorithm::Sha1, bytes),
        ))),
        "sha256" if args.is_empty() => Ok(LoweredValue::Digest(Box::new(
            crate::modules::hash::digest_bytes(crate::modules::hash::HashAlgorithm::Sha256, bytes),
        ))),
        "sha512" if args.is_empty() => Ok(LoweredValue::Digest(Box::new(
            crate::modules::hash::digest_bytes(crate::modules::hash::HashAlgorithm::Sha512, bytes),
        ))),
        "byte_at" if args.len() == 1 || args.len() == 2 => {
            let LoweredValue::Int(index) = &args[0] else {
                return Err(RuntimeError::new("type-error", "byte_at expected Int").with_span(span));
            };
            let default = match args.get(1) {
                Some(LoweredValue::Int(value)) => *value,
                Some(_) => {
                    return Err(
                        RuntimeError::new("type-error", "byte_at default expected Int")
                            .with_span(span),
                    );
                }
                None => -1,
            };
            Ok(LoweredValue::Int(
                bytes
                    .get(usize::try_from(*index).unwrap_or(usize::MAX))
                    .copied()
                    .map(i64::from)
                    .unwrap_or(default),
            ))
        }
        "slice" if args.len() == 1 || args.len() == 2 => {
            let LoweredValue::Int(offset) = &args[0] else {
                return Err(RuntimeError::new("type-error", "slice expected Int").with_span(span));
            };
            let length = match args.get(1) {
                Some(LoweredValue::Int(length)) => Some(*length),
                Some(_) => {
                    return Err(RuntimeError::new("type-error", "slice length expected Int")
                        .with_span(span));
                }
                None => None,
            };
            Ok(LoweredValue::Bytes(
                crate::modules::bytes::slice(bytes.to_vec(), *offset, length, span)?.into(),
            ))
        }
        "utf8" if args.is_empty() => match std::str::from_utf8(bytes) {
            Ok(text) => Ok(LoweredValue::ResultOk(Box::new(LoweredValue::Str(
                text.into(),
            )))),
            Err(error) => Ok(LoweredValue::ResultErr(Box::new(Value::Error(Box::new(
                RuntimeError::new(
                    "invalid-utf8",
                    format!(
                        "byte data is not valid UTF-8 at byte {}",
                        error.valid_up_to()
                    ),
                )
                .with_span(span),
            ))))),
        },
        _ => Err(
            RuntimeError::new("unsupported-call", "unsupported lowered Bytes method")
                .with_span(span),
        ),
    }
}

pub(super) fn lowered_digest_method_value(
    digest: Box<crate::runtime::value::DigestValue>,
    name: &str,
    args: Vec<LoweredValue>,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    match name {
        "hex" if args.is_empty() => Ok(LoweredValue::Str(
            crate::modules::hash::digest_hex(digest.as_ref()).into(),
        )),
        "base64" if args.is_empty() => Ok(LoweredValue::Str(
            crate::modules::hash::digest_base64(digest.as_ref()).into(),
        )),
        _ => Err(
            RuntimeError::new("unsupported-call", "unsupported lowered Digest method")
                .with_span(span),
        ),
    }
}

pub(super) fn lowered_regex_method_value(
    regex: RegexValue,
    name: &str,
    args: Vec<LoweredValue>,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    match name {
        "matches" if args.len() == 1 => {
            let text = lowered_str_arg(&args[0], "matches", span)?;
            Ok(LoweredValue::Bool(regex.regex.is_match(text)))
        }
        "find" if args.len() == 1 => {
            let text = lowered_str_arg(&args[0], "find", span)?;
            Ok(LoweredValue::List(
                regex
                    .regex
                    .find_iter(text)
                    .map(|found| {
                        LoweredValue::Record(BTreeMap::from([
                            (Arc::from("start"), LoweredValue::Int(found.start() as i64)),
                            (Arc::from("end"), LoweredValue::Int(found.end() as i64)),
                            (Arc::from("text"), LoweredValue::Str(found.as_str().into())),
                        ]))
                    })
                    .collect(),
            ))
        }
        "captures" if args.len() == 1 => {
            let text = lowered_str_arg(&args[0], "captures", span)?;
            let captures = match regex.regex.captures(text) {
                Some(captures) => captures
                    .iter()
                    .map(|capture| {
                        LoweredValue::Str(capture.map_or("", |matched| matched.as_str()).into())
                    })
                    .collect(),
                None => Vec::new(),
            };
            Ok(LoweredValue::List(captures))
        }
        "replace" if args.len() == 2 => {
            let text = lowered_str_arg(&args[0], "replace", span)?;
            let replacement = lowered_str_arg(&args[1], "replace", span)?;
            Ok(LoweredValue::Str(
                regex
                    .regex
                    .replace_all(text, replacement)
                    .into_owned()
                    .into(),
            ))
        }
        _ => Err(
            RuntimeError::new("unsupported-call", "unsupported lowered Regex method")
                .with_span(span),
        ),
    }
}

pub(super) fn lowered_status_method_value(
    status: ProcessStatus,
    name: &str,
    args: Vec<LoweredValue>,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    match name {
        "exited" if args.is_empty() => Ok(LoweredValue::Bool(matches!(
            status.kind,
            ProcessStatusKind::Exit
        ))),
        "signaled" if args.is_empty() => Ok(LoweredValue::Bool(matches!(
            status.kind,
            ProcessStatusKind::Signal
        ))),
        "exited_with" if args.len() == 1 => {
            let LoweredValue::Int(code) = args[0] else {
                return Err(
                    RuntimeError::new("type-error", "exited_with expected Int").with_span(span)
                );
            };
            Ok(LoweredValue::Bool(status.code == Some(code as i32)))
        }
        "exit_code" if args.is_empty() => {
            if matches!(status.kind, ProcessStatusKind::Exit) {
                Ok(LoweredValue::ResultOk(Box::new(LoweredValue::Int(
                    status.code.unwrap_or_default() as i64,
                ))))
            } else {
                Ok(lowered_result_err("status-kind", "status was not an exit"))
            }
        }
        "signal_number" if args.is_empty() => {
            if matches!(status.kind, ProcessStatusKind::Signal) {
                Ok(LoweredValue::ResultOk(Box::new(LoweredValue::Int(
                    status.code.unwrap_or_default() as i64,
                ))))
            } else {
                Ok(lowered_result_err("status-kind", "status was not a signal"))
            }
        }
        _ => Err(
            RuntimeError::new("unsupported-call", "unsupported lowered Status method")
                .with_span(span),
        ),
    }
}

pub(super) fn lowered_path_method_value(
    path: PathValue,
    name: &str,
    args: Vec<LoweredValue>,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    match name {
        "display" if args.is_empty() => Ok(LoweredValue::Str(path.display().into())),
        "name" if args.is_empty() => path_text_field(&path, "name")
            .map(|value| LoweredValue::Str(value.into()))
            .map_err(|error| error.with_span(span)),
        "ext" if args.is_empty() => path_text_field(&path, "ext")
            .map(|value| LoweredValue::Str(value.into()))
            .map_err(|error| error.with_span(span)),
        "with_ext" if args.len() == 1 => {
            let ext = lowered_str_arg(&args[0], "with_ext", span)?;
            path_with_ext(&path, ext)
                .map(LoweredValue::Path)
                .map_err(|error| error.with_span(span))
        }
        "normalize" if args.is_empty() => normalize_path_value(&path)
            .map(LoweredValue::Path)
            .map_err(|error| error.with_span(span)),
        "parent" if args.is_empty() => path_parent(&path)
            .map(LoweredValue::Path)
            .map_err(|error| error.with_span(span)),
        "strip_prefix" if args.len() == 1 => {
            let LoweredValue::Path(prefix) = &args[0] else {
                return Err(
                    RuntimeError::new("type-error", "strip_prefix expected Path").with_span(span),
                );
            };
            let pathbuf = pathbuf_from_path_value(&path);
            let prefix = pathbuf_from_path_value(prefix);
            match pathbuf.strip_prefix(&prefix) {
                Ok(stripped) if stripped.as_os_str().is_empty() => {
                    PathValue::from_text(".").map(LoweredValue::Path)
                }
                Ok(stripped) => {
                    path_value_from_pathbuf(stripped.to_path_buf()).map(LoweredValue::Path)
                }
                Err(_) => {
                    return Ok(lowered_result_err(
                        "path-prefix",
                        "path does not start with prefix",
                    ));
                }
            }
            .map(|value| LoweredValue::ResultOk(Box::new(value)))
            .map_err(|error| error.with_span(span))
        }
        "relative_to" if args.len() == 1 => {
            let LoweredValue::Path(base) = &args[0] else {
                return Err(
                    RuntimeError::new("type-error", "relative_to expected Path").with_span(span)
                );
            };
            let pathbuf = pathbuf_from_path_value(&path);
            let base_buf = pathbuf_from_path_value(base);
            let relative = match pathbuf.strip_prefix(&base_buf) {
                Ok(stripped) if stripped.as_os_str().is_empty() => PathValue::from_text("."),
                Ok(stripped) => path_value_from_pathbuf(stripped.to_path_buf()),
                Err(_) => Ok(path),
            };
            relative
                .map(LoweredValue::Path)
                .map_err(|error| error.with_span(span))
        }
        _ => Err(
            RuntimeError::new("unsupported-call", "unsupported lowered Path method")
                .with_span(span),
        ),
    }
}

pub(super) fn lowered_record_method_value(
    record: BTreeMap<Arc<str>, LoweredValue>,
    name: &str,
    args: Vec<LoweredValue>,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    match name {
        "has" if args.len() == 1 => {
            let field = lowered_str_arg(&args[0], "has", span)?;
            Ok(LoweredValue::Bool(record.contains_key(field)))
        }
        "get" if args.len() == 1 => {
            let field = lowered_str_arg(&args[0], "get", span)?;
            Ok(match record.get(field).cloned() {
                Some(value) => LoweredValue::ResultOk(Box::new(value)),
                None => lowered_result_err("missing-field", format!("missing field `{field}`")),
            })
        }
        "keys" if args.is_empty() => Ok(LoweredValue::List(
            record
                .keys()
                .map(|key| LoweredValue::Str(key.clone()))
                .collect(),
        )),
        _ => Err(
            RuntimeError::new("unsupported-call", "unsupported lowered Record method")
                .with_span(span),
        ),
    }
}

fn lowered_record_vec_method_value(
    record: &[(Name, LoweredValue)],
    name: &str,
    args: Vec<LoweredValue>,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    lowered_record_vec_method_ref(record, name, args, span).and_then(|value| {
        value.ok_or_else(|| {
            RuntimeError::new("unsupported-call", "unsupported lowered Record method")
                .with_span(span)
        })
    })
}

fn lowered_stats_method_value(
    stats: &LoweredStatsValue,
    name: &str,
    args: Vec<LoweredValue>,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    lowered_stats_method_ref(stats, name, args, span).and_then(|value| {
        value.ok_or_else(|| {
            RuntimeError::new("unsupported-call", "unsupported lowered Record method")
                .with_span(span)
        })
    })
}

fn lowered_inline_stats_method_value(
    blanks: i64,
    code: i64,
    comments: i64,
    name: &str,
    args: Vec<LoweredValue>,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    lowered_inline_stats_method_ref(blanks, code, comments, name, args, span).and_then(|value| {
        value.ok_or_else(|| {
            RuntimeError::new("unsupported-call", "unsupported lowered Record method")
                .with_span(span)
        })
    })
}

pub(super) fn lowered_index_value(
    base: LoweredValue,
    index: LoweredValue,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    match (base, index) {
        (LoweredValue::List(values), LoweredValue::Int(index)) => values
            .get(index as usize)
            .cloned()
            .ok_or_else(|| RuntimeError::new("index-out-of-range", "list index").with_span(span)),
        (LoweredValue::SharedList(values), LoweredValue::Int(index)) => values
            .get(index as usize)
            .cloned()
            .ok_or_else(|| RuntimeError::new("index-out-of-range", "list index").with_span(span)),
        (LoweredValue::Record(fields) | LoweredValue::Module(fields), index)
            if lowered_str_value(&index).is_some() =>
        {
            let index = lowered_str_value(&index).expect("checked string index");
            fields.get(index).cloned().ok_or_else(|| {
                RuntimeError::new("missing-field", index.to_string()).with_span(span)
            })
        }
        (LoweredValue::RecordVec(fields), index) if lowered_str_value(&index).is_some() => {
            let index = lowered_str_value(&index).expect("checked string index");
            lowered_record_vec_get(fields.as_slice(), index)
                .cloned()
                .ok_or_else(|| {
                    RuntimeError::new("missing-field", index.to_string()).with_span(span)
                })
        }
        (
            LoweredValue::Stats {
                blanks,
                code,
                comments,
            },
            index,
        ) if lowered_str_value(&index).is_some() => {
            let index = lowered_str_value(&index).expect("checked string index");
            lowered_inline_stats_field_value(blanks, code, comments, index).ok_or_else(|| {
                RuntimeError::new("missing-field", index.to_string()).with_span(span)
            })
        }
        (LoweredValue::StatsBlob(stats), index) if lowered_str_value(&index).is_some() => {
            let index = lowered_str_value(&index).expect("checked string index");
            lowered_stats_field_value(&stats, index).ok_or_else(|| {
                RuntimeError::new("missing-field", index.to_string()).with_span(span)
            })
        }
        (base, index) => Err(RuntimeError::new(
            "type-error",
            format!(
                "cannot index {} with {}",
                base.type_name(),
                index.type_name()
            ),
        )
        .with_span(span)),
    }
}

pub(super) fn lowered_index_ref(
    base: &LoweredValue,
    index: LoweredValue,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    match (base, index) {
        (LoweredValue::List(values), LoweredValue::Int(index)) => values
            .get(index as usize)
            .cloned()
            .ok_or_else(|| RuntimeError::new("index-out-of-range", "list index").with_span(span)),
        (LoweredValue::SharedList(values), LoweredValue::Int(index)) => values
            .get(index as usize)
            .cloned()
            .ok_or_else(|| RuntimeError::new("index-out-of-range", "list index").with_span(span)),
        (LoweredValue::Record(fields) | LoweredValue::Module(fields), index)
            if lowered_str_value(&index).is_some() =>
        {
            let index = lowered_str_value(&index).expect("checked string index");
            fields.get(index).cloned().ok_or_else(|| {
                RuntimeError::new("missing-field", index.to_string()).with_span(span)
            })
        }
        (LoweredValue::RecordVec(fields), index) if lowered_str_value(&index).is_some() => {
            let index = lowered_str_value(&index).expect("checked string index");
            lowered_record_vec_get(fields.as_slice(), index)
                .cloned()
                .ok_or_else(|| {
                    RuntimeError::new("missing-field", index.to_string()).with_span(span)
                })
        }
        (
            LoweredValue::Stats {
                blanks,
                code,
                comments,
            },
            index,
        ) if lowered_str_value(&index).is_some() => {
            let index = lowered_str_value(&index).expect("checked string index");
            lowered_inline_stats_field_value(*blanks, *code, *comments, index).ok_or_else(|| {
                RuntimeError::new("missing-field", index.to_string()).with_span(span)
            })
        }
        (LoweredValue::StatsBlob(stats), index) if lowered_str_value(&index).is_some() => {
            let index = lowered_str_value(&index).expect("checked string index");
            lowered_stats_field_value(stats, index).ok_or_else(|| {
                RuntimeError::new("missing-field", index.to_string()).with_span(span)
            })
        }
        (base, index) => Err(RuntimeError::new(
            "type-error",
            format!(
                "cannot index {} with {}",
                base.type_name(),
                index.type_name()
            ),
        )
        .with_span(span)),
    }
}

pub(super) fn lowered_slice_value(
    base: LoweredValue,
    start: Option<LoweredValue>,
    end: Option<LoweredValue>,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    fn to_index(
        value: Option<LoweredValue>,
        len: usize,
        span: Span,
    ) -> Result<Option<usize>, RuntimeError> {
        match value {
            None => Ok(None),
            Some(LoweredValue::Int(index)) if index >= 0 => Ok(Some((index as usize).min(len))),
            Some(LoweredValue::Int(index)) => Ok(Some((len as i64 + index).max(0) as usize)),
            Some(value) => Err(RuntimeError::new(
                "type-error",
                format!("slice index expected Int, found {}", value.type_name()),
            )
            .with_span(span)),
        }
    }

    match base {
        LoweredValue::List(values) => {
            let len = values.len();
            let start = to_index(start, len, span)?.unwrap_or(0);
            let end = to_index(end, len, span)?.unwrap_or(len).max(start);
            Ok(LoweredValue::List(values[start..end].to_vec()))
        }
        LoweredValue::SharedList(values) => {
            let len = values.len();
            let start = to_index(start, len, span)?.unwrap_or(0);
            let end = to_index(end, len, span)?.unwrap_or(len).max(start);
            Ok(LoweredValue::List(values[start..end].to_vec()))
        }
        LoweredValue::Str(text) => {
            let len = text.chars().count();
            let start = to_index(start, len, span)?.unwrap_or(0);
            let end = to_index(end, len, span)?.unwrap_or(len).max(start);
            Ok(LoweredValue::Str(
                text.chars()
                    .skip(start)
                    .take(end - start)
                    .collect::<String>()
                    .into(),
            ))
        }
        LoweredValue::StrView(view) => {
            let text = view.as_str();
            let len = text.chars().count();
            let start = to_index(start, len, span)?.unwrap_or(0);
            let end = to_index(end, len, span)?.unwrap_or(len).max(start);
            Ok(LoweredValue::Str(
                text.chars()
                    .skip(start)
                    .take(end - start)
                    .collect::<String>()
                    .into(),
            ))
        }
        LoweredValue::Bytes(bytes) => {
            let len = bytes.len();
            let start = to_index(start, len, span)?.unwrap_or(0);
            let end = to_index(end, len, span)?.unwrap_or(len).max(start);
            Ok(lowered_bytes_view_value(bytes, start, end))
        }
        LoweredValue::BytesView(view) => {
            let bytes = view.as_slice();
            let len = bytes.len();
            let start = to_index(start, len, span)?.unwrap_or(0);
            let end = to_index(end, len, span)?.unwrap_or(len).max(start);
            Ok(LoweredValue::Bytes(bytes[start..end].into()))
        }
        value => Err(RuntimeError::new(
            "type-error",
            format!("cannot slice {}", value.type_name()),
        )
        .with_span(span)),
    }
}

pub(super) fn lowered_list_method_value(
    items: Vec<LoweredValue>,
    name: &str,
    args: Vec<LoweredValue>,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    match name {
        "collect" if args.is_empty() => Ok(LoweredValue::List(items)),
        "len" if args.is_empty() => Ok(LoweredValue::Int(items.len() as i64)),
        "contains" if args.len() == 1 => Ok(LoweredValue::Bool(
            items.iter().any(|item| item == &args[0]),
        )),
        "get" if args.len() == 1 || args.len() == 2 => {
            let LoweredValue::Int(index) = &args[0] else {
                return Err(
                    RuntimeError::new("type-error", "get expected Int index").with_span(span)
                );
            };
            if *index >= 0
                && let Some(value) = items.get(*index as usize).cloned()
            {
                return if args.len() == 2 {
                    Ok(value)
                } else {
                    Ok(LoweredValue::ResultOk(Box::new(value)))
                };
            }
            if args.len() == 2 {
                Ok(args[1].clone())
            } else {
                Ok(lowered_result_err(
                    "index-out-of-bounds",
                    format!("list index {index} is out of bounds"),
                ))
            }
        }
        "push" if args.len() == 1 => {
            let mut items = items;
            items.push(args[0].clone());
            Ok(LoweredValue::List(items))
        }
        "extend" if args.len() == 1 => {
            let mut items = items;
            match &args[0] {
                LoweredValue::List(other) => items.extend(other.iter().cloned()),
                LoweredValue::SharedList(other) => items.extend(other.iter().cloned()),
                other => {
                    return Err(RuntimeError::new(
                        "type-error",
                        format!("extend expected List, found {}", other.type_name()),
                    )
                    .with_span(span));
                }
            }
            Ok(LoweredValue::List(items))
        }
        "join" if args.is_empty() || args.len() == 1 => lowered_join_list(&items, &args, span),
        _ => Err(
            RuntimeError::new("unsupported-call", "unsupported lowered List method")
                .with_span(span),
        ),
    }
}

pub(super) fn lowered_list_method_ref(
    items: &[LoweredValue],
    name: &str,
    args: Vec<LoweredValue>,
    span: Span,
) -> Result<Option<LoweredValue>, RuntimeError> {
    match name {
        "len" if args.is_empty() => Ok(Some(LoweredValue::Int(items.len() as i64))),
        "contains" if args.len() == 1 => Ok(Some(LoweredValue::Bool(
            items.iter().any(|item| item == &args[0]),
        ))),
        "get" if args.len() == 1 || args.len() == 2 => {
            let LoweredValue::Int(index) = &args[0] else {
                return Err(
                    RuntimeError::new("type-error", "get expected Int index").with_span(span)
                );
            };
            if *index >= 0
                && let Some(value) = items.get(*index as usize).cloned()
            {
                return if args.len() == 2 {
                    Ok(Some(value))
                } else {
                    Ok(Some(LoweredValue::ResultOk(Box::new(value))))
                };
            }
            if args.len() == 2 {
                Ok(Some(args[1].clone()))
            } else {
                Ok(Some(lowered_result_err(
                    "index-out-of-bounds",
                    format!("list index {index} is out of bounds"),
                )))
            }
        }
        "join" if args.is_empty() || args.len() == 1 => {
            lowered_join_list(items, &args, span).map(Some)
        }
        "extend" if args.len() == 1 => {
            let other = match &args[0] {
                LoweredValue::List(other) => other.as_slice(),
                LoweredValue::SharedList(other) => other.as_slice(),
                other => {
                    return Err(RuntimeError::new(
                        "type-error",
                        format!("extend expected List, found {}", other.type_name()),
                    )
                    .with_span(span));
                }
            };
            let mut values = Vec::with_capacity(items.len() + other.len());
            values.extend(items.iter().cloned());
            values.extend(other.iter().cloned());
            Ok(Some(LoweredValue::List(values)))
        }
        _ => Ok(None),
    }
}

pub(super) fn lowered_nonnegative_count(
    value: LoweredValue,
    span: Span,
) -> Result<usize, RuntimeError> {
    let LoweredValue::Int(value) = value else {
        return Err(RuntimeError::new("type-error", "pipeline count expected Int").with_span(span));
    };
    if value <= 0 {
        Ok(0)
    } else {
        Ok(value as usize)
    }
}

pub(super) fn lowered_map_method_value(
    map: BTreeMap<String, LoweredValue>,
    name: &str,
    args: Vec<LoweredValue>,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    match name {
        "len" if args.is_empty() => Ok(LoweredValue::Int(map.len() as i64)),
        "has" if args.len() == 1 => {
            let key = lowered_str_arg(&args[0], "has", span)?;
            Ok(LoweredValue::Bool(map.contains_key(key)))
        }
        "get" if args.len() == 1 || args.len() == 2 => {
            let key = lowered_str_arg(&args[0], "get", span)?;
            if let Some(value) = map.get(key).cloned() {
                return if args.len() == 2 {
                    Ok(value)
                } else {
                    Ok(LoweredValue::ResultOk(Box::new(value)))
                };
            }
            if args.len() == 2 {
                Ok(args[1].clone())
            } else {
                Ok(LoweredValue::ResultErr(Box::new(Value::Error(Box::new(
                    RuntimeError::new("map-missing", format!("map has no key `{key}`")),
                )))))
            }
        }
        "set" if args.len() == 2 => {
            let key = lowered_str_arg(&args[0], "set", span)?;
            let mut map = map;
            map.insert(key.to_string(), args[1].clone());
            Ok(LoweredValue::Map(map))
        }
        "remove" if args.len() == 1 => {
            let key = lowered_str_arg(&args[0], "remove", span)?;
            let mut map = map;
            map.remove(key);
            Ok(LoweredValue::Map(map))
        }
        "push" if args.len() == 2 => {
            let key = lowered_str_arg(&args[0], "push", span)?;
            let mut map = map;
            match map.remove(key) {
                Some(LoweredValue::List(mut items)) => {
                    items.push(args[1].clone());
                    map.insert(key.to_string(), LoweredValue::List(items));
                }
                Some(LoweredValue::SharedList(items)) => {
                    let mut items = items.as_ref().clone();
                    items.push(args[1].clone());
                    map.insert(key.to_string(), LoweredValue::List(items));
                }
                Some(other) => {
                    return Err(RuntimeError::new(
                        "type-error",
                        format!("push expected List value, found {}", other.type_name()),
                    )
                    .with_span(span));
                }
                None => {
                    map.insert(key.to_string(), LoweredValue::List(vec![args[1].clone()]));
                }
            }
            Ok(LoweredValue::Map(map))
        }
        "keys" if args.is_empty() => Ok(LoweredValue::List(
            map.keys()
                .map(|key| LoweredValue::Str(key.as_str().into()))
                .collect(),
        )),
        "values" if args.is_empty() => Ok(LoweredValue::List(map.into_values().collect())),
        _ => Err(
            RuntimeError::new("unsupported-call", "unsupported lowered Map method").with_span(span),
        ),
    }
}

pub(super) fn lowered_map_method_ref(
    map: &BTreeMap<String, LoweredValue>,
    name: &str,
    args: Vec<LoweredValue>,
    span: Span,
) -> Result<Option<LoweredValue>, RuntimeError> {
    match name {
        "len" if args.is_empty() => Ok(Some(LoweredValue::Int(map.len() as i64))),
        "has" if args.len() == 1 => {
            let key = lowered_str_arg(&args[0], "has", span)?;
            Ok(Some(LoweredValue::Bool(map.contains_key(key))))
        }
        "get" if args.len() == 1 || args.len() == 2 => {
            let key = lowered_str_arg(&args[0], "get", span)?;
            if let Some(value) = map.get(key).cloned() {
                return if args.len() == 2 {
                    Ok(Some(value))
                } else {
                    Ok(Some(LoweredValue::ResultOk(Box::new(value))))
                };
            }
            if args.len() == 2 {
                Ok(Some(args[1].clone()))
            } else {
                Ok(Some(LoweredValue::ResultErr(Box::new(Value::Error(
                    Box::new(RuntimeError::new(
                        "map-missing",
                        format!("map has no key `{key}`"),
                    )),
                )))))
            }
        }
        "keys" if args.is_empty() => Ok(Some(LoweredValue::List(
            map.keys()
                .map(|key| LoweredValue::Str(key.as_str().into()))
                .collect(),
        ))),
        "values" if args.is_empty() => {
            Ok(Some(LoweredValue::List(map.values().cloned().collect())))
        }
        "remove" if args.len() == 1 => {
            let key = lowered_str_arg(&args[0], "remove", span)?;
            let mut map = map.clone();
            map.remove(key);
            Ok(Some(LoweredValue::Map(map)))
        }
        "push" if args.len() == 2 => {
            let key = lowered_str_arg(&args[0], "push", span)?;
            let mut map = map.clone();
            match map.remove(key) {
                Some(LoweredValue::List(mut items)) => {
                    items.push(args[1].clone());
                    map.insert(key.to_string(), LoweredValue::List(items));
                }
                Some(LoweredValue::SharedList(items)) => {
                    let mut items = items.as_ref().clone();
                    items.push(args[1].clone());
                    map.insert(key.to_string(), LoweredValue::List(items));
                }
                Some(other) => {
                    return Err(RuntimeError::new(
                        "type-error",
                        format!("push expected List value, found {}", other.type_name()),
                    )
                    .with_span(span));
                }
                None => {
                    map.insert(key.to_string(), LoweredValue::List(vec![args[1].clone()]));
                }
            }
            Ok(Some(LoweredValue::Map(map)))
        }
        _ => Ok(None),
    }
}

pub(super) fn lowered_result_method_value(
    result: LoweredValue,
    name: &str,
    args: Vec<LoweredValue>,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    match name {
        "context" if args.len() == 1 || args.len() == 2 => {
            let kind = lowered_str_arg(&args[0], "context kind", span)?;
            let message = if args.len() == 2 {
                Some(lowered_str_arg(&args[1], "context message", span)?.to_string())
            } else {
                None
            };
            let context = ErrorContext {
                kind: kind.to_string(),
                message,
            };
            Ok(match result {
                LoweredValue::ResultOk(value) => LoweredValue::ResultOk(value),
                LoweredValue::ResultErr(error) => {
                    LoweredValue::ResultErr(Box::new(add_error_context(*error, context)))
                }
                _ => unreachable!("lowered Result method expected Result"),
            })
        }
        _ => Err(
            RuntimeError::new("unsupported-call", "unsupported lowered Result method")
                .with_span(span),
        ),
    }
}

pub(super) fn lowered_record_method_ref(
    record: &BTreeMap<Arc<str>, LoweredValue>,
    name: &str,
    args: Vec<LoweredValue>,
    span: Span,
) -> Result<Option<LoweredValue>, RuntimeError> {
    match name {
        "has" if args.len() == 1 => {
            let field = lowered_str_arg(&args[0], "has", span)?;
            Ok(Some(LoweredValue::Bool(record.contains_key(field))))
        }
        "get" if args.len() == 1 => {
            let field = lowered_str_arg(&args[0], "get", span)?;
            Ok(Some(match record.get(field).cloned() {
                Some(value) => LoweredValue::ResultOk(Box::new(value)),
                None => lowered_result_err("missing-field", format!("missing field `{field}`")),
            }))
        }
        "keys" if args.is_empty() => Ok(Some(LoweredValue::List(
            record
                .keys()
                .map(|key| LoweredValue::Str(key.clone()))
                .collect(),
        ))),
        _ => Ok(None),
    }
}

fn lowered_record_vec_method_ref(
    record: &[(Name, LoweredValue)],
    name: &str,
    args: Vec<LoweredValue>,
    span: Span,
) -> Result<Option<LoweredValue>, RuntimeError> {
    match name {
        "has" if args.len() == 1 => {
            let field = lowered_str_arg(&args[0], "has", span)?;
            Ok(Some(LoweredValue::Bool(
                lowered_record_vec_get(record, field).is_some(),
            )))
        }
        "get" if args.len() == 1 => {
            let field = lowered_str_arg(&args[0], "get", span)?;
            Ok(Some(match lowered_record_vec_get(record, field).cloned() {
                Some(value) => LoweredValue::ResultOk(Box::new(value)),
                None => lowered_result_err("missing-field", format!("missing field `{field}`")),
            }))
        }
        "keys" if args.is_empty() => Ok(Some(LoweredValue::List(
            record
                .iter()
                .map(|(key, _)| LoweredValue::Str(Arc::from(key.as_str())))
                .collect(),
        ))),
        _ => Ok(None),
    }
}

fn lowered_stats_method_ref(
    stats: &LoweredStatsValue,
    name: &str,
    args: Vec<LoweredValue>,
    span: Span,
) -> Result<Option<LoweredValue>, RuntimeError> {
    match name {
        "has" if args.len() == 1 => {
            let field = lowered_str_arg(&args[0], "has", span)?;
            Ok(Some(LoweredValue::Bool(
                lowered_stats_field_value(stats, field).is_some(),
            )))
        }
        "get" if args.len() == 1 => {
            let field = lowered_str_arg(&args[0], "get", span)?;
            Ok(Some(match lowered_stats_field_value(stats, field) {
                Some(value) => LoweredValue::ResultOk(Box::new(value)),
                None => lowered_result_err("missing-field", format!("missing field `{field}`")),
            }))
        }
        "keys" if args.is_empty() => Ok(Some(LoweredValue::List(vec![
            LoweredValue::Str(Arc::from("blanks")),
            LoweredValue::Str(Arc::from("blobs")),
            LoweredValue::Str(Arc::from("code")),
            LoweredValue::Str(Arc::from("comments")),
        ]))),
        _ => Ok(None),
    }
}

fn lowered_inline_stats_method_ref(
    blanks: i64,
    code: i64,
    comments: i64,
    name: &str,
    args: Vec<LoweredValue>,
    span: Span,
) -> Result<Option<LoweredValue>, RuntimeError> {
    match name {
        "has" if args.len() == 1 => {
            let field = lowered_str_arg(&args[0], "has", span)?;
            Ok(Some(LoweredValue::Bool(
                lowered_inline_stats_field_value(blanks, code, comments, field).is_some(),
            )))
        }
        "get" if args.len() == 1 => {
            let field = lowered_str_arg(&args[0], "get", span)?;
            Ok(Some(
                match lowered_inline_stats_field_value(blanks, code, comments, field) {
                    Some(value) => LoweredValue::ResultOk(Box::new(value)),
                    None => lowered_result_err("missing-field", format!("missing field `{field}`")),
                },
            ))
        }
        "keys" if args.is_empty() => Ok(Some(LoweredValue::List(vec![
            LoweredValue::Str(Arc::from("blanks")),
            LoweredValue::Str(Arc::from("blobs")),
            LoweredValue::Str(Arc::from("code")),
            LoweredValue::Str(Arc::from("comments")),
        ]))),
        _ => Ok(None),
    }
}

pub(super) fn lowered_method_ref(
    receiver: &LoweredValue,
    name: &str,
    args: Vec<LoweredValue>,
    span: Span,
) -> Result<Option<LoweredValue>, RuntimeError> {
    match receiver {
        LoweredValue::Str(_) | LoweredValue::StrView(_) => {
            lowered_str_method_value(receiver, name, args, span).map(Some)
        }
        LoweredValue::List(items) => lowered_list_method_ref(items, name, args, span),
        LoweredValue::SharedList(items) => lowered_list_method_ref(items, name, args, span),
        LoweredValue::Map(map) => lowered_map_method_ref(map, name, args, span),
        LoweredValue::Record(record) | LoweredValue::Module(record) => {
            lowered_record_method_ref(record, name, args, span)
        }
        LoweredValue::RecordVec(record) => {
            lowered_record_vec_method_ref(record.as_slice(), name, args, span)
        }
        LoweredValue::Stats {
            blanks,
            code,
            comments,
        } => lowered_inline_stats_method_ref(*blanks, *code, *comments, name, args, span),
        LoweredValue::StatsBlob(stats) => lowered_stats_method_ref(stats, name, args, span),
        _ => Ok(None),
    }
}

pub(super) fn lowered_result_err(
    kind: impl Into<String>,
    message: impl Into<String>,
) -> LoweredValue {
    LoweredValue::ResultErr(Box::new(error_constructor(kind, message)))
}
