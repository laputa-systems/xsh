use super::{IrVerifyError, FullStore, FullDecoder, VerifiedSchemeScopes, BTreeMap, SchemeType, SchemeInstantiationBudget, schema, Type, IrFunctionId, ScopedEffectProof, SchemeScope, BTreeSet, FullScheme, FullCallableContract, bounded, decode_scheme_type, SchemeTypeId, decode_scheme_schema, effects, applications, operations, methods, wrappers, mark_range};
use super::applications::ApplicationSubstitution;
use super::super::scheme_values::FullSchemeCallableTarget;

fn invalid() -> IrVerifyError { IrVerifyError::new("scheme callable target contracts disagree") }

pub(super) fn verify_call_site(store: &FullStore, decoder: &FullDecoder<'_>, instruction: usize, effects: &VerifiedSchemeScopes, captured: bool, types: &BTreeMap<usize, SchemeType>, budget: &mut SchemeInstantiationBudget) -> Result<Vec<usize>, IrVerifyError> {
    budget.charge(1)?;
    let site = store.scheme_callable_calls.binary_search_by_key(&(instruction as u32), |site| site.instruction)
        .map_err(|_| IrVerifyError::new("generic scheme value call has no owned relationship"))?;
    if store.scheme_application_calls.binary_search_by_key(&(instruction as u32), |site| site.instruction).is_ok() {
        return Err(IrVerifyError::new("generic value call has conflicting application authority"));
    }
    let site = store.scheme_callable_calls[site];
    let schema::ExprView::DynamicCall { callee, args, .. } = decoder.expression_view(instruction)? else {
        return Err(IrVerifyError::new("generic scheme value call has no physical callee"));
    };
    let source = types.get(&(callee.raw() as usize)).ok_or_else(|| IrVerifyError::new("generic scheme value callee has no expression type"))?;
    let (SchemeType::SchemeCallable(source) | SchemeType::Ground(Type::SchemeCallable(source))) = source else {
        return Err(IrVerifyError::new("generic scheme value callee has no relationship type"));
    };
    if store.semantic.scheme_callable_index(*source)? != site.relationship {
        return Err(IrVerifyError::new("generic scheme value call disagrees with its callee relationship"));
    }
    let relationship = store.scheme_callable_values.get(site.relationship as usize).ok_or_else(|| IrVerifyError::new("generic scheme value relationship is out of bounds"))?;
    let owner = IrFunctionId::from_raw(decoder.owner).ok_or_else(|| IrVerifyError::new("generic scheme value call has no function owner"))?.index();
    let owner_kind = store.function_metadata.get(owner).ok_or_else(|| IrVerifyError::new("generic scheme value owner metadata is missing"))?.callable_kind;
    let target_kind = store.function_metadata.get(relationship.canonical.index()).ok_or_else(|| IrVerifyError::new("generic scheme value target metadata is missing"))?.callable_kind;
    if owner_kind == crate::sema::types::CallableKind::Pure && target_kind != crate::sema::types::CallableKind::Pure {
        return Err(IrVerifyError::new("generic pure body has an impure scheme value call"));
    }
    let proof = ScopedEffectProof::new(store, effects);
    let contract = proof.value_call_contract(decoder, instruction, relationship.canonical.index(), budget)?;
    proof.require_at(owner, instruction, &contract, captured || owner_kind == crate::sema::types::CallableKind::Pure, budget)?;
    budget.charge(1 + args.len())?;
    let mut children = Vec::with_capacity(1 + args.len());
    children.push(callee.raw() as usize);
    for argument in args {
        match argument? {
            super::super::decode::CallArgView::Single(value) | super::super::decode::CallArgView::Splice(value) => children.push(value.raw() as usize),
            super::super::decode::CallArgView::Default(_) => {}
        }
    }
    Ok(children)
}

fn normalize_scope(scope: &SchemeScope, types: &[u32], rows: &[u32], effects: &[u32], budget: &mut SchemeInstantiationBudget) -> Result<SchemeScope, IrVerifyError> {
    budget.charge(scope.type_parameters.len() + scope.row_parameters.len() + scope.effects.parameters.len() + scope.row_lacks.iter().map(BTreeSet::len).sum::<usize>())?;
    let mut normalized = scope.clone();
    fn ids(mapping: &[u32], count: usize, mut set: impl FnMut(usize, u32)) -> Result<(), IrVerifyError> {
        if mapping.len() != count { return Err(invalid()); }
        let mut seen = vec![false; count];
        for (canonical, &actual) in mapping.iter().enumerate() {
            let slot = seen.get_mut(actual as usize).ok_or_else(invalid)?;
            if std::mem::replace(slot, true) { return Err(invalid()); }
            set(actual as usize, canonical as u32);
        }
        Ok(())
    }
    let bindings = std::sync::Arc::make_mut(&mut normalized.bindings);
    ids(types, bindings.type_parameters.len(), |index, id| bindings.type_parameters[index].id = id)?;
    ids(rows, bindings.row_parameters.len(), |index, id| bindings.row_parameters[index].id = id)?;
    ids(effects, bindings.effects.parameters.len(), |index, id| bindings.effects.parameters[index].id = id)?;
    bindings.type_binders = bindings.type_parameters.iter().enumerate().map(|(ordinal, parameter)| (parameter.id, ordinal)).collect();
    bindings.row_binders = bindings.row_parameters.iter().enumerate().map(|(ordinal, parameter)| (parameter.id, ordinal)).collect();
    bindings.effects.binders = bindings.effects.parameters.iter().enumerate().map(|(ordinal, parameter)| (parameter.id, ordinal)).collect();
    Ok(normalized)
}

fn scheme_for_target<'a>(store: &'a FullStore, scopes: &'a [Option<SchemeScope>], function: IrFunctionId) -> Result<(&'a FullScheme, &'a SchemeScope), IrVerifyError> {
    let FullCallableContract::Scheme(id) = store.functions.get(function.index()).ok_or_else(invalid)?.contract else { return Err(invalid()); };
    let scheme = store.scheme(id)?;
    if !scheme.is_quantified(store)? { return Err(invalid()); }
    Ok((scheme, scopes.get(id.index()).and_then(Option::as_ref).ok_or_else(invalid)?))
}

fn compare_contracts(store: &FullStore, canonical: (&FullScheme, &SchemeScope), actual: (&FullScheme, &SchemeScope), target: FullSchemeCallableTarget, budget: &mut SchemeInstantiationBudget) -> Result<(), IrVerifyError> {
    let (left, left_scope) = canonical;
    let (right, right_scope) = actual;
    if left.callable_kind != right.callable_kind || left.explicit_return != right.explicit_return || left.inferred_effects != right.inferred_effects { return Err(invalid()); }
    let types = bounded(target.types, &store.scheme_callable_binders, "scheme callable type correspondence range is invalid")?;
    let rows = bounded(target.rows, &store.scheme_callable_binders, "scheme callable row correspondence range is invalid")?;
    let effects = bounded(target.effects, &store.scheme_callable_binders, "scheme callable effect correspondence range is invalid")?;
    let identity = |count: usize| (0..count as u32).collect::<Vec<_>>();
    budget.charge(types.len() + rows.len() + effects.len())?;
    let normalized_left = normalize_scope(left_scope, &identity(left_scope.type_parameters.len()), &identity(left_scope.row_parameters.len()), &identity(left_scope.effects.parameters.len()), budget)?;
    let normalized_right = normalize_scope(right_scope, types, rows, effects, budget)?;
    let left_scope = &normalized_left;
    let right_scope = &normalized_right;
    let mut same_type = |left, right| -> Result<bool, IrVerifyError> {
        let left = decode_scheme_type(store, left_scope, left, budget, 0)?;
        let right = decode_scheme_type(store, right_scope, right, budget, 0)?;
        budget.charge_scheme_clone(&left)?;
        budget.charge_scheme_clone(&right)?;
        Ok(left == right)
    };
    if !same_type(left.return_type, right.return_type)? { return Err(invalid()); }
    let left_params = bounded(left.params, &store.scheme_params, "scheme callable parameter range is invalid")?;
    let right_params = bounded(right.params, &store.scheme_params, "scheme callable parameter range is invalid")?;
    if left_params.len() != right_params.len() { return Err(invalid()); }
    for (left, right) in left_params.iter().zip(right_params) {
        if !same_type(left.ty, right.ty)? { return Err(invalid()); }
    }
    for (index, &actual) in types.iter().enumerate() {
        budget.charge(1)?;
        let left = left_scope.type_parameters.get(index).ok_or_else(invalid)?;
        let right = right_scope.type_parameters.get(actual as usize).ok_or_else(invalid)?;
        if left.map_key != right.map_key || left.conversions != right.conversions || left.iteration_source_kind != right.iteration_source_kind || left.iteration_policy != right.iteration_policy { return Err(invalid()); }
        let left = left.iteration()?;
        let right = right.iteration()?;
        let decode = |requirement: crate::sema::schemes::IterationRequirement<SchemeTypeId>, scope: &SchemeScope, budget: &mut SchemeInstantiationBudget| {
            use crate::sema::schemes::IterationRequirement;
            Ok::<_, IrVerifyError>(IterationRequirement { item: decode_scheme_type(store, scope, requirement.item, budget, 0)?,
                policy: requirement.policy.try_map(|error| decode_scheme_type(store, scope, *error, budget, 0))?, source_kind: requirement.source_kind })
        };
        let left = left.map(|item| decode(item, left_scope, budget)).transpose()?;
        let right = right.map(|item| decode(item, right_scope, budget)).transpose()?;
        if left != right { return Err(invalid()); }
    }
    for (index, &actual) in rows.iter().enumerate() {
        budget.charge(1 + left_scope.row_lacks.get(index).ok_or_else(invalid)?.len())?;
        if left_scope.row_lacks.get(index) != right_scope.row_lacks.get(actual as usize) { return Err(invalid()); }
    }
    let schema = |schema: Option<super::super::schemes::FullSchemeSchemaExpectationId>, scope: &SchemeScope, budget: &mut SchemeInstantiationBudget| schema.map(|id| decode_scheme_schema(store, scope, id, budget, 0)).transpose();
    if schema(left.return_schema, left_scope, budget)? != schema(right.return_schema, right_scope, budget)? { return Err(invalid()); }
    for (left, right) in left_params.iter().zip(right_params) {
        if schema(left.schema, left_scope, budget)? != schema(right.schema, right_scope, budget)? { return Err(invalid()); }
    }
    if effects::decode_contract(store, left_scope, left.effects, budget)? != effects::decode_contract(store, right_scope, right.effects, budget)? { return Err(invalid()); }
    let left_predicates = bounded(left.effect_predicates, &store.scheme_effect_predicates, "scheme callable predicate range is invalid")?;
    let right_predicates = bounded(right.effect_predicates, &store.scheme_effect_predicates, "scheme callable predicate range is invalid")?;
    if left_predicates.len() != right_predicates.len() { return Err(invalid()); }
    for (left, right) in left_predicates.iter().zip(right_predicates) {
        if effects::decode_predicate(store, left_scope, *left, budget)? != effects::decode_predicate(store, right_scope, *right, budget)? { return Err(invalid()); }
    }
    let left_applications = bounded(left.applications, &store.scheme_applications, "scheme callable application range is invalid")?;
    let right_applications = bounded(right.applications, &store.scheme_applications, "scheme callable application range is invalid")?;
    if left_applications.len() != right_applications.len() { return Err(invalid()); }
    for (left, right) in left_applications.iter().zip(right_applications) {
        if !applications::equivalent_contracts(store, left_scope, *left, right_scope, *right, budget)? { return Err(invalid()); }
    }
    macro_rules! requirements {
        ($range:ident, $table:ident, $decoder:ident) => {{
            let left = bounded(left.$range, &store.$table, "scheme callable requirement range is invalid")?;
            let right = bounded(right.$range, &store.$table, "scheme callable requirement range is invalid")?;
            if left.len() != right.len() { return Err(invalid()); }
            for (left, right) in left.iter().zip(right) {
                if $decoder::decode(store, left_scope, *left, &ApplicationSubstitution::Identity, budget)? != $decoder::decode(store, right_scope, *right, &ApplicationSubstitution::Identity, budget)? { return Err(invalid()); }
            }
        }};
    }
    requirements!(operations, scheme_operations, operations);
    requirements!(methods, scheme_methods, methods);
    requirements!(wrappers, scheme_wrappers, wrappers);
    Ok(())
}

pub(super) fn verify_relationships(store: &FullStore, scopes: &[Option<SchemeScope>]) -> Result<(), IrVerifyError> {
    if store.semantic.scheme_callable_count() != store.scheme_callable_values.len() { return Err(IrVerifyError::new("semantic scheme callable ownership disagrees with its relationship table")); }
    let mut budget = SchemeInstantiationBudget::new(store);
    budget.charge(store.scheme_callable_values.len() + store.scheme_callable_targets.len() + store.scheme_callable_binders.len())?;
    let mut target_coverage = vec![false; store.scheme_callable_targets.len()];
    let mut binder_coverage = vec![false; store.scheme_callable_binders.len()];
    for relationship in &store.scheme_callable_values {
        let canonical = scheme_for_target(store, scopes, relationship.canonical)?;
        let targets = mark_range(relationship.targets, &store.scheme_callable_targets, &mut target_coverage, "scheme callable target range is invalid or overlaps")?;
        budget.charge(targets.len())?;
        if targets.is_empty() || !targets.iter().any(|target| target.function == relationship.canonical) { return Err(invalid()); }
        let mut previous = None;
        for target in targets {
            budget.charge(1)?;
            let metadata = store.functions.get(target.function.index()).ok_or_else(invalid)?;
            let canonical_function = &store.functions[relationship.canonical.index()];
            let canonical_params = bounded(canonical_function.params, &store.params, "scheme callable declaration parameter range is invalid")?;
            let actual_params = bounded(metadata.params, &store.params, "scheme callable declaration parameter range is invalid")?;
            budget.charge(canonical_params.len())?;
            if canonical_params.len() != actual_params.len() { return Err(invalid()); }
            for (left, right) in canonical_params.iter().zip(actual_params) { if left.flags != right.flags || store.string(left.name)? != store.string(right.name)? { return Err(invalid()); } }
            let key = target.function.index();
            if previous.is_some_and(|previous| previous >= key) { return Err(IrVerifyError::new("scheme callable actual targets are not sorted and unique")); }
            previous = Some(key);
            for range in [target.types, target.rows, target.effects] { mark_range(range, &store.scheme_callable_binders, &mut binder_coverage, "scheme callable binder ranges are invalid or overlap")?; }
            compare_contracts(store, canonical, scheme_for_target(store, scopes, target.function)?, *target, &mut budget)?;
        }
    }
    if target_coverage.iter().chain(&binder_coverage).any(|covered| !covered) { return Err(IrVerifyError::new("scheme callable tables contain unowned rows")); }
    Ok(())
}
