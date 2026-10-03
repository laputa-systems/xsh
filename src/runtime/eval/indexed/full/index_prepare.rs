use super::*;
use super::super::generic::{OriginalIndex, OperationSourceOrigin};
use crate::sema::check::{ExpressionIdentity, ProducerFlowSource};
use crate::sema::inference::{ScopedRequirementRoot, ScopedRoot};
use crate::sema::operation_graph::PreparedLanguageOperation;

/// Source operands and lowered children have different identity domains.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct BuildIndexOrigin {
    pub origin: ExpressionIdentity,
    pub base: ExpressionIdentity,
    pub index: ExpressionIdentity,
    pub base_row: BuildExprId,
    pub base_material_row: BuildExprId,
    pub postfix_base: bool,
    pub index_row: BuildExprId,
    pub index_material_row: BuildExprId,
    pub uint_key_validation_row: Option<BuildExprId>,
    pub index_material_instruction: Option<u32>,
    pub base_material_instruction: Option<u32>,
    pub uint_key_validation: Option<(u32, Box<[u32]>)>,
}

fn problem(reason: &'static str) -> IrBuildError { IrBuildError::format(reason, None, 0, 0) }

impl FullBuilder {
    pub(super) fn stage_original_index(&mut self, expression: BuildExprId, instruction: u32, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        let Some(original) = scratch.index_origins.get(&expression) else { return Ok(()); };
        let raw = self.current_owner.ok_or_else(|| problem("index_original_owner_missing"))?;
        let owner = if let Some(index) = driver_owner_index(raw) { InstructionOwner::Driver(index as u32) } else { InstructionOwner::Function(IrFunctionId::from_raw(raw).ok_or_else(|| problem("index_original_owner_invalid"))?) };
        let BuildExprRow::Index { base, index, .. } = scratch.expressions.get(expression.index()).ok_or_else(|| problem("index_original_row_missing"))? else { return Err(problem("index_original_row_changed")); };
        if *base != original.base_row || *index != original.index_row || self.active_expression_origins.get(&expression) != Some(&original.origin) {
            return Err(problem("index_original_children_changed"));
        }
        if self.active_expression_origins.get(&original.index_material_row) != Some(&original.index) { return Err(problem("index_original_key_material_changed")); }
        let words = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| problem("index_original_payload"))?;
        if self.store.tags.get(instruction as usize) != Some(&FullTag::ExprIndex) || words.len() != 3
            || self.active_encoded_expressions.get(&original.base_row) != words.first()
            || self.active_encoded_expressions.get(&original.index_row) != words.get(1) { return Err(problem("index_original_children_changed")); }
        if self.active_expression_origins.get(&original.base_material_row) != Some(&original.base) { return Err(problem("index_original_base_material_changed")); }
        let mut original = original.clone();
        original.base_material_instruction = self.active_encoded_expressions.get(&original.base_material_row).copied();
        original.index_material_instruction = self.active_encoded_expressions.get(&original.index_material_row).copied();
        if let Some(validation_row) = original.uint_key_validation_row {
            let BuildExprRow::Try(validation) = scratch.expressions.get(original.index_row.index()).ok_or_else(|| problem("index_original_key_wrapper_missing"))? else { return Err(problem("index_original_key_wrapper_changed")); };
            let BuildExprRow::Require { value, check, .. } = scratch.expressions.get(validation_row.index()).ok_or_else(|| problem("index_original_key_validation_missing"))? else { return Err(problem("index_original_key_validation_changed")); };
            if *validation != validation_row || check.ty != Type::UInt || check.schema.is_some() { return Err(problem("index_original_key_validation_changed")); }
            let validation = *self.active_encoded_expressions.get(&validation_row).ok_or_else(|| problem("index_original_key_validation_not_encoded"))?;
            let payload = self.store.payload(self.store.data[validation as usize].range()).map_err(|_| problem("index_original_key_validation_payload"))?.to_vec().into_boxed_slice();
            if payload.first() != self.active_encoded_expressions.get(value) || self.store.tags.get(validation as usize) != Some(&FullTag::ExprRequire) { return Err(problem("index_original_key_validation_changed")); }
            original.uint_key_validation = Some((validation, payload));
        }
        self.index_rows.push((original, instruction, owner));
        Ok(())
    }

    pub(super) fn prepare_original_indices(&mut self) -> Result<(), IrBuildError> {
        let Some(solved) = self.solved.clone() else { return Ok(()); };
        let mut operations = FxHashMap::default();
        if let Some(generic) = self.generic.as_ref() {
            for (id, operation) in generic.operations() {
                let source = generic.operation_source(operation.source).map_err(|_| problem("index_original_prepared_source"))?;
                if matches!(operation.authority, super::super::generic::PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Index { .. } | PreparedLanguageOperation::ConstantKeyProjection { .. }, .. }) {
                    operations.insert(source.instruction, (id, operation.clone(), source.clone()));
                }
            }
        }
        let origins = self.generic_expression_rows.iter().map(|&(instruction, origin, owner)| (instruction, (origin, owner))).collect::<FxHashMap<_, _>>();
        let mut layouts = BTreeMap::new();
        for (original, instruction, owner) in self.index_rows.clone() {
            let operation = solved.operations.get(&original.origin).ok_or_else(|| problem("index_original_operation_missing"))?;
            let (id, prepared, source) = operations.get(&instruction).ok_or_else(|| problem("index_original_prepared_operation_missing"))?;
            let scope = solved.operation_scope(ProducerFlowSource::Expression(original.origin), operation).map_err(|_| problem("index_original_scope"))?;
            solved.graph.validate_requirement_scoped(ScopedRequirementRoot { requirement: operation.requirement, scope }).map_err(|_| problem("index_original_requirement_scope"))?;
            if source.origin != OperationSourceOrigin::Expression(original.origin) || source.owner != owner || origins.get(&instruction) != Some(&(original.origin, owner))
                || operation.receiver.is_some() || operation.actual_arguments.len() != 2 || operation.binding.supplied_slots != [0, 1]
                || !operation.binding.default_slots.is_empty() || operation.binding.rest_slot.is_some() || operation.binding.dynamic.is_some() || !operation.argument_coercions.is_empty() {
                return Err(problem("index_original_source_changed"));
            }
            match (owner, operation.caller) {
                (InstructionOwner::Function(owner), Some(caller)) if self.declaration_functions.get(&caller) == Some(&owner) => {},
                (InstructionOwner::Driver(_), None) => {},
                _ => return Err(problem("index_original_caller_changed")),
            }
            GenericEvidenceStore::verify_index_operation_contract(&self.store.semantic, prepared).map_err(|_| problem("index_original_contract"))?;
            let words = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| problem("index_original_payload"))?;
            let [base, index, _] = words else { return Err(problem("index_original_payload")); };
            let (base, index) = (*base, *index);
            let index_material = original.index_material_instruction.ok_or_else(|| problem("index_original_key_material_missing"))?;
            let uint_key_validation = original.uint_key_validation.clone();
            let postfix_base = if original.postfix_base {
                let Some(TypeRef::Ground(base_type)) = prepared.arguments[0] else { return Err(problem("index_original_postfix_domain")); };
                let expected = self.store.semantic.to_type(base_type).map_err(|_| problem("index_original_postfix_descriptor"))?;
                Some(self.prepare_checked_result_receiver(base, owner, &expected, operation.caller)?.ok_or_else(|| problem("index_original_postfix_authority_missing"))?)
            } else { None };
            // The authored base owns the Result carrier; its generated Try has no expression identity.
            let (base_material, base_wrappers) = if let Some(postfix) = &postfix_base {
                if postfix.origin != original.base { return Err(problem("index_original_postfix_source_changed")); }
                (postfix.source_instruction, Box::new([]) as Box<[_]>)
            } else { self.argument_initializer_lineage(base, owner)? };
            let index_initializer = uint_key_validation.as_ref().map_or(index, |(_, payload)| payload[0]);
            let (material, index_wrappers) = self.argument_initializer_lineage(index_initializer, owner)?;
            if material != index_material || original.base_material_instruction != Some(base_material) {
                return Err(problem("index_original_material_lineage_changed"));
            }
            let base_parameter = super::super::super::BuildIterationBindingOrigin::original_parameter(&solved, original.base, operation.caller).ok_or_else(|| problem("index_original_base_parameter"))?;
            let index_parameter = super::super::super::BuildIterationBindingOrigin::original_parameter(&solved, original.index, operation.caller).ok_or_else(|| problem("index_original_key_parameter"))?;
            let operands = [(base_material, original.base), (index_material, original.index)];
            for (ordinal, (operand, origin)) in operands.into_iter().enumerate() {
                if origins.get(&operand) != Some(&(origin, owner)) || origin.source != original.origin.source || origin.namespace != original.origin.namespace {
                    return Err(problem("index_original_operand_source_changed"));
                }
                let ty = *solved.expressions.get(&origin).ok_or_else(|| problem("index_original_operand_type_missing"))?;
                let operand_scope = solved.expression_scope(origin, operation.caller).map_err(|_| problem("index_original_operand_scope"))?;
                solved.graph.validate_scoped(ScopedRoot { ty, scope: operand_scope }).map_err(|_| problem("index_original_operand_scope"))?;
                let checked_argument = operation.actual_arguments[ordinal];
                solved.graph.validate_scoped(ScopedRoot { ty: checked_argument, scope }).map_err(|_| problem("index_original_argument_scope"))?;
                let actual = super::super::generic::graph_ground_type(&solved.graph, ty).map_err(|_| problem("index_original_operand_not_ground"))?;
                let argument = super::super::generic::graph_ground_type(&solved.graph, checked_argument).map_err(|_| problem("index_original_argument_not_ground"))?;
                let actual = if ordinal == 0 && let Some(postfix) = &postfix_base {
                    let (TypeRef::Ground(source), TypeRef::Ground(success)) = (postfix.source_type, postfix.success_type) else { return Err(problem("index_original_postfix_not_ground")); };
                    if self.store.semantic.to_type(source).map_err(|_| problem("index_original_postfix_source_descriptor"))? != actual { return Err(problem("index_original_postfix_source_type_changed")); }
                    self.store.semantic.to_type(success).map_err(|_| problem("index_original_postfix_success_descriptor"))?
                } else { actual };
                if actual != argument { return Err(problem("index_original_operand_type_changed")); }
                let Some(TypeRef::Ground(prepared_ty)) = prepared.arguments[ordinal] else { return Err(problem("index_original_operand_not_ground")); };
                if self.store.semantic.to_type(prepared_ty).map_err(|_| problem("index_original_operand_descriptor"))? != actual { return Err(problem("index_original_operand_type_changed")); }
            }
            let ty = *solved.expressions.get(&original.origin).ok_or_else(|| problem("index_original_result_type_missing"))?;
            let result_scope = solved.expression_scope(original.origin, operation.caller).map_err(|_| problem("index_original_result_scope"))?;
            solved.graph.validate_scoped(ScopedRoot { ty, scope: result_scope }).map_err(|_| problem("index_original_result_scope"))?;
            solved.graph.validate_scoped(ScopedRoot { ty: operation.result, scope }).map_err(|_| problem("index_original_operation_result_scope"))?;
            let TypeRef::Ground(prepared_result) = prepared.result else { return Err(problem("index_original_result_not_ground")); };
            let result = super::super::generic::graph_ground_type(&solved.graph, ty).map_err(|_| problem("index_original_result_not_ground"))?;
            let operation_result = super::super::generic::graph_ground_type(&solved.graph, operation.result).map_err(|_| problem("index_original_operation_result_not_ground"))?;
            if result != operation_result { return Err(problem("index_original_result_type_changed")); }
            if self.store.semantic.to_type(prepared_result).map_err(|_| problem("index_original_result_descriptor"))? != result { return Err(problem("index_original_result_type_changed")); }
            let projection = if let super::super::generic::PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::ConstantKeyProjection { field }, .. } = prepared.authority {
                let Some(TypeRef::Ground(receiver)) = prepared.arguments[0] else { return Err(problem("index_original_projection_receiver")); };
                let layout = self.ground_projection_layout(receiver, &mut layouts)?;
                let physical = self.generic.as_ref().unwrap().layout(layout).map_err(|_| problem("index_original_projection_layout"))?;
                let slot = physical.fields.iter().position(|&(name, ty)| name == field && ty == prepared_result).ok_or_else(|| problem("index_original_projection_field"))?;
                Some((field, layout, u32::try_from(slot).map_err(|_| problem("index_original_projection_slot"))?))
            } else { None };
            self.generic_evidence_mut().add_original_index(OriginalIndex {
                origin: original.origin, base_origin: original.base, index_origin: original.index,
                requirement: operation.requirement, operation: *id, instruction, base, index, index_material, base_material, postfix_base, base_wrappers, index_wrappers, projection, uint_key_validation, owner,
                base_parameter, index_parameter,
            }).map_err(|_| problem("index_original_receipt_capacity"))?;
        }
        Ok(())
    }
}

impl FullVerifier {
    pub(super) fn verify_index_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, expected: &Type, active: &mut Vec<u32>) -> Result<bool, IrVerifyError> {
        let Some(original) = generic.original_index(instruction)? else { return Ok(false); };
        let operation = generic.operation(original.operation)?;
        let source = generic.operation_source(operation.source)?;
        GenericEvidenceStore::verify_index_operation_contract(&store.semantic, operation)?;
        if original.owner != owner || source.instruction != instruction || source.owner != owner || source.origin != OperationSourceOrigin::Expression(original.origin)
            || store.tags.get(instruction as usize) != Some(&FullTag::ExprIndex) { return Err(IrVerifyError::new("original index belongs to another operation, opcode, or owner")); }
        let words = store.payload(store.data[instruction as usize].range())?;
        if words.len() != 3 || words[0] != original.base || words[1] != original.index || operation.binding.operands.as_ref() != [original.base, original.index]
            || words.get(2).and_then(|&location| IrLocationId::from_raw(location)).and_then(|location| store.location_sources.get(location.index())) != Some(&original.origin.source) {
            return Err(IrVerifyError::new("index changes its original source operands or location"));
        }
        for (operand, origin) in [(instruction, original.origin), (original.base_material, original.base_origin), (original.index_material, original.index_origin)] {
            if generic.registered_instruction_origin(operand, false) != Some((OperationSourceOrigin::Expression(origin), owner)) {
                return Err(IrVerifyError::new("index loses its original operand or instruction source"));
            }
        }
        let Some(TypeRef::Ground(base_type)) = operation.arguments.first().copied().flatten() else { return Err(IrVerifyError::new("index receiver is not ground")); };
        let uint_key = matches!(store.semantic.to_type(base_type)?, Type::Map(key, _) if *key == Type::UInt);
        if uint_key {
            let (validation, payload) = original.uint_key_validation.as_ref().ok_or_else(|| IrVerifyError::new("unsigned map index loses its original key validation"))?;
            if store.tags.get(original.index as usize) != Some(&FullTag::ExprTry) || store.payload(store.data[original.index as usize].range())? != [*validation]
                || store.tags.get(*validation as usize) != Some(&FullTag::ExprRequire) || store.payload(store.data[*validation as usize].range())? != payload.as_ref()
                || payload.first() != Some(&original.index_wrappers.first().map_or(original.index_material, |wrapper| wrapper.instruction)) || payload.get(3) != Some(&0)
                || payload.get(1).and_then(|&ty| TypeId::from_raw(ty)).map(|ty| store.semantic.to_type(ty)).transpose()? != Some(Type::UInt) {
                return Err(IrVerifyError::new("unsigned map index changes its original key validation or material source"));
            }
        } else if original.uint_key_validation.is_some() { return Err(IrVerifyError::new("index introduces an unsigned validation into another key domain")); }
        if let Some(postfix) = &original.postfix_base {
            if postfix.instruction != original.base || postfix.source_instruction != original.base_material || postfix.owner != owner || postfix.origin != original.base_origin || !original.base_wrappers.is_empty() {
                return Err(IrVerifyError::new("index changes its original Result postfix carrier or owner"));
            }
        } else { Self::verify_argument_initializer_lineage(store, generic, original.base, original.base_material, &original.base_wrappers, owner)?; }
        let key_initializer = original.uint_key_validation.as_ref().map_or(original.index, |(_, payload)| payload[0]);
        Self::verify_argument_initializer_lineage(store, generic, key_initializer, original.index_material, &original.index_wrappers, owner)?;
        if let Some((field, layout, slot)) = original.projection {
            let layout = generic.layout(layout)?;
            if layout.fields.get(slot as usize).map(|&(name, _)| name) != Some(field) {
                return Err(IrVerifyError::new("record index changes its original physical field layout"));
            }
            Self::verify_constant_field_key(store, original.index_material, field)?;
        }
        for (operand, parameter) in [(original.base_material, original.base_parameter), (original.index_material, original.index_parameter)] {
            if let Some((declaration, slot)) = parameter {
                let function = generic.checked_function(declaration)?;
                if owner != InstructionOwner::Function(function.target) || store.tags.get(operand as usize) != Some(&FullTag::ExprParam)
                    || store.payload(store.data[operand as usize].range())? != [slot] {
                    return Err(IrVerifyError::new("index operand changes its original parameter port"));
                }
            }
        }
        let TypeRef::Ground(result) = operation.result else { return Err(IrVerifyError::new("index result is not ground")); };
        if store.semantic.to_type(result)? != *expected { return Err(IrVerifyError::new("index result disagrees with its consumer")); }
        let already_active = active.last() == Some(&instruction);
        if active.len() >= 256 || (!already_active && active.contains(&instruction)) { return Err(IrVerifyError::new("index operands are cyclic or too deep")); }
        if !already_active { active.push(instruction); }
        for (ordinal, (&operand, &ty)) in [original.base_material, original.index_material].iter().zip(operation.arguments.iter()).enumerate() {
            let Some(TypeRef::Ground(ty)) = ty else { return Err(IrVerifyError::new("index operand is not ground")); };
            let expected = store.semantic.to_type(ty)?;
            if ordinal == 0 && let Some(postfix) = &original.postfix_base {
                Self::verify_result_receiver(store, generic, postfix, original.origin, &expected, active)?;
            } else { Self::verify_generic_source(store, generic, operand, owner, &expected, None, active)?; }
        }
        if !already_active { active.pop(); }
        Ok(true)
    }
}

#[cfg(test)]
#[path = "index_prepare/tests.rs"]
mod tests;
