#![allow(clippy::single_call_fn)]

use crate::runtime::process::{
    CancellationPolicy, ProcessEnd, ProcessInvocation, ProcessStatus, run_capture,
    run_capture_with_policy, run_capture_with_stderr, run_capture_with_stderr_policy,
    run_pipeline_inherit, run_pipeline_inherit_with_policy,
};
use crate::runtime::value::{RecordMap, RunError, RuntimeError, StreamValue, Value};
use crate::source::Span;
use crate::syntax::node::RunKind;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct RunExecution {
    pub value: Result<Value, RuntimeError>,
    pub end: ProcessEnd,
}

pub fn execute_run(
    kind: RunKind,
    invocations: &[ProcessInvocation],
    span: Span,
    assert_success: bool,
) -> RunExecution {
    match kind {
        RunKind::Plain | RunKind::Status => run_status_form(invocations, span, assert_success),
        RunKind::CaptureText
        | RunKind::CaptureBytes
        | RunKind::CaptureTextRecord
        | RunKind::CaptureBytesRecord
        | RunKind::StreamText
        | RunKind::StreamBytes => run_capture_form(kind, &invocations[0], span),
    }
}

pub(crate) fn execute_run_with_policy(
    kind: RunKind,
    invocations: &[ProcessInvocation],
    span: Span,
    assert_success: bool,
    policy: &mut dyn CancellationPolicy,
) -> RunExecution {
    match kind {
        RunKind::Plain | RunKind::Status => {
            run_status_form_with_policy(invocations, span, assert_success, policy)
        }
        RunKind::CaptureText
        | RunKind::CaptureBytes
        | RunKind::CaptureTextRecord
        | RunKind::CaptureBytesRecord
        | RunKind::StreamText
        | RunKind::StreamBytes => run_capture_form_with_policy(kind, &invocations[0], span, policy),
    }
}

fn run_status_form(
    invocations: &[ProcessInvocation],
    span: Span,
    assert_success: bool,
) -> RunExecution {
    match run_pipeline_inherit(invocations) {
        Ok(mut end) => {
            let status = end.status.clone().expect("completed process has status");
            if assert_success {
                if status.success {
                    RunExecution {
                        value: Ok(Value::ok(Value::Status(status))),
                        end,
                    }
                } else {
                    let error = run_error_from_status(status, invocations).with_span(span);
                    end.error = Some(error.clone());
                    RunExecution {
                        value: Ok(Value::err(Value::RunError(Box::new(error)))),
                        end,
                    }
                }
            } else {
                RunExecution {
                    value: Ok(Value::Status(status)),
                    end,
                }
            }
        }
        Err(error) => run_error_value(error, span),
    }
}

fn run_status_form_with_policy(
    invocations: &[ProcessInvocation],
    span: Span,
    assert_success: bool,
    policy: &mut dyn CancellationPolicy,
) -> RunExecution {
    match run_pipeline_inherit_with_policy(invocations, policy) {
        Ok(mut end) => {
            let status = end.status.clone().expect("completed process has status");
            if assert_success {
                if status.success {
                    RunExecution {
                        value: Ok(Value::ok(Value::Status(status))),
                        end,
                    }
                } else {
                    let error = run_error_from_status(status, invocations).with_span(span);
                    end.error = Some(error.clone());
                    RunExecution {
                        value: Ok(Value::err(Value::RunError(Box::new(error)))),
                        end,
                    }
                }
            } else {
                RunExecution {
                    value: Ok(Value::Status(status)),
                    end,
                }
            }
        }
        Err(error) => run_error_value(error, span),
    }
}

fn run_capture_form(kind: RunKind, invocation: &ProcessInvocation, span: Span) -> RunExecution {
    let capture_stderr = matches!(
        kind,
        RunKind::CaptureTextRecord | RunKind::CaptureBytesRecord
    );
    let captured = if capture_stderr {
        run_capture_with_stderr(invocation)
    } else {
        run_capture(invocation)
    };
    match captured {
        Ok(mut output) => {
            let status = output
                .end
                .status
                .clone()
                .expect("completed process has status");
            if !status.success
                && !matches!(
                    kind,
                    RunKind::CaptureTextRecord | RunKind::CaptureBytesRecord
                )
            {
                let error =
                    run_error_from_status(status, std::slice::from_ref(invocation)).with_span(span);
                output.end.error = Some(error.clone());
                return RunExecution {
                    value: Ok(Value::err(Value::RunError(Box::new(error)))),
                    end: output.end,
                };
            }

            let value = match kind {
                RunKind::CaptureText => match String::from_utf8(output.stdout) {
                    Ok(text) => Value::ok(Value::Str(text.into())),
                    Err(_) => {
                        let error =
                            RunError::new("invalid-utf8", "captured stdout was not valid UTF-8")
                                .with_span(span);
                        output.end.error = Some(error.clone());
                        Value::err(Value::RunError(Box::new(error)))
                    }
                },
                RunKind::CaptureBytes => Value::ok(Value::Bytes(output.stdout)),
                RunKind::CaptureTextRecord => match (
                    String::from_utf8(output.stdout),
                    String::from_utf8(output.stderr),
                ) {
                    (Ok(stdout), Ok(stderr)) => Value::ok(capture_record(
                        status,
                        Value::Str(stdout.into()),
                        Value::Str(stderr.into()),
                    )),
                    _ => {
                        let error = RunError::new(
                            "invalid-utf8",
                            "captured stdout or stderr was not valid UTF-8",
                        )
                        .with_span(span);
                        output.end.error = Some(error.clone());
                        Value::err(Value::RunError(Box::new(error)))
                    }
                },
                RunKind::CaptureBytesRecord => Value::ok(capture_record(
                    status,
                    Value::Bytes(output.stdout),
                    Value::Bytes(output.stderr),
                )),
                RunKind::StreamText => match String::from_utf8(output.stdout) {
                    Ok(text) => Value::ok(Value::stream(StreamValue::from_values(
                        text.lines().map(|line| Value::Str(line.into())).collect(),
                    ))),
                    Err(_) => {
                        let error =
                            RunError::new("invalid-utf8", "streamed stdout was not valid UTF-8")
                                .with_span(span);
                        output.end.error = Some(error.clone());
                        Value::err(Value::RunError(Box::new(error)))
                    }
                },
                RunKind::StreamBytes => {
                    Value::ok(Value::stream(StreamValue::from_values(vec![Value::Bytes(
                        output.stdout,
                    )])))
                }
                RunKind::Plain | RunKind::Status => unreachable!("capture form expected"),
            };
            RunExecution {
                value: Ok(value),
                end: output.end,
            }
        }
        Err(error) => run_error_value(error, span),
    }
}

fn run_capture_form_with_policy(
    kind: RunKind,
    invocation: &ProcessInvocation,
    span: Span,
    policy: &mut dyn CancellationPolicy,
) -> RunExecution {
    let capture_stderr = matches!(
        kind,
        RunKind::CaptureTextRecord | RunKind::CaptureBytesRecord
    );
    let captured = if capture_stderr {
        run_capture_with_stderr_policy(invocation, policy)
    } else {
        run_capture_with_policy(invocation, policy)
    };
    match captured {
        Ok(mut output) => {
            let status = output
                .end
                .status
                .clone()
                .expect("completed process has status");
            if !status.success
                && !matches!(
                    kind,
                    RunKind::CaptureTextRecord | RunKind::CaptureBytesRecord
                )
            {
                let error =
                    run_error_from_status(status, std::slice::from_ref(invocation)).with_span(span);
                output.end.error = Some(error.clone());
                return RunExecution {
                    value: Ok(Value::err(Value::RunError(Box::new(error)))),
                    end: output.end,
                };
            }

            let value = match kind {
                RunKind::CaptureText => match String::from_utf8(output.stdout) {
                    Ok(text) => Value::ok(Value::Str(text.into())),
                    Err(_) => {
                        let error =
                            RunError::new("invalid-utf8", "captured stdout was not valid UTF-8")
                                .with_span(span);
                        output.end.error = Some(error.clone());
                        Value::err(Value::RunError(Box::new(error)))
                    }
                },
                RunKind::CaptureBytes => Value::ok(Value::Bytes(output.stdout)),
                RunKind::CaptureTextRecord => match (
                    String::from_utf8(output.stdout),
                    String::from_utf8(output.stderr),
                ) {
                    (Ok(stdout), Ok(stderr)) => Value::ok(capture_record(
                        status,
                        Value::Str(stdout.into()),
                        Value::Str(stderr.into()),
                    )),
                    _ => {
                        let error = RunError::new(
                            "invalid-utf8",
                            "captured stdout or stderr was not valid UTF-8",
                        )
                        .with_span(span);
                        output.end.error = Some(error.clone());
                        Value::err(Value::RunError(Box::new(error)))
                    }
                },
                RunKind::CaptureBytesRecord => Value::ok(capture_record(
                    status,
                    Value::Bytes(output.stdout),
                    Value::Bytes(output.stderr),
                )),
                RunKind::StreamText => match String::from_utf8(output.stdout) {
                    Ok(text) => Value::ok(Value::stream(StreamValue::from_values(
                        text.lines().map(|line| Value::Str(line.into())).collect(),
                    ))),
                    Err(_) => {
                        let error =
                            RunError::new("invalid-utf8", "streamed stdout was not valid UTF-8")
                                .with_span(span);
                        output.end.error = Some(error.clone());
                        Value::err(Value::RunError(Box::new(error)))
                    }
                },
                RunKind::StreamBytes => {
                    Value::ok(Value::stream(StreamValue::from_values(vec![Value::Bytes(
                        output.stdout,
                    )])))
                }
                RunKind::Plain | RunKind::Status => unreachable!("capture form expected"),
            };
            RunExecution {
                value: Ok(value),
                end: output.end,
            }
        }
        Err(error) => run_error_value(error, span),
    }
}

fn capture_record(
    status: crate::runtime::process::ProcessStatus,
    stdout: Value,
    stderr: Value,
) -> Value {
    Value::Record(RecordMap::from([
        (Arc::from("status"), Value::Status(status)),
        (Arc::from("stdout"), stdout),
        (Arc::from("stderr"), stderr),
    ]))
}

fn run_error_from_status(status: ProcessStatus, invocations: &[ProcessInvocation]) -> RunError {
    let mut error = RunError::from_status(status);
    let Some(status) = error.status.as_deref() else {
        return error;
    };
    let Some(segment) = status.segments.iter().find(|segment| !segment.success) else {
        return error;
    };
    let Some(invocation) = invocations.get(segment.index) else {
        return error;
    };
    error.message.push_str("\ncwd: ");
    error
        .message
        .push_str(&invocation.cwd.display().to_string());
    error.message.push_str("\nargv: ");
    error
        .message
        .push_str(&shell_escaped_argv(&invocation.target, &invocation.argv));
    error
}

fn shell_escaped_argv(target: &[u8], argv: &[Vec<u8>]) -> String {
    std::iter::once(target)
        .chain(argv.iter().map(Vec::as_slice))
        .map(shell_escape_bytes)
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_escape_bytes(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    if !text.is_empty()
        && text.bytes().all(|byte| {
            matches!(
                byte,
                b'A'..=b'Z'
                    | b'a'..=b'z'
                    | b'0'..=b'9'
                    | b'_'
                    | b'@'
                    | b'%'
                    | b'+'
                    | b'='
                    | b':'
                    | b','
                    | b'.'
                    | b'/'
                    | b'-'
            )
        })
    {
        return text.into_owned();
    }
    format!("'{}'", text.replace('\'', "'\\''"))
}

fn run_error_value(error: RunError, span: Span) -> RunExecution {
    let error = error.with_span(span);
    RunExecution {
        value: Ok(Value::err(Value::RunError(Box::new(error.clone())))),
        end: ProcessEnd {
            pid: None,
            status: error.status.as_deref().cloned(),
            error: Some(error),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{RunKind, Value, execute_run};
    use crate::runtime::process::ProcessInvocation;
    use crate::runtime::value::ResultValue;
    use crate::source::{SourceId, Span};
    use std::path::PathBuf;

    #[test]
    fn missing_target_returns_run_error_value_and_process_end() {
        let span = Span::new(SourceId::new(0), 0, 4);
        let mut env: std::collections::BTreeMap<Vec<u8>, Vec<u8>> = std::env::vars_os()
            .map(|(name, value)| {
                use std::os::unix::ffi::OsStrExt;
                (
                    name.as_os_str().as_bytes().to_vec(),
                    value.as_os_str().as_bytes().to_vec(),
                )
            })
            .collect();
        env.insert(b"PATH".to_vec(), b"/bin:/usr/bin".to_vec());
        let invocation = ProcessInvocation {
            target: b"xsh-definitely-missing-command".to_vec(),
            argv: Vec::new(),
            cwd: PathBuf::from("."),
            env,
            env_overlay: Default::default(),
            redirections: Vec::new(),
            timeout: None,
            cpu_max: None,
        };

        let execution = execute_run(RunKind::Plain, &[invocation], span, true);

        let Value::Result(ResultValue::Err(error)) = execution.value.unwrap() else {
            panic!("expected Err result");
        };
        assert_eq!(error.error_kind(), Some("not-found"));
        assert_eq!(
            execution
                .end
                .error
                .as_ref()
                .map(|error| error.kind.as_str()),
            Some("not-found")
        );
    }
}
