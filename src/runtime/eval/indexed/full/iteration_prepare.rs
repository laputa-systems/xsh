use super::*;
use super::super::generic::{OriginalIterationBinding, OriginalIterationUse, OperationSourceOrigin, PreparedOperationAuthority};
use crate::sema::inference::ScopedRoot;
use crate::sema::operation_graph::{IterableDomain, PreparedLanguageOperation};

fn problem(reason: &'static str) -> IrBuildError { IrBuildError::format(reason, None, 0, 0) }

impl FullBuilder {
    fn iteration_ground_root(&mut self, solved: &crate::sema::check::SolvedTypes, root: ScopedRoot) -> Result<TypeId, IrBuildError> {
        solved.graph.validate_scoped(root).map_err(|_| problem("iteration_original_root_scope"))?;
        let reference = self.generic.get_or_insert_with(GenericEvidenceBuilder::default)
            .prepare_closed_reference(&solved.graph, root.ty, &mut self.store.semantic, &mut self.semantic)
            .map_err(|_| problem("iteration_original_root_not_ground"))?;
        let TypeRef::Ground(ty) = reference else { return Err(problem("iteration_original_root_not_ground")); };
        Ok(ty)
    }

    pub(super) fn prepare_original_iteration_bindings(&mut self) -> Result<(), IrBuildError> {
        let Some(solved) = self.solved.clone() else { return Ok(()); };
        let mut bindings = BTreeMap::new();
        let origins: FxHashMap<_, _> = self.generic_expression_rows.iter().map(|&(instruction, origin, owner)| (instruction, (origin, owner))).collect();
        for (original, instruction, iterator, slot, body, owner) in self.iteration_binding_rows.clone() {
            let operation = solved.statement_operations.get(&original.statement).ok_or_else(|| problem("iteration_original_operation_missing"))?;
            let binding = solved.bindings.get(&original.binding).ok_or_else(|| problem("iteration_original_binding_missing"))?;
            let input = ScopedRoot { ty: *solved.expressions.get(&original.iterator_source).ok_or_else(|| problem("iteration_original_iterator_missing"))?, scope: solved.expression_scope(original.iterator_source, operation.caller).map_err(|_| problem("iteration_original_iterator_scope"))? };
            let item = ScopedRoot { ty: operation.result, scope: solved.operation_scope(crate::sema::check::ProducerFlowSource::Statement(original.statement), operation).map_err(|_| problem("iteration_original_item_scope"))? };
            let binding_type = ScopedRoot { ty: binding.ty, scope: binding.scheme.or(item.scope) };
            let iterator_parameter = super::super::super::BuildIterationBindingOrigin::original_parameter(&solved, original.iterator_source, operation.caller).ok_or_else(|| problem("iteration_original_parameter_port"))?;
            if original.caller != operation.caller || binding.owner != operation.caller || binding.mutable
                || original.input.ty != input.ty || original.input.scope != input.scope
                || original.item.ty != item.ty || original.item.scope != item.scope
                || original.binding_type.ty != binding_type.ty || original.binding_type.scope != binding_type.scope
                || original.iterator_parameter != iterator_parameter
                || original.slot != slot as usize || origins.get(&iterator) != Some(&(original.iterator_source, owner))
                || operation.receiver.is_some() || operation.actual_arguments.len() != 1 || operation.binding.supplied_slots != [0]
                || !operation.binding.default_slots.is_empty() || operation.binding.rest_slot.is_some() || operation.binding.dynamic.is_some()
                || solved.graph.resolved(operation.actual_arguments[0]).map_err(|_| problem("iteration_original_operand"))? != solved.graph.resolved(input.ty).map_err(|_| problem("iteration_original_operand"))? {
                return Err(problem("iteration_original_source_changed"));
            }
            match iterator_parameter {
                Some((_, index)) if self.store.tags.get(iterator as usize) == Some(&FullTag::ExprParam)
                    && self.store.payload(self.store.data[iterator as usize].range()).map_err(|_| problem("iteration_original_parameter_payload"))? == [index] => {},
                None if self.store.tags.get(iterator as usize) == Some(&FullTag::ExprList) => {},
                _ => return Err(problem("iteration_original_parameter_changed")),
            }
            let selected = solved.graph.candidate_evidence(operation.requirement).map_err(|_| problem("iteration_original_selected_owner"))?.ok_or_else(|| problem("iteration_original_selected_missing"))?;
            if selected.candidate != original.selected { return Err(problem("iteration_original_selected_changed")); }
            let crate::sema::check::SolvedOperationAuthority::Language(metadata) = solved.operation_catalog.candidate(&solved.graph, selected.candidate).map_err(|_| problem("iteration_original_authority"))? else { return Err(problem("iteration_original_authority_kind")); };
            if metadata.operation != (PreparedLanguageOperation::Iteration { domain: IterableDomain::List, outer_result: false }) { return Err(problem("iteration_item_protocol_not_prepared")); }
            let authority = PreparedOperationAuthority::Language { identity: metadata.identity, authority: metadata.authority, operation: metadata.operation, argument_order: metadata.argument_order, statement_result_is_unit: metadata.statement_result_is_unit };
            let input = self.iteration_ground_root(&solved, input)?;
            let item = self.iteration_ground_root(&solved, item)?;
            let binding_type = self.iteration_ground_root(&solved, binding_type)?;
            let id = self.generic_evidence_mut().add_iteration_binding(OriginalIterationBinding { statement: original.statement, binding: original.binding, iterator_origin: original.iterator_source, authority, instruction, iterator, slot, body, owner, input, item, binding_type, iterator_parameter }).map_err(|_| problem("iteration_original_binding_capacity"))?;
            if bindings.insert(original.binding, (id, owner)).is_some() { return Err(problem("iteration_original_binding_ambiguous")); }
        }
        for (origin, binding, instruction, owner) in self.iteration_use_rows.clone() {
            let &(binding, original_owner) = bindings.get(&binding).ok_or_else(|| problem("iteration_original_use_binding_missing"))?;
            if owner != original_owner || origins.get(&instruction) != Some(&(origin, owner)) { return Err(problem("iteration_original_use_owner_changed")); }
            self.generic_evidence_mut().add_iteration_use(OriginalIterationUse { origin, binding, instruction, owner }).map_err(|_| problem("iteration_original_use_capacity"))?;
        }
        Ok(())
    }
}

impl FullVerifier {
    pub(super) fn verify_original_iteration_bindings(store: &FullStore, tree: &super::super::pattern::PatternTree) -> Result<(), IrVerifyError> {
        let Some(generic) = store.generic.as_deref() else { return Ok(()); };
        let owners = store.generic_instruction_owners()?;
        let mut assigned = std::collections::BTreeSet::new();
        for (instruction, owner) in owners.iter().copied().enumerate() {
            if !matches!(store.tags[instruction], FullTag::StmtAssign | FullTag::StmtAssignField | FullTag::StmtAssignFieldInt | FullTag::StmtAssignPath | FullTag::StmtAssignInt | FullTag::StmtAssignBool) { continue; }
            if let (Some(owner), Some(&slot)) = (owner, store.payload(store.data[instruction].range())?.first()) {
                let owner_key = match owner { InstructionOwner::Function(function) => (false, function.raw()), InstructionOwner::Driver(step) => (true, step) };
                assigned.insert((owner_key, slot));
            }
        }
        let mut body_roots = FxHashMap::default();
        for (id, _) in generic.iteration_bindings() {
            let binding = generic.iteration_binding(id)?;
            if store.tags.get(binding.instruction as usize) != Some(&FullTag::StmtFor) { return Err(IrVerifyError::new("iteration original statement changes its operation")); }
            let words = store.payload(store.data[binding.instruction as usize].range())?;
            if words.len() != 4 || words[0] != binding.slot || words[1] != binding.iterator || words[2] != binding.body.raw() { return Err(IrVerifyError::new("iteration original statement changes its operand, slot, or body")); }
            let block = store.blocks.get(binding.body.index()).ok_or_else(|| IrVerifyError::new("iteration body is missing"))?;
            if block.flags != BLOCK_STATEMENTS { return Err(IrVerifyError::new("iteration body has another structural kind")); }
            let roots = store.payload(block.instructions)?.get(1..).ok_or_else(|| IrVerifyError::new("iteration body roots are missing"))?;
            for &root in roots {
                if body_roots.insert(root, id).is_some() { return Err(IrVerifyError::new("iteration bodies share a structural root")); }
            }
            let owner_key = match binding.owner { InstructionOwner::Function(function) => (false, function.raw()), InstructionOwner::Driver(step) => (true, step) };
            if assigned.contains(&(owner_key, binding.slot)) { return Err(IrVerifyError::new("iteration item is mutable without an assignment proof")); }
            match binding.iterator_parameter {
                Some((_, index)) if store.tags.get(binding.iterator as usize) == Some(&FullTag::ExprParam)
                    && store.payload(store.data[binding.iterator as usize].range())? == [index] => {},
                None if store.tags.get(binding.iterator as usize) == Some(&FullTag::ExprList) => {},
                _ => return Err(IrVerifyError::new("iteration iterator changes its original parameter source")),
            }
            Self::verify_generic_source(store, generic, binding.iterator, binding.owner, &store.semantic.to_type(binding.input)?, None, &mut Vec::new())?;
        }
        for use_ in generic.iteration_uses() {
            let binding = generic.iteration_binding(use_.binding)?;
            if generic.iteration_use(use_.instruction)?.is_none() || store.tags.get(use_.instruction as usize) != Some(&FullTag::ExprParam)
                || store.payload(store.data[use_.instruction as usize].range())? != [binding.slot] {
                return Err(IrVerifyError::new("iteration item read changes its original slot"));
            }
            let mut visible = false;
            let mut ancestor = Some(use_.instruction);
            for _ in 0..=512 {
                let Some(instruction) = ancestor else { break; };
                if body_roots.get(&instruction) == Some(&use_.binding) { visible = true; break; }
                ancestor = tree.parent(instruction)?;
            }
            if !visible || tree.is_descendant(binding.iterator, use_.instruction)? { return Err(IrVerifyError::new("iteration item read is outside its original loop body")); }
        }
        Ok(())
    }

    pub(super) fn verify_iteration_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, expected: &Type) -> Result<bool, IrVerifyError> {
        let Some(use_) = generic.iteration_use(instruction)? else { return Ok(false); };
        let binding = generic.iteration_binding(use_.binding)?;
        if use_.owner != owner || binding.owner != owner || store.tags.get(instruction as usize) != Some(&FullTag::ExprParam)
            || store.payload(store.data[instruction as usize].range())? != [binding.slot]
            || generic.registered_instruction_origin(instruction, false) != Some((OperationSourceOrigin::Expression(use_.origin), owner))
            || store.semantic.to_type(binding.binding_type)? != *expected { return Err(IrVerifyError::new("iteration operand changes its original item contract")); }
        Ok(true)
    }
}

#[cfg(test)]
mod tests;
