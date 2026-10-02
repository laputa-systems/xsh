use super::*;

#[test]
fn prepared_generic_callback_invocation_keeps_independent_signatures_after_frontend_drop() {
    crate::runtime::eval::run_eval(|| {
        let source = r#"pure apply(callback, value) { callback(value) }
pure integer(operand: Int) -> Int { operand }
pure text(message: Str) -> Str { message }
proc selected_number() [] -> Int { apply(integer, 7) }
proc selected_text() [] -> Str { apply(text, "word") }
"#;
        let (mut evaluator, program, span) = prepared_frame_fixture(source);
        let _symbols = program.symbol_owner().enter();
        let functions = program.function_count();
        let retained = program.retained_bytes();
        for recursive in [false, true] {
            assert_eq!(call_prepared_fixture_route(&mut evaluator, &program, fixture_function(&program, "selected_number"), span, recursive).unwrap(), LoweredValue::Int(7));
            assert_eq!(call_prepared_fixture_route(&mut evaluator, &program, fixture_function(&program, "selected_text"), span, recursive).unwrap(), LoweredValue::Str(Arc::from("word")));
        }
        assert_eq!(program.function_count(), functions);
        assert_eq!(program.retained_bytes(), retained);
    });
}

#[test]
fn prepared_scoped_callback_uses_original_named_order_and_handle_defaults() {
    crate::runtime::eval::run_eval(|| {
        let source = r#"let suffix: Str = "original"
pure combine(first: Str, second: Str = suffix) -> Str { first + second }
pure reversed(callback, left, right) { callback(second: right, first: left) }
pure defaulted(callback, value) { callback(value) }
proc ordered() [] -> Str { reversed(combine, "left-", "right") }
proc omitted(suffix: Str) [] -> Str { defaulted(combine, "head-") }
"#;
        for recursive in [false, true] {
            let (mut evaluator, program, span) = prepared_frame_fixture(source);
            let _symbols = program.symbol_owner().enter();
            evaluator.eval_indexed_driver_step(0, span).unwrap().unwrap();
            assert_eq!(call_prepared_fixture_route(&mut evaluator, &program, fixture_function(&program, "ordered"), span, recursive).unwrap(), LoweredValue::Str(Arc::from("left-right")));
            assert_eq!(call_prepared_fixture_route_with_values(&mut evaluator, &program, fixture_function(&program, "omitted"), &[LoweredValue::Str(Arc::from("shadow"))], span, recursive).unwrap(), LoweredValue::Str(Arc::from("head-original")));
            assert!(evaluator.call_stack.is_empty());
        }
    });
}

#[test]
fn prepared_forwarded_callback_invocation_keeps_rebased_requirement_ancestry() {
    crate::runtime::eval::run_eval(|| {
        let source = r#"pure apply(callback, value) { callback(value) }
pure forwarded(value, callback) { apply(callback, value) }
pure integer(operand: Int) -> Int { operand }
pure text(message: Str) -> Str { message }
proc selected_number() [] -> Int { forwarded(7, integer) }
proc selected_text() [] -> Str { forwarded("word", text) }
"#;
        let (mut evaluator, program, span) = prepared_frame_fixture(source);
        let _symbols = program.symbol_owner().enter();
        for recursive in [false, true] {
            assert_eq!(call_prepared_fixture_route(&mut evaluator, &program, fixture_function(&program, "selected_number"), span, recursive).unwrap(), LoweredValue::Int(7));
            assert_eq!(call_prepared_fixture_route(&mut evaluator, &program, fixture_function(&program, "selected_text"), span, recursive).unwrap(), LoweredValue::Str(Arc::from("word")));
        }
    });
}

#[test]
fn prepared_repeated_forwarded_callbacks_keep_distinct_original_obligations() {
    crate::runtime::eval::run_eval(|| {
        let source = r#"pure apply(callback, value) { callback(value) }
pure forwarded(value, callback) { apply(callback, value) }
pure twice(callback, value) { let _ = forwarded(value, callback); forwarded(value, callback) }
pure integer(operand: Int) -> Int { operand }
pure text(message: Str) -> Str { message }
proc selected_number() [] -> Int { twice(integer, 7) }
proc selected_text() [] -> Str { twice(text, "word") }
"#;
        let (mut evaluator, program, span) = prepared_frame_fixture(source);
        let _symbols = program.symbol_owner().enter();
        let evidence = program.generic_evidence().unwrap();
        let (_, original) = evidence.scoped_invocation_sources().next().unwrap();
        assert!(original.obligations.iter().any(|obligation| obligation.ancestry.len() >= 3));
        assert!(original.obligations.iter().any(|left| original.obligations.iter().any(|right|
            left.scope == right.scope && left.requirement != right.requirement
                && left.immediate_original != right.immediate_original)));
        for recursive in [false, true] {
            assert_eq!(call_prepared_fixture_route(&mut evaluator, &program, fixture_function(&program, "selected_number"), span, recursive).unwrap(), LoweredValue::Int(7));
            assert_eq!(call_prepared_fixture_route(&mut evaluator, &program, fixture_function(&program, "selected_text"), span, recursive).unwrap(), LoweredValue::Str(Arc::from("word")));
        }
    });
}
