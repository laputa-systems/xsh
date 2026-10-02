use super::*;
use crate::runtime::eval::require::PreparedSchema;
use crate::sema::types::Type;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) enum PreparedNativeRecordCarrier { ResultRecord, ResultList, ResultStream, List, Stream }

impl PreparedNativeRecordCarrier {
    pub(in crate::runtime::eval) fn select(result: &Type) -> Option<(Self, &Type)> {
        match result {
            Type::Result(success, _) => match success.as_ref() {
                record @ Type::Record(_) => Some((Self::ResultRecord, record)),
                Type::List(record) if matches!(record.as_ref(), Type::Record(_)) => Some((Self::ResultList, record)),
                Type::Stream(record) if matches!(record.as_ref(), Type::Record(_)) => Some((Self::ResultStream, record)),
                _ => None,
            },
            Type::List(record) if matches!(record.as_ref(), Type::Record(_)) => Some((Self::List, record)),
            Type::Stream(record) if matches!(record.as_ref(), Type::Record(_)) => Some((Self::Stream, record)),
            _ => None,
        }
    }
}

/// The selected native output owns both its carrier and canonical record row.
/// A lazy filesystem item retains the shape without fetching metadata.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedNativeResultRecord {
    pub record: GroundTypeId,
    pub carrier: PreparedNativeRecordCarrier,
    pub schema: Arc<PreparedSchema>,
    pub shape: crate::runtime::value::RecordShape,
}

impl PreparedNativeResultRecord {
    pub(in crate::runtime::eval) fn retained_bytes(&self) -> usize { self.schema.retained_bytes() + self.shape.retained_bytes() }

    pub(in crate::runtime::eval) fn materialize_item(&self, evaluator: &crate::runtime::eval::Evaluator, value: crate::runtime::eval::LoweredValue, span: crate::source::Span) -> Result<crate::runtime::eval::LoweredValue, crate::runtime::value::RuntimeError> {
        use crate::runtime::eval::LoweredValue;
        if let LoweredValue::FsEntry(entry) = value {
            if self.shape.field_names().iter().any(|name| !entry.has_field(name.as_str().as_str())) {
                return Err(crate::runtime::value::RuntimeError::new("indexed-ir", "native filesystem item differs from its original record fields").with_span(span));
            }
            return Ok(LoweredValue::FsEntry(entry.with_prepared_shape(self.shape.clone())));
        }
        crate::runtime::eval::require::materialize_record_layout(evaluator, &self.schema, value, span)
    }

    fn materialize_items(&self, evaluator: &crate::runtime::eval::Evaluator, value: crate::runtime::eval::LoweredValue, stream: bool, span: crate::source::Span) -> Result<crate::runtime::eval::LoweredValue, crate::runtime::value::RuntimeError> {
        use crate::runtime::eval::LoweredValue;
        match value {
            LoweredValue::List(values) => values.into_iter().map(|value| self.materialize_item(evaluator, value, span)).collect::<Result<Vec<_>, _>>().map(LoweredValue::List),
            LoweredValue::SharedList(values) => crate::runtime::eval::lower::take_shared(values).into_iter().map(|value| self.materialize_item(evaluator, value, span)).collect::<Result<Vec<_>, _>>()
                .map(|values| LoweredValue::SharedList(Arc::new(values))),
            LoweredValue::Stream(value) if stream => Ok(crate::runtime::eval::lowered_run::wrap_native_record_items_stream(*value, self.clone())),
            _ => Err(crate::runtime::value::RuntimeError::new("indexed-ir", "native record items changed their prepared collection carrier").with_span(span)),
        }
    }

    pub(in crate::runtime::eval) fn materialize_result(&self, evaluator: &crate::runtime::eval::Evaluator, value: crate::runtime::eval::LoweredValue, span: crate::source::Span) -> Result<crate::runtime::eval::LoweredValue, crate::runtime::value::RuntimeError> {
        use crate::runtime::eval::LoweredValue;
        match (self.carrier, value) {
            (PreparedNativeRecordCarrier::ResultRecord, LoweredValue::ResultOk(payload)) => self.materialize_item(evaluator, *payload, span).map(|payload| LoweredValue::ResultOk(Box::new(payload))),
            (PreparedNativeRecordCarrier::ResultList | PreparedNativeRecordCarrier::ResultStream, LoweredValue::ResultOk(payload)) =>
                self.materialize_items(evaluator, *payload, self.carrier == PreparedNativeRecordCarrier::ResultStream, span).map(|payload| LoweredValue::ResultOk(Box::new(payload))),
            (PreparedNativeRecordCarrier::ResultRecord | PreparedNativeRecordCarrier::ResultList | PreparedNativeRecordCarrier::ResultStream, value @ LoweredValue::ResultErr(_)) => Ok(value),
            (PreparedNativeRecordCarrier::List | PreparedNativeRecordCarrier::Stream, value) => self.materialize_items(evaluator, value, self.carrier == PreparedNativeRecordCarrier::Stream, span),
            _ => Err(crate::runtime::value::RuntimeError::new("indexed-ir", "native operation returned a value outside its prepared Result carrier").with_span(span)),
        }
    }
}

impl NativeCallSource {
    pub(in crate::runtime::eval) fn verify_result_record_layout(&self, pools: &SemanticPools) -> Result<(), IrVerifyError> {
        let TypeRef::Ground(result) = self.expected.result else { return Err(failure("native result layout requires a closed result contract")); };
        let ty = pools.to_type(result)?;
        let selected = PreparedNativeRecordCarrier::select(&ty);
        match (&self.result_record_layout, selected) {
            (Some(layout), Some((carrier, record))) => {
                if self.expected.cli_descriptor.is_some() || self.expected.result != TypeRef::Ground(pools.signature_return_type(self.expected.signature)?)
                    || layout.carrier != carrier || pools.to_type(layout.record)? != *record
                    || !layout.schema.valid() || !layout.schema.matches_type(record) {
                    return Err(failure("native record output changes its canonical selected carrier or row"));
                }
                let (PreparedSchema::Record(fields), Type::Record(types)) = (layout.schema.as_ref(), record) else { return Err(failure("native record output has another schema kind")); };
                if fields.iter().map(|(name, _)| *name).ne(types.keys().copied()) || layout.shape.field_names().iter().copied().ne(types.keys().copied()) {
                    return Err(failure("native record output changes its canonical physical field order"));
                }
            }
            (None, Some(_)) if self.expected.cli_descriptor.is_some() => {},
            (None, None) => {},
            _ => return Err(failure("native record output loses its original layout receipt")),
        }
        Ok(())
    }
}
