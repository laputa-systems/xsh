#![allow(clippy::single_call_fn)]

#[cfg(feature = "ssh")]
use std::collections::BTreeMap;
#[cfg(feature = "ssh")]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(feature = "ssh")]
use std::path::PathBuf;
use std::sync::Arc;
#[cfg(feature = "ssh")]
use std::sync::{Mutex, OnceLock};

use crate::runtime::value::{RecordMap, RuntimeError, SshSessionValue, Value};
use crate::source::Span;

#[cfg(feature = "ssh")]
use xsh_ssh::{
    Certainty, ClientConfig, Connection, Error as SshError, ExecOptions, Fingerprint, HostTrust,
    Identity, KnownHosts,
};

#[cfg(feature = "ssh")]
struct RegistrySession {
    owner: Arc<()>,
    scope: u64,
    connection: Arc<Mutex<Connection>>,
}

#[cfg(feature = "ssh")]
fn sessions() -> &'static Mutex<BTreeMap<u64, RegistrySession>> {
    static SESSIONS: OnceLock<Mutex<BTreeMap<u64, RegistrySession>>> = OnceLock::new();
    SESSIONS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

#[cfg(feature = "ssh")]
fn record_field<'a>(record: &'a RecordMap, name: &str, span: Span) -> Result<&'a Value, RuntimeError> {
    record.get(name).ok_or_else(|| {
        failure("Options", format!("missing {name} option")).with_span(span)
    })
}

#[cfg(feature = "ssh")]
fn text_field(record: &RecordMap, name: &str, span: Span) -> Result<String, RuntimeError> {
    match record_field(record, name, span)? {
        Value::Str(value) => Ok(value.to_string()),
        _ => Err(failure("Options", format!("{name} must be Str")).with_span(span)),
    }
}

#[cfg(feature = "ssh")]
fn optional_int(record: &RecordMap, name: &str, span: Span) -> Result<Option<i64>, RuntimeError> {
    match record.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Int(value)) => Ok(Some(*value)),
        Some(_) => Err(failure("Options", format!("{name} must be Int")).with_span(span)),
    }
}

#[cfg(feature = "ssh")]
fn int_field(record: &RecordMap, name: &str, span: Span) -> Result<i64, RuntimeError> {
    optional_int(record, name, span)?.ok_or_else(|| {
        failure("Options", format!("missing {name} option")).with_span(span)
    })
}

#[cfg(feature = "ssh")]
fn path_field(record: &RecordMap, name: &str, span: Span) -> Result<PathBuf, RuntimeError> {
    match record_field(record, name, span)? {
        Value::Path(path) => String::from_utf8(path.bytes.clone())
            .map(PathBuf::from)
            .map_err(|_| failure("Options", format!("{name} is not UTF-8")).with_span(span)),
        _ => Err(failure("Options", format!("{name} must be Path")).with_span(span)),
    }
}

#[cfg(feature = "ssh")]
fn optional_path(record: &RecordMap, name: &str, span: Span) -> Result<Option<PathBuf>, RuntimeError> {
    match record.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Path(_)) => path_field(record, name, span).map(Some),
        Some(_) => Err(failure("Options", format!("{name} must be Path")).with_span(span)),
    }
}

#[cfg(feature = "ssh")]
fn optional_bytes(record: &RecordMap, name: &str, span: Span) -> Result<Option<Vec<u8>>, RuntimeError> {
    match record.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bytes(value)) => Ok(Some(value.clone())),
        Some(_) => Err(failure("Options", format!("{name} must be Bytes")).with_span(span)),
    }
}

#[cfg(feature = "ssh")]
fn interrupt() -> xsh_ssh::Interrupt {
    Arc::new(|| crate::runtime::process::signal_snapshot().primary.is_some())
}

fn structured(
    variant: &'static str,
    message: String,
    certainty: &'static str,
) -> RuntimeError {
    let facets = xsh_registry::errors::SSH_ERROR_VARIANTS
        .iter()
        .find(|candidate| candidate.name == variant)
        .unwrap_or_else(|| panic!("`{variant}` is not an SshError variant"))
        .facets
        .iter()
        .map(|facet| facet.name().to_string())
        .collect();
    let payload = RecordMap::from([
        (Arc::from("message"), Value::Str(message.as_str().into())),
        (Arc::from("certainty"), Value::Str(certainty.into())),
    ]);
    RuntimeError::structured("SshError", variant, payload, facets, message)
}

/// A failure that never reached the server.
fn failure(variant: &'static str, message: impl Into<String>) -> RuntimeError {
    structured(variant, message.into(), "not-submitted")
}

#[cfg(feature = "ssh")]
fn ssh_error(error: SshError, span: Span) -> RuntimeError {
    let variant = match &error {
        SshError::InvalidFingerprint { .. }
        | SshError::UnsupportedTrust { .. }
        | SshError::InvalidTrust { .. } => "TrustMaterial",
        SshError::HostKeyMismatch { .. } => "HostKeyMismatch",
        SshError::UnknownHostKey { .. } => "HostKeyUnknown",
        SshError::RevokedHostKey { .. } => "HostKeyRevoked",
        SshError::HostKey { .. } => "HostKeyInvalid",
        SshError::UnsupportedAlgorithm { .. } | SshError::StrictKexUnsupported => "Algorithm",
        SshError::UnsupportedHardware { .. } => "Hardware",
        SshError::KeyFile { .. } | SshError::EncryptedKey | SshError::Agent { .. } => "Key",
        SshError::Authentication { .. } => "Authentication",
        SshError::Rejected { .. } => "Rejected",
        SshError::InvalidEnvironment { .. } => "Options",
        SshError::Channel { .. } => "Channel",
        SshError::Protocol { .. } => "Protocol",
        SshError::NotSubmitted { .. } | SshError::Submitted { .. } => "Transport",
        SshError::Timeout { .. } => "Timeout",
        SshError::Cancelled { .. } => "Canceled",
        SshError::OutputLimit { .. } => "OutputLimit",
        SshError::Unusable => "Unusable",
    };
    let certainty = match error.certainty() {
        Certainty::NotSubmitted => "not-submitted",
        Certainty::Submitted => "submitted",
    };
    structured(variant, error.to_string(), certainty).with_span(span)
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

#[cfg(feature = "ssh")]
fn environment(options: &RecordMap, span: Span) -> Result<Vec<(String, String)>, RuntimeError> {
    match options.get("env") {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Record(record)) => record
            .iter()
            .map(|(name, value)| match value {
                Value::Str(text) => Ok((name.to_string(), text.to_string())),
                _ => Err(failure("Options", format!("env.{name} must be Str")).with_span(span)),
            })
            .collect(),
        Some(_) => Err(failure("Options", "env must be a Record of Str").with_span(span)),
    }
}

#[cfg(feature = "ssh")]
fn config_from_options(options: &RecordMap, span: Span) -> Result<ClientConfig, RuntimeError> {
    let host = text_field(options, "host", span)?;
    let port = int_field(options, "port", span)?;
    if !(1..=65535).contains(&port) {
        return Err(failure("Options", "port must be 1..=65535").with_span(span));
    }
    let port = port as u16;
    let user = text_field(options, "user", span)?;
    let timeout_ms = optional_int(options, "timeout_ms", span)?.unwrap_or(10_000);
    if timeout_ms <= 0 {
        return Err(failure("Options", "timeout_ms must be positive").with_span(span));
    }
    let identity = match (
        optional_path(options, "key", span)?,
        optional_path(options, "agent", span)?,
        options.get("agent_identity"),
    ) {
        (Some(key), None, None | Some(Value::Null)) => Identity::KeyFile(key),
        (None, Some(socket), Some(Value::Str(fingerprint))) => Identity::Agent {
            socket,
            fingerprint: Fingerprint::parse(fingerprint).map_err(|error| ssh_error(error, span))?,
        },
        (None, Some(_), _) => {
            return Err(failure("Options", "agent requires agent_identity, a SHA256 fingerprint")
                .with_span(span));
        }
        _ => {
            return Err(failure(
                "Options",
                "provide exactly one of key, or agent with agent_identity",
            )
            .with_span(span));
        }
    };
    let trust = match (
        options.get("host_key").filter(|value| !matches!(value, Value::Null)),
        optional_path(options, "known_hosts", span)?,
    ) {
        (Some(Value::Str(pin)), None) => {
            HostTrust::Pinned(Fingerprint::parse(pin).map_err(|error| ssh_error(error, span))?)
        }
        (Some(_), None) => {
            return Err(failure("Options", "host_key must be Str").with_span(span));
        }
        (None, Some(path)) => HostTrust::KnownHosts(
            KnownHosts::from_file(&path).map_err(|error| ssh_error(error, span))?,
        ),
        _ => {
            return Err(failure("Options", "provide exactly one of host_key or known_hosts")
                .with_span(span));
        }
    };
    let mut config = ClientConfig::new(
        host,
        port,
        user,
        identity,
        trust,
        std::time::Duration::from_millis(timeout_ms as u64),
    );
    config.interrupt = Some(interrupt());
    Ok(config)
}

#[cfg(feature = "ssh")]
fn open(config: &ClientConfig) -> Result<Connection, SshError> {
    let mut connection = Connection::connect(config)?;
    connection.authenticate()?;
    Ok(connection)
}

#[cfg(feature = "ssh")]
fn register(
    connection: Connection,
    owner: Arc<()>,
    scope: u64,
    span: Span,
) -> Result<Value, RuntimeError> {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    sessions()
        .lock()
        .map_err(|_| failure("Session", "session registry is poisoned").with_span(span))?
        .insert(
            id,
            RegistrySession {
                owner,
                scope,
                connection: Arc::new(Mutex::new(connection)),
            },
        );
    Ok(Value::SshSession(SshSessionValue { id }))
}

/// Connect, authenticate, and register a session capability.
#[cfg(feature = "ssh")]
pub(crate) fn connect(
    options: &RecordMap,
    owner: Arc<()>,
    scope: u64,
    span: Span,
) -> Result<Value, RuntimeError> {
    let config = config_from_options(options, span)?;
    let connection = open(&config).map_err(|error| ssh_error(error, span))?;
    register(connection, owner, scope, span)
}

/// The most hosts `connect_all` opens at once.
#[cfg(feature = "ssh")]
const CONNECT_ALL_JOBS: usize = 16;

/// Connect to several hosts in parallel and register each success in the calling scope.
///
/// The result lists one `Ok` session or `Err` failure per target, in order, so a
/// failure on one host never hides the others.
#[cfg(feature = "ssh")]
pub(crate) fn connect_all(
    batch: &RecordMap,
    owner: &Arc<()>,
    scope: u64,
    span: Span,
) -> Result<Vec<Value>, RuntimeError> {
    let Some(Value::List(targets)) = batch.get("targets") else {
        return Err(failure("Options", "targets must be List[Record]").with_span(span));
    };
    let jobs = match optional_int(batch, "jobs", span)? {
        None => 4,
        Some(jobs) if (1..=CONNECT_ALL_JOBS as i64).contains(&jobs) => jobs as usize,
        Some(_) => {
            return Err(
                failure("Options", format!("jobs must be 1..={CONNECT_ALL_JOBS}")).with_span(span)
            );
        }
    };
    let configs: Vec<Result<ClientConfig, RuntimeError>> = targets
        .iter()
        .map(|target| match target {
            Value::Record(record) => config_from_options(record, span),
            _ => Err(failure("Options", "targets must be List[Record]").with_span(span)),
        })
        .collect();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let opened: Vec<Mutex<Option<Result<Connection, SshError>>>> =
        configs.iter().map(|_| Mutex::new(None)).collect();
    std::thread::scope(|workers| {
        for _ in 0..jobs.min(configs.len()) {
            workers.spawn(|| {
                loop {
                    let index = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    let Some(Ok(config)) = configs.get(index) else {
                        if index >= configs.len() {
                            return;
                        }
                        continue;
                    };
                    let outcome = open(config);
                    if let Ok(mut slot) = opened[index].lock() {
                        *slot = Some(outcome);
                    }
                }
            });
        }
    });
    let mut results = Vec::with_capacity(configs.len());
    for (config, slot) in configs.into_iter().zip(opened) {
        let outcome = match config {
            Err(error) => Err(error),
            Ok(_) => match slot.into_inner().ok().flatten() {
                Some(Ok(connection)) => register(connection, Arc::clone(owner), scope, span),
                Some(Err(error)) => Err(ssh_error(error, span)),
                None => Err(failure("Session", "connection worker did not finish").with_span(span)),
            },
        };
        results.push(match outcome {
            Ok(value) => Value::ok(value),
            Err(error) => Value::err(Value::Error(Box::new(error))),
        });
    }
    Ok(results)
}

#[cfg(not(feature = "ssh"))]
pub(crate) fn connect(
    _options: &RecordMap,
    _owner: Arc<()>,
    _scope: u64,
    span: Span,
) -> Result<Value, RuntimeError> {
    Err(failure("Disabled", "ssh feature is disabled").with_span(span))
}

#[cfg(not(feature = "ssh"))]
pub(crate) fn connect_all(
    _batch: &RecordMap,
    _owner: &Arc<()>,
    _scope: u64,
    span: Span,
) -> Result<Vec<Value>, RuntimeError> {
    Err(failure("Disabled", "ssh feature is disabled").with_span(span))
}

#[cfg(feature = "ssh")]
fn session(handle: SshSessionValue, span: Span) -> Result<Arc<Mutex<Connection>>, RuntimeError> {
    let registry = sessions()
        .lock()
        .map_err(|_| failure("Session", "session registry is poisoned").with_span(span))?;
    let Some(session) = registry.get(&handle.id) else {
        return Err(failure("Session", "session is closed or unknown").with_span(span));
    };
    Ok(Arc::clone(&session.connection))
}

/// Execute one command through a registered session.
#[cfg(feature = "ssh")]
pub(crate) fn exec(
    handle: SshSessionValue,
    command: &str,
    options: &RecordMap,
    owner: &Arc<()>,
    span: Span,
) -> Result<Value, RuntimeError> {
    let _ = owner;
    if command.contains('\0') {
        return Err(failure("Options", "command must not contain NUL").with_span(span));
    }
    let connection = session(handle, span)?;
    let timeout = match optional_int(options, "timeout_ms", span)? {
        Some(millis) if millis > 0 => Some(std::time::Duration::from_millis(millis as u64)),
        Some(_) => {
            return Err(failure("Options", "timeout_ms must be positive").with_span(span));
        }
        None => None,
    };
    let max_output = match optional_int(options, "max_output", span)? {
        Some(value) if value > 0 => Some(usize::try_from(value).unwrap_or(usize::MAX)),
        Some(_) => {
            return Err(failure("Options", "max_output must be positive").with_span(span));
        }
        None => None,
    };
    let stdin = optional_bytes(options, "stdin", span)?;
    let stdin_path = optional_path(options, "stdin_path", span)?;
    let env = environment(options, span)?;
    let stdout_path = optional_path(options, "stdout_path", span)?;
    let stderr_path = optional_path(options, "stderr_path", span)?;
    let mut connection = connection.try_lock().map_err(|_| {
        failure("Busy", "session is already in use").with_span(span)
    })?;

    let create = |path: &PathBuf| {
        std::fs::File::create(path).map_err(|error| {
            failure("Local", format!("cannot create {}: {error}", path.display())).with_span(span)
        })
    };
    let stdin_file = match &stdin_path {
        Some(path) => Some(std::fs::File::open(path).map_err(|error| {
            failure("Local", format!("cannot open {}: {error}", path.display())).with_span(span)
        })?),
        None => None,
    };
    let stdout_file = stdout_path.as_ref().map(create).transpose()?;
    let stderr_file = stderr_path.as_ref().map(create).transpose()?;
    let result = connection
        .exec(
            command,
            &ExecOptions {
                stdin,
                stdin_file,
                stdout_file,
                stderr_file,
                timeout,
                max_output: max_output.unwrap_or(xsh_ssh::exec::DEFAULT_MAX_OUTPUT),
                interrupt: Some(interrupt()),
                env,
            },
        )
        .map_err(|error| ssh_error(error, span))?;
    Ok(result_value(result))
}

#[cfg(not(feature = "ssh"))]
pub(crate) fn exec(
    _session: SshSessionValue,
    _command: &str,
    _options: &RecordMap,
    _owner: &Arc<()>,
    span: Span,
) -> Result<Value, RuntimeError> {
    Err(failure("Disabled", "ssh feature is disabled").with_span(span))
}

/// Describe the authenticated host identity and negotiated algorithms of a session.
#[cfg(feature = "ssh")]
pub(crate) fn info(handle: SshSessionValue, span: Span) -> Result<Value, RuntimeError> {
    let registry = sessions()
        .lock()
        .map_err(|_| failure("Session", "session registry is poisoned").with_span(span))?;
    let session = registry
        .get(&handle.id)
        .ok_or_else(|| failure("Session", "session is closed or unknown").with_span(span))?;
    let connection = session
        .connection
        .try_lock()
        .map_err(|_| failure("Busy", "session is already in use").with_span(span))?;
    let algorithms = connection.algorithms();
    let text = |value: &str| Value::Str(value.into());
    Ok(Value::Record(RecordMap::from([
        (Arc::from("host"), text(connection.host())),
        (Arc::from("port"), Value::Int(i64::from(connection.port()))),
        (Arc::from("user"), text(connection.user())),
        (Arc::from("usable"), Value::Bool(connection.is_usable())),
        (Arc::from("host_key"), text(connection.fingerprint())),
        (Arc::from("kex"), text(&algorithms.kex)),
        (Arc::from("host_key_algorithm"), text(&algorithms.host_key)),
        (
            Arc::from("cipher_client_to_server"),
            text(&algorithms.cipher_client_to_server),
        ),
        (
            Arc::from("cipher_server_to_client"),
            text(&algorithms.cipher_server_to_client),
        ),
    ])))
}

#[cfg(not(feature = "ssh"))]
pub(crate) fn info(_session: SshSessionValue, span: Span) -> Result<Value, RuntimeError> {
    Err(failure("Disabled", "ssh feature is disabled").with_span(span))
}

/// Close a session and return its owning scope, if it was live.
#[cfg(feature = "ssh")]
pub(crate) fn close(
    handle: SshSessionValue,
    owner: &Arc<()>,
    if_open: bool,
    span: Span,
) -> Result<Option<u64>, RuntimeError> {
    let mut registry = sessions()
        .lock()
        .map_err(|_| failure("Session", "session registry is poisoned").with_span(span))?;
    let Some(session) = registry.get(&handle.id) else {
        return if if_open {
            Ok(None)
        } else {
            Err(failure("Session", "session is already closed").with_span(span))
        };
    };
    let _ = owner;
    if session.connection.try_lock().is_err() {
        return Err(
            failure("Busy", "session is already in use").with_span(span),
        );
    }
    let session = registry.remove(&handle.id).expect("live session");
    let scope = session.scope;
    Ok(Some(scope))
}

#[cfg(not(feature = "ssh"))]
pub(crate) fn close(
    _session: SshSessionValue,
    _owner: &Arc<()>,
    _if_open: bool,
    span: Span,
) -> Result<Option<u64>, RuntimeError> {
    Err(failure("Disabled", "ssh feature is disabled").with_span(span))
}

/// Remove every session owned by `owner` in `scope`.
#[cfg(feature = "ssh")]
pub(crate) fn take_scope(owner: &Arc<()>, scope: u64) -> Vec<u64> {
    let Ok(mut registry) = sessions().lock() else {
        return Vec::new();
    };
    let ids: Vec<u64> = registry
        .iter()
        .filter(|(_, session)| session.scope == scope && Arc::ptr_eq(&session.owner, owner))
        .map(|(id, _)| *id)
        .collect();
    for id in &ids {
        registry.remove(id);
    }
    ids
}

#[cfg(not(feature = "ssh"))]
pub(crate) fn take_scope(_owner: &Arc<()>, _scope: u64) -> Vec<u64> {
    Vec::new()
}

/// The scope that currently owns a session.
#[cfg(feature = "ssh")]
pub(crate) fn owner_scope(id: u64, owner: &Arc<()>) -> Option<u64> {
    let registry = sessions().lock().ok()?;
    let session = registry.get(&id)?;
    if !Arc::ptr_eq(&session.owner, owner) {
        return None;
    }
    Some(session.scope)
}

#[cfg(not(feature = "ssh"))]
pub(crate) fn owner_scope(_id: u64, _owner: &Arc<()>) -> Option<u64> {
    None
}

/// Move a session to a descendant scope and return its previous scope.
#[cfg(feature = "ssh")]
pub(crate) fn transfer(id: u64, owner: &Arc<()>, target: u64) -> Option<u64> {
    let mut registry = sessions().lock().ok()?;
    let session = registry.get_mut(&id)?;
    if !Arc::ptr_eq(&session.owner, owner) {
        return None;
    }
    let previous = session.scope;
    session.scope = target;
    Some(previous)
}

#[cfg(not(feature = "ssh"))]
pub(crate) fn transfer(_id: u64, _owner: &Arc<()>, _target: u64) -> Option<u64> {
    None
}
