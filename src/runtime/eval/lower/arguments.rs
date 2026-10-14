use super::{
    ArenaCallArg, ArenaCallArgKind, ArenaExprKind, BuildExprId, BuildExprRow, BuildPatternRow,
    CallArgument, CallableParamType, CompactLowerConstructProbe, DurationValue, ExprId, FxHashMap,
    LoweredArgumentValues, LoweredCallArg, LoweredFunctionKey, LoweredRecordEntry, Name,
    PathValue, SlotScope, Span, Type, api_spec,
    compact_checked_type_is_concrete, compact_error_family_info, lower_literal_constant,
};

impl<'p> CompactLowerConstructProbe<'p, '_> {
    pub(super) fn source_call_arguments(&self, args: &[ArenaCallArg]) -> Option<Vec<CallArgument>> {
        args.iter()
            .map(|arg| {
                let (name, source, splice, span) = match arg.kind {
                    ArenaCallArgKind::Positional(expr) => (
                        None,
                        crate::sema::arguments::ArgumentValueSource::Expression(expr),
                        false,
                        self.program.arena.expr(expr).span,
                    ),
                    ArenaCallArgKind::Named { name, value, .. } => (
                        Some(name),
                        crate::sema::arguments::ArgumentValueSource::Expression(value),
                        false,
                        self.program.arena.expr(value).span,
                    ),
                    ArenaCallArgKind::Splice { value, span } => (
                        None,
                        crate::sema::arguments::ArgumentValueSource::PositionalSplice(value),
                        true,
                        self.program.arena.span(span),
                    ),
                    ArenaCallArgKind::NamedSpread { .. } => return None,
                };
                Some(CallArgument {
                    name,
                    splice,
                    source,
                    ty: Type::Unknown,
                    span,
                    lowered: None,
                })
            })
            .collect()
    }

    pub(super) fn lower_call_argument(
        &mut self,
        argument: &CallArgument,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        if let Some(value) = argument.lowered {
            return Some(value);
        }
        match argument.source {
            crate::sema::arguments::ArgumentValueSource::Expression(expr)
            | crate::sema::arguments::ArgumentValueSource::PositionalSplice(expr) => {
                self.lower_expr(expr, slots, current_function, item_slot)
            }
            crate::sema::arguments::ArgumentValueSource::RecordField { .. }
            | crate::sema::arguments::ArgumentValueSource::CheckedType => None,
        }
    }

    pub(super) fn lower_optional_call_argument(
        &mut self,
        argument: Option<&CallArgument>,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<Option<BuildExprId>> {
        match argument {
            Some(argument) => self
                .lower_call_argument(argument, slots, current_function, item_slot)
                .map(Some),
            None => Some(None),
        }
    }

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

    pub(super) fn lower_checked_call_arguments(
        &mut self,
        args: &[(CallArgument, Type)],
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<(Vec<BuildExprId>, Vec<(BuildExprId, usize)>)> {
        if !args.iter().any(|(_, ty)| ty.has_unsigned_constraint()) {
            return Some((
                args.iter()
                    .map(|(arg, _)| {
                        self.lower_call_argument(arg, slots, current_function, item_slot)
                    })
                    .collect::<Option<Vec<_>>>()?,
                Vec::new(),
            ));
        }
        let mut fields = Vec::with_capacity(args.len());
        let mut bindings = Vec::new();
        for (arg, ty) in args {
            let value = self.lower_call_argument(arg, slots, current_function, item_slot)?;
            let slot = slots.reserve("checked call argument");
            bindings.push((value, slot));
            let mut value = push_build_row!(self, expr, BuildExprRow::Param(slot));
            if ty.has_unsigned_constraint() {
                value = self.checked_unsigned_value(value, ty, arg.span);
            }
            fields.push(value);
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
        args: &[CallArgument],
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<Vec<LoweredCallArg>> {
        let mut lowered = Vec::with_capacity(args.len());
        for arg in args {
            if arg.name.is_some() {
                return None;
            }
            let value = self.lower_call_argument(arg, slots, current_function, item_slot)?;
            lowered.push(if arg.splice {
                LoweredCallArg::Splice(value)
            } else {
                LoweredCallArg::Single(value)
            });
        }
        Some(lowered)
    }

    fn record_prepared_callable_type(
        &mut self,
        value: BuildExprId,
        ty: Type,
        slots: &mut SlotScope,
        slot: Option<usize>,
        span: Span,
    ) -> Option<()> {
        let ty = self.canonicalize_checked_type(ty);
        if !ty.contains_typed_callable() {
            return Some(());
        }
        if let Some(slot) = slot
            && !slots.record_checked_slot_type(slot, ty.clone())
        {
            self.last_blocker_detail = Some((
                span,
                "prepared argument conflicts with its checked callable slot type".to_string(),
            ));
            return None;
        }
        if !self
            .scratch
            .borrow_mut()
            .record_checked_expr_type(value, ty, false)
        {
            self.last_blocker_detail = Some((
                span,
                "prepared argument row conflicts with its checked callable type".to_string(),
            ));
            return None;
        }
        Some(())
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
        let mut arguments = Vec::with_capacity(expanded.len());
        let mut record_entry = None;
        for arg in expanded {
            let span = match arg.value {
                ArgumentValueSource::Expression(expr) => self.program.arena.expr(expr).span,
                _ => arg.span,
            };
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
                            let record_ty = self
                                .bodies
                                .expr_types
                                .get(&record)
                                .filter(|ty| compact_checked_type_is_concrete(ty))
                                .cloned()
                                .or_else(|| self.checked_expr_type(record))?;
                            let slot = slots.reserve("named spread record");
                            bindings.push((value, slot));
                            let value = push_build_row!(self, expr, BuildExprRow::Param(slot));
                            self.record_prepared_callable_type(
                                value,
                                record_ty,
                                slots,
                                Some(slot),
                                self.program.arena.expr(record).span,
                            )?;
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
                    let value = push_build_row!(
                        self,
                        expr,
                        BuildExprRow::Field {
                            base,
                            name: field.as_str(),
                            span: arg.span
                        }
                    );
                    self.record_prepared_callable_type(
                        value,
                        arg.ty.clone(),
                        slots,
                        None,
                        arg.span,
                    )?;
                    arguments.push(CallArgument {
                        name: arg.name,
                        splice: false,
                        source: arg.value,
                        ty: arg.ty.clone(),
                        span,
                        lowered: Some(value),
                    });
                    continue;
                }
                ArgumentValueSource::CheckedType => {
                    unreachable!("checked types are not lowered as source arguments")
                }
            };
            let slot = slots.reserve("call argument");
            bindings.push((value, slot));
            let value = push_build_row!(self, expr, BuildExprRow::Param(slot));
            self.record_prepared_callable_type(
                value,
                arg.ty.clone(),
                slots,
                Some(slot),
                arg.span,
            )?;
            arguments.push(CallArgument {
                name: arg.name,
                splice: matches!(arg.value, ArgumentValueSource::PositionalSplice(_)),
                source: arg.value,
                ty: arg.ty.clone(),
                span,
                lowered: Some(value),
            });
        }
        Some(LoweredArgumentValues { arguments, bindings })
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
        use crate::sema::arguments::expand_named_arguments;
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
        if let ArenaExprKind::Field { base, .. } = self.program.arena.expr(callee).kind {
            let namespace = self
                .resolved_compact_error_family_key(base)
                .and_then(|key| compact_error_family_info(self.declarations, key))
                .is_some()
                || matches!(self.checked_expr_type(base), Some(Type::ErrorFamily(_)))
                || matches!(self.program.arena.expr(base).kind, ArenaExprKind::Ident(_) if self.source_slot(base, slots).is_none() && matches!(self.checked_expr_type(base), Some(Type::Module(_))))
                || matches!(self.program.arena.expr(base).kind, ArenaExprKind::Ident(module) if self.source_slot(base, slots).is_none() && (module == "Path" || api_spec().module(&module.as_str()).is_some() || self.declarations.error_families_by_name.contains_key(&module)))
                || self
                    .bodies
                    .resolved_type_expressions
                    .instantiations
                    .contains_key(&id);
            if !namespace {
                let receiver = self.lower_expr(base, slots, current_function, item_slot)?;
                let slot = slots.reserve("call receiver");
                if let Some(ty) = self.concrete_checked_type(base)
                    && !slots.record_checked_slot_type(slot, ty)
                {
                    return None;
                }
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
            let Some(ty @ Type::Callable(_)) = self.bodies.expr_types.get(&callee).cloned() else {
                return None;
            };
            let value = self.lower_expr(callee, slots, current_function, item_slot)?;
            let slot = slots.reserve("typed callee");
            if !slots.record_checked_slot_type(slot, ty) {
                return None;
            }
            bindings.push((value, slot));
            let bound = push_build_row!(self, expr, BuildExprRow::Param(slot));
            overrides.push((callee, slots.postfix_receivers.insert(callee, bound)));
        }
        let lowered =
            self.lower_expanded_argument_values(&expanded, slots, current_function, item_slot);
        let result = (|| {
            let lowered = lowered?;
            bindings.extend(lowered.bindings);
            let value = self.lower_call(
                id,
                callee,
                args,
                Some(&lowered.arguments),
                slots,
                current_function,
                item_slot,
            )?;
            Some(self.wrap_argument_bindings(
                value,
                bindings,
                self.program.arena.expr(id).span,
            ))
        })();
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
        args: &[CallArgument],
        params: Option<&[CallableParamType]>,
        argument_slots: Option<&[usize]>,
        definition: Option<(LoweredFunctionKey, usize)>,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<Vec<LoweredCallArg>> {
        if !args
            .iter()
            .any(|arg| arg.name.is_some())
        {
            return self.lower_call_args(args, slots, current_function, item_slot);
        }
        let (params, argument_slots) = (params?, argument_slots?);
        let mut by_slot = vec![Vec::new(); argument_slots.iter().copied().max()? + 1];
        for (arg, &slot) in args.iter().zip(argument_slots) { by_slot[slot].push(arg); }
        let mut values = Vec::new();
        for (slot, entries) in by_slot.into_iter().enumerate() {
            if entries.is_empty() {
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
            for arg in entries {
                let value = self.lower_call_argument(arg, slots, current_function, item_slot)?;
                values.push(if arg.splice {
                    LoweredCallArg::Splice(value)
                } else {
                    LoweredCallArg::Single(value)
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

#[cfg(test)]
mod named_spread_lowering_tests {
    use super::*;

    #[test]
    fn named_spread_lowering_does_not_clone_program_or_body_facts() {
        let mut source = String::new();
        for index in 0..2_048 {
            source.push_str(&format!("let unrelated_{index} = {index}\n"));
        }
        source.push_str(
            "type Options = {x: Int, y: Int, z: Int}\nlet result = Options(x: 1, ...{y: 2, z: 3})\n",
        );
        let entry_source = crate::loader::entry_source_from_text(
            "named-spread-build-rows.xsh",
            source.to_string(),
        );
        let source_id = entry_source.source_id;
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
            "named-spread-build-rows.xsh",
            entry_source,
            Vec::new(),
        );
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let declarations = crate::sema::check::Checker::check_compact_declarations(
            &parsed.arena,
            &sources,
            source_id,
        );
        assert!(
            declarations.diagnostics.is_empty(),
            "{:?}",
            declarations.diagnostics
        );

        let (call, callee, args) = (0..parsed.arena.arena.expr_tags.len())
            .map(ExprId::from_index)
            .find_map(|id| match parsed.arena.arena.expr(id).kind {
                ArenaExprKind::Call { callee, args }
                    if matches!(parsed.arena.arena.expr(callee).kind, ArenaExprKind::Ident(name) if name == "Options")
                        && parsed
                        .arena
                        .arena
                        .call_args(args)
                        .iter()
                        .any(|arg| matches!(arg.kind, ArenaCallArgKind::NamedSpread { .. })) =>
                {
                    Some((id, callee, args))
                }
                _ => None,
            })
            .expect("the fixture contains a named-spread call");
        let source_args = parsed.arena.arena.call_args(args);
        let named_span = source_args
            .iter()
            .find_map(|arg| match arg.kind {
                ArenaCallArgKind::Named { value, span, .. } => {
                    Some((parsed.arena.arena.span(span), parsed.arena.arena.expr(value).span))
                }
                _ => None,
            })
            .expect("the call contains a named value");
        assert_ne!(named_span.0, named_span.1);
        let (spread_span, spread_expr) = source_args
            .iter()
            .find_map(|arg| match arg.kind {
                ArenaCallArgKind::NamedSpread { value, span } => {
                    Some((parsed.arena.arena.span(span), value))
                }
                _ => None,
            })
            .expect("the call contains a spread");
        let expanded = crate::sema::arguments::expand_named_arguments(
            &parsed.arena,
            source_args,
            |expr| declarations.bodies.expr_types.get(&expr).cloned(),
        )
        .expect("the checker supplied a finite spread type");
        let expression_count = parsed.arena.arena.expr_tags.len();
        let arena_retained_bytes = parsed.arena.stats().retained_bytes;
        let expression_type_storage_bytes = declarations.bodies.expr_types.capacity()
            * std::mem::size_of::<(ExprId, Type)>();
        assert!(declarations.bodies.expr_types.len() >= 2_048);
        assert!(expression_type_storage_bytes > 0);
        let mut probe = CompactLowerConstructProbe {
            program: &parsed.arena,
            scope: std::rc::Rc::new(super::super::CompactRootScope::whole_program(
                &parsed.arena,
            )),
            declarations: &declarations,
            bodies: &declarations.bodies,
            source: &source,
            sources: Some(&sources),
            current_namespace: None,
            functions: None,
            top_level_known: crate::runtime::eval::lower::FxHashMap::default(),
            output: super::super::CompactLowerConstructProbeOutput::default(),
            last_blocker_detail: None,
            stdlib_linkage: super::super::StdlibLowerLinkage::Local,
            function_defs: std::rc::Rc::new(std::cell::RefCell::new(None)),
            scratch: std::rc::Rc::new(std::cell::RefCell::new(
                super::super::BuildScratch::default(),
            )),
            with_error_source_exprs: crate::runtime::eval::lower::FxHashMap::default(),
        };
        crate::mem_track::enable_tracking();
        crate::mem_track::begin_stage();
        let lowered_call = probe.lower_named_spread_call(
            call,
            callee,
            args,
            &mut SlotScope::default(),
            None,
            None,
        );
        let traffic = crate::mem_track::end_stage();
        assert!(lowered_call.is_some(), "the spread constructor lowers");
        assert!(traffic.tracking_active, "the test allocator tracks requests");
        assert!(traffic.alloc_count > 0, "the test allocator observed requests");
        // A program clone copies these unrelated arena tables, and a body-facts
        // clone copies the expression-type map. This three-field call needs only
        // a few rows, so either clone exceeds the conservative bound.
        let allocation_limit = (arena_retained_bytes / 2).min(expression_type_storage_bytes / 4);
        assert!(
            traffic.alloc_bytes < allocation_limit,
            "named-spread lowering allocated {} bytes (limit {}, arena {}, expression types {})",
            traffic.alloc_bytes,
            allocation_limit,
            arena_retained_bytes,
            expression_type_storage_bytes,
        );
        let lowered = probe
            .lower_expanded_argument_values(&expanded, &mut SlotScope::default(), None, None)
            .expect("expanded entries lower");

        assert_eq!(parsed.arena.arena.expr_tags.len(), expression_count);
        assert_eq!(lowered.arguments.len(), 3);
        assert_eq!(lowered.bindings.len(), 2);
        assert_eq!(lowered.arguments[0].span, named_span.1);
        let spread_arguments = lowered.arguments.iter().skip(1).collect::<Vec<_>>();
        assert!(spread_arguments.iter().all(|argument| argument.span == spread_span));
        assert!(spread_arguments.iter().all(|argument| matches!(
            argument.source,
            crate::sema::arguments::ArgumentValueSource::RecordField { record, .. }
                if record == spread_expr
        )));
        let scratch = probe.scratch.borrow();
        let field_bases = spread_arguments
            .iter()
            .map(|argument| {
                let id = argument.lowered.expect("spread projection is pre-lowered");
                let BuildExprRow::Field { base, .. } = &scratch.expressions[id.index()] else {
                    panic!("spread projection is a lowered field row")
                };
                *base
            })
            .collect::<Vec<_>>();
        assert_eq!(field_bases[0], field_bases[1]);
        assert!(matches!(
            &scratch.expressions[field_bases[0].index()],
            BuildExprRow::Param { .. }
        ));
    }
}
