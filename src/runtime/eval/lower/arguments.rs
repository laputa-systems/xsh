use super::{
    ArenaCallArg, ArenaCallArgKind, ArenaExprKind, BuildExprId, BuildExprRow, BuildPatternRow,
    CallableParamType, CompactLowerConstructProbe, DurationValue, ExprId, FxHashMap,
    LoweredArgumentValues, LoweredCallArg, LoweredFunctionKey, LoweredRecordEntry, Name,
    PathValue, Rc, SlotScope, Span, SpreadPrograms, Type, api_spec,
    compact_checked_type_is_concrete, compact_error_family_info, lower_literal_constant,
};

#[cfg(test)]
use super::SPREAD_PROGRAM_COPIES;

impl<'p> CompactLowerConstructProbe<'p, '_> {
    pub(super) fn lower_checked_call_values(
        &mut self,
        args: &[ExprId],
        types: &[Type],
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<(Vec<BuildExprId>, Vec<(BuildExprId, usize)>)> {
        if !types.iter().any(Type::has_unsigned_constraint) {
            return Some((
                self.lower_expr_ids(args, slots, current_function, item_slot)?,
                Vec::new(),
            ));
        }
        let mut fields = Vec::with_capacity(args.len());
        let mut bindings = Vec::with_capacity(args.len());
        // Evaluate authored operands before this call's parameter validation.
        for (arg, ty) in args.iter().zip(types) {
            let value = self.lower_expr(*arg, slots, current_function, item_slot)?;
            let slot = slots.reserve("checked call argument");
            bindings.push((value, slot));
            let value = push_build_row!(self, expr, BuildExprRow::Param(slot));
            fields.push(self.checked_unsigned_value(value, ty, self.program.arena.expr(*arg).span));
        }
        Some((fields, bindings))
    }

    pub(super) fn lower_expr_ids(
        &mut self,
        ids: &[ExprId],
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<Vec<BuildExprId>> {
        let mut lowered = Vec::with_capacity(ids.len());
        for id in ids {
            lowered.push(self.lower_expr(*id, slots, current_function, item_slot)?);
        }
        Some(lowered)
    }

    pub(super) fn lower_call_args(
        &mut self,
        args: &[ArenaCallArg],
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<Vec<LoweredCallArg>> {
        let mut lowered = Vec::with_capacity(args.len());
        for arg in args {
            match arg.kind {
                ArenaCallArgKind::Positional(expr) => {
                    lowered.push(LoweredCallArg::Single(self.lower_expr(
                        expr,
                        slots,
                        current_function,
                        item_slot,
                    )?));
                }
                ArenaCallArgKind::Splice { value, .. } => {
                    lowered.push(LoweredCallArg::Splice(self.lower_expr(
                        value,
                        slots,
                        current_function,
                        item_slot,
                    )?));
                }
                ArenaCallArgKind::Named { .. } | ArenaCallArgKind::NamedSpread { .. } => {
                    return None;
                }
            }
        }
        Some(lowered)
    }

    /// Save each source entry once and project a spread's visible fields before
    /// beginning the next entry. Slots preserve this order when a callable's
    /// parameter order differs from its written argument order.
    pub(super) fn lower_expanded_argument_values(
        &mut self,
        expanded: &[crate::sema::arguments::ExpandedArgument],
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<LoweredArgumentValues> {
        use crate::sema::arguments::ArgumentValueSource;
        let mut bindings = Vec::new();
        let mut values = Vec::new();
        let mut record_entry = None;
        for arg in expanded {
            let value = match arg.value {
                ArgumentValueSource::Expression(expr)
                | ArgumentValueSource::PositionalSplice(expr) => {
                    record_entry = None;
                    self.lower_expr(expr, slots, current_function, item_slot)?
                }
                ArgumentValueSource::RecordField { record, field } => {
                    let base = match record_entry {
                        Some((entry, value)) if entry == arg.entry_index => value,
                        _ => {
                            let value =
                                self.lower_expr(record, slots, current_function, item_slot)?;
                            let slot = slots.reserve("named spread record");
                            bindings.push((value, slot));
                            let value = push_build_row!(self, expr, BuildExprRow::Param(slot));
                            record_entry = Some((arg.entry_index, value));
                            value
                        }
                    };
                    // The record is bound where the spread is written, which
                    // fixes when it is evaluated. Reading a field of the
                    // bound record has no effect and cannot fail, so the
                    // field is read where it is passed: a binding of its own
                    // would nest one level per field, and every pass over
                    // the lowered program recurses through that nesting.
                    values.push(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::Field {
                            base,
                            name: field.as_str(),
                            span: arg.span
                        }
                    ));
                    continue;
                }
            };
            let slot = slots.reserve("call argument");
            bindings.push((value, slot));
            values.push(push_build_row!(self, expr, BuildExprRow::Param(slot)));
        }
        Some(LoweredArgumentValues { values, bindings })
    }

    /// Sequence slot initialization around an ordinary value expression.
    pub(super) fn wrap_argument_bindings(
        &mut self,
        mut value: BuildExprId,
        bindings: Vec<(BuildExprId, usize)>,
        span: Span,
    ) -> BuildExprId {
        for (subject, slot) in bindings.into_iter().rev() {
            let pattern = push_build_row!(self, pattern, BuildPatternRow::Bind { slot });
            value = push_build_row!(
                self,
                expr,
                BuildExprRow::MatchExpr {
                    value: subject,
                    arms: vec![(pattern, None, value)],
                    span,
                }
            );
        }
        value
    }

    pub(super) fn lower_named_spread_call(
        &mut self,
        id: ExprId,
        callee: ExprId,
        args: crate::syntax::arena::ArenaRange,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        use crate::sema::arguments::{ArgumentValueSource, expand_named_arguments};
        use crate::syntax::arena::ArenaCallArgInput;
        let expanded =
            expand_named_arguments(self.program, self.program.arena.call_args(args), |expr| {
                self.bodies
                    .expr_types
                    .get(&expr)
                    .filter(|ty| compact_checked_type_is_concrete(ty))
                    .cloned()
                    .or_else(|| self.checked_expr_type(expr))
            })
            .ok()?;
        let mut bindings = Vec::new();
        let mut overrides = Vec::new();
        // Namespaces and static function names have no runtime receiver. A
        // method's value receiver is evaluated before its argument entries.
        if let ArenaExprKind::Field { base, .. } = self.program.arena.expr(callee).kind {
            let namespace = self
                .resolved_compact_error_family_key(base)
                .and_then(|key| compact_error_family_info(self.declarations, key))
                .is_some()
                || matches!(self.checked_expr_type(base), Some(Type::ErrorFamily(_)))
                || matches!(self.program.arena.expr(base).kind, ArenaExprKind::Ident(module) if slots.resolve(module).is_none() && matches!(self.checked_expr_type(base), Some(Type::Module(_))))
                || matches!(self.program.arena.expr(base).kind, ArenaExprKind::Ident(module) if module == "Path" || api_spec().module(&module.as_str()).is_some() || self.declarations.error_families_by_name.contains_key(&module))
                || self
                    .declarations
                    .record_constructors
                    .resolve_call(&self.program.arena, callee, self.current_namespace)
                    .is_some();
            if !namespace {
                let receiver = self.lower_expr(base, slots, current_function, item_slot)?;
                let slot = slots.reserve("call receiver");
                bindings.push((receiver, slot));
                let bound = push_build_row!(self, expr, BuildExprRow::Param(slot));
                overrides.push((base, slots.postfix_receivers.insert(base, bound)));
            }
        }
        // A typed call's callee may be any expression; it is evaluated before
        // the argument entries, like a method's receiver.
        if self.bodies.typed_callable_calls.contains_key(&id)
            && !matches!(
                self.program.arena.expr(callee).kind,
                ArenaExprKind::Ident(_) | ArenaExprKind::Field { .. }
            )
        {
            let value = self.lower_expr(callee, slots, current_function, item_slot)?;
            let slot = slots.reserve("typed callee");
            bindings.push((value, slot));
            let bound = push_build_row!(self, expr, BuildExprRow::Param(slot));
            overrides.push((callee, slots.postfix_receivers.insert(callee, bound)));
        }
        let lowered =
            self.lower_expanded_argument_values(&expanded, slots, current_function, item_slot);
        let call_was_bound = slots.bound_call_entries.insert(id);
        let result = (|| {
            let lowered = lowered?;
            bindings.extend(lowered.bindings);
            if expanded
                .iter()
                .all(|arg| !matches!(arg.value, ArgumentValueSource::RecordField { .. }))
            {
                for (arg, bound) in expanded.iter().zip(lowered.values.iter().copied()) {
                    let (ArgumentValueSource::Expression(expr)
                    | ArgumentValueSource::PositionalSplice(expr)) = arg.value
                    else {
                        unreachable!();
                    };
                    overrides.push((expr, slots.postfix_receivers.insert(expr, bound)));
                }
                let value =
                    self.lower_call(id, callee, args, slots, current_function, item_slot)?;
                return Some(self.wrap_argument_bindings(
                    value,
                    bindings,
                    self.program.arena.expr(id).span,
                ));
            }
            // Synthetic projections exist only during static lowering. Existing
            // expression IDs and source argument ranges remain unchanged. The
            // copy they are appended to is taken out of the cell while this
            // call is lowered over it and put back afterwards.
            let taken = self.spread_programs.borrow_mut().take();
            let mut extended = taken.unwrap_or_else(|| {
                #[cfg(test)]
                SPREAD_PROGRAM_COPIES.with(|copies| copies.set(copies.get() + 1));
                Box::new(SpreadPrograms {
                    program: self.program.clone(),
                    bodies: self.bodies.clone(),
                })
            });
            let SpreadPrograms {
                program: temporary,
                bodies,
            } = &mut *extended;
            let mut inputs = Vec::new();
            for (arg, bound) in expanded.iter().zip(lowered.values) {
                let expr = match arg.value {
                    ArgumentValueSource::Expression(expr)
                    | ArgumentValueSource::PositionalSplice(expr) => expr,
                    ArgumentValueSource::RecordField { record, field } => {
                        let expr = temporary
                            .arena
                            .append_argument_projection(record, field, arg.span);
                        bodies.expr_types.insert(expr, arg.ty.clone());
                        expr
                    }
                };
                overrides.push((expr, slots.postfix_receivers.insert(expr, bound)));
                inputs.push(if let Some(name) = arg.name {
                    ArenaCallArgInput::Named {
                        name,
                        value: expr,
                        span: arg.span,
                    }
                } else if matches!(arg.value, ArgumentValueSource::PositionalSplice(_)) {
                    ArenaCallArgInput::Splice {
                        value: expr,
                        span: arg.span,
                    }
                } else {
                    ArenaCallArgInput::Positional(expr)
                });
            }
            let args = temporary.arena.append_call_arguments(&inputs);
            let (temporary, bodies) = (&*temporary, &*bodies);
            let mut child = CompactLowerConstructProbe {
                program: temporary,
                bodies,
                declarations: self.declarations,
                source: self.source,
                sources: self.sources,
                current_namespace: self.current_namespace,
                functions: self.functions,
                top_level_known: self.top_level_known.clone(),
                output: std::mem::take(&mut self.output),
                last_blocker_detail: self.last_blocker_detail.take(),
                stdlib_linkage: self.stdlib_linkage,
                function_defs: Rc::clone(&self.function_defs),
                scratch: Rc::clone(&self.scratch),
                // The child's program is the extended copy itself.
                spread_programs: Rc::default(),
            };
            let value = child.lower_call(id, callee, args, slots, current_function, item_slot);
            self.output = child.output;
            self.last_blocker_detail = child.last_blocker_detail;
            *self.spread_programs.borrow_mut() = Some(extended);
            value.map(|value| {
                self.wrap_argument_bindings(value, bindings, self.program.arena.expr(id).span)
            })
        })();
        if call_was_bound {
            slots.bound_call_entries.remove(&id);
        }
        for (expr, previous) in overrides.into_iter().rev() {
            if let Some(previous) = previous {
                slots.postfix_receivers.insert(expr, previous);
            } else {
                slots.postfix_receivers.remove(&expr);
            }
        }
        result
    }

    /// The checker's binding of a user callable call's source entries to
    /// parameter slots.
    pub(super) fn call_argument_slots(&self, call: ExprId) -> Option<&'p [usize]> {
        self.bodies
            .argument_bindings
            .get(&self.program.arena.expr(call).span)
            .map(|binding| binding.argument_slots.as_slice())
    }

    /// Arrange named entries in their checked slots. Named calls evaluate their
    /// entries in source order before reaching here, so slot order is free.
    pub(super) fn lower_function_call_args(
        &mut self,
        args: &[ArenaCallArg],
        params: Option<&[CallableParamType]>,
        argument_slots: Option<&[usize]>,
        definition: Option<(LoweredFunctionKey, usize)>,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<Vec<LoweredCallArg>> {
        if !args
            .iter()
            .any(|arg| matches!(arg.kind, ArenaCallArgKind::Named { .. }))
        {
            return self.lower_call_args(args, slots, current_function, item_slot);
        }
        let (params, argument_slots) = (params?, argument_slots?);
        let mut values = Vec::new();
        #[allow(clippy::needless_range_loop)]
        for slot in 0..=argument_slots.iter().copied().max()? {
            let mut entries = args
                .iter()
                .zip(argument_slots)
                .filter(|(_, bound)| **bound == slot)
                .peekable();
            if entries.peek().is_none() {
                if params[slot].rest {
                    continue;
                }
                let definitions = self.function_index();
                let parameter = definition.and_then(|(key, skip)| {
                    definitions.definition(key).and_then(|def| {
                        self.program
                            .arena
                            .params(self.program.arena.function_def(def.id).params)
                            .get(slot + skip)
                    })
                });
                let Some(default) = parameter.and_then(|parameter| parameter.default) else {
                    values.push(LoweredCallArg::Default(
                        slot + definition.map_or(0, |(_, skip)| skip),
                    ));
                    continue;
                };
                let value = if let Some(constant) = crate::sema::constants::LiteralConstant::analyze(
                    &self.program.arena,
                    default,
                    &FxHashMap::default(),
                ) {
                    self.lower_record_default(&constant.in_type(&params[slot].ty))?
                } else {
                    self.lower_expr(default, slots, current_function, item_slot)?
                };
                values.push(LoweredCallArg::Single(value));
            }
            for (arg, _) in entries {
                values.push(match arg.kind {
                    ArenaCallArgKind::Positional(value) | ArenaCallArgKind::Named { value, .. } => {
                        LoweredCallArg::Single(self.lower_expr(
                            value,
                            slots,
                            current_function,
                            item_slot,
                        )?)
                    }
                    ArenaCallArgKind::Splice { value, .. } => LoweredCallArg::Splice(
                        self.lower_expr(value, slots, current_function, item_slot)?,
                    ),
                    ArenaCallArgKind::NamedSpread { .. } => return None,
                });
            }
        }
        Some(values)
    }

    pub(super) fn lower_record_default(
        &mut self,
        value: &crate::sema::constants::LiteralConstant,
    ) -> Option<BuildExprId> {
        use crate::sema::constants::LiteralConstant as C;
        let row = match value {
            C::Regex(_) | C::Tag { .. } => {
                BuildExprRow::PreparedConstant(super::super::PreparedConstantValue(
                    lower_literal_constant(value, Some(&self.declarations.wire_enums))?,
                ))
            }
            C::Null => BuildExprRow::Null,
            C::Bool(value) => BuildExprRow::Bool(*value),
            C::Int(value) => BuildExprRow::Int(*value),
            C::Float(value) => BuildExprRow::Float(crate::runtime::value::FloatValue::new(
                f64::from_bits(*value),
            )),
            C::Duration(millis) => BuildExprRow::Duration(DurationValue { millis: *millis }),
            C::Str(value) => BuildExprRow::Str(value.clone()),
            C::Bytes(value) => BuildExprRow::Bytes(value.clone()),
            C::Path(value) => BuildExprRow::Path(PathValue::from_text(value).ok()?),
            C::EmptyMap => BuildExprRow::EmptyMap,
            C::Map(_) | C::Set(_) => BuildExprRow::PreparedConstant(super::super::PreparedConstantValue(
                lower_literal_constant(value, Some(&self.declarations.wire_enums))?,
            )),
            C::List(values) => BuildExprRow::List(
                values
                    .iter()
                    .map(|value| self.lower_record_default(value))
                    .collect::<Option<Vec<_>>>()?,
            ),
            C::Record(values) => BuildExprRow::Record(
                values
                    .iter()
                    .map(|(name, value)| {
                        Some(LoweredRecordEntry::Field(
                            *name,
                            self.lower_record_default(value)?,
                        ))
                    })
                    .collect::<Option<Vec<_>>>()?,
            ),
        };
        Some(push_build_row!(self, expr, row))
    }
}
