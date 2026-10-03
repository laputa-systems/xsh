use super::*;
use crate::runtime::eval::Evaluator;
use crate::runtime::value::Value;
use crate::sema::operation_graph::PreparedLanguageOperation;

fn on_large_stack(work: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new().stack_size(16 * 1024 * 1024).spawn(work).unwrap().join().unwrap_or_else(|payload| std::panic::resume_unwind(payload));
}

fn fixture() -> FullProgram {
    std::thread::Builder::new().stack_size(16 * 1024 * 1024).spawn(|| super::super::operation_prepare::tests::source_fixture(
        "pure measured(text: Str) -> Int { let width = text.byte_len(); width }\npure copied(text: Str) -> Int { let copy = text; let width = copy.byte_len(); width }\npure other(first: Str, second: Str) -> Int { let width = second.byte_len(); width }\npure scoped(ignored, text: Str) -> Int { let width = text.byte_len(); width }\npure invoke_scoped() -> Int { scoped(0, \"é\") }\npure comparison(left: Int, right: Int) -> Bool { left == right }\n",
        PreparedLanguageOperation::Equality { op: BinaryOp::Eq })).unwrap().join().unwrap_or_else(|payload| std::panic::resume_unwind(payload))
}

#[test]
fn folded_native_scalar_keeps_original_receiver_after_frontend_disposal_on_both_routes() {
    on_large_stack(|| {
    let program = Arc::new(fixture());
    assert_eq!(program.generic_evidence().unwrap().native_scalar_sources().count(), 4);
    for recursive in [false, true] {
        program.symbol_owner().with_current(|| {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(Arc::clone(&program));
            for (name, arguments, expected) in [
                ("measured", vec![Value::Str(Arc::from("three"))], 5),
                ("copied", vec![Value::Str(Arc::from("é"))], 2),
                ("other", vec![Value::Str(Arc::from("first")), Value::Str(Arc::from("last"))], 4),
                ("invoke_scoped", vec![], 2),
            ] {
                let key = LoweredFunctionKey::Name(Name::intern(name));
                let execute = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure,
                    &arguments, Span::new(program.store.source_id, 0, 0)).unwrap();
                let actual = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, execute);
                assert_eq!(actual.unwrap(), Value::Int(expected));
            }
        });
    }
    });
}

#[test]
fn folded_native_scalar_rejects_missing_foreign_changed_and_coforged_receivers() {
    on_large_stack(|| {
    let program = fixture();
    let foreign = fixture();
    program.symbol_owner().with_current(|| {
        let generic = program.generic_evidence().unwrap();
        let (id, source) = generic.native_scalar_sources().find(|(_, source)| matches!(source.receiver, NativeScalarReceiver::Parameter { slot: 1, .. })).unwrap();
        let mut changed = program.store.clone();
        let range = changed.data[source.instruction as usize].range();
        changed.extra[range.start as usize] = 0;
        assert!(FullVerifier::verify_generic_evidence(&changed).is_err(), "equal parameter types cannot replace the authored receiver");
        let receipt = changed.generic.as_deref_mut().unwrap().test_native_scalar_mut(id).unwrap();
        receipt.payload[0] = 0;
        if let NativeScalarReceiver::Parameter { slot, .. } = &mut receipt.receiver { *slot = 0; }
        assert!(FullVerifier::verify_generic_evidence(&changed).is_err(), "coforging physical and semantic copies cannot replace the protected source receipt");
        let mut missing = program.store.clone();
        missing.generic.as_deref_mut().unwrap().test_remove_native_scalars();
        assert!(FullVerifier::verify_generic_evidence(&missing).is_err());
        let foreign_id = foreign.generic_evidence().unwrap().native_scalar_sources().next().unwrap().0;
        assert!(generic.native_scalar_source(foreign_id).is_err());
    });
    });
}

#[test]
fn folded_native_scalar_rejects_retired_handles_and_replaced_checkpoints_atomically() {
    on_large_stack(|| {
    let program = fixture();
    program.symbol_owner().with_current(|| {
        let generic = program.generic_evidence().unwrap();
        let (foreign, source) = generic.native_scalar_sources().find(|(_, source)| matches!(source.receiver, NativeScalarReceiver::Parameter { .. })).unwrap();
        let mut builder = super::super::super::generic::GenericEvidenceBuilder::default();
        for (_, function) in generic.checked_functions() { builder.add_checked_function(*function).unwrap(); }
        builder.register_instruction_origin(source.instruction, super::super::super::generic::OperationSourceOrigin::Expression(source.origin), source.owner).unwrap();
        let checkpoint = builder.checkpoint();
        let retired = builder.add_native_scalar_source(source.clone()).unwrap();
        let stale = builder.checkpoint();
        builder.rewind(checkpoint).unwrap();
        let replacement = builder.add_native_scalar_source(source.clone()).unwrap();
        assert!(builder.rewind(stale).is_err());
        let mut store = builder.finish(&program.store.semantic, program.store.functions.len(), &program.store.generic_instruction_owners().unwrap()).unwrap();
        assert!(store.native_scalar_source(retired).is_err());
        assert!(store.native_scalar_source(foreign).is_err());
        assert!(store.native_scalar_source(replacement).is_ok());
        let bytes = store.retained_bytes();
        store.shrink_to_fit();
        assert!(store.retained_bytes() <= bytes);
        assert!(store.native_scalar_source(replacement).is_ok());
    });
    });
}

fn expression_fixture() -> FullProgram {
    super::super::operation_prepare::tests::source_fixture(
        "pure direct(text: Str) -> Int { text.byte_len() }\npure other(first: Str, second: Str) -> Int { second.byte_len() }\npure counts(text: Str) -> List[Int] { [text.byte_len(), text.count_lines(), text.count_chars(), text.count_words()] }\npure comparison(left: Int, right: Int) -> Bool { left == right }\n",
        PreparedLanguageOperation::Equality { op: BinaryOp::Eq })
}

#[test]
fn native_scalar_expression_keeps_original_receiver_and_runs_both_observed_routes() {
    on_large_stack(|| {
        let program = Arc::new(expression_fixture());
        assert_eq!(program.generic_evidence().unwrap().native_scalar_sources().count(), 6);
        for recursive in [false, true] {
            program.symbol_owner().with_current(|| {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                for (name, expected) in [("direct", Value::Int(6)), ("counts", Value::List(vec![Value::Int(6), Value::Int(1), Value::Int(5), Value::Int(3)]))] {
                    let key = LoweredFunctionKey::Name(Name::intern(name));
                    let args = [Value::Str(Arc::from("é a b"))];
                    let execute = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &args, Span::new(program.store.source_id, 0, 0)).unwrap();
                    assert_eq!(crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, execute).unwrap(), expected);
                }
            });
        }
    });
}

#[test]
fn native_scalar_expression_rejects_wrong_missing_and_foreign_original_receivers() {
    on_large_stack(|| {
        let program = expression_fixture();
        let foreign = expression_fixture();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, source) = generic.native_scalar_sources().next().unwrap();
            let alternate = generic.native_scalar_sources().find(|(_, candidate)| candidate.owner != source.owner).unwrap().1;
            let NativeScalarReceiver::ByteLengthExpression { instruction: alternate } = alternate.receiver else { panic!("another material byte length receiver"); };
            let mut changed = program.store.clone();
            let range = changed.data[source.instruction as usize].range();
            changed.extra[range.start as usize] = alternate;
            assert!(FullVerifier::verify_generic_evidence(&changed).is_err());
            let receipt = changed.generic.as_deref_mut().unwrap().test_native_scalar_mut(id).unwrap();
            receipt.payload[0] = alternate;
            receipt.receiver = NativeScalarReceiver::ByteLengthExpression { instruction: alternate };
            assert!(FullVerifier::verify_generic_evidence(&changed).is_err());
            let mut missing = program.store.clone();
            missing.generic.as_deref_mut().unwrap().test_remove_native_scalars();
            assert!(FullVerifier::verify_generic_evidence(&missing).is_err());
            assert!(generic.native_scalar_source(foreign.generic_evidence().unwrap().native_scalar_sources().next().unwrap().0).is_err());
        });
    });
}

fn byte_at_fallback_fixture() -> FullProgram {
    super::super::operation_prepare::tests::source_fixture(
        "pure sentinel(text: Str, index: Int) -> Int { let byte = text.byte_at(index) ?? -1; byte }\npure alternative(first: Str, text: Str, index: Int) -> Int { let byte = text.byte_at(index + 1) ?? 7; byte }\npure copied(text: Str, index: Int) -> Int { let copy = text; let byte = copy.byte_at(index) ?? 0; byte }\npure comparison(left: Int, right: Int) -> Bool { left == right }\n",
        PreparedLanguageOperation::Equality { op: BinaryOp::Eq })
}

#[test]
fn folded_bytes_byte_at_fallback_retains_original_bytes_domain_on_both_routes() {
    on_large_stack(|| {
        let program = Arc::new(super::super::operation_prepare::tests::source_fixture(
            "pure byte(bytes: Bytes, index: Int) -> Int { let found = bytes.byte_at(index) ?? 7; found }\npure comparison(left: Int, right: Int) -> Bool { left == right }\n",
            PreparedLanguageOperation::Equality { op: BinaryOp::Eq }));
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, source) = generic.native_scalar_sources().find(|(_, source)| source.byte_at_fallback.is_some()).expect("the original Bytes lookup retains its folded composite");
            assert_eq!(program.store.semantic.to_type(source.receiver_type).unwrap(), Type::Bytes);
            assert!(matches!(source.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::BytesByteAt, .. }));
            let mut changed = program.as_ref().clone();
            changed.store.generic.as_deref_mut().unwrap().test_native_scalar_mut(id).unwrap().contract.registry_owner = crate::sema::registry_graph::RegistryOwner::Method(crate::modules::signature::MethodReceiver::Str);
            assert!(FullVerifier::verify(&changed).is_err(), "a Bytes lookup cannot use Str receiver authority");
            for recursive in [false, true] {
                for (index, expected) in [(-1, 7), (0, 0), (1, 255), (2, 7)] {
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let key = LoweredFunctionKey::Name(Name::intern("byte"));
                    let arguments = [Value::Bytes(vec![0, 255]), Value::Int(index)];
                    let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &arguments, Span::new(program.store.source_id, 0, 0)).unwrap();
                    assert_eq!(crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap(), Value::Int(expected));
                }
            }
        });
    });
}

#[test]
fn folded_bytes_iteration_byte_at_fallback_retains_original_loop_item_authority() {
    on_large_stack(|| check_folded_bytes_iteration(
        "pure summed(lines: List[Bytes], index: Int) -> Int { var total = 0; for line in lines { let current = line.byte_at(index) ?? 7; total += current }; total }\npure comparison(left: Int, right: Int) -> Bool { left == right }\n",
        Value::List(vec![Value::Bytes(vec![0]), Value::Bytes(vec![255])]),
    ));
}

#[test]
fn folded_bytes_lines_byte_at_fallback_retains_original_loop_item_authority() {
    on_large_stack(|| check_folded_bytes_iteration(
        "pure summed(text: Bytes, index: Int) -> Int { var total = 0; for line in text.lines() { let current = line.byte_at(index) ?? 7; total += current }; total }\npure comparison(left: Int, right: Int) -> Bool { left == right }\n",
        Value::Bytes(vec![0, b'\n', 255]),
    ));
}

fn check_folded_bytes_iteration(source: &str, input: Value) {
        let program = Arc::new(super::super::operation_prepare::tests::source_fixture(
            source,
            PreparedLanguageOperation::Equality { op: BinaryOp::Eq }));
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, source) = generic.native_scalar_sources().find(|(_, source)| source.byte_at_fallback.is_some()).expect("folded lookup retains the original Bytes iteration item");
            assert_eq!(program.store.semantic.to_type(source.receiver_type).unwrap(), Type::Bytes);
            let NativeScalarReceiver::Iteration { binding, application, .. } = source.receiver else { panic!("the original loop owns the folded receiver") };
            assert_eq!(generic.iteration_binding(application).unwrap().binding, binding);
            let mut missing = program.as_ref().clone();
            missing.store.generic.as_deref_mut().unwrap().test_remove_native_scalars();
            assert!(FullVerifier::verify(&missing).is_err());
            let mut missing_iteration = program.as_ref().clone();
            missing_iteration.store.generic.as_deref_mut().unwrap().test_remove_iteration_bindings();
            assert!(FullVerifier::verify(&missing_iteration).is_err(), "the folded receiver requires its original protected iteration binding");
            let mut changed = program.as_ref().clone();
            let range = changed.store.data[source.instruction as usize].range();
            changed.store.extra[range.start as usize] = 0;
            assert!(FullVerifier::verify(&changed).is_err(), "a function parameter cannot replace the original loop item slot");
            changed.store.generic.as_deref_mut().unwrap().test_native_scalar_mut(id).unwrap().payload[0] = 0;
            assert!(FullVerifier::verify(&changed).is_err(), "rewriting the physical receipt cannot replace original loop item authority");
            let mut joint = program.as_ref().clone();
            joint.store.extra[range.start as usize] = 0;
            joint.store.generic.as_deref_mut().unwrap().test_iteration_binding_mut(application).unwrap().slot = 0;
            let scalar = joint.store.generic.as_deref_mut().unwrap().test_native_scalar_mut(id).unwrap();
            scalar.payload[0] = 0;
            scalar.receiver = NativeScalarReceiver::Iteration { binding, application, slot: 0 };
            assert!(FullVerifier::verify(&joint).is_err(), "joint scalar and iteration rewrites cannot replace original loop allocation");
            for recursive in [false, true] {
                for (index, expected) in [(0, 255), (1, 14)] {
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let key = LoweredFunctionKey::Name(Name::intern("summed"));
                    let arguments = [input.clone(), Value::Int(index)];
                    let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &arguments, Span::new(program.store.source_id, 0, 0)).unwrap();
                    assert_eq!(crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap(), Value::Int(expected));
                }
            }
        });
}

fn byte_at_expression_fixture() -> FullProgram {
    super::super::operation_prepare::tests::source_fixture(
        "pure nullable(first: Str, text: Str, earlier: Int, index: Int) -> Int? { let previous = first.byte_at(earlier); let byte = text.byte_at(index); byte }\npure copied(text: Str, index: Int) -> Int? { let copy = text; copy.byte_at(index) }\npure comparison(left: Int, right: Int) -> Bool { left == right }\n",
        PreparedLanguageOperation::Equality { op: BinaryOp::Eq })
}

#[test]
fn original_byte_at_expression_keeps_nullable_result_after_frontend_disposal_on_both_routes() {
    on_large_stack(|| {
        let program = Arc::new(byte_at_expression_fixture());
        assert_eq!(program.generic_evidence().unwrap().ground_native_calls().filter(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::TextByteAt, .. })).count(), 3);
        for recursive in [false, true] {
            program.symbol_owner().with_current(|| {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(program.clone());
                for (name, arguments, expected) in [
                    ("nullable", vec![Value::Str(Arc::from("first")), Value::Str(Arc::from("é")), Value::Int(1), Value::Int(0)], Value::Int(195)),
                    ("nullable", vec![Value::Str(Arc::from("first")), Value::Str(Arc::from("é")), Value::Int(0), Value::Int(1)], Value::Int(169)),
                    ("nullable", vec![Value::Str(Arc::from("first")), Value::Str(Arc::from("é")), Value::Int(0), Value::Int(-1)], Value::Null),
                    ("nullable", vec![Value::Str(Arc::from("first")), Value::Str(Arc::from("é")), Value::Int(0), Value::Int(2)], Value::Null),
                    ("copied", vec![Value::Str(Arc::from("é")), Value::Int(0)], Value::Int(195)),
                    ("copied", vec![Value::Str(Arc::from("é")), Value::Int(2)], Value::Null),
                ] {
                    let key = LoweredFunctionKey::Name(Name::intern(name));
                    let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &arguments, Span::new(program.store.source_id, 0, 0)).unwrap();
                    assert_eq!(crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap(), expected);
                }
            });
        }
    });
}

#[test]
fn original_byte_at_expression_refuses_receiver_index_result_missing_foreign_and_coforged_receipts() {
    on_large_stack(|| {
        let program = byte_at_expression_fixture();
        let foreign = byte_at_expression_fixture();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let calls = generic.ground_native_calls().filter(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::TextByteAt, .. })).collect::<Vec<_>>();
            let (id, proof) = calls[0];
            let source = generic.native_call_source(proof.source).unwrap();
            let alternate = calls.iter().find(|(_, candidate)| generic.native_call_source(candidate.source).unwrap().owner == source.owner && candidate.source != proof.source).unwrap().1;
            let range = program.store.data[source.instruction as usize].range();
            for word in 0..2 {
                let mut changed = program.store.clone();
                changed.extra[range.start as usize + word] = alternate.contract.argument_sources[word].unwrap();
                assert!(FullVerifier::verify_generic_evidence(&changed).is_err(), "an equal typed operand cannot replace the original receiver or index expression");
            }
            let mut result = program.store.clone();
            let wrong_result = proof.contract.arguments[0].ty;
            result.generic.as_deref_mut().unwrap().test_ground_native_call_mut(id).unwrap().contract.result = wrong_result;
            assert!(FullVerifier::verify_generic_evidence(&result).is_err());
            let mut coforged = program.store.clone();
            let replacement = alternate.contract.argument_sources[1].unwrap();
            coforged.extra[range.start as usize + 1] = replacement;
            let evidence = coforged.generic.as_deref_mut().unwrap();
            let application = evidence.test_ground_native_call_mut(id).unwrap();
            application.contract.arguments[0].instruction = replacement;
            application.contract.binding.operands[0] = replacement;
            application.contract.argument_sources[1] = Some(replacement);
            let altered = application.contract.clone();
            evidence.test_native_call_source_mut(proof.source).unwrap().expected = altered;
            assert!(FullVerifier::verify_generic_evidence(&coforged).unwrap_err().message.contains("original receipt"));
            let mut missing = program.store.clone();
            missing.generic.as_deref_mut().unwrap().test_remove_ground_native_calls();
            assert!(FullVerifier::verify_generic_evidence(&missing).is_err());
            assert!(generic.native_call_source(foreign.generic_evidence().unwrap().ground_native_calls().next().unwrap().1.source).is_err());
        });
    });
}

#[test]
fn folded_byte_at_fallback_preserves_nullable_call_index_and_literal_on_both_routes() {
    on_large_stack(|| {
        let program = Arc::new(byte_at_fallback_fixture());
        assert_eq!(program.generic_evidence().unwrap().native_scalar_sources().filter(|(_, source)| source.byte_at_fallback.is_some()).count(), 3);
        for recursive in [false, true] {
            program.symbol_owner().with_current(|| {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(program.clone());
                for (name, arguments, expected) in [
                    ("sentinel", vec![Value::Str(Arc::from("é")), Value::Int(0)], 195),
                    ("sentinel", vec![Value::Str(Arc::from("é")), Value::Int(-1)], -1),
                    ("sentinel", vec![Value::Str(Arc::from("é")), Value::Int(2)], -1),
                    ("alternative", vec![Value::Str(Arc::from("first")), Value::Str(Arc::from("é")), Value::Int(0)], 169),
                    ("alternative", vec![Value::Str(Arc::from("first")), Value::Str(Arc::from("é")), Value::Int(1)], 7),
                    ("copied", vec![Value::Str(Arc::from("é")), Value::Int(5)], 0),
                ] {
                    let key = LoweredFunctionKey::Name(Name::intern(name));
                    let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &arguments, Span::new(program.store.source_id, 0, 0)).unwrap();
                    assert_eq!(crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap(), Value::Int(expected));
                }
            });
        }
    });
}

#[test]
fn folded_byte_at_fallback_rejects_receiver_index_literal_and_joint_receipt_rewrites() {
    on_large_stack(|| {
        let program = byte_at_fallback_fixture();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, source) = generic.native_scalar_sources().find(|(_, source)| source.byte_at_fallback.as_ref().is_some_and(|composite| composite.fallback_value == 7)).unwrap();
            let composite = source.byte_at_fallback.as_ref().unwrap();
            let foreign = generic.native_scalar_sources().find(|(_, candidate)| candidate.owner != source.owner && candidate.byte_at_fallback.is_some()).unwrap().1.byte_at_fallback.as_ref().unwrap();
            let mut receiver = program.store.clone();
            let range = receiver.data[source.instruction as usize].range();
            receiver.extra[range.start as usize] = 0;
            assert!(FullVerifier::verify_generic_evidence(&receiver).is_err());
            let mut index = program.store.clone();
            index.extra[range.start as usize + 1] = foreign.index_instruction;
            assert!(FullVerifier::verify_generic_evidence(&index).is_err());
            let default = composite.fallback_instruction.unwrap();
            let mut literal = program.store.clone();
            let default_range = literal.data[default as usize].range();
            literal.extra[default_range.start as usize] = 8;
            assert!(FullVerifier::verify_generic_evidence(&literal).is_err());
            literal.generic.as_deref_mut().unwrap().test_native_scalar_mut(id).unwrap().byte_at_fallback.as_mut().unwrap().fallback_value = 8;
            assert!(FullVerifier::verify_generic_evidence(&literal).is_err(), "rewritten literal and composite cannot replace original source selection");
            let mut erased = program.store.clone();
            erased.generic.as_deref_mut().unwrap().test_native_scalar_mut(id).unwrap().byte_at_fallback = None;
            assert!(FullVerifier::verify_generic_evidence(&erased).is_err());
        });
    });
}
