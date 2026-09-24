use crate::runtime::value::{FloatValue, RecordMap, RuntimeError, Value};
use crate::source::Span;
use crate::symbol::Name;
use miniserde::json::{Array, Number, Object, Value as JsonValue};
use std::collections::BTreeMap;
use std::sync::Arc;

fn btree_map<K: Ord, V>(entries: impl IntoIterator<Item = (K, V)>) -> BTreeMap<K, V> {
    let mut map = BTreeMap::new();
    map.extend(entries);
    map
}

const JSON_NUMBER_MESSAGE: &str = "JSON numbers must be i64 integers or finite Float values";

#[allow(clippy::single_call_fn)]
pub(crate) fn parse_json_lines(text: &str, span: Span) -> Result<Vec<Value>, RuntimeError> {
    let mut values = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        values.push(parse_json_value(line, span, "json-lines")?);
    }
    Ok(values)
}

pub(crate) fn parse_json(text: &str, span: Span) -> Result<Value, RuntimeError> {
    parse_json_value(text, span, "json")
}

pub(crate) fn encode_json(value: &Value, pretty: bool, span: Span) -> Result<String, RuntimeError> {
    let value = xsh_to_json(value, span)?;
    Ok(if pretty {
        pretty_raw_json(&value)
    } else {
        compact_raw_json(&value)
    })
}

pub(crate) fn encode_json_lines(values: &Value, span: Span) -> Result<String, RuntimeError> {
    let Value::List(items) = values else {
        return Err(RuntimeError::new(
            "type-error",
            format!("expected List, found {}", values.type_name()),
        )
        .with_span(span));
    };
    let mut output = String::new();
    for item in items {
        output.push_str(&encode_json(item, false, span)?);
        output.push('\n');
    }
    Ok(output)
}

pub(crate) fn json_path_get(
    value: &Value,
    path: &Value,
    span: Span,
) -> Result<Value, RuntimeError> {
    let segments = json_path_segments(path, span)?;
    let mut current = value;
    for segment in segments {
        current = match segment {
            JsonPathSegment::Key(key) => match current {
                Value::Record(fields) => fields.get(&key).ok_or_else(|| {
                    RuntimeError::new("json-path", format!("missing object key `{key}`"))
                        .with_span(span)
                })?,
                Value::Map(fields) => fields.get(&key).ok_or_else(|| {
                    RuntimeError::new("json-path", format!("missing object key `{key}`"))
                        .with_span(span)
                })?,
                other => {
                    return Err(RuntimeError::new(
                        "json-path",
                        format!(
                            "expected object at key `{key}`, found {}",
                            other.type_name()
                        ),
                    )
                    .with_span(span));
                }
            },
            JsonPathSegment::Index(index) => match current {
                Value::List(items) => items.get(index).ok_or_else(|| {
                    RuntimeError::new("json-path", format!("list index {index} out of bounds"))
                        .with_span(span)
                })?,
                other => {
                    return Err(RuntimeError::new(
                        "json-path",
                        format!(
                            "expected list at index {index}, found {}",
                            other.type_name()
                        ),
                    )
                    .with_span(span));
                }
            },
        };
    }
    Ok(current.clone())
}

pub(crate) fn json_path_set(
    value: &Value,
    path: &Value,
    replacement: Value,
    span: Span,
) -> Result<Value, RuntimeError> {
    let segments = json_path_segments(path, span)?;
    set_at_path(value.clone(), &segments, replacement, span)
}

pub(crate) fn json_path_remove(
    value: &Value,
    path: &Value,
    span: Span,
) -> Result<Value, RuntimeError> {
    let segments = json_path_segments(path, span)?;
    remove_at_path(value.clone(), &segments, span)
}

#[derive(Clone)]
enum JsonPathSegment {
    Key(String),
    Index(usize),
}

fn json_path_segments(value: &Value, span: Span) -> Result<Vec<JsonPathSegment>, RuntimeError> {
    let Value::List(items) = value else {
        return Err(RuntimeError::new(
            "json-path",
            format!("path expected List, found {}", value.type_name()),
        )
        .with_span(span));
    };
    items
        .iter()
        .map(|item| match item {
            Value::Str(key) => Ok(JsonPathSegment::Key(key.to_string())),
            Value::Int(index) if *index >= 0 => Ok(JsonPathSegment::Index(*index as usize)),
            Value::Int(_) => Err(RuntimeError::new(
                "json-path",
                "path list indexes must be non-negative",
            )
            .with_span(span)),
            other => Err(RuntimeError::new(
                "json-path",
                format!(
                    "path segments must be Str or Int, found {}",
                    other.type_name()
                ),
            )
            .with_span(span)),
        })
        .collect()
}

fn set_at_path(
    value: Value,
    segments: &[JsonPathSegment],
    replacement: Value,
    span: Span,
) -> Result<Value, RuntimeError> {
    let Some((segment, rest)) = segments.split_first() else {
        return Ok(replacement);
    };
    match segment {
        JsonPathSegment::Key(key) => match value {
            Value::Record(fields) => {
                let is_leaf = rest.is_empty();
                let mut map = btree_map(fields);
                if is_leaf {
                    map.insert(Arc::from(key.as_str()), replacement);
                } else {
                    let Some(child) = map.remove(key.as_str()) else {
                        return Err(RuntimeError::new(
                            "json-path",
                            format!("missing intermediate object key `{key}`"),
                        )
                        .with_span(span));
                    };
                    map.insert(
                        Arc::from(key.as_str()),
                        set_at_path(child, rest, replacement, span)?,
                    );
                }
                Ok(Value::Record(RecordMap::from(map)))
            }
            Value::Map(mut fields) => {
                if rest.is_empty() {
                    fields.insert(key.clone(), replacement);
                } else {
                    let Some(child) = fields.remove(key) else {
                        return Err(RuntimeError::new(
                            "json-path",
                            format!("missing intermediate object key `{key}`"),
                        )
                        .with_span(span));
                    };
                    fields.insert(key.clone(), set_at_path(child, rest, replacement, span)?);
                }
                Ok(Value::Map(fields))
            }
            other => Err(RuntimeError::new(
                "json-path",
                format!(
                    "expected object at key `{key}`, found {}",
                    other.type_name()
                ),
            )
            .with_span(span)),
        },
        JsonPathSegment::Index(index) => match value {
            Value::List(mut items) => {
                if *index >= items.len() {
                    return Err(RuntimeError::new(
                        "json-path",
                        format!("list index {index} out of bounds"),
                    )
                    .with_span(span));
                }
                if rest.is_empty() {
                    items[*index] = replacement;
                } else {
                    let child = std::mem::replace(&mut items[*index], Value::Null);
                    items[*index] = set_at_path(child, rest, replacement, span)?;
                }
                Ok(Value::List(items))
            }
            other => Err(RuntimeError::new(
                "json-path",
                format!(
                    "expected list at index {index}, found {}",
                    other.type_name()
                ),
            )
            .with_span(span)),
        },
    }
}

fn remove_at_path(
    value: Value,
    segments: &[JsonPathSegment],
    span: Span,
) -> Result<Value, RuntimeError> {
    let Some((segment, rest)) = segments.split_first() else {
        return Ok(Value::Null);
    };
    match segment {
        JsonPathSegment::Key(key) => match value {
            Value::Record(fields) => {
                let mut map = btree_map(fields);
                if rest.is_empty() {
                    if map.remove(key.as_str()).is_none() {
                        return Err(RuntimeError::new(
                            "json-path",
                            format!("missing object key `{key}`"),
                        )
                        .with_span(span));
                    }
                } else {
                    let Some(child) = map.remove(key.as_str()) else {
                        return Err(RuntimeError::new(
                            "json-path",
                            format!("missing intermediate object key `{key}`"),
                        )
                        .with_span(span));
                    };
                    map.insert(Arc::from(key.as_str()), remove_at_path(child, rest, span)?);
                }
                Ok(Value::Record(RecordMap::from(map)))
            }
            Value::Map(mut fields) => {
                if rest.is_empty() {
                    if fields.remove(key).is_none() {
                        return Err(RuntimeError::new(
                            "json-path",
                            format!("missing object key `{key}`"),
                        )
                        .with_span(span));
                    }
                } else {
                    let Some(child) = fields.remove(key) else {
                        return Err(RuntimeError::new(
                            "json-path",
                            format!("missing intermediate object key `{key}`"),
                        )
                        .with_span(span));
                    };
                    fields.insert(key.clone(), remove_at_path(child, rest, span)?);
                }
                Ok(Value::Map(fields))
            }
            other => Err(RuntimeError::new(
                "json-path",
                format!(
                    "expected object at key `{key}`, found {}",
                    other.type_name()
                ),
            )
            .with_span(span)),
        },
        JsonPathSegment::Index(index) => match value {
            Value::List(mut items) => {
                if *index >= items.len() {
                    return Err(RuntimeError::new(
                        "json-path",
                        format!("list index {index} out of bounds"),
                    )
                    .with_span(span));
                }
                if rest.is_empty() {
                    items.remove(*index);
                } else {
                    let child = std::mem::replace(&mut items[*index], Value::Null);
                    items[*index] = remove_at_path(child, rest, span)?;
                }
                Ok(Value::List(items))
            }
            other => Err(RuntimeError::new(
                "json-path",
                format!(
                    "expected list at index {index}, found {}",
                    other.type_name()
                ),
            )
            .with_span(span)),
        },
    }
}

fn parse_json_value(text: &str, span: Span, kind: &'static str) -> Result<Value, RuntimeError> {
    parse_raw_json(text)
        .map_err(|message| RuntimeError::new(kind, message).with_span(span))
        .and_then(|value| json_to_xsh(value, span, kind))
}

fn json_to_xsh(value: JsonValue, span: Span, kind: &'static str) -> Result<Value, RuntimeError> {
    match value {
        JsonValue::Null => Ok(Value::Null),
        JsonValue::Bool(value) => Ok(Value::Bool(value)),
        JsonValue::Number(Number::I64(value)) => Ok(Value::Int(value)),
        JsonValue::Number(Number::U64(value)) => i64::try_from(value)
            .map(Value::Int)
            .map_err(|_| RuntimeError::new(kind, JSON_NUMBER_MESSAGE).with_span(span)),
        JsonValue::Number(Number::F64(value)) => {
            if value.is_finite()
                && value.fract() == 0.0
                && (-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&value)
                && (value as i64) as f64 == value
            {
                Ok(Value::Int(value as i64))
            } else if value.is_finite() {
                Ok(Value::Float(FloatValue::new(value)))
            } else {
                Err(RuntimeError::new(kind, JSON_NUMBER_MESSAGE).with_span(span))
            }
        }
        JsonValue::String(value) => Ok(Value::Str(value.into())),
        JsonValue::Array(items) => {
            let mut values = Vec::with_capacity(items.len());
            for item in items {
                values.push(json_to_xsh(item, span, kind)?);
            }
            Ok(Value::List(values))
        }
        JsonValue::Object(fields) => {
            let mut values = Vec::with_capacity(fields.len());
            for (key, item) in fields {
                values.push((Name::intern(&key), json_to_xsh(item, span, kind)?));
            }
            Ok(Value::Record(RecordMap::from_name_values(values)))
        }
    }
}

fn xsh_to_json(value: &Value, span: Span) -> Result<JsonValue, RuntimeError> {
    match value {
        Value::Null => Ok(JsonValue::Null),
        Value::Bool(value) => Ok(raw_json_bool(*value)),
        Value::Int(value) => Ok(raw_json_i64(*value)),
        Value::Float(value) if value.0.is_finite() => Ok(raw_json_f64(value.0)),
        Value::Float(_) => Err(RuntimeError::new(
            "json-compatible",
            "non-finite Float values are not JSON-compatible",
        )
        .with_span(span)),
        Value::Str(value) => Ok(raw_json_string(value.as_ref())),
        Value::List(items) => {
            let mut values = Vec::with_capacity(items.len());
            for item in items {
                values.push(xsh_to_json(item, span)?);
            }
            Ok(raw_json_array(values))
        }
        Value::Map(fields) => {
            let mut values = Object::new();
            for (key, item) in fields {
                values.insert(key.clone(), xsh_to_json(item, span)?);
            }
            Ok(JsonValue::Object(values))
        }
        Value::Record(fields) => {
            let mut values = Object::new();
            for (key, item) in fields.iter() {
                values.insert(key.to_string(), xsh_to_json(item, span)?);
            }
            Ok(JsonValue::Object(values))
        }
        value => Err(
            RuntimeError::new("json-compatible", json_compatible_message(value)).with_span(span),
        ),
    }
}

fn json_compatible_message(value: &Value) -> String {
    format!(
        "{} is not JSON-compatible; convert Path, Bytes, Status, Result, and errors explicitly",
        value.type_name()
    )
}

pub fn parse_raw_json(text: &str) -> Result<JsonValue, String> {
    miniserde::json::from_str(text).map_err(|_| "invalid JSON".to_string())
}

pub fn compact_raw_json(value: &JsonValue) -> String {
    miniserde::json::to_string(value)
}

pub fn pretty_raw_json(value: &JsonValue) -> String {
    let mut output = String::new();
    write_pretty_raw_json(value, 0, &mut output);
    output
}

pub fn raw_json_object(fields: impl IntoIterator<Item = (String, JsonValue)>) -> JsonValue {
    let mut object = Object::new();
    for (key, value) in fields {
        object.insert(key, value);
    }
    JsonValue::Object(object)
}

pub fn raw_json_array(items: impl IntoIterator<Item = JsonValue>) -> JsonValue {
    JsonValue::Array(items.into_iter().collect::<Array>())
}

pub fn raw_json_string(value: impl Into<String>) -> JsonValue {
    JsonValue::String(value.into())
}

pub fn raw_json_bool(value: bool) -> JsonValue {
    JsonValue::Bool(value)
}

pub fn raw_json_u64(value: u64) -> JsonValue {
    JsonValue::Number(Number::U64(value))
}

pub fn raw_json_i64(value: i64) -> JsonValue {
    JsonValue::Number(Number::I64(value))
}

pub fn raw_json_usize(value: usize) -> JsonValue {
    JsonValue::Number(Number::U64(value as u64))
}

pub fn raw_json_f64(value: f64) -> JsonValue {
    JsonValue::Number(Number::F64(value))
}

pub fn raw_json_get<'a>(value: &'a JsonValue, key: &str) -> Option<&'a JsonValue> {
    match value {
        JsonValue::Object(object) => object.get(key),
        _ => None,
    }
}

pub fn raw_json_as_str(value: &JsonValue) -> Option<&str> {
    match value {
        JsonValue::String(value) => Some(value),
        _ => None,
    }
}

pub fn raw_json_as_u64(value: &JsonValue) -> Option<u64> {
    match value {
        JsonValue::Number(Number::U64(value)) => Some(*value),
        JsonValue::Number(Number::I64(value)) => u64::try_from(*value).ok(),
        JsonValue::Number(Number::F64(value)) if *value >= 0.0 && value.fract() == 0.0 => {
            Some(*value as u64)
        }
        _ => None,
    }
}

pub fn raw_json_as_bool(value: &JsonValue) -> Option<bool> {
    match value {
        JsonValue::Bool(value) => Some(*value),
        _ => None,
    }
}

fn write_pretty_raw_json(value: &JsonValue, indent: usize, output: &mut String) {
    match value {
        JsonValue::Array(items) if items.is_empty() => output.push_str("[]"),
        JsonValue::Array(items) => {
            output.push('[');
            output.push('\n');
            for (index, item) in items.iter().enumerate() {
                write_indent(indent + 2, output);
                write_pretty_raw_json(item, indent + 2, output);
                if index + 1 != items.len() {
                    output.push(',');
                }
                output.push('\n');
            }
            write_indent(indent, output);
            output.push(']');
        }
        JsonValue::Object(fields) if fields.is_empty() => output.push_str("{}"),
        JsonValue::Object(fields) => {
            output.push('{');
            output.push('\n');
            for (index, (key, item)) in fields.iter().enumerate() {
                write_indent(indent + 2, output);
                output.push_str(&miniserde::json::to_string(key));
                output.push_str(": ");
                write_pretty_raw_json(item, indent + 2, output);
                if index + 1 != fields.len() {
                    output.push(',');
                }
                output.push('\n');
            }
            write_indent(indent, output);
            output.push('}');
        }
        _ => output.push_str(&miniserde::json::to_string(value)),
    }
}

fn write_indent(indent: usize, output: &mut String) {
    for _ in 0..indent {
        output.push(' ');
    }
}
