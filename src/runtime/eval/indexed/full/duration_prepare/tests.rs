use super::*;

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
