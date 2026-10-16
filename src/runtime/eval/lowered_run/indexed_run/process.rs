use super::{
    ControlFlow, Evaluator, FileRedirectionMode, LoweredValue, PathValue,
    ProcessEnd, ProcessInvocation, ProcessStatus, RunArg, RunKind,
    ProcessRedirection, RedirectionKind, RedirectionStream, RunError, RuntimeError, Span, Value,
    RUN_ARG_SPLICE, execute_run_with_policy, lowered_process_run_error,
    lowered_value_from_runtime_any, run_pipeline_inherit_with_policy,
    runtime_error_from_value, splice_to_argv, value_to_argv_bytes,
};

/// A spliced target supplies the executable and its leading argv words. An empty
/// vector fails before anything starts because it names no program.
pub(super) fn run_target_and_leading_argv(
    mode: u32,
    span: Span,
    mut items: Vec<Vec<u8>>,
) -> Result<(Vec<u8>, Vec<Vec<u8>>), RuntimeError> {
    if mode == RUN_ARG_SPLICE {
        if items.is_empty() {
            let error = RunError::new(
                "empty-command",
                "spliced command is empty: its first element names the program to run",
            )
            .with_span(span);
            let mut failure = runtime_error_from_value(Value::RunError(Box::new(error)), span);
            failure.propagated = true;
            return Err(failure);
        }
        let executable = items.remove(0);
        return Ok((executable, items));
    }
    let [executable]: [Vec<u8>; 1] = items.try_into().map_err(|_| {
        RuntimeError::new("argv-conversion", "run target must produce one argv item")
            .with_span(span)
    })?;
    Ok((executable, Vec::new()))
}

impl Evaluator {
    pub(super) fn run_arg_items_for(
        mode: u32,
        span: Span,
        value: Value,
    ) -> Result<Vec<Vec<u8>>, RuntimeError> {
        Self::run_arg_items_mode(mode, span, value)
    }

    /// Convert one evaluated run argument without changing its source descriptor.
    fn run_arg_items_mode(
        mode: u32,
        span: Span,
        value: Value,
    ) -> Result<Vec<Vec<u8>>, RuntimeError> {
        match mode {
            0 => Ok(vec![value_to_argv_bytes(value, span)?]),
            1 => match value {
                Value::List(_) => splice_to_argv(value, span),
                value => Ok(vec![value_to_argv_bytes(value, span)?]),
            },
            RUN_ARG_SPLICE => splice_to_argv(value, span),
            _ => unreachable!("indexed run argument tag was checked"),
        }
    }

    /// Convert an evaluated environment word, retaining Path-list search-path behavior.
    pub(super) fn run_env_value_for(
        mode: u32,
        span: Span,
        value: Value,
    ) -> Result<Vec<u8>, RuntimeError> {
        Self::run_env_value_mode(mode, span, value)
    }

    pub(super) fn run_env_value(
        arg: &RunArg,
        value: Value,
    ) -> Result<Vec<u8>, RuntimeError> {
        Self::run_env_value_mode(arg.mode, arg.span, value)
    }

    fn run_env_value_mode(
        mode: u32,
        span: Span,
        value: Value,
    ) -> Result<Vec<u8>, RuntimeError> {
        let is_path_list = matches!(
            &value,
            Value::List(items)
                if !items.is_empty() && items.iter().all(|item| matches!(item, Value::Path(_)))
        );
        if mode == 0 || (mode == 1 && is_path_list) {
            super::super::super::value_to_env_bytes(value, span)
        } else {
            let items = Self::run_arg_items_mode(mode, span, value)?;
            let [value]: [Vec<u8>; 1] = items.try_into().map_err(|_| {
                RuntimeError::new("env-value", "environment values must be one value")
                    .with_span(span)
            })?;
            Ok(value)
        }
    }

    pub(super) fn run_redirection_value(
        &self,
        kind: RedirectionKind,
        mode: u32,
        target_span: Span,
        span: Span,
        value: LoweredValue,
    ) -> Result<ProcessRedirection, RuntimeError> {
        if let LoweredValue::Bytes(bytes) = value {
            if kind != RedirectionKind::StdinRead || mode == RUN_ARG_SPLICE {
                return Err(RuntimeError::new(
                    "redirection-target",
                    "Bytes are only valid as a single stdin input",
                )
                .with_span(span));
            }
            return Ok(ProcessRedirection::Input { bytes });
        }
        let value = value.into_value();
        let target = match mode {
            RUN_ARG_SPLICE => splice_to_argv(value, target_span)?,
            1 if matches!(value, Value::List(_)) => splice_to_argv(value, target_span)?,
            _ => vec![value_to_argv_bytes(value, target_span)?],
        };
        let [target]: [Vec<u8>; 1] = target.try_into().map_err(|_| {
            RuntimeError::new(
                "redirection-target",
                "redirection target must produce one path",
            )
            .with_span(span)
        })?;
        if matches!(kind, RedirectionKind::StdoutDup | RedirectionKind::StdinDup) {
            let text = String::from_utf8(target).map_err(|_| {
                RuntimeError::new(
                    "redirection-target",
                    "fd redirection target must be a number",
                )
                .with_span(span)
            })?;
            let fd = text.trim().parse::<i32>().map_err(|_| {
                RuntimeError::new(
                    "redirection-target",
                    "fd redirection target must be a number",
                )
                .with_span(span)
            })?;
            return Ok(ProcessRedirection::Dup {
                stream: if kind == RedirectionKind::StdinDup {
                    RedirectionStream::Stdin
                } else {
                    RedirectionStream::Stdout
                },
                fd,
            });
        }
        let path = PathValue::new(target).map_err(|error| error.with_span(span))?;
        Ok(ProcessRedirection::File {
            stream: match kind {
                RedirectionKind::StdinRead => RedirectionStream::Stdin,
                RedirectionKind::StderrWrite | RedirectionKind::StderrAppend => {
                    RedirectionStream::Stderr
                }
                _ => RedirectionStream::Stdout,
            },
            mode: match kind {
                RedirectionKind::StdinRead => FileRedirectionMode::Read,
                RedirectionKind::StdoutAppend | RedirectionKind::StderrAppend => {
                    FileRedirectionMode::Append
                }
                _ => FileRedirectionMode::Write,
            },
            path: self.host_path(&path),
        })
    }
}

pub(super) fn run_cpu_max_value(value: LoweredValue, span: Span) -> Result<i64, RuntimeError> {
    match value {
        LoweredValue::Int(value) if value > 0 => Ok(value),
        LoweredValue::Int(_) => Err(
            RuntimeError::new("cpu-max", "cpu_max must be positive").with_span(span),
        ),
        other => Err(RuntimeError::new(
            "type-error",
            format!("run cpumax expected Int, found {}", other.type_name()),
        )
        .with_span(span)),
    }
}

impl Evaluator {
    pub(super) fn apply_indexed_run_capture(
        &mut self,
        kind: RunKind,
        invocation: ProcessInvocation,
        propagate: bool,
        assert_success: bool,
        span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        if invocation.accepted_exit_codes.is_some()
            && matches!(kind, RunKind::StreamText | RunKind::StreamBytes)
        {
            let value = self.start_policy_process_stream(
                &invocation,
                kind == RunKind::StreamText,
                span,
            )?;
            if propagate && matches!(value, LoweredValue::ResultErr(_)) {
                let value = self.lowered_question_propagation_value(value, span)?;
                return Ok(self.preserve_lexical_expression_flow(
                    super::StmtFlow::Propagate(value),
                ));
            }
            return Ok(ControlFlow::Continue(value));
        }
        self.trace_process_run_start(span, &invocation);
        let execution = execute_run_with_policy(
            kind,
            std::slice::from_ref(&invocation),
            span,
            assert_success,
            self,
        );
        if let Some(status) = execution.end.status.clone() {
            self.last_status = Some(status);
        }
        self.trace_process_run_end(span, &execution.end);
        if self.signal_state.shutdown_complete && self.signal_state.shutdown_status.is_some() {
            return Ok(ControlFlow::Continue(LoweredValue::ResultOk(Box::new(
                LoweredValue::Status(Box::new(
                    execution
                        .end
                        .status
                        .clone()
                        .unwrap_or_else(|| ProcessStatus::signaled(libc::SIGTERM)),
                )),
            ))));
        }
        let value = execution.value?;
        let mut value = lowered_value_from_runtime_any(&value).ok_or_else(|| {
            RuntimeError::new(
                "type-error",
                format!("lowered run produced unsupported {}", value.type_name()),
            )
            .with_span(span)
        })?;
        if matches!(kind, RunKind::Status)
            && let LoweredValue::ResultOk(inner) = value
        {
            value = *inner;
        }
        if (propagate
            || (matches!(kind, RunKind::Status | RunKind::Plain)
                && invocation.accepted_exit_codes.is_some()))
            && matches!(value, LoweredValue::ResultErr(_))
        {
            let value = self.lowered_question_propagation_value(value, span)?;
            return Ok(self.preserve_lexical_expression_flow(super::StmtFlow::Propagate(value)));
        }
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn apply_indexed_run_pipeline(
        &mut self,
        head: RunKind,
        invocations: &[ProcessInvocation],
        propagate: bool,
        span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        self.trace_lowered_pipeline_enter(span);
        if !matches!(head, RunKind::Plain | RunKind::Status) {
            return self.eval_capture_pipeline(head, invocations, span);
        }
        let mut end = match run_pipeline_inherit_with_policy(invocations, self) {
            Ok(end) => end,
            Err(error) => {
                self.trace_lowered_pipeline_end(
                    span,
                    &ProcessEnd {
                        pid: Some(0),
                        status: error.status.as_deref().cloned(),
                        error: Some(error.clone()),
                    },
                );
                return Ok(ControlFlow::Continue(lowered_process_run_error(error)));
            }
        };
        if let Some(status) = &end.status {
            self.last_status = Some(status.clone());
        }
        let validation_error = end.status.as_ref().and_then(|status| {
            crate::runtime::run::run_completion_error(status, invocations, propagate)
        });
        end.error = validation_error.clone();
        self.trace_lowered_pipeline_end(span, &end);
        if self.signal_state.shutdown_complete && self.signal_state.shutdown_status.is_some() {
            return Ok(ControlFlow::Continue(LoweredValue::ResultOk(Box::new(
                LoweredValue::Status(Box::new(
                    end.status
                        .clone()
                        .unwrap_or_else(|| ProcessStatus::signaled(libc::SIGTERM)),
                )),
            ))));
        }
        let status = end
            .status
            .clone()
            .unwrap_or_else(|| ProcessStatus::exited(1));
        if let Some(error) = validation_error {
            let value = lowered_process_run_error(error.with_span(span));
            if invocations
                .iter()
                .any(|invocation| invocation.accepted_exit_codes.is_some())
            {
                let value = self.lowered_question_propagation_value(value, span)?;
                return Ok(self.preserve_lexical_expression_flow(
                    super::StmtFlow::Propagate(value),
                ));
            }
            Ok(ControlFlow::Continue(value))
        } else if propagate {
            Ok(ControlFlow::Continue(LoweredValue::ResultOk(Box::new(
                LoweredValue::Status(Box::new(status)),
            ))))
        } else {
            Ok(ControlFlow::Continue(LoweredValue::Status(Box::new(status))))
        }
    }
}
