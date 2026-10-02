use super::*;
use super::super::generic::{NativeCallableContract, NativeCallableSource, PreparedNativeCallableValue, GroundNativeInvocationContract, NativeInvocationSource, PreparedNativeInvocationPlan, PreparedOperationAuthority, PreparedOperationEffects, GroundNativeCallContract, PreparedOperationBinding, PreparedInvocationArgument};
use crate::sema::inference::{TypeNode, NativeAuthority, CallableAuthority, EffectSummary, EffectRoleReference, InvocationDefaultTiming, OperationBinding, RequirementTemplate};

fn problem(message: &'static str) -> IrBuildError { IrBuildError::format(message, None, 0, 0) }

impl FullBuilder {
    pub(super) fn intern_checked_native_callable_type(&mut self, graph: &crate::sema::inference::SolvedGraph, ty: crate::sema::inference::TypeId) -> Result<TypeId, IrBuildError> {
        let ty = graph.resolved(ty).map_err(|_| problem("native_callable_storage_owner"))?;
        let TypeNode::NativeCallable(wrapper) = graph.node(ty).map_err(|_| problem("native_callable_storage_owner"))? else { return Err(problem("native_callable_storage_protocol")); };
        let [CallableAuthority::Native { authority: NativeAuthority::Single(member) }] = wrapper.alternatives.as_slice() else { return Err(problem("native_callable_choice_not_prepared")); };
        let contract = graph.native_contract(*member).map_err(|_| problem("native_callable_storage_contract"))?;
        self.intern_checked_callable_type(graph, contract.instance.ty)
    }

    pub(super) fn prepare_native_callable_values(&mut self) -> Result<(), IrBuildError> {
        let Some(solved) = self.solved.clone() else { return Ok(()); };
        let graph = &solved.graph;
        let mut values = FxHashMap::default();
        let origins = self.generic_expression_rows.iter().map(|&(instruction, expression, owner)| (instruction, (expression, owner))).collect::<FxHashMap<_, _>>();
        for (instruction, expression, owner) in self.generic_expression_rows.clone() {
            if self.store.tags[instruction as usize] != FullTag::ExprNativeCallableRef { continue; }
            let reference = solved.registry_references.get(&expression).ok_or_else(|| problem("native_callable_original_reference_missing"))?;
            let Some(NativeAuthority::Single(member)) = reference.native_authority() else { return Err(problem("native_callable_choice_not_prepared")); };
            let callable = reference.value_type();
            let scope = reference.caller.and_then(|declaration| self.generic_declarations.get(&declaration).copied());
            let source_scope = solved.expression_scope(expression, reference.caller).map_err(|_| problem("native_callable_original_scope"))?;
            graph.validate_native_contract_scoped(member, source_scope).map_err(|_| problem("native_callable_original_certificate"))?;
            graph.validate_scoped(crate::sema::inference::ScopedRoot { ty: callable, scope: source_scope }).map_err(|_| problem("native_callable_original_type"))?;
            if solved.expression_owners.get(&expression).copied() != reference.caller || solved.expressions.get(&expression) != Some(&callable) { return Err(problem("native_callable_original_expression_changed")); }
            let original = graph.native_contract(member).map_err(|_| problem("native_callable_original_contract"))?;
            let template = graph.candidate(original.candidate).map_err(|_| problem("native_callable_original_candidate"))?;
            let crate::sema::check::SolvedOperationAuthority::Registry(metadata) = solved.operation_catalog.candidate(graph, original.candidate).map_err(|_| problem("native_callable_original_authority"))? else { return Err(problem("native_callable_original_authority")); };
            if metadata.binding != crate::modules::signature::ImplBinding::Native || metadata.semantic_rule != crate::modules::signature::SemanticRule::Standard
                || !matches!(metadata.owner, crate::sema::registry_graph::RegistryOwner::Module(_)) || template.has_receiver { return Err(problem("native_callable_registry_protocol_not_prepared")); }
            let descriptor = self.intern_checked_native_callable_type(graph, callable)?;
            let (kind, signature) = self.store.semantic.callable_descriptor(descriptor).map_err(|_| problem("native_callable_descriptor"))?.ok_or_else(|| problem("native_callable_descriptor"))?;
            let closed = |summary| match graph.closed_effect_summary(summary).map_err(|_| problem("native_callable_effect_owner"))? { EffectSummary::Closed(bits) => Ok(bits), _ => Err(problem("native_callable_effect_scope_not_prepared")) };
            let creation = self.store.semantic.signature_closed_effects(signature).map_err(|_| problem("native_callable_effect_descriptor"))?;
            let inputs = template.effect_roles.iter().map(|&(role, root)| match root {
                EffectRoleReference::Fixed(bits) => Ok((role, bits)),
                EffectRoleReference::Binder(index) => closed(EffectSummary::Variable(*original.instance.effect_substitutions.get(index as usize).ok_or_else(|| problem("native_callable_input_root"))?)).map(|bits| (role, bits)),
            }).collect::<Result<Vec<_>, _>>()?;
            let outputs = template.output_effect_roles.iter().map(|&(role, index)| closed(*original.instance.effect_roots.get(index as usize).ok_or_else(|| problem("native_callable_output_root"))?).map(|bits| (role, bits))).collect::<Result<Vec<_>, _>>()?;
            let contract = NativeCallableContract {
                authority: PreparedOperationAuthority::Registry { identity: metadata.identity, operation: metadata.operation, binding: metadata.binding, argument_check: metadata.argument_check,
                    semantic_rule: metadata.semantic_rule, lifecycle: metadata.lifecycle, producer_transfer: metadata.producer_transfer.clone() },
                registry_owner: metadata.owner, signature, kind, effects: PreparedOperationEffects { creation, inputs: inputs.into_boxed_slice(), outputs: outputs.into_boxed_slice() },
                input_eligibility: template.actual_eligibility.clone().into_boxed_slice(), argument_relations: template.argument_relations.clone().into_boxed_slice(),
            };
            let id = self.generic_evidence_mut().add_native_callable_value(PreparedNativeCallableValue {
                source: NativeCallableSource { origin: expression, instruction, owner, scope }, contract,
            }).map_err(|_| problem("native_callable_value_allocation"))?;
            let origin = graph.native_contract_origin(member).map_err(|_| problem("native_callable_original_origin"))?;
            if values.insert(origin, id).is_some() { return Err(problem("native_callable_duplicate_original_creation")); }
        }
        for (instruction, expression, owner) in self.generic_expression_rows.clone() {
            if self.store.tags[instruction as usize] != FullTag::ExprDynamicCall { continue; }
            let Some(invocation) = solved.invocations.get(&expression) else { continue; };
            let Some(evidence) = graph.invocation_evidence(invocation.requirement).map_err(|_| problem("native_invocation_original_evidence"))? else {
                let RequirementTemplate::CallableInvocation { call } = graph.requirement_template(invocation.requirement).map_err(|_| problem("native_invocation_original_requirement"))? else { return Err(problem("native_invocation_original_requirement")); };
                let callable = graph.invocation_call(call).map_err(|_| problem("native_invocation_original_requirement"))?.callable;
                if matches!(graph.node(graph.resolved(callable).map_err(|_| problem("native_invocation_original_callable"))?).map_err(|_| problem("native_invocation_original_callable"))?, TypeNode::NativeCallable(_)) { return Err(problem("native_invocation_requires_scope")); }
                continue;
            };
            if evidence.native_alternatives.is_empty() { continue; }
            let [alternative] = evidence.native_alternatives.as_slice() else { return Err(problem("native_invocation_all_not_prepared")); };
            let NativeAuthority::Single(member) = alternative.authority else { return Err(problem("native_invocation_choice_not_prepared")); };
            let origin = graph.native_contract_origin(member).map_err(|_| problem("native_invocation_authority_origin"))?;
            let callable = *values.get(&origin).ok_or_else(|| problem("native_invocation_original_creation_missing"))?;
            let value = self.generic.as_ref().unwrap().native_callable_value(callable).map_err(|_| problem("native_invocation_value_owner"))?.clone();
            let (signature, binding, timing) = evidence.unique_plan().ok_or_else(|| problem("native_invocation_all_not_prepared"))?;
            if timing != InvocationDefaultTiming::AtCall || binding.dynamic.is_some() || binding.rest_slot.is_some() { return Err(problem("native_invocation_binding_not_prepared")); }
            let descriptor = self.intern_checked_callable_type(graph, signature)?;
            if self.store.semantic.callable_descriptor(descriptor).map_err(|_| problem("native_invocation_descriptor"))? != Some((value.contract.kind, value.contract.signature)) { return Err(problem("native_invocation_original_signature_changed")); }
            let source_scope = solved.expression_scope(expression, invocation.caller).map_err(|_| problem("native_invocation_original_scope"))?;
            if solved.expression_owners.get(&expression).copied() != invocation.caller { return Err(problem("native_invocation_original_caller_changed")); }
            graph.validate_requirement_scoped(crate::sema::inference::ScopedRequirementRoot { requirement: invocation.requirement, scope: source_scope }).map_err(|_| problem("native_invocation_original_certificate"))?;
            let result = *solved.expressions.get(&expression).ok_or_else(|| problem("native_invocation_original_result_missing"))?;
            graph.validate_scoped(crate::sema::inference::ScopedRoot { ty: result, scope: source_scope }).map_err(|_| problem("native_invocation_original_result_scope"))?;
            let result = super::super::generic::graph_ground_type(graph, result).map_err(|_| problem("native_invocation_original_result_requires_scope"))?;
            if self.store.semantic.to_type(self.store.semantic.signature_return_type(value.contract.signature).map_err(|_| problem("native_invocation_original_result"))?).map_err(|_| problem("native_invocation_original_result"))? != result { return Err(problem("native_invocation_original_result_changed")); }
            let candidate = graph.candidate_evidence(alternative.operation).map_err(|_| problem("native_invocation_original_candidate"))?.ok_or_else(|| problem("native_invocation_original_candidate"))?;
            let original = graph.native_contract(member).map_err(|_| problem("native_invocation_original_contract"))?;
            if candidate.candidate != original.candidate { return Err(problem("native_invocation_original_candidate_changed")); }
            let RequirementTemplate::Operation { family, call } = graph.requirement_template(alternative.operation).map_err(|_| problem("native_invocation_operation_owner"))? else { return Err(problem("native_invocation_operation_kind")); };
            let operation = graph.operation_call(call).map_err(|_| problem("native_invocation_operation_owner"))?;
            if family != original.family || operation.receiver.is_some() || operation.mono_authority != Some(alternative.authority) { return Err(problem("native_invocation_original_authority_changed")); }
            if !matches!(operation.binding, OperationBinding::Slots | OperationBinding::Invocation(_)) { return Err(problem("native_invocation_original_binding")); }
            let words = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| problem("native_invocation_payload"))?;
            let callee_instruction = *words.first().ok_or_else(|| problem("native_invocation_callee"))?;
            let &(callee_origin, callee_owner) = origins.get(&callee_instruction).ok_or_else(|| problem("native_invocation_original_callee"))?;
            if callee_owner != owner || solved.expression_owners.get(&callee_origin).copied() != invocation.caller { return Err(problem("native_invocation_original_callee_owner")); }
            let callee_type = *solved.expressions.get(&callee_origin).ok_or_else(|| problem("native_invocation_original_callee_type"))?;
            let callee_scope = solved.expression_scope(callee_origin, invocation.caller).map_err(|_| problem("native_invocation_original_callee_scope"))?;
            graph.validate_scoped(crate::sema::inference::ScopedRoot { ty: callee_type, scope: callee_scope }).map_err(|_| problem("native_invocation_original_callee_scope"))?;
            let TypeNode::NativeCallable(wrapper) = graph.node(graph.resolved(callee_type).map_err(|_| problem("native_invocation_original_callee_type"))?).map_err(|_| problem("native_invocation_original_callee_type"))? else { return Err(problem("native_invocation_original_callee_authority")); };
            let [CallableAuthority::Native { authority: NativeAuthority::Single(callee_member) }] = wrapper.alternatives.as_slice() else { return Err(problem("native_invocation_original_callee_authority")); };
            if graph.native_contract_origin(*callee_member).map_err(|_| problem("native_invocation_original_callee_authority"))? != origin { return Err(problem("native_invocation_original_callee_authority")); }
            let callee_slot = match self.store.tags[callee_instruction as usize] {
                FullTag::ExprParam => Some(*self.store.payload(self.store.data[callee_instruction as usize].range()).map_err(|_| problem("native_invocation_callee_slot"))?.first().ok_or_else(|| problem("native_invocation_callee_slot"))?),
                FullTag::ExprNativeCallableRef => None,
                _ => return Err(problem("native_invocation_callee_transport_not_prepared")),
            };
            let argument_sources = encoded_arguments(&self.store, instruction, &self.store.semantic, value.contract.signature)?;
            let recipes = solved.argument_sources.get(&expression).ok_or_else(|| problem("native_invocation_original_recipes"))?;
            if recipes.len() != binding.supplied_slots.len() || candidate.actual_arguments.len() != argument_sources.len() { return Err(problem("native_invocation_original_binding_count")); }
            let mut arguments = Vec::new();
            let mut operands = Vec::new();
            for (ordinal, (recipe, &slot)) in recipes.iter().zip(&binding.supplied_slots).enumerate() {
                let instruction = argument_sources.get(slot).copied().flatten().ok_or_else(|| problem("native_invocation_original_operand"))?;
                let origin = self.original_argument_expression(instruction, expression, ordinal, recipe, owner)?;
                let original = *solved.expressions.get(&origin).ok_or_else(|| problem("native_invocation_original_operand_type"))?;
                let selected = candidate.actual_arguments[slot].ok_or_else(|| problem("native_invocation_selected_operand"))?;
                let original_type = super::super::generic::graph_ground_type(graph, original).map_err(|_| problem("native_invocation_operand_scope_not_prepared"))?;
                if super::super::generic::graph_ground_type(graph, selected).map_err(|_| problem("native_invocation_selected_operand_scope"))? != original_type { return Err(problem("native_invocation_original_operand_changed")); }
                let ty = TypeRef::Ground(self.intern_generic_ground_type(&original_type)?);
                arguments.push(PreparedInvocationArgument { original: recipe.clone(), instruction, ty }); operands.push(instruction);
            }
            for (slot, source) in argument_sources.iter().enumerate() {
                if source.is_some() != binding.supplied_slots.contains(&slot) || source.is_none() != binding.default_slots.contains(&slot)
                    || source.is_some() != candidate.actual_arguments[slot].is_some() { return Err(problem("native_invocation_original_default_mask_changed")); }
            }
            let call = GroundNativeCallContract { authority: value.contract.authority.clone(), registry_owner: value.contract.registry_owner,
                signature: value.contract.signature, kind: value.contract.kind, result: TypeRef::Ground(self.store.semantic.signature_return_type(value.contract.signature).map_err(|_| problem("native_invocation_result"))?),
                effects: value.contract.effects.clone(), argument_relations: value.contract.argument_relations.clone(), input_eligibility: value.contract.input_eligibility.clone(), arguments: arguments.into_boxed_slice(), argument_sources: argument_sources.into_boxed_slice(),
                binding: PreparedOperationBinding { supplied_slots: binding.supplied_slots.iter().map(|&slot| slot as u32).collect(), default_slots: binding.default_slots.iter().map(|&slot| slot as u32).collect(), rest_slot: None, dynamic: None, operands: operands.into_boxed_slice() } };
            let scope = invocation.caller.and_then(|declaration| self.generic_declarations.get(&declaration).copied());
            self.generic_evidence_mut().add_native_invocation_plan(PreparedNativeInvocationPlan {
                source: NativeInvocationSource { origin: expression, instruction, owner, scope },
                contract: GroundNativeInvocationContract { callee_instruction, callee_origin, callee_slot, callable, call, timing },
            }).map_err(|_| problem("native_invocation_plan_allocation"))?;
        }
        Ok(())
    }
}

fn encoded_arguments(store: &FullStore, instruction: u32, semantic: &SemanticPools, signature: SignatureId) -> Result<Vec<Option<u32>>, IrBuildError> {
    let words = store.payload(store.data[instruction as usize].range()).map_err(|_| problem("native_invocation_arguments"))?;
    let block = words.get(1).copied().and_then(IrBlockId::from_raw).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| problem("native_invocation_arguments"))?;
    let words = store.payload(block.instructions).map_err(|_| problem("native_invocation_arguments"))?;
    let count = semantic.signature_param_count(signature).map_err(|_| problem("native_invocation_signature"))?;
    let supplied = words.first().copied().ok_or_else(|| problem("native_invocation_arguments"))? as usize;
    if count > 65536 || supplied > count || words.len() != 1 + supplied * 2 { return Err(problem("native_invocation_argument_arity")); }
    let mut sources = words[1..].chunks_exact(2).enumerate().map(|(slot, words)| match words[0] {
        0 => Ok(Some(words[1])),
        2 if words[1] as usize == slot && semantic.signature_parameter_defaulted(signature, slot).map_err(|_| problem("native_invocation_default"))? => Ok(None),
        _ => Err(problem("native_invocation_splice_not_prepared")),
    }).collect::<Result<Vec<_>, _>>()?;
    for slot in supplied..count { if !semantic.signature_parameter_defaulted(signature, slot).map_err(|_| problem("native_invocation_default"))? { return Err(problem("native_invocation_missing_required_operand")); } sources.push(None); }
    Ok(sources)
}

impl FullVerifier {
    pub(super) fn verify_native_callable_values(store: &FullStore, generic: &GenericEvidenceStore) -> Result<(), IrVerifyError> {
        for (_, value) in generic.native_callable_values() {
            if store.tags.get(value.source.instruction as usize) != Some(&FullTag::ExprNativeCallableRef)
                || !store.payload(store.data[value.source.instruction as usize].range())?.is_empty() { return Err(IrVerifyError::new("native callable creation changes its original opcode")); }
        }
        for (_, plan) in generic.native_invocation_plans() {
            let source = &plan.source;
            let contract = &plan.contract;
            if store.tags.get(source.instruction as usize) != Some(&FullTag::ExprDynamicCall) { return Err(IrVerifyError::new("native invocation proof is attached to another opcode")); }
            let words = store.payload(store.data[source.instruction as usize].range())?;
            if words.first() != Some(&contract.callee_instruction) { return Err(IrVerifyError::new("native invocation changes its original callee")); }
            match contract.callee_slot {
                Some(slot) if store.tags.get(contract.callee_instruction as usize) == Some(&FullTag::ExprParam)
                    && store.payload(store.data[contract.callee_instruction as usize].range())? == [slot] => {},
                None if store.tags.get(contract.callee_instruction as usize) == Some(&FullTag::ExprNativeCallableRef)
                    && generic.native_callable_value_at(contract.callee_instruction)? == Some(contract.callable) => {},
                _ => return Err(IrVerifyError::new("native invocation changes its original callee carrier")),
            }
            let arguments = encoded_arguments(store, source.instruction, &store.semantic, contract.call.signature).map_err(|_| IrVerifyError::new("native invocation encoded packet differs from its selected signature"))?;
            if arguments.as_slice() != contract.call.argument_sources.as_ref() { return Err(IrVerifyError::new("native invocation changes its original supplied or default packet")); }
            for argument in &contract.call.arguments {
                let TypeRef::Ground(ty) = argument.ty else { return Err(IrVerifyError::new("native invocation operand has no prepared ground proof")); };
                Self::verify_generic_source(store, generic, argument.instruction, source.owner, &store.semantic.to_type(ty)?, None, &mut Vec::new())?;
            }
        }
        for (instruction, tag) in store.tags.iter().enumerate() {
            if *tag == FullTag::ExprNativeCallableRef && generic.native_callable_value_at(instruction as u32)?.is_none() { return Err(IrVerifyError::new("native callable creation lacks its original registry proof")); }
        }
        Ok(())
    }
}
