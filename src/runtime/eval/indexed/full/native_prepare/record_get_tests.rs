use super::*;
use crate::runtime::eval::Evaluator;
use crate::runtime::value::Value;
use crate::sema::operation_graph::PreparedLanguageOperation;

fn on_large_stack(work: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new().stack_size(16 * 1024 * 1024).spawn(work).unwrap().join().unwrap_or_else(|payload| std::panic::resume_unwind(payload));
}

fn fixture() -> FullProgram {
    super::super::operation_prepare::tests::source_fixture("type Config = {workers: Int, limit: Int}\nconst field = \"workers\"\npure counts() -> Int { let config: Config = {workers: 4, limit: 9}; config.get(field) ?? 0 }\npure other() -> Int { let config: Config = {workers: 7, limit: 8}; config.get(\"workers\") ?? 0 }\npure comparison(left: Int, right: Int) -> Bool { left == right }\n", PreparedLanguageOperation::Equality { op: BinaryOp::Eq })
}

#[test]
fn direct_native_record_get_keeps_selected_erasure_and_original_field_producer_on_both_routes() {
    on_large_stack(|| {
        let program = Arc::new(fixture());
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let calls = generic.ground_native_calls().filter(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::RecordGet, .. })).collect::<Vec<_>>();
            assert_eq!(calls.len(), 2);
            for (_, proof) in calls {
                let source = generic.native_call_source(proof.source).unwrap();
                assert!(source.verify_record_get_refinement(&program.store.semantic).unwrap());
                let refinement = source.result_refinement.as_ref().unwrap();
                assert_eq!(refinement.field, Name::intern("workers"));
                let TypeRef::Ground(selected) = refinement.registry_result else { panic!("selected closed result") };
                let TypeRef::Ground(producer) = refinement.producer_result else { panic!("closed field producer") };
                let Type::Result(success, error) = program.store.semantic.to_type(selected).unwrap() else { panic!("selected fallible result") };
                assert_eq!(*success, Type::Any);
                assert_eq!(program.store.semantic.to_type(producer).unwrap(), Type::Result(Box::new(Type::Int), error));
                assert_eq!(proof.contract.result, refinement.registry_result);
            }
        });
        for recursive in [false, true] {
            for (function, expected) in [("counts", 4), ("other", 7)] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(function)));
                let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &[], Span::new(program.store.source_id, 0, 0)).unwrap();
                assert_eq!(crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap(), Value::Int(expected));
            }
        }
    });
}

#[test]
fn direct_native_record_get_rejects_missing_foreign_and_rewritten_field_receipts() {
    on_large_stack(|| {
        let program = fixture();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let calls = generic.ground_native_calls().filter(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::RecordGet, .. })).collect::<Vec<_>>();
            let (id, proof) = calls[0];
            let source = generic.native_call_source(proof.source).unwrap();
            let foreign = calls[1].1.contract.receiver.as_ref().unwrap();
            let mut missing = program.clone();
            missing.store.generic.as_deref_mut().unwrap().test_remove_ground_native_calls();
            let mut field = program.clone();
            field.store.generic.as_deref_mut().unwrap().test_native_call_source_mut(proof.source).unwrap().result_refinement.as_mut().unwrap().field = Name::intern("limit");
            let mut receiver = program.clone();
            let range = receiver.store.data[source.instruction as usize].range();
            receiver.store.extra[range.start as usize] = foreign.instruction;
            let evidence = receiver.store.generic.as_deref_mut().unwrap();
            evidence.test_ground_native_call_mut(id).unwrap().contract.receiver = Some(foreign.clone());
            evidence.test_ground_native_call_mut(id).unwrap().contract.argument_sources[0] = Some(foreign.instruction);
            let altered = evidence.ground_native_call(id).unwrap().contract.clone();
            let source = evidence.test_native_call_source_mut(proof.source).unwrap();
            source.expected = altered;
            source.result_refinement.as_mut().unwrap().receiver = foreign.instruction;
            for changed in [missing, field, receiver] {
                assert!(FullVerifier::verify(&changed).is_err());
                for recursive in [false, true] {
                    let changed = Arc::new(changed.clone());
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*changed.sources).clone());
                    evaluator.indexed_program = Some(Arc::clone(&changed));
                    let key = LoweredFunctionKey::Name(Name::intern("counts"));
                    let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &[], Span::new(changed.store.source_id, 0, 0)).unwrap();
                    assert_eq!(crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap_err().kind, "indexed-ir");
                }
            }
        });
    });
}
