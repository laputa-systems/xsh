#![allow(clippy::single_call_fn)]

#[cfg(feature = "native-tests")]
use super::TestCall;
use super::{Evaluator, module_error};
#[cfg(feature = "native-tests")]
use crate::modules::api_spec;
use crate::modules::net::{NetBody, NetHeader};
use crate::runtime::process::{
    FileRedirectionMode, ProcessInvocation, ProcessRedirection, RedirectionStream,
};
use crate::runtime::value::{
    CommandPlan, CommandRedirection, CommandRedirectionMode, CommandRedirectionStream, PathValue,
    RecordMap, RunError, RuntimeError, Value,
};
#[cfg(feature = "native-tests")]
use crate::sema::records::standard_record_type;
#[cfg(feature = "native-tests")]
use crate::runtime::value::ResultValue;
#[cfg(feature = "native-tests")]
use crate::sema::types::Type;
use crate::source::Span;
use std::collections::BTreeMap;
use std::time::Duration;

pub(in crate::runtime::eval) mod auth;
#[path = "modules/lib/crypt.rs"]
pub(super) mod crypt;
mod fs;
#[path = "modules/linux.rs"]
mod linux_eval;
#[path = "modules/net.rs"]
mod net_eval;
mod process;
#[path = "modules/unix.rs"]
mod unix_eval;

impl Evaluator {
    pub(super) fn invocation_from_command_plan(
        &mut self,
        plan: &CommandPlan,
        span: Span,
    ) -> Result<ProcessInvocation, RuntimeError> {
        let mut env = self.env.snapshot_clone();
        let mut env_overlay = BTreeMap::new();
        for (name, value) in &plan.env {
            if name.is_empty() || name.contains('\0') || name.contains('=') {
                return Err(RuntimeError::new(
                    "env-name",
                    "environment names cannot be empty or contain NUL or `=`",
                )
                .with_span(span));
            }
            if value.contains('\0') {
                return Err(RuntimeError::new(
                    "env-value",
                    "environment values cannot contain NUL",
                )
                .with_span(span));
            }
            env_overlay.insert(name.as_bytes().to_vec(), value.as_bytes().to_vec());
        }
        env.extend(env_overlay.clone());
        let cwd = plan
            .cwd
            .as_ref()
            .map(|cwd| self.host_path(cwd))
            .unwrap_or_else(|| self.cwd.clone());
        let redirections = plan
            .redirections
            .iter()
            .map(|redirection| match redirection {
                CommandRedirection::Input { bytes } => ProcessRedirection::Input { bytes: bytes.clone() },
                CommandRedirection::File { stream, mode, path } => ProcessRedirection::File {
                    stream: match stream {
                        CommandRedirectionStream::Stdin => RedirectionStream::Stdin,
                        CommandRedirectionStream::Stdout => RedirectionStream::Stdout,
                        CommandRedirectionStream::Stderr => RedirectionStream::Stderr,
                    },
                    mode: match mode {
                        CommandRedirectionMode::Read => FileRedirectionMode::Read,
                        CommandRedirectionMode::Write => FileRedirectionMode::Write,
                        CommandRedirectionMode::Append => FileRedirectionMode::Append,
                    },
                    path: self.host_path(path),
                },
            })
            .collect();
        Ok(ProcessInvocation {
            target: plan.target.clone(),
            argv: plan.argv.clone(),
            cwd,
            env,
            env_overlay,
            redirections,
            timeout: plan
                .timeout
                .as_ref()
                .map(|duration| Duration::from_millis(duration.millis)),
            cpu_max: plan.cpu_max,
            accepted_exit_codes: plan.accepted_exit_codes,
        })
    }
}

#[cfg(feature = "native-tests")]
pub(super) fn test_failure(message: impl Into<String>) -> Value {
    Value::err(Value::Error(Box::new(RuntimeError::new(
        "test-fail",
        message.into(),
    ))))
}

#[cfg(feature = "native-tests")]
pub(super) fn test_contains_value(haystack: &Value, needle: &Value) -> bool {
    match (haystack, needle) {
        (Value::Str(haystack), Value::Str(needle)) => haystack.contains(&**needle),
        (Value::Bytes(haystack), Value::Bytes(needle)) => {
            needle.is_empty()
                || haystack
                    .windows(needle.len())
                    .any(|window| window == needle)
        }
        (Value::List(items), needle) => items.iter().any(|item| item == needle),
        _ => false,
    }
}

#[cfg(feature = "native-tests")]
pub(super) fn test_error_kind(value: &Value) -> Option<String> {
    match value {
        Value::Result(ResultValue::Ok(inner)) => test_error_kind(inner),
        Value::Result(ResultValue::Err(error)) => error.error_kind().map(str::to_string),
        value => value.error_kind().map(str::to_string),
    }
}

#[cfg(feature = "native-tests")]
pub(super) fn test_temp_path(
    evaluator: &mut Evaluator,
    ctx: &RecordMap,
    name: &str,
    span: Span,
) -> Result<PathValue, RuntimeError> {
    let root = record_path(ctx, "temp_root", span)?;
    evaluator.test_temp_counter += 1;
    let prefix = if name.trim().is_empty() {
        "tmp".to_string()
    } else {
        name.chars()
            .map(|ch| match ch {
                '/' | '\\' | ':' | '\0' => '_',
                ch => ch,
            })
            .collect()
    };
    root.join_text(&format!("{prefix}-{}", evaluator.test_temp_counter))
        .map_err(|error| error.with_span(span))
}

#[cfg(feature = "native-tests")]
pub(super) fn intercept_test_host_call(
    evaluator: &mut Evaluator,
    op: &str,
    args: RecordMap,
    span: Span,
) -> Option<Value> {
    evaluator.test_calls.push(TestCall {
        op: op.to_string(),
        args: args.clone(),
    });
    let mocks = evaluator.test_mocks.get_mut(op)?;
    for mock in mocks.iter_mut().filter(|mock| mock.remaining > 0) {
        if test_record_matches(&mock.matcher, &args) {
            mock.remaining -= 1;
            return Some(mock.result.clone());
        }
    }
    Some(Value::err(Value::Error(Box::new(
        RuntimeError::new(
            "test-unmatched-mock",
            format!("no mock matched host operation `{op}`"),
        )
        .with_span(span),
    ))))
}

#[cfg(not(feature = "native-tests"))]
pub(super) fn intercept_test_host_call(
    _evaluator: &mut Evaluator,
    _op: &str,
    _args: RecordMap,
    _span: Span,
) -> Option<Value> {
    None
}

#[cfg(feature = "native-tests")]
pub(super) fn test_record_matches(matcher: &RecordMap, actual: &RecordMap) -> bool {
    matcher.iter().all(|(field, expected)| {
        actual
            .get(field.as_ref())
            .is_some_and(|actual| test_value_matches(expected, actual))
    })
}

#[cfg(feature = "native-tests")]
pub(super) fn test_value_matches(expected: &Value, actual: &Value) -> bool {
    match (expected, actual) {
        (Value::Record(expected), Value::Record(actual)) => test_record_matches(expected, actual),
        _ => expected == actual,
    }
}

#[cfg(feature = "native-tests")]
pub(super) fn test_mock_expected_return_type(op: &str) -> Option<Type> {
    if op == "net.start" {
        return standard_record_type("NetResponse")
            .map(|response| Type::Result(Box::new(response), Box::new(Type::Error)));
    }
    let (module, name) = op.split_once('.')?;
    if !matches!(module, "dns" | "net") {
        return None;
    }
    api_spec()
        .module_overloads(module, name)
        .and_then(|overloads| overloads.first())
        .map(|sig| sig.return_ty.clone())
}

#[cfg(feature = "native-tests")]
pub(super) fn test_value_matches_type(value: &Value, ty: &Type) -> bool {
    match ty {
        Type::ReceiverMapKey | Type::ReceiverMapValue | Type::ReceiverMapListItem => false,
        Type::Inference(_) => false,
        Type::Any | Type::Unknown | Type::Invalid => true,
        Type::Null => matches!(value, Value::Null),
        Type::Bool => matches!(value, Value::Bool(_)),
        Type::Int => matches!(value, Value::Int(_)),
        Type::UInt => matches!(value, Value::Int(value) if *value >= 0),
        Type::Float => matches!(value, Value::Float(_)),
        Type::Duration => matches!(value, Value::Duration(_)),
        Type::Str => matches!(value, Value::Str(_)),
        Type::Bytes => matches!(value, Value::Bytes(_)),
        Type::Digest => matches!(value, Value::Digest(_)),
        Type::Regex => matches!(value, Value::Regex(_)),
        Type::Path => matches!(value, Value::Path(_)),
        Type::List(item_ty) => match value {
            Value::List(items) => items
                .iter()
                .all(|item| test_value_matches_type(item, item_ty)),
            _ => false,
        },
        Type::Map(key_ty, item_ty) => match value {
            Value::Map(items) => items.iter().all(|(key, item)| super::map_key_matches_type(key, key_ty) && test_value_matches_type(item, item_ty)),
            _ => false,
        },
        Type::Stream(item_ty) => match value {
            Value::Stream(stream) => stream
                .items
                .iter()
                .all(|item| test_value_matches_type(&item.value, item_ty)),
            _ => false,
        },
        Type::Record(fields) => match value {
            Value::Record(_) if fields.is_empty() => true,
            Value::Record(values) => fields.iter().all(|(field, field_ty)| {
                values
                    .get(&field.as_str())
                    .is_some_and(|value| test_value_matches_type(value, field_ty))
            }),
            _ => false,
        },
        Type::Module(exports) => matches!(value, Value::Module(_)) && exports.is_empty(),
        Type::DynamicModule => matches!(value, Value::Module(_)),
        Type::Result(ok_ty, err_ty) => match value {
            Value::Result(ResultValue::Ok(value)) => test_value_matches_type(value, ok_ty),
            Value::Result(ResultValue::Err(value)) => test_value_matches_type(value, err_ty),
            _ => false,
        },
        Type::Status => matches!(value, Value::Status(_)),
        Type::EnvPathList => matches!(value, Value::EnvPathList),
        Type::Error => matches!(value, Value::Error(_)),
        Type::ErrorFamily(family) => {
            matches!(value, Value::Error(error) if error.family_name() == *family)
        }
        Type::ErrorVariant { family, variant } => {
            matches!(value, Value::Error(error) if error.family_name() == *family && error.variant_name() == *variant)
        }
        Type::ErrorFacet(facet) => {
            matches!(value, Value::Error(error) if error.facets.iter().any(|value| value == facet))
        }
        Type::ProcessError => matches!(value, Value::RunError(_)),
        Type::Pure => matches!(value, Value::Pure(_)),
        Type::Proc => matches!(value, Value::Proc(_)),
        Type::Command => matches!(value, Value::Command(_)),
        Type::ProcessHandle => matches!(value, Value::ProcessHandle(_)),
        Type::NetJob => matches!(value, Value::NetJob(_)),
        Type::FsRoot => matches!(value, Value::FsRoot(_)),
        Type::Unit => matches!(value, Value::Unit),
        Type::Tag(name) => {
            matches!(value, Value::Tag { type_name, .. } if type_name == name)
        }
        Type::Optional(inner) => {
            matches!(value, Value::Null) || test_value_matches_type(value, inner)
        }
    }
}

pub(super) fn net_body_from_record(
    evaluator: &Evaluator,
    record: &RecordMap,
    span: Span,
) -> Result<NetBody, RuntimeError> {
    let body = record.get("body");
    let body_text = record.get("body_text");
    let body_file = record.get("body_file");
    let count = [body, body_text, body_file]
        .into_iter()
        .filter(Option::is_some)
        .count();
    if count > 1 {
        return Err(RuntimeError::new(
            "net-body",
            "only one of body, body_text, or body_file can be set",
        )
        .with_span(span));
    }
    if let Some(value) = body {
        return match value {
            Value::Bytes(bytes) => Ok(NetBody::Bytes(bytes.clone())),
            value => Err(RuntimeError::new(
                "type-error",
                format!("body expected Bytes, found {}", value.type_name()),
            )
            .with_span(span)),
        };
    }
    if let Some(value) = body_text {
        return match value {
            Value::Str(text) => Ok(NetBody::Bytes(text.as_bytes().to_vec())),
            value => Err(RuntimeError::new(
                "type-error",
                format!("body_text expected Str, found {}", value.type_name()),
            )
            .with_span(span)),
        };
    }
    if let Some(value) = body_file {
        let path = value_to_path(value, "body_file", span)?;
        return Ok(NetBody::File(evaluator.host_path(&path)));
    }
    Ok(NetBody::Empty)
}

pub(super) fn record_headers(
    record: &RecordMap,
    span: Span,
) -> Result<Vec<NetHeader>, RuntimeError> {
    let Some(value) = record.get("headers") else {
        return Ok(Vec::new());
    };
    let Value::List(items) = value else {
        return Err(
            RuntimeError::new("type-error", "headers expected List[Record]").with_span(span),
        );
    };
    items
        .iter()
        .map(|item| {
            let Value::Record(fields) = item else {
                return Err(
                    RuntimeError::new("type-error", "headers expected List[Record]")
                        .with_span(span),
                );
            };
            Ok(NetHeader {
                name: record_str(fields, "name", None, span)?,
                value: record_str(fields, "value", None, span)?,
            })
        })
        .collect()
}

pub(super) fn record_str(
    record: &RecordMap,
    name: &str,
    default: Option<&str>,
    span: Span,
) -> Result<String, RuntimeError> {
    match record.get(name) {
        Some(Value::Str(value)) => Ok(value.to_string()),
        Some(value) => Err(RuntimeError::new(
            "type-error",
            format!("{name} expected Str, found {}", value.type_name()),
        )
        .with_span(span)),
        None => default.map(str::to_string).ok_or_else(|| {
            RuntimeError::new("missing-field", format!("missing `{name}`")).with_span(span)
        }),
    }
}

pub(super) fn record_bool(
    record: &RecordMap,
    name: &str,
    default: bool,
    span: Span,
) -> Result<bool, RuntimeError> {
    match record.get(name) {
        Some(Value::Bool(value)) => Ok(*value),
        Some(value) => Err(RuntimeError::new(
            "type-error",
            format!("{name} expected Bool, found {}", value.type_name()),
        )
        .with_span(span)),
        None => Ok(default),
    }
}

pub(super) fn record_path(
    record: &RecordMap,
    name: &str,
    span: Span,
) -> Result<PathValue, RuntimeError> {
    let value = record.get(name).ok_or_else(|| {
        RuntimeError::new("missing-field", format!("missing `{name}`")).with_span(span)
    })?;
    value_to_path(value, name, span)
}

pub(super) fn value_to_path(
    value: &Value,
    name: &str,
    span: Span,
) -> Result<PathValue, RuntimeError> {
    match value {
        Value::Path(path) => Ok(path.clone()),
        Value::Str(text) => PathValue::from_text(text).map_err(|error| error.with_span(span)),
        value => Err(RuntimeError::new(
            "type-error",
            format!("{name} expected Path, found {}", value.type_name()),
        )
        .with_span(span)),
    }
}

pub(super) fn record_duration(
    record: &RecordMap,
    name: &str,
    span: Span,
) -> Result<Option<Duration>, RuntimeError> {
    match record.get(name) {
        Some(Value::Duration(value)) => Ok(Some(Duration::from_millis(value.millis))),
        Some(value) => Err(RuntimeError::new(
            "type-error",
            format!("{name} expected Duration, found {}", value.type_name()),
        )
        .with_span(span)),
        None => Ok(None),
    }
}

pub(super) fn record_nonnegative_usize(
    record: &RecordMap,
    name: &str,
    default: usize,
    span: Span,
) -> Result<usize, RuntimeError> {
    match record.get(name) {
        Some(Value::Int(value)) if *value >= 0 => Ok(*value as usize),
        Some(Value::Int(_)) => Err(RuntimeError::new(
            "range-error",
            format!("{name} cannot be negative"),
        )
        .with_span(span)),
        Some(value) => Err(RuntimeError::new(
            "type-error",
            format!("{name} expected Int, found {}", value.type_name()),
        )
        .with_span(span)),
        None => Ok(default),
    }
}

pub(super) fn record_positive_u64(
    record: &RecordMap,
    name: &str,
    default: u64,
    span: Span,
) -> Result<u64, RuntimeError> {
    match record_optional_positive_u64(record, name, span)? {
        Some(value) => Ok(value),
        None => Ok(default),
    }
}

pub(super) fn record_optional_positive_u64(
    record: &RecordMap,
    name: &str,
    span: Span,
) -> Result<Option<u64>, RuntimeError> {
    match record.get(name) {
        Some(Value::Int(value)) if *value >= 0 => Ok(Some(*value as u64)),
        Some(Value::Int(_)) => Err(RuntimeError::new(
            "range-error",
            format!("{name} cannot be negative"),
        )
        .with_span(span)),
        Some(value) => Err(RuntimeError::new(
            "type-error",
            format!("{name} expected Int, found {}", value.type_name()),
        )
        .with_span(span)),
        None => Ok(None),
    }
}

pub(super) fn run_error_to_runtime(error: RunError, span: Span) -> RuntimeError {
    RuntimeError::new(error.kind, error.message).with_span(span)
}

pub(super) fn record_int_field(
    record: &RecordMap,
    field: &str,
    kind: &'static str,
    span: Span,
) -> Result<i64, RuntimeError> {
    match record.get(field) {
        Some(Value::Int(value)) => Ok(*value),
        Some(value) => Err(RuntimeError::new(
            "type-error",
            format!("expected `{field}` to be Int, found {}", value.type_name()),
        )
        .with_span(span)),
        None => Err(RuntimeError::new(kind, format!("missing `{field}` field")).with_span(span)),
    }
}

pub(super) fn display_spawn_argv(target: &[u8], argv: &[Vec<u8>]) -> String {
    std::iter::once(target)
        .chain(argv.iter().map(Vec::as_slice))
        .map(|item| String::from_utf8_lossy(item).into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

// Collision-free cache key: every variable-length component is preceded by its byte count,
// so the encoding is unambiguous regardless of what characters appear in content.
//
// Format: {fn_name_len}:{fn_name}{arg_count}:{arg1_len}:{arg1_encoded}...
pub(super) fn utils_cache_key(fn_name: &str, args: &[Value]) -> Result<String, &'static str> {
    let mut key = format!("{}:{}{}", fn_name.len(), fn_name, args.len());
    for arg in args {
        let encoded = encode_cache_key_value(arg)?;
        key.push_str(&format!(":{}:{}", encoded.len(), encoded));
    }
    Ok(key)
}

pub(super) fn encode_cache_key_value(value: &Value) -> Result<String, &'static str> {
    Ok(match value {
        Value::Null => "N".to_string(),
        Value::Bool(b) => format!("B{}", *b as u8),
        Value::Int(n) => format!("I{n}"),
        Value::Float(n) => format!("F{:016x}", n.0.to_bits()),
        Value::Duration(d) => format!("D{}", d.millis),
        Value::Str(s) => format!("S{}:{}", s.len(), s),
        Value::Bytes(b) => {
            let hex: String = b.iter().map(|byte| format!("{byte:02x}")).collect();
            format!("Y{}:{}", b.len(), hex)
        }
        Value::Path(p) => format!("P{}:{}", p.bytes.len(), String::from_utf8_lossy(&p.bytes)),
        Value::Digest(d) => {
            let hex: String = d.bytes.iter().map(|byte| format!("{byte:02x}")).collect();
            format!(
                "G{}:{}{}:{}",
                d.algorithm.len(),
                d.algorithm,
                d.bytes.len(),
                hex
            )
        }
        Value::Regex(r) => format!("X{}:{}", r.pattern.len(), r.pattern),
        Value::List(items) => {
            let mut out = format!("L{}", items.len());
            for item in items {
                let enc = encode_cache_key_value(item)?;
                out.push_str(&format!(":{}:{}", enc.len(), enc));
            }
            out
        }
        Value::Map(map) => {
            let mut out = format!("M{}", map.len());
            for (k, v) in map {
                let k_enc = encode_cache_key_value(&k.clone().into_value())?;
                let v_enc = encode_cache_key_value(v)?;
                out.push_str(&format!(":{}:{}:{}:{}", k_enc.len(), k_enc, v_enc.len(), v_enc));
            }
            out
        }
        Value::Record(fields) | Value::Module(fields) => {
            let mut out = format!("R{}", fields.len());
            for (k, v) in fields {
                let v_enc = encode_cache_key_value(v)?;
                out.push_str(&format!(":{}:{}:{}:{}", k.len(), k, v_enc.len(), v_enc));
            }
            out
        }
        Value::FsEntry(entry) => {
            let fields = entry.to_record_map().map_err(|_| "Record")?;
            let mut out = format!("R{}", fields.len());
            for (k, v) in fields {
                let v_enc = encode_cache_key_value(&v)?;
                out.push_str(&format!(":{}:{}:{}:{}", k.len(), k, v_enc.len(), v_enc));
            }
            out
        }
        Value::Tag { name, fields, .. } => {
            let mut out = format!("T{}:{}{}", name.len(), name, fields.len());
            for field in fields {
                let enc = encode_cache_key_value(field)?;
                out.push_str(&format!(":{}:{}", enc.len(), enc));
            }
            out
        }
        Value::Stream(_) => return Err("Stream"),
        Value::Result(_) => return Err("Result"),
        Value::Status(_) => return Err("Status"),
        Value::EnvPathList => return Err("EnvPathList"),
        Value::Error(_) => return Err("Error"),
        Value::RunError(_) => return Err("ProcessError"),
        Value::Pure(_) => return Err("Pure"),
        Value::Proc(_) => return Err("Proc"),
        Value::Command(_) => return Err("Command"),
        Value::ProcessHandle(_) => return Err("ProcessHandle"),
        Value::NetJob(_) => return Err("NetJob"),
        Value::FsRoot(_) => return Err("FsRoot"),
        Value::Unit => return Err("Unit"),
    })
}
