use super::*;
use crate::runtime::eval::Evaluator;
use crate::runtime::value::{RecordMap, Value};
use crate::sema::check::Checker;

const SOURCE: &str = "error Failure = Failed(message: Str) : InvalidData | Other(message: Str)\npure erased(value: Error) -> Str { value.message }\npure family(value: Failure) -> Str { value.message }\npure facet(value: InvalidData) -> Str { value.message }\npure process_message(value: ProcessError) -> Str { value.message }\npure exact(text: Str) -> Str { Failure.Failed(message: text).message }\npure compare(first: Error, second: Error) -> Bool { first.message == second.message }\n";

fn fixture() -> FullProgram {
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
        "original-error-field.xsh", crate::loader::entry_source_from_text("original-error-field.xsh", SOURCE.to_owned()), Vec::new());
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let source_id = SourceMap::files(&sources).first().unwrap().id();
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
    assert!(declarations.diagnostics.is_empty(), "{:?}", declarations.diagnostics);
    assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
    let weak = Arc::downgrade(&bodies.solved);
    let counters = bodies.solved.graph.counters().clone();
    let program = FullBuilder::build_compact(&parsed.arena, &declarations, &bodies, SOURCE, Arc::new(sources), source_id).unwrap();
    assert_eq!(&counters, bodies.solved.graph.counters());
    drop(parsed); drop(declarations); drop(bodies);
    assert!(weak.upgrade().is_none());
    program
}

#[test]
fn original_error_field_executes_exact_typed_receivers_on_both_routes_after_frontend_disposal() {
    crate::runtime::eval::run_eval(|| {
        let program = Arc::new(fixture());
        program.symbol_owner().with_current(|| {
            FullVerifier::verify(&program).unwrap();
            let generic = program.generic_evidence().unwrap();
            let source_id = SourceMap::files(&program.sources).first().unwrap().id();
            assert_eq!(generic.operations().filter(|(_, operation)| matches!(operation.authority,
                PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::ErrorField { .. }, .. })).count(), 7);
            let family = crate::runtime::value::structured_error_constructor("Failure", "Failed", RecordMap::from([
                (Arc::from("message"), Value::Str(Arc::from("kept"))),
            ]), vec!["InvalidData".to_owned()], "kept");
            for recursive in [false, true] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let mut execute = || {
                    assert_eq!(crate::runtime::eval::lowered_run::recursive_fast_path_forced(), recursive);
                    for (function, argument) in [
                        ("erased", crate::runtime::value::error_constructor("failure", "kept")),
                        ("family", family.clone()), ("facet", family.clone()),
                        ("process_message", crate::runtime::value::run_error_constructor("failed", "kept")),
                        ("exact", Value::Str(Arc::from("kept"))),
                    ] {
                        let value = evaluator.call_indexed_direct(LoweredFunctionKey::Name(Name::intern(function)), LoweredFunctionKind::Pure,
                            &[argument], Span::new(source_id, 0, 0)).unwrap().unwrap();
                        assert_eq!(value, Value::Str(Arc::from("kept")), "function={function} recursive={recursive}");
                    }
                };
                if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(execute); } else { execute(); }
            }
        });
    });
}

#[test]
fn original_error_field_refuses_rewritten_receiver_field_result_source_and_foreign_receipts() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture();
        let foreign = fixture();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let selected = generic.operations().filter(|(_, operation)| matches!(operation.authority,
                PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::ErrorField { receiver: Atom::Error, .. }, .. })).collect::<Vec<_>>();
            let (id, operation) = selected[1];
            let source = generic.operation_source(operation.source).unwrap();
            let replacement = selected[2].1.binding.operands[0];
            let words = program.store.data[source.instruction as usize].range();
            let mut receiver = program.store.clone();
            receiver.extra[words.start as usize] = replacement;
            assert!(FullVerifier::verify_generic_evidence(&receiver).is_err());
            receiver.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().binding.operands[0] = replacement;
            assert!(FullVerifier::verify_generic_evidence(&receiver).unwrap_err().message.contains("original receipt"));
            let mut field = program.store.clone();
            let wrong = (0..field.strings.len() as u32).find(|&id| field.string(id).is_ok_and(|value| value != "message")).unwrap();
            field.extra[words.start as usize + 1] = wrong;
            assert!(FullVerifier::verify_generic_evidence(&field).is_err());
            let mut result = program.store.clone();
            result.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().result = operation.arguments[0].unwrap();
            assert!(FullVerifier::verify_generic_evidence(&result).is_err());
            let mut owner = program.store.clone();
            owner.generic.as_deref_mut().unwrap().test_operation_source_mut(operation.source).unwrap().owner = InstructionOwner::Driver(0);
            assert!(FullVerifier::verify_generic_evidence(&owner).is_err());
            let mut origin = program.store.clone();
            origin.generic.as_deref_mut().unwrap().test_operation_source_mut(operation.source).unwrap().origin = generic.operation_source(selected[0].1.source).unwrap().origin;
            assert!(FullVerifier::verify_generic_evidence(&origin).is_err());
            let mut missing = program.store.clone();
            missing.generic.as_deref_mut().unwrap().test_remove_operations();
            assert!(FullVerifier::verify_generic_evidence(&missing).is_err());
            let foreign_id = foreign.generic_evidence().unwrap().operations().find(|(_, operation)| matches!(operation.authority,
                PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::ErrorField { .. }, .. })).unwrap().0;
            assert!(generic.operation(foreign_id).is_err());
        });
    });
}

#[test]
fn original_error_field_keeps_explicit_any_message_without_finite_authority() {
    let source = "pure erased(value: Any) -> Str { value.message }\nlet selected = erased({message: \"kept\"})\n";
    let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty());
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(!checked.solved.operations.values().any(|operation| {
        let graph = &checked.solved.graph;
        graph.candidate_evidence(operation.requirement).unwrap().is_some_and(|selected| matches!(
            checked.solved.operation_catalog.candidate(graph, selected.candidate).unwrap(),
            crate::sema::check::SolvedOperationAuthority::Language(metadata) if matches!(metadata.operation, PreparedLanguageOperation::ErrorField { .. })))
    }));
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("erased-error-field.xsh", source);
    let mut preparing = Evaluator::new_with_sources(Vec::new(), sources);
    assert!(preparing.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked).is_err(), "the explicit Any receiver cannot establish an error field producer from a concrete caller");
}
