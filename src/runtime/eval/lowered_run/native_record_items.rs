use super::*;
use crate::runtime::eval::indexed::generic::PreparedNativeResultRecord;
use crate::runtime::value::{ScriptStream, ScriptStreamState, ScriptStreamStep};

struct NativeRecordItems {
    source: Option<StreamValue>,
    pending: std::vec::IntoIter<crate::runtime::value::StreamItem>,
    layout: PreparedNativeResultRecord,
}

/// Wrapping does not pull the native source. Each reached item crosses its
/// original selected record boundary before a downstream consumer sees it.
pub(in crate::runtime::eval) fn wrap_native_record_items_stream(mut source: StreamValue, layout: PreparedNativeResultRecord) -> LoweredValue {
    let pending = std::mem::take(&mut source.items).into_iter();
    LoweredValue::Stream(Box::new(StreamValue::from_script(ScriptStreamState::new(NativeRecordItems { source: Some(source), pending, layout }))))
}

impl ScriptStream for NativeRecordItems {
    fn finished(&self) -> bool { self.source.is_none() }

    fn poll(&mut self, evaluator: &mut Evaluator, span: Span) -> Result<ScriptStreamStep, RuntimeError> {
        let value = if let Some(item) = self.pending.next() { item.value } else {
            let Some(source) = self.source.as_mut() else { return Ok(ScriptStreamStep::Finished); };
            let Some(value) = evaluator.stream_next(source, span)? else { self.source = None; return Ok(ScriptStreamStep::Finished); };
            value
        };
        let value = lowered_value_from_runtime_any(&value).ok_or_else(|| RuntimeError::new("indexed-ir", "native record stream returned an unsupported item").with_span(span))?;
        self.layout.materialize_item(evaluator, value, span).map(|value| ScriptStreamStep::Yielded(value.into_value()))
    }

    fn validate_item(&self, _value: &Value, _span: Span) -> Result<(), RuntimeError> { Ok(()) }
    fn delegated_finished(&mut self) {}
    fn take_delegated(&mut self) -> (Option<ScriptStreamState>, Vec<u64>, Option<super::super::ScopedProducerContext>) { (None, Vec::new(), None) }

    fn cancel(&mut self, evaluator: &mut Evaluator, span: Span) -> Result<(), RuntimeError> {
        self.pending = Vec::new().into_iter();
        if let Some(mut source) = self.source.take() { evaluator.stream_cancel(&mut source, span)?; }
        Ok(())
    }
}
