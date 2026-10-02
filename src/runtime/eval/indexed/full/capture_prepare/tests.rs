use super::*;
use crate::sema::check::Checker;

const SOURCE: &str = r#"
pure produce(fail: Bool) -> Result[Int, Str] {
    if fail { Err("failed") } else { Ok(7) }
}
pure report(value: Result[Int, Str]) -> Bool {
    match value { Ok(_) => false, Err(_) => true }
}
proc observed(fail: Bool) [error] -> Bool {
    let first = try { produce(fail)? }
    let second = try { produce(false)? }
    report(first)
}
print ${observed(false)}
print ${observed(true)}
"#;

fn prepared() -> FullProgram {
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
        "try-capture-evidence.xsh", crate::loader::entry_source_from_text("try-capture-evidence.xsh", SOURCE.to_owned()), Vec::new());
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let source_id = SourceMap::files(&sources).first().unwrap().id();
    let checked = Checker::check_arena(&parsed.arena, SOURCE);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let weak = Arc::downgrade(&checked.solved);
    let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources);
    evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked).unwrap();
    drop(checked); drop(parsed);
    assert!(weak.upgrade().is_none());
    evaluator.indexed_program.as_ref().unwrap().as_ref().clone()
}

#[test]
fn original_try_capture_carriers_survive_frontend_drop_on_both_routes() {
    crate::runtime::eval::run_eval(|| {
        for recursive in [false, true] {
            let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
                "try-capture-routes.xsh", crate::loader::entry_source_from_text("try-capture-routes.xsh", SOURCE.to_owned()), Vec::new());
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let source_id = SourceMap::files(&sources).first().unwrap().id();
            let checked = Checker::check_arena(&parsed.arena, SOURCE);
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
            assert_eq!(generic.try_capture_sources().count(), 2);
            for (_, source) in generic.try_capture_sources() {
                assert_eq!(program.store.semantic.to_type(source.carrier).unwrap(), Type::Result(Box::new(Type::Int), Box::new(Type::Str)));
                assert_eq!(program.store.semantic.to_type(source.completion).unwrap(), Type::Int);
                assert_eq!(source.propagation, Some(source.carrier));
            }
            let symbols = program.symbol_owner().clone();
            let target = symbols.with_current(|| LoweredFunctionKey::Name(Name::intern("observed")));
            let execute = || symbols.with_current(|| evaluator.try_eval_installed_compact_indexed_only_inner(plan)
                .unwrap_or_else(|_| panic!("prepared try capture program remains installed")));
            let output = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(target, recursive, execute);
            assert_eq!(output.status, 0, "{:?}; {:?}", output.diagnostics, output.traceback);
            assert_eq!(output.stdout, b"false\ntrue\n");
            assert!(output.stderr.is_empty()); assert!(output.diagnostics.is_empty());
        }
    });
}

#[test]
fn try_capture_rejects_equal_type_body_substitution_and_foreign_receipts() {
    crate::runtime::eval::run_eval(|| {
        let program = prepared();
        let _symbols = program.symbol_owner().enter();
        let generic = program.generic_evidence().unwrap();
        let sources = generic.try_capture_sources().collect::<Vec<_>>();
        let (source_id, source) = sources[0];
        let (_, other) = sources[1];
        assert_eq!(source.owner, other.owner);
        assert_eq!(source.carrier, other.carrier);

        let mut changed_body = program.clone();
        let range = changed_body.store.data[source.instruction as usize].range().bounds(changed_body.store.extra.len()).unwrap();
        changed_body.store.extra[range.start] = other.body;
        assert!(FullVerifier::verify(&changed_body).is_err(), "another same-owner body cannot replace the original error boundary");

        let mut changed_completion = program.clone();
        let range = changed_completion.store.data[source.statement as usize].range().bounds(changed_completion.store.extra.len()).unwrap();
        changed_completion.store.extra[range.start] = other.tail;
        assert!(FullVerifier::verify(&changed_completion).is_err(), "equal completion types do not authorize another tail expression");

        let mut changed_producer = program.clone();
        let material = source.producer_source.unwrap();
        let other_material = other.producer_source.unwrap();
        let original = changed_producer.store.data[material as usize].range().bounds(changed_producer.store.extra.len()).unwrap();
        let replacement = program.store.payload(program.store.data[other_material as usize].range()).unwrap();
        assert_eq!(original.len(), replacement.len());
        changed_producer.store.extra[original].copy_from_slice(replacement);
        assert!(FullVerifier::verify(&changed_producer).is_err(), "same-result calls retain their original encoded operands");

        let mut coupled = program.clone();
        coupled.store.generic.as_mut().unwrap().test_try_capture_source_mut(source_id).unwrap().body = other.body;
        assert!(FullVerifier::verify(&coupled).is_err(), "the original capture receipt is sealed independently of encoded operands");

        let foreign = prepared();
        assert!(foreign.generic_evidence().unwrap().try_capture_source(source_id).is_err());
        let wrong_owner = match source.owner { InstructionOwner::Function(_) => InstructionOwner::Driver(0), InstructionOwner::Driver(_) => InstructionOwner::Function(IrFunctionId::new(0).unwrap()) };
        assert!(FullVerifier::verify_try_capture_operand(&program.store, generic, source.instruction, wrong_owner,
            &program.store.semantic.to_type(source.carrier).unwrap()).is_err());
    });
}
