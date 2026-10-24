use super::*;

fn verify_imported_guarded_program(file: &str, function: &str) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = root.join("tests/fixtures/runtime").join(file);
    let source = std::fs::read_to_string(&path).unwrap();
    verify_guarded_source(&path, source, function);
}

fn verify_guarded_source(path: &std::path::Path, source: String, function: &str) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
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

#[test]
fn benign_null_guards_preserve_opaque_values_without_payload_requirements() {
    on_scoped_body_test_stack(|| verify_imported_guarded_program("guarded-requirements-benign.xsh", "verify_guarded_benign"));
}

#[test]
fn typed_guarded_local_reads_retain_arithmetic_paths_and_opaque_record_fields() {
    on_scoped_body_test_stack(|| {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/runtime/guarded-requirements-local-reads.xsh");
        let source = std::fs::read_to_string(&path).unwrap()
            .replace("guarded_add(value)", "guarded_add(value: Int?)")
            .replace("guarded_parent(value)", "guarded_parent(value: Path?)")
            .replace("guarded_name(value)", "guarded_name(value: GuardedNamedRow?)");
        verify_guarded_source(&path, source, "verify_guarded_local_reads");
    });
}

#[test]
fn inferred_guarded_local_reads_retain_arithmetic_paths_and_opaque_record_fields() {
    on_scoped_body_test_stack(|| verify_imported_guarded_program("guarded-requirements-local-reads.xsh", "verify_guarded_local_reads"));
}

#[test]
fn inferred_root_record_rows_preserve_opaque_fields_from_actual_closed_sources() {
    on_scoped_body_test_stack(|| verify_imported_guarded_program("generic-opaque-record-rows.xsh", "verify_opaque_record_rows"));
}

#[test]
fn imported_guarded_requirements_forward_through_generic_calls() {
    on_scoped_body_test_stack(|| verify_imported_guarded_program("guarded-requirements-forwarding.xsh", "verify_guarded_forwarding"));
}

#[test]
fn typed_guarded_local_forwarding_preserves_each_visible_payload() {
    on_scoped_body_test_stack(|| {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/runtime/guarded-requirements-local-forwarding.xsh");
        let source = std::fs::read_to_string(&path).unwrap()
            .replace("guarded_identity(value)", "guarded_identity(value: Str?)")
            .replace("guarded_relay(value)", "guarded_relay(value: Str?)")
            .replace("guarded_nested(value, other)", "guarded_nested(value: Str?, other: Str?)");
        verify_guarded_source(&path, source, "verify_guarded_local_forwarding");
    });
}

#[test]
fn inferred_guarded_local_forwarding_preserves_each_visible_payload() {
    on_scoped_body_test_stack(|| verify_imported_guarded_program("guarded-requirements-local-forwarding.xsh", "verify_guarded_local_forwarding"));
}

#[test]
fn typed_guarded_callbacks_preserve_arguments_invocation_and_cleanup() {
    on_scoped_body_test_stack(|| {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/runtime/guarded-requirements-callbacks.xsh");
        let source = std::fs::read_to_string(&path).unwrap()
            .replace("proc guarded_fixed_callback(action,", "proc guarded_fixed_callback(action: GuardedObserver?,")
            .replace("proc guarded_inferred_callback(action,", "proc guarded_inferred_callback(action: GuardedObserver?,")
            .replace("proc guarded_callback_absence() []", "proc guarded_callback_absence() [env, error]");
        verify_guarded_source(&path, source, "verify_guarded_callbacks");
    });
}

#[test]
fn inferred_guarded_callbacks_preserve_arguments_invocation_and_cleanup() {
    on_scoped_body_test_stack(|| verify_imported_guarded_program("guarded-requirements-callbacks.xsh", "verify_guarded_callbacks"));
}


#[test]
fn inferred_root_callbacks_propagate_only_their_actual_error_contract() {
    on_scoped_body_test_stack(|| {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".work/tmp/root-callback-propagation.xsh");
        let source = r#"
var invoked = 0
proc observe(value: Int) [error] -> Result[Int, Error] { invoked += 1; Ok(value + 1) }
proc apply(action, value: Int) [error] -> Result[Unit, Error] { let _ = action(value)?; Ok() }
proc verify_root_callback() [error] -> Result[Unit, Error] {
  let observer: proc(value: Int) [error] -> Result[Int, Error] = observe
  let _ = apply(observer, 5)?
  assert invoked == 1
  Ok()
}
"#;
        verify_guarded_source(&path, source.to_owned(), "verify_root_callback");
    });
}


#[test]
fn guarded_callbacks_preserve_written_error_and_effect_consumers() {
    on_scoped_body_test_stack(|| {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/runtime/guarded-requirements-callbacks.xsh");
        let source = std::fs::read_to_string(&path).unwrap()
            .replace("proc guarded_inferred_callback(action,", "proc guarded_inferred_callback(action: GuardedObserver?,")
            .replace("proc guarded_callback_absence() []", "proc guarded_callback_absence() [env, error]");
        verify_guarded_source(&path, source, "verify_guarded_callbacks");
    });
}


#[test]
fn imported_guarded_callbacks_forward_conditional_effects_after_disposal() {
    on_scoped_body_test_stack(|| {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/runtime/guarded-requirements-callbacks.xsh");
        let source = std::fs::read_to_string(&path).unwrap()
            .replace("let absent = guarded_callback_absence()", "let absent = guarded_callbacks.absent()")
            .replace("let _ = guarded_fixed_callback(", "let _ = guarded_callbacks.fixed(")
            .replace("let _ = guarded_inferred_callback(", "let _ = guarded_callbacks.relay(");
        verify_guarded_source(&path, format!("use guarded_callbacks\n{source}"), "verify_guarded_callbacks");
    });
}

#[test]
fn guarded_known_declarations_specialize_their_actual_pending_effects() {
    on_scoped_body_test_stack(|| verify_imported_guarded_program("guarded-requirements-declarations.xsh", "verify_guarded_known_targets"));
}

#[test]
fn typed_guarded_outputs_preserve_their_written_optional_domains() {
    on_scoped_body_test_stack(|| {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/runtime/guarded-requirements-outputs.xsh");
        let source = std::fs::read_to_string(&path).unwrap()
            .replace("pure guarded_add_output(value)", "pure guarded_add_output(value: Int?) -> Int?")
            .replace("pure guarded_parent_output(value)", "pure guarded_parent_output(value: Path?) -> Path?")
            .replace("pure guarded_bound_output(value)", "pure guarded_bound_output(value: Int?) -> Int?")
            .replace("pure guarded_name_output(value)", "pure guarded_name_output(value: GuardedOutputRow?) -> Str?")
            .replace("let bound: Int =", "let bound: Int? =")
            .replace("let absent_bound: Null =", "let absent_bound: Int? =")
            .replace("let name: Str =", "let name: Str? =")
            .replace("let absent_name: Null =", "let absent_name: Str? =")
            .replace("let exact: Int =", "let exact: Int? =")
            .replace("let absent: Null =", "let absent: Int? =")
            .replace("let exact_parent: Path =", "let exact_parent: Path? =")
            .replace("let absent_parent: Null =", "let absent_parent: Path? =")
            .replace("let relative_parent: RelPath =", "let relative_parent: Path? =");
        verify_guarded_source(&path, source, "verify_guarded_outputs");
    });
}

#[test]
fn inferred_guarded_outputs_preserve_actual_null_optional_and_selected_results() {
    on_scoped_body_test_stack(|| verify_imported_guarded_program("guarded-requirements-outputs.xsh", "verify_guarded_outputs"));
}

#[test]
fn guarded_outputs_keep_compound_conditions_as_ordinary_joins() {
    on_scoped_body_test_stack(|| {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".work/tmp/guard-output-compound.xsh");
        let source = r#"
pure gated(value, flag: Bool) { if value != null and flag { value + 1 } else { null } }
proc verify_compound() [error] -> Result[Unit, Error] {
    let present: Int? = 4
    let selected: Int? = gated(2, true)
    let skipped: Int? = gated(2, false)
    let optional: Int? = gated(present, true)
    let absent: Int? = gated(null, true)
    assert selected == 3
    assert skipped == null
    assert optional == 5
    assert absent == null
    Ok()
}
"#;
        verify_guarded_source(&path, source.to_owned(), "verify_compound");
        let source = format!("{source}\nlet invalid: Int = gated(2, false)\n");
        let mut sources = crate::source::SourceMap::default();
        let source_id = sources.add_file(path.to_str().unwrap(), source.clone());
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(source_id, &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckTypeMismatch)), "{:?}", checked.diagnostics);
    });
}

#[test]
fn benign_guarded_outputs_do_not_create_payload_requirements_for_opaque_inputs() {
    on_scoped_body_test_stack(|| {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".work/tmp/guard-output-benign.xsh");
        let source = r#"
pure constant(value) { if value != null { 1 } else { null } }
proc verify_benign_output() [error] -> Result[Unit, Error] {
    let present: Any = {hidden: [1, 2]}
    let missing: Any = null
    let selected: Int? = constant(present)
    let absent: Int? = constant(missing)
    assert selected == 1
    assert absent == null
    Ok()
}
"#;
        verify_guarded_source(&path, source.to_owned(), "verify_benign_output");
    });
}
