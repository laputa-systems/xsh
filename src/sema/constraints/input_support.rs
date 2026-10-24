use super::{Binding, CallableApplicationScope, ConstraintResolutionError, EffectVariableId, MAX_TYPE_DEPTH, RowTail, RowVariableId, TypeConstraints, TypeVariableId, Variable};
use crate::sema::effects::EffectContract;
use crate::sema::types::{ModuleExportType, Type};
use rustc_hash::FxHashSet;
use std::collections::BTreeSet;

#[derive(Default)]
pub(super) struct UnresolvedVariables {
    pub(super) types: FxHashSet<TypeVariableId>,
    rows: FxHashSet<RowVariableId>,
    effects: BTreeSet<EffectVariableId>,
}

impl UnresolvedVariables {
    pub(super) fn contains_type(&self, variable: TypeVariableId) -> bool { self.types.contains(&variable) }

    fn is_subset(&self, other: &Self) -> bool {
        self.types.is_subset(&other.types) && self.rows.is_subset(&other.rows)
            && self.effects.is_subset(&other.effects)
    }

    pub(super) fn extend(&mut self, other: Self) {
        self.types.extend(other.types);
        self.rows.extend(other.rows);
        self.effects.extend(other.effects);
    }
}

impl TypeConstraints {
    pub(crate) fn unresolved_local_holes_are_input_bound(
        &self,
        owner: crate::sema::check::EffectDeclarationId,
        component: &BTreeSet<crate::sema::check::EffectDeclarationId>,
        holes: &[Type],
        formals: &[Type],
    ) -> Result<bool, ConstraintResolutionError> {
        let holes = self.unresolved_input_variables(holes)?;
        Ok(holes.is_subset(&self.input_bound_variables(owner, component, formals)?))
    }

    pub(super) fn input_bound_variables(
        &self, owner: crate::sema::check::EffectDeclarationId,
        component: &BTreeSet<crate::sema::check::EffectDeclarationId>, formals: &[Type],
    ) -> Result<UnresolvedVariables, ConstraintResolutionError> {
        if !component.contains(&owner) { return Err(ConstraintResolutionError::ForeignVariable); }
        let mut supported = self.unresolved_input_variables(formals)?;
        let mut visited = BTreeSet::new();
        let mut operations = BTreeSet::new();
        let mut wrappers = BTreeSet::new();
        loop {
            let mut changed = false;
            for (&id, entry) in &self.operations.entries {
                if operations.contains(&id) || !matches!(entry.owner, CallableApplicationScope::Declaration(source) if component.contains(&source)) { continue; }
                let inputs = self.unresolved_input_variables(&entry.operation.inputs().cloned().collect::<Vec<_>>())?;
                if !inputs.is_subset(&supported) { continue; }
                operations.insert(id);
                if let Some(result) = entry.operation.result() {
                    supported.extend(self.unresolved_input_variables(std::slice::from_ref(result))?);
                }
                changed = true;
            }
            for (&id, entry) in &self.wrappers.entries {
                if wrappers.contains(&id) || !matches!(entry.owner, CallableApplicationScope::Declaration(source) if component.contains(&source)) { continue; }
                use crate::sema::wrapper_operation::{WrapperRequirementKind, WrapperErrorContribution, WrapperCompletionContribution, WrapperCompletionContributions};
                let (inputs, outputs) = match &entry.requirement.kind {
                    WrapperRequirementKind::Field { source, result, .. } => (vec![source.clone()], vec![result.clone()]),
                    WrapperRequirementKind::Fallback { source, fallback, result } => (vec![source.clone()], vec![result.clone(), fallback.clone()]),
                    WrapperRequirementKind::Handler { source, result, error, handler } => (vec![source.clone()], vec![result.clone(), error.clone(), handler.clone()]),
                    WrapperRequirementKind::Capture { contributions, result, .. } => {
                        let mut inputs = Vec::new();
                        for term in contributions.iter() {
                            match term {
                                WrapperErrorContribution::Explicit(ty) | WrapperErrorContribution::Source(ty) => inputs.push(ty.clone()),
                                WrapperErrorContribution::IndexValidation(crate::sema::wrapper_operation::WrapperOperationRef::Source(operation)) => {
                                    let entry = self.operations.entries.get(operation).ok_or(ConstraintResolutionError::ForeignVariable)?;
                                    inputs.extend(entry.operation.inputs().cloned());
                                }
                                WrapperErrorContribution::IndexValidation(crate::sema::wrapper_operation::WrapperOperationRef::Ordinal(_)) => return Err(ConstraintResolutionError::ForeignVariable),
                                WrapperErrorContribution::IndexValidationBoundary(crate::sema::wrapper_operation::WrapperRequirementRef::Source(requirement)) => {
                                    let source = self.wrappers.entries.get(requirement).ok_or(ConstraintResolutionError::ForeignVariable)?;
                                    if source.owner != entry.owner { return Err(ConstraintResolutionError::ForeignVariable); }
                                    let WrapperRequirementKind::IndexValidation {
                                        operation: crate::sema::wrapper_operation::WrapperOperationRef::Source(operation), boundary,
                                    } = &source.requirement.kind else { return Err(ConstraintResolutionError::ForeignVariable); };
                                    let operation_entry = self.operations.entries.get(operation).ok_or(ConstraintResolutionError::ForeignVariable)?;
                                    if operation_entry.owner != entry.owner { return Err(ConstraintResolutionError::ForeignVariable); }
                                    inputs.extend(operation_entry.operation.inputs().cloned());
                                    if let crate::sema::wrapper_operation::WrapperBoundary::Return(ty)
                                    | crate::sema::wrapper_operation::WrapperBoundary::ErrorReturn(ty) = boundary { inputs.push(ty.clone()); }
                                }
                                WrapperErrorContribution::IndexValidationBoundary(crate::sema::wrapper_operation::WrapperRequirementRef::Ordinal(_)) => return Err(ConstraintResolutionError::ForeignVariable),
                            }
                        }
                        (inputs, vec![result.clone()])
                    }
                    WrapperRequirementKind::IndexValidation { operation, boundary } => {
                        let crate::sema::wrapper_operation::WrapperOperationRef::Source(operation) = operation else { return Err(ConstraintResolutionError::ForeignVariable); };
                        let indexed = self.operations.entries.get(operation).ok_or(ConstraintResolutionError::ForeignVariable)?;
                        if indexed.owner != entry.owner { return Err(ConstraintResolutionError::ForeignVariable); }
                        let mut inputs = indexed.operation.inputs().cloned().collect::<Vec<_>>();
                        match boundary {
                            crate::sema::wrapper_operation::WrapperBoundary::Return(ty)
                            | crate::sema::wrapper_operation::WrapperBoundary::ErrorReturn(ty) => inputs.push(ty.clone()),
                            crate::sema::wrapper_operation::WrapperBoundary::ProcCompletion { returned, .. } => inputs.push(returned.clone()),
                            crate::sema::wrapper_operation::WrapperBoundary::Captured
                            | crate::sema::wrapper_operation::WrapperBoundary::Producer
                            | crate::sema::wrapper_operation::WrapperBoundary::TopLevel => {}
                        }
                        (inputs, Vec::new())
                    }
                    WrapperRequirementKind::ProcCompletion { returned: Type::Result(_, output), contributions: WrapperCompletionContributions::Ready(contributions), .. } => {
                        let mut inputs = Vec::new();
                        for term in contributions.iter() {
                            match term {
                                WrapperCompletionContribution::PropagatedError(error) => inputs.push(error.clone()),
                                WrapperCompletionContribution::ReturnedResult(source) | WrapperCompletionContribution::FieldSource(source) => match source {
                                    Type::Result(_, error) => inputs.push((**error).clone()),
                                    Type::Inference(_) => inputs.push(source.clone()),
                                    _ => {}
                                },
                                WrapperCompletionContribution::ApplicationSource(source) => match source {
                                    Type::Result(_, error) => inputs.push((**error).clone()),
                                    Type::Inference(_) => inputs.push(source.clone()),
                                    _ => {}
                                },
                                WrapperCompletionContribution::IndexValidation(crate::sema::wrapper_operation::WrapperOperationRef::Source(operation)) => {
                                    let indexed = self.operations.entries.get(operation).ok_or(ConstraintResolutionError::ForeignVariable)?;
                                    if indexed.owner != entry.owner { return Err(ConstraintResolutionError::ForeignVariable); }
                                    inputs.extend(indexed.operation.inputs().cloned());
                                }
                                WrapperCompletionContribution::IndexValidation(crate::sema::wrapper_operation::WrapperOperationRef::Ordinal(_)) => return Err(ConstraintResolutionError::ForeignVariable),
                            }
                        }
                        (inputs, vec![(**output).clone()])
                    }
                    WrapperRequirementKind::ProcCompletion { .. } => (Vec::new(), Vec::new()),
                };
                if !self.unresolved_input_variables(&inputs)?.is_subset(&supported) { continue; }
                wrappers.insert(id);
                supported.extend(self.unresolved_input_variables(&outputs)?);
                if let WrapperRequirementKind::Field { effects, .. } = &entry.requirement.kind
                    && let EffectContract::Known(expression) = self.resolve_effect(effects)? { supported.effects.extend(expression.variables().map(|(id, _)| *id)); }
                changed = true;
            }
            for (&id, entry) in &self.applications.entries {
                if visited.contains(&id)
                    || !matches!(entry.owner, CallableApplicationScope::Declaration(source) if component.contains(&source))
                { continue; }
                let application = &entry.application;
                if let crate::sema::callable_application::CallableApplication::KnownArguments(application) = application {
                    let arguments = application.arguments.iter().map(|argument| argument.ty.clone()).collect::<Vec<_>>();
                    if !self.unresolved_input_variables(&arguments)?.is_subset(&supported) { continue; }
                    visited.insert(id);
                    supported.extend(self.unresolved_input_variables(&application.parameters.iter().map(|parameter| parameter.ty.clone()).collect::<Vec<_>>())?);
                    changed = true;
                    continue;
                }
                let crate::sema::callable_application::CallableApplication::Invocation(application) = application else { unreachable!() };
                let callee = self.resolve(&application.callee)?;
                if !matches!(callee.unvalidated(), Type::Inference(_))
                    || !self.unresolved_input_variables(std::slice::from_ref(&callee))?.is_subset(&supported)
                { continue; }
                visited.insert(id);
                let types = std::iter::once(application.result.clone())
                    .chain(application.arguments.iter().map(|argument| argument.ty.clone()))
                    .chain(application.proc_returns.iter().flat_map(|envelope| std::iter::once(envelope.result.clone()).chain(envelope.propagated_errors.iter().cloned())))
                    .collect::<Vec<_>>();
                supported.extend(self.unresolved_input_variables(&types)?);
                if let EffectContract::Known(expression) = self.resolve_effect(&application.effects)? {
                    supported.effects.extend(expression.variables().map(|(id, _)| *id));
                }
                changed = true;
            }
            if !changed { return Ok(supported); }
        }
    }

    pub(super) fn unresolved_input_variables(&self, types: &[Type]) -> Result<UnresolvedVariables, ConstraintResolutionError> {
        let mut variables = UnresolvedVariables::default();
        let mut pending = types.iter().map(|ty| self.resolve(ty).map(|ty| (ty, 0))).collect::<Result<Vec<_>, _>>()?;
        while let Some((ty, depth)) = pending.pop() {
            if depth > MAX_TYPE_DEPTH { return Err(ConstraintResolutionError::TypeDepth); }
            if let Type::Inference(id) = ty {
                let root = self.representative_root(id)?;
                if !variables.types.insert(root) { continue; }
                let Some(Variable::Representative(variable)) = self.variables.get(&root) else { unreachable!() };
                if let Some(iteration) = &variable.iteration {
                    for ty in iteration.types() { pending.push((self.resolve(ty)?, depth + 1)); }
                }
                match &variable.binding {
                    Some(Binding::Type { ty, .. }) => pending.push((self.resolve(ty)?, depth)),
                    Some(Binding::Record(record)) => {
                        let row = self.resolve_row(&record.shape)?;
                        if let RowTail::Variable(id) = row.tail { variables.rows.insert(self.row_representative_root(id)?); }
                        for field in row.fields.values() { pending.push((self.resolve(field)?, depth + 1)); }
                    }
                    #[cfg(test)]
                    Some(Binding::Rigid { .. }) => {}
                    None => {}
                }
                continue;
            }
            let mut contracts = Vec::new();
            match &ty {
                Type::Callable(callable) => contracts.push(callable.sig.effects.contract()),
                Type::Module(module) => {
                    for export in module.values() {
                        if let ModuleExportType::Callable { sig, .. } = export {
                            contracts.push(sig.effects.contract());
                        }
                    }
                }
                _ => {}
            }
            for contract in contracts {
                if let EffectContract::Known(expression) = self.resolve_effect(contract)? {
                    variables.effects.extend(expression.variables().map(|(id, _)| *id));
                }
            }
            ty.for_each_child(|child| pending.push((child.clone(), depth + 1)));
        }
        Ok(variables)
    }
}

#[cfg(test)]
mod tests {
    use super::{BTreeSet, CallableApplicationScope, ConstraintResolutionError, EffectContract, Type, TypeConstraints};
    use crate::sema::callable_application::CallableKindRequirement;
    use crate::sema::check::EffectDeclarationId;
    use crate::source::{SourceId, Span};
    use std::sync::Arc;

    fn owner(start: usize) -> EffectDeclarationId {
        EffectDeclarationId { namespace: None, body: Span::new(SourceId::new(0), start, start + 1) }
    }

    #[test]
    fn local_input_support_follows_all_operation_inputs_without_reversing_results_or_cycles() {
        let owner = owner(0);
        let component = BTreeSet::from([owner]);
        let mut constraints = TypeConstraints::default();
        let left = constraints.fresh(owner.body);
        let right = constraints.fresh(owner.body);
        let result = constraints.fresh(owner.body);
        constraints.register_sealed_operation(CallableApplicationScope::Declaration(owner), crate::sema::sealed_operation::SealedBinaryOperation { operator: crate::sema::sealed_operation::ArithmeticOperator::Add, left: left.clone(), right: right.clone(), result: result.clone(), span: owner.body }).unwrap();
        assert!(constraints.unresolved_local_holes_are_input_bound(owner, &component, std::slice::from_ref(&result), &[left.clone(), right.clone()]).unwrap());
        assert!(!constraints.unresolved_local_holes_are_input_bound(owner, &component, std::slice::from_ref(&result), std::slice::from_ref(&left)).unwrap());
        assert!(!constraints.unresolved_local_holes_are_input_bound(owner, &component, std::slice::from_ref(&left), std::slice::from_ref(&result)).unwrap());
        constraints.register_sealed_operation(CallableApplicationScope::Declaration(owner), crate::sema::sealed_operation::SealedBinaryOperation { operator: crate::sema::sealed_operation::ArithmeticOperator::Add, left: result.clone(), right, result: left, span: owner.body }).unwrap();
        assert!(!constraints.unresolved_local_holes_are_input_bound(owner, &component, &[result], &[]).unwrap());
    }

    fn application(constraints: &mut TypeConstraints, owner: EffectDeclarationId, callee: Type, result: Type) {
        let effects = constraints.fresh_effect_at_level(owner.body, 1);
        constraints.register_callable_application(CallableApplicationScope::Declaration(owner), crate::sema::callable_application::CallableInvocation {
            callee, result, arguments: Arc::from([]), effects: EffectContract::variable(effects),
            proc_returns: Arc::from([]), kind: CallableKindRequirement::Effectful, span: owner.body,
        }).unwrap();
    }

    #[test]
    fn local_input_support_follows_original_aliases_and_rejects_foreign_holes() {
        let mut constraints = TypeConstraints::default();
        let declaration = owner(0);
        let component = BTreeSet::from([declaration]);
        let formal = constraints.fresh_at_level(declaration.body, 1);
        let hole = constraints.fresh_at_level(declaration.body, 1);
        assert!(!constraints.unresolved_local_holes_are_input_bound(declaration, &component, std::slice::from_ref(&hole), std::slice::from_ref(&formal)).unwrap());
        constraints.constrain(&hole, &formal, declaration.body).unwrap();
        assert!(constraints.unresolved_local_holes_are_input_bound(declaration, &component, std::slice::from_ref(&hole), std::slice::from_ref(&formal)).unwrap());
        let foreign = TypeConstraints::default().fresh_at_level(declaration.body, 1);
        assert_eq!(constraints.unresolved_local_holes_are_input_bound(declaration, &component, &[foreign], &[formal]), Err(ConstraintResolutionError::ForeignVariable));
        assert_eq!(constraints.unresolved_local_holes_are_input_bound(owner(2), &component, &[hole], &[]), Err(ConstraintResolutionError::ForeignVariable));
    }

    #[test]
    fn local_input_support_reaches_callback_results_but_not_unrooted_cycles_or_peers() {
        let mut constraints = TypeConstraints::default();
        let declaration = owner(0);
        let peer = owner(2);
        let component = BTreeSet::from([declaration]);
        let formal = constraints.fresh_at_level(declaration.body, 1);
        let returned = constraints.fresh_at_level(declaration.body, 1);
        let nested = constraints.fresh_at_level(declaration.body, 1);
        application(&mut constraints, declaration, formal.clone(), returned.clone());
        application(&mut constraints, declaration, returned.clone(), nested.clone());
        assert!(constraints.unresolved_local_holes_are_input_bound(declaration, &component, &[nested], std::slice::from_ref(&formal)).unwrap());
        let first = constraints.fresh_at_level(declaration.body, 1);
        let second = constraints.fresh_at_level(declaration.body, 1);
        application(&mut constraints, declaration, first.clone(), second.clone());
        application(&mut constraints, declaration, second.clone(), first.clone());
        assert!(!constraints.unresolved_local_holes_are_input_bound(declaration, &component, &[first, second], std::slice::from_ref(&formal)).unwrap());
        let unrelated = constraints.fresh_at_level(peer.body, 1);
        application(&mut constraints, peer, formal.clone(), unrelated.clone());
        assert!(!constraints.unresolved_local_holes_are_input_bound(declaration, &component, std::slice::from_ref(&unrelated), std::slice::from_ref(&formal)).unwrap());
        assert!(constraints.unresolved_local_holes_are_input_bound(declaration, &BTreeSet::from([declaration, peer]), &[unrelated], &[formal]).unwrap());
    }

    #[test]
    fn local_input_support_does_not_invent_argument_relationships_erased_by_actual_any() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            for generic_return in [false, true] {
                let mut constraints = TypeConstraints::default();
                let declaration = owner(0);
                let component = BTreeSet::from([declaration]);
                let returned = if generic_return { constraints.fresh_at_level(declaration.body, 1) } else { Type::Int };
                let formals = if generic_return { vec![returned.clone()] } else { Vec::new() };
                let hole = constraints.fresh_at_level(declaration.body, 1);
                let actual = Type::Callable(Arc::new(crate::sema::types::TypedCallable {
                    kind: crate::sema::types::CallableKind::Pure,
                    sig: crate::sema::types::CallableType {
                        params: vec![crate::sema::types::CallableParamType {
                            name: crate::symbol::Name::intern("value"), ty: Type::Any, defaulted: false, rest: false,
                        }],
                        return_ty: Box::new(returned.clone()), effects: crate::sema::types::CallableEffects::finite([]),
                    },
                }));
                constraints.register_callable_application(CallableApplicationScope::Declaration(declaration), crate::sema::callable_application::CallableInvocation {
                    callee: actual, result: returned,
                    arguments: Arc::from([crate::sema::callable_application::CallableApplicationArgument {
                        kind: crate::sema::arguments::StaticArgumentKind::Positional,
                        ty: Type::List(Box::new(hole.clone())), span: declaration.body,
                    }]),
                    effects: EffectContract::finite(crate::sema::effects::EffectSet::default()),
                    proc_returns: Arc::from([]), kind: CallableKindRequirement::Pure, span: declaration.body,
                }).unwrap();
                constraints.solve_callable_applications().unwrap();
                assert!(!constraints.unresolved_local_holes_are_input_bound(declaration, &component, &[hole], &formals).unwrap());
            }
        });
    }

    #[test]
    fn local_input_support_preserves_open_row_descendants_and_effect_only_relationships() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let mut constraints = TypeConstraints::default();
            let declaration = owner(0);
            let component = BTreeSet::from([declaration]);
            let formal = constraints.fresh_at_level(declaration.body, 1);
            let field = constraints.project_record_field(&formal, crate::symbol::Name::intern("value"), 1, declaration.body).unwrap();
            assert!(constraints.unresolved_local_holes_are_input_bound(declaration, &component, &[field], &[formal]).unwrap());
            let first = constraints.fresh_effect_at_level(declaration.body, 1);
            let second = constraints.fresh_effect_at_level(declaration.body, 1);
            let callable = |effect| Type::Callable(Arc::new(crate::sema::types::TypedCallable {
                kind: crate::sema::types::CallableKind::Proc,
                sig: crate::sema::types::CallableType {
                    params: Vec::new(), return_ty: Box::new(Type::Int),
                    effects: crate::sema::types::CallableEffects::from_contract(EffectContract::variable(effect)),
                },
            }));
            assert!(!constraints.unresolved_local_holes_are_input_bound(declaration, &component, &[callable(second)], &[callable(first)]).unwrap());
            constraints.constrain_effect(crate::sema::effects::EffectPredicate::Equal(EffectContract::variable(first), EffectContract::variable(second))).unwrap();
            assert!(constraints.unresolved_local_holes_are_input_bound(declaration, &component, &[callable(second)], &[callable(first)]).unwrap());
            let foreign = TypeConstraints::default().fresh_effect_at_level(declaration.body, 1);
            assert_eq!(constraints.unresolved_local_holes_are_input_bound(declaration, &component, &[callable(foreign)], &[]), Err(ConstraintResolutionError::ForeignVariable));
        });
    }
}
