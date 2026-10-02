use super::*;
use super::super::super::generic::{ByteAtFallbackComposite, PreparedInvocationArgument};
use crate::sema::operation_graph::{OperationArgumentOrder, PreparedLanguageOperation};

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct BuildByteAtFallbackOriginal {
    pub call: ExpressionIdentity,
    pub receiver: BuildFoldedNativeReceiver,
    pub index_origin: ExpressionIdentity,
    pub fallback_origin: ExpressionIdentity,
    pub fallback_value: i64,
}

fn folded_operands(store: &FullStore, instruction: u32) -> Result<(u32, u32, Option<u32>, u32), IrVerifyError> {
    if store.tags.get(instruction as usize) != Some(&FullTag::IntStrByteAtSlot) { return Err(IrVerifyError::new("folded byte lookup has another opcode")); }
    let mut cursor = FullCursor::new(store.payload(store.data[instruction as usize].range())?);
    let slot = cursor.raw()?;
    let index = cursor.raw()?;
    let default = match cursor.raw()? { 0 => None, 1 => Some(cursor.raw()?), _ => return Err(IrVerifyError::new("folded byte lookup has an invalid literal flag")) };
    let location = cursor.raw()?;
    cursor.finish()?;
    Ok((slot, index, default, location))
}

impl FullBuilder {
    pub(in crate::runtime::eval::indexed::full) fn stage_original_byte_at_fallback(&mut self, instruction: u32, original: BuildByteAtFallbackOriginal, owner: InstructionOwner) {
        self.byte_at_fallback_rows.push((instruction, original, owner));
    }

    pub(super) fn prepare_byte_at_fallback_sources(&mut self) -> Result<(), IrBuildError> {
        if self.byte_at_fallback_rows.is_empty() { return Ok(()); }
        let solved = self.solved.clone().ok_or_else(|| problem("byte_at_fallback_original_graph_missing"))?;
        let graph = &solved.graph;
        let origins = self.generic_expression_rows.iter().map(|&(instruction, origin, owner)| (instruction, (origin, owner))).collect::<FxHashMap<_, _>>();
        let mut bindings = BTreeMap::new();
        if let Some(generic) = &self.generic {
            for row in generic.native_scalar_binding_sources() {
                let (binding, application) = row.map_err(|_| problem("byte_at_fallback_binding_owner"))?;
                if bindings.insert(binding, application).is_some() { return Err(problem("byte_at_fallback_binding_ambiguous")); }
            }
        }
        for (instruction, original, owner) in self.byte_at_fallback_rows.clone() {
            let &(origin, source_owner) = origins.get(&instruction).ok_or_else(|| problem("byte_at_fallback_original_parent_missing"))?;
            if owner != source_owner { return Err(problem("byte_at_fallback_original_owner")); }
            let (slot, index_instruction, fallback_instruction, _) = folded_operands(&self.store, instruction).map_err(|_| problem("byte_at_fallback_original_encoding"))?;
            if slot != original.receiver.slot || origins.get(&index_instruction) != Some(&(original.index_origin, owner)) { return Err(problem("byte_at_fallback_original_operands_changed")); }
            let operation = solved.operations.get(&original.call).ok_or_else(|| problem("byte_at_fallback_original_call_missing"))?;
            let fallback_operation = solved.operations.get(&origin).ok_or_else(|| problem("byte_at_fallback_original_language_missing"))?;
            if operation.caller != fallback_operation.caller { return Err(problem("byte_at_fallback_original_caller_changed")); }
            for (identity, operation) in [(original.call, operation), (origin, fallback_operation)] {
                let scope = solved.expression_scope(identity, operation.caller).map_err(|_| problem("byte_at_fallback_original_scope"))?;
                graph.validate_requirement_scoped(crate::sema::inference::ScopedRequirementRoot { requirement: operation.requirement, scope }).map_err(|_| problem("byte_at_fallback_original_certificate"))?;
            }
            let selected = graph.candidate_evidence(operation.requirement).map_err(|_| problem("byte_at_fallback_original_candidate"))?.ok_or_else(|| problem("byte_at_fallback_original_candidate_missing"))?;
            let crate::sema::check::SolvedOperationAuthority::Registry(metadata) = solved.operation_catalog.candidate(graph, selected.candidate).map_err(|_| problem("byte_at_fallback_original_authority"))? else { return Err(problem("byte_at_fallback_original_authority")); };
            if metadata.operation != RuntimeOp::TextByteAt || metadata.owner != crate::sema::registry_graph::RegistryOwner::Method(crate::modules::signature::MethodReceiver::Str)
                || metadata.binding != crate::modules::signature::ImplBinding::Native || metadata.semantic_rule != crate::modules::signature::SemanticRule::Standard
                || operation.binding.supplied_slots.as_slice() != [0] || !operation.binding.default_slots.is_empty() || operation.binding.rest_slot.is_some() || operation.binding.dynamic.is_some()
                || operation.actual_arguments.len() != 1 || !operation.argument_coercions.is_empty() || !selected.callback_invocations.is_empty() { return Err(problem("byte_at_fallback_original_call_contract")); }
            let RequirementTemplate::Operation { call, .. } = graph.requirement_template(operation.requirement).map_err(|_| problem("byte_at_fallback_original_requirement"))? else { return Err(problem("byte_at_fallback_original_requirement")); };
            let call = graph.operation_call(call).map_err(|_| problem("byte_at_fallback_original_call_owner"))?;
            if call.binding != OperationBinding::Slots || call.receiver.is_none() || operation.receiver.is_none() { return Err(problem("byte_at_fallback_original_receiver_missing")); }
            let optional_int = Type::Optional(Box::new(Type::Int));
            for (identity, expected) in [(origin, Type::Int), (original.call, optional_int.clone()), (original.receiver.origin, Type::Str), (original.index_origin, Type::Int), (original.fallback_origin, Type::Int)] {
                let checked = *solved.expressions.get(&identity).ok_or_else(|| problem("byte_at_fallback_original_expression_missing"))?;
                let scope = solved.expression_scope(identity, operation.caller).map_err(|_| problem("byte_at_fallback_original_expression_scope"))?;
                graph.validate_scoped(crate::sema::inference::ScopedRoot { ty: checked, scope }).map_err(|_| problem("byte_at_fallback_original_expression_certificate"))?;
                if solved.expression_owners.get(&identity).copied() != operation.caller || graph_ground_type(graph, checked).map_err(|_| problem("byte_at_fallback_original_expression_type"))? != expected { return Err(problem("byte_at_fallback_original_expression_changed")); }
            }
            if graph_ground_type(graph, operation.actual_arguments[0]).map_err(|_| problem("byte_at_fallback_original_index_type"))? != Type::Int
                || selected.actual_arguments.len() != 1 || selected.actual_arguments[0].is_none() || graph_ground_type(graph, selected.actual_arguments[0].unwrap()).map_err(|_| problem("byte_at_fallback_original_selected_index"))? != Type::Int
                || graph_ground_type(graph, operation.receiver.unwrap()).map_err(|_| problem("byte_at_fallback_original_receiver_type"))? != Type::Str
                || graph_ground_type(graph, call.receiver.unwrap()).map_err(|_| problem("byte_at_fallback_original_receiver_type"))? != Type::Str
                || graph_ground_type(graph, selected.result).map_err(|_| problem("byte_at_fallback_original_result"))? != optional_int { return Err(problem("byte_at_fallback_original_call_types_changed")); }
            let signature_root = graph.resolved(selected.signature).map_err(|_| problem("byte_at_fallback_original_signature"))?;
            let TypeNode::Arrow(arrow) = graph.node(signature_root).map_err(|_| problem("byte_at_fallback_original_signature"))? else { return Err(problem("byte_at_fallback_original_signature_kind")); };
            if arrow.params.len() != 2 || arrow.params[0].label != Name::intern("<receiver>") || arrow.params.iter().any(|parameter| parameter.defaulted || parameter.rest)
                || graph_ground_type(graph, arrow.params[0].ty).map_err(|_| problem("byte_at_fallback_original_signature_receiver"))? != Type::Str
                || graph_ground_type(graph, arrow.params[1].ty).map_err(|_| problem("byte_at_fallback_original_signature_index"))? != Type::Int { return Err(problem("byte_at_fallback_original_signature_changed")); }
            let recipes = solved.argument_sources.get(&original.call).ok_or_else(|| problem("byte_at_fallback_original_recipe_missing"))?;
            if recipes.len() != 1 || !matches!(recipes[0].value, crate::sema::arguments::ArgumentValueSource::Expression(expression) if expression == original.index_origin.expression) { return Err(problem("byte_at_fallback_original_recipe_changed")); }
            let descriptor = self.intern_checked_callable_type(graph, signature_root)?;
            let (kind, signature) = self.store.semantic.callable_descriptor(descriptor).map_err(|_| problem("byte_at_fallback_original_descriptor"))?.ok_or_else(|| problem("byte_at_fallback_original_descriptor"))?;
            let int = self.intern_generic_ground_type(&Type::Int)?;
            let receiver_type = self.intern_generic_ground_type(&Type::Str)?;
            let call_result = self.intern_generic_ground_type(&optional_int)?;
            let receiver_checked = *solved.expressions.get(&original.receiver.origin).ok_or_else(|| problem("byte_at_fallback_original_receiver_type"))?;
            let receiver = if let Some(binding) = original.receiver.binding {
                let application = *bindings.get(&binding).ok_or_else(|| problem("byte_at_fallback_original_binding_missing"))?;
                NativeScalarReceiver::Binding { binding, application, slot }
            } else {
                let declaration = operation.caller.ok_or_else(|| problem("byte_at_fallback_original_parameter_owner"))?;
                let callable = solved.declarations.get(&declaration).ok_or_else(|| problem("byte_at_fallback_original_declaration_missing"))?;
                let TypeNode::Arrow(arrow) = graph.node(graph.resolved(callable.signature).map_err(|_| problem("byte_at_fallback_original_declaration"))?).map_err(|_| problem("byte_at_fallback_original_declaration"))? else { return Err(problem("byte_at_fallback_original_declaration_kind")); };
                let parameter = arrow.params.get(slot as usize).ok_or_else(|| problem("byte_at_fallback_original_parameter_slot"))?;
                if parameter.label != original.receiver.name || graph.resolved(parameter.ty).map_err(|_| problem("byte_at_fallback_original_parameter_type"))? != graph.resolved(receiver_checked).map_err(|_| problem("byte_at_fallback_original_parameter_type"))? { return Err(problem("byte_at_fallback_original_parameter_changed")); }
                let InstructionOwner::Function(function) = owner else { return Err(problem("byte_at_fallback_original_parameter_body")); };
                if let Some(&scope) = self.generic_declarations.get(&declaration) { NativeScalarReceiver::ScopedParameter { scope, name: original.receiver.name, slot } }
                else { NativeScalarReceiver::Parameter { declaration, signature: SignatureId::from_raw(self.store.functions[function.index()].signature).ok_or_else(|| problem("byte_at_fallback_original_parameter_signature"))?, name: original.receiver.name, slot } }
            };
            let fallback_selected = graph.candidate_evidence(fallback_operation.requirement).map_err(|_| problem("byte_at_fallback_original_language_candidate"))?.ok_or_else(|| problem("byte_at_fallback_original_language_candidate_missing"))?;
            let crate::sema::check::SolvedOperationAuthority::Language(language) = solved.operation_catalog.candidate(graph, fallback_selected.candidate).map_err(|_| problem("byte_at_fallback_original_language_authority"))? else { return Err(problem("byte_at_fallback_original_language_authority")); };
            if language.operation != (PreparedLanguageOperation::Fallback { result: false }) || language.argument_order != OperationArgumentOrder::SourceOrder || language.statement_result_is_unit
                || fallback_operation.actual_arguments.len() != 2 || fallback_operation.binding.supplied_slots.as_slice() != [0, 1] || !fallback_operation.binding.default_slots.is_empty()
                || fallback_operation.binding.rest_slot.is_some() || fallback_operation.binding.dynamic.is_some() || !fallback_operation.argument_coercions.is_empty() || !fallback_selected.callback_invocations.is_empty()
                || graph_ground_type(graph, fallback_operation.actual_arguments[0]).map_err(|_| problem("byte_at_fallback_original_language_left"))? != optional_int
                || graph_ground_type(graph, fallback_operation.actual_arguments[1]).map_err(|_| problem("byte_at_fallback_original_language_right"))? != Type::Int
                || graph_ground_type(graph, fallback_selected.result).map_err(|_| problem("byte_at_fallback_original_language_result"))? != Type::Int { return Err(problem("byte_at_fallback_original_language_contract")); }
            if fallback_selected.actual_arguments.len() != 2 || fallback_selected.actual_arguments.iter().zip([&optional_int, &Type::Int]).any(|(actual, expected)| actual.is_none_or(|actual| graph_ground_type(graph, actual).ok().as_ref() != Some(expected))) { return Err(problem("byte_at_fallback_original_selected_language_operands")); }
            let RequirementTemplate::Operation { call: fallback_call, .. } = graph.requirement_template(fallback_operation.requirement).map_err(|_| problem("byte_at_fallback_original_language_requirement"))? else { return Err(problem("byte_at_fallback_original_language_requirement")); };
            let fallback_call = graph.operation_call(fallback_call).map_err(|_| problem("byte_at_fallback_original_language_call"))?;
            if fallback_call.binding != OperationBinding::Slots || fallback_call.receiver.is_some() || !fallback_call.effect_bindings.is_empty() || !fallback_call.output_effect_bindings.is_empty() { return Err(problem("byte_at_fallback_original_language_protocol")); }
            let fallback_descriptor = self.intern_checked_callable_type(graph, fallback_selected.signature)?;
            let (_, fallback_signature) = self.store.semantic.callable_descriptor(fallback_descriptor).map_err(|_| problem("byte_at_fallback_original_language_descriptor"))?.ok_or_else(|| problem("byte_at_fallback_original_language_descriptor"))?;
            let fallback_authority = PreparedOperationAuthority::Language { identity: language.identity, authority: language.authority, operation: language.operation.clone(), argument_order: language.argument_order, statement_result_is_unit: language.statement_result_is_unit };
            let closed = |summary| match graph.closed_effect_summary(summary).map_err(|_| problem("byte_at_fallback_original_effect_owner"))? { EffectSummary::Closed(bits) => Ok(bits), _ => Err(problem("byte_at_fallback_original_effect_scope")) };
            if closed(fallback_selected.effects)? != crate::sema::inference::EffectSet::EMPTY { return Err(problem("byte_at_fallback_original_language_effect")); }
            let effects = PreparedOperationEffects { creation: closed(selected.effects)?, inputs: call.effect_bindings.iter().map(|&(role, summary)| closed(summary).map(|bits| (role, bits))).collect::<Result<Vec<_>, _>>()?.into_boxed_slice(), outputs: call.output_effect_bindings.iter().map(|&(role, summary)| closed(summary).map(|bits| (role, bits))).collect::<Result<Vec<_>, _>>()?.into_boxed_slice() };
            let template = graph.candidate(selected.candidate).map_err(|_| problem("byte_at_fallback_original_template"))?;
            let contract = GroundNativeCallContract {
                authority: PreparedOperationAuthority::Registry { identity: metadata.identity, operation: metadata.operation, binding: metadata.binding, argument_check: metadata.argument_check, semantic_rule: metadata.semantic_rule, lifecycle: metadata.lifecycle, producer_transfer: metadata.producer_transfer.clone() },
                registry_owner: metadata.owner, receiver: None, cli_descriptor: None, process_command_argv: None, signature, kind, result: TypeRef::Ground(call_result), effects,
                arguments: vec![PreparedInvocationArgument { original: recipes[0].clone(), instruction: index_instruction, ty: TypeRef::Ground(int) }].into_boxed_slice(),
                binding: PreparedOperationBinding { supplied_slots: vec![0].into_boxed_slice(), default_slots: Box::new([]), rest_slot: None, dynamic: None, operands: vec![index_instruction].into_boxed_slice() },
                argument_sources: vec![Some(index_instruction)].into_boxed_slice(), argument_relations: template.argument_relations.clone().into_boxed_slice(), input_eligibility: template.actual_eligibility.clone().into_boxed_slice(),
            };
            let composite = ByteAtFallbackComposite { call_origin: original.call, index_origin: original.index_origin, index_instruction, index_type: int, fallback_origin: original.fallback_origin, fallback_type: int, fallback_value: original.fallback_value, fallback_instruction, fallback_authority, fallback_signature, result: int };
            let payload = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| problem("byte_at_fallback_original_payload"))?.into();
            self.generic_evidence_mut().add_native_scalar_source(NativeScalarSource { origin, receiver_origin: original.receiver.origin, instruction, owner, receiver, receiver_type, contract, byte_at_fallback: Some(Box::new(composite)), payload }).map_err(|_| problem("byte_at_fallback_original_allocation"))?;
        }
        Ok(())
    }
}

impl FullVerifier {
    pub(super) fn verify_byte_at_fallback_material(store: &FullStore, generic: &GenericEvidenceStore, source: &NativeScalarSource, composite: &ByteAtFallbackComposite, owner: InstructionOwner, active: &mut Vec<u32>) -> Result<u32, IrVerifyError> {
        GenericEvidenceStore::verify_byte_at_fallback_contract(&store.semantic, source, composite)?;
        let (slot, index, default, location) = folded_operands(store, source.instruction)?;
        let receiver_slot = match source.receiver { NativeScalarReceiver::Parameter { slot, .. } | NativeScalarReceiver::ScopedParameter { slot, .. } | NativeScalarReceiver::Binding { slot, .. } => slot, _ => return Err(IrVerifyError::new("folded byte lookup lacks its original receiver slot")) };
        if slot != receiver_slot || index != composite.index_instruction || default != composite.fallback_instruction { return Err(IrVerifyError::new("folded byte lookup changes its original receiver index or literal alternative")); }
        Self::verify_generic_source(store, generic, index, owner, &Type::Int, None, active)?;
        if let Some(default) = default {
            if store.tags.get(default as usize) != Some(&FullTag::IntInt) { return Err(IrVerifyError::new("folded byte lookup alternative is no longer an inert literal")); }
            let mut cursor = FullCursor::new(store.payload(store.data[default as usize].range())?);
            let low = cursor.raw()? as u64;
            let high = cursor.raw()? as u64;
            let value = (low | high << 32) as i64;
            cursor.finish()?;
            if value != composite.fallback_value { return Err(IrVerifyError::new("folded byte lookup changes its original alternative value")); }
        } else if composite.fallback_value != -1 { return Err(IrVerifyError::new("folded byte lookup erases its original alternative value")); }
        Ok(location)
    }
}
