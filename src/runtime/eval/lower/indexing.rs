use super::*;

impl CompactLowerConstructProbe<'_, '_> {
    pub(super) fn record_original_index(&self, expression: ExprId, base: ExprId, index: ExprId, base_row: BuildExprId, index_row: BuildExprId, row: BuildExprId) -> Option<()> {
        let origin = self.expression_identity(expression);
        let Some(operation) = self.solved().operations.get(&origin) else { return Some(()); };
        let Some(selected) = self.solved().graph.candidate_evidence(operation.requirement).ok()? else { return Some(()); };
        let crate::sema::check::SolvedOperationAuthority::Language(metadata) = self.solved().operation_catalog.candidate(&self.solved().graph, selected.candidate).ok()? else { return Some(()); };
        let supported = match metadata.operation {
            crate::sema::operation_graph::PreparedLanguageOperation::Index { map } => Some((map, None)),
            crate::sema::operation_graph::PreparedLanguageOperation::ConstantKeyProjection { field } => Some((false, Some(field))),
            _ => None,
        };
        let Some((map, field)) = supported else { return Some(()); };
        if operation.receiver.is_some() || operation.actual_arguments.len() != 2 || operation.binding.supplied_slots != [0, 1]
            || !operation.binding.default_slots.is_empty() || operation.binding.rest_slot.is_some() || operation.binding.dynamic.is_some() {
            return None;
        }
        if operation.actual_arguments.iter().any(|&ty| super::super::indexed::generic::graph_ground_type(&self.solved().graph, ty).is_err())
            || super::super::indexed::generic::graph_ground_type(&self.solved().graph, operation.result).is_err() { return Some(()); }
        if !super::super::indexed::generic::GenericEvidenceStore::supports_ground_index_operation(map, field,
            &super::super::indexed::generic::graph_ground_type(&self.solved().graph, operation.actual_arguments[0]).ok()?,
            &super::super::indexed::generic::graph_ground_type(&self.solved().graph, operation.actual_arguments[1]).ok()?,
            &super::super::indexed::generic::graph_ground_type(&self.solved().graph, operation.result).ok()?) { return Some(()); }
        let postfix = self.scratch.borrow().result_receiver_origins.get(&base_row).cloned();
        let base = self.expression_identity(base);
        let index = self.expression_identity(index);
        for (identity, &ty) in [base, index].into_iter().zip(&operation.actual_arguments) {
            let actual = *self.solved().expressions.get(&identity)?;
            self.solved().graph.validate_scoped(crate::sema::inference::ScopedRoot {
                ty: actual, scope: self.solved().expression_scope(identity, operation.caller).ok()?,
            }).ok()?;
            self.solved().graph.validate_scoped(crate::sema::inference::ScopedRoot {
                ty, scope: self.solved().operation_scope(crate::sema::check::ProducerFlowSource::Expression(origin), operation).ok()?,
            }).ok()?;
            let actual = if identity == base && let Some(postfix) = &postfix {
                if postfix.origin != base || postfix.source_type.ty != actual { return None; }
                self.solved().graph.validate_scoped(postfix.success_type).ok()?;
                postfix.success_type.ty
            } else { actual };
            if super::super::indexed::generic::graph_ground_type(&self.solved().graph, actual).ok()?
                != super::super::indexed::generic::graph_ground_type(&self.solved().graph, ty).ok()? { return None; }
        }
        let uint_key = matches!(super::super::indexed::generic::graph_ground_type(&self.solved().graph, operation.actual_arguments[0]).ok()?, Type::Map(key, _) if *key == Type::UInt);
        let (index_material_row, uint_key_validation_row) = if uint_key {
            let scratch = self.scratch.borrow();
            let BuildExprRow::Try(validation) = scratch.expressions.get(index_row.index())? else { return None; };
            let BuildExprRow::Require { value, check, .. } = scratch.expressions.get(validation.index())? else { return None; };
            if check.ty != Type::UInt || check.schema.is_some() { return None; }
            (self.original_source_instruction(*value)?, Some(*validation))
        } else { (self.original_source_instruction(index_row)?, None) };
        let base_material_row = self.original_source_instruction(postfix.as_ref().map_or(base_row, |postfix| postfix.carrier))?;
        let original = super::super::indexed::full::BuildIndexOrigin { origin, base, index, base_row, base_material_row, postfix_base: postfix.is_some(), index_row, index_material_row, uint_key_validation_row,
            index_material_instruction: None, base_material_instruction: None, uint_key_validation: None };
        if self.scratch.borrow_mut().index_origins.insert(row, original).is_some() { return None; }
        Some(())
    }
}
