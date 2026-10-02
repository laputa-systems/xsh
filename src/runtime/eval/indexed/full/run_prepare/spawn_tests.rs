use super::*;
use crate::sema::check::Checker;
use crate::source::SourceMap;
use crate::runtime::value::{ResultValue, Value};

const SOURCE: &str = r#"
proc started() [process,error] -> Result[ProcessHandle, ProcessError] {
    let first = spawn run --accept=[0] sh -c "exit 0"
    spawn run --accept=[0,1] sh -c "exit 1"
}
proc propagated() [process,error] -> ProcessHandle {
    let child = spawn run --accept=[0,1] sh -c "exit 1" ?
    child
}
"#;

fn prepared() -> FullProgram {
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("spawn-run-proof.xsh", crate::loader::entry_source_from_text("spawn-run-proof.xsh", SOURCE.to_owned()), Vec::new());
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let source_id = SourceMap::files(&sources).first().unwrap().id();
    let checked = Checker::check_arena(&parsed.arena, SOURCE);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    for origin in checked.solved.spawn_operations.keys() {
        assert_eq!(checked.solved.expressions.get(origin), Some(&checked.solved.operations.get(origin).unwrap().result), "a spawn expression retains its original selected result port");
    }
    let weak = Arc::downgrade(&checked.solved);
    let counters = checked.solved.graph.counters().clone();
    let declarations = Checker::compact_declarations_from_checked(&parsed.arena, &checked);
    let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
    let program = FullBuilder::build_compact(&parsed.arena, &declarations, &bodies, SOURCE, Arc::new(sources), source_id).unwrap();
    assert_eq!(checked.solved.graph.counters(), &counters);
    drop(parsed); drop(checked); drop(declarations); drop(bodies);
    assert!(weak.upgrade().is_none(), "spawn receipts keep checked target authority without the checker graph");
    FullVerifier::verify(&program).unwrap();
    assert_eq!(program.generic_evidence().unwrap().run_producers().filter(|run| run.spawn.is_some()).count(), 3);
    program
}

#[test]
fn original_spawn_run_keeps_process_handle_result_and_outer_propagation_after_disposal_on_both_workers() {
    crate::runtime::eval::run_eval(|| {
        let program = Arc::new(prepared());
        for recursive in [false, true] {
            for name in ["started", "propagated"] {
                let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(program.clone());
                let target = program.symbol_owner().with_current(|| crate::runtime::eval::LoweredFunctionKey::Name(Name::intern(name)));
                let span = Span::new(program.store.source_id, 0, 0);
                let call = || evaluator.call_indexed_direct(target, crate::runtime::eval::LoweredFunctionKind::Proc, &[], span).unwrap();
                let value = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(target, recursive, call).unwrap();
                let handle = match value {
                    Value::Result(ResultValue::Ok(value)) if name == "started" => {
                        let Value::ProcessHandle(handle) = *value else { panic!("the checked spawn result carries a process handle") }; *handle
                    }
                    Value::ProcessHandle(handle) if name == "propagated" => *handle,
                    other => panic!("spawn caller returned an incorrect checked carrier: {other:?}"),
                };
                assert!(handle.pid > 0);
                assert_eq!(handle.argv.iter().map(|word| word.as_ref()).collect::<Vec<_>>(), ["sh", "-c", "exit 1"]);
                assert_eq!(evaluator.process_handles.len(), 1, "only the returned handle crosses the restored {name} function scope on recursive route {recursive}");
                let Value::Result(ResultValue::Ok(status)) = evaluator.wait_one_process_handle(handle, span).unwrap() else { panic!("spawned child can be reaped once") };
                let Value::Status(status) = *status else { panic!("wait keeps the process status") };
                assert_eq!(status.code, Some(1));
                assert!(evaluator.process_handles.is_empty());
            }
        }
    });
}

#[test]
fn spawn_scopes_release_body_handles_before_defers_and_close_on_error_and_forced_abort_on_both_workers() {
    crate::runtime::eval::run_eval(|| {
        let source = r#"
proc observe(cleaned: Path, observed: Path) [fs,error] {
    if fs.exists(cleaned)? {
        fs.write(observed, "reaped")?
    } else {
        fs.write(observed, "live")?
    }
}
proc child(cleaned: Path, ready: Path) [process,error] -> Result[ProcessHandle] {
    let handle = spawn run sh -c "trap 'printf closed > \"$1\"; exit 0' TERM; : > \"$2\"; while :; do sleep 60; done" "cleanup-child" $cleaned $ready ?
    run --timeout=5s sh -c "while ! test -f \"$1\"; do sleep 0.001; done" "cleanup-ready" $ready ?
    handle
}
proc completed(cleaned: Path, ready: Path, observed: Path) [process,fs,error] -> Result[Int] {
    let handle = child(cleaned, ready)?
    defer observe(cleaned, observed)
    7
}
proc escaped(cleaned: Path, ready: Path, observed: Path) [process,fs,error] -> Result[ProcessHandle] {
    let handle = child(cleaned, ready)?
    defer observe(cleaned, observed)
    handle
}
proc abandoned(cleaned: Path, ready: Path, observed: Path) [process,fs,error] -> Result[ProcessHandle] {
    let handle = child(cleaned, ready)?
    defer {
        observe(cleaned, observed)?
        assert false, "deferred cleanup failure"
    }
    handle
}
proc nested(cleaned: Path, ready: Path, observed: Path) [process,fs,error] -> Result[Int] {
    if true {
        let handle = child(cleaned, ready)?
        defer observe(cleaned, observed)
    }
    7
}
proc failed(cleaned: Path, ready: Path, observed: Path) [process,fs,error] -> Result[Int] {
    let handle = child(cleaned, ready)?
    defer observe(cleaned, observed)
    assert false, "primary body failure"
    7
}
proc forced(cleaned: Path, ready: Path, observed: Path) [process,fs,error] -> Result[Int] {
    let handle = child(cleaned, ready)?
    defer observe(cleaned, observed)
    abort(7, force: true)
    7
}
"#;
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("spawn-cleanup-proof.xsh", crate::loader::entry_source_from_text("spawn-cleanup-proof.xsh", source.to_owned()), Vec::new());
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = SourceMap::files(&sources).first().unwrap().id();
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let declarations = Checker::compact_declarations_from_checked(&parsed.arena, &checked);
        let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
        let program = Arc::new(FullBuilder::build_compact(&parsed.arena, &declarations, &bodies, source, Arc::new(sources), source_id).unwrap());
        drop(parsed); drop(checked); drop(declarations); drop(bodies);
        FullVerifier::verify(&program).unwrap();
        for (recursive, name) in [false, true].into_iter().flat_map(|recursive| ["completed", "escaped", "abandoned", "nested", "failed", "forced"].map(|name| (recursive, name))) {
            use std::os::unix::ffi::OsStrExt;
            let directory = tempfile::tempdir().unwrap();
            let arguments = ["cleaned", "ready", "observed"].map(|file| Value::Path(crate::runtime::value::PathValue::new(directory.path().join(file).as_os_str().as_bytes().to_vec()).unwrap()));
            let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(program.clone());
            let target = program.symbol_owner().with_current(|| crate::runtime::eval::LoweredFunctionKey::Name(Name::intern(name)));
            let scopes = evaluator.scope_ids.clone();
            let span = Span::new(program.store.source_id, 0, 0);
            let call = || evaluator.call_indexed_direct(target, crate::runtime::eval::LoweredFunctionKind::Proc, &arguments, span).unwrap();
            let result = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(target, recursive, call);
            assert!(directory.path().join("ready").exists(), "{name} child installed its termination handler in the isolated fixture");
            match name {
                "completed" | "nested" => assert_eq!(result.unwrap(), Value::ok(Value::Int(7))),
                "escaped" => {
                    let Value::Result(ResultValue::Ok(value)) = result.unwrap() else { panic!("the escaping handle retains its checked Result carrier") };
                    let Value::ProcessHandle(handle) = *value else { panic!("the escaping value retains its process handle") };
                    assert_eq!(evaluator.process_handles.len(), 1);
                    assert!(evaluator.process_handles.contains_key(&handle.id), "the escaping child survives body cleanup");
                }
                "failed" => {
                    let error = result.unwrap_err();
                    assert!(error.propagated);
                    assert!(error.message.contains("primary body failure"));
                }
                "abandoned" => {
                    let error = result.unwrap_err();
                    assert!(error.propagated);
                    assert!(error.message.contains("deferred cleanup failure"));
                }
                "forced" => {
                    let error = result.unwrap_err();
                    let abort = error.abort.expect("the body retains forced abort control flow");
                    assert!(abort.force);
                    assert_eq!(abort.status, 7);
                }
                _ => unreachable!(),
            }
            let observed = directory.path().join("observed");
            if name == "forced" {
                assert!(!observed.exists(), "forced abort skips the defer");
            } else {
                assert_eq!(std::fs::read_to_string(observed).unwrap(), if matches!(name, "escaped" | "abandoned") { "live" } else { "reaped" }, "{name} cleanup observes the lexical child's ownership on recursive route {recursive}");
            }
            assert_eq!(evaluator.scope_ids, scopes, "{name} restores the caller's ownership scopes");
            let remaining = evaluator.process_handles.len();
            if matches!(name, "escaped" | "abandoned") {
                evaluator.cleanup_scope_process_handles(evaluator.current_scope_id(), Ok(crate::runtime::eval::Flow::Continue(Value::Unit))).unwrap();
            }
            if name == "abandoned" {
                assert_eq!(remaining, 0, "a failed defer does not transfer the abandoned return handle to its caller on recursive route {recursive}");
            }
            assert!(evaluator.process_handles.is_empty(), "{name} retains no body-owned process handle");
        }
    });
}

#[test]
fn original_spawn_run_rejects_missing_foreign_same_type_target_acceptance_and_result_before_host_effects() {
    crate::runtime::eval::run_eval(|| {
        let program = prepared();
        let runs = program.generic_evidence().unwrap().run_producers().filter(|run| run.spawn.is_some()).cloned().collect::<Vec<_>>();
        let left = &runs[0]; let right = &runs[1];
        assert_eq!(left.owner, right.owner);
        assert_eq!(left.carrier, right.carrier);
        let mut missing = program.clone();
        missing.store.generic.as_mut().unwrap().test_remove_context_producers();
        assert!(FullVerifier::verify(&missing).is_err());
        assert!(missing.generic_evidence().unwrap().run_producer_at(left.capture).is_err());
        let mut foreign = program.clone();
        foreign.store.generic.as_mut().unwrap().test_run_producer_mut(left.capture).unwrap().spawn.as_mut().unwrap().target.source = crate::source::SourceId::new(99);
        assert!(FullVerifier::verify(&foreign).is_err());
        let mut source = program.clone();
        source.store.generic.as_mut().unwrap().test_run_producer_mut(left.capture).unwrap().spawn = right.spawn.clone();
        assert!(FullVerifier::verify(&source).is_err(), "an equal ProcessHandle carrier cannot grant another source run's authority");
        let mut policy = program.clone();
        policy.store.generic.as_mut().unwrap().test_run_producer_mut(left.capture).unwrap().accept = right.accept.clone();
        assert!(FullVerifier::verify(&policy).is_err());
        let mut result = program.clone();
        result.store.generic.as_mut().unwrap().test_run_producer_mut(left.capture).unwrap().carrier = left.accept.as_ref().unwrap().ty;
        assert!(FullVerifier::verify(&result).is_err());
        let mut changed = program.clone();
        let range = changed.store.data[left.capture as usize].range().bounds(changed.store.extra.len()).unwrap();
        changed.store.extra[range.start + 8] = 0;
        assert!(FullVerifier::verify(&changed).is_err());
        let mut argv = program.clone();
        let operand = &left.operands[0];
        let range = argv.store.data[operand.instruction as usize].range().bounds(argv.store.extra.len()).unwrap();
        argv.store.extra[range.start] ^= 1;
        assert!(FullVerifier::verify(&argv).is_err(), "literal argv is independently sealed before spawning");
    });
}

#[test]
fn original_spawn_expression_retains_selected_result_port_before_lowering() {
    crate::runtime::eval::run_eval(|| {
        let source = "proc started() [process,error] -> Result[ProcessHandle, ProcessError] { spawn run --accept=[0,1] sh -c \"exit 1\" }\n";
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(41), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert_eq!(checked.solved.spawn_operations.len(), 1);
        for origin in checked.solved.spawn_operations.keys() {
            assert_eq!(checked.solved.expressions.get(origin), Some(&checked.solved.operations.get(origin).unwrap().result), "the selected spawn result retains one original source port");
        }
    });
}
