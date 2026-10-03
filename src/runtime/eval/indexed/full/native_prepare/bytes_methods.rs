use super::*;

pub(super) fn bytes_method_operation_is_supported(operation: RuntimeOp) -> bool {
    operation == RuntimeOp::BytesDump
}

// The selected default slot authorizes an absent format operand. The packet
// retains that absence so the backend can apply its canonical format directly.
pub(super) fn encoded_bytes_method_arguments(store: &FullStore, instruction: u32, count: usize, operation: RuntimeOp) -> Result<(RuntimeOp, Vec<Option<u32>>, u32), IrVerifyError> {
    if operation != RuntimeOp::BytesDump || store.tags.get(instruction as usize) != Some(&FullTag::ExprMethod) || count != 2 {
        return Err(IrVerifyError::new("byte dump changes its original method protocol"));
    }
    let words = store.payload(store.data[instruction as usize].range())?;
    if words.len() != 4 || store.string(words[1])? != "dump" {
        return Err(IrVerifyError::new("byte dump changes its original method spelling"));
    }
    let block = IrBlockId::from_raw(words[2]).and_then(|id| store.blocks.get(id.index()))
        .ok_or_else(|| IrVerifyError::new("byte dump argument block is invalid"))?;
    if block.flags & BLOCK_SEQUENCE_KIND_MASK != BLOCK_LIST { return Err(IrVerifyError::new("byte dump arguments have another block kind")); }
    let mut cursor = FullCursor::new(store.payload(block.instructions)?);
    let supplied = cursor.raw()?;
    if supplied > 1 { return Err(IrVerifyError::new("byte dump changes its original supplied argument count")); }
    let format = if supplied == 0 { None } else { Some(cursor.raw()?) };
    cursor.finish()?;
    Ok((operation, vec![Some(words[0]), format], words[3]))
}

pub(super) fn verify_bytes_method_contract(contract: &GroundNativeCallContract, pools: &SemanticPools) -> Result<bool, IrVerifyError> {
    if contract.registry_owner != RegistryOwner::Method(crate::modules::signature::MethodReceiver::Bytes)
        || !matches!(contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::BytesDump, .. }) {
        return Ok(false);
    }
    if !matches!(contract.authority, PreparedOperationAuthority::Registry {
        operation: RuntimeOp::BytesDump, binding: ImplBinding::Native,
        argument_check: crate::modules::signature::ApiArgCheck::Standard, semantic_rule: SemanticRule::Standard, ..
    }) { return Err(IrVerifyError::new("byte dump loses its original native registry boundary")); }
    let receiver = contract.receiver.as_ref().ok_or_else(|| IrVerifyError::new("byte dump loses its original receiver"))?;
    let (TypeRef::Ground(actual), TypeRef::Ground(source), TypeRef::Ground(result)) = (receiver.ty, receiver.source_type, contract.result) else {
        return Err(IrVerifyError::new("byte dump receiver or result is not closed"));
    };
    // A Result postfix keeps the original carrier descriptor. Its authenticated
    // success descriptor supplies the Bytes receiver after error propagation.
    let original_source_matches = match (pools.to_type(source)?, &receiver.postfix) {
        (Type::Bytes, None) => true,
        (Type::Result(success, _), Some(postfix)) => *success == Type::Bytes
            && postfix.source_type == receiver.source_type && postfix.success_type == receiver.ty,
        _ => false,
    };
    if contract.kind != crate::runtime::eval::indexed::generic::CallableKind::Pure
        || contract.effects.creation != crate::sema::inference::EffectSet::EMPTY
        || !contract.effects.inputs.is_empty() || !contract.effects.outputs.is_empty()
        || pools.signature_param_count(contract.signature)? != 2
        || pools.to_type(actual)? != Type::Bytes || !original_source_matches
        || pools.to_type(result)? != Type::Str || receiver.method_name != Name::intern("dump")
        || contract.binding.rest_slot.is_some() || contract.binding.dynamic.is_some() {
        return Err(IrVerifyError::new("byte dump changes its original receiver, result, kind, or modes"));
    }
    for slot in 0..2 {
        let (label, formal, _) = pools.signature_param(contract.signature, slot)?;
        let (expected_label, expected_type, defaulted) = if slot == 0 { ("<receiver>", Type::Bytes, false) } else { ("format", Type::Str, true) };
        if label != Name::intern(expected_label) || pools.to_type(formal)? != expected_type
            || pools.signature_parameter_defaulted(contract.signature, slot)? != defaulted || pools.signature_parameter_rest(contract.signature, slot)? {
            return Err(IrVerifyError::new("byte dump changes its original hidden receiver or format formal"));
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::eval::Evaluator;
    use crate::runtime::value::Value;
    use crate::sema::operation_graph::PreparedLanguageOperation;

    fn fixture() -> FullProgram {
        super::super::super::operation_prepare::tests::source_fixture("pure hex(data: Bytes) -> Str { data.dump(\"hex-u8\") }\npure octal(data: Bytes) -> Str { data.dump(format: \"octal-u8\") }\npure canonical(data: Bytes) -> Str { data.dump() }\npure other(data: Bytes) -> Str { data.dump(\"hex-u8\") }\npure encoded(data: Bytes) -> Str { data.base64() }\npure invalid(data: Bytes) -> Str { data.dump(\"bad\") }\npure checked(data: Bytes) -> Bool { data.dump(\"hex-u8\") == data.base64() }\n", PreparedLanguageOperation::Equality { op: BinaryOp::Eq })
    }

    #[test]
    fn direct_native_bytes_dump_keeps_format_defaults_and_original_operands_on_both_routes() {
        crate::runtime::eval::run_eval(|| {
            let program = Arc::new(fixture());
            program.symbol_owner().with_current(|| {
                let calls = program.generic_evidence().unwrap().ground_native_calls().filter(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::BytesDump, .. })).collect::<Vec<_>>();
                assert_eq!(calls.len(), 6);
                assert_eq!(calls.iter().filter(|(_, proof)| proof.contract.binding.default_slots.as_ref() == [1]).count(), 1);
                for (_, proof) in calls {
                    assert!(verify_bytes_method_contract(&proof.contract, &program.store.semantic).unwrap());
                    let (TypeRef::Ground(receiver), TypeRef::Ground(result)) = (proof.contract.receiver.as_ref().unwrap().ty, proof.contract.result) else { panic!("closed bytes and text types") };
                    assert_eq!(program.store.semantic.to_type(receiver).unwrap(), Type::Bytes);
                    assert_eq!(program.store.semantic.to_type(result).unwrap(), Type::Str);
                    assert_eq!(proof.contract.argument_sources.len(), 2);
                }
            });
            for recursive in [false, true] {
                for (function, argument, expected) in [
                    ("hex", b"hello".to_vec(), Value::Str("0000000 68 65 6c 6c 6f".into())),
                    ("octal", b"hello".to_vec(), Value::Str("0000000 150 145 154 154 157".into())),
                    ("canonical", Vec::new(), Value::Str("00000000".into())),
                    ("checked", b"hello".to_vec(), Value::Bool(false)),
                ] {
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(function)));
                    let arguments = [Value::Bytes(argument)];
                    let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &arguments, Span::new(program.store.source_id, 0, 0)).unwrap();
                    assert_eq!(crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap(), expected);
                }
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("invalid")));
                let arguments = [Value::Bytes(b"hello".to_vec())];
                let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &arguments, Span::new(program.store.source_id, 0, 0)).unwrap();
                assert_eq!(crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap_err().kind, "bytes-dump");
            }
        });
    }

    #[test]
    fn direct_native_bytes_dump_keeps_original_result_postfix_receiver_on_both_routes() {
        crate::runtime::eval::run_eval(|| {
            let program = Arc::new(super::super::super::operation_prepare::tests::source_fixture("pure dump(text: Str) -> Result[Str, Error] { text.base64_decode()?.dump(\"hex-u8\") }\npure comparison(left: Str, right: Str) -> Bool { left == right }\n", PreparedLanguageOperation::Equality { op: BinaryOp::Eq }));
            let changed = program.symbol_owner().with_current(|| {
                let generic = program.generic_evidence().unwrap();
                let (id, proof) = generic.ground_native_calls().find(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::BytesDump, .. })).unwrap();
                let receiver = proof.contract.receiver.as_ref().unwrap();
                let postfix = receiver.postfix.as_ref().unwrap();
                assert_eq!(postfix.source_type, receiver.source_type);
                assert_eq!(postfix.success_type, receiver.ty);
                let TypeRef::Ground(source) = receiver.source_type else { panic!("closed original result carrier") };
                assert!(matches!(program.store.semantic.to_type(source).unwrap(), Type::Result(success, _) if *success == Type::Bytes));
                let mut changed = (*program).clone();
                let evidence = changed.store.generic.as_deref_mut().unwrap();
                let altered = evidence.test_ground_native_call_mut(id).unwrap();
                let receiver = altered.contract.receiver.as_mut().unwrap();
                receiver.source_type = receiver.ty;
                receiver.postfix = None;
                let contract = altered.contract.clone();
                evidence.test_native_call_source_mut(proof.source).unwrap().expected = contract;
                assert!(FullVerifier::verify(&changed).is_err());
                Arc::new(changed)
            });
            for recursive in [false, true] {
                for (program, altered) in [(Arc::clone(&program), false), (Arc::clone(&changed), true)] {
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("dump")));
                    let arguments = [Value::Str("aGVsbG8=".into())];
                    let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &arguments, Span::new(program.store.source_id, 0, 0)).unwrap();
                    let result = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call);
                    if altered { assert_eq!(result.unwrap_err().kind, "indexed-ir"); }
                    else { assert_eq!(result.unwrap(), Value::ok(Value::Str("0000000 68 65 6c 6c 6f".into()))); }
                }
            }
        });
    }

    #[test]
    fn direct_native_bytes_dump_refuses_missing_foreign_default_and_joint_operation_receipts() {
        crate::runtime::eval::run_eval(|| {
            let program = fixture();
            program.symbol_owner().with_current(|| {
                let generic = program.generic_evidence().unwrap();
                let calls = generic.ground_native_calls().filter(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::BytesDump, .. })).collect::<Vec<_>>();
                let (id, _) = calls[0];
                let (default_id, _) = calls.iter().copied().find(|(_, proof)| !proof.contract.binding.default_slots.is_empty()).unwrap();
                let (joint_id, joint_proof) = *calls.last().unwrap();
                let joint_source = generic.native_call_source(joint_proof.source).unwrap();
                let (_, encoded) = generic.ground_native_calls().find(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::BytesBase64, .. }) && generic.native_call_source(proof.source).unwrap().owner == joint_source.owner).unwrap();
                let mut missing = program.clone();
                missing.store.generic.as_deref_mut().unwrap().test_remove_ground_native_calls();
                let mut foreign = program.clone();
                foreign.store.generic.as_deref_mut().unwrap().test_ground_native_call_mut(id).unwrap().source = calls[3].1.source;
                let mut default = program.clone();
                default.store.generic.as_deref_mut().unwrap().test_ground_native_call_mut(default_id).unwrap().contract.binding.default_slots[0] = 0;
                let mut joint = program.clone();
                let encoded_source = generic.native_call_source(encoded.source).unwrap();
                let target_range = joint.store.data[joint_source.instruction as usize].range();
                let encoded_range = joint.store.data[encoded_source.instruction as usize].range();
                let encoded_words = joint.store.extra[encoded_range.bounds(joint.store.extra.len()).unwrap()].to_vec();
                let target_bounds = target_range.bounds(joint.store.extra.len()).unwrap();
                joint.store.extra[target_bounds].copy_from_slice(&encoded_words);
                let evidence = joint.store.generic.as_deref_mut().unwrap();
                evidence.test_ground_native_call_mut(joint_id).unwrap().contract = encoded.contract.clone();
                let rewritten = evidence.test_native_call_source_mut(joint_proof.source).unwrap();
                rewritten.expected = encoded.contract.clone();
                rewritten.argument_lineages = encoded_source.argument_lineages.clone();
                for (changed, function) in [(missing, "hex"), (foreign, "hex"), (default, "canonical"), (joint, "checked")] {
                    assert!(FullVerifier::verify(&changed).is_err());
                    for recursive in [false, true] {
                        let changed = Arc::new(changed.clone());
                        let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*changed.sources).clone());
                        evaluator.indexed_program = Some(Arc::clone(&changed));
                        let key = LoweredFunctionKey::Name(Name::intern(function));
                        let arguments = [Value::Bytes(b"hello".to_vec())];
                        let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &arguments, Span::new(changed.store.source_id, 0, 0)).unwrap();
                        assert_eq!(crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap_err().kind, "indexed-ir");
                    }
                }
            });
        });
    }
}
