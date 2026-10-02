use super::*;
use super::super::generic::{PreparedRecordEntry, PreparedRecordSource, RecordChildWrapper, RecordEntryKind, graph_ground_type};

fn record_problem(construct: &'static str) -> IrBuildError { IrBuildError::format(construct, None, 0, 0) }

impl FullBuilder {
    pub(super) fn stage_original_record_source(&mut self, row: BuildExprId, instruction: u32, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        let Some(original) = scratch.record_sources.get(&row) else { return Ok(()); };
        let raw = self.current_owner.ok_or_else(|| record_problem("record_source_instruction_owner"))?;
        let owner = if let Some(index) = driver_owner_index(raw) { InstructionOwner::Driver(index as u32) }
            else { InstructionOwner::Function(IrFunctionId::from_raw(raw).ok_or_else(|| record_problem("record_source_instruction_owner"))?) };
        let entries = original.entries.iter().map(|entry| {
            let instruction = *self.active_encoded_expressions.get(&entry.row).ok_or_else(|| record_problem("record_entry_encoded_row_missing"))?;
            let source = *self.active_encoded_expressions.get(&entry.material).ok_or_else(|| record_problem("record_entry_original_row_missing"))?;
            Ok((instruction, source))
        }).collect::<Result<Vec<_>, IrBuildError>>()?;
        self.record_source_rows.push((original.clone(), instruction, owner, entries.into_boxed_slice()));
        Ok(())
    }

    pub(super) fn prepare_record_sources(&mut self) -> Result<(), IrBuildError> {
        let Some(solved) = self.solved.clone() else { return Ok(()); };
        let mut layouts = BTreeMap::new();
        if let Some(generic) = self.generic.as_ref() {
            for constructor in generic.constructors() {
                let layout = generic.layout(constructor.layout).map_err(|_| record_problem("record_source_layout_owner"))?;
                let (names, _) = self.store.semantic.record_fields(layout.record_type).map_err(|_| record_problem("record_source_layout_fields"))?;
                if names.iter().copied().eq(layout.fields.iter().map(|(name, _)| *name)) {
                    layouts.insert(layout.record_type, constructor.layout);
                }
            }
        }
        for (original, instruction, owner, encoded) in self.record_source_rows.clone() {
            if self.store.tags.get(instruction as usize) != Some(&FullTag::ExprRecord) || encoded.len() != original.entries.len() {
                return Err(record_problem("record_source_original_allocation"));
            }
            let scope = solved.expression_owners.get(&original.origin).copied();
            if solved.expressions.get(&original.origin) != Some(&original.checked.ty)
                || solved.expression_scope(original.origin, scope).map_err(|_| record_problem("record_source_original_scope"))? != original.checked.scope {
                return Err(record_problem("record_source_original_checked_root"));
            }
            solved.graph.validate_scoped(original.checked).map_err(|_| record_problem("record_source_original_root_owner"))?;
            let ty = graph_ground_type(&solved.graph, original.checked.ty).map_err(|_| record_problem("record_source_closed_type"))?;
            if !matches!(ty, Type::Record(_)) { return Err(record_problem("record_source_record_type")); }
            let schema = super::super::super::require::PreparedSchema::compile_record_layout(&ty).ok_or_else(|| record_problem("record_source_physical_schema"))?;
            let result = self.intern_generic_ground_type(&ty)?;
            let layout = self.ground_projection_layout(result, &mut layouts)?;
            let mut entries = Vec::with_capacity(original.entries.len());
            for (entry, &(instruction, source_instruction)) in original.entries.iter().zip(encoded.iter()) {
                if solved.expressions.get(&entry.origin) != Some(&entry.checked.ty)
                    || solved.expression_scope(entry.origin, scope).map_err(|_| record_problem("record_entry_original_scope"))? != entry.checked.scope {
                    return Err(record_problem("record_entry_original_checked_root"));
                }
                solved.graph.validate_scoped(entry.checked).map_err(|_| record_problem("record_entry_original_root_owner"))?;
                let ty = graph_ground_type(&solved.graph, entry.checked.ty).map_err(|_| record_problem("record_entry_closed_type"))?;
                if entry.kind == RecordEntryKind::Spread && !matches!(ty, Type::Record(_)) { return Err(record_problem("record_spread_closed_record")); }
                let ty = self.intern_generic_ground_type(&ty)?;
                let mut wrappers = Vec::new();
                let mut material = instruction;
                let mut seen = std::collections::BTreeSet::new();
                while material != source_instruction {
                    if wrappers.len() >= 256 || !seen.insert(material) || self.store.tags.get(material as usize) != Some(&FullTag::ExprCheckedValue) {
                        return Err(record_problem("record_child_original_wrapper_lineage"));
                    }
                    let payload = self.store.payload(self.store.data[material as usize].range()).map_err(|_| record_problem("record_child_original_wrapper_payload"))?.to_vec().into_boxed_slice();
                    let child = *payload.first().ok_or_else(|| record_problem("record_child_original_wrapper_child"))?;
                    wrappers.push(RecordChildWrapper { instruction: material, payload });
                    material = child;
                }
                entries.push(PreparedRecordEntry { kind: entry.kind, origin: entry.origin, checked: entry.checked, instruction, source_instruction, ty, wrappers: wrappers.into_boxed_slice() });
            }
            self.generic_evidence_mut().add_record_source(PreparedRecordSource {
                origin: original.origin, checked: original.checked, instruction, owner, result, layout, schema, entries: entries.into_boxed_slice(),
            }).map_err(|_| record_problem("record_source_proof_allocation"))?;
        }
        Ok(())
    }
}

impl<'a> FullExecution<'a> {
    pub(in crate::runtime::eval) fn record_source_layout(&self, instruction: u32) -> Result<Option<(super::super::generic::PhysicalLayoutId, Arc<super::super::super::require::PreparedSchema>)>, IrVerifyError> {
        if !self.decoder.instruction_range.contains(&(instruction as usize)) { return Err(IrVerifyError::new("record source layout belongs to another body")); }
        let Some(generic) = self.generic_evidence() else { return Ok(None); };
        let Some(source) = generic.record_source_at(instruction)? else { return Ok(None); };
        let owner = if let Some(driver) = driver_owner_index(self.decoder.owner) { InstructionOwner::Driver(driver as u32) }
            else { InstructionOwner::Function(IrFunctionId::from_raw(self.decoder.owner).ok_or_else(|| IrVerifyError::new("record source layout owner is invalid"))?) };
        if source.instruction != instruction || source.owner != owner || self.decoder.store.tags.get(instruction as usize) != Some(&FullTag::ExprRecord) {
            return Err(IrVerifyError::new("record source layout belongs to another instruction or owner"));
        }
        generic.layout(source.layout)?;
        Ok(Some((source.layout, Arc::clone(&source.schema))))
    }
}

impl FullVerifier {
    pub(super) fn verify_record_sources(store: &FullStore, generic: &GenericEvidenceStore) -> Result<(), IrVerifyError> {
        for (_, source) in generic.record_sources() {
            Self::verify_record_source_operand(store, generic, source.instruction, source.owner, &store.semantic.to_type(source.result)?, None, &mut Vec::new())?;
        }
        Ok(())
    }

    pub(super) fn verify_record_source_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, expected: &Type, instance: Option<InstantiationId>, active: &mut Vec<u32>) -> Result<(), IrVerifyError> {
        let source = generic.record_source_at(instruction)?.ok_or_else(|| IrVerifyError::new("record spread lacks its original source proof"))?;
        if source.owner != owner || store.tags.get(instruction as usize) != Some(&FullTag::ExprRecord)
            || store.semantic.to_type(source.result)? != *expected {
            return Err(IrVerifyError::new("record spread changes its original owner, opcode, or result"));
        }
        let payload = store.payload(store.data[instruction as usize].range())?;
        if payload.len() != 1 { return Err(IrVerifyError::new("record spread source payload is invalid")); }
        let block = payload.first().copied().and_then(IrBlockId::from_raw).and_then(|id| store.blocks.get(id.index()))
            .ok_or_else(|| IrVerifyError::new("record spread source block is invalid"))?;
        if block.flags & BLOCK_SEQUENCE_KIND_MASK != BLOCK_LIST { return Err(IrVerifyError::new("record spread source block has another kind")); }
        let mut words = FullCursor::new(store.payload(block.instructions)?);
        if words.raw()? as usize != source.entries.len() { return Err(IrVerifyError::new("record spread changes its original entry count")); }
        let already_active = active.last() == Some(&instruction);
        if active.len() >= 256 || (!already_active && active.contains(&instruction)) { return Err(IrVerifyError::new("record spread source is cyclic or too deep")); }
        if !already_active { active.push(instruction); }
        for entry in source.entries.iter() {
            let actual = match (words.raw()?, entry.kind) {
                (0, RecordEntryKind::Field(name)) if words.raw()? == name.symbol().raw() => words.raw()?,
                (1, RecordEntryKind::Spread) => words.raw()?,
                _ => return Err(IrVerifyError::new("record spread changes its original entry kind or key")),
            };
            if actual != entry.instruction { return Err(IrVerifyError::new("record spread changes its original supplied value")); }
            let mut material = actual;
            for wrapper in entry.wrappers.iter() {
                if material != wrapper.instruction || store.tags.get(material as usize) != Some(&FullTag::ExprCheckedValue)
                    || store.payload(store.data[material as usize].range())? != wrapper.payload.as_ref() {
                    return Err(IrVerifyError::new("record spread changes its original child wrapper lineage"));
                }
                material = *wrapper.payload.first().ok_or_else(|| IrVerifyError::new("record spread child wrapper is empty"))?;
            }
            if material != entry.source_instruction { return Err(IrVerifyError::new("record spread loses its original child source")); }
            Self::verify_generic_source(store, generic, actual, owner, &store.semantic.to_type(entry.ty)?, instance, active)?;
        }
        words.finish()?;
        if !already_active { active.pop(); }
        Ok(())
    }
}

#[cfg(test)]
#[path = "record_prepare/tests.rs"]
mod tests;
