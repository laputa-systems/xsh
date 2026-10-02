use super::*;
use super::super::generic::{PreparedRecordReplacement, PreparedRecordUpdateSource, PreparedUpdateProjection, PreparedUpdateValue, PreparedUpdateWrapper, graph_ground_type};
use super::super::super::lower::record_update::{OriginalRecordUpdate, OriginalRecordUpdateValue};
use crate::sema::check::{RecordUpdateValueSource, SolvedTypes};
use crate::sema::inference::{ConstraintRelation, ScopedRoot};

type EncodedUpdateValue = (u32, u32);
pub(super) type StagedRecordUpdate = (OriginalRecordUpdate, u32, InstructionOwner, EncodedUpdateValue, Box<[EncodedUpdateValue]>);

fn update_problem(reason: &'static str) -> IrBuildError { IrBuildError::format(reason, None, 0, 0) }

impl FullBuilder {
    pub(super) fn stage_original_record_update(&mut self, row: BuildExprId, instruction: u32, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        let Some(original) = scratch.record_update_sources.get(&row) else { return Ok(()); };
        let raw = self.current_owner.ok_or_else(|| update_problem("record_update_instruction_owner"))?;
        let owner = if let Some(driver) = driver_owner_index(raw) { InstructionOwner::Driver(driver as u32) }
            else { InstructionOwner::Function(IrFunctionId::from_raw(raw).ok_or_else(|| update_problem("record_update_instruction_owner"))?) };
        let encode = |value: &OriginalRecordUpdateValue| -> Result<EncodedUpdateValue, IrBuildError> {
            Ok((*self.active_encoded_expressions.get(&value.row).ok_or_else(|| update_problem("record_update_encoded_value_missing"))?,
                *self.active_encoded_expressions.get(&value.material).ok_or_else(|| update_problem("record_update_original_value_missing"))?))
        };
        let base = encode(&original.base)?;
        let replacements = original.replacements.iter().map(encode).collect::<Result<Vec<_>, _>>()?.into_boxed_slice();
        self.record_update_rows.push((original.clone(), instruction, owner, base, replacements));
        Ok(())
    }

    pub(super) fn prepare_record_updates(&mut self) -> Result<(), IrBuildError> {
        let Some(solved) = self.solved.clone() else { return Ok(()); };
        let mut layouts = BTreeMap::new();
        for (original, instruction, owner, base, encoded) in self.record_update_rows.clone() {
            let current = solved.record_updates.get(&original.origin).ok_or_else(|| update_problem("record_update_original_contract_missing"))?;
            let contract = &original.contract;
            if self.store.tags.get(instruction as usize) != Some(&FullTag::ExprRecordUpdate)
                || current.base != contract.base || current.receiver != contract.receiver || current.result != contract.result || current.caller != contract.caller
                || current.replacements.len() != contract.replacements.len() || encoded.len() != contract.replacements.len()
                || original.replacements.len() != contract.replacements.len()
                || solved.expressions.get(&original.origin) != Some(&original.checked.ty)
                || solved.expression_scope(original.origin, contract.caller).map_err(|_| update_problem("record_update_original_scope"))? != original.checked.scope {
                return Err(update_problem("record_update_original_checked_contract"));
            }
            solved.graph.validate_scoped(original.checked).map_err(|_| update_problem("record_update_original_root_owner"))?;
            let resolved = |ty| solved.graph.resolved(ty).map_err(|_| update_problem("record_update_original_graph_endpoint"));
            if resolved(original.checked.ty)? != resolved(contract.result)? || resolved(contract.result)? != resolved(contract.receiver)?
                || original.base.origin != contract.base || resolved(original.base.checked.ty)? != resolved(contract.receiver)? {
                return Err(update_problem("record_update_original_complete_receiver"));
            }
            let ty = graph_ground_type(&solved.graph, original.checked.ty).map_err(|_| update_problem("record_update_closed_result"))?;
            let schema = super::super::super::require::PreparedSchema::compile_record_layout(&ty).ok_or_else(|| update_problem("record_update_canonical_schema"))?;
            let result = self.intern_generic_ground_type(&ty)?;
            let layout = self.ground_projection_layout(result, &mut layouts)?;
            let base = self.prepare_record_update_value(&solved, &original.base, base, contract.caller)?;
            let mut replacements = Vec::with_capacity(encoded.len());
            for (((old, current), value), &encoded) in contract.replacements.iter().zip(&current.replacements).zip(original.replacements.iter()).zip(encoded.iter()) {
                if old.path != current.path || old.value != current.value || old.source != current.source || old.assignability != current.assignability
                    || old.producer_flow != current.producer_flow || old.projections.len() != current.projections.len()
                    || old.projections.iter().zip(&current.projections).any(|(old, current)| old.receiver != current.receiver || old.field != current.field || old.result != current.result)
                    || old.path.is_empty() || old.path.len() != old.projections.len() || old.source != RecordUpdateValueSource::Expression(value.origin)
                    || resolved(value.checked.ty)? != resolved(old.value)? {
                    return Err(update_problem("record_update_original_replacement_contract"));
                }
                if solved.expression_producer_flows.get(&value.origin) != Some(&old.producer_flow)
                    || solved.producer_flows.node(old.producer_flow).map_err(|_| update_problem("record_update_original_replacement_flow_owner"))?.source != crate::sema::check::ProducerFlowSource::Expression(value.origin) {
                    return Err(update_problem("record_update_original_replacement_flow_source"));
                }
                let mut selected = contract.receiver;
                let mut projections = Vec::with_capacity(old.projections.len());
                for (&field, projection) in old.path.iter().zip(&old.projections) {
                    if field != projection.field || resolved(selected)? != resolved(projection.receiver)? { return Err(update_problem("record_update_original_projection_endpoint")); }
                    for ty in [projection.receiver, projection.result] {
                        solved.graph.validate_scoped(ScopedRoot { ty, scope: original.checked.scope }).map_err(|_| update_problem("record_update_original_projection_owner"))?;
                    }
                    let receiver = graph_ground_type(&solved.graph, projection.receiver).map_err(|_| update_problem("record_update_closed_projection_receiver"))?;
                    let output = graph_ground_type(&solved.graph, projection.result).map_err(|_| update_problem("record_update_closed_projection_result"))?;
                    let Type::Record(fields) = &receiver else { return Err(update_problem("record_update_projection_record")); };
                    if fields.get(&field) != Some(&output) { return Err(update_problem("record_update_original_selected_field")); }
                    let receiver = self.intern_generic_ground_type(&receiver)?;
                    let result = self.intern_generic_ground_type(&output)?;
                    projections.push(PreparedUpdateProjection { receiver,
                        receiver_root: ScopedRoot { ty: projection.receiver, scope: original.checked.scope }, field, result,
                        result_root: ScopedRoot { ty: projection.result, scope: original.checked.scope } });
                    selected = projection.result;
                }
                let relation = solved.graph.constraint_origins().get(old.assignability).ok_or_else(|| update_problem("record_update_original_assignability"))?;
                let ConstraintRelation::Assignable { expected, actual } = relation.relation else { return Err(update_problem("record_update_original_assignability")); };
                if resolved(expected)? != resolved(selected)? || resolved(actual)? != resolved(old.value)? { return Err(update_problem("record_update_original_assignability_endpoint")); }
                let value = self.prepare_record_update_value(&solved, value, encoded, contract.caller)?;
                replacements.push(PreparedRecordReplacement { path: old.path.clone().into_boxed_slice(), projections: projections.into_boxed_slice(),
                    assignability: old.assignability, supplied_source: old.source, producer_flow: old.producer_flow, value });
            }
            self.generic_evidence_mut().add_record_update_source(PreparedRecordUpdateSource {
                origin: original.origin, checked: original.checked, instruction, owner, result, layout, schema, base, replacements: replacements.into_boxed_slice(),
            }).map_err(|_| update_problem("record_update_source_allocation"))?;
        }
        Ok(())
    }

    fn prepare_record_update_value(&mut self, solved: &SolvedTypes, value: &OriginalRecordUpdateValue, encoded: EncodedUpdateValue, owner: Option<crate::sema::check::DeclarationIdentity>) -> Result<PreparedUpdateValue, IrBuildError> {
        if solved.expressions.get(&value.origin) != Some(&value.checked.ty)
            || solved.expression_scope(value.origin, owner).map_err(|_| update_problem("record_update_value_original_scope"))? != value.checked.scope {
            return Err(update_problem("record_update_value_original_root"));
        }
        solved.graph.validate_scoped(value.checked).map_err(|_| update_problem("record_update_value_original_owner"))?;
        let ty = graph_ground_type(&solved.graph, value.checked.ty).map_err(|_| update_problem("record_update_value_closed_type"))?;
        let ty = self.intern_generic_ground_type(&ty)?;
        let (instruction, source_instruction) = encoded;
        let mut material = instruction;
        let mut wrappers = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        while material != source_instruction {
            if wrappers.len() >= 256 || !seen.insert(material) { return Err(update_problem("record_update_value_original_wrapper_lineage")); }
            let tag = *self.store.tags.get(material as usize).ok_or_else(|| update_problem("record_update_value_original_wrapper_lineage"))?;
            let payload = self.store.payload(self.store.data[material as usize].range()).map_err(|_| update_problem("record_update_value_original_wrapper_payload"))?.to_vec().into_boxed_slice();
            let child = match tag {
                FullTag::ExprCheckedValue => *payload.first().ok_or_else(|| update_problem("record_update_value_original_wrapper_child"))?,
                FullTag::ExprMatch => self.compiler_argument_wrappers.get(&material).ok_or_else(|| update_problem("record_update_value_original_compiler_wrapper"))?.body,
                _ => return Err(update_problem("record_update_value_original_wrapper_lineage")),
            };
            wrappers.push(PreparedUpdateWrapper { instruction: material, tag, payload, child });
            material = child;
        }
        let source_tag = *self.store.tags.get(source_instruction as usize).ok_or_else(|| update_problem("record_update_value_original_source"))?;
        let source_payload = self.store.payload(self.store.data[source_instruction as usize].range()).map_err(|_| update_problem("record_update_value_original_payload"))?.to_vec().into_boxed_slice();
        Ok(PreparedUpdateValue { origin: value.origin, checked: value.checked, instruction, source_instruction, ty,
            wrappers: wrappers.into_boxed_slice(), source_tag, source_payload })
    }
}

impl<'a> FullExecution<'a> {
    pub(in crate::runtime::eval) fn record_update_schema(&self, instruction: u32) -> Result<Option<Arc<super::super::super::require::PreparedSchema>>, IrVerifyError> {
        if !self.decoder.instruction_range.contains(&(instruction as usize)) { return Err(IrVerifyError::new("record update schema belongs to another body")); }
        let Some(generic) = self.generic_evidence() else { return Ok(None); };
        let Some(source) = generic.record_update_source_at(instruction)? else { return Ok(None); };
        let owner = if let Some(driver) = driver_owner_index(self.decoder.owner) { InstructionOwner::Driver(driver as u32) }
            else { InstructionOwner::Function(IrFunctionId::from_raw(self.decoder.owner).ok_or_else(|| IrVerifyError::new("record update schema owner is invalid"))?) };
        if source.instruction != instruction || source.owner != owner || self.decoder.store.tags.get(instruction as usize) != Some(&FullTag::ExprRecordUpdate) {
            return Err(IrVerifyError::new("record update schema belongs to another instruction or owner"));
        }
        generic.layout(source.layout)?;
        Ok(Some(Arc::clone(&source.schema)))
    }
}

impl FullVerifier {
    pub(super) fn verify_record_updates(store: &FullStore, generic: &GenericEvidenceStore) -> Result<(), IrVerifyError> {
        for (_, source) in generic.record_update_sources() {
            Self::verify_record_update_operand(store, generic, source.instruction, source.owner, &store.semantic.to_type(source.result)?, None, &mut Vec::new())?;
        }
        Ok(())
    }

    pub(super) fn verify_record_update_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, expected: &Type, instance: Option<InstantiationId>, active: &mut Vec<u32>) -> Result<(), IrVerifyError> {
        let source = generic.record_update_source_at(instruction)?.ok_or_else(|| IrVerifyError::new("record update lacks its original source proof"))?;
        if source.owner != owner || store.tags.get(instruction as usize) != Some(&FullTag::ExprRecordUpdate)
            || store.semantic.to_type(source.result)? != *expected {
            return Err(IrVerifyError::new("record update changes its original owner, opcode, or result row"));
        }
        let payload = store.payload(store.data[instruction as usize].range())?;
        if payload.len() != 3 || payload[0] != source.base.instruction { return Err(IrVerifyError::new("record update changes its original receiver")); }
        let block = IrBlockId::from_raw(payload[1]).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("record update replacement block is invalid"))?;
        if block.flags & BLOCK_SEQUENCE_KIND_MASK != BLOCK_LIST { return Err(IrVerifyError::new("record update replacement block has another kind")); }
        let mut words = FullCursor::new(store.payload(block.instructions)?);
        if words.raw()? as usize != source.replacements.len() { return Err(IrVerifyError::new("record update changes its original replacement count")); }
        let already_active = active.last() == Some(&instruction);
        if active.len() >= 256 || (!already_active && active.contains(&instruction)) { return Err(IrVerifyError::new("record update source is cyclic or too deep")); }
        if !already_active { active.push(instruction); }
        Self::verify_record_update_value_operand(store, generic, &source.base, owner, instance, active)?;
        for replacement in source.replacements.iter() {
            let path = IrBlockId::from_raw(words.raw()?).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("record update path block is invalid"))?;
            if path.flags & BLOCK_SEQUENCE_KIND_MASK != BLOCK_LIST { return Err(IrVerifyError::new("record update path block has another kind")); }
            let names = store.payload(path.instructions)?;
            if names.first().copied() != Some(replacement.path.len() as u32) || names.len() != replacement.path.len() + 1
                || names[1..].iter().copied().ne(replacement.path.iter().map(|name| name.symbol().raw())) {
                return Err(IrVerifyError::new("record update changes its original selected field path"));
            }
            if words.raw()? != replacement.value.instruction { return Err(IrVerifyError::new("record update changes its original supplied replacement")); }
            words.raw()?;
            Self::verify_record_update_value_operand(store, generic, &replacement.value, owner, instance, active)?;
        }
        words.finish()?;
        if !already_active { active.pop(); }
        Ok(())
    }

    fn verify_record_update_value_operand(store: &FullStore, generic: &GenericEvidenceStore, value: &PreparedUpdateValue, owner: InstructionOwner, instance: Option<InstantiationId>, active: &mut Vec<u32>) -> Result<(), IrVerifyError> {
        let mut material = value.instruction;
        for wrapper in value.wrappers.iter() {
            if material != wrapper.instruction || store.tags.get(material as usize) != Some(&wrapper.tag)
                || store.payload(store.data[material as usize].range())? != wrapper.payload.as_ref() {
                return Err(IrVerifyError::new("record update changes its original value wrapper lineage"));
            }
            let child = match wrapper.tag {
                FullTag::ExprCheckedValue => *wrapper.payload.first().ok_or_else(|| IrVerifyError::new("record update value wrapper is empty"))?,
                FullTag::ExprMatch => Self::original_compiler_argument_wrapper_body(store, generic, material, owner)?.ok_or_else(|| IrVerifyError::new("record update value loses its original compiler wrapper"))?,
                _ => return Err(IrVerifyError::new("record update value has an unauthenticated wrapper")),
            };
            if child != wrapper.child { return Err(IrVerifyError::new("record update value changes its original wrapper child")); }
            material = child;
        }
        if material != value.source_instruction || store.tags.get(material as usize) != Some(&value.source_tag)
            || store.payload(store.data[material as usize].range())? != value.source_payload.as_ref() {
            return Err(IrVerifyError::new("record update changes its original value source allocation"));
        }
        Self::verify_generic_source(store, generic, value.instruction, owner, &store.semantic.to_type(value.ty)?, instance, active)
    }
}

#[cfg(test)]
#[path = "record_update_prepare/tests.rs"]
mod tests;
