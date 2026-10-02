use super::*;
use crate::sema::types::Type;
use crate::sema::operation_graph::{OperationArgumentOrder, PreparedLanguageOperation, ValueConstructor};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct PreparedRangeLowering {
    pub payload: Box<[u32]>,
    pub endpoints: [u32; 2],
    pub arguments: Box<[PreparedInvocationArgument]>,
}

impl PreparedRangeLowering {
    pub(super) fn retained_bytes(&self) -> usize {
        self.payload.len() * std::mem::size_of::<u32>() + self.arguments.len() * std::mem::size_of::<PreparedInvocationArgument>()
    }
}

impl GenericEvidenceStore {
    pub(in crate::runtime::eval::indexed) fn verify_range_operation_contract(&self, pools: &SemanticPools, operation: &PreparedOperation) -> Result<(), IrVerifyError> {
        let PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Constructor { kind: ValueConstructor::Range, arity }, argument_order: OperationArgumentOrder::SourceOrder, statement_result_is_unit: false, .. } = operation.authority else { return Err(failure("range lacks its selected language constructor")); };
        let source = self.operation_source(operation.source)?;
        let OperationSourceOrigin::Expression(origin) = source.origin else { return Err(failure("range source has another origin kind")); };
        let lowering = operation.range_lowering.as_ref().ok_or_else(|| failure("range loses its original endpoint lowering"))?;
        if !(1..=2).contains(&arity) || operation.receiver.is_some() || operation.arguments.len() != arity
            || operation.binding.supplied_slots.as_ref() != (0..arity as u32).collect::<Vec<_>>().as_slice()
            || operation.binding.operands.len() != arity || lowering.arguments.len() != arity
            || !operation.binding.default_slots.is_empty() || operation.binding.rest_slot.is_some() || operation.binding.dynamic.is_some()
            || operation.fallback_lowering.is_some() || operation.original_integer_addition.is_some()
            || operation.effects.creation != crate::sema::inference::EffectSet::EMPTY || !operation.effects.inputs.is_empty() || !operation.effects.outputs.is_empty()
            || lowering.payload.len() != 3 || lowering.payload[..2] != lowering.endpoints {
            return Err(failure("range changes its original arity, binding or effects"));
        }
        let expected_operands = if arity == 1 { &lowering.endpoints[1..] } else { lowering.endpoints.as_slice() };
        if operation.binding.operands.as_ref() != expected_operands { return Err(failure("range changes its original endpoint operands")); }
        for (ordinal, ((argument, reference), &operand)) in lowering.arguments.iter().zip(operation.arguments.iter()).zip(operation.binding.operands.iter()).enumerate() {
            let Some(TypeRef::Ground(ty)) = reference else { return Err(failure("range endpoint lacks a ground type")); };
            if argument.instruction != operand || argument.ty != TypeRef::Ground(*ty) || pools.type_tag(*ty)? != TypeTag::Int
                || !self.argument_has_original_source(operand, origin, ordinal, &argument.original, source.owner, argument.ty) {
                return Err(failure("range endpoint changes its original recipe or Int domain"));
            }
        }
        let TypeRef::Ground(result) = operation.result else { return Err(failure("range result lacks a ground type")); };
        if pools.to_type(result)? != Type::Stream(Box::new(Type::Int)) { return Err(failure("range changes its Stream Int result")); }
        Ok(())
    }
}
