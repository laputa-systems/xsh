use super::*;
use crate::sema::operation_graph::{ArithmeticDomain, PreparedLanguageOperation};
use crate::syntax::node::BinaryOp;

impl GenericEvidenceStore {
    pub(in crate::runtime::eval::indexed) fn is_duration_operation(operation: &PreparedOperation) -> bool {
        matches!(operation.authority, PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Arithmetic {
            domain: ArithmeticDomain::DurationPair | ArithmeticDomain::DurationScale { .. } | ArithmeticDomain::DurationRatio, ..
        }, .. } | PreparedOperationAuthority::Sealed { operation: crate::sema::inference::SealedOperation::AddDuration })
    }

    // Duration arithmetic has independent operand and result domains: ratios
    // return counts, while scaling accepts a count on its selected side.
    pub(in crate::runtime::eval::indexed) fn verify_duration_operation_contract(pools: &SemanticPools, operation: &PreparedOperation) -> Result<(), IrVerifyError> {
        let (left, right, result) = match operation.authority {
            PreparedOperationAuthority::Sealed { operation: crate::sema::inference::SealedOperation::AddDuration } => (TypeTag::Duration, TypeTag::Duration, TypeTag::Duration),
            PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Arithmetic { op, domain },
                argument_order: crate::sema::operation_graph::OperationArgumentOrder::SourceOrder, statement_result_is_unit: false, .. } => match (op, domain) {
                (BinaryOp::Add | BinaryOp::Sub, ArithmeticDomain::DurationPair) => (TypeTag::Duration, TypeTag::Duration, TypeTag::Duration),
                (BinaryOp::Mul | BinaryOp::Div, ArithmeticDomain::DurationScale { duration_left: true }) => (TypeTag::Duration, TypeTag::Int, TypeTag::Duration),
                (BinaryOp::Mul, ArithmeticDomain::DurationScale { duration_left: false }) => (TypeTag::Int, TypeTag::Duration, TypeTag::Duration),
                (BinaryOp::Div, ArithmeticDomain::DurationRatio) => (TypeTag::Duration, TypeTag::Duration, TypeTag::Int),
                _ => return Err(failure("Duration operation changes its selected operator domain")),
            },
            _ => return Err(failure("Duration operation lacks checked arithmetic authority")),
        };
        if operation.receiver.is_some() || operation.arguments.len() != 2 || operation.binding.supplied_slots.as_ref() != [0, 1]
            || !operation.binding.default_slots.is_empty() || operation.binding.operands.len() != 2
            || operation.binding.rest_slot.is_some() || operation.binding.dynamic.is_some() || operation.fallback_lowering.is_some()
            || !operation.effects.inputs.is_empty() || !operation.effects.outputs.is_empty()
            || operation.effects.creation != crate::sema::inference::EffectSet::EMPTY {
            return Err(failure("Duration operation changes its original binding or effect contract"));
        }
        for (reference, expected) in operation.arguments.iter().copied().chain([Some(operation.result)]).zip([left, right, result]) {
            let Some(TypeRef::Ground(ty)) = reference else { return Err(failure("Duration operation lacks a ground domain proof")); };
            if pools.type_tag(ty)? != expected { return Err(failure("Duration operation changes its checked operand or result domain")); }
        }
        Ok(())
    }
}
