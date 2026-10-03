use super::*;
use super::super::generic::{OperationSourceOrigin, PreparedResultReceiver, graph_ground_type};

fn problem(reason: &'static str) -> IrBuildError { IrBuildError::format(reason, None, 0, 0) }

impl FullBuilder {
    pub(super) fn stage_original_result_receiver(
        &mut self, row: BuildExprId, instruction: u32, owner: InstructionOwner, scratch: &BuildScratch,
    ) -> Result<(), IrBuildError> {
        let Some(original) = scratch.result_receiver_origins.get(&row) else { return Ok(()); };
        let carrier = *self.active_encoded_expressions.get(&original.carrier)
            .ok_or_else(|| problem("result_receiver_carrier_not_encoded"))?;
        if self.store.tags.get(instruction as usize) != Some(&FullTag::ExprTry)
            || self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| problem("result_receiver_try_payload"))? != [carrier] {
            return Err(problem("result_receiver_original_try_changed"));
        }
        self.result_receiver_rows.push((original.clone(), instruction, owner, carrier));
        Ok(())
    }

    pub(super) fn prepare_result_receiver(
        &mut self, instruction: u32, owner: InstructionOwner, expected: &Type,
        call: crate::sema::check::ExpressionIdentity,
    ) -> Result<Option<PreparedResultReceiver>, IrBuildError> {
        if !self.result_receiver_rows.iter().any(|(_, generated, _, _)| *generated == instruction) { return Ok(None); }
        let solved = self.solved.clone().ok_or_else(|| problem("result_receiver_original_solved_missing"))?;
        let operation = solved.operations.get(&call).ok_or_else(|| problem("result_receiver_original_call_missing"))?;
        if graph_ground_type(&solved.graph, operation.receiver.ok_or_else(|| problem("result_receiver_selected_receiver_missing"))?)
            .map_err(|_| problem("result_receiver_selected_receiver_requires_closed_type"))? != *expected {
            return Err(problem("result_receiver_original_success_boundary_changed"));
        }
        self.prepare_checked_result_receiver(instruction, owner, expected, operation.caller)
    }

    pub(super) fn prepare_projection_result_receiver(
        &mut self, instruction: u32, owner: InstructionOwner, expected: &Type,
        expression: crate::sema::check::ExpressionIdentity,
    ) -> Result<Option<PreparedResultReceiver>, IrBuildError> {
        let Some((original, _, _, _)) = self.result_receiver_rows.iter().find(|(_, generated, _, _)| *generated == instruction) else { return Ok(None); };
        let solved = self.solved.clone().ok_or_else(|| problem("result_receiver_original_solved_missing"))?;
        let projection = solved.projections.get(&expression).ok_or_else(|| problem("result_receiver_original_projection_missing"))?;
        if projection.receiver != original.success_type.ty { return Err(problem("result_receiver_original_projection_success_changed")); }
        self.prepare_checked_result_receiver(instruction, owner, expected, solved.expression_owners.get(&expression).copied())
    }

    pub(super) fn prepare_checked_result_receiver(
        &mut self, instruction: u32, owner: InstructionOwner, expected: &Type,
        caller: Option<crate::sema::check::DeclarationIdentity>,
    ) -> Result<Option<PreparedResultReceiver>, IrBuildError> {
        let Some((original, generated, receiver_owner, carrier)) = self.result_receiver_rows.iter()
            .find(|(_, generated, _, _)| *generated == instruction).cloned() else { return Ok(None); };
        if receiver_owner != owner { return Err(problem("result_receiver_original_owner_changed")); }
        let solved = self.solved.clone().ok_or_else(|| problem("result_receiver_original_solved_missing"))?;
        let graph = &solved.graph;
        for root in [original.source_type, original.success_type, original.error_type] {
            graph.validate_scoped(root).map_err(|_| problem("result_receiver_original_root_owner"))?;
        }
        if solved.expressions.get(&original.origin) != Some(&original.source_type.ty)
            || solved.expression_owners.get(&original.origin).copied() != caller
            || solved.expression_scope(original.origin, caller).map_err(|_| problem("result_receiver_original_scope"))? != original.source_type.scope
            || original.success_type.scope != original.source_type.scope || original.error_type.scope != original.source_type.scope {
            return Err(problem("result_receiver_original_source_changed"));
        }
        let crate::sema::inference::TypeNode::Result(success, error) = graph.node(graph.resolved(original.source_type.ty).map_err(|_| problem("result_receiver_original_carrier_root"))?)
            .map_err(|_| problem("result_receiver_original_carrier_node"))? else { return Err(problem("result_receiver_original_carrier_not_result")); };
        if *success != original.success_type.ty || *error != original.error_type.ty { return Err(problem("result_receiver_original_domains_changed")); }
        let source_type = graph_ground_type(graph, original.source_type.ty).map_err(|_| problem("result_receiver_carrier_requires_closed_type"))?;
        let success_type = graph_ground_type(graph, original.success_type.ty).map_err(|_| problem("result_receiver_success_requires_closed_type"))?;
        let error_type = graph_ground_type(graph, original.error_type.ty).map_err(|_| problem("result_receiver_error_requires_closed_type"))?;
        if success_type != *expected || source_type != Type::Result(Box::new(success_type.clone()), Box::new(error_type.clone())) {
            return Err(problem("result_receiver_original_success_boundary_changed"));
        }
        let (source_instruction, source_wrappers) = self.argument_initializer_lineage(carrier, owner)?;
        if !self.generic_expression_rows.iter().any(|&(instruction, origin, source_owner)| instruction == source_instruction && origin == original.origin && source_owner == owner)
            || self.generic_expression_rows.iter().any(|&(instruction, _, _)| instruction == generated) {
            return Err(problem("result_receiver_original_carrier_identity_missing"));
        }
        let payload = self.store.payload(self.store.data[generated as usize].range()).map_err(|_| problem("result_receiver_original_payload"))?.to_vec().into_boxed_slice();
        if self.store.tags.get(generated as usize) != Some(&FullTag::ExprTry) || payload.as_ref() != [carrier] { return Err(problem("result_receiver_original_try_changed")); }
        Ok(Some(PreparedResultReceiver {
            origin: original.origin, instruction: generated, carrier, source_instruction, source_wrappers, owner,
            source_type: TypeRef::Ground(self.intern_generic_ground_type(&source_type)?),
            success_type: TypeRef::Ground(self.intern_generic_ground_type(&success_type)?),
            error_type: TypeRef::Ground(self.intern_generic_ground_type(&error_type)?), payload,
        }))
    }
}

impl FullVerifier {
    pub(super) fn verify_result_receiver(
        store: &FullStore, generic: &GenericEvidenceStore, receiver: &PreparedResultReceiver,
        call: crate::sema::check::ExpressionIdentity, expected: &Type, active: &mut Vec<u32>,
    ) -> Result<Type, IrVerifyError> {
        if receiver.origin.source != call.source
            || generic.registered_instruction_origin(receiver.instruction, false).is_some()
            || generic.registered_instruction_origin(receiver.source_instruction, false) != Some((OperationSourceOrigin::Expression(receiver.origin), receiver.owner)) {
            return Err(IrVerifyError::new("Result postfix receiver changes its original carrier identity or owner"));
        }
        let range = match receiver.owner {
            InstructionOwner::Function(function) => store.function_instruction_range(function.index()),
            InstructionOwner::Driver(driver) => store.driver_instruction_range(driver as usize),
        }?;
        if !range.contains(&(receiver.instruction as usize)) || !range.contains(&(receiver.carrier as usize))
            || receiver.carrier >= receiver.instruction
            || store.tags.get(receiver.instruction as usize) != Some(&FullTag::ExprTry)
            || receiver.payload.as_ref() != [receiver.carrier]
            || store.payload(store.data[receiver.instruction as usize].range())? != receiver.payload.as_ref() {
            return Err(IrVerifyError::new("Result postfix receiver changes its original generated Try"));
        }
        let (TypeRef::Ground(source), TypeRef::Ground(success), TypeRef::Ground(error)) = (receiver.source_type, receiver.success_type, receiver.error_type) else {
            return Err(IrVerifyError::new("Result postfix receiver requires closed carrier domains"));
        };
        let source = store.semantic.to_type(source)?;
        let success = store.semantic.to_type(success)?;
        let error = store.semantic.to_type(error)?;
        if source != Type::Result(Box::new(success.clone()), Box::new(error)) || &success != expected {
            return Err(IrVerifyError::new("Result postfix receiver changes its original success domain"));
        }
        Self::verify_argument_initializer_lineage(store, generic, receiver.carrier, receiver.source_instruction, &receiver.source_wrappers, receiver.owner)?;
        Self::verify_generic_source(store, generic, receiver.source_instruction, receiver.owner, &source, None, active)?;
        Ok(success)
    }
}

#[cfg(test)]
mod tests;
