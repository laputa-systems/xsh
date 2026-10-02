use super::*;
use crate::runtime::eval::callable_value::{RuntimeCallableCapture, RuntimeCallableValue};
use crate::runtime::eval::indexed::generic::{InvocationPlanId, UserInvocationAuthority};

impl Evaluator {
    pub(super) fn create_indexed_callable(&self, execution: &FullExecution<'_>, instruction: u32, span: Span) -> Result<Option<LoweredValue>, RuntimeError> {
        let Some(id) = execution.callable_value(instruction).map_err(|error| indexed_error(error, span))? else { return Ok(None); };
        let program = Arc::clone(self.indexed_program.as_ref().ok_or_else(|| RuntimeError::new("indexed-ir", "callable creation has no installed program").with_span(span))?);
        let evidence = program.generic_evidence().ok_or_else(|| RuntimeError::new("indexed-ir", "callable creation lacks prepared authority").with_span(span))?;
        let contract = evidence.callable_value(id).map_err(|error| indexed_error(error, span))?.contract;
        let header = program.function_view_by_id(contract.target).map_err(|error| indexed_error(error, span))?.header().map_err(|error| indexed_error(error, span))?;
        let mut captures = Vec::with_capacity(header.captures.len());
        for capture in &header.captures {
            let owner = contract.declaration.namespace;
            let binding = if let Some(bindings) = owner.and_then(|owner| self.indexed_module_bindings.get(&owner))
                && capture.name != Name::intern("args") {
                bindings.get(&capture.name)
            } else { self.lookup(capture.name) };
            let binding = binding.ok_or_else(|| RuntimeError::new("unknown-name", "callable creation lost its original capture binding").with_span(span))?;
            captures.push(RuntimeCallableCapture { slot: capture.slot, value: binding.value.clone() });
        }
        RuntimeCallableValue::new(program, id, captures).map(LoweredValue::Callable).map(Some).map_err(|error| error.with_span(span))
    }

    pub(super) fn checked_indexed_callable(&self, execution: &FullExecution<'_>, plan: InvocationPlanId, callee: &LoweredValue, span: Span) -> Result<RuntimeCallableValue, RuntimeError> {
        self.checked_indexed_callable_authority(execution, UserInvocationAuthority::Ground(plan), callee, span)
    }

    pub(super) fn checked_indexed_callable_authority(&self, execution: &FullExecution<'_>, authority: UserInvocationAuthority, callee: &LoweredValue, span: Span) -> Result<RuntimeCallableValue, RuntimeError> {
        let LoweredValue::Callable(handle) = callee else { return Err(RuntimeError::new("indexed-ir", "prepared user invocation requires its original typed callable value").with_span(span)); };
        let program = self.indexed_program.as_ref().ok_or_else(|| RuntimeError::new("indexed-ir", "prepared user invocation has no installed program").with_span(span))?;
        if !Arc::ptr_eq(program, handle.program()) { return Err(RuntimeError::new("indexed-ir", "prepared invocation and callable belong to different programs").with_span(span)); }
        let evidence = execution.generic_evidence().ok_or_else(|| RuntimeError::new("indexed-ir", "prepared user invocation lacks evidence").with_span(span))?;
        match authority {
            UserInvocationAuthority::Ground(plan) => { evidence.validate_user_invocation(handle.id(), plan).map_err(|error| indexed_error(error, span))?; }
            UserInvocationAuthority::Scoped { source, instance, witness } => {
                let source = evidence.scoped_invocation_source(source).map_err(|error| indexed_error(error, span))?;
                if execution.active_instantiation() != Some(instance) || execution.generic_scope() != Some(source.scope)
                    || evidence.scoped_invocation_authority(source.instruction, instance).map_err(|error| indexed_error(error, span))? != Some(authority) {
                    return Err(RuntimeError::new("indexed-ir", "prepared callback uses another frame's invocation authority").with_span(span));
                }
                let witness = evidence.scoped_invocation_witness(witness).map_err(|error| indexed_error(error, span))?;
                let actual = handle.contract();
                if actual.signature != witness.signature || actual.kind != witness.kind || actual.creation != witness.effects {
                    return Err(RuntimeError::new("indexed-ir", "prepared callback differs from its contextual signature or effects").with_span(span));
                }
            }
        }
        Ok(handle.clone())
    }

    pub(super) fn prepared_indexed_callable_arguments(authority: UserInvocationAuthority, handle: &RuntimeCallableValue, arguments: IndexedCallArguments, span: Span) -> Result<IndexedCallArguments, RuntimeError> {
        let UserInvocationAuthority::Scoped { witness, .. } = authority else { return Ok(arguments); };
        let evidence = handle.program().generic_evidence().ok_or_else(|| RuntimeError::new("indexed-ir", "prepared callback arguments have no evidence").with_span(span))?;
        let witness = evidence.scoped_invocation_witness(witness).map_err(|error| indexed_error(error, span))?;
        if !arguments.omitted_parameters.is_empty() || witness.binding.dynamic.is_some() || witness.binding.rest_slot.is_some()
            || witness.binding.supplied_slots.len() != arguments.values.len() {
            return Err(RuntimeError::new("indexed-ir", "prepared callback operands have another supplied shape").with_span(span));
        }
        let header = handle.program().function_view_by_id(handle.contract().target).map_err(|error| indexed_error(error, span))?.header().map_err(|error| indexed_error(error, span))?;
        let mut destinations = vec![None; header.params.len()];
        let mut defaulted = vec![false; header.params.len()];
        for (value, &slot) in arguments.values.into_iter().zip(&witness.binding.supplied_slots) {
            let target = destinations.get_mut(slot as usize).ok_or_else(|| RuntimeError::new("indexed-ir", "prepared callback supplied slot is invalid").with_span(span))?;
            if target.replace(value).is_some() { return Err(RuntimeError::new("indexed-ir", "prepared callback supplies a parameter twice").with_span(span)); }
        }
        for &slot in &witness.binding.default_slots {
            let target = defaulted.get_mut(slot as usize).ok_or_else(|| RuntimeError::new("indexed-ir", "prepared callback default slot is invalid").with_span(span))?;
            if *target || destinations[slot as usize].is_some() { return Err(RuntimeError::new("indexed-ir", "prepared callback default overlaps a supplied parameter").with_span(span)); }
            *target = true;
        }
        let mut ordered = IndexedCallArguments::default();
        for (slot, value) in destinations.into_iter().enumerate() {
            if let Some(value) = value { ordered.values.push(value); }
            else if defaulted[slot] { ordered.omit(slot, span)?; }
            else { return Err(RuntimeError::new("indexed-ir", "prepared callback omits a required parameter").with_span(span)); }
        }
        Ok(ordered)
    }

    pub(super) fn hydrate_indexed_callable_environment(captures: &[RuntimeCallableCapture], slots: &mut [LoweredValue], span: Span) -> Result<(), RuntimeError> {
        for capture in captures {
            let value = lowered_value_from_runtime_any(&capture.value).ok_or_else(|| RuntimeError::new("indexed-ir", "prepared callable capture cannot cross the value boundary").with_span(span))?;
            let slot = slots.get_mut(capture.slot).ok_or_else(|| RuntimeError::new("indexed-ir", "prepared callable capture slot is out of bounds").with_span(span))?;
            *slot = value;
        }
        Ok(())
    }

    pub(super) fn eval_indexed_prepared_callable(&mut self, handle: RuntimeCallableValue, arguments: IndexedCallArguments, span: Span) -> Result<LoweredValue, RuntimeError> {
        let program = Arc::clone(handle.program());
        let view = program.function_view_by_id(handle.contract().target).map_err(|error| indexed_error(error, span))?;
        let header = view.header().map_err(|error| indexed_error(error, span))?;
        let bound = self.bind_indexed_call_arguments(&header, arguments, span)?;
        if self.indexed_frames_supported(view, span)? && !super::super::indexed_recursive_fast_path_allowed(header.return_kind) {
            return super::super::with_indexed_explicit_frames(|| self.eval_indexed_with_callable_slots(program.as_ref(), &handle, bound, span));
        }
        let (function, kind) = view.execution().map_err(|error| indexed_error(error, span))?.function_identity().map_err(|error| indexed_error(error, span))?;
        let IndexedCallSlots { slots: mut slots, pending_defaults } = bound;
        let result = self.eval_indexed_call_frame_with_environment(function, kind, view, &header, &mut slots, pending_defaults, span, Some(handle.captures()))
            .and_then(|value| super::super::checked_lowered_return_value(&header, value, span));
        self.recycle_lowered_slots(slots);
        result
    }
}
