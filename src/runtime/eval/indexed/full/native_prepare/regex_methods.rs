use super::*;

pub(super) fn regex_method_operation_is_supported(operation: RuntimeOp) -> bool {
    matches!(operation, RuntimeOp::RegexMatches | RuntimeOp::RegexFind | RuntimeOp::RegexCaptures | RuntimeOp::RegexReplace)
}

/// Closed Regex methods preserve the compiled receiver and return ordinary
/// closed data. Their hidden receiver and authored text operands have distinct
/// slots, and every operand is supplied before the pure native operation runs.
pub(super) fn verify_regex_method_contract(contract: &GroundNativeCallContract, pools: &SemanticPools) -> Result<bool, IrVerifyError> {
    if contract.registry_owner != RegistryOwner::Method(crate::modules::signature::MethodReceiver::Regex) { return Ok(false); }
    let PreparedOperationAuthority::Registry { operation, binding: ImplBinding::Native, argument_check: crate::modules::signature::ApiArgCheck::Standard, semantic_rule: SemanticRule::Standard, .. } = contract.authority else {
        return Err(IrVerifyError::new("Regex method loses its original native registry boundary"));
    };
    let spelling = super::super::super::native_methods::nondefault_method_spelling(crate::modules::signature::MethodReceiver::Regex, operation)
        .ok_or_else(|| IrVerifyError::new("Regex method has another selected operation"))?;
    let count = if operation == RuntimeOp::RegexReplace { 3 } else { 2 };
    let expected_result = match operation {
        RuntimeOp::RegexMatches => Type::Bool,
        RuntimeOp::RegexFind => Type::List(Box::new(Type::Record(BTreeMap::from([
            (Name::intern("start"), Type::Int), (Name::intern("end"), Type::Int), (Name::intern("text"), Type::Str),
        ])))),
        RuntimeOp::RegexCaptures => Type::List(Box::new(Type::Str)),
        RuntimeOp::RegexReplace => Type::Str,
        _ => unreachable!(),
    };
    let receiver = contract.receiver.as_ref().ok_or_else(|| IrVerifyError::new("Regex method loses its compiled receiver"))?;
    let (TypeRef::Ground(actual), TypeRef::Ground(source), TypeRef::Ground(result)) = (receiver.ty, receiver.source_type, contract.result) else {
        return Err(IrVerifyError::new("Regex method receiver or result is not closed"));
    };
    if contract.kind != crate::runtime::eval::indexed::generic::CallableKind::Pure
        || contract.effects.creation != crate::sema::inference::EffectSet::EMPTY
        || pools.signature_param_count(contract.signature)? != count
        || pools.to_type(actual)? != Type::Regex || pools.to_type(source)? != Type::Regex
        || pools.to_type(result)? != expected_result || receiver.method_name != Name::intern(spelling)
        || !contract.binding.default_slots.is_empty() || contract.binding.rest_slot.is_some() || contract.binding.dynamic.is_some() {
        return Err(IrVerifyError::new("Regex method changes its original receiver, result, kind, or modes"));
    }
    for slot in 0..count {
        let (label, formal, _) = pools.signature_param(contract.signature, slot)?;
        let expected_label = match slot { 0 => "<receiver>", 1 => "text", _ => "replacement" };
        let expected = if slot == 0 { Type::Regex } else { Type::Str };
        if label != Name::intern(expected_label) || pools.to_type(formal)? != expected
            || pools.signature_parameter_defaulted(contract.signature, slot)? || pools.signature_parameter_rest(contract.signature, slot)? {
            return Err(IrVerifyError::new("Regex method changes its original hidden receiver or text formal"));
        }
    }
    Ok(true)
}
