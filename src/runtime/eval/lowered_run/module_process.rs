use super::{
    Evaluator, Arc, BTreeMap, CancellationPolicy, ChildWaitOutcome, ControlFlow, Duration, Instant,
    LoweredValue, NativeArgumentValues, ProcessInvocation, RunError, RuntimeError, RuntimeOp, Span,
    SpawnOptions, display_spawn_argv, lowered_command_arg, lowered_duration_arg,
    lowered_process_handle_list_arg, lowered_process_run_error, lowered_process_wait_any_record,
    lowered_result_err_value, lowered_result_ok, lowered_str_arg_owned, lowered_timeout_elapsed,
    path_value_from_pathbuf, poll_managed, resolve_executable, run_error_to_runtime, spawn_command,
};

impl Evaluator {
    pub(super) fn eval_lowered_process_which_values(
        &mut self, mut values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            // A Path names the program by its native bytes; text is
            // UTF-8.
            let name = match values.pop() {
                Some(LoweredValue::Path(path)) => path.bytes,
                value => lowered_str_arg_owned(value, "", "process.which", span)?.into_bytes(),
            };
            if name.is_empty() || name.contains(&0) {
                lowered_result_err_value(
                    RuntimeError::new(
                        "process-which",
                        "command name cannot be empty or contain NUL",
                    )
                    .with_span(span),
                )
            } else {
                let invocation = ProcessInvocation {
                    target: name,
                    argv: Vec::new(),
                    cwd: self.cwd.clone(),
                    env: self.env.snapshot_clone(),
                    env_overlay: BTreeMap::new(),
                    redirections: Vec::new(),
                    timeout: None,
                    cpu_max: None,
                    accepted_exit_codes: None,
                    namespaces: None,
                };
                match resolve_executable(&invocation)
                    .map_err(|error| run_error_to_runtime(error, span))
                    .and_then(|path| {
                        path_value_from_pathbuf(path).map_err(|error| error.with_span(span))
                    }) {
                    Ok(path) => lowered_result_ok(LoweredValue::Path(path)),
                    Err(error) => lowered_result_err_value(error),
                }
            }
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_process_spawn_values(
        &mut self, mut values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let plan = lowered_command_arg(
                values.pop().expect("checked value length"),
                "process.spawn",
                span,
            )?;
            let invocation = self.invocation_from_command_plan(&plan, span)?;
            let options = SpawnOptions {
                detach: plan.detach,
                new_session: plan.new_session,
                ignore_hup: plan.ignore_hup,
                same_group: plan.same_group,
            };
            self.flush_shared_stdio();
            match spawn_command(&invocation, options) {
                Ok(started) => {
                    lowered_result_ok(LoweredValue::Record(Arc::new(BTreeMap::from([
                        (Arc::from("pid"), LoweredValue::Int(started.pid as i64)),
                        (
                            Arc::from("command"),
                            LoweredValue::Str(
                                String::from_utf8_lossy(&started.target).into_owned().into(),
                            ),
                        ),
                        (
                            Arc::from("argv"),
                            LoweredValue::Str(
                                display_spawn_argv(&started.target, &started.argv).into(),
                            ),
                        ),
                        (
                            Arc::from("detach"),
                            LoweredValue::Bool(started.options.detach),
                        ),
                        (
                            Arc::from("new_session"),
                            LoweredValue::Bool(started.options.new_session),
                        ),
                        (
                            Arc::from("ignore_hup"),
                            LoweredValue::Bool(started.options.ignore_hup),
                        ),
                    ]))))
                }
                Err(error) => lowered_result_err_value(run_error_to_runtime(error, span)),
            }
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_process_wait_any_values(
        &mut self, op: RuntimeOp, mut values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let deadline = if op == RuntimeOp::ProcessWaitTimeout {
            let timeout = lowered_duration_arg(values.pop(), "process.wait_timeout", span)?;
            Some(Instant::now() + Duration::from_millis(timeout.millis))
        } else {
            None
        };
        let handles = match lowered_process_handle_list_arg(
            values.pop().expect("checked value length"),
            "process.wait_any",
            span,
            &self.resource_owner,
        )? {
            Ok(handles) => handles,
            Err(error) => {
                return Ok(ControlFlow::Continue(lowered_process_run_error(error)));
            }
        };

        loop {
            for (index, handle) in handles.iter().enumerate() {
                let Some(live) = self.process_handles.get_mut(&handle.id) else {
                    return Ok(ControlFlow::Continue(lowered_process_run_error(
                        RunError::new("unknown", "process handle is no longer live")
                            .with_span(span),
                    )));
                };
                match poll_managed(&mut live.child) {
                    Ok(
                        ChildWaitOutcome::Exited(status)
                        | ChildWaitOutcome::Signaled(status),
                    ) => {
                        let validation_error = live.child.completion_error(&status);
                        let pid = live.child.pid;
                        let group = live.child.process_group();
                        let _ = live;
                        self.process_handles.remove(&handle.id);
                        <Self as CancellationPolicy>::process_group_finished(self, group);
                        self.last_status = Some(status.clone());
                        if let Some(error) = validation_error {
                            return Ok(ControlFlow::Continue(lowered_process_run_error(
                                error.with_span(span),
                            )));
                        }
                        return Ok(ControlFlow::Continue(lowered_result_ok(
                            lowered_process_wait_any_record(index, pid, status),
                        )));
                    }
                    Ok(
                        ChildWaitOutcome::Stopped { .. } | ChildWaitOutcome::StillRunning,
                    ) => {
                        if lowered_timeout_elapsed(live.child.deadline) {
                            live.child.process_group().kill();
                        }
                    }
                    Err(error) => {
                        return Ok(ControlFlow::Continue(lowered_process_run_error(
                            error.with_span(span),
                        )));
                    }
                }
            }

            if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                return Ok(ControlFlow::Continue(lowered_result_ok(LoweredValue::Null)));
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    pub(super) fn eval_lowered_process_wait_ready_values(
        &mut self, mut values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let handles = match lowered_process_handle_list_arg(
            values.pop().expect("checked value length"),
            "process.wait_ready",
            span,
            &self.resource_owner,
        )? {
            Ok(handles) => handles,
            Err(error) => {
                return Ok(ControlFlow::Continue(lowered_process_run_error(error)));
            }
        };

        loop {
            let mut completed = Vec::new();
            let mut completed_ids = rustc_hash::FxHashSet::default();

            for (index, handle) in handles.iter().enumerate() {
                let Some(live) = self.process_handles.get_mut(&handle.id) else {
                    return Ok(ControlFlow::Continue(lowered_process_run_error(
                        RunError::new("unknown", "process handle is no longer live")
                            .with_span(span),
                    )));
                };
                match poll_managed(&mut live.child) {
                    Ok(
                        ChildWaitOutcome::Exited(status)
                        | ChildWaitOutcome::Signaled(status),
                    ) => {
                        completed_ids.insert(handle.id);
                        completed.push((
                            index,
                            handle.id,
                            live.child.pid,
                            live.child.process_group(),
                            live.child.completion_error(&status),
                            status,
                        ));
                    }
                    Ok(
                        ChildWaitOutcome::Stopped { .. } | ChildWaitOutcome::StillRunning,
                    ) => {
                        if lowered_timeout_elapsed(live.child.deadline) {
                            live.child.process_group().kill();
                        }
                    }
                    Err(error) => {
                        return Ok(ControlFlow::Continue(lowered_process_run_error(
                            error.with_span(span),
                        )));
                    }
                }
            }

            if !completed.is_empty() {
                let drain_until = Instant::now() + Duration::from_millis(1);

                while completed.len() < handles.len() && Instant::now() < drain_until {
                    let mut drained = false;

                    for (index, handle) in handles.iter().enumerate() {
                        if completed_ids.contains(&handle.id) {
                            continue;
                        }

                        let Some(live) = self.process_handles.get_mut(&handle.id) else {
                            return Ok(ControlFlow::Continue(lowered_process_run_error(
                                RunError::new(
                                    "unknown",
                                    "process handle is no longer live",
                                )
                                .with_span(span),
                            )));
                        };
                        match poll_managed(&mut live.child) {
                            Ok(
                                ChildWaitOutcome::Exited(status)
                                | ChildWaitOutcome::Signaled(status),
                            ) => {
                                completed_ids.insert(handle.id);
                                completed.push((
                                    index,
                                    handle.id,
                                    live.child.pid,
                                    live.child.process_group(),
                                    live.child.completion_error(&status),
                                    status,
                                ));
                                drained = true;
                            }
                            Ok(
                                ChildWaitOutcome::Stopped { .. }
                                | ChildWaitOutcome::StillRunning,
                            ) => {
                                if lowered_timeout_elapsed(live.child.deadline) {
                                    live.child.process_group().kill();
                                }
                            }
                            Err(error) => {
                                return Ok(ControlFlow::Continue(
                                    lowered_process_run_error(error.with_span(span)),
                                ));
                            }
                        }
                    }

                    if !drained {
                        std::thread::sleep(Duration::from_micros(250));
                    }
                }

                let mut values = Vec::with_capacity(completed.len());
                let mut first_error = None;
                for (index, id, pid, group, error, status) in completed {
                    self.process_handles.remove(&id);
                    <Self as CancellationPolicy>::process_group_finished(self, group);
                    self.last_status = Some(status.clone());
                    if let Some(error) = error {
                        first_error.get_or_insert(error);
                    }
                    values.push(lowered_process_wait_any_record(index, pid, status));
                }

                if let Some(error) = first_error {
                    return Ok(ControlFlow::Continue(lowered_process_run_error(
                        error.with_span(span),
                    )));
                }
                return Ok(ControlFlow::Continue(lowered_result_ok(
                    LoweredValue::List(values),
                )));
            }

            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
