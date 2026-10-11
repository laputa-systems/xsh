use super::{
    Arc, ArenaAssignTargetKind, ArenaBindingTargetKind, ArenaCallArg, ArenaCallArgKind,
    ArenaExprKind, ArenaFmtPart, ArenaRecordFieldKind, AstArena, BTreeMap, BindingTargetId,
    BuildExprId, BuildExprRow, CompactErrorFamilyKey, CompactLowerConstructProbe, DurationValue,
    ExprId, FxHashMap, LoweredAssignStep, LoweredCompFields, LoweredCompTarget, LoweredErrorExpr,
    LoweredFmtPart, LoweredRecordEntry, LoweredRecordUpdates, LoweredType, LoweredValue, Name,
    PathValue, QualifiedName, RegexValue, SlotScope, Span, Type, compact_error_family_display,
    compact_error_family_info, compact_error_family_key, is_discard_name, lower_literal_constant,
    lowered_value_matches, positional_call_args, single_positional_arena_call_arg,
};

pub(super) fn lower_const_param_default(
    arena: &AstArena,
    expr: ExprId,
    kind: LoweredType,
    expected: Option<&Type>,
) -> Option<LoweredValue> {
    if let ArenaExprKind::Record(fields) = arena.expr(expr).kind
        && (matches!(expected, Some(Type::Map(_, _)))
            || arena
                .record_fields(fields)
                .iter()
                .any(|field| matches!(field.kind, ArenaRecordFieldKind::Computed { .. })))
    {
        let element = match expected {
            Some(Type::Map(_, item)) => Some(item.as_ref()),
            _ => None,
        };
        let mut values = BTreeMap::new();
        for field in arena.record_fields(fields) {
            match field.kind {
                ArenaRecordFieldKind::Computed { key, value, .. } => {
                    let key_type = match expected {
                        Some(Type::Map(key, _)) => Some(key.as_ref()),
                        _ => None,
                    };
                    let key = lower_const_param_default(arena, key, LoweredType::Any, key_type)?;
                    let key =
                        super::super::lowered_ops::lowered_map_literal_key(&key, arena.expr(expr).span)
                            .ok()?;
                    values.insert(
                        key,
                        lower_const_param_default(arena, value, LoweredType::Any, element)?,
                    );
                }
                ArenaRecordFieldKind::Named { name, value, .. } => {
                    values.insert(
                        crate::map_key::MapKey::from(name.as_str().as_str()),
                        lower_const_param_default(arena, value, LoweredType::Any, element)?,
                    );
                }
                ArenaRecordFieldKind::Spread { expr, .. } => {
                    let LoweredValue::Map(spread) =
                        lower_const_param_default(arena, expr, LoweredType::Map, expected)?
                    else {
                        return None;
                    };
                    values.extend(
                        spread
                            .iter()
                            .map(|(key, value)| (key.clone(), value.clone())),
                    );
                }
                ArenaRecordFieldKind::Shorthand { .. } | ArenaRecordFieldKind::Path { .. } => {
                    return None;
                }
            }
        }
        let value = LoweredValue::Map(Arc::new(values));
        return lowered_value_matches(kind, &value).then_some(value);
    }
    if let Some(constant) =
        crate::sema::constants::LiteralConstant::analyze(arena, expr, &FxHashMap::default())
    {
        let constant = if let Some(expected) = expected {
            constant.in_type(expected)
        } else {
            constant
        };
        let value = lower_literal_constant(&constant, None)?;
        return lowered_value_matches(kind, &value).then_some(value);
    }
    let value = match arena.expr(expr).kind {
        ArenaExprKind::Null => LoweredValue::Null,
        ArenaExprKind::Bool(value) => LoweredValue::Bool(value),
        ArenaExprKind::Int(value) => LoweredValue::Int(arena.int_literal(value).value()?),
        ArenaExprKind::Float(value) => LoweredValue::Float(crate::runtime::value::FloatValue::new(
            arena.float_literal(value).value()?,
        )),
        ArenaExprKind::Duration(value) => LoweredValue::Duration(DurationValue {
            millis: arena.duration_literal(value).millis()?,
        }),
        ArenaExprKind::Regex(value) => {
            let literal = arena.regex_literal(value);
            let regex = literal.prepared.get()?.as_ref().ok()?.clone();
            LoweredValue::Regex(Box::new(RegexValue {
                pattern: literal.pattern.to_string(),
                regex,
            }))
        }
        ArenaExprKind::Str(value) => LoweredValue::Str(arena.string_literal(value).clone()),
        ArenaExprKind::PathStr(value) => {
            LoweredValue::Path(PathValue::from_text(arena.string_literal(value).as_ref()).ok()?)
        }
        ArenaExprKind::Call { callee, args }
            if kind == LoweredType::Path
                && matches!(arena.expr(callee).kind, ArenaExprKind::Ident(name) if name == "Path") =>
        {
            let value = single_positional_arena_call_arg(arena.call_args(args))?;
            let ArenaExprKind::Str(value) = arena.expr(value).kind else {
                return None;
            };
            LoweredValue::Path(PathValue::from_text(arena.string_literal(value).as_ref()).ok()?)
        }
        ArenaExprKind::Bytes(value) => LoweredValue::Bytes(arena.bytes_literal(value).clone()),
        ArenaExprKind::List(items) => {
            let mut values = Vec::new();
            for item in arena.list_elements(items) {
                let value = lower_const_param_default(arena, item.value, LoweredType::Any, None)?;
                if item.splice_span.is_some() {
                    match value {
                        LoweredValue::List(items) => values.extend(items),
                        LoweredValue::SharedList(items) => values.extend(items.iter().cloned()),
                        _ => return None,
                    }
                } else {
                    values.push(value);
                }
            }
            LoweredValue::List(values)
        }
        ArenaExprKind::Record(fields) => {
            let mut values = BTreeMap::new();
            for field in arena.record_fields(fields) {
                match field.kind {
                    ArenaRecordFieldKind::Computed { .. } => return None,
                    ArenaRecordFieldKind::Named { name, value, .. } => {
                        values.insert(
                            Arc::<str>::from(name.as_str().as_str()),
                            lower_const_param_default(arena, value, LoweredType::Any, None)?,
                        );
                    }
                    ArenaRecordFieldKind::Spread { expr, .. } => {
                        let spread =
                            lower_const_param_default(arena, expr, LoweredType::Any, None)?;
                        match spread {
                            LoweredValue::Record(spread) => values.extend(
                                spread
                                    .iter()
                                    .map(|(key, value)| (key.clone(), value.clone())),
                            ),
                            LoweredValue::RecordVec(spread) => {
                                for (name, value) in spread.iter() {
                                    values.insert(
                                        Arc::<str>::from(name.as_str().as_str()),
                                        value.clone(),
                                    );
                                }
                            }
                            _ => return None,
                        }
                    }
                    ArenaRecordFieldKind::Shorthand { .. } | ArenaRecordFieldKind::Path { .. } => {
                        return None;
                    }
                }
            }
            LoweredValue::Record(Arc::new(values))
        }
        _ => return None,
    };
    lowered_value_matches(kind, &value).then_some(value)
}

impl<'p> CompactLowerConstructProbe<'p, '_> {
    /// A set where a loop, a comprehension, or a pipeline reads items: its
    /// elements in key order.
    pub(super) fn set_as_list(&mut self, set: BuildExprId, span: Span) -> BuildExprId {
        push_build_row!(
            self,
            expr,
            BuildExprRow::Method {
                receiver: set,
                name: Name::intern("to_list").as_str(),
                args: Vec::new(),
                span,
            }
        )
    }

    /// The set of a lowered list's elements.
    pub(super) fn list_as_set(&mut self, elements: Vec<BuildExprId>, span: Span) -> BuildExprId {
        let list = push_build_row!(self, expr, BuildExprRow::List(elements));
        self.set_of_list(list, span)
    }

    pub(super) fn set_of_list(&mut self, list: BuildExprId, span: Span) -> BuildExprId {
        push_build_row!(
            self,
            expr,
            BuildExprRow::Method {
                receiver: list,
                name: Name::intern("to_set").as_str(),
                args: Vec::new(),
                span,
            }
        )
    }

    pub(super) fn lower_direct_iterable(
        &mut self,
        iter: ExprId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let checked = self
            .checked_expr_type(iter)
            .or_else(|| self.concrete_checked_type(iter))
            .or_else(|| self.bodies.expr_types.get(&iter).cloned());
        let lowered = self.lower_expr(iter, slots, current_function, item_slot)?;
        if matches!(checked, Some(Type::Set(_))) {
            let span = self.program.arena.expr(iter).span;
            return Some(self.set_as_list(lowered, span));
        }
        if matches!(checked, Some(Type::Result(ok, _)) if matches!(ok.as_ref(), Type::Map(_, _) | Type::Str | Type::Bytes))
        {
            Some(push_build_row!(
                self,
                expr,
                BuildExprRow::Try {
                    value: lowered,
                    span: self.program.arena.expr(iter).span
                }
            ))
        } else {
            Some(lowered)
        }
    }

    // Conditions and returned Bool values keep ordinary short-circuit behavior;
    // only statement assertions ask the chain to retain failed pair values.
    pub(super) fn mark_comparison_chain_assertion(&mut self, value: BuildExprId) {
        if let BuildExprRow::ComparisonChain { assertion, .. } =
            &mut self.scratch.borrow_mut().expressions[value.index()]
        {
            *assertion = true;
        }
    }

    pub(super) fn lower_fmt_string(
        &mut self,
        parts: crate::syntax::arena::ArenaRange,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        Some(push_build_row!(
            self,
            expr,
            BuildExprRow::FmtString(self.lower_fmt_parts(
                parts,
                slots,
                current_function,
                item_slot,
            )?)
        ))
    }

    pub(super) fn lower_fmt_parts(
        &mut self,
        parts: crate::syntax::arena::ArenaRange,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<Vec<LoweredFmtPart>> {
        let parts = self.program.arena.fmt_parts(parts).collect::<Vec<_>>();
        let mut lowered = Vec::with_capacity(parts.len());
        for part in parts {
            match part {
                ArenaFmtPart::Text(text) => {
                    lowered.push(LoweredFmtPart::Text(Arc::from(self.text_value(&text)?)));
                }
                ArenaFmtPart::Expr(expr, spec) => {
                    let span = self.program.arena.expr(expr).span;
                    lowered.push(LoweredFmtPart::Expr(
                        self.lower_expr(expr, slots, current_function, item_slot)?,
                        span,
                        spec,
                    ));
                }
            }
        }
        Some(lowered)
    }

    pub(super) fn lower_map_literal(
        &mut self,
        id: ExprId,
        fields: crate::syntax::arena::ArenaRange,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let uint_key = matches!(self.bodies.expr_types.get(&id), Some(Type::Map(key, _)) if **key == Type::UInt);
        // A quoted label of a map the checker keyed by Path is a Path key.
        let path_key = matches!(self.bodies.expr_types.get(&id), Some(Type::Map(key, _)) if **key == Type::Path);
        let mut entries = Vec::new();
        for field in self.program.arena.record_fields(fields).to_vec() {
            let key_span = match &field.kind {
                ArenaRecordFieldKind::Computed { key, .. } => {
                    Some(self.program.arena.expr(*key).span)
                }
                _ => None,
            };
            let (key, value, span) = match field.kind {
                ArenaRecordFieldKind::Computed { key, value, span } => (
                    Some(self.lower_expr(key, slots, current_function, item_slot)?),
                    self.lower_expr(value, slots, current_function, item_slot)?,
                    self.program.arena.span(span),
                ),
                ArenaRecordFieldKind::Named { name, value, span } => (
                    Some(if path_key {
                        let path = PathValue::from_text(name.as_str().as_str()).ok()?;
                        push_build_row!(self, expr, BuildExprRow::Path(path))
                    } else {
                        push_build_row!(
                            self,
                            expr,
                            BuildExprRow::Str(Arc::from(name.as_str().as_str()))
                        )
                    }),
                    self.lower_expr(value, slots, current_function, item_slot)?,
                    self.program.arena.span(span),
                ),
                ArenaRecordFieldKind::Shorthand { name, span } => (
                    Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::Str(Arc::from(name.as_str().as_str()))
                    )),
                    push_build_row!(self, expr, BuildExprRow::Param(slots.resolve(name)?)),
                    self.program.arena.span(span),
                ),
                ArenaRecordFieldKind::Spread { expr, span } => (
                    None,
                    self.lower_expr(expr, slots, current_function, item_slot)?,
                    self.program.arena.span(span),
                ),
                ArenaRecordFieldKind::Path { .. } => return None,
            };
            let key = key.map(|key| {
                if uint_key {
                    self.require_uint_key(key, key_span.unwrap_or(span))
                } else {
                    key
                }
            });
            entries.push((key, value, span));
        }
        Some(push_build_row!(
            self,
            expr,
            BuildExprRow::MapLiteral(entries)
        ))
    }

    pub(super) fn lower_record(
        &mut self,
        fields: crate::syntax::arena::ArenaRange,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let fields = self.program.arena.record_fields(fields).to_vec();
        if fields
            .iter()
            .any(|field| matches!(field.kind, ArenaRecordFieldKind::Path { .. }))
        {
            let ArenaRecordFieldKind::Spread { expr, span } = fields.first()?.kind else {
                return None;
            };
            let span = self.program.arena.span(span);
            let base = self.lower_expr(expr, slots, current_function, item_slot)?;
            let mut updates = Vec::new();
            for field in fields.into_iter().skip(1) {
                let (path, value, span) = match field.kind {
                    ArenaRecordFieldKind::Path { path, value, span } => (
                        self.program.arena.names(path).collect(),
                        self.lower_expr(value, slots, current_function, item_slot)?,
                        self.program.arena.span(span),
                    ),
                    ArenaRecordFieldKind::Named { name, value, span } => (
                        vec![name],
                        self.lower_expr(value, slots, current_function, item_slot)?,
                        self.program.arena.span(span),
                    ),
                    ArenaRecordFieldKind::Shorthand { name, span } => (
                        vec![name],
                        self.lower_bare_ident(name, slots)?,
                        self.program.arena.span(span),
                    ),
                    ArenaRecordFieldKind::Spread { .. } | ArenaRecordFieldKind::Computed { .. } => {
                        return None;
                    }
                };
                updates.push((path, value, span));
            }
            return Some(push_build_row!(
                self,
                expr,
                BuildExprRow::RecordUpdate {
                    base,
                    updates: LoweredRecordUpdates(updates),
                    span
                }
            ));
        }
        let mut lowered = Vec::with_capacity(fields.len());
        for field in fields {
            match field.kind {
                ArenaRecordFieldKind::Computed { .. } => return None,
                ArenaRecordFieldKind::Path { .. } => return None,
                ArenaRecordFieldKind::Named { name, value, .. } => {
                    lowered.push(LoweredRecordEntry::Field(
                        name,
                        self.lower_expr(value, slots, current_function, item_slot)?,
                    ));
                }
                ArenaRecordFieldKind::Shorthand { name, .. } => {
                    lowered.push(LoweredRecordEntry::Field(
                        name,
                        push_build_row!(self, expr, BuildExprRow::Param(slots.resolve(name)?)),
                    ));
                }
                ArenaRecordFieldKind::Spread { expr, .. } => {
                    lowered.push(LoweredRecordEntry::Spread(self.lower_expr(
                        expr,
                        slots,
                        current_function,
                        item_slot,
                    )?));
                }
            }
        }
        Some(push_build_row!(self, expr, BuildExprRow::Record(lowered)))
    }

    pub(super) fn lower_compact_error_expr(
        &mut self,
        call: ExprId,
        callee: ExprId,
        args: &[ArenaCallArg],
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<(LoweredErrorExpr, Vec<(BuildExprId, usize)>)> {
        if let ArenaExprKind::Ident(name) = self.program.arena.expr(callee).kind
            && name == Name::ERROR
        {
            let mut kind = None;
            let mut message = None;
            for (index, arg) in args.iter().enumerate() {
                let (name, value) = match arg.kind {
                    ArenaCallArgKind::Named { name, value, .. } => (Some(name), value),
                    ArenaCallArgKind::Positional(value) => (None, value),
                    ArenaCallArgKind::Splice { .. } | ArenaCallArgKind::NamedSpread { .. } => {
                        return None;
                    }
                };
                let ArenaExprKind::Str(text) = self.program.arena.expr(value).kind else {
                    return None;
                };
                let text = self.program.arena.string_literal(text).to_string();
                match name {
                    Some(name) if name == "kind" => kind = Some(text),
                    Some(name) if name == "message" => message = Some(text),
                    None if index == 0 => kind = Some(text),
                    None if index == 1 => message = Some(text),
                    _ => {}
                }
            }
            return Some((
                LoweredErrorExpr::Simple {
                    kind: kind.unwrap_or_default(),
                    message: message.unwrap_or_default(),
                },
                Vec::new(),
            ));
        }

        let ArenaExprKind::Field {
            base,
            name: variant,
        } = self.program.arena.expr(callee).kind
        else {
            return None;
        };
        let family_key = self.resolved_compact_error_family_key(base)?;
        self.lower_error_variant_payload(
            call,
            family_key,
            variant,
            args,
            slots,
            current_function,
            item_slot,
        )
    }

    /// An error variant value from its constructor arguments. The checker
    /// decided which payload field each argument fills; a call it published
    /// no binding for is not lowered.
    fn lower_error_variant_payload(
        &mut self,
        call: ExprId,
        family_key: CompactErrorFamilyKey,
        variant: Name,
        args: &[ArenaCallArg],
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<(LoweredErrorExpr, Vec<(BuildExprId, usize)>)> {
        let family_name = compact_error_family_display(family_key);
        let info = compact_error_family_info(self.declarations, family_key)
            .and_then(|family| family.variants.get(&variant))?;
        let binding = self.bodies.error_constructors.get(&call)?.clone();
        if binding.fields.len() != args.len() {
            return None;
        }
        let mut fields = Vec::with_capacity(info.fields.len());
        let mut bindings = Vec::new();
        let checked = info.fields.values().any(Type::has_unsigned_constraint);
        for (arg, field) in args.iter().zip(&binding.fields) {
            let (ArenaCallArgKind::Named { value, .. } | ArenaCallArgKind::Positional(value)) =
                arg.kind
            else {
                return None;
            };
            let lowered = self.lower_expr(value, slots, current_function, item_slot)?;
            let lowered = if checked {
                let slot = slots.reserve("error payload argument");
                bindings.push((lowered, slot));
                let bound = push_build_row!(self, expr, BuildExprRow::Param(slot));
                self.checked_unsigned_value(
                    bound,
                    info.fields.get(field)?,
                    self.program.arena.expr(value).span,
                )
            } else {
                lowered
            };
            fields.push((Arc::<str>::from(field.as_str().as_str()), lowered));
        }
        if binding.default_message {
            // A variant without a payload always carries its message, so a
            // `{message}` pattern binds what `.message` reads when the
            // constructor omitted it.
            let message = push_build_row!(
                self,
                expr,
                BuildExprRow::Str(format!("{family_name}.{variant}").into())
            );
            fields.push((Arc::<str>::from("message"), message));
        }
        Some((
            LoweredErrorExpr::Structured {
                family: family_name,
                variant: variant.to_string(),
                fields,
                facets: info.facets.clone(),
            },
            bindings,
        ))
    }

    // Constructor namespaces resolve through their defining module, including
    // import aliases; they have no runtime receiver to evaluate.
    pub(super) fn resolved_compact_error_family_key(&self, base: ExprId) -> Option<CompactErrorFamilyKey> {
        Some(self.resolve_compact_error_family_owner(compact_error_family_key(self.program, base)?))
    }

    fn resolve_compact_error_family_owner(
        &self,
        key: CompactErrorFamilyKey,
    ) -> CompactErrorFamilyKey {
        match key {
            CompactErrorFamilyKey::Local(name) => CompactErrorFamilyKey::Local(name),
            CompactErrorFamilyKey::Qualified(name) => {
                CompactErrorFamilyKey::Qualified(QualifiedName::new(
                    self.compact_imported_module_owner(name.namespace)
                        .unwrap_or(name.namespace),
                    name.member,
                ))
            }
        }
    }

    /// A leading-dot variant, built from the declaration the checker selected
    /// from the expected type. `id` is the call for `.Name(args)` and the
    /// member expression for a bare `.Name`.
    pub(super) fn lower_inferred_variant(
        &mut self,
        id: ExprId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let variant = self.bodies.inferred_variants.get(&id)?.clone();
        let span = self.program.arena.expr(id).span;
        let args = match self.program.arena.expr(id).kind {
            ArenaExprKind::Call { args, .. } => self.program.arena.call_args(args).to_vec(),
            _ => Vec::new(),
        };
        match variant {
            crate::sema::check::InferredVariant::Tag {
                type_name,
                variant,
                field_types,
            } => {
                let positional = positional_call_args(&args)?;
                if positional.len() != field_types.len() {
                    return None;
                }
                let (fields, bindings) = self.lower_checked_call_values(
                    &positional,
                    &field_types,
                    slots,
                    current_function,
                    item_slot,
                )?;
                let value = push_build_row!(
                    self,
                    expr,
                    BuildExprRow::Tag {
                        type_name,
                        wire: self
                            .declarations
                            .wire_enums
                            .mappings
                            .get(&type_name)
                            .cloned(),
                        name: Arc::<str>::from(variant.as_str().as_str()),
                        fields,
                    }
                );
                Some(self.wrap_argument_bindings(value, bindings, span))
            }
            crate::sema::check::InferredVariant::Error { family, variant } => {
                let key = match family.as_str().split_once('.') {
                    Some((namespace, member)) => CompactErrorFamilyKey::Qualified(
                        QualifiedName::new(Name::intern(namespace), Name::intern(member)),
                    ),
                    None => CompactErrorFamilyKey::Local(family),
                };
                let key = self.resolve_compact_error_family_owner(key);
                let (error, bindings) = self.lower_error_variant_payload(
                    id,
                    key,
                    variant,
                    &args,
                    slots,
                    current_function,
                    item_slot,
                )?;
                let value = push_build_row!(self, expr, BuildExprRow::Error(Box::new(error)));
                Some(self.wrap_argument_bindings(value, bindings, span))
            }
        }
    }

    pub(super) fn lower_comp_qualifiers(
        &mut self,
        range: crate::syntax::arena::ArenaRange,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<super::super::LoweredCompQualifiers> {
        let mut qualifiers = Vec::new();
        for qualifier in self.program.arena.comp_qualifiers(range).to_vec() {
            match qualifier {
                crate::syntax::arena::ArenaCompQualifier::For { target, iter, span } => {
                    let item_ty = self.loop_item_checked_type(iter);
                    let iter =
                        self.lower_direct_iterable(iter, slots, current_function, item_slot)?;
                    slots.enter();
                    let target =
                        Box::new(self.lower_comp_target_typed(target, slots, item_ty.as_ref())?);
                    qualifiers.push(super::super::LoweredCompQualifier::For { target, iter, span });
                }
                crate::syntax::arena::ArenaCompQualifier::If { condition, span } => {
                    let condition =
                        self.lower_expr(condition, slots, current_function, item_slot)?;
                    qualifiers.push(super::super::LoweredCompQualifier::If { condition, span });
                }
            }
        }
        if !matches!(
            qualifiers.first(),
            Some(super::super::LoweredCompQualifier::For { .. })
        ) {
            return None;
        }
        Some(super::super::LoweredCompQualifiers(qualifiers))
    }

    pub(super) fn lower_comp_target_typed(
        &self,
        id: BindingTargetId,
        slots: &mut SlotScope,
        ty: Option<&Type>,
    ) -> Option<LoweredCompTarget> {
        match self.program.arena.binding_target(id).kind {
            ArenaBindingTargetKind::Name(name) => {
                if is_discard_name(name) {
                    return Some(LoweredCompTarget::Discard);
                }
                if slots.is_declared_here(name) {
                    return None;
                }
                Some(LoweredCompTarget::Slot(
                    slots.declare_with_type(name, ty.cloned()),
                ))
            }
            ArenaBindingTargetKind::Record { fields, .. } => {
                let mut lowered = LoweredCompFields::new();
                for field in self.program.arena.destructure_fields(fields) {
                    let field_ty = match ty {
                        Some(Type::Record(fields)) => fields.get(&field.name),
                        _ => None,
                    };
                    let target = self.lower_comp_target_typed(field.target, slots, field_ty)?;
                    lowered.push((
                        field.name,
                        Box::new(target),
                        self.program.arena.span(field.span),
                    ));
                }
                Some(LoweredCompTarget::Record { fields: lowered })
            }
        }
    }

    pub(super) fn lower_assign_path(
        &mut self,
        target: crate::syntax::arena::AssignTargetId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<Vec<LoweredAssignStep>> {
        match self.program.arena.assign_target(target).kind {
            ArenaAssignTargetKind::Name(_) => Some(Vec::new()),
            ArenaAssignTargetKind::Env(_) => None,
            ArenaAssignTargetKind::Field { base, name } => {
                let mut path = self.lower_assign_path(base, slots, current_function, item_slot)?;
                path.push(LoweredAssignStep::Field(name));
                Some(path)
            }
            ArenaAssignTargetKind::Index { base, index } => {
                let mut path = self.lower_assign_path(base, slots, current_function, item_slot)?;
                let lowered = self.lower_expr(index, slots, current_function, item_slot)?;
                let lowered = if matches!(self.assign_target_checked_type(base, slots), Some(Type::Map(key, _)) if *key == Type::UInt)
                {
                    self.require_uint_key(lowered, self.program.arena.expr(index).span)
                } else {
                    lowered
                };
                path.push(LoweredAssignStep::Index(lowered));
                Some(path)
            }
        }
    }

    /// The binding an assignment writes through; an environment variable
    /// target has none.
    pub(super) fn assign_target_root_name(&self, id: crate::syntax::arena::AssignTargetId) -> Option<Name> {
        match self.program.arena.assign_target(id).kind {
            ArenaAssignTargetKind::Name(name) => Some(name),
            ArenaAssignTargetKind::Env(_) => None,
            ArenaAssignTargetKind::Field { base, .. }
            | ArenaAssignTargetKind::Index { base, .. } => self.assign_target_root_name(base),
        }
    }
}
