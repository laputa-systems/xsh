use super::*;

impl CompactLowerConstructProbe<'_, '_> {
    pub(super) fn checked_script_call(&self, id: ExprId, receiver: bool) -> Option<crate::modules::signature::ScriptImpl> {
        let solved = self.solved();
        let operation = solved.operations.get(&self.expression_identity(id))?;
        if operation.receiver.is_some() != receiver { return None; }
        let selected = solved.graph.candidate_evidence(operation.requirement).ok()??;
        let crate::sema::check::SolvedOperationAuthority::Registry(metadata) = solved.operation_catalog.candidate(&solved.graph, selected.candidate).ok()? else { return None; };
        match metadata.binding {
            crate::modules::signature::ImplBinding::Script(script) => Some(script),
            crate::modules::signature::ImplBinding::Native => None,
        }
    }

    pub(super) fn checked_call_argument_binding(&self, id: ExprId) -> Option<crate::sema::check::CallBinding> {
        let solved = self.solved();
        let identity = self.expression_identity(id);
        if let Some(call) = solved.calls.get(&identity) { return Some(call.binding.clone()); }
        if let Some(operation) = solved.operations.get(&identity) {
            solved.graph.candidate_evidence(operation.requirement).ok()??;
            return Some(operation.binding.clone());
        }
        let invocation = solved.invocations.get(&identity)?;
        let evidence = solved.graph.invocation_evidence(invocation.requirement).ok()??;
        let (_, binding, _) = evidence.unique_plan()?;
        Some(crate::sema::check::CallBinding {
            supplied_slots: binding.supplied_slots.clone(), default_slots: binding.default_slots.clone(),
            rest_slot: binding.rest_slot, dynamic: binding.dynamic.clone(),
        })
    }

    pub(super) fn checked_residual_invocation(&self, id: ExprId) -> Option<()> {
        let solved = self.solved();
        let identity = self.expression_identity(id);
        let invocation = solved.invocations.get(&identity)?;
        if solved.graph.invocation_evidence(invocation.requirement).ok()?.is_some() { return None; }
        let caller = invocation.caller?;
        if solved.expression_owners.get(&identity) != Some(&caller) { return None; }
        let declaration = solved.declarations.get(&caller)?;
        let scheme = solved.graph.scheme(declaration.scheme).ok()?;
        if !scheme.requirement_origins.contains(&invocation.requirement) { return None; }
        let crate::sema::inference::RequirementTemplate::CallableInvocation { call } = solved.graph.requirement_template(invocation.requirement).ok()? else { return None; };
        if solved.graph.invocation_call(call).ok()?.domain != crate::sema::inference::CallableDomain::Pure { return None; }
        Some(())
    }

    // Saved supplied values are indexed by the original checked expansion.
    // Parameter order only controls placement; it cannot repeat evaluation or
    // move lexical default expressions into the caller's environment.
    pub(super) fn lower_function_call_args(
        &mut self, id: ExprId, default_offset: usize, slots: &mut SlotScope,
        current_function: Option<Name>, item_slot: Option<usize>,
    ) -> Option<Vec<LoweredCallArg>> {
        let identity = self.expression_identity(id);
        let Some(sources) = self.solved().argument_sources.get(&identity).cloned() else {
            self.last_blocker_detail = Some((self.program.arena.expr(id).span, "call has no original checked argument recipe".into()));
            return None;
        };
        let binding = self.checked_call_argument_binding(id);
        let residual = binding.is_none() && self.checked_residual_invocation(id).is_some();
        if binding.is_none() && !residual {
            let all = self.solved().invocations.get(&identity).and_then(|invocation| {
                self.solved().graph.invocation_evidence(invocation.requirement).ok().flatten()
            }).is_some_and(|evidence| matches!(evidence.plan, crate::sema::inference::InvocationPlan::All { .. }));
            self.last_blocker_detail = Some((self.program.arena.expr(id).span, if all {
                "conditional call needs distinct runtime argument plans"
            } else { "call has no checked argument binding" }.into()));
            return None;
        }
        let values = if let Some(values) = slots.bound_argument_values.get(&id) {
            if values.len() != sources.len() { return None; }
            values.clone()
        } else {
            sources.iter().map(|source| match source.value {
                crate::sema::arguments::ArgumentValueSource::Expression(expression)
                | crate::sema::arguments::ArgumentValueSource::PositionalSplice(expression) => {
                    self.lower_expr(expression, slots, current_function, item_slot)
                }
                crate::sema::arguments::ArgumentValueSource::RecordField { .. } => None,
            }).collect::<Option<Vec<_>>>()?
        };
        let argument = |entry: usize| -> Option<LoweredCallArg> {
            let value = *values.get(entry)?;
            Some(if matches!(sources.get(entry)?.value, crate::sema::arguments::ArgumentValueSource::PositionalSplice(_)) {
                LoweredCallArg::Splice(value)
            } else { LoweredCallArg::Single(value) })
        };
        // A residual callback has no selected formal slots. Its instance proof
        // supplies those destinations after the authored operands are evaluated.
        if residual { return (0..sources.len()).map(argument).collect(); }
        let binding = binding?;
        if let Some(dynamic) = &binding.dynamic {
            if sources.iter().any(|source| source.name.is_some()) || !binding.default_slots.is_empty() {
                self.last_blocker_detail = Some((self.program.arena.expr(id).span,
                    "dynamic call argument destinations need an indexed runtime plan".into()));
                return None;
            }
            if dynamic.segments.len() != sources.len() { return None; }
            for (entry, segment) in dynamic.segments.iter().enumerate() {
                let actual = match segment {
                    crate::sema::inference::InvocationArgumentSegment::StaticSlot { argument, .. }
                    | crate::sema::inference::InvocationArgumentSegment::DynamicRange { argument, .. } => *argument,
                };
                if actual != entry { return None; }
            }
            return (0..sources.len()).map(argument).collect();
        }
        if binding.supplied_slots.len() != sources.len() { return None; }
        let last_supplied = binding.supplied_slots.iter().copied().max();
        let mut destinations = binding.default_slots.iter().copied().filter(|slot| last_supplied.is_some_and(|last| *slot <= last))
            .map(|slot| (slot, LoweredCallArg::Default(slot + default_offset))).collect::<Vec<_>>();
        for (entry, slot) in binding.supplied_slots.iter().copied().enumerate() {
            destinations.push((slot, argument(entry)?));
        }
        destinations.sort_by_key(|(slot, _)| *slot);
        Some(destinations.into_iter().map(|(_, value)| value).collect())
    }
}
