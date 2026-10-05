#![allow(clippy::single_call_fn)]

//! Target-typed variants: `.Name` and `.Name(args)` select a variant of the
//! one enum or error family the expected type names. Stream stages and
//! implicit item callbacks use `.` for their item, so `.name` there stays a
//! field read.

use super::{Checker, Diagnostic, InferredVariant, Label, Name, Span, TagVariantInfo, Type};
use crate::diagnostic::DiagnosticCode;
use crate::syntax::arena::{ArenaCallArg, ArenaExprKind, ArenaProgram, ExprId};

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
    /// Reserve a leading dot for item syntax in a stream stage or an implicit
    /// one-item callback; elsewhere it may name a target-typed variant.
    pub(super) fn item_shorthand_in_scope(&self) -> bool {
        self.item_frames.last().is_some_and(|frame| {
            frame.is_stage() || matches!(frame, super::ItemFrame::Implicit { .. })
        })
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
    /// `.variant`. Only offer the fix outside item-shorthand scopes, where
    /// `.variant` is not an item read.
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
