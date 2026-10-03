use super::*;
use crate::sema::check::Checker;

#[test]
fn original_native_result_fields_in_native_test_declarations_keep_checked_success_producers() {
    crate::runtime::eval::run_eval(|| {
        for source in [
            "test inspect [fs, error] { elf.inspect(p\"plain\")?.type == \"not-elf\" }\n",
            "test uname [env, error] { system.uname()?.sysname != \"\" }\n",
            "test groups [process, env, error] { unix.id()?.groups.len() >= 0 }\n",
        ] {
            let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
                "original-native-result-fields.xsh", crate::loader::entry_source_from_text("original-native-result-fields.xsh", source.to_owned()), Vec::new());
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let source_id = SourceMap::files(&sources).first().unwrap().id();
            let declarations = Checker::check_compact_declarations(&parsed.arena);
            let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
            assert!(declarations.diagnostics.is_empty(), "{:?}", declarations.diagnostics);
            assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
            let weak = Arc::downgrade(&bodies.solved);
            let counters = bodies.solved.graph.counters().clone();
            let prepared = FullBuilder::build_compact(&parsed.arena, &declarations, &bodies, source, Arc::new(sources), source_id);
            assert_eq!(&counters, bodies.solved.graph.counters());
            let projections = bodies.solved.projections.iter().map(|(origin, projection)| (*origin, projection.field,
                graph_ground_type(&bodies.solved.graph, projection.receiver), graph_ground_type(&bodies.solved.graph, projection.result))).collect::<Vec<_>>();
            drop(parsed); drop(declarations); drop(bodies);
            assert!(weak.upgrade().is_none());
            let program = prepared.unwrap_or_else(|error| panic!("source {source:?}, original projection facts {projections:?}: {error:?}"));
            program.symbol_owner().with_current(|| FullVerifier::verify(&program).unwrap());
        }
    });
}

fn native_result_projection_fixture() -> FullProgram {
    let source = "proc inspect_projection(target: Path) [fs, error] -> Result[Bool] { elf.inspect(target)?.type == \"not-elf\" }\nproc uname_projection() [env, error] -> Result[Bool] { system.uname()?.sysname != \"\" }\nproc groups_projection() [process, env, error] -> Result[Bool] { unix.id()?.groups.len() >= 0 }\nproc other_inspect_projection(target: Path) [fs, error] -> Result[Bool] { elf.inspect(target)?.type == \"not-elf\" }\n";
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
        "native-result-projection.xsh", crate::loader::entry_source_from_text("native-result-projection.xsh", source.to_owned()), Vec::new());
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let source_id = SourceMap::files(&sources).first().unwrap().id();
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
    assert!(declarations.diagnostics.is_empty(), "{:?}", declarations.diagnostics);
    assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
    let weak = Arc::downgrade(&bodies.solved);
    let counters = bodies.solved.graph.counters().clone();
    let prepared = FullBuilder::build_compact(&parsed.arena, &declarations, &bodies, source, Arc::new(sources), source_id);
    assert_eq!(&counters, bodies.solved.graph.counters());
    drop(parsed); drop(declarations); drop(bodies);
    assert!(weak.upgrade().is_none());
    let program = prepared.unwrap();
    program.symbol_owner().with_current(|| FullVerifier::verify(&program).unwrap());
    program
}

#[test]
fn original_native_result_fields_execute_success_layouts_after_frontend_disposal_on_both_routes() {
    crate::runtime::eval::run_eval(|| {
        use std::os::unix::ffi::OsStrExt;
        use crate::runtime::eval::Evaluator;
        use crate::runtime::value::{PathValue, Value};
        let program = Arc::new(native_result_projection_fixture());
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("plain");
        std::fs::write(&file, b"plain text").unwrap();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            assert_eq!(generic.ground_projections().count(), 4);
            for (_, projection) in generic.ground_projections() {
                let source = generic.ground_projection_source(projection.source).unwrap();
                let postfix = source.postfix.as_ref().expect("Result selection retains the generated success receiver");
                assert_eq!(postfix.success_type, projection.receiver);
                assert_ne!(postfix.instruction, postfix.source_instruction);
                assert!(generic.registered_instruction_origin(postfix.instruction, false).is_none());
            }
            for recursive in [false, true] {
                for (name, arguments) in [
                    ("inspect_projection", vec![Value::Path(PathValue::new(file.as_os_str().as_bytes().to_vec()).unwrap())]),
                    ("uname_projection", vec![]),
                    ("groups_projection", vec![]),
                ] {
                    let key = LoweredFunctionKey::Name(Name::intern(name));
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Proc, &arguments, Span::new(program.store.source_id, 0, 0)).unwrap();
                    assert_eq!(crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap(), Value::ok(Value::Bool(true)), "{name}, recursive={recursive}");
                }
            }
        });
    });
}

#[test]
fn original_native_result_field_proofs_refuse_missing_foreign_and_rewritten_success_receivers() {
    crate::runtime::eval::run_eval(|| {
        let program = native_result_projection_fixture();
        let foreign = native_result_projection_fixture();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, proof) = generic.ground_projections().find(|(_, proof)| generic.ground_projection_source(proof.source).unwrap().field == "type").unwrap();
            let source = generic.ground_projection_source(proof.source).unwrap();
            let postfix = source.postfix.as_ref().unwrap();
            let (_, alternate) = generic.ground_projections().find(|(other_id, other)| *other_id != id && generic.ground_projection_source(other.source).unwrap().field == "type").unwrap();
            let alternate_source = generic.ground_projection_source(alternate.source).unwrap();
            assert_eq!(proof.receiver, alternate.receiver);
            assert_eq!(proof.result, alternate.result);
            let mut missing = program.store.clone();
            missing.generic.as_deref_mut().unwrap().test_ground_projection_source_mut(proof.source).unwrap().postfix = None;
            assert!(FullVerifier::verify_generic_evidence(&missing).is_err());
            let mut missing_producer = program.store.clone();
            missing_producer.generic.as_deref_mut().unwrap().test_remove_ground_native_calls();
            assert!(FullVerifier::verify_generic_evidence(&missing_producer).is_err(), "cached success layouts cannot replace native producer proofs");
            let mut rewritten = program.store.clone();
            let range = rewritten.data[postfix.instruction as usize].range();
            rewritten.extra[range.start as usize] = alternate_source.postfix.as_ref().unwrap().carrier;
            assert!(FullVerifier::verify_generic_evidence(&rewritten).is_err(), "a same-typed native producer cannot replace the original Result carrier");
            let mut coforged = program.store.clone();
            coforged.generic.as_deref_mut().unwrap().test_ground_projection_source_mut(proof.source).unwrap().postfix = alternate_source.postfix.clone();
            assert!(FullVerifier::verify_generic_evidence(&coforged).is_err());
            let mut changed_success = program.store.clone();
            let changed = changed_success.generic.as_deref_mut().unwrap().test_ground_projection_source_mut(proof.source).unwrap().postfix.as_mut().unwrap();
            changed.success_type = changed.error_type;
            assert!(FullVerifier::verify_generic_evidence(&changed_success).is_err());
            let mut changed_owner = program.store.clone();
            changed_owner.generic.as_deref_mut().unwrap().test_ground_projection_source_mut(proof.source).unwrap().postfix.as_mut().unwrap().owner = InstructionOwner::Driver(0);
            assert!(FullVerifier::verify_generic_evidence(&changed_owner).is_err());
            let mut foreign_source = program.store.clone();
            foreign_source.generic.as_deref_mut().unwrap().test_ground_projection_mut(id).unwrap().source = foreign.generic_evidence().unwrap().ground_projections().next().unwrap().1.source;
            assert!(FullVerifier::verify_generic_evidence(&foreign_source).is_err());
        });
    });
}
