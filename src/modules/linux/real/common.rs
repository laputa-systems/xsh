use crate::modules::linux::str_value;
use crate::runtime::value::{PathValue, RuntimeError, Value};
use crate::source::Span;
use std::ffi::CString;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::sync::Arc;

pub(super) fn path_value(path: &Path, span: Span) -> Result<PathValue, RuntimeError> {
    PathValue::new(path.as_os_str().as_bytes().to_vec()).map_err(|error| error.with_span(span))
}

pub(super) fn parse_uevent_message(bytes: &[u8], span: Span) -> Result<Value, RuntimeError> {
    let mut action = String::new();
    let mut subsystem = String::new();
    let mut devname = String::new();
    let mut devpath = String::new();
    let mut env = Vec::new();

    for (index, field) in bytes
        .split(|byte| *byte == 0)
        .filter(|field| !field.is_empty())
        .enumerate()
    {
        if let Some(eq) = field.iter().position(|byte| *byte == b'=') {
            let name = uevent_text(&field[..eq], span)?;
            let value = uevent_text(&field[eq + 1..], span)?;
            match name.as_str() {
                "ACTION" => action = value.clone(),
                "SUBSYSTEM" => subsystem = value.clone(),
                "DEVNAME" => devname = value.clone(),
                "DEVPATH" => devpath = value.clone(),
                _ => {}
            }
            env.push(Value::Record(crate::runtime::value::RecordMap::from([
                (Arc::from("name"), str_value(name)),
                (Arc::from("value"), str_value(value)),
            ])));
        } else if index == 0 {
            let header = uevent_text(field, span)?;
            if let Some((header_action, header_devpath)) = header.split_once('@') {
                if action.is_empty() {
                    action = header_action.to_string();
                }
                if devpath.is_empty() {
                    devpath = header_devpath.to_string();
                }
            }
        }
    }

    Ok(Value::Record(crate::runtime::value::RecordMap::from([
        (Arc::from("action"), str_value(action)),
        (Arc::from("subsystem"), str_value(subsystem)),
        (Arc::from("devname"), str_value(devname)),
        (Arc::from("devpath"), str_value(devpath)),
        (Arc::from("env"), Value::List(env)),
    ])))
}

fn uevent_text(bytes: &[u8], span: Span) -> Result<String, RuntimeError> {
    String::from_utf8(bytes.to_vec()).map_err(|_| {
        RuntimeError::new("invalid-utf8", "uevent field is not valid UTF-8").with_span(span)
    })
}

pub(super) fn cstring_text(value: &str, kind: &str, span: Span) -> Result<CString, RuntimeError> {
    CString::new(value).map_err(|_| RuntimeError::new(kind, "value contains NUL").with_span(span))
}

pub(super) fn cstring_path(path: &Path, kind: &str, span: Span) -> Result<CString, RuntimeError> {
    CString::new(path.as_os_str().as_bytes())
        .map_err(|_| RuntimeError::new(kind, "path contains NUL").with_span(span))
}

pub(super) fn ok_unit() -> Value {
    Value::ok(Value::Unit)
}

pub(super) fn io_error(kind: &str, error: io::Error, span: Span) -> Value {
    error_value(kind, error.to_string(), span)
}

pub(super) fn error_value(kind: &str, message: impl Into<String>, span: Span) -> Value {
    Value::err(Value::Error(Box::new(
        RuntimeError::new(kind, message.into()).with_span(span),
    )))
}
