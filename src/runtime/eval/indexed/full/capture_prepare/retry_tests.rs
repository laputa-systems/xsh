use super::*;
use crate::sema::check::Checker;

const SOURCE: &str = r#"
error RetryError = Busy(message: Str) | Fatal(message: Str)
proc attempt() [io] -> Result[Int, RetryError] {
    print "attempt"
    Err(RetryError.Busy("busy"))
}
proc observed() [io,time] -> Result[Int, RetryError] {
    retry [0ms, 0ms] on (RetryError.Busy) {
        defer { print "cleanup" }
        attempt()?
    }
}
pure report(value: Result[Int, RetryError]) -> Bool {
    match value { Ok(_) => false, Err(RetryError.Busy) => true, Err(_) => false }
}
print ${report(observed())}
"#;

fn prepared(source: &str) -> FullProgram {
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("retry-proof.xsh", crate::loader::entry_source_from_text("retry-proof.xsh", source.to_owned()), Vec::new());
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let source_id = SourceMap::files(&sources).first().unwrap().id();
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let weak = Arc::downgrade(&checked.solved);
    let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources);
    evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked).unwrap();
    drop(checked); drop(parsed);
    assert!(weak.upgrade().is_none(), "retry receipts must not retain frontend graph authority");
    evaluator.indexed_program.as_ref().unwrap().as_ref().clone()
}

#[test]
fn original_retry_capture_preserves_attempt_defer_and_nominal_error_after_frontend_drop_on_both_workers() {
    crate::runtime::eval::run_eval(|| {
        for recursive in [false, true] {
            let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("retry-routes.xsh", crate::loader::entry_source_from_text("retry-routes.xsh", SOURCE.to_owned()), Vec::new());
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
            let (_, source) = generic.try_capture_sources().find(|(_, source)| source.retry.is_some()).unwrap();
            let symbols = program.symbol_owner().clone();
            let expected = symbols.with_current(|| Type::Result(Box::new(Type::Int), Box::new(Type::ErrorFamily(Name::intern("RetryError")))));
            assert_eq!(source.original_carrier, expected);
            assert_eq!(source.retry.as_ref().unwrap().delays.len(), 2);
            assert_eq!(source.body_words[0], 2, "defer registration and completion remain in the actual attempt body");
            let target = symbols.with_current(|| LoweredFunctionKey::Name(Name::intern("observed")));
            let execute = || symbols.with_current(|| evaluator.try_eval_installed_compact_indexed_only_inner(plan).unwrap_or_else(|_| panic!("prepared retry program remains installed")));
            let output = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(target, recursive, execute);
            assert_eq!(output.status, 0, "{:?}; {:?}", output.diagnostics, output.traceback);
            assert_eq!(output.stdout, b"attempt\ncleanup\nattempt\ncleanup\nattempt\ncleanup\ntrue\n");
            assert!(output.stderr.is_empty()); assert!(output.diagnostics.is_empty());
        }
    });
}

#[test]
fn original_retry_capture_rejects_missing_foreign_altered_delay_selection_and_body_receipts() {
    crate::runtime::eval::run_eval(|| {
        let program = prepared(SOURCE);
        let generic = program.generic_evidence().unwrap();
        let (id, source) = generic.try_capture_sources().find(|(_, source)| source.retry.is_some()).unwrap();
        let policy = source.retry.as_ref().unwrap();
        let mut missing = program.clone();
        missing.store.generic.as_mut().unwrap().test_remove_try_capture_sources();
        assert!(FullVerifier::verify(&missing).is_err());
        assert!(missing.generic_evidence().unwrap().try_capture_source_at(source.instruction).is_err());
        let foreign = prepared(SOURCE);
        assert!(foreign.generic_evidence().unwrap().try_capture_source(id).is_err());
        let mut changed = program.clone();
        changed.store.generic.as_mut().unwrap().test_try_capture_source_mut(id).unwrap().retry.as_mut().unwrap().delays[0].origin.source = SourceId::new(99);
        assert!(FullVerifier::verify(&changed).is_err());
        let mut delay = program.clone();
        let first = &policy.delays[0];
        let words = delay.store.data[first.instruction as usize].range().bounds(delay.store.extra.len()).unwrap();
        delay.store.extra[words.start] ^= 1;
        assert!(FullVerifier::verify(&delay).is_err(), "changing one Duration retains its type but changes original attempt policy");
        let mut selection = program.clone();
        selection.store.generic.as_mut().unwrap().test_try_capture_source_mut(id).unwrap().retry.as_mut().unwrap().selection = None;
        assert!(FullVerifier::verify(&selection).is_err());
        let mut body = program.clone();
        let block = IrBlockId::from_raw(source.body).unwrap();
        let words = body.store.blocks[block.index()].instructions.bounds(body.store.extra.len()).unwrap();
        body.store.extra[words.start + 1] = source.statement;
        assert!(FullVerifier::verify(&body).is_err(), "completion cannot replace the registered defer in an attempt");
    });
}

#[test]
fn original_selective_retry_fixture_builds_with_checked_carrier_before_pattern_refusal_checks() {
    crate::runtime::eval::run_eval(|| {
        let source = include_str!("../../../../../../tests/fixtures/frontend-indexed/selective-retry.xsh");
        let program = prepared(source);
        FullVerifier::verify(&program).unwrap();
        assert_eq!(program.generic_evidence().unwrap().try_capture_sources().filter(|(_, source)| source.retry.is_some()).count(), 1);
    });
}
