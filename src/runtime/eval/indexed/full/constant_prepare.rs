use super::*;
use super::super::generic::{OriginalConstantSource, PreparedConstantSource, graph_ground_type};
use crate::runtime::eval::require::PreparedSchema;

fn constant_problem(reason: &'static str) -> IrBuildError { IrBuildError::format(reason, None, 0, 0) }

impl FullBuilder {
    pub(super) fn stage_original_constant(&mut self, row: BuildExprId, instruction: u32, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        let Some(source) = scratch.constant_sources.get(&row) else { return Ok(()); };
        let raw = self.current_owner.ok_or_else(|| constant_problem("constant_source_instruction_owner"))?;
        let owner = if let Some(driver) = driver_owner_index(raw) { InstructionOwner::Driver(driver as u32) }
            else { InstructionOwner::Function(IrFunctionId::from_raw(raw).ok_or_else(|| constant_problem("constant_source_instruction_owner"))?) };
        self.constant_source_rows.push((source.clone(), instruction, owner));
        Ok(())
    }

    pub(super) fn prepare_constant_sources(&mut self) -> Result<(), IrBuildError> {
        let Some(solved) = self.solved.clone() else { return Ok(()); };
        for (source, instruction, owner) in self.constant_source_rows.clone() {
            if self.store.tags.get(instruction as usize) != Some(&FullTag::ExprPreparedConstant)
                || solved.expressions.get(&source.origin) != Some(&source.checked.ty)
                || solved.expression_owners.get(&source.origin).copied() != source.lexical_owner
                || solved.expression_scope(source.origin, source.lexical_owner).map_err(|_| constant_problem("constant_source_original_scope"))? != source.checked.scope {
                return Err(constant_problem("constant_source_original_checked_expression"));
            }
            solved.graph.validate_scoped(source.checked).map_err(|_| constant_problem("constant_source_original_root_owner"))?;
            let ty = graph_ground_type(&solved.graph, source.checked.ty).map_err(|_| constant_problem("constant_source_closed_type"))?;
            let words = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| constant_problem("constant_source_original_pool_operand"))?;
            if words.len() != 1 { return Err(constant_problem("constant_source_original_pool_operand")); }
            let pool = words[0];
            let value = self.store.prepared_constants.get(pool as usize).ok_or_else(|| constant_problem("constant_source_original_pool_allocation"))?.0.clone();
            let mut enums = crate::sema::wire_enums::PreparedWireEnums::default();
            enums.mappings.extend(self.store.wire_enums.iter().map(|mapping| (mapping.type_name, Arc::clone(mapping))));
            let schema = PreparedSchema::compile(&ty, &enums);
            let original = source.literal.as_ref().clone().in_type(&ty);
            let original = super::super::super::lower::lower_literal_constant(&original, Some(&enums)).ok_or_else(|| constant_problem("constant_source_original_literal"))?;
            let original = schema.materialize_constant_layout(original).ok_or_else(|| constant_problem("constant_source_original_physical_layout"))?;
            let value = schema.materialize_constant_layout(value).ok_or_else(|| constant_problem("constant_source_pool_physical_layout"))?;
            if original != value { return Err(constant_problem("constant_source_original_literal_changed")); }
            let mut work = 0;
            if !super::cli_call_prepare::prepared_constant_matches_type(&value, &ty, 0, &mut work).map_err(|_| constant_problem("constant_source_original_validation_bound"))? {
                return Err(constant_problem("constant_source_original_value_type"));
            }
            self.store.prepared_constants[pool as usize].0 = value.clone();
            let ty = self.intern_generic_ground_type(&ty)?;
            self.generic_evidence_mut().add_constant_source(PreparedConstantSource { original: source, instruction, owner, ty, pool, value, schema })
                .map_err(|_| constant_problem("constant_source_proof_allocation"))?;
        }
        Ok(())
    }
}

impl FullVerifier {
    pub(super) fn verify_constant_sources(store: &FullStore, generic: &GenericEvidenceStore) -> Result<(), IrVerifyError> {
        for (_, source) in generic.constant_sources() {
            Self::verify_constant_operand(store, generic, source.instruction, source.owner, &store.semantic.to_type(source.ty)?)?;
        }
        Ok(())
    }

    pub(super) fn verify_constant_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, expected: &Type) -> Result<(), IrVerifyError> {
        let source = generic.constant_source_at(instruction)?.ok_or_else(|| IrVerifyError::new("prepared constant lacks its original source receipt"))?;
        if source.owner != owner || store.tags.get(instruction as usize) != Some(&FullTag::ExprPreparedConstant)
            || store.semantic.to_type(source.ty)? != *expected || store.payload(store.data[instruction as usize].range())? != [source.pool]
            || store.prepared_constants.get(source.pool as usize).map(|value| &value.0) != Some(&source.value) {
            return Err(IrVerifyError::new("prepared constant changes its original owner, pool allocation, value, or checked type"));
        }
        let mut work = 0;
        if !super::cli_call_prepare::prepared_constant_matches_type(&source.value, expected, 0, &mut work)? {
            return Err(IrVerifyError::new("prepared constant disagrees with its original checked value contract"));
        }
        if source.schema.materialize_constant_layout(source.value.clone()).as_ref() != Some(&source.value) {
            return Err(IrVerifyError::new("prepared constant loses its original canonical physical layout"));
        }
        if !source.schema.visit_wire_mappings(&mut |mapping| wire_mapping_matches_pool(mapping, store)) {
            return Err(IrVerifyError::new("prepared constant changes its original schema wire authority"));
        }
        if !visit_value_wire_mappings(&source.value, &mut |mapping| wire_mapping_matches_pool(mapping, store)) {
            return Err(IrVerifyError::new("prepared constant changes its original nominal wire authority"));
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "constant_prepare/tests.rs"]
mod tests;
