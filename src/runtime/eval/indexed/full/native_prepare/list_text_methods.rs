use super::*;
use crate::runtime::eval::indexed::generic::CallableKind;

pub(super) fn list_text_method_operation_is_supported(owner: RegistryOwner, operation: RuntimeOp) -> bool {
    matches!((owner, operation),
        (RegistryOwner::Method(crate::modules::signature::MethodReceiver::Bytes), RuntimeOp::BytesStrings)
        | (RegistryOwner::Method(crate::modules::signature::MethodReceiver::Record), RuntimeOp::RecordKeys))
}

// A missing minimum length remains absent in the argument packet. The
// selected default slot authorizes the backend's original threshold.
pub(super) fn encoded_list_text_method_arguments(
    store: &FullStore, instruction: u32, parameter_count: usize, operation: RuntimeOp,
) -> Result<(RuntimeOp, Vec<Option<u32>>, u32), IrVerifyError> {
    let (spelling, count, optional) = match operation {
        RuntimeOp::BytesStrings => ("strings", 2, true),
        RuntimeOp::RecordKeys => ("keys", 1, false),
        _ => return Err(IrVerifyError::new("text list method has another selected operation")),
    };
    if store.tags.get(instruction as usize) != Some(&FullTag::ExprMethod) || parameter_count != count {
        return Err(IrVerifyError::new("text list method changes its original method protocol"));
    }
    let words = store.payload(store.data[instruction as usize].range())?;
    if words.len() != 4 || store.string(words[1])? != spelling {
        return Err(IrVerifyError::new("text list method changes its original method spelling"));
    }
    let block = IrBlockId::from_raw(words[2]).and_then(|id| store.blocks.get(id.index()))
        .ok_or_else(|| IrVerifyError::new("text list method argument block is invalid"))?;
    if block.flags & BLOCK_SEQUENCE_KIND_MASK != BLOCK_LIST { return Err(IrVerifyError::new("text list method arguments have another block kind")); }
    let mut cursor = FullCursor::new(store.payload(block.instructions)?);
    let supplied = cursor.raw()?;
    if supplied > u32::from(optional) { return Err(IrVerifyError::new("text list method changes its original supplied argument count")); }
    let mut arguments = vec![Some(words[0])];
    if optional { arguments.push(if supplied == 0 { None } else { Some(cursor.raw()?) }); }
    cursor.finish()?;
    Ok((operation, arguments, words[3]))
}

// Record keys expose names while preserving the actual record or module
// descriptor. Only the declared native erasure authorizes the hidden formal.
pub(in crate::runtime::eval::indexed) fn record_keys_receiver_accepts(
    formal: &Type, actual: &Type, relation: crate::sema::inference::ArgumentRelation,
) -> bool {
    relation == crate::sema::inference::ArgumentRelation::DeclaredErasure && *formal == Type::ErasedRecord
        && matches!(actual, Type::Record(_) | Type::ErasedRecord | Type::Module(_) | Type::DynamicModule)
}

pub(super) fn verify_list_text_method_contract(contract: &GroundNativeCallContract, pools: &SemanticPools) -> Result<bool, IrVerifyError> {
    let PreparedOperationAuthority::Registry { operation, binding, argument_check, semantic_rule, .. } = contract.authority else { return Ok(false); };
    if !list_text_method_operation_is_supported(contract.registry_owner, operation) { return Ok(false); }
    if binding != ImplBinding::Native || argument_check != crate::modules::signature::ApiArgCheck::Standard
        || semantic_rule != SemanticRule::Standard || contract.kind != CallableKind::Pure
        || contract.cli_descriptor.is_some() || contract.process_command_argv.is_some()
        || contract.binding.rest_slot.is_some() || contract.binding.dynamic.is_some()
        || contract.effects.creation != crate::sema::inference::EffectSet::EMPTY
        || !contract.effects.inputs.is_empty() || !contract.effects.outputs.is_empty()
        || pools.signature_closed_effects(contract.signature)? != crate::sema::inference::EffectSet::EMPTY {
        return Err(IrVerifyError::new("text list method loses its original pure native boundary"));
    }
    let receiver = contract.receiver.as_ref().ok_or_else(|| IrVerifyError::new("text list method loses its original receiver"))?;
    let (TypeRef::Ground(actual), TypeRef::Ground(source), TypeRef::Ground(result)) = (receiver.ty, receiver.source_type, contract.result) else {
        return Err(IrVerifyError::new("text list method receiver or result requires a closed descriptor"));
    };
    let actual = pools.to_type(actual)?;
    let source = pools.to_type(source)?;
    let original_source_matches = source == actual || receiver.postfix.as_ref().is_some_and(|postfix|
        postfix.source_type == receiver.source_type && postfix.success_type == receiver.ty);
    let count = if operation == RuntimeOp::BytesStrings { 2 } else { 1 };
    let spelling = if operation == RuntimeOp::BytesStrings { "strings" } else { "keys" };
    if !original_source_matches || receiver.method_name != Name::intern(spelling)
        || pools.signature_param_count(contract.signature)? != count
        || pools.to_type(result)? != Type::List(Box::new(Type::Str))
        || contract.result != TypeRef::Ground(pools.signature_return_type(contract.signature)?) {
        return Err(IrVerifyError::new("text list method changes its original receiver, spelling, or result"));
    }
    let (label, formal, _) = pools.signature_param(contract.signature, 0)?;
    if label != Name::intern("<receiver>") || pools.signature_parameter_defaulted(contract.signature, 0)?
        || pools.signature_parameter_rest(contract.signature, 0)? {
        return Err(IrVerifyError::new("text list method changes its original hidden receiver formal"));
    }
    let formal = pools.to_type(formal)?;
    if operation == RuntimeOp::BytesStrings {
        let (label, formal_minimum, _) = pools.signature_param(contract.signature, 1)?;
        if actual != Type::Bytes || formal != Type::Bytes || label != Name::intern("min_len")
            || pools.to_type(formal_minimum)? != Type::Int
            || !pools.signature_parameter_defaulted(contract.signature, 1)?
            || pools.signature_parameter_rest(contract.signature, 1)? {
            return Err(IrVerifyError::new("byte strings changes its original receiver or optional minimum length"));
        }
    } else if !record_keys_receiver_accepts(&formal, &actual,
        contract.argument_relations.first().copied().ok_or_else(|| IrVerifyError::new("record keys loses its original receiver relation"))?) {
        return Err(IrVerifyError::new("record keys changes its original declared receiver erasure"));
    }
    Ok(true)
}

#[cfg(test)]
mod tests;
