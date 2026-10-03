use super::*;

pub(super) fn verify_map_push_contract(contract: &GroundNativeCallContract, pools: &SemanticPools) -> Result<bool, IrVerifyError> {
    if contract.registry_owner != RegistryOwner::Method(crate::modules::signature::MethodReceiver::Map)
        || !matches!(contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::MapPush, .. }) {
        return Ok(false);
    }
    if !matches!(contract.authority, PreparedOperationAuthority::Registry {
        operation: RuntimeOp::MapPush, binding: ImplBinding::Native,
        argument_check: crate::modules::signature::ApiArgCheck::Standard, semantic_rule: SemanticRule::Standard, ..
    }) { return Err(IrVerifyError::new("map push loses its original native registry boundary")); }
    let receiver = contract.receiver.as_ref().ok_or_else(|| IrVerifyError::new("map push loses its original receiver"))?;
    let (TypeRef::Ground(actual), TypeRef::Ground(source), TypeRef::Ground(result)) = (receiver.ty, receiver.source_type, contract.result) else {
        return Err(IrVerifyError::new("map push receiver or result is not closed"));
    };
    let actual = pools.to_type(actual)?;
    let Type::Map(key, values) = &actual else { return Err(IrVerifyError::new("map push receiver has another collection domain")); };
    let Type::List(item) = values.as_ref() else { return Err(IrVerifyError::new("map push receiver has another value domain")); };
    let original_source_matches = match (pools.to_type(source)?, &receiver.postfix) {
        (source, None) => source == actual,
        (Type::Result(success, _), Some(postfix)) => *success == actual
            && postfix.source_type == receiver.source_type && postfix.success_type == receiver.ty,
        _ => false,
    };
    if contract.kind != crate::runtime::eval::indexed::generic::CallableKind::Pure
        || contract.effects.creation != crate::sema::inference::EffectSet::EMPTY
        || !contract.effects.inputs.is_empty() || !contract.effects.outputs.is_empty()
        || pools.signature_param_count(contract.signature)? != 3 || !original_source_matches
        || pools.to_type(result)? != actual || receiver.method_name != Name::intern("push")
        || !contract.binding.default_slots.is_empty() || contract.binding.rest_slot.is_some() || contract.binding.dynamic.is_some() {
        return Err(IrVerifyError::new("map push changes its original receiver, result, kind, or modes"));
    }
    for (slot, (expected_label, expected_type)) in [("<receiver>", &actual), ("key", key.as_ref()), ("value", item.as_ref())].into_iter().enumerate() {
        let (label, formal, _) = pools.signature_param(contract.signature, slot)?;
        if label != Name::intern(expected_label) || &pools.to_type(formal)? != expected_type
            || pools.signature_parameter_defaulted(contract.signature, slot)? || pools.signature_parameter_rest(contract.signature, slot)? {
            return Err(IrVerifyError::new("map push changes its original hidden receiver, key, or item formal"));
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::eval::Evaluator;
    use crate::runtime::value::{MapKey, Value};
    use crate::sema::operation_graph::PreparedLanguageOperation;

    fn fixture() -> FullProgram {
        super::super::super::operation_prepare::tests::source_fixture("pure push(values: Map[Str, List[UInt]], key: Str, value: Int) -> Map[Str, List[UInt]] { values.push(value: value, key: key) }\npure other(values: Map[Str, List[UInt]], key: Str, value: Int) -> Map[Str, List[UInt]] { values.push(value: value, key: key) }\npure size(values: Map[Str, List[UInt]], key: Str, value: Int) -> Int { values.push(key, value).len() }\npure checked(values: Map[Str, List[UInt]], key: Str, value: Int, replacement: List[UInt]) -> Bool { values.push(key, value) == values.set(key, replacement) }\npure comparison(left: Int, right: Int) -> Bool { left == right }\n", PreparedLanguageOperation::Equality { op: BinaryOp::Eq })
    }

    fn initial() -> Value {
        Value::Map(BTreeMap::from([
            (MapKey::Str("a".into()), Value::List(vec![Value::Int(1), Value::Int(2)])),
            (MapKey::Str("b".into()), Value::List(vec![Value::Int(8)])),
        ]))
    }

    #[test]
    fn direct_native_map_push_keeps_original_named_slots_and_checked_uint_items_on_both_routes() {
        crate::runtime::eval::run_eval(|| {
            let program = Arc::new(fixture());
            program.symbol_owner().with_current(|| {
                let generic = program.generic_evidence().unwrap();
                let calls = generic.ground_native_calls().filter(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::MapPush, .. })).collect::<Vec<_>>();
                assert_eq!(calls.len(), 4);
                let (_, named) = calls[0];
                assert_eq!(named.contract.binding.supplied_slots.as_ref(), &[2, 1]);
                assert_eq!(named.contract.arguments[0].original.name, Some(Name::intern("value")));
                assert_eq!(named.contract.arguments[1].original.name, Some(Name::intern("key")));
                for (_, proof) in calls {
                    assert!(verify_map_push_contract(&proof.contract, &program.store.semantic).unwrap());
                    let TypeRef::Ground(result) = proof.contract.result else { panic!("closed map result") };
                    assert_eq!(program.store.semantic.to_type(result).unwrap(), Type::Map(Box::new(Type::Str), Box::new(Type::List(Box::new(Type::UInt)))));
                    let source = generic.native_call_source(proof.source).unwrap();
                    let ordinal = proof.contract.binding.supplied_slots.iter().position(|slot| *slot == 2).unwrap();
                    let lineage = &source.argument_lineages[ordinal];
                    let (TypeRef::Ground(authored), TypeRef::Ground(material)) = (lineage.source_type, lineage.material_type) else { panic!("closed authored and checked item types") };
                    assert_eq!(program.store.semantic.to_type(authored).unwrap(), Type::Int);
                    assert_eq!(program.store.semantic.to_type(material).unwrap(), Type::UInt);
                    assert_eq!(proof.contract.arguments[ordinal].ty, lineage.material_type);
                }
            });
            for recursive in [false, true] {
                for key in ["a", "new"] {
                    for value in [3, -1] {
                        for function in ["push", "size"] {
                            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                            evaluator.indexed_program = Some(Arc::clone(&program));
                            let function_key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(function)));
                            let arguments = [initial(), Value::Str(key.into()), Value::Int(value)];
                            let call = || evaluator.call_indexed_direct(function_key, LoweredFunctionKind::Pure, &arguments, Span::new(program.store.source_id, 0, 0)).unwrap();
                            let result = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(function_key, recursive, call);
                            if value < 0 { assert_eq!(result.unwrap_err().kind, "type-error"); }
                            else if function == "size" { assert_eq!(result.unwrap(), Value::Int(if key == "a" { 2 } else { 3 })); }
                            else {
                                let Value::Map(mut expected) = initial() else { unreachable!() };
                                let entry = expected.entry(MapKey::Str(key.into())).or_insert_with(|| Value::List(Vec::new()));
                                let Value::List(items) = entry else { unreachable!() };
                                items.push(Value::Int(value));
                                assert_eq!(result.unwrap(), Value::Map(expected));
                            }
                        }
                    }
                }
            }
        });
    }

    #[test]
    fn direct_native_map_push_refuses_missing_foreign_removed_check_and_joint_set_receipts() {
        crate::runtime::eval::run_eval(|| {
            let program = fixture();
            program.symbol_owner().with_current(|| {
                let generic = program.generic_evidence().unwrap();
                let calls = generic.ground_native_calls().filter(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::MapPush, .. })).collect::<Vec<_>>();
                let (id, proof) = calls[0];
                let source = generic.native_call_source(proof.source).unwrap();
                let (joint_id, joint_proof) = *calls.last().unwrap();
                let joint_source = generic.native_call_source(joint_proof.source).unwrap();
                let (_, set) = generic.ground_native_calls().find(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::MapSet, .. }) && generic.native_call_source(proof.source).unwrap().owner == joint_source.owner).unwrap();
                let mut missing = program.clone();
                missing.store.generic.as_deref_mut().unwrap().test_remove_ground_native_calls();
                let mut foreign = program.clone();
                foreign.store.generic.as_deref_mut().unwrap().test_ground_native_call_mut(id).unwrap().source = calls[1].1.source;
                let mut removed = program.clone();
                let ordinal = proof.contract.binding.supplied_slots.iter().position(|slot| *slot == 2).unwrap();
                let lineage = &source.argument_lineages[ordinal];
                let checked = lineage.wrappers.iter().find(|wrapper| matches!(wrapper.kind, super::super::super::super::generic::ValueInitializerWrapperKind::CheckedValue)).unwrap();
                removed.store.tags[checked.instruction as usize] = FullTag::ExprParam;
                removed.store.data[checked.instruction as usize] = removed.store.data[lineage.source_instruction as usize];
                let mut joint = program.clone();
                let set_source = generic.native_call_source(set.source).unwrap();
                let target_bounds = joint.store.data[joint_source.instruction as usize].range().bounds(joint.store.extra.len()).unwrap();
                let set_bounds = joint.store.data[set_source.instruction as usize].range().bounds(joint.store.extra.len()).unwrap();
                let set_words = joint.store.extra[set_bounds].to_vec();
                joint.store.extra[target_bounds].copy_from_slice(&set_words);
                let evidence = joint.store.generic.as_deref_mut().unwrap();
                evidence.test_ground_native_call_mut(joint_id).unwrap().contract = set.contract.clone();
                let rewritten = evidence.test_native_call_source_mut(joint_proof.source).unwrap();
                rewritten.expected = set.contract.clone();
                rewritten.argument_lineages = set_source.argument_lineages.clone();
                for (changed, function) in [(missing, "push"), (foreign, "push"), (removed, "push"), (joint, "checked")] {
                    assert!(FullVerifier::verify(&changed).is_err());
                    for recursive in [false, true] {
                        let changed = Arc::new(changed.clone());
                        let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*changed.sources).clone());
                        evaluator.indexed_program = Some(Arc::clone(&changed));
                        let key = LoweredFunctionKey::Name(Name::intern(function));
                        let mut arguments = vec![initial(), Value::Str("a".into()), Value::Int(-1)];
                        if function == "checked" { arguments.push(Value::List(vec![Value::Int(9)])); }
                        let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &arguments, Span::new(changed.store.source_id, 0, 0)).unwrap();
                        assert_eq!(crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap_err().kind, "indexed-ir");
                    }
                }
            });
        });
    }
}
