#![allow(clippy::single_call_fn)]

//! Target-typed variants: `.Name` and `.Name(args)` select a variant of the
//! one enum or error family the expected type names. Inside a stream stage
//! block `.` is the item, so there `.name` stays a field read.

use super::{
    Checker, Diagnostic, InferredVariant, InferredVariantPattern, Label, Name, Span,
    TagVariantInfo, Type,
};
use crate::diagnostic::DiagnosticCode;
use crate::syntax::arena::{ArenaCallArg, ArenaExprKind, ArenaPatternKind, ArenaProgram, ExprId};

/// Why a leading-dot name selects no variant.
enum VariantMiss {
    /// The expected type is a recovery type; its cause is already reported.
    Recovery,
    NoExpectedType,
    Uninferred,
    /// `Error`, a facet, or `ProcessError`: no single family.
    CommonError(Type),
    NotVariantType(Type),
    UnknownVariant {
        ty: Type,
        variants: Vec<String>,
    },
    NotVisible(Type),
}

impl Checker {
    /// `.name` reads the current item wherever a stream stage block supplies
    /// one; everywhere else it is a target-typed variant.
    pub(super) fn item_shorthand_in_scope(&self) -> bool {
        !self.stream_item_types.is_empty()
    }

    /// Whether `expr` is `.Name` or `.Name(...)` naming a variant here.
    pub(super) fn is_inferred_variant_expr(&self, arena: &ArenaProgram, expr: ExprId) -> bool {
        let member = match arena.arena.expr(expr).kind {
            ArenaExprKind::Call { callee, .. } => callee,
            _ => expr,
        };
        matches!(arena.arena.expr(member).kind, ArenaExprKind::Field { base, .. }
            if matches!(arena.arena.expr(base).kind, ArenaExprKind::Item))
            && !self.item_shorthand_in_scope()
    }

    /// An equality operand spelled `module.Variant` whose other operand has
    /// the variant's enum type: `.Variant` would compare the same value.
    pub(super) fn note_compared_variant_qualifier(
        &mut self,
        arena: &ArenaProgram,
        operand: ExprId,
        other: &Type,
    ) {
        let expr = arena.arena.expr(operand);
        let ArenaExprKind::Field { base, name } = expr.kind else {
            return;
        };
        let ArenaExprKind::Ident(namespace) = arena.arena.expr(base).kind else {
            return;
        };
        let Some(info) = self
            .tag_variants
            .get(&Name::intern(format!("{namespace}.{name}")))
        else {
            return;
        };
        if info.field_count != 0 || *other != Type::Tag(info.type_name) {
            return;
        }
        let selected = InferredVariant::Tag {
            type_name: info.type_name,
            variant: name,
            field_types: Vec::new(),
        };
        self.note_variant_qualifier(expr.span, expr.span, name, &selected, Some(other));
    }

    /// The enum or error family variant `name` selects in `expected`. An
    /// optional expectation selects from its inner type.
    fn select_inferred_variant(
        &self,
        name: Name,
        expected: Option<&Type>,
    ) -> Result<InferredVariant, VariantMiss> {
        let Some(mut expected) = expected else {
            return Err(VariantMiss::NoExpectedType);
        };
        while let Type::Optional(inner) = expected {
            expected = inner;
        }
        match expected {
            Type::Unknown | Type::Invalid => Err(VariantMiss::Recovery),
            Type::Any => Err(VariantMiss::NoExpectedType),
            Type::Inference(_) => Err(VariantMiss::Uninferred),
            Type::Error | Type::ErrorFacet(_) | Type::ProcessError => {
                Err(VariantMiss::CommonError(expected.clone()))
            }
            Type::ErrorFamily(family) | Type::ErrorVariant { family, .. } => {
                let Some(info) = self.error_families.get(family) else {
                    return Err(VariantMiss::NotVisible(expected.clone()));
                };
                if info.variants.contains_key(&name) {
                    Ok(InferredVariant::Error {
                        family: *family,
                        variant: name,
                    })
                } else {
                    Err(VariantMiss::UnknownVariant {
                        ty: Type::ErrorFamily(*family),
                        variants: info.variants.keys().map(ToString::to_string).collect(),
                    })
                }
            }
            Type::Tag(type_name) => {
                // Every visible spelling of one variant shares its declaration;
                // the unqualified spelling, then the first qualified one, is chosen
                // only so the published field types are deterministic.
                let mut spellings: Vec<(&Name, &TagVariantInfo)> = self
                    .tag_variants
                    .iter()
                    .filter(|(_, info)| info.type_name == *type_name)
                    .collect();
                if spellings.is_empty() {
                    return Err(VariantMiss::NotVisible(expected.clone()));
                }
                spellings.sort_by_key(|(spelling, _)| {
                    let spelling = spelling.as_str();
                    (spelling.contains('.'), spelling.to_string())
                });
                let selected = spellings.iter().find(|(spelling, _)| {
                    let spelling = spelling.as_str();
                    spelling.rsplit('.').next() == Some(name.as_str().as_str())
                });
                match selected {
                    Some((_, info)) => Ok(InferredVariant::Tag {
                        type_name: *type_name,
                        variant: name,
                        field_types: info.field_types.clone(),
                    }),
                    None => {
                        let mut variants: Vec<String> = spellings
                            .iter()
                            .filter_map(|(spelling, _)| {
                                spelling.as_str().rsplit('.').next().map(str::to_string)
                            })
                            .collect();
                        variants.sort();
                        variants.dedup();
                        Err(VariantMiss::UnknownVariant {
                            ty: expected.clone(),
                            variants,
                        })
                    }
                }
            }
            other => Err(VariantMiss::NotVariantType(other.clone())),
        }
    }

    fn report_variant_miss(&mut self, name: Name, miss: VariantMiss, span: Span) {
        let (message, note) = match miss {
            VariantMiss::Recovery => return,
            VariantMiss::NoExpectedType => (
                format!("`.{name}` has no expected enum or error family type to select a variant from"),
                Some(
                    "qualify the variant, or give the target a type: an annotation, a parameter, or a declared return type; `.name` reads an item only inside a stream stage block"
                        .to_string(),
                ),
            ),
            VariantMiss::Uninferred => (
                format!("`.{name}` needs an expected type that is already known"),
                Some("annotate the binding or qualify the variant".to_string()),
            ),
            VariantMiss::CommonError(ty) => (
                format!("the expected type `{ty}` names no single error family, so `.{name}` is ambiguous"),
                Some(
                    "qualify the variant (`Family.Name(...)`), or declare the family in the type, as in `Result[T, Family]`"
                        .to_string(),
                ),
            ),
            VariantMiss::NotVariantType(ty) => (
                format!("the expected type `{ty}` is not an enum or error family, so `.{name}` names no variant"),
                None,
            ),
            VariantMiss::UnknownVariant { ty, variants } => {
                let note = super::nearest_name(&name.as_str(), variants.iter())
                    .map(|nearby| format!("did you mean `.{nearby}`?"))
                    .or_else(|| Some(format!("its variants are {}", variants.iter().map(|variant| format!("`{variant}`")).collect::<Vec<_>>().join(", "))));
                (format!("`{ty}` has no variant `{name}`"), note)
            }
            VariantMiss::NotVisible(ty) => (
                format!("the variants of `{ty}` are not visible here"),
                Some("import the module that declares it, or qualify the variant".to_string()),
            ),
        };
        let mut diagnostic = Diagnostic::error(message)
            .with_code(DiagnosticCode::CheckInferredVariant)
            .with_label(Label::primary(span, "target-typed variant"));
        if let Some(note) = note {
            diagnostic = diagnostic.with_note(note);
        }
        self.diagnostics.push(diagnostic);
    }

    /// Records the qualifier of a qualified variant pattern when the type of
    /// the matched value selects the same variant, so `.Name` is the same
    /// pattern. `span` is the pattern's, which begins with the qualifier.
    pub(super) fn note_pattern_qualifier(
        &mut self,
        kind: &ArenaPatternKind,
        value_ty: &Type,
        span: Span,
    ) {
        self.redundant_variant_qualifiers.remove(&span);
        let (qualifier, variant) = match kind {
            ArenaPatternKind::ErrorVariant {
                family, variant, ..
            } => (family.to_string(), *variant),
            ArenaPatternKind::Constructor { name, .. } | ArenaPatternKind::TestName { name, .. } => {
                let spelled = name.as_str();
                let Some((qualifier, variant)) = spelled.rsplit_once('.') else {
                    return;
                };
                (qualifier.to_string(), Name::intern(variant))
            }
            _ => return,
        };
        let spelled = Name::intern(format!("{qualifier}.{variant}"));
        let family = Name::intern(&qualifier);
        let same = match self.select_inferred_variant(variant, Some(value_ty)) {
            Ok(InferredVariant::Tag { type_name, .. }) => self
                .tag_variants
                .get(&spelled)
                .is_some_and(|info| info.type_name == type_name),
            Ok(InferredVariant::Error {
                family: selected, ..
            }) => {
                selected == family
                    && !self.tag_variants.contains_key(&spelled)
                    && self
                        .error_families
                        .get(&family)
                        .is_some_and(|info| info.variants.contains_key(&variant))
            }
            Err(_) => false,
        };
        if same {
            self.redundant_variant_qualifiers.insert(
                span,
                Span::new(span.source_id, span.start(), span.start() + qualifier.len()),
            );
        }
    }

    /// Resolves a `.Name` pattern against the type of the value it matches
    /// and publishes the qualified pattern it stands for. `kind` is the
    /// pattern as parsed: a constructor named `.Name`, or an error variant
    /// with an empty family. Returns the qualified kind, or `None` after
    /// reporting why the type selects no such pattern.
    pub(super) fn resolve_inferred_variant_pattern(
        &mut self,
        kind: &ArenaPatternKind,
        value_ty: &Type,
        span: Span,
    ) -> Option<ArenaPatternKind> {
        self.inferred_variant_patterns.remove(&span);
        let (name, arg, fields) = match kind {
            ArenaPatternKind::Constructor { name, arg } => (
                Name::intern(name.as_str().trim_start_matches('.')),
                Some(*arg),
                None,
            ),
            ArenaPatternKind::ErrorVariant {
                variant, fields, ..
            } => (*variant, None, Some(*fields)),
            _ => return None,
        };
        let variant = match self.select_inferred_variant(name, Some(value_ty)) {
            Ok(variant) => variant,
            Err(miss) => {
                let reported = self.diagnostics.len();
                self.report_variant_miss(name, miss, span);
                if let Some(diagnostic) = self.diagnostics.get_mut(reported) {
                    diagnostic.notes.push(format!(
                        "a `.{name}` pattern takes its enum or error family from the matched value, which has type `{value_ty}`"
                    ));
                    // A union gives no member priority, here as for a
                    // `.Name` value: the member is tested first.
                    if matches!(value_ty, Type::Union(_)) {
                        diagnostic.notes.push(
                            "test the member first, as in `value is Family`; the narrowed value then selects the variant"
                                .to_string(),
                        );
                    }
                }
                return None;
            }
        };
        let (resolved, fact) = match variant {
            InferredVariant::Tag {
                type_name,
                field_types,
                ..
            } => {
                if fields.is_some_and(|fields| fields.len != 0) {
                    self.error(
                        span,
                        &format!("`.{name}` is an enum variant; its payload is matched as `.{name}(...)`"),
                        DiagnosticCode::CheckPatternConstructor,
                    );
                    return None;
                }
                if arg.is_none() && !field_types.is_empty() {
                    self.error(
                        span,
                        &format!(
                            "variant `.{name}` has {} payload value(s); match them as `.{name}(...)`",
                            field_types.len()
                        ),
                        DiagnosticCode::CheckPatternArity,
                    );
                    return None;
                }
                // The spelling a qualified pattern would use here: the bare
                // constructor when it is visible, else the first namespace's.
                let mut spellings: Vec<Name> = self
                    .tag_variants
                    .iter()
                    .filter(|(spelling, info)| {
                        info.type_name == type_name
                            && spelling.as_str().rsplit('.').next() == Some(name.as_str().as_str())
                    })
                    .map(|(spelling, _)| *spelling)
                    .collect();
                spellings.sort_by_key(|spelling| {
                    let spelling = spelling.as_str();
                    (spelling.contains('.'), spelling.to_string())
                });
                let constructor = *spellings.first()?;
                let resolved = match (arg, constructor.as_str().split_once('.')) {
                    (Some(arg), _) => ArenaPatternKind::Constructor {
                        name: constructor,
                        arg,
                    },
                    (None, Some((namespace, variant))) => ArenaPatternKind::ErrorVariant {
                        family: Name::intern(namespace),
                        variant: Name::intern(variant),
                        fields: fields.unwrap_or_default(),
                    },
                    (None, None) => ArenaPatternKind::Binding(constructor),
                };
                (resolved, InferredVariantPattern::Tag { constructor })
            }
            InferredVariant::Error { family, variant } => {
                let Some(fields) = fields else {
                    self.error(
                        span,
                        &format!("`.{name}` is an error variant; its fields are matched as `.{name} {{...}}`"),
                        DiagnosticCode::CheckPatternConstructor,
                    );
                    return None;
                };
                (
                    ArenaPatternKind::ErrorVariant {
                        family,
                        variant,
                        fields,
                    },
                    InferredVariantPattern::Error { family },
                )
            }
        };
        self.inferred_variant_patterns.insert(span, fact);
        Some(resolved)
    }

    /// A bare `.Name` outside a stream stage block.
    pub(super) fn check_inferred_variant_value(
        &mut self,
        name: Name,
        expected: Option<&Type>,
        span: Span,
    ) -> Type {
        self.inferred_variants.remove(&span);
        let variant = match self.select_inferred_variant(name, expected) {
            Ok(variant) => variant,
            Err(miss) => {
                self.report_variant_miss(name, miss, span);
                return Type::Invalid;
            }
        };
        match &variant {
            InferredVariant::Tag {
                type_name,
                field_types,
                ..
            } => {
                let ty = Type::Tag(*type_name);
                if !field_types.is_empty() {
                    self.error(
                        span,
                        &format!(
                            "variant `.{name}` expects {} argument(s); call it as `.{name}(...)`",
                            field_types.len()
                        ),
                        DiagnosticCode::CheckArity,
                    );
                    return ty;
                }
                self.inferred_variants.insert(span, variant);
                ty
            }
            InferredVariant::Error { family, .. } => {
                let ty = Type::ErrorFamily(*family);
                self.error(
                    span,
                    &format!("error variants are constructed with a call: `.{name}(...)`"),
                    DiagnosticCode::CheckErrorConstructor,
                );
                ty
            }
        }
    }

    /// `.Name(args)` outside a stream stage block; `span` is the call's.
    pub(super) fn check_inferred_variant_call(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        name: Name,
        args: &[ArenaCallArg],
        span: Span,
        expected: Option<&Type>,
    ) -> Type {
        self.inferred_variants.remove(&span);
        let variant = match self.select_inferred_variant(name, expected) {
            Ok(variant) => variant,
            Err(miss) => {
                self.report_variant_miss(name, miss, span);
                for arg in args {
                    self.check_call_arg_arena(arena, source, &arg.kind, None);
                }
                return Type::Invalid;
            }
        };
        let ty = match &variant {
            InferredVariant::Tag {
                type_name,
                field_types,
                ..
            } => self.check_tag_constructor_args_arena(
                arena,
                source,
                &format!(".{name}"),
                *type_name,
                field_types,
                args,
                span,
            ),
            InferredVariant::Error { family, variant } => {
                self.check_error_variant_constructor_arena(
                    arena, source, *family, *variant, args, span,
                )
            }
        };
        self.inferred_variants.insert(span, variant);
        ty
    }

    /// Positional tag constructor arguments against the variant's fields.
    pub(super) fn check_tag_constructor_args_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        display: &str,
        type_name: Name,
        field_types: &[Type],
        args: &[ArenaCallArg],
        span: Span,
    ) -> Type {
        if args.len() != field_types.len() {
            self.error(
                span,
                &format!(
                    "tag constructor `{display}` expects {} argument(s), got {}",
                    field_types.len(),
                    args.len()
                ),
                DiagnosticCode::CheckArity,
            );
        }
        for (arg, expected_ty) in args.iter().zip(field_types.iter()) {
            let actual = self.check_call_arg_arena(arena, source, &arg.kind, Some(expected_ty));
            self.expect_type(
                expected_ty,
                &actual,
                super::call_arg_span_arena(arena, &arg.kind),
            );
        }
        Type::Tag(type_name)
    }

    /// Records that the qualified constructor at `expr_span` could drop its
    /// qualifier: `.variant` would select the same declaration from
    /// `expected`. `callee_span` covers the qualified name, which ends with
    /// `.variant`. Outside a stream stage block only, where `.variant` is not
    /// an item read.
    pub(super) fn note_variant_qualifier(
        &mut self,
        expr_span: Span,
        callee_span: Span,
        variant: Name,
        selected: &InferredVariant,
        expected: Option<&Type>,
    ) {
        self.redundant_variant_qualifiers.remove(&expr_span);
        if self.item_shorthand_in_scope() {
            return;
        }
        let same = match (self.select_inferred_variant(variant, expected), selected) {
            (
                Ok(InferredVariant::Tag {
                    type_name,
                    variant: name,
                    ..
                }),
                InferredVariant::Tag {
                    type_name: selected_type,
                    variant: selected_name,
                    ..
                },
            ) => type_name == *selected_type && name == *selected_name,
            (Ok(inferred @ InferredVariant::Error { .. }), selected) => inferred == *selected,
            _ => false,
        };
        let name_len = variant.as_str().len();
        if !same || callee_span.end() - callee_span.start() <= name_len + 1 {
            return;
        }
        // The qualifier ends at the dot before the variant name.
        let qualifier_end = callee_span.end() - name_len - 1;
        self.redundant_variant_qualifiers.insert(
            expr_span,
            Span::new(callee_span.source_id, callee_span.start(), qualifier_end),
        );
    }
}

#[cfg(test)]
mod pattern_tests {
    use crate::sema::check::{Checker, InferredVariantPattern};
    use crate::source::SourceId;
    use crate::syntax::parser::Parser;

    /// What each `.Name` pattern resolved to, in source order.
    fn resolved(source: &str) -> Vec<String> {
        let program = Parser::parse_source_arena_only(SourceId::new(0), source).arena;
        program.symbol_owner().with_current(|| {
            Checker::check_arena(&program, source)
                .inferred_variant_patterns
                .values()
                .map(|pattern| match pattern {
                    InferredVariantPattern::Tag { constructor } => format!("tag {constructor}"),
                    InferredVariantPattern::Error { family } => format!("error {family}"),
                })
                .collect()
        })
    }

    const PRELUDE: &str = "enum Level { Info, Fault(Str) }\n\
        error E = Usage | Other(code: Int)\n\
        pure narrow() -> Result[Level, E] { Ok(Info) }\n\
        proc broad() -> Result[Level] { Ok(Info) }\n";

    #[test]
    fn checker_publishes_what_a_variant_pattern_stands_for() {
        let source = format!(
            "{PRELUDE}let a = narrow() is Ok(.Info)\n\
             let b = narrow() is Ok(.Fault(_))\n\
             let c = narrow() is Err(.Usage)\n\
             let d = narrow() is Err(.Other {{code: 1}})\n"
        );
        assert_eq!(
            resolved(&source),
            ["tag Info", "tag Fault", "error E", "error E"]
        );
    }

    #[test]
    fn unresolved_variant_patterns_publish_nothing() {
        for test in [
            "broad() is Err(.Usage)",
            "narrow() is Ok(.Missing)",
            "narrow() is Ok(.Fault)",
            "narrow() is Err(.Other(_))",
        ] {
            let source = format!("{PRELUDE}let a = {test}\n");
            assert!(resolved(&source).is_empty(), "{test} published a pattern");
        }
    }
}
