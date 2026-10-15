use super::{
    Arc, ArenaExprKind, ArenaPatternKind, BuildExprId, BuildExprRow, BuildPatternId,
    BuildPatternIdSlots, BuildPatternRow, CompactLowerConstructProbe, DurationValue, ExprId,
    CheckedPatternField, CheckedPatternVariantKey, CheckedPatternVariantKind, LoweredValue, Name, PathValue, PatternId,
    QualifiedName, SlotScope, Type, UnaryOp, checked_type_annotation,
    cleanup_lowered_pattern_slots,
};

fn compact_pattern_tag_name(name: Name) -> Name {
    name.as_str()
        .rsplit_once('.')
        .map_or(name, |(_, member)| Name::intern(member))
}

impl<'p> CompactLowerConstructProbe<'p, '_> {
    pub(super) fn lower_pattern_condition_parts(
        &mut self,
        id: ExprId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<(BuildExprId, Vec<usize>)> {
        let ArenaExprKind::PatternCondition { value, arms } = self.program.arena.expr(id).kind
        else {
            return Some((
                self.lower_expr(id, slots, current_function, item_slot)?,
                Vec::new(),
            ));
        };
        let span = self.program.arena.expr(id).span;
        let (ok_ty, err_ty) = self.compact_match_scrutinee_result_types(value);
        // Resolve the subject before installing captures that may shadow it.
        let subject = self.lower_expr(value, slots, current_function, item_slot)?;
        let pattern = self.program.arena.match_expr_arms(arms)[0].pattern;
        let (pattern, cleanup) =
            self.lower_pattern(pattern, slots, ok_ty.as_ref(), err_ty.as_ref())?;
        let captures = cleanup.into_iter().map(|(_, slot)| slot).collect();
        let wildcard = push_build_row!(self, pattern, BuildPatternRow::Wildcard);
        let yes = push_build_row!(self, expr, BuildExprRow::Bool(true));
        let no = push_build_row!(self, expr, BuildExprRow::Bool(false));
        let mut arms = vec![(pattern, None, yes), (wildcard, None, no)];
        if self.bodies.optional_binding_conditions.contains(&id) {
            // An optional binding fails on `null` before its pattern, which
            // would otherwise accept and bind it.
            let null_pattern =
                push_build_row!(self, pattern, BuildPatternRow::Literal(LoweredValue::Null));
            let absent = push_build_row!(self, expr, BuildExprRow::Bool(false));
            arms.insert(0, (null_pattern, None, absent));
        }
        Some((
            push_build_row!(
                self,
                expr,
                BuildExprRow::MatchExpr {
                    value: subject,
                    arms,
                    span
                }
            ),
            captures,
        ))
    }

    fn pattern_str_literal(&mut self, pattern: PatternId) -> Option<Option<Arc<str>>> {
        let lowered = match self.program.arena.pattern(pattern).kind {
            ArenaPatternKind::Wildcard => Some(None),
            // A literal the checker typed as a Path matches a Path subject,
            // which the text-keyed match cannot compare.
            ArenaPatternKind::Literal(expr) => match self.program.arena.expr(expr).kind {
                ArenaExprKind::Str(value) if !self.bodies.path_literals.contains(&expr) => {
                    Some(Some(self.program.arena.string_literal(value).clone()))
                }
                _ => None,
            },
            ArenaPatternKind::Alternation(alts) => {
                let first = self.program.arena.pattern_ids(alts).next()?;
                return self.pattern_str_literal(first);
            }
            _ => None,
        }?;
        self.output.patterns += 1;
        self.output.constructed_patterns += 1;
        Some(lowered)
    }

    /// Like `pattern_str_literal` but expands an alternation `"a" | "b" | …`
    /// into all its literal arms. Returns `Some(None)` for a wildcard (fallback),
    /// `Some(Some(vec))` for one-or-more string literals, `None` if unsupported.
    pub(super) fn pattern_str_literals(&mut self, pattern: PatternId) -> Option<Option<Vec<Arc<str>>>> {
        match self.program.arena.pattern(pattern).kind {
            ArenaPatternKind::Alternation(alts) => {
                let mut literals = Vec::new();
                for alt in self.program.arena.pattern_ids(alts).collect::<Vec<_>>() {
                    // Each alternative must itself be a string literal.
                    {
                        let literal = self.pattern_str_literal(alt)??;
                        literals.push(literal)
                    }
                }
                if literals.is_empty() {
                    return None;
                }
                Some(Some(literals))
            }
            _ => Some(
                self.pattern_str_literal(pattern)?
                    .map(|literal| vec![literal]),
            ),
        }
    }

    /// A pattern's kind, with a target-typed `.Name` replaced by the
    /// qualified pattern the checker resolved it to. `None` when the checker
    /// published no resolution, so such a pattern is never lowered by name.
    fn qualified_pattern_kind(&self, id: PatternId) -> Option<ArenaPatternKind> {
        let kind = &self.program.arena.pattern(id).kind;
        if !kind.is_inferred_variant() {
            return Some(kind.clone());
        }
        Some(
            self.bodies
                .inferred_variant_patterns
                .get(&id)?
                .qualify(kind),
        )
    }

    pub(super) fn pattern_tag_name(&mut self, pattern: PatternId) -> Option<Option<Arc<str>>> {
        let lowered = match self.qualified_pattern_kind(pattern)? {
            ArenaPatternKind::Wildcard => Some(None),
            ArenaPatternKind::Constructor { name, arg: None }
                if self.compact_tag_variant_arity(name) == Some(0) =>
            {
                Some(Some(Arc::<str>::from(name.as_str().as_str())))
            }
            _ => None,
        }?;
        self.output.patterns += 1;
        self.output.constructed_patterns += 1;
        Some(lowered)
    }

    fn pattern_declaration(&self, pattern: PatternId, name: Name) -> Option<super::BindingId> {
        let node = self.program.arena.pattern(pattern);
        let span = match node.kind {
            ArenaPatternKind::Alias { name: alias, name_span, .. } if alias == name => name_span,
            _ => node.span,
        };
        self.declaration_binding(self.program.arena.span(span), name)
    }

    pub(super) fn lower_pattern(
        &mut self,
        id: PatternId,
        slots: &mut SlotScope,
        ok_binding_ty: Option<&Type>,
        err_binding_ty: Option<&Type>,
    ) -> Option<(BuildPatternId, Vec<(Name, usize)>)> {
        self.output.patterns += 1;
        let kind = self.qualified_pattern_kind(id)?;
        let lowered = match &kind {
            ArenaPatternKind::Group(pattern) => {
                self.lower_pattern(*pattern, slots, ok_binding_ty, err_binding_ty)
            }
            ArenaPatternKind::Alias { pattern, name, .. } => {
                let pattern = *pattern;
                let name = *name;
                let (pattern, mut cleanup) =
                    self.lower_pattern(pattern, slots, ok_binding_ty, err_binding_ty)?;
                let slot = slots.declare_source(self.pattern_declaration(id, name)?, name, None);
                cleanup.push((name, slot));
                Some((
                    push_build_row!(self, pattern, BuildPatternRow::Alias { pattern, slot }),
                    cleanup,
                ))
            }
            ArenaPatternKind::Alternation(children) => {
                let children: Vec<_> = self.program.arena.pattern_ids(*children).collect();
                let span = self.program.arena.span(self.program.arena.pattern(id).span);
                let mut bindings = self.bodies.resolved_bindings.declaration_sites.iter()
                    .filter_map(|(&(site, name), &binding)| (site == span).then_some((name, binding)))
                    .collect::<Vec<_>>();
                bindings.sort_unstable_by_key(|(name, _)| *name);
                let mut cleanup = Vec::new();
                for (name, binding) in bindings {
                    let slot = slots.declare_source(binding, name, None);
                    cleanup.push((name, slot));
                }
                let patterns: Option<Vec<_>> = children
                    .into_iter()
                    .map(|child| {
                        self.lower_pattern(child, slots, ok_binding_ty, err_binding_ty)
                            .map(|(pattern, _)| pattern)
                    })
                    .collect();
                Some((
                    push_build_row!(
                        self,
                        pattern,
                        BuildPatternRow::Alternation {
                            patterns: patterns?
                        }
                    ),
                    cleanup,
                ))
            }
            ArenaPatternKind::TestName { name, ty } => {
                if name == "Ok" || name == "Err" {
                    let row = if name == "Ok" {
                        BuildPatternRow::ResultOk {
                            slot: None,
                            unit_only: true,
                        }
                    } else {
                        BuildPatternRow::ResultErr {
                            slot: None,
                            unit_only: true,
                        }
                    };
                    return Some((push_build_row!(self, pattern, row), Vec::new()));
                }
                let text = name.as_str();
                if let Some((family, variant)) = text.rsplit_once('.') {
                    let family = Name::intern(family);
                    let variant = Name::intern(variant);
                    if let Some(family) = self.record_error_pattern_schema(family, variant) {
                        return Some((
                            push_build_row!(
                                self,
                                pattern,
                                BuildPatternRow::ErrorTest {
                                    family,
                                    variant,
                                    fields: Vec::new()
                                }
                            ),
                            Vec::new(),
                        ));
                    }
                }
                if self.compact_tag_variant_arity(*name) == Some(0) {
                    let (type_name, variant) = self.record_tag_pattern_schema(*name)?;
                    Some((
                        push_build_row!(
                            self,
                            pattern,
                            BuildPatternRow::Tag {
                                type_name,
                                name: variant,
                                slots: Default::default()
                            }
                        ),
                        Vec::new(),
                    ))
                } else {
                    let ty = if let Some(facet) = self.compact_qualified_pattern_facet(*name) {
                        Type::ErrorFacet(facet)
                    } else {
                        checked_type_annotation(
                            self.declarations,
                            self.current_namespace,
                            *ty,
                        )?
                        .clone()
                    };
                    if let Type::ErrorFacet(facet) = ty {
                        Some((
                            push_build_row!(
                                self,
                                pattern,
                                BuildPatternRow::Facet {
                                    facet,
                                    result_wrapped: false
                                }
                            ),
                            Vec::new(),
                        ))
                    } else if let Type::Tag(type_name) = ty {
                        let namespace = name
                            .as_str()
                            .rsplit_once('.')
                            .and_then(|(namespace, _)| {
                                self.compact_imported_module_owner(Name::intern(namespace))
                            })
                            .or(self.current_namespace);
                        let variants = if let Some(namespace) = namespace {
                            self.declarations
                                .qualified_tag_variants
                                .iter()
                                .filter_map(|(key, info)| {
                                    (key.namespace == namespace && info.type_name == type_name)
                                        .then_some(key.member)
                                })
                                .collect()
                        } else {
                            self.declarations
                                .tag_variants_by_name
                                .iter()
                                .filter_map(|(name, info)| {
                                    (info.type_name == type_name).then_some(*name)
                                })
                                .collect()
                        };
                        Some((
                            push_build_row!(
                                self,
                                pattern,
                                BuildPatternRow::TagType {
                                    type_name,
                                    variants
                                }
                            ),
                            Vec::new(),
                        ))
                    } else {
                        Some((
                            push_build_row!(
                                self,
                                pattern,
                                BuildPatternRow::Type { ty, slot: None }
                            ),
                            Vec::new(),
                        ))
                    }
                }
            }
            ArenaPatternKind::List { elements, rest } => {
                let children: Vec<_> = self.program.arena.pattern_ids(*elements).collect();
                let rest_id = *rest;
                let mut elements = Vec::new();
                let mut cleanup = Vec::new();
                for child in children {
                    let (pattern, bindings) = self.lower_pattern(child, slots, None, None)?;
                    elements.push(pattern);
                    cleanup.extend(bindings);
                }
                let rest = if let Some(child) = rest_id {
                    let (pattern, bindings) = self.lower_pattern(child, slots, None, None)?;
                    cleanup.extend(bindings);
                    Some(pattern)
                } else {
                    None
                };
                Some((
                    push_build_row!(self, pattern, BuildPatternRow::List { elements, rest }),
                    cleanup,
                ))
            }
            ArenaPatternKind::Record { fields, .. } => {
                let mut lowered = Vec::new();
                let mut cleanup = Vec::new();
                for field in self.program.arena.pattern_fields(*fields).to_vec() {
                    let (pattern, bindings) =
                        self.lower_pattern(field.pattern, slots, None, None)?;
                    cleanup.extend(bindings);
                    lowered.push((field.name, pattern));
                }
                Some((
                    push_build_row!(
                        self,
                        pattern,
                        BuildPatternRow::RecordTest { fields: lowered }
                    ),
                    cleanup,
                ))
            }
            ArenaPatternKind::Wildcard => Some((
                push_build_row!(self, pattern, BuildPatternRow::Wildcard),
                Vec::new(),
            )),
            // The segments and hole kinds are the checker's compilation of
            // the pattern; the syntax supplies only the name each hole binds.
            ArenaPatternKind::Text(parts) => {
                let compiled = self.bodies.text_patterns.get(&id)?.clone();
                let mut holes = Vec::with_capacity(compiled.holes.len());
                let mut cleanup = Vec::new();
                for part in self.program.arena.pattern_ids(*parts).collect::<Vec<_>>() {
                    let ArenaPatternKind::TextHole { binding, .. } =
                        self.program.arena.pattern(part).kind
                    else {
                        continue;
                    };
                    let row = match binding {
                        Some(name) if self.pattern_declaration(part, name).is_some() => {
                            let slot = slots.declare_source(self.pattern_declaration(part, name)?, name, None);
                            cleanup.push((name, slot));
                            BuildPatternRow::Bind { slot }
                        }
                        Some(_) => return None,
                        None => BuildPatternRow::Wildcard,
                    };
                    holes.push(push_build_row!(self, pattern, row));
                }
                if holes.len() != compiled.holes.len() {
                    return None;
                }
                Some((
                    push_build_row!(
                        self,
                        pattern,
                        BuildPatternRow::Text {
                            holes,
                            kinds: compiled.holes,
                            segments: compiled.segments,
                        }
                    ),
                    cleanup,
                ))
            }
            ArenaPatternKind::Literal(expr) => self
                .lower_pattern_literal(*expr)
                .map(|pattern| (pattern, Vec::new())),
            ArenaPatternKind::Binding(name) if self.compact_tag_variant_arity(*name) == Some(0) => {
                let (type_name, variant) = self.record_tag_pattern_schema(*name)?;
                Some((
                    push_build_row!(
                        self,
                        pattern,
                        BuildPatternRow::Tag {
                            type_name,
                            name: variant,
                            slots: Default::default(),
                        }
                    ),
                    Vec::new(),
                ))
            }
            ArenaPatternKind::Binding(name) if self.pattern_declaration(id, *name).is_some() => {
                let slot = slots.declare_source(self.pattern_declaration(id, *name)?, *name, None);
                Some((
                    push_build_row!(self, pattern, BuildPatternRow::Bind { slot }),
                    vec![(*name, slot)],
                ))
            }
            ArenaPatternKind::Type {
                binding: Some(name),
                ty,
            } if self.pattern_declaration(id, *name).is_some() => {
                let lowered_ty = checked_type_annotation(
                    self.declarations,
                    self.current_namespace,
                    *ty,
                )?
                .clone();
                let slot = slots.declare_source(self.pattern_declaration(id, *name)?, *name, None);
                Some((
                    push_build_row!(
                        self,
                        pattern,
                        BuildPatternRow::Type {
                            ty: lowered_ty,
                            slot: Some(slot),
                        }
                    ),
                    vec![(*name, slot)],
                ))
            }
            ArenaPatternKind::Type { binding: None, ty } => {
                let checked_ty = checked_type_annotation(
                    self.declarations,
                    self.current_namespace,
                    *ty,
                )?;
                // Checked imported facets keep their module alias, while runtime
                // errors store the declared facet name. Validate the alias owner
                // and declaration before canonicalizing the checked facet.
                let lowered_ty = match checked_ty {
                    Type::ErrorFacet(facet) if facet.as_str().contains('.') => {
                        Type::ErrorFacet(self.compact_qualified_pattern_facet(*facet)?)
                    }
                    _ => checked_ty.clone(),
                };
                Some((
                    push_build_row!(
                        self,
                        pattern,
                        BuildPatternRow::Type {
                            ty: lowered_ty,
                            slot: None,
                        }
                    ),
                    Vec::new(),
                ))
            }
            ArenaPatternKind::ErrorVariant {
                family,
                variant,
                fields,
            } => {
                let qualified = Name::intern(format!("{family}.{variant}"));
                if fields.len == 0 && self.compact_tag_variant_arity(qualified) == Some(0) {
                    let (type_name, tag_variant) = self.record_tag_pattern_schema(qualified)?;
                    return Some((
                        push_build_row!(
                            self,
                            pattern,
                            BuildPatternRow::Tag {
                                type_name,
                                name: tag_variant,
                                slots: Default::default()
                            }
                        ),
                        Vec::new(),
                    ));
                }
                let family = self.record_error_pattern_schema(*family, *variant)?;
                let mut lowered = Vec::new();
                let mut cleanup = Vec::new();
                for field in self.program.arena.pattern_fields(*fields).to_vec() {
                    let (pattern, bindings) =
                        self.lower_pattern(field.pattern, slots, None, None)?;
                    cleanup.extend(bindings);
                    lowered.push((field.name, pattern));
                }
                Some((
                    push_build_row!(
                        self,
                        pattern,
                        BuildPatternRow::ErrorTest {
                            family,
                            variant: *variant,
                            fields: lowered
                        }
                    ),
                    cleanup,
                ))
            }
            ArenaPatternKind::Facet(facet) => Some((
                push_build_row!(
                    self,
                    pattern,
                    BuildPatternRow::Facet {
                        facet: *facet,
                        result_wrapped: false,
                    }
                ),
                Vec::new(),
            )),
            ArenaPatternKind::Constructor { name, arg } => {
                if let Some(arg) = arg
                    && !matches!(
                        self.program.arena.pattern(*arg).kind,
                        ArenaPatternKind::Wildcard | ArenaPatternKind::Binding(_)
                    )
                {
                    if name == "Ok" || name == "Err" {
                        let (inner, cleanup) = self.lower_pattern(*arg, slots, None, None)?;
                        return Some((
                            push_build_row!(
                                self,
                                pattern,
                                BuildPatternRow::ResultTest {
                                    ok: name == "Ok",
                                    inner
                                }
                            ),
                            cleanup,
                        ));
                    }
                    let patterns = match self.program.arena.pattern(*arg).kind {
                        ArenaPatternKind::Tuple(fields) => {
                            self.program.arena.pattern_ids(fields).collect::<Vec<_>>()
                        }
                        _ => vec![*arg],
                    };
                    let mut fields = Vec::new();
                    let mut cleanup = Vec::new();
                    for pattern in patterns {
                        let (field, bindings) = self.lower_pattern(pattern, slots, None, None)?;
                        fields.push(field);
                        cleanup.extend(bindings);
                    }
                    return Some((
                        {
                            self.record_tag_pattern_schema(*name)?;
                            push_build_row!(
                                self,
                                pattern,
                                BuildPatternRow::TagTest {
                                    type_name: self.compact_tag_type_name(*name)?,
                                    name: compact_pattern_tag_name(*name),
                                    fields
                                }
                            )
                        },
                        cleanup,
                    ));
                }
                if name == "Ok" || name == "Err" {
                    let mut cleanup = Vec::new();
                    let binding_ty = if name == "Ok" {
                        ok_binding_ty
                    } else {
                        err_binding_ty
                    };
                    let (slot, unit_only) =
                        self.lower_result_pattern_slot(*arg, slots, &mut cleanup, binding_ty)?;
                    return Some((
                        if name == "Ok" {
                            push_build_row!(
                                self,
                                pattern,
                                BuildPatternRow::ResultOk { slot, unit_only }
                            )
                        } else {
                            push_build_row!(
                                self,
                                pattern,
                                BuildPatternRow::ResultErr { slot, unit_only }
                            )
                        },
                        cleanup,
                    ));
                }
                let arity = self.compact_tag_variant_arity(*name)?;
                let mut cleanup = Vec::new();
                let field_slots =
                    match self.lower_tag_pattern_slots(*arg, arity, slots, &mut cleanup) {
                        Some(field_slots) => field_slots,
                        None => {
                            cleanup_lowered_pattern_slots(slots, cleanup);
                            return None;
                        }
                    };
                self.record_tag_pattern_schema(*name)?;
                Some((
                    push_build_row!(
                        self,
                        pattern,
                        BuildPatternRow::Tag {
                            type_name: self.compact_tag_type_name(*name)?,
                            name: compact_pattern_tag_name(*name),
                            slots: field_slots,
                        }
                    ),
                    cleanup,
                ))
            }
            ArenaPatternKind::Binding(_)
            | ArenaPatternKind::Type {
                binding: Some(_), ..
            }
            | ArenaPatternKind::Tuple(_)
            | ArenaPatternKind::TextHole { .. } => None,
        }?;
        self.output.constructed_patterns += 1;
        Some(lowered)
    }

    fn lower_pattern_literal(&self, id: ExprId) -> Option<BuildPatternId> {
        match self.program.arena.expr(id).kind {
            ArenaExprKind::Unary {
                op: UnaryOp::Neg,
                expr,
            } => {
                let value = match self.program.arena.expr(expr).kind {
                    ArenaExprKind::Int(value) => LoweredValue::Int(
                        self.program
                            .arena
                            .int_literal(value)
                            .value()?
                            .checked_neg()?,
                    ),
                    ArenaExprKind::Float(value) => {
                        LoweredValue::Float(crate::runtime::value::FloatValue::new(
                            -self.program.arena.float_literal(value).value()?,
                        ))
                    }
                    _ => return None,
                };
                Some(push_build_row!(
                    self,
                    pattern,
                    BuildPatternRow::Literal(value)
                ))
            }
            ArenaExprKind::Float(value) => self
                .program
                .arena
                .float_literal(value)
                .value()
                .map(crate::runtime::value::FloatValue::new)
                .map(|value| {
                    push_build_row!(
                        self,
                        pattern,
                        BuildPatternRow::Literal(LoweredValue::Float(value))
                    )
                }),
            ArenaExprKind::Bytes(value) => Some(push_build_row!(
                self,
                pattern,
                BuildPatternRow::Literal(LoweredValue::Bytes(
                    self.program.arena.bytes_literal(value).clone()
                ))
            )),
            ArenaExprKind::Null => Some(push_build_row!(
                self,
                pattern,
                BuildPatternRow::Literal(LoweredValue::Null)
            )),
            ArenaExprKind::Int(value) => {
                self.program.arena.int_literal(value).value().map(|value| {
                    push_build_row!(
                        self,
                        pattern,
                        BuildPatternRow::Literal(LoweredValue::Int(value))
                    )
                })
            }
            ArenaExprKind::Bool(value) => Some(push_build_row!(
                self,
                pattern,
                BuildPatternRow::Literal(LoweredValue::Bool(value))
            )),
            ArenaExprKind::Duration(value) => self
                .program
                .arena
                .duration_literal(value)
                .millis()
                .map(|millis| {
                    push_build_row!(
                        self,
                        pattern,
                        BuildPatternRow::Literal(LoweredValue::Duration(DurationValue { millis }))
                    )
                }),
            ArenaExprKind::Str(value) if self.bodies.path_literals.contains(&id) => {
                let path = PathValue::from_text(self.program.arena.string_literal(value)).ok()?;
                Some(push_build_row!(
                    self,
                    pattern,
                    BuildPatternRow::Literal(LoweredValue::Path(path))
                ))
            }
            ArenaExprKind::Str(value) => Some(push_build_row!(
                self,
                pattern,
                BuildPatternRow::Literal(LoweredValue::Str(
                    self.program.arena.string_literal(value).clone(),
                ))
            )),
            ArenaExprKind::GlobStr(_)
            | ArenaExprKind::FmtString(_)
            | ArenaExprKind::PathFmtString(_)
            | ArenaExprKind::PathStr(_)
            | ArenaExprKind::Regex(_)
            | ArenaExprKind::Ident(_)
            | ArenaExprKind::Item
            | ArenaExprKind::LastStatus
            | ArenaExprKind::List(_)
            | ArenaExprKind::ListComp { .. }
            | ArenaExprKind::MapComp { .. }
            | ArenaExprKind::Set(_)
            | ArenaExprKind::SetComp { .. }
            | ArenaExprKind::Record(_)
            | ArenaExprKind::If { .. }
            | ArenaExprKind::Match { .. }
            | ArenaExprKind::PatternTest { .. }
            | ArenaExprKind::PatternCondition { .. }
            | ArenaExprKind::Unary {
                op: UnaryOp::Not,
                ..
            }
            | ArenaExprKind::ComparisonChain(_)
            | ArenaExprKind::Binary { .. }
            | ArenaExprKind::Call { .. }
            | ArenaExprKind::ValuePipelineCall { .. }
            | ArenaExprKind::Field { .. }
            | ArenaExprKind::NullSafeField { .. }
            | ArenaExprKind::Index { .. }
            | ArenaExprKind::Slice { .. }
            | ArenaExprKind::EnvString(_)
            | ArenaExprKind::EnvPathList
            | ArenaExprKind::Pipeline { .. }
            | ArenaExprKind::StructuredPipeline { .. }
            | ArenaExprKind::Run(_)
            | ArenaExprKind::Spawn(_)
            | ArenaExprKind::Wait(_)
            | ArenaExprKind::BuilderCall { .. }
            | ArenaExprKind::Try(_)
            | ArenaExprKind::Capture(_)
            | ArenaExprKind::Require { .. }
            | ArenaExprKind::Convert { .. }
            | ArenaExprKind::Collect { .. }
            | ArenaExprKind::Loop { .. }
            | ArenaExprKind::Retry { .. }
            | ArenaExprKind::ValueBlock(_)
            | ArenaExprKind::ErrorContext { .. }
            | ArenaExprKind::ContextScope { .. }
            | ArenaExprKind::TempDirScope { .. }
            | ArenaExprKind::ResourceScope { .. } => None,
        }
    }

    fn lower_result_pattern_slot(
        &self,
        arg: Option<PatternId>,
        slots: &mut SlotScope,
        cleanup: &mut Vec<(Name, usize)>,
        binding_type: Option<&Type>,
    ) -> Option<(Option<usize>, bool)> {
        let Some(pattern) = arg else {
            return Some((None, true));
        };
        match self.program.arena.pattern(pattern).kind {
            ArenaPatternKind::Wildcard => Some((None, false)),
            ArenaPatternKind::Binding(name) if self.pattern_declaration(pattern, name).is_some() => {
                let slot = slots.declare_source(self.pattern_declaration(pattern, name)?, name, binding_type.cloned());
                cleanup.push((name, slot));
                Some((Some(slot), false))
            }
            _ => None,
        }
    }

    fn lower_tag_pattern_slots(
        &self,
        arg: Option<PatternId>,
        arity: usize,
        slots: &mut SlotScope,
        cleanup: &mut Vec<(Name, usize)>,
    ) -> Option<BuildPatternIdSlots> {
        match (arity, arg) {
            (0, None) => Some(Default::default()),
            (1, Some(pattern)) => {
                let mut field_slots = BuildPatternIdSlots::new();
                field_slots.push(self.lower_tag_pattern_field(pattern, slots, cleanup)?);
                Some(field_slots)
            }
            (_, Some(pattern)) => {
                let ArenaPatternKind::Tuple(fields) = self.program.arena.pattern(pattern).kind
                else {
                    return None;
                };
                let fields = self.program.arena.pattern_ids(fields).collect::<Vec<_>>();
                if fields.len() != arity {
                    return None;
                }
                let mut field_slots = BuildPatternIdSlots::with_capacity(fields.len());
                for field in fields {
                    field_slots.push(self.lower_tag_pattern_field(field, slots, cleanup)?);
                }
                Some(field_slots)
            }
            _ => None,
        }
    }

    fn lower_tag_pattern_field(
        &self,
        id: PatternId,
        slots: &mut SlotScope,
        cleanup: &mut Vec<(Name, usize)>,
    ) -> Option<Option<usize>> {
        match self.program.arena.pattern(id).kind {
            ArenaPatternKind::Wildcard => Some(None),
            ArenaPatternKind::Binding(name) => {
                if !self.pattern_declaration(id, name).is_some() {
                    return None;
                }
                let slot = slots.declare_source(self.pattern_declaration(id, name)?, name, None);
                cleanup.push((name, slot));
                Some(Some(slot))
            }
            _ => None,
        }
    }

    fn compact_pattern_error_family(
        &self,
        family: Name,
    ) -> Option<(Name, crate::sema::check::ErrorFamilyInfo)> {
        if let Some((namespace, member)) = family.as_str().rsplit_once('.') {
            let key =
                self.compact_qualified_function_key(Name::intern(namespace), Name::intern(member));
            let info = self
                .declarations
                .qualified_error_families
                .get(&key)?
                .clone();
            Some((Name::intern(key.to_string()), info))
        } else {
            self.current_namespace
                .and_then(|namespace| {
                    let key = self.compact_qualified_function_key(namespace, family);
                    self.declarations
                        .qualified_error_families
                        .get(&key)
                        .cloned()
                        .map(|info| (Name::intern(key.to_string()), info))
                })
                .or_else(|| {
                    self.declarations
                        .error_families_by_name
                        .get(&family)
                        .cloned()
                        .map(|info| (family, info))
                })
        }
    }

    pub(super) fn compact_qualified_pattern_facet(&self, name: Name) -> Option<Name> {
        let text = name.as_str();
        let (namespace, facet) = text.rsplit_once('.')?;
        let namespace = self.compact_imported_module_owner(Name::intern(namespace))?;
        let facet = Name::intern(facet);
        self.declarations
            .qualified_error_families
            .iter()
            .any(|(key, info)| {
                key.namespace == namespace
                    && info
                        .variants
                        .values()
                        .any(|variant| variant.facets.contains(&facet))
            })
            .then_some(facet)
    }

    pub(super) fn compact_tag_type_name(&self, name: Name) -> Option<Name> {
        if let Some((namespace, member)) = name.as_str().rsplit_once('.') {
            return self
                .declarations
                .qualified_tag_variants
                .get(
                    &self.compact_qualified_function_key(
                        Name::intern(namespace),
                        Name::intern(member),
                    ),
                )
                .map(|variant| variant.type_name);
        }
        self.current_namespace
            .and_then(|namespace| {
                self.declarations
                    .qualified_tag_variants
                    .get(&QualifiedName::new(namespace, name))
            })
            .or_else(|| self.declarations.tag_variants_by_name.get(&name))
            .map(|variant| variant.type_name)
    }

    pub(super) fn compact_tag_wire(
        &self,
        name: Name,
    ) -> Option<Arc<crate::sema::wire_enums::WireEnumMapping>> {
        self.declarations
            .wire_enums
            .mappings
            .get(&self.compact_tag_type_name(name)?)
            .cloned()
    }

    pub(super) fn compact_tag_variant_field_types(&self, name: Name) -> Option<Vec<Type>> {
        if let Some((namespace, member)) = name.as_str().rsplit_once('.') {
            return self
                .declarations
                .qualified_tag_variants
                .get(
                    &self.compact_qualified_function_key(
                        Name::intern(namespace),
                        Name::intern(member),
                    ),
                )
                .map(|variant| variant.field_types.clone());
        }
        self.current_namespace
            .and_then(|namespace| {
                self.declarations
                    .qualified_tag_variants
                    .get(&QualifiedName::new(namespace, name))
            })
            .or_else(|| self.declarations.tag_variants_by_name.get(&name))
            .map(|variant| variant.field_types.clone())
    }

    pub(super) fn record_tag_pattern_schema(&self, name: Name) -> Option<(Name, Name)> {
        let type_name = self.compact_tag_type_name(name)?;
        let variant = compact_pattern_tag_name(name);
        self.record_checked_tag_schema(
            type_name,
            variant,
            &self.compact_tag_variant_field_types(name)?,
        )
        .then_some((type_name, variant))
    }

    pub(super) fn record_checked_tag_schema(
        &self,
        type_name: Name,
        variant: Name,
        field_types: &[Type],
    ) -> bool {
        let fields = field_types
            .iter()
            .cloned()
            .map(|ty| CheckedPatternField { name: None, ty })
            .collect();
        let key = CheckedPatternVariantKey {
            kind: CheckedPatternVariantKind::Tag,
            owner: type_name,
            variant,
        };
        self.scratch
            .borrow_mut()
            .record_checked_pattern_variant(key, fields)
    }

    pub(super) fn record_error_pattern_schema(&self, family: Name, variant: Name) -> Option<Name> {
        let (family, info) = self.compact_pattern_error_family(family)?;
        let variant_info = info.variants.get(&variant)?;
        let fields = variant_info
            .fields
            .keys()
            .zip(variant_info.fields.values())
            .map(|(name, ty)| CheckedPatternField {
                name: Some(*name),
                ty: ty.clone(),
            })
            .collect();
        let key = CheckedPatternVariantKey {
            kind: CheckedPatternVariantKind::Error,
            owner: family,
            variant,
        };
        self.scratch
            .borrow_mut()
            .record_checked_pattern_variant(key, fields)
            .then_some(family)
    }

    pub(super) fn compact_tag_variant_arity(&self, name: Name) -> Option<usize> {
        if let Some((namespace, member)) = name.as_str().rsplit_once('.') {
            return self.compact_qualified_tag_variant_arity(
                Name::intern(namespace),
                Name::intern(member),
            );
        }
        if let Some(namespace) = self.current_namespace
            && let Some(variant) = self
                .declarations
                .qualified_tag_variants
                .get(&QualifiedName::new(namespace, name))
        {
            return Some(variant.field_count);
        }
        self.declarations
            .tag_variants_by_name
            .get(&name)
            .map(|variant| variant.field_count)
    }

    pub(super) fn compact_qualified_tag_variant_arity(&self, module: Name, name: Name) -> Option<usize> {
        self.declarations
            .qualified_tag_variants
            .get(&self.compact_qualified_function_key(module, name))
            .map(|variant| variant.field_count)
    }
}
