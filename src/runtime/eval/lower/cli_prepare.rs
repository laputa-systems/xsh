use super::*;

impl CompactLowerConstructProbe<'_, '_> {
    pub(super) fn original_cli_static_plan(&self, id: ExprId) -> Option<(crate::sema::registry_graph::RegistryCandidate,
        crate::sema::check::SolvedOperation, crate::sema::inference::Arrow, Arc<crate::modules::cli::CliDescriptorPlan>)> {
        use crate::modules::signature::{ImplBinding, SemanticRule};
        use crate::sema::registry_graph::RegistryOwner;
        let solved = self.solved();
        let origin = self.expression_identity(id);
        let operation = solved.operations.get(&origin)?;
        let selected = solved.graph.candidate_evidence(operation.requirement).ok()??;
        let crate::sema::check::SolvedOperationAuthority::Registry(metadata) = solved.operation_catalog.candidate(&solved.graph, selected.candidate).ok()? else { return None; };
        let boundary = solved.registry_boundaries.get(&origin)?;
        let plan = boundary.shared_descriptor()?;
        if metadata.binding != ImplBinding::Native || !matches!(metadata.semantic_rule, SemanticRule::CliDescriptor | SemanticRule::CliCommands)
            || !matches!(metadata.owner, RegistryOwner::Module(_)) || operation.receiver.is_some()
            || operation.binding.rest_slot.is_some() || operation.binding.dynamic.is_some() || !operation.argument_coercions.is_empty()
            || boundary.requirement != Some(operation.requirement) || boundary.caller != operation.caller
            || boundary.input != operation.result || boundary.descriptor_operation() != Some(metadata.operation)
            || !plan.matches_operation(metadata.operation) { return None; }
        let signature = solved.graph.resolved(selected.signature).ok()?;
        let crate::sema::inference::TypeNode::Arrow(arrow) = solved.graph.node(signature).ok()? else { return None; };
        if arrow.kind != metadata.kind || arrow.params.len() != metadata.parameters.len()
            || arrow.params.iter().zip(&metadata.parameters).any(|(formal, original)| formal.label != original.label
                || formal.defaulted != original.defaulted || formal.rest) { return None; }
        Some((metadata.clone(), operation.clone(), arrow.clone(), Arc::clone(plan)))
    }

    // Authored argument recipes preserve the spread's source allocation and
    // field membership without assigning identities to generated projections.
    pub(super) fn lower_original_cli_named_call(&mut self, id: ExprId, slots: &mut SlotScope,
        current_function: Option<Name>, item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let (metadata, operation, arrow, plan) = self.original_cli_static_plan(id)?;
        let sources = self.solved().argument_sources.get(&self.expression_identity(id)).cloned()?;
        if sources.len() != operation.binding.supplied_slots.len() { return None; }
        let span = self.program.arena.expr(id).span;
        let lowered = self.lower_source_argument_values(sources.iter().map(|source| (source.entry_index, source.value, source.span)),
            slots, current_function, item_slot)?;
        self.record_original_argument_bindings(id, &sources, &lowered)?;
        let mut arguments = vec![None; arrow.params.len()];
        for ((source, value), &slot) in sources.iter().zip(lowered.values).zip(&operation.binding.supplied_slots) {
            let parameter = arrow.params.get(slot)?;
            if source.name.is_some_and(|name| name != parameter.label)
                || !matches!(source.value, crate::sema::arguments::ArgumentValueSource::Expression(_)
                    | crate::sema::arguments::ArgumentValueSource::RecordField { .. }) || arguments.get(slot)?.is_some() { return None; }
            let ty = self.solved_type(parameter.ty)?;
            arguments[slot] = Some(self.checked_unsigned_value(value, &ty, source.span));
        }
        for &slot in &operation.binding.default_slots {
            if !arrow.params.get(slot)?.defaulted || arguments.get(slot)?.is_some() { return None; }
        }
        if arguments.iter().enumerate().any(|(slot, value)| value.is_none() != operation.binding.default_slots.contains(&slot)) { return None; }
        let value = push_build_row!(self, expr, BuildExprRow::ModuleCall {
            cli_plan: Some(plan), op: metadata.operation, args: arguments, span,
        });
        Some(self.wrap_argument_bindings(value, lowered.bindings, span))
    }
}
