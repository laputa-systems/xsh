#![allow(clippy::single_call_fn)]
#![allow(dead_code)]

use crate::runtime::value::{RuntimeError, Value};
use crate::source::Span;
use rustc_hash::FxHashSet;

pub(crate) fn split_text(text: &str, separator: &str) -> Vec<Value> {
    if separator.is_empty() {
        text.chars()
            .map(|ch| Value::Str(ch.to_string().into()))
            .collect()
    } else {
        text.split(separator)
            .map(|part| Value::Str(part.into()))
            .collect()
    }
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
