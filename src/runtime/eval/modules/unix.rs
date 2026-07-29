use super::{Evaluator, module_error};
use crate::modules::process::signal_info;
use crate::runtime::process::record_signal;
use crate::runtime::value::{RuntimeError, Value};
use crate::source::Span;
use rustix::{io as rio, process as rprocess};
use std::num::NonZeroI32;

impl Evaluator {
    pub(in crate::runtime::eval) fn unix_dry_run(&self) -> bool {
        self.env
            .get_owned(b"XSH_UNIX_DRY_RUN".as_slice())
            .and_then(|value| String::from_utf8(value).ok())
            .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
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
