use super::*;

pub(in crate::runtime::eval::indexed) fn error_field_receiver_type(receiver: Atom) -> Option<Type> {
    match receiver {
        Atom::Error => Some(Type::Error),
        Atom::ProcessError => Some(Type::ProcessError),
        Atom::ErrorFamily(family) => Some(Type::ErrorFamily(family)),
        Atom::ErrorVariant { family, variant } => Some(Type::ErrorVariant { family, variant }),
        Atom::ErrorFacet(facet) => Some(Type::ErrorFacet(facet)),
        _ => None,
    }
}

impl FullVerifier {
    pub(in crate::runtime::eval::indexed) fn verify_prepared_error_field_contract(pools: &SemanticPools, operation: &PreparedOperation) -> Result<(), IrVerifyError> {
        let PreparedOperationAuthority::Language {
            authority: "language.projection.error_message", operation: PreparedLanguageOperation::ErrorField { receiver, field },
            argument_order: crate::sema::operation_graph::OperationArgumentOrder::SourceOrder, statement_result_is_unit: false, ..
        } = operation.authority else { return Err(IrVerifyError::new("error field loses its selected builtin authority")); };
        let receiver = error_field_receiver_type(receiver).ok_or_else(|| IrVerifyError::new("error field has another receiver domain"))?;
        if field != "message" || operation.receiver.is_some() || operation.arguments.len() != 1
            || operation.binding.supplied_slots.as_ref() != [0] || !operation.binding.default_slots.is_empty()
            || operation.binding.rest_slot.is_some() || operation.binding.dynamic.is_some() || operation.binding.operands.len() != 1
            || operation.effects.creation != crate::sema::inference::EffectSet::EMPTY
            || !operation.effects.inputs.is_empty() || !operation.effects.outputs.is_empty()
            || operation.fallback_lowering.is_some() || operation.original_integer_addition.is_some()
            || operation.range_lowering.is_some() || operation.literal_comparison_slot.is_some() {
            return Err(IrVerifyError::new("error field changes its original operand, field or effect contract"));
        }
        let Some(TypeRef::Ground(input)) = operation.arguments[0] else { return Err(IrVerifyError::new("error field receiver lacks a ground proof")); };
        let TypeRef::Ground(output) = operation.result else { return Err(IrVerifyError::new("error field result lacks a ground proof")); };
        if pools.to_type(input)? != receiver || pools.to_type(output)? != Type::Str {
            return Err(IrVerifyError::new("error field changes its exact checked receiver or result type"));
        }
        Ok(())
    }

    pub(in crate::runtime::eval::indexed) fn verify_error_field_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, expected: &Type, instance: Option<InstantiationId>, active: &mut Vec<u32>) -> Result<bool, IrVerifyError> {
        let Some(operation) = generic.operation_at(instruction)? else { return Ok(false); };
        let PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::ErrorField { field, .. }, .. } = operation.authority else { return Ok(false); };
        Self::verify_prepared_error_field_contract(&store.semantic, operation)?;
        let source = generic.operation_source(operation.source)?;
        let OperationSourceOrigin::Expression(origin) = source.origin else { return Err(IrVerifyError::new("error field has another original source kind")); };
        if source.instruction != instruction || source.owner != owner || operation.authority != source.expected
            || generic.registered_instruction_origin(instruction, false) != Some((source.origin, owner))
            || store.tags.get(instruction as usize) != Some(&FullTag::ExprField) || *expected != Type::Str {
            return Err(IrVerifyError::new("error field changes its original source, owner, opcode or consumer type"));
        }
        let words = store.payload(store.data[instruction as usize].range())?;
        if words.len() != 3 || words.first() != operation.binding.operands.first()
            || words.get(1).copied().and_then(|field| store.string(field).ok()) != Some(field.as_str().as_str())
            || words.get(2).and_then(|&location| IrLocationId::from_raw(location)).and_then(|location| store.location_sources.get(location.index())) != Some(&origin.source) {
            return Err(IrVerifyError::new("error field changes its original receiver, field or source location"));
        }
        let Some(TypeRef::Ground(receiver)) = operation.arguments[0] else { unreachable!() };
        Self::verify_generic_source(store, generic, words[0], owner, &store.semantic.to_type(receiver)?, instance, active)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests;
