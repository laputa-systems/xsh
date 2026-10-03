use super::*;
use super::super::super::generic::ValueFieldPresence;
use crate::sema::operation_graph::{MembershipDomain, PreparedLanguageOperation};
use crate::sema::inference::{OperationBinding, RequirementTemplate};

fn branch_payload(store: &FullStore, control: u32, ordinal: u32) -> Result<(u32, u32, Box<[u32]>, Box<[u32]>), IrVerifyError> {
    if store.tags.get(control as usize) != Some(&FullTag::StmtIf) { return Err(IrVerifyError::new("field-presence control is not its original conditional")); }
    let words = store.payload(store.data[control as usize].range())?;
    if !matches!(words, [_, 0] | [_, 1, _]) { return Err(IrVerifyError::new("field-presence conditional payload is invalid")); }
    let block = IrBlockId::from_raw(words[0]).and_then(|block| store.blocks.get(block.index())).ok_or_else(|| IrVerifyError::new("field-presence branch table is missing"))?;
    let branches = store.payload(block.instructions)?;
    let count = *branches.first().ok_or_else(|| IrVerifyError::new("field-presence branch table is empty"))?;
    if branches.len() != 1 + 2 * count as usize || ordinal >= count { return Err(IrVerifyError::new("field-presence branch ordinal is invalid")); }
    let offset = 1 + 2 * ordinal as usize;
    let predicate = branches[offset];
    let body = branches[offset + 1];
    let block = IrBlockId::from_raw(body).and_then(|block| store.blocks.get(block.index())).ok_or_else(|| IrVerifyError::new("field-presence successful branch is missing"))?;
    let body_words = store.payload(block.instructions)?;
    if body_words.first().is_none_or(|&count| body_words.len() != 1 + count as usize) { return Err(IrVerifyError::new("field-presence successful branch payload is invalid")); }
    Ok((predicate, body, branches.to_vec().into_boxed_slice(), body_words.to_vec().into_boxed_slice()))
}

fn record_presence_types(material: &Type, narrowed: &Type, field: Name) -> bool {
    let (Type::Record(base), Type::Record(read)) = (material, narrowed) else { return false; };
    if base.contains_key(&field) { return false; }
    let mut expected = base.clone();
    expected.insert(field, Type::Any);
    *read == expected
}

impl FullBuilder {
    pub(super) fn prepare_value_field_presence(&mut self, origin: OperationSourceOrigin, binding: ValueBindingIdentity,
        application: super::super::super::generic::ValueBindingId, instruction: u32, owner: InstructionOwner,
        solved: &crate::sema::check::SolvedTypes) -> Result<Option<ValueFieldPresence>, IrBuildError> {
        let OperationSourceOrigin::Expression(read) = origin else { return Ok(None); };
        let Some(original) = solved.checked_field_presence_read(read).map_err(|_| value_problem("field_presence_original_read_changed"))?.cloned() else { return Ok(None); };
        if binding.named() != Some(original.binding) || !original.writes.is_empty() || self.store.tags.get(instruction as usize) != Some(&FullTag::ExprParam) {
            return Err(value_problem("field_presence_original_binding_changed"));
        }
        for root in [original.material, original.subject_type, original.narrowed] {
            solved.graph.validate_scoped(root).map_err(|_| value_problem("field_presence_original_type_scope"))?;
        }
        let material = graph_ground_type(&solved.graph, original.material.ty).map_err(|_| value_problem("field_presence_material_requires_ground_type"))?;
        let subject_type = graph_ground_type(&solved.graph, original.subject_type.ty).map_err(|_| value_problem("field_presence_subject_requires_ground_type"))?;
        let narrowed = graph_ground_type(&solved.graph, original.narrowed.ty).map_err(|_| value_problem("field_presence_read_requires_ground_type"))?;
        if material != subject_type || !record_presence_types(&material, &narrowed, original.field) { return Err(value_problem("field_presence_original_record_relationship")); }
        let operation = solved.operations.get(&original.predicate).ok_or_else(|| value_problem("field_presence_original_predicate_missing"))?;
        let predicate_scope = solved.expression_scope(original.predicate, original.caller).map_err(|_| value_problem("field_presence_original_predicate_scope"))?;
        solved.graph.validate_requirement_scoped(crate::sema::inference::ScopedRequirementRoot { requirement: operation.requirement, scope: predicate_scope })
            .map_err(|_| value_problem("field_presence_original_predicate_certificate"))?;
        let selected = solved.graph.candidate_evidence(operation.requirement).map_err(|_| value_problem("field_presence_original_candidate"))?
            .ok_or_else(|| value_problem("field_presence_original_candidate_missing"))?;
        let crate::sema::check::SolvedOperationAuthority::Language(metadata) = solved.operation_catalog.candidate(&solved.graph, selected.candidate)
            .map_err(|_| value_problem("field_presence_original_authority"))? else { return Err(value_problem("field_presence_original_authority")); };
        let RequirementTemplate::Operation { call, .. } = solved.graph.requirement_template(operation.requirement).map_err(|_| value_problem("field_presence_original_requirement"))? else { return Err(value_problem("field_presence_original_requirement")); };
        let call = solved.graph.operation_call(call).map_err(|_| value_problem("field_presence_original_call"))?;
        if metadata.operation != (PreparedLanguageOperation::Membership { negated: false, domain: MembershipDomain::Record })
            || operation.caller != original.caller || operation.receiver.and_then(|root| graph_ground_type(&solved.graph, root).ok()) != Some(subject_type.clone())
            || operation.actual_arguments.len() != 1 || selected.actual_arguments.len() != 1
            || call.binding != OperationBinding::Slots || operation.binding.supplied_slots.as_slice() != [0]
            || !operation.binding.default_slots.is_empty() || operation.binding.dynamic.is_some() || operation.binding.rest_slot.is_some()
            || !operation.argument_coercions.is_empty() || !selected.callback_invocations.is_empty()
            || graph_ground_type(&solved.graph, operation.actual_arguments[0]).ok() != Some(Type::Str)
            || selected.actual_arguments[0].and_then(|root| graph_ground_type(&solved.graph, root).ok()) != Some(Type::Str)
            || graph_ground_type(&solved.graph, selected.result).ok() != Some(Type::Bool)
            || graph_ground_type(&solved.graph, *solved.expressions.get(&original.key).ok_or_else(|| value_problem("field_presence_original_key_missing"))?).ok() != Some(Type::Str) {
            return Err(value_problem("field_presence_original_membership_contract"));
        }
        let source_instruction = |identity| {
            let mut matches = self.generic_expression_rows.iter().filter(|(_, source, source_owner)| *source == identity && *source_owner == owner);
            let instruction = matches.next().map(|row| row.0).ok_or_else(|| value_problem("field_presence_original_instruction_missing"))?;
            if matches.next().is_some() { return Err(value_problem("field_presence_original_instruction_ambiguous")); }
            Ok(instruction)
        };
        let predicate = source_instruction(original.predicate)?;
        let subject = source_instruction(original.subject)?;
        let key = source_instruction(original.key)?;
        let mut controls = self.field_presence_control_rows.iter().filter(|(control, _, source_owner)| *control == original.control && *source_owner == owner);
        let control = controls.next().map(|row| row.1).ok_or_else(|| value_problem("field_presence_original_control_missing"))?;
        if controls.next().is_some() { return Err(value_problem("field_presence_original_control_ambiguous")); }
        let (actual_predicate, branch_body, branch_words, body_words) = branch_payload(&self.store, control, original.branch).map_err(|_| value_problem("field_presence_original_branch_changed"))?;
        if actual_predicate != predicate { return Err(value_problem("field_presence_original_branch_predicate_changed")); }
        let predicate_payload = self.store.payload(self.store.data[predicate as usize].range()).map_err(|_| value_problem("field_presence_original_predicate_payload"))?.to_vec().into_boxed_slice();
        let key_payload = self.store.payload(self.store.data[key as usize].range()).map_err(|_| value_problem("field_presence_original_key_payload"))?.to_vec().into_boxed_slice();
        let control_payload = self.store.payload(self.store.data[control as usize].range()).map_err(|_| value_problem("field_presence_original_control_payload"))?.to_vec().into_boxed_slice();
        let contract = self.generic.as_ref().ok_or_else(|| value_problem("field_presence_binding_store_missing"))?.value_binding(application)
            .map_err(|_| value_problem("field_presence_binding_receipt_changed"))?.contract.clone();
        if self.store.tags.get(predicate as usize) != Some(&FullTag::ExprBinary) || predicate_payload.len() != 4
            || self.store.binary_ops.get(predicate_payload[0] as usize) != Some(&BinaryOp::In)
            || predicate_payload.get(1..3) != Some(&[key, subject])
            || self.store.tags.get(subject as usize) != Some(&FullTag::ExprParam)
            || self.store.payload(self.store.data[subject as usize].range()).ok() != Some(&[contract.slot])
            || self.store.tags.get(key as usize) != Some(&FullTag::ExprStr) || key_payload.len() != 1
            || self.store.string(key_payload[0]).ok() != Some(original.field.as_str().as_str())
            || self.store.semantic.to_type(contract.binding_type).map_err(|_| value_problem("field_presence_material_type"))? != material {
            return Err(value_problem("field_presence_original_operand_changed"));
        }
        let material_type = self.intern_generic_ground_type(&material)?;
        let narrowed_type = self.intern_generic_ground_type(&narrowed)?;
        Ok(Some(ValueFieldPresence { original, material_type, narrowed_type, predicate, subject, key, control, branch_body,
            predicate_payload, key_payload, control_payload, branch_payload: branch_words, branch_body_payload: body_words }))
    }
}

impl FullVerifier {
    pub(super) fn verify_value_field_presence_dominance(store: &FullStore, tree: &super::super::super::pattern::PatternTree, use_: &ValueBindingUse) -> Result<(), IrVerifyError> {
        let Some(presence) = &use_.presence else { return Ok(()); };
        let (predicate, body, branches, body_words) = branch_payload(store, presence.control, presence.original.branch)?;
        let mut visible = false;
        for &statement in body_words.iter().skip(1) {
            if tree.is_descendant(statement, use_.instruction)? { visible = true; break; }
        }
        if predicate != presence.predicate || body != presence.branch_body || branches != presence.branch_payload || body_words != presence.branch_body_payload
            || store.payload(store.data[presence.control as usize].range())? != presence.control_payload.as_ref()
            || !visible {
            return Err(IrVerifyError::new("field-presence read is outside its original successful branch"));
        }
        Ok(())
    }

    pub(super) fn verify_value_field_presence(store: &FullStore, generic: &GenericEvidenceStore, use_: &ValueBindingUse, presence: &ValueFieldPresence,
        owner: InstructionOwner, active: &mut Vec<u32>) -> Result<(), IrVerifyError> {
        let binding = generic.value_binding(use_.application)?;
        let source = generic.value_binding_source(binding.source)?;
        let original = &presence.original;
        if use_.owner != owner || source.binding.named() != Some(original.binding)
            || use_.origin != OperationSourceOrigin::Expression(original.read)
            || original.material.ty != source.source_type.ty || original.material.scope != source.source_type.scope
            || presence.material_type != binding.contract.binding_type || !original.writes.is_empty()
            || !record_presence_types(&store.semantic.to_type(presence.material_type)?, &store.semantic.to_type(presence.narrowed_type)?, original.field)
            || store.tags.get(presence.predicate as usize) != Some(&FullTag::ExprBinary)
            || store.payload(store.data[presence.predicate as usize].range())? != presence.predicate_payload.as_ref()
            || presence.predicate_payload.len() != 4 || store.binary_ops.get(presence.predicate_payload[0] as usize) != Some(&BinaryOp::In)
            || presence.predicate_payload.get(1..3) != Some(&[presence.key, presence.subject])
            || store.tags.get(presence.subject as usize) != Some(&FullTag::ExprParam)
            || store.payload(store.data[presence.subject as usize].range())? != [binding.contract.slot]
            || store.tags.get(presence.key as usize) != Some(&FullTag::ExprStr)
            || store.payload(store.data[presence.key as usize].range())? != presence.key_payload.as_ref()
            || presence.key_payload.len() != 1 || store.string(presence.key_payload[0])? != original.field.as_str().as_str() {
            return Err(IrVerifyError::new("field-presence read changes its original checked relationship"));
        }
        let operation = generic.operation_at(presence.predicate)?.ok_or_else(|| IrVerifyError::new("field-presence read loses its original membership proof"))?;
        if !matches!(operation.authority, super::super::super::generic::PreparedOperationAuthority::Language {
            operation: PreparedLanguageOperation::Membership { negated: false, domain: MembershipDomain::Record }, .. })
            || operation.receiver != Some(TypeRef::Ground(presence.material_type)) || generic.operation_source(operation.source)?.owner != owner {
            return Err(IrVerifyError::new("field-presence read changes its original membership domain"));
        }
        Self::verify_generic_source(store, generic, presence.predicate, owner, &Type::Bool, None, active)?;
        Self::verify_generic_source(store, generic, presence.subject, owner, &store.semantic.to_type(presence.material_type)?, None, active)?;
        Ok(())
    }
}
