use super::*;
use super::super::generic::{PreparedBridgeCall, graph_ground_type};
use crate::sema::inference::{ScopedRoot, TypeNode};

fn problem(reason: &'static str) -> IrBuildError { IrBuildError::format(reason, None, 0, 0) }

fn bridge_allocation(store: &FullStore, instruction: u32) -> Result<(RuntimeOp, u32, Box<[u32]>, (u32, Box<[u32]>)), IrVerifyError> {
    if store.tags.get(instruction as usize) != Some(&FullTag::ExprModuleCall) { return Err(IrVerifyError::new("native bridge receipt has another opcode")); }
    let payload = store.payload(store.data[instruction as usize].range())?;
    if payload.len() != 4 || payload[1] != 0 { return Err(IrVerifyError::new("native bridge descriptor protocol changed")); }
    let operation = *store.runtime_ops.get(payload[0] as usize).ok_or_else(|| IrVerifyError::new("native bridge operation is invalid"))?;
    let block = IrBlockId::from_raw(payload[2]).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("native bridge operand block is missing"))?;
    if block.flags & BLOCK_SEQUENCE_KIND_MASK != BLOCK_LIST { return Err(IrVerifyError::new("native bridge operand block changes kind")); }
    let operands = store.payload(block.instructions)?;
    if operands.len() != 3 || operands[..2] != [1, 1] { return Err(IrVerifyError::new("native bridge requires its one original supplied operand")); }
    Ok((operation, operands[2], payload.into(), (payload[2], operands.into())))
}

impl FullBuilder {
    pub(super) fn prepare_embedded_bridges(&mut self) -> Result<(), IrBuildError> {
        let Some(solved) = self.solved.clone() else { return Ok(()); };
        solved.validate_embedded_bridges().map_err(|_| problem("native_bridge_original_authority_changed"))?;
        for (instruction, origin, owner) in self.generic_expression_rows.clone() {
            let Some(original) = solved.embedded_bridge_call(origin).cloned() else { continue; };
            let selected = original.declaration();
            let caller = original.call().caller.ok_or_else(|| problem("native_bridge_original_caller_missing"))?;
            let InstructionOwner::Function(function) = owner else { return Err(problem("native_bridge_original_caller_changed")); };
            if self.declaration_functions.get(&caller) != Some(&function) { return Err(problem("native_bridge_original_caller_changed")); }
            let (operation, operand, payload, argument_block) = bridge_allocation(&self.store, instruction).map_err(|_| problem("native_bridge_allocation_changed"))?;
            if operation != selected.op() { return Err(problem("native_bridge_operation_changed")); }
            let scope = solved.expression_scope(origin, Some(caller)).map_err(|_| problem("native_bridge_original_scope"))?;
            for ty in [original.call().signature, original.call().actual_arguments[0], original.result()] {
                solved.graph.validate_scoped(ScopedRoot { ty, scope }).map_err(|_| problem("native_bridge_original_type_scope"))?;
            }
            let TypeNode::Arrow(arrow) = solved.graph.node(solved.graph.resolved(original.call().signature).map_err(|_| problem("native_bridge_call_signature"))?).map_err(|_| problem("native_bridge_call_signature"))? else { return Err(problem("native_bridge_call_signature")); };
            if arrow.kind != crate::sema::inference::CallableKind::Pure || arrow.params.len() != 1 || arrow.params[0].label != "value" || arrow.params[0].defaulted || arrow.params[0].rest
                || solved.graph.closed_effect_summary(arrow.effects).map_err(|_| problem("native_bridge_call_effects"))? != crate::sema::inference::EffectSummary::Closed(crate::sema::inference::EffectSet::EMPTY)
                || graph_ground_type(&solved.graph, arrow.params[0].ty).map_err(|_| problem("native_bridge_formal_type"))? != Type::Any
                || graph_ground_type(&solved.graph, arrow.result).map_err(|_| problem("native_bridge_result_type"))? != Type::Str
                || graph_ground_type(&solved.graph, original.result()).map_err(|_| problem("native_bridge_original_result_type"))? != Type::Str { return Err(problem("native_bridge_call_contract_changed")); }
            self.original_argument_expression(operand, origin, 0, &original.recipes()[0], owner)?;
            let actual_type = graph_ground_type(&solved.graph, original.call().actual_arguments[0]).map_err(|_| problem("native_bridge_original_actual_type"))?;
            let actual = self.intern_generic_ground_type(&actual_type)?;
            let formal = self.intern_generic_ground_type(&Type::Any)?;
            let result = self.intern_generic_ground_type(&Type::Str)?;
            let descriptor = self.intern_checked_callable_type(&solved.graph, selected.signature())?;
            let (_, signature) = self.store.semantic.callable_descriptor(descriptor).map_err(|_| problem("native_bridge_signature_descriptor"))?.ok_or_else(|| problem("native_bridge_signature_descriptor"))?;
            let caller_signature = SignatureId::from_raw(self.store.functions[function.index()].signature).ok_or_else(|| problem("native_bridge_caller_signature"))?;
            self.generic_evidence_mut().add_bridge_call(PreparedBridgeCall { original, instruction, owner, caller_signature, signature, formal, actual, result, operand, payload, argument_block }).map_err(|_| problem("native_bridge_receipt_allocation"))?;
        }
        Ok(())
    }
}

impl FullVerifier {
    pub(super) fn bridge_result(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner) -> Result<Type, IrVerifyError> {
        let source = generic.bridge_call(instruction)?.ok_or_else(|| IrVerifyError::new("native bridge is missing its original invocation receipt"))?;
        let (operation, operand, payload, argument_block) = bridge_allocation(store, instruction)?;
        if source.owner != owner || operation != source.original.declaration().op() || operand != source.operand || payload != source.payload || argument_block != source.argument_block { return Err(IrVerifyError::new("native bridge allocation changed its original operation or operand")); }
        Self::verify_generic_source(store, generic, operand, owner, &store.semantic.to_type(source.actual)?, None, &mut Vec::new())?;
        Ok(store.semantic.to_type(source.result)?)
    }
    pub(super) fn verify_embedded_bridges(store: &FullStore, generic: &GenericEvidenceStore) -> Result<(), IrVerifyError> {
        for source in generic.bridge_calls() { Self::bridge_result(store, generic, source.instruction, source.owner)?; }
        for (instruction, tag) in store.tags.iter().enumerate() {
            if *tag != FullTag::ExprModuleCall { continue; }
            let payload = store.payload(store.data[instruction].range())?;
            if payload.first().and_then(|&operation| store.runtime_ops.get(operation as usize)).is_some_and(|&operation| crate::stdlib::is_private_bridge_op(operation))
                && generic.bridge_call(instruction as u32)?.is_none() { return Err(IrVerifyError::new("private native bridge lacks its original invocation receipt")); }
        }
        Ok(())
    }
    pub(super) fn verify_bridge_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, expected: &Type) -> Result<bool, IrVerifyError> {
        if generic.bridge_call(instruction)?.is_none() { return Ok(false); }
        if Self::bridge_result(store, generic, instruction, owner)? != *expected { return Err(IrVerifyError::new("native bridge result differs from its consumer")); }
        Ok(true)
    }
}

#[cfg(test)]
mod tests;
