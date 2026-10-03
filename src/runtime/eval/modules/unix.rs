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
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UnixFake {
    values: BTreeMap<&'static str, String>,
}

#[cfg(feature = "native-tests")]
impl UnixFake {
    /// Settings a fake accepts: the call log path and the fixed values some
    /// entries report.
    pub const KEYS: [&'static str; 9] = [
        "log",
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
        self.values.insert(key, value.into());
        Ok(())
    }

    pub fn settings(&self) -> impl Iterator<Item = (&'static str, &str)> {
        self.values.iter().map(|(key, value)| (*key, value.as_str()))
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
        return self.unix_fake.as_deref()?.values.get(key).map(String::as_str);
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

    pub(in crate::runtime::eval) fn unix_fake_value(&self, key: &str, default: &str) -> String {
        self.unix_fake_setting(key).unwrap_or(default).to_string()
    }

    pub(in crate::runtime::eval) fn unix_fake_log(
        &self,
        op: &str,
        fields: &[(&str, String)],
        span: Span,
    ) -> Result<(), RuntimeError> {
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
                RuntimeError::new(kind, message).with_span(span),
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
                    RuntimeError::new(kind, message).with_span(span),
                ))))
            }
        }
    }
}

fn signal_from_i32(signal: i32) -> Option<rprocess::Signal> {
    NonZeroI32::new(signal)
        .map(|signal| unsafe { rprocess::Signal::from_raw_nonzero_unchecked(signal) })
}
