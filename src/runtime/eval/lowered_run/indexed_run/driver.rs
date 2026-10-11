use super::{
    Arc, AssignOp, Binding, ControlFlow, Evaluator, Flow, FullDriverTag, LoweredModuleExportKind,
    LoweredTopLevelSlot, LoweredType, LoweredTypeCheck, LoweredValue, QualifiedName, RecordMap,
    RuntimeError, Span, StmtFlow, Type, Value,
    bind_lowered_comp_target, compound_assignment_value, indexed_error,
    lowered_stmt_flow_to_flow, lowered_type_name,
    lowered_value_from_runtime, lowered_value_from_runtime_any, runtime_error_from_value,
    ValueView, value_matches_static_type,
};
use crate::runtime::eval::indexed::full::schema::DriverPayloadView;

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
        Some(self.eval_indexed_driver_step_inner(view, call_span))
    }

    pub(super) fn eval_indexed_driver_step_inner(
        &mut self,
        view: crate::runtime::eval::indexed::full::FullDriverStepView<'_>,
        call_span: Span,
    ) -> Result<Option<Flow>, RuntimeError> {
        let execution = view
            .execution()
            .map_err(|error| indexed_error(error, call_span))?;
        let payload = view
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
            let row = view
                .decode_payload(&execution, payload)
                .map_err(|error| indexed_error(error, call_span))?;
            let flow = match row {
                DriverPayloadView::Skip {} => Flow::Continue(Value::Unit),
                DriverPayloadView::Use {
                    key, alias, path, namespace, exports, child_program, span,
                } => {
                    if path.len() == 0 {
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
                        .driver_program_step_views(child_program)
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
                        let export = export
                            .map_err(|error| indexed_error(error, call_span))?
                            .materialize();
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
                DriverPayloadView::Let {
                    target, ty, validation, mutable, value, value_span,
                } => {
                    let ty = ty
                        .map(|ty| ty.materialize_lowered())
                        .transpose()
                        .map_err(|error| indexed_error(error, call_span))?;
                    let validation = validation
                        .map(|check| {
                            Ok(LoweredTypeCheck {
                                ty: check.ty.materialize()?,
                                name: Arc::from(check.name),
                                schema: check.schema.map(Arc::clone),
                            })
                        })
                        .transpose()
                        .map_err(|error| indexed_error(error, call_span))?;
                    let mut value =
                        match self.eval_indexed_expr(&execution, value.raw(), &mut slots, call_span)? {
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
                        if !value_matches_static_type(ValueView::Runtime(&value), &check.ty) {
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
                DriverPayloadView::Assign { target, op, value, span } => {
                    let value =
                        match self.eval_indexed_expr(&execution, value.raw(), &mut slots, call_span)? {
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
                DriverPayloadView::LetRecord {
                    source, mut fields, target, mutable, span,
                } => {
                    let target = target
                        .materialize()
                        .map_err(|error| indexed_error(error, call_span))?;
                    let source =
                        match self.eval_indexed_expr(&execution, source.raw(), &mut slots, call_span)? {
                            ControlFlow::Continue(value) => value,
                            ControlFlow::Break(value) => {
                                return Ok(Some(
                                    self.indexed_driver_expression_escape(value, call_span),
                                ));
                            }
                        };
                    bind_lowered_comp_target(&target, source, &mut slots, span)?;
                    for field in fields.by_ref() {
                        let (name, slot) = field.map_err(|error| indexed_error(error, call_span))?;
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
                DriverPayloadView::Discard { value, span } => {
                    match self.eval_indexed_expr(&execution, value.raw(), &mut slots, span)? {
                        ControlFlow::Continue(_) => Flow::Continue(Value::Unit),
                        ControlFlow::Break(value) => {
                            return Ok(Some(self.indexed_driver_expression_escape(value, span)));
                        }
                    }
                }
                DriverPayloadView::Stmt { statement } => {
                    let flow = self.eval_indexed_top_level_statement(
                        &execution, statement.raw(), &mut slots, call_span,
                    )?;
                    match flow {
                        StmtFlow::Propagate(value) => {
                            self.indexed_driver_expression_escape(value, call_span)
                        }
                        flow => lowered_stmt_flow_to_flow(flow),
                    }
                }
                DriverPayloadView::Expr { value } => {
                    let value =
                        match self.eval_indexed_expr(&execution, value.raw(), &mut slots, call_span)? {
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
                DriverPayloadView::Defer { value, on_error: _, span } => {
                    // Whether the action runs at all is the driver's decision,
                    // made before it reaches this step.
                    self.eval_indexed_deferred_expr(&execution, value.raw(), &mut slots, span)?;
                    Flow::Continue(Value::Unit)
                }
                DriverPayloadView::SignalHook {
                    signal, pre_cancel, body, slots: mut hook_slots, slot_count, span,
                } => {
                    let mut materialized_slots: Vec<LoweredTopLevelSlot> =
                        Vec::with_capacity(hook_slots.len());
                    for slot in hook_slots.by_ref() {
                        materialized_slots.push(
                            slot.map_err(|error| indexed_error(error, call_span))?
                                .materialize()
                                .map_err(|error| indexed_error(error, call_span))?,
                        );
                    }
                    let program = Arc::clone(
                        self.indexed_program
                            .as_ref()
                            .expect("indexed driver retains its program"),
                    );
                    self.register_indexed_signal_hook(
                        &signal.as_str(),
                        pre_cancel,
                        program,
                        view.index(),
                        body.block_id(),
                        materialized_slots,
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
