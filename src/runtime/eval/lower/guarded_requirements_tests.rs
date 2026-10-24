use super::*;

fn verify_imported_guarded_program(file: &str, function: &str) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = root.join("tests/fixtures/runtime").join(file);
    let source = std::fs::read_to_string(&path).unwrap();
    let modules = root.join("tests/fixtures/modules/guarded-requirements");
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(path.to_str().unwrap(), crate::loader::entry_source_from_text(path.to_str().unwrap(), source), vec![modules]);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let source_id = parsed.arena.source_text_source_id().unwrap();
    let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let prepared = Evaluator::new_with_sources(Vec::new(), sources).prepare_test_program(Arc::new(parsed.arena), source_id, checked).unwrap();
    for force_frames in [false, true] {
        let result = prepared.eval_test(function, Value::Unit, false, force_frames, Vec::new(), crate::runtime::eval::TestCancellation::default());
        assert_eq!(result.output.status, 0, "force_frames={force_frames}: {:?} {:?}", result.output.diagnostics, result.output.traceback);
        assert!(result.output.diagnostics.is_empty(), "{:?}", result.output.diagnostics);
        assert!(result.output.traceback.is_none(), "{:?}", result.output.traceback);
        assert!(matches!(&result.result, Some(Value::Result(ResultValue::Ok(value))) if matches!(value.as_ref(), Value::Unit)), "force_frames={force_frames}: {:?}", result.result);
    }
}

#[test]
fn imported_guarded_render_accepts_plain_optional_and_null_after_disposal() {
    on_scoped_body_test_stack(|| verify_imported_guarded_program("guarded-requirements-render.xsh", "verify_guarded_render"));
}

#[test]
fn imported_guarded_preload_accepts_plain_optional_and_null_after_disposal() {
    on_scoped_body_test_stack(|| verify_imported_guarded_program("guarded-requirements-preload.xsh", "verify_guarded_preload"));
}
