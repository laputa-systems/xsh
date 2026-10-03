use super::*;
use crate::sema::check::Checker;

fn pipeline_source(stage: &str) -> String {
    let pipeline = if stage == "fold" {
        "[1, 2, 3] |> fold(0) { |acc, item| combine(acc, item)? }".to_owned()
    } else {
        format!("[1, 2, 3] |> par-map(jobs: {stage}) {{ |item| combine(0, item)? }}")
    };
    format!(r#"
error CombineError = Stop(item: Int)
pure combine(acc: Int, item: Int) -> Result[Int, CombineError] {{
    if item == 2 {{ Err(CombineError.Stop(item)) }} else {{ acc + item }}
}}
proc observed() [error] -> Bool {{
    let outcome = try {{ {pipeline} }}
    outcome is Err(CombineError.Stop {{item: 2}})
}}
print ${{observed()}}
"#)
}

#[test]
fn pipeline_nominal_error_capture_survives_frontend_drop_on_both_routes() {
    crate::runtime::eval::run_eval(|| {
        for stage in ["fold", "1", "2"] {
            for recursive in [false, true] {
                let source = pipeline_source(stage);
                let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
                    "pipeline-error-capture.xsh", crate::loader::entry_source_from_text("pipeline-error-capture.xsh", source.clone()), Vec::new());
                assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                let source_id = SourceMap::files(&sources).first().unwrap().id();
                let checked = Checker::check_arena(&parsed.arena, &source);
                assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
                assert!(!checked.solved.stage_operations.is_empty());
                assert_eq!(checked.solved.error_captures.len(), 1, "the original capture publishes its reached error join before lowering");
                let weak = Arc::downgrade(&checked.solved);
                let counters = checked.solved.graph.counters().clone();
                let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources);
                let plan = evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked).unwrap();
                assert_eq!(checked.solved.graph.counters(), &counters);
                drop(checked); drop(parsed);
                assert!(weak.upgrade().is_none());
                let program = evaluator.indexed_program.as_ref().unwrap();
                let symbols = program.symbol_owner().clone();
                symbols.with_current(|| {
                    let captures = program.generic_evidence().unwrap().try_capture_sources().collect::<Vec<_>>();
                    assert_eq!(captures.len(), 1);
                    let capture = captures[0].1;
                    let relation = capture.error_capture.as_ref().expect("pipeline retains its original reached error join");
                    let family = Type::ErrorFamily(Name::intern("CombineError"));
                    assert_eq!(relation.output, family);
                    assert!(!relation.inputs.is_empty());
                    assert!(relation.inputs.iter().all(|(_, ty)| *ty == family));
                    let success = if stage == "fold" { Type::Int } else { Type::List(Box::new(Type::Int)) };
                    assert_eq!(program.store.semantic.to_type(capture.carrier).unwrap(), Type::Result(Box::new(success), Box::new(family)));
                });
                let target = symbols.with_current(|| LoweredFunctionKey::Name(Name::intern("observed")));
                let execute = || symbols.with_current(|| evaluator.try_eval_installed_compact_indexed_only_inner(plan)
                    .unwrap_or_else(|_| panic!("prepared pipeline capture remains installed")));
                let output = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(target, recursive, execute);
                assert_eq!(output.status, 0, "stage={stage}, recursive={recursive}: {:?}; {:?}", output.diagnostics, output.traceback);
                assert_eq!(output.stdout, b"true\n");
                assert!(output.stderr.is_empty());
                assert!(output.diagnostics.is_empty());
            }
        }
    });
}

fn prepared_program() -> FullProgram {
    let source = pipeline_source("fold");
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
        "pipeline-error-proof.xsh", crate::loader::entry_source_from_text("pipeline-error-proof.xsh", source.clone()), Vec::new());
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let source_id = SourceMap::files(&sources).first().unwrap().id();
    let checked = Checker::check_arena(&parsed.arena, &source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let weak = Arc::downgrade(&checked.solved);
    let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources);
    evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked).unwrap();
    drop(checked); drop(parsed);
    assert!(weak.upgrade().is_none());
    evaluator.indexed_program.as_ref().unwrap().as_ref().clone()
}

#[test]
fn pipeline_error_capture_refuses_missing_foreign_and_rewritten_boundary_receipts() {
    crate::runtime::eval::run_eval(|| {
        let program = prepared_program();
        let _symbols = program.symbol_owner().enter();
        let generic = program.generic_evidence().unwrap();
        let (id, source) = generic.try_capture_sources().next().unwrap();
        let relation = source.error_capture.as_ref().unwrap();

        let mut missing = program.clone();
        missing.store.generic.as_mut().unwrap().test_try_capture_source_mut(id).unwrap().error_capture = None;
        assert!(FullVerifier::verify(&missing).is_err(), "the original boundary cannot lose its reached error contributions");

        let foreign = prepared_program();
        let foreign_source = foreign.generic_evidence().unwrap().try_capture_sources().next().unwrap().1;
        assert!(relation.verify(foreign_source).is_err(), "another source publication cannot replace an equal nominal error join");

        let mut changed_owner = source.clone();
        changed_owner.owner = InstructionOwner::Driver(0);
        assert!(relation.verify(&changed_owner).is_err(), "the source error boundary keeps its function owner");

        let mut changed_output = source.clone();
        let Type::Result(_, error) = &mut changed_output.original_carrier else { unreachable!(); };
        **error = Type::ErrorVariant { family: Name::intern("CombineError"), variant: Name::intern("Stop") };
        assert!(relation.verify(&changed_output).is_err(), "a member payload cannot replace the originally checked family output");

        let mut changed_inputs = program.clone();
        let source = changed_inputs.store.generic.as_mut().unwrap().test_try_capture_source_mut(id).unwrap();
        let relation = Arc::make_mut(source.error_capture.as_mut().unwrap());
        relation.inputs[0].1 = Type::Error;
        assert!(FullVerifier::verify(&changed_inputs).is_err(), "the sealed original join cannot substitute a broad error contribution");
    });
}
