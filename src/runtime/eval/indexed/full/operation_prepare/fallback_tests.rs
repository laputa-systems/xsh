use crate::sema::check::Checker;
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

fn unsigned_fixture() -> FullProgram {
    let source = "pure nullable(value: UInt?, fallback: Int) -> Int { let selected = value ?? fallback; selected }\npure before_later(value: UInt?, fallback: Int) -> Int { let selected = value ?? fallback; let later = 1 / 0; selected }\npure failed(value: Result[UInt, Int], fallback: Int) -> Int { let selected = value ?? fallback; selected }\nproc before_output(value: UInt?, fallback: Int) [io] -> Int { let selected = value ?? fallback; print \"later\"; selected }\n";
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
        "unsigned-fallback.xsh", crate::loader::entry_source_from_text("unsigned-fallback.xsh", source.to_owned()), Vec::new());
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let source_id = SourceMap::files(&sources).first().unwrap().id();
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    drop(checked);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
    assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
    let mut fallbacks = 0;
    for original in bodies.solved.operations.values() {
        let graph = &bodies.solved.graph;
        let Some(selected) = graph.candidate_evidence(original.requirement).unwrap() else { continue; };
        if matches!(bodies.solved.operation_catalog.candidate(graph, selected.candidate).unwrap(),
            crate::sema::check::SolvedOperationAuthority::Language(metadata)
                if metadata.operation == (PreparedLanguageOperation::Fallback { result: false })) {
            assert_eq!(graph_ground_type(graph, selected.actual_arguments[0].unwrap()).unwrap(), Type::Optional(Box::new(Type::UInt)));
            assert_eq!(graph_ground_type(graph, selected.actual_arguments[1].unwrap()).unwrap(), Type::Int);
            assert_eq!(graph_ground_type(graph, selected.result).unwrap(), Type::UInt);
            assert_eq!(graph.candidate(selected.candidate).unwrap().argument_relations[1], crate::sema::inference::ArgumentRelation::Assignable);
            let crate::sema::inference::TypeNode::Arrow(signature) = graph.node(graph.resolved(selected.signature).unwrap()).unwrap() else { panic!("fallback has its original callable signature"); };
            assert_eq!(graph_ground_type(graph, signature.params[1].ty).unwrap(), Type::UInt);
            fallbacks += 1;
        }
    }
    assert_eq!(fallbacks, 3);
    let weak = Arc::downgrade(&bodies.solved);
    let counters = bodies.solved.graph.counters().clone();
    let prepared = FullBuilder::build_compact(&parsed.arena, &declarations, &bodies, source, Arc::new(sources), source_id);
    assert_eq!(bodies.solved.graph.counters(), &counters);
    drop(parsed); drop(declarations); drop(bodies);
    assert!(weak.upgrade().is_none());
    prepared.unwrap()
}

#[test]
fn unsigned_fallback_validates_selected_value_before_later_evaluation_on_both_routes() {
    crate::runtime::eval::run_eval(|| {
        let program = Arc::new(unsigned_fixture());
        FullVerifier::verify(&program).unwrap();
        program.symbol_owner().with_current(|| {
            for recursive in [false, true] {
                for (name, value, fallback, expected) in [
                    ("nullable", Value::Null, 0, Some(Value::Int(0))),
                    ("nullable", Value::Int(7), -1, Some(Value::Int(7))),
                    ("nullable", Value::Null, -1, None),
                    ("before_later", Value::Null, -1, None),
                    ("failed", Value::Result(crate::runtime::value::ResultValue::Err(Box::new(Value::Int(9)))), 0, Some(Value::Int(0))),
                    ("failed", Value::Result(crate::runtime::value::ResultValue::Ok(Box::new(Value::Int(7)))), -1, Some(Value::Int(7))),
                    ("failed", Value::Result(crate::runtime::value::ResultValue::Err(Box::new(Value::Int(9)))), -1, None),
                    ("before_output", Value::Null, -1, None),
                    ("before_output", Value::Null, 3, Some(Value::Int(3))),
                ] {
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let target = LoweredFunctionKey::Name(Name::intern(name));
                    let arguments = [value, Value::Int(fallback)];
                    let kind = if name == "before_output" { LoweredFunctionKind::Proc } else { LoweredFunctionKind::Pure };
                    let expected_output: &[u8] = if name == "before_output" && expected.is_some() { b"later\n" } else { b"" };
                    let call = || evaluator.call_indexed_direct(target, kind, &arguments,
                        Span::new(program.store.source_id, 0, 0)).expect("unsigned fallback function exists");
                    let result = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(target, recursive, call);
                    if let Some(expected) = expected { assert_eq!(result.unwrap(), expected); }
                    else {
                        let error = result.unwrap_err();
                        assert_eq!(error.kind, "type-error");
                        assert!(error.message.contains("UInt"), "{}", error.message);
                    }
                    assert_eq!(evaluator.stdout.as_slice(), expected_output, "selected-value validation must precede the later output effect");
                }
            }
        });
    });
}


fn assert_unsigned_guard_refused(program: &FullProgram, instruction: u32, owner: InstructionOwner) {
    assert!(FullVerifier::verify(program).is_err());
    let InstructionOwner::Function(function) = owner else { panic!("the isolated fallback belongs to its authored function"); };
    let view = program.function_view_at(function.index()).unwrap();
    if program.store.generic.is_none() { assert!(view.execution().is_err()); }
    else { assert!(view.execution().unwrap().prepared_fallback(instruction).is_err()); }
}

#[test]
fn unsigned_fallback_guard_refuses_missing_foreign_and_jointly_rewritten_original_proofs() {
    crate::runtime::eval::run_eval(|| {
        let program = unsigned_fixture();
        let generic = program.generic_evidence().unwrap();
        let (id, operation) = generic.operations().find(|(_, operation)| matches!(operation.authority,
            PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Fallback { result: false }, .. })).unwrap();
        let source = generic.operation_source(operation.source).unwrap();
        let lowering = operation.fallback_lowering.as_ref().unwrap();
        let guard = super::fallback::creation_check(lowering).unwrap();
        assert_eq!(operation.arguments[1].map(|reference| match reference { TypeRef::Ground(ty) => program.store.semantic.to_type(ty).unwrap(), _ => panic!("the original right source is ground") }), Some(Type::Int));
        assert_eq!(guard.right_instruction, operation.binding.operands[1]);
        let range = program.store.data[guard.instruction as usize].range();
        let TypeRef::Ground(original_right) = operation.arguments[1].unwrap() else { unreachable!() };
        let mut changed_type = program.clone();
        changed_type.store.extra[range.start as usize + 1] = original_right.raw();
        assert_unsigned_guard_refused(&changed_type, source.instruction, source.owner);
        let mut coforged = changed_type.clone();
        let lowering = coforged.store.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().fallback_lowering.as_mut().unwrap();
        match lowering { PreparedFallbackLowering::Result { creation_check, .. } | PreparedFallbackLowering::Optional { creation_check, .. } => creation_check.as_mut().unwrap().payload[1] = original_right.raw() }
        assert_unsigned_guard_refused(&coforged, source.instruction, source.owner);
        let mut erased = program.clone();
        let lowering = erased.store.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().fallback_lowering.as_mut().unwrap();
        match lowering { PreparedFallbackLowering::Result { creation_check, .. } | PreparedFallbackLowering::Optional { creation_check, .. } => *creation_check = None }
        assert_unsigned_guard_refused(&erased, source.instruction, source.owner);
        let mut missing = program.clone();
        missing.store.generic.as_deref_mut().unwrap().test_remove_operations();
        assert_unsigned_guard_refused(&missing, source.instruction, source.owner);
        let mut missing_owner = program.clone();
        missing_owner.store.generic = None;
        assert_unsigned_guard_refused(&missing_owner, source.instruction, source.owner);
        let foreign = unsigned_fixture();
        let mut foreign_source = program.clone();
        foreign_source.store.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().source = foreign.generic_evidence().unwrap().operations().next().unwrap().1.source;
        assert_unsigned_guard_refused(&foreign_source, source.instruction, source.owner);
        let mut changed_child = program.clone();
        changed_child.store.extra[range.start as usize] = operation.binding.operands[1];
        assert_unsigned_guard_refused(&changed_child, source.instruction, source.owner);
        let InstructionOwner::Function(function) = source.owner else { unreachable!() };
        let statement = program.store.function_instruction_range(function.index()).unwrap().find(|&instruction| {
            if program.store.tags[instruction] != FullTag::StmtLet { return false; }
            let words = program.store.payload(program.store.data[instruction].range()).unwrap();
            let Some(&initializer) = words.get(1) else { return false; };
            program.store.tags.get(initializer as usize) == Some(&FullTag::ExprCheckedValue)
                && program.store.payload(program.store.data[initializer as usize].range()).unwrap().first() == Some(&guard.instruction)
        }).unwrap();
        let mut bypassed = program.clone();
        let range = bypassed.store.data[statement].range();
        bypassed.store.extra[range.start as usize + 1] = source.instruction;
        assert!(FullVerifier::verify(&bypassed).is_err(), "the original binding cannot bypass its selected-value validation boundary");
    });
}
