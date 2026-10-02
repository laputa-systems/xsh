use super::*;
use crate::sema::check::Checker;
use crate::runtime::value::{ResultValue, Value};

const SOURCE: &str = r#"
proc captured(target_path: Path) [process] -> Result[Str, ProcessError] {
    run.text sh -c "printf '%s' \"$1\"" "dynamic-path" $target_path
}
proc started(left: Path, right: Path) [process] -> Result[ProcessHandle, ProcessError] {
    spawn run sh -c "test \"$1\" = \"$2\"" "dynamic-path" $left $right
}
proc expanded(values: List[Path]) [process] -> Result[Str, ProcessError] {
    run.text printf "<%s>" $values
}
"#;

fn prepared() -> FullProgram {
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("dynamic-run-proof.xsh", crate::loader::entry_source_from_text("dynamic-run-proof.xsh", SOURCE.to_owned()), Vec::new());
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let source_id = SourceMap::files(&sources).first().unwrap().id();
    let checked = Checker::check_arena(&parsed.arena, SOURCE);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let weak = Arc::downgrade(&checked.solved);
    let counters = checked.solved.graph.counters().clone();
    let declarations = Checker::compact_declarations_from_checked(&parsed.arena, &checked);
    let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
    let program = FullBuilder::build_compact(&parsed.arena, &declarations, &bodies, SOURCE, Arc::new(sources), source_id).unwrap();
    assert_eq!(checked.solved.graph.counters(), &counters);
    drop(parsed); drop(checked); drop(declarations); drop(bodies);
    assert!(weak.upgrade().is_none(), "rendering receipts retain original authority without the checker graph");
    FullVerifier::verify(&program).unwrap();
    assert_eq!(program.generic_evidence().unwrap().run_producers().filter(|run| !run.arguments.is_empty()).count(), 3);
    program
}

#[test]
fn checked_dynamic_path_argv_preserves_word_boundaries_and_expansion_after_disposal_on_both_workers() {
    crate::runtime::eval::run_eval(|| {
        let program = Arc::new(prepared());
        let path = Value::Path(crate::runtime::value::PathValue::new(b"path with spaces".to_vec()).unwrap());
        let other = Value::Path(crate::runtime::value::PathValue::new(b"second path".to_vec()).unwrap());
        for recursive in [false, true] {
            for name in ["captured", "started", "expanded"] {
                let arguments = match name { "started" => vec![path.clone(), path.clone()], "expanded" => vec![Value::List(vec![path.clone(), other.clone()])], _ => vec![path.clone()] };
                let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(program.clone());
                let target = program.symbol_owner().with_current(|| crate::runtime::eval::LoweredFunctionKey::Name(Name::intern(name)));
                let span = Span::new(program.store.source_id, 0, 0);
                let call = || evaluator.call_indexed_direct(target, crate::runtime::eval::LoweredFunctionKind::Proc, &arguments, span).unwrap();
                let value = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(target, recursive, call).unwrap();
                let Value::Result(ResultValue::Ok(value)) = value else { panic!("checked process result remains Ok") };
                if name == "started" {
                    let Value::ProcessHandle(handle) = *value else { panic!("spawn keeps its handle result") };
                    assert_eq!(handle.argv.iter().map(|word| word.as_ref()).collect::<Vec<_>>(), ["sh", "-c", "test \"$1\" = \"$2\"", "dynamic-path", "path with spaces", "path with spaces"]);
                    let Value::Result(ResultValue::Ok(status)) = evaluator.wait_one_process_handle(*handle, span).unwrap() else { panic!("dynamic child can be consumed once") };
                    let Value::Status(status) = *status else { panic!("wait keeps status data") };
                    assert_eq!(status.code, Some(0));
                    assert!(evaluator.process_handles.is_empty());
                } else {
                    let Value::Str(text) = *value else { panic!("capture keeps checked text") };
                    assert_eq!(text.as_ref(), if name == "expanded" { "<path with spaces><second path>" } else { "path with spaces" });
                }
            }
        }
    });
}

#[test]
fn checked_dynamic_argv_rejects_missing_foreign_same_type_and_jointly_changed_word_authority_before_execution() {
    crate::runtime::eval::run_eval(|| {
        let program = prepared();
        let run = program.generic_evidence().unwrap().run_producers().find(|run| run.spawn.is_some()).unwrap().clone();
        assert_eq!(run.arguments.len(), 2);
        assert_eq!(run.arguments[0].root.original_type, run.arguments[1].root.original_type);
        let mut missing = program.clone();
        missing.store.generic.as_mut().unwrap().test_remove_context_producers();
        assert!(FullVerifier::verify(&missing).is_err());
        assert!(missing.generic_evidence().unwrap().run_producer_at(run.capture).is_err());
        let mut foreign = program.clone();
        let argument = &mut foreign.store.generic.as_mut().unwrap().test_run_producer_mut(run.capture).unwrap().arguments[0];
        let super::super::super::super::generic::OperationSourceOrigin::Expression(origin) = &mut argument.root.origin else { panic!("authored expression") };
        origin.source = crate::source::SourceId::new(99);
        assert!(FullVerifier::verify(&foreign).is_err());
        let mut sibling = program.clone();
        sibling.store.generic.as_mut().unwrap().test_run_producer_mut(run.capture).unwrap().arguments[0].root = run.arguments[1].root.clone();
        assert!(FullVerifier::verify(&sibling).is_err(), "another Path cannot replace the original word authority");
        let mut mode = program.clone();
        mode.store.generic.as_mut().unwrap().test_run_producer_mut(run.capture).unwrap().arguments[0].mode = crate::sema::check::RunArgumentMode::Splice;
        assert!(FullVerifier::verify(&mode).is_err());
        let mut executable = program.clone();
        let block = IrBlockId::from_raw(run.payload[3]).unwrap();
        let range = executable.store.blocks[block.index()].instructions.bounds(executable.store.extra.len()).unwrap();
        let word = run.arguments[0].word as usize;
        executable.store.extra[range.start + 1 + (word - 1) * 3 + 1] = run.arguments[1].root.instruction;
        assert!(FullVerifier::verify(&executable).is_err());
        let mut coforge = executable;
        coforge.store.generic.as_mut().unwrap().test_run_producer_mut(run.capture).unwrap().arguments[0].root = run.arguments[1].root.clone();
        assert!(FullVerifier::verify(&coforge).is_err());
        assert!(coforge.generic_evidence().unwrap().run_producer_at(run.capture).is_err(), "runtime refuses coforged receipts before argv evaluation");
    });
}
