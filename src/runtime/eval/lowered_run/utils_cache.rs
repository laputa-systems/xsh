use super::*;
use crate::runtime::eval::callable_value::{RuntimeCallableCapture, RuntimeCallableValue};
use crate::runtime::eval::indexed::generic::OperationSourceOrigin;

/// Entries retain their original callable and capture environment. Repeated
/// references to one declaration share a result only when their captures refer
/// to the same immutable values or mutable allocations.
#[derive(Default)]
pub(in crate::runtime::eval) struct PreparedUtilsCache {
    entries: FxHashMap<String, Vec<PreparedUtilsCacheEntry>>,
}

struct PreparedUtilsCacheEntry {
    handle: RuntimeCallableValue,
    value: Value,
}

fn same_capture(left: &RuntimeCallableCapture, right: &RuntimeCallableCapture) -> bool {
    if left.slot != right.slot { return false; }
    match (&left.live_cell, &right.live_cell) {
        (Some(left), Some(right)) => return left.same_allocation(right),
        (None, None) => {},
        _ => return false,
    }
    match (&left.host_binding, &right.host_binding) {
        (Some(left), Some(right)) if left.capture_id() != right.capture_id() => return false,
        (Some(_), Some(_)) | (None, None) => {},
        _ => return false,
    }
    left.captured_value() == right.captured_value()
}

fn same_target(left: &RuntimeCallableValue, right: &RuntimeCallableValue) -> bool {
    Arc::ptr_eq(left.program(), right.program()) && left.contract() == right.contract()
        && left.captures().len() == right.captures().len()
        && left.captures().iter().zip(right.captures()).all(|(left, right)| same_capture(left, right))
}

impl Evaluator {
    fn validate_utils_cache_callable(&self, handle: &RuntimeCallableValue, span: Span) -> Result<(), RuntimeError> {
        let invalid = |message: &str| RuntimeError::new("indexed-ir", message).with_span(span);
        let program = self.indexed_program.as_ref().ok_or_else(|| invalid("utils.cache callable has no installed prepared program"))?;
        if !Arc::ptr_eq(program, handle.program()) { return Err(invalid("utils.cache callable belongs to another prepared program")); }
        let evidence = program.generic_evidence().ok_or_else(|| invalid("utils.cache callable has no original prepared evidence"))?;
        let proof = evidence.callable_value(handle.id()).map_err(|error| invalid(&error.message))?;
        let source = evidence.callable_source(proof.source).map_err(|error| invalid(&error.message))?;
        if proof.contract != handle.contract() || source.expected != handle.contract()
            || evidence.callable_value_at(source.instruction).map_err(|error| invalid(&error.message))? != Some(handle.id())
            || evidence.registered_instruction_origin(source.instruction, false) != Some((OperationSourceOrigin::Expression(source.origin), source.owner)) {
            return Err(invalid("utils.cache callable differs from its original creation receipt"));
        }
        RuntimeCallableValue::new(program.clone(), handle.id(), handle.captures().to_vec())
            .map_err(|error| invalid(&error.message))?;
        Ok(())
    }

    pub(super) fn eval_prepared_utils_cache(&mut self, handle: RuntimeCallableValue, arguments: Vec<LoweredValue>, span: Span) -> Result<LoweredValue, RuntimeError> {
        let program = Arc::clone(handle.program());
        let _symbols = program.symbol_owner().enter();
        self.validate_utils_cache_callable(&handle, span)?;
        let header = handle.program().function_view_by_id(handle.contract().target)
            .and_then(|view| view.header()).map_err(|error| RuntimeError::new("indexed-ir", error.message).with_span(span))?;
        // Cache hits must still satisfy the original parameter contract. Binding
        // leaves omitted default recipes pending, so hits do not run their effects.
        let bound = self.bind_indexed_call_arguments(&header, IndexedCallArguments::supplied(arguments.clone()), span)?;
        self.recycle_lowered_slots(bound.slots);
        let key_arguments = arguments.iter().cloned().map(LoweredValue::into_value).collect::<Vec<_>>();
        let key = utils_cache_key("", &key_arguments).map_err(|bad_type| RuntimeError::new("cache-key-error",
            format!("args contains a {bad_type}, which cannot be used as a cache key")).with_span(span))?;
        if let Some(entry) = self.prepared_utils_cache.entries.get(&key).and_then(|entries| entries.iter().find(|entry| same_target(&entry.handle, &handle))) {
            return lowered_value_from_runtime_any(&entry.value).ok_or_else(|| RuntimeError::new("type-error",
                format!("utils.cache returned unsupported {}", entry.value.type_name())).with_span(span));
        }
        let value = self.eval_indexed_prepared_callable(handle.clone(), IndexedCallArguments::supplied(arguments), span)?;
        self.prepared_utils_cache.entries.entry(key).or_default().push(PreparedUtilsCacheEntry { handle, value: value.clone().into_value() });
        Ok(value)
    }
}

#[cfg(test)]
mod tests;
