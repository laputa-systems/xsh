use crate::map_key::MapKeyRef;
use crate::runtime::value::{RecordMap, RuntimeError, Value};
use crate::source::Span;
use std::collections::BTreeMap;
use std::sync::Arc;

fn btree_map<K: Ord, V>(entries: impl IntoIterator<Item = (K, V)>) -> BTreeMap<K, V> {
    let mut map = BTreeMap::new();
    map.extend(entries);
    map
}


#[allow(clippy::single_call_fn)]
pub(crate) fn parse_json_lines(text: &str, span: Span) -> Result<Vec<Value>, RuntimeError> {
    let mut values = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        values.push(crate::modules::codec::decode_json(line, span)?);
    }
    Ok(values)
}

pub(crate) fn parse_json(text: &str, span: Span) -> Result<Value, RuntimeError> {
    crate::modules::codec::decode_json(text, span)
}

pub(crate) fn encode_json(value: &Value, pretty: bool, span: Span) -> Result<String, RuntimeError> {
    crate::runtime::eval::encode_runtime_json(value, pretty, span)
}

fn require_json_object_keys(
    fields: &std::collections::BTreeMap<crate::map_key::MapKey, Value>,
    span: Span,
) -> Result<(), RuntimeError> {
    if fields.keys().any(|key| key.as_str().is_none()) {
        return Err(
            RuntimeError::new("json-path", "JSON paths require Str map keys")
                .with_span(span),
        );
    }
    Ok(())
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
                Value::Map(fields) => {
                    require_json_object_keys(fields, span)?;
                    MapKeyRef::Str(&key).get(fields).ok_or_else(|| {
                        RuntimeError::new("json-path", format!("missing object key `{key}`"))
                            .with_span(span)
                    })?
                }
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
                require_json_object_keys(&fields, span)?;
                if rest.is_empty() {
                    fields.insert(key.clone().into(), replacement);
                } else {
                    let Some(child) = MapKeyRef::Str(key).remove(&mut fields) else {
                        return Err(RuntimeError::new(
                            "json-path",
                            format!("missing intermediate object key `{key}`"),
                        )
                        .with_span(span));
                    };
                    fields.insert(
                        key.clone().into(),
                        set_at_path(child, rest, replacement, span)?,
                    );
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
                require_json_object_keys(&fields, span)?;
                if rest.is_empty() {
                    if MapKeyRef::Str(key).remove(&mut fields).is_none() {
                        return Err(RuntimeError::new(
                            "json-path",
                            format!("missing object key `{key}`"),
                        )
                        .with_span(span));
                    }
                } else {
                    let Some(child) = MapKeyRef::Str(key).remove(&mut fields) else {
                        return Err(RuntimeError::new(
                            "json-path",
                            format!("missing intermediate object key `{key}`"),
                        )
                        .with_span(span));
                    };
                    fields.insert(key.clone().into(), remove_at_path(child, rest, span)?);
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

pub(crate) fn read_json(path: &std::path::Path, input_path: &crate::runtime::value::PathValue, span: Span) -> Result<Value, RuntimeError> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|error| RuntimeError::host("fs-read", &error).with_span(span))?;
    let mut bytes = Vec::new();
    file.take((crate::modules::codec::INPUT_LIMIT + 1) as u64).read_to_end(&mut bytes)
        .map_err(|error| RuntimeError::host("fs-read", &error).with_span(span))?;
    let result = crate::modules::codec::decode_json_bytes(&bytes, span);
    result.map_err(|mut error| {
        error.payload.insert("input_path".into(), Value::Path(input_path.clone()));
        error
    })
}
