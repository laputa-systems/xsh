use super::*;

// The omitted join separator remains absent in the packet. Only the original
// selected default slot authorizes the backend's empty separator behavior.
pub(super) fn encoded_list_join_arguments(store: &FullStore, instruction: u32, count: usize) -> Result<(RuntimeOp, Vec<Option<u32>>, u32), IrVerifyError> {
    if store.tags.get(instruction as usize) != Some(&FullTag::ExprMethod) || count != 2 {
        return Err(IrVerifyError::new("list join changes its original method protocol"));
    }
    let words = store.payload(store.data[instruction as usize].range())?;
    if words.len() != 4 || store.string(words[1])? != "join" {
        return Err(IrVerifyError::new("list join changes its original method spelling"));
    }
    let block = IrBlockId::from_raw(words[2]).and_then(|id| store.blocks.get(id.index()))
        .ok_or_else(|| IrVerifyError::new("list join argument block is invalid"))?;
    if block.flags & BLOCK_SEQUENCE_KIND_MASK != BLOCK_LIST { return Err(IrVerifyError::new("list join arguments have another block kind")); }
    let mut cursor = FullCursor::new(store.payload(block.instructions)?);
    let supplied = cursor.raw()?;
    if supplied > 1 { return Err(IrVerifyError::new("list join changes its original supplied argument count")); }
    let separator = if supplied == 0 { None } else { Some(cursor.raw()?) };
    cursor.finish()?;
    Ok((RuntimeOp::TextJoin, vec![Some(words[0]), separator], words[3]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::eval::Evaluator;
    use crate::runtime::value::Value;
    use crate::sema::operation_graph::PreparedLanguageOperation;

    fn fixture() -> FullProgram {
        super::super::super::operation_prepare::tests::source_fixture("pure extend(left: List[Int], right: List[Int]) -> List[Int] { left.extend(other: right) }\npure other(left: List[Int], right: List[Int]) -> List[Int] { left.extend(other: right) }\npure count(left: List[Int], right: List[Int]) -> Int { left.extend(other: right).len() }\npure comparison(left: Int, right: Int) -> Bool { left == right }\n", PreparedLanguageOperation::Equality { op: BinaryOp::Eq })
    }

    #[test]
    fn direct_native_list_extend_preserves_closed_items_and_order_on_both_routes() {
        crate::runtime::eval::run_eval(|| {
            let program = Arc::new(fixture());
            program.symbol_owner().with_current(|| {
                let generic = program.generic_evidence().unwrap();
                let calls = generic.ground_native_calls().filter(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::ListExtend, .. })).collect::<Vec<_>>();
                assert_eq!(calls.len(), 3);
                for (_, proof) in calls {
                    let TypeRef::Ground(result) = proof.contract.result else { panic!("closed list result") };
                    assert_eq!(program.store.semantic.to_type(result).unwrap(), Type::List(Box::new(Type::Int)));
                    assert_eq!(proof.contract.arguments.len(), 1);
                    assert!(proof.contract.binding.default_slots.is_empty());
                }
            });
            for recursive in [false, true] {
                for (left, right) in [(vec![Value::Int(1), Value::Int(2)], vec![Value::Int(3)]), (Vec::new(), vec![Value::Int(3)]), (vec![Value::Int(1)], Vec::new())] {
                    for function in ["extend", "count"] {
                        let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                        evaluator.indexed_program = Some(Arc::clone(&program));
                        let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(function)));
                        let arguments = [Value::List(left.clone()), Value::List(right.clone())];
                        let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &arguments, Span::new(program.store.source_id, 0, 0)).unwrap();
                        let value = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap();
                        if function == "count" { assert_eq!(value, Value::Int((left.len() + right.len()) as i64)); }
                        else { assert_eq!(value, Value::List(left.iter().chain(&right).cloned().collect())); }
                    }
                }
            }
        });
    }

    #[test]
    fn direct_native_list_extend_rejects_missing_foreign_and_joint_operation_receipts() {
        crate::runtime::eval::run_eval(|| {
            let program = fixture();
            program.symbol_owner().with_current(|| {
                let generic = program.generic_evidence().unwrap();
                let calls = generic.ground_native_calls().filter(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::ListExtend, .. })).collect::<Vec<_>>();
                let (id, proof) = calls[0];
                let source = generic.native_call_source(proof.source).unwrap();
                let (_, len) = generic.ground_native_calls().find(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::ListLen, .. })).unwrap();
                let mut missing = program.clone();
                missing.store.generic.as_deref_mut().unwrap().test_remove_ground_native_calls();
                let mut foreign = program.clone();
                foreign.store.generic.as_deref_mut().unwrap().test_ground_native_call_mut(id).unwrap().source = calls[1].1.source;
                let mut joint = program.clone();
                let range = joint.store.data[source.instruction as usize].range();
                let len_source = generic.native_call_source(len.source).unwrap();
                let len_range = joint.store.data[len_source.instruction as usize].range();
                joint.store.extra[range.start as usize + 1] = joint.store.extra[len_range.start as usize + 1];
                let evidence = joint.store.generic.as_deref_mut().unwrap();
                evidence.test_ground_native_call_mut(id).unwrap().contract = len.contract.clone();
                let rewritten = evidence.test_native_call_source_mut(proof.source).unwrap();
                rewritten.expected = len.contract.clone();
                rewritten.argument_lineages = len_source.argument_lineages.clone();
                for changed in [missing, foreign, joint] {
                    assert!(FullVerifier::verify(&changed).is_err());
                    for recursive in [false, true] {
                        let changed = Arc::new(changed.clone());
                        let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*changed.sources).clone());
                        evaluator.indexed_program = Some(Arc::clone(&changed));
                        let key = LoweredFunctionKey::Name(Name::intern("extend"));
                        let arguments = [Value::List(vec![Value::Int(1)]), Value::List(vec![Value::Int(2)])];
                        let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &arguments, Span::new(changed.store.source_id, 0, 0)).unwrap();
                        assert_eq!(crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap_err().kind, "indexed-ir");
                    }
                }
            });
        });
    }
}
