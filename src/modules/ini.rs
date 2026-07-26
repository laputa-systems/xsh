use crate::runtime::value::{RecordMap, RuntimeError, Value};
use crate::source::Span;
use std::collections::BTreeMap;
use std::sync::Arc;

pub fn decode(text: &str, span: Span) -> Result<Value, RuntimeError> {
    parse_ini(text, span).map(|fields| Value::Record(record_from_ini(fields)))
}

pub(crate) fn encode(value: &RecordMap, span: Span) -> Result<String, RuntimeError> {
    let mut globals = BTreeMap::new();
    let mut sections = BTreeMap::new();
    for (name, value) in value {
        match value {
            Value::Str(text) => {
                globals.insert(name.to_string(), text.to_string());
            }
            Value::Record(fields) => {
                let mut section = BTreeMap::new();
                for (key, value) in fields {
                    let Value::Str(text) = value else {
                        return Err(RuntimeError::new(
                            "ini-encode",
                            "INI section values must be strings",
                        )
                        .with_span(span));
                    };
                    let key = normalize_key(key);
                    validate_key(&key, span)?;
                    section.insert(key, text.to_string());
                }
                sections.insert(name.to_string(), section);
            }
            _ => {
                return Err(RuntimeError::new(
                    "ini-encode",
                    "INI records may contain only global string keys or section records",
                )
                .with_span(span));
            }
        }
    }

    let mut output = String::new();
    for (key, value) in globals {
        validate_key(&key, span)?;
        write_key_value(&mut output, &key, &value);
    }
    if !output.is_empty() && !sections.is_empty() {
        output.push('\n');
    }
    for (section_index, (section, values)) in sections.into_iter().enumerate() {
        if section_index > 0 {
            output.push('\n');
        }
        validate_section(&section, span)?;
        output.push('[');
        output.push_str(&section);
        output.push_str("]\n");
        for (key, value) in values {
            write_key_value(&mut output, &key, &value);
        }
    }
    Ok(output)
}

type IniData = BTreeMap<String, IniValue>;

#[derive(Debug, Eq, PartialEq)]
enum IniValue {
    Global(String),
    Section(BTreeMap<String, String>),
}

fn parse_ini(text: &str, span: Span) -> Result<IniData, RuntimeError> {
    let mut output = BTreeMap::<String, IniValue>::new();
    let mut current_section: Option<String> = None;
    let mut current_key: Option<String> = None;

    for (index, line) in text.lines().enumerate() {
        let line_number = index + 1;
        let trimmed_start = line.trim_start();
        if trimmed_start.is_empty()
            || trimmed_start.starts_with('#')
            || trimmed_start.starts_with(';')
        {
            continue;
        }

        if line.starts_with(' ') || line.starts_with('\t') {
            let Some(key) = current_key.as_deref() else {
                return Err(ini_error(line_number, "continuation without a key", span));
            };
            let value = value_mut(&mut output, current_section.as_deref(), key, span)?;
            value.push('\n');
            value.push_str(trimmed_start);
            continue;
        }

        if trimmed_start.starts_with('[') {
            let Some(section) = trimmed_start
                .strip_prefix('[')
                .and_then(|rest| rest.strip_suffix(']'))
            else {
                return Err(ini_error(line_number, "malformed section header", span));
            };
            let section = section.trim().to_string();
            validate_section(&section, span)?;
            if matches!(output.get(&section), Some(IniValue::Global(_))) {
                return Err(ini_error(
                    line_number,
                    "section conflicts with global key",
                    span,
                ));
            }
            if output
                .insert(section.clone(), IniValue::Section(BTreeMap::new()))
                .is_some()
            {
                return Err(ini_error(line_number, "duplicate section", span));
            }
            current_section = Some(section);
            current_key = None;
            continue;
        }

        let Some((key, value)) = split_key_value(line) else {
            return Err(ini_error(line_number, "expected key/value entry", span));
        };
        let key = normalize_key(key.trim());
        validate_key(&key, span)?;
        let value = value.trim().to_string();
        match current_section.as_deref() {
            Some(section) => {
                let Some(IniValue::Section(values)) = output.get_mut(section) else {
                    unreachable!("current section exists")
                };
                if values.insert(key.clone(), value).is_some() {
                    return Err(ini_error(line_number, "duplicate key", span));
                }
            }
            None => {
                if matches!(output.get(&key), Some(IniValue::Section(_))) {
                    return Err(ini_error(
                        line_number,
                        "global key conflicts with section",
                        span,
                    ));
                }
                if output
                    .insert(key.clone(), IniValue::Global(value))
                    .is_some()
                {
                    return Err(ini_error(line_number, "duplicate key", span));
                }
            }
        }
        current_key = Some(key);
    }
    Ok(output)
}

fn value_mut<'a>(
    output: &'a mut IniData,
    section: Option<&str>,
    key: &str,
    span: Span,
) -> Result<&'a mut String, RuntimeError> {
    match section {
        Some(section) => {
            let Some(IniValue::Section(values)) = output.get_mut(section) else {
                return Err(RuntimeError::new("ini-decode", "missing section").with_span(span));
            };
            values
                .get_mut(key)
                .ok_or_else(|| RuntimeError::new("ini-decode", "missing key").with_span(span))
        }
        None => {
            let Some(IniValue::Global(value)) = output.get_mut(key) else {
                return Err(RuntimeError::new("ini-decode", "missing key").with_span(span));
            };
            Ok(value)
        }
    }
}

fn record_from_ini(fields: IniData) -> RecordMap {
    let mut record = RecordMap::new();
    for (key, value) in fields {
        let value = match value {
            IniValue::Global(value) => Value::Str(value.into()),
            IniValue::Section(values) => {
                let mut section = RecordMap::new();
                for (key, value) in values {
                    section.insert(Arc::from(key), Value::Str(value.into()));
                }
                Value::Record(section)
            }
        };
        record.insert(Arc::from(key), value);
    }
    record
}

fn split_key_value(line: &str) -> Option<(&str, &str)> {
    let equals = line.find('=');
    let colon = line.find(':');
    let index = match (equals, colon) {
        (Some(left), Some(right)) => left.min(right),
        (Some(index), None) | (None, Some(index)) => index,
        (None, None) => return None,
    };
    Some((&line[..index], &line[index + 1..]))
}

fn normalize_key(key: &str) -> String {
    key.to_ascii_lowercase()
}

fn validate_key(key: &str, span: Span) -> Result<(), RuntimeError> {
    if key.is_empty()
        || key.contains('\0')
        || key.contains('\n')
        || key.contains('[')
        || key.contains(']')
    {
        Err(RuntimeError::new("ini-key", "invalid INI key").with_span(span))
    } else {
        Ok(())
    }
}

fn validate_section(section: &str, span: Span) -> Result<(), RuntimeError> {
    if section.is_empty()
        || section.contains('\0')
        || section.contains('\n')
        || section.contains('[')
        || section.contains(']')
    {
        Err(RuntimeError::new("ini-section", "invalid INI section").with_span(span))
    } else {
        Ok(())
    }
}

fn write_key_value(output: &mut String, key: &str, value: &str) {
    let mut lines = value.split('\n');
    output.push_str(key);
    output.push_str(" = ");
    output.push_str(lines.next().unwrap_or(""));
    output.push('\n');
    for line in lines {
        output.push_str("  ");
        output.push_str(line);
        output.push('\n');
    }
}

fn ini_error(line: usize, message: &str, span: Span) -> RuntimeError {
    RuntimeError::new("ini-decode", format!("line {line}: {message}")).with_span(span)
}

#[cfg(test)]
mod tests {
    use super::{decode, encode};
    use crate::runtime::value::{RecordMap, Value};
    use crate::source::{SourceId, Span};
    use std::sync::Arc;

    fn span() -> Span {
        Span::new(SourceId::new(0), 0, 0)
    }

    #[test]
    fn decode_accepts_globals_sections_comments_colon_and_continuations() {
        let value = decode(
            r#"
# comment
Global: root
[server]
Host = example.test
message = hello
  world
"#,
            span(),
        )
        .unwrap();
        let Value::Record(fields) = value else {
            panic!("record expected");
        };
        assert_eq!(field_str(&fields, "global"), Some("root"));
        let Value::Record(server) = fields.get("server").unwrap() else {
            panic!("server record expected");
        };
        assert_eq!(field_str(server, "host"), Some("example.test"));
        assert_eq!(field_str(server, "message"), Some("hello\nworld"));
    }

    #[test]
    fn decode_rejects_duplicates_case_insensitively() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let error = decode("[s]\nHost = a\nhost = b\n", span()).unwrap_err();
            assert_eq!(error.kind, "ini-decode");
        });
    }

    #[test]
    fn decode_rejects_global_section_collisions() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let error = decode("server = root\n[server]\nhost = x\n", span()).unwrap_err();
            assert_eq!(error.kind, "ini-decode");
        });
    }

    #[test]
    fn encode_is_deterministic_and_multiline() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let value = RecordMap::from([
                (
                    Arc::from("server"),
                    Value::Record(RecordMap::from([
                        (Arc::from("host"), Value::Str("example.test".into())),
                        (Arc::from("message"), Value::Str("hello\nworld".into())),
                    ])),
                ),
                (Arc::from("global"), Value::Str("root".into())),
            ]);
            assert_eq!(
                encode(&value, span()).unwrap(),
                "global = root\n\n[server]\nhost = example.test\nmessage = hello\n  world\n"
            );
        });
    }

    #[test]
    fn encode_rejects_non_string_section_values() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let value = RecordMap::from([(
                Arc::from("s"),
                Value::Record(RecordMap::from([(Arc::from("answer"), Value::Int(42))])),
            )]);
            let error = encode(&value, span()).unwrap_err();
            assert_eq!(error.kind, "ini-encode");
        });
    }

    fn field_str<'a>(fields: &'a RecordMap, key: &str) -> Option<&'a str> {
        let Value::Str(value) = fields.get(key)? else {
            return None;
        };
        Some(value)
    }
}
