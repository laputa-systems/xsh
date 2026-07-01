use crate::runtime::value::{RecordMap, RuntimeError, Value};
use crate::source::Span;
use rustc_hash::FxHashMap;
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MimeEntry {
    pub(crate) mime: String,
    pub(crate) exts: Vec<String>,
}

pub(crate) fn lookup_ext(ext: &str) -> Option<Value> {
    let key = normalize_ext(ext);
    if key.is_empty() {
        return None;
    }
    mime_table().get(&key).cloned().map(mime_entry_value)
}

pub(crate) fn lookup_path(path: &str) -> Option<Value> {
    for ext in path_extensions(path) {
        if let Some(value) = lookup_ext(&ext) {
            return Some(value);
        }
    }
    None
}

pub(crate) fn parse(value: &str, span: Span) -> Result<Value, RuntimeError> {
    let parsed = parse_media_type(value)
        .ok_or_else(|| RuntimeError::new("mime-parse", "malformed media type").with_span(span))?;
    let mut params = BTreeMap::new();
    for (name, value) in parsed.params {
        params.insert(name, Value::Str(value.into()));
    }
    Ok(Value::Record(RecordMap::from([
        (Arc::from("type"), Value::Str(parsed.mime.into())),
        (Arc::from("params"), Value::Map(params)),
    ])))
}

fn mime_table() -> FxHashMap<String, MimeEntry> {
    let mut table = builtin_table();
    if let Ok(text) = std::fs::read_to_string("/etc/mime.types") {
        apply_mime_types(&mut table, &text);
    }
    table
}

fn builtin_table() -> FxHashMap<String, MimeEntry> {
    let mut table = FxHashMap::default();
    for (mime, exts) in [
        ("application/gzip", &["gz"][..]),
        ("application/json", &["json"]),
        ("application/octet-stream", &["bin"]),
        ("application/pdf", &["pdf"]),
        ("application/tar+gzip", &["tar.gz", "tgz"]),
        ("application/x-tar", &["tar"]),
        ("application/xml", &["xml"]),
        ("application/zip", &["zip"]),
        ("image/gif", &["gif"]),
        ("image/jpeg", &["jpg", "jpeg"]),
        ("image/png", &["png"]),
        ("image/svg+xml", &["svg"]),
        ("text/css", &["css"]),
        ("text/csv", &["csv"]),
        ("text/html", &["html", "htm"]),
        ("text/markdown", &["md", "markdown"]),
        ("text/plain", &["txt", "text", "log"]),
        ("text/x-shellscript", &["sh"]),
    ] {
        insert_entry(&mut table, mime, exts.iter().copied());
    }
    table
}

fn apply_mime_types(table: &mut FxHashMap<String, MimeEntry>, text: &str) {
    for line in text.lines() {
        let raw = line
            .split_once('#')
            .map_or(line, |(before, _)| before)
            .trim();
        if raw.is_empty() {
            continue;
        }
        let parts = raw.split_whitespace().collect::<Vec<_>>();
        let Some((mime, exts)) = parts.split_first() else {
            continue;
        };
        if exts.is_empty() || parse_media_type(mime).is_none() {
            continue;
        }
        insert_entry(table, &mime.to_ascii_lowercase(), exts.iter().copied());
    }
}

fn insert_entry<'a>(
    table: &mut FxHashMap<String, MimeEntry>,
    mime: &str,
    exts: impl IntoIterator<Item = &'a str>,
) {
    let exts = exts
        .into_iter()
        .map(normalize_ext)
        .filter(|ext| !ext.is_empty())
        .collect::<Vec<_>>();
    for ext in &exts {
        table.insert(
            ext.clone(),
            MimeEntry {
                mime: mime.to_string(),
                exts: exts.clone(),
            },
        );
    }
}

fn mime_entry_value(entry: MimeEntry) -> Value {
    Value::Record(RecordMap::from([
        (Arc::from("mime"), Value::Str(entry.mime.into())),
        (
            Arc::from("exts"),
            Value::List(
                entry
                    .exts
                    .into_iter()
                    .map(|ext| Value::Str(ext.into()))
                    .collect(),
            ),
        ),
    ]))
}

fn normalize_ext(ext: &str) -> String {
    ext.trim_start_matches('.').to_ascii_lowercase()
}

fn path_extensions(path: &str) -> Vec<String> {
    let Some(name) = path.rsplit('/').next() else {
        return Vec::new();
    };
    let parts = name.split('.').collect::<Vec<_>>();
    if parts.len() < 2 {
        return Vec::new();
    }
    let mut exts = Vec::new();
    for index in 1..parts.len() {
        exts.push(parts[index..].join(".").to_ascii_lowercase());
    }
    exts
}

#[derive(Debug, Eq, PartialEq)]
struct ParsedMediaType {
    mime: String,
    params: BTreeMap<String, String>,
}

fn parse_media_type(value: &str) -> Option<ParsedMediaType> {
    let mut parts = value.split(';');
    let mime = parts.next()?.trim();
    let (ty, subtype) = mime.split_once('/')?;
    if !is_token(ty) || !is_token(subtype) {
        return None;
    }
    let mut params = BTreeMap::new();
    for part in parts {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (name, raw_value) = part.split_once('=')?;
        let name = name.trim().to_ascii_lowercase();
        if !is_token(&name) {
            return None;
        }
        let value = parse_param_value(raw_value.trim())?;
        params.insert(name, value);
    }
    Some(ParsedMediaType {
        mime: format!(
            "{}/{}",
            ty.to_ascii_lowercase(),
            subtype.to_ascii_lowercase()
        ),
        params,
    })
}

fn parse_param_value(value: &str) -> Option<String> {
    if let Some(inner) = value.strip_prefix('"') {
        if !inner.ends_with('"') {
            return None;
        }
        let inner = &inner[..inner.len() - 1];
        let mut output = String::new();
        let mut escaped = false;
        for ch in inner.chars() {
            if escaped {
                output.push(ch);
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                return None;
            } else {
                output.push(ch);
            }
        }
        if escaped {
            return None;
        }
        Some(output)
    } else if is_token(value) {
        Some(value.to_string())
    } else {
        None
    }
}

fn is_token(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| matches!(byte, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'!' | b'#' | b'$' | b'&' | b'^' | b'_' | b'.' | b'+' | b'-'))
}

#[cfg(test)]
mod tests {
    use super::{apply_mime_types, builtin_table, lookup_path, parse_media_type};

    #[test]
    fn lookup_path_prefers_known_multi_extensions() {
        let value = lookup_path("dist/package.tar.gz").unwrap();
        assert_eq!(value.get_field("mime"), Some("application/tar+gzip"));
    }

    #[test]
    fn host_mime_types_override_builtins_by_extension() {
        let mut table = builtin_table();
        apply_mime_types(&mut table, "text/vnd.demo txt demo\n");
        let entry = table.get("txt").unwrap();
        assert_eq!(entry.mime, "text/vnd.demo");
        assert_eq!(entry.exts, vec!["txt", "demo"]);
    }

    #[test]
    fn parse_media_type_normalizes_type_and_params() {
        let parsed = parse_media_type("Text/Plain; Charset=UTF-8; name=\"a b\"").unwrap();
        assert_eq!(parsed.mime, "text/plain");
        assert_eq!(parsed.params["charset"], "UTF-8");
        assert_eq!(parsed.params["name"], "a b");
    }

    #[test]
    fn parse_media_type_rejects_malformed_values() {
        for value in [
            "text",
            "text/",
            "text/plain; bad",
            "text/plain; name=\"open",
        ] {
            assert!(parse_media_type(value).is_none());
        }
    }

    trait FieldText {
        fn get_field(&self, name: &str) -> Option<&str>;
    }

    impl FieldText for crate::runtime::value::Value {
        fn get_field(&self, name: &str) -> Option<&str> {
            let crate::runtime::value::Value::Record(fields) = self else {
                return None;
            };
            let crate::runtime::value::Value::Str(value) = fields.get(name)? else {
                return None;
            };
            Some(value)
        }
    }
}
