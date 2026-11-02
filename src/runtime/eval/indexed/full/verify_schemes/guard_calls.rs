use super::{bounded, instantiate_invocation, CallSubstitution, FullCallableContract, FullSchemeId, FullStore, FullTag, IrVerifyError, SchemeInstantiationBudget, SchemeScope, IR_NONE};
use super::super::guard_schemes::{FullGuardBinding, FullGuardCall, FullGuardCallTarget, FullGuardScope};
use super::super::schemes::FullInstantiation;
use crate::sema::guarded_requirements::GuardScopeId;
use crate::sema::schemes::InstantiationRole;
use std::sync::Arc;

pub(in crate::runtime::eval::indexed::full) struct ResolvedGuardCall {
    pub(in crate::runtime::eval::indexed::full) owner: FullSchemeId,
    pub(in crate::runtime::eval::indexed::full) scope: GuardScopeId,
    pub(in crate::runtime::eval::indexed::full) metadata: FullGuardScope,
    pub(in crate::runtime::eval::indexed::full) parent: Arc<CallSubstitution>,
}

pub(super) fn source(store: &FullStore, call: FullGuardCall, budget: &mut SchemeInstantiationBudget) -> Result<(FullInstantiation, FullSchemeId), IrVerifyError> {
    let (arguments, target) = source_target(store, call, budget)?;
    let function = store.functions.get(target.index()).ok_or_else(|| IrVerifyError::new("guard call target is out of bounds"))?;
    let FullCallableContract::Scheme(owner) = function.contract else { return Err(IrVerifyError::new("guard call target has no scoped requirement contract")); };
    Ok((arguments, owner))
}

pub(in crate::runtime::eval::indexed::full) fn source_target(store: &FullStore, call: FullGuardCall, budget: &mut SchemeInstantiationBudget) -> Result<(FullInstantiation, super::IrFunctionId), IrVerifyError> {
    budget.charge(1)?;
    let instruction = call.instruction as usize;
    let arguments;
    let target;
    if call.leaf != IR_NONE {
        if call.role != InstantiationRole::Value || store.tags.get(instruction) != Some(&FullTag::ExprCheckedValue) { return Err(IrVerifyError::new("guard call materialization has another physical source role")); }
        let payload = store.payload(store.data[instruction].range())?;
        if payload.get(1) != Some(&(super::super::decode::ValueCheckTag::Materialization as u32)) { return Err(IrVerifyError::new("guard call source has no physical materialization")); }
        let plan = payload.get(2).and_then(|id| store.value_materializations.get(*id as usize)).ok_or_else(|| IrVerifyError::new("guard call materialization source is invalid"))?;
        if plan.instruction != call.instruction { return Err(IrVerifyError::new("guard call materialization belongs to another instruction")); }
        let mut pending = vec![plan.root];
        let mut found = false;
        while let Some(node) = pending.pop() {
            budget.charge(1)?;
            let node = store.materialization_nodes.get(node as usize).ok_or_else(|| IrVerifyError::new("guard call materialization node is invalid"))?;
            let children = bounded(node.children, &store.materialization_edges, "guard call materialization edges are invalid")?;
            budget.charge(children.len())?;
            pending.extend(children.iter().map(|child| child.node));
            if node.kind == super::super::value_materialization::FullMaterializationKind::Callable && node.leaf == call.leaf { found = true; }
        }
        if !found { return Err(IrVerifyError::new("guard call leaf belongs to another materialization")); }
        let leaf = store.materialization_leaves.get(call.leaf as usize).ok_or_else(|| IrVerifyError::new("guard call materialization leaf is invalid"))?;
        arguments = leaf.instance;
        target = store.scheme_callable_values.get(leaf.relationship as usize).ok_or_else(|| IrVerifyError::new("guard call materialization relationship is invalid"))?.canonical;
    } else {
        let tag = store.tags.get(instruction).ok_or_else(|| IrVerifyError::new("guard call source instruction is invalid"))?;
        let actual_role = if matches!(tag, FullTag::ExprCall | FullTag::ExprDirectPureCall | FullTag::ExprSelfCall | FullTag::ExprDynamicCall) { InstantiationRole::Call } else if matches!(tag, FullTag::ExprFunctionRef | FullTag::ExprParam | FullTag::ExprCheckedValue) { InstantiationRole::Value } else { return Err(IrVerifyError::new("guard call source has no invocation or value role")); };
        if call.role != actual_role { return Err(IrVerifyError::new("guard call source has another physical role")); }
        let index = store.call_instantiations.binary_search_by_key(&call.instruction, |instance| instance.instruction).map_err(|_| IrVerifyError::new("guard call source has no actual call instantiation"))?;
        arguments = store.call_instantiations[index].arguments;
        target = if let Ok(index) = store.checked_call_targets.binary_search_by_key(&call.instruction, |target| target.instruction) {
            store.checked_call_targets[index].function
        } else {
            let index = store.scheme_callable_calls.binary_search_by_key(&call.instruction, |call| call.instruction).map_err(|_| IrVerifyError::new("guard call source has no checked target"))?;
            let relationship = store.scheme_callable_calls[index].relationship;
            store.scheme_callable_values.get(relationship as usize).ok_or_else(|| IrVerifyError::new("guard call source relationship is invalid"))?.canonical
        };
    }
    Ok((arguments, target))
}

pub(super) fn verify_origin(store: &FullStore, call: FullGuardCall, ordinal: u32, budget: &mut SchemeInstantiationBudget) -> Result<(), IrVerifyError> {
    let (arguments, owner) = source(store, call, budget)?;
    let scheme = store.scheme(owner)?;
    let binding = match call.target {
        FullGuardCallTarget::Scope(id) => {
            let metadata = bounded(scheme.guards, &store.scheme_guards, "guard call target scopes are invalid")?.get(id as usize).filter(|guard| guard.id == id && guard.parent == IR_NONE).ok_or_else(|| IrVerifyError::new("guard call target is not an actual root guard"))?;
            bounded(arguments.guard_bindings, &store.call_guard_bindings, "guard call source bindings are invalid")?.get(metadata.binding_ordinal as usize)
        }
        FullGuardCallTarget::Call(id) => {
            bounded(scheme.guard_calls, &store.scheme_guard_calls, "guard call target call range is invalid")?.get(id as usize).ok_or_else(|| IrVerifyError::new("guard call target call is out of bounds"))?;
            bounded(arguments.guard_call_bindings, &store.call_guard_bindings, "guard call source call bindings are invalid")?.get(id as usize)
        }
    };
    if binding != Some(&FullGuardBinding::Call(ordinal)) { return Err(IrVerifyError::new("guard call origin disagrees with its actual target binding")); }
    Ok(())
}

pub(super) enum Recursion<'a> {
    Own(&'a super::SymbolicCallSubstitution),
    Peer { caller: &'a super::FullScheme, substitution: &'a super::SymbolicCallSubstitution },
}

pub(super) fn verify_forwarded_bindings(store: &FullStore, callee: &super::FullScheme, callee_scope: &SchemeScope, caller: &SchemeScope, instruction: u32, arguments: FullInstantiation, recursion: Recursion<'_>, budget: &mut SchemeInstantiationBudget) -> Result<bool, IrVerifyError> {
    let caller_root = caller;
    let caller = caller.at_instruction(instruction as usize)?;
    let calls = bounded(caller.guard_calls, &store.scheme_guard_calls, "forwarded guard call source range is invalid")?;
    let guards = bounded(callee.guards, &store.scheme_guards, "forwarded guard target range is invalid")?;
    let roots = guards.iter().filter(|guard| guard.parent == IR_NONE).collect::<Vec<_>>();
    let physical = bounded(arguments.guard_bindings, &store.call_guard_bindings, "forwarded guard binding range is invalid")?;
    let residual = bounded(arguments.guard_call_bindings, &store.call_guard_bindings, "forwarded guard call binding range is invalid")?;
    let target_calls = bounded(callee_scope.guard_calls, &store.scheme_guard_calls, "forwarded target call range is invalid")?;
    if roots.len() != physical.len() || target_calls.len() != residual.len() { return Err(IrVerifyError::new("forwarded guard bindings are incomplete")); }
    if physical.iter().chain(residual).any(|binding| *binding == FullGuardBinding::Recursive) {
        if physical.iter().chain(residual).any(|binding| *binding != FullGuardBinding::Recursive) { return Err(IrVerifyError::new("recursive guard binding is mixed with another binding")); }
        budget.charge(physical.len() + residual.len())?;
        if matches!(recursion, Recursion::Peer { .. }) && !residual.is_empty() { return Err(IrVerifyError::new("component peer call forwards a guard call")); }
        match recursion {
            Recursion::Own(substitution) => if !substitution.is_identity(callee_scope) { return Err(IrVerifyError::new("recursive guard binding has a non-identity call substitution")); },
            Recursion::Peer { caller: caller_scheme, substitution } => verify_component_correspondence(store, (callee, callee_scope), (caller_scheme, caller_root), substitution, budget)?,
        }
        return Ok(true);
    }
    let mut concrete = false;
    for (target, binding) in roots.iter().map(|guard| FullGuardCallTarget::Scope(guard.id)).zip(physical)
        .chain(target_calls.iter().enumerate().map(|(ordinal, _)| FullGuardCallTarget::Call(ordinal as u32)).zip(residual))
    {
        budget.charge(1)?;
        let FullGuardBinding::Call(ordinal) = binding else { concrete = true; continue; };
        let call = calls.get(*ordinal as usize).ok_or_else(|| IrVerifyError::new("forwarded guard call ordinal is outside its caller pack"))?;
        if call.instruction != instruction || call.target != target { return Err(IrVerifyError::new("forwarded guard call names another source or target requirement")); }
        verify_origin(store, *call, *ordinal, budget)?;
    }
    Ok(!concrete)
}

#[derive(Clone, Copy)]
struct GuardCallHop {
    caller: FullSchemeId,
    scope: Option<GuardScopeId>,
    callee: FullSchemeId,
    arguments: FullInstantiation,
}

pub(super) struct GuardOutputEvidence {
    owner: FullSchemeId,
    scope: Option<GuardScopeId>,
    binders: super::SchemeBinderEvidence,
}

impl GuardOutputEvidence {
    pub(super) fn extend(self, owner: FullSchemeId, scope: &SchemeScope, evidence: &mut super::SchemeBinderEvidence) -> Result<(), IrVerifyError> {
        if self.owner != owner || self.scope != scope.active_guard { return Err(IrVerifyError::new("guard output evidence belongs to another callable or lexical scope")); }
        for &binder in &self.binders.types { scope.check_type_binder(binder)?; }
        for &binder in &self.binders.rows { scope.check_row_binder(binder)?; }
        evidence.types.extend(self.binders.types);
        evidence.rows.extend(self.binders.rows);
        Ok(())
    }
}

pub(in crate::runtime::eval::indexed::full) fn resolve(store: &FullStore, call: FullGuardCall, ordinal: u32, caller: Arc<CallSubstitution>, budget: &mut SchemeInstantiationBudget) -> Result<ResolvedGuardCall, IrVerifyError> {
    resolve_inner(store, call, ordinal, caller, None, budget)
}

pub(super) fn resolve_with_output_evidence(store: &FullStore, call: FullGuardCall, ordinal: u32, caller: Arc<CallSubstitution>, budget: &mut SchemeInstantiationBudget) -> Result<(ResolvedGuardCall, Option<GuardOutputEvidence>), IrVerifyError> {
    let mut hops = Vec::new();
    let resolved = resolve_inner(store, call, ordinal, caller, Some(&mut hops), budget)?;
    let Some(output) = resolved.metadata.output else { return Ok((resolved, None)); };
    let (types, rows) = super::scheme_type_binders(store, output.outward, budget)?;
    let mut evidence = super::SchemeBinderEvidence { types, rows };
    let mut owner = resolved.owner;
    let origin = *hops.first().ok_or_else(|| IrVerifyError::new("guard output has no actual call origin"))?;
    for hop in hops.into_iter().rev() {
        budget.charge(1)?;
        if hop.callee != owner { return Err(IrVerifyError::new("guard output crosses another callable namespace")); }
        let mut next = super::SchemeBinderEvidence::default();
        for argument in bounded(hop.arguments.type_arguments, &store.call_type_arguments, "guard output type arguments are invalid")? {
            budget.charge(1)?;
            if !evidence.types.remove(&(argument.binder as usize)) { continue; }
            let (types, rows) = super::scheme_type_binders(store, argument.ty, budget)?;
            next.types.extend(types);
            next.rows.extend(rows);
        }
        for argument in bounded(hop.arguments.row_arguments, &store.call_row_arguments, "guard output row arguments are invalid")? {
            budget.charge(1)?;
            if !evidence.rows.remove(&(argument.binder as usize)) { continue; }
            if argument.tail != IR_NONE { next.rows.insert(argument.tail as usize); }
            for field in bounded(argument.fields, &store.call_row_fields, "guard output row fields are invalid")? {
                budget.charge(1)?;
                let (types, rows) = super::scheme_type_binders(store, field.ty, budget)?;
                next.types.extend(types);
                next.rows.extend(rows);
            }
        }
        if !evidence.types.is_empty() || !evidence.rows.is_empty() { return Err(IrVerifyError::new("guard output binder has no actual call substitution")); }
        evidence = next;
        owner = hop.caller;
    }
    if owner != origin.caller { return Err(IrVerifyError::new("guard output has another original caller")); }
    Ok((resolved, Some(GuardOutputEvidence { owner, scope: origin.scope, binders: evidence })))
}

fn resolve_inner(store: &FullStore, mut call: FullGuardCall, mut ordinal: u32, caller: Arc<CallSubstitution>, mut hops: Option<&mut Vec<GuardCallHop>>, budget: &mut SchemeInstantiationBudget) -> Result<ResolvedGuardCall, IrVerifyError> {
    let mut parent = caller;
    for _ in 0..=crate::sema::constraints::MAX_TYPE_DEPTH {
        budget.charge(1)?;
        let (current_owner, current_scope, pack) = store.requirement_pack_at(call.function, call.instruction)?.ok_or_else(|| IrVerifyError::new("guard call source has no callable owner"))?;
        if !parent.owns_prepared_scope(current_owner, current_scope) { return Err(IrVerifyError::new("guard call inherits another callable or lexical scope")); }
        let actual = bounded(pack.guard_calls, &store.scheme_guard_calls, "guard call owner range is invalid")?.get(ordinal as usize).ok_or_else(|| IrVerifyError::new("guard call ordinal is outside its actual source scope"))?;
        if *actual != call { return Err(IrVerifyError::new("guard call metadata belongs to another source")); }
        verify_origin(store, call, ordinal, budget)?;
        let (arguments, owner) = source(store, call, budget)?;
        let scheme = store.scheme(owner)?;
        if let Some(hops) = hops.as_deref_mut() {
            budget.charge(4)?;
            hops.push(GuardCallHop { caller: current_owner, scope: current_scope, callee: owner, arguments });
        }
        parent = Arc::new(instantiate_invocation(store, owner, scheme, &arguments, &parent, budget)?);
        match call.target {
            FullGuardCallTarget::Scope(id) => {
                let metadata = *bounded(scheme.guards, &store.scheme_guards, "guard call target scope range is invalid")?.get(id as usize).filter(|guard| guard.id == id && guard.parent == IR_NONE).ok_or_else(|| IrVerifyError::new("guard call target scope is not a root guard"))?;
                let scope = GuardScopeId::from_index(id as usize).map_err(|_| IrVerifyError::new("guard call target scope id is invalid"))?;
                return Ok(ResolvedGuardCall { owner, scope, metadata, parent });
            }
            FullGuardCallTarget::Call(id) => {
                call = *bounded(scheme.guard_calls, &store.scheme_guard_calls, "guard call target call range is invalid")?.get(id as usize).ok_or_else(|| IrVerifyError::new("guard call target call is absent"))?;
                ordinal = id;
            }
        }
    }
    Err(IrVerifyError::new("guard call origin chain is cyclic or too deep"))
}

pub(super) fn verify_component_correspondence(store: &FullStore, callee: (&super::FullScheme, &SchemeScope), caller: (&super::FullScheme, &SchemeScope), substitution: &super::SymbolicCallSubstitution, budget: &mut SchemeInstantiationBudget) -> Result<(), IrVerifyError> {
    use crate::sema::effects::EffectContract;
    use crate::sema::schemes::{SchemeEffectParameterId, SchemeRowParameterId, SchemeRowTail, SchemeType, SchemeTypeParameterId, ValueConversion, ValueConversionSet};
    let invalid = || IrVerifyError::new("component guard slot disagrees with its caller slot");
    let ((callee, callee_root), (caller, caller_root)) = ((callee.0, callee.1.with_guard(None)?), (caller.0, caller.1.with_guard(None)?));
    let callee_guards = bounded(callee.guards, &store.scheme_guards, "component callee guard range is invalid")?;
    let caller_guards = bounded(caller.guards, &store.scheme_guards, "component caller guard range is invalid")?;
    if callee_guards.len() != caller_guards.len() { return Err(invalid()); }
    budget.charge(callee_guards.len())?;
    let mut extended = super::SymbolicCallSubstitution {
        types: substitution.types.clone(),
        rows: substitution.rows.iter().map(|row| super::SchemeRowSubstitution { fields: row.fields.clone(), tail: row.tail }).collect(),
        effects: substitution.effects.clone(),
    };
    let scopes = |id: u32, parent: u32| -> Result<(Option<GuardScopeId>, GuardScopeId), IrVerifyError> {
        let parent = if parent == IR_NONE { None } else { Some(GuardScopeId::from_index(parent as usize).map_err(|_| invalid())?) };
        Ok((parent, GuardScopeId::from_index(id as usize).map_err(|_| invalid())?))
    };
    let mut views = Vec::with_capacity(callee_guards.len());
    for (callee_guard, caller_guard) in callee_guards.iter().zip(caller_guards) {
        if callee_guard.id != caller_guard.id || callee_guard.parent != caller_guard.parent || callee_guard.binding_ordinal != caller_guard.binding_ordinal { return Err(invalid()); }
        let (parent, id) = scopes(callee_guard.id, callee_guard.parent)?;
        let (callee_local, caller_local) = (callee_root.with_guard(Some(id))?, caller_root.with_guard(Some(id))?);
        let (callee_pack, caller_pack) = (callee_local.binding_pack()?, caller_local.binding_pack()?);
        if callee_pack.types.len() != caller_pack.types.len() || callee_pack.rows.len() != caller_pack.rows.len() || callee_pack.effects.len() != caller_pack.effects.len()
            || callee_pack.types.start != extended.types.len() || callee_pack.rows.start != extended.rows.len() || callee_pack.effects.start != extended.effects.len()
        { return Err(invalid()); }
        budget.charge(callee_pack.types.len() + callee_pack.rows.len() + callee_pack.effects.len())?;
        extended.types.extend(caller_pack.types.clone().map(|binder| SchemeType::Parameter(SchemeTypeParameterId(caller_local.type_parameters[binder].id))));
        extended.rows.extend(caller_pack.rows.clone().map(|binder| super::SchemeRowSubstitution { fields: std::collections::BTreeMap::new(), tail: SchemeRowTail::Parameter(SchemeRowParameterId(caller_local.row_parameters[binder].id)) }));
        extended.effects.extend(caller_pack.effects.clone().map(|binder| EffectContract::variable(SchemeEffectParameterId(caller_local.effects.parameters[binder].id))));
        views.push((callee_guard, caller_guard, parent, callee_local, caller_local, callee_pack, caller_pack));
    }
    if extended.types.len() != callee_root.type_parameters.len() || extended.rows.len() != callee_root.row_parameters.len() || extended.effects.len() != callee_root.effects.parameters.len() { return Err(invalid()); }
    for (callee_guard, caller_guard, parent, callee_local, caller_local, callee_pack, caller_pack) in views {
        let (callee_parent, caller_parent) = (callee_root.with_guard(parent)?, caller_root.with_guard(parent)?);
        let same = |callee_type, callee_scope: &SchemeScope, caller_type, caller_scope: &SchemeScope, budget: &mut SchemeInstantiationBudget| -> Result<(), IrVerifyError> {
            let actual = super::substitute_scheme_type(&super::decode_scheme_type(store, callee_scope, callee_type, budget, 0)?, callee_scope, &extended, budget, 0)?;
            let required = super::decode_scheme_type(store, caller_scope, caller_type, budget, 0)?;
            if super::body_types_equal(&actual, &required) { Ok(()) } else { Err(invalid()) }
        };
        same(callee_guard.source, &callee_parent, caller_guard.source, &caller_parent, budget)?;
        same(callee_guard.view, &callee_local, caller_guard.view, &caller_local, budget)?;
        let callee_reads = bounded(callee_guard.reads, &store.scheme_guard_reads, "component callee guard reads are invalid")?;
        let caller_reads = bounded(caller_guard.reads, &store.scheme_guard_reads, "component caller guard reads are invalid")?;
        if callee_reads.len() != caller_reads.len() { return Err(invalid()); }
        for (callee_read, caller_read) in callee_reads.iter().zip(caller_reads) {
            same(callee_read.source, &callee_parent, caller_read.source, &caller_parent, budget)?;
            same(callee_read.view, &callee_local, caller_read.view, &caller_local, budget)?;
        }
        match (callee_guard.output, caller_guard.output) {
            (None, None) => {}
            (Some(callee_output), Some(caller_output)) if callee_output.nonnull_when == caller_output.nonnull_when => {
                same(callee_output.outward, &callee_parent, caller_output.outward, &caller_parent, budget)?;
                same(callee_output.absent, &callee_parent, caller_output.absent, &caller_parent, budget)?;
                same(callee_output.present, &callee_local, caller_output.present, &caller_local, budget)?;
            }
            _ => return Err(invalid()),
        }
        let same_contract = |callee_contract, callee_scope: &SchemeScope, caller_contract, caller_scope: &SchemeScope, budget: &mut SchemeInstantiationBudget| -> Result<(), IrVerifyError> {
            let actual = super::effects::substitute_contract(&super::effects::decode_contract(store, callee_scope, callee_contract, budget)?, callee_scope, &extended.effects, budget)?;
            if actual == super::effects::decode_contract(store, caller_scope, caller_contract, budget)? { Ok(()) } else { Err(invalid()) }
        };
        match (callee_guard.effects, caller_guard.effects) {
            (None, None) => {}
            (Some(callee_effects), Some(caller_effects)) => {
                same_contract(callee_effects.outward, &callee_parent, caller_effects.outward, &caller_parent, budget)?;
                same_contract(callee_effects.local, &callee_local, caller_effects.local, &caller_local, budget)?;
            }
            _ => return Err(invalid()),
        }
        for (callee_binder, caller_binder) in callee_pack.types.clone().zip(caller_pack.types.clone()) {
            let (callee_parameter, caller_parameter) = (&callee_local.type_parameters[callee_binder], &caller_local.type_parameters[caller_binder]);
            let required = ValueConversionSet::from_bits(callee_parameter.conversions).ok_or_else(invalid)?;
            let retained = ValueConversionSet::from_bits(caller_parameter.conversions).ok_or_else(invalid)?;
            if ValueConversion::ALL.into_iter().any(|conversion| required.contains(conversion) && !retained.contains(conversion)) || callee_parameter.map_key && !caller_parameter.map_key { return Err(invalid()); }
        }
        for (callee_binder, caller_binder) in callee_pack.rows.clone().zip(caller_pack.rows.clone()) {
            if !callee_local.row_lacks[callee_binder].is_subset(&caller_local.row_lacks[caller_binder]) { return Err(invalid()); }
        }
        super::verify_symbolic_iterations(store, &callee_local, &caller_local, &extended, budget)?;
        let (callee_requirements, caller_requirements) = (callee_local.owned_requirements(callee)?, caller_local.owned_requirements(caller)?);
        if callee_requirements.guard_calls.len != 0 || caller_requirements.guard_calls.len != 0
            || callee_requirements.applications.len != caller_requirements.applications.len || callee_requirements.operations.len != caller_requirements.operations.len
            || callee_requirements.methods.len != caller_requirements.methods.len || callee_requirements.wrappers.len != caller_requirements.wrappers.len
        { return Err(invalid()); }
        let callee_predicates = bounded(callee_requirements.effect_predicates, &store.scheme_effect_predicates, "component callee guard predicates are invalid")?;
        let caller_predicates = bounded(caller_requirements.effect_predicates, &store.scheme_effect_predicates, "component caller guard predicates are invalid")?;
        if callee_predicates.len() != caller_predicates.len() { return Err(invalid()); }
        for (callee_predicate, caller_predicate) in callee_predicates.iter().zip(caller_predicates) {
            budget.charge(1)?;
            let actual = super::effects::decode_predicate(store, &callee_local, *callee_predicate, budget)?.try_map_contracts(|contract| super::effects::substitute_contract(contract, &callee_local, &extended.effects, budget))?;
            if actual != super::effects::decode_predicate(store, &caller_local, *caller_predicate, budget)? { return Err(invalid()); }
        }
        let symbolic = super::applications::ApplicationSubstitution::Symbolic(&extended);
        let forwarded = |count: u32| 0..count;
        let operations = forwarded(callee_requirements.operations.len).map(crate::sema::schemes::SealedOperationBinding::Forwarded).collect::<Vec<_>>();
        super::applications::verify_bindings(store, callee, &callee_local, symbolic, Some((caller, &caller_local)), &forwarded(callee_requirements.applications.len).map(super::super::schemes::FullApplicationBinding::Forwarded).collect::<Vec<_>>(), budget)?;
        super::operations::verify_bindings(store, callee, &callee_local, symbolic, Some((caller, &caller_local)), &operations, budget)?;
        super::methods::verify_bindings(store, callee, &callee_local, symbolic, Some((caller, &caller_local)), &forwarded(callee_requirements.methods.len).map(crate::sema::schemes::SealedMethodBinding::Forwarded).collect::<Vec<_>>(), budget)?;
        super::wrappers::verify_bindings(store, callee, &callee_local, symbolic, Some((caller, &caller_local)), &operations, &forwarded(callee_requirements.wrappers.len).map(crate::sema::wrapper_operation::WrapperBinding::Forwarded).collect::<Vec<_>>(), budget)?;
    }
    Ok(())
}
