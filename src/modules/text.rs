#![allow(clippy::single_call_fn)]

use crate::runtime::value::{RuntimeError, Value};
use crate::source::Span;
use rustc_hash::FxHashSet;

pub(crate) fn split_text(text: &str, separator: &str, maxsplit: Option<i64>) -> Vec<Value> {
    if separator.is_empty() {
        let Some(maxsplit) = maxsplit.filter(|value| *value >= 0) else {
            return text
                .chars()
                .map(|ch| Value::Str(ch.to_string().into()))
                .collect();
        };
        if maxsplit == 0 {
            return vec![Value::Str(text.into())];
        }
        let chars = text.chars().collect::<Vec<_>>();
        let split_count = (maxsplit as usize).min(chars.len());
        let mut output = chars[..split_count]
            .iter()
            .map(|ch| Value::Str(ch.to_string().into()))
            .collect::<Vec<_>>();
        if split_count < chars.len() {
            output.push(Value::Str(
                chars[split_count..].iter().collect::<String>().into(),
            ));
        }
        return output;
    }

    let Some(maxsplit) = maxsplit.filter(|value| *value >= 0) else {
        return text
            .split(separator)
            .map(|part| Value::Str(part.into()))
            .collect();
    };
    let limit = maxsplit.saturating_add(1) as usize;
    text.splitn(limit, separator)
        .map(|part| Value::Str(part.into()))
        .collect()
}

pub(crate) fn fields_text(text: &str, delimiter: &str) -> Vec<Value> {
    if delimiter.is_empty() {
        text.split_whitespace()
            .map(|field| Value::Str(field.into()))
            .collect()
    } else {
        text.split(delimiter)
            .filter(|field| !field.is_empty())
            .map(|field| Value::Str(field.into()))
            .collect()
    }
}

pub(crate) fn wrap_text(text: &str, width: i64, span: Span) -> Result<Vec<Value>, RuntimeError> {
    if width <= 0 {
        return Err(RuntimeError::new("text-wrap", "width must be positive").with_span(span));
    }
    let width = width as usize;
    let mut output = Vec::new();
    for line in text.lines() {
        wrap_line(line, width, &mut output);
    }
    if text.ends_with('\n') {
        output.push(Value::Str("".into()));
    }
    Ok(output)
}

pub(crate) fn translate_text(text: &str, from: &str, to: &str) -> String {
    // ASCII fast path (e.g. case folding): scan the byte slices directly. Same
    // O(text * from) shape as the char fallback below, but with none of its two
    // per-call `Vec<char>` allocations — which matters when `translate` runs once
    // per stream item. (A 256-byte lookup table would be a loss here: building it
    // every call dwarfs the work of translating a short string like a file ext.)
    if from.len() == to.len() && text.is_ascii() && from.is_ascii() && to.is_ascii() {
        let from = from.as_bytes();
        let to = to.as_bytes();
        return text
            .bytes()
            .map(
                |byte| match from.iter().position(|&candidate| candidate == byte) {
                    Some(index) => to[index] as char,
                    None => byte as char,
                },
            )
            .collect();
    }
    let from = from.chars().collect::<Vec<_>>();
    let to = to.chars().collect::<Vec<_>>();
    text.chars()
        .filter_map(|ch| {
            if let Some(index) = from.iter().position(|candidate| candidate == &ch) {
                to.get(index).copied()
            } else {
                Some(ch)
            }
        })
        .collect()
}

// Unicode case folding. An `is_ascii()` byte fast path was measured to make no
// difference on real workloads (short strings like file extensions), so it isn't
// worth the extra scan + branch — `to_lowercase` is already cheap for them.
pub(crate) fn lower_text(text: &str) -> String {
    text.to_lowercase()
}

pub(crate) fn upper_text(text: &str) -> String {
    text.to_uppercase()
}

pub(crate) fn delete_text(text: &str, chars: &str) -> String {
    let deletes = chars.chars().collect::<FxHashSet<_>>();
    text.chars().filter(|ch| !deletes.contains(ch)).collect()
}

pub(crate) fn squeeze_text(text: &str, chars: &str) -> String {
    let squeezes = chars.chars().collect::<FxHashSet<_>>();
    let squeeze_all = squeezes.is_empty();
    let mut output = String::new();
    let mut previous = None;
    for ch in text.chars() {
        if previous == Some(ch) && (squeeze_all || squeezes.contains(&ch)) {
            continue;
        }
        output.push(ch);
        previous = Some(ch);
    }
    output
}

pub(crate) fn parse_int_text(text: &str, span: Span) -> Result<i64, RuntimeError> {
    let trimmed = text.trim();
    let (negative, digits) = trimmed
        .strip_prefix('-')
        .map_or((false, trimmed), |rest| (true, rest));
    let digits = digits.strip_prefix('+').unwrap_or(digits);
    let (base, digits) = if let Some(rest) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        (16, rest)
    } else if let Some(rest) = digits
        .strip_prefix("0o")
        .or_else(|| digits.strip_prefix("0O"))
    {
        (8, rest)
    } else if let Some(rest) = digits
        .strip_prefix("0b")
        .or_else(|| digits.strip_prefix("0B"))
    {
        (2, rest)
    } else {
        (10, digits)
    };
    let digits = digits.replace('_', "");
    if digits.is_empty() {
        return Err(RuntimeError::new("parse-int", "expected integer").with_span(span));
    }
    let unsigned = i64::from_str_radix(&digits, base).map_err(|_| {
        RuntimeError::new("parse-int", format!("invalid integer `{text}`")).with_span(span)
    })?;
    Ok(if negative { -unsigned } else { unsigned })
}

pub(crate) fn parse_int_decimal_text(text: &str, span: Span) -> Result<i64, RuntimeError> {
    let valid_digits = !text.is_empty()
        && text.bytes().all(|byte| byte.is_ascii_digit())
        && (text == "0" || !text.starts_with('0'));
    if !valid_digits {
        return Err(
            RuntimeError::new("parse-int", format!("invalid integer `{text}`")).with_span(span),
        );
    }
    text.parse::<i64>().map_err(|_| {
        RuntimeError::new("parse-int", format!("invalid integer `{text}`")).with_span(span)
    })
}

pub(crate) fn parse_uint_positive_text(text: &str, span: Span) -> Result<i64, RuntimeError> {
    let trimmed = text.trim();
    if trimmed.is_empty() || !trimmed.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(RuntimeError::new(
            "parse-uint-positive",
            format!("invalid positive integer `{text}`"),
        )
        .with_span(span));
    }
    let value = trimmed.parse::<i64>().map_err(|_| {
        RuntimeError::new(
            "parse-uint-positive",
            format!("invalid positive integer `{text}`"),
        )
        .with_span(span)
    })?;
    if value == 0 {
        return Err(
            RuntimeError::new("parse-uint-positive", "expected positive integer").with_span(span),
        );
    }
    Ok(value)
}

pub(crate) fn parse_uint_text(text: &str, span: Span) -> Result<i64, RuntimeError> {
    let trimmed = text.trim();
    if trimmed.starts_with('+') || trimmed.starts_with('-') {
        return Err(RuntimeError::new("parse-uint", "expected unsigned integer").with_span(span));
    }
    if trimmed.is_empty() || !trimmed.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(
            RuntimeError::new("parse-uint", format!("invalid unsigned integer `{text}`"))
                .with_span(span),
        );
    }
    trimmed.parse::<i64>().map_err(|_| {
        RuntimeError::new("parse-uint", format!("invalid unsigned integer `{text}`"))
            .with_span(span)
    })
}

pub(crate) fn parse_float_text(text: &str, span: Span) -> Result<f64, RuntimeError> {
    let trimmed = text.trim();
    let cleaned = trimmed.replace('_', "");
    if cleaned.is_empty() {
        return Err(RuntimeError::new("parse-float", "expected number").with_span(span));
    }
    cleaned.parse::<f64>().map_err(|_| {
        RuntimeError::new("parse-float", format!("invalid float `{text}`")).with_span(span)
    })
}

fn wrap_line(line: &str, width: usize, output: &mut Vec<Value>) {
    let mut current = String::new();
    for word in line.split_whitespace() {
        for chunk in wrap_word(word, width) {
            if current.is_empty() {
                current.push_str(&chunk);
            } else if current.chars().count() + 1 + chunk.chars().count() <= width {
                current.push(' ');
                current.push_str(&chunk);
            } else {
                output.push(Value::Str(std::mem::take(&mut current).into()));
                current.push_str(&chunk);
            }
        }
    }
    output.push(Value::Str(current.into()));
}

fn wrap_word(word: &str, width: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    for ch in word.chars() {
        if current.chars().count() == width {
            chunks.push(std::mem::take(&mut current));
        }
        current.push(ch);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceId;

    fn test_span() -> Span {
        Span::new(SourceId::new(0), 0, 0)
    }

    fn strings(values: Vec<Value>) -> Vec<String> {
        values
            .into_iter()
            .map(|value| match value {
                Value::Str(text) => text.to_string(),
                other => panic!("expected Str, found {}", other.type_name()),
            })
            .collect()
    }

    #[test]
    fn text_helpers_cover_script_methods() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            assert_eq!(strings(split_text("ab", "", None)), ["a", "b"]);
            assert_eq!(strings(split_text("a,b", ",", None)), ["a", "b"]);
            assert_eq!(strings(split_text("a,b,c", ",", Some(1))), ["a", "b,c"]);
            assert_eq!(strings(split_text("a,b,c", ",", Some(0))), ["a,b,c"]);
            assert_eq!(strings(split_text("a,b,c", ",", Some(-1))), ["a", "b", "c"]);
            assert_eq!(lower_text("HeLLo"), "hello");
            assert_eq!(upper_text("HeLLo"), "HELLO");
            assert_eq!(parse_int_text("0x2a", test_span()).expect("parse int"), 42);
            assert_eq!(
                parse_int_decimal_text("42", test_span()).expect("decimal parse"),
                42
            );
            assert_eq!(
                parse_uint_positive_text("42", test_span()).expect("positive uint"),
                42
            );
            assert_eq!(
                parse_uint_positive_text(" 5 ", test_span()).expect("positive uint"),
                5
            );
            for input in ["0", "+5", "-1", "0x10", "", "9223372036854775808"] {
                assert_eq!(
                    parse_uint_positive_text(input, test_span())
                        .expect_err("invalid positive uint")
                        .kind,
                    "parse-uint-positive"
                );
            }
            for input in ["0x10", "+5", " 5 ", "05", ""] {
                assert_eq!(
                    parse_int_decimal_text(input, test_span())
                        .expect_err("invalid decimal int")
                        .kind,
                    "parse-int"
                );
            }
            assert_eq!(
                parse_int_text("nope", test_span())
                    .expect_err("invalid int")
                    .kind,
                "parse-int"
            );
        });
    }
}
