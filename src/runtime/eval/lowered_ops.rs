//! Lowered value operations and method dispatch.
//!
//! Pure functions over `LoweredValue`/`LoweredType` (no `Evaluator`/`self`),
//! split out of the monolithic `eval.rs`. The IR types live in the parent
//! module and are imported via `super::`.

use super::lower::take_shared;
use super::{
    LoweredReturnKind, LoweredStatsValue, LoweredStrPredicate, LoweredTagValue, LoweredType,
    LoweredValue, add_error_context, bytes_contains, bytes_find, format_duration,
    lowered_bytes_view_value, lowered_inline_stats_field_value, lowered_record_vec_get,
    lowered_stats_field_value, lowered_str_view_value, normalize_path_value, path_parent,
    path_posix_dirname, path_posix_extension, path_text_field, path_value_from_pathbuf,
    path_with_ext, pathbuf_from_path_value,
};
use crate::map_key::{MapKey, MapKeyRef};
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

/// Appends one literal element or the elements of an explicitly spliced list.
/// Capacity failures retain the splice's source attribution.
pub(super) fn append_lowered_list_element(
    output: &mut Vec<LoweredValue>,
    value: LoweredValue,
    splice: bool,
    span: Span,
) -> Result<(), RuntimeError> {
    if splice {
        let items = match value {
            LoweredValue::List(items) => items,
            LoweredValue::SharedList(items) => take_shared(items),
            other => {
                return Err(RuntimeError::new(
                    "type-error",
                    format!(
                        "list literal splice requires List, found {}",
                        other.type_name()
                    ),
                )
                .with_span(span));
            }
        };
        output.try_reserve(items.len()).map_err(|_| {
            RuntimeError::new("list-capacity", "list literal exceeds available capacity")
                .with_span(span)
        })?;
        output.extend(items);
    } else {
        output.try_reserve(1).map_err(|_| {
            RuntimeError::new("list-capacity", "list literal exceeds available capacity")
                .with_span(span)
        })?;
        output.push(value);
    }
    Ok(())
}

pub(super) fn lowered_map_key_ref(
    value: &LoweredValue,
    span: Span,
) -> Result<MapKeyRef<'_>, RuntimeError> {
    let key = match value {
        LoweredValue::Int(value) => MapKeyRef::Int(*value),
        LoweredValue::Bool(value) => MapKeyRef::Bool(*value),
        LoweredValue::Duration(value) => MapKeyRef::Duration(value.millis),
        LoweredValue::Path(value) => MapKeyRef::Path(&value.bytes),
        _ if lowered_str_value(value).is_some() => {
            MapKeyRef::Str(lowered_str_value(value).unwrap())
        }
        _ if lowered_bytes_value(value).is_some() => {
            MapKeyRef::Bytes(lowered_bytes_value(value).unwrap())
        }
        _ => {
            return Err(
                RuntimeError::new("type-error", "Map keys require an ordered scalar value")
                    .with_span(span),
            );
        }
    };
    Ok(key)
}

pub(super) fn lowered_map_key_value(key: &MapKey) -> LoweredValue {
    match key {
        MapKey::Str(value) => LoweredValue::Str(value.clone()),
        MapKey::Int(value) => LoweredValue::Int(*value),
        MapKey::Bool(value) => LoweredValue::Bool(*value),
        MapKey::Bytes(value) => LoweredValue::Bytes(value.clone()),
        MapKey::Path(value) => LoweredValue::Path(PathValue {
            bytes: value.to_vec(),
        }),
        MapKey::Duration(value) => {
            LoweredValue::Duration(crate::runtime::value::DurationValue { millis: *value })
        }
    }
}

pub(super) fn lowered_map_literal_key(
    value: &LoweredValue,
    span: Span,
) -> Result<MapKey, RuntimeError> {
    Ok(lowered_map_key_ref(value, span)?.to_owned())
}

pub(super) fn require_lowered_map_key_domain(
    map: &BTreeMap<MapKey, LoweredValue>,
    key: MapKeyRef<'_>,
    span: Span,
) -> Result<(), RuntimeError> {
    if map
        .first_key_value()
        .is_some_and(|(existing, _)| !existing.as_ref().same_domain(key))
    {
        return Err(RuntimeError::new(
            "type-error",
            "Map keys require one scalar domain without conversion",
        )
        .with_span(span));
    }
    Ok(())
}

pub(super) fn append_lowered_map_literal(
    output: &mut BTreeMap<MapKey, LoweredValue>,
    key: Option<MapKey>,
    value: LoweredValue,
    span: Span,
) -> Result<(), RuntimeError> {
    if let Some(key) = key {
        require_lowered_map_key_domain(output, key.as_ref(), span)?;
        output.insert(key, value);
    } else {
        let LoweredValue::Map(values) = value else {
            return Err(
                RuntimeError::new("type-error", "map literal spreads require Map").with_span(span),
            );
        };
        if let Some((key, _)) = values.first_key_value() {
            require_lowered_map_key_domain(output, key.as_ref(), span)?;
        }
        output.extend(take_shared(values));
    }
    Ok(())
}

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
    if matches!(op, BinaryOp::In | BinaryOp::NotIn) {
        let found = lowered_membership_value(&left, &right, span)?;
        return Ok(LoweredValue::Bool(if op == BinaryOp::In {
            found
        } else {
            !found
        }));
    }
    if op == BinaryOp::Eq {
        return Ok(LoweredValue::Bool(left == right));
    }
    if op == BinaryOp::Ne {
        return Ok(LoweredValue::Bool(left != right));
    }
    if op == BinaryOp::Add && matches!(left, LoweredValue::List(_) | LoweredValue::SharedList(_)) {
        return lowered_method_value(left, "extend", vec![right], span);
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
            _ => Err(
                RuntimeError::new("type-error", "invalid lowered binary operation").with_span(span),
            ),
        };
    }
    if matches!(&left, LoweredValue::Duration(_)) || matches!(&right, LoweredValue::Duration(_)) {
        if let (LoweredValue::Duration(a), LoweredValue::Duration(b)) = (&left, &right) {
            let ordered = match op {
                BinaryOp::Lt => Some(a.millis < b.millis),
                BinaryOp::Le => Some(a.millis <= b.millis),
                BinaryOp::Gt => Some(a.millis > b.millis),
                BinaryOp::Ge => Some(a.millis >= b.millis),
                _ => None,
            };
            if let Some(value) = ordered {
                return Ok(LoweredValue::Bool(value));
            }
        }
        let operand = |value: &LoweredValue| match value {
            LoweredValue::Duration(value) => {
                Ok(crate::duration::DurationOperand::Millis(value.millis))
            }
            LoweredValue::Int(value) => Ok(crate::duration::DurationOperand::Count(*value)),
            _ => Err(
                RuntimeError::new("type-error", "invalid Duration arithmetic dimensions")
                    .with_span(span),
            ),
        };
        return crate::duration::checked_duration_binary(op, operand(&left)?, operand(&right)?)
            .map(|value| match value {
                crate::duration::DurationResult::Millis(millis) => {
                    LoweredValue::Duration(crate::runtime::value::DurationValue { millis })
                }
                crate::duration::DurationResult::Count(value) => LoweredValue::Int(value),
            })
            .map_err(|error| {
                let (code, message) = error.diagnostic();
                RuntimeError::new(code, message).with_span(span)
            });
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
        (
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem,
            LoweredValue::Int(left),
            LoweredValue::Int(right),
        ) => Ok(LoweredValue::Int(checked_int_binary(
            op, left, right, span,
        )?)),
        _ => {
            Err(RuntimeError::new("type-error", "invalid lowered binary operation").with_span(span))
        }
    }
}

/// Membership borrows backing storage, including string and byte views, and
/// tests key presence independently of the value stored under that key.
pub(super) fn lowered_membership_value(
    needle: &LoweredValue,
    container: &LoweredValue,
    span: Span,
) -> Result<bool, RuntimeError> {
    if let (Some(needle), Some(container)) =
        (lowered_str_value(needle), lowered_str_value(container))
    {
        return Ok(bytes_contains(container.as_bytes(), needle.as_bytes()));
    }
    if let (Some(needle), Some(container)) =
        (lowered_bytes_value(needle), lowered_bytes_value(container))
    {
        return Ok(bytes_contains(container, needle));
    }
    match container {
        LoweredValue::List(items) => Ok(items.iter().any(|item| item == needle)),
        LoweredValue::SharedList(items) => Ok(items.iter().any(|item| item == needle)),
        LoweredValue::Map(fields) => {
            let key = lowered_map_key_ref(needle, span)?;
            require_lowered_map_key_domain(fields, key, span)?;
            Ok(key.contains_key(fields))
        }
        LoweredValue::Record(fields) => {
            Ok(fields.contains_key(lowered_str_arg(needle, "in", span)?))
        }
        LoweredValue::RecordVec(fields) => {
            Ok(lowered_record_vec_get(fields, lowered_str_arg(needle, "in", span)?).is_some())
        }
        LoweredValue::Stats { .. } | LoweredValue::StatsBlob(_) => Ok(matches!(
            lowered_str_arg(needle, "in", span)?,
            "blanks" | "blobs" | "code" | "comments"
        )),
        LoweredValue::FsEntry(entry) => Ok(entry.has_field(lowered_str_arg(needle, "in", span)?)),
        LoweredValue::Path(path) => {
            let needle_path = match needle {
                LoweredValue::Path(path) => Some(path.display()),
                _ => None,
            };
            let needle = match needle_path.as_deref() {
                Some(text) => text,
                None => lowered_str_arg(needle, "in", span)?,
            };
            Ok(bytes_contains(path.display().as_bytes(), needle.as_bytes()))
        }
        _ => {
            Err(RuntimeError::new("type-error", "unsupported membership operands").with_span(span))
        }
    }
}

pub(super) fn lowered_assertion_comparison(
    op: BinaryOp,
    left: &LoweredValue,
    right: &LoweredValue,
    span: Span,
) -> Result<bool, RuntimeError> {
    match op {
        BinaryOp::Eq => return Ok(left == right),
        BinaryOp::Ne => return Ok(left != right),
        BinaryOp::In | BinaryOp::NotIn => {
            let found = lowered_membership_value(left, right, span)?;
            return Ok(if op == BinaryOp::In { found } else { !found });
        }
        _ => {}
    }
    let order = if let (Some(left), Some(right)) =
        (lowered_str_value(left), lowered_str_value(right))
    {
        Some(left.cmp(right))
    } else {
        match (left, right) {
            (LoweredValue::Int(left), LoweredValue::Int(right)) => Some(left.cmp(right)),
            (LoweredValue::Float(left), LoweredValue::Float(right)) => left.0.partial_cmp(&right.0),
            (LoweredValue::Duration(left), LoweredValue::Duration(right)) => {
                Some(left.millis.cmp(&right.millis))
            }
            _ => {
                return Err(
                    RuntimeError::new("type-error", "unsupported comparison operands")
                        .with_span(span),
                );
            }
        }
    };
    Ok(match op {
        BinaryOp::Lt => order == Some(Ordering::Less),
        BinaryOp::Le => matches!(order, Some(Ordering::Less | Ordering::Equal)),
        BinaryOp::Gt => order == Some(Ordering::Greater),
        BinaryOp::Ge => matches!(order, Some(Ordering::Greater | Ordering::Equal)),
        _ => {
            return Err(
                RuntimeError::new("indexed-ir", "invalid assertion comparison").with_span(span),
            );
        }
    })
}

/// Failure details are bounded and built only after the predicate failed.
pub(super) fn lowered_assertion_value_detail(value: &LoweredValue) -> String {
    fn render(value: &LoweredValue, depth: usize) -> String {
        if depth == 3 {
            return value.type_name().to_string();
        }
        if let Some(text) = lowered_str_value(value) {
            let mut result = format!("{:?}", text.chars().take(256).collect::<String>());
            if text.chars().nth(256).is_some() {
                result.push('…');
            }
            return result;
        }
        if let Some(bytes) = lowered_bytes_value(value) {
            return format!(
                "{:?}{}",
                &bytes[..bytes.len().min(256)],
                if bytes.len() > 256 { "…" } else { "" }
            );
        }
        fn items(values: &[LoweredValue], depth: usize) -> String {
            let mut result = values
                .iter()
                .take(8)
                .map(|value| render(value, depth + 1))
                .collect::<Vec<_>>()
                .join(", ");
            if values.len() > 8 {
                result.push_str(", …");
            }
            format!("[{result}]")
        }
        fn render_map_key(key: &MapKey, depth: usize) -> String {
            match key {
                MapKey::Str(value) => format!("{:?}", value.chars().take(64).collect::<String>()),
                _ => render(&lowered_map_key_value(key), depth + 1),
            }
        }
        match value {
            LoweredValue::Int(value) => value.to_string(),
            LoweredValue::Float(value) => value.format(),
            LoweredValue::Bool(value) => value.to_string(),
            LoweredValue::Null => "null".to_string(),
            LoweredValue::Path(path) => format!(
                "p{:?}",
                path.display().chars().take(256).collect::<String>()
            ),
            LoweredValue::List(values) => items(values, depth),
            LoweredValue::SharedList(values) => items(values, depth),
            LoweredValue::Map(fields) => format!(
                "{{{}}}",
                fields
                    .iter()
                    .take(8)
                    .map(|(key, value)| format!(
                        "{}: {}",
                        render_map_key(key, depth),
                        render(value, depth + 1)
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            LoweredValue::Record(fields) => format!(
                "{{{}}}",
                fields
                    .iter()
                    .take(8)
                    .map(|(key, value)| format!(
                        "{}: {}",
                        key.chars().take(64).collect::<String>(),
                        render(value, depth + 1)
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            LoweredValue::RecordVec(fields) => format!(
                "{{{}}}",
                fields
                    .iter()
                    .take(8)
                    .map(|(key, value)| format!(
                        "{}: {}",
                        key.as_str().as_str().chars().take(64).collect::<String>(),
                        render(value, depth + 1)
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            _ => value.type_name().to_string(),
        }
    }
    render(value, 0)
}

#[cfg(test)]
mod assertion_detail_tests {
    use super::{LoweredValue, lowered_assertion_value_detail};
    use crate::symbol::{Name, SymbolOwner};
    use std::sync::Arc;

    #[test]
    fn assertion_record_vec_details_bound_field_names() {
        SymbolOwner::new().with_current(|| {
            let field = Name::intern("large_field".repeat(1000));
            let value = LoweredValue::RecordVec(Arc::new(vec![(field, LoweredValue::Null)]));
            let detail = lowered_assertion_value_detail(&value);
            assert!(
                detail.len() < 512,
                "field names must not dominate failure details"
            );
            assert!(detail.starts_with("{large_field"));
            assert!(detail.ends_with(": null}"));
        });
    }
}

pub(super) fn checked_int_binary(
    op: BinaryOp,
    left: i64,
    right: i64,
    span: Span,
) -> Result<i64, RuntimeError> {
    let result = match op {
        BinaryOp::Add => left.checked_add(right),
        BinaryOp::Sub => left.checked_sub(right),
        BinaryOp::Mul => left.checked_mul(right),
        BinaryOp::Div => {
            if right == 0 {
                return Err(
                    RuntimeError::new("division-by-zero", "division by zero").with_span(span)
                );
            }
            left.checked_div(right)
        }
        BinaryOp::Rem => {
            if right == 0 {
                return Err(
                    RuntimeError::new("division-by-zero", "division by zero").with_span(span)
                );
            }
            left.checked_rem(right)
        }
        _ => unreachable!("verified integer binary operation"),
    };
    result.ok_or_else(|| RuntimeError::new("integer-overflow", "integer overflow").with_span(span))
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
    // A receiver typed only at run time can hold a Path, whose predicates
    // compare components rather than bytes.
    if let LoweredValue::Path(path) = value {
        let name = match predicate {
            LoweredStrPredicate::StartsWith => "starts_with",
            LoweredStrPredicate::EndsWith => "ends_with",
        };
        return path_component_predicate(path, name, needle, span);
    }
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
    lowered_membership_value(needle, receiver, span)
}

/// Projected keys must have the ordering promised by their checked scalar,
/// list, or record-field types. Reject unsupported values instead of silently
/// comparing them equal and leaving the stream unsorted.
pub(super) fn lowered_sort_key_orderable(value: &LoweredValue) -> bool {
    match value {
        LoweredValue::Int(_)
        | LoweredValue::Bool(_)
        | LoweredValue::Str(_)
        | LoweredValue::StrView(_)
        | LoweredValue::Bytes(_)
        | LoweredValue::BytesView(_)
        | LoweredValue::Path(_) => true,
        LoweredValue::Record(fields) => fields.values().all(lowered_sort_key_orderable),
        LoweredValue::RecordVec(fields) => fields
            .iter()
            .all(|(_, value)| lowered_sort_key_orderable(value)),
        _ => false,
    }
}

pub(super) fn compare_lowered_sort_keys(left: &LoweredValue, right: &LoweredValue) -> Ordering {
    if let (Some(left), Some(right)) = (lowered_str_value(left), lowered_str_value(right)) {
        return left.cmp(right);
    }
    if let (Some(left), Some(right)) = (lowered_bytes_value(left), lowered_bytes_value(right)) {
        return left.cmp(right);
    }
    match (left, right) {
        (LoweredValue::Int(left), LoweredValue::Int(right)) => left.cmp(right),
        (LoweredValue::Bool(left), LoweredValue::Bool(right)) => left.cmp(right),
        (LoweredValue::Path(left), LoweredValue::Path(right)) => left.bytes.cmp(&right.bytes),
        (
            LoweredValue::Record(_) | LoweredValue::RecordVec(_),
            LoweredValue::Record(_) | LoweredValue::RecordVec(_),
        ) => compare_lowered_record_sort_keys(left, right),
        _ => left.type_name().cmp(right.type_name()),
    }
}

/// Lexicographic record comparison for sort keys: fields compare one by one in
/// sorted field-name order, and a shorter record precedes a longer one when
/// every shared field is equal. Normalizing field order makes the comparison
/// independent of how the record was built (map or vec representation).
fn compare_lowered_record_sort_keys(left: &LoweredValue, right: &LoweredValue) -> Ordering {
    let left_fields = lowered_record_sort_fields(left);
    let right_fields = lowered_record_sort_fields(right);
    for (left_field, right_field) in left_fields.iter().zip(right_fields.iter()) {
        match left_field.0.cmp(&right_field.0) {
            Ordering::Equal => {}
            ordering => return ordering,
        }
        match compare_lowered_sort_keys(left_field.1, right_field.1) {
            Ordering::Equal => {}
            ordering => return ordering,
        }
    }
    left_fields.len().cmp(&right_fields.len())
}

/// Deterministic field-name-ordered view of a record for key comparison.
fn lowered_record_sort_fields(value: &LoweredValue) -> Vec<(Arc<str>, &LoweredValue)> {
    let mut fields = match value {
        LoweredValue::Record(fields) => fields
            .iter()
            .map(|(name, value)| (name.clone(), value))
            .collect::<Vec<(Arc<str>, &LoweredValue)>>(),
        LoweredValue::RecordVec(fields) => fields
            .iter()
            .map(|(name, value)| (Arc::<str>::from(name.as_str().as_str()), value))
            .collect::<Vec<(Arc<str>, &LoweredValue)>>(),
        _ => Vec::new(),
    };
    fields.sort_by(|(left, _), (right, _)| left.cmp(right));
    fields
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
        (LoweredReturnKind::Plain(_), LoweredValue::ResultErr(error)) => {
            let mut error = super::runtime_error_from_value(*error, span);
            error.propagated = true;
            Err(error)
        }
        (LoweredReturnKind::Plain(kind), value) if lowered_value_matches(kind, &value) => Ok(value),
        // The error arm comes first because a `Result[Any]` slot matches every
        // value: without this order a returned `Err` would be wrapped back into
        // `Ok` and the caller would never see the failure.
        (LoweredReturnKind::Result(_), LoweredValue::ResultErr(value)) => {
            Ok(LoweredValue::ResultErr(value))
        }
        (LoweredReturnKind::Result(kind), LoweredValue::ResultOk(value))
            if lowered_value_matches(kind, &value) =>
        {
            Ok(LoweredValue::ResultOk(value))
        }
        (LoweredReturnKind::Result(kind), value) if lowered_value_matches(kind, &value) => {
            Ok(LoweredValue::ResultOk(Box::new(value)))
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
            | (LoweredType::NetJob, LoweredValue::NetJob(_))
            | (LoweredType::FsRoot, LoweredValue::FsRoot(_))
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
        LoweredType::NetJob => "NetJob",
        LoweredType::FsRoot => "FsRoot",
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
        (LoweredType::NetJob, Value::NetJob(value)) => Some(LoweredValue::NetJob(value.clone())),
        (LoweredType::FsRoot, Value::FsRoot(value)) => Some(LoweredValue::FsRoot(value.clone())),
        (LoweredType::Stream, Value::Stream(value)) => Some(LoweredValue::Stream(value.clone())),
        (LoweredType::Pure, Value::Pure(value)) => Some(LoweredValue::Pure(*value)),
        (LoweredType::Proc, Value::Proc(value)) => Some(LoweredValue::Proc(*value)),
        (LoweredType::Error, Value::Error(_)) => Some(LoweredValue::Error(Box::new(value.clone()))),
        (LoweredType::Record, Value::Record(value)) => lowered_record_from_runtime(value),
        (LoweredType::Record, Value::FsEntry(value)) => Some(LoweredValue::FsEntry(value.clone())),
        (LoweredType::Module, Value::Module(value)) => lowered_module_from_runtime(value),
        (LoweredType::List, Value::List(value)) => lowered_list_from_runtime(value),
        (LoweredType::Map, Value::Map(value)) => lowered_map_from_runtime(value),
        (
            LoweredType::Tag,
            Value::Tag {
                type_name,
                name,
                fields,
                wire,
            },
        ) => Some(LoweredValue::Tag(Box::new(LoweredTagValue {
            type_name: *type_name,
            wire: wire.clone(),
            name: name.clone(),
            fields: fields
                .iter()
                .map(lowered_value_from_runtime_any)
                .collect::<Option<Vec<_>>>()?,
        }))),
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
        Value::NetJob(value) => Some(LoweredValue::NetJob(value.clone())),
        Value::FsRoot(value) => Some(LoweredValue::FsRoot(value.clone())),
        Value::Stream(value) => Some(LoweredValue::Stream(value.clone())),
        Value::Pure(value) => Some(LoweredValue::Pure(*value)),
        Value::Proc(value) => Some(LoweredValue::Proc(*value)),
        Value::Error(_) | Value::RunError(_) => Some(LoweredValue::Error(Box::new(value.clone()))),
        Value::Record(value) => lowered_record_from_runtime(value),
        Value::Module(value) => lowered_module_from_runtime(value),
        Value::List(value) => lowered_list_from_runtime(value),
        Value::Map(value) => lowered_map_from_runtime(value),
        Value::Tag {
            type_name,
            name,
            fields,
            wire,
        } => Some(LoweredValue::Tag(Box::new(LoweredTagValue {
            type_name: *type_name,
            wire: wire.clone(),
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
    for (key, value) in value.owned_key_iter() {
        record.insert(key.into_arc(), lowered_value_from_runtime_any(value)?);
    }
    Some(LoweredValue::Record(Arc::new(record)))
}

pub(super) fn lowered_module_from_runtime(value: &RecordMap) -> Option<LoweredValue> {
    let mut module = BTreeMap::new();
    for (key, value) in value.owned_key_iter() {
        module.insert(key.into_arc(), lowered_value_from_runtime_any(value)?);
    }
    Some(LoweredValue::Module(Arc::new(module)))
}

pub(super) fn lowered_list_from_runtime(value: &[Value]) -> Option<LoweredValue> {
    let mut items = Vec::with_capacity(value.len());
    for item in value {
        items.push(lowered_value_from_runtime_any(item)?);
    }
    Some(LoweredValue::List(items))
}

pub(super) fn lowered_map_from_runtime(value: &BTreeMap<MapKey, Value>) -> Option<LoweredValue> {
    let mut map = BTreeMap::new();
    for (key, value) in value {
        map.insert(key.clone(), lowered_value_from_runtime_any(value)?);
    }
    Some(LoweredValue::Map(Arc::new(map)))
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
    let receiver_type = receiver.type_name();
    if matches!(receiver, LoweredValue::Str(_) | LoweredValue::StrView(_)) {
        return lowered_str_method_value(&receiver, name, args, span)
            .map_err(|error| improve_unsupported_method_error(error, receiver_type, name, span));
    }
    if matches!(
        receiver,
        LoweredValue::Bytes(_) | LoweredValue::BytesView(_)
    ) {
        return lowered_bytes_method_value(&receiver, name, args, span)
            .map_err(|error| improve_unsupported_method_error(error, receiver_type, name, span));
    }
    let result = match receiver {
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
                &record
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
            lowered_record_method_value(&record, name, args, span)
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
                // The updating methods (`push`, `extend`) consume their
                // receiver, and the caller has already given this value up:
                // taking the vector out of its `Arc` keeps repeated
                // accumulation linear, where cloning it on every update copies
                // the whole list per element added.
                lowered_list_method_value(take_shared(items), name, args, span)
            }
        }
        LoweredValue::Map(map) => {
            if let Some(value) = lowered_map_method_ref(&map, name, &args, span)? {
                Ok(value)
            } else {
                lowered_map_method_value(take_shared(map), name, args, span)
            }
        }
        LoweredValue::ResultOk(value) => {
            lowered_result_method_value(LoweredValue::ResultOk(value), name, args, span)
        }
        LoweredValue::ResultErr(value) => {
            lowered_result_method_value(LoweredValue::ResultErr(value), name, args, span)
        }
        _ => Err(RuntimeError::new("type-error", "unsupported lowered method").with_span(span)),
    };
    result.map_err(|error| improve_unsupported_method_error(error, receiver_type, name, span))
}

fn improve_unsupported_method_error(
    error: RuntimeError,
    receiver_type: &str,
    name: &str,
    span: Span,
) -> RuntimeError {
    if !error.message.contains("unsupported lowered") {
        return error;
    }
    let candidates = match receiver_type {
        "Str" => crate::modules::MethodReceiver::Str,
        "Bytes" => crate::modules::MethodReceiver::Bytes,
        "Int" => crate::modules::MethodReceiver::Int,
        "Float" => crate::modules::MethodReceiver::Float,
        "Path" => crate::modules::MethodReceiver::Path,
        "List" => crate::modules::MethodReceiver::List,
        "Map" => crate::modules::MethodReceiver::Map,
        "Record" => crate::modules::MethodReceiver::Record,
        "Result" => crate::modules::MethodReceiver::Result,
        _ => return error,
    };
    let mut names = crate::modules::api_spec()
        .method_names(candidates)
        .filter(|candidate| *candidate == name || method_name_is_nearby(name, candidate))
        .map(|candidate| format!("`{candidate}()`"))
        .collect::<Vec<_>>();
    if receiver_type == "Str" && matches!(name, "len" | "length") {
        names = ["byte_len", "count_chars"]
            .into_iter()
            .map(|candidate| format!("`{candidate}()`"))
            .collect();
    }
    let mut message = format!("unknown method `{name}` on {receiver_type}");
    if !names.is_empty() {
        message.push_str(&format!("; candidates: {}", names.join(", ")));
    }
    RuntimeError::new("unsupported-call", message).with_span(span)
}

fn method_name_is_nearby(unknown: &str, candidate: &str) -> bool {
    let distance = edit_distance(unknown, candidate);
    distance <= unknown.chars().count().max(candidate.chars().count()) / 3 + 1
}

fn edit_distance(left: &str, right: &str) -> usize {
    let right = right.chars().collect::<Vec<_>>();
    let mut previous = (0..=right.len()).collect::<Vec<_>>();
    for (left_index, left_char) in left.chars().enumerate() {
        let mut current = vec![left_index + 1];
        for (right_index, right_char) in right.iter().enumerate() {
            let cost = usize::from(left_char != *right_char);
            current.push(
                (current[right_index] + 1)
                    .min(previous[right_index + 1] + 1)
                    .min(previous[right_index] + cost),
            );
        }
        previous = current;
    }
    previous[right.len()]
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
        "bit_and" | "bit_or" | "clear_bits" if args.len() == 1 => {
            let mask = match args.into_iter().next() {
                Some(LoweredValue::Int(mask)) => mask,
                Some(other) => {
                    return Err(RuntimeError::new(
                        "type-error",
                        format!("Int.{name} expected Int, found {}", other.type_name()),
                    )
                    .with_span(span));
                }
                None => unreachable!("one checked bitset argument"),
            };
            if value < 0 || mask < 0 {
                return Err(RuntimeError::new(
                    "integer-bitset",
                    "bitset methods require non-negative Int operands",
                )
                .with_span(span));
            }
            Ok(LoweredValue::Int(match name {
                "bit_and" => value & mask,
                "bit_or" => value | mask,
                "clear_bits" => value & !mask,
                _ => unreachable!("matched bitset method"),
            }))
        }
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
        "words" if args.is_empty() => Ok(LoweredValue::List(
            text_value
                .split_whitespace()
                .map(|word| LoweredValue::Str(word.into()))
                .collect(),
        )),
        "fields" if args.len() <= 1 => {
            let delimiter = match args.first() {
                Some(value) => lowered_str_arg(value, "fields", span)?,
                None => "",
            };
            lowered_runtime_list(
                crate::modules::text::fields_text(text_value, delimiter),
                span,
            )
        }
        "split" if args.len() == 1 || args.len() == 2 => {
            let separator = lowered_str_arg(&args[0], "split", span)?;
            let maxsplit = match args.get(1) {
                Some(LoweredValue::Int(value)) => Some(*value),
                Some(_) => {
                    return Err(
                        RuntimeError::new("type-error", "split maxsplit expected Int")
                            .with_span(span),
                    );
                }
                None => None,
            };
            lowered_runtime_list(
                crate::modules::text::split_text(text_value, separator, maxsplit),
                span,
            )
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
        "parse_int_decimal" if args.is_empty() => {
            match crate::modules::text::parse_int_decimal_text(text_value, span) {
                Ok(value) => Ok(LoweredValue::ResultOk(Box::new(LoweredValue::Int(value)))),
                Err(error) => Ok(LoweredValue::ResultErr(Box::new(Value::Error(Box::new(
                    error,
                ))))),
            }
        }
        "parse_uint" if args.is_empty() => {
            match crate::modules::text::parse_uint_text(text_value, span) {
                Ok(value) => Ok(LoweredValue::ResultOk(Box::new(LoweredValue::Int(value)))),
                Err(error) => Ok(LoweredValue::ResultErr(Box::new(Value::Error(Box::new(
                    error,
                ))))),
            }
        }
        "parse_uint_positive" if args.is_empty() => {
            match crate::modules::text::parse_uint_positive_text(text_value, span) {
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
        "byte_len" if args.is_empty() => Ok(LoweredValue::Int(text_value.len() as i64)),
        "byte_at" if args.len() == 1 => {
            let LoweredValue::Int(index) = &args[0] else {
                return Err(RuntimeError::new("type-error", "byte_at expected Int").with_span(span));
            };
            Ok(usize::try_from(*index)
                .ok()
                .and_then(|index| text_value.as_bytes().get(index))
                .map(|value| LoweredValue::Int(i64::from(*value)))
                .unwrap_or(LoweredValue::Null))
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
            let position = lowered_find_text_bytes(text_value, needle, start);
            Ok(if position < 0 {
                LoweredValue::Null
            } else {
                LoweredValue::Int(position)
            })
        }
        "starts_with" if args.len() == 1 => {
            let prefix = lowered_str_arg(&args[0], "starts_with", span)?;
            Ok(LoweredValue::Bool(text_value.starts_with(prefix)))
        }
        "ends_with" if args.len() == 1 => {
            let suffix = lowered_str_arg(&args[0], "ends_with", span)?;
            Ok(LoweredValue::Bool(text_value.ends_with(suffix)))
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
        "byte_at" if args.len() == 1 => {
            let LoweredValue::Int(index) = &args[0] else {
                return Err(RuntimeError::new("type-error", "byte_at expected Int").with_span(span));
            };
            Ok(usize::try_from(*index)
                .ok()
                .and_then(|index| bytes.get(index))
                .map(|value| LoweredValue::Int(i64::from(*value)))
                .unwrap_or(LoweredValue::Null))
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
                        LoweredValue::Record(Arc::new(BTreeMap::from([
                            (Arc::from("start"), LoweredValue::Int(found.start() as i64)),
                            (Arc::from("end"), LoweredValue::Int(found.end() as i64)),
                            (Arc::from("text"), LoweredValue::Str(found.as_str().into())),
                        ])))
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
        "shell_code" if args.is_empty() => match status.kind {
            ProcessStatusKind::Exit => Ok(LoweredValue::ResultOk(Box::new(LoweredValue::Int(
                status.code.unwrap_or_default() as i64,
            )))),
            ProcessStatusKind::Signal => Ok(LoweredValue::ResultOk(Box::new(LoweredValue::Int(
                128 + status.code.unwrap_or_default() as i64,
            )))),
            ProcessStatusKind::Exec => Ok(lowered_result_err(
                "status-kind",
                "status was neither an exit nor a signal",
            )),
        },
        _ => Err(
            RuntimeError::new("unsupported-call", "unsupported lowered Status method")
                .with_span(span),
        ),
    }
}

/// `Path.starts_with` and `Path.ends_with`: the other path's components must
/// be a leading or trailing run of the receiver's. Components are split the
/// way `strip_prefix` splits them, so `starts_with` holds exactly when
/// `strip_prefix` succeeds.
fn path_component_predicate(
    path: &PathValue,
    name: &str,
    other: &LoweredValue,
    span: Span,
) -> Result<bool, RuntimeError> {
    let LoweredValue::Path(other) = other else {
        return Err(RuntimeError::new(
            "type-error",
            format!("{name} expected Path, found {}", other.type_name()),
        )
        .with_span(span));
    };
    let path = pathbuf_from_path_value(path);
    let other = pathbuf_from_path_value(other);
    Ok(if name == "starts_with" {
        path.starts_with(other)
    } else {
        path.ends_with(other)
    })
}

pub(super) fn lowered_path_method_value(
    path: PathValue,
    name: &str,
    args: Vec<LoweredValue>,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    match name {
        "starts_with" | "ends_with" if args.len() == 1 => {
            path_component_predicate(&path, name, &args[0], span).map(LoweredValue::Bool)
        }
        "bytes" if args.is_empty() => Ok(LoweredValue::Bytes(path.bytes.into())),
        // The same split `path_component_predicate` compares, so a prefix
        // test and a prefix of this list cannot disagree.
        "components" if args.is_empty() => pathbuf_from_path_value(&path)
            .components()
            .map(|component| {
                let bytes = std::os::unix::ffi::OsStrExt::as_bytes(component.as_os_str());
                PathValue::new(bytes.to_vec()).map(LoweredValue::Path)
            })
            .collect::<Result<Vec<_>, _>>()
            .map(LoweredValue::List)
            .map_err(|error| error.with_span(span)),
        "display" if args.is_empty() => Ok(LoweredValue::Str(path.display().into())),
        "name" if args.is_empty() => path_text_field(&path, "name")
            .map(|value| LoweredValue::Str(value.into()))
            .map_err(|error| error.with_span(span)),
        "basename" if args.is_empty() => path_text_field(&path, "basename")
            .map(|value| LoweredValue::Str(value.into()))
            .map_err(|error| error.with_span(span)),
        "dirname" if args.is_empty() => path_posix_dirname(&path)
            .map(LoweredValue::Path)
            .map_err(|error| error.with_span(span)),
        "ext" if args.is_empty() => path_text_field(&path, "ext")
            .map(|value| LoweredValue::Str(value.into()))
            .map_err(|error| error.with_span(span)),
        "ext_or" if args.len() == 1 => {
            let fallback = lowered_str_arg(&args[0], "ext_or", span)?;
            Ok(LoweredValue::Str(
                path_posix_extension(&path)
                    .unwrap_or_else(|| fallback.to_string())
                    .into(),
            ))
        }
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

/// Evaluate a read-only `Record` method against a borrowed receiver.
///
/// Every method a record supports here reads: `len`, `has`, `get`, and `keys`.
/// Taking the receiver by reference is what keeps a read from copying every
/// entry of the container it reads.
pub(super) fn lowered_record_method_value(
    record: &BTreeMap<Arc<str>, LoweredValue>,
    name: &str,
    args: Vec<LoweredValue>,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    match name {
        "len" if args.is_empty() => Ok(LoweredValue::Int(record.len() as i64)),
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

// All replacements are evaluated and checked before this private snapshot is
// rebuilt. Grouping siblings makes each shared ancestor detach only once.
pub(super) fn lowered_record_update_batch(
    mut base: LoweredValue,
    updates: Vec<(Vec<Name>, LoweredValue, Span)>,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    #[derive(Default)]
    struct UpdateBranch {
        replacement: Option<LoweredValue>,
        fields: BTreeMap<Name, UpdateBranch>,
    }
    fn apply(
        value: &mut LoweredValue,
        branch: UpdateBranch,
        span: Span,
    ) -> Result<(), RuntimeError> {
        if let Some(replacement) = branch.replacement {
            *value = replacement;
        } else {
            for (field, child) in branch.fields {
                apply(lowered_record_field_mut(value, field, span)?, child, span)?;
            }
        }
        Ok(())
    }
    let mut root = UpdateBranch::default();
    for (path, replacement, field_span) in updates {
        let mut selected = &base;
        let mut branch = &mut root;
        for field in path {
            selected =
                super::lower::lowered_record_field(selected, &field.as_str()).ok_or_else(|| {
                    RuntimeError::new(
                        "missing-field",
                        format!("record update field `{field}` is absent"),
                    )
                    .with_span(field_span)
                })?;
            branch = branch.fields.entry(field).or_default();
        }
        branch.replacement = Some(replacement);
    }
    apply(&mut base, root, span)?;
    Ok(base)
}

/// Selects an existing record field while retaining value semantics.
/// Shared storage is copied only when another value still owns it.
pub(super) fn lowered_record_field_mut(
    value: &mut LoweredValue,
    field: Name,
    span: Span,
) -> Result<&mut LoweredValue, RuntimeError> {
    if matches!(
        value,
        LoweredValue::Stats { .. } | LoweredValue::StatsBlob(_)
    ) {
        let stats = std::mem::replace(value, LoweredValue::Unit);
        *value = LoweredValue::RecordVec(Arc::new(match stats {
            LoweredValue::Stats {
                blanks,
                code,
                comments,
            } => super::lowered_inline_stats_to_record_vec(blanks, code, comments),
            LoweredValue::StatsBlob(stats) => stats.to_record_vec(),
            _ => unreachable!("checked stats record"),
        }));
    }
    let selected = match value {
        LoweredValue::Record(fields) => Arc::make_mut(fields).get_mut(field.as_str().as_str()),
        LoweredValue::RecordVec(fields) => super::lowered_record_vec_get_mut(
            Arc::make_mut(fields).as_mut_slice(),
            field.as_str().as_str(),
        ),
        _ => {
            return Err(
                RuntimeError::new("type-error", "lowered expression expected Record")
                    .with_span(span),
            );
        }
    };
    selected.ok_or_else(|| RuntimeError::new("missing-field", field.to_string()).with_span(span))
}

pub(super) fn lowered_index_value(
    base: LoweredValue,
    index: LoweredValue,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    match (base, index) {
        (LoweredValue::Map(values), index) => {
            let key = lowered_map_key_ref(&index, span)?;
            key.get(&values).cloned().ok_or_else(|| {
                RuntimeError::new("map-missing", format!("map has no key {key:?}")).with_span(span)
            })
        }
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
        value @ (LoweredValue::Str(_) | LoweredValue::StrView(_)) => {
            let (text, view_start, view_end) =
                lowered_str_parts(&value).expect("text slice receiver");
            let slice = &text[view_start..view_end];
            let len = slice.chars().count();
            let start = to_index(start, len, span)?.unwrap_or(0);
            let end = to_index(end, len, span)?.unwrap_or(len).max(start);
            // Bounds count Unicode scalars; the backing view uses UTF-8 bytes.
            let mut boundaries = slice
                .char_indices()
                .map(|(offset, _)| offset)
                .chain(std::iter::once(slice.len()));
            let byte_start = boundaries.nth(start).expect("normalized scalar start");
            let byte_end = if start == end {
                byte_start
            } else {
                boundaries
                    .nth(end - start - 1)
                    .expect("normalized scalar end")
            };
            Ok(lowered_str_view_value(
                text,
                view_start + byte_start,
                view_start + byte_end,
            ))
        }
        value @ (LoweredValue::Bytes(_) | LoweredValue::BytesView(_)) => {
            let (bytes, view_start, view_end) =
                lowered_bytes_parts(&value).expect("byte slice receiver");
            let len = view_end - view_start;
            let start = to_index(start, len, span)?.unwrap_or(0);
            let end = to_index(end, len, span)?.unwrap_or(len).max(start);
            Ok(lowered_bytes_view_value(
                bytes,
                view_start + start,
                view_start + end,
            ))
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
        "get" if args.len() == 1 => {
            let LoweredValue::Int(index) = &args[0] else {
                return Err(
                    RuntimeError::new("type-error", "get expected Int index").with_span(span)
                );
            };
            let result = match usize::try_from(*index)
                .ok()
                .and_then(|index| items.get(index))
            {
                Some(value) => LoweredValue::ResultOk(Box::new(value.clone())),
                None => lowered_result_err(
                    "index-out-of-bounds",
                    format!("list index {index} is out of bounds"),
                ),
            };
            Ok(result)
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
        "get" if args.len() == 1 => {
            let LoweredValue::Int(index) = &args[0] else {
                return Err(
                    RuntimeError::new("type-error", "get expected Int index").with_span(span)
                );
            };
            let result = match usize::try_from(*index)
                .ok()
                .and_then(|index| items.get(index))
            {
                Some(value) => LoweredValue::ResultOk(Box::new(value.clone())),
                None => lowered_result_err(
                    "index-out-of-bounds",
                    format!("list index {index} is out of bounds"),
                ),
            };
            Ok(Some(result))
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

/// Evaluate a read-only `Map` method against a borrowed receiver, or report that
/// the method needs an owned map.
///
/// `len` and `get` only read, so evaluating them against the receiver
/// the caller already holds avoids copying the whole map for one lookup. The
/// updating methods (`set`, `push`, `remove`) return a new map and take the
/// receiver by value instead.
fn lowered_map_method_ref(
    map: &BTreeMap<MapKey, LoweredValue>,
    name: &str,
    args: &[LoweredValue],
    span: Span,
) -> Result<Option<LoweredValue>, RuntimeError> {
    match name {
        "keys" if args.is_empty() => Ok(Some(LoweredValue::List(
            map.keys().map(lowered_map_key_value).collect(),
        ))),
        "values" if args.is_empty() => {
            Ok(Some(LoweredValue::List(map.values().cloned().collect())))
        }
        "len" if args.is_empty() => Ok(Some(LoweredValue::Int(map.len() as i64))),
        "get" if args.len() == 1 => {
            let key = lowered_map_key_ref(&args[0], span)?;
            require_lowered_map_key_domain(map, key, span)?;
            let result = match key.get(map) {
                Some(value) => LoweredValue::ResultOk(Box::new(value.clone())),
                None => lowered_result_err("map-missing", format!("map has no key {key:?}")),
            };
            Ok(Some(result))
        }
        _ => Ok(None),
    }
}

pub(super) fn lowered_map_method_value(
    map: BTreeMap<MapKey, LoweredValue>,
    name: &str,
    args: Vec<LoweredValue>,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    match name {
        "len" if args.is_empty() => Ok(LoweredValue::Int(map.len() as i64)),
        "get" if args.len() == 1 => {
            let key = lowered_map_key_ref(&args[0], span)?;
            require_lowered_map_key_domain(&map, key, span)?;
            let result = match key.get(&map) {
                Some(value) => LoweredValue::ResultOk(Box::new(value.clone())),
                None => lowered_result_err("map-missing", format!("map has no key {key:?}")),
            };
            Ok(result)
        }
        "set" if args.len() == 2 => {
            let key = lowered_map_key_ref(&args[0], span)?;
            require_lowered_map_key_domain(&map, key, span)?;
            let mut map = map;
            map.insert(key.to_owned(), args[1].clone());
            Ok(LoweredValue::Map(Arc::new(map)))
        }
        "remove" if args.len() == 1 => {
            let key = lowered_map_key_ref(&args[0], span)?;
            require_lowered_map_key_domain(&map, key, span)?;
            let mut map = map;
            key.remove(&mut map);
            Ok(LoweredValue::Map(Arc::new(map)))
        }
        "push" if args.len() == 2 => {
            let key = lowered_map_key_ref(&args[0], span)?;
            require_lowered_map_key_domain(&map, key, span)?;
            let mut map = map;
            match key.remove(&mut map) {
                Some(LoweredValue::List(mut items)) => {
                    items.push(args[1].clone());
                    map.insert(key.to_owned(), LoweredValue::List(items));
                }
                Some(LoweredValue::SharedList(items)) => {
                    let mut items = take_shared(items);
                    items.push(args[1].clone());
                    map.insert(key.to_owned(), LoweredValue::List(items));
                }
                Some(other) => {
                    return Err(RuntimeError::new(
                        "type-error",
                        format!("push expected List value, found {}", other.type_name()),
                    )
                    .with_span(span));
                }
                None => {
                    map.insert(key.to_owned(), LoweredValue::List(vec![args[1].clone()]));
                }
            }
            Ok(LoweredValue::Map(Arc::new(map)))
        }
        "keys" if args.is_empty() => Ok(LoweredValue::List(
            map.keys().map(lowered_map_key_value).collect(),
        )),
        "values" if args.is_empty() => Ok(LoweredValue::List(map.into_values().collect())),
        _ => Err(
            RuntimeError::new("unsupported-call", "unsupported lowered Map method").with_span(span),
        ),
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
                span: None,
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

fn lowered_record_vec_method_ref(
    record: &[(Name, LoweredValue)],
    name: &str,
    args: Vec<LoweredValue>,
    span: Span,
) -> Result<Option<LoweredValue>, RuntimeError> {
    match name {
        "len" if args.is_empty() => Ok(Some(LoweredValue::Int(record.len() as i64))),
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
                .map(|(key, _)| LoweredValue::Str(Arc::<str>::from(key.as_str().as_str())))
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

pub(super) fn lowered_result_err(
    kind: impl Into<String>,
    message: impl Into<String>,
) -> LoweredValue {
    LoweredValue::ResultErr(Box::new(error_constructor(kind, message)))
}

#[cfg(test)]
mod slice_tests {
    use std::sync::Arc;

    use super::{LoweredValue, lowered_bytes_parts, lowered_slice_value, lowered_str_parts};
    use crate::source::{SourceId, Span};

    #[test]
    fn nested_text_and_byte_slices_retain_immutable_backing_storage() {
        let span = Span::new(SourceId::new(0), 0, 1);
        let text: Arc<str> = Arc::from("aé🦀z");
        let selected = lowered_slice_value(
            LoweredValue::Str(text.clone()),
            Some(LoweredValue::Int(1)),
            None,
            span,
        )
        .unwrap();
        let selected =
            lowered_slice_value(selected, None, Some(LoweredValue::Int(2)), span).unwrap();
        let (backing, start, end) = lowered_str_parts(&selected).unwrap();
        assert!(Arc::ptr_eq(&text, &backing));
        assert_eq!(&backing[start..end], "é🦀");

        let bytes: Arc<[u8]> = Arc::from(&b"a\0\xffbcd"[..]);
        let selected = lowered_slice_value(
            LoweredValue::Bytes(bytes.clone()),
            Some(LoweredValue::Int(1)),
            Some(LoweredValue::Int(5)),
            span,
        )
        .unwrap();
        let selected = lowered_slice_value(
            selected,
            Some(LoweredValue::Int(1)),
            Some(LoweredValue::Int(-1)),
            span,
        )
        .unwrap();
        let (backing, start, end) = lowered_bytes_parts(&selected).unwrap();
        assert!(Arc::ptr_eq(&bytes, &backing));
        assert_eq!(&backing[start..end], b"\xffb");
    }
}

#[cfg(test)]
mod record_update_tests {
    use super::{LoweredValue, lowered_record_update_batch};
    use crate::source::{SourceId, Span};
    use crate::symbol::Name;
    use std::sync::Arc;

    #[test]
    fn record_update_detaches_shared_ancestors_and_retains_untouched_storage() {
        let symbols = crate::symbol::SymbolOwner::new();
        let _symbols = symbols.enter();
        let span = Span::new(SourceId::new(0), 0, 1);
        let a = Name::intern("a");
        let b = Name::intern("b");
        let c = Name::intern("c");
        let untouched = Name::intern("untouched");
        let inner = Arc::new(vec![(b, LoweredValue::Int(1)), (c, LoweredValue::Int(2))]);
        let retained = Arc::new(vec![(b, LoweredValue::Int(5))]);
        let base = Arc::new(vec![
            (a, LoweredValue::RecordVec(inner.clone())),
            (untouched, LoweredValue::RecordVec(retained.clone())),
        ]);
        let updated = lowered_record_update_batch(
            LoweredValue::RecordVec(base.clone()),
            vec![
                (vec![a, b], LoweredValue::Int(3), span),
                (vec![a, c], LoweredValue::Int(4), span),
            ],
            span,
        )
        .unwrap();
        let LoweredValue::RecordVec(updated) = updated else {
            panic!("record representation");
        };
        assert!(!Arc::ptr_eq(&base, &updated));
        let LoweredValue::RecordVec(changed) = &updated[0].1 else {
            panic!("changed ancestor");
        };
        assert!(!Arc::ptr_eq(&inner, changed));
        assert!(matches!(changed[0].1, LoweredValue::Int(3)));
        assert!(matches!(changed[1].1, LoweredValue::Int(4)));
        assert!(matches!(inner[0].1, LoweredValue::Int(1)));
        let LoweredValue::RecordVec(shared) = &updated[1].1 else {
            panic!("untouched sibling");
        };
        assert!(Arc::ptr_eq(&retained, shared));
    }
}
