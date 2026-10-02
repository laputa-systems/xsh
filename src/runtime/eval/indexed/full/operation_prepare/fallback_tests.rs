use super::*;
use crate::runtime::eval::Evaluator;
use crate::runtime::value::Value;

fn fixture() -> FullProgram {
    super::tests::source_fixture(
        "pure selected(value: Result[Int]) -> Int { value ?? 9 }\npure nullable(value: Int?) -> Int { value ?? 9 }\npure lazy(value: Result[Int]) -> Int { value ?? (1 / 0) }\npure nullable_lazy(value: Int?) -> Int { value ?? (1 / 0) }\n",
        PreparedLanguageOperation::Fallback { result: true },
    )
}

#[test]
fn original_fallback_carriers_execute_lazy_selection_after_frontend_disposal_on_both_routes() {
    crate::runtime::eval::run_eval(|| {
        let program = Arc::new(fixture());
        program.symbol_owner().with_current(|| {
            FullVerifier::verify(&program).unwrap();
            assert_eq!(program.generic_evidence().unwrap().operations().filter(|(_, operation)|
                matches!(operation.authority, PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Fallback { .. }, .. })).count(), 4);
            for recursive in [false, true] {
                for (name, argument, expected) in [
                    ("selected", Value::Result(crate::runtime::value::ResultValue::Ok(Box::new(Value::Int(7)))), Value::Int(7)),
                    ("lazy", Value::Result(crate::runtime::value::ResultValue::Ok(Box::new(Value::Int(7)))), Value::Int(7)),
                    ("nullable", Value::Null, Value::Int(9)),
                    ("nullable", Value::Int(4), Value::Int(4)),
                    ("nullable_lazy", Value::Int(4), Value::Int(4)),
                ] {
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let function = LoweredFunctionKey::Name(Name::intern(name));
                    let call = || evaluator.call_indexed_direct(function, LoweredFunctionKind::Pure,
                        std::slice::from_ref(&argument), Span::new(program.store.source_id, 0, 0)).expect("fallback function exists");
                    let result = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(function, recursive, call);
                    assert_eq!(result.unwrap(), expected);
                }
            }
        });
    });
}

#[test]
fn original_fallback_authority_rejects_rewritten_missing_and_foreign_proofs() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture();
        let foreign = fixture();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, operation) = generic.operations().find(|(_, operation)|
                matches!(operation.authority, PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Fallback { result: true }, .. })).unwrap();
            let mut missing = program.clone();
            missing.store.generic.as_deref_mut().unwrap().test_remove_operations();
            assert!(FullVerifier::verify(&missing).is_err());
            let mut rewritten = program.clone();
            let evidence = rewritten.store.generic.as_deref_mut().unwrap();
            let PreparedOperationAuthority::Language { operation: selected, .. } = &mut evidence.test_operation_mut(id).unwrap().authority else { unreachable!() };
            *selected = PreparedLanguageOperation::Fallback { result: false };
            let error = FullVerifier::verify_generic_evidence(&rewritten.store).unwrap_err();
            assert!(error.message.contains("original"), "{}", error.message);
            let mut coforged = rewritten.clone();
            let PreparedOperationAuthority::Language { operation: selected, .. } = &mut coforged.store.generic.as_deref_mut().unwrap().test_operation_source_mut(operation.source).unwrap().expected else { unreachable!() };
            *selected = PreparedLanguageOperation::Fallback { result: false };
            assert!(FullVerifier::verify(&coforged).is_err(), "another carrier kind cannot replace the original Result fallback");
            let mut renamed = program.clone();
            let forged_identity = Name::intern("language.binary.ResultFallback.Forged");
            let evidence = renamed.store.generic.as_deref_mut().unwrap();
            let original = evidence.test_operation_source_mut(operation.source).unwrap();
            original.identity = forged_identity;
            let PreparedOperationAuthority::Language { identity, authority, .. } = &mut original.expected else { unreachable!() };
            *identity = forged_identity;
            *authority = "foreign.fallback";
            let PreparedOperationAuthority::Language { identity, authority, .. } = &mut evidence.test_operation_mut(id).unwrap().authority else { unreachable!() };
            *identity = forged_identity;
            *authority = "foreign.fallback";
            assert!(FullVerifier::verify(&renamed).is_err(), "jointly rewritten metadata cannot replace the original selected authority");
            let mut foreign_source = program.clone();
            foreign_source.store.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().source = foreign.generic_evidence().unwrap().operations().next().unwrap().1.source;
            assert!(FullVerifier::verify(&foreign_source).is_err());
            let mut effects = program.clone();
            effects.store.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().effects.creation = crate::sema::inference::EffectSet(1);
            assert!(FullVerifier::verify(&effects).is_err(), "a fallback cannot gain an unselected effect role");
            let mut wrong_right = program.clone();
            wrong_right.store.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().arguments[1] = operation.arguments[0];
            assert!(FullVerifier::verify(&wrong_right).is_err(), "a fallback carrier cannot serve as its success-domain operand");
            let source = generic.operation_source(operation.source).unwrap();
            let mut swapped = program.clone();
            let range = swapped.store.data[source.instruction as usize].range();
            swapped.store.extra[range.start as usize] = operation.binding.operands[1];
            assert!(FullVerifier::verify(&swapped).is_err(), "the right value cannot impersonate the original carrier");
        });
    });
}

#[test]
fn original_optional_fallback_refuses_jointly_rewritten_binding_and_present_read() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (_, operation) = generic.operations().find(|(_, operation)| matches!(operation.authority,
                PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Fallback { result: false }, .. })).unwrap();
            let source = generic.operation_source(operation.source).unwrap();
            let words = program.store.payload(program.store.data[source.instruction as usize].range()).unwrap();
            let block = &program.store.blocks[IrBlockId::from_raw(words[1]).unwrap().index()];
            let arms = program.store.payload(block.instructions).unwrap();
            let bind = program.store.pattern_data[arms[4] as usize].range();
            let read = program.store.data[arms[6] as usize].range();
            assert_ne!(program.store.extra[bind.start as usize], 0, "the compiler binding is distinct from the original parameter");
            let mut coforged = program.clone();
            coforged.store.extra[bind.start as usize] = 0;
            coforged.store.extra[read.start as usize] = 0;
            let error = FullVerifier::verify_fallback_operand(&coforged.store, coforged.generic_evidence().unwrap(),
                source.instruction, source.owner, &Type::Int, None, &mut vec![source.instruction]).unwrap_err();
            assert!(error.message.contains("original source"), "{}", error.message);
            assert!(FullVerifier::verify(&coforged).is_err());
        });
    });
}
