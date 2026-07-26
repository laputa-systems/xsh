use super::Diagnostic;
use super::{Binding, Checker, FxHashSet, Name, Span, Type, TypeDefBody, result_types};
use crate::syntax::arena::{ArenaPatternKind, ArenaProgram, PatternId};

fn type_pattern_input_is_dynamic(ty: &Type) -> bool {
    match ty {
        Type::Any | Type::Unknown | Type::Invalid => true,
        Type::Record(fields) => fields.is_empty(),
        _ => false,
    }
}

/// Arena-native mirror of `check_pattern` and its callees. Fully self-contained:
/// `check_pattern` (unlike `check_match`, the statement-level match in
/// stmt.rs) never calls `check_block` — only `check_expr`/type annotations on
/// leaf sub-expressions, so no `Block` support is needed to port it.
#[allow(dead_code)]
impl Checker {
    pub(super) fn check_pattern_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        pattern_id: PatternId,
        value_ty: &Type,
    ) {
        let pattern = arena.arena.pattern(pattern_id);
        let span = arena.arena.span(pattern.span);
        match &pattern.kind {
            ArenaPatternKind::Wildcard => {}
            ArenaPatternKind::Binding(name) => {
                if let Some(info) = self.tag_variants.get(name).cloned()
                    && info.field_count == 0
                {
                    if !matches!(value_ty, Type::Unknown)
                        && !matches!(value_ty, Type::Tag(t) if t == &info.type_name)
                    {
                        self.error(
                            span,
                            &format!(
                                "tag pattern `{name}` is for type `{}`, but value has type `{value_ty}`",
                                info.type_name
                            ),
                            "check.pattern-type",
                        );
                    }
                    return;
                }
                self.define(*name, Binding::new(value_ty.clone(), false), span);
            }
            ArenaPatternKind::Type { binding, ty } => {
                if !type_pattern_input_is_dynamic(value_ty) {
                    self.error(
                        span,
                        "type patterns require a dynamic value",
                        "check.pattern-type",
                    );
                }
                let narrowed_ty = self.type_from_arena(arena, *ty);
                if let Some(name) = binding {
                    self.define(*name, Binding::new(narrowed_ty, false), span);
                }
            }
            ArenaPatternKind::Literal(expr) => {
                let actual = self.check_expr_arena(arena, source, *expr, Some(value_ty));
                let expr_span = arena.arena.expr(*expr).span;
                self.expect_type(value_ty, &actual, expr_span);
            }
            ArenaPatternKind::Record { fields, .. } => {
                let record_fields = match value_ty {
                    Type::Record(fields) => Some(fields),
                    Type::Error | Type::ErrorFamily(_) | Type::ErrorVariant { .. } => {
                        self.error(
                            span,
                            "record matching on error fields was removed; match exact variants or facets instead",
                            "check.error-removed",
                        );
                        None
                    }
                    Type::ProcessError => None,
                    Type::Unknown => None,
                    _ => {
                        self.error(
                            span,
                            "record patterns require a record-like value",
                            "check.pattern-type",
                        );
                        None
                    }
                };
                let mut names = FxHashSet::default();
                for field in arena.arena.pattern_fields(*fields) {
                    let field_span = arena.arena.span(field.span);
                    if !names.insert(field.name) {
                        self.error(field_span, "duplicate pattern field", "check.pattern-field");
                    }
                    let field_ty = record_fields
                        .and_then(|fields| fields.get(&field.name))
                        .cloned()
                        .unwrap_or(Type::Unknown);
                    if let Some(fields) = record_fields
                        && !fields.contains_key(&field.name)
                    {
                        self.error(field_span, "unknown pattern field", "check.pattern-field");
                    }
                    self.check_pattern_arena(arena, source, field.pattern, &field_ty);
                }
            }
            ArenaPatternKind::Alternation(patterns) => {
                for sub_id in arena.arena.pattern_ids(*patterns) {
                    self.check_pattern_arena(arena, source, sub_id, value_ty);
                }
            }
            ArenaPatternKind::Tuple(patterns) => {
                for sub_id in arena.arena.pattern_ids(*patterns) {
                    self.check_pattern_arena(arena, source, sub_id, &Type::Unknown);
                }
            }
            ArenaPatternKind::Constructor { name, arg } => {
                if let Some(info) = self.tag_variants.get(name).cloned() {
                    if !matches!(value_ty, Type::Unknown)
                        && !matches!(value_ty, Type::Tag(t) if t == &info.type_name)
                    {
                        self.error(
                            span,
                            &format!(
                                "tag pattern `{name}` is for type `{}`, but value has type `{value_ty}`",
                                info.type_name
                            ),
                            "check.pattern-type",
                        );
                    }
                    if info.field_count == 0 {
                        if arg.is_some() {
                            self.error(
                                span,
                                &format!("tag variant `{name}` has no fields"),
                                "check.pattern-arity",
                            );
                        }
                    } else if let Some(arg) = arg {
                        if info.field_count == 1 {
                            self.check_pattern_arena(
                                arena,
                                source,
                                *arg,
                                &info.field_types[0].clone(),
                            );
                        } else if let ArenaPatternKind::Tuple(sub_patterns) =
                            &arena.arena.pattern(*arg).kind
                        {
                            for (sub_id, field_ty) in arena
                                .arena
                                .pattern_ids(*sub_patterns)
                                .zip(info.field_types.iter())
                            {
                                self.check_pattern_arena(arena, source, sub_id, field_ty);
                            }
                        } else {
                            self.check_pattern_arena(arena, source, *arg, &Type::Unknown);
                        }
                    } else {
                        self.error(
                            span,
                            &format!(
                                "tag variant `{name}` has {} field(s) — provide a binding",
                                info.field_count
                            ),
                            "check.pattern-arity",
                        );
                    }
                    return;
                }
                let Some((ok_ty, err_ty)) = result_types(value_ty) else {
                    if !matches!(value_ty, Type::Unknown) {
                        self.error(
                            span,
                            "constructor patterns require a Result value",
                            "check.pattern-type",
                        );
                    }
                    return;
                };
                let target = match name.as_str().as_str() {
                    "Ok" => ok_ty,
                    "Err" => err_ty,
                    _ => {
                        self.error(
                            span,
                            "unknown constructor pattern",
                            "check.pattern-constructor",
                        );
                        Type::Unknown
                    }
                };
                if let Some(arg) = arg {
                    self.check_pattern_arena(arena, source, *arg, &target);
                } else if !matches!(target, Type::Unit | Type::Unknown) {
                    self.error(
                        span,
                        "constructor pattern needs an argument for this Result type",
                        "check.pattern-arity",
                    );
                }
            }
            ArenaPatternKind::ErrorVariant {
                family,
                variant,
                fields,
            } => {
                let Some(variant_info) = self
                    .error_families
                    .get(family)
                    .and_then(|family| family.variants.get(variant))
                    .cloned()
                else {
                    self.error(
                        span,
                        "unknown error variant pattern",
                        "check.pattern-constructor",
                    );
                    return;
                };
                let type_matches = match value_ty {
                    Type::Unknown | Type::Any | Type::Error | Type::ProcessError => true,
                    Type::ErrorFamily(name) => name == family,
                    Type::ErrorVariant {
                        family: actual_family,
                        variant: actual_variant,
                    } => actual_family == family && actual_variant == variant,
                    _ => false,
                };
                if !type_matches {
                    self.error(
                        span,
                        "error variant pattern does not match value type",
                        "check.pattern-type",
                    );
                }
                let mut names = FxHashSet::default();
                for field in arena.arena.pattern_fields(*fields) {
                    let field_span = arena.arena.span(field.span);
                    if !names.insert(field.name) {
                        self.error(field_span, "duplicate pattern field", "check.pattern-field");
                    }
                    let Some(field_ty) = variant_info.fields.get(&field.name).cloned() else {
                        self.error(
                            field_span,
                            "unknown error payload field",
                            "check.pattern-field",
                        );
                        self.check_pattern_arena(arena, source, field.pattern, &Type::Unknown);
                        continue;
                    };
                    self.check_pattern_arena(arena, source, field.pattern, &field_ty);
                }
            }
            ArenaPatternKind::Facet(name) => {
                if !self.error_facets.contains(name) {
                    self.error(
                        span,
                        "unknown error facet pattern",
                        "check.pattern-constructor",
                    );
                }
                if !matches!(
                    value_ty,
                    Type::Unknown
                        | Type::Any
                        | Type::Error
                        | Type::ProcessError
                        | Type::ErrorFamily(_)
                        | Type::ErrorVariant { .. }
                ) {
                    self.error(
                        span,
                        "error facet patterns require an error value",
                        "check.pattern-type",
                    );
                }
            }
        }
    }

    pub(super) fn check_tag_exhaustiveness_arena(
        &mut self,
        arena: &ArenaProgram,
        value_ty: &Type,
        arm_patterns: Vec<(PatternId, Span)>,
        span: Span,
    ) {
        let Type::Tag(type_name) = value_ty else {
            return;
        };
        let Some(body) = self.type_defs.get(type_name).cloned() else {
            return;
        };
        let TypeDefBody::TagUnion(variants) = body else {
            return;
        };
        let has_catch_all = arm_patterns.iter().any(|(pattern_id, _)| {
            match &arena.arena.pattern(*pattern_id).kind {
                ArenaPatternKind::Wildcard => true,
                ArenaPatternKind::Binding(name) => !self.tag_variants.contains_key(name),
                _ => false,
            }
        });
        if has_catch_all {
            return;
        }
        let mut covered: FxHashSet<Name> = FxHashSet::default();
        for (pattern_id, _) in &arm_patterns {
            collect_covered_constructors_arena(arena, *pattern_id, &mut covered);
        }
        let missing: Vec<String> = variants
            .iter()
            .filter(|v| !covered.contains(&v.name))
            .map(|v| v.name.as_str().to_string())
            .collect();
        if missing.is_empty() {
            return;
        }
        let missing_list = missing.join(", ");
        self.diagnostics.push(
            Diagnostic::new(
                crate::diagnostic::Severity::Warning,
                format!("non-exhaustive match: missing variant(s) `{missing_list}`"),
            )
            .with_code("check.non-exhaustive-match")
            .with_label(crate::diagnostic::Label::secondary(
                span,
                "not all variants of this tag union are handled",
            )),
        );
    }
}

#[allow(dead_code)]
pub(super) fn collect_covered_constructors_arena(
    arena: &ArenaProgram,
    pattern_id: PatternId,
    covered: &mut FxHashSet<Name>,
) {
    match &arena.arena.pattern(pattern_id).kind {
        ArenaPatternKind::Constructor { name, .. } | ArenaPatternKind::Binding(name) => {
            covered.insert(*name);
        }
        ArenaPatternKind::Alternation(patterns) => {
            for sub_id in arena.arena.pattern_ids(*patterns) {
                collect_covered_constructors_arena(arena, sub_id, covered);
            }
        }
        _ => {}
    }
}
