use super::*;
use crate::runtime::value::{RecordMap, Value};
use crate::sema::operation_graph::PreparedLanguageOperation;

fn prepared() -> FullProgram {
    super::super::super::operation_prepare::tests::source_fixture(
        "type Plugin = module { export let name: Str; export optional let description: Str }\ntype Pair = {left: Int, right: Str}\npure strings(data: Bytes) -> List[Str] { data.strings() }\npure minimum(data: Bytes, length: Int) -> List[Str] { data.strings(min_len: length) }\npure keys(raw: Any) -> Result[List[Str]] { let plugin = raw.require(Plugin)?; plugin.keys() }\npure record_keys(value: Pair) -> List[Str] { value.keys() }\npure comparison(left: Str, right: Str) -> Bool { left == right }\n",
        PreparedLanguageOperation::Equality { op: BinaryOp::Eq })
}

#[test]
fn native_list_text_methods_preserve_defaults_module_carriers_and_results_on_both_routes() {
    std::thread::Builder::new().stack_size(16 * 1024 * 1024).spawn(|| crate::runtime::eval::run_eval(|| {
        let program = Arc::new(prepared());
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let calls = generic.ground_native_calls().filter(|(_, proof)| list_text_method_operation_is_supported(proof.contract.registry_owner,
                match proof.contract.authority { PreparedOperationAuthority::Registry { operation, .. } => operation, _ => panic!("original native member") })).collect::<Vec<_>>();
            assert_eq!(calls.len(), 4);
            assert_eq!(calls.iter().filter(|(_, proof)| proof.contract.binding.default_slots.as_ref() == [1]).count(), 1);
            for (_, proof) in &calls { assert!(verify_list_text_method_contract(&proof.contract, &program.store.semantic).unwrap()); }
            let module = Value::Module(RecordMap::from_name_values(vec![(Name::intern("name"), Value::Str(Arc::from("demo")))]));
            let record = Value::Record(RecordMap::from_name_values(vec![(Name::intern("left"), Value::Int(1)), (Name::intern("right"), Value::Str(Arc::from("two")))]));
            for recursive in [false, true] {
                for (name, arguments, expected) in [
                    ("strings", vec![Value::Bytes(b"\0abc\0hello\xff".to_vec())], vec!["hello"]),
                    ("minimum", vec![Value::Bytes(b"\0abc\0hello\xff".to_vec()), Value::Int(3)], vec!["abc", "hello"]),
                    ("keys", vec![module.clone()], vec!["name"]),
                    ("record_keys", vec![record.clone()], vec!["left", "right"]),
                ] {
                    let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let function = LoweredFunctionKey::Name(Name::intern(name));
                    let call = || evaluator.call_indexed_direct(function, LoweredFunctionKind::Pure, &arguments,
                        Span::new(program.store.source_id, 0, 0)).expect("original native producer remains executable");
                    let expected = Value::List(expected.into_iter().map(|value| Value::Str(Arc::from(value))).collect());
                    let expected = if name == "keys" { Value::ok(expected) } else { expected };
                    assert_eq!(crate::runtime::eval::lowered_run::with_observed_indexed_call_route(function, recursive, call).unwrap(), expected);
                }
            }
        });
    })).unwrap().join().unwrap_or_else(|payload| std::panic::resume_unwind(payload));
}

#[test]
fn native_list_text_methods_refuse_missing_foreign_and_jointly_changed_receiver_contracts() {
    std::thread::Builder::new().stack_size(16 * 1024 * 1024).spawn(|| crate::runtime::eval::run_eval(|| {
        let program = prepared();
        let foreign = prepared();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, proof) = generic.ground_native_calls().find(|(_, proof)| matches!(proof.contract.authority,
                PreparedOperationAuthority::Registry { operation: RuntimeOp::RecordKeys, .. })).unwrap();
            let mut missing = program.clone();
            missing.store.generic.as_deref_mut().unwrap().test_remove_ground_native_calls();
            assert!(FullVerifier::verify(&missing).is_err());
            let foreign_source = foreign.generic_evidence().unwrap().ground_native_calls().find(|(_, proof)| matches!(proof.contract.authority,
                PreparedOperationAuthority::Registry { operation: RuntimeOp::RecordKeys, .. })).unwrap().1.source;
            let mut other = program.clone();
            other.store.generic.as_deref_mut().unwrap().test_ground_native_call_mut(id).unwrap().source = foreign_source;
            assert!(FullVerifier::verify(&other).is_err());
            let mut changed = program.clone();
            let evidence = changed.store.generic.as_deref_mut().unwrap();
            let contract = &mut evidence.test_ground_native_call_mut(id).unwrap().contract;
            contract.receiver.as_mut().unwrap().method_name = Name::intern("strings");
            contract.argument_relations[0] = crate::sema::inference::ArgumentRelation::Assignable;
            let contract = contract.clone();
            evidence.test_native_call_source_mut(proof.source).unwrap().expected = contract;
            assert!(FullVerifier::verify(&changed).is_err(), "joint selected-source and method changes cannot replace original keys authority");
        });
    })).unwrap().join().unwrap_or_else(|payload| std::panic::resume_unwind(payload));
}
