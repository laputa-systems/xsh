use super::Diagnostic;
use super::{Binding, Checker, FxHashSet, Name, Span, Type, result_types};
use crate::diagnostic::DiagnosticCode;
use crate::syntax::arena::{ArenaPatternKind, ArenaProgram, PatternId};

fn type_pattern_input_is_dynamic(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Any | Type::ErasedRecord | Type::Unknown | Type::Invalid
    )
}

/// Arena-native mirror of `check_pattern` and its callees. Fully self-contained:
/// `check_pattern` (unlike `check_match`, the statement-level match in
/// stmt.rs) never calls `check_block` — only `check_expr`/type annotations on
/// leaf sub-expressions, so no `Block` support is needed to port it.
#[allow(dead_code)]
impl Checker {
    pub(super) fn check_nonbinding_pattern_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        pattern: PatternId,
        value_ty: &Type,
    ) {
        self.reject_pattern_test_bindings(arena, pattern, false);
        self.push_scope();
        self.check_pattern_arena(arena, source, pattern, value_ty);
        self.pop_scope();
    }

    fn reject_pattern_test_bindings(
        &mut self,
        arena: &ArenaProgram,
        pattern: PatternId,
        grouped: bool,
    ) {
        let node = arena.arena.pattern(pattern);
        let span = arena.arena.span(node.span);
        match &node.kind {
            ArenaPatternKind::Binding(name)
                if self
                    .tag_variants
                    .get(name)
                    .is_some_and(|info| info.field_count == 0) => {}
            ArenaPatternKind::Binding(_)
            | ArenaPatternKind::Type {
                binding: Some(_), ..
            } => {
                self.error(
                    span,
                    "pattern tests cannot bind names; use `_` or a non-binding pattern",
                    DiagnosticCode::CheckPatternTestBinding,
                );
            }
            ArenaPatternKind::Alias { .. } => self.error(
                span,
                "pattern tests cannot contain aliases",
                DiagnosticCode::CheckPatternTestBinding,
            ),
            ArenaPatternKind::Group(child) => {
                self.reject_pattern_test_bindings(arena, *child, true)
            }
            ArenaPatternKind::Alternation(children) => {
                if !grouped {
                    self.error(
                        span,
                        "group alternatives in a pattern test",
                        DiagnosticCode::CheckPatternTestAlternation,
                    );
                }
                for child in arena.arena.pattern_ids(*children) {
                    self.reject_pattern_test_bindings(arena, child, false);
                }
            }
            ArenaPatternKind::Constructor { arg: Some(arg), .. } => {
                self.reject_pattern_test_bindings(arena, *arg, false)
            }
            ArenaPatternKind::Record { fields, .. }
            | ArenaPatternKind::ErrorVariant { fields, .. } => {
                for field in arena.arena.pattern_fields(*fields) {
                    self.reject_pattern_test_bindings(arena, field.pattern, false);
                }
            }
            ArenaPatternKind::List { elements, rest } => {
                for child in arena
                    .arena
                    .pattern_ids(*elements)
                    .chain(rest.iter().copied())
                {
                    self.reject_pattern_test_bindings(arena, child, false);
                }
            }
            ArenaPatternKind::Tuple(patterns) | ArenaPatternKind::Text(patterns) => {
                for pattern in arena.arena.pattern_ids(*patterns) {
                    self.reject_pattern_test_bindings(arena, pattern, false);
                }
            }
            ArenaPatternKind::TextHole {
                binding: Some(_), ..
            } => {
                self.error(
                    span,
                    "pattern tests cannot bind names; write the hole as `{_}`",
                    DiagnosticCode::CheckPatternTestBinding,
                );
            }
            _ => {}
        }
    }

    pub(super) fn pattern_test_narrowed_type(
        &self,
        arena: &ArenaProgram,
        pattern: PatternId,
    ) -> Option<Type> {
        match arena.arena.pattern(pattern).kind {
            ArenaPatternKind::Group(child) => self.pattern_test_narrowed_type(arena, child),
            ArenaPatternKind::Alternation(children) => {
                let mut children = arena.arena.pattern_ids(children);
                let common = self.pattern_test_narrowed_type(arena, children.next()?)?;
                children
                    .all(|child| {
                        self.pattern_test_narrowed_type(arena, child)
                            .is_some_and(|ty| ty == common)
                    })
                    .then_some(common)
            }
            ArenaPatternKind::TestName { .. } | ArenaPatternKind::Type { binding: None, .. } => {
                self.pattern_test_types.get(&pattern).cloned()
            }
            ArenaPatternKind::ErrorVariant {
                family, variant, ..
            } => {
                let node = arena.arena.pattern(pattern);
                if !node.kind.is_inferred_variant() {
                    return Some(Type::ErrorVariant { family, variant });
                }
                // A target-typed variant narrows as the family it resolved to.
                match self
                    .inferred_variant_patterns
                    .get(&arena.arena.span(node.span))?
                {
                    super::InferredVariantPattern::Error { family } => Some(Type::ErrorVariant {
                        family: *family,
                        variant,
                    }),
                    super::InferredVariantPattern::Tag { .. } => None,
                }
            }
            ArenaPatternKind::Facet(facet) => Some(Type::ErrorFacet(facet)),
            _ => None,
        }
    }

    /// The types a pattern made only of type tests checks for, one per
    /// alternative, or `None` when an alternative is anything else. Such a
    /// pattern matches exactly the values of those types, whether or not a
    /// test also binds a name, so it decides both branches for its subject.
    pub(super) fn pattern_test_member_types(
        &self,
        arena: &ArenaProgram,
        pattern: PatternId,
    ) -> Option<Vec<Type>> {
        match arena.arena.pattern(pattern).kind {
            ArenaPatternKind::Group(child) => self.pattern_test_member_types(arena, child),
            ArenaPatternKind::Alternation(children) => {
                let mut types = Vec::new();
                for child in arena.arena.pattern_ids(children) {
                    types.extend(self.pattern_test_member_types(arena, child)?);
                }
                Some(types)
            }
            ArenaPatternKind::TestName { .. } | ArenaPatternKind::Type { .. } => {
                Some(vec![self.pattern_test_types.get(&pattern).cloned()?])
            }
            _ => None,
        }
    }

    pub(super) fn define_pattern_binding(&mut self, name: Name, ty: Type, span: Span) {
        if self.current_scope().contains_key(&name) {
            self.error(
                span,
                "duplicate name in pattern",
                DiagnosticCode::CheckPatternBinding,
            );
            return;
        }
        self.define(name, Binding::new(ty, false), span);
    }

    fn check_error_facet_applicability(&mut self, facet: Name, value_ty: &Type, span: Span) {
        // Qualification resolves the module; facets retain their declared identity.
        let facet = facet
            .as_str()
            .rsplit_once('.')
            .map_or(facet, |(_, member)| Name::intern(member));
        let applicable = match value_ty {
            Type::ErrorFamily(family) => self.error_families.get(family).is_none_or(|family| {
                family
                    .variants
                    .values()
                    .any(|variant| variant.facets.contains(&facet))
            }),
            Type::ErrorVariant { family, variant } => self
                .error_families
                .get(family)
                .and_then(|family| family.variants.get(variant))
                .is_none_or(|variant| variant.facets.contains(&facet)),
            _ => true,
        };
        if !applicable {
            self.error(
                span,
                "error facet pattern does not match value type",
                DiagnosticCode::CheckPatternType,
            );
        }
    }

    /// Facets are the built-in vocabulary plus the ones visible `error`
    /// declarations implement; a misspelling names the nearest one.
    fn report_unknown_error_facet(&mut self, facet: Name, span: Span) {
        let unknown = facet.as_str();
        let unknown: &str = unknown.as_ref();
        let mut diagnostic = Diagnostic::error(format!("unknown error facet `{unknown}`"))
            .with_code(DiagnosticCode::CheckPatternConstructor)
            .with_label(super::Label::primary(span, "unknown error facet"));
        if let Some(nearby) = super::method::nearest_name(
            unknown,
            self.error_facets
                .iter()
                .map(|known| known.as_str().to_string()),
        ) {
            diagnostic = diagnostic.with_note(format!("did you mean `{nearby}`?"));
        }
        self.diagnostics.push(diagnostic);
    }

    fn check_type_pattern_applicability(&mut self, tested: &Type, value_ty: &Type, span: Span) {
        if self.reject_typed_callable_test(tested, span) {
            return;
        }
        if type_pattern_input_is_dynamic(value_ty) {
            return;
        }
        // A union is tested for listed members only, one or several at a
        // time: the test then tells the checker exactly which members remain
        // when it fails.
        if let Type::Union(members) = value_ty {
            let listed =
                |tested: &Type| members.iter().any(|member| member.matches_invariant(tested));
            let names_members = match tested {
                Type::Union(tested) => tested.iter().all(listed),
                tested => listed(tested),
            };
            if !tested.is_recovery() && !names_members {
                self.error(
                    span,
                    &format!("`{tested}` is not a member of {value_ty}"),
                    DiagnosticCode::CheckPatternType,
                );
            }
            return;
        }
        // A value of a validated type's base may be tested for the
        // validation: the test is the validation's one runtime check, and it
        // narrows the value where it passes. A value that already has the
        // type passes the same test.
        if let Type::Validated(validated) = tested
            && value_ty.unvalidated().matches_invariant(validated.base())
        {
            return;
        }
        let family = match tested {
            Type::ErrorFamily(family) => Some(*family),
            Type::ProcessError => Some(Name::PROCESS_ERROR),
            Type::Error => None,
            _ => {
                self.error(
                    span,
                    "type patterns require a dynamic value",
                    DiagnosticCode::CheckPatternType,
                );
                return;
            }
        };
        // A nominal error selector preserves a statically known family. The
        // broad error carrier permits selection, but unrelated families cannot match.
        let applicable = match value_ty {
            Type::Error | Type::ErrorFacet(_) => true,
            Type::ErrorFamily(actual) | Type::ErrorVariant { family: actual, .. } => {
                family.is_none_or(|family| family == *actual)
            }
            Type::ProcessError => family.is_none_or(|family| family == Name::PROCESS_ERROR),
            _ => false,
        };
        if !applicable {
            self.error(
                span,
                "error type pattern does not match value type",
                DiagnosticCode::CheckPatternType,
            );
        }
    }

    /// Checks what a `.Name` pattern binds after its variant failed to
    /// resolve, so the names it introduces are still defined.
    fn check_unresolved_variant_subpatterns(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        kind: &ArenaPatternKind,
    ) {
        match kind {
            ArenaPatternKind::Constructor { arg: Some(arg), .. } => {
                self.check_pattern_arena(arena, source, *arg, &Type::Unknown);
            }
            ArenaPatternKind::ErrorVariant { fields, .. } => {
                for field in arena.arena.pattern_fields(*fields) {
                    self.check_pattern_arena(arena, source, field.pattern, &Type::Unknown);
                }
            }
            _ => {}
        }
    }

    pub(super) fn check_pattern_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        pattern_id: PatternId,
        value_ty: &Type,
    ) {
        let pattern = arena.arena.pattern(pattern_id);
        let span = arena.arena.span(pattern.span);
        // A `.Name` pattern is checked as the qualified pattern the matched
        // value's type selects.
        let qualified;
        let kind = if pattern.kind.is_inferred_variant() {
            let Some(resolved) =
                self.resolve_inferred_variant_pattern(&pattern.kind, value_ty, span)
            else {
                self.check_unresolved_variant_subpatterns(arena, source, &pattern.kind);
                return;
            };
            qualified = resolved;
            &qualified
        } else {
            self.note_pattern_qualifier(&pattern.kind, value_ty, span);
            &pattern.kind
        };
        match kind {
            ArenaPatternKind::Group(child) => {
                self.check_pattern_arena(arena, source, *child, value_ty)
            }
            ArenaPatternKind::Alias {
                pattern,
                name,
                name_span,
            } => {
                self.check_pattern_arena(arena, source, *pattern, value_ty);
                self.define_pattern_binding(*name, value_ty.clone(), arena.arena.span(*name_span));
            }
            ArenaPatternKind::Wildcard => {}
            ArenaPatternKind::TestName { name, ty } => {
                if name == "Ok" || name == "Err" {
                    if self.type_defs.contains_key(name)
                        || self.error_facets.contains(name)
                        || self.tag_variants.contains_key(name)
                    {
                        self.error(
                            span,
                            "ambiguous pattern test name; qualify the type or constructor",
                            DiagnosticCode::CheckPatternTestAmbiguous,
                        );
                    }
                    if let Some((ok, err)) = result_types(value_ty) {
                        let target = if name == "Ok" { ok } else { err };
                        if !matches!(target, Type::Unit | Type::Unknown) {
                            self.error(
                                span,
                                "constructor pattern needs an argument for this Result type",
                                DiagnosticCode::CheckPatternArity,
                            );
                        }
                    } else if !matches!(value_ty, Type::Any | Type::Unknown) {
                        self.error(
                            span,
                            "constructor patterns require a Result value",
                            DiagnosticCode::CheckPatternType,
                        );
                    }
                    return;
                }
                let text = name.as_str();
                if let Some((family, variant)) = text.rsplit_once('.') {
                    let family = Name::intern(family);
                    let variant = Name::intern(variant);
                    if self
                        .error_families
                        .get(&family)
                        .is_some_and(|info| info.variants.contains_key(&variant))
                    {
                        if self.tag_variants.contains_key(name)
                            || self.error_facets.contains(name)
                            || self
                                .type_namespaces
                                .get(&family)
                                .is_some_and(|types| types.contains_key(&variant))
                        {
                            self.error(
                                span,
                                "ambiguous pattern test name; qualify the type or constructor",
                                DiagnosticCode::CheckPatternTestAmbiguous,
                            );
                        }
                        self.pattern_test_types
                            .insert(pattern_id, Type::ErrorVariant { family, variant });
                        let applicable = match value_ty {
                            Type::Any | Type::Unknown | Type::Error | Type::ProcessError => true,
                            Type::ErrorFamily(actual) => *actual == family,
                            Type::ErrorVariant {
                                family: actual_family,
                                variant: actual_variant,
                            } => *actual_family == family && *actual_variant == variant,
                            _ => false,
                        };
                        if !applicable {
                            self.error(
                                span,
                                "error variant pattern does not match value type",
                                DiagnosticCode::CheckPatternType,
                            );
                        }
                        return;
                    }
                }
                let is_type = Type::builtin_from_name(&name.as_str()).is_some()
                    || super::standard_record_type(&name.as_str()).is_some()
                    || self.type_defs.contains_key(name)
                    || self.error_families.contains_key(name)
                    || text.rsplit_once('.').is_some_and(|(namespace, member)| {
                        self.type_namespaces
                            .get(&Name::intern(namespace))
                            .is_some_and(|types| types.contains_key(&Name::intern(member)))
                    });
                let is_facet = self.error_facets.contains(name);
                let constructor = self.tag_variants.get(name).cloned();
                if usize::from(is_type) + usize::from(is_facet) + usize::from(constructor.is_some())
                    > 1
                {
                    self.error(
                        span,
                        "ambiguous pattern test name; use a qualified type, facet, or constructor",
                        DiagnosticCode::CheckPatternTestAmbiguous,
                    );
                } else if let Some(info) = constructor {
                    if info.field_count != 0 {
                        self.error(
                            span,
                            "constructor pattern needs arguments; use `_` for payloads",
                            DiagnosticCode::CheckPatternArity,
                        );
                    }
                    if !matches!(value_ty, Type::Any | Type::Unknown)
                        && !matches!(value_ty, Type::Tag(t) if t == &info.type_name)
                    {
                        self.error(
                            span,
                            "constructor does not match subject type",
                            DiagnosticCode::CheckPatternType,
                        );
                    }
                } else {
                    let tested = self.type_from_arena(arena, *ty);
                    self.pattern_test_types.insert(pattern_id, tested.clone());
                    if let Type::ErrorFacet(facet) = tested {
                        self.check_error_facet_applicability(facet, value_ty, span);
                    }
                    if !matches!(tested, Type::ErrorFacet(_)) {
                        self.check_type_pattern_applicability(&tested, value_ty, span);
                    }
                    if matches!(tested, Type::ErrorFacet(_))
                        && !matches!(
                            value_ty,
                            Type::Any
                                | Type::Unknown
                                | Type::Error
                                | Type::ProcessError
                                | Type::ErrorFamily(_)
                                | Type::ErrorVariant { .. }
                                | Type::ErrorFacet(_)
                        )
                    {
                        self.error(
                            span,
                            "error facet patterns require an error value",
                            DiagnosticCode::CheckPatternType,
                        );
                    }
                }
            }

            ArenaPatternKind::Binding(name) => {
                if let Some(info) = self.tag_variants.get(name).cloned()
                    && info.field_count == 0
                {
                    if !matches!(value_ty, Type::Any | Type::Unknown)
                        && !matches!(value_ty, Type::Tag(t) if t == &info.type_name)
                    {
                        self.error(
                            span,
                            &format!(
                                "tag pattern `{name}` is for type `{}`, but value has type `{value_ty}`",
                                info.type_name
                            ),
                            DiagnosticCode::CheckPatternType,
                        );
                    }
                    return;
                }
                // A capitalized name that is not a known variant is almost
                // always a mistyped variant or facet; binding it would silently
                // match every value.
                if name
                    .as_str()
                    .starts_with(|first: char| first.is_ascii_uppercase())
                {
                    self.error(
                        span,
                        &format!(
                            "`{name}` would bind a new name that matches anything; write `is {name}` for an error facet, `Family.{name}` for a variant, or a lowercase name to bind"
                        ),
                        DiagnosticCode::CheckPatternCapitalizedBinding,
                    );
                    return;
                }
                self.define_pattern_binding(*name, value_ty.clone(), span);
            }
            ArenaPatternKind::Type { binding, ty } => {
                let narrowed_ty = self.type_from_arena(arena, *ty);
                self.check_type_pattern_applicability(&narrowed_ty, value_ty, span);
                self.pattern_test_types
                    .insert(pattern_id, narrowed_ty.clone());
                if let Some(name) = binding {
                    self.define_pattern_binding(*name, narrowed_ty, span);
                }
            }
            ArenaPatternKind::Literal(expr) => {
                let actual = self.check_expr_arena(arena, source, *expr, Some(value_ty));
                let expr_span = arena.arena.expr(*expr).span;
                self.expect_type(value_ty, &actual, expr_span);
            }
            ArenaPatternKind::List { elements, rest } => {
                let element_ty = match value_ty.unvalidated() {
                    Type::List(element) => element.as_ref().clone(),
                    Type::Any => Type::Any,
                    Type::Unknown | Type::Invalid => Type::Unknown,
                    _ => {
                        self.error(
                            span,
                            "list patterns require a List value",
                            DiagnosticCode::CheckPatternType,
                        );
                        Type::Unknown
                    }
                };
                for child in arena.arena.pattern_ids(*elements) {
                    self.check_pattern_arena(arena, source, child, &element_ty);
                }
                if let Some(rest) = rest {
                    if !matches!(
                        arena.arena.pattern(*rest).kind,
                        ArenaPatternKind::Wildcard | ArenaPatternKind::Binding(_)
                    ) {
                        self.error(
                            arena.arena.span(arena.arena.pattern(*rest).span),
                            "list rest must be a wildcard or name",
                            DiagnosticCode::CheckPatternRest,
                        );
                    }
                    self.check_pattern_arena(
                        arena,
                        source,
                        *rest,
                        &Type::List(Box::new(element_ty)),
                    );
                }
            }
            ArenaPatternKind::Record { fields, .. } => {
                let record_fields = match value_ty {
                    Type::ErasedRecord => None,
                    Type::Record(fields) => Some(fields),
                    Type::Error | Type::ErrorFamily(_) | Type::ErrorVariant { .. } => {
                        self.error(
                            span,
                            "record matching on error fields was removed; match exact variants or facets instead",
                            DiagnosticCode::CheckErrorRemoved,
                        );
                        None
                    }
                    Type::ProcessError => None,
                    Type::Any | Type::Unknown => None,
                    _ => {
                        self.error(
                            span,
                            "record patterns require a record-like value",
                            DiagnosticCode::CheckPatternType,
                        );
                        None
                    }
                };
                let mut names = FxHashSet::default();
                for field in arena.arena.pattern_fields(*fields) {
                    let field_span = arena.arena.span(field.span);
                    if !names.insert(field.name) {
                        self.error(
                            field_span,
                            "duplicate pattern field",
                            DiagnosticCode::CheckPatternField,
                        );
                    }
                    let field_ty = record_fields
                        .and_then(|fields| fields.get(&field.name))
                        .cloned()
                        .unwrap_or(Type::Unknown);
                    if let Some(fields) = record_fields
                        && !fields.contains_key(&field.name)
                    {
                        self.error(
                            field_span,
                            "unknown pattern field",
                            DiagnosticCode::CheckPatternField,
                        );
                    }
                    self.check_pattern_arena(arena, source, field.pattern, &field_ty);
                }
            }
            ArenaPatternKind::Alternation(patterns) => {
                let existing = self.current_scope().clone();
                let mut common: Option<super::FxHashMap<Name, Binding>> = None;
                for sub_id in arena.arena.pattern_ids(*patterns) {
                    self.push_scope();
                    *self.current_scope_mut() = existing.clone();
                    self.check_pattern_arena(arena, source, sub_id, value_ty);
                    let captures: super::FxHashMap<_, _> = self
                        .current_scope()
                        .iter()
                        .filter(|(name, _)| !existing.contains_key(name))
                        .map(|(name, binding)| (*name, binding.clone()))
                        .collect();
                    self.pop_scope();
                    if let Some(common) = &common {
                        if common.len() != captures.len()
                            || common.iter().any(|(name, binding)| {
                                !captures
                                    .get(name)
                                    .is_some_and(|other| binding.ty == other.ty)
                            })
                        {
                            self.error(arena.arena.span(arena.arena.pattern(sub_id).span), "pattern alternatives must bind the same names with identical resolved types", DiagnosticCode::CheckPatternAlternativeBinding);
                        }
                    } else {
                        common = Some(captures);
                    }
                }
                for (name, binding) in common.unwrap_or_default() {
                    self.define_pattern_binding(name, binding.ty, span);
                }
            }
            ArenaPatternKind::Tuple(patterns) => {
                for sub_id in arena.arena.pattern_ids(*patterns) {
                    self.check_pattern_arena(arena, source, sub_id, &Type::Unknown);
                }
            }
            ArenaPatternKind::Text(parts) => {
                self.check_text_pattern_arena(arena, *parts, value_ty, span)
            }
            // A hole is checked with the text pattern that holds it.
            ArenaPatternKind::TextHole { .. } => {}
            ArenaPatternKind::Constructor { name, arg } => {
                if let Some(info) = self.tag_variants.get(name).cloned() {
                    if !matches!(value_ty, Type::Any | Type::Unknown)
                        && !matches!(value_ty, Type::Tag(t) if t == &info.type_name)
                    {
                        self.error(
                            span,
                            &format!(
                                "tag pattern `{name}` is for type `{}`, but value has type `{value_ty}`",
                                info.type_name
                            ),
                            DiagnosticCode::CheckPatternType,
                        );
                    }
                    // The parser wraps a constructor's arguments in a tuple
                    // pattern only when it parses more than one; a lone
                    // argument (or none) is the whole pattern. So the argument
                    // count is the tuple length, 1, or 0.
                    let args: Vec<PatternId> = match arg {
                        None => Vec::new(),
                        Some(arg) => match &arena.arena.pattern(*arg).kind {
                            ArenaPatternKind::Tuple(sub_patterns) => arena
                                .arena
                                .pattern_ids(*sub_patterns)
                                .collect(),
                            _ => vec![*arg],
                        },
                    };
                    if args.len() != info.field_count {
                        self.error(
                            span,
                            &format!(
                                "tag variant `{name}` expects {} argument(s), got {}",
                                info.field_count,
                                args.len()
                            ),
                            DiagnosticCode::CheckPatternArity,
                        );
                        for arg in args {
                            self.check_pattern_arena(arena, source, arg, &Type::Unknown);
                        }
                    } else {
                        for (arg, field_ty) in args.into_iter().zip(info.field_types.iter()) {
                            self.check_pattern_arena(arena, source, arg, field_ty);
                        }
                    }
                    return;
                }
                let Some((ok_ty, err_ty)) = result_types(value_ty) else {
                    if !matches!(value_ty, Type::Any | Type::Unknown) {
                        self.error(
                            span,
                            "constructor patterns require a Result value",
                            DiagnosticCode::CheckPatternType,
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
                            DiagnosticCode::CheckPatternConstructor,
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
                        DiagnosticCode::CheckPatternArity,
                    );
                }
            }
            ArenaPatternKind::ErrorVariant {
                family,
                variant,
                fields,
            } => {
                let qualified = Name::intern(format!("{family}.{variant}"));
                if fields.len == 0
                    && let Some(info) = self.tag_variants.get(&qualified).cloned()
                {
                    if info.field_count != 0 {
                        self.error(
                            span,
                            "constructor pattern needs arguments",
                            DiagnosticCode::CheckPatternArity,
                        );
                    }
                    if !matches!(value_ty, Type::Any | Type::Unknown)
                        && !matches!(value_ty, Type::Tag(name) if *name == info.type_name)
                    {
                        self.error(
                            span,
                            "constructor does not match subject type",
                            DiagnosticCode::CheckPatternType,
                        );
                    }
                    return;
                }
                let Some(variant_info) = self
                    .error_families
                    .get(family)
                    .and_then(|family| family.variants.get(variant))
                    .cloned()
                else {
                    self.error(
                        span,
                        "unknown error variant pattern",
                        DiagnosticCode::CheckPatternConstructor,
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
                        DiagnosticCode::CheckPatternType,
                    );
                }
                let mut names = FxHashSet::default();
                for field in arena.arena.pattern_fields(*fields) {
                    let field_span = arena.arena.span(field.span);
                    if !names.insert(field.name) {
                        self.error(
                            field_span,
                            "duplicate pattern field",
                            DiagnosticCode::CheckPatternField,
                        );
                    }
                    let Some(field_ty) = variant_info.fields.get(&field.name).cloned() else {
                        self.error(
                            field_span,
                            "unknown error payload field",
                            DiagnosticCode::CheckPatternField,
                        );
                        self.check_pattern_arena(arena, source, field.pattern, &Type::Unknown);
                        continue;
                    };
                    self.check_pattern_arena(arena, source, field.pattern, &field_ty);
                }
            }
            ArenaPatternKind::Facet(name) => {
                if self.error_facets.contains(name) {
                    self.check_error_facet_applicability(*name, value_ty, span);
                } else {
                    self.report_unknown_error_facet(*name, span);
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
                        DiagnosticCode::CheckPatternType,
                    );
                }
            }
        }
    }

    pub(super) fn check_list_match_coverage_arena(
        &mut self,
        arena: &ArenaProgram,
        value_ty: &Type,
        arms: impl Iterator<Item = (PatternId, Span, bool)>,
        span: Span,
    ) {
        let mut patterns = Vec::new();
        for (pattern, arm_span, guarded) in arms {
            if super::stmt::patterns_are_exhaustive_arena(
                arena,
                value_ty,
                patterns.iter().copied(),
                &self.exhaustiveness_facts(),
            ) {
                self.diagnostics.push(
                    Diagnostic::new(
                        crate::diagnostic::Severity::Warning,
                        "unreachable match arm",
                    )
                    .with_code(DiagnosticCode::CheckUnreachableMatchArm)
                    .with_label(crate::diagnostic::Label::secondary(
                        arm_span,
                        "earlier unguarded patterns cover every subject",
                    )),
                );
            }
            if !guarded {
                patterns.push(pattern);
            }
        }
        if matches!(value_ty.unvalidated(), Type::List(_))
            && !super::stmt::patterns_are_exhaustive_arena(
                arena,
                value_ty,
                patterns.into_iter(),
                &self.exhaustiveness_facts(),
            )
        {
            self.error(
                span,
                "list match requires a catchall or complete length partition",
                DiagnosticCode::CheckNonExhaustiveMatch,
            );
        }
    }

    /// The comma-separated members of a union subject that no unguarded arm
    /// tests for, or `None` when the subject is not a union or is covered.
    pub(super) fn missing_union_members_arena(
        &self,
        arena: &ArenaProgram,
        value_ty: &Type,
        arm_patterns: &[(PatternId, Span)],
    ) -> Option<String> {
        let members = value_ty.union_members()?;
        if super::stmt::patterns_are_exhaustive_arena(
            arena,
            value_ty,
            arm_patterns.iter().map(|(pattern, _)| *pattern),
            &self.exhaustiveness_facts(),
        ) {
            return None;
        }
        let mut tested = Vec::new();
        for (pattern, _) in arm_patterns {
            super::stmt::collect_tested_types_arena(
                arena,
                *pattern,
                &self.pattern_test_types,
                &mut tested,
            );
        }
        let missing = members
            .iter()
            .filter(|member| !tested.iter().any(|tested| member.matches_expected(tested)))
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        (!missing.is_empty()).then(|| missing.join(", "))
    }

    pub(super) fn exhaustiveness_facts(&self) -> super::stmt::ExhaustivenessFacts<'_> {
        super::stmt::ExhaustivenessFacts {
            type_defs: &self.type_defs,
            tag_variants: &self.tag_variants,
            pattern_types: &self.pattern_test_types,
            inferred_variant_patterns: &self.inferred_variant_patterns,
            error_families: &self.error_families,
        }
    }

    /// A statement `match` over an enum, a declared error family, or a union
    /// must handle every variant or member, or end in a catch-all: an
    /// unmatched value would fail at run time, and a case added later must
    /// not slip past an old match unnoticed.
    /// A value-producing `match` reports the same gap through
    /// `check.match-value-exhaustive` instead, so it calls
    /// `missing_tag_variants_arena` and `missing_error_variants_arena`
    /// directly and never gets both.
    pub(super) fn check_tag_exhaustiveness_arena(
        &mut self,
        arena: &ArenaProgram,
        value_ty: &Type,
        arm_patterns: Vec<(PatternId, Span)>,
        span: Span,
    ) {
        if let Some(missing) = self.missing_union_members_arena(arena, value_ty, &arm_patterns) {
            self.diagnostics.push(
                Diagnostic::error(format!(
                    "non-exhaustive match: missing member(s) `{missing}`"
                ))
                .with_code(DiagnosticCode::CheckNonExhaustiveMatch)
                .with_label(crate::diagnostic::Label::primary(
                    span,
                    format!("not every member of {value_ty} is handled"),
                ))
                .with_note(
                    "handle each missing member, or end the match with `else =>` as the deliberate catch-all",
                ),
            );
            return;
        }
        let (missing_list, label) =
            match self.missing_tag_variants_arena(arena, value_ty, &arm_patterns) {
                Some(missing) => (missing, "not every variant of this enum is handled"),
                None => match self.missing_error_variants_arena(arena, value_ty, &arm_patterns) {
                    Some(missing) => (
                        missing,
                        "not every variant of this error family is handled",
                    ),
                    None => return,
                },
            };
        self.diagnostics.push(
            Diagnostic::error(format!(
                "non-exhaustive match: missing variant(s) `{missing_list}`"
            ))
            .with_code(DiagnosticCode::CheckNonExhaustiveMatch)
            .with_label(crate::diagnostic::Label::primary(span, label))
            .with_note(
                "handle each missing variant, or end the match with `else =>` as the deliberate catch-all",
            ),
        );
    }

    /// The comma-separated enum variants that the unguarded arms leave some
    /// value of unmatched, or `None` when the value is not an enum whose
    /// variants are visible here or the arms cover it. A variant matched
    /// only with a payload pattern that can fail is missing.
    pub(super) fn missing_tag_variants_arena(
        &self,
        arena: &ArenaProgram,
        value_ty: &Type,
        arm_patterns: &[(PatternId, Span)],
    ) -> Option<String> {
        let Type::Tag(type_name) = value_ty else {
            return None;
        };
        let facts = self.exhaustiveness_facts();
        let variants = facts.enum_variants(*type_name)?;
        if super::stmt::patterns_are_exhaustive_arena(
            arena,
            value_ty,
            arm_patterns.iter().map(|(pattern, _)| *pattern),
            &facts,
        ) {
            return None;
        }
        let mut coverage = super::stmt::VariantCoverage::default();
        for (pattern, _) in arm_patterns {
            super::stmt::collect_variant_coverage(arena, *pattern, &facts, &mut coverage);
        }
        let missing: Vec<String> = variants
            .iter()
            .filter(|variant| !coverage.constructors.contains(variant))
            .map(ToString::to_string)
            .collect();
        (!missing.is_empty()).then(|| missing.join(", "))
    }

    /// The comma-separated variants of an error family subject that the
    /// unguarded arms leave some value of unmatched, or `None` when the value
    /// is not exactly one family whose declaration is visible here or the
    /// arms cover it. The broad `Error`, a facet, and a `Result` name no
    /// single family, so they are never enumerated. A variant is covered by
    /// a pattern for it whose field patterns cannot fail, or by a test for a
    /// facet it implements.
    pub(super) fn missing_error_variants_arena(
        &self,
        arena: &ArenaProgram,
        value_ty: &Type,
        arm_patterns: &[(PatternId, Span)],
    ) -> Option<String> {
        let Type::ErrorFamily(family) = value_ty else {
            return None;
        };
        let info = self.error_families.get(family)?;
        let facts = self.exhaustiveness_facts();
        if super::stmt::patterns_are_exhaustive_arena(
            arena,
            value_ty,
            arm_patterns.iter().map(|(pattern, _)| *pattern),
            &facts,
        ) {
            return None;
        }
        let mut coverage = super::stmt::VariantCoverage::default();
        for (pattern, _) in arm_patterns {
            super::stmt::collect_variant_coverage(arena, *pattern, &facts, &mut coverage);
        }
        let missing: Vec<String> = info
            .variants
            .iter()
            .filter(|(name, variant)| !coverage.covers_error_variant(**name, variant))
            .map(|(name, _)| name.to_string())
            .collect();
        (!missing.is_empty()).then(|| missing.join(", "))
    }

    /// A value-producing `match` has no value for an uncovered case, so the
    /// gap is an error; it names the missing enum variants when it can.
    pub(super) fn report_value_match_not_exhaustive(
        &mut self,
        arena: &ArenaProgram,
        value_ty: &Type,
        arm_patterns: &[(PatternId, Span)],
        span: Span,
    ) {
        let message = if let Some(missing) =
            self.missing_union_members_arena(arena, value_ty, arm_patterns)
        {
            format!("value-producing match must be exhaustive: missing member(s) `{missing}`")
        } else {
            match self
                .missing_tag_variants_arena(arena, value_ty, arm_patterns)
                .or_else(|| self.missing_error_variants_arena(arena, value_ty, arm_patterns))
            {
                Some(missing) => {
                    format!(
                        "value-producing match must be exhaustive: missing variant(s) `{missing}`"
                    )
                }
                None => "value-producing match must be exhaustive".to_string(),
            }
        };
        self.error(span, &message, DiagnosticCode::CheckMatchValueExhaustive);
    }
}
