use super::*;
use crate::runtime::value::{PathValue, Value};
use crate::sema::operation_graph::PreparedLanguageOperation;

fn prepared() -> FullProgram {
    let source = "pure projected(value: Path, prefix: Path) -> Result[Str] { value.strip_prefix(prefix)?.display() }\npure other(value: Path, prefix: Path) -> Result[Str] { value.strip_prefix(prefix)?.display() }\npure comparison(left: Int, right: Int) -> Bool { left == right }\n";
    super::super::operation_prepare::tests::source_fixture(source, PreparedLanguageOperation::Equality { op: BinaryOp::Eq })
}

#[test]
fn result_postfix_receiver_preserves_success_and_error_after_frontend_drop_on_both_routes() {
    std::thread::Builder::new().stack_size(16 * 1024 * 1024).spawn(|| crate::runtime::eval::run_eval(|| {
        let program = Arc::new(prepared());
        program.symbol_owner().with_current(|| {
            FullVerifier::verify(&program).unwrap();
            let receipts = program.generic_evidence().unwrap().ground_native_calls()
                .filter_map(|(_, proof)| proof.contract.receiver.as_ref()?.postfix.as_ref()).collect::<Vec<_>>();
            assert_eq!(receipts.len(), 2);
            for receipt in receipts {
                assert!(program.generic_evidence().unwrap().registered_instruction_origin(receipt.instruction, false).is_none());
                assert_eq!(program.store.tags[receipt.instruction as usize], FullTag::ExprTry);
                let TypeRef::Ground(success) = receipt.success_type else { panic!("closed success domain") };
                assert_eq!(program.store.semantic.to_type(success).unwrap(), Type::Path);
            }
            let function = LoweredFunctionKey::Name(Name::intern("projected"));
            for recursive in [false, true] {
                for (path, accepted) in [("base/file.txt", true), ("outside/file.txt", false)] {
                    let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let arguments = [Value::Path(PathValue::new(path.as_bytes().to_vec()).unwrap()), Value::Path(PathValue::new(b"base".to_vec()).unwrap())];
                    let call = || evaluator.call_indexed_direct(function, LoweredFunctionKind::Pure, &arguments,
                        Span::new(program.store.source_id, 0, 0)).expect("projected function remains executable");
                    let result = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(function, recursive, call).unwrap();
                    if accepted { assert_eq!(result, Value::ok(Value::Str(Arc::from("file.txt")))); }
                    else { assert!(matches!(result, Value::Result(crate::runtime::value::ResultValue::Err(_))), "postfix propagation preserves the carrier error"); }
                }
            }
        });
    })).unwrap().join().unwrap_or_else(|payload| std::panic::resume_unwind(payload));
}

#[test]
fn result_postfix_receiver_rejects_missing_foreign_and_jointly_rewritten_carrier_authority() {
    std::thread::Builder::new().stack_size(16 * 1024 * 1024).spawn(|| crate::runtime::eval::run_eval(|| {
        let program = prepared();
        let foreign = prepared();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, proof) = generic.ground_native_calls().find(|(_, proof)| proof.contract.receiver.as_ref().is_some_and(|receiver| receiver.postfix.is_some())).unwrap();
            let receipt = proof.contract.receiver.as_ref().unwrap().postfix.as_ref().unwrap();
            let mut missing = program.clone();
            missing.store.generic.as_deref_mut().unwrap().test_ground_native_call_mut(id).unwrap().contract.receiver.as_mut().unwrap().postfix = None;
            assert!(FullVerifier::verify(&missing).is_err());
            let foreign_source = foreign.generic_evidence().unwrap().ground_native_calls()
                .find(|(_, proof)| proof.contract.receiver.as_ref().is_some_and(|receiver| receiver.postfix.is_some())).unwrap().1.source;
            let mut other_program = program.clone();
            other_program.store.generic.as_deref_mut().unwrap().test_ground_native_call_mut(id).unwrap().source = foreign_source;
            assert!(FullVerifier::verify(&other_program).is_err());
            let mut changed = program.clone();
            let range = changed.store.data[receipt.instruction as usize].range();
            changed.store.extra[range.start as usize] = receipt.instruction;
            let evidence = changed.store.generic.as_deref_mut().unwrap();
            let rewritten = evidence.test_ground_native_call_mut(id).unwrap().contract.receiver.as_mut().unwrap().postfix.as_mut().unwrap();
            rewritten.carrier = receipt.instruction;
            rewritten.payload = Box::new([receipt.instruction]);
            let contract = evidence.ground_native_call(id).unwrap().contract.clone();
            evidence.test_native_call_source_mut(proof.source).unwrap().expected = contract;
            assert!(FullVerifier::verify(&changed).is_err(), "joint receipt and bytecode changes cannot replace original carrier authority");
        });
    })).unwrap().join().unwrap_or_else(|payload| std::panic::resume_unwind(payload));
}
