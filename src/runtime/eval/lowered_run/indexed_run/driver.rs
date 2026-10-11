use super::{
    Arc, AssignOp, Binding, ControlFlow, Evaluator, Flow, FullDriverTag, LoweredCompTarget,
    LoweredModuleExport, LoweredModuleExportKind, LoweredTopLevelSlot, LoweredType, LoweredValue,
    Name, QualifiedName, RecordMap, RuntimeError, Span, StmtFlow, Type, Value,
    bind_lowered_comp_target, compound_assignment_value, indexed_decode, indexed_error,
    indexed_finish, indexed_raw, lowered_stmt_flow_to_flow, lowered_type_name,
    lowered_value_from_runtime, lowered_value_from_runtime_any, runtime_error_from_value,
    value_matches_static_type,
};

impl Evaluator {
    // Expression transport retains the statement target separately from its payload.
    // In particular, callback returns and loop controls must cross retry and cleanup.
    pub(super) fn indexed_driver_expression_escape(&mut self, value: LoweredValue, span: Span) -> Flow {
        match self
            .pending_value_block_flow
            .take()
            .unwrap_or(StmtFlow::Propagate(value))
        {
            StmtFlow::Propagate(value) => self.question_flow(value.into_value(), span),
            flow => lowered_stmt_flow_to_flow(flow),
        }
    }

    pub(in crate::runtime::eval) fn eval_indexed_driver_step(
        &mut self,
        index: usize,
        call_span: Span,
    ) -> Option<Result<Option<Flow>, RuntimeError>> {
        let program = Arc::clone(self.indexed_program.as_ref()?);
        let _symbols = program.symbol_owner().enter();
        let view = match program.driver_step_view(index) {
            Ok(view) => view,
            Err(error) => return Some(Err(indexed_error(error, call_span))),
        };
        if !matches!(
            view.tag(),
            FullDriverTag::Skip
                | FullDriverTag::Use
                | FullDriverTag::Let
                | FullDriverTag::LetRecord
                | FullDriverTag::Assign
                | FullDriverTag::Discard
                | FullDriverTag::Stmt
                | FullDriverTag::Expr
                | FullDriverTag::Defer
                | FullDriverTag::SignalHook
        ) {
            return None;
        }
        // A top-level statement starts only once any earlier `Err` was handled.
        self.pending_traceback = None;
        let outcome = self.eval_indexed_driver_step_inner(view, call_span);
        // A top-level statement is a boundary at which nothing can still reach a
        // producer it built and dropped.
        let swept = self.sweep_script_producers(call_span);
        Some(match (outcome, swept) {
            (Ok(flow), Ok(())) => Ok(flow),
            (Err(error), _) | (Ok(_), Err(error)) => Err(error),
        })
    }

    pub(super) fn eval_indexed_driver_step_inner(
        &mut self,
        view: crate::runtime::eval::indexed::full::FullDriverStepView<'_>,
        call_span: Span,
    ) -> Result<Option<Flow>, RuntimeError> {
        let execution = view
            .execution()
            .map_err(|error| indexed_error(error, call_span))?;
        let mut payload = view
            .payload()
            .map_err(|error| indexed_error(error, call_span))?;
        let top_level_slots = view
            .slots()
            .map_err(|error| indexed_error(error, call_span))?;
        let mut slots = vec![LoweredValue::Unit; view.slot_count()];
        for slot in &top_level_slots {
            let Some(binding) = self.lookup(slot.name) else {
                return Ok(None);
            };
            let Some(value) = lowered_value_from_runtime(&binding.value, slot.kind)
                .or_else(|| lowered_value_from_runtime_any(&binding.value))
            else {
                return Ok(None);
            };
            slots[slot.slot] = Self::share_indexed_root_value(value);
        }
        let previous_root_slots = self
            .indexed_root_slots
            .replace(super::super::super::IndexedRootSlots {
                address: slots.as_ptr() as usize,
                scope_revision: self.scope_write_revision,
                bindings: top_level_slots
                    .iter()
                    .filter(|slot| slot.mutable)
                    .map(|slot| (slot.clone(), slots[slot.slot].clone()))
                    .collect(),
            });
        let result = (|| {
            let flow = match view.tag() {
                FullDriverTag::Skip => {
                    indexed_finish(payload, call_span)?;
                    Flow::Continue(Value::Unit)
                }
                FullDriverTag::Use => {
                    let key = indexed_decode::<Arc<str>>(&mut payload, &execution, call_span)?;
                    let alias =
                        indexed_decode::<Option<Name>>(&mut payload, &execution, call_span)?;
                    let path = indexed_decode::<Vec<Name>>(&mut payload, &execution, call_span)?;
                    let namespace = indexed_decode::<Name>(&mut payload, &execution, call_span)?;
                    let exports = indexed_decode::<Vec<LoweredModuleExport>>(
                        &mut payload,
                        &execution,
                        call_span,
                    )?;
                    let child = indexed_raw(&mut payload, call_span)?;
                    let span = indexed_decode::<Span>(&mut payload, &execution, call_span)?;
                    indexed_finish(payload, call_span)?;
                    if path.is_empty() {
                        return Err(RuntimeError::new("unknown-module", "empty module path")
                            .with_span(span));
                    }
                    let import_name = alias.unwrap_or(namespace);
                    let program = Arc::clone(
                        self.indexed_program
                            .as_ref()
                            .expect("indexed driver retains its program"),
                    );
                    let child_steps = program
                        .driver_program_step_views(child)
                        .map_err(|error| indexed_error(error, span))?;
                    for child_view in child_steps {
                        if child_view.tag() == FullDriverTag::Defer {
                            continue;
                        }
                        let child_span = child_view
                            .source_span()
                            .map_err(|error| indexed_error(error, span))?;
                        let mut modules_before = Vec::new();
                        if let Some(scope) = self.scopes.last() {
                            for (&name, binding) in scope {
                                if let Value::Module(record) = &binding.value {
                                    modules_before.push((name, record.clone()));
                                }
                            }
                        }
                        match self.eval_indexed_driver_step_inner(child_view, child_span)? {
                            Some(Flow::Continue(_)) | None => {}
                            Some(Flow::Propagate(propagation)) => {
                                return Err(runtime_error_from_value(
                                    propagation.error,
                                    child_span,
                                ));
                            }
                            Some(_) => {
                                return Err(RuntimeError::new(
                                    "module-load",
                                    format!("invalid control flow while importing {key}"),
                                )
                                .with_span(child_span));
                            }
                        }
                        for (name, record) in &modules_before {
                            if let Some(binding) = self.lookup(*name)
                                && !matches!(&binding.value, Value::Module(_))
                            {
                                self.define(
                                    *name,
                                    Binding {
                                        value: Value::Module(record.clone()),
                                        mutable: false,
                                    },
                                );
                            }
                        }
                    }
                    let mut modules_protected = Vec::new();
                    if let Some(scope) = self.scopes.last() {
                        for (&name, binding) in scope {
                            if let Value::Module(record) = &binding.value {
                                modules_protected.push((name, record.clone()));
                            }
                        }
                    }
                    let mut record_fields = Vec::with_capacity(exports.len());
                    for export in exports {
                        let value = match export.kind {
                            LoweredModuleExportKind::Value => self
                                .lookup(export.name)
                                .map(|binding| binding.value.clone())
                                .ok_or_else(|| {
                                    RuntimeError::new(
                                        "missing-field",
                                        format!(
                                            "module export `{}` was not materialized",
                                            export.name
                                        ),
                                    )
                                    .with_span(span)
                                })?,
                            LoweredModuleExportKind::Pure => {
                                let owner = export.function_namespace.unwrap_or(namespace);
                                Value::Pure(QualifiedName::new(owner, export.name).into())
                            }
                            LoweredModuleExportKind::Proc => {
                                let owner = export.function_namespace.unwrap_or(namespace);
                                Value::Proc(QualifiedName::new(owner, export.name).into())
                            }
                        };
                        record_fields.push((export.name, value));
                    }
                    for (name, module_record) in modules_protected {
                        if let Some(binding) = self.lookup(name)
                            && !matches!(&binding.value, Value::Module(_))
                        {
                            self.define(
                                name,
                                Binding {
                                    value: Value::Module(module_record),
                                    mutable: false,
                                },
                            );
                        }
                    }
                    self.define(
                        import_name,
                        Binding {
                            value: Value::Module(RecordMap::from_name_values(record_fields)),
                            mutable: false,
                        },
                    );
                    Flow::Continue(Value::Unit)
                }
                FullDriverTag::Let => {
                    let target = indexed_decode::<Name>(&mut payload, &execution, call_span)?;
                    let ty =
                        indexed_decode::<Option<LoweredType>>(&mut payload, &execution, call_span)?;
                    let validation = indexed_decode::<Option<super::super::super::LoweredTypeCheck>>(
                        &mut payload,
                        &execution,
                        call_span,
                    )?;
                    let mutable = indexed_decode::<bool>(&mut payload, &execution, call_span)?;
                    let value = indexed_raw(&mut payload, call_span)?;
                    let value_span = indexed_decode::<Span>(&mut payload, &execution, call_span)?;
                    indexed_finish(payload, call_span)?;
                    let mut value =
                        match self.eval_indexed_expr(&execution, value, &mut slots, call_span)? {
                            ControlFlow::Continue(value) => value.into_value(),
                            ControlFlow::Break(value) => {
                                return Ok(Some(
                                    self.indexed_driver_expression_escape(value, call_span),
                                ));
                            }
                        };
                    if let Some(check) = &validation {
                        if matches!(&check.ty, Type::Map(_, _))
                            && let Value::Record(record) = &value
                            && record.is_empty()
                        {
                            value = Value::Map(Default::default());
                        }
                        if !value_matches_static_type(&value, &check.ty) {
                            return Err(RuntimeError::new(
                                "type-error",
                                format!("expected {}, found {}", check.name, value.type_name()),
                            )
                            .with_span(value_span));
                        }
                    } else if let Some(ty) = ty
                        && lowered_value_from_runtime(&value, ty).is_none()
                    {
                        return Err(RuntimeError::new(
                            "type-error",
                            format!("expected {}", lowered_type_name(ty)),
                        )
                        .with_span(value_span));
                    }
                    if validation.is_none()
                        && ty == Some(LoweredType::Map)
                        && let Value::Record(record) = &value
                        && record.is_empty()
                    {
                        value = Value::Map(Default::default());
                    }
                    self.define(target, Binding { value, mutable });
                    Flow::Continue(Value::Unit)
                }
                FullDriverTag::Assign => {
                    let target = indexed_decode::<Name>(&mut payload, &execution, call_span)?;
                    let op = indexed_decode::<AssignOp>(&mut payload, &execution, call_span)?;
                    let value = indexed_raw(&mut payload, call_span)?;
                    let span = indexed_decode::<Span>(&mut payload, &execution, call_span)?;
                    indexed_finish(payload, call_span)?;
                    let value =
                        match self.eval_indexed_expr(&execution, value, &mut slots, call_span)? {
                            ControlFlow::Continue(value) => value.into_value(),
                            ControlFlow::Break(value) => {
                                return Ok(Some(
                                    self.indexed_driver_expression_escape(value, call_span),
                                ));
                            }
                        };
                    let value = if op == AssignOp::Set {
                        value
                    } else {
                        let current = self
                            .lookup(target)
                            .map(|binding| binding.value.clone())
                            .ok_or_else(|| {
                                RuntimeError::new("unresolved-name", target).with_span(span)
                            })?;
                        compound_assignment_value(op, current, value, span)?
                    };
                    self.assign(&target.as_str(), value, span)?;
                    Flow::Continue(Value::Unit)
                }
                FullDriverTag::LetRecord => {
                    let source = indexed_raw(&mut payload, call_span)?;
                    let fields =
                        indexed_decode::<Vec<(Name, usize)>>(&mut payload, &execution, call_span)?;
                    let target =
                        indexed_decode::<LoweredCompTarget>(&mut payload, &execution, call_span)?;
                    let mutable = indexed_decode::<bool>(&mut payload, &execution, call_span)?;
                    let span = indexed_decode::<Span>(&mut payload, &execution, call_span)?;
                    indexed_finish(payload, call_span)?;
                    let source =
                        match self.eval_indexed_expr(&execution, source, &mut slots, call_span)? {
                            ControlFlow::Continue(value) => value,
                            ControlFlow::Break(value) => {
                                return Ok(Some(
                                    self.indexed_driver_expression_escape(value, call_span),
                                ));
                            }
                        };
                    bind_lowered_comp_target(&target, source, &mut slots, span)?;
                    for (name, slot) in fields {
                        self.define(
                            name,
                            Binding {
                                value: slots[slot].clone().into_value(),
                                mutable,
                            },
                        );
                    }
                    Flow::Continue(Value::Unit)
                }
                FullDriverTag::Discard => {
                    let value = indexed_raw(&mut payload, call_span)?;
                    let span = indexed_decode::<Span>(&mut payload, &execution, call_span)?;
                    indexed_finish(payload, call_span)?;
                    match self.eval_indexed_expr(&execution, value, &mut slots, span)? {
                        ControlFlow::Continue(_) => Flow::Continue(Value::Unit),
                        ControlFlow::Break(value) => {
                            return Ok(Some(self.indexed_driver_expression_escape(value, span)));
                        }
                    }
                }
                FullDriverTag::Stmt => {
                    let statement = indexed_raw(&mut payload, call_span)?;
                    indexed_finish(payload, call_span)?;
                    let flow = self.eval_indexed_top_level_statement(
                        &execution, statement, &mut slots, call_span,
                    )?;
                    match flow {
                        StmtFlow::Propagate(value) => {
                            self.indexed_driver_expression_escape(value, call_span)
                        }
                        flow => lowered_stmt_flow_to_flow(flow),
                    }
                }
                FullDriverTag::Expr => {
                    let value = indexed_raw(&mut payload, call_span)?;
                    indexed_finish(payload, call_span)?;
                    let value =
                        match self.eval_indexed_expr(&execution, value, &mut slots, call_span)? {
                            ControlFlow::Continue(value) => value.into_value(),
                            ControlFlow::Break(value) => {
                                return Ok(Some(
                                    self.indexed_driver_expression_escape(value, call_span),
                                ));
                            }
                        };
                    if matches!(value, Value::Result(_)) {
                        self.question_flow(value, call_span)
                    } else {
                        Flow::Continue(value)
                    }
                }
                FullDriverTag::Defer => {
                    let value = indexed_raw(&mut payload, call_span)?;
                    // Whether the action runs at all is the driver's decision,
                    // made before it reaches this step.
                    indexed_decode::<bool>(&mut payload, &execution, call_span)?;
                    let span = indexed_decode::<Span>(&mut payload, &execution, call_span)?;
                    indexed_finish(payload, call_span)?;
                    self.eval_indexed_deferred_expr(&execution, value, &mut slots, span)?;
                    Flow::Continue(Value::Unit)
                }
                FullDriverTag::SignalHook => {
                    let signal = indexed_decode::<Name>(&mut payload, &execution, call_span)?;
                    let pre_cancel =
                        indexed_decode::<Option<String>>(&mut payload, &execution, call_span)?;
                    let body = indexed_raw(&mut payload, call_span)?;
                    let hook_slots = indexed_decode::<Vec<LoweredTopLevelSlot>>(
                        &mut payload,
                        &execution,
                        call_span,
                    )?;
                    let slot_count = indexed_raw(&mut payload, call_span)? as usize;
                    let span = indexed_decode::<Span>(&mut payload, &execution, call_span)?;
                    indexed_finish(payload, call_span)?;
                    let program = Arc::clone(
                        self.indexed_program
                            .as_ref()
                            .expect("indexed driver retains its program"),
                    );
                    self.register_indexed_signal_hook(
                        &signal.as_str(),
                        pre_cancel.as_deref(),
                        program,
                        view.index(),
                        body,
                        hook_slots,
                        slot_count,
                        span,
                    )?;
                    Flow::Continue(Value::Unit)
                }
            };
            Ok(Some(flow))
        })();
        let publication = self.sync_indexed_root_slots(&mut slots, call_span);
        self.indexed_root_slots = previous_root_slots;
        match (result, publication) {
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(error),
            (Ok(flow), Ok(())) => Ok(flow),
        }
    }

    pub(super) fn share_indexed_root_value(value: LoweredValue) -> LoweredValue {
        match value {
            LoweredValue::List(values) => LoweredValue::SharedList(Arc::new(values)),
            value => value,
        }
    }

    pub(super) fn indexed_root_value_unchanged(value: &LoweredValue, previous: &LoweredValue) -> bool {
        match (value, previous) {
            (LoweredValue::SharedList(value), LoweredValue::SharedList(previous)) => {
                Arc::ptr_eq(value, previous)
            }
            (LoweredValue::Map(value), LoweredValue::Map(previous)) => Arc::ptr_eq(value, previous),
            (LoweredValue::Record(value), LoweredValue::Record(previous)) => {
                Arc::ptr_eq(value, previous)
            }
            (LoweredValue::RecordVec(value), LoweredValue::RecordVec(previous)) => {
                Arc::ptr_eq(value, previous)
            }
            _ => value == previous,
        }
    }

    /// Whether `slot` holds a script binding that is also visible through scopes.
    pub(super) fn indexed_root_binds(&self, slots: &[LoweredValue], slot: usize) -> bool {
        self.indexed_root_slots.as_ref().is_some_and(|root| {
            root.address == slots.as_ptr() as usize
                && root
                    .bindings
                    .iter()
                    .any(|(binding, _)| binding.slot == slot)
        })
    }

    pub(super) fn sync_indexed_root_slots(
        &mut self,
        slots: &mut [LoweredValue],
        span: Span,
    ) -> Result<(), RuntimeError> {
        let Some(mut root) = self.indexed_root_slots.take() else {
            return Ok(());
        };
        if root.address != slots.as_ptr() as usize {
            self.indexed_root_slots = Some(root);
            return Ok(());
        }
        let result = (|| {
            let scope_changed = root.scope_revision != self.scope_write_revision;
            for (binding, previous) in &mut root.bindings {
                let slot = &mut slots[binding.slot];
                if !Self::indexed_root_value_unchanged(slot, previous) {
                    *slot =
                        Self::share_indexed_root_value(std::mem::replace(slot, LoweredValue::Unit));
                    self.assign(&binding.name.as_str(), slot.clone().into_value(), span)?;
                } else if scope_changed
                    && let Some(value) = self
                        .lookup(binding.name)
                        .and_then(|binding| lowered_value_from_runtime_any(&binding.value))
                {
                    *slot = Self::share_indexed_root_value(value);
                }
                *previous = slot.clone();
            }
            root.scope_revision = self.scope_write_revision;
            Ok(())
        })();
        self.indexed_root_slots = Some(root);
        result
    }
}
