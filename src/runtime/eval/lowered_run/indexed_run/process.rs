use super::{
    BLOCK_LIST, BTreeMap, ControlFlow, Duration, Evaluator, FileRedirectionMode, FullExecution,
    FullPayload, LoweredValue, Name, PathValue, ProcessCommandEntry, ProcessInvocation,
    ProcessRedirection, RUN_ARG_SPLICE, RedirectionKind, RedirectionStream, RunArg, RunEnv,
    RunError, RunKind, RunRedirection, RunSegment, RuntimeError, Span, Value, indexed_decode,
    indexed_error, indexed_finish, indexed_optional_raw, indexed_raw, runtime_error_from_value,
    splice_to_argv, value_to_argv_bytes,
};

/// The executable of a run form and the arguments its target contributes
/// after it. A spliced target, `run @argv`, is a whole command vector: its
/// first element is the executable and the rest lead the arguments. An empty
/// vector names no program, which fails as `ProcessError.InvalidTarget`
/// before anything starts. Any other target is one argv item.
pub(super) fn run_target_and_leading_argv(
    target: &RunArg,
    mut items: Vec<Vec<u8>>,
) -> Result<(Vec<u8>, Vec<Vec<u8>>), RuntimeError> {
    if target.mode == RUN_ARG_SPLICE {
        if items.is_empty() {
            let error = RunError::new(
                "empty-command",
                "spliced command is empty: its first element names the program to run",
            )
            .with_span(target.span);
            let mut failure =
                runtime_error_from_value(Value::RunError(Box::new(error)), target.span);
            failure.propagated = true;
            return Err(failure);
        }
        let executable = items.remove(0);
        return Ok((executable, items));
    }
    let [executable]: [Vec<u8>; 1] = items.try_into().map_err(|_| {
        RuntimeError::new("argv-conversion", "run target must produce one argv item")
            .with_span(target.span)
    })?;
    Ok((executable, Vec::new()))
}

impl Evaluator {
    pub(super) fn decode_indexed_run_arg<'a>(
        payload: &mut FullPayload<'a>,
        execution: &FullExecution<'a>,
        span: Span,
    ) -> Result<RunArg, RuntimeError> {
        let mode = indexed_raw(payload, span)?;
        if mode > 2 {
            return Err(
                RuntimeError::new("indexed-ir", "invalid indexed run argument tag").with_span(span),
            );
        }
        Ok(RunArg {
            mode,
            value: indexed_raw(payload, span)?,
            span: indexed_decode::<Span>(payload, execution, span)?,
        })
    }

    pub(super) fn decode_indexed_run_args<'a>(
        payload: &mut FullPayload<'a>,
        execution: &FullExecution<'a>,
        span: Span,
    ) -> Result<Vec<RunArg>, RuntimeError> {
        let (_, mut values) = execution
            .block(payload, BLOCK_LIST)
            .map_err(|error| indexed_error(error, span))?;
        let len = indexed_raw(&mut values, span)? as usize;
        let mut decoded = Vec::with_capacity(len);
        for _ in 0..len {
            decoded.push(Self::decode_indexed_run_arg(&mut values, execution, span)?);
        }
        indexed_finish(values, span)?;
        Ok(decoded)
    }

    pub(super) fn decode_indexed_run_env<'a>(
        payload: &mut FullPayload<'a>,
        execution: &FullExecution<'a>,
        span: Span,
    ) -> Result<Vec<RunEnv>, RuntimeError> {
        let (_, mut values) = execution
            .block(payload, BLOCK_LIST)
            .map_err(|error| indexed_error(error, span))?;
        let len = indexed_raw(&mut values, span)? as usize;
        let mut decoded = Vec::with_capacity(len);
        for _ in 0..len {
            decoded.push(RunEnv {
                name: indexed_decode::<Name>(&mut values, execution, span)?,
                value: Self::decode_indexed_run_arg(&mut values, execution, span)?,
            });
        }
        indexed_finish(values, span)?;
        Ok(decoded)
    }

    pub(super) fn decode_indexed_run_redirections<'a>(
        payload: &mut FullPayload<'a>,
        execution: &FullExecution<'a>,
        span: Span,
    ) -> Result<Vec<RunRedirection>, RuntimeError> {
        let (_, mut values) = execution
            .block(payload, BLOCK_LIST)
            .map_err(|error| indexed_error(error, span))?;
        let len = indexed_raw(&mut values, span)? as usize;
        let mut decoded = Vec::with_capacity(len);
        for _ in 0..len {
            decoded.push(RunRedirection {
                kind: indexed_decode::<RedirectionKind>(&mut values, execution, span)?,
                target: Self::decode_indexed_run_arg(&mut values, execution, span)?,
                span: indexed_decode::<Span>(&mut values, execution, span)?,
            });
        }
        indexed_finish(values, span)?;
        Ok(decoded)
    }

    pub(super) fn decode_indexed_run_segments<'a>(
        payload: &mut FullPayload<'a>,
        execution: &FullExecution<'a>,
        span: Span,
    ) -> Result<Vec<RunSegment>, RuntimeError> {
        let (_, mut values) = execution
            .block(payload, BLOCK_LIST)
            .map_err(|error| indexed_error(error, span))?;
        let len = indexed_raw(&mut values, span)? as usize;
        let mut decoded = Vec::with_capacity(len);
        for _ in 0..len {
            decoded.push(RunSegment {
                kind: indexed_decode::<RunKind>(&mut values, execution, span)?,
                target: Self::decode_indexed_run_arg(&mut values, execution, span)?,
                args: Self::decode_indexed_run_args(&mut values, execution, span)?,
                env: Self::decode_indexed_run_env(&mut values, execution, span)?,
                redirections: Self::decode_indexed_run_redirections(&mut values, execution, span)?,
                timeout: indexed_optional_raw(&mut values, span)?,
                cpu_max: indexed_optional_raw(&mut values, span)?,
                accept: indexed_optional_raw(&mut values, span)?,
            });
        }
        indexed_finish(values, span)?;
        Ok(decoded)
    }

    pub(super) fn decode_indexed_process_command_entries<'a>(
        payload: &mut FullPayload<'a>,
        execution: &FullExecution<'a>,
        span: Span,
    ) -> Result<Vec<ProcessCommandEntry>, RuntimeError> {
        let (_, mut values) = execution
            .block(payload, BLOCK_LIST)
            .map_err(|error| indexed_error(error, span))?;
        let len = indexed_raw(&mut values, span)? as usize;
        let mut decoded = Vec::with_capacity(len);
        for _ in 0..len {
            decoded.push(match indexed_raw(&mut values, span)? {
                0 => ProcessCommandEntry::Field {
                    name: indexed_decode::<Name>(&mut values, execution, span)?,
                    value: indexed_raw(&mut values, span)?,
                    span: indexed_decode::<Span>(&mut values, execution, span)?,
                },
                1 => ProcessCommandEntry::Run {
                    target: Self::decode_indexed_run_arg(&mut values, execution, span)?,
                    args: Self::decode_indexed_run_args(&mut values, execution, span)?,
                    env: Self::decode_indexed_run_env(&mut values, execution, span)?,
                    timeout: indexed_optional_raw(&mut values, span)?,
                    cpu_max: indexed_optional_raw(&mut values, span)?,
                    accept: indexed_optional_raw(&mut values, span)?,
                    span: indexed_decode::<Span>(&mut values, execution, span)?,
                },
                _ => {
                    return Err(RuntimeError::new(
                        "indexed-ir",
                        "invalid indexed process command entry tag",
                    )
                    .with_span(span));
                }
            });
        }
        indexed_finish(values, span)?;
        Ok(decoded)
    }

    pub(super) fn eval_indexed_run_arg(
        &mut self,
        execution: &FullExecution<'_>,
        arg: &RunArg,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<ControlFlow<LoweredValue, Vec<Vec<u8>>>, RuntimeError> {
        let value = match self.eval_indexed_expr(execution, arg.value, slots, call_span)? {
            ControlFlow::Continue(value) => value.into_value(),
            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
        };
        Self::run_arg_items(arg, value).map(ControlFlow::Continue)
    }

    /// The argv items one evaluated run argument contributes.
    pub(super) fn run_arg_items(arg: &RunArg, value: Value) -> Result<Vec<Vec<u8>>, RuntimeError> {
        match arg.mode {
            0 => Ok(vec![value_to_argv_bytes(value, arg.span)?]),
            1 => match value {
                Value::List(_) => splice_to_argv(value, arg.span),
                value => Ok(vec![value_to_argv_bytes(value, arg.span)?]),
            },
            RUN_ARG_SPLICE => splice_to_argv(value, arg.span),
            _ => unreachable!("indexed run argument tag was checked"),
        }
    }

    pub(super) fn eval_indexed_run_env(
        &mut self,
        execution: &FullExecution<'_>,
        env: &[RunEnv],
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<ControlFlow<LoweredValue, BTreeMap<Vec<u8>, Vec<u8>>>, RuntimeError> {
        let mut overlay = BTreeMap::new();
        for assignment in env {
            let arg = &assignment.value;
            let value = match self.eval_indexed_expr(execution, arg.value, slots, call_span)? {
                ControlFlow::Continue(value) => value.into_value(),
                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
            };
            // An expression value converts as any environment value does. A
            // `$name` word is one value too, so its list is a search path
            // when it holds paths; every other list keeps the word rule that
            // accepts exactly one item, because an empty list does not say
            // what it is a list of.
            let is_path_list = matches!(
                &value,
                Value::List(items)
                    if !items.is_empty() && items.iter().all(|item| matches!(item, Value::Path(_)))
            );
            let value = if arg.mode == 0 || (arg.mode == 1 && is_path_list) {
                super::super::super::value_to_env_bytes(value, arg.span)?
            } else {
                let items = Self::run_arg_items(arg, value)?;
                let [value]: [Vec<u8>; 1] = items.try_into().map_err(|_| {
                    RuntimeError::new("env-value", "environment values must be one value")
                        .with_span(arg.span)
                })?;
                value
            };
            overlay.insert(assignment.name.as_str().as_bytes().to_vec(), value);
        }
        Ok(ControlFlow::Continue(overlay))
    }

    pub(super) fn eval_indexed_run_redirections(
        &mut self,
        execution: &FullExecution<'_>,
        redirections: &[RunRedirection],
        slots: &mut [LoweredValue],
    ) -> Result<ControlFlow<LoweredValue, Vec<ProcessRedirection>>, RuntimeError> {
        let mut out = Vec::with_capacity(redirections.len());
        for redirection in redirections {
            let value = match self.eval_indexed_expr(
                execution,
                redirection.target.value,
                slots,
                redirection.span,
            )? {
                ControlFlow::Continue(value) => value,
                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
            };
            if let LoweredValue::Bytes(bytes) = value {
                if redirection.kind != RedirectionKind::StdinRead || redirection.target.mode == 2 {
                    return Err(RuntimeError::new(
                        "redirection-target",
                        "Bytes are only valid as a single stdin input",
                    )
                    .with_span(redirection.span));
                }
                out.push(ProcessRedirection::Input { bytes });
                continue;
            }
            let value = value.into_value();
            let target = match redirection.target.mode {
                2 => splice_to_argv(value, redirection.target.span)?,
                1 if matches!(value, Value::List(_)) => {
                    splice_to_argv(value, redirection.target.span)?
                }
                _ => vec![value_to_argv_bytes(value, redirection.target.span)?],
            };
            let [target]: [Vec<u8>; 1] = target.try_into().map_err(|_| {
                RuntimeError::new(
                    "redirection-target",
                    "redirection target must produce one path",
                )
                .with_span(redirection.span)
            })?;
            if matches!(
                redirection.kind,
                RedirectionKind::StdoutDup | RedirectionKind::StdinDup
            ) {
                let text = String::from_utf8(target).map_err(|_| {
                    RuntimeError::new(
                        "redirection-target",
                        "fd redirection target must be a number",
                    )
                    .with_span(redirection.span)
                })?;
                let fd = text.trim().parse::<i32>().map_err(|_| {
                    RuntimeError::new(
                        "redirection-target",
                        "fd redirection target must be a number",
                    )
                    .with_span(redirection.span)
                })?;
                out.push(ProcessRedirection::Dup {
                    stream: if redirection.kind == RedirectionKind::StdinDup {
                        RedirectionStream::Stdin
                    } else {
                        RedirectionStream::Stdout
                    },
                    fd,
                });
                continue;
            }
            let path = PathValue::new(target).map_err(|error| error.with_span(redirection.span))?;
            out.push(ProcessRedirection::File {
                stream: match redirection.kind {
                    RedirectionKind::StdinRead => RedirectionStream::Stdin,
                    RedirectionKind::StderrWrite | RedirectionKind::StderrAppend => {
                        RedirectionStream::Stderr
                    }
                    _ => RedirectionStream::Stdout,
                },
                mode: match redirection.kind {
                    RedirectionKind::StdinRead => FileRedirectionMode::Read,
                    RedirectionKind::StdoutAppend | RedirectionKind::StderrAppend => {
                        FileRedirectionMode::Append
                    }
                    _ => FileRedirectionMode::Write,
                },
                path: self.host_path(&path),
            });
        }
        Ok(ControlFlow::Continue(out))
    }

    pub(super) fn indexed_process_invocation(
        &mut self,
        execution: &FullExecution<'_>,
        target: &RunArg,
        args: &[RunArg],
        env: &[RunEnv],
        redirections: &[RunRedirection],
        timeout: Option<u32>,
        cpu_max: Option<u32>,
        accept: Option<u32>,
        slots: &mut [LoweredValue],
        span: Span,
    ) -> Result<ControlFlow<LoweredValue, ProcessInvocation>, RuntimeError> {
        let target_items = match self.eval_indexed_run_arg(execution, target, slots, span)? {
            ControlFlow::Continue(items) => items,
            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
        };
        let (target_value, mut argv) = run_target_and_leading_argv(target, target_items)?;
        for arg in args {
            match self.eval_indexed_run_arg(execution, arg, slots, span)? {
                ControlFlow::Continue(items) => argv.extend(items),
                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
            }
        }
        let env_overlay = match self.eval_indexed_run_env(execution, env, slots, span)? {
            ControlFlow::Continue(value) => value,
            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
        };
        let redirections =
            match self.eval_indexed_run_redirections(execution, redirections, slots)? {
                ControlFlow::Continue(value) => value,
                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
            };
        let timeout = match self.eval_indexed_optional_expr(execution, timeout, slots, span)? {
            ControlFlow::Continue(Some(LoweredValue::Duration(duration))) => {
                Some(Duration::from_millis(duration.millis))
            }
            ControlFlow::Continue(Some(other)) => {
                return Err(RuntimeError::new(
                    "type-error",
                    format!("run timeout expected Duration, found {}", other.type_name()),
                )
                .with_span(span));
            }
            ControlFlow::Continue(None) => None,
            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
        };
        let cpu_max = match self.eval_indexed_optional_expr(execution, cpu_max, slots, span)? {
            ControlFlow::Continue(Some(LoweredValue::Int(value))) => Some(value),
            ControlFlow::Continue(Some(other)) => {
                return Err(RuntimeError::new(
                    "type-error",
                    format!("run cpumax expected Int, found {}", other.type_name()),
                )
                .with_span(span));
            }
            ControlFlow::Continue(None) => None,
            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
        };
        let accepted_exit_codes =
            match self.eval_indexed_optional_expr(execution, accept, slots, span)? {
                ControlFlow::Continue(value) => value
                    .map(|value| super::super::lowered_accepted_exit_codes(value, span))
                    .transpose()?,
                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
            };
        let mut full_env = self.env.snapshot_clone();
        full_env.extend(env_overlay.clone());
        Ok(ControlFlow::Continue(ProcessInvocation {
            target: target_value,
            argv,
            cwd: self.cwd.clone(),
            env: full_env,
            env_overlay,
            redirections,
            timeout,
            cpu_max,
            accepted_exit_codes,
            namespaces: None,
        }))
    }
}
