use super::*;
use super::super::generic::{GroundNativeCallContract, NativeScalarReceiver, NativeScalarSource, PreparedOperationAuthority, PreparedOperationBinding, PreparedOperationEffects, graph_ground_type};
use crate::sema::check::{BindingIdentity, ExpressionIdentity};
use crate::sema::inference::{EffectSummary, OperationBinding, RequirementTemplate, TypeNode};

#[cfg(test)]
mod tests;
mod byte_at_fallback;
pub(in crate::runtime::eval) use byte_at_fallback::BuildByteAtFallbackOriginal;

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct BuildFoldedNativeReceiver {
    pub origin: ExpressionIdentity,
    pub name: Name,
    pub slot: u32,
    pub binding: Option<BindingIdentity>,
}

fn problem(message: &'static str) -> IrBuildError { IrBuildError::format(message, None, 0, 0) }

impl FullBuilder {
    pub(super) fn stage_folded_native_receiver(&mut self, instruction: u32, original: BuildFoldedNativeReceiver, owner: InstructionOwner) {
        self.folded_native_receiver_rows.push((instruction, original, owner));
    }

    pub(super) fn prepare_native_scalar_sources(&mut self) -> Result<(), IrBuildError> {
        self.prepare_byte_at_fallback_sources()?;
        self.prepare_native_expression_scalar_sources()?;
        if self.folded_native_receiver_rows.is_empty() { return Ok(()); }
        let Some(solved) = self.solved.clone() else { return Ok(()); };
        let origins = self.generic_expression_rows.iter().map(|&(instruction, origin, owner)| (instruction, (origin, owner))).collect::<FxHashMap<_, _>>();
        let mut bindings = BTreeMap::new();
        if let Some(generic) = &self.generic {
            for row in generic.native_scalar_binding_sources() {
                let (binding, application) = row.map_err(|_| problem("native_scalar_binding_owner"))?;
                if bindings.insert(binding, application).is_some() { return Err(problem("native_scalar_binding_ambiguous")); }
            }
        }
        for (instruction, original, owner) in self.folded_native_receiver_rows.clone() {
            let &(origin, source_owner) = origins.get(&instruction).ok_or_else(|| problem("native_scalar_original_source_missing"))?;
            if owner != source_owner || self.store.tags.get(instruction as usize) != Some(&FullTag::IntStrByteLenSlot) { return Err(problem("native_scalar_original_material_kind")); }
            let operation = solved.operations.get(&origin).ok_or_else(|| problem("native_scalar_original_operation_missing"))?;
            let graph = &solved.graph;
            let scope = solved.expression_scope(origin, operation.caller).map_err(|_| problem("native_scalar_original_scope"))?;
            graph.validate_requirement_scoped(crate::sema::inference::ScopedRequirementRoot { requirement: operation.requirement, scope }).map_err(|_| problem("native_scalar_original_certificate"))?;
            let selected = graph.candidate_evidence(operation.requirement).map_err(|_| problem("native_scalar_candidate_owner"))?.ok_or_else(|| problem("native_scalar_original_candidate_missing"))?;
            let crate::sema::check::SolvedOperationAuthority::Registry(metadata) = solved.operation_catalog.candidate(graph, selected.candidate).map_err(|_| problem("native_scalar_original_authority"))? else { return Err(problem("native_scalar_original_authority")); };
            if metadata.operation != RuntimeOp::TextByteLen || metadata.owner != crate::sema::registry_graph::RegistryOwner::Method(crate::modules::signature::MethodReceiver::Str)
                || metadata.binding != crate::modules::signature::ImplBinding::Native || metadata.semantic_rule != crate::modules::signature::SemanticRule::Standard
                || !operation.actual_arguments.is_empty() || !operation.binding.supplied_slots.is_empty() || !operation.binding.default_slots.is_empty()
                || operation.binding.rest_slot.is_some() || operation.binding.dynamic.is_some() || !operation.argument_coercions.is_empty() || !selected.callback_invocations.is_empty() {
                return Err(problem("native_scalar_original_selected_contract"));
            }
            let RequirementTemplate::Operation { call, .. } = graph.requirement_template(operation.requirement).map_err(|_| problem("native_scalar_original_requirement"))? else { return Err(problem("native_scalar_original_requirement")); };
            let call = graph.operation_call(call).map_err(|_| problem("native_scalar_original_call"))?;
            if call.binding != OperationBinding::Slots || call.receiver.is_none() || operation.receiver.is_none() { return Err(problem("native_scalar_original_receiver_missing")); }
            let checked = *solved.expressions.get(&original.origin).ok_or_else(|| problem("native_scalar_original_receiver_type_missing"))?;
            let receiver_scope = solved.expression_scope(original.origin, operation.caller).map_err(|_| problem("native_scalar_original_receiver_scope"))?;
            graph.validate_scoped(crate::sema::inference::ScopedRoot { ty: checked, scope: receiver_scope }).map_err(|_| problem("native_scalar_original_receiver_certificate"))?;
            if solved.expression_owners.get(&original.origin).copied() != operation.caller || graph_ground_type(graph, checked).map_err(|_| problem("native_scalar_receiver_type"))? != Type::Str
                || graph_ground_type(graph, operation.receiver.unwrap()).map_err(|_| problem("native_scalar_receiver_type"))? != Type::Str
                || graph_ground_type(graph, call.receiver.unwrap()).map_err(|_| problem("native_scalar_receiver_type"))? != Type::Str { return Err(problem("native_scalar_original_receiver_changed")); }
            let signature_root = graph.resolved(selected.signature).map_err(|_| problem("native_scalar_signature_owner"))?;
            let TypeNode::Arrow(arrow) = graph.node(signature_root).map_err(|_| problem("native_scalar_signature_owner"))? else { return Err(problem("native_scalar_signature_kind")); };
            if arrow.params.len() != 1 || arrow.params[0].label != Name::intern("<receiver>") || arrow.params[0].defaulted || arrow.params[0].rest
                || graph_ground_type(graph, arrow.params[0].ty).map_err(|_| problem("native_scalar_signature_receiver"))? != Type::Str
                || graph_ground_type(graph, selected.result).map_err(|_| problem("native_scalar_selected_result"))? != Type::Int
                || graph_ground_type(graph, *solved.expressions.get(&origin).ok_or_else(|| problem("native_scalar_original_result_missing"))?).map_err(|_| problem("native_scalar_original_result"))? != Type::Int { return Err(problem("native_scalar_original_signature_changed")); }
            let descriptor = self.intern_checked_callable_type(graph, signature_root)?;
            let (kind, signature) = self.store.semantic.callable_descriptor(descriptor).map_err(|_| problem("native_scalar_signature_descriptor"))?.ok_or_else(|| problem("native_scalar_signature_descriptor"))?;
            let receiver_type = self.intern_generic_ground_type(&Type::Str)?;
            let result = self.intern_generic_ground_type(&Type::Int)?;
            let receiver = if let Some(binding) = original.binding {
                let application = *bindings.get(&binding).ok_or_else(|| problem("native_scalar_original_binding_missing"))?;
                NativeScalarReceiver::Binding { binding, application, slot: original.slot }
            } else {
                let declaration = operation.caller.ok_or_else(|| problem("native_scalar_original_parameter_owner"))?;
                let callable = solved.declarations.get(&declaration).ok_or_else(|| problem("native_scalar_original_declaration_missing"))?;
                let TypeNode::Arrow(arrow) = graph.node(graph.resolved(callable.signature).map_err(|_| problem("native_scalar_original_declaration_type"))?).map_err(|_| problem("native_scalar_original_declaration_type"))? else { return Err(problem("native_scalar_original_declaration_kind")); };
                let parameter = arrow.params.get(original.slot as usize).ok_or_else(|| problem("native_scalar_original_parameter_slot"))?;
                if parameter.label != original.name || graph.resolved(parameter.ty).map_err(|_| problem("native_scalar_original_parameter_type"))? != graph.resolved(checked).map_err(|_| problem("native_scalar_original_parameter_type"))? { return Err(problem("native_scalar_original_parameter_changed")); }
                let InstructionOwner::Function(function) = owner else { return Err(problem("native_scalar_parameter_material_owner")); };
                if let Some(&scope) = self.generic_declarations.get(&declaration) {
                    NativeScalarReceiver::ScopedParameter { scope, name: original.name, slot: original.slot }
                } else {
                    let signature = SignatureId::from_raw(self.store.functions[function.index()].signature).ok_or_else(|| problem("native_scalar_original_parameter_signature"))?;
                    NativeScalarReceiver::Parameter { declaration, signature, name: original.name, slot: original.slot }
                }
            };
            let closed = |summary| match graph.closed_effect_summary(summary).map_err(|_| problem("native_scalar_effect_owner"))? { EffectSummary::Closed(bits) => Ok(bits), _ => Err(problem("native_scalar_effect_requires_scope")) };
            let effects = PreparedOperationEffects { creation: closed(selected.effects)?, inputs: call.effect_bindings.iter().map(|&(role, summary)| closed(summary).map(|bits| (role, bits))).collect::<Result<Vec<_>, _>>()?.into_boxed_slice(), outputs: call.output_effect_bindings.iter().map(|&(role, summary)| closed(summary).map(|bits| (role, bits))).collect::<Result<Vec<_>, _>>()?.into_boxed_slice() };
            let template = graph.candidate(selected.candidate).map_err(|_| problem("native_scalar_original_candidate_template"))?;
            let contract = GroundNativeCallContract {
                authority: PreparedOperationAuthority::Registry { identity: metadata.identity, operation: metadata.operation, binding: metadata.binding, argument_check: metadata.argument_check, semantic_rule: metadata.semantic_rule, lifecycle: metadata.lifecycle, producer_transfer: metadata.producer_transfer.clone() },
                registry_owner: metadata.owner, receiver: None, cli_descriptor: None, process_command_argv: None, signature, kind, result: TypeRef::Ground(result), effects,
                arguments: Box::new([]), binding: PreparedOperationBinding { supplied_slots: Box::new([]), default_slots: Box::new([]), rest_slot: None, dynamic: None, operands: Box::new([]) },
                argument_sources: Box::new([]), argument_relations: template.argument_relations.clone().into_boxed_slice(), input_eligibility: template.actual_eligibility.clone().into_boxed_slice(),
            };
            let payload = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| problem("native_scalar_material_payload"))?.into();
            self.generic_evidence_mut().add_native_scalar_source(NativeScalarSource { origin, receiver_origin: original.origin, instruction, owner, receiver, receiver_type, contract, byte_at_fallback: None, payload }).map_err(|_| problem("native_scalar_original_allocation"))?;
        }
        Ok(())
    }

    fn prepare_native_expression_scalar_sources(&mut self) -> Result<(), IrBuildError> {
        let Some(solved) = self.solved.clone() else { return Ok(()); };
        let graph = &solved.graph;
        let origins = self.generic_expression_rows.iter().map(|&(instruction, origin, owner)| (instruction, (origin, owner))).collect::<FxHashMap<_, _>>();
        for (instruction, origin, owner) in self.generic_expression_rows.clone() {
            let tag = self.store.tags[instruction as usize];
            if !matches!(tag, FullTag::ExprStrByteLen | FullTag::ExprMethod) { continue; }
            let Some(operation) = solved.operations.get(&origin) else { continue; };
            let Some(selected) = graph.candidate_evidence(operation.requirement).map_err(|_| problem("native_scalar_candidate_owner"))? else { continue; };
            let crate::sema::check::SolvedOperationAuthority::Registry(metadata) = solved.operation_catalog.candidate(graph, selected.candidate).map_err(|_| problem("native_scalar_original_authority"))? else { continue; };
            if metadata.owner != crate::sema::registry_graph::RegistryOwner::Method(crate::modules::signature::MethodReceiver::Str)
                || !matches!(metadata.operation, RuntimeOp::TextByteLen | RuntimeOp::TextCountLines | RuntimeOp::TextCountWords | RuntimeOp::TextCountChars) { continue; }
            let scope = solved.expression_scope(origin, operation.caller).map_err(|_| problem("native_scalar_original_scope"))?;
            graph.validate_requirement_scoped(crate::sema::inference::ScopedRequirementRoot { requirement: operation.requirement, scope }).map_err(|_| problem("native_scalar_original_certificate"))?;
            if metadata.binding != crate::modules::signature::ImplBinding::Native || metadata.semantic_rule != crate::modules::signature::SemanticRule::Standard
                || !operation.actual_arguments.is_empty() || !operation.binding.supplied_slots.is_empty() || !operation.binding.default_slots.is_empty()
                || operation.binding.rest_slot.is_some() || operation.binding.dynamic.is_some() || !operation.argument_coercions.is_empty() || !selected.callback_invocations.is_empty() {
                return Err(problem("native_scalar_original_selected_contract"));
            }
            let RequirementTemplate::Operation { call, .. } = graph.requirement_template(operation.requirement).map_err(|_| problem("native_scalar_original_requirement"))? else { return Err(problem("native_scalar_original_requirement")); };
            let call = graph.operation_call(call).map_err(|_| problem("native_scalar_original_call"))?;
            if call.binding != OperationBinding::Slots || call.receiver.is_none() || operation.receiver.is_none() { return Err(problem("native_scalar_original_receiver_missing")); }
            let words = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| problem("native_scalar_material_payload"))?;
            let receiver_instruction = *words.first().ok_or_else(|| problem("native_scalar_material_receiver_missing"))?;
            let &(receiver_origin, receiver_owner) = origins.get(&receiver_instruction).ok_or_else(|| problem("native_scalar_material_receiver_original_missing"))?;
            let checked = *solved.expressions.get(&receiver_origin).ok_or_else(|| problem("native_scalar_original_receiver_type_missing"))?;
            let receiver_scope = solved.expression_scope(receiver_origin, operation.caller).map_err(|_| problem("native_scalar_original_receiver_scope"))?;
            graph.validate_scoped(crate::sema::inference::ScopedRoot { ty: checked, scope: receiver_scope }).map_err(|_| problem("native_scalar_original_receiver_certificate"))?;
            if receiver_owner != owner || solved.expression_owners.get(&receiver_origin).copied() != operation.caller
                || graph_ground_type(graph, checked).map_err(|_| problem("native_scalar_receiver_type"))? != Type::Str
                || graph_ground_type(graph, operation.receiver.unwrap()).map_err(|_| problem("native_scalar_receiver_type"))? != Type::Str
                || graph_ground_type(graph, call.receiver.unwrap()).map_err(|_| problem("native_scalar_receiver_type"))? != Type::Str { return Err(problem("native_scalar_original_receiver_changed")); }
            let signature_root = graph.resolved(selected.signature).map_err(|_| problem("native_scalar_signature_owner"))?;
            let TypeNode::Arrow(arrow) = graph.node(signature_root).map_err(|_| problem("native_scalar_signature_owner"))? else { return Err(problem("native_scalar_signature_kind")); };
            if arrow.params.len() != 1 || arrow.params[0].label != Name::intern("<receiver>") || arrow.params[0].defaulted || arrow.params[0].rest
                || graph_ground_type(graph, arrow.params[0].ty).map_err(|_| problem("native_scalar_signature_receiver"))? != Type::Str
                || graph_ground_type(graph, selected.result).map_err(|_| problem("native_scalar_selected_result"))? != Type::Int
                || graph_ground_type(graph, *solved.expressions.get(&origin).ok_or_else(|| problem("native_scalar_original_result_missing"))?).map_err(|_| problem("native_scalar_original_result"))? != Type::Int { return Err(problem("native_scalar_original_signature_changed")); }
            let descriptor = self.intern_checked_callable_type(graph, signature_root)?;
            let (kind, signature) = self.store.semantic.callable_descriptor(descriptor).map_err(|_| problem("native_scalar_signature_descriptor"))?.ok_or_else(|| problem("native_scalar_signature_descriptor"))?;
            let receiver_type = self.intern_generic_ground_type(&Type::Str)?;
            let result = self.intern_generic_ground_type(&Type::Int)?;
            let closed = |summary| match graph.closed_effect_summary(summary).map_err(|_| problem("native_scalar_effect_owner"))? { EffectSummary::Closed(bits) => Ok(bits), _ => Err(problem("native_scalar_effect_requires_scope")) };
            let effects = PreparedOperationEffects { creation: closed(selected.effects)?, inputs: call.effect_bindings.iter().map(|&(role, summary)| closed(summary).map(|bits| (role, bits))).collect::<Result<Vec<_>, _>>()?.into_boxed_slice(), outputs: call.output_effect_bindings.iter().map(|&(role, summary)| closed(summary).map(|bits| (role, bits))).collect::<Result<Vec<_>, _>>()?.into_boxed_slice() };
            let template = graph.candidate(selected.candidate).map_err(|_| problem("native_scalar_original_candidate_template"))?;
            let contract = GroundNativeCallContract {
                authority: PreparedOperationAuthority::Registry { identity: metadata.identity, operation: metadata.operation, binding: metadata.binding, argument_check: metadata.argument_check, semantic_rule: metadata.semantic_rule, lifecycle: metadata.lifecycle, producer_transfer: metadata.producer_transfer.clone() },
                registry_owner: metadata.owner, receiver: None, cli_descriptor: None, process_command_argv: None, signature, kind, result: TypeRef::Ground(result), effects,
                arguments: Box::new([]), binding: PreparedOperationBinding { supplied_slots: Box::new([]), default_slots: Box::new([]), rest_slot: None, dynamic: None, operands: Box::new([]) },
                argument_sources: Box::new([]), argument_relations: template.argument_relations.clone().into_boxed_slice(), input_eligibility: template.actual_eligibility.clone().into_boxed_slice(),
            };
            let receiver = if tag == FullTag::ExprStrByteLen {
                if metadata.operation != RuntimeOp::TextByteLen { return Err(problem("native_scalar_material_operation_changed")); }
                NativeScalarReceiver::ByteLengthExpression { instruction: receiver_instruction }
            } else { NativeScalarReceiver::MethodExpression { instruction: receiver_instruction, name: Name::intern(metadata.entry) } };
            let payload = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| problem("native_scalar_material_payload"))?.into();
            self.generic_evidence_mut().add_native_scalar_source(NativeScalarSource { origin, receiver_origin, instruction, owner, receiver, receiver_type, contract, byte_at_fallback: None, payload }).map_err(|_| problem("native_scalar_original_allocation"))?;
        }
        Ok(())
    }
}

impl FullVerifier {
    pub(super) fn verify_native_scalar_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, expected: &Type, active: &mut Vec<u32>) -> Result<(), IrVerifyError> {
        let id = generic.native_scalar_at(instruction)?.ok_or_else(|| IrVerifyError::new("native scalar lacks its original checked receiver proof"))?;
        let source = generic.native_scalar_source(id)?;
        let TypeRef::Ground(result) = source.contract.result else { return Err(IrVerifyError::new("native scalar result requires a scoped proof")); };
        let payload = store.payload(store.data[instruction as usize].range())?;
        let material_result = source.byte_at_fallback.as_ref().map_or(result, |composite| composite.result);
        if source.owner != owner || store.semantic.to_type(material_result)? != *expected || payload != source.payload.as_ref() {
            return Err(IrVerifyError::new("native scalar changes its original opcode receiver slot or result"));
        }
        let location = if let Some(composite) = &source.byte_at_fallback {
            Self::verify_byte_at_fallback_material(store, generic, source, composite, owner, active)?
        } else { match source.receiver {
            NativeScalarReceiver::Parameter { slot, .. } | NativeScalarReceiver::ScopedParameter { slot, .. } | NativeScalarReceiver::Binding { slot, .. } => {
                if store.tags.get(instruction as usize) != Some(&FullTag::IntStrByteLenSlot) || payload.len() != 2 || payload[0] != slot { return Err(IrVerifyError::new("native scalar changes its original folded receiver slot")); }
                payload[1]
            }
            NativeScalarReceiver::Iteration { .. } => return Err(IrVerifyError::new("iteration native scalar receiver requires its original folded byte lookup")),
            NativeScalarReceiver::ByteLengthExpression { instruction: receiver } => {
                if store.tags.get(instruction as usize) != Some(&FullTag::ExprStrByteLen) || payload.len() != 2 || payload[0] != receiver { return Err(IrVerifyError::new("native scalar changes its original byte length receiver expression")); }
                Self::verify_generic_source(store, generic, receiver, owner, &store.semantic.to_type(source.receiver_type)?, None, active)?;
                payload[1]
            }
            NativeScalarReceiver::MethodExpression { instruction: receiver, name } => {
                if store.tags.get(instruction as usize) != Some(&FullTag::ExprMethod) || payload.len() != 4 || payload[0] != receiver || store.string(payload[1])? != name.as_str().as_str() { return Err(IrVerifyError::new("native scalar changes its original method receiver expression")); }
                let block = IrBlockId::from_raw(payload[2]).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("native scalar method argument block is invalid"))?;
                if block.flags & BLOCK_SEQUENCE_KIND_MASK != BLOCK_LIST || store.payload(block.instructions)? != [0] { return Err(IrVerifyError::new("native scalar method changes its original empty argument packet")); }
                Self::verify_generic_source(store, generic, receiver, owner, &store.semantic.to_type(source.receiver_type)?, None, active)?;
                payload[3]
            }
        } };
        if IrLocationId::from_raw(location).and_then(|location| store.location_sources.get(location.index())) != Some(&source.origin.source) { return Err(IrVerifyError::new("native scalar changes its original source location")); }
        if let NativeScalarReceiver::Binding { application, .. } = source.receiver {
            let contract = &generic.value_binding(application)?.contract;
            Self::verify_value_initializer_lineage(store, generic, contract)?;
            Self::verify_generic_source(store, generic, contract.initializer_source_instruction, owner, &store.semantic.to_type(contract.initializer_type)?, None, active)?;
        }
        Ok(())
    }
    pub(super) fn verify_native_scalar_sources(store: &FullStore, generic: &GenericEvidenceStore) -> Result<(), IrVerifyError> {
        for (id, _) in generic.native_scalar_sources() {
            let source = generic.native_scalar_source(id)?;
            Self::verify_native_scalar_operand(store, generic, source.instruction, source.owner, &Type::Int, &mut Vec::new())?;
        }
        Ok(())
    }
    pub(super) fn verify_native_scalar_dominance(store: &FullStore, tree: &super::super::pattern::PatternTree) -> Result<(), IrVerifyError> {
        let Some(generic) = store.generic.as_deref() else { return Ok(()); };
        if generic.native_scalar_sources().next().is_none() { return Ok(()); }
        let index = super::callable_prepare::CallableLexicalIndex::new(store, tree)?;
        let owners = store.generic_instruction_owners()?;
        let mut iteration_body_roots = FxHashMap::default();
        for (id, _) in generic.iteration_bindings() {
            let binding = generic.iteration_binding(id)?;
            let block = store.blocks.get(binding.body.index()).ok_or_else(|| IrVerifyError::new("native scalar iteration body is missing"))?;
            if block.flags != BLOCK_STATEMENTS { return Err(IrVerifyError::new("native scalar iteration body has another structural kind")); }
            let roots = store.payload(block.instructions)?.get(1..).ok_or_else(|| IrVerifyError::new("native scalar iteration body roots are missing"))?;
            for &root in roots {
                if iteration_body_roots.insert(root, id).is_some() { return Err(IrVerifyError::new("native scalar iteration bodies share a structural root")); }
            }
        }
        let mut writes = std::collections::BTreeSet::new();
        let owner_key = |owner| match owner { InstructionOwner::Function(function) => (false, function.raw()), InstructionOwner::Driver(driver) => (true, driver) };
        for (instruction, tag) in store.tags.iter().enumerate() {
            if matches!(tag, FullTag::StmtAssign | FullTag::StmtAssignInt | FullTag::StmtAssignBool | FullTag::StmtAssignField | FullTag::StmtAssignFieldInt | FullTag::StmtAssignPath)
                && let (Some(owner), Some(&slot)) = (owners[instruction], store.payload(store.data[instruction].range())?.first()) {
                writes.insert((owner_key(owner), slot));
            }
        }
        for (id, _) in generic.native_scalar_sources() {
            let source = generic.native_scalar_source(id)?;
            let slot = match source.receiver {
                NativeScalarReceiver::Parameter { slot, .. } | NativeScalarReceiver::ScopedParameter { slot, .. } | NativeScalarReceiver::Binding { slot, .. } | NativeScalarReceiver::Iteration { slot, .. } => slot,
                NativeScalarReceiver::ByteLengthExpression { .. } | NativeScalarReceiver::MethodExpression { .. } => continue,
            };
            if writes.contains(&(owner_key(source.owner), slot)) { return Err(IrVerifyError::new("native scalar receiver has an unprepared assignment")); }
            if let NativeScalarReceiver::Binding { application, .. } = source.receiver {
                if !index.dominates(tree, generic.value_binding(application)?.contract.instruction, source.instruction)? { return Err(IrVerifyError::new("native scalar receiver is outside its original binding scope")); }
            }
            if let NativeScalarReceiver::Iteration { application, .. } = source.receiver {
                let binding = generic.iteration_binding(application)?;
                let mut visible = false;
                let mut ancestor = Some(source.instruction);
                for _ in 0..=512 {
                    let Some(instruction) = ancestor else { break; };
                    if iteration_body_roots.get(&instruction) == Some(&application) { visible = true; break; }
                    ancestor = tree.parent(instruction)?;
                }
                if !visible || tree.is_descendant(binding.iterator, source.instruction)? {
                    return Err(IrVerifyError::new("native scalar iteration item is outside its original loop body"));
                }
            }
        }
        Ok(())
    }
}
