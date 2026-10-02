use super::*;
use super::super::super::generic::{PreparedNativeResultRecord, PreparedNativeRecordCarrier};

impl FullBuilder {
    pub(in crate::runtime::eval::indexed::full) fn prepare_native_result_record(&mut self, result: &Type) -> Result<Option<PreparedNativeResultRecord>, IrBuildError> {
        let Some((carrier, record)) = PreparedNativeRecordCarrier::select(result) else { return Ok(None); };
        let Type::Record(fields) = record else { unreachable!() };
        let shape = crate::runtime::value::RecordShape::new(fields.keys().map(|name| Arc::from(name.as_str().as_str())).collect());
        let schema = crate::runtime::eval::require::PreparedSchema::compile_record_layout(record).ok_or_else(|| native_problem("native_result_record_canonical_layout"))?;
        let record = self.intern_generic_ground_type(record)?;
        Ok(Some(PreparedNativeResultRecord { record, carrier, schema, shape }))
    }
}

impl FullExecution<'_> {
    pub(in crate::runtime::eval) fn materialize_native_result_record(&self, instruction: u32, evaluator: &crate::runtime::eval::Evaluator, value: LoweredValue, span: Span) -> Result<LoweredValue, crate::runtime::value::RuntimeError> {
        let result = || -> Result<Option<&PreparedNativeResultRecord>, IrVerifyError> {
            if !self.decoder.instruction_range.contains(&(instruction as usize)) { return Err(IrVerifyError::new("native result record belongs to another body")); }
            let Some(generic) = self.decoder.store.generic.as_deref() else { return Ok(None); };
            let Some(id) = generic.ground_native_call_at(instruction)? else { return Ok(None); };
            let proof = generic.ground_native_call(id)?;
            let source = generic.native_call_source(proof.source)?;
            let owner = if let Some(driver) = driver_owner_index(self.decoder.owner) { InstructionOwner::Driver(driver as u32) } else { InstructionOwner::Function(IrFunctionId::from_raw(self.decoder.owner).ok_or_else(|| IrVerifyError::new("native result record owner is invalid"))?) };
            if source.owner != owner || source.instruction != instruction || source.expected != proof.contract { return Err(IrVerifyError::new("native result record loses its original call authority")); }
            source.verify_result_record_layout(&self.decoder.store.semantic)?;
            Ok(source.result_record_layout.as_ref())
        };
        let layout = result().map_err(|error| crate::runtime::value::RuntimeError::new("indexed-ir", error.message).with_span(span))?;
        match layout { Some(layout) => layout.materialize_result(evaluator, value, span), None => Ok(value) }
    }
}
