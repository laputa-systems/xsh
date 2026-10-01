#![allow(clippy::single_call_fn)]

use super::{
    BTreeMap, CallableParamType, CallableType, Checker, ContractParam, Diagnostic, Effect, Label,
    ModuleContractEntryKind, ModuleExportType, Span, Type, TypeAnnRef, TypeDefBody,
};
use crate::sema::records::standard_record_type;
use crate::symbol::{Name, Symbol};
use crate::syntax::arena::{ArenaProgram, ArenaTypeExprTag, TypeExprId};
use xsh_registry::types::BuiltinTypeName;

impl Checker {
    // A fresh inference variable can retain explicit dynamic data. Validation
    // is required only where that data would cross an established contract.
    pub(super) fn graph_argument_needs_validation(&mut self, expected: crate::sema::inference::TypeId, actual: crate::sema::inference::TypeId) -> Result<bool, crate::sema::inference::InferenceError> {
        use crate::sema::inference::{Atom, TypeNode};
        let mut pairs = vec![(expected, actual)];
        let mut visited = std::collections::BTreeSet::new();
        let mut state = self.generic.borrow_mut();
        while let Some((expected, actual)) = pairs.pop() {
            let graph = &mut state.facts.graph;
            let expected = graph.resolved(expected)?;
            let actual_id = graph.resolved(actual)?;
            if !visited.insert((expected, actual_id)) { continue; }
            graph.charge_source_fact_work(1)?;
            let expected = graph.node(expected)?.clone();
            let actual = graph.node(actual_id)?.clone();
            if matches!(expected, TypeNode::Poison) || matches!(actual, TypeNode::Poison) { continue; }
            if matches!(actual, TypeNode::Atom(Atom::Any))
                && !matches!(expected, TypeNode::Meta(_) | TypeNode::Atom(Atom::Any)) { return Ok(true); }
            match (expected, actual) {
                (TypeNode::List(expected), TypeNode::List(actual)) | (TypeNode::Stream(expected), TypeNode::Stream(actual)) | (TypeNode::Optional(expected), TypeNode::Optional(actual)) => pairs.push((expected, actual)),
                (TypeNode::Optional(_), TypeNode::Atom(Atom::Null)) => {},
                (TypeNode::Optional(expected), _) => pairs.push((expected, actual_id)),
                (TypeNode::Map(ek, ev), TypeNode::Map(ak, av)) | (TypeNode::Result(ek, ev), TypeNode::Result(ak, av)) => pairs.extend([(ek, ak), (ev, av)]),
                (TypeNode::Record(expected), TypeNode::Record(actual)) => {
                    let expected = graph.row_data(expected)?;
                    let actual = graph.row_data(actual)?;
                    for field in &expected.fields {
                        if let Some(actual) = actual.fields.iter().find(|actual| actual.label == field.label) { pairs.push((field.ty, actual.ty)); }
                    }
                }
                (TypeNode::Record(_), TypeNode::Atom(Atom::ErasedRecord)) | (TypeNode::Module(_), TypeNode::Atom(Atom::DynamicModule)) => return Ok(true),
                (TypeNode::Module(expected), TypeNode::Module(actual)) => {
                    for field in expected {
                        if let Some(actual) = actual.iter().find(|actual| actual.label == field.label) { pairs.push((field.ty, actual.ty)); }
                    }
                }
                (TypeNode::Arrow(expected), TypeNode::Arrow(actual)) => {
                    pairs.extend(expected.params.iter().zip(actual.params.iter()).map(|(expected, actual)| (expected.ty, actual.ty)));
                    pairs.push((expected.result, actual.result));
                }
                _ => {}
            }
        }
        Ok(false)
    }

    fn propagation_result_parts(&mut self, ty: &Type, span: Span) -> Option<(Type, Type)> {
        if let Some(parts) = result_types(ty) { return Some(parts); }
        let Type::Graph(operand) = ty else { return None; };
        let outcome = (|| {
            use crate::sema::inference::{InferenceError, TypeNode};
            let mut state = self.generic.borrow_mut();
            let graph = &mut state.facts.graph;
            match graph.node(graph.resolved(*operand)?)?.clone() {
                TypeNode::Result(success, error) => Ok(Some((success, error))),
                TypeNode::Meta(_) if self.graph_generation => {
                    let level = graph.variable(*operand)?.ok_or(InferenceError::InvalidScheme)?.level;
                    let success = graph.fresh(level, span)?;
                    let error = graph.fresh(level, span)?;
                    let result = graph.result(success, error)?;
                    let reason = graph.reason(span, None)?;
                    // Postfix propagation fixes the container shape at its
                    // source operand; success and failure remain independent.
                    graph.unify(*operand, result, reason)?;
                    Ok(Some((success, error)))
                }
                _ => Ok(None),
            }
        })();
        match outcome {
            Ok(parts) => parts.map(|(success, error)| (self.graph_view(success), self.graph_view(error))),
            Err(error) => { self.graph_error(span, error); Some((Type::Invalid, Type::Invalid)) }
        }
    }

    pub(super) fn check_propagation(&mut self, ty: &Type, span: Span) -> Type {
        if self.retry_attempt_depth > 0 {
            return self.check_attempt_propagation(ty, span);
        }
        let parts = self.propagation_result_parts(ty, span);
        if let Some(errors) = &mut self.with_initializer_errors
            && let Some((_, error)) = &parts {
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
                "check.effect-violation",
            );
        }
        let Some((ok, err)) = parts else {
            self.error(
                span,
                "`?` can be applied only to Result values",
                "check.try-result",
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
        let inferring = self.inferred_returns.is_some() && (self.current_return == Some(Type::Unknown) || self.current_generic.is_some());
        if inferring { self.inferred_propagations.push((err.clone(), span)); }
        if !allowed && !inferring {
            self.error(
                span,
                "`?` requires a Result-returning context",
                "check.try-context",
            );
        }
        if let Some(Type::Result(_, return_err)) = &self.current_return
            && !err.matches_expected(return_err)
        {
            self.diagnostics.push(
                Diagnostic::error("incompatible propagated error")
                    .with_code("check.try-error")
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
        let Some((ok, err)) = self.propagation_result_parts(ty, span) else {
            self.error(
                span,
                "`?` can be applied only to Result values",
                "check.try-result",
            );
            return Type::Unknown;
        };
        if let Some(errors) = self.error_boundary_errors.last_mut() { errors.push((err, span)); }
        ok
    }

    pub(super) fn begin_error_boundary(&mut self) {
        self.retry_attempt_depth += 1;
        self.error_boundary_errors.push(Vec::new());
        self.error_boundary_producer_flows.push(Vec::new());
    }

    pub(super) fn end_error_boundary(&mut self, expected: Option<&Type>) -> Type {
        self.retry_attempt_depth -= 1;
        let errors = self.error_boundary_errors.pop().expect("checked error boundary");
        self.error_boundary_producer_flows.pop().expect("checked producer error boundary");
        if self.graph_generation && errors.iter().any(|(error, _)| error.contains_graph()) {
            let span = errors[0].1;
            let outcome = (|| {
                use crate::sema::inference::{ErrorJoin, InferenceError};
                let inputs = errors.iter().map(|(error, span)| self.graph_type(error, *span)).collect::<Result<Vec<_>, _>>()?;
                let bound = expected.map(|bound| self.graph_type(bound, span)).transpose()?;
                let level = self.local_initializer_level.unwrap_or(if self.current_generic.is_some() { 1 } else { 0 });
                let mut state = self.generic.borrow_mut();
                let graph = &mut state.facts.graph;
                // A single reached failure keeps its original payload port.
                // Multiple ports remain independent until the join is known.
                let result = if inputs.len() == 1 && bound.is_none() { inputs[0] } else { graph.fresh(level, span)? };
                let reason = graph.reason(span, None)?;
                let requirement = graph.require_error_join(ErrorJoin { inputs, result, bound }, reason)?;
                graph.solve()?;
                if let Some(owner) = self.current_generic { state.pending.get_mut(&owner).ok_or(InferenceError::InvalidScheme)?.requirements.push(requirement); }
                Ok(result)
            })();
            return match outcome {
                Ok(result) => self.graph_view(result),
                Err(error) => { self.graph_error(span, error); Type::Invalid }
            };
        }
        let mut inferred = expected.cloned();
        for (error, span) in errors {
            if let Some(current) = &inferred {
                if error.matches_expected(current) { continue; }
                if expected.is_none() && current.matches_expected(&error) { inferred = Some(error); continue; }
                if expected.is_none() {
                    let family = match (current, &error) {
                        (Type::ErrorVariant { family: left, .. } | Type::ErrorFamily(left), Type::ErrorVariant { family: right, .. } | Type::ErrorFamily(right)) if left == right => Some(*left),
                        _ => None,
                    };
                    inferred = Some(family.map(Type::ErrorFamily).unwrap_or(Type::Error));
                }
                else { self.expect_type(current, &error, span); }
            } else { inferred = Some(error); }
        }
        inferred.unwrap_or(Type::Error)
    }

    pub(super) fn record_callee_propagation(&mut self, effects: &Option<Vec<Effect>>, return_ty: &Type, span: Span) {
        let return_ty = self.resolved_graph_view(return_ty.clone());
        if self.retry_attempt_depth > 0 && !return_ty.is_result() && !matches!(return_ty, Type::Stream(_))
            && effects.as_ref().is_some_and(|effects| effects.contains(&Effect::Error))
            && let Some(errors) = self.error_boundary_errors.last_mut() {
            errors.push((Type::Error, span));
        }
    }

    pub(super) fn record_statement_error(&mut self, ty: &Type, span: Span) {
        if matches!(ty, Type::Bool | Type::Result(_, _)) {
            self.require_effect(Effect::Error, span, "statement failure propagation");
        }
        let error = match ty {
            Type::Result(_, error) => Some(error.as_ref().clone()),
            Type::Bool => Some(Type::ErrorFamily(Name::intern("AssertionError"))),
            _ => None,
        };
        if self.retry_attempt_depth == 0 {
            if self.inferred_returns.is_some() && ty.is_result_unit() && let Some(error) = error {
                self.inferred_propagations.push((error, span));
            }
            return;
        }
        if let (Some(errors), Some(error)) = (self.error_boundary_errors.last_mut(), error) { errors.push((error, span)); }
    }

    pub(super) fn reject_ignored_result(&mut self, ty: &Type, span: Span) {
        if ty.is_result() {
            self.error(span, "ignored Result value", "check.ignored-result");
        }
    }

    pub(super) fn expect_type(&mut self, expected: &Type, actual: &Type, span: Span) {
        let expected_graph_view = if let Type::Graph(id) = expected { Some(self.graph_view(*id)) } else { None };
        let expected_view = expected_graph_view.as_ref().unwrap_or(expected);
        let actual_graph_view = if let Type::Graph(id) = actual { Some(self.graph_view(*id)) } else { None };
        let actual_view = actual_graph_view.as_ref().unwrap_or(actual);
        let requires_validation = if expected.contains_graph() || actual.contains_graph() {
            if !self.graph_generation { return; }
            let result = (|| {
                let expected = self.graph_type(expected, span)?;
                let actual = self.graph_type(actual, span)?;
                self.graph_argument_needs_validation(expected, actual)
            })();
            match result {
                Ok(required) => required,
                Err(error) => { self.graph_error(span, error); return; }
            }
        } else { actual_view.any_flows_to_concrete(expected_view) };
        if requires_validation {
            self.error(span, &format!("unchecked {actual_view} cannot establish {expected_view}; validate with `.require(Type)` or use a checked type pattern"), "check.dynamic-boundary");
            return;
        }
        if self.graph_expect(expected, actual, span) { return; }
        if expected.contains_inference() || actual.contains_inference() {
            let constrained = if expected.contains_inference() {
                self.type_constraints.constrain(expected, actual, span)
            } else {
                self.type_constraints.constrain_context(expected, actual, span)
            };
            if let Err(conflict) = constrained {
                let mut diagnostic = Diagnostic::error("inferred types disagree")
                    .with_code("check.type-mismatch")
                    .with_label(Label::primary(conflict.contribution,
                        format!("expected {}, found {}", conflict.expected, conflict.actual)));
                if let Some(origin) = conflict.initializer {
                    diagnostic = diagnostic.with_label(Label::secondary(origin, "type inference started here"));
                }
                if let Some(established) = conflict.established {
                    diagnostic = diagnostic.with_label(Label::secondary(established, "type established here"));
                }
                self.diagnostics.push(diagnostic);
            }
            return;
        }
        if actual.any_flows_to_concrete(expected) {
            self.error(
                span,
                &format!("unchecked {actual} cannot establish {expected}; validate with `.require(Type)` or use a checked type pattern"),
                "check.dynamic-boundary",
            );
            return;
        }
        if !actual.matches_expected(expected) {
            self.diagnostics.push(
                Diagnostic::error("type mismatch")
                    .with_code("check.type-mismatch")
                    .with_label(Label::primary(
                        span,
                        format!("expected {expected}, found {actual}"),
                    )),
            );
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
            ArenaTypeExprTag::Applied => match self.record_constructors.resolve_type_checked(&program.arena, type_id, self.current_namespace) {
                Ok(ty) => ty,
                Err(error) => { self.error(span, &error.message, error.code); Type::Invalid }
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
                match self.record_constructors.resolve_type_checked(&program.arena, type_id, self.current_namespace) {
                    Ok(ty) => ty,
                    Err(error) if matches!(error.code, "check.type-arity" | "check.recursive-type") => {
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
                let key = TypeExprId::from_optional_raw(data.rhs).map_or(Type::Str, |id| self.type_from_arena(program, id));
                if !key.is_map_key() && !key.is_recovery() {
                    self.error(span, "Map keys require Str, Int, UInt, Bool, Bytes, Path, or Duration", "check.map-key-type");
                }
                Type::Map(Box::new(key), Box::new(self.type_from_arena(program, TypeExprId::from_index(data.lhs as usize))))
            },
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
                            .collect();
                        Type::Module(exports)
                    }
                    Type::Unknown | Type::Invalid => Type::Module(BTreeMap::new()),
                    other => {
                        self.error(
                            program.arena.type_expr_span(inner),
                            &format!("Module[...] expected a module contract, found `{other}`"),
                            "check.type-mismatch",
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
        }
    }

    pub(super) fn type_from_name(&mut self, name: Name, span: Span) -> Type {
        if BuiltinTypeName::parse(&name.as_str()) == Some(BuiltinTypeName::Unknown) {
            self.error(
                span,
                "`Unknown` is not a source type; use `Any` for dynamic values",
                "check.unknown-type",
            );
            return Type::Invalid;
        }
        if let Some(builtin) = Type::builtin_from_name(&name.as_str()) {
            return builtin;
        }
        if let Some(record) = self.check_graph_registry_schema(name, span) { return record; }
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
            self.error(span, "unknown type", "check.unknown-type");
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
            self.error(span, "unknown type namespace", "check.unknown-type");
            return Type::Invalid;
        };
        let Some(ty) = types.get(&name).cloned() else {
            self.error(span, "unknown exported type", "check.unknown-type");
            return Type::Invalid;
        };
        ty
    }

    pub(super) fn type_from_body(&mut self, key: Name, body: TypeDefBody, span: Span) -> Type {
        if self.resolving_types.contains(&key) {
            self.error(
                span,
                "recursive type aliases are not supported",
                "check.recursive-type",
            );
            return Type::Invalid;
        }
        self.resolving_types.push(key);
        let ty = match body {
            TypeDefBody::Declared(program, definition) => match self.record_constructors.resolve_definition_checked(&program.arena, definition) {
                Ok(ty) => ty,
                Err(error) => { self.error(span, &error.message, error.code); Type::Invalid }
            },
            TypeDefBody::Parameterized(arity) => {
                self.error(span, &format!("type `{key}` requires {arity} type arguments"), "check.type-arity");
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
            TypeDefBody::ModuleContract(entries) => {
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
                Type::Module(exports)
            }
            TypeDefBody::TagUnion(variants) => Type::Tag(variants.first().map_or(key, |variant| variant.type_name)),
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
                    ty: if param.source.ty_defaulted { self.infer_checked_parameter(&param.ty.program, "", &param.source) } else { self.type_from_ann(&param.ty) },
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

#[cfg(test)]
mod tests {
    use super::super::Checker;
    use crate::source::SourceId;
    use crate::syntax::parser::Parser;

    #[test]
    fn retry_return_preserves_written_enclosing_result_and_nested_results() {
        let source = "proc outer() -> Result[Int] { let value = retry [] { return Ok(7) }?; value }\nlet nested = retry [] { Ok(7) }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let (outer_identity, outer) = checked.solved.declarations.iter().next().unwrap();
        assert!(checked.solved.graph.scheme(outer.scheme).unwrap().quantifiers.is_empty(), "the absent retry payload does not become a procedure type parameter");
        let crate::sema::inference::TypeNode::Arrow(signature) = checked.solved.graph.node(outer.signature).unwrap() else { panic!("the written procedure retains its signature") };
        assert_eq!(checked.solved.graph.export_type(signature.result).unwrap(), crate::sema::types::Type::Result(Box::new(crate::sema::types::Type::Int), Box::new(crate::sema::types::Type::Error)));
        let outer_body = parsed.arena.arena.function_def(outer_identity.declaration).body;
        let propagation = parsed.arena.arena.stmt_ids(parsed.arena.arena.block(outer_body).statements).find_map(|statement| match parsed.arena.arena.stmt(statement).kind {
            crate::syntax::arena::ArenaStmtKind::Let { initializer: crate::syntax::arena::ArenaExprOrRun::Expr(expression), .. } => Some(super::super::ExpressionIdentity { source: parsed.arena.arena.expr(expression).span.source_id, namespace: outer_identity.namespace, expression }),
            _ => None,
        }).unwrap();
        let crate::syntax::arena::ArenaExprKind::Try(retry) = parsed.arena.arena.expr(propagation.expression).kind else { panic!("the original initializer propagates the retry expression") };
        let retry = super::super::ExpressionIdentity { expression: retry, ..propagation };
        let scope = checked.solved.expression_schemes[&retry];
        assert!(checked.solved.non_completing_expressions.contains(&propagation));
        assert_eq!(checked.solved.expression_value_scopes[&propagation], scope);
        assert!(!checked.solved.expression_schemes.contains_key(&propagation), "a noncompleting projection inherits scope without becoming a polymorphic value");
        assert!(checked.solved.result_statement_wrappings.is_empty(), "the unreachable procedure tail contributes no successful wrapper");
        let nested = *checked.solved.bindings.keys().find(|identity| matches!(parsed.arena.arena.binding_target(identity.target).kind, crate::syntax::arena::ArenaBindingTargetKind::Name(name) if name == "nested")).unwrap();
        let nested = checked.solved.bindings[&nested].ty;
        drop(parsed);
        checked.solved.validate().unwrap();
        assert_eq!(checked.solved.graph.export_type(nested).unwrap(), crate::sema::types::Type::Result(Box::new(crate::sema::types::Type::Int), Box::new(crate::sema::types::Type::Error)));
        for invalid in [
            "proc outer() -> Result[Int] { let value = retry [] { return Ok(\"wrong\") }?; value }\n",
            "proc outer(flag: Bool) -> Result[Int] { let value = retry [] { if flag { return Ok(7) }; \"wrong\" }?; value }\n",
        ] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), invalid);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, invalid);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.type-mismatch")), "{invalid}: {:?}", checked.diagnostics);
        }
    }
}
