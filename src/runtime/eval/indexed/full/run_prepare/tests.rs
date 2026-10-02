use super::*;
use crate::sema::check::Checker;
use crate::source::SourceMap;
use crate::runtime::value::Value;

fn fixture() -> FullProgram {
    let source = "proc checked() [process,error] -> Str { let text = run.text --accept=[0,1] sh -c \"printf accepted; exit 1\" ?; text }\nproc rejected() [process,error] -> Str { let text = run.text --accept=[0] sh -c \"exit 1\" ?; text }\n";
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("run-policy-proof.xsh", crate::loader::entry_source_from_text("run-policy-proof.xsh", source.to_owned()), Vec::new());
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let source_id = SourceMap::files(&sources).first().unwrap().id();
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let weak = Arc::downgrade(&checked.solved);
    let declarations = Checker::compact_declarations_from_checked(&parsed.arena, &checked);
    let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
    let program = FullBuilder::build_compact(&parsed.arena, &declarations, &bodies, source, Arc::new(sources), source_id).unwrap();
    drop(parsed); drop(checked); drop(declarations); drop(bodies);
    assert!(weak.upgrade().is_none(), "run policy authority must survive without the checker graph");
    FullVerifier::verify(&program).unwrap();
    assert_eq!(program.generic_evidence().unwrap().run_producers().filter(|run| run.accept.is_some()).count(), 2);
    program
}

#[test]
fn literal_run_acceptance_preserves_checked_success_and_failure_after_disposal_on_both_workers() {
    crate::runtime::eval::run_eval(|| {
        let program = Arc::new(fixture());
        for recursive in [false, true] {
            for name in ["checked", "rejected"] {
                let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(program.clone());
                let name_id = program.symbol_owner().with_current(|| Name::intern(name));
                let key = crate::runtime::eval::LoweredFunctionKey::Name(name_id);
                let call = || evaluator.call_indexed_direct(key, crate::runtime::eval::LoweredFunctionKind::Proc, &[], Span::new(program.store.source_id, 0, 0)).unwrap();
                let result = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call);
                if name == "checked" { assert_eq!(result.unwrap(), Value::Str(Arc::from("accepted"))); }
                else {
                    let error = result.expect_err("a plain Str proc propagates rejected process completion");
                    assert_eq!(error.family, "ProcessError");
                    assert!(error.propagated);
                    assert!(error.propagated_run_error.is_some(), "the original process failure remains available at the proc boundary");
                }
            }
        }
    });
}

#[test]
fn literal_run_acceptance_rejects_missing_foreign_rewritten_and_equal_type_policy_receipts_before_effects() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture();
        let runs = program.generic_evidence().unwrap().run_producers().cloned().collect::<Vec<_>>();
        let left = &runs[0]; let right = &runs[1];
        let mut missing = program.clone();
        missing.store.generic.as_mut().unwrap().test_remove_context_producers();
        assert!(FullVerifier::verify(&missing).is_err(), "removing visible receipts cannot retire original run policy authority");
        assert!(missing.generic_evidence().unwrap().run_producer_at(left.capture).is_err(), "execution lookup refuses a removed original receipt before argv evaluation");
        let mut foreign = program.clone();
        let receipt = foreign.store.generic.as_mut().unwrap().test_run_producer_mut(left.capture).unwrap();
        let crate::runtime::eval::indexed::generic::OperationSourceOrigin::Expression(origin) = &mut receipt.accept.as_mut().unwrap().origin else { unreachable!() };
        origin.source = crate::source::SourceId::new(123);
        assert!(FullVerifier::verify(&foreign).is_err());
        assert!(foreign.generic_evidence().unwrap().run_producer_at(left.capture).is_err());
        let mut rewritten = program.clone();
        rewritten.store.generic.as_mut().unwrap().test_run_producer_mut(left.capture).unwrap().accept = right.accept.clone();
        assert!(FullVerifier::verify(&rewritten).is_err(), "same checked List[Int] cannot grant a sibling its acceptance source");
        assert!(rewritten.generic_evidence().unwrap().run_producer_at(left.capture).is_err());
        let mut changed = program.clone();
        let operand = left.operands.last().unwrap();
        let range = changed.store.data[operand.instruction as usize].range().bounds(changed.store.extra.len()).unwrap();
        changed.store.extra[range.start] ^= 1;
        assert!(FullVerifier::verify(&changed).is_err(), "changing an accepted exit code retains its type but changes process policy");
        let mut effects = program.clone();
        effects.store.generic.as_mut().unwrap().test_run_producer_mut(left.capture).unwrap().effects = crate::sema::inference::EffectSet::EMPTY;
        assert!(FullVerifier::verify(&effects).is_err());
    });
}
