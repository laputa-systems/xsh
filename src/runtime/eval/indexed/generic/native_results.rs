use super::*;
use crate::runtime::eval::require::PreparedSchema;
use crate::sema::types::Type;

/// A native Result retains the canonical success record layout independently
/// of the host value that will populate its fields.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedNativeResultRecord {
    pub success: GroundTypeId,
    pub schema: Arc<PreparedSchema>,
}

impl PreparedNativeResultRecord {
    pub(in crate::runtime::eval) fn retained_bytes(&self) -> usize { self.schema.retained_bytes() }

    pub(in crate::runtime::eval) fn materialize_result(&self, evaluator: &crate::runtime::eval::Evaluator, value: crate::runtime::eval::LoweredValue, span: crate::source::Span) -> Result<crate::runtime::eval::LoweredValue, crate::runtime::value::RuntimeError> {
        use crate::runtime::eval::LoweredValue;
        match value {
            LoweredValue::ResultOk(payload) => crate::runtime::eval::require::materialize_record_layout(evaluator, &self.schema, *payload, span)
                .map(|payload| LoweredValue::ResultOk(Box::new(payload))),
            value @ LoweredValue::ResultErr(_) => Ok(value),
            _ => Err(crate::runtime::value::RuntimeError::new("indexed-ir", "native operation returned a value outside its prepared Result carrier").with_span(span)),
        }
    }
}

impl NativeCallSource {
    pub(in crate::runtime::eval) fn verify_result_record_layout(&self, pools: &SemanticPools) -> Result<(), IrVerifyError> {
        let TypeRef::Ground(result) = self.expected.result else { return Err(failure("native result layout requires a closed result contract")); };
        let ty = pools.to_type(result)?;
        let success = match &ty { Type::Result(success, _) if matches!(success.as_ref(), Type::Record(_)) => Some(success.as_ref()), _ => None };
        match (&self.result_record_layout, success) {
            (Some(layout), Some(success)) => {
                if self.expected.cli_descriptor.is_some() || self.expected.result != TypeRef::Ground(pools.signature_return_type(self.expected.signature)?)
                    || pools.to_type(layout.success)? != *success || !matches!(layout.schema.as_ref(), PreparedSchema::Record(_))
                    || !layout.schema.valid() || !layout.schema.matches_type(success) {
                    return Err(failure("native Result success record changes its canonical selected layout"));
                }
            }
            (None, Some(_)) if self.expected.cli_descriptor.is_some() => {},
            (None, None) => {},
            _ => return Err(failure("native Result success record loses its original layout receipt")),
        }
        Ok(())
    }
}
