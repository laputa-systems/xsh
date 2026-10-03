use super::*;
use crate::sema::check::Checker;
use crate::runtime::value::Value;

const SOURCE: &str = r#"
proc overlay() [process,error] -> Str {
    let text = run.text STAMP="literal with spaces" sh -c "printf '%s' \"\$STAMP\"" ?
    text
}
proc input_bytes(input_path: Path, other_path: Path) [process,error] -> Bytes {
    let data = run.bytes sh -c "cat" "input-script" $other_path < ${input_path} ?
    data
}
"#;

fn prepared() -> FullProgram {
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("run-packet-proof.xsh", crate::loader::entry_source_from_text("run-packet-proof.xsh", SOURCE.to_owned()), Vec::new());
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
    assert!(weak.upgrade().is_none());
    FullVerifier::verify(&program).unwrap();
    program
}

#[test]
fn original_environment_directive_names_retain_symbol_words_independently_from_literal_string_transport() {
    crate::runtime::eval::run_eval(|| {
        let source = "proc observed() [process,error] -> Str { let line = run.text CC=cc CFLAGS=\"-O2 -pipe\" sh -c \"printf ok\" ?; line }\n";
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("env-name-packet-proof.xsh", crate::loader::entry_source_from_text("env-name-packet-proof.xsh", source.to_owned()), Vec::new());
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = SourceMap::files(&sources).first().unwrap().id();
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let declarations = Checker::compact_declarations_from_checked(&parsed.arena, &checked);
        let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
        let program = FullBuilder::build_compact(&parsed.arena, &declarations, &bodies, source, Arc::new(sources), source_id).unwrap();
        FullVerifier::verify(&program).unwrap();
        let run = program.generic_evidence().unwrap().run_producers().next().unwrap();
        let packet = run.packet.as_ref().unwrap();
        assert_eq!(packet.environment.len(), 2);
        let block = IrBlockId::from_raw(run.payload[5]).unwrap();
        let words = program.store.payload(program.store.blocks[block.index()].instructions).unwrap();
        for (entry, original) in words[1..].chunks_exact(4).zip(&packet.environment) {
            assert_eq!(entry[0], original.name.symbol().raw());
        }
        program.symbol_owner().with_current(|| {
            assert_eq!(packet.environment.iter().map(|entry| (entry.name.as_str().to_string(), entry.text.as_ref())).collect::<Vec<_>>(), [("CC".to_owned(), "cc"), ("CFLAGS".to_owned(), "-O2 -pipe")]);
        });
    });
}

#[test]
fn original_literal_environment_and_stdin_path_capture_keep_native_bytes_after_disposal_on_both_workers() {
    crate::runtime::eval::run_eval(|| {
        let program = Arc::new(prepared());
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("input with spaces");
        let contents = b"\0native\xff\n";
        std::fs::write(&input, contents).unwrap();
        use std::os::unix::ffi::OsStrExt;
        let input = Value::Path(crate::runtime::value::PathValue::new(input.as_os_str().as_bytes().to_vec()).unwrap());
        for recursive in [false, true] {
            for name in ["overlay", "input_bytes"] {
                let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(program.clone());
                let arguments = if name == "input_bytes" { vec![input.clone(), input.clone()] } else { Vec::new() };
                let target = program.symbol_owner().with_current(|| crate::runtime::eval::LoweredFunctionKey::Name(Name::intern(name)));
                let span = Span::new(program.store.source_id, 0, 0);
                let call = || evaluator.call_indexed_direct(target, crate::runtime::eval::LoweredFunctionKind::Proc, &arguments, span).unwrap();
                let value = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(target, recursive, call).unwrap();
                assert_eq!(value, if name == "overlay" { Value::Str(Arc::from("literal with spaces")) } else { Value::Bytes(contents.to_vec()) });
            }
        }
    });
}

#[test]
fn original_run_packet_refuses_changed_environment_stdin_kind_and_jointly_rewritten_execution_receipts() {
    crate::runtime::eval::run_eval(|| {
        let program = prepared();
        let runs = program.generic_evidence().unwrap().run_producers().cloned().collect::<Vec<_>>();
        let mut missing = program.clone();
        missing.store.generic.as_mut().unwrap().test_remove_context_producers();
        assert!(FullVerifier::verify(&missing).is_err());
        assert!(missing.generic_evidence().unwrap().run_producer_at(runs[0].capture).is_err());
        let env = &runs[0];
        let mut changed = program.clone();
        let block = IrBlockId::from_raw(env.payload[5]).unwrap();
        let range = changed.store.blocks[block.index()].instructions.bounds(changed.store.extra.len()).unwrap();
        changed.store.extra[range.start + 1] ^= 1;
        assert!(FullVerifier::verify(&changed).is_err());
        let input = &runs[1];
        let mut foreign = program.clone();
        let root = &mut foreign.store.generic.as_mut().unwrap().test_run_producer_mut(input.capture).unwrap().packet.as_mut().unwrap().stdin[0].root;
        let crate::runtime::eval::indexed::generic::OperationSourceOrigin::Expression(origin) = &mut root.origin else { panic!("stdin is an authored expression") };
        origin.source = crate::source::SourceId::new(99);
        assert!(FullVerifier::verify(&foreign).is_err());
        let mut sibling = program.clone();
        assert_eq!(input.arguments[0].root.original_type, input.packet.as_ref().unwrap().stdin[0].root.original_type);
        sibling.store.generic.as_mut().unwrap().test_run_producer_mut(input.capture).unwrap().packet.as_mut().unwrap().stdin[0].root = input.arguments[0].root.clone();
        assert!(FullVerifier::verify(&sibling).is_err(), "an argv Path cannot grant the original stdin Path's authority");
        let mut redirect = program.clone();
        let block = IrBlockId::from_raw(input.payload[6]).unwrap();
        let range = redirect.store.blocks[block.index()].instructions.bounds(redirect.store.extra.len()).unwrap();
        redirect.store.extra[range.start + 1] ^= 1;
        assert!(FullVerifier::verify(&redirect).is_err());
        let mut coforge = redirect;
        coforge.store.generic.as_mut().unwrap().test_run_producer_mut(input.capture).unwrap().blocks.iter_mut().find(|(id, _)| *id == block).unwrap().1[1] ^= 1;
        assert!(FullVerifier::verify(&coforge).is_err());
        assert!(coforge.generic_evidence().unwrap().run_producer_at(input.capture).is_err());
    });
}
