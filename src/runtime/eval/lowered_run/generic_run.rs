use crate::runtime::eval::indexed::generic::{ConcreteOperationId, GenericReturnPlan};
use crate::runtime::eval::lowered_ops::{checked_int_binary, lowered_str_value};
use crate::runtime::eval::{LoweredValue, StmtFlow};
use crate::runtime::value::{FloatValue, RuntimeError};
use crate::source::Span;
use crate::syntax::node::BinaryOp;

/// The verifier connects this numeric slot to the argument's constructor layout.
/// Execution reads existing storage without looking up a field name.
pub(super) fn project_record_slot(
    receiver: &LoweredValue,
    slot: u32,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    project_record_slot_ref(receiver, slot, span).cloned()
}

pub(super) fn project_record_slot_ref(
    receiver: &LoweredValue,
    slot: u32,
    span: Span,
) -> Result<&LoweredValue, RuntimeError> {
    let LoweredValue::RecordVec(fields) = receiver else {
        return Err(RuntimeError::new("indexed-ir", "generic projection requires its prepared record layout").with_span(span));
    };
    fields.get(slot as usize).map(|(_, value)| value)
        .ok_or_else(|| RuntimeError::new("indexed-ir", "generic projection slot is outside its record layout").with_span(span))
}

/// Operand values cannot choose a different operation than the prepared witness.
pub(super) fn execute_operation(
    operation: ConcreteOperationId,
    left: &LoweredValue,
    right: &LoweredValue,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    match operation {
        ConcreteOperationId::AddInt => match (left, right) {
            (LoweredValue::Int(left), LoweredValue::Int(right)) => {
                checked_int_binary(BinaryOp::Add, *left, *right, span).map(LoweredValue::Int)
            }
            _ => Err(incompatible_operation(span)),
        },
        ConcreteOperationId::AddFloat => match (left, right) {
            (LoweredValue::Float(left), LoweredValue::Float(right)) => {
                Ok(LoweredValue::Float(FloatValue::new(left.0 + right.0)))
            }
            _ => Err(incompatible_operation(span)),
        },
        ConcreteOperationId::AddStr => {
            let (Some(left), Some(right)) = (lowered_str_value(left), lowered_str_value(right)) else {
                return Err(incompatible_operation(span));
            };
            let mut text = left.to_owned();
            text.push_str(right);
            Ok(LoweredValue::Str(text.into()))
        }
    }
}

fn incompatible_operation(span: Span) -> RuntimeError {
    RuntimeError::new("indexed-ir", "generic operands disagree with their prepared operation").with_span(span)
}

/// The declaration and lexical exit determine wrapping; the payload cannot
/// change a value return into failure propagation or flatten a nested result.
pub(super) fn finish_return(
    plan: GenericReturnPlan,
    flow: StmtFlow,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    match (plan, flow) {
        (GenericReturnPlan::Value, StmtFlow::Value(value) | StmtFlow::Return(value) | StmtFlow::Propagate(value)) => Ok(value),
        (GenericReturnPlan::Result, StmtFlow::Value(value) | StmtFlow::Return(value)) => {
            Ok(LoweredValue::ResultOk(Box::new(value)))
        }
        (GenericReturnPlan::Result, StmtFlow::Propagate(value @ LoweredValue::ResultErr(_))) => Ok(value),
        (GenericReturnPlan::ResultUnit, StmtFlow::None | StmtFlow::Value(LoweredValue::Unit) | StmtFlow::Return(LoweredValue::Unit)) => Ok(LoweredValue::ResultOk(Box::new(LoweredValue::Unit))),
        (GenericReturnPlan::ResultUnit, StmtFlow::Value(value @ LoweredValue::ResultErr(_)) | StmtFlow::Return(value @ LoweredValue::ResultErr(_)) | StmtFlow::Propagate(value @ LoweredValue::ResultErr(_))) => Ok(value),
        (GenericReturnPlan::ResultUnit, StmtFlow::Value(value @ LoweredValue::ResultOk(_)) | StmtFlow::Return(value @ LoweredValue::ResultOk(_))) if matches!(&value, LoweredValue::ResultOk(unit) if matches!(unit.as_ref(), LoweredValue::Unit)) => Ok(value),
        (GenericReturnPlan::Unit, StmtFlow::None | StmtFlow::Value(LoweredValue::Unit) | StmtFlow::Return(LoweredValue::Unit)) => Ok(LoweredValue::Unit),
        (_, _) => Err(RuntimeError::new("indexed-ir", "generic exit disagrees with its declaration return plan").with_span(span)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceId;
    use crate::symbol::Name;
    use std::sync::Arc;

    fn span() -> Span { Span::new(SourceId::new(0), 0, 1) }

    #[test]
    fn projection_reads_distinct_physical_slots_and_shares_container_payloads() {
        let values = Arc::new(vec![LoweredValue::Int(7)]);
        let narrow = LoweredValue::RecordVec(Arc::new(vec![
            (Name::intern("name"), LoweredValue::SharedList(Arc::clone(&values))),
        ]));
        let wide = LoweredValue::RecordVec(Arc::new(vec![
            (Name::intern("age"), LoweredValue::Int(11)),
            (Name::intern("name"), LoweredValue::SharedList(Arc::clone(&values))),
        ]));
        for (record, slot) in [(&narrow, 0), (&wide, 1)] {
            let LoweredValue::SharedList(selected) = project_record_slot(record, slot, span()).unwrap() else { panic!("container payload changed") };
            assert!(Arc::ptr_eq(&selected, &values));
        }
        assert!(project_record_slot(&narrow, 1, span()).is_err());
        assert!(project_record_slot(&LoweredValue::Bool(false), 0, span()).is_err());
    }

    #[test]
    fn prepared_operation_rejects_other_supported_domains_instead_of_reselecting() {
        let int = LoweredValue::Int(7);
        let float = LoweredValue::Float(FloatValue::new(1.25));
        let text = LoweredValue::Str(Arc::from("value"));
        for (operation, operand, expected) in [
            (ConcreteOperationId::AddInt, &int, LoweredValue::Int(14)),
            (ConcreteOperationId::AddFloat, &float, LoweredValue::Float(FloatValue::new(2.5))),
            (ConcreteOperationId::AddStr, &text, LoweredValue::Str(Arc::from("valuevalue"))),
        ] {
            assert_eq!(execute_operation(operation, operand, operand, span()).unwrap(), expected);
            for other in [&int, &float, &text] {
                if std::ptr::eq(operand, other) { continue; }
                assert!(execute_operation(operation, other, other, span()).is_err());
            }
            assert!(execute_operation(operation, &LoweredValue::Bool(false), operand, span()).is_err());
        }
    }

    #[test]
    fn prepared_integer_add_preserves_checked_overflow() {
        assert!(execute_operation(ConcreteOperationId::AddInt,
            &LoweredValue::Int(i64::MAX), &LoweredValue::Int(1), span()).is_err());
    }

    #[test]
    fn declaration_return_plan_preserves_false_and_nested_results() {
        assert_eq!(finish_return(GenericReturnPlan::Value, StmtFlow::Return(LoweredValue::Bool(false)), span()).unwrap(), LoweredValue::Bool(false));
        let nested = LoweredValue::ResultOk(Box::new(LoweredValue::Bool(false)));
        assert_eq!(finish_return(GenericReturnPlan::Value, StmtFlow::Return(nested.clone()), span()).unwrap(), nested);
        assert_eq!(finish_return(GenericReturnPlan::Result, StmtFlow::Return(nested.clone()), span()).unwrap(), LoweredValue::ResultOk(Box::new(nested)));
        let failure = LoweredValue::ResultErr(Box::new(crate::runtime::value::Value::Error(Box::new(RuntimeError::new("failure", "checked failure")))));
        assert_eq!(finish_return(GenericReturnPlan::Value, StmtFlow::Return(failure.clone()), span()).unwrap(), failure);
        assert_eq!(finish_return(GenericReturnPlan::Result, StmtFlow::Propagate(failure.clone()), span()).unwrap(), failure);
        assert!(finish_return(GenericReturnPlan::Unit, StmtFlow::Return(LoweredValue::Bool(false)), span()).is_err());
    }
}
