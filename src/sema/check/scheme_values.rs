pub(super) mod rebase;

use super::{CallableAlias, Checker, Span, Type};
use crate::diagnostic::DiagnosticCode;
use crate::sema::scheme_callable::{SchemeCallableError, SchemeCallableRelationship};
use crate::sema::types::{CallableKind, SchemeCallableId};
use crate::syntax::arena::{ArenaCallArg, ArenaExprKind, ArenaProgram, ExprId};
use std::sync::Arc;

impl Checker {
    pub(super) fn merge_scheme_callable_values(&mut self, left: &Type, right: &Type) -> Option<Type> {
        if !left.contains_scheme_callable() && !right.contains_scheme_callable() { return None; }
        self.merge_scheme_callable_parts(left, right)
    }

    fn merge_scheme_callable_parts(&mut self, left: &Type, right: &Type) -> Option<Type> {
        if left == right { return Some(left.clone()); }
        match (left, right) {
            (Type::SchemeCallable(left), Type::SchemeCallable(right)) => {
                let left = self.scheme_callable_relationship(*left).ok()?;
                let right = self.scheme_callable_relationship(*right).ok()?;
                let callables = self.scheme_callables.lock().expect("checked scheme callable table is poisoned");
                let mut targets = std::collections::BTreeMap::new();
                for target in left.targets().iter().chain(right.targets()) {
                    let identity = target.target();
                    let key = (identity.namespace, identity.name, identity.definition.index());
                    if let Some((_, contract)) = targets.get(&key)
                        && !target.contract().equivalent_to_in_table(contract, &callables)
                    { return None; }
                    targets.insert(key, (identity, target.contract().clone()));
                }
                let prepared = SchemeCallableRelationship::new(targets.into_values().collect(), &callables).ok()?;
                drop(callables);
                Some(Type::SchemeCallable(self.intern_scheme_callable_relationship(prepared).ok()?))
            }
            (Type::List(left), Type::List(right)) => Some(Type::List(Box::new(self.merge_scheme_callable_parts(left, right)?))),
            (Type::Optional(left), Type::Optional(right)) => Some(Type::Optional(Box::new(self.merge_scheme_callable_parts(left, right)?))),
            (Type::Result(left, left_error), Type::Result(right, right_error)) if left_error == right_error => Some(Type::Result(Box::new(self.merge_scheme_callable_parts(left, right)?), left_error.clone())),
            (Type::Record(left), Type::Record(right)) if left.keys().eq(right.keys()) => {
                let fields = left.iter().map(|(&name, left)| Some((name, self.merge_scheme_callable_parts(left, &right[&name])?))).collect::<Option<std::collections::BTreeMap<_, _>>>()?;
                Some(Type::Record(Arc::new(fields)))
            }
            _ => None,
        }
    }

    pub(super) fn scheme_callable_types_compatible(&self, left: &Type, right: &Type) -> bool {
        if !left.contains_scheme_callable() && !right.contains_scheme_callable() { return false; }
        self.scheme_callable_parts_compatible(left, right)
    }

    fn scheme_callable_parts_compatible(&self, left: &Type, right: &Type) -> bool {
        if left == right { return true; }
        match (left, right) {
            (Type::SchemeCallable(left), Type::SchemeCallable(right)) => {
                let callables = self.scheme_callables.lock().expect("checked scheme callable table is poisoned");
                callables.relationship(*left).ok().zip(callables.relationship(*right).ok())
                    .is_some_and(|(left, right)| left.canonical().equivalent_to_in_table(right.canonical(), &callables))
            }
            (Type::List(left), Type::List(right)) | (Type::Optional(left), Type::Optional(right)) => self.scheme_callable_parts_compatible(left, right),
            (Type::Result(left, left_error), Type::Result(right, right_error)) => left_error == right_error && self.scheme_callable_parts_compatible(left, right),
            (Type::Record(left), Type::Record(right)) => left.keys().eq(right.keys()) && left.iter().all(|(name, left)| self.scheme_callable_parts_compatible(left, &right[name])),
            _ => false,
        }
    }

    pub(super) fn merge_checked_branches(&mut self, expectation: Option<super::types::TypeExpectation<'_>>, previous: &Type, actual: &Type) -> Option<Type> {
        self.merge_scheme_callable_values(previous, actual).or_else(|| super::types::merge_checked_branches(expectation, previous, actual))
    }

    pub(super) fn scheme_callable_relationship(&self, id: SchemeCallableId) -> Result<Arc<SchemeCallableRelationship>, SchemeCallableError> {
        self.scheme_callables.lock().expect("checked scheme callable table is poisoned").relationship(id)
    }

    fn intern_scheme_callable_relationship(&mut self, prepared: SchemeCallableRelationship) -> Result<SchemeCallableId, SchemeCallableError> {
        let prepared = Arc::new(prepared);
        let mut callables = self.scheme_callables.lock().expect("checked scheme callable table is poisoned");
        let (id, actual) = callables.intern_prepared(Arc::clone(&prepared))?;
        if !actual.agrees_with(&prepared, &callables) { return Err(SchemeCallableError::IncompatibleContract); }
        Ok(id)
    }

    pub(super) fn retain_named_scheme_callable(&mut self, program: &ArenaProgram, named: &CallableAlias, expression: ExprId) -> Option<Type> {
        let span = program.arena.expr(expression).span;
        if !named.signature.scheme().callable().is_quantified() { return None; }
        let target = named.signature.definition_target()?;
        let contract = self.scheme_value_contract(program, &named.signature);
        let prepared = {
            let callables = self.scheme_callables.lock().expect("checked scheme callable table is poisoned");
            SchemeCallableRelationship::new(vec![(target, contract)], &callables)
        };
        let prepared = match prepared {
            Ok(relationship) => relationship,
            Err(error) => {
                self.error(span, &format!("callable value has no complete scheme: {error:?}"), DiagnosticCode::CheckLocalInference);
                return Some(Type::Invalid);
            }
        };
        match self.intern_scheme_callable_relationship(prepared) {
            Ok(id) => {
                let ty = Type::SchemeCallable(id);
                if let ArenaExprKind::Try(inner) = program.arena.expr(expression).kind
                    && let Some(projection) = self.projections.get(&program.arena.expr(inner).span)
                    && projection.operation == crate::sema::projection::ProjectionOperation::Get
                    && let Some(actual) = projection.callable.as_ref().and_then(|export| self.resolve_generic_module_callable_alias(program, projection.receiver, projection.field, export))
                    && actual.signature.definition_target() == Some(target)
                    && let Some(Type::Result(success, error)) = self.expr_types.get(&program.arena.expr(inner).span)
                    && success.as_ref() == &Type::Function(named.kind)
                {
                    self.record_expr_type(program.arena.expr(inner).span, Type::Result(Box::new(ty.clone()), error.clone()));
                }
                self.record_selected_function_value(span, &named.signature);
                self.record_expr_type(span, ty.clone());
                Some(ty)
            }
            Err(error) => {
                self.error(span, &format!("callable value relationship is not owned: {error:?}"), DiagnosticCode::CheckLocalInference);
                Some(Type::Invalid)
            }
        }
    }

    pub(super) fn check_scheme_callable_application(
        &mut self, program: &ArenaProgram, source: &str, callee: ExprId, id: SchemeCallableId,
        arguments: &[ArenaCallArg], span: Span, checked_argument: Option<super::args::CheckedCallArgument<'_>>,
        expected_context: Option<&Type>,
    ) -> Type {
        let relationship = match self.scheme_callable_relationship(id) {
            Ok(relationship) => relationship,
            Err(error) => {
                self.error(span, &format!("callable value relationship is not owned: {error:?}"), DiagnosticCode::CheckCallableMismatch);
                return Type::Invalid;
            }
        };
        self.record_expr_type(program.arena.expr(callee).span, Type::SchemeCallable(id));
        self.scheme_callable_calls.insert(crate::sema::schemes::InstantiationSite::call(span), id);
        let scheme = relationship.canonical().scheme();
        let contract = self.scheme_call_contract(scheme, None, None, span);
        let rejected_in_pure = scheme.kind() != CallableKind::Pure && self.boundary.in_pure;
        let invocation = if scheme.kind() == CallableKind::Pure { None }
            else if rejected_in_pure {
                self.error(span, "effectful callable value is not allowed in pure functions", DiagnosticCode::CheckPureEffect);
                None
            } else { self.check_function_invocation_effects(&contract, "callable value", span) };
        if !self.check_function_contract_arguments_arena(program, source, arguments, &contract, span, checked_argument, expected_context) {
            self.reject_invocation_return(invocation, &contract.effects, span);
            return Type::Invalid;
        }
        if rejected_in_pure {
            self.reject_invocation_return(None, &contract.effects, span);
            return Type::Invalid;
        }
        self.record_invocation_propagation(&contract.effects, &contract.return_ty, span, invocation);
        if scheme.kind() != CallableKind::Pure { self.invalidate_mutable_narrowings(); }
        contract.return_ty
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callable_lists_join_actual_target_families_and_deduplicate_members() {
        let source = "pure preserve(value) { value }\npure echo(value) { value }\nlet combined = [preserve, echo, preserve]\nlet duplicated = [preserve, preserve]\n";
        let source_id = crate::source::SourceId::new(0);
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source, super::super::CheckOptions::for_execution());
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let lists = parsed.arena.arena.source_rows().expr_ids(source_id).filter(|&expression| {
            matches!(parsed.arena.arena.expr(expression).kind, crate::syntax::arena::ArenaExprKind::List(_))
        }).collect::<Vec<_>>();
        assert_eq!(lists.len(), 2);
        for (expression, expected) in lists.into_iter().zip([vec!["echo", "preserve"], vec!["preserve"]]) {
            let span = parsed.arena.arena.expr(expression).span;
            let Type::List(item) = &checked.expr_types[&span] else { panic!("a list keeps its item contract") };
            let Type::SchemeCallable(relationship) = item.as_ref() else { panic!("a callable list keeps its quantified relationship") };
            let relationship = checked.scheme_callables.get(*relationship).unwrap();
            let mut actual = relationship.targets().iter().map(|target| target.target().name.to_string()).collect::<Vec<_>>();
            actual.sort();
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn callable_lists_reject_incompatible_parameter_and_result_relationships() {
        for other in ["pure other(left, right) { left }", "pure other(value) { [value] }"] {
            let source = format!("pure preserve(value) {{ value }}\n{other}\nlet operations = [preserve, other]\n");
            let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena_with_options(&parsed.arena, &source, super::super::CheckOptions::for_execution());
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckTypeMismatch)), "{source}\n{:?}", checked.diagnostics);
        }
    }

    #[test]
    fn callable_lists_merge_nested_relationships_but_reject_changed_targets() {
        fn check(second_target: &str) -> (String, crate::sema::check::CheckOutput, Vec<crate::source::Span>) {
            let source = format!(
                "pure identity(value) {{ value }}\npure alternative(value) {{ value }}\npure first(marker) {{ let _ = marker; let forwarded = identity; forwarded }}\npure second(marker) {{ let _ = marker; let forwarded = {second_target}; forwarded }}\nlet operations = [first, second]\n"
            );
            let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena_with_options(&parsed.arena, &source, super::super::CheckOptions::for_execution());
            let lists = parsed.arena.arena.source_rows().expr_ids(crate::source::SourceId::new(0)).filter(|&expression| {
                matches!(parsed.arena.arena.expr(expression).kind, crate::syntax::arena::ArenaExprKind::List(_))
            }).map(|expression| parsed.arena.arena.expr(expression).span).collect();
            (source, checked, lists)
        }

        let (_, checked, lists) = check("identity");
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let [list] = lists.as_slice() else { panic!("the source contains one callable family"); };
        let Type::List(item) = &checked.expr_types[list] else { panic!("the joined value remains a list"); };
        let Type::SchemeCallable(relationship) = item.as_ref() else { panic!("the list retains callable target relationships"); };
        let relationship = checked.scheme_callables.get(*relationship).unwrap();
        let mut names = relationship.targets().iter().map(|target| target.target().name.to_string()).collect::<Vec<_>>();
        names.sort();
        assert_eq!(names, ["first", "second"]);

        let (source, rejected, _) = check("alternative");
        assert!(rejected.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckTypeMismatch)),
            "{source}\n{:?}", rejected.diagnostics);
    }

    #[test]
    fn checker_probes_share_append_identity_without_mutating_published_snapshots() {
        let source = "pure first(value) { value }\npure second(value) { value }\n";
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source, super::super::CheckOptions::for_execution());
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let targets = checked.checked_declarations.functions.iter().map(|(&definition, function)| (
            super::super::CheckedFunctionTarget { definition, namespace: function.namespace, name: function.name },
            function.callable_contract().unwrap(),
        )).collect::<Vec<_>>();
        let mut checker = Checker::new(super::super::CheckOptions::for_execution());
        let first_relationship = {
            let callables = checker.scheme_callables.lock().unwrap();
            SchemeCallableRelationship::new(vec![targets[0].clone()], &callables).unwrap()
        };
        let first = checker.intern_scheme_callable_relationship(first_relationship).unwrap();
        let published = checker.scheme_callables.lock().unwrap().snapshot();
        let mut probe = checker.clone();
        assert!(Arc::ptr_eq(&checker.scheme_callables, &probe.scheme_callables));
        let second_relationship = {
            let callables = probe.scheme_callables.lock().unwrap();
            SchemeCallableRelationship::new(vec![targets[1].clone()], &callables).unwrap()
        };
        let second = probe.intern_scheme_callable_relationship(second_relationship).unwrap();
        assert!(checker.scheme_callable_relationship(second).is_ok());
        assert!(published.get(first).is_ok());
        assert_eq!(published.get(second).unwrap_err(), SchemeCallableError::UnknownRelationship);
        let repeated_relationship = {
            let callables = checker.scheme_callables.lock().unwrap();
            SchemeCallableRelationship::new(vec![targets[1].clone()], &callables).unwrap()
        };
        assert_eq!(checker.intern_scheme_callable_relationship(repeated_relationship).unwrap(), second);
        let complete = probe.scheme_callables.lock().unwrap().snapshot();
        let mut rebaser = crate::sema::scheme_callable::SchemeCallableRebaser::new(Arc::new(std::sync::Mutex::new(Default::default())));
        rebaser.import_table(&complete).unwrap();
        let rebased = rebaser.ty(&Type::SchemeCallable(second), &complete).unwrap();
        let Type::SchemeCallable(rebased) = rebased else { panic!("rebasing must preserve the relationship leaf"); };
        assert_ne!(rebased, second);
        assert!(rebaser.snapshot().get(rebased).is_ok());
        assert_eq!(complete.get(rebased).unwrap_err(), SchemeCallableError::ForeignOwner);
        assert!(published.get(first).is_ok());
        assert_eq!(published.get(second).unwrap_err(), SchemeCallableError::UnknownRelationship);
    }
}
