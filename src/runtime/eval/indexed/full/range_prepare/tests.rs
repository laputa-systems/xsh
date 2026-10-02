use super::*;

fn fixture(source: &str) -> FullProgram {
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
        "range-proof.xsh", crate::loader::entry_source_from_text("range-proof.xsh", source.to_owned()), Vec::new());
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let source_id = SourceMap::files(&sources).first().unwrap().id();
    let checked = crate::sema::check::Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let weak = Arc::downgrade(&checked.solved);
    let counters = checked.solved.graph.counters().clone();
    let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources);
    evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked).unwrap();
    assert_eq!(checked.solved.graph.counters(), &counters);
    drop(checked); drop(parsed);
    assert!(weak.upgrade().is_none());
    let program = evaluator.indexed_program.as_ref().unwrap().as_ref().clone();
    FullVerifier::verify(&program).unwrap();
    program
}

#[test]
fn range_original_unary_and_binary_arity_survive_frontend_drop() {
    crate::runtime::eval::run_eval(|| {
        let program = Arc::new(fixture("pure observed() -> Int { range(4) |> sum() }\npure sibling() -> Int { range(4, 1) |> sum() }\n"));
        let generic = program.generic_evidence().unwrap();
        let arities = generic.operations().filter_map(|(_, operation)| operation.range_lowering.as_ref().map(|lowering| lowering.arguments.len())).collect::<Vec<_>>();
        assert_eq!(arities, [1, 2]);
        for recursive in [false, true] {
            for (name, expected) in [("observed", 6), ("sibling", 9)] {
                let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(program.clone());
                let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
                let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &[], Span::new(program.store.source_id, 0, 0)).unwrap();
                let value = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call);
                assert_eq!(value.unwrap(), crate::runtime::value::Value::Int(expected));
            }
        }
    });
}

#[test]
fn range_refuses_endpoint_rewrite_erased_and_coforged_original_receipts() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture("pure observed() -> Stream[Int] { range(0, 4) }\n");
        let generic = program.generic_evidence().unwrap();
        let (id, operation) = generic.operations().find(|(_, operation)| operation.range_lowering.is_some()).unwrap();
        let instruction = generic.operation_source(operation.source).unwrap().instruction;
        let lowering = operation.range_lowering.as_ref().unwrap();
        let mut changed = program.clone();
        let range = changed.store.data[instruction as usize].range().bounds(changed.store.extra.len()).unwrap();
        changed.store.extra[range.start] = lowering.endpoints[1];
        assert!(FullVerifier::verify(&changed).is_err());
        assert!(changed.verify_range_execution(instruction).is_err());
        changed.store.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().range_lowering.as_mut().unwrap().payload[0] = lowering.endpoints[1];
        assert!(FullVerifier::verify(&changed).is_err());
        assert!(changed.verify_range_execution(instruction).is_err());
        let mut missing = program.clone();
        missing.store.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().range_lowering = None;
        assert!(FullVerifier::verify(&missing).is_err());
        assert!(missing.verify_range_execution(instruction).is_err());
        let mut whole_missing = program.clone();
        whole_missing.store.generic.as_deref_mut().unwrap().test_remove_operations();
        assert!(FullVerifier::verify(&whole_missing).is_err());
        assert!(whole_missing.verify_range_execution(instruction).is_err());
        let mut wrong_owner = program.clone();
        wrong_owner.store.generic.as_deref_mut().unwrap().test_operation_source_mut(operation.source).unwrap().owner = InstructionOwner::Driver(0);
        assert!(FullVerifier::verify(&wrong_owner).is_err());
        assert!(wrong_owner.verify_range_execution(instruction).is_err());
    });
}

#[test]
fn range_is_lazy_and_excludes_endpoints_at_integer_limits() {
    use crate::runtime::value::LiveStream;
    let mut ascending = RangeStream { cursor: Some(i64::MIN), end: i64::MAX, ascending: true };
    assert_eq!(ascending.next(Span::new(SourceId::new(0), 0, 0)).unwrap(), Some(crate::runtime::value::Value::Int(i64::MIN)));
    assert_eq!(ascending.next(Span::new(SourceId::new(0), 0, 0)).unwrap(), Some(crate::runtime::value::Value::Int(i64::MIN + 1)));
    let mut descending = RangeStream { cursor: Some(i64::MIN + 1), end: i64::MIN, ascending: false };
    assert_eq!(descending.next(Span::new(SourceId::new(0), 0, 0)).unwrap(), Some(crate::runtime::value::Value::Int(i64::MIN + 1)));
    assert_eq!(descending.next(Span::new(SourceId::new(0), 0, 0)).unwrap(), None);
    let mut empty = RangeStream { cursor: Some(i64::MAX), end: i64::MAX, ascending: true };
    assert_eq!(empty.next(Span::new(SourceId::new(0), 0, 0)).unwrap(), None);
}
