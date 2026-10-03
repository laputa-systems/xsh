use super::*;

const ALL_OPERATIONS: &str = "pure identity(value) { value }\npure duration(value: Duration) -> Duration { value }\npure count(value: Int) -> Int { value }\npure add(left: Duration, right: Duration) -> Duration { duration(identity(left + right)) }\npure subtract(left: Duration, right: Duration) -> Duration { identity(duration(left - right)) }\npure ratio(left: Duration, right: Duration) -> Int { count(identity(left / right)) }\npure divide(left: Duration, right: Int) -> Duration { duration(identity(left / right)) }\npure multiply(left: Duration, right: Int) -> Duration { identity(duration(left * right)) }\npure reverse(left: Int, right: Duration) -> Duration { duration(identity(left * right)) }\n";

fn all_operations_fixture() -> FullProgram {
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
        "duration-all-operations.xsh", crate::loader::entry_source_from_text("duration-all-operations.xsh", ALL_OPERATIONS.to_owned()), Vec::new());
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let source_id = SourceMap::files(&sources).first().unwrap().id();
    let declarations = crate::sema::check::Checker::check_compact_declarations(&parsed.arena);
    let bodies = crate::sema::check::Checker::probe_compact_bodies(&parsed.arena, &declarations);
    assert!(declarations.diagnostics.is_empty(), "{:?}", declarations.diagnostics);
    assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
    let weak = Arc::downgrade(&bodies.solved);
    let counters = bodies.solved.graph.counters().clone();
    let program = FullBuilder::build_compact(&parsed.arena, &declarations, &bodies, ALL_OPERATIONS, Arc::new(sources), source_id).unwrap();
    assert_eq!(&counters, bodies.solved.graph.counters());
    drop(parsed); drop(declarations); drop(bodies);
    assert!(weak.upgrade().is_none());
    program.symbol_owner().with_current(|| FullVerifier::verify(&program).unwrap());
    program
}

#[test]
fn duration_all_six_operations_keep_typed_and_generic_result_consumers_on_both_workers() {
    crate::runtime::eval::run_eval(|| {
        let program = Arc::new(all_operations_fixture());
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            assert_eq!(generic.operations().filter(|(_, operation)| GenericEvidenceStore::is_duration_operation(operation)).count(), 6);
            let duration = |millis| crate::runtime::value::Value::Duration(crate::runtime::value::DurationValue { millis });
            for recursive in [false, true] {
                for (name, arguments, expected) in [
                    ("add", [duration(7), duration(2)], duration(9)),
                    ("subtract", [duration(7), duration(2)], duration(5)),
                    ("ratio", [duration(7), duration(2)], crate::runtime::value::Value::Int(3)),
                    ("divide", [duration(7), crate::runtime::value::Value::Int(2)], duration(3)),
                    ("multiply", [duration(7), crate::runtime::value::Value::Int(2)], duration(14)),
                    ("reverse", [crate::runtime::value::Value::Int(2), duration(7)], duration(14)),
                ] {
                    let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let function = LoweredFunctionKey::Name(Name::intern(name));
                    let execute = || evaluator.call_indexed_direct(function, LoweredFunctionKind::Pure,
                        &arguments, Span::new(program.store.source_id, 0, 0)).expect("prepared Duration function exists");
                    let result = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(function, recursive, execute);
                    assert_eq!(result.unwrap(), expected, "function={name} recursive={recursive}");
                }
                for (name, arguments, code, expression) in [
                    ("add", [duration(u64::MAX), duration(1)], "duration-overflow", "left + right"),
                    ("subtract", [duration(0), duration(1)], "duration-underflow", "left - right"),
                    ("ratio", [duration(1), duration(0)], "division-by-zero", "left / right"),
                    ("ratio", [duration(u64::MAX), duration(1)], "integer-overflow", "left / right"),
                    ("divide", [duration(1), crate::runtime::value::Value::Int(-1)], "division-by-zero", "left / right"),
                    ("multiply", [duration(1), crate::runtime::value::Value::Int(-1)], "duration-negative-factor", "left * right"),
                    ("reverse", [crate::runtime::value::Value::Int(-1), duration(1)], "duration-negative-factor", "left * right"),
                ] {
                    let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let function = LoweredFunctionKey::Name(Name::intern(name));
                    let execute = || evaluator.call_indexed_direct(function, LoweredFunctionKind::Pure,
                        &arguments, Span::new(program.store.source_id, 0, 0)).expect("prepared Duration function exists");
                    let result = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(function, recursive, execute);
                    let error = result.expect_err("checked Duration arithmetic refuses invalid reached values");
                    assert_eq!(error.kind, code, "function={name} recursive={recursive}");
                    assert_eq!(&ALL_OPERATIONS[error.span.unwrap().range()], expression);
                }
            }
        });
    });
}

#[test]
fn duration_all_six_operations_refuse_missing_foreign_and_joint_original_operand_rewrites() {
    crate::runtime::eval::run_eval(|| {
        let program = all_operations_fixture();
        let foreign = all_operations_fixture();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let foreign_source = foreign.generic_evidence().unwrap().operations().find(|(_, operation)| GenericEvidenceStore::is_duration_operation(operation)).unwrap().1.source;
            let mut missing = program.clone();
            missing.store.generic.as_deref_mut().unwrap().test_remove_operations();
            assert!(FullVerifier::verify(&missing).is_err());
            for (id, operation) in generic.operations().filter(|(_, operation)| GenericEvidenceStore::is_duration_operation(operation)) {
                let source = generic.operation_source(operation.source).unwrap();
                let range = program.store.data[source.instruction as usize].range();
                let mut swapped = program.clone();
                swapped.store.extra.swap(range.start as usize + 1, range.start as usize + 2);
                swapped.store.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().binding.operands.swap(0, 1);
                assert!(FullVerifier::verify(&swapped).unwrap_err().message.contains("original receipt"));
                let mut foreign = program.clone();
                foreign.store.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().source = foreign_source;
                assert!(FullVerifier::verify(&foreign).is_err());
                let mut wrong_owner = program.clone();
                wrong_owner.store.generic.as_deref_mut().unwrap().test_operation_source_mut(operation.source).unwrap().owner = InstructionOwner::Driver(0);
                assert!(FullVerifier::verify(&wrong_owner).is_err());
            }
        });
    });
}

fn fixture() -> FullProgram {
    super::super::operation_prepare::tests::source_fixture(
        "pure ratio(left: Duration, right: Duration) -> Int { left / right }\nlet count = ratio(7ms, 2ms)\n",
        PreparedLanguageOperation::Arithmetic { op: BinaryOp::Div, domain: ArithmeticDomain::DurationRatio },
    )
}

#[test]
fn duration_operands_reject_same_result_domain_and_foreign_owner() {
    let program = fixture();
    let _symbols = program.symbol_owner().enter();
    let selected = program.generic_evidence().unwrap().operations().find(|(_, operation)| GenericEvidenceStore::is_duration_operation(operation)).map(|(id, _)| id).unwrap();
    let mut wrong_domain = program.clone();
    let evidence = wrong_domain.store.generic.as_deref_mut().unwrap();
    let result = evidence.operation(selected).unwrap().result;
    evidence.test_operation_mut(selected).unwrap().arguments[0] = Some(result);
    assert!(FullVerifier::verify(&wrong_domain).is_err());
    let mut wrong_owner = program.clone();
    let evidence = wrong_owner.store.generic.as_deref_mut().unwrap();
    let source = evidence.operation(selected).unwrap().source;
    evidence.test_operation_source_mut(source).unwrap().owner = InstructionOwner::Driver(0);
    assert!(FullVerifier::verify(&wrong_owner).is_err());
}

#[test]
fn duration_ratio_rejects_changed_original_operator() {
    let program = fixture();
    let _symbols = program.symbol_owner().enter();
    let operation = program.generic_evidence().unwrap().operations().find(|(_, operation)| GenericEvidenceStore::is_duration_operation(operation)).unwrap().1;
    let instruction = program.generic_evidence().unwrap().operation_source(operation.source).unwrap().instruction;
    let mut changed = program.clone();
    let words = changed.store.payload(changed.store.data[instruction as usize].range()).unwrap();
    let opcode = words[0] as usize;
    changed.store.binary_ops[opcode] = BinaryOp::Mul;
    assert!(FullVerifier::verify(&changed).is_err());
}

#[test]
fn duration_domains_survive_forwarding_and_frontend_drop_on_both_routes() {
    crate::runtime::eval::run_eval(|| {
        const SOURCE: &str = "pure ratio(left: Duration, right: Duration) -> Int { let count = left / right; count }\npure observed(base: Duration, count: Int) -> Int { let pause = count * base / count + 1ms; ratio(pause, 1ms) }\nprint ${observed(250ms, 3)}\n";
        for recursive in [false, true] {
            let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
                "duration-domain-routes.xsh", crate::loader::entry_source_from_text("duration-domain-routes.xsh", SOURCE.to_owned()), Vec::new());
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let source_id = SourceMap::files(&sources).first().unwrap().id();
            let checked = crate::sema::check::Checker::check_arena(&parsed.arena, SOURCE);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let weak = Arc::downgrade(&checked.solved);
            let counters = checked.solved.graph.counters().clone();
            let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources);
            let plan = evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked).unwrap();
            assert_eq!(checked.solved.graph.counters(), &counters);
            drop(checked); drop(parsed);
            assert!(weak.upgrade().is_none());
            let program = evaluator.indexed_program.as_ref().unwrap();
            let generic = program.generic_evidence().unwrap();
            assert_eq!(generic.operations().filter(|(_, operation)| GenericEvidenceStore::is_duration_operation(operation)).count(), 4);
            let symbols = program.symbol_owner().clone();
            let target = symbols.with_current(|| LoweredFunctionKey::Name(Name::intern("observed")));
            let execute = || symbols.with_current(|| evaluator.try_eval_installed_compact_indexed_only_inner(plan)
                .unwrap_or_else(|_| panic!("prepared Duration program remains installed")));
            let output = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(target, recursive, execute);
            assert_eq!(output.status, 0, "{:?}; {:?}", output.diagnostics, output.traceback);
            assert_eq!(output.stdout, b"251\n");
            assert!(output.stderr.is_empty()); assert!(output.diagnostics.is_empty());
        }
    });
}

#[test]
fn sealed_duration_addition_retains_its_original_authority_and_operands() {
    let program = super::super::operation_prepare::tests::source_fixture(
        "pure selected(left: Duration, right: Duration) -> Duration { let total = left + right; total - right }\nlet kept = selected(7ms, 2ms)\n",
        PreparedLanguageOperation::Arithmetic { op: BinaryOp::Sub, domain: ArithmeticDomain::DurationPair },
    );
    let _symbols = program.symbol_owner().enter();
    let generic = program.generic_evidence().unwrap();
    let (id, operation) = generic.operations().find(|(_, operation)| matches!(operation.authority,
        PreparedOperationAuthority::Sealed { operation: crate::sema::inference::SealedOperation::AddDuration })).unwrap();
    let instruction = generic.operation_source(operation.source).unwrap().instruction;
    let mut changed_operator = program.clone();
    let words = changed_operator.store.payload(changed_operator.store.data[instruction as usize].range()).unwrap();
    let opcode = words[0] as usize;
    changed_operator.store.binary_ops[opcode] = BinaryOp::Sub;
    assert!(FullVerifier::verify(&changed_operator).is_err());
    let mut changed_authority = program.clone();
    changed_authority.store.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().authority
        = PreparedOperationAuthority::Sealed { operation: crate::sema::inference::SealedOperation::AddInt };
    assert!(FullVerifier::verify(&changed_authority).is_err());
}
