use super::{
    BTreeMap, ContextScopeRestore, ControlFlow, Duration, Evaluator, Flow, FullExecution,
    LoweredValue, Propagation, RegisteredDefer, RuntimeError, Span, StmtFlow, TraceArg, TraceError,
    TraceKind, TracePayload, Traceback, Value, capture_checked_error, check_env_name, fs_module,
    indexed_error, lowered_duration_arg, lowered_path_like_arg, path_bytes,
    runtime_error_from_value,
};

impl Evaluator {
    pub(super) fn enter_indexed_context_scope(
        &mut self,
        kind: crate::syntax::arena::ContextScopeKind,
        value: LoweredValue,
        span: Span,
    ) -> Result<ContextScopeRestore, RuntimeError> {
        match kind {
            crate::syntax::arena::ContextScopeKind::Cwd => {
                let target = lowered_path_like_arg(value, "cd", span)?;
                let previous = self.cwd.clone();
                let next = self.host_path(&target);
                match fs_module::cd_target_is_dir(&next) {
                    Ok(true) => {}
                    Ok(false) => {
                        return Err(RuntimeError::new(
                            "cwd-not-directory",
                            "cwd target is not a directory",
                        )
                        .with_span(span));
                    }
                    Err(error) => return Err(RuntimeError::host("cwd", &error).with_span(span)),
                }
                self.trace_enter(
                    TraceKind::CwdEnter,
                    Some(span),
                    Some("cd"),
                    TracePayload::Cwd {
                        previous: TraceArg::bytes(path_bytes(&previous)),
                        current: TraceArg::bytes(path_bytes(&next)),
                    },
                );
                self.cwd = next;
                Ok(ContextScopeRestore::Cwd { previous, span })
            }
            crate::syntax::arena::ContextScopeKind::Env => {
                let fields = match value {
                    LoweredValue::Record(fields) => fields
                        .iter()
                        .map(|(name, value)| (name.to_string(), value.clone()))
                        .collect::<Vec<_>>(),
                    LoweredValue::RecordVec(fields) => fields
                        .iter()
                        .map(|(name, value)| (name.to_string(), value.clone()))
                        .collect(),
                    LoweredValue::Map(fields) => fields
                        .iter()
                        .map(|(name, value)| {
                            let name = name.as_str().ok_or_else(|| {
                                RuntimeError::new(
                                    "env-name",
                                    "environment overlay keys must be Str",
                                )
                                .with_span(span)
                            })?;
                            Ok((name.to_string(), value.clone()))
                        })
                        .collect::<Result<Vec<_>, RuntimeError>>()?,
                    _ => {
                        return Err(RuntimeError::new(
                            "type-error",
                            "environment overlay requires Record or string-keyed Map",
                        )
                        .with_span(span));
                    }
                };
                let mut overlay = BTreeMap::new();
                for (name, value) in fields {
                    check_env_name(&name, span)?;
                    let value = super::super::super::value_to_env_bytes(value.into_value(), span)?;
                    overlay.insert(name.into_bytes(), value);
                }
                let previous = self.env.clone();
                self.env.extend(overlay);
                Ok(ContextScopeRestore::Env(previous))
            }
            crate::syntax::arena::ContextScopeKind::Within => {
                let limit = lowered_duration_arg(Some(value), "within", span)?;
                let id = self.open_within_deadline(Duration::from_millis(limit.millis), span);
                Ok(ContextScopeRestore::Within { id })
            }
        }
    }

    pub(in crate::runtime::eval) fn context_scope_runtime_value_escapes(value: &Value) -> bool {
        value.resource_reachable_values().any(|value| {
            matches!(
                value,
                Value::Stream(_) | Value::ProcessHandle(_) | Value::NetJob(_)
            )
        })
    }

    pub(super) fn context_scope_runtime_error_escapes(error: &RuntimeError) -> bool {
        error.abort.is_none()
            && error.propagated
            && error.resource_reachable_values().any(|value| {
                matches!(
                    value,
                    Value::Stream(_) | Value::ProcessHandle(_) | Value::NetJob(_)
                )
            })
    }

    pub(in crate::runtime::eval) fn context_scope_value_escapes(value: &LoweredValue) -> bool {
        match value {
            LoweredValue::Stream(_) | LoweredValue::ProcessHandle(_) | LoweredValue::NetJob(_) => {
                true
            }
            LoweredValue::List(items) => items.iter().any(Self::context_scope_value_escapes),
            LoweredValue::SharedList(items) => items.iter().any(Self::context_scope_value_escapes),
            LoweredValue::Map(fields) => fields.values().any(Self::context_scope_value_escapes),
            LoweredValue::Record(fields) | LoweredValue::Module(fields) => {
                fields.values().any(Self::context_scope_value_escapes)
            }
            LoweredValue::RecordVec(fields) => fields
                .iter()
                .any(|(_, value)| Self::context_scope_value_escapes(value)),
            LoweredValue::Tag(tag) => tag.fields.iter().any(Self::context_scope_value_escapes),
            LoweredValue::ResultOk(value) => Self::context_scope_value_escapes(value),
            LoweredValue::Error(value) | LoweredValue::ResultErr(value) => {
                Self::context_scope_runtime_value_escapes(value)
            }
            _ => false,
        }
    }

    pub(super) fn restore_indexed_context_scope(&mut self, restore: ContextScopeRestore) {
        match restore {
            ContextScopeRestore::Cwd { previous, span } => {
                let current = self.cwd.clone();
                self.cwd = previous.clone();
                self.trace_exit(
                    TraceKind::CwdExit,
                    Some(span),
                    Some("cd"),
                    TracePayload::Cwd {
                        previous: TraceArg::bytes(path_bytes(&current)),
                        current: TraceArg::bytes(path_bytes(&previous)),
                    },
                );
            }
            ContextScopeRestore::Env(previous) => {
                self.env = previous;
            }
            ContextScopeRestore::Within { id } => self.close_within_deadline(id),
        }
    }

    pub(in crate::runtime::eval) fn eval_indexed_body_as_signal_hook(
        &mut self,
        view: crate::runtime::eval::indexed::full::FullDriverStepView<'_>,
        body: u32,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<Flow, RuntimeError> {
        let execution = view
            .execution()
            .map_err(|error| indexed_error(error, call_span))?;
        let flow = self.eval_indexed_statement_block(&execution, body, slots, call_span)?;
        match flow {
            StmtFlow::None => Ok(Flow::Continue(Value::Unit)),
            StmtFlow::Value(value) | StmtFlow::Return(value) => {
                Ok(Flow::Continue(value.into_value()))
            }
            StmtFlow::Propagate(value) => {
                let error = match value {
                    LoweredValue::Error(error) => *error,
                    LoweredValue::ResultErr(error) => *error,
                    other => Value::Error(Box::new(
                        RuntimeError::new(
                            "signal-hook",
                            format!("propagated {}", other.type_name()),
                        )
                        .with_span(call_span),
                    )),
                };
                let traceback = self.pending_traceback.take().unwrap_or_else(|| Traceback {
                    failing_span: Some(call_span),
                    exe_path: self.exe_path_for_traceback(),
                    operation_kind: "signal.hook".to_string(),
                    error: TraceError::from_value(&error),
                    frames: self.call_stack.clone(),
                });
                Ok(Flow::Propagate(Propagation { error, traceback }))
            }
            StmtFlow::Break(_) | StmtFlow::Continue => Ok(Flow::Continue(Value::Unit)),
        }
    }

    pub(in crate::runtime::eval::lowered_run) fn eval_indexed_deferred_expr(
        &mut self,
        execution: &FullExecution<'_>,
        value: u32,
        slots: &mut [LoweredValue],
        span: Span,
    ) -> Result<(), RuntimeError> {
        // Cleanup is never cut short by a `within` deadline: the scopes that
        // are open are set aside while the action runs, and a deadline that
        // passes meanwhile is delivered at the first checkpoint after it. A
        // scope the action opens itself applies to it as usual.
        let shielded = self.shield_within_deadlines();
        self.cleanup_depth += 1;
        let result = self.eval_indexed_expr(execution, value, slots, span);
        self.cleanup_depth -= 1;
        self.unshield_within_deadlines(shielded);
        let pending = self.pending_value_block_flow.take();
        let value = match (result?, pending) {
            (_, Some(StmtFlow::Propagate(value) | StmtFlow::Return(value))) => value,
            (_, Some(_)) => {
                return Err(RuntimeError::new(
                    "defer-control-flow",
                    "deferred cleanup produced invalid control flow",
                )
                .with_span(span));
            }
            // A deferred `Result[Unit]` that arrives as a value was written
            // without `?`. It fails the action exactly as `defer f()?` does,
            // so it records the same propagation and traceback.
            (ControlFlow::Continue(value @ LoweredValue::ResultErr(_)), None) => {
                self.lowered_question_propagation_value(value, span)?
            }
            (ControlFlow::Continue(value) | ControlFlow::Break(value), None) => value,
        };
        match value {
            LoweredValue::ResultErr(error) => {
                let mut error = runtime_error_from_value(*error, span);
                error.propagated = true;
                Err(error)
            }
            LoweredValue::ResultOk(_) | LoweredValue::Unit | LoweredValue::Status(_) => Ok(()),
            _ => Err(
                RuntimeError::new("defer-type", "deferred cleanup must produce Unit")
                    .with_span(span),
            ),
        }
    }

    /// Runs a scope's deferred actions, last registered first.
    ///
    /// `leaves_with_error` says whether the scope is leaving with an error,
    /// which is when its `errdefer` actions run. A failing action makes that
    /// true for the actions registered before it: from there on the scope
    /// leaves with that failure.
    pub(in crate::runtime::eval::lowered_run) fn run_indexed_defers(
        &mut self,
        execution: &FullExecution<'_>,
        defers: &[RegisteredDefer],
        mut leaves_with_error: bool,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<(), RuntimeError> {
        let primary_traceback = self.pending_traceback.take();
        let mut first_error = None;
        let mut first_traceback = None;
        for deferred in defers.iter().rev().copied() {
            if deferred.on_error() && !leaves_with_error {
                continue;
            }
            if let Err(error) =
                self.eval_indexed_deferred_expr(execution, deferred.value(), slots, call_span)
            {
                leaves_with_error = true;
                if error.abort.as_ref().is_some_and(|signal| signal.force) {
                    self.pending_traceback = primary_traceback;
                    return Err(error);
                }
                if first_error.is_none() {
                    first_error = Some(error);
                    first_traceback = self.pending_traceback.take();
                } else {
                    self.report_cleanup_error(&error, call_span);
                    self.pending_traceback = None;
                }
            }
        }
        self.pending_traceback = primary_traceback.or(first_traceback);
        first_error.map_or(Ok(()), |mut error| {
            // The caller reports this failure and drops it when the scope
            // already leaves with an error; otherwise it is the scope's own.
            error.scope_cleanup = error.abort.is_none() && error.propagated;
            Err(error)
        })
    }

    pub(super) fn eval_indexed_error_boundary_block(
        &mut self,
        execution: &FullExecution<'_>,
        block: u32,
        slots: &mut [LoweredValue],
        span: Span,
    ) -> Result<StmtFlow, RuntimeError> {
        match self.eval_indexed_statement_block(execution, block, slots, span) {
            Err(error) => capture_checked_error(error).map(StmtFlow::Propagate),
            result => result,
        }
    }
}
