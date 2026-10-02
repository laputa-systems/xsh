use super::*;
use crate::sema::check::Checker;

const SOURCE: &str = r#"
proc observed() [process, error] -> Status {
    let command = process.command_argv("sh", ["sh", "-c", "exit 1"], accept: [0,1])
    process.run(command)?
}
proc sibling() [process, error] -> Status {
    let command = process.command_argv("sh", ["sh", "-c", "exit 0"], accept: [0])
    process.run(command)?
}
"#;

fn fixture() -> FullProgram { fixture_source(SOURCE, 2) }

fn fixture_source(source: &str, expected_factories: usize) -> FullProgram {
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
        "command-argv-proof.xsh", crate::loader::entry_source_from_text("command-argv-proof.xsh", source.to_owned()), Vec::new());
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let source_id = SourceMap::files(&sources).first().unwrap().id();
    let checked = Checker::check_arena(&parsed.arena, source);
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
    assert_eq!(program.generic_evidence().unwrap().ground_native_calls().filter(|(_, proof)| proof.contract.process_command_argv.is_some()).count(), expected_factories);
    program
}

#[test]
fn command_argv_checked_policy_survives_frontend_drop_on_both_workers() {
    crate::runtime::eval::run_eval(|| {
        let program = Arc::new(fixture());
        for recursive in [false, true] {
            let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(program.clone());
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("observed")));
            let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Proc, &[], Span::new(program.store.source_id, 0, 0)).unwrap();
            let result = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call);
            let crate::runtime::value::Value::Status(status) = result.unwrap() else { panic!("command factory produces its checked process status"); };
            assert_eq!(status.kind, crate::runtime::process::ProcessStatusKind::Exit);
            assert_eq!(status.code, Some(1));
            assert!(!status.success);
            let [segment] = status.segments.as_slice() else { panic!("the original argv starts one child"); };
            assert_eq!(segment.target, b"sh");
            assert!(segment.pid.is_some_and(|pid| pid > 0));
        }
    });
}

#[test]
fn command_argv_refuses_rewritten_argv_policy_and_erased_receipts_before_effects() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture();
        let generic = program.generic_evidence().unwrap();
        let (id, proof) = generic.ground_native_calls().find(|(_, proof)| proof.contract.process_command_argv.is_some()).unwrap();
        let instruction = generic.native_call_source(proof.source).unwrap().instruction;
        let snapshot = proof.contract.process_command_argv.as_ref().unwrap();
        let argv = proof.contract.argument_sources[1].unwrap();
        let material_argv = snapshot.transports.iter().find_map(|&(read, material)| (read == argv).then_some(material)).unwrap_or(argv);
        let argv_list = snapshot.rows.iter().find(|row| row.instruction == material_argv).unwrap();
        let block_id = IrBlockId::from_raw(argv_list.payload[0]).unwrap();
        let (block, list) = snapshot.blocks.iter().find(|(id, _)| *id == block_id).unwrap();
        let mut changed_argv = program.clone();
        let range = changed_argv.store.blocks[block.index()].instructions.bounds(changed_argv.store.extra.len()).unwrap();
        changed_argv.store.extra[range.start + 1] = list[2];
        assert!(FullVerifier::verify(&changed_argv).is_err());
        assert!(changed_argv.verify_process_command_argv_execution(instruction).is_err());

        let policy = snapshot.rows.iter().find(|row| row.tag == FullTag::ExprInt).unwrap();
        let mut changed_policy = program.clone();
        let range = changed_policy.store.data[policy.instruction as usize].range().bounds(changed_policy.store.extra.len()).unwrap();
        changed_policy.store.extra[range.start] ^= 1;
        assert!(FullVerifier::verify(&changed_policy).is_err());
        assert!(changed_policy.verify_process_command_argv_execution(instruction).is_err());

        let mut missing = program.clone();
        missing.store.generic.as_deref_mut().unwrap().test_ground_native_call_mut(id).unwrap().contract.process_command_argv = None;
        assert!(FullVerifier::verify(&missing).is_err());
        assert!(missing.verify_process_command_argv_execution(instruction).is_err());
    });
}

#[test]
fn command_argv_named_timeout_keeps_source_order_defaults_and_duration_before_effects() {
    crate::runtime::eval::run_eval(|| {
        let program = Arc::new(fixture_source(r#"
proc timed() [process, error] -> Status {
    let command = process.command_argv(argv: ["sh", "-c", "exit 0"], timeout: 1s, target: "sh")
    process.run(command)?
}
"#, 1));
        let generic = program.generic_evidence().unwrap();
        let (_, proof) = generic.ground_native_calls().find(|(_, proof)| proof.contract.process_command_argv.is_some()).unwrap();
        assert_eq!(proof.contract.binding.supplied_slots.as_ref(), [1, 9, 0]);
        assert_eq!(proof.contract.binding.default_slots.as_ref(), [2, 3, 4, 5, 6, 7, 8, 10, 11, 12, 13, 14]);
        for recursive in [false, true] {
            let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(program.clone());
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("timed")));
            let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Proc, &[], Span::new(program.store.source_id, 0, 0)).unwrap();
            let result = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call);
            let crate::runtime::value::Value::Status(status) = result.unwrap() else { panic!("named timeout retains its checked process status"); };
            assert_eq!(status.code, Some(0));
            assert!(status.success);
        }
        let instruction = generic.native_call_source(proof.source).unwrap().instruction;
        let timeout = proof.contract.process_command_argv.as_ref().unwrap().rows.iter().find(|row| row.tag == FullTag::ExprDuration).unwrap();
        let mut changed = program.as_ref().clone();
        let range = changed.store.data[timeout.instruction as usize].range().bounds(changed.store.extra.len()).unwrap();
        changed.store.extra[range.start] ^= 1;
        assert!(FullVerifier::verify(&changed).is_err(), "another nonnegative Duration cannot rewrite the authored timeout");
        assert!(changed.verify_process_command_argv_execution(instruction).is_err());
    });
}
