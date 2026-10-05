#![allow(clippy::single_call_fn)]

use super::{
    BTreeMap, CallableParamType, CallableType, Checker, ContractParam, Diagnostic, Effect, FixHint,
    Label, ModuleContractEntryKind, ModuleExportType, Span, Type, TypeAnnRef, TypeDefBody,
};
use crate::diagnostic::DiagnosticCode;
use crate::sema::records::standard_record_type;
use crate::symbol::{Name, Symbol};
use crate::syntax::arena::{ArenaProgram, ArenaTypeExprTag, TypeExprId};
use xsh_registry::types::BuiltinTypeName;

/// The `.require(T)` spelling of a target type a dynamic-boundary fix can
/// name. Structural records, nominal errors, handles, and callables have no
/// spelling that is independent of the declaration in scope, so they get none.
fn require_target_spelling(target: &Type) -> Option<String> {
    let spelled = match target {
        Type::Bool
        | Type::Int
        | Type::UInt
        | Type::Float
        | Type::Duration
        | Type::Str
        | Type::Bytes
        | Type::Path => target.to_string(),
        Type::Tag(name) if !name.as_str().contains('.') => name.to_string(),
        Type::List(item) => format!("List[{}]", require_target_spelling(item)?),
        Type::Map(key, value) if **key == Type::Str => {
            format!("Map[{}]", require_target_spelling(value)?)
        }
        Type::Map(key, value) => format!(
            "Map[{}, {}]",
            require_target_spelling(key)?,
            require_target_spelling(value)?
        ),
        Type::Optional(inner) if !matches!(**inner, Type::Optional(_)) => {
            format!("{}?", require_target_spelling(inner)?)
        }
        Type::Any => "Any".to_owned(),
        _ => return None,
    };
    Some(spelled)
}

impl Checker {
    pub(super) fn check_propagation(&mut self, ty: &Type, span: Span) -> Type {
        if self.retry_attempt_depth > 0 {
            return self.check_attempt_propagation(ty, span);
        }
        if let Some(errors) = &mut self.with_initializer_errors
            && let Some((_, error)) = result_types(ty)
        {
            errors.push(error.clone());
        }
        if matches!(ty, Type::Unknown | Type::Invalid) {
            return ty.clone();
        }
        if matches!(ty, Type::Any) {
            self.check_opaque_callable_effects("opaque Result propagation", span);
            return Type::Any;
        }
        self.record_required_effect(Effect::Error);
        if let Some(effs) = &self.current_effects
            && !effs.contains(&Effect::Error)
        {
            self.error(
                span,
                "`?` requires the `error` effect",
                DiagnosticCode::CheckEffectViolation,
            );
        }
        let Some((ok, err)) = result_types(ty) else {
            self.error(
                span,
                "`?` can be applied only to Result values",
                DiagnosticCode::CheckTryResult,
            );
            return Type::Unknown;
        };
        let allowed = self
            .current_effects
            .as_ref()
            .is_some_and(|effects| effects.contains(&Effect::Error))
            || self
                .current_return
                .as_ref()
                .is_none_or(|return_ty| return_ty.is_result())
            || self.current_yield.is_some();
        let inferring =
            self.inferred_returns.is_some() && self.current_return == Some(Type::Unknown);
        if inferring {
            self.inferred_propagations.push((err.clone(), span));
        }
        if !allowed && !inferring {
            self.error(
                span,
                "`?` requires a Result-returning context",
                DiagnosticCode::CheckTryContext,
            );
        }
        if let Some(Type::Result(_, return_err)) = &self.current_return
            && !err.matches_expected(return_err)
        {
            self.diagnostics.push(
                Diagnostic::error("incompatible propagated error")
                    .with_code(DiagnosticCode::CheckTryError)
                    .with_label(Label::primary(
                        span,
                        format!("cannot propagate {err} from function returning {return_err}"),
                    )),
            );
        }
        ok
    }

    pub(super) fn check_attempt_propagation(&mut self, ty: &Type, span: Span) -> Type {
        if matches!(ty, Type::Unknown | Type::Invalid) {
            return ty.clone();
        }
        if matches!(ty, Type::Any) {
            return Type::Any;
        }
        let Some((ok, err)) = result_types(ty) else {
            self.error(
                span,
                "`?` can be applied only to Result values",
                DiagnosticCode::CheckTryResult,
            );
            return Type::Unknown;
        };
        if let Some(errors) = self.error_boundary_errors.last_mut() {
            errors.push((err, span));
        }
        ok
    }

    pub(super) fn begin_error_boundary(&mut self) {
        self.retry_attempt_depth += 1;
        self.error_boundary_errors.push(Vec::new());
    }

    pub(super) fn end_error_boundary(&mut self, expected: Option<&Type>) -> Type {
        self.retry_attempt_depth -= 1;
        let errors = self
            .error_boundary_errors
            .pop()
            .expect("checked error boundary");
        let mut inferred = expected.cloned();
        for (error, span) in errors {
            if let Some(current) = &inferred {
                if error.matches_expected(current) {
                    continue;
                }
                if expected.is_none() && current.matches_expected(&error) {
                    inferred = Some(error);
                    continue;
                }
                if expected.is_none() {
                    let family = match (current, &error) {
                        (
                            Type::ErrorVariant { family: left, .. } | Type::ErrorFamily(left),
                            Type::ErrorVariant { family: right, .. } | Type::ErrorFamily(right),
                        ) if left == right => Some(*left),
                        _ => None,
                    };
                    inferred = Some(family.map(Type::ErrorFamily).unwrap_or(Type::Error));
                } else {
                    self.expect_type(current, &error, span);
                }
            } else {
                inferred = Some(error);
            }
        }
        inferred.unwrap_or(Type::Error)
    }

    pub(super) fn record_callee_propagation(
        &mut self,
        effects: &Option<Vec<Effect>>,
        return_ty: &Type,
        span: Span,
    ) {
        if self.retry_attempt_depth > 0
            && !return_ty.is_result()
            && !matches!(return_ty, Type::Stream(_))
            && effects
                .as_ref()
                .is_some_and(|effects| effects.contains(&Effect::Error))
            && let Some(errors) = self.error_boundary_errors.last_mut()
        {
            errors.push((Type::Error, span));
        }
    }

    /// A pure function reports failure only through its `Result`. A statement
    /// that propagates outside any capture needs that `Result` exactly as `?`
    /// does; otherwise the failure would unwind out of a function whose
    /// signature says it cannot fail. A proc is held to its `error` effect
    /// instead.
    fn require_pure_propagation_context(&mut self, error: &Type, span: Span) {
        if !self.in_pure || self.current_yield.is_some() {
            return;
        }
        if self.inferred_returns.is_some() && self.current_return == Some(Type::Unknown) {
            self.inferred_propagations.push((error.clone(), span));
        } else if self
            .current_return
            .as_ref()
            .is_some_and(|return_ty| !return_ty.is_result())
        {
            self.error(
                span,
                "a statement-position `Result[Unit]` propagates its failure, which requires a Result-returning context",
                DiagnosticCode::CheckTryContext,
            );
        }
    }

    pub(super) fn record_statement_error(&mut self, ty: &Type, span: Span) {
        let Type::Result(_, error) = ty else {
            return;
        };
        self.require_effect(Effect::Error, span, "statement failure propagation");
        if self.retry_attempt_depth == 0 {
            self.require_pure_propagation_context(error, span);
            return;
        }
        if let Some(errors) = self.error_boundary_errors.last_mut() {
            errors.push((error.as_ref().clone(), span));
        }
    }

    pub(super) fn expect_type(&mut self, expected: &Type, actual: &Type, span: Span) {
        if expected.contains_inference() || actual.contains_inference() {
            let constrained = if expected.contains_inference() {
                self.type_constraints.constrain(expected, actual, span)
            } else {
                self.type_constraints
                    .constrain_context(expected, actual, span)
            };
            if let Err(conflict) = constrained {
                let mut diagnostic = Diagnostic::error("inferred types disagree")
                    .with_code(DiagnosticCode::CheckTypeMismatch)
                    .with_label(Label::primary(
                        conflict.contribution,
                        format!("expected {}, found {}", conflict.expected, conflict.actual),
                    ));
                if let Some(origin) = conflict.initializer {
                    diagnostic = diagnostic
                        .with_label(Label::secondary(origin, "type inference started here"));
                }
                if let Some(established) = conflict.established {
                    diagnostic = diagnostic
                        .with_label(Label::secondary(established, "type established here"));
                }
                self.diagnostics.push(diagnostic);
            }
            return;
        }
        if actual.any_flows_to_concrete(expected) {
            let target = (*actual == Type::Any).then_some(expected);
            self.report_dynamic_boundary(
                format!("unchecked {actual} cannot establish {expected}; validate with `.require(Type)` or use a checked type pattern"),
                target,
                span,
            );
            return;
        }
        if !actual.matches_expected(expected) {
            // Two module types print alike; the exports that differ are the
            // useful part of the report.
            let unmet_exports = match (expected, actual) {
                (Type::Module(expected), Type::Module(actual)) => expected.unmet_by(actual),
                _ => Vec::new(),
            };
            let unnarrowed_member = !matches!(expected, Type::Union(_))
                && matches!(actual, Type::Union(members)
                    if members.iter().any(|member| member.matches_expected(expected)));
            // Collections are invariant, so a list of one member is not a
            // list of the union even though each element would fit.
            let member_collection = match (expected, actual) {
                (Type::List(expected), Type::List(actual)) => {
                    matches!(**expected, Type::Union(_)) && actual.matches_expected(expected)
                }
                _ => false,
            };
            let (expected, actual) = self.mismatch_labels(expected, actual);
            let mut diagnostic = Diagnostic::error("type mismatch")
                .with_code(DiagnosticCode::CheckTypeMismatch)
                .with_label(Label::primary(
                    span,
                    format!("expected {expected}, found {actual}"),
                ));
            if unnarrowed_member {
                diagnostic = diagnostic.with_note(format!(
                    "a union value is a {expected} only after `value is {expected}` or a `name is {expected}` match arm proves it"
                ));
            }
            if member_collection {
                diagnostic = diagnostic.with_note(
                    "a List is invariant in its element type; build the list with the union element type, for example `[item for item in items]` where the union list is expected",
                );
            }
            for reason in unmet_exports {
                diagnostic = diagnostic.with_note(reason);
            }
            self.diagnostics.push(diagnostic);
        }
    }

    /// Reports `check.union-narrow` and returns true when `ty` is a union. A
    /// union value supports only what needs no knowledge of its member, so
    /// every operation that reads the value calls this with the receiver or
    /// operand type before it looks for a concrete one.
    pub(super) fn reject_unnarrowed_union(&mut self, ty: &Type, use_site: &str, span: Span) -> bool {
        let Type::Union(members) = ty else {
            return false;
        };
        let first = members[0]
            .annotation_source()
            .unwrap_or_else(|| "Member".to_string());
        self.diagnostics.push(
            Diagnostic::error(format!("{ty} must be narrowed to one member before {use_site}"))
                .with_code(DiagnosticCode::CheckUnionNarrow)
                .with_label(Label::primary(span, format!("this value is {ty}")))
                .with_note(format!(
                    "test it (`if value is {first} {{ ... }}`) or match it with one `name is Member` arm per member"
                )),
        );
        true
    }

    /// Rejects an `Any` value used by an operation that interprets it: an
    /// operand, a condition, a display, or a command word. `target` is the
    /// one type the operation accepts here, when the context names one.
    pub(super) fn reject_dynamic_use(&mut self, use_site: &str, target: Option<&Type>, span: Span) {
        let message = match target {
            Some(target) => format!(
                "unchecked Any used as {use_site} must be validated as {target}; validate with `.require({target})` or use a checked type pattern"
            ),
            None => format!(
                "unchecked Any used as {use_site} must be validated; validate with `.require(Type)` or use a checked type pattern"
            ),
        };
        self.report_dynamic_boundary(message, target, span);
    }

    /// Reports `check.dynamic-boundary`. When the `Any` expression at `span`
    /// was recorded and `target` has a source spelling, the fix appends
    /// `.require(T)?`; it is offered only where that `?` may propagate an
    /// `Error`, so applying it never introduces an effect or error mismatch.
    fn report_dynamic_boundary(&mut self, message: String, target: Option<&Type>, span: Span) {
        let mut diagnostic = Diagnostic::error(message.clone())
            .with_code(DiagnosticCode::CheckDynamicBoundary)
            .with_label(Label::primary(span, message));
        if let Some(target) = target
            && self.may_propagate_error()
            && let Some(receiver) = self.dynamic_require_receivers.get(&span)
            && let Some(spelled) = if receiver.inferred.as_ref() == Some(target) {
                Some(String::new())
            } else {
                require_target_spelling(target)
            }
        {
            let message = format!("validate the dynamic value as {target}");
            let grouped = receiver.grouped;
            if grouped {
                diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                    Span::at(span.source_id, span.start()),
                    message.clone(),
                    "(",
                ));
            }
            let suffix = format!("{}.require({spelled})?", if grouped { ")" } else { "" });
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                Span::at(span.source_id, span.end()),
                message,
                suffix,
            ));
        }
        self.diagnostics.push(diagnostic);
    }

    /// Whether `?` on a `Result[_, Error]` checks here without a new
    /// diagnostic; mirrors the context rules of `check_propagation`.
    fn may_propagate_error(&self) -> bool {
        if self.retry_attempt_depth > 0 {
            return true;
        }
        let effects = self.current_effects.as_ref();
        if effects.is_some_and(|effects| !effects.contains(&Effect::Error)) {
            return false;
        }
        let context = effects.is_some()
            || self.current_return.as_ref().is_none_or(Type::is_result)
            || self.current_yield.is_some()
            || (self.inferred_returns.is_some() && self.current_return == Some(Type::Unknown));
        let error_fits = match &self.current_return {
            Some(Type::Result(_, error)) => Type::Error.matches_expected(error),
            _ => true,
        };
        context && error_fits
    }

    /// `Record` alone cannot explain a mismatch between two records, so
    /// records are spelled as their checked application (`Observation[Int]`)
    /// or, failing that, their structural fields.
    fn mismatch_labels(&self, expected: &Type, actual: &Type) -> (String, String) {
        let label = |ty: &Type| match ty {
            Type::Record(fields) => self
                .record_constructors
                .application_label(ty)
                .unwrap_or_else(|| {
                    let fields = fields
                        .iter()
                        .map(|(name, ty)| format!("{name}: {ty}"))
                        .collect::<Vec<_>>();
                    format!("{{{}}}", fields.join(", "))
                }),
            _ => ty.to_string(),
        };
        if matches!((expected, actual), (Type::Record(_), Type::Record(_))) {
            (label(expected), label(actual))
        } else {
            (expected.to_string(), actual.to_string())
        }
    }

    pub(super) fn type_from_ann(&mut self, type_ann: &TypeAnnRef) -> Type {
        self.type_from_arena(&type_ann.program, type_ann.id)
    }

    pub(super) fn type_from_arena(&mut self, program: &ArenaProgram, type_id: TypeExprId) -> Type {
        let tag = program.arena.type_expr_tags[type_id.index()];
        let data = program.arena.type_expr_data[type_id.index()];
        let span = program.arena.type_expr_span(type_id);
        match tag {
            ArenaTypeExprTag::Applied => match self.record_constructors.resolve_type_checked(
                &program.arena,
                type_id,
                self.current_namespace,
            ) {
                Ok(ty) => ty,
                Err(error) => {
                    self.error(span, &error.message, error.code);
                    Type::Invalid
                }
            },
            ArenaTypeExprTag::Named => {
                let name = Name::from_symbol(Symbol::from_raw(data.lhs));
                self.type_from_name(name, span)
            }
            ArenaTypeExprTag::Qualified => {
                let namespace = Name::from_symbol(Symbol::from_raw(data.lhs));
                let name = Name::from_symbol(Symbol::from_raw(data.rhs));
                let qualified = Name::intern(format!("{namespace}.{name}"));
                // Imported error identities use the checked namespace, just as
                // constructors and error patterns do. Schema resolution must
                // not replace them with a module-local spelling.
                if self.error_families.contains_key(&qualified) {
                    return Type::ErrorFamily(qualified);
                }
                if self.error_facets.contains(&qualified) {
                    return Type::ErrorFacet(qualified);
                }
                match self.record_constructors.resolve_type_checked(
                    &program.arena,
                    type_id,
                    self.current_namespace,
                ) {
                    Ok(ty) => ty,
                    Err(error)
                        if matches!(
                            error.code,
                            DiagnosticCode::CheckTypeArity | DiagnosticCode::CheckRecursiveType
                        ) =>
                    {
                        self.error(span, &error.message, error.code);
                        Type::Invalid
                    }
                    Err(_) => self.type_from_qualified_name(namespace, name, span),
                }
            }
            ArenaTypeExprTag::List => Type::List(Box::new(
                self.type_from_arena(program, TypeExprId::from_index(data.lhs as usize)),
            )),
            ArenaTypeExprTag::Map => {
                let key = TypeExprId::from_optional_raw(data.rhs)
                    .map_or(Type::Str, |id| self.type_from_arena(program, id));
                if !key.is_map_key() && !key.is_recovery() {
                    self.error(
                        span,
                        "Map keys require Str, Int, UInt, Bool, Bytes, Path, or Duration",
                        DiagnosticCode::CheckMapKeyType,
                    );
                }
                Type::Map(
                    Box::new(key),
                    Box::new(
                        self.type_from_arena(program, TypeExprId::from_index(data.lhs as usize)),
                    ),
                )
            }
            ArenaTypeExprTag::Stream => Type::Stream(Box::new(
                self.type_from_arena(program, TypeExprId::from_index(data.lhs as usize)),
            )),
            ArenaTypeExprTag::Module => {
                let inner = TypeExprId::from_index(data.lhs as usize);
                match self.type_from_arena(program, inner) {
                    Type::Module(exports) => Type::Module(exports),
                    Type::Record(fields) => {
                        let exports = fields
                            .into_iter()
                            .map(|(name, ty)| {
                                (
                                    name,
                                    ModuleExportType::Value {
                                        ty,
                                        optional: false,
                                    },
                                )
                            })
                            .collect::<BTreeMap<_, _>>();
                        Type::Module(std::sync::Arc::new(crate::sema::types::ModuleType::open(exports)))
                    }
                    Type::Unknown | Type::Invalid => {
                        Type::Module(std::sync::Arc::new(crate::sema::types::ModuleType::open(BTreeMap::new())))
                    }
                    other => {
                        self.error(
                            program.arena.type_expr_span(inner),
                            &format!("Module[...] expected a module contract, found `{other}`"),
                            DiagnosticCode::CheckTypeMismatch,
                        );
                        Type::Invalid
                    }
                }
            }
            ArenaTypeExprTag::Result => Type::Result(
                Box::new(self.type_from_arena(program, TypeExprId::from_index(data.lhs as usize))),
                Box::new(
                    TypeExprId::from_optional_raw(data.rhs)
                        .map_or(Type::Error, |err| self.type_from_arena(program, err)),
                ),
            ),
            ArenaTypeExprTag::Optional => Type::Optional(Box::new(
                self.type_from_arena(program, TypeExprId::from_index(data.lhs as usize)),
            )),
            ArenaTypeExprTag::Union => {
                let members = program
                    .arena
                    .union_type_members(type_id)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .map(|member| self.type_from_arena(program, member))
                    .collect::<Vec<_>>();
                if let Some(reason) = crate::sema::types::union_member_error(&members) {
                    self.error(span, &reason, DiagnosticCode::CheckUnionType);
                    return Type::Invalid;
                }
                Type::Union(members)
            }
        }
    }

    pub(super) fn type_from_name(&mut self, name: Name, span: Span) -> Type {
        if BuiltinTypeName::parse(&name.as_str()) == Some(BuiltinTypeName::Unknown) {
            self.error(
                span,
                "`Unknown` is not a source type; use `Any` for dynamic values",
                DiagnosticCode::CheckUnknownType,
            );
            return Type::Invalid;
        }
        if let Some(builtin) = Type::builtin_from_name(&name.as_str()) {
            return builtin;
        }
        if let Some(record) = standard_record_type(&name.as_str()) {
            return record;
        }
        if self.error_families.contains_key(&name) {
            return Type::ErrorFamily(name);
        }
        if self.error_facets.contains(&name) {
            return Type::ErrorFacet(name);
        }
        let Some(body) = self.type_defs.get(&name).cloned() else {
            self.error(span, "unknown type", DiagnosticCode::CheckUnknownType);
            return Type::Invalid;
        };
        self.type_from_body(name, body, span)
    }

    pub(super) fn type_from_qualified_name(
        &mut self,
        namespace: Name,
        name: Name,
        span: Span,
    ) -> Type {
        let qualified = Name::intern(format!("{namespace}.{name}"));
        if self.error_families.contains_key(&qualified) {
            return Type::ErrorFamily(qualified);
        }
        if self.error_facets.contains(&qualified) {
            return Type::ErrorFacet(qualified);
        }
        let Some(types) = self.type_namespaces.get(&namespace) else {
            self.error(
                span,
                "unknown type namespace",
                DiagnosticCode::CheckUnknownType,
            );
            return Type::Invalid;
        };
        let Some(ty) = types.get(&name).cloned() else {
            self.error(
                span,
                "unknown exported type",
                DiagnosticCode::CheckUnknownType,
            );
            return Type::Invalid;
        };
        ty
    }

    pub(super) fn type_from_body(&mut self, key: Name, body: TypeDefBody, span: Span) -> Type {
        if self.resolving_types.contains(&key) {
            self.error(
                span,
                "recursive type aliases are not supported",
                DiagnosticCode::CheckRecursiveType,
            );
            return Type::Invalid;
        }
        self.resolving_types.push(key);
        let ty = match body {
            TypeDefBody::Declared(program, definition) => match self
                .record_constructors
                .resolve_definition_checked(&program.arena, definition)
            {
                Ok(ty) => ty,
                Err(error) => {
                    self.error(span, &error.message, error.code);
                    Type::Invalid
                }
            },
            TypeDefBody::Parameterized(arity) => {
                self.error(
                    span,
                    &format!("type `{key}` requires {arity} type arguments"),
                    DiagnosticCode::CheckTypeArity,
                );
                Type::Invalid
            }
            TypeDefBody::Resolved(ty) => ty,
            TypeDefBody::Alias(alias) => self.type_from_ann(&alias),
            TypeDefBody::RecordSchema(fields) => {
                let mut record = BTreeMap::new();
                for field in fields {
                    record.insert(field.name, self.type_from_ann(&field.ty));
                }
                Type::Record(record)
            }
            TypeDefBody::ModuleContract { entries, exact } => {
                let mut exports = BTreeMap::new();
                for entry in entries {
                    let export_ty = match entry.kind {
                        ModuleContractEntryKind::Value(ty) => ModuleExportType::Value {
                            ty: self.type_from_ann(&ty),
                            optional: entry.optional,
                        },
                        ModuleContractEntryKind::Proc {
                            params,
                            effects,
                            return_ty,
                        } => ModuleExportType::Proc {
                            sig: self.callable_type_from_parts(params, effects, return_ty),
                            optional: entry.optional,
                        },
                        ModuleContractEntryKind::Pure { params, return_ty } => {
                            ModuleExportType::Pure {
                                sig: self.callable_type_from_parts(params, None, return_ty),
                                optional: entry.optional,
                            }
                        }
                    };
                    exports.insert(entry.name, export_ty);
                }
                Type::Module(std::sync::Arc::new(crate::sema::types::ModuleType { exports, exact }))
            }
            TypeDefBody::TagUnion(variants) => {
                Type::Tag(variants.first().map_or(key, |variant| variant.type_name))
            }
        };
        self.resolving_types.pop();
        ty
    }

    fn callable_type_from_parts(
        &mut self,
        params: Vec<ContractParam>,
        effects: Option<Vec<Effect>>,
        return_ty: TypeAnnRef,
    ) -> CallableType {
        CallableType {
            params: params
                .into_iter()
                .map(|param| CallableParamType {
                    name: param.name,
                    ty: if param.source.ty_defaulted {
                        self.infer_checked_parameter(&param.ty.program, "", &param.source)
                    } else {
                        self.type_from_ann(&param.ty)
                    },
                    defaulted: param.defaulted,
                    rest: param.rest,
                })
                .collect(),
            return_ty: Box::new(self.type_from_ann(&return_ty)),
            effects,
        }
    }
}

pub(super) fn tail_type_matches_expected(expected: &Type, actual: &Type) -> bool {
    actual.matches_expected(expected)
        || (!actual.is_result()
            && matches!(expected, Type::Result(ok, _) if actual.matches_expected(ok)))
}

pub(super) fn result_types(ty: &Type) -> Option<(Type, Type)> {
    match ty {
        Type::Result(ok, err) => Some((ok.as_ref().clone(), err.as_ref().clone())),
        Type::Unknown => Some((Type::Unknown, Type::Unknown)),
        Type::Any => Some((Type::Any, Type::Any)),
        _ => None,
    }
}

pub(super) fn collection_item_ty(ty: &Type) -> Type {
    match ty {
        Type::List(item) => item.as_ref().clone(),
        Type::Unknown => Type::Unknown,
        Type::Any => Type::Any,
        _ => Type::Unknown,
    }
}
