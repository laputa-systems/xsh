pub(super) mod rebase;

use super::{CallableAlias, Checker, Span, Type};
use crate::diagnostic::DiagnosticCode;
use crate::sema::scheme_callable::{SchemeCallableError, SchemeCallableRelationship};
use crate::sema::types::{CallableKind, SchemeCallableId};
use crate::syntax::arena::{ArenaCallArg, ArenaProgram, ExprId};
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
                let mut targets = std::collections::BTreeMap::new();
                for target in left.targets().iter().chain(right.targets()) {
                    let identity = target.target();
                    let key = (identity.namespace, identity.name, identity.definition.index());
                    if let Some((_, contract)) = targets.get(&key)
                        && !target.contract().equivalent_to(contract)
                    { return None; }
                    targets.insert(key, (identity, target.contract().clone()));
                }
                let prepared = SchemeCallableRelationship::new(targets.into_values().collect()).ok()?;
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
            (Type::SchemeCallable(left), Type::SchemeCallable(right)) => self.scheme_callable_relationship(*left).ok()
                .zip(self.scheme_callable_relationship(*right).ok())
                .is_some_and(|(left, right)| left.canonical().equivalent_to(right.canonical())),
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
        let (id, actual) = self.scheme_callables.lock().expect("checked scheme callable table is poisoned").intern_prepared(Arc::clone(&prepared))?;
        if !actual.agrees_with(&prepared) { return Err(SchemeCallableError::IncompatibleContract); }
        Ok(id)
    }

    pub(super) fn retain_named_scheme_callable(&mut self, program: &ArenaProgram, named: &CallableAlias, span: Span) -> Option<Type> {
        if !named.signature.scheme().callable().is_quantified() { return None; }
        let target = named.signature.definition_target()?;
        let contract = self.scheme_value_contract(program, &named.signature);
        let prepared = match SchemeCallableRelationship::new(vec![(target, contract)]) {
            Ok(relationship) => relationship,
            Err(error) => {
                self.error(span, &format!("callable value has no complete scheme: {error:?}"), DiagnosticCode::CheckLocalInference);
                return Some(Type::Invalid);
            }
        };
        match self.intern_scheme_callable_relationship(prepared) {
            Ok(id) => {
                let ty = Type::SchemeCallable(id);
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
        let invocation = if scheme.kind() == CallableKind::Pure { None }
            else if self.boundary.in_pure {
                self.error(span, "effectful callable value is not allowed in pure functions", DiagnosticCode::CheckPureEffect);
                None
            } else { self.check_function_invocation_effects(&contract, "callable value", span) };
        if !self.check_function_contract_arguments_arena(program, source, arguments, &contract, span, checked_argument) {
            self.reject_invocation_return(invocation, &contract.effects, span);
            return Type::Invalid;
        }
        self.record_invocation_propagation(&contract.effects, &contract.return_ty, span, invocation);
        if scheme.kind() != CallableKind::Pure { self.invalidate_mutable_narrowings(); }
        contract.return_ty
    }
}
