use super::linux_eval::append_fake_log;
use super::{Evaluator, module_error};
use crate::modules::process::signal_info;
use crate::runtime::process::record_signal;
use crate::runtime::value::{RuntimeError, Value};
use crate::source::Span;
use rustix::{io as rio, process as rprocess};
#[cfg(feature = "native-tests")]
use std::collections::BTreeMap;
use std::num::NonZeroI32;

/// The native-test double for the `unix` process-group, PID 1, tty, identity,
/// hostname, and exec entries. While installed those entries return fixed
/// values instead of touching the host and append one JSON line per call to
/// `log`. Only the test harness installs it (`test.unix_fake` and the scripts
/// that test runs); no environment variable or production flag reaches it.
#[cfg(feature = "native-tests")]
#[derive(Clone, Debug, Default)]
pub struct UnixFake {
    values: BTreeMap<&'static str, String>,
    replay: Option<std::sync::Arc<Replay>>,
}

/// An ordered, complete, redacted observation script for the fake. Each line
/// names one expected `unix` call, optionally followed by `key=value` results
/// the call returns (any fake setting except `log` and `replay`) and
/// `?key=value` checks against the call's logged fields. The k-th fake call
/// must be the k-th line, every result the call needs must be on its line, and
/// every line must be consumed. Values are limited to `[A-Za-z0-9_.,-]` and 32
/// bytes, so paths, hostnames, and free text cannot enter a fixture. While a
/// replay is installed no setting supplies a default and the `unix` entries
/// the fake does not cover fail instead of reaching the host.
#[cfg(feature = "native-tests")]
#[derive(Debug)]
struct Replay {
    entries: Vec<ReplayEntry>,
    next: std::sync::atomic::AtomicUsize,
}

#[cfg(feature = "native-tests")]
#[derive(Debug)]
struct ReplayEntry {
    line: usize,
    op: String,
    results: BTreeMap<&'static str, String>,
    checks: Vec<(String, String)>,
}

#[cfg(feature = "native-tests")]
const REPLAY_VALUE_LIMIT: usize = 32;

#[cfg(feature = "native-tests")]
fn replay_token_is_redacted(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= REPLAY_VALUE_LIMIT
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b',' | b'-'))
}

#[cfg(feature = "native-tests")]
impl Replay {
    fn parse(text: &str) -> Result<Self, String> {
        let mut entries = Vec::new();
        for (index, raw) in text.lines().enumerate() {
            let line = index + 1;
            let raw = raw.trim();
            if raw.is_empty() || raw.starts_with('#') {
                continue;
            }
            let mut tokens = raw.split_whitespace();
            let op = tokens.next().unwrap_or_default();
            if !replay_token_is_redacted(op) {
                return Err(format!("replay line {line}: `{op}` is not a redacted operation name"));
            }
            let mut results = BTreeMap::new();
            let mut checks: Vec<(String, String)> = Vec::new();
            for token in tokens {
                let (checked, pair) = match token.strip_prefix('?') {
                    Some(rest) => (true, rest),
                    None => (false, token),
                };
                let Some((key, value)) = pair.split_once('=') else {
                    return Err(format!("replay line {line}: `{token}` is not KEY=VALUE"));
                };
                if !replay_token_is_redacted(key) || !replay_token_is_redacted(value) {
                    return Err(format!(
                        "replay line {line}: `{key}` is not a redacted key and value; keys and values use [A-Za-z0-9_.,-] and at most {REPLAY_VALUE_LIMIT} bytes"
                    ));
                }
                if checked {
                    if checks.iter().any(|(known, _)| known == key) {
                        return Err(format!("replay line {line}: duplicate check `{key}`"));
                    }
                    checks.push((key.to_string(), value.to_string()));
                } else {
                    let Some(known) = UnixFake::KEYS
                        .into_iter()
                        .find(|known| *known == key && !matches!(*known, "log" | "replay"))
                    else {
                        return Err(format!("replay line {line}: `{key}` is not a replayable result"));
                    };
                    if results.insert(known, value.to_string()).is_some() {
                        return Err(format!("replay line {line}: duplicate result `{key}`"));
                    }
                }
            }
            entries.push(ReplayEntry { line, op: op.to_string(), results, checks });
        }
        if entries.is_empty() {
            return Err("replay has no observations".to_string());
        }
        Ok(Self { entries, next: std::sync::atomic::AtomicUsize::new(0) })
    }

    fn consumed(&self) -> usize {
        self.next.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn current(&self) -> Option<&ReplayEntry> {
        self.entries.get(self.consumed())
    }

    fn step(&self, op: &str, fields: &[(&str, String)]) -> Result<(), String> {
        let Some(entry) = self.current() else {
            return Err(format!(
                "unexpected `{op}` after all {} replayed observations were consumed",
                self.entries.len()
            ));
        };
        let at = format!("replay line {}", entry.line);
        if entry.op != op {
            return Err(format!("{at}: expected `{}`, observed `{op}`", entry.op));
        }
        for key in replay_required_results(op, &entry.results) {
            if !entry.results.contains_key(key) {
                return Err(format!("{at}: `{op}` needs result `{key}`"));
            }
        }
        for (key, expected) in &entry.checks {
            match fields.iter().find(|(name, _)| name == key) {
                Some((_, observed)) if observed == expected => {}
                Some((_, observed)) => {
                    return Err(format!("{at}: `{op}` field `{key}` expected `{expected}`, observed `{observed}`"));
                }
                None => return Err(format!("{at}: `{op}` has no field `{key}`")),
            }
        }
        self.next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    fn finish(&self) -> Result<(), String> {
        match self.current() {
            None => Ok(()),
            Some(entry) => Err(format!(
                "replay incomplete: line {} `{}` and {} later observations were never consumed",
                entry.line,
                entry.op,
                self.entries.len() - self.consumed() - 1
            )),
        }
    }
}

#[cfg(feature = "native-tests")]
fn replay_required_results(op: &str, results: &BTreeMap<&'static str, String>) -> Vec<&'static str> {
    match op {
        "tty" => vec!["tty"],
        "notify_ready" => vec!["ready"],
        "reap_child_events" => match results.get("child_pid").map(String::as_str) {
            Some("0") | None => vec!["child_pid"],
            Some(_) => vec!["child_pid", "status_kind", "status_code"],
        },
        "wait_pid1_event" => match results.get("event_kind").map(String::as_str) {
            Some("child" | "children") => vec!["event_kind", "pid", "status_kind", "status_code"],
            Some("signal") => vec!["event_kind", "signal"],
            _ => vec!["event_kind"],
        },
        _ => Vec::new(),
    }
}

#[cfg(feature = "native-tests")]
impl UnixFake {
    /// Settings a fake accepts: the call log path and the fixed values some
    /// entries report.
    pub const KEYS: [&'static str; 10] = [
        "log",
        "replay",
        "tty",
        "event_kind",
        "signal",
        "pid",
        "child_pid",
        "status_kind",
        "status_code",
        "ready",
    ];

    pub fn set(&mut self, key: &str, value: impl Into<String>) -> Result<(), String> {
        let Some(key) = Self::KEYS.into_iter().find(|known| *known == key) else {
            return Err(format!(
                "unknown unix fake setting `{key}`; expected one of {}",
                Self::KEYS.join(", ")
            ));
        };
        let value = value.into();
        if key == "replay" {
            if self.values.keys().any(|known| *known != "log") {
                return Err("unix fake replay cannot be combined with fixed result settings".to_string());
            }
            let text = std::fs::read_to_string(&value)
                .map_err(|error| format!("unix fake replay `{value}` is unreadable: {error}"))?;
            self.replay = Some(std::sync::Arc::new(Replay::parse(&text)?));
        } else if self.replay.is_some() && key != "log" {
            return Err(format!("unix fake setting `{key}` cannot be combined with a replay"));
        }
        self.values.insert(key, value);
        Ok(())
    }

    /// Fails when a replay is installed and some observation was never
    /// consumed by the run.
    pub fn finish_replay(&self) -> Result<(), String> {
        match &self.replay {
            Some(replay) => replay.finish(),
            None => Ok(()),
        }
    }

    pub fn settings(&self) -> impl Iterator<Item = (&'static str, &str)> {
        self.values
            .iter()
            .map(|(key, value)| (*key, value.as_str()))
    }
}

impl Evaluator {
    #[cfg(feature = "native-tests")]
    pub fn with_unix_fake(mut self, fake: UnixFake) -> Self {
        self.unix_fake = Some(std::sync::Arc::new(fake));
        self
    }

    /// One fake setting, or `None` when no fake is installed. Builds without
    /// `native-tests` have no fake at all.
    fn unix_fake_setting(&self, key: &str) -> Option<&str> {
        #[cfg(feature = "native-tests")]
        {
            let fake = self.unix_fake.as_deref()?;
            if key != "log"
                && let Some(replay) = &fake.replay
            {
                return replay.current()?.results.get(key).map(String::as_str);
            }
            return fake.values.get(key).map(String::as_str);
        }
        #[cfg(not(feature = "native-tests"))]
        {
            let _ = key;
            None
        }
    }

    pub(in crate::runtime::eval) fn unix_fake_active(&self) -> bool {
        #[cfg(feature = "native-tests")]
        return self.unix_fake.is_some();
        #[cfg(not(feature = "native-tests"))]
        false
    }

    pub(in crate::runtime::eval) fn unix_fake_replay_active(&self) -> bool {
        #[cfg(feature = "native-tests")]
        return self
            .unix_fake
            .as_deref()
            .is_some_and(|fake| fake.replay.is_some());
        #[cfg(not(feature = "native-tests"))]
        false
    }

    pub(in crate::runtime::eval) fn unix_fake_value(&self, key: &str, default: &str) -> String {
        self.unix_fake_setting(key).unwrap_or(default).to_string()
    }

    pub(in crate::runtime::eval) fn unix_fake_log(
        &self,
        op: &str,
        fields: &[(&str, String)],
        span: Span,
    ) -> Result<(), RuntimeError> {
        #[cfg(feature = "native-tests")]
        if let Some(replay) = self.unix_fake.as_deref().and_then(|fake| fake.replay.as_ref()) {
            replay
                .step(op, fields)
                .map_err(|message| RuntimeError::new("unix-fake-replay", message).with_span(span))?;
        }
        let Some(path) = self.unix_fake_setting("log") else {
            return Ok(());
        };
        append_fake_log(path, "unix-fake-log", op, fields, span)
    }

    pub(in crate::runtime::eval) fn process_kill(
        &mut self,
        pid: i64,
        signal: &str,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        if !(1..=i32::MAX as i64).contains(&pid) {
            return Ok(module_error(
                "pid-range",
                "pid must be a positive process id",
                span,
            ));
        }
        let signal = match signal_info(signal, span) {
            Ok(signal) => signal,
            Err(error) => return Ok(Value::err(Value::Error(Box::new(error)))),
        };
        let Some(pid) = rprocess::Pid::from_raw(pid as i32) else {
            return Ok(module_error(
                "pid-range",
                "pid must be a positive process id",
                span,
            ));
        };
        if signal.number == 0 {
            let error = match rprocess::test_kill_process(pid) {
                Ok(()) => return Ok(Value::ok(Value::Unit)),
                Err(error) => std::io::Error::from(error),
            };
            let (kind, message) = match error.raw_os_error() {
                Some(n) if n == rio::Errno::SRCH.raw_os_error() => {
                    ("process-missing", "process does not exist".to_string())
                }
                Some(n) if n == rio::Errno::PERM.raw_os_error() => {
                    ("permission-denied", "permission denied".to_string())
                }
                Some(n) if n == rio::Errno::INVAL.raw_os_error() => {
                    ("invalid-signal", "invalid signal".to_string())
                }
                _ => ("process-kill", error.to_string()),
            };
            return Ok(Value::err(Value::Error(Box::new(
                RuntimeError::new(kind, message)
                    .with_host_facet(&error)
                    .with_span(span),
            ))));
        }
        let Some(signal) = signal_from_i32(signal.number) else {
            return Ok(module_error("invalid-signal", "invalid signal", span));
        };
        if pid.as_raw_nonzero().get() == std::process::id() as i32
            && self
                .signal_hooks
                .values()
                .any(|hook| hook.signal.number == signal.as_raw_nonzero().get())
        {
            record_signal(signal.as_raw_nonzero().get());
            return Ok(Value::ok(Value::Unit));
        }
        match rprocess::kill_process(pid, signal) {
            Ok(()) => Ok(Value::ok(Value::Unit)),
            Err(error) => {
                let error = std::io::Error::from(error);
                let (kind, message) = match error.raw_os_error() {
                    Some(n) if n == rio::Errno::SRCH.raw_os_error() => {
                        ("process-missing", "process does not exist".to_string())
                    }
                    Some(n) if n == rio::Errno::PERM.raw_os_error() => {
                        ("permission-denied", "permission denied".to_string())
                    }
                    Some(n) if n == rio::Errno::INVAL.raw_os_error() => {
                        ("invalid-signal", "invalid signal".to_string())
                    }
                    _ => ("process-kill", error.to_string()),
                };
                Ok(Value::err(Value::Error(Box::new(
                    RuntimeError::new(kind, message)
                        .with_host_facet(&error)
                        .with_span(span),
                ))))
            }
        }
    }
}

fn signal_from_i32(signal: i32) -> Option<rprocess::Signal> {
    NonZeroI32::new(signal)
        .map(|signal| unsafe { rprocess::Signal::from_raw_nonzero_unchecked(signal) })
}
