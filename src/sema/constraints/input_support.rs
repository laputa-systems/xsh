use super::{Binding, CallableApplicationScope, ConstraintResolutionError, EffectVariableId, MAX_TYPE_DEPTH, RowTail, RowVariableId, TypeConstraints, TypeVariableId, Variable};
use crate::sema::effects::EffectContract;
use crate::sema::types::{ModuleExportType, Type};
use rustc_hash::FxHashSet;
use std::collections::BTreeSet;

#[derive(Default)]
struct UnresolvedVariables {
    types: FxHashSet<TypeVariableId>,
    rows: FxHashSet<RowVariableId>,
    effects: BTreeSet<EffectVariableId>,
}

impl UnresolvedVariables {
    fn is_subset(&self, other: &Self) -> bool {
        self.types.is_subset(&other.types) && self.rows.is_subset(&other.rows)
            && self.effects.is_subset(&other.effects)
    }

    fn extend(&mut self, other: Self) {
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
        if !component.contains(&owner) { return Err(ConstraintResolutionError::ForeignVariable); }
        let holes = self.unresolved_input_variables(holes)?;
        let mut supported = self.unresolved_input_variables(formals)?;
        let mut visited = BTreeSet::new();
        loop {
            let mut changed = false;
            for (&id, entry) in &self.applications.entries {
                if visited.contains(&id)
                    || !matches!(entry.owner, CallableApplicationScope::Declaration(source) if component.contains(&source))
                { continue; }
                let application = &entry.application;
                let callee = self.resolve(&application.callee)?;
                if !matches!(callee.unvalidated(), Type::Inference(_) | Type::Callable(_))
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
            if !changed { return Ok(holes.is_subset(&supported)); }
        }
    }

    fn unresolved_input_variables(&self, types: &[Type]) -> Result<UnresolvedVariables, ConstraintResolutionError> {
        let mut variables = UnresolvedVariables::default();
        let mut pending = types.iter().map(|ty| self.resolve(ty).map(|ty| (ty, 0))).collect::<Result<Vec<_>, _>>()?;
        while let Some((ty, depth)) = pending.pop() {
            if depth > MAX_TYPE_DEPTH { return Err(ConstraintResolutionError::TypeDepth); }
            if let Type::Inference(id) = ty {
                let root = self.representative_root(id)?;
                if !variables.types.insert(root) { continue; }
                let Some(Variable::Representative(variable)) = self.variables.get(&root) else { unreachable!() };
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
                        if let ModuleExportType::Pure { sig, .. } | ModuleExportType::Proc { sig, .. } = export {
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
    use crate::sema::callable_application::{CallableApplication, CallableKindRequirement};
    use crate::sema::check::EffectDeclarationId;
    use crate::source::{SourceId, Span};
    use std::sync::Arc;

    fn owner(start: usize) -> EffectDeclarationId {
        EffectDeclarationId { namespace: None, body: Span::new(SourceId::new(0), start, start + 1) }
    }

    fn application(constraints: &mut TypeConstraints, owner: EffectDeclarationId, callee: Type, result: Type) {
        let effects = constraints.fresh_effect_at_level(owner.body, 1);
        constraints.register_callable_application(CallableApplicationScope::Declaration(owner), CallableApplication {
            callee, result, arguments: Arc::from([]), effects: EffectContract::variable(effects),
            proc_returns: Arc::from([]), kind: CallableKindRequirement::Immediate, span: owner.body,
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
                pure: false,
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
