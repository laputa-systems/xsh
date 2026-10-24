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

fn source(store: &FullStore, call: FullGuardCall, budget: &mut SchemeInstantiationBudget) -> Result<(FullInstantiation, FullSchemeId), IrVerifyError> {
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
    let function = store.functions.get(target.index()).ok_or_else(|| IrVerifyError::new("guard call target is out of bounds"))?;
    let FullCallableContract::Scheme(owner) = function.contract else { return Err(IrVerifyError::new("guard call target has no scoped requirement contract")); };
    Ok((arguments, owner))
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

pub(super) fn verify_forwarded_bindings(store: &FullStore, callee: &super::FullScheme, callee_scope: &SchemeScope, caller: &SchemeScope, instruction: u32, arguments: FullInstantiation, budget: &mut SchemeInstantiationBudget) -> Result<bool, IrVerifyError> {
    let caller = caller.at_instruction(instruction as usize)?;
    let calls = bounded(caller.guard_calls, &store.scheme_guard_calls, "forwarded guard call source range is invalid")?;
    let guards = bounded(callee.guards, &store.scheme_guards, "forwarded guard target range is invalid")?;
    let roots = guards.iter().filter(|guard| guard.parent == IR_NONE).collect::<Vec<_>>();
    let physical = bounded(arguments.guard_bindings, &store.call_guard_bindings, "forwarded guard binding range is invalid")?;
    let residual = bounded(arguments.guard_call_bindings, &store.call_guard_bindings, "forwarded guard call binding range is invalid")?;
    let target_calls = bounded(callee_scope.guard_calls, &store.scheme_guard_calls, "forwarded target call range is invalid")?;
    if roots.len() != physical.len() || target_calls.len() != residual.len() { return Err(IrVerifyError::new("forwarded guard bindings are incomplete")); }
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

pub(in crate::runtime::eval::indexed::full) fn resolve(store: &FullStore, mut call: FullGuardCall, mut ordinal: u32, caller: Arc<CallSubstitution>, budget: &mut SchemeInstantiationBudget) -> Result<ResolvedGuardCall, IrVerifyError> {
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
