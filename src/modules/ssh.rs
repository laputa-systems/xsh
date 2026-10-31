#![allow(clippy::single_call_fn)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use crate::runtime::value::{RecordMap, RuntimeError, Value};
use crate::source::Span;

#[cfg(feature = "ssh")]
use xsh_ssh::{Certainty, ClientConfig, Connection, Error as SshError, ExecOptions, StreamOptions};

/// The opaque token field of a session record.
pub(crate) const SESSION_TOKEN: &str = "token";

#[cfg(feature = "ssh")]
struct RegistrySession {
    owner: Arc<()>,
    scope: u64,
    connection: Arc<Mutex<Connection>>,
}

#[cfg(feature = "ssh")]
fn sessions() -> &'static Mutex<BTreeMap<String, RegistrySession>> {
    static SESSIONS: OnceLock<Mutex<BTreeMap<String, RegistrySession>>> = OnceLock::new();
    SESSIONS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn record_field<'a>(record: &'a RecordMap, name: &str, span: Span) -> Result<&'a Value, RuntimeError> {
    record.get(name).ok_or_else(|| {
        RuntimeError::new("ssh-options", format!("missing {name} option")).with_span(span)
    })
}

fn text_field(record: &RecordMap, name: &str, span: Span) -> Result<String, RuntimeError> {
    match record_field(record, name, span)? {
        Value::Str(value) => Ok(value.to_string()),
        _ => Err(RuntimeError::new("ssh-options", format!("{name} must be Str")).with_span(span)),
    }
}

fn optional_int(record: &RecordMap, name: &str, span: Span) -> Result<Option<i64>, RuntimeError> {
    match record.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Int(value)) => Ok(Some(*value)),
        Some(_) => Err(RuntimeError::new("ssh-options", format!("{name} must be Int")).with_span(span)),
    }
}

fn int_field(record: &RecordMap, name: &str, span: Span) -> Result<i64, RuntimeError> {
    optional_int(record, name, span)?.ok_or_else(|| {
        RuntimeError::new("ssh-options", format!("missing {name} option")).with_span(span)
    })
}

fn path_field(record: &RecordMap, name: &str, span: Span) -> Result<PathBuf, RuntimeError> {
    match record_field(record, name, span)? {
        Value::Path(path) => String::from_utf8(path.bytes.clone())
            .map(PathBuf::from)
            .map_err(|_| RuntimeError::new("ssh-options", format!("{name} is not UTF-8")).with_span(span)),
        _ => Err(RuntimeError::new("ssh-options", format!("{name} must be Path")).with_span(span)),
    }
}

fn optional_path(record: &RecordMap, name: &str, span: Span) -> Result<Option<PathBuf>, RuntimeError> {
    match record.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Path(_)) => path_field(record, name, span).map(Some),
        Some(_) => Err(RuntimeError::new("ssh-options", format!("{name} must be Path")).with_span(span)),
    }
}

fn optional_bytes(record: &RecordMap, name: &str, span: Span) -> Result<Option<Vec<u8>>, RuntimeError> {
    match record.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bytes(value)) => Ok(Some(value.clone())),
        Some(_) => Err(RuntimeError::new("ssh-options", format!("{name} must be Bytes")).with_span(span)),
    }
}

/// Extract a session token from a record, rejecting any other shape.
pub(crate) fn token_from_record(record: &RecordMap) -> Option<String> {
    if record.len() != 1 {
        return None;
    }
    match record.get(SESSION_TOKEN) {
        Some(Value::Str(value)) => Some(value.to_string()),
        _ => None,
    }
}

#[cfg(feature = "ssh")]
fn token() -> Result<String, RuntimeError> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|error| RuntimeError::new("ssh-token", error.to_string()))?;
    Ok(data_encoding::HEXLOWER.encode(&bytes))
}

#[cfg(feature = "ssh")]
fn ssh_error(error: SshError, span: Span) -> RuntimeError {
    let kind = match &error {
        SshError::InvalidHostKeyPin { .. } => "ssh-host-key-pin",
        SshError::HostKeyMismatch { .. } => "ssh-host-key",
        SshError::HostKey { .. } => "ssh-host-key",
        SshError::UnsupportedAlgorithm { .. } => "ssh-algorithm",
        SshError::StrictKexUnsupported => "ssh-strict-kex",
        SshError::KeyFile { .. } => "ssh-key",
        SshError::EncryptedKey => "ssh-key-encrypted",
        SshError::Authentication { .. } => "ssh-auth",
        SshError::Channel { .. } => "ssh-channel",
        SshError::NotSubmitted { .. } => "ssh-not-submitted",
        SshError::Submitted { .. } => "ssh-submitted",
        SshError::Protocol { .. } => "ssh-protocol",
        SshError::Timeout {
            certainty: Certainty::NotSubmitted,
        } => "ssh-timeout-not-submitted",
        SshError::Timeout {
            certainty: Certainty::Submitted,
        } => "ssh-timeout-submitted",
    };
    RuntimeError::new(kind, error.to_string()).with_span(span)
}

#[cfg(feature = "ssh")]
fn result_value(result: xsh_ssh::ExecResult) -> Value {
    let (exit_code, signal, missing) = match &result.status {
        xsh_ssh::ExecStatus::Exited(code) => (i64::from(*code), String::new(), false),
        xsh_ssh::ExecStatus::Signaled(signal) => (-1, signal.clone(), false),
        xsh_ssh::ExecStatus::Missing => (-1, String::new(), true),
    };
    let certainty = match result.certainty {
        Certainty::NotSubmitted => "not-submitted",
        Certainty::Submitted => "submitted",
    };
    Value::Record(RecordMap::from([
        (Arc::from("exit_code"), Value::Int(exit_code)),
        (Arc::from("signal"), Value::Str(signal.into())),
        (Arc::from("missing"), Value::Bool(missing)),
        (Arc::from("stdout"), Value::Bytes(result.stdout)),
        (Arc::from("stderr"), Value::Bytes(result.stderr)),
        (Arc::from("certainty"), Value::Str(certainty.into())),
    ]))
}

/// Connect, authenticate, and register a session capability.
#[cfg(feature = "ssh")]
pub(crate) fn connect(
    options: &RecordMap,
    owner: Arc<()>,
    scope: u64,
    span: Span,
) -> Result<Value, RuntimeError> {
    let host = text_field(options, "host", span)?;
    let port = int_field(options, "port", span)?;
    if !(1..=65535).contains(&port) {
        return Err(RuntimeError::new("ssh-options", "port must be 1..=65535").with_span(span));
    }
    let port = port as u16;
    let user = text_field(options, "user", span)?;
    let key = path_field(options, "key", span)?;
    let host_key = text_field(options, "host_key", span)?;
    let timeout_ms = optional_int(options, "timeout_ms", span)?.unwrap_or(10_000);
    if timeout_ms <= 0 {
        return Err(RuntimeError::new("ssh-options", "timeout_ms must be positive").with_span(span));
    }
    let config = ClientConfig::new(
        host,
        port,
        user,
        key,
        &host_key,
        std::time::Duration::from_millis(timeout_ms as u64),
    )
    .map_err(|error| ssh_error(error, span))?;
    let mut connection = Connection::connect(&config).map_err(|error| ssh_error(error, span))?;
    connection.authenticate().map_err(|error| ssh_error(error, span))?;
    let token = token()?;
    sessions()
        .lock()
        .map_err(|_| RuntimeError::new("ssh-session", "session registry is poisoned").with_span(span))?
        .insert(
            token.clone(),
            RegistrySession {
                owner,
                scope,
                connection: Arc::new(Mutex::new(connection)),
            },
        );
    Ok(Value::Record(RecordMap::from([(
        Arc::from(SESSION_TOKEN),
        Value::Str(token.into()),
    )])))
}

#[cfg(not(feature = "ssh"))]
pub(crate) fn connect(
    _options: &RecordMap,
    _owner: Arc<()>,
    _scope: u64,
    span: Span,
) -> Result<Value, RuntimeError> {
    Err(RuntimeError::new("ssh-disabled", "ssh feature is disabled").with_span(span))
}

#[cfg(feature = "ssh")]
fn session(token: &str, owner: &Arc<()>, span: Span) -> Result<Arc<Mutex<Connection>>, RuntimeError> {
    let registry = sessions()
        .lock()
        .map_err(|_| RuntimeError::new("ssh-session", "session registry is poisoned").with_span(span))?;
    let Some(session) = registry.get(token) else {
        return Err(RuntimeError::new("ssh-session", "session is closed or unknown").with_span(span));
    };
    let _ = owner;
    Ok(Arc::clone(&session.connection))
}

/// Execute one command through a registered session.
#[cfg(feature = "ssh")]
pub(crate) fn exec(
    session_record: &RecordMap,
    command: &str,
    options: &RecordMap,
    owner: &Arc<()>,
    span: Span,
) -> Result<Value, RuntimeError> {
    let token = token_from_record(session_record)
        .ok_or_else(|| RuntimeError::new("ssh-session", "invalid session handle").with_span(span))?;
    let connection = session(&token, owner, span)?;
    let timeout = match optional_int(options, "timeout_ms", span)? {
        Some(millis) if millis > 0 => Some(std::time::Duration::from_millis(millis as u64)),
        Some(_) => {
            return Err(RuntimeError::new("ssh-options", "timeout_ms must be positive").with_span(span));
        }
        None => None,
    };
    let max_output = match optional_int(options, "max_output", span)? {
        Some(value) if value > 0 => Some(usize::try_from(value).unwrap_or(usize::MAX)),
        Some(_) => {
            return Err(RuntimeError::new("ssh-options", "max_output must be positive").with_span(span));
        }
        None => None,
    };
    let stdin = optional_bytes(options, "stdin", span)?;
    let stdin_path = optional_path(options, "stdin_path", span)?;
    if stdin.is_some() && stdin_path.is_some() {
        return Err(RuntimeError::new("ssh-options", "stdin and stdin_path conflict").with_span(span));
    }
    let stdout_path = optional_path(options, "stdout_path", span)?;
    let stderr_path = optional_path(options, "stderr_path", span)?;
    let mut connection = connection.try_lock().map_err(|_| {
        RuntimeError::new("ssh-session-busy", "session is already in use").with_span(span)
    })?;

    if stdout_path.is_some() || stderr_path.is_some() || stdin_path.is_some() {
        let mut input_file = match stdin_path {
            Some(path) => Some(std::fs::File::open(&path).map_err(|error| {
                RuntimeError::new("ssh-stdin", format!("cannot open {}: {error}", path.display()))
                    .with_span(span)
            })?),
            None => None,
        };
        let mut input_bytes = stdin.map(std::io::Cursor::new);
        let input: Option<&mut dyn std::io::Read> = match (&mut input_file, &mut input_bytes) {
            (Some(file), _) => Some(file),
            (None, Some(bytes)) => Some(bytes),
            (None, None) => None,
        };
        let mut stdout_buffer = Vec::new();
        let mut stderr_buffer = Vec::new();
        let mut stdout_file = match &stdout_path {
            Some(path) => Some(std::fs::File::create(path).map_err(|error| {
                RuntimeError::new("ssh-stdout", format!("cannot create {}: {error}", path.display()))
                    .with_span(span)
            })?),
            None => None,
        };
        let mut stderr_file = match &stderr_path {
            Some(path) => Some(std::fs::File::create(path).map_err(|error| {
                RuntimeError::new("ssh-stderr", format!("cannot create {}: {error}", path.display()))
                    .with_span(span)
            })?),
            None => None,
        };
        let stdout: &mut dyn std::io::Write = match &mut stdout_file {
            Some(file) => file,
            None => &mut stdout_buffer,
        };
        let stderr: &mut dyn std::io::Write = match &mut stderr_file {
            Some(file) => file,
            None => &mut stderr_buffer,
        };
        let result = connection
            .exec_stream(
                command,
                &StreamOptions { timeout },
                input,
                stdout,
                stderr,
            )
            .map_err(|error| ssh_error(error, span))?;
        let mut value = result_value(result);
        if let Value::Record(fields) = &mut value {
            fields.insert(Arc::from("stdout"), Value::Bytes(stdout_buffer));
            fields.insert(Arc::from("stderr"), Value::Bytes(stderr_buffer));
        }
        return Ok(value);
    }

    let result = connection
        .exec(
            command,
            &ExecOptions {
                stdin,
                timeout,
                max_output: max_output.unwrap_or(xsh_ssh::exec::DEFAULT_MAX_OUTPUT),
            },
        )
        .map_err(|error| ssh_error(error, span))?;
    Ok(result_value(result))
}

#[cfg(not(feature = "ssh"))]
pub(crate) fn exec(
    _session: &RecordMap,
    _command: &str,
    _options: &RecordMap,
    _owner: &Arc<()>,
    span: Span,
) -> Result<Value, RuntimeError> {
    Err(RuntimeError::new("ssh-disabled", "ssh feature is disabled").with_span(span))
}

/// Close a session and return its owning scope, if it was live.
#[cfg(feature = "ssh")]
pub(crate) fn close(
    session_record: &RecordMap,
    owner: &Arc<()>,
    if_open: bool,
    span: Span,
) -> Result<Option<u64>, RuntimeError> {
    let token = token_from_record(session_record)
        .ok_or_else(|| RuntimeError::new("ssh-session", "invalid session handle").with_span(span))?;
    let mut registry = sessions()
        .lock()
        .map_err(|_| RuntimeError::new("ssh-session", "session registry is poisoned").with_span(span))?;
    let Some(session) = registry.get(&token) else {
        return if if_open {
            Ok(None)
        } else {
            Err(RuntimeError::new("ssh-session", "session is already closed").with_span(span))
        };
    };
    let _ = owner;
    if session.connection.try_lock().is_err() {
        return Err(
            RuntimeError::new("ssh-session-busy", "session is already in use").with_span(span),
        );
    }
    let session = registry.remove(&token).expect("live session");
    let scope = session.scope;
    Ok(Some(scope))
}

#[cfg(not(feature = "ssh"))]
pub(crate) fn close(
    _session: &RecordMap,
    _owner: &Arc<()>,
    _if_open: bool,
    span: Span,
) -> Result<Option<u64>, RuntimeError> {
    Err(RuntimeError::new("ssh-disabled", "ssh feature is disabled").with_span(span))
}

/// Remove every session owned by `owner` in `scope`.
#[cfg(feature = "ssh")]
pub(crate) fn take_scope(owner: &Arc<()>, scope: u64) -> Vec<String> {
    let Ok(mut registry) = sessions().lock() else {
        return Vec::new();
    };
    let tokens: Vec<String> = registry
        .iter()
        .filter(|(_, session)| session.scope == scope && Arc::ptr_eq(&session.owner, owner))
        .map(|(token, _)| token.clone())
        .collect();
    for token in &tokens {
        registry.remove(token);
    }
    tokens
}

#[cfg(not(feature = "ssh"))]
pub(crate) fn take_scope(_owner: &Arc<()>, _scope: u64) -> Vec<String> {
    Vec::new()
}

/// The scope that currently owns a session.
#[cfg(feature = "ssh")]
pub(crate) fn owner_scope(token: &str, owner: &Arc<()>) -> Option<u64> {
    let registry = sessions().lock().ok()?;
    let session = registry.get(token)?;
    if !Arc::ptr_eq(&session.owner, owner) {
        return None;
    }
    Some(session.scope)
}

#[cfg(not(feature = "ssh"))]
pub(crate) fn owner_scope(_token: &str, _owner: &Arc<()>) -> Option<u64> {
    None
}

/// Move a session to a descendant scope and return its previous scope.
#[cfg(feature = "ssh")]
pub(crate) fn transfer(token: &str, owner: &Arc<()>, target: u64) -> Option<u64> {
    let mut registry = sessions().lock().ok()?;
    let session = registry.get_mut(token)?;
    if !Arc::ptr_eq(&session.owner, owner) {
        return None;
    }
    let previous = session.scope;
    session.scope = target;
    Some(previous)
}

#[cfg(not(feature = "ssh"))]
pub(crate) fn transfer(_token: &str, _owner: &Arc<()>, _target: u64) -> Option<u64> {
    None
}
