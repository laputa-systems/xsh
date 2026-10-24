use crate::runtime::eval::{Evaluator, FullBuilder, LoweredFunctionKey, CallableKind, ValueView, value_matches_static_type, runtime_values_semantically_equal};
use crate::runtime::eval::lowered_ops::lowered_value_from_runtime_any;
use crate::runtime::value::{ResultValue, Value};
use crate::sema::check::Checker;
use crate::sema::types::Type;
use crate::source::{SourceId, SourceMap, Span};
use crate::symbol::Name;
use crate::syntax::parser::Parser;
use std::sync::Arc;

fn verify_comprehension_argument_call(typed: bool) {
    let same = if typed { "pure same(value: List[Int]) -> List[Int] { value }" } else { "pure same(value) { value }" };
    let source = format!("{same}\npure selected(values: List[Int]) -> List[Int] {{ same([item for item in values if item > 0]) }}\n");
    let (program, sources, source_id) = record_form_program(&source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("selected")));
        for (values, expected) in [(vec![Value::Int(-1), Value::Int(7), Value::Int(0)], vec![Value::Int(7)]), (Vec::new(), Vec::new())] {
            let actual = evaluator.call_indexed_direct(key, CallableKind::Pure, &[Value::List(values)], Span::at(source_id, 0)).unwrap().unwrap();
            assert_eq!(actual, Value::List(expected), "typed={typed} frames={force_frames}");
        }
    }
}

#[test]
fn typed_comprehension_arguments_preserve_filtered_values() {
    verify_comprehension_argument_call(true);
}

#[test]
fn inferred_comprehension_arguments_preserve_filtered_values() {
    verify_comprehension_argument_call(false);
}

#[test]
fn inferred_comprehension_arguments_preserve_records_and_opaque_results() {
    let source = r#"type Row = {issue: Str, name: Str, path: Path, stdin: Bool}
pure same(value) { value }
pure records(values: List[Row]) -> List[Row] { same([item for item in values if item.issue == ""]) }
pure outcomes(values: List[Result[Int, Str]]) -> List[Result[Int, Str]] { same([item for item in values]) }
"#;
    let (program, sources, source_id) = record_form_program(source);
    let record = |issue: &str, name: &str| program.symbol_owner().with_current(|| Value::Record([
        (Arc::from("issue"), Value::Str(Arc::from(issue))),
        (Arc::from("name"), Value::Str(Arc::from(name))),
        (Arc::from("path"), Value::Path(crate::runtime::value::PathValue::from_text("input").unwrap())),
        (Arc::from("stdin"), Value::Bool(false)),
     ].into_iter().collect()));
    let kept = record("", "kept");
    let outcomes = vec![Value::Result(ResultValue::Ok(Box::new(Value::Int(7)))), Value::Result(ResultValue::Err(Box::new(Value::Str(Arc::from("opaque")))))];
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        for (name, input, expected) in [
            ("records", vec![record("bad", "removed"), kept.clone()], vec![kept.clone()]),
            ("records", Vec::new(), Vec::new()),
            ("outcomes", outcomes.clone(), outcomes.clone()),
            ("outcomes", Vec::new(), Vec::new()),
        ] {
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
            let actual = evaluator.call_indexed_direct(key, CallableKind::Pure, &[Value::List(input)], Span::at(source_id, 0)).unwrap().unwrap();
            assert_eq!(actual, Value::List(expected), "{name} frames={force_frames}");
        }
    }
}

#[test]
fn inferred_comprehension_arguments_keep_evaluation_and_cleanup_order() {
    let source = r#"pure same(value, later: Int) { let _ = later; value }
pure ordinary(value: List[Int], later: Int) -> List[Int] { let _ = later; value }
stream items() [io, error] -> Stream[Int] { defer { print "close" }; print "pull"; yield 1; yield 2 }
proc later() [io] -> Int { print "later"; 7 }
proc selected(fail: Bool) [io, error] -> List[Int] {
  same([{ print "project"; let _ = [item][if fail { 9 } else { 0 }]; item } for item in items() if item > 0], later())
}
proc typed(fail: Bool) [io, error] -> List[Int] {
  ordinary([{ print "project"; let _ = [item][if fail { 9 } else { 0 }]; item } for item in items() if item > 0], later())
}
"#;
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        for fail in [false, true] {
            let key = |name| program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
            evaluator.stdout.clear();
            let expected = evaluator.call_indexed_direct(key("typed"), CallableKind::Proc, &[Value::Bool(fail)], Span::at(source_id, 0)).unwrap();
            let stdout = evaluator.stdout.clone();
            evaluator.stdout.clear();
            let actual = evaluator.call_indexed_direct(key("selected"), CallableKind::Proc, &[Value::Bool(fail)], Span::at(source_id, 0)).unwrap();
            assert_eq!(evaluator.stdout, stdout, "fail={fail} frames={force_frames}");
            assert_eq!(stdout.as_slice(), if fail { b"pull\nproject\nclose\n".as_slice() } else { b"pull\nproject\nproject\nclose\nlater\n".as_slice() });
            match (expected, actual) {
                (Ok(expected), Ok(actual)) => { assert!(!fail); assert_eq!(actual, expected); assert_eq!(actual, Value::List(vec![Value::Int(1), Value::Int(2)])); }
                (Err(expected), Err(actual)) => { assert!(fail); assert_eq!(actual.kind, "index-out-of-range"); assert_eq!(actual.kind, expected.kind); assert_eq!(actual.message, expected.message); assert_eq!(actual.family, expected.family); }
                (expected, actual) => panic!("fail={fail} frames={force_frames}: {expected:?} vs {actual:?}"),
            }
        }
    }
}

fn verify_unsigned_arithmetic_context(context: &str) {
    let (body, returned) = match context {
        "return" => ("value / 2", "UInt"),
        "storage" => ("let whole: UInt = value / 2; whole", "UInt"),
        "alias" => ("let raw = value / 2; let whole: UInt = raw; whole", "UInt"),
        "record" => ("{high: value / 2, low: value % 2}", "Parts"),
        _ => panic!("unknown unsigned arithmetic context"),
    };
    let source = format!("type Parts = {{high: UInt, low: UInt}}\npure typed(value: Int) -> {returned} {{ {body} }}\npure generic(value) -> {returned} {{ {body} }}\npure selected(value: Int) -> {returned} {{ generic(value) }}\n");
    let (program, sources, source_id) = record_form_program(&source);
    let expected = program.symbol_owner().with_current(|| if context == "record" {
        Value::Record([(Arc::from("high"), Value::Int(4)), (Arc::from("low"), Value::Int(1))].into_iter().collect())
    } else { Value::Int(4) });
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        for name in ["typed", "selected"] {
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
            let actual = evaluator.call_indexed_direct(key, CallableKind::Pure, &[Value::Int(9)], Span::at(source_id, 0)).unwrap().unwrap();
            assert_eq!(actual, expected, "{context} {name} frames={force_frames}");
        }
    }
}

#[test]
fn inferred_unsigned_arithmetic_returns_preserve_intrinsic_integer_results() {
    verify_unsigned_arithmetic_context("return");
}

#[test]
fn inferred_unsigned_arithmetic_storage_preserves_intrinsic_integer_results() {
    verify_unsigned_arithmetic_context("storage");
}

#[test]
fn inferred_unsigned_arithmetic_record_fields_preserve_intrinsic_integer_results() {
    verify_unsigned_arithmetic_context("record");
}

#[test]
fn inferred_unsigned_arithmetic_aliases_preserve_intrinsic_integer_results() {
    verify_unsigned_arithmetic_context("alias");
}

#[test]
fn inferred_unsigned_arithmetic_preserves_typed_failure_and_cleanup_order() {
    let source = r#"proc typed(value: Int, divisor: Int) [io, error] -> UInt {
  defer { print "clean" }
  value / divisor
}

proc generic(value, divisor: Int) [io, error] -> UInt {
  defer { print "clean" }
  value / divisor
}
proc selected(value: Int, divisor: Int) [io, error] -> UInt { generic(value, divisor) }
"#;
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        let key = |name| program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
        for (value, divisor) in [(9, 2), (-9, 2), (9, 0), (i64::MIN, -1)] {
            evaluator.stdout.clear();
            let expected = evaluator.call_indexed_direct(key("typed"), CallableKind::Proc, &[Value::Int(value), Value::Int(divisor)], Span::at(source_id, 0)).unwrap();
            let expected_stdout = evaluator.stdout.clone();
            evaluator.stdout.clear();
            let actual = evaluator.call_indexed_direct(key("selected"), CallableKind::Proc, &[Value::Int(value), Value::Int(divisor)], Span::at(source_id, 0)).unwrap();
            assert_eq!(evaluator.stdout, expected_stdout, "value={value} divisor={divisor} frames={force_frames}");
            assert_eq!(evaluator.stdout, b"clean\n");
            match (expected, actual) {
                (Ok(expected), Ok(actual)) => { assert!(value >= 0 && divisor > 0); assert_eq!(actual, expected); }
                (Err(expected), Err(actual)) => { assert_eq!(actual.kind, expected.kind); assert_eq!(actual.message, expected.message); assert_eq!(actual.family, expected.family); }
                (expected, actual) => panic!("value={value} divisor={divisor} frames={force_frames}: expected {expected:?}, found {actual:?}"),
            }
        }
    }
}

#[test]
fn unsigned_arithmetic_contexts_preserve_duration_ratio_domains() {
    let source = "pure divide(left, right) -> UInt { left / right }\npure ordinary() -> UInt { 8s / 2s }\npure selected() -> UInt { divide(8s, 2s) }\n";
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        for name in ["ordinary", "selected"] {
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
            assert_eq!(evaluator.call_indexed_direct(key, CallableKind::Pure, &[], Span::at(source_id, 0)).unwrap().unwrap(), Value::Int(4));
        }
    }
}

#[test]
fn unsigned_arithmetic_contexts_reject_incompatible_result_domains() {
    for call in ["divide(8.0, 2.0)", "divide(8s, 2)", "divide(8, \"two\")"] {
        let source = format!("pure divide(left, right) -> UInt {{ left / right }}\nlet invalid = {call}\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error), "{source}\n{:?}", checked.diagnostics);
    }
}


#[test]
fn typed_driver_conditions_preserve_lexical_boolean_producers_on_both_routes() {
    on_scoped_body_test_stack(|| {
        for source in [
            "let ready: Bool = true\nassert ready\n",
            "tempdir site { let ready: Bool = true; assert ready }\n",
            "tempdir site { with root = fs.open_root(site)? { let ready: Bool = true; assert ready } }\n",
            "error Failure { Bad }\npure failed() -> Result[Str] { Err(Failure.Bad(\"bad\")) }\nmatch failed() { Err(error) => assert error.message != \"\"; Ok(_) => assert false }\n",
        ] {
            let (program, sources, source_id) = record_form_program(source);
            for force_frames in [false, true] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
                evaluator.force_frames = force_frames;
                evaluator.indexed_program = Some(Arc::clone(&program));
                for step in 0..program.driver_step_count().unwrap() {
                    let flow = evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap()
                        .unwrap_or_else(|error| panic!("{source}\nframes={force_frames}: {error:?}"));
                    assert!(matches!(&flow, None | Some(crate::runtime::eval::Flow::Continue(_))), "{source}\nframes={force_frames} step={step}: {flow:?}");
                }
            }
        }
    });
}

#[test]
fn nullable_target_relations_execute_after_frontend_disposal() {
    let source = r#"
type Item = {value: Int}
pure accepts(value: Item?) -> Int { let _ = value; 0 }
pure forward(value) { accepts(value) }
pure forward_named(value) { accepts(value: value) }
pure forward_splice(value) { accepts(@[value]) }
pure read(value) -> Int { let _ = accepts(value); value.value }
let present: Item? = {value: 9}
let absent: Item? = null
assert forward({value: 7}) == 0
assert forward(present) == 0
assert forward(absent) == 0
assert forward(null) == 0
assert forward_named({value: 7}) == 0
assert forward_named(null) == 0
assert forward_splice({value: 7}) == 0
assert forward_splice(null) == 0
assert read({value: 11}) == 11
"#;
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        for step in 0..program.driver_step_count().unwrap() {
            evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap_or_else(|error| panic!("frames={force_frames}: {error:?}"));
        }
    }
}

#[test]
fn nullable_target_recursive_proc_calls_preserve_their_actual_effect_boundary() {
    let source = r#"
type Item = {value: Int}
proc accepts(value: Item?) [] -> Int { let _ = value; 0 }
proc read(value, count: Int) -> Int {
    if count == 0 { let _ = accepts(value); value.value } else { read(value, count - 1) }
}
assert read({value: 11}, 3) == 11
"#;
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        for step in 0..program.driver_step_count().unwrap() {
            evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap_or_else(|error| panic!("frames={force_frames}: {error:?}"));
        }
    }
}

#[test]
fn nullable_target_requirements_keep_unsigned_argument_validation() {
    for annotation in [": Int", ""] {
        for input in [3, -1] {
            let source = format!("pure accepts(value: UInt?) -> Int {{ let _ = value; 0 }}\npure forward(value{annotation}) {{ accepts(value) }}\nlet dynamic: Int = {input}\nassert forward(dynamic) == 0\n");
            let (program, sources, source_id) = record_form_program(&source);
            for force_frames in [false, true] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
                evaluator.force_frames = force_frames;
                evaluator.indexed_program = Some(Arc::clone(&program));
                let mut failure = None;
                for step in 0..program.driver_step_count().unwrap() {
                    if let Err(error) = evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap() {
                        failure = Some(error);
                        break;
                    }
                }
                if input < 0 {
                    let failure = failure.expect("the known nullable formal validates its unsigned payload");
                    assert_eq!(failure.kind, "type-error");
                    assert!(failure.message.contains("UInt"), "{failure:?}");
                } else { assert!(failure.is_none(), "{annotation} frames={force_frames}: {failure:?}"); }
            }
        }
    }
}

#[test]
fn nullable_target_unsigned_argument_checks_preserve_later_argument_failure_order() {
    for force_frames in [false, true] {
        for later_argument in ["later(0)", "1"] {
            let mut failures = Vec::new();
            for annotation in [": Int", ""] {
                let source = format!("pure consume(value: UInt?, later: Int) -> Int {{ let _ = value; later }}\npure later(divisor: Int) -> Int {{ 1 / divisor }}\npure forward(value{annotation}) {{ consume(value, {later_argument}) }}\nlet dynamic: Int = -1\nlet result = forward(dynamic)\n");
                let (program, sources, source_id) = record_form_program(&source);
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
                evaluator.force_frames = force_frames;
                evaluator.indexed_program = Some(Arc::clone(&program));
                let mut failure = None;
                for step in 0..program.driver_step_count().unwrap() {
                    if let Err(error) = evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap() {
                        failure = Some(error);
                        break;
                    }
                }
                let error = failure.expect("a typed boundary or later argument must fail");
                let failed_source = error.span.map(|span| source[span.range()].to_owned());
                failures.push((error.kind, error.message, failed_source));
            }
            assert_eq!(failures[0], failures[1], "frames={force_frames}, later={later_argument}");
        }
    }
}

#[test]
fn generic_value_conversions_execute_after_frontend_disposal() {
    for (argument, number, text, prefix) in [
        ("(value)", "7", "\"word\"", ""),
        ("$value", "7", "\"word\"", ""),
        ("$value", "[7, 8]", "[\"word\", \"other\"]", ""),
        ("@value", "[7, 8]", "[\"word\", \"other\"]", ""),
        ("item:${value}", "7", "\"word\"", "item:"),
    ] {
        let number_output = if number.starts_with('[') { "7 8" } else { "7" };
        let text_output = if text.starts_with('[') { "word other" } else { "word" };
        let source = format!(r#"
pure display(value) {{ f"item:{{value}}" }}
proc send(value) {{ return (run.text /bin/echo {argument} ?) }}
assert display(7) == "item:7"
assert display("word") == "item:word"
assert send({number}) == Ok("{prefix}{number_output}\n")
assert send({text}) == Ok("{prefix}{text_output}\n")
"#);
        let (program, sources, source_id) = record_form_program(&source);
        for force_frames in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            for step in 0..program.driver_step_count().unwrap() {
                evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap_or_else(|error| panic!("{argument} frames={force_frames}: {error:?}"));
            }
        }
    }
}

fn assert_generic_static_run_return(form: &str, expected: Value) {
    let source = format!("proc send(marker) {{ let _ = marker; {form} }}\nproc number() {{ send(7) }}\nproc text() {{ send(\"row\") }}\n");
    let (program, sources, source_id) = record_form_program(&source);
    for force_frames in [false, true] {
        for name in ["number", "text"] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
            let actual = evaluator.call_indexed_direct(key, CallableKind::Proc, &[], Span::at(source_id, 0)).unwrap().unwrap();
            assert_eq!(actual, Value::Result(ResultValue::Ok(Box::new(expected.clone()))), "{form}: {name}, frames={force_frames}");
        }
    }
}

#[test]
fn generic_bare_run_tails_discard_captured_output() {
    for form in [
        "run.text /bin/echo fixed ?",
        "if true { run.text /bin/echo fixed ? } else { run.text /bin/echo other ? }",
        "match true { true => { run.text /bin/echo fixed ? }; false => { run.text /bin/echo other ? } }",
    ] { assert_generic_static_run_return(form, Value::Unit); }
}

#[test]
fn generic_direct_run_returns_preserve_captured_output() {
    assert_generic_static_run_return("return run.text /bin/echo fixed ?", Value::Str(Arc::from("fixed\n")));
}

#[test]
fn generic_grouped_run_returns_preserve_captured_output() {
    assert_generic_static_run_return("return (run.text /bin/echo fixed ?)", Value::Str(Arc::from("fixed\n")));
}

#[test]
fn generic_run_returns_preserve_early_returns_and_output_kinds() {
    for form in [
        "if true { return run.text /bin/echo fixed ? }; return run.text /bin/echo wrong ?",
        "if false { return run.text /bin/echo wrong ? }; return run.text /bin/echo fixed ?",
    ] { assert_generic_static_run_return(form, Value::Str(Arc::from("fixed\n"))); }
    assert_generic_static_run_return("return run.bytes /bin/echo fixed ?", Value::Bytes(b"fixed\n".to_vec()));
}

#[test]
fn generic_run_returns_transfer_streams_after_frontend_disposal() {
    for form in [
        "return run.stream --text /bin/echo fixed ?",
        "if true { return run.stream --text /bin/echo fixed ? }; return run.stream --text /bin/echo wrong ?",
        "return (run.stream --text /bin/echo fixed ?)",
    ] {
        let source = format!("proc send(marker) {{ let _ = marker; {form} }}\nassert (send(7)?).collect() == [\"fixed\"]\nassert (send(\"row\")?).collect() == [\"fixed\"]\n");
        let (program, sources, source_id) = record_form_program(&source);
        for force_frames in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            for step in 0..program.driver_step_count().unwrap() {
                evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap_or_else(|error| panic!("{form}: frames={force_frames}: {error:?}"));
            }
        }
    }
}

#[test]
fn generic_run_returns_preserve_failure_and_cleanup_once() {
    let directory = tempfile::Builder::new().prefix("generic-run-return-").tempdir_in(".work/tmp").unwrap();
    let path = directory.path().canonicalize().unwrap().join("trace");
    for status in [0, 7] {
        let source = format!(r#"proc send(marker) {{ let _ = marker; defer run /bin/sh -c "printf d >> \"$1\"" cleanup {:?} ?; return run.text /bin/sh -c "printf b >> \"$1\"; printf fixed; exit {status}" body {:?} ? }}
proc number() {{ send(7) }}
proc text() {{ send("row") }}
"#, path.to_str().unwrap(), path.to_str().unwrap());
        let (program, sources, source_id) = record_form_program(&source);
        for force_frames in [false, true] {
            for name in ["number", "text"] {
                std::fs::write(&path, b"").unwrap();
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
                evaluator.force_frames = force_frames;
                evaluator.indexed_program = Some(Arc::clone(&program));
                let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
                let actual = evaluator.call_indexed_direct(key, CallableKind::Proc, &[], Span::at(source_id, 0)).unwrap().unwrap();
                if status == 0 {
                    assert_eq!(actual, Value::Result(ResultValue::Ok(Box::new(Value::Str(Arc::from("fixed"))))));
                } else {
                    let Value::Result(ResultValue::Err(error)) = actual else { panic!("the failed child returns Err") };
                    let Value::RunError(error) = *error else { panic!("the child failure retains its process status") };
                    assert_eq!(error.status.as_ref().unwrap().code, Some(status));
                }
                assert_eq!(std::fs::read(&path).unwrap(), b"bd", "{name}: status={status}, frames={force_frames}");
            }
        }
    }
}

#[test]
fn ordinary_generic_bodies_execute_after_frontend_disposal() {
    for definition in [
        "pure keep(value) { match true { true => value; false => value } }",
        "pure keep(value) { let result = loop { break value }; result }",
        "pure keep(value) { let result = loop { let inner = loop { break 1 }; let _ = inner; break value }; result }",
        "pure keep(value) { let result = { let copy = value; copy }; result }",
        "pure keep(value) { let _ = \"value\".starts_with(\"v\"); value }",
        "proc keep(value) [] { var result = value; { result = value }; result }",
        "pure keep(value) { match value { selected => selected } }",
        "pure keep(value) { var result = value; if true { result = value } else { result = value }; result }",
        "pure keep(value) { var result = value; while true { result = value; break }; result }",
        "pure keep(value) { var result = value; var again = true; while again { again = false; result = value; continue }; result }",
        "pure keep(value) { var result = value; match true { true => { result = value }; false => { result = value } }; result }",
        "pure keep(value) { if true { return value }; value }",
        "pure keep(value) { let result = { let copy = value; { let copy = \"shadow\"; let _ = copy }; copy }; result }",
        "proc keep(value) [] { if true { return value }; value }",
    ] {
        let (program, sources, kind) = {
            let proc = definition.starts_with("proc ");
            let wrappers = if proc {
                "proc number() [] -> Result[Int] { keep(7) }\nproc text() [] -> Result[Str] { keep(\"value\") }"
            } else {
                "pure number() -> Int { keep(7) }\npure text() -> Str { keep(\"value\") }"
            };
            let source = format!("{definition}\n{wrappers}\n");
            let mut sources = SourceMap::default();
            let source_id = sources.add_file("ordinary-generic-body.xsh", source.clone());
            let parsed = Parser::parse_source_arena_only(source_id, &source);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            let declarations = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
            assert!(declarations.diagnostics.is_empty(), "{source}\n{:?}", declarations.diagnostics);
            let program = FullBuilder::build_compact(&parsed.arena, &declarations, &source, Arc::new(sources.clone()), source_id)
                .unwrap_or_else(|error| panic!("{source}\n{error:?}"));
            (Arc::new(program), sources, if proc { CallableKind::Proc } else { CallableKind::Pure })
        };
        for force_frames in [false, true] {
            for (name, expected) in [("number", Value::Int(7)), ("text", Value::Str(Arc::from("value")))] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
                evaluator.force_frames = force_frames;
                evaluator.indexed_program = Some(Arc::clone(&program));
                let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
                let actual = evaluator.call_indexed_direct(key, kind, &[], Span::at(SourceId::new(0), 0))
                    .expect("checked wrapper exists").expect("ordinary generic body succeeds");
                let actual = match actual { Value::Result(ResultValue::Ok(value)) => *value, value => value };
                assert_eq!(actual, expected, "{definition}\n{name} frames={force_frames}");
            }
        }
    }
}

#[test]
fn ordinary_generic_explicit_discards_preserve_result_data() {
    let source = "proc keep(value) [] -> Int { let _ = value; 7 }\nproc number(input: Result[Int, Str]) [] -> Int { keep(input) }\n";
    let mut sources = SourceMap::default();
    let source_id = sources.add_file("generic-discard-result.xsh", source.to_string());
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let program = Arc::new(FullBuilder::build_compact(&parsed.arena, &checked, source, Arc::new(sources.clone()), source_id).unwrap());
    drop(checked);
    drop(parsed);
    for force_frames in [false, true] {
        for input in [
            Value::Result(ResultValue::Ok(Box::new(Value::Int(9)))),
            Value::Result(ResultValue::Err(Box::new(Value::Str(Arc::from("dropped"))))),
        ] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("number")));
            let actual = evaluator.call_indexed_direct(key, CallableKind::Proc, &[input], Span::at(source_id, 0)).unwrap().unwrap();
            assert_eq!(actual, Value::Int(7));
        }
    }
}

#[test]
fn ordinary_generic_proc_early_returns_preserve_nested_result_data() {
    let source = "proc keep(value) [] { if true { return value }; value }\nproc wrap(input: Result[Int, Str]) [] -> Result[Result[Int, Str]] { keep(input) }\n";
    let mut sources = SourceMap::default();
    let source_id = sources.add_file("generic-early-return-result.xsh", source.to_string());
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let program = Arc::new(FullBuilder::build_compact(&parsed.arena, &checked, source, Arc::new(sources.clone()), source_id).unwrap());
    drop(checked);
    drop(parsed);
    for force_frames in [false, true] {
        for input in [
            Value::Result(ResultValue::Ok(Box::new(Value::Int(9)))),
            Value::Result(ResultValue::Err(Box::new(Value::Str(Arc::from("data"))))),
        ] {
            let expected = Value::Result(ResultValue::Ok(Box::new(input.clone())));
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("wrap")));
            let actual = evaluator.call_indexed_direct(key, CallableKind::Proc, &[input], Span::at(source_id, 0)).unwrap().unwrap();
            assert_eq!(actual, expected);
        }
    }
}

#[test]
fn ordinary_generic_local_storage_preserves_signed_widening() {
    let source = "pure keep(value, unsigned: UInt) { let signed: Int = unsigned; let _ = signed; value }\npure number() -> Int { keep(7, 7) }\npure text() -> Str { keep(\"value\", 7) }\n";
    let (program, sources) = {
        let mut sources = SourceMap::default();
        let source_id = sources.add_file("generic-signed-storage.xsh", source.to_string());
        let parsed = Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let program = FullBuilder::build_compact(&parsed.arena, &checked, source, Arc::new(sources.clone()), source_id).unwrap();
        (Arc::new(program), sources)
    };
    for force_frames in [false, true] {
        for (name, expected) in [("number", Value::Int(7)), ("text", Value::Str(Arc::from("value")))] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
            let actual = evaluator.call_indexed_direct(key, CallableKind::Pure, &[], Span::at(SourceId::new(0), 0)).unwrap().unwrap();
            assert_eq!(actual, expected, "{name} frames={force_frames}");
        }
    }
}

#[test]
fn ordinary_generic_aggregate_projections_execute_after_frontend_disposal() {
    for definition in [
        "pure keep(value) { [value][0] }",
        "pure keep(value) { [value][-1] }",
        "pure keep(value) { [value, value][0..1][0] }",
        "pure keep(value) { {item: value}[\"item\"] }",
        "pure keep(value) { let _ = 1 + 2; value }",
    ] {
        let (program, sources, kind) = {
            let proc = definition.starts_with("proc ");
            let wrappers = if proc {
                "proc number() [] -> Result[Int] { keep(7) }\nproc text() [] -> Result[Str] { keep(\"value\") }"
            } else {
                "pure number() -> Int { keep(7) }\npure text() -> Str { keep(\"value\") }"
            };
            let source = format!("{definition}\n{wrappers}\n");
            let mut sources = SourceMap::default();
            let source_id = sources.add_file("ordinary-generic-body.xsh", source.clone());
            let parsed = Parser::parse_source_arena_only(source_id, &source);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            let declarations = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
            assert!(declarations.diagnostics.is_empty(), "{source}\n{:?}", declarations.diagnostics);
            let program = FullBuilder::build_compact(&parsed.arena, &declarations, &source, Arc::new(sources.clone()), source_id)
                .unwrap_or_else(|error| panic!("{source}\n{error:?}"));
            (Arc::new(program), sources, if proc { CallableKind::Proc } else { CallableKind::Pure })
        };
        for force_frames in [false, true] {
            for (name, expected) in [("number", Value::Int(7)), ("text", Value::Str(Arc::from("value")))] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
                evaluator.force_frames = force_frames;
                evaluator.indexed_program = Some(Arc::clone(&program));
                let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
                let actual = evaluator.call_indexed_direct(key, kind, &[], Span::at(SourceId::new(0), 0))
                    .expect("checked wrapper exists").expect("ordinary generic body succeeds");
                let actual = match actual { Value::Result(ResultValue::Ok(value)) => *value, value => value };
                assert_eq!(actual, expected, "{definition}\n{name} frames={force_frames}");
            }
        }
    }
}

#[test]
fn ordinary_generic_aggregate_concatenation_preserves_validation_boundaries() {
    let source = "pure keep(value, unsigned: List[UInt]) { let _ = unsigned + unsigned; value }\npure number(items: List[UInt]) -> Int { keep(7, items) }\npure text(items: List[UInt]) -> Str { keep(\"value\", items) }\npure joined(value, items: NonEmpty[Int]) { {payload: value, items: items + items} }\npure nonempty() -> NonEmpty[Int] { joined(7, [1]).items }\n";
    let mut sources = SourceMap::default();
    let source_id = sources.add_file("generic-concatenation.xsh", source.to_string());
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let program = Arc::new(FullBuilder::build_compact(&parsed.arena, &checked, source, Arc::new(sources.clone()), source_id).unwrap());
    drop(checked);
    drop(parsed);
    for force_frames in [false, true] {
        for (name, expected) in [("number", Value::Int(7)), ("text", Value::Str(Arc::from("value")))] {
            for item in [2, -1] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
                evaluator.force_frames = force_frames;
                evaluator.indexed_program = Some(Arc::clone(&program));
                let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
                let actual = evaluator.call_indexed_direct(key, CallableKind::Pure, &[Value::List(vec![Value::Int(item)])], Span::at(source_id, 0)).unwrap();
                if item >= 0 { assert_eq!(actual.unwrap(), expected); }
                else {
                    let error = actual.unwrap_err();
                    assert_eq!(error.kind, "type-error");
                    assert_eq!(error.family, "Error");
                    assert!(error.message.contains("UInt"), "{error:?}");
                }
            }
        }
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("nonempty")));
        assert_eq!(evaluator.call_indexed_direct(key, CallableKind::Pure, &[], Span::at(source_id, 0)).unwrap().unwrap(), Value::List(vec![Value::Int(1), Value::Int(1)]));
    }
}

#[test]
fn ordinary_generic_aggregate_computed_negative_indexes_fail_on_both_routes() {
    let source = "pure keep(value, index: Int) { [value][index] }\npure number(index: Int) -> Int { keep(7, index) }\n";
    let mut sources = SourceMap::default();
    let source_id = sources.add_file("generic-computed-index.xsh", source.to_string());
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let program = Arc::new(FullBuilder::build_compact(&parsed.arena, &checked, source, Arc::new(sources.clone()), source_id).unwrap());
    drop(checked);
    drop(parsed);
    for force_frames in [false, true] {
        for index in [0, -1] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("number")));
            let actual = evaluator.call_indexed_direct(key, CallableKind::Pure, &[Value::Int(index)], Span::at(source_id, 0)).unwrap();
            if index == 0 { assert_eq!(actual.unwrap(), Value::Int(7)); }
            else {
                let error = actual.unwrap_err();
                assert_eq!(error.kind, "index-out-of-range");
                assert_eq!(&source[error.span.unwrap().range()], "[value][index]");
            }
        }
    }
}

#[test]
fn ordinary_generic_guarded_receivers_preserve_null_and_present_values() {
    for (prefix, input_type, operation, present) in [
        ("", "List[Int]?", "input?[0]", Value::List(vec![Value::Int(9)])),
        ("", "List[Int]?", "input?[-1]", Value::List(vec![Value::Int(9)])),

        ("", "List[Int]?", "input?[0..1]", Value::List(vec![Value::Int(9)])),
        ("type Row = {item: Int}\n", "Row?", "input?.item", Value::Record([(Arc::from("item"), Value::Int(9))].into_iter().collect())),
        ("", "Str?", "input?.starts_with(\"v\")", Value::Str(Arc::from("value"))),
    ] {
        let source = format!("{prefix}pure keep(value, input: {input_type}) {{ let _ = {operation}; value }}\npure number(input: {input_type}) -> Int {{ keep(7, input) }}\npure text(input: {input_type}) -> Str {{ keep(\"value\", input) }}\n");
        let (program, sources, source_id) = {
            let mut sources = SourceMap::default();
            let source_id = sources.add_file("generic-guarded-receiver.xsh", source.clone());
            let parsed = Parser::parse_source_arena_only(source_id, &source);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
            assert!(checked.diagnostics.is_empty(), "{source}\n{:?}", checked.diagnostics);
            let program = FullBuilder::build_compact(&parsed.arena, &checked, &source, Arc::new(sources.clone()), source_id).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
            (Arc::new(program), sources, source_id)
        };
        for force_frames in [false, true] {
            for input in [Value::Null, present.clone()] {
                for (name, expected) in [("number", Value::Int(7)), ("text", Value::Str(Arc::from("value")))] {
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
                    evaluator.force_frames = force_frames;
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
                    let actual = evaluator.call_indexed_direct(key, CallableKind::Pure, std::slice::from_ref(&input), Span::at(source_id, 0)).unwrap().unwrap();
                    assert_eq!(actual, expected, "{operation} frames={force_frames}");
                }
            }
        }
    }
}

fn guarded_program(source: &str) -> (Arc<crate::runtime::eval::indexed::full::FullProgram>, SourceMap, SourceId) {
    let mut sources = SourceMap::default();
    let source_id = sources.add_file("generic-guarded-transport.xsh", source.to_string());
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
    let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
    assert!(checked.diagnostics.is_empty(), "{source}\n{:?}", checked.diagnostics);
    let program = FullBuilder::build_compact(&parsed.arena, &checked, source, Arc::new(sources.clone()), source_id).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
    (Arc::new(program), sources, source_id)
}

#[test]
fn ordinary_generic_guarded_operands_are_evaluated_only_when_present() {
    for (input_type, operation, present) in [
        ("List[Int]?", "input?[bad_index(1)]", Value::List(vec![Value::Int(9)])),
        ("List[Int]?", "input?[bad_index(1)..]", Value::List(vec![Value::Int(9)])),
        ("List[Int]?", "input?[..bad_index(1)]", Value::List(vec![Value::Int(9)])),
        ("Str?", "input?.starts_with(bad_needle(1))", Value::Str(Arc::from("value"))),
    ] {
        let source = format!("pure bad_index(index: Int) -> Int {{ [0][index] }}\npure bad_needle(index: Int) -> Str {{ [\"v\"][index] }}\npure keep(value, input: {input_type}) {{ let _ = {operation}; value }}\npure number(input: {input_type}) -> Int {{ keep(7, input) }}\npure text(input: {input_type}) -> Str {{ keep(\"value\", input) }}\n");
        let (program, sources, source_id) = guarded_program(&source);
        for force_frames in [false, true] {
            for input in [Value::Null, present.clone()] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
                evaluator.force_frames = force_frames;
                evaluator.indexed_program = Some(Arc::clone(&program));
                let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("number")));
                let actual = evaluator.call_indexed_direct(key, CallableKind::Pure, std::slice::from_ref(&input), Span::at(source_id, 0)).unwrap();
                if matches!(input, Value::Null) { assert_eq!(actual.unwrap(), Value::Int(7), "{operation} frames={force_frames}"); }
                else { assert_eq!(actual.unwrap_err().kind, "index-out-of-range", "{operation} frames={force_frames}"); }
            }
        }
    }
}

#[test]
fn ordinary_generic_guarded_result_receivers_preserve_error_data() {
    let source = "pure keep(value, items: Result[List[Int], Str]) -> Result[Int, Str] { let _ = value; Ok(items?[0]) }\npure number(input: Result[List[Int], Str]) -> Result[Int, Str] { keep(7, input) }\npure text(input: Result[List[Int], Str]) -> Result[Int, Str] { keep(\"value\", input) }\n";
    let (program, sources, source_id) = guarded_program(source);
    for force_frames in [false, true] {
        for (input, expected) in [
            (Value::Result(ResultValue::Ok(Box::new(Value::List(vec![Value::Int(9)])))), Value::Result(ResultValue::Ok(Box::new(Value::Int(9))))),
            (Value::Result(ResultValue::Err(Box::new(Value::Str(Arc::from("unchanged"))))), Value::Result(ResultValue::Err(Box::new(Value::Str(Arc::from("unchanged")))))),
        ] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("number")));
            let actual = evaluator.call_indexed_direct(key, CallableKind::Pure, &[input], Span::at(source_id, 0)).unwrap().unwrap();
            assert_eq!(actual, expected, "frames={force_frames}");
        }
    }
}

#[test]
fn ordinary_generic_guarded_nullable_returns_preserve_nested_result_data() {
    for (input_type, inputs) in [
        ("Int?", vec![Value::Null, Value::Int(9)]),
        ("Result[Int, Str]?", vec![Value::Null, Value::Result(ResultValue::Ok(Box::new(Value::Int(9)))), Value::Result(ResultValue::Err(Box::new(Value::Str(Arc::from("data")))))]),
    ] {
        let source = format!("pure keep(value: {input_type}, marker) {{ let _ = marker; value }}\npure number(input: {input_type}) -> {input_type} {{ keep(input, 7) }}\npure text(input: {input_type}) -> {input_type} {{ keep(input, \"value\") }}\n");
        let (program, sources, source_id) = guarded_program(&source);
        for force_frames in [false, true] {
            for input in &inputs {
                for name in ["number", "text"] {
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
                    evaluator.force_frames = force_frames;
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
                    let actual = evaluator.call_indexed_direct(key, CallableKind::Pure, std::slice::from_ref(input), Span::at(source_id, 0)).unwrap().unwrap();
                    assert_eq!(actual, *input, "{input_type} {name} frames={force_frames}");
                }
            }
        }
    }
}

#[test]
fn ordinary_generic_guarded_operation_results_preserve_optional_payloads() {
    for (input_type, result_type, operation, present, expected) in [
        ("List[Int]?", "Int?", "input?[0]", Value::List(vec![Value::Int(9)]), Value::Int(9)),
        ("List[Int?]?", "Int?", "input?[0]", Value::List(vec![Value::Null]), Value::Null),
        ("Str?", "Int?", "input?.byte_at(0)", Value::Str(Arc::from("value")), Value::Int(118)),
        ("Map[Int]?", "Int?", "input?[\"k\"]", Value::Map([(crate::map_key::MapKey::Str(Arc::from("k")), Value::Int(9))].into_iter().collect()), Value::Int(9)),
        ("Row?", "Int?", "input?[\"item\"]", Value::Record([(Arc::from("item"), Value::Int(9))].into_iter().collect()), Value::Int(9)),
        ("Str?", "Str?", "input?[0..1]", Value::Str(Arc::from("value")), Value::Str(Arc::from("v"))),
        ("Bytes?", "Bytes?", "input?[0..1]", Value::Bytes(b"value".to_vec()), Value::Bytes(b"v".to_vec())),
    ] {
        let source = format!("type Row = {{item: Int}}\npure keep(marker, input: {input_type}) {{ let _ = marker; {operation} }}\npure number(input: {input_type}) -> {result_type} {{ keep(7, input) }}\npure text(input: {input_type}) -> {result_type} {{ keep(\"value\", input) }}\n");
        let (program, sources, source_id) = guarded_program(&source);
        for force_frames in [false, true] {
            for (input, expected) in [(Value::Null, Value::Null), (present.clone(), expected.clone())] {
                for name in ["number", "text"] {
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
                    evaluator.force_frames = force_frames;
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
                    let actual = evaluator.call_indexed_direct(key, CallableKind::Pure, std::slice::from_ref(&input), Span::at(source_id, 0)).unwrap().unwrap();
                    assert_eq!(actual, expected, "{operation} {name} frames={force_frames}");
                }
            }
        }
    }
}

#[test]
fn ordinary_generic_guarded_uint_map_keys_validate_only_present_receivers() {
    let source = "pure keep(marker, items: Map[UInt, Int]?, key: Int) { let _ = items?[key]; marker }\npure number(items: Map[UInt, Int]?, key: Int) -> Int { keep(7, items, key) }\npure text(items: Map[UInt, Int]?, key: Int) -> Str { keep(\"value\", items, key) }\n";
    let (program, sources, source_id) = guarded_program(source);
    for force_frames in [false, true] {
        for (items, key) in [
            (Value::Null, -1),
            (Value::Map([(crate::map_key::MapKey::Int(0), Value::Int(9))].into_iter().collect()), 0),
            (Value::Map([(crate::map_key::MapKey::Int(0), Value::Int(9))].into_iter().collect()), -1),
        ] {
            let present = !matches!(items, Value::Null);
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let function = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("number")));
            let actual = evaluator.call_indexed_direct(function, CallableKind::Pure, &[items, Value::Int(key)], Span::at(source_id, 0)).unwrap();
            if present && key < 0 {
                let error = actual.unwrap_err();
                assert_eq!(error.kind, "schema");
                assert!(error.message.contains("UInt"), "{error:?}");
                assert_eq!(&source[error.span.unwrap().range()], "key");
            } else { assert_eq!(actual.unwrap(), Value::Int(7)); }
        }
    }
}

#[test]
fn nullable_branch_inference_executes_independent_payloads_after_frontend_disposal() {
    for (expression, present_when_true) in [
        ("if flag { value } else { null }", true),
        ("if flag { null } else { value }", false),
        ("match flag { true => value; false => null }", true),
        ("match flag { true => null; false => value }", false),
    ] {
        for body in [format!("let joined = {expression}; joined"), expression.to_string()] {
            let source = format!("pure plain(value: Any) -> Any {{ value }}\npure maybe(flag: Bool, value) {{ {body} }}\npure number(flag: Bool) -> Int? {{ maybe(flag, 7) }}\npure text(flag: Bool) -> Str? {{ maybe(flag, \"value\") }}\npure data(flag: Bool, value: Result[Int, Str]) -> Result[Int, Str]? {{ maybe(flag, value) }}\n");
            let (program, sources, source_id) = guarded_program(&source);
            for force_frames in [false, true] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
                evaluator.force_frames = force_frames;
                evaluator.indexed_program = Some(Arc::clone(&program));
                let plain = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("plain")));
                let opaque = Value::Result(ResultValue::Err(Box::new(Value::Str(Arc::from("plain")))));
                let actual = evaluator.call_indexed_direct(plain, CallableKind::Pure, std::slice::from_ref(&opaque), Span::at(source_id, 0)).unwrap().unwrap();
                assert_eq!(actual, opaque);
                for flag in [false, true] {
                    let present = flag == present_when_true;
                    for (name, args, expected) in [
                        ("number", vec![Value::Bool(flag)], Value::Int(7)),
                        ("text", vec![Value::Bool(flag)], Value::Str(Arc::from("value"))),
                        ("data", vec![Value::Bool(flag), Value::Result(ResultValue::Ok(Box::new(Value::Int(9))))], Value::Result(ResultValue::Ok(Box::new(Value::Int(9))))),
                        ("data", vec![Value::Bool(flag), Value::Result(ResultValue::Err(Box::new(Value::Str(Arc::from("data")))))], Value::Result(ResultValue::Err(Box::new(Value::Str(Arc::from("data")))))),
                    ] {
                        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
                        evaluator.force_frames = force_frames;
                        evaluator.indexed_program = Some(Arc::clone(&program));
                        let function = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
                        let actual = evaluator.call_indexed_direct(function, CallableKind::Pure, &args, Span::at(source_id, 0)).unwrap().unwrap();
                        assert_eq!(actual, if present { expected } else { Value::Null }, "{expression} {name} frames={force_frames}");
                    }
                }
            }
        }
    }
}

#[test]
fn generic_construction_values_execute_after_frontend_disposal() {
    use crate::runtime::value::{MapKey, PathValue};
    let record = |item: Value, name: &str, value: Value| Value::Map([(MapKey::Str(Arc::from("item")), item), (MapKey::Str(Arc::from(name)), value)].into_iter().collect());
    for (prefix, definition, number_type, text_type, number, text) in [
        ("type Number = {item: Int, formatted: Str}\ntype Text = {item: Str, formatted: Str}",
         "pure keep(value) { {item: value, formatted: f\"item:{2:03}\"} }", "Number", "Text",
         record(Value::Int(7), "formatted", Value::Str(Arc::from("item:002"))), record(Value::Str(Arc::from("value")), "formatted", Value::Str(Arc::from("item:002")))),
        ("type Number = {item: Int, path: Path}\ntype Text = {item: Str, path: Path}",
         "pure keep(value) { {item: value, path: fp\"{/tmp}/{\"item\"}\"} }", "Number", "Text",
         record(Value::Int(7), "path", Value::Path(PathValue::from_text("/tmp/item").unwrap())), record(Value::Str(Arc::from("value")), "path", Value::Path(PathValue::from_text("/tmp/item").unwrap()))),
        ("", "pure keep(value) { [@[value], value] }", "List[Int]", "List[Str]",
         Value::List(vec![Value::Int(7), Value::Int(7)]), Value::List(vec![Value::Str(Arc::from("value")), Value::Str(Arc::from("value"))])),
        ("type Number = {item: Int, count: Int}\ntype Text = {item: Str, count: Int}",
         "pure keep(value) { let row = {item: value, count: 0}; {count: false, ...row, count: 1} }", "Number", "Text",
         record(Value::Int(7), "count", Value::Int(1)), record(Value::Str(Arc::from("value")), "count", Value::Int(1))),
        ("", "pure keep(value) { let initial = {[0]: value}; {...initial, [1]: value} }", "Map[Int, Int]", "Map[Int, Str]",
         Value::Map([(MapKey::Int(0), Value::Int(7)), (MapKey::Int(1), Value::Int(7))].into_iter().collect()), Value::Map([(MapKey::Int(0), Value::Str(Arc::from("value"))), (MapKey::Int(1), Value::Str(Arc::from("value")))].into_iter().collect())),
        ("type Number = {item: Int, unique: Set[Int]}\ntype Text = {item: Str, unique: Set[Int]}",
         "pure keep(value) { {item: value, unique: {1, 2, 1}} }", "Number", "Text",
         record(Value::Int(7), "unique", Value::Set([MapKey::Int(1), MapKey::Int(2)].into_iter().collect())), record(Value::Str(Arc::from("value")), "unique", Value::Set([MapKey::Int(1), MapKey::Int(2)].into_iter().collect()))),
    ] {
        let source = format!("{prefix}\n{definition}\npure number() -> {number_type} {{ keep(7) }}\npure text() -> {text_type} {{ keep(\"value\") }}\n");
        let (program, sources, source_id) = {
            let mut sources = SourceMap::default();
            let source_id = sources.add_file("generic-construction.xsh", source.clone());
            let parsed = Parser::parse_source_arena_only(source_id, &source);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
            assert!(checked.diagnostics.is_empty(), "{source}\n{:?}", checked.diagnostics);
            let program = FullBuilder::build_compact(&parsed.arena, &checked, &source, Arc::new(sources.clone()), source_id).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
            (Arc::new(program), sources, source_id)
        };
        for force_frames in [false, true] {
            for (name, expected) in [("number", number.clone()), ("text", text.clone())] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
                evaluator.force_frames = force_frames;
                evaluator.indexed_program = Some(Arc::clone(&program));
                let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
                let actual = evaluator.call_indexed_direct(key, CallableKind::Pure, &[], Span::at(source_id, 0)).unwrap().unwrap();
                let actual = match actual {
                    Value::Record(fields) => Value::Map(fields.iter().map(|(name, value)| (MapKey::Str(Arc::from(name)), value.clone())).collect()),
                    actual => actual,
                };
                assert_eq!(actual, expected, "{definition} {name} frames={force_frames}");
            }
        }
    }
}

#[test]
fn generic_construction_operands_preserve_order_and_first_failure() {
    for operation in [
        "f\"{ { count = count + 1; count } }:{ { count = count + 1; count } }\"",
        "[@[{ count = count + 1; count }], { count = count + 1; count }]",
        "{first: { count = count + 1; count }, ...{first: 99}, second: { count = count + 1; count }}",
        "{...{items: [], first: { count = count + 1; count }}, ...{items: [7]}, first: 99, second: { count = count + 1; count }}",
        "{[0]: { count = count + 1; count }, [0]: { count = count + 1; count }}",
    ] {
        let source = format!("proc keep(value) [] {{ var count = 0; let built = {operation}; {{item: value, count: count, built: built}} }}\nproc number() [] {{ keep(7) }}\n");
        let mut sources = SourceMap::default();
        let source_id = sources.add_file("generic-construction-order.xsh", source.clone());
        let parsed = Parser::parse_source_arena_only(source_id, &source);
        assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
        let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        assert!(checked.diagnostics.is_empty(), "{source}\n{:?}", checked.diagnostics);
        let program = Arc::new(FullBuilder::build_compact(&parsed.arena, &checked, &source, Arc::new(sources.clone()), source_id).unwrap_or_else(|error| panic!("{source}\n{error:?}")));
        drop(checked);
        drop(parsed);
        for force_frames in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("number")));
            let actual = evaluator.call_indexed_direct(key, CallableKind::Proc, &[], Span::at(source_id, 0)).unwrap().unwrap();
            let Value::Result(ResultValue::Ok(actual)) = actual else { panic!("a proc preserves its result envelope") };
            let Value::Record(fields) = *actual else { panic!("the ordered constructor returns its counters") };
            assert_eq!(fields.get("count"), Some(&Value::Int(2)), "{operation}");
            let expected = if operation.starts_with('f') { Value::Str(Arc::from("1:2")) }
                else if operation.starts_with('[') { Value::List(vec![Value::Int(1), Value::Int(2)]) }
                else if operation.starts_with("{first:") { Value::Record([(Arc::from("first"), Value::Int(99)), (Arc::from("second"), Value::Int(2))].into_iter().collect()) }
                else if operation.starts_with("{...") { Value::Record([(Arc::from("items"), Value::List(vec![Value::Int(7)])), (Arc::from("first"), Value::Int(99)), (Arc::from("second"), Value::Int(2))].into_iter().collect()) }
                else { Value::Map([(crate::runtime::value::MapKey::Int(0), Value::Int(2))].into_iter().collect()) };
            assert_eq!(fields.get("built"), Some(&expected), "{operation}");
        }
    }
    for operation in ["{...{items: [], first: [1][first]}, ...{items: [7]}, second: [2][second]}", "f\"{[1][first]}:{[2][second]}\"", "[@[[1][first]], [2][second]]", "{...{first: [1][first]}, second: [2][second]}", "{[0]: [1][first], [1]: [2][second]}"] {
        let source = format!("pure keep(value, first: Int, second: Int) {{ let _ = value; {operation} }}\npure number() {{ keep(7, 1, 2) }}\n");
        let mut sources = SourceMap::default();
        let source_id = sources.add_file("generic-construction-failure.xsh", source.clone());
        let parsed = Parser::parse_source_arena_only(source_id, &source);
        assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
        let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        assert!(checked.diagnostics.is_empty(), "{source}\n{:?}", checked.diagnostics);
        let program = Arc::new(FullBuilder::build_compact(&parsed.arena, &checked, &source, Arc::new(sources.clone()), source_id).unwrap_or_else(|error| panic!("{source}\n{error:?}")));
        drop(checked);
        drop(parsed);
        for force_frames in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("number")));
            let error = evaluator.call_indexed_direct(key, CallableKind::Pure, &[], Span::at(source_id, 0)).unwrap().unwrap_err();
            assert_eq!(error.kind, "index-out-of-range");
            assert_eq!(&source[error.span.unwrap().range()], "[1][first]");
        }
    }
}

fn record_form_program(source: &str) -> (Arc<crate::runtime::eval::FullProgram>, SourceMap, SourceId) {
    let mut sources = SourceMap::default();
    let source_id = sources.add_file("generic-record-forms.xsh", source.to_string());
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
    let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
    assert!(checked.diagnostics.is_empty(), "{source}\n{:?}", checked.diagnostics);
    let program = FullBuilder::build_compact(&parsed.arena, &checked, source, Arc::new(sources.clone()), source_id).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
    (Arc::new(program), sources, source_id)
}

#[test]
fn selected_same_signature_stream_callback_runs_after_frontend_disposal() {
    let source = r#"
type Pair = {left: Stream[Int], right: Stream[Int]}
type Callback = pure(value: Pair) -> Stream[Int]
pure left(value: Pair) -> Stream[Int] { value.left }
pure right(value: Pair) -> Stream[Int] { value.right }
let left_callback: Callback = left
let right_callback: Callback = right
pure select(first, second, value) { second(value) }
pure relay_left(value: Pair) -> Stream[Int] { select(right, left, value) }
pure relay_right(value: Pair) -> Stream[Int] { select(left, right, value) }
pure relay_union(value: Pair, choose: Bool) -> Stream[Int] { select(left, if choose { left_callback } else { right_callback }, value) }
stream left_values() [] -> Stream[Int] { yield 11 }
stream right_values() [] -> Stream[Int] { yield 22 }
let selected_left = relay_left({left: left_values(), right: right_values()})
assert selected_left.collect() == [11]
let selected_right = relay_right({left: left_values(), right: right_values()})
assert selected_right.collect() == [22]
let selected_union_left = relay_union({left: left_values(), right: right_values()}, true)
assert selected_union_left.collect() == [11]
let selected_union_right = relay_union({left: left_values(), right: right_values()}, false)
assert selected_union_right.collect() == [22]
"#;
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        for step in 0..program.driver_step_count().unwrap() {
            evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap_or_else(|error| panic!("frames={force_frames}: {error:?}"));
        }
    }
}

#[test]
fn selected_rest_stream_callbacks_execute_after_frontend_disposal() {
    let mut source = r#"
type Pair = {left: Stream[Int], right: Stream[Int]}
pure rest_left(head: Pair, ...rows: List[Pair]) -> Stream[Int] { rows[0].left }
pure rest_right(head: Pair, ...rows: List[Pair]) -> Stream[Int] { rows[0].right }
pure select_items(first, second, head: Pair, left_row: Pair, right_row: Pair) { second(head, left_row, right_row) }
pure select_splice(first, second, head: Pair, rows: List[Pair]) { second(head, @rows) }
pure relay_items_left(head: Pair, first: Pair, second: Pair) -> Stream[Int] { select_items(rest_right, rest_left, head, first, second) }
pure relay_items_right(head: Pair, first: Pair, second: Pair) -> Stream[Int] { select_items(rest_left, rest_right, head, first, second) }
pure relay_splice_left(head: Pair, rows: List[Pair]) -> Stream[Int] { select_splice(rest_right, rest_left, head, rows) }
pure relay_splice_right(head: Pair, rows: List[Pair]) -> Stream[Int] { select_splice(rest_left, rest_right, head, rows) }
stream values(value: Int) [] -> Stream[Int] { yield value }
"#.to_string();
    let head = "{left: values(1), right: values(2)}";
    let first = "{left: values(11), right: values(12)}";
    let second = "{left: values(21), right: values(22)}";
    for (relay, splice, expected) in [
        ("relay_items_left", false, 11),
        ("relay_items_right", false, 12),
        ("relay_splice_left", true, 11),
        ("relay_splice_right", true, 12),
    ] {
        let arguments = if splice { format!("[{first}, {second}]") } else { format!("{first}, {second}") };
        source.push_str(&format!("let selected_{relay} = {relay}({head}, {arguments})\nassert selected_{relay}.collect() == [{expected}]\n"));
    }
    let (program, sources, source_id) = record_form_program(&source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        for step in 0..program.driver_step_count().unwrap() {
            evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap_or_else(|error| panic!("frames={force_frames}: {error:?}"));
        }
    }
}

#[test]
fn selected_callback_defaults_execute_after_frontend_disposal() {
    let source = r#"
type Pair = {left: Stream[Int], right: Stream[Int]}
pure fresh_values() -> Stream[Int] { range(31, 32) }
pure defaulted(head: Pair, value: Stream[Int] = fresh_values()) -> Stream[Int] { value }
pure select(first, second, head: Pair) -> Stream[Int] { second(head) }
pure direct(head: Pair) -> Stream[Int] { defaulted(head) }
pure relay(head: Pair) -> Stream[Int] { select(defaulted, defaulted, head) }
let head = {left: fresh_values(), right: fresh_values()}
let direct_values = direct(head)
let selected_values = relay(head)
assert direct_values.collect() == [31]
assert selected_values.collect() == [31]
"#;
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        for step in 0..program.driver_step_count().unwrap() {
            evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap_or_else(|error| panic!("frames={force_frames}: {error:?}"));
        }
    }
}

#[test]
fn materialized_named_callable_arguments_execute_after_frontend_disposal() {
    let source = r#"
pure identity(payload) { payload }
proc keep(action, value) { let _ = action(value); value }
proc nested(value) { let wrapped = Ok(value); keep(identity, wrapped) }
proc explicit(value) { let wrapped = Ok(value); return keep(identity, wrapped) }
proc formal(action, value) { let wrapped = Ok(value); keep(action, wrapped) }
assert (nested(7)?)? == 7
assert (nested("value")?)? == "value"
assert (explicit(8)?)? == 8
assert (formal(identity, "formal")?)? == "formal"
"#;
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        for step in 0..program.driver_step_count().unwrap() { evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap(); }
    }
}

#[test]
fn generic_optional_row_empty_fallbacks_execute_after_frontend_disposal() {
    let source = r#"
type IntRow = {items: List[Int]}
type StrRow = {items: List[Str]}
pure items(maybe) { maybe?.items ?? [] }
pure fallback(maybe, otherwise) { maybe?.items ?? otherwise }
pure forward(maybe) { items(maybe) }
pure relay(maybe) { let lookup = items; lookup(maybe) }
let numbers: IntRow? = {items: [2, 4]}
let absent: IntRow? = null
let words: StrRow? = {items: ["a", "b"]}
assert relay(numbers) == [2, 4]
assert relay(absent) == []
assert relay(words) == ["a", "b"]
assert forward(numbers) == [2, 4]
assert forward(absent) == []
assert forward(words) == ["a", "b"]
assert fallback(otherwise: [], maybe: numbers) == [2, 4]
assert fallback(otherwise: [], maybe: absent) == []
assert fallback(otherwise: [], maybe: words) == ["a", "b"]
"#;
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        for step in 0..program.driver_step_count().unwrap() { evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap(); }
    }
}

#[test]
fn generic_optional_rows_compose_empty_fallbacks_index_and_slice() {
    let source = r#"
type IntRow = {name: Str, items: List[Int]}
type StrRow = {name: Str, items: List[Str]}
pure inspect(maybe, start, end) {
  let name = maybe?.name ?? "missing"
  let items = maybe?.items ?? []
  let selected = if items.len() == 0 { null } else { items[start] }
  let slice = items[start..end]
  {name: name, selected: selected, slice: slice}
}
let numbers: IntRow? = {name: "numbers", items: [2, 4, 6]}
let absent: IntRow? = null
let words: StrRow? = {name: "words", items: ["a", "b"]}
let numeric = inspect(numbers, 1, 3)
let empty = inspect(absent, 0, 0)
let textual = inspect(words, 1, 2)
assert numeric == {name: "numbers", selected: 4, slice: [4, 6]}
assert empty == {name: "missing", selected: null, slice: []}
assert textual == {name: "words", selected: "b", slice: ["b"]}
"#;
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        for step in 0..program.driver_step_count().unwrap() { evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap(); }
    }
}

fn record_form_observable(value: Value) -> Value {
    match value {
        Value::Record(fields) => Value::Map(fields.into_iter().map(|(name, value)| (crate::runtime::value::MapKey::Str(name), record_form_observable(value))).collect()),
        value => value,
    }
}

#[test]
fn generic_record_forms_execute_after_frontend_disposal() {
    use crate::runtime::value::MapKey;
    let record = |fields: Vec<(&str, Value)>| Value::Map(fields.into_iter().map(|(name, value)| (MapKey::Str(Arc::from(name)), value)).collect());
    for (prefix, definition, number_type, text_type, number, text) in [
        ("type Number = {value: Int}\ntype Text = {value: Str}", "pure copy(value) { {value} }", "Number", "Text", record(vec![("value", Value::Int(7))]), record(vec![("value", Value::Str(Arc::from("value")))])),
        ("type Number = {item: Int}\ntype Text = {item: Str}", "pure copy(value) { let item = value; { let item = 1; let _ = {item} }; {item} }", "Number", "Text", record(vec![("item", Value::Int(7))]), record(vec![("item", Value::Str(Arc::from("value")))])),
        ("let shared = 9\ntype Number = {value: Int, shared: Int}\ntype Text = {value: Str, shared: Int}", "pure copy(value) { {value, shared} }", "Number", "Text", record(vec![("value", Value::Int(7)), ("shared", Value::Int(9))]), record(vec![("value", Value::Str(Arc::from("value"))), ("shared", Value::Int(9))])),
        ("", "pure copy(value) { {[\"other\"]: value, value} }", "Map[Str, Int]", "Map[Str, Str]", record(vec![("other", Value::Int(7)), ("value", Value::Int(7))]), record(vec![("other", Value::Str(Arc::from("value"))), ("value", Value::Str(Arc::from("value")))])),
        ("type Number = {value: Int, items: Set[Int]}\ntype Text = {value: Str, items: Set[Int]}", "pure copy(value) { let first = 1; let second = 2; let items: Set[Int] = {first, second}; {value, items} }", "Number", "Text", record(vec![("value", Value::Int(7)), ("items", Value::Set([MapKey::Int(1), MapKey::Int(2)].into_iter().collect()))]), record(vec![("value", Value::Str(Arc::from("value"))), ("items", Value::Set([MapKey::Int(1), MapKey::Int(2)].into_iter().collect()))])),
        ("type NumberInner = {item: Int, count: Int}\ntype TextInner = {item: Str, count: Int}\ntype Number = {nested: NumberInner, other: Int}\ntype Text = {nested: TextInner, other: Str}", "pure copy(value) { let row = {nested: {item: value, count: 0}, other: value}; {...row, nested.item: value, nested.count: 1} }", "Number", "Text", record(vec![("nested", record(vec![("item", Value::Int(7)), ("count", Value::Int(1))])), ("other", Value::Int(7))]), record(vec![("nested", record(vec![("item", Value::Str(Arc::from("value"))), ("count", Value::Int(1))])), ("other", Value::Str(Arc::from("value")))])),
    ] {
        let source = format!("{prefix}\n{definition}\npure number() -> {number_type} {{ copy(7) }}\npure text() -> {text_type} {{ copy(\"value\") }}\n");
        let (program, sources, source_id) = record_form_program(&source);
        for force_frames in [false, true] {
            for (name, expected) in [("number", number.clone()), ("text", text.clone())] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
                evaluator.force_frames = force_frames;
                evaluator.indexed_program = Some(Arc::clone(&program));
                for step in 0..program.driver_step_count().unwrap() { evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap(); }
                let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
                let actual = evaluator.call_indexed_direct(key, CallableKind::Pure, &[], Span::at(source_id, 0)).unwrap().unwrap();
                assert_eq!(record_form_observable(actual), expected, "{definition} {name} frames={force_frames}");
            }
        }
    }
}

#[test]
fn generic_record_forms_updates_preserve_order_failure_and_unsigned_checks() {
    let source = "proc copy(value) [] { var count = 0; let row = {nested: {first: 0, second: 0}}; let updated = {...{ count = count + 1; row }, nested.first: { count = count + 1; count }, nested.second: { count = count + 1; count }}; {value, count, updated, row} }\nproc number() [] { copy(7) }\n";
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("number")));
        let actual = evaluator.call_indexed_direct(key, CallableKind::Proc, &[], Span::at(source_id, 0)).unwrap().unwrap();
        let Value::Result(ResultValue::Ok(actual)) = actual else { panic!("the proc retains its checked result boundary") };
        let Value::Record(fields) = *actual else { panic!("the update returns its actual counter and records") };
        assert_eq!(fields.get("count"), Some(&Value::Int(3)));
        for (name, first, second) in [("updated", 2, 3), ("row", 0, 0)] {
            let Value::Record(row) = fields.get(name).unwrap() else { panic!("the record shape is preserved") };
            let Value::Record(nested) = row.get("nested").unwrap() else { panic!("the nested record shape is preserved") };
            assert_eq!(nested.get("first"), Some(&Value::Int(first)));
            assert_eq!(nested.get("second"), Some(&Value::Int(second)));
        }
    }
    for operation in ["{...{ let _ = [1][first]; row }, nested.first: [2][second]}", "{...row, nested.first: [1][first], nested.second: [2][second]}"] {
        let source = format!("pure copy(value, first: Int, second: Int) {{ let _ = value; let row = {{nested: {{first: 0, second: 0}}}}; {operation} }}\npure number() {{ copy(7, 1, 2) }}\n");
        let (program, sources, source_id) = record_form_program(&source);
        for force_frames in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("number")));
            let error = evaluator.call_indexed_direct(key, CallableKind::Pure, &[], Span::at(source_id, 0)).unwrap().unwrap_err();
            assert_eq!(error.kind, "index-out-of-range");
            assert_eq!(&source[error.span.unwrap().range()], "[1][first]");
        }
    }
    let source = "type Inner = {count: UInt}\ntype Row = {nested: Inner}\npure replace(value, row: Row, signed: Int) { let _ = value; {...row, nested.count: signed} }\npure number(signed: Int) -> UInt { replace(7, {nested: {count: 1}}, signed).nested.count }\npure text(signed: Int) -> UInt { replace(\"value\", {nested: {count: 1}}, signed).nested.count }\n";
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        for name in ["number", "text"] {
            for input in [7, -1] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
                evaluator.force_frames = force_frames;
                evaluator.indexed_program = Some(Arc::clone(&program));
                let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
                let actual = evaluator.call_indexed_direct(key, CallableKind::Pure, &[Value::Int(input)], Span::at(source_id, 0)).unwrap();
                if input == 7 { assert_eq!(actual.unwrap(), Value::Int(7)); }
                else { let error = actual.unwrap_err(); assert_eq!(error.kind, "type-error"); assert!(error.message.contains("UInt"), "{error:?}"); assert_eq!(&source[error.span.unwrap().range()], "{...row, nested.count: signed}"); }
            }
        }
    }
}

#[test]
fn generic_operators_short_circuit_preserves_lazy_failure_after_frontend_disposal() {
    for (operator, skipped, reached) in [("and", false, true), ("or", true, false)] {
        let source = format!("pure copy(value, flag: Bool, index: Int) {{ let result = flag {operator} [true][index]; {{value, result}} }}\npure number(flag: Bool, index: Int) -> Bool {{ copy(7, flag, index).result }}\npure text(flag: Bool, index: Int) -> Bool {{ copy(\"value\", flag, index).result }}\n");
        let (program, sources, source_id) = record_form_program(&source);
        for force_frames in [false, true] {
            for name in ["number", "text"] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
                evaluator.force_frames = force_frames;
                evaluator.indexed_program = Some(Arc::clone(&program));
                let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
                let skipped = evaluator.call_indexed_direct(key, CallableKind::Pure, &[Value::Bool(skipped), Value::Int(9)], Span::at(source_id, 0)).unwrap().unwrap();
                assert_eq!(skipped, Value::Bool(operator == "or"), "{operator} {name} frames={force_frames}");
                let error = evaluator.call_indexed_direct(key, CallableKind::Pure, &[Value::Bool(reached), Value::Int(9)], Span::at(source_id, 0)).unwrap().unwrap_err();
                assert_eq!(error.kind, "index-out-of-range");
                assert_eq!(&source[error.span.unwrap().range()], "[true][index]");
                let actual = evaluator.call_indexed_direct(key, CallableKind::Pure, &[Value::Bool(reached), Value::Int(0)], Span::at(source_id, 0)).unwrap().unwrap();
                assert_eq!(actual, Value::Bool(true), "{operator} {name} frames={force_frames}");
            }
        }
    }
}

#[test]
fn generic_operators_short_circuit_preserves_observable_operand_order() {
    for operator in ["and", "or"] {
        let source = format!("proc copy(value, flag: Bool) [] {{ var count = 0; let result = {{ count = count + 1; flag }} {operator} {{ count = count + 10; true }}; {{value, result, count}} }}\nproc observe(flag: Bool) [error] {{ copy(7, flag)?.count }}\n");
        let (program, sources, source_id) = record_form_program(&source);
        for force_frames in [false, true] {
            for flag in [false, true] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
                evaluator.force_frames = force_frames;
                evaluator.indexed_program = Some(Arc::clone(&program));
                let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("observe")));
                let actual = evaluator.call_indexed_direct(key, CallableKind::Proc, &[Value::Bool(flag)], Span::at(source_id, 0)).unwrap().unwrap();
                let count = if flag == (operator == "and") { 11 } else { 1 };
                assert_eq!(actual, Value::ok(Value::Int(count)), "{operator} {flag} frames={force_frames}");
            }
        }
    }
}

#[test]
fn generic_operators_selected_domains_execute_after_frontend_disposal() {
    let source = "pure inspect(value, number: Int, text: Str, left: Set[Int], right: Set[Int]) { let union = left | right; let intersection = left & right; let difference = left - right; [value == value, value != value, number < 7, text >= \"a\", number in [1, 2, 3], number not in [4, 5], \"item\" in {[\"item\"]: value}, \"item\" in {item: value}, union == {1, 2, 3}, intersection == {2,}, difference == {1,}, 1s <= 2s, b\"a\" in b\"value\"] }\npure number() -> List[Bool] { inspect(7, 2, \"value\", {1, 2}, {2, 3}) }\npure text() -> List[Bool] { inspect(\"value\", 3, \"other\", {1, 2}, {2, 3}) }\n";
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        for name in ["number", "text"] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
            let actual = evaluator.call_indexed_direct(key, CallableKind::Pure, &[], Span::at(source_id, 0)).unwrap().unwrap();
            let mut expected = vec![Value::Bool(true); 13];
            expected[1] = Value::Bool(false);
            assert_eq!(actual, Value::List(expected), "{name} frames={force_frames}");
        }
    }
}

#[test]
fn generic_operators_comparison_chains_evaluate_only_reached_operands_once() {
    let source = "proc copy(value, number: Int) [] { var count = 0; let result = { count = count + 1; 0 } <= { count = count + 10; number } < { count = count + 100; 7 }; {value, result, count} }\nproc observe(number: Int) [error] { copy(7, number)?.count }\npure choose(value, number: Int, index: Int) { let result = 0 <= number < [7][index]; {value, result} }\npure checked(number: Int, index: Int) -> Bool { choose(7, number, index).result }\n";
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        let observed = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("observe")));
        for (number, count) in [(-1, 11), (2, 111), (9, 111)] {
            let actual = evaluator.call_indexed_direct(observed, CallableKind::Proc, &[Value::Int(number)], Span::at(source_id, 0)).unwrap().unwrap();
            assert_eq!(actual, Value::ok(Value::Int(count)), "{number} frames={force_frames}");
        }
        let checked = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("checked")));
        let skipped = evaluator.call_indexed_direct(checked, CallableKind::Pure, &[Value::Int(-1), Value::Int(9)], Span::at(source_id, 0)).unwrap().unwrap();
        assert_eq!(skipped, Value::Bool(false));
        let error = evaluator.call_indexed_direct(checked, CallableKind::Pure, &[Value::Int(2), Value::Int(9)], Span::at(source_id, 0)).unwrap().unwrap_err();
        assert_eq!(error.kind, "index-out-of-range");
        assert_eq!(&source[error.span.unwrap().range()], "[7][index]");
    }
}

#[test]
fn recursive_ordering_inference_runs_after_frontend_disposal() {
    let source = "pure sum_to(value) {\n  if value <= 0 { 0 } else { value + sum_to(value - 1) }\n}\npure even(value) {\n  if value <= 0 { true } else { odd(value - 1) }\n}\npure odd(value) {\n  if value <= 0 { false } else { even(value - 1) }\n}\nassert sum_to(4) == 10\nassert even(6)\n";
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        for step in 0..program.driver_step_count().unwrap() {
            evaluator
                .eval_indexed_driver_step(step, Span::at(source_id, 0))
                .unwrap()
                .unwrap_or_else(|error| panic!("frames={force_frames}: {error:?}"));
        }
    }
}

#[test]
fn generic_operators_unsigned_map_membership_keeps_the_ordinary_validation_boundary() {
    let source = "pure selected(value, key: Int, items: Map[UInt, Int]) { let _ = value; key in items }\npure ordinary(key: Int, items: Map[UInt, Int]) -> Bool { key in items }\npure number(key: Int, items: Map[UInt, Int]) -> Bool { selected(7, key, items) }\npure text(key: Int, items: Map[UInt, Int]) -> Bool { selected(\"value\", key, items) }\n";
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        for key in [-1, 0, 3] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let arguments = [Value::Int(key), Value::Map(Default::default())];
            let ordinary = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("ordinary")));
            let expected = evaluator.call_indexed_direct(ordinary, CallableKind::Pure, &arguments, Span::at(source_id, 0)).unwrap();
            if key < 0 { assert!(expected.as_ref().unwrap_err().message.contains("UInt")); }
            else { assert_eq!(expected.as_ref().unwrap(), &Value::Bool(false)); }
            for name in ["number", "text"] {
                let selected = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
                let actual = evaluator.call_indexed_direct(selected, CallableKind::Pure, &arguments, Span::at(source_id, 0)).unwrap();
                match (&expected, actual) {
                    (Ok(expected), Ok(actual)) => assert_eq!(&actual, expected),
                    (Err(expected), Err(actual)) => { assert_eq!(actual.kind, expected.kind); assert_eq!(actual.message, expected.message); assert_eq!(actual.family, expected.family); }
                    (expected, actual) => panic!("{name} {key} frames={force_frames}: expected {expected:?}, found {actual:?}"),
                }
            }
        }
    }
}

#[test]
fn generic_iteration_destructured_sources_preserve_order_and_binder_identity() {
    let source = "proc copy(value) [] { var count = 0; let {item: original} = { count = count + 1; {item: value} }; var {item: changed, nested: {copy}} = { count = count + 10; {item: original, nested: {copy: original}} }; changed = value; let result = { let {item: original} = {item: copy}; original }; {value: result, changed, count} }\nproc number() [] { copy(7) }\nproc text() [] { copy(\"value\") }\n";
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        for (name, expected) in [("number", Value::Int(7)), ("text", Value::Str(Arc::from("value")))] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
            let actual = evaluator.call_indexed_direct(key, CallableKind::Proc, &[], Span::at(source_id, 0)).unwrap().unwrap();
            let Value::Result(ResultValue::Ok(actual)) = actual else { panic!("the checked proc retains its success boundary") };
            let Value::Record(fields) = *actual else { panic!("the checked result remains a record") };
            assert_eq!(fields.get("value"), Some(&expected));
            assert_eq!(fields.get("changed"), Some(&expected));
            assert_eq!(fields.get("count"), Some(&Value::Int(11)));
        }
    }
}

#[test]
fn generic_iteration_for_sources_evaluate_once_and_keep_the_snapshot() {
    let source = "proc copy(value) [] { var count = 0; var rows = [value, value]; var result = value; for item in { count = count + 1; rows } { count = count + 10; result = item; rows = [] }; {value: result, count} }\nproc number() [] { copy(7) }\nproc text() [] { copy(\"value\") }\n";
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        for (name, expected) in [("number", Value::Int(7)), ("text", Value::Str(Arc::from("value")))] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
            let actual = evaluator.call_indexed_direct(key, CallableKind::Proc, &[], Span::at(source_id, 0)).unwrap().unwrap();
            let Value::Result(ResultValue::Ok(actual)) = actual else { panic!("the checked proc retains its success boundary") };
            let Value::Record(fields) = *actual else { panic!("the checked result remains a record") };
            assert_eq!(fields.get("value"), Some(&expected));
            assert_eq!(fields.get("count"), Some(&Value::Int(21)));
        }
    }
}

#[test]
fn generic_iteration_finite_sources_preserve_canonical_item_order() {
    for (ty, value, item, expected) in [
        ("List[Int]", "[2, 1]", "item", 21),
        ("Map[Str, Int]", "{[\"b\"]: 2, [\"a\"]: 1}", "item.value", 12),
        ("Str", "\"x🙂\"", "if item == \"x\" { 1 } else { 2 }", 12),
        ("Bytes", "b\"ab\"", "item", 1068),
        ("Set[Int]", "{2, 1}", "item", 12),
    ] {
        for wrapped in [false, true] {
            let ty = if wrapped { format!("Result[{ty}, Error]") } else { ty.to_string() };
            let value = if wrapped { format!("Ok({value})") } else { value.to_string() };
            let source = format!("proc copy(marker, rows: {ty}) [error] {{ var count = 0; var ordered = 0; for item in rows {{ count = count + 1; let selected = {item}; ordered = ordered * 10 + selected }}; {{marker, count, ordered}} }}\nproc number() [error] {{ copy(7, {value}) }}\nproc text() [error] {{ copy(\"value\", {value}) }}\n");
            let (program, sources, source_id) = record_form_program(&source);
            for force_frames in [false, true] {
                for (name, marker) in [("number", Value::Int(7)), ("text", Value::Str(Arc::from("value")))] {
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
                    evaluator.force_frames = force_frames;
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
                    let actual = evaluator.call_indexed_direct(key, CallableKind::Proc, &[], Span::at(source_id, 0)).unwrap().unwrap_or_else(|error| panic!("{ty} frames={force_frames}: {error:?}"));
                    let Value::Result(ResultValue::Ok(actual)) = actual else { panic!("the loop retains its checked result boundary") };
                    let Value::Record(fields) = *actual else { panic!("the loop returns its observations") };
                    assert_eq!(fields.get("marker"), Some(&marker), "{ty} frames={force_frames}");
                    assert_eq!(fields.get("count"), Some(&Value::Int(2)), "{ty} frames={force_frames}");
                    assert_eq!(fields.get("ordered"), Some(&Value::Int(expected)), "{ty} frames={force_frames}");
                }
            }
        }
    }
}

#[test]
fn generic_iteration_result_sources_preserve_the_actual_error_payload() {
    for item in ["List[Int]", "Map[Str, Int]", "Str", "Bytes", "Set[Int]"] {
        let source = format!("error IterationFailure = Missing(message: Str) : NotFound\nproc copy(marker, rows: Result[{item}, IterationFailure]) [error] {{ for item in rows {{ let _ = item }}; marker }}\nproc number() [error] {{ copy(7, Err(.Missing(message: \"source failed\"))) }}\nproc text() [error] {{ copy(\"value\", Err(.Missing(message: \"source failed\"))) }}\n");
        let (program, sources, source_id) = record_form_program(&source);
        for force_frames in [false, true] {
            for name in ["number", "text"] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
                evaluator.force_frames = force_frames;
                evaluator.indexed_program = Some(Arc::clone(&program));
                let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
                let actual = evaluator.call_indexed_direct(key, CallableKind::Proc, &[], Span::at(source_id, 0)).unwrap().unwrap();
                let Value::Result(ResultValue::Err(error)) = actual else { panic!("the source failure must propagate") };
                let Value::Error(error) = *error else { panic!("the nominal source failure remains an error value") };
                assert_eq!(error.family, "IterationFailure", "{item} frames={force_frames}");
                assert_eq!(error.variant, "Missing");
                assert_eq!(error.message, "source failed");
            }
        }
    }
}

#[test]
fn generic_iteration_stream_break_stops_pulling_and_releases_the_source() {
    let source = "var pulls = 0\nvar closed = 0\nproc close() [] { closed = closed + 1 }\nstream rows() [error] -> Stream[Int] { defer close(); pulls = pulls + 1; yield 1; pulls = pulls + 100; yield [1][9] }\nproc copy(marker, source: Stream[Int]) [error] { for item in source { let _ = item; break }; marker }\nproc number() [error] { let value = copy(7, rows())?; {value, pulls, closed} }\nproc text() [error] { let value = copy(\"value\", rows())?; {value, pulls, closed} }\n";
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        for (name, expected) in [("number", Value::Int(7)), ("text", Value::Str(Arc::from("value")))] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            for step in 0..program.driver_step_count().unwrap() { evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap(); }
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
            let actual = evaluator.call_indexed_direct(key, CallableKind::Proc, &[], Span::at(source_id, 0)).unwrap().unwrap();
            let Value::Result(ResultValue::Ok(actual)) = actual else { panic!("the loop retains its checked result boundary") };
            let Value::Record(fields) = *actual else { panic!("the loop returns its observations") };
            assert_eq!(fields.get("value"), Some(&expected));
            assert_eq!(fields.get("pulls"), Some(&Value::Int(1)), "frames={force_frames}");
            assert_eq!(fields.get("closed"), Some(&Value::Int(1)), "frames={force_frames}");
        }
    }
}

#[test]
fn generic_iteration_result_streams_keep_pulls_lazy_and_preserve_source_errors() {
    let source = "error IterationFailure = Missing(message: Str) : NotFound\nvar created = 0\nvar pulls = 0\nvar closed = 0\nproc close() [] { closed = closed + 1 }\nstream rows() [error] -> Stream[Int] { defer close(); pulls = pulls + 1; yield 1; pulls = pulls + 100; yield [1][9] }\nproc source(fail: Bool) [error] -> Result[Stream[Int], IterationFailure] { created = created + 1; if fail { Err(.Missing(message: \"source failed\")) } else { Ok(rows()) } }\nproc copy(marker, source: Result[Stream[Int], IterationFailure]) [error] { for item in source { let _ = item; break }; marker }\nproc number() [error] { let value = copy(7, source(false))?; {value, created, pulls, closed} }\nproc text() [error] { let value = copy(\"value\", source(false))?; {value, created, pulls, closed} }\nproc failed() [error] { copy(7, source(true)) }\n";
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        for (name, expected) in [("number", Some(Value::Int(7))), ("text", Some(Value::Str(Arc::from("value")))), ("failed", None)] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            for step in 0..program.driver_step_count().unwrap() { evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap(); }
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
            let actual = evaluator.call_indexed_direct(key, CallableKind::Proc, &[], Span::at(source_id, 0)).unwrap().unwrap_or_else(|error| panic!("{name} frames={force_frames}: {error:?}"));
            if let Some(expected) = expected {
                let Value::Result(ResultValue::Ok(actual)) = actual else { panic!("the checked loop succeeds") };
                let Value::Record(fields) = *actual else { panic!("the loop returns its observations") };
                assert_eq!(fields.get("value"), Some(&expected));
                assert_eq!(fields.get("created"), Some(&Value::Int(1)));
                assert_eq!(fields.get("pulls"), Some(&Value::Int(1)));
                assert_eq!(fields.get("closed"), Some(&Value::Int(1)));
            } else {
                let Value::Result(ResultValue::Err(error)) = actual else { panic!("the source failure propagates") };
                let Value::Error(error) = *error else { panic!("the error preserves its nominal payload") };
                assert_eq!((error.family.as_str(), error.variant.as_str(), error.message.as_str()), ("IterationFailure", "Missing", "source failed"));
                for (name, expected) in [("created", 1), ("pulls", 0), ("closed", 0)] {
                    let value = program.symbol_owner().with_current(|| evaluator.lookup(Name::intern(name)).map(|binding| binding.value.clone()));
                    assert_eq!(value, Some(Value::Int(expected)));
                }
            }
        }
    }
}

#[test]
fn generic_iteration_result_maps_preserve_the_payload_binder() {
    let source = "pure copy(value) { var result = value; for {value: item} in Ok({[\"key\"]: value}) { result = item }; Ok(result) }\npure number() -> Result[Int] { copy(7) }\npure text() -> Result[Str] { copy(\"value\") }\n";
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        for (name, expected) in [("number", Value::Int(7)), ("text", Value::Str(Arc::from("value")))] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
            let actual = evaluator.call_indexed_direct(key, CallableKind::Pure, &[], Span::at(source_id, 0)).unwrap().unwrap();
            let Value::Result(ResultValue::Ok(actual)) = actual else { panic!("the iterable retains its checked success boundary") };
            assert_eq!(*actual, expected, "{name} frames={force_frames}");
        }
    }
}

#[test]
fn generic_comprehension_constructions_preserve_the_checked_payload_relationships() {
    use crate::runtime::value::MapKey;
    let record = |value, field: &str, payload| Value::Map([(MapKey::Str(Arc::from("value")), value), (MapKey::Str(Arc::from(field)), payload)].into_iter().collect());
    for (prefix, definition, number_type, text_type, suffix, number, text) in [
        ("", "pure copy(value) { [item for item in [value, value]] }", "List[Int]", "List[Str]", "", Value::List(vec![Value::Int(7), Value::Int(7)]), Value::List(vec![Value::Str(Arc::from("value")), Value::Str(Arc::from("value"))])),
        ("", "pure copy(value) { [inner for outer in [value] for {item: inner} in [{item: outer}]] }", "List[Int]", "List[Str]", "", Value::List(vec![Value::Int(7)]), Value::List(vec![Value::Str(Arc::from("value"))])),
        ("", "pure copy(value) { [item for item in [value] for item in [item]] }", "List[Int]", "List[Str]", "", Value::List(vec![Value::Int(7)]), Value::List(vec![Value::Str(Arc::from("value"))])),
        ("", "pure copy(value) { {key: value for key in [\"b\", \"a\"]} }", "Map[Str, Int]", "Map[Str, Str]", "", Value::Map([(MapKey::Str(Arc::from("a")), Value::Int(7)), (MapKey::Str(Arc::from("b")), Value::Int(7))].into_iter().collect()), Value::Map([(MapKey::Str(Arc::from("a")), Value::Str(Arc::from("value"))), (MapKey::Str(Arc::from("b")), Value::Str(Arc::from("value")))].into_iter().collect())),
        ("type Number = {value: Int, unique: Set[Int]}\ntype Text = {value: Str, unique: Set[Int]}", "pure copy(value) { let unique = {item for item in [2, 1, 2]}; {value, unique} }", "Number", "Text", "", record(Value::Int(7), "unique", Value::Set([MapKey::Int(1), MapKey::Int(2)].into_iter().collect())), record(Value::Str(Arc::from("value")), "unique", Value::Set([MapKey::Int(1), MapKey::Int(2)].into_iter().collect()))),
        ("type Number = {value: Int, copied: NonEmpty[Int]}\ntype Text = {value: Str, copied: NonEmpty[Int]}", "pure copy(value, items: NonEmpty[Int]) { let copied = [item for item in items]; {value, copied} }", "Number", "Text", ", [1, 2]", record(Value::Int(7), "copied", Value::List(vec![Value::Int(1), Value::Int(2)])), record(Value::Str(Arc::from("value")), "copied", Value::List(vec![Value::Int(1), Value::Int(2)]))),
        ("", "pure copy(value) { Ok([item.value for item in Ok({[\"key\"]: value})]) }", "Result[List[Int]]", "Result[List[Str]]", "", Value::Result(ResultValue::Ok(Box::new(Value::List(vec![Value::Int(7)])))), Value::Result(ResultValue::Ok(Box::new(Value::List(vec![Value::Str(Arc::from("value"))]))))),
    ] {
        let source = format!("{prefix}\n{definition}\npure number() -> {number_type} {{ copy(7{suffix}) }}\npure text() -> {text_type} {{ copy(\"value\"{suffix}) }}\n");
        let (program, sources, source_id) = record_form_program(&source);
        for force_frames in [false, true] {
            for (name, expected) in [("number", number.clone()), ("text", text.clone())] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
                evaluator.force_frames = force_frames;
                evaluator.indexed_program = Some(Arc::clone(&program));
                let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
                let actual = evaluator.call_indexed_direct(key, CallableKind::Pure, &[], Span::at(source_id, 0)).unwrap().unwrap();
                assert_eq!(record_form_observable(actual), expected, "{definition} {name} frames={force_frames}");
            }
        }
    }
}

#[test]
fn generic_comprehension_qualifiers_keep_nested_evaluation_and_failure_lazy() {
    let source = "proc copy(value, selected: Bool) [error] { var trace = 0; let rows = [{ trace = trace * 10 + 4; item } for outer in { trace = trace * 10 + 1; [value, value] } if { trace = trace * 10 + 2; selected } for item in { trace = trace * 10 + 3; if selected { [outer] } else { let _ = [outer][9]; [outer] } }]; {rows, trace} }\nproc number() [error] { copy(7, true) }\nproc text() [error] { copy(\"value\", false) }\n";
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        for (name, rows, trace) in [("number", vec![Value::Int(7), Value::Int(7)], 1234234), ("text", Vec::new(), 122)] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
            let actual = evaluator.call_indexed_direct(key, CallableKind::Proc, &[], Span::at(source_id, 0)).unwrap().unwrap();
            let Value::Result(ResultValue::Ok(actual)) = actual else { panic!("the selected qualifier path succeeds") };
            let Value::Record(fields) = *actual else { panic!("the comprehension returns its observations") };
            assert_eq!(fields.get("rows"), Some(&Value::List(rows)), "{name} frames={force_frames}");
            assert_eq!(fields.get("trace"), Some(&Value::Int(trace)), "{name} frames={force_frames}");
        }
    }
}

#[test]
fn generic_comprehension_status_filters_keep_exit_success_selection() {
    let source = "pure copy(value, selected: Status) { [item for item in [value] if selected] }\npure number(selected: Status) -> List[Int] { copy(7, selected) }\npure text(selected: Status) -> List[Str] { copy(\"value\", selected) }\n";
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        for code in [0, 7] {
            for (name, value) in [("number", Value::Int(7)), ("text", Value::Str(Arc::from("value")))] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
                evaluator.force_frames = force_frames;
                evaluator.indexed_program = Some(Arc::clone(&program));
                let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
                let status = Value::Status(crate::runtime::process::ProcessStatus::exited(code));
                let actual = evaluator.call_indexed_direct(key, CallableKind::Pure, &[status], Span::at(source_id, 0)).unwrap().unwrap();
                assert_eq!(actual, Value::List(if code == 0 { vec![value] } else { Vec::new() }), "{name} status={code} frames={force_frames}");
            }
        }
    }
}

#[test]
fn generic_comprehension_streams_preserve_source_projection_and_cleanup_boundaries() {
    let source = "error SourceFailure = Missing(message: Str) : NotFound\nvar created = 0\nvar pulls = 0\nvar closed = 0\nproc close() [] { closed = closed + 1 }\nstream items() [error] -> Stream[Int] { defer close(); pulls = pulls + 1; yield 1; pulls = pulls + 1; yield 2 }\nproc source(fail: Bool) [error] -> Result[Stream[Int], SourceFailure] { created = created + 1; if fail { Err(.Missing(message: \"source failed\")) } else { Ok(items()) } }\nproc copy(marker, source_fails: Bool, projection_fails: Bool) [error] { [ { let _ = [marker][if projection_fails { 9 } else { 0 }]; marker } for item in source(source_fails) ] }\nproc number() [error] { let rows = copy(7, false, false)?; {rows, created, pulls, closed} }\nproc text() [error] { let rows = copy(\"value\", false, false)?; {rows, created, pulls, closed} }\nproc source_failed() [error] { copy(7, true, false) }\nproc projection_failed() [error] { copy(7, false, true) }\n";
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        for (name, marker) in [("number", Some(Value::Int(7))), ("text", Some(Value::Str(Arc::from("value")))), ("source_failed", None), ("projection_failed", None)] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            for step in 0..program.driver_step_count().unwrap() { evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap(); }
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
            let actual = evaluator.call_indexed_direct(key, CallableKind::Proc, &[], Span::at(source_id, 0)).unwrap();
            if let Some(marker) = marker {
                let Value::Result(ResultValue::Ok(actual)) = actual.unwrap() else { panic!("the comprehension succeeds") };
                let Value::Record(fields) = *actual else { panic!("the ground caller returns its observations") };
                assert_eq!(fields.get("rows"), Some(&Value::List(vec![marker.clone(), marker])));
                assert_eq!(fields.get("created"), Some(&Value::Int(1)));
                assert_eq!(fields.get("pulls"), Some(&Value::Int(2)));
                assert_eq!(fields.get("closed"), Some(&Value::Int(1)));
            } else {
                let (pulls, closed) = if name == "source_failed" {
                    let Value::Result(ResultValue::Err(error)) = actual.unwrap() else { panic!("the selected failure propagates") };
                    let Value::Error(error) = *error else { panic!("the original nominal error remains data") };
                    assert_eq!((error.family.as_str(), error.variant.as_str(), error.message.as_str()), ("SourceFailure", "Missing", "source failed"));
                    (0, 0)
                } else {
                    let error = actual.unwrap_err();
                    assert_eq!(error.kind, "index-out-of-range");
                    assert_eq!(&source[error.span.unwrap().range()], "[marker][if projection_fails { 9 } else { 0 }]");
                    (1, 1)
                };
                for (name, expected) in [("created", 1), ("pulls", pulls), ("closed", closed)] {
                    let actual = program.symbol_owner().with_current(|| evaluator.lookup(Name::intern(name)).map(|binding| binding.value.clone()));
                    assert_eq!(actual, Some(Value::Int(expected)));
                }
            }
        }
    }
}

#[test]
fn generic_comprehension_unsigned_keys_keep_the_ordinary_checked_elaboration() {
    use crate::runtime::value::MapKey;
    let source = "pure copy(value, keys: List[UInt]) { {key: value for key in keys} }\npure number(keys: List[UInt]) -> Map[UInt, Int] { copy(7, keys) }\npure text(keys: List[UInt]) -> Map[UInt, Str] { copy(\"value\", keys) }\n";
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        for (name, value) in [("number", Value::Int(7)), ("text", Value::Str(Arc::from("value")))] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
            let actual = evaluator.call_indexed_direct(key, CallableKind::Pure, &[Value::List(vec![Value::Int(1), Value::Int(2)])], Span::at(source_id, 0)).unwrap().unwrap();
            assert_eq!(actual, Value::Map([(MapKey::Int(1), value.clone()), (MapKey::Int(2), value)].into_iter().collect()), "{name} frames={force_frames}");
        }
    }
}

#[test]
fn generic_assignment_selected_paths_preserve_the_payload_relationships() {
    for (definition, arguments, selection, replaced) in [
        ("pure replace(value, other) { var items = [value]; items[0] = other; items[0] }", "", "", true),
        ("pure replace(value, other) { var row = {nested: {item: value}, count: 1}; row.nested.item = other; row.nested.item }", "", "", true),
        ("pure replace(value, other, key: Str) { var items = {[key]: value}; items[key] = other; items[key] }", ", \"key\"", "", true),
        ("pure replace(value, other, index: Int) { var items = [{nested: [value]}]; items[index].nested[0] = other; items[index].nested[0] }", ", 0", "", true),
        ("pure replace(value, other) { var items = [value]; items += [other]; items }", "", "[1]", true),
        ("pure replace(value, other) { var items = [value]; let before = items; items[0] = other; {before, after: items} }", "", ".before[0]", false),
        ("pure replace(value, other) { var items = [value]; let before = items; items[0] = other; {before, after: items} }", "", ".after[0]", true),
    ] {
        let source = format!("{definition}\npure number() -> Int {{ replace(7, 8{arguments}){selection} }}\npure text() -> Str {{ replace(\"value\", \"other\"{arguments}){selection} }}\n");
        let (program, sources, source_id) = record_form_program(&source);
        for force_frames in [false, true] {
            for (name, expected) in [("number", Value::Int(if replaced { 8 } else { 7 })), ("text", Value::Str(Arc::from(if replaced { "other" } else { "value" })))] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
                evaluator.force_frames = force_frames;
                evaluator.indexed_program = Some(Arc::clone(&program));
                let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
                let actual = evaluator.call_indexed_direct(key, CallableKind::Pure, &[], Span::at(source_id, 0)).unwrap().unwrap();
                assert_eq!(actual, expected, "{definition} {name} frames={force_frames}");
            }
        }
    }
}

#[test]
fn generic_assignment_selectors_precede_rhs_and_compounds_read_after_rhs() {
    let source = "pure replace(value, other) { var trace = 0; var items = [[value]]; items[{trace = trace * 10 + 1; 0}][{trace = trace * 10 + 2; 0}] = {trace = trace * 10 + 3; other}; var count = 1; count += {count = 10; 2}; count -= 1; count *= 2; count /= 2; count %= 7; {value: items[0][0], trace, count} }\npure number() { replace(7, 8) }\npure text() { replace(\"value\", \"other\") }\n";
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        for (name, expected) in [("number", Value::Int(8)), ("text", Value::Str(Arc::from("other")))] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
            let Value::Record(actual) = evaluator.call_indexed_direct(key, CallableKind::Pure, &[], Span::at(source_id, 0)).unwrap().unwrap() else { panic!("the caller returns its checked observations") };
            assert_eq!(actual.get("value"), Some(&expected), "{name} frames={force_frames}");
            assert_eq!(actual.get("trace"), Some(&Value::Int(123)), "{name} frames={force_frames}");
            assert_eq!(actual.get("count"), Some(&Value::Int(4)), "{name} frames={force_frames}");
        }
    }
}

#[test]
fn generic_assignment_mixed_record_unsigned_fields_keep_the_actual_boundary() {
    for definition in [
        "pure update(value, initial: UInt, delta: Int) { var row = {value, count: initial}; row.count += delta; row }",
        "pure update(value, initial: UInt, delta: Int) { var {item: row} = {item: {value, count: initial}}; row.count += delta; row }",
    ] {
        let source = format!("{definition}\npure number(initial: UInt, delta: Int) -> UInt {{ update(7, initial, delta).count }}\npure text(initial: UInt, delta: Int) -> UInt {{ update(\"value\", initial, delta).count }}\n");
        let (program, sources, source_id) = record_form_program(&source);
        for force_frames in [false, true] {
            for name in ["number", "text"] {
                for delta in [2, -2] {
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
                    evaluator.force_frames = force_frames;
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
                    let actual = evaluator.call_indexed_direct(key, CallableKind::Pure, &[Value::Int(1), Value::Int(delta)], Span::at(source_id, 0)).unwrap();
                    if delta == 2 { assert_eq!(actual.unwrap(), Value::Int(3), "{name} frames={force_frames}"); }
                    else { assert_eq!(actual.unwrap_err().kind, "type-error", "{name} frames={force_frames}"); }
                }
            }
        }
    }
}

#[test]
fn generic_assignment_failures_return_no_partial_value_and_preserve_caller_aliases() {
    for (operator, rhs, failure) in [("=", "other", "index-out-of-range"), ("/=", "0", "division-by-zero")] {
        let source = format!("pure copy(marker, items: List[Int], index: Int, other: Int) {{ let _ = marker; var local = items; local[index] {operator} {rhs}; local }}\npure number(original: List[Int], index: Int) -> List[Int] {{ copy(7, original, index, 9) }}\npure text(original: List[Int], index: Int) -> List[Int] {{ copy(\"value\", original, index, 9) }}\n");
        let (program, sources, source_id) = record_form_program(&source);
        for force_frames in [false, true] {
            for name in ["number", "text"] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
                evaluator.force_frames = force_frames;
                evaluator.indexed_program = Some(Arc::clone(&program));
                let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
                let original = Value::List(vec![Value::Int(1), Value::Int(2)]);
                let index = if operator == "=" { 9 } else { 0 };
                let arguments = [original.clone(), Value::Int(index)];
                let error = evaluator.call_indexed_direct(key, CallableKind::Pure, &arguments, Span::at(source_id, 0)).unwrap().unwrap_err();
                assert_eq!(error.kind, failure, "{operator} {name} frames={force_frames}");
                assert_eq!(arguments[0], original, "{operator} {name} frames={force_frames}");
                assert_eq!(original, Value::List(vec![Value::Int(1), Value::Int(2)]));
            }
        }
    }
}

#[test]
fn generic_assignment_unsigned_map_selectors_keep_the_checked_key_domain() {
    let source = "pure put(value, other, key: UInt, index: Int) { var items = {[key]: value}; items[index] = other; items[key] }\npure ordinary(key: UInt, index: Int) -> Int { var items = {[key]: 1}; items[index] = 2; items[key] }\npure number(key: UInt, index: Int) -> Int { put(7, 8, key, index) }\npure text(key: UInt, index: Int) -> Str { put(\"value\", \"other\", key, index) }\n";
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        for index in [0, -1] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let arguments = [Value::Int(0), Value::Int(index)];
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("ordinary")));
            let ordinary = evaluator.call_indexed_direct(key, CallableKind::Pure, &arguments, Span::at(source_id, 0)).unwrap();
            for (name, expected) in [("number", Value::Int(8)), ("text", Value::Str(Arc::from("other")))] {
                let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
                let actual = evaluator.call_indexed_direct(key, CallableKind::Pure, &arguments, Span::at(source_id, 0)).unwrap();
                if index == 0 { assert_eq!(actual.unwrap(), expected, "{name} frames={force_frames}"); }
                else {
                    let error = actual.unwrap_err();
                    let expected = ordinary.as_ref().unwrap_err();
                    assert_eq!((&error.kind, &error.family, &error.message), (&expected.kind, &expected.family, &expected.message), "{name} frames={force_frames}");
                    assert!(error.message.contains("UInt"));
                }
            }
        }
    }
}

#[test]
fn generic_assignment_validated_and_scalar_compounds_keep_their_domains() {
    let source = "pure update(marker, items: NonEmpty[Int], text: Str, period: Duration, ratio: Float) { var names = items; names += [3]; var word = text; word += \"!\"; var elapsed = period; elapsed *= 2; elapsed /= 2; var rate = ratio; rate += 1.0; rate -= 1.0; rate *= 2.0; rate /= 2.0; {marker, names, word, elapsed, rate} }\npure number(items: NonEmpty[Int]) { update(7, items, \"value\", 1s, 2.0) }\npure text(items: NonEmpty[Int]) { update(\"value\", items, \"value\", 1s, 2.0) }\n";
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        for (name, marker) in [("number", Value::Int(7)), ("text", Value::Str(Arc::from("value")))] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
            let Value::Record(actual) = evaluator.call_indexed_direct(key, CallableKind::Pure, &[Value::List(vec![Value::Int(1)])], Span::at(source_id, 0)).unwrap().unwrap() else { panic!("compound updates retain their checked record") };
            assert_eq!(actual.get("marker"), Some(&marker));
            assert_eq!(actual.get("names"), Some(&Value::List(vec![Value::Int(1), Value::Int(3)])));
            assert_eq!(actual.get("word"), Some(&Value::Str(Arc::from("value!"))));
            assert_eq!(actual.get("rate"), Some(&Value::Float(crate::runtime::value::FloatValue::new(2.0))));
            assert_eq!(actual.get("elapsed"), Some(&Value::Duration(crate::runtime::value::DurationValue { millis: 1_000 })));
        }
    }
}

fn argument_calls_preserve_payloads(definition: &str) {
    let source = format!("{definition}\npure number() -> Int {{ call(7) }}\npure text() -> Str {{ call(\"value\") }}\n");
    let (program, sources, source_id) = record_form_program(&source);
    for force_frames in [false, true] {
        for (name, expected) in [("number", Value::Int(7)), ("text", Value::Str(Arc::from("value")))] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
            let actual = evaluator.call_indexed_direct(key, CallableKind::Pure, &[], Span::at(source_id, 0)).unwrap().unwrap();
            assert_eq!(actual, expected, "{definition} {name} frames={force_frames}");
        }
    }
}

#[test]
fn generic_arguments_spreads_and_rest_execute_after_frontend_disposal() {
    for definition in [
        "pure select(value, flag: Bool) { let _ = flag; value }\npure call(value) { select(...{flag: true, value: value}) }",
        "pure select(value, flag: Bool) { let _ = flag; value }\npure call(value) { let args = {flag: true, value: value}; select(...args) }",
        "pure head(value, ...items: List[Int]) { let _ = items; value }\npure call(value) { let _ = head(value); head(value, 1, 2) }",
        "pure head(value, ...items: List[Int]) { let _ = items; value }\npure call(value) { let tail = [1, 2]; head(value, @tail) }",
        "pure head(value, flag: Bool, ...items: List[Int]) { let _ = flag; let _ = items; value }\npure call(value) { let tail = [1, 2]; head(...{flag: true, value: value}, @tail) }",
    ] { argument_calls_preserve_payloads(definition); }
}

#[test]
fn generic_arguments_defaults_execute_after_frontend_disposal() {
    for definition in [
        "pure select(value, flag: Bool = false) { let _ = flag; value }\npure call(value) { select(value) }",
        "pure select(value, flag: Bool = false) { let _ = flag; value }\npure call(value) { select(flag: true, value: value) }",
        "pure call(value, flag = false) { let _ = flag; value }",
    ] { argument_calls_preserve_payloads(definition); }
}

#[test]
fn generic_arguments_defaults_keep_the_declaration_scope() {
    let source = "let fallback = 7\npure select(value, count: Int = fallback, trailing: Int = 0) { let _ = trailing; {value, count} }\npure call(value) { let fallback = 99; let _ = fallback; select(trailing: 1, value: value) }\npure number() { call(7) }\npure text() { call(\"value\") }\n";
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        for (name, expected) in [("number", Value::Int(7)), ("text", Value::Str(Arc::from("value")))] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            for step in 0..program.driver_step_count().unwrap() { evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap(); }
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
            let Value::Record(fields) = evaluator.call_indexed_direct(key, CallableKind::Pure, &[], Span::at(source_id, 0)).unwrap().unwrap() else { panic!("the selected default stays in the returned record") };
            assert_eq!(fields.get("value"), Some(&expected), "{name} frames={force_frames}");
            assert_eq!(fields.get("count"), Some(&Value::Int(7)), "{name} frames={force_frames}");
        }
    }
}

#[test]
fn generic_arguments_spreads_and_splices_keep_source_entry_order() {
    let source = "pure select(value, first: Int, ...items: List[Int]) { {value, first, items} }\npure call(value) { var trace = 0; let result = select(...{value: {trace = trace * 10 + 1; value}, first: {trace = trace * 10 + 2; 2}}, @{trace = trace * 10 + 3; [3, 4]}); {value: result.value, trace, first: result.first, items: result.items} }\npure number() { call(7) }\npure text() { call(\"value\") }\n";
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        for (name, expected) in [("number", Value::Int(7)), ("text", Value::Str(Arc::from("value")))] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
            let Value::Record(fields) = evaluator.call_indexed_direct(key, CallableKind::Pure, &[], Span::at(source_id, 0)).unwrap().unwrap() else { panic!("the call returns its checked observations") };
            assert_eq!(fields.get("value"), Some(&expected), "{name} frames={force_frames}");
            assert_eq!(fields.get("trace"), Some(&Value::Int(123)), "{name} frames={force_frames}");
            assert_eq!(fields.get("first"), Some(&Value::Int(2)), "{name} frames={force_frames}");
            assert_eq!(fields.get("items"), Some(&Value::List(vec![Value::Int(3), Value::Int(4)])), "{name} frames={force_frames}");
        }
    }
}

#[test]
fn generic_arguments_ordinary_interior_defaults_keep_the_declaration_scope() {
    let source = "let fallback = 7\npure select(value: Int, count: Int = fallback, trailing: Int = 0) { let _ = trailing; {value, count} }\npure call(value: Int) { let fallback = 99; let _ = fallback; select(trailing: 1, value: value) }\npure number() { call(7) }\n";
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        for step in 0..program.driver_step_count().unwrap() { evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap(); }
        let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("number")));
        let Value::Record(fields) = evaluator.call_indexed_direct(key, CallableKind::Pure, &[], Span::at(source_id, 0)).unwrap().unwrap() else { panic!("the ordinary call returns its checked record") };
        assert_eq!(fields.get("value"), Some(&Value::Int(7)), "frames={force_frames}");
        assert_eq!(fields.get("count"), Some(&Value::Int(7)), "frames={force_frames}");
    }
}

#[test]
fn generic_arguments_defaults_run_after_supplied_entries_once() {
    let source = "var trace = 0\nproc tick(digit: Int) [] -> Int { trace = trace * 10 + digit; digit }\nproc select(value, left: Int = tick(3), right: Int = tick(4)) [error] { let _ = left; let _ = right; value }\nproc call(value, supplied: Bool) [error] { let result = if supplied { select(right: tick(2), value: {let _ = tick(1); value}) } else { select({let _ = tick(1); value}) }; result? }\nproc number(supplied: Bool) [error] { let value = call(7, supplied)?; {value, trace} }\nproc text(supplied: Bool) [error] { let value = call(\"value\", supplied)?; {value, trace} }\n";
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        for supplied in [false, true] {
            for (name, expected) in [("number", Value::Int(7)), ("text", Value::Str(Arc::from("value")))] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
                evaluator.force_frames = force_frames;
                evaluator.indexed_program = Some(Arc::clone(&program));
                for step in 0..program.driver_step_count().unwrap() { evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap(); }
                let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
                let Value::Result(ResultValue::Ok(fields)) = evaluator.call_indexed_direct(key, CallableKind::Proc, &[Value::Bool(supplied)], Span::at(source_id, 0)).unwrap().unwrap() else { panic!("the checked default call succeeds") };
                let Value::Record(fields) = *fields else { panic!("the call returns its observations") };
                assert_eq!(fields.get("value"), Some(&expected), "{name} supplied={supplied} frames={force_frames}");
                assert_eq!(fields.get("trace"), Some(&Value::Int(if supplied { 213 } else { 134 })), "{name} supplied={supplied} frames={force_frames}");
            }
        }
    }
}

#[test]
fn generic_arguments_defaults_preserve_failure_and_cleanup() {
    let source = "error ArgumentError = invalid(message: Str)\nvar closed = 0\nvar later = 0\nproc close() [] { closed = closed + 1 }\nproc fail() [error] -> Result[Int, ArgumentError] { defer close(); Err(ArgumentError.invalid(message: \"default failed\")) }\nproc next() [] -> Int { later = later + 1; 4 }\nproc select(value, left: Int = fail()?, right: Int = next()) [error] { let _ = left; let _ = right; value }\nproc call(value, omitted: Bool) [error] { let result = if omitted { select(value) } else { select(value, 1, 2) }; result? }\nproc number(omitted: Bool) [error] { let result = try { call(7, omitted)? }; {result, closed, later} }\n";
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        for omitted in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            for step in 0..program.driver_step_count().unwrap() { evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap(); }
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("number")));
            let Value::Result(ResultValue::Ok(fields)) = evaluator.call_indexed_direct(key, CallableKind::Proc, &[Value::Bool(omitted)], Span::at(source_id, 0)).unwrap().unwrap() else { panic!("the outer caller catches the actual default failure") };
            let Value::Record(fields) = *fields else { panic!("the caller returns failure observations") };
            assert_eq!(fields.get("closed"), Some(&Value::Int(i64::from(omitted))), "omitted={omitted} frames={force_frames}");
            assert_eq!(fields.get("later"), Some(&Value::Int(0)), "omitted={omitted} frames={force_frames}");
            if omitted {
                let Some(Value::Result(ResultValue::Err(error))) = fields.get("result") else { panic!("the default preserves its error result") };
                let Value::Error(error) = error.as_ref() else { panic!("the actual nominal error is preserved") };
                assert_eq!((&*error.family, &*error.variant, &*error.message), ("ArgumentError", "invalid", "default failed"));
            } else { assert_eq!(fields.get("result"), Some(&Value::Result(ResultValue::Ok(Box::new(Value::Int(7)))))); }
        }
    }
}

#[test]
fn callable_kind_stream_aliases_keep_independent_invocation_bindings() {
    for through_callback in [false, true] {
        let creations = if through_callback {
            "let integers = relay(create, 7)?; let words = relay(create, \"row\")?"
        } else {
            "let integers = create(7); let words = create(\"row\")"
        };
        let effects = if through_callback { "[error]" } else { "[]" };
        let source = format!("stream values(value) [] {{ yield value }}\nproc relay(action, value) [] {{ action(value) }}\nproc exercise() {effects} {{ let create = values; {creations}; var integer = 0; for item in integers {{ integer = item; break }}; var text = \"\"; for item in words {{ text = item; break }}; {{integer, text}} }}\n");
        let (program, sources, source_id) = record_form_program(&source);
        for force_frames in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("exercise")));
            let Value::Result(ResultValue::Ok(fields)) = evaluator.call_indexed_direct(key, CallableKind::Proc, &[], Span::at(source_id, 0)).unwrap().unwrap() else { panic!("stream invocation returns its independently specialized values") };
            let Value::Record(fields) = *fields else { panic!("the stream consumers return their actual items") };
            assert_eq!(fields.get("integer"), Some(&Value::Int(7)), "callback={through_callback} frames={force_frames}");
            assert_eq!(fields.get("text"), Some(&Value::Str(Arc::from("row"))), "callback={through_callback} frames={force_frames}");
        }
    }
}

#[test]
fn callable_kind_stream_aliases_preserve_lazy_defaults_and_cleanup() {
    let source = "var defaults = 0\nvar pulls = 0\nvar closed = 0\nproc count_default() [] -> Int { defaults = defaults + 1; 1 }\nproc close() [] { closed = closed + 1 }\nstream rows(value, count: Int = count_default()) [error] { defer close(); pulls = pulls + 1; repeat count times { yield value }; pulls = pulls + 100 }\nproc exercise() [error] { let create = rows; let integers = create(7); let words = create(\"row\", count: 1); let before = {defaults, pulls, closed}; var integer = 0; for item in integers { integer = item; break }; let after_first = {defaults, pulls, closed}; var text = \"\"; for item in words { text = item; break }; {before, after_first, defaults, pulls, closed, integer, text} }\n";
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        for step in 0..program.driver_step_count().unwrap() { evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap(); }
        let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("exercise")));
        let Value::Result(ResultValue::Ok(fields)) = evaluator.call_indexed_direct(key, CallableKind::Proc, &[], Span::at(source_id, 0)).unwrap().unwrap() else { panic!("stream aliases preserve the lazy producer lifecycle") };
        let Value::Record(fields) = *fields else { panic!("the caller returns lifecycle observations") };
        for (stage, expected) in [("before", 0), ("after_first", 1)] {
            let Some(Value::Record(counters)) = fields.get(stage) else { panic!("each stage retains its own counter snapshot") };
            for name in ["defaults", "pulls", "closed"] { assert_eq!(counters.get(name), Some(&Value::Int(expected)), "{stage}.{name} frames={force_frames}"); }
        }
        for (name, expected) in [("defaults", 1), ("pulls", 2), ("closed", 2), ("integer", 7)] { assert_eq!(fields.get(name), Some(&Value::Int(expected)), "{name} frames={force_frames}"); }
        assert_eq!(fields.get("text"), Some(&Value::Str(Arc::from("row"))));
    }
}

#[test]
fn callable_kind_stream_captures_follow_interleaved_writes_and_cancel() {
    let source = "var counter = 0\nvar defaults = 0\nvar closed = -1\nproc initial() [] -> Int { defaults = defaults + 1; 1 }\nproc close() [] -> Unit { closed = counter; counter = counter + 100 }\nstream rows(value, count: Int = initial()) [error] { defer close(); counter = counter + 1; yield value; counter = counter + 1; yield value }\nproc make() [error] -> Stream[Int] { rows(7) }\nproc write(value: Int) [] -> Unit { counter = value }\nproc observe() [] { {counter, defaults, closed} }\n";
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        let span = Span::at(source_id, 0);
        for step in 0..program.driver_step_count().unwrap() { evaluator.eval_indexed_driver_step(step, span).unwrap().unwrap(); }
        let (make, write, observe) = program.symbol_owner().with_current(|| (LoweredFunctionKey::Name(Name::intern("make")), LoweredFunctionKey::Name(Name::intern("write")), LoweredFunctionKey::Name(Name::intern("observe"))));
        let Value::Stream(mut stream) = evaluator.call_indexed_direct(make, CallableKind::Proc, &[], span).unwrap().unwrap() else { panic!("creation returns an unstarted generic producer") };
        let observe_state = |evaluator: &mut Evaluator, expected: [i64; 3]| {
            let Value::Result(ResultValue::Ok(value)) = evaluator.call_indexed_direct(observe, CallableKind::Proc, &[], span).unwrap().unwrap() else { panic!("observations preserve the Proc envelope") };
            let Value::Record(fields) = *value else { panic!("observations return the live storage state") };
            for (name, expected) in ["counter", "defaults", "closed"].into_iter().zip(expected) { assert_eq!(fields.get(name), Some(&Value::Int(expected)), "{name} frames={force_frames}"); }
        };
        evaluator.call_indexed_direct(write, CallableKind::Proc, &[Value::Int(10)], span).unwrap().unwrap();
        observe_state(&mut evaluator, [10, 0, -1]);
        assert_eq!(evaluator.stream_next(&mut stream, span).unwrap(), Some(Value::Int(7)));
        observe_state(&mut evaluator, [11, 1, -1]);
        evaluator.call_indexed_direct(write, CallableKind::Proc, &[Value::Int(20)], span).unwrap().unwrap();
        assert_eq!(evaluator.stream_next(&mut stream, span).unwrap(), Some(Value::Int(7)));
        observe_state(&mut evaluator, [21, 1, -1]);
        evaluator.call_indexed_direct(write, CallableKind::Proc, &[Value::Int(30)], span).unwrap().unwrap();
        evaluator.stream_cancel(&mut stream, span).unwrap();
        observe_state(&mut evaluator, [130, 1, 30]);
        evaluator.stream_cancel(&mut stream, span).unwrap();
        observe_state(&mut evaluator, [130, 1, 30]);
        let Value::Stream(mut unstarted) = evaluator.call_indexed_direct(make, CallableKind::Proc, &[], span).unwrap().unwrap() else { panic!("creation remains lazy after cancellation") };
        evaluator.call_indexed_direct(write, CallableKind::Proc, &[Value::Int(40)], span).unwrap().unwrap();
        evaluator.stream_cancel(&mut unstarted, span).unwrap();
        observe_state(&mut evaluator, [40, 1, 30]);
    }
}

#[test]
fn callable_kind_stream_aliases_instrumented_defaults_wait_for_pull() {
    let source = "var defaults = 0\nvar pulls = 0\nvar closed = 0\nproc count_default() [] -> Int { defaults = defaults + 1; 1 }\nproc close() [] { closed = closed + 1 }\nproc observe_pull() [] { pulls = pulls + 1 }\nproc observe_late_pull() [] { pulls = pulls + 100 }\nstream rows(value, count: Int = count_default()) [error] { defer close(); let _ = observe_pull(); if count > 0 { yield value }; let _ = observe_late_pull() }\nproc exercise() [error] { let create = rows; let integers = create(7); let words = create(\"row\", count: 1); let before = {defaults, pulls, closed}; var integer = 0; for item in integers { integer = item; break }; let after_first = {defaults, pulls, closed}; var text = \"\"; for item in words { text = item; break }; {before, after_first, defaults, pulls, closed, integer, text} }\n";
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.trace_enabled = true;
        evaluator.indexed_program = Some(Arc::clone(&program));
        for step in 0..program.driver_step_count().unwrap() { evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap(); }
        let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("exercise")));
        let Value::Result(ResultValue::Ok(fields)) = evaluator.call_indexed_direct(key, CallableKind::Proc, &[], Span::at(source_id, 0)).unwrap().unwrap() else { panic!("stream aliases preserve the lazy producer lifecycle") };
        let Value::Record(fields) = *fields else { panic!("the caller returns lifecycle observations") };
        for (stage, expected) in [("before", 0), ("after_first", 1)] {
            let Some(Value::Record(counters)) = fields.get(stage) else { panic!("each stage retains its own counter snapshot") };
            for name in ["defaults", "pulls", "closed"] { assert_eq!(counters.get(name), Some(&Value::Int(expected)), "{stage}.{name} frames={force_frames}"); }
        }
        for (name, expected) in [("defaults", 1), ("pulls", 2), ("closed", 2), ("integer", 7)] { assert_eq!(fields.get(name), Some(&Value::Int(expected)), "{name} frames={force_frames}"); }
        assert_eq!(fields.get("text"), Some(&Value::Str(Arc::from("row"))));
        let entries = evaluator.trace_events.iter().filter(|event| event.kind == crate::trace::TraceKind::StreamEnter && event.name.as_deref() == Some("rows")).collect::<Vec<_>>();
        assert_eq!(entries.len(), 2, "each actual stream body enters once at its first pull");
        let default = evaluator.trace_events.iter().position(|event| event.kind == crate::trace::TraceKind::ProcEnter && event.name.as_deref() == Some("count_default")).expect("the omitted default runs on first pull");
        let first_body = evaluator.trace_events.iter().position(|event| event.kind == crate::trace::TraceKind::StreamEnter && event.name.as_deref() == Some("rows")).unwrap();
        let first_close = evaluator.trace_events.iter().position(|event| event.kind == crate::trace::TraceKind::ProcEnter && event.name.as_deref() == Some("close")).expect("breaking closes the first producer");
        assert!(first_body < default && default < first_close);
        assert_eq!(evaluator.trace_events.iter().filter(|event| event.kind == crate::trace::TraceKind::ProcEnter && event.name.as_deref() == Some("close")).count(), 2);
    }
}

#[test]
fn callable_kind_stream_delegation_preserves_bindings_and_close_order() {
    for nested in [false, true] {
        let factory = if nested { "nested" } else { "listed" };
        let source = format!("var closed = \"\"\nproc close(label: Str) [] -> Unit {{ closed = closed + label }}\nstream listed(value) [] {{ defer close(\"c\"); yield @[value, value] }}\nstream nested(value) [] {{ defer close(\"p\"); yield @listed(value) }}\nproc exercise() [error] {{ let create = {factory}; var integer = 0; for item in create(7) {{ integer = item; break }}; var text = \"\"; for item in create(\"row\") {{ text = item; break }}; {{integer, text, closed}} }}\n");
        let (program, sources, source_id) = record_form_program(&source);
        for force_frames in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let span = Span::at(source_id, 0);
            for step in 0..program.driver_step_count().unwrap() { evaluator.eval_indexed_driver_step(step, span).unwrap().unwrap(); }
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("exercise")));
            let Value::Result(ResultValue::Ok(fields)) = evaluator.call_indexed_direct(key, CallableKind::Proc, &[], span).unwrap().unwrap() else { panic!("the caller returns delegated items and cleanup observations") };
            let Value::Record(fields) = *fields else { panic!("the caller retains its independent item types") };
            assert_eq!(fields.get("integer"), Some(&Value::Int(7)));
            assert_eq!(fields.get("text"), Some(&Value::Str(Arc::from("row"))));
            assert_eq!(fields.get("closed"), Some(&Value::Str(Arc::from(if nested { "cpcp" } else { "cc" }))), "nested={nested} frames={force_frames}");
        }
    }
}

#[test]
fn callable_kind_stream_delegation_preserves_late_nominal_errors() {
    let source = "error RowsError = Late(row: Int)\nvar closed = \"\"\nproc close(label: Str) [] -> Unit { closed = closed + label }\nproc fail() [] -> Result[Unit, RowsError] { Err(RowsError.Late(row: 4)) }\nstream child(value) [error] { defer close(\"c\"); yield value; fail()? }\nstream parent(value) [error] { defer close(\"p\"); yield @child(value) }\nproc make() [error] -> Stream[Int] { parent(7) }\nproc observations() [] -> Str { closed }\n";
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        let span = Span::at(source_id, 0);
        for step in 0..program.driver_step_count().unwrap() { evaluator.eval_indexed_driver_step(step, span).unwrap().unwrap(); }
        let (make, observations) = program.symbol_owner().with_current(|| (LoweredFunctionKey::Name(Name::intern("make")), LoweredFunctionKey::Name(Name::intern("observations"))));
        let Value::Stream(mut stream) = evaluator.call_indexed_direct(make, CallableKind::Proc, &[], span).unwrap().unwrap() else { panic!("creation retains an unpulled generic delegator") };
        assert_eq!(evaluator.call_indexed_direct(observations, CallableKind::Proc, &[], span).unwrap().unwrap(), Value::Str(Arc::from("")));
        assert_eq!(evaluator.stream_next(&mut stream, span).unwrap(), Some(Value::Int(7)));
        let error = evaluator.stream_next(&mut stream, span).unwrap_err();
        assert_eq!((&*error.family, &*error.variant), ("RowsError", "Late"));
        assert_eq!(error.payload.get("row"), Some(&Value::Int(4)));
        assert_eq!(&source[error.span.unwrap().range()], "fail()?");
        assert_eq!(evaluator.call_indexed_direct(observations, CallableKind::Proc, &[], span).unwrap().unwrap(), Value::Str(Arc::from("cp")));
        assert_eq!(evaluator.stream_next(&mut stream, span).unwrap(), None);
        assert_eq!(evaluator.call_indexed_direct(observations, CallableKind::Proc, &[], span).unwrap().unwrap(), Value::Str(Arc::from("cp")));
    }
}

#[test]
fn callable_kind_stream_delegation_normalizes_late_string_errors() {
    let source = "var closed = \"\"\nproc close(label: Str) [] -> Unit { closed = closed + label }\nproc fail() [] -> Result[Unit, Str] { Err(\"late failure\") }\nstream child(value) [error] { defer close(\"c\"); yield value; fail()? }\nstream parent(value) [error] { defer close(\"p\"); yield @child(value) }\nproc make() [error] -> Stream[Int] { parent(7) }\nproc observations() [] -> Str { closed }\n";
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        let span = Span::at(source_id, 0);
        for step in 0..program.driver_step_count().unwrap() { evaluator.eval_indexed_driver_step(step, span).unwrap().unwrap(); }
        let (make, observations) = program.symbol_owner().with_current(|| (LoweredFunctionKey::Name(Name::intern("make")), LoweredFunctionKey::Name(Name::intern("observations"))));
        let Value::Stream(mut stream) = evaluator.call_indexed_direct(make, CallableKind::Proc, &[], span).unwrap().unwrap() else { panic!("creation retains an unpulled generic delegator") };
        assert_eq!(evaluator.call_indexed_direct(observations, CallableKind::Proc, &[], span).unwrap().unwrap(), Value::Str(Arc::from("")));
        assert_eq!(evaluator.stream_next(&mut stream, span).unwrap(), Some(Value::Int(7)));
        let error = evaluator.stream_next(&mut stream, span).unwrap_err();
        assert_eq!((&*error.family, &*error.variant), ("Error", "stream-stage-error"));
        assert_eq!(&source[error.span.unwrap().range()], "fail()?");
        assert_eq!(evaluator.call_indexed_direct(observations, CallableKind::Proc, &[], span).unwrap().unwrap(), Value::Str(Arc::from("cp")));
        assert_eq!(evaluator.stream_next(&mut stream, span).unwrap(), None);
        assert_eq!(evaluator.call_indexed_direct(observations, CallableKind::Proc, &[], span).unwrap().unwrap(), Value::Str(Arc::from("cp")));
    }
}

#[test]
fn generic_arguments_stream_defaults_keep_pull_and_cleanup_ownership() {
    let source = "var created = 0\nvar pulls = 0\nvar closed = 0\nproc close() [] { closed = closed + 1 }\nstream rows() [error] -> Stream[Int] { defer close(); pulls = pulls + 1; yield 1; pulls = pulls + 100; yield 2 }\nproc make() [error] -> Stream[Int] { created = created + 1; rows() }\nproc supplied() [error] -> Stream[Int] { created = created + 10; rows() }\nproc select(value, items: Stream[Int] = make()) [error] { for item in items { let _ = item; break }; value }\nproc call(value, omitted: Bool) [error] { let result = if omitted { select(value) } else { select(value, supplied()) }; result? }\nproc number(omitted: Bool) [error] { let value = call(7, omitted)?; {value, created, pulls, closed} }\n";
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        for omitted in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            for step in 0..program.driver_step_count().unwrap() { evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap(); }
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("number")));
            let Value::Result(ResultValue::Ok(fields)) = evaluator.call_indexed_direct(key, CallableKind::Proc, &[Value::Bool(omitted)], Span::at(source_id, 0)).unwrap().unwrap() else { panic!("the lazy default stream remains owned by the callee") };
            let Value::Record(fields) = *fields else { panic!("the caller returns lifecycle observations") };
            for (name, expected) in [("value", 7), ("created", if omitted { 1 } else { 10 }), ("pulls", 1), ("closed", 1)] {
                assert_eq!(fields.get(name), Some(&Value::Int(expected)), "{name} omitted={omitted} frames={force_frames}");
            }
        }
    }
}

#[test]
fn generic_arguments_ground_layouts_keep_default_and_rest_meanings() {
    let source = "pure increment(number: Int = 1) -> Int { number + 1 }\npure total(...items: List[Int]) -> Int { var result = 0; for item in items { result += item }; result }\npure call(value) { let tail = [3, 4]; {value, omitted: increment(), explicit: increment(2), empty: total(), rest: total(1, 2), spliced: total(@tail)} }\npure number() { call(7) }\npure text() { call(\"value\") }\n";
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        for (name, expected) in [("number", Value::Int(7)), ("text", Value::Str(Arc::from("value")))] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
            let Value::Record(fields) = evaluator.call_indexed_direct(key, CallableKind::Pure, &[], Span::at(source_id, 0)).unwrap().unwrap() else { panic!("the ordinary helpers retain their checked parameter layouts") };
            assert_eq!(fields.get("value"), Some(&expected), "{name} frames={force_frames}");
            for (field, value) in [("omitted", 2), ("explicit", 3), ("empty", 0), ("rest", 3), ("spliced", 7)] {
                assert_eq!(fields.get(field), Some(&Value::Int(value)), "{name}.{field} frames={force_frames}");
            }
        }
    }
}

#[test]
fn generic_arguments_registered_spreads_use_the_selected_layout() {
    let source = "pure call(value) { let args = {prefix: \"a\"}; {value, matched: \"abc\".starts_with(...args), data: bytes.from_text(...{text: \"data\"})} }\npure number() { call(7) }\npure text() { call(\"value\") }\n";
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        for (name, expected) in [("number", Value::Int(7)), ("text", Value::Str(Arc::from("value")))] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
            let Value::Record(fields) = evaluator.call_indexed_direct(key, CallableKind::Pure, &[], Span::at(source_id, 0)).unwrap().unwrap() else { panic!("the registered calls retain their selected outputs") };
            assert_eq!(fields.get("value"), Some(&expected), "{name} frames={force_frames}");
            assert_eq!(fields.get("matched"), Some(&Value::Bool(true)), "{name} frames={force_frames}");
            assert_eq!(fields.get("data"), Some(&Value::Bytes(b"data".to_vec())), "{name} frames={force_frames}");
        }
    }
}

const CAPTURED_CALLABLE_SOURCE: &str = "error CallbackError = invalid(message: Str)\nvar calls = 0\nvar closed = 0\nproc close() [] { closed = closed + 1 }\nproc side(payload: Int) [error] -> Int { defer close(); calls = calls + 1; assert payload > 0; payload }\npure data(payload: Int) -> Result[Int, CallbackError] { Err(CallbackError.invalid(message: \"stored\")) }\nproc invoke(action, value) { let result = try { action(value) }; {result} }\nproc observe(payload: Int) [error] { let returned = invoke(side, payload)?; {result: returned.result, calls, closed} }\nproc data_run() [error] { let returned = invoke(data, 7)?; returned.result }\n";

#[test]
fn generic_captured_callable_invocations_preserve_error_and_cleanup() {
    let (program, sources, source_id) = record_form_program(CAPTURED_CALLABLE_SOURCE);
    for force_frames in [false, true] {
        for payload in [7, 0] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            for step in 0..program.driver_step_count().unwrap() { evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap(); }
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("observe")));
            let returned = evaluator.call_indexed_direct(key, CallableKind::Proc, &[Value::Int(payload)], Span::at(source_id, 0)).unwrap().unwrap();
            let Value::Result(ResultValue::Ok(record)) = returned else { panic!("the outer result carries the captured invocation") };
            let Value::Record(record) = *record else { panic!("the invocation returns its lifecycle observations") };
            assert_eq!(record.get("calls"), Some(&Value::Int(1)), "frames={force_frames} payload={payload}");
            assert_eq!(record.get("closed"), Some(&Value::Int(1)), "frames={force_frames} payload={payload}");
            if payload == 7 { assert_eq!(record.get("result"), Some(&Value::ok(Value::Int(7)))); }
            else {
                let Some(Value::Result(ResultValue::Err(error))) = record.get("result") else { panic!("the actual invocation error is captured") };
                let Value::Error(error) = error.as_ref() else { panic!("capture retains the actual error value") };
                assert_eq!((&*error.family, &*error.variant, &*error.kind), ("AssertionError", "Failed", "AssertionError.Failed"));
                assert!(error.message.contains("payload > 0"), "{}", error.message);
            }
        }
    }
}

#[test]
fn generic_captured_callable_results_remain_data() {
    let (program, sources, source_id) = record_form_program(CAPTURED_CALLABLE_SOURCE);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        for step in 0..program.driver_step_count().unwrap() { evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap(); }
        let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("data_run")));
        let returned = evaluator.call_indexed_direct(key, CallableKind::Proc, &[], Span::at(source_id, 0)).unwrap().unwrap();
        let Value::Result(ResultValue::Ok(data)) = returned else { panic!("capture succeeds when the callback returns an Err as data") };
        let Value::Result(ResultValue::Err(error)) = *data else { panic!("the callback result is retained inside capture success") };
        let Value::Error(error) = *error else { panic!("the original nominal error is retained as data") };
        assert_eq!((&*error.family, &*error.variant, &*error.message), ("CallbackError", "invalid", "stored"));
    }
}

#[test]
fn prepared_applications_results_anchor_empty_collection_arguments() {
    let source = "pure item() -> Int { 7 }\npure text_item() -> Str { \"value\" }\npure collect(action, rest, flag: Bool) { if flag { [action()] } else { rest } }\npure number(flag: Bool) -> List[Int] { collect(item, [], flag) }\npure text(flag: Bool) -> List[Str] { collect(text_item, [], flag) }\n";
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        for (name, expected) in [("number", Value::Int(7)), ("text", Value::Str(Arc::from("value")))] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
            for present in [false, true] {
                let actual = evaluator.call_indexed_direct(key, CallableKind::Pure, &[Value::Bool(present)], Span::at(source_id, 0)).unwrap().unwrap();
                assert_eq!(actual, Value::List(if present { vec![expected.clone()] } else { Vec::new() }));
            }
        }
    }
}

fn assert_prepared_application_result(source: &str, kind: CallableKind, expected: Value) {
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        for step in 0..program.driver_step_count().unwrap() {
            evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap();
        }
        let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("number")));
        let actual = evaluator.call_indexed_direct(key, kind, &[], Span::at(source_id, 0)).unwrap().unwrap();
        assert_eq!(record_form_observable(actual), expected, "frames={force_frames}\n{source}");
    }
}

#[test]
fn inferred_application_runtime_named_arguments_keep_source_order() {
    let source = "pure target(first: Int, last: Int) -> Int { first * 100 + last }\npure apply(action, value) { var trace = 0; let result = action(last: { trace = trace * 10 + 1; value }, first: { trace = trace * 10 + 2; 1 }); {result, trace} }\npure number() { apply(target, 7) }\n";
    assert_prepared_application_result(source, CallableKind::Pure, Value::Map([
        (crate::runtime::value::MapKey::Str(Arc::from("result")), Value::Int(107)),
        (crate::runtime::value::MapKey::Str(Arc::from("trace")), Value::Int(12)),
    ].into_iter().collect()));
}

#[test]
fn inferred_application_runtime_interior_defaults_use_actual_layout() {
    let source = "pure target(first: Int = 11, payload: Int = 7) -> Int { first * 100 + payload }\npure apply(action, value) { action(payload: value) }\npure number() -> Int { apply(target, 7) }\n";
    assert_prepared_application_result(source, CallableKind::Pure, Value::Int(1107));
}

#[test]
fn inferred_application_runtime_named_rest_preserves_splice_groups() {
    let source = "pure target(first: Int, payload: Int, ...items: List[Int]) -> Int { first * 100 + payload * 10 + items.len() }\npure apply(action, value) { action(payload: value, first: 1, @[2, 3], @[4]) }\npure number() -> Int { apply(target, 7) }\n";
    assert_prepared_application_result(source, CallableKind::Pure, Value::Int(173));
}

#[test]
fn inferred_application_runtime_returned_callable_keeps_authored_labels() {
    let source = "pure target(payload: Int, scale: Int = 2) -> Int { payload * scale }\npure keep(action, value) { let _ = action(value); action }\npure number() -> Int { let returned = keep(target, 7); returned(scale: 4, payload: 3) }\n";
    assert_prepared_application_result(source, CallableKind::Pure, Value::Int(12));
}

#[test]
fn inferred_application_runtime_proc_returns_follow_actual_application_origin() {
    for (definition, callback, value, expected) in [
        ("pure identity(payload: Int) -> Int { payload }", "identity", "7", Value::Result(ResultValue::Ok(Box::new(Value::Int(7))))),
        ("pure data(payload: Int) -> Result[Int, Str] { Err(\"stored\") }", "data", "7", Value::Result(ResultValue::Err(Box::new(Value::Str(Arc::from("stored")))))),
        ("proc identity(payload: Int) [] -> Int { payload }", "identity", "7", Value::Result(ResultValue::Ok(Box::new(Value::Int(7))))),
        ("pure data(payload: Int) -> Result[Int, Str] { Ok(payload) }", "data", "7", Value::ok(Value::Int(7))),
        ("proc data(payload: Int) [] -> Result[Int, Str] { Ok(payload) }", "data", "7", Value::ok(Value::Int(7))),
        ("proc data(payload: Int) [] -> Result[Int, Str] { Err(\"stored\") }", "data", "7", Value::err(Value::Str(Arc::from("stored")))),
    ] {
        for returned in ["action(value)", "let result = action(value); result", "return action(value)", "let result = action(value); return result"] {
            let source = format!("{definition}\nproc relay(action, value) {{ {returned} }}\nproc number() [] {{ relay({callback}, {value}) }}\n");
            assert_prepared_application_result(&source, CallableKind::Proc, expected.clone());
        }
    }
}

#[test]
fn inferred_application_runtime_formal_result_keeps_its_data_envelope() {
    let source = "pure identity(payload: Result[Int, Str]) -> Result[Int, Str] { payload }\npure data() -> Result[Int, Str] { Err(\"stored\") }\nproc keep(action, value) { let _ = action(value); value }\nproc number() [] -> Result[Result[Int, Str]] { keep(identity, data()) }\n";
    let data = Value::Result(ResultValue::Err(Box::new(Value::Str(Arc::from("stored")))));
    assert_prepared_application_result(source, CallableKind::Proc, Value::Result(ResultValue::Ok(Box::new(data))));
}

#[test]
fn inferred_application_runtime_forwarded_layout_retains_caller_binding() {
    let source = "pure target(first: Int, last: Int) -> Int { first * 100 + last }\npure apply(action, value) { action(last: value, first: 1) }\npure forward(action, value) { apply(action, value) }\npure number() -> Int { forward(target, 7) }\n";
    assert_prepared_application_result(source, CallableKind::Pure, Value::Int(107));
}

#[test]
fn inferred_application_runtime_defaults_keep_order_failure_and_cleanup() {
    for fail in [false, true] {
        let valid = if fail { "false" } else { "true" };
        let source = format!("var trace = 0\nvar closed = 0\nproc close() [] {{ closed = closed + 1 }}\nproc tick(digit: Int) [] -> Int {{ trace = trace * 10 + digit; digit }}\nproc first(valid: Bool) [error] -> Int {{ defer close(); let _ = tick(3); assert valid; 3 }}\nproc target(payload: Int = first({valid}), scale: Int = tick(4)) [error] -> Int {{ payload * 100 + scale }}\nproc apply(action, value) {{ let captured = try {{ action(scale: {{ let _ = tick(2); value }}) }}; {{captured}} }}\nproc omitted(action) {{ let captured = try {{ action() }}; {{captured}} }}\nproc number(omit: Bool) [error] {{ let _ = tick(1); let returned = if omit {{ omitted(target) }} else {{ apply(target, 7) }}; let observed = returned?; {{result: observed.captured, trace, closed}} }}\n");
        let (program, sources, source_id) = record_form_program(&source);
        for force_frames in [false, true] {
            for omit in [false, true] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
                evaluator.force_frames = force_frames;
                evaluator.indexed_program = Some(Arc::clone(&program));
                for step in 0..program.driver_step_count().unwrap() {
                    evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap();
                }
                let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("number")));
                let actual = evaluator.call_indexed_direct(key, CallableKind::Proc, &[Value::Bool(omit)], Span::at(source_id, 0)).unwrap().unwrap();
                let Value::Result(ResultValue::Ok(record)) = actual else { panic!("the enclosing proc preserves its success boundary") };
                let Value::Record(fields) = *record else { panic!("the observations retain their checked record") };
                assert_eq!(fields.get("closed"), Some(&Value::Int(1)));
                assert_eq!(fields.get("trace"), Some(&Value::Int(if omit { if fail { 13 } else { 134 } } else { 123 })));
                match fields.get("result").unwrap() {
                    Value::Result(ResultValue::Ok(value)) if !fail => assert_eq!(**value, Value::Int(if omit { 304 } else { 307 })),
                    Value::Result(ResultValue::Err(error)) if fail => {
                        let Value::Error(error) = error.as_ref() else { panic!("capture retains the actual assertion error") };
                        assert_eq!((&*error.family, &*error.variant), ("AssertionError", "Failed"));
                    }
                    other => panic!("default result transport differs: {other:?}"),
                }
            }
        }
    }
}

#[test]
fn inferred_application_runtime_proc_branches_keep_actual_application_origins() {
    for body in [
        "let result = action(value); if flag { let selected = result; selected } else { result }",
        "if flag { action(value) } else { action(value) }",
        "match flag { true => action(value); false => action(value) }",
    ] {
        for flag in [false, true] {
            for (definition, callback, expected) in [
                ("pure identity(payload: Int) -> Int { payload }", "identity", Value::ok(Value::Int(7))),
                ("pure data(payload: Int) -> Result[Int, Str] { Err(\"stored\") }", "data", Value::err(Value::Str("stored".into()))),
            ] {
                let source = format!("{definition}\nproc relay(action, value, flag: Bool) {{ {body} }}\nproc number() [] {{ relay({callback}, 7, {flag}) }}\n");
                assert_prepared_application_result(&source, CallableKind::Proc, expected);
            }
        }
    }
}

#[test]
fn inferred_application_runtime_proc_mixed_data_branches_keep_ordinary_envelopes() {
    for flag in [false, true] {
        let source = format!("pure identity(payload: Int) -> Int {{ payload }}\nproc relay(action, value, flag: Bool) {{ if flag {{ action(value) }} else {{ value }} }}\nproc number() [] {{ relay(identity, 7, {flag}) }}\n");
        assert_prepared_application_result(&source, CallableKind::Proc, Value::ok(Value::Int(7)));
    }
}

#[test]
fn inferred_application_runtime_generic_callback_retains_actual_substitution() {
    let source = "pure identity(payload) { payload }\npure apply(action, value) { action(value) }\npure number() -> Int { apply(identity, 7) }\n";
    assert_prepared_application_result(source, CallableKind::Pure, Value::Int(7));
}

#[test]
fn inferred_application_runtime_polymorphic_aliases_keep_each_call_substitution() {
    for through_apply in [false, true] {
        for text_first in [false, true] {
            let integer = if through_apply { "apply(f, 7)" } else { "f(7)" };
            let text = if through_apply { "apply(f, \"word\")" } else { "f(\"word\")" };
            let bindings = if text_first {
                format!("let text = {text}; let integer = {integer};")
            } else {
                format!("let integer = {integer}; let text = {text};")
            };
            let source = format!("pure identity(payload) {{ payload }}\npure apply(action, value) {{ action(value) }}\npure number() {{ let f = identity; {bindings} {{integer, text}} }}\n");
            assert_prepared_application_result(&source, CallableKind::Pure, Value::Map([
                (crate::runtime::value::MapKey::Str(Arc::from("integer")), Value::Int(7)),
                (crate::runtime::value::MapKey::Str(Arc::from("text")), Value::Str(Arc::from("word"))),
            ].into_iter().collect()));
        }
    }
}

fn prepared_host_callable_program(answer: i64) -> (Arc<crate::runtime::eval::FullProgram>, SourceMap, SourceId) {
    record_form_program(&format!(
        "pure identity(payload) {{ let _ = payload; {answer} }}\npure keep(action, value) {{ let _ = action(value); action }}\npure make() {{ keep(identity, 7) }}\npure direct(handle: Pure) -> Any {{ handle.call(7) }}\npure direct_value(handle: Pure, value: Any) -> Any {{ handle.call(value) }}\nproc cached(handle: Pure) -> Any {{ utils.cache(handle, [7]) }}\nproc cached_value(handle: Pure, value: Any) -> Any {{ utils.cache(handle, [value]) }}\n"
    ))
}

#[test]
fn prepared_callable_host_call_retains_its_program() {
    let (first, sources, source_id) = prepared_host_callable_program(8);
    let (second, _, _) = prepared_host_callable_program(9);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&first));
        let key = first.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("make")));
        let prepared = evaluator.call_indexed_direct(key, CallableKind::Pure, &[], Span::at(source_id, 0)).unwrap().unwrap();
        assert!(matches!(&prepared, Value::Callable(callable) if callable.kind() == crate::sema::types::CallableKind::Pure && callable.instance().is_some()));
        for program in [&first, &second] {
            evaluator.indexed_program = Some(Arc::clone(program));
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("direct")));
            let actual = evaluator.call_indexed_direct(key, CallableKind::Pure, std::slice::from_ref(&prepared), Span::at(source_id, 0)).unwrap().unwrap();
            assert_eq!(actual, Value::Int(8), "frames={force_frames}");
        }
    }
}

#[test]
fn prepared_callable_cache_separates_programs_and_preserves_clones() {
    let (first, sources, source_id) = prepared_host_callable_program(8);
    let (second, _, _) = prepared_host_callable_program(9);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        let mut handles = Vec::new();
        for program in [&first, &second] {
            evaluator.indexed_program = Some(Arc::clone(program));
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("make")));
            let value = evaluator.call_indexed_direct(key, CallableKind::Pure, &[], Span::at(source_id, 0)).unwrap().unwrap();
            assert!(matches!(&value, Value::Callable(callable) if callable.kind() == crate::sema::types::CallableKind::Pure && callable.instance().is_some()));
            handles.push(value);
        }
        evaluator.trace_enabled = true;
        evaluator.trace_events.clear();
        for (program, handle, answer) in [(&first, &handles[0], 8), (&first, &handles[0], 8), (&second, &handles[1], 9), (&second, &handles[1], 9)] {
            evaluator.indexed_program = Some(Arc::clone(program));
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("cached")));
            let actual = evaluator.call_indexed_direct(key, CallableKind::Proc, std::slice::from_ref(handle), Span::at(source_id, 0)).unwrap().unwrap();
            assert_eq!(actual, Value::Int(answer), "frames={force_frames}");
        }
        assert_eq!(evaluator.trace_events.iter().filter(|event| event.kind == crate::trace::TraceKind::PureEnter && event.name.as_deref() == Some("identity")).count(), 2);
    }
}

#[test]
fn prepared_callable_host_call_rejects_an_open_named_declaration() {
    let (program, sources, source_id) = prepared_host_callable_program(8);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.trace_enabled = true;
        evaluator.indexed_program = Some(Arc::clone(&program));
        let (key, open) = program.symbol_owner().with_current(|| (
            LoweredFunctionKey::Name(Name::intern("direct")),
            Value::Callable(crate::runtime::value::CallableValue::named(crate::sema::types::CallableKind::Pure, Name::intern("identity").into())),
        ));
        let error = evaluator.call_indexed_direct(key, CallableKind::Pure, &[open], Span::at(source_id, 0)).unwrap()
            .expect_err("a declaration name supplies no checked generic invocation");
        assert!(error.message.contains("checked invocation instance"), "{error:?}");
        assert!(!evaluator.trace_events.iter().any(|event| event.kind == crate::trace::TraceKind::PureEnter && event.name.as_deref() == Some("identity")));
    }
}

#[test]
fn prepared_callable_host_call_checks_its_specialized_input() {
    let (program, sources, source_id) = prepared_host_callable_program(8);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        let make = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("make")));
        let prepared = evaluator.call_indexed_direct(make, CallableKind::Pure, &[], Span::at(source_id, 0)).unwrap().unwrap();
        assert!(matches!(&prepared, Value::Callable(callable) if callable.kind() == crate::sema::types::CallableKind::Pure && callable.instance().is_some()));
        evaluator.trace_enabled = true;
        evaluator.trace_events.clear();
        let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("direct_value")));
        let error = evaluator.call_indexed_direct(key, CallableKind::Pure, &[prepared.clone(), Value::Str(Arc::from("wrong"))], Span::at(source_id, 0)).unwrap()
            .expect_err("the retained Int invocation does not admit a Str argument");
        assert_eq!(error.kind, "type-error");
        assert!(!evaluator.trace_events.iter().any(|event| event.kind == crate::trace::TraceKind::PureEnter && event.name.as_deref() == Some("identity")));
        let cached = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("cached_value")));
        for _ in 0..2 {
            let value = evaluator.call_indexed_direct(cached, CallableKind::Proc, &[prepared.clone(), Value::Int(7)], Span::at(source_id, 0)).unwrap().unwrap();
            assert_eq!(value, Value::Int(8));
        }
        evaluator.trace_events.clear();
        let error = evaluator.call_indexed_direct(cached, CallableKind::Proc, &[prepared, Value::Str(Arc::from("wrong"))], Span::at(source_id, 0)).unwrap()
            .expect_err("a primed cache retains its actual input contract");
        assert_eq!(error.kind, "type-error");
        assert!(!evaluator.trace_events.iter().any(|event| event.kind == crate::trace::TraceKind::PureEnter && event.name.as_deref() == Some("identity")));
    }
}

#[test]
fn prepared_callable_host_call_checks_nested_callable_contracts() {
    let (program, sources, source_id) = record_form_program(
        "type Callback = pure(payload: Int) -> Int
         pure identity(payload) { let _ = payload; 8 }
         pure keep(action, value) { let _ = action(value); action }
         pure number(payload: Int) -> Int { payload + 1 }
         pure text(payload: Str) -> Str { payload }
         proc timed(payload: Int) [time] -> Int { let _ = time.now(); payload }
         proc quiet(payload: Int) [] -> Int { payload }
         proc filesystem(payload: Int) [fs] -> Int { payload }
         pure make() { keep(identity, number) }
         pure make_record() { let callback: Callback = number; keep(identity, {callback}) }
         pure make_broad_record() { keep(identity, {callback: number}) }
         pure make_effect() { keep(identity, timed) }
         pure direct(handle: Pure, value: Any) -> Any { handle.call(value) }
"
    );
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        let (direct, number, text, timed, quiet, filesystem) = program.symbol_owner().with_current(|| (
            LoweredFunctionKey::Name(Name::intern("direct")),
            Value::Callable(crate::runtime::value::CallableValue::named(crate::sema::types::CallableKind::Pure, Name::intern("number").into())), Value::Callable(crate::runtime::value::CallableValue::named(crate::sema::types::CallableKind::Pure, Name::intern("text").into())),
            Value::Callable(crate::runtime::value::CallableValue::named(crate::sema::types::CallableKind::Proc, Name::intern("timed").into())), Value::Callable(crate::runtime::value::CallableValue::named(crate::sema::types::CallableKind::Proc, Name::intern("quiet").into())),
            Value::Callable(crate::runtime::value::CallableValue::named(crate::sema::types::CallableKind::Proc, Name::intern("filesystem").into())),
        ));
        let record = |callback| program.symbol_owner().with_current(|| Value::Record([(Arc::from("callback"), callback)].into_iter().collect()));
        let broad = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("make_broad_record")));
        let prepared = evaluator.call_indexed_direct(broad, CallableKind::Pure, &[], Span::at(source_id, 0)).unwrap().unwrap();
        let Value::Callable(callable) = &prepared else { panic!("the broad record retains its actual handle") };
        let instance = callable.instance().unwrap();
        let target = program.function_view(instance.function, instance.kind).unwrap().unwrap().index();
        program.symbol_owner().with_current(|| {
            let parameters = program.invocation_parameter_types(target, instance.applications().expect("instantiated callable bindings")).unwrap().unwrap();
            assert!(matches!(&parameters[0], Type::Record(fields) if fields.len() == 1 && fields.values().all(|ty| ty == &Type::Function(crate::sema::types::CallableKind::Pure))));
        });
        for callback in [number.clone(), text.clone()] {
            let actual = evaluator.call_indexed_direct(direct, CallableKind::Pure, &[prepared.clone(), record(callback)], Span::at(source_id, 0)).unwrap().unwrap();
            assert_eq!(actual, Value::Int(8), "an actual opaque Pure field admits either Pure signature");
        }
        for (factory, valid, invalid, smaller) in [
            ("make", number.clone(), text.clone(), None),
            ("make_record", record(number), record(text), None),
            ("make_effect", timed, filesystem, Some(quiet)),
        ] {
            let make = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(factory)));
            let prepared = evaluator.call_indexed_direct(make, CallableKind::Pure, &[], Span::at(source_id, 0)).unwrap().unwrap();
            let Value::Callable(callable) = &prepared else { panic!("{factory} must return the actual retained handle") };
            let instance = callable.instance().expect("a prepared invocation retains its closed argument contract");
            let target = program.function_view(instance.function, instance.kind).unwrap().unwrap().index();
            program.symbol_owner().with_current(|| {
                let parameters = program.invocation_parameter_types(target, instance.applications().expect("instantiated callable bindings")).unwrap().unwrap();
                assert!(parameters[0].contains_typed_callable(), "{factory} must retain the actual nested signature: {parameters:?}");
            });
            let actual = evaluator.call_indexed_direct(direct, CallableKind::Pure, &[prepared.clone(), valid], Span::at(source_id, 0)).unwrap().unwrap();
            assert_eq!(actual, Value::Int(8), "{factory}, frames={force_frames}");
            if let Some(smaller) = smaller {
                let actual = evaluator.call_indexed_direct(direct, CallableKind::Pure, &[prepared.clone(), smaller], Span::at(source_id, 0)).unwrap().unwrap();
                assert_eq!(actual, Value::Int(8), "a smaller Proc effect contract remains permitted");
            }
            evaluator.trace_enabled = true;
            evaluator.trace_events.clear();
            let error = evaluator.call_indexed_direct(direct, CallableKind::Pure, &[prepared, invalid], Span::at(source_id, 0)).unwrap()
                .expect_err(&format!("{factory}: the actual nested signature and effects must satisfy the retained contract"));
            assert_eq!(error.kind, "type-error", "{factory}, frames={force_frames}");
            assert!(!evaluator.trace_events.iter().any(|event| event.kind == crate::trace::TraceKind::PureEnter && event.name.as_deref() == Some("identity")));
        }
    }
}

#[test]
fn prepared_callable_host_call_checks_module_export_signatures() {
    let (program, sources, source_id) = record_form_program(
        "type Service = module { export pure item(payload: Int) -> Int }
         pure identity(payload) { let _ = payload; 8 }
         pure keep(action, value) { let _ = action(value); action }
         pure number(payload: Int) -> Int { payload + 1 }
         pure text(payload: Str) -> Str { payload }
         pure make(service: Service) { keep(identity, service) }
         pure direct(handle: Pure, value: Any) -> Any { handle.call(value) }
"
    );
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        let (make, direct, number, text) = program.symbol_owner().with_current(|| (
            LoweredFunctionKey::Name(Name::intern("make")), LoweredFunctionKey::Name(Name::intern("direct")),
            Value::Callable(crate::runtime::value::CallableValue::named(crate::sema::types::CallableKind::Pure, Name::intern("number").into())), Value::Callable(crate::runtime::value::CallableValue::named(crate::sema::types::CallableKind::Pure, Name::intern("text").into())),
        ));
        program.symbol_owner().with_current(|| {
            for (name, expected) in [("number", Type::Int), ("text", Type::Str)] {
                let target = program.function_view(LoweredFunctionKey::Name(Name::intern(name)), CallableKind::Pure).unwrap().unwrap().index();
                let actual = program.invocation_callable_type(target, &crate::runtime::eval::indexed::full::PreparedInvocationBindings::default()).unwrap();
                assert_eq!(actual.sig.params[0].ty, expected);
                assert_eq!(*actual.sig.return_ty, expected);
            }
        });
        let module = |callable| program.symbol_owner().with_current(|| Value::Module([(Arc::from("item"), callable)].into_iter().collect()));
        let prepared = evaluator.call_indexed_direct(make, CallableKind::Pure, &[module(number.clone())], Span::at(source_id, 0)).unwrap().unwrap();
        let Value::Callable(callable) = &prepared else { panic!("the module argument retains its actual invocation") };
        let instance = callable.instance().unwrap();
        program.symbol_owner().with_current(|| {
            let target = program.function_view(instance.function, instance.kind).unwrap().unwrap().index();
            let parameters = program.invocation_parameter_types(target, instance.applications().expect("instantiated callable bindings")).unwrap().unwrap();
            let Type::Module(exports) = &parameters[0] else { panic!("the actual retained input is a typed module") };
            let Some(crate::sema::types::ModuleExportType::Callable { kind: crate::sema::types::CallableKind::Pure, sig, .. }) = exports.get(&Name::intern("item")) else { panic!("the module retains the actual Pure export contract") };
            assert_eq!(sig.params[0].ty, Type::Int);
            assert_eq!(*sig.return_ty, Type::Int);
        });
        let value = evaluator.call_indexed_direct(direct, CallableKind::Pure, &[prepared.clone(), module(number)], Span::at(source_id, 0)).unwrap().unwrap();
        assert_eq!(value, Value::Int(8));
        evaluator.trace_enabled = true;
        evaluator.trace_events.clear();
        let error = evaluator.call_indexed_direct(direct, CallableKind::Pure, &[prepared, module(text)], Span::at(source_id, 0)).unwrap()
            .expect_err("the actual exported signature must satisfy the retained module contract");
        assert_eq!(error.kind, "type-error");
        assert!(!evaluator.trace_events.iter().any(|event| event.kind == crate::trace::TraceKind::PureEnter && event.name.as_deref() == Some("identity")));
    }
}

#[test]
fn prepared_callable_host_call_checks_module_export_parameter_labels() {
    let (program, sources, source_id) = record_form_program(
        "type Service = module { export pure item(left: Int, right: Int) -> Int }
         pure identity(payload) { let _ = payload; 8 }
         pure keep(action, value) { let _ = action(value); action }
         pure ordered(left: Int, right: Int) -> Int { left * 10 + right }
         pure swapped(right: Int, left: Int) -> Int { right * 10 + left }
         pure make(service: Service) { keep(identity, service) }
         pure direct(handle: Pure, value: Any) -> Any { handle.call(value) }
"
    );
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        let (make, direct, ordered, swapped) = program.symbol_owner().with_current(|| {
            for (name, labels) in [("ordered", ["left", "right"]), ("swapped", ["right", "left"])] {
                let target = program.function_view(LoweredFunctionKey::Name(Name::intern(name)), CallableKind::Pure).unwrap().unwrap().index();
                let actual = program.invocation_callable_type(target, &crate::runtime::eval::indexed::full::PreparedInvocationBindings::default()).unwrap();
                assert!((actual.kind == crate::sema::types::CallableKind::Pure));
                assert_eq!(actual.sig.params.len(), 2);
                for (parameter, label) in actual.sig.params.iter().zip(labels) {
                    assert_eq!(parameter.name, Name::intern(label));
                    assert_eq!(parameter.ty, Type::Int);
                }
                assert_eq!(*actual.sig.return_ty, Type::Int);
            }
            (LoweredFunctionKey::Name(Name::intern("make")), LoweredFunctionKey::Name(Name::intern("direct")),
                Value::Callable(crate::runtime::value::CallableValue::named(crate::sema::types::CallableKind::Pure, Name::intern("ordered").into())), Value::Callable(crate::runtime::value::CallableValue::named(crate::sema::types::CallableKind::Pure, Name::intern("swapped").into())))
        });
        let module = |callable| program.symbol_owner().with_current(|| Value::Module([(Arc::from("item"), callable)].into_iter().collect()));
        let span = Span::at(source_id, 0);
        let prepared = evaluator.call_indexed_direct(make, CallableKind::Pure, &[module(ordered.clone())], span).unwrap().unwrap();
        assert_eq!(evaluator.call_indexed_direct(direct, CallableKind::Pure, &[prepared.clone(), module(ordered)], span).unwrap().unwrap(), Value::Int(8));
        evaluator.trace_enabled = true;
        evaluator.trace_events.clear();
        let error = evaluator.call_indexed_direct(direct, CallableKind::Pure, &[prepared, module(swapped)], span).unwrap().expect_err("the actual export's swapped parameter labels cannot satisfy the retained named argument layout");
        assert_eq!(error.kind, "type-error");
        assert!(!evaluator.trace_events.iter().any(|event| event.kind == crate::trace::TraceKind::PureEnter && event.name.as_deref() == Some("identity")));
    }
}

#[test]
fn prepared_callable_host_call_checks_stream_items_without_pulling() {
    let (program, sources, source_id) = record_form_program(
        "var pulls = 0
         var closed = 0
         proc close() [] { closed = closed + 1 }
         stream numbers() [error] -> Stream[Int] { defer close(); pulls = pulls + 1; yield 7 }
         stream texts() [error] -> Stream[Str] { defer close(); pulls = pulls + 1; yield \"wrong\" }
         stream unsigned() [] -> Stream[UInt] { yield 7 }
         pure identity(payload) { let _ = payload; 8 }
         pure keep(action, value) { let _ = action(value); action }
         pure make(items: Stream[Int]) { keep(identity, items) }
         pure make_unsigned(items: Stream[UInt]) { keep(identity, items) }
         pure direct(handle: Pure, value: Any) -> Any { handle.call(value) }
         proc observations() [] { {pulls, closed} }
"
    );
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        for step in 0..program.driver_step_count().unwrap() { evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap(); }
        let (numbers, texts, unsigned, make, make_unsigned, direct, observations) = program.symbol_owner().with_current(|| (
            LoweredFunctionKey::Name(Name::intern("numbers")), LoweredFunctionKey::Name(Name::intern("texts")),
            LoweredFunctionKey::Name(Name::intern("unsigned")), LoweredFunctionKey::Name(Name::intern("make")),
            LoweredFunctionKey::Name(Name::intern("make_unsigned")), LoweredFunctionKey::Name(Name::intern("direct")),
            LoweredFunctionKey::Name(Name::intern("observations")),
        ));
        let numbers = evaluator.call_indexed_direct(numbers, CallableKind::Stream, &[], Span::at(source_id, 0)).unwrap().unwrap();
        let texts = evaluator.call_indexed_direct(texts, CallableKind::Stream, &[], Span::at(source_id, 0)).unwrap().unwrap();
        let unsigned = evaluator.call_indexed_direct(unsigned, CallableKind::Stream, &[], Span::at(source_id, 0)).unwrap().unwrap();
        for source in [&numbers, &texts, &unsigned] {
            assert!(matches!(source, Value::Stream(stream) if stream.script.is_some() && stream.source.is_none() && stream.items.is_empty()));
        }
        let prepared = evaluator.call_indexed_direct(make, CallableKind::Pure, std::slice::from_ref(&numbers), Span::at(source_id, 0)).unwrap().unwrap();
        let Value::Callable(callable) = &prepared else { panic!("the stream argument retains its actual invocation") };
        let instance = callable.instance().unwrap();
        program.symbol_owner().with_current(|| {
            let target = program.function_view(instance.function, instance.kind).unwrap().unwrap().index();
            let parameters = program.invocation_parameter_types(target, instance.applications().expect("instantiated callable bindings")).unwrap().unwrap();
            assert_eq!(parameters[0], Type::Stream(Box::new(Type::Int)));
        });
        let actual = evaluator.call_indexed_direct(direct, CallableKind::Pure, &[prepared.clone(), numbers.clone()], Span::at(source_id, 0)).unwrap().unwrap();
        assert_eq!(actual, Value::Int(8));
        evaluator.trace_enabled = true;
        evaluator.trace_events.clear();
        let actual = evaluator.call_indexed_direct(direct, CallableKind::Pure, &[prepared, texts], Span::at(source_id, 0)).unwrap();
        let observed = evaluator.call_indexed_direct(observations, CallableKind::Proc, &[], Span::at(source_id, 0)).unwrap().unwrap();
        let Value::Result(ResultValue::Ok(observed)) = observed else { panic!("observations retain their ordinary result envelope") };
        let Value::Record(fields) = *observed else { panic!("observations retain their record") };
        assert_eq!(fields.get("pulls"), Some(&Value::Int(0)), "validation must not pull either producer");
        assert_eq!(fields.get("closed"), Some(&Value::Int(0)), "an unstarted producer has registered no deferred cleanup");
        let error = actual.expect_err("the actual Str producer cannot satisfy the retained Stream[Int] input");
        assert_eq!(error.kind, "type-error");
        assert!(!evaluator.trace_events.iter().any(|event| event.kind == crate::trace::TraceKind::PureEnter && event.name.as_deref() == Some("identity")));
        let prepared_unsigned = evaluator.call_indexed_direct(make_unsigned, CallableKind::Pure, std::slice::from_ref(&unsigned), Span::at(source_id, 0)).unwrap().unwrap();
        assert_eq!(evaluator.call_indexed_direct(direct, CallableKind::Pure, &[prepared_unsigned.clone(), unsigned], Span::at(source_id, 0)).unwrap().unwrap(), Value::Int(8));
        evaluator.trace_events.clear();
        let error = evaluator.call_indexed_direct(direct, CallableKind::Pure, &[prepared_unsigned, numbers], Span::at(source_id, 0)).unwrap().expect_err("a signed item contract cannot promise UInt even when its unpulled literal happens to be positive");
        assert_eq!(error.kind, "type-error");
        assert!(!evaluator.trace_events.iter().any(|event| event.kind == crate::trace::TraceKind::PureEnter && event.name.as_deref() == Some("identity")));
    }
}

#[test]
fn prepared_stream_contracts_preserve_lazy_uint_and_delegation_checks() {
    let (program, sources, source_id) = record_form_program(
        "stream checked(value: Int) [] -> Stream[UInt] { yield value }
         stream listed() [] -> Stream[Int] { yield @[1, 2] }
         stream delegated() [] -> Stream[Int] { yield @listed() }
"
    );
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        let (checked, delegated) = program.symbol_owner().with_current(|| (
            LoweredFunctionKey::Name(Name::intern("checked")), LoweredFunctionKey::Name(Name::intern("delegated")),
        ));
        let span = Span::at(source_id, 0);
        for input in [7, -1] {
            let value = evaluator.call_indexed_direct(checked, CallableKind::Stream, &[Value::Int(input)], span).unwrap().unwrap();
            let Value::Stream(mut stream) = value else { panic!("creation retains an unpulled script producer") };
            assert!(stream.contract.is_some());
            assert!(stream.items.is_empty());
            let pulled = evaluator.stream_next(&mut stream, span);
            if input >= 0 { assert_eq!(pulled.unwrap(), Some(Value::Int(input))); }
            else { assert_eq!(pulled.unwrap_err().kind, "type-error"); }
        }
        let value = evaluator.call_indexed_direct(delegated, CallableKind::Stream, &[], span).unwrap().unwrap();
        let Value::Stream(mut stream) = value else { panic!("delegation retains an unpulled script producer") };
        assert!(stream.contract.is_some());
        assert_eq!(evaluator.stream_next(&mut stream, span).unwrap(), Some(Value::Int(1)));
        assert_eq!(evaluator.stream_next(&mut stream, span).unwrap(), Some(Value::Int(2)));
        assert_eq!(evaluator.stream_next(&mut stream, span).unwrap(), None);
    }
}

#[test]
fn prepared_callable_host_call_checks_ground_stream_parameters() {
    let (program, sources, source_id) = record_form_program(
        "stream numbers() [] -> Stream[Int] { yield 7 }
         stream texts() [] -> Stream[Str] { yield \"wrong\" }
         pure accept(items: Stream[Int]) -> Int { let _ = items; 8 }
         pure direct(handle: Pure, value: Any) -> Any { handle.call(value) }
"
    );
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        let (numbers, texts, direct, accept) = program.symbol_owner().with_current(|| (
            LoweredFunctionKey::Name(Name::intern("numbers")), LoweredFunctionKey::Name(Name::intern("texts")),
            LoweredFunctionKey::Name(Name::intern("direct")), Value::Callable(crate::runtime::value::CallableValue::named(crate::sema::types::CallableKind::Pure, Name::intern("accept").into())),
        ));
        let numbers = evaluator.call_indexed_direct(numbers, CallableKind::Stream, &[], Span::at(source_id, 0)).unwrap().unwrap();
        let texts = evaluator.call_indexed_direct(texts, CallableKind::Stream, &[], Span::at(source_id, 0)).unwrap().unwrap();
        assert_eq!(evaluator.call_indexed_direct(direct, CallableKind::Pure, &[accept.clone(), numbers], Span::at(source_id, 0)).unwrap().unwrap(), Value::Int(8));
        evaluator.trace_enabled = true;
        evaluator.trace_events.clear();
        let error = evaluator.call_indexed_direct(direct, CallableKind::Pure, &[accept, texts], Span::at(source_id, 0)).unwrap().expect_err("an opaque ground call must validate its actual Stream[Int] parameter before entering the body");
        assert_eq!(error.kind, "type-error");
        assert!(!evaluator.trace_events.iter().any(|event| event.kind == crate::trace::TraceKind::PureEnter && event.name.as_deref() == Some("accept")));
    }
}

#[test]
fn prepared_callable_host_call_retains_native_stream_factory_contracts() {
    on_scoped_body_test_stack(prepared_callable_host_call_retains_native_stream_factory_contracts_on_large_stack);
}

fn prepared_callable_host_call_retains_native_stream_factory_contracts_on_large_stack() {
    let directory = tempfile::Builder::new().prefix("host-stream-").tempdir_in(".work/tmp").unwrap();
    let path = directory.path().join("lines");
    std::fs::write(&path, b"first\nsecond\n").unwrap();
    let (program, sources, source_id) = record_form_program(
        "type Lines = {data: Stream[Str]}
         proc factory(file: Path) [fs, error] -> Result[Stream[Str]] { file.lines() }
         proc boxed(file: Path) [fs, error] -> Result[Lines] { Ok({data: file.lines()?}) }
         stream wrong() [] -> Stream[Int] { yield 7 }
         pure identity(payload) { let _ = payload; 8 }
         pure keep(action, value) { let _ = action(value); action }
         pure make(items: Stream[Str]) { keep(identity, items) }
         pure direct(handle: Pure, value: Any) -> Any { handle.call(value) }
"
    );
    for force_frames in [false, true] {
        for boxed in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let (factory, make, direct, wrong) = program.symbol_owner().with_current(|| (
                LoweredFunctionKey::Name(Name::intern(if boxed { "boxed" } else { "factory" })),
                LoweredFunctionKey::Name(Name::intern("make")), LoweredFunctionKey::Name(Name::intern("direct")),
                LoweredFunctionKey::Name(Name::intern("wrong")),
            ));
            let path = Value::Path(crate::runtime::value::PathValue::from_text(path.to_str().unwrap()).unwrap());
            let output = evaluator.call_indexed_direct(factory, CallableKind::Proc, &[path], Span::at(source_id, 0)).unwrap().unwrap();
            let Value::Result(ResultValue::Ok(output)) = output else { panic!("native factory keeps its Result envelope") };
            let actual = if boxed {
                let Value::Record(fields) = *output else { panic!("native stream keeps its aggregate") };
                fields.get("data").unwrap().clone()
            } else { *output };
            let Value::Stream(stream) = &actual else { panic!("native factory returns a stream") };
            assert!(stream.source.is_some() && stream.script.is_none() && stream.items.is_empty());
            assert!(stream.contract.is_some(), "ordinary unchecked native factory output must retain its solved contract before aggregate escape");
            let prepared = evaluator.call_indexed_direct(make, CallableKind::Pure, std::slice::from_ref(&actual), Span::at(source_id, 0)).unwrap().unwrap();
            assert_eq!(evaluator.call_indexed_direct(direct, CallableKind::Pure, &[prepared.clone(), actual.clone()], Span::at(source_id, 0)).unwrap().unwrap(), Value::Int(8));
            assert_eq!(stream.next_live(Span::at(source_id, 0)).unwrap(), Some(Value::Str(Arc::from("first"))), "contract validation must leave the first native item untouched");
            let wrong = evaluator.call_indexed_direct(wrong, CallableKind::Stream, &[], Span::at(source_id, 0)).unwrap().unwrap();
            let error = evaluator.call_indexed_direct(direct, CallableKind::Pure, &[prepared, wrong], Span::at(source_id, 0)).unwrap().expect_err("actual Int producer cannot satisfy retained native Str item contract");
            assert_eq!(error.kind, "type-error");
        }
    }
}

#[test]
fn prepared_callable_cache_separates_specializations_with_identical_arguments() {
    let (program, sources, source_id) = record_form_program(concat!(
        "pure identity(payload) { let _ = payload; 8 }\n",
        "pure keep(action, value) { let _ = action(value); action }\n",
        "pure make_number() { let values: List[Int] = [1]; keep(identity, values) }\n",
        "pure make_text() { let values: List[Str] = [\"x\"]; keep(identity, values) }\n",
        "pure make_number_again() { let values: List[Int] = [2]; keep(identity, values) }\n",
        "proc cached(handle: Pure, value: Any) -> Any { utils.cache(handle, [value]) }\n",
    ));
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        let mut handles = Vec::new();
        for name in ["make_number", "make_text", "make_number_again"] {
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
            let value = evaluator.call_indexed_direct(key, CallableKind::Pure, &[], Span::at(source_id, 0)).unwrap().unwrap();
            assert!(matches!(&value, Value::Callable(callable) if callable.kind() == crate::sema::types::CallableKind::Pure && callable.instance().is_some()));
            handles.push(value);
        }
        assert_eq!(handles[0], handles[1], "source equality remains declaration identity");
        evaluator.trace_enabled = true;
        evaluator.trace_events.clear();
        let cached = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("cached")));
        for handle in [&handles[0], &handles[0], &handles[2], &handles[1], &handles[1]] {
            let actual = evaluator.call_indexed_direct(cached, CallableKind::Proc, &[handle.clone(), Value::List(Vec::new())], Span::at(source_id, 0)).unwrap().unwrap();
            assert_eq!(actual, Value::Int(8));
        }
        assert_eq!(evaluator.trace_events.iter().filter(|event| event.kind == crate::trace::TraceKind::PureEnter && event.name.as_deref() == Some("identity")).count(), 2, "distinct List[Int]/List[Str] instances miss independently; clones hit");
    }
}

#[test]
fn generic_proc_cleanup_preserves_lifo_and_error_triggers() {
    for generic in [false, true] {
        let annotation = if generic { "" } else { ": Int" };
        let source = format!("error CleanupFailure = Failed(message: Str)\nvar trace = \"\"\nproc mark(label: Str) [] -> Unit {{ trace = trace + label }}\npure fail() -> Result[Unit, CleanupFailure] {{ Err(CleanupFailure.Failed(message: \"original\")) }}\nproc keep(value{annotation}, failing: Bool) [error] {{ defer mark(\"a\"); errdefer mark(\"e\"); defer {{ mark(\"b\") }}; if failing {{ fail()? }}; value }}\nproc number(failing: Bool) [error] {{ keep(7, failing) }}\nproc observe() [] -> Str {{ trace }}\n");
        let source = if generic { format!("{source}proc text() [error] {{ keep(\"row\", false) }}\n") } else { source };
        let (program, sources, source_id) = record_form_program(&source);
        for force_frames in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let span = Span::at(source_id, 0);
            for step in 0..program.driver_step_count().unwrap() { evaluator.eval_indexed_driver_step(step, span).unwrap().unwrap(); }
            let (number, observe) = program.symbol_owner().with_current(|| (LoweredFunctionKey::Name(Name::intern("number")), LoweredFunctionKey::Name(Name::intern("observe"))));
            assert_eq!(evaluator.call_indexed_direct(number, CallableKind::Proc, &[Value::Bool(false)], span).unwrap().unwrap(), Value::Result(ResultValue::Ok(Box::new(Value::Int(7)))));
            assert_eq!(evaluator.call_indexed_direct(observe, CallableKind::Proc, &[], span).unwrap().unwrap(), Value::Str(Arc::from("ba")));
            let Value::Result(ResultValue::Err(error)) = evaluator.call_indexed_direct(number, CallableKind::Proc, &[Value::Bool(true)], span).unwrap().unwrap() else { panic!("the original failure remains returned data") };
            let Value::Error(error) = *error else { panic!("the nominal cleanup source error remains intact") };
            assert_eq!((&*error.family, &*error.variant), ("CleanupFailure", "Failed"));
            assert_eq!(error.payload.get("message"), Some(&Value::Str(Arc::from("original"))));
            assert_eq!(evaluator.call_indexed_direct(observe, CallableKind::Proc, &[], span).unwrap().unwrap(), Value::Str(Arc::from("babea")), "generic={generic} frames={force_frames}");
            if generic {
                let text = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("text")));
                assert_eq!(evaluator.call_indexed_direct(text, CallableKind::Proc, &[], span).unwrap().unwrap(), Value::Result(ResultValue::Ok(Box::new(Value::Str(Arc::from("row"))))));
                assert_eq!(evaluator.call_indexed_direct(observe, CallableKind::Proc, &[], span).unwrap().unwrap(), Value::Str(Arc::from("babeaba")));
            }
        }
    }
}

#[test]
fn generic_proc_cleanup_run_preserves_order_and_failure_status() {
    let directory = tempfile::Builder::new().prefix("generic-cleanup-").tempdir_in(".work/tmp").unwrap();
    let path = directory.path().canonicalize().unwrap().join("trace");
    for generic in [false, true] {
        for failing in [false, true] {
            for block in [false, true] {
                let annotation = if generic { "" } else { ": Int" };
                let action = |label: &str, status: i32| {
                    let run = format!(r#"run sh -c "printf {label} >> \"$1\"; exit {status}" cleanup {:?} ?"#, path.to_str().unwrap());
                    if block { format!("defer {{ {run} }}") } else { format!("defer {run}") }
                };
                let cleanup = if failing { action("c", 7) } else { format!("{}; {}", action("a", 0), action("b", 0)) };
                let source = format!("proc keep(value{annotation}) [io, error] {{ {cleanup}; value }}\nproc number() [io, error] {{ keep(7) }}\n");
                let (program, sources, source_id) = record_form_program(&source);
                for force_frames in [false, true] {
                    std::fs::write(&path, b"").unwrap();
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
                    evaluator.force_frames = force_frames;
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let span = Span::at(source_id, 0);
                    let number = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("number")));
                    let actual = evaluator.call_indexed_direct(number, CallableKind::Proc, &[], span).unwrap().unwrap();
                    if failing && block {
                        let Value::Result(ResultValue::Err(error)) = actual else { panic!("a cleanup process failure stays Err data: generic={generic} frames={force_frames} actual={actual:?}") };
                        let Value::RunError(error) = *error else { panic!("the cleanup retains its process status") };
                        assert_eq!(error.status.as_ref().unwrap().code, Some(7));
                        assert_eq!(std::fs::read(&path).unwrap(), b"c");
                    } else {
                        assert_eq!(actual, Value::Result(ResultValue::Ok(Box::new(Value::Int(7)))));
                        assert_eq!(std::fs::read(&path).unwrap(), if failing { b"c".as_slice() } else { b"ba".as_slice() });
                    }
                }
            }
        }
    }
}

#[test]
fn generic_with_guard_with_bindings_keep_order_and_scopes() {
    for generic in [false, true] {
        let annotation = if generic { "" } else { ": Int" };
        let source = format!("error BindingFailure = Missing(message: Str)\nvar trace = \"\"\nproc step(value: Int, valid: Bool) [] -> Result[Int, BindingFailure] {{ trace = trace + \"i\"; if valid {{ Ok(value) }} else {{ Err(BindingFailure.Missing(message: \"stopped\")) }} }}\nproc keep(value{annotation}, valid: Bool) [] {{ var total = 0; with first = step(3, valid), second = step(first + 1, true) {{ total = first + second; trace = trace + \"b\" }} else {{ |failure| let _ = failure; total = -1; trace = trace + \"e\" }}; {{value, total}} }}\nproc number(valid: Bool) [] {{ keep(7, valid) }}\nproc observe() [] -> Str {{ trace }}\n");
        let (program, sources, source_id) = record_form_program(&source);
        for force_frames in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let span = Span::at(source_id, 0);
            for step in 0..program.driver_step_count().unwrap() { evaluator.eval_indexed_driver_step(step, span).unwrap().unwrap(); }
            let (number, observe) = program.symbol_owner().with_current(|| (LoweredFunctionKey::Name(Name::intern("number")), LoweredFunctionKey::Name(Name::intern("observe"))));
            for (valid, total, trace) in [(true, 7, "iib"), (false, -1, "iibie")] {
                let Value::Result(ResultValue::Ok(value)) = evaluator.call_indexed_direct(number, CallableKind::Proc, &[Value::Bool(valid)], span).unwrap().unwrap() else { panic!("With handles its failed initializer locally") };
                let Value::Record(fields) = *value else { panic!("the generic payload and branch result retain their record") };
                assert_eq!(fields.get("value"), Some(&Value::Int(7)));
                assert_eq!(fields.get("total"), Some(&Value::Int(total)));
                assert_eq!(evaluator.call_indexed_direct(observe, CallableKind::Proc, &[], span).unwrap().unwrap(), Value::Str(Arc::from(trace)), "generic={generic} frames={force_frames}");
            }
        }
    }
}

#[test]
fn generic_with_guard_guards_keep_result_optional_and_destructuring() {
    for generic in [false, true] {
        let annotation = if generic { "" } else { ": Int" };
        let source = format!("type Row = {{number: Int}}\nerror BindingFailure = Missing(message: Str)\nproc keep(value{annotation}, selected: Result[Row, BindingFailure], optional: Int?) [] {{ guard let {{number}} = selected else {{ |failure| let _ = failure; return {{value, total: -1}} }}; guard let present = optional else {{ return {{value, total: -2}} }}; {{value, total: number + present}} }}\nproc number(mode: Int) [] {{ let selected: Result[Row, BindingFailure] = if mode == 0 {{ Err(BindingFailure.Missing(message: \"missing\")) }} else {{ Ok(Row(number: 3)) }}; let optional: Int? = if mode == 1 {{ null }} else {{ 4 }}; keep(7, selected, optional) }}\n");
        let (program, sources, source_id) = record_form_program(&source);
        for force_frames in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let span = Span::at(source_id, 0);
            let number = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("number")));
            for (mode, total) in [(0, -1), (1, -2), (2, 7)] {
                let Value::Result(ResultValue::Ok(value)) = evaluator.call_indexed_direct(number, CallableKind::Proc, &[Value::Int(mode)], span).unwrap().unwrap() else { panic!("guard failure returns its checked ordinary value") };
                let Value::Record(fields) = *value else { panic!("guard success bindings preserve the payload record") };
                assert_eq!(fields.get("value"), Some(&Value::Int(7)));
                assert_eq!(fields.get("total"), Some(&Value::Int(total)), "generic={generic} mode={mode} frames={force_frames}");
            }
        }
    }
}

#[test]
fn inferred_application_runtime_proc_error_origins_preserve_cleanup() {
    for payload in [-1, 7] {
        let source = format!("var closed = 0\nproc close() [] {{ closed = closed + 1 }}\nproc side(payload: Int) [error] -> Int {{ defer close(); assert payload > 0; payload }}\nproc relay(action, value) {{ action(value) }}\nproc number() [error] {{ let actual = relay(side, {payload}); {{actual, closed}} }}\n");
        let (program, sources, source_id) = record_form_program(&source);
        for force_frames in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            for step in 0..program.driver_step_count().unwrap() {
                evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap();
            }
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("number")));
            let result = evaluator.call_indexed_direct(key, CallableKind::Proc, &[], Span::at(source_id, 0)).unwrap().unwrap();
            let Value::Result(ResultValue::Ok(record)) = result else { panic!("the enclosing proc preserves the observed Result as data") };
            let Value::Record(fields) = *record else { panic!("the observations retain their record") };
            assert_eq!(fields.get("closed"), Some(&Value::Int(1)));
            match fields.get("actual").unwrap() {
                Value::Result(ResultValue::Ok(value)) if payload > 0 => assert_eq!(**value, Value::Int(7)),
                Value::Result(ResultValue::Err(error)) if payload < 0 => {
                    let Value::Error(error) = error.as_ref() else { panic!("the original assertion error remains the actual failure") };
                    assert_eq!((&*error.family, &*error.variant), ("AssertionError", "Failed"));
                }
                other => panic!("proc application result differs: {other:?}"),
            }
        }
    }
}

fn assert_prepared_deferred_error_result(source: &str) {
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        for step in 0..program.driver_step_count().unwrap() {
            evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap();
        }
        let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("number")));
        let result = evaluator.call_indexed_direct(key, CallableKind::Proc, &[], Span::at(source_id, 0)).unwrap().unwrap();
        let Value::Result(ResultValue::Ok(record)) = result else { panic!("the callee cleanup failure is returned data, not an error from the observer: {result:?}") };
        let Value::Record(fields) = *record else { panic!("the observer retains its record") };
        let Some(Value::Result(ResultValue::Err(error))) = fields.get("actual") else { panic!("the callee preserves the deferred failure as an actual Err") };
        let Value::Error(error) = error.as_ref() else { panic!("the actual cleanup error remains intact") };
        assert_eq!((&*error.family, &*error.variant), ("AssertionError", "Failed"));
    }
}

#[test]
fn inferred_application_runtime_concrete_proc_cleanup_returns_error_data() {
    assert_prepared_deferred_error_result(concat!(
        "proc close() [error] { assert false }\n",
        "proc relay(value: Int) [error] -> Result[Int] { defer close(); Ok(value) }\n",
        "proc number() [error] { let actual = relay(7); {actual} }\n",
    ));
}

#[test]
fn inferred_application_runtime_dependent_proc_cleanup_returns_error_data() {
    assert_prepared_deferred_error_result(concat!(
        "proc close() [error] { assert false }\n",
        "pure identity(payload: Int) -> Int { payload }\n",
        "proc relay(action, value) { defer close(); action(value) }\n",
        "proc number() [error] { let actual = relay(identity, 7); {actual} }\n",
    ));
}

#[test]
fn inferred_application_runtime_dependent_proc_cleanup_widens_nominal_errors() {
    assert_prepared_deferred_error_result(concat!(
        "error CallbackFailure = Missing(message: Str)\n",
        "proc close() [error] { assert false }\n",
        "pure identity(payload: Int) -> Result[Int, CallbackFailure] { Ok(payload) }\n",
        "proc relay(action, value) { defer close(); action(value) }\n",
        "proc number() [error] { let actual = relay(identity, 7); {actual} }\n",
    ));
}

#[test]
fn generic_nonmaterial_function_shorthand_preserves_selected_ground_value() {
    let source = concat!(
        "pure fixed() -> Int { 11 }\n",
        "pure keep(payload) { let _ = {items: [], fixed, payload}; payload }\n",
        "pure number() -> Int { keep(7) }\n",
        "pure text() -> Str { keep(\"value\") }\n",
    );
    let mut sources = SourceMap::default();
    let source_id = sources.add_file("generic-nonmaterial-function-shorthand.xsh", source.to_string());
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    assert!(checked.bodies.selected_function_references.values().any(|(target, _)| target.name == "fixed"));
    let program = Arc::new(FullBuilder::build_compact(&parsed.arena, &checked, source, Arc::new(sources.clone()), source_id).unwrap());
    drop(checked);
    drop(parsed);
    for force_frames in [false, true] {
        for (name, expected) in [("number", Value::Int(7)), ("text", Value::Str(Arc::from("value")))] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
            let actual = evaluator.call_indexed_direct(key, CallableKind::Pure, &[], Span::at(source_id, 0)).unwrap().unwrap();
            assert_eq!(actual, expected, "{name} frames={force_frames}");
        }
    }
}

#[test]
fn generic_nonmaterial_constructors_execute_after_frontend_disposal() {
    for prefix in [
        "let state = {items: [], ...{items: [7]}, payload};",
        "let _ = map.empty();",
        "let _ = [];",
        "let state = {...{items: []}, ...{items: [7]}, payload};",
        "let state = {...{items: [], payload}, ...{items: [7]}};",
        "let fixed = {other: 1}; let state = {...{items: [], ...fixed, payload}, ...{items: [7]}};",
        "let state = {...{nested: {items: []}}, ...{nested: {items: [7]}}, payload};",
        "let state = {items: map.empty(), ...{items: {[\"key\"]: 7}}, payload};",
        "let state = {...{items: map.empty(), payload}, ...{items: {[\"key\"]: 7}}};",
        "let _ = {items: [], payload};",
        "let _ = {items: map.empty(), payload};",
    ] {
        let source = format!("pure keep(payload) {{ {prefix} payload }}\npure number() -> Int {{ keep(7) }}\npure text() -> Str {{ keep(\"value\") }}\n");
        let mut sources = SourceMap::default();
        let source_id = sources.add_file("generic-nonmaterial-constructor.xsh", source.clone());
        let parsed = Parser::parse_source_arena_only(source_id, &source);
        assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
        let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        assert!(checked.diagnostics.is_empty(), "{source}\n{:?}", checked.diagnostics);
        let program = Arc::new(FullBuilder::build_compact(&parsed.arena, &checked, &source, Arc::new(sources.clone()), source_id).unwrap_or_else(|error| panic!("{source}\n{error:?}")));
        drop(checked);
        drop(parsed);
        for force_frames in [false, true] {
            for (name, expected) in [("number", Value::Int(7)), ("text", Value::Str(Arc::from("value")))] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
                evaluator.force_frames = force_frames;
                evaluator.indexed_program = Some(Arc::clone(&program));
                let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
                let actual = evaluator.call_indexed_direct(key, CallableKind::Pure, &[], Span::at(source_id, 0)).unwrap().unwrap();
                assert_eq!(actual, expected, "{prefix} frames={force_frames}");
            }
        }
    }
}

#[test]
fn generic_proc_cleanup_terminal_commands_keep_unit_result() {
    for annotation in [": Int", ""] {
        let source = format!("proc report(payload{annotation}) {{ print \"terminal\" }}\nproc integer() {{ report(7) }}\n");
        let source = if annotation.is_empty() { source + "proc text() { report(\"word\") }\n" } else { source };
        let (program, sources, source_id) = record_form_program(&source);
        for force_frames in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let integer = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("integer")));
            assert_eq!(evaluator.call_indexed_direct(integer, CallableKind::Proc, &[], Span::at(source_id, 0)).unwrap().unwrap(), Value::Result(ResultValue::Ok(Box::new(Value::Unit))));
            assert_eq!(evaluator.stdout, b"terminal\n");
            if annotation.is_empty() {
                let text = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("text")));
                assert_eq!(evaluator.call_indexed_direct(text, CallableKind::Proc, &[], Span::at(source_id, 0)).unwrap().unwrap(), Value::Result(ResultValue::Ok(Box::new(Value::Unit))));
                assert_eq!(evaluator.stdout, b"terminal\nterminal\n");
            }
        }
    }
}

#[test]
fn generic_proc_cleanup_pure_callbacks_keep_owner_restriction() {
    for cleanup in [false, true] {
        let body = if cleanup { "defer { let _ = value }; value" } else { "value" };
        let source = format!("pure callback(value) {{ {body} }}\nproc apply(value) {{ callback(value) }}\nlet _ = apply(7)\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        if cleanup {
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckPureDefer)), "{:?}", checked.diagnostics);
        } else {
            assert!(!checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error), "{:?}", checked.diagnostics);
        }
    }
}

fn on_scoped_body_test_stack(test: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new().stack_size(16 * 1024 * 1024).spawn(test).unwrap().join()
        .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
}

#[test]
fn generic_guard_run_initializers_preserve_success_and_failure_handlers() {
    on_scoped_body_test_stack(|| {
    for generic in [false, true] {
        let annotation = if generic { "" } else { ": Int" };
        let mut source = format!(r#"
proc keep(payload{annotation}, failing: Bool) [process, error] {{
  guard let text = (try run.text sh -c (if failing {{ "exit 7" }} else {{ "printf kept" }})) else {{ |failure|
    let _ = failure
    return {{payload, text: "failed"}}
  }}
  {{payload, text}}
}}
assert keep(7, false) == Ok({{payload: 7, text: "kept"}})
assert keep(7, true) == Ok({{payload: 7, text: "failed"}})
"#);
        if generic { source.push_str("assert keep(\"row\", false) == Ok({payload: \"row\", text: \"kept\"})\n"); }
        let (program, sources, source_id) = record_form_program(&source);
        for force_frames in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            for step in 0..program.driver_step_count().unwrap() {
                evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap_or_else(|error| panic!("generic={generic} frames={force_frames}: {error:?}"));
            }
        }
    }
    });
}

#[test]
fn generic_retry_blocks_preserve_attempt_selection_and_result_data() {
    on_scoped_body_test_stack(|| {
    for generic in [false, true] {
        let annotation = if generic { "" } else { ": Int" };
        let mut source = format!(r#"
error RetryFailure = Transient(message: Str)
var attempts = 0
proc attempt() [error] -> Result[Int, RetryFailure] {{
  attempts += 1
  if attempts < 2 {{ return Err(RetryFailure.Transient(message: "retry")) }}
  Ok(9)
}}
proc keep(payload{annotation}) [time, error] {{
  let result = retry [0ms] on (RetryFailure) {{ attempt()? }}
  let data = retry [] {{ Ok(payload) }}
  {{payload, result, data}}
}}
assert keep(7) == Ok({{payload: 7, result: Ok(9), data: Ok(7)}})
assert attempts == 2
"#);
        if generic { source.push_str("attempts = 0\nassert keep(\"row\") == Ok({payload: \"row\", result: Ok(9), data: Ok(\"row\")})\nassert attempts == 2\n"); }
        let (program, sources, source_id) = record_form_program(&source);
        for force_frames in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            for step in 0..program.driver_step_count().unwrap() {
                evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap_or_else(|error| panic!("generic={generic} frames={force_frames}: {error:?}"));
            }
        }
    }
    });
}

#[test]
fn generic_error_context_blocks_decorate_propagation_and_preserve_error_data() {
    on_scoped_body_test_stack(|| {
    for generic in [false, true] {
        let annotation = if generic { "" } else { ": Int" };
        let mut source = format!(r#"
error ContextFailure = Failed(message: Str)
pure fail() -> Result[Int, ContextFailure] {{ Err(ContextFailure.Failed(message: "original")) }}
proc keep(payload{annotation}) [error] {{
  let failed = try {{ ctx "outer" {{ ctx "inner" {{ fail()? }} }} }}
  let data = ctx "stored" {{ fail() }}
  {{payload, failed, data}}
}}
let actual = keep(7)?
assert actual.payload == 7
if let Err(failure) = actual.failed {{ assert failure.message == "original (ctx: inner) (ctx: outer)" }} else {{ assert false }}
if let Err(failure) = actual.data {{ assert failure.message == "original" }} else {{ assert false }}
"#);
        if generic { source.push_str("let other = keep(\"row\")?\nassert other.payload == \"row\"\n"); }
        let (program, sources, source_id) = record_form_program(&source);
        for force_frames in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            for step in 0..program.driver_step_count().unwrap() {
                evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap_or_else(|error| panic!("generic={generic} frames={force_frames}: {error:?}"));
            }
        }
    }
    });
}

#[test]
fn generic_context_scopes_restore_state_and_preserve_nested_results() {
    on_scoped_body_test_stack(|| {
    for generic in [false, true] {
        let annotation = if generic { "" } else { ": Int" };
        let mut source = format!(r#"
proc keep(payload{annotation}) [env, fs, time, error] {{
  let before = env.get("XSH_GENERIC_CONTEXT_PROBE")
  let inside = env ({{XSH_GENERIC_CONTEXT_PROBE: "inside"}}) {{ env.get("XSH_GENERIC_CONTEXT_PROBE") }}?
  let after = env.get("XSH_GENERIC_CONTEXT_PROBE")
  let original = fs.cwd()?
  let cwd = cd (p".") {{ fs.cwd()? }}?
  let restored = fs.cwd()?
  let nested = within 1h {{ Ok(9) }}?
  {{payload, before, inside, after, original, cwd, restored, nested}}
}}
let actual = keep(7)?
assert actual.payload == 7
assert actual.inside == Ok("inside")
assert actual.before == actual.after
assert actual.original == actual.cwd
assert actual.original == actual.restored
assert actual.nested == Ok(9)
"#);
        if generic { source.push_str("let other = keep(\"row\")?\nassert other.payload == \"row\"\nassert other.nested == Ok(9)\n"); }
        let (program, sources, source_id) = record_form_program(&source);
        for force_frames in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            for step in 0..program.driver_step_count().unwrap() {
                evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap_or_else(|error| panic!("generic={generic} frames={force_frames}: {error:?}"));
            }
        }
    }
    });
}

#[test]
fn generic_native_effects_preserve_proc_and_stream_contracts() {
    on_scoped_body_test_stack(|| {
        let source = r#"
proc read(payload) [fs] { {payload, cwd: fs.cwd()} }
stream rows(payload) [fs] { yield {payload, cwd: fs.cwd()} }
let cwd = fs.cwd()?
assert read(7) == Ok({payload: 7, cwd: Ok(cwd)})
assert read("row") == Ok({payload: "row", cwd: Ok(cwd)})
var count = 0
for row in rows(7) { assert row.payload == 7; assert row.cwd == Ok(cwd); count += 1 }
for row in rows("row") { assert row.payload == "row"; assert row.cwd == Ok(cwd); count += 1 }
assert count == 2
"#;
        let (program, sources, source_id) = record_form_program(source);
        for force_frames in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            for step in 0..program.driver_step_count().unwrap() {
                evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap_or_else(|error| panic!("frames={force_frames}: {error:?}"));
            }
        }
        for definition in ["pure wrong(payload) { {payload, cwd: fs.cwd()} }", "proc wrong(payload) [] { {payload, cwd: fs.cwd()} }"] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), definition);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, definition);
            assert!(checked.diagnostics.iter().any(|diagnostic| matches!(diagnostic.code, Some(crate::diagnostic::DiagnosticCode::CheckPureEffect | crate::diagnostic::DiagnosticCode::CheckEffectViolation))), "{:?}", checked.diagnostics);
        }
    });
}

#[test]
fn generic_context_scopes_preserve_symbolic_outputs_and_runtime_escape_guards() {
    on_scoped_body_test_stack(|| {
        let source = r#"
stream rows() [] -> Stream[Int] { yield 7 }
proc scoped(payload) [time] { within 1h { payload } }
proc number() [time] -> Result[Int] { scoped(7) }
proc text() [time] -> Result[Str] { scoped("row") }
proc escape() [time] -> Result[Stream[Int]] { scoped(rows()) }
"#;
        let (program, sources, source_id) = record_form_program(source);
        for force_frames in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let span = Span::at(source_id, 0);
            for (name, expected) in [("number", Value::Int(7)), ("text", Value::Str(Arc::from("row")))] {
                let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
                assert_eq!(evaluator.call_indexed_direct(key, CallableKind::Proc, &[], span).unwrap().unwrap(), Value::Result(ResultValue::Ok(Box::new(expected))), "{name} frames={force_frames}");
            }
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("escape")));
            let error = evaluator.call_indexed_direct(key, CallableKind::Proc, &[], span).unwrap().unwrap_err();
            assert_eq!(error.kind, "context-scope-escape", "frames={force_frames}");
            assert_eq!(&source[error.span.unwrap().range()], "scoped(rows())");
            assert!(evaluator.within_deadlines.is_empty());
        }
        let source = "stream rows() [] -> Stream[Int] { yield 7 }\nproc known() [time] -> Result[Stream[Int]] { within 1h { rows() } }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckContextScopeEscape)), "{:?}", checked.diagnostics);
    });
}

#[test]
fn generic_dedicated_path_calls_preserve_trusted_path_values() {
    on_scoped_body_test_stack(|| {
        for generic in [false, true] {
            let annotation = if generic { "" } else { ": Int" };
            let mut source = format!(r#"
pure keep(payload{annotation}, text: Str) {{ {{payload, path: Path(text)}} }}
let actual = keep(7, "a/../b")
assert actual.payload == 7
assert actual.path == p"a/../b"
"#);
            if generic { source.push_str("let other = keep(\"row\", \"./second\")\nassert other.payload == \"row\"\nassert other.path == p\"./second\"\n"); }
            let (program, sources, source_id) = record_form_program(&source);
            for force_frames in [false, true] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
                evaluator.force_frames = force_frames;
                evaluator.indexed_program = Some(Arc::clone(&program));
                for step in 0..program.driver_step_count().unwrap() {
                    evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap_or_else(|error| panic!("generic={generic} frames={force_frames}: {error:?}"));
                }
            }
        }
    });
}

fn dedicated_environment_body_case(operation: &str, expected: &str) {
    for generic in [false, true] {
        let annotation = if generic { "" } else { ": Int" };
        let mut checks = String::from("let actual = keep(7, \"XSH_GENERIC_BUILTIN_PROBE\")?\nassert actual.payload == 7\n");
        checks.push_str(&format!("assert actual.value == {expected}\n"));
        if generic { checks.push_str(&format!("let other = keep(\"row\", \"XSH_GENERIC_BUILTIN_PROBE\")?\nassert other.payload == \"row\"\nassert other.value == {expected}\n")); }
        if operation == "env(name)" { checks.push_str("assert keep(7, \"\")?.value is Err(_)\n"); }
        let source = format!(r#"
proc keep(payload{annotation}, name: Str) [env] {{ let _ = name; {{payload, value: {operation}}} }}
env ({{XSH_GENERIC_BUILTIN_PROBE: "present", PATH: "/xsh/first:/xsh/second"}}) {{
{checks}
}}?
"#);
        let (program, sources, source_id) = record_form_program(&source);
        for force_frames in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            for step in 0..program.driver_step_count().unwrap() {
                evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap_or_else(|error| panic!("generic={generic} frames={force_frames}: {error:?}"));
            }
        }
    }
}

#[test]
fn generic_dedicated_env_calls_preserve_result_data() {
    on_scoped_body_test_stack(|| dedicated_environment_body_case("env(name)", "Ok(\"present\")"));
}

#[test]
fn generic_dedicated_env_literals_preserve_result_data() {
    on_scoped_body_test_stack(|| dedicated_environment_body_case("e\"XSH_GENERIC_BUILTIN_PROBE\"", "Ok(\"present\")"));
}

#[test]
fn generic_dedicated_env_path_views_preserve_entry_membership() {
    on_scoped_body_test_stack(|| dedicated_environment_body_case("p\"/xsh/first\" in env.PATH", "true"));
}

#[test]
fn generic_authored_conversions_preserve_parse_and_bound_failures() {
    on_scoped_body_test_stack(|| {
        for generic in [false, true] {
            let annotation = if generic { "" } else { ": Int" };
            let mut source = format!(r#"
type Small = Int range 1..=9
proc keep(payload{annotation}, text: Str) [error] {{ let number = text as Int; {{payload, number: number as Small}} }}
let actual = keep(7, "7")?
assert actual.payload == 7
assert actual.number == 7
assert keep(7, "wrong") is Err(_)
assert keep(7, "-1") is Err(_)
"#);
            if generic { source.push_str("let other = keep(\"row\", \"4\")?\nassert other.payload == \"row\"\nassert other.number == 4\n"); }
            let (program, sources, source_id) = record_form_program(&source);
            for force_frames in [false, true] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
                evaluator.force_frames = force_frames;
                evaluator.indexed_program = Some(Arc::clone(&program));
                for step in 0..program.driver_step_count().unwrap() {
                    evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap_or_else(|error| panic!("generic={generic} frames={force_frames}: {error:?}"));
                }
            }
        }
    });
}

#[test]
fn generic_authored_requirements_preserve_success_and_error_data() {
    on_scoped_body_test_stack(|| {
        for generic in [false, true] {
            let annotation = if generic { "" } else { ": Int" };
            let mut source = format!(r#"
pure keep(payload{annotation}, value: Any) {{ let _ = payload; value.require(Int) }}
assert keep(7, 7) == Ok(7)
assert keep(7, "wrong") is Err(_)
"#);
            if generic { source.push_str("assert keep(\"row\", 4) == Ok(4)\nassert keep(\"row\", false) is Err(_)\n"); }
            let (program, sources, source_id) = record_form_program(&source);
            for force_frames in [false, true] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
                evaluator.force_frames = force_frames;
                evaluator.indexed_program = Some(Arc::clone(&program));
                for step in 0..program.driver_step_count().unwrap() {
                    evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap_or_else(|error| panic!("generic={generic} frames={force_frames}: {error:?}"));
                }
            }
        }
    });
}

#[test]
fn generic_dedicated_environment_effects_follow_actual_owner_kind() {
    on_scoped_body_test_stack(|| {
        let source = r#"
stream rows(payload, name: Str) [env] { let _ = env(name); yield payload }
env ({XSH_GENERIC_BUILTIN_PROBE:"present"}) {
  assert (rows(7, "XSH_GENERIC_BUILTIN_PROBE") |> collect()) == [7]
  assert (rows("row", "XSH_GENERIC_BUILTIN_PROBE") |> collect()) == ["row"]
}?
"#;
        let (program, sources, source_id) = record_form_program(source);
        for force_frames in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            for step in 0..program.driver_step_count().unwrap() {
                evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap_or_else(|error| panic!("frames={force_frames}: {error:?}"));
            }
        }
        for definition in [
            "pure keep(payload, name: Str) { let _ = env(name); payload }",
            "proc keep(payload, name: Str) [] { let _ = env(name); payload }",
            "stream rows(payload, name: Str) [] { let _ = env(name); yield payload }",
        ] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), definition);
            assert!(parsed.diagnostics.is_empty(), "{definition}\n{:?}", parsed.diagnostics);
            let checked = Checker::check_arena_with_options(&parsed.arena, definition, crate::sema::check::CheckOptions::for_execution());
            assert!(checked.diagnostics.iter().any(|diagnostic| matches!(diagnostic.code, Some(crate::diagnostic::DiagnosticCode::CheckPureEffect | crate::diagnostic::DiagnosticCode::CheckEffectViolation))), "{definition}\n{:?}", checked.diagnostics);
        }
    });
}

#[test]
fn ordinary_environment_path_views_preserve_snapshots_and_active_mutations() {
    on_scoped_body_test_stack(|| {
        let source = r#"
proc snapshot() [env] { {paths: env.PATH} }
env ({PATH: "/xsh/before"}) {
  let held = env.PATH
  let returned = snapshot()?.paths
  env ({PATH: "/xsh/after"}) {
    assert p"/xsh/before" in held
    assert p"/xsh/before" in returned
    assert p"/xsh/after" not in held
    assert p"/xsh/after" in env.PATH
    held.append(p"/xsh/added")?
    assert p"/xsh/added" in env.PATH
    assert p"/xsh/added" not in held
  }?
  assert p"/xsh/before" in env.PATH
  assert p"/xsh/after" not in env.PATH
}?
"#;
        let (program, sources, source_id) = record_form_program(source);
        for force_frames in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            for step in 0..program.driver_step_count().unwrap() {
                evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap_or_else(|error| panic!("frames={force_frames}: {error:?}"));
            }
        }
    });
}

#[test]
fn generic_environment_path_values_preserve_their_checked_view_domain() {
    on_scoped_body_test_stack(|| {
        let source = r#"
pure relay(value) { value }
assert relay(7) == 7
assert relay("row") == "row"
env ({PATH: "/xsh/before"}) {
  let held = relay(env.PATH)
  assert p"/xsh/before" in held
  assert p"/xsh/before" in relay(env.PATH)
}?
"#;
        let (program, sources, source_id) = record_form_program(source);
        for force_frames in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            for step in 0..program.driver_step_count().unwrap() {
                evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap_or_else(|error| panic!("frames={force_frames}: {error:?}"));
            }
        }
    });
}

#[test]
fn generic_membership_resolves_actual_container_and_needle_contracts() {
    let source = r#"
pure relay(value) { value }
assert "/xsh/before" in relay([p"/xsh/before"])
assert "/xsh/missing" not in relay([p"/xsh/before"])
assert relay(p"/xsh/before") in relay([p"/xsh/before"])
env ({PATH: "/xsh/before"}) {
  assert "/xsh/before" in relay(env.PATH)
  assert relay(p"/xsh/before") in relay(env.PATH)
}?
"#;
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    for expression in [
        "let wrong = \"/xsh/before\"; let _ = wrong in relay([p\"/xsh/before\"])",
        "let _ = true in relay([1])",
        "let _ = p\"/xsh/before\" in relay(7)",
    ] {
        let source = format!("pure relay(value) {{ value }}\n{expression}\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.iter().any(|diagnostic| matches!(diagnostic.code, Some(crate::diagnostic::DiagnosticCode::CheckTypeMismatch | crate::diagnostic::DiagnosticCode::CheckMembershipType))), "{source}\n{:?}", checked.diagnostics);
    }
}

#[test]
fn environment_path_any_boundaries_preserve_material_list_observations() {
    on_scoped_body_test_stack(|| {
        let source = r#"
type PathEnvelope = {paths: List[Path]}
proc inspect(value: Any) [error] {
  assert [value is EnvPathList, value is List[Path]] == [false, true]
  assert value.require(EnvPathList) is Err(_)
  let paths = value.require(List[Path])?
  assert paths.len() == 2
  assert paths[0] == p"/xsh/first"
  var count = 0
  for entry in paths { assert entry in [p"/xsh/first", p"/xsh/second"]; count += 1 }
  assert count == 2
  if value is List[Path] {
    assert value.len() == 2
    assert value[1] == p"/xsh/second"
    var narrowed_count = 0
    for entry in value { assert entry in paths; narrowed_count += 1 }
    assert narrowed_count == 2
  } else { assert false }
  match value { selected is List[Path] => { assert selected.len() == 2; assert selected[0] == paths[0] }, _ => assert false }
  assert value == paths
  assert paths == value
  let envelope: Any = {paths: value}
  let checked = envelope.require(PathEnvelope)?
  let nested: Any = [value]
  let nested_paths = nested.require(List[List[Path]])?
  assert nested_paths[0] == paths
  let ordinary_record: Any = {items:[7]}
  assert ordinary_record.require(Map[Str, List[Int]]) is Err(_)
  assert checked.paths.len() == 2
  assert checked.paths[0] == paths[0]
  match envelope { selected is PathEnvelope => { assert selected.paths.len() == 2; assert selected.paths[1] == paths[1] }, _ => assert false }
  match json.encode(value) {
    Err(error) => assert error.message == "Path is not JSON-compatible; convert Path, Bytes, Status, Result, and errors explicitly"
    Ok(_) => assert false
  }
}
env ({PATH:"/xsh/first:/xsh/second"}) { inspect(env.PATH)?; inspect([p"/xsh/first", p"/xsh/second"])? }?
"#;
        let (program, sources, source_id) = record_form_program(source);
        for force_frames in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            for step in 0..program.driver_step_count().unwrap() {
                evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap_or_else(|error| panic!("frames={force_frames}: {error:?}"));
            }
        }
    });
}

#[test]
fn environment_path_any_list_patterns_preserve_material_list_observations() {
    on_scoped_body_test_stack(|| {
        let source = r#"
pure pair(value: Any) -> Bool {
  match value { [_, _] => true, _ => false }
}
env ({PATH:"/xsh/first:/xsh/second"}) {
  assert pair([p"/xsh/first", p"/xsh/second"])
  assert pair(env.PATH)
  assert !pair([p"/xsh/first"])
}?
let snapshot = env ({PATH:"/xsh/first:/xsh/second"}) { env.PATH }?
"#;
        let (program, sources, source_id) = record_form_program(source);
        for force_frames in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            for step in 0..program.driver_step_count().unwrap() {
                evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap_or_else(|error| panic!("frames={force_frames}: {error:?}"));
            }
            let (snapshot, target) = program.symbol_owner().with_current(|| {
                (evaluator.lookup(Name::intern("snapshot")).unwrap().value.clone(), LoweredFunctionKey::Name(Name::intern("pair")))
            });
            assert!(matches!(&snapshot, Value::EnvPathList(_)), "actual native snapshot: {snapshot:?}");
            assert_eq!(evaluator.call_indexed_direct(target, CallableKind::Pure, &[snapshot], Span::at(source_id, 0)).unwrap().unwrap(), Value::Bool(true));
        }
    });
}

#[test]
fn environment_path_validated_list_consumers_preserve_material_list_observations() {
    on_scoped_body_test_stack(|| {
        let source = r#"
pure spliced(value: List[Path]) -> List[Path] { [@value] }
pure left_added(value: List[Path]) -> List[Path] { let empty: List[Path] = []; value + empty }
pure right_added(value: List[Path]) -> List[Path] { let empty: List[Path] = []; empty + value }
pure extended(value: List[Path]) -> List[Path] { let empty: List[Path] = []; empty.extend(value) }
pure flattened(value: List[Path]) -> List[Path] { [1] |> flat-map { |_| value } |> collect() }
pure set_items(value: List[Path]) -> List[Path] { set.from(value).to_list() }
let snapshot = env ({PATH:"/xsh/first:/xsh/second"}) { env.PATH }?
let ordinary = [p"/xsh/first", p"/xsh/second"]
assert spliced(ordinary) == ordinary
assert left_added(ordinary) == ordinary
assert right_added(ordinary) == ordinary
assert extended(ordinary) == ordinary
assert flattened(ordinary) == ordinary
assert set_items(ordinary) == ordinary
"#;
        let (program, sources, source_id) = record_form_program(source);
        for force_frames in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            for step in 0..program.driver_step_count().unwrap() {
                evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap_or_else(|error| panic!("frames={force_frames}: {error:?}"));
            }
            let (snapshot, ordinary) = program.symbol_owner().with_current(|| {
                (evaluator.lookup(Name::intern("snapshot")).unwrap().value.clone(), evaluator.lookup(Name::intern("ordinary")).unwrap().value.clone())
            });
            assert!(matches!(&snapshot, Value::EnvPathList(_)), "actual native snapshot: {snapshot:?}");
            let mut failures = Vec::new();
            for name in ["spliced", "left_added", "right_added", "extended", "flattened", "set_items"] {
                let target = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
                match evaluator.call_indexed_direct(target, CallableKind::Pure, std::slice::from_ref(&snapshot), Span::at(source_id, 0)).unwrap() {
                    Ok(actual) if runtime_values_semantically_equal(&actual, &ordinary) => {}
                    actual => failures.push(format!("{name}: {actual:?}")),
                }
            }
            assert!(failures.is_empty(), "frames={force_frames}: {failures:?}");
        }
    });
}

#[test]
fn dynamic_equality_preserves_any_and_optional_null_domains() {
    on_scoped_body_test_stack(|| {
        let source = r#"
pure equal(left: Any, right: Any) -> Bool { left == right }
pure unequal(left: Any, right: Any) -> Bool { left != right }
pure nullable(value: Int?) -> List[Bool] {
  let absent = null
  [value == absent, absent == value, value != absent, absent != value]
}
assert !equal(7, "seven")
assert !equal("seven", 7)
assert unequal(7, "seven")
assert unequal("seven", 7)
assert equal(7, 7)
assert nullable(null) == [true, true, false, false]
assert nullable(7) == [false, false, true, true]
"#;
        let (program, sources, source_id) = record_form_program(source);
        for force_frames in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            for step in 0..program.driver_step_count().unwrap() {
                evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap_or_else(|error| panic!("frames={force_frames}: {error:?}"));
            }
        }
    });
}

#[test]
fn prepared_callable_environment_paths_reject_material_list_forgery() {
    on_scoped_body_test_stack(|| {
        let source = r#"
pure identity(value) { let _ = value; 7 }
pure keep(action, value) { let _ = action(value); action }
pure direct_value(handle: Pure, value: Any) -> Any { handle.call(value) }
proc cached_value(handle: Pure, value: Any) -> Any { utils.cache(handle, [value]) }
let pair = env ({PATH:"/xsh/first:/xsh/second"}) { let view = env.PATH; {view, handle: keep(identity, view)} }?
"#;
        let (program, sources, source_id) = record_form_program(source);
        for force_frames in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            for step in 0..program.driver_step_count().unwrap() {
                evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap();
            }
            let (view, handle) = program.symbol_owner().with_current(|| {
                let Value::Record(pair) = &evaluator.lookup(Name::intern("pair")).unwrap().value else { panic!("actual native view and checked instance remain in their returned record") };
                (pair.get("view").unwrap().clone(), pair.get("handle").unwrap().clone())
            });
            let Value::EnvPathList(paths) = &view else { panic!("native output retains sealed snapshot provenance") };
            assert_eq!(view.type_name(), "List");
            assert_eq!(lowered_value_from_runtime_any(&view).unwrap().into_value(), view);
            let forged = Value::List(paths.entries().iter().cloned().map(Value::Path).collect());
            assert!(runtime_values_semantically_equal(&view, &forged));
            assert!(!value_matches_static_type(ValueView::Runtime(&forged), &Type::EnvPathList));
            for (name, kind) in [("direct_value", CallableKind::Pure), ("cached_value", CallableKind::Proc)] {
                let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
                for _ in 0..2 {
                    assert_eq!(evaluator.call_indexed_direct(key, kind, &[handle.clone(), view.clone()], Span::at(source_id, 0)).unwrap().unwrap(), Value::Int(7));
                }
                let error = evaluator.call_indexed_direct(key, kind, &[handle.clone(), forged.clone()], Span::at(source_id, 0)).unwrap().expect_err("material list cannot forge the captured checked view domain, including after a cache hit");
                assert_eq!(error.kind, "type-error", "{error:?}");
            }
        }
    });
}

fn assert_constant_null_return_representations(definitions: &str) {
    let source = format!(r#"{definitions}
let integer: Null = absent(7)
let text: Null = absent("kept")
let task_integer: Null = null_task(7)?
let task_text: Null = null_task("kept")?
assert integer == null
assert text == null
assert task_integer == null
assert task_text == null
"#);
    let (program, sources, source_id) = record_form_program(&source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        for step in 0..program.driver_step_count().unwrap() {
            evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap();
        }
    }
}

#[test]
fn typed_constant_null_returns_keep_their_physical_representation() {
    assert_constant_null_return_representations("proc null_task(marker: Any) -> Result[Null] { return null }\npure absent(marker: Any) -> Null { return null }");
}

#[test]
fn generic_constant_null_returns_keep_their_physical_representation() {
    assert_constant_null_return_representations("proc null_task(marker) { return null }\npure absent(marker) { return null }");
}

fn assert_adjacent_range_boundaries(annotation: &str) {
    let source = format!(r#"type Span = {{first: Int, last: Int}}
proc ranges(spans: List[Span]) -> List[Span] {{
    var merged{annotation} = []
    assert merged.is_empty(), "initial range storage must be empty"
    for span in spans |> sort-by .first {{
        if ! merged.is_empty() and span.first <= merged[-1].last {{
            let previous = merged[-1]
            merged = [@merged[..merged.len() - 1], {{first: previous.first, last: if span.last > previous.last {{ span.last }} else {{ previous.last }}}}]
        }} else {{ merged += [span] }}
    }}
    merged
}}
assert ranges([{{first: 1, last: 2}}, {{first: 3, last: 4}}]) == [{{first: 1, last: 2}}, {{first: 3, last: 4}}]
assert ranges([{{first: 2, last: 4}}, {{first: 1, last: 3}}]) == [{{first: 1, last: 4}}]
"#);
    let (program, sources, source_id) = record_form_program(&source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        for step in 0..program.driver_step_count().unwrap() {
            evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap_or_else(|error| panic!("frames={force_frames}: {error:?}"));
        }
    }
}

#[test]
fn typed_range_storage_preserves_guarded_last_element_reads() {
    assert_adjacent_range_boundaries(": List[Span]");
}

#[test]
fn inferred_range_storage_preserves_guarded_last_element_reads() {
    assert_adjacent_range_boundaries("");
}

#[test]
fn closed_negative_literal_index_modes_preserve_list_map_and_slice_domains() {
    let source = r#"pure list_tail(values) {
    let tail = values[-1]
    let anchored: List[Int] = values
    tail
}
pure map_values(values, index: Int) {
    let literal = values[-1]
    let computed = values[index]
    let anchored: Map[Int, Str] = values
    {literal, computed}
}
let mapping: Map[Int, Str] = {[-1]: "minus", [0]: "zero"}
assert list_tail([7, 8]) == 8
assert map_values(mapping, -1) == {literal: "minus", computed: "minus"}
assert map_values(mapping, 0) == {literal: "minus", computed: "zero"}
assert [1, 2, 3][-2..] == [2, 3]
assert "aé🦀"[-2..] == "é🦀"
assert b"abcdef"[-2..] == b"ef"
"#;
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        for step in 0..program.driver_step_count().unwrap() {
            evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap();
        }
    }
}

#[test]
fn typed_bare_pipeline_sources_preserve_set_result_and_stream_domains() {
    let source = r#"stream source_values() [] -> Stream[Int] { yield 1; yield 2 }
pure collect_list(values: List[Int]) -> List[Int] { values |> collect }
pure collect_set(values: Set[Int]) -> List[Int] { values |> collect }
pure collect_pure_stream(values: Stream[Int]) -> List[Int] { values |> collect }
proc collect_stream(values: Stream[Int]) [error] -> Result[List[Int]] { values |> collect }
proc collect_result_list(values: Result[List[Int]]) [error] -> Result[List[Int]] { values |> collect }
proc collect_result_stream(values: Result[Stream[Int]]) [error] -> Result[List[Int]] { values |> collect }
assert collect_list([1, 2]) == [1, 2]
assert collect_set({2, 1}) == [1, 2]
assert collect_pure_stream(source_values()) == [1, 2]
assert collect_stream(source_values())? == [1, 2]
assert collect_result_list(Ok([1, 2]))? == [1, 2]
assert collect_result_stream(Ok(source_values()))? == [1, 2]
"#;
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        for step in 0..program.driver_step_count().unwrap() {
            evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap();
        }
    }
}

#[test]
fn typed_pure_pipeline_collector_preserves_observed_stream_failure() {
    let source = r#"error SourceFailure = Late(message: Str)
proc fail() [] -> Result[Unit, SourceFailure] { Err(SourceFailure.Late(message: "late source failure")) }
stream failing_values() [error] -> Stream[Int] { yield 1; fail()? }
pure collect_values(values: Stream[Int]) -> List[Int] { values |> collect }
let result = try { collect_values(failing_values()) }
assert match result { Ok(_) => false, Err(error) => error.message == "late source failure" }
"#;
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        for step in 0..program.driver_step_count().unwrap() {
            evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap();
        }
    }
}

#[test]
fn generic_bare_pipeline_sources_preserve_independent_container_domains() {
    let source = r#"stream source_values() [] -> Stream[Int] { yield 1; yield 2 }
pure collect_values(values) { values |> collect }
pure text_source() -> Result[List[Str], Str] { Ok(["a", "b"]) }
assert collect_values([1, 2]) == [1, 2]
assert collect_values({2, 1}) == [1, 2]
assert collect_values(source_values()) == [1, 2]
assert collect_values(text_source()) == ["a", "b"]
let wrapped_stream: Result[Stream[Int]] = Ok(source_values())
assert collect_values(wrapped_stream) == [1, 2]
"#;
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        for step in 0..program.driver_step_count().unwrap() {
            evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap();
        }
    }
}

#[test]
fn generic_pipeline_result_string_failure_matches_typed_transport() {
    for force_frames in [false, true] {
        let mut errors = Vec::new();
        for annotation in [": Result[List[Int], Str]", ""] {
            let source = format!("pure collect_values(values{annotation}) -> List[Int] {{ values |> collect }}\nlet stopped: Result[List[Int], Str] = Err(\"source failed\")\nlet _ = collect_values(stopped)\n");
            let (program, sources, source_id) = record_form_program(&source);
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let mut failure = None;
            for step in 0..program.driver_step_count().unwrap() {
                match evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap() {
                    Ok(_) => {},
                    Err(error) => { failure = Some(error); break; },
                }
            }
            let error = failure.expect("the Result source must fail before exposing a collected list");
            errors.push((error.family, error.kind, error.message, error.propagated));
        }
        assert_eq!(errors[0], errors[1]);
        assert_eq!((&*errors[0].0, &*errors[0].1, &*errors[0].2, errors[0].3), ("Error", "stream-stage-error", "stream stage failed with Str", false));
    }
}

#[test]
fn generic_pipeline_input_preserves_lazy_pulls_cleanup_and_checked_errors() {
    let source = r#"error SourceFailure = Late(message: Str)
var pulls = 0
var closed = 0
proc close() [] { closed += 1 }
proc fail() [] -> Result[Unit, SourceFailure] { Err(SourceFailure.Late(message: "late source failure")) }
stream source_values() [error] -> Stream[Int] {
  defer close()
  pulls += 1
  yield 1
  pulls += 1
  fail()?
}
pure take_values(values, count: Int) { values |> take(count) |> collect }
assert take_values(source_values(), 0) == []
assert pulls == 0
assert closed == 0
assert take_values(source_values(), 1) == [1]
assert pulls == 1
assert closed == 1
let wrapped: Result[Stream[Int]] = Ok(source_values())
assert take_values(wrapped, 0) == []
assert pulls == 1
assert closed == 1
let wrapped_again: Result[Stream[Int]] = Ok(source_values())
assert take_values(wrapped_again, 1) == [1]
assert pulls == 2
assert closed == 2
let failed = try { take_values(source_values(), 2) }
assert match failed { Ok(_) => false, Err(error) => error.message == "late source failure" }
assert pulls == 4
assert closed == 3
let wrapped_failure: Result[Stream[Int]] = Ok(source_values())
let failed_wrapped = try { take_values(wrapped_failure, 2) }
assert match failed_wrapped { Ok(_) => false, Err(error) => error.message == "late source failure" }
assert pulls == 6
assert closed == 4
"#;
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        for step in 0..program.driver_step_count().unwrap() {
            evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap();
        }
    }
}

#[test]
fn generic_mixed_pipeline_and_for_sources_keep_the_for_error_bound() {
    let source = r#"proc mixed(values) [error] -> Result[List[Int], Str] {
  let _ = values |> take(0) |> collect
  [item for item in values]
}
assert mixed([1, 2])? == [1, 2]
let wrapped: Result[List[Int], Str] = Ok([1, 2])
assert mixed(wrapped)? == [1, 2]
assert mixed({2, 1})? == [1, 2]
"#;
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        for step in 0..program.driver_step_count().unwrap() {
            evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap();
        }
    }
}

#[test]
fn pipeline_result_source_views_preserve_opaque_items_and_explicit_peeling() {
    let source = r#"pure collect_values(values: Result[List[Result[Int, Str]]]) -> List[Result[Int, Str]] { values |> collect }
pure collect_nested(values: Result[Result[List[Int]]]) -> Result[List[Int]] { values? |> collect }
stream source_values() [] -> Stream[Int] { yield 1; yield 2 }
proc collect_nested_stream(values: Result[Result[Stream[Int]]]) [error] -> Result[List[Int]] { values? |> collect }
let values: Result[List[Result[Int, Str]]] = Ok([Err("opaque")])
let observed = collect_values(values)
assert match observed[0] { Ok(_) => false, Err(message) => message == "opaque" }
let nested: Result[Result[List[Int]]] = Ok(Ok([1, 2]))
assert collect_nested(nested)? == [1, 2]
let nested_stream: Result[Result[Stream[Int]]] = Ok(Ok(source_values()))
assert collect_nested_stream(nested_stream)? == [1, 2]
"#;
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        for step in 0..program.driver_step_count().unwrap() {
            evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap();
        }
    }
}

#[test]
fn typed_collect_expression_preserves_passive_any_items() {
    let source = "pure gather(value: Any) -> List[Any] { collect { yield value } }";
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("gather")));
        let actual = evaluator.call_indexed_direct(key, CallableKind::Pure, &[Value::Int(7)], Span::at(source_id, 0)).unwrap().unwrap();
        assert_eq!(actual, Value::List(vec![Value::Int(7)]));
    }
}

#[test]
fn typed_collect_expression_rejects_a_stream_as_one_item() {
    let source = "pure gather(value: Stream[Int]) -> List[Stream[Int]] { collect { yield value } }";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena_with_options(&parsed.arena, source, crate::sema::check::CheckOptions::for_execution());
    assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckYieldStream)), "{:?}", checked.diagnostics);
}

#[test]
fn generic_collect_expression_preserves_item_correlation_and_result_data() {
    let source = r#"pure gather(value) { collect { yield value; yield value } }
assert gather(7) == [7, 7]
assert gather("kept") == ["kept", "kept"]
let data: Result[Int, Str] = Err("opaque")
let rows = gather(data)
assert match rows[0] { Ok(_) => false, Err(message) => message == "opaque" }
"#;
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        for step in 0..program.driver_step_count().unwrap() {
            evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap();
        }
    }
}

#[test]
fn generic_collect_delegation_preserves_list_item_relationships() {
    let source = r#"pure gather(values) { collect { yield @values } }
assert gather([1, 2]) == [1, 2]
assert gather(["kept"]) == ["kept"]
"#;
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        for step in 0..program.driver_step_count().unwrap() {
            evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap();
        }
    }
}

#[test]
fn generic_collect_expression_preserves_explicit_any_arguments() {
    let source = "pure gather(value) { collect { yield value } }\npure erased(value: Any) -> List[Any] { gather(value) }\n";
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("erased")));
        let actual = evaluator.call_indexed_direct(key, CallableKind::Pure, &[Value::Int(7)], Span::at(source_id, 0)).unwrap().unwrap();
        assert_eq!(actual, Value::List(vec![Value::Int(7)]));
    }
}

#[test]
fn generic_collect_expression_rejects_actual_stream_arguments() {
    let source = "pure gather(value) { collect { yield value } }\nstream values() [] -> Stream[Int] { yield 1 }\nlet _ = gather(values())\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena_with_options(&parsed.arena, source, crate::sema::check::CheckOptions::for_execution());
    assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckYieldStream)), "{:?}", checked.diagnostics);
}

#[test]
fn generic_collect_expression_keeps_nested_yield_targets_and_control_flow() {
    let source = r#"pure nested(value) { collect { yield value; yield @collect { yield value }; yield value } }
pure stopped(value, stop: Bool) { collect { yield value; if stop { return [value] }; yield value } }
pure looped(value) {
  var bound = 0
  for round in [1, 2, 3] {
    let rows = collect { yield value; break when round == 2 }
    bound += rows.len()
  }
  {value, bound}
}
assert nested(7) == [7, 7, 7]
assert nested("kept") == ["kept", "kept", "kept"]
assert stopped(7, true) == [7]
assert stopped("kept", false) == ["kept", "kept"]
assert looped(7).bound == 1
assert looped("kept").value == "kept"
pure gather(value) { collect { yield value } }
let data: Result[Result[Int, Str], Str] = Ok(Err("nested"))
let rows = gather(data)
match rows[0] {
  Ok(inner) => assert match inner { Ok(_) => false, Err(message) => message == "nested" }
  Err(_) => assert false
}
"#;
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        for step in 0..program.driver_step_count().unwrap() {
            evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap();
        }
    }
}

#[test]
fn generic_collect_expression_preserves_yield_error_and_cleanup_order() {
    let source = r#"error CollectFailure = Stopped(message: Str)
var events = ""
proc close() [] { events += "c" }
proc fail() [] -> Result[Unit, CollectFailure] { Err(CollectFailure.Stopped(message: "stop")) }
proc gather(value, stop: Bool) [error] {
  collect {
    defer close()
    yield { events += "y"; value }
    if stop { fail()? }
    yield { events += "y"; value }
  }
}
assert gather(7, false)? == [7, 7]
assert events == "yyc"
let failed = try { gather("kept", true)? }
assert match failed { Ok(_) => false, Err(error) => error.message == "stop" }
assert events == "yycyc"
"#;
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        for step in 0..program.driver_step_count().unwrap() {
            evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap();
        }
    }
}

#[test]
fn generic_collect_unsigned_items_match_typed_validation_timing() {
    let source = r#"var events = ""
proc clear() [] { events = "" }
proc observed() [] -> Str { events }
proc typed(number: Int) [error] -> List[UInt] {
  let result: List[UInt] = collect { defer { events += "d" }; yield { events += "y"; number }; events += "a" }
  result
}
proc generic(marker, number: Int) [error] -> List[UInt] {
  let _ = marker
  let result: List[UInt] = collect { defer { events += "d" }; yield { events += "y"; number }; events += "a" }
  result
}
"#;
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        for step in 0..program.driver_step_count().unwrap() {
            evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap();
        }
        let key = |name| program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
        let span = Span::at(source_id, 0);
        for number in [7, -1] {
            evaluator.call_indexed_direct(key("clear"), CallableKind::Proc, &[], span).unwrap().unwrap();
            let expected = evaluator.call_indexed_direct(key("typed"), CallableKind::Proc, &[Value::Int(number)], span).unwrap();
            let expected_events = evaluator.call_indexed_direct(key("observed"), CallableKind::Proc, &[], span).unwrap().unwrap();
            evaluator.call_indexed_direct(key("clear"), CallableKind::Proc, &[], span).unwrap().unwrap();
            let actual = evaluator.call_indexed_direct(key("generic"), CallableKind::Proc, &[Value::Str(Arc::from("marker")), Value::Int(number)], span).unwrap();
            let actual_events = evaluator.call_indexed_direct(key("observed"), CallableKind::Proc, &[], span).unwrap().unwrap();
            assert_eq!(actual_events, expected_events, "{number} frames={force_frames}");
            match (expected, actual) {
                (Ok(expected), Ok(actual)) => assert_eq!(actual, expected),
                (Err(expected), Err(actual)) => { assert_eq!(actual.kind, expected.kind); assert_eq!(actual.message, expected.message); assert_eq!(actual.family, expected.family); }
                (expected, actual) => panic!("{number} frames={force_frames}: expected {expected:?}, found {actual:?}"),
            }
        }
    }
}

#[test]
fn generic_collect_unsigned_storage_proves_uninvoked_owners_without_captures() {
    let (program, sources, source_id) = record_form_program("pure gather(marker, number: Int) -> List[UInt] { let _ = marker; let rows: List[UInt] = collect { yield number }; rows }");
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("gather")));
        let span = Span::at(source_id, 0);
        let actual = evaluator.call_indexed_direct(key, CallableKind::Pure, &[Value::Bool(true), Value::Int(7)], span).unwrap().unwrap();
        assert_eq!(actual, Value::List(vec![Value::Int(7)]));
        let error = evaluator.call_indexed_direct(key, CallableKind::Pure, &[Value::Str(Arc::from("independent")), Value::Int(-1)], span).unwrap().unwrap_err();
        assert_eq!(error.kind, "type-error");
        assert!(error.message.contains("UInt constraint"), "{error:?}");
    }
}

#[test]
fn generic_collect_delegation_rejects_non_list_sources_and_callback_yield_escape() {
    for argument_type in ["Stream[Int]", "Map[Str, Int]", "Set[Int]", "Str", "Bytes", "Any", "Result[List[Int]]"] {
        let source = format!("pure gather(values) {{ collect {{ yield @values }} }}\npure rejected(value: {argument_type}) {{ gather(value) }}\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, &source, crate::sema::check::CheckOptions::for_execution());
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error), "{source}");
    }
    let source = "pure gather(value) { collect { let _ = [value] |> map { |item| yield item; item }; yield value } }";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena_with_options(&parsed.arena, source, crate::sema::check::CheckOptions::for_execution());
    assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckYield)), "{:?}", checked.diagnostics);
}

#[test]
fn ground_resource_contracts_preserve_bounded_producer_verification() {
    let declarations = [
        "var argument_order = \"\"",
        "proc selected_path(location: Path) [] -> Path { argument_order += \"p\"; location }",
        "proc selected_order() [] -> Bool { argument_order += \"o\"; true }",
        "proc observed_order() [] -> Str { argument_order }",
        "proc closed(root: FsRoot) [fs] -> Result[Unit] { root.close() }",
        "proc typed(root: FsRoot, location: Path) [fs, error] { root.children(ordered: selected_order(), path: selected_path(location))? }",
        "proc listed(marker: Any, root: FsRoot, location: Path) [fs, error] { let _ = marker; root.children(ordered: selected_order(), path: selected_path(location))? }",
        "proc written(marker: Any, root: FsRoot, location: Path, text: Str) [fs, error] { let _ = marker; root.write(location, text)? }",
        "proc read(marker: Any, root: FsRoot, location: Path) [fs, error] { let _ = marker; root.read_text(location)? }",
    ];
    let cases = [
        ("children", vec![0, 1, 2, 5]),
        ("children-marker", vec![0, 1, 2, 6]),
        ("children-pair", vec![0, 1, 2, 5, 6]),
        ("pair-write", vec![0, 1, 2, 5, 6, 7]),
        ("pair-read", vec![0, 1, 2, 5, 6, 8]),
        ("pair-write-read", vec![0, 1, 2, 5, 6, 7, 8]),
        ("full", (0..declarations.len()).collect()),
    ];
    let mut failures = Vec::new();
    for (name, selected) in cases {
        let source = selected.into_iter().map(|index| declarations[index]).collect::<Vec<_>>().join("\n");
        let mut sources = SourceMap::default();
        let source_id = sources.add_file("ground-resource-contracts.xsh", source.clone());
        let parsed = Parser::parse_source_arena_only(source_id, &source);
        assert!(parsed.diagnostics.is_empty(), "{name}: {:?}", parsed.diagnostics);
        let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        assert!(checked.diagnostics.is_empty(), "{name}: {:?}", checked.diagnostics);
        assert!(checked.function_schemes.is_empty(), "{name}: ordinary ground functions");
        match FullBuilder::build_compact(&parsed.arena, &checked, &source, Arc::new(sources), source_id) {
            Ok(_) => {},
            Err(error) => failures.push(format!("{name}: {error:?}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn typed_fsroot_methods_preserve_selected_named_arguments() {
    record_form_program("proc listed(root: FsRoot) [fs, error] { root.children(p\".\", max_entries: 256)? }");
}

#[test]
fn generic_fsroot_methods_preserve_selected_named_arguments() {
    record_form_program("proc listed(marker, root: FsRoot) [fs, error] { let _ = marker; root.children(p\".\", max_entries: 256)? }");
}

#[test]
fn generic_fsroot_methods_preserve_resource_calls_and_argument_order_on_both_routes() {
    let directory = tempfile::Builder::new().prefix("generic-fsroot-").tempdir_in(".work/tmp").unwrap();
    let source = r#"var argument_order = ""
proc selected_path(location: Path) [] -> Path { argument_order += "p"; location }
proc selected_order() [] -> Bool { argument_order += "o"; true }
proc observed_order() [] -> Str { argument_order }
proc closed(root: FsRoot) [fs] -> Result[Unit] { root.close() }
proc typed(root: FsRoot, location: Path) [fs, error] { root.children(ordered: selected_order(), path: selected_path(location))? }
proc listed(marker, root: FsRoot, location: Path) [fs, error] { let _ = marker; root.children(ordered: selected_order(), path: selected_path(location))? }
proc written(marker, root: FsRoot, location: Path, text: Str) [fs, error] { let _ = marker; root.write(location, text)? }
proc read(marker, root: FsRoot, location: Path) [fs, error] { let _ = marker; root.read_text(location)? }
"#;
    let typed = source.replace("marker,", "marker: Any,");
    let path = |text: &str| Value::Path(crate::runtime::value::PathValue::from_text(text).unwrap());
    let success = |value| match value { Value::Result(ResultValue::Ok(value)) => *value, value => panic!("expected success, found {value:?}") };
    for source in [typed.as_str(), source] {
        let (program, sources, source_id) = record_form_program(source);
        for force_frames in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let span = Span::at(source_id, 0);
            for step in 0..program.driver_step_count().unwrap() {
                evaluator.eval_indexed_driver_step(step, span).unwrap().unwrap();
            }
            let key = |name| program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
            let rooted = crate::modules::fs::open_root(directory.path().to_path_buf(), span).unwrap();
            let id = evaluator.push_fs_capability(crate::runtime::eval::FsCapability::Root(Box::new(crate::runtime::eval::FsRootHandle::Dir(rooted))));
            let root = Value::FsRoot(crate::runtime::value::FsRootValue { id, owner: Arc::clone(&evaluator.resource_owner) });
            for (marker, name, text) in [(Value::Int(7), "a", "first"), (Value::Str(Arc::from("independent")), "b", "second")] {
                let output = evaluator.call_indexed_direct(key("written"), CallableKind::Proc, &[marker, root.clone(), path(name), Value::Str(Arc::from(text))], span).unwrap().unwrap();
                assert_eq!(success(output), Value::Unit);
            }
            let expected = success(evaluator.call_indexed_direct(key("typed"), CallableKind::Proc, &[root.clone(), path(".")], span).unwrap().unwrap());
            let actual = success(evaluator.call_indexed_direct(key("listed"), CallableKind::Proc, &[Value::Bool(true), root.clone(), path(".")], span).unwrap().unwrap());
            let (Value::Record(actual), Value::Record(expected)) = (actual, expected) else { panic!("children retains its registered record result") };
            assert_eq!(actual.get("children"), expected.get("children"));
            let Some(Value::List(children)) = actual.get("children") else { panic!("children is a list of paths") };
            assert_eq!(children.len(), 2);
            let order = evaluator.call_indexed_direct(key("observed_order"), CallableKind::Proc, &[], span).unwrap().unwrap();
            assert_eq!(order, Value::Str(Arc::from("opop")));
            for (marker, name, text) in [(Value::Int(7), "a", "first"), (Value::Str(Arc::from("independent")), "b", "second")] {
                let output = evaluator.call_indexed_direct(key("read"), CallableKind::Proc, &[marker, root.clone(), path(name)], span).unwrap().unwrap();
                assert_eq!(success(output), Value::Str(Arc::from(text)));
            }
            let missing = evaluator.call_indexed_direct(key("read"), CallableKind::Proc, &[Value::Bool(true), root.clone(), path("missing")], span).unwrap().unwrap();
            let Value::Result(ResultValue::Err(error)) = missing else { panic!("missing rooted input must retain Err") };
            assert!(error.error_message().is_some_and(|message| !message.is_empty()));
            assert_eq!(success(evaluator.call_indexed_direct(key("closed"), CallableKind::Proc, std::slice::from_ref(&root), span).unwrap().unwrap()), Value::Unit);
            let closed = evaluator.call_indexed_direct(key("read"), CallableKind::Proc, &[Value::Bool(true), root, path("a")], span).unwrap().unwrap();
            assert!(matches!(closed, Value::Result(ResultValue::Err(_))));
        }
    }
}

#[test]
fn generic_fsroot_methods_reject_wrong_receivers_arguments_and_effects() {
    for source in [
        "proc listed(marker, root: Str) [fs, error] { let _ = marker; root.children(p\".\")? }",
        "proc listed(marker, root: FsRoot) [] { let _ = marker; root.children(p\".\")? }",
        "pure listed(marker, root: FsRoot) { let _ = marker; root.children(p\".\") }",
        "proc listed(marker, root: FsRoot) [fs, error] { let _ = marker; root.children(p\".\", max_entries: true)? }",
    ] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source, crate::sema::check::CheckOptions::for_execution());
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error), "{source}");
    }
}

#[test]
fn generic_fsroot_native_fixture_executes_after_frontend_disposal_on_both_routes() {
    on_scoped_body_test_stack(|| {
    let source = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/runtime/generic-fsroot-calls.xsh")).unwrap();
    let (program, sources, source_id) = record_form_program(&source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        for step in 0..program.driver_step_count().unwrap() {
            evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap_or_else(|error| panic!("frames={force_frames}: {error:?}"));
        }
    }
    });
}

#[test]
fn concrete_stream_alias_result_preserves_creation_pull_failure_and_cleanup() {
    let source = r#"error AliasFailure = Stopped(message: Str)
var pulls = 0
var closed = 0
pure failure() -> Result[Unit, AliasFailure] { Err(AliasFailure.Stopped("late")) }
stream values(value: Int) [error] -> Stream[Int] { defer { closed += 1 }; pulls += 1; yield value; let _ = failure()? }
proc make() [error] -> Result[Stream[Int]] { let create = values; Ok(create(42)) }
proc observed_pulls() [] -> Int { pulls }
proc observed_closed() [] -> Int { closed }
proc take_one(items: Stream[Int]) [error] -> Result[Int] { var found = 0; for item in items { found = item; break }; Ok(found) }
proc drain(items: Stream[Int]) [error] -> Result[List[Int]] { items.collect() }
"#;
    for alias in [false, true] {
      let source = source.replace("Ok(create(42))", if alias { "Ok(create(42))" } else { "Ok(values(42))" });
      let (program, sources, source_id) = record_form_program(&source);
      for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        let span = Span::at(source_id, 0);
        for step in 0..program.driver_step_count().unwrap() { evaluator.eval_indexed_driver_step(step, span).unwrap().unwrap(); }
        let key = |name| program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
        let success = |value| match value { Value::Result(ResultValue::Ok(value)) => *value, value => panic!("expected success, found {value:?}") };
        let first = success(evaluator.call_indexed_direct(key("make"), CallableKind::Proc, &[], span).unwrap().unwrap());
        assert!(matches!(&first, Value::Stream(_)));
        for name in ["observed_pulls", "observed_closed"] { assert_eq!(evaluator.call_indexed_direct(key(name), CallableKind::Proc, &[], span).unwrap().unwrap(), Value::Int(0)); }
        let first = evaluator.call_indexed_direct(key("take_one"), CallableKind::Proc, &[first], span).unwrap().unwrap();
        assert_eq!(success(first), Value::Int(42));
        for name in ["observed_pulls", "observed_closed"] { assert_eq!(evaluator.call_indexed_direct(key(name), CallableKind::Proc, &[], span).unwrap().unwrap(), Value::Int(1)); }
        let later = success(evaluator.call_indexed_direct(key("make"), CallableKind::Proc, &[], span).unwrap().unwrap());
        for name in ["observed_pulls", "observed_closed"] { assert_eq!(evaluator.call_indexed_direct(key(name), CallableKind::Proc, &[], span).unwrap().unwrap(), Value::Int(1)); }
        let error = evaluator.call_indexed_direct(key("drain"), CallableKind::Proc, &[later], span).unwrap().expect_err("pulling past the first item retains the producer failure");
        assert_eq!((&*error.family, &*error.variant, &*error.message), ("AliasFailure", "Stopped", "late"));
        assert!(error.propagated);
        assert_eq!(&source[error.span.unwrap().range()], "failure()?");
        for name in ["observed_pulls", "observed_closed"] { assert_eq!(evaluator.call_indexed_direct(key(name), CallableKind::Proc, &[], span).unwrap().unwrap(), Value::Int(2)); }
      }
    }
    for call in ["values(42)", "create(42)"] {
        let source = format!("stream values(value: Int) [error] -> Stream[Int] {{ yield value }}\nproc make() [] -> Result[Stream[Int]] {{ let create = values; Ok({call}) }}\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, &source, crate::sema::check::CheckOptions::for_execution());
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckEffectViolation)
            && diagnostic.labels.iter().any(|label| &source[label.span.range()] == call)), "{source}\n{:?}", checked.diagnostics);
    }

}

fn pipeline_adapter_program_source() -> &'static str {
    r#"pure lines(marker, input: Str) -> List[Str] { let _ = marker; input |> text.lines() }
pure chunks(marker, input: Bytes, size: Int) -> List[Bytes] { let _ = marker; input |> bytes.chunks(size) }
pure json_lines(marker, input: Str) -> List[Any] { let _ = marker; input |> json.lines() }
pure json_stream(marker, input: Str) -> List[Any] { let _ = marker; input |> json.stream() }
pure ignored_json(marker, input: Str) -> List[Any] { let _ = marker; input |> json.lines() |> take(0) }
"#
}

fn verify_pipeline_adapter_results(source: &str, markers: &[Value]) {
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        let span = Span::at(source_id, 0);
        let key = |name| program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
        for marker in markers {
            let lines = evaluator.call_indexed_direct(key("lines"), CallableKind::Pure, &[marker.clone(), Value::Str(Arc::from("one\r\n\ntwo\n"))], span).unwrap().unwrap();
            assert_eq!(lines, Value::List(vec![Value::Str(Arc::from("one")), Value::Str(Arc::from("")), Value::Str(Arc::from("two"))]));
            let bytes = |text: &[u8]| Value::Bytes(text.to_vec());
            let chunks = evaluator.call_indexed_direct(key("chunks"), CallableKind::Pure, &[marker.clone(), bytes(b"abcde"), Value::Int(2)], span).unwrap().unwrap();
            assert_eq!(chunks, Value::List(vec![bytes(b"ab"), bytes(b"cd"), bytes(b"e")]));
            for name in ["json_lines", "json_stream"] {
                let values = evaluator.call_indexed_direct(key(name), CallableKind::Pure, &[marker.clone(), Value::Str(Arc::from("1\ntrue\n\"word\"\n"))], span).unwrap().unwrap();
                assert_eq!(values, Value::List(vec![Value::Int(1), Value::Bool(true), Value::Str(Arc::from("word"))]));
            }
            let invalid = evaluator.call_indexed_direct(key("ignored_json"), CallableKind::Pure, &[marker.clone(), Value::Str(Arc::from("1\ninvalid"))], span).unwrap().expect_err("JSON adapters validate the complete source before take(0)");
            assert_eq!(invalid.kind, "json-lines");
            assert!(!invalid.message.is_empty());
            let invalid = evaluator.call_indexed_direct(key("chunks"), CallableKind::Pure, &[marker.clone(), bytes(b"abc"), Value::Int(0)], span).unwrap().expect_err("a dynamic chunk size must be positive");
            assert_eq!(invalid.kind, "bytes-chunks");
            assert_eq!(invalid.message, "chunk size must be positive");
        }
    }
}

#[test]
fn typed_pipeline_adapters_preserve_buffer_and_validation_semantics() {
    verify_pipeline_adapter_results(&pipeline_adapter_program_source().replace("marker,", "marker: Int,"), &[Value::Int(7)]);
}

#[test]
fn generic_pipeline_adapters_preserve_buffer_and_validation_semantics() {
    verify_pipeline_adapter_results(pipeline_adapter_program_source(), &[Value::Int(7), Value::Str(Arc::from("independent"))]);
}

#[test]
fn inferred_pipeline_adapter_sources_preserve_their_canonical_buffer_types() {
    let source = pipeline_adapter_program_source().replace("input: Str", "input").replace("input: Bytes", "input");
    verify_pipeline_adapter_results(&source, &[Value::Int(7), Value::Str(Arc::from("independent"))]);
}

#[test]
fn generic_pipeline_adapter_fixture_executes_after_frontend_disposal() {
    on_scoped_body_test_stack(|| {
    let source = include_str!("../../../../tests/fixtures/runtime/generic-pipeline-adapters.xsh");
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        for step in 0..program.driver_step_count().unwrap() {
            let flow = evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap_or_else(|error| panic!("frames={force_frames}: {error:?}"));
            assert!(matches!(flow, None | Some(crate::runtime::eval::Flow::Continue(_))), "frames={force_frames}: {flow:?}");
        }
    }
    });
}

#[test]
fn generic_pipeline_adapters_reject_wrong_sources_and_configuration() {
    for source in [
        "pure convert(marker, value: Bytes) { let _ = marker; value |> text.lines() }",
        "pure convert(marker, value: Str) { let _ = marker; value |> bytes.chunks(2) }",
        "pure convert(marker, value: Any) { let _ = marker; value |> json.lines() }",
        "pure convert(marker, value: Str) { let _ = marker; value |> json.stream(2) }",
        "pure convert(marker, value: Bytes) { let _ = marker; value |> bytes.chunks(true) }",
        "pure convert(marker, value: Bytes) { let _ = marker; value |> bytes.chunks(0) }",
        "pure convert(marker) { [marker] |> map { \"text\" } |> text.lines() }",
    ] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source, crate::sema::check::CheckOptions::for_execution());
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error), "{source}\n{:?}", checked.diagnostics);
    }
}

#[test]
fn generic_json_adapter_returns_require_authored_opaque_boundaries() {
    for adapter in ["json.lines()", "json.stream()"] {
        let source = format!("pure parsed(marker, input: Str) {{ let _ = marker; input |> {adapter} }}");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, &source, crate::sema::check::CheckOptions::for_execution());
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckInferReturn)), "{source}\n{:?}", checked.diagnostics);
    }
}

#[test]
fn generic_pipeline_adapters_preserve_source_configuration_and_cleanup_order() {
    let source = r#"var events = ""
proc bytes_input(value: Bytes) [] -> Bytes { defer { events += "d" }; events += "s"; value }
proc text_input(value: Str) [] -> Str { defer { events += "d" }; events += "s"; value }
proc chunk_size(value: Int) [] -> Int { events += "c"; value }
proc chunked(marker, input: Bytes, size: Int) [] -> List[Bytes] { let _ = marker; bytes_input(input) |> bytes.chunks(chunk_size(size)) }
proc parsed(marker, input: Str) [] -> List[Any] { let _ = marker; text_input(input) |> json.stream() |> take(0) }
proc observed() [] -> Str { events }
"#;
    for source in [source.to_owned(), source.replace("marker,", "marker: Int,")] {
        let (program, sources, source_id) = record_form_program(&source);
        for force_frames in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let span = Span::at(source_id, 0);
            for step in 0..program.driver_step_count().unwrap() {
                let flow = evaluator.eval_indexed_driver_step(step, span).unwrap().unwrap();
                assert!(matches!(flow, None | Some(crate::runtime::eval::Flow::Continue(_))), "frames={force_frames}: {flow:?}");
            }
            let key = |name| program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
            let output = evaluator.call_indexed_direct(key("chunked"), CallableKind::Proc, &[Value::Int(7), Value::Bytes(b"abc".to_vec()), Value::Int(2)], span).unwrap().unwrap();
            assert_eq!(output, Value::List(vec![Value::Bytes(b"ab".to_vec()), Value::Bytes(b"c".to_vec())]));
            assert_eq!(evaluator.call_indexed_direct(key("observed"), CallableKind::Proc, &[], span).unwrap().unwrap(), Value::Str(Arc::from("sdc")));
            let output = evaluator.call_indexed_direct(key("chunked"), CallableKind::Proc, &[Value::Int(7), Value::Bytes(b"abc".to_vec()), Value::Int(0)], span).unwrap();
            assert!(output.is_err());
            assert_eq!(evaluator.call_indexed_direct(key("observed"), CallableKind::Proc, &[], span).unwrap().unwrap(), Value::Str(Arc::from("sdcsdc")));
            let error = evaluator.call_indexed_direct(key("parsed"), CallableKind::Proc, &[Value::Int(7), Value::Str(Arc::from("1\ninvalid"))], span).unwrap().expect_err("the adapter parses before take(0)");
            assert_eq!(&*error.kind, "json-lines");
            assert!(!error.message.is_empty());
            assert_eq!(evaluator.call_indexed_direct(key("observed"), CallableKind::Proc, &[], span).unwrap().unwrap(), Value::Str(Arc::from("sdcsdcsd")));
        }
    }
}

#[cfg(feature = "native-tests")]
#[test]
fn generic_optional_guard_original_slots_execute_after_frontend_disposal() {
    let source = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/runtime/generic-guard-original-slot.xsh")).unwrap();
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("generic-guard-original-slot.xsh", crate::loader::entry_source_from_text("generic-guard-original-slot.xsh", source), Vec::new());
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let source_id = parsed.arena.source_text_source_id().unwrap();
    let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let prepared = Evaluator::new_with_sources(Vec::new(), sources).prepare_test_program(Arc::new(parsed.arena), source_id, checked).unwrap();
    for force_frames in [false, true] {
        let result = prepared.eval_test("verify_guard_original_slot", Value::Unit, false, force_frames, Vec::new(), crate::runtime::eval::TestCancellation::default());
        assert_eq!(result.output.status, 0, "force_frames={force_frames}: {:?} {:?}", result.output.diagnostics, result.output.traceback);
        assert!(result.output.diagnostics.is_empty(), "{:?}", result.output.diagnostics);
        assert!(result.output.traceback.is_none(), "{:?}", result.output.traceback);
        assert!(matches!(&result.result, Some(Value::Result(ResultValue::Ok(value))) if matches!(value.as_ref(), Value::Unit)), "force_frames={force_frames}: {:?}", result.result);
    }
}

fn pipeline_table_program_source() -> &'static str {
    "type Row = {name: Str, size: Int, opaque: Any}\nproc render(marker, row, columns: List[Str]) [] -> Unit { let _ = marker; [row] |> table.print(columns: columns) }\nproc defaults(marker, row) [] -> Unit { let _ = marker; [row] |> table.print() }\n"
}

fn verify_pipeline_table_output(source: &str, markers: &[Value]) {
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        let key = |name| program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
        let row = program.symbol_owner().with_current(|| Value::Record([(Arc::from("name"), Value::Str(Arc::from("alpha"))), (Arc::from("size"), Value::Int(2)), (Arc::from("opaque"), Value::List(vec![Value::Int(9)]))].into_iter().collect()));
        for marker in markers {
            evaluator.stdout.clear();
            let output = evaluator.call_indexed_direct(key("render"), CallableKind::Proc, &[marker.clone(), row.clone(), Value::List(vec![Value::Str(Arc::from("name"))])], Span::at(source_id, 0)).unwrap().unwrap();
            assert_eq!(output, Value::Unit);
            assert_eq!(String::from_utf8(evaluator.stdout.clone()).unwrap(), "┌───────┐\n│ name  │\n├───────┤\n│ alpha │\n└───────┘\n");
            evaluator.stdout.clear();
            let output = evaluator.call_indexed_direct(key("render"), CallableKind::Proc, &[marker.clone(), row.clone(), Value::List(vec![Value::Str(Arc::from("absent"))])], Span::at(source_id, 0)).unwrap().unwrap();
            assert_eq!(output, Value::Unit);
            assert_eq!(String::from_utf8(evaluator.stdout.clone()).unwrap(), "┌────────┐\n│ absent │\n├────────┤\n│        │\n└────────┘\n");
            evaluator.stdout.clear();
            let output = evaluator.call_indexed_direct(key("defaults"), CallableKind::Proc, &[marker.clone(), row.clone()], Span::at(source_id, 0)).unwrap().unwrap();
            assert_eq!(output, Value::Unit);
            let output = String::from_utf8(evaluator.stdout.clone()).unwrap();
            for cell in ["name", "alpha", "size", "2", "opaque", "<List>"] { assert!(output.contains(cell), "{cell}: {output}"); }
        }
    }
}

#[test]
fn typed_pipeline_table_preserves_record_output_and_opaque_fields() {
    let source = pipeline_table_program_source().replace("marker,", "marker: Int,").replace("row,", "row: Row,").replace("row)", "row: Row)");
    verify_pipeline_table_output(&source, &[Value::Int(7)]);
}

#[test]
fn generic_pipeline_table_preserves_record_output_and_opaque_fields() {
    let source = pipeline_table_program_source().replace("row,", "row: Row,").replace("row)", "row: Row)");
    verify_pipeline_table_output(&source, &[Value::Int(7), Value::Str(Arc::from("independent"))]);
}

#[test]
fn inferred_pipeline_table_requires_only_the_record_head() {
    let source = pipeline_table_program_source().replace("columns: List[Str]", "columns");
    verify_pipeline_table_output(&source, &[Value::Int(7), Value::Str(Arc::from("independent"))]);
}

#[test]
fn inferred_pipeline_table_source_retains_the_record_iteration_relationship() {
    let source = "proc render(rows, columns: List[Str]) [] -> Unit { rows |> table.print(columns: columns) }\n";
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("render")));
        let row = program.symbol_owner().with_current(|| Value::Record([(Arc::from("name"), Value::Str(Arc::from("alpha")))].into_iter().collect()));
        let output = evaluator.call_indexed_direct(key, CallableKind::Proc, &[Value::List(vec![row]), Value::List(vec![Value::Str(Arc::from("name"))])], Span::at(source_id, 0)).unwrap().unwrap();
        assert_eq!(output, Value::Unit);
        assert_eq!(String::from_utf8(evaluator.stdout.clone()).unwrap(), "┌───────┐\n│ name  │\n├───────┤\n│ alpha │\n└───────┘\n");
    }
}

#[test]
fn generic_pipeline_table_preserves_configuration_pull_cleanup_and_failure_order() {
    let source = r#"type Row = {name: Str}
error TableFailure = Stopped(message: Str)
var events = ""
pure failure() -> Result[Unit, TableFailure] { Err(TableFailure.Stopped("late")) }
stream rows(failed: Bool) [error] -> Stream[Row] {
  defer { events += "d"; print "closed" }
  events += "p"
  yield {name: "row"}
  if failed { let _ = failure()? }
}
proc columns() [] -> List[Str] { events += "c"; ["name"] }
pure bad_columns() -> Result[List[Str], TableFailure] { Err(TableFailure.Stopped("configuration")) }
proc render(marker, failed: Bool) [error] -> Unit { let _ = marker; rows(failed) |> table.print(columns: columns()) }
proc rejected(marker) [error] -> Unit { let _ = marker; rows(false) |> table.print(columns: bad_columns()?) }
proc observed() [] -> Str { events }
"#;
    for source in [source.to_owned(), source.replace("marker,", "marker: Int,").replace("marker)", "marker: Int)")] {
        let (program, sources, source_id) = record_form_program(&source);
        for force_frames in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let span = Span::at(source_id, 0);
            for step in 0..program.driver_step_count().unwrap() {
                let flow = evaluator.eval_indexed_driver_step(step, span).unwrap().unwrap();
                assert!(matches!(flow, None | Some(crate::runtime::eval::Flow::Continue(_))), "{flow:?}");
            }
            let key = |name| program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
            assert_eq!(evaluator.call_indexed_direct(key("render"), CallableKind::Proc, &[Value::Int(7), Value::Bool(false)], span).unwrap().unwrap(), Value::Unit);
            assert_eq!(String::from_utf8(evaluator.stdout.clone()).unwrap(), "closed\n┌──────┐\n│ name │\n├──────┤\n│ row  │\n└──────┘\n");
            assert_eq!(evaluator.call_indexed_direct(key("observed"), CallableKind::Proc, &[], span).unwrap().unwrap(), Value::Str(Arc::from("cpd")));
            evaluator.stdout.clear();
            let error = evaluator.call_indexed_direct(key("render"), CallableKind::Proc, &[Value::Int(7), Value::Bool(true)], span).unwrap().expect_err("failed materialization does not render a partial table");
            assert_eq!((&*error.family, &*error.variant, &*error.message), ("TableFailure", "Stopped", "late"));
            assert!(error.propagated);
            assert_eq!(evaluator.stdout, b"closed\n");
            assert_eq!(evaluator.call_indexed_direct(key("observed"), CallableKind::Proc, &[], span).unwrap().unwrap(), Value::Str(Arc::from("cpdcpd")));
            evaluator.stdout.clear();
            let error = evaluator.call_indexed_direct(key("rejected"), CallableKind::Proc, &[Value::Int(7)], span).unwrap().expect_err("configuration fails before the source is pulled");
            assert_eq!((&*error.family, &*error.variant, &*error.message), ("TableFailure", "Stopped", "configuration"));
            assert!(error.propagated);
            assert!(evaluator.stdout.is_empty());
            assert_eq!(evaluator.call_indexed_direct(key("observed"), CallableKind::Proc, &[], span).unwrap().unwrap(), Value::Str(Arc::from("cpdcpd")));
        }
    }
}

#[test]
fn generic_pipeline_table_rejects_scalar_erased_and_invalid_column_inputs() {
    for source in [
        "proc show(row) [] -> Unit { [row] |> table.print() }\nshow(7)",
        "proc show(row) [] -> Unit { [row] |> table.print() }\nlet erased: Any = {name: \"row\"}\nshow(erased)",
        "proc show(row: Any) [] -> Unit { [row] |> table.print() }",
        "proc show(row: Int) [] -> Unit { [row] |> table.print() }",
        "proc show(row) [] -> Unit { [row] |> table.print(columns: [1]) }",
    ] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source, crate::sema::check::CheckOptions::for_execution());
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error), "{source}\n{:?}", checked.diagnostics);
    }
    let source = "proc show(row) [] -> Unit { [row] |> table.print() { \"extra\" } }";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::ParseExpectedTerminator)), "{:?}", parsed.diagnostics);
}

#[test]
fn generic_pipeline_table_retains_its_actual_stream_effects() {
    for annotation in [": Int", ""] {
        let source = format!("type Row = {{name: Str}}\nstream rows() [error] -> Stream[Row] {{ yield {{name: \"row\"}} }}\nproc show(marker{annotation}) [] -> Unit {{ let _ = marker; rows() |> table.print() }}\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, &source, crate::sema::check::CheckOptions::for_execution());
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckEffectViolation)), "{source}\n{:?}", checked.diagnostics);
    }
}

#[test]
fn generic_pipeline_table_fixture_executes_after_frontend_disposal() {
    on_scoped_body_test_stack(|| {
        let source = include_str!("../../../../tests/fixtures/runtime/generic-pipeline-table.xsh");
        let (program, sources, source_id) = record_form_program(source);
        let mut outputs = Vec::new();
        for force_frames in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            for step in 0..program.driver_step_count().unwrap() {
                let flow = evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap().unwrap();
                assert!(matches!(flow, None | Some(crate::runtime::eval::Flow::Continue(_))), "frames={force_frames}: {flow:?}");
            }
            let output = String::from_utf8(evaluator.stdout.clone()).unwrap();
            for name in ["alpha", "beta", "gamma", "delta", "stream"] { assert!(output.contains(name), "{output}"); }
            assert!(output.contains("closed\n┌"), "{output}");
            assert!(output.ends_with("table stages preserved\n"));
            outputs.push(output);
        }
        assert_eq!(outputs[0], outputs[1]);
    });
}

fn verify_line_loop_argument_program(typed: bool) {
    let source = include_str!("../../../../tests/fixtures/runtime/generic-line-loop-arguments.xsh");
    let source = if typed { source.replace("pure line_identity(value)", "pure line_identity(value: Str) -> Str") } else { source.to_owned() };
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("generic-line-loop-arguments.xsh", crate::loader::entry_source_from_text("generic-line-loop-arguments.xsh", source), Vec::new());
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let source_id = parsed.arena.source_text_source_id().unwrap();
    let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let prepared = Evaluator::new_with_sources(Vec::new(), sources).prepare_test_program(Arc::new(parsed.arena), source_id, checked).unwrap();
    for force_frames in [false, true] {
        let result = prepared.eval_test("verify_line_loop_arguments", Value::Unit, false, force_frames, Vec::new(), crate::runtime::eval::TestCancellation::default());
        assert_eq!(result.output.status, 0, "force_frames={force_frames}: {:?} {:?}", result.output.diagnostics, result.output.traceback);
        assert!(result.output.diagnostics.is_empty(), "{:?}", result.output.diagnostics);
        assert!(result.output.traceback.is_none(), "{:?}", result.output.traceback);
        assert!(matches!(&result.result, Some(Value::Result(ResultValue::Ok(value))) if matches!(value.as_ref(), Value::Unit)), "force_frames={force_frames}: {:?}", result.result);
    }
}

#[test]
fn typed_line_loop_arguments_preserve_source_items_after_frontend_disposal() {
    verify_line_loop_argument_program(true);
}

#[test]
fn inferred_line_loop_arguments_preserve_source_items_after_frontend_disposal() {
    verify_line_loop_argument_program(false);
}

#[test]
fn generic_membership_container_domains_execute_on_both_routes() {
    on_scoped_body_test_stack(|| {
        let source = r#"
pure has(item, container) { item in container }
pure lacks(item, container) { item not in container }
pure forward(item, container) { has(item, container) }
enum MembershipFirst: Str { MembershipBuild = "build", MembershipRun = "run" }
enum MembershipSecond: Str { MembershipTask = "task" }
assert has(MembershipBuild, [MembershipBuild]) and lacks(MembershipRun, [MembershipBuild])
assert forward(MembershipTask, [MembershipTask])
let payload: Result[Int, Str] = Err("data")
assert has(payload, [payload])
let unsigned: Map[UInt, Int] = {[1]: 7}
let signed: Map[Int, Str] = {[-1]: "negative"}
let list_expected = 7 in [7, 8]
assert has(7, [7, 8]) == list_expected
assert has("word", ["word"]) and lacks("absent", ["word"])
let set_expected = 2 in {1, 2}
assert has(2, {1, 2}) == set_expected
assert has("key", {["key"]: null})
assert has(-1, signed)
assert has(1, unsigned) and lacks(0, unsigned)
assert has("key", {key: null})
assert has("key", "a key") and lacks("missing", "a key")
assert has(b"bc", b"abcd") and lacks(b"ce", b"abcd")
assert has("bin", p"/xsh/bin") and has(p"/xsh", p"/xsh/bin")
assert forward(7, [7]) and forward("word", "word")
env ({PATH: "/xsh/bin:/xsh/other"}) {
  assert has(p"/xsh/bin", env.PATH)
  assert lacks(p"/xsh/missing", env.PATH)
}?
"#;
        let (program, sources, source_id) = record_form_program(source);
        for force_frames in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            for step in 0..program.driver_step_count().unwrap() {
                let flow = evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap()
                    .unwrap_or_else(|error| panic!("frames={force_frames}: {error:?}"));
                assert!(matches!(flow, None | Some(crate::runtime::eval::Flow::Continue(_))), "frames={force_frames} step={step}: {flow:?}");
            }
        }
    });
}

#[test]
fn generic_membership_checks_computed_and_mutated_unsigned_keys_on_both_routes() {
    on_scoped_body_test_stack(|| {
        let source = r#"
pure computed(container) { 1 - 2 in container }
pure mutated(container) { var key = 1; key -= 2; key in container }
pure ordinary(container: Map[UInt, Int]) -> Bool { var key = 1; key -= 2; key in container }
pure captured(container) { try { var key = 1; key -= 2; key in container } }
pure relay(container) { mutated(container) }
pure caught_relay(container) { try { relay(container) } }
pure unsigned_relay(container: Map[UInt, Int]) -> Bool { relay(container) }
pure unsigned_caught_relay(container: Map[UInt, Int]) -> Result[Bool] { caught_relay(container) }
pure unsigned_computed(container: Map[UInt, Int]) -> Bool { computed(container) }
pure unsigned_mutated(container: Map[UInt, Int]) -> Bool { mutated(container) }
pure unsigned_captured(container: Map[UInt, Int]) -> Result[Bool] { captured(container) }
pure signed_mutated(container: Map[Int, Int]) -> Bool { mutated(container) }
"#;
        let (program, sources, source_id) = record_form_program(source);
        for force_frames in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let arguments = [Value::Map(Default::default())];
            for name in ["ordinary", "unsigned_computed", "unsigned_mutated", "unsigned_relay"] {
                let function = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
                let error = evaluator.call_indexed_direct(function, CallableKind::Pure, &arguments, Span::at(source_id, 0)).unwrap().expect_err(name);
                assert_eq!(error.kind, "schema", "frames={force_frames} {name}: {error:?}");
                assert_eq!(error.message, "schema check failed: expected UInt, found Int");
                let callee = match name { "ordinary" => "ordinary", "unsigned_computed" => "computed", _ => "mutated" };
                let definition = source.find(&format!("pure {callee}(" )).unwrap();
                let needle = if callee == "computed" { "1 - 2" } else { "key in container" };
                let start = definition + source[definition..].find(needle).unwrap();
                let length = if callee == "computed" { 5 } else { 3 };
                assert_eq!(error.span, Some(Span::new(source_id, start, start + length)), "frames={force_frames} {name}");
            }
            let captured = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("unsigned_captured")));
            let result = evaluator.call_indexed_direct(captured, CallableKind::Pure, &arguments, Span::at(source_id, 0)).unwrap().unwrap();
            assert!(matches!(result, Value::Result(ResultValue::Err(_))), "frames={force_frames}: {result:?}");
            let forwarded = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("unsigned_caught_relay")));
            let result = evaluator.call_indexed_direct(forwarded, CallableKind::Pure, &arguments, Span::at(source_id, 0)).unwrap().unwrap();
            let Value::Result(ResultValue::Err(error)) = result else { panic!("forwarding retains the captured UInt failure") };
            let Value::Error(error) = *error else { panic!("UInt validation retains ordinary Error data") };
            assert_eq!(error.kind, "schema");
            assert_eq!(error.message, "schema check failed: expected UInt, found Int");
            let signed = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("signed_mutated")));
            assert_eq!(evaluator.call_indexed_direct(signed, CallableKind::Pure, &arguments, Span::at(source_id, 0)).unwrap().unwrap(), Value::Bool(false));
        }
    });
}

#[test]
fn generic_membership_preserves_conditional_errors_and_evaluation_order() {
    on_scoped_body_test_stack(|| {
        let source = r#"
proc selected(container, key: Int) { key in container }
pure ordered(container) {
  var visits = 0
  let result = try {
    let key = { visits += 1; -1 }
    key in { visits += 10; container }
  }
  {visits, result}
}
let signed: Map[Int, Int] = {[0]: 3}
let unsigned: Map[UInt, Int] = {[0]: 7}
assert selected(signed, 0) == Ok(true)
assert selected(unsigned, 0) == Ok(true)
match selected(unsigned, -1) {
  Ok(_) => { assert false },
  Err(error) => { assert error.message == "schema check failed: expected UInt, found Int" },
}
let caught_unsigned = ordered(unsigned)
assert caught_unsigned.visits == 1
match caught_unsigned.result {
  Ok(_) => { assert false },
  Err(error) => { assert error.message == "schema check failed: expected UInt, found Int" },
}
let caught_signed = ordered(signed)
assert caught_signed.visits == 11
assert caught_signed.result == Ok(false)
"#;
        let (program, sources, source_id) = record_form_program(source);
        for force_frames in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            for step in 0..program.driver_step_count().unwrap() {
                let flow = evaluator.eval_indexed_driver_step(step, Span::at(source_id, 0)).unwrap()
                    .unwrap_or_else(|error| panic!("frames={force_frames}: {error:?}"));
                assert!(matches!(flow, None | Some(crate::runtime::eval::Flow::Continue(_))), "frames={force_frames} step={step}: {flow:?}");
            }
        }
    });
}

fn verify_record_field_guard_program(typed: bool) {
    let source = include_str!("../../../../tests/fixtures/runtime/generic-record-field-guards.xsh");
    let source = if typed {
        source.replace("pure record_guard(marker,", "pure record_guard(marker: Any,")
            .replace("pure local_record_guard(marker,", "pure local_record_guard(marker: Any,")
            .replace("pure nested_record_guard(marker,", "pure nested_record_guard(marker: Any,")
            .replace("pure record_argument_guard(marker,", "pure record_argument_guard(marker: Any,")
            .replace("pure written_record_guard(marker,", "pure written_record_guard(marker: Any,")
            .replace("pure stale_record_guard(marker,", "pure stale_record_guard(marker: Any,")
            .replace("pure stale_block_record_guard(marker,", "pure stale_block_record_guard(marker: Any,")
    } else { source.to_owned() };
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("generic-record-field-guards.xsh", crate::loader::entry_source_from_text("generic-record-field-guards.xsh", source), Vec::new());
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let source_id = parsed.arena.source_text_source_id().unwrap();
    let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let prepared = Evaluator::new_with_sources(Vec::new(), sources).prepare_test_program(Arc::new(parsed.arena), source_id, checked).unwrap();
    for force_frames in [false, true] {
        let result = prepared.eval_test("verify_record_field_guards", Value::Unit, false, force_frames, Vec::new(), crate::runtime::eval::TestCancellation::default());
        assert_eq!(result.output.status, 0, "force_frames={force_frames}: {:?} {:?}", result.output.diagnostics, result.output.traceback);
        assert!(result.output.diagnostics.is_empty(), "{:?}", result.output.diagnostics);
        assert!(result.output.traceback.is_none(), "{:?}", result.output.traceback);
        assert!(matches!(&result.result, Some(Value::Result(ResultValue::Ok(value))) if matches!(value.as_ref(), Value::Unit)), "force_frames={force_frames}: {:?}", result.result);
    }
}

#[test]
fn typed_record_field_guards_preserve_actual_closed_records_on_both_routes() {
    verify_record_field_guard_program(true);
}

#[test]
fn generic_record_field_guards_preserve_actual_closed_records_on_both_routes() {
    verify_record_field_guard_program(false);
}

#[test]
fn generic_closed_status_fields_preserve_their_actual_receiver() {
    let source = "type StatusFields = {ok: Bool, success: Bool, kind: Str}\npure ordinary(status: Status) -> StatusFields { let _ = status.segments; {ok: status.ok, success: status.success, kind: status.kind} }\npure view(marker, status: Status) { let _ = marker; let _ = status.segments; {ok: status.ok, success: status.success, kind: status.kind} }\npure number(status: Status) -> StatusFields { view(7, status) }\npure text(status: Status) -> StatusFields { view(\"word\", status) }\n";
    let (program, sources, source_id) = record_form_program(source);
    for force_frames in [false, true] {
        for code in [0, 7] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let status = Value::Status(crate::runtime::process::ProcessStatus::exited(code));
            let ordinary = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("ordinary")));
            let expected = evaluator.call_indexed_direct(ordinary, CallableKind::Pure, std::slice::from_ref(&status), Span::at(source_id, 0)).unwrap().unwrap();
            for name in ["number", "text"] {
                let function = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
                let actual = evaluator.call_indexed_direct(function, CallableKind::Pure, std::slice::from_ref(&status), Span::at(source_id, 0)).unwrap().unwrap();
                assert_eq!(actual, expected, "{name} code={code} frames={force_frames}");
            }
        }
    }
}

#[test]
fn generic_proc_bare_returns_run_native_unit_assertions_on_both_routes() {
    on_scoped_body_test_stack(|| {
        let source = include_str!("../../../../tests/xsh/generic-proc-bare-returns.xsh");
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("generic-proc-bare-returns.xsh", crate::loader::entry_source_from_text("generic-proc-bare-returns.xsh", source.to_owned()), Vec::new());
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = parsed.arena.source_text_source_id().unwrap();
        let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let prepared = Evaluator::new_with_sources(Vec::new(), sources).prepare_test_program(Arc::new(parsed.arena), source_id, checked).unwrap();
        for force_frames in [false, true] {
            let result = prepared.eval_test("generic_proc_bare_returns_preserve_unit", Value::Unit, false, force_frames, Vec::new(), crate::runtime::eval::TestCancellation::default());
            assert_eq!(result.output.status, 0, "frames={force_frames}: {:?} {:?}", result.output.diagnostics, result.output.traceback);
            assert!(result.output.diagnostics.is_empty(), "{:?}", result.output.diagnostics);
            assert!(result.output.traceback.is_none(), "{:?}", result.output.traceback);
            assert!(matches!(&result.result, Some(Value::Result(ResultValue::Ok(value))) if matches!(value.as_ref(), Value::Unit)), "frames={force_frames}: {:?}", result.result);
        }
    });
}

#[test]
fn generic_proc_bare_returns_preserve_cleanup_and_inferred_io() {
    on_scoped_body_test_stack(|| {
        for typed in [true, false] {
            let marker = if typed { ": Int" } else { "" };
            let effects = if typed { " [io]" } else { "" };
            let mut source = format!(r#"
proc early(marker{marker}, done: Bool){effects} {{
  defer {{ print "clean" }}
  return when done
  print "work"
  let _ = marker
}}
proc terminal(marker{marker}){effects} {{
  defer {{ print "clean" }}
  let _ = marker
  return
}}
proc number(done: Bool) [io] {{ early(7, done) }}
proc end_number() [io] {{ terminal(7) }}
"#);
            if !typed { source.push_str("proc text(done: Bool) [io] { early(\"word\", done) }\nproc end_text() [io] { terminal(\"word\") }\n"); }
            let (program, sources, source_id) = record_form_program(&source);
            for force_frames in [false, true] {
                for done in [true, false] {
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
                    evaluator.force_frames = force_frames;
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let names = if typed { &["number"][..] } else { &["number", "text"][..] };
                    for &name in names {
                        evaluator.stdout.clear();
                        let function = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
                        assert_eq!(evaluator.call_indexed_direct(function, CallableKind::Proc, &[Value::Bool(done)], Span::at(source_id, 0)).unwrap().unwrap(), Value::ok(Value::Unit));
                        assert_eq!(evaluator.stdout, if done { b"clean\n".as_slice() } else { b"work\nclean\n".as_slice() }, "typed={typed} {name} done={done} frames={force_frames}");
                    }
                    let names = if typed { &["end_number"][..] } else { &["end_number", "end_text"][..] };
                    for &name in names {
                        evaluator.stdout.clear();
                        let function = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
                        assert_eq!(evaluator.call_indexed_direct(function, CallableKind::Proc, &[], Span::at(source_id, 0)).unwrap().unwrap(), Value::ok(Value::Unit));
                        assert_eq!(evaluator.stdout, b"clean\n", "typed={typed} {name} frames={force_frames}");
                    }
                }
            }
        }
    });
}

#[test]
fn generic_closed_fixed_fields_preserve_path_and_error_receivers() {
    for (parameter, field, value) in [
        ("location: Path", "location.name", Value::Path(crate::runtime::value::PathValue::from_text("/xsh/item.ext").unwrap())),
        ("error: Error", "error.message", Value::Error(Box::new(crate::runtime::value::RuntimeError::new("probe", "actual message")))),
    ] {
        let argument = parameter.split(':').next().unwrap();
        let source = format!("pure ordinary({parameter}) -> Str {{ {field} }}\npure view(marker, {parameter}) {{ let _ = marker; {field} }}\npure number({parameter}) -> Str {{ view(7, {argument}) }}\npure text({parameter}) -> Str {{ view(\"word\", {argument}) }}\n");
        let (program, sources, source_id) = record_form_program(&source);
        for force_frames in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let function = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("ordinary")));
            let expected = evaluator.call_indexed_direct(function, CallableKind::Pure, std::slice::from_ref(&value), Span::at(source_id, 0)).unwrap().unwrap();
            for name in ["number", "text"] {
                let function = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
                let actual = evaluator.call_indexed_direct(function, CallableKind::Pure, std::slice::from_ref(&value), Span::at(source_id, 0)).unwrap().unwrap();
                assert_eq!(actual, expected, "{parameter} {name} frames={force_frames}");
            }
        }
    }
}

fn verify_record_guard_continuation_program(typed: bool) {
    let source = format!("{}\n{}", include_str!("../../../../tests/fixtures/runtime/generic-record-field-guards.xsh"), include_str!("../../../../tests/fixtures/runtime/generic-record-guard-continuations.xsh"));
    let source = if typed {
        source.replace("(marker,", "(marker: Any,")
    } else { source };
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("generic-record-guard-continuations.xsh", crate::loader::entry_source_from_text("generic-record-guard-continuations.xsh", source), Vec::new());
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let source_id = parsed.arena.source_text_source_id().unwrap();
    let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let prepared = Evaluator::new_with_sources(Vec::new(), sources).prepare_test_program(Arc::new(parsed.arena), source_id, checked).unwrap();
    for force_frames in [false, true] {
        let result = prepared.eval_test("verify_record_guard_continuations", Value::Unit, false, force_frames, Vec::new(), crate::runtime::eval::TestCancellation::default());
        assert_eq!(result.output.status, 0, "force_frames={force_frames}: {:?} {:?}", result.output.diagnostics, result.output.traceback);
        assert!(result.output.diagnostics.is_empty(), "{:?}", result.output.diagnostics);
        assert!(result.output.traceback.is_none(), "{:?}", result.output.traceback);
        assert!(matches!(&result.result, Some(Value::Result(ResultValue::Ok(value))) if matches!(value.as_ref(), Value::Unit)), "force_frames={force_frames}: {:?}", result.result);
    }
}

#[test]
fn typed_record_guard_continuations_preserve_actual_closed_records_on_both_routes() {
    verify_record_guard_continuation_program(true);
}

#[test]
fn generic_record_guard_continuations_preserve_actual_closed_records_on_both_routes() {
    verify_record_guard_continuation_program(false);
}

fn verify_direct_env_value_program(typed: bool) {
    let source = include_str!("../../../../tests/fixtures/runtime/generic-env-values.xsh");
    let source = if typed {
        source.replace("proc direct_env_value(value)", "proc direct_env_value(value: EnvScalar)")
            .replace("proc direct_env_paths(values)", "proc direct_env_paths(values: List[Path])")
            .replace("proc direct_env_map(value)", "proc direct_env_map(value: EnvScalar)")
            .replace("proc direct_env_pair(left, right)", "proc direct_env_pair(left: Str, right: Str)")
            .replace("proc observe_env(value)", "proc observe_env(value: Str)")
            .replace("proc fail_env(value)", "proc fail_env(value: Str)")
            .replace("proc collect_env(value)", "proc collect_env(value: Str)")
    } else { source.to_owned() };
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("generic-env-values.xsh", crate::loader::entry_source_from_text("generic-env-values.xsh", source), Vec::new());
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let source_id = parsed.arena.source_text_source_id().unwrap();
    let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let prepared = Evaluator::new_with_sources(Vec::new(), sources).prepare_test_program(Arc::new(parsed.arena), source_id, checked).unwrap();
    for force_frames in [false, true] {
        let result = prepared.eval_test("verify_generic_env_values", Value::Unit, false, force_frames, Vec::new(), crate::runtime::eval::TestCancellation::default());
        assert_eq!(result.output.status, 0, "force_frames={force_frames}: {:?} {:?}", result.output.diagnostics, result.output.traceback);
        assert!(result.output.diagnostics.is_empty(), "{:?}", result.output.diagnostics);
        assert!(result.output.traceback.is_none(), "{:?}", result.output.traceback);
        assert!(matches!(&result.result, Some(Value::Result(ResultValue::Ok(value))) if matches!(value.as_ref(), Value::Unit)), "force_frames={force_frames}: {:?}", result.result);
    }
}

#[test]
fn typed_env_values_preserve_conversion_scope_cleanup_and_stream_timing_on_both_routes() {
    verify_direct_env_value_program(true);
}

#[test]
fn inferred_env_values_preserve_conversion_scope_cleanup_and_stream_timing_on_both_routes() {
    verify_direct_env_value_program(false);
}

#[test]
fn inferred_env_values_reject_nonconvertible_nullable_and_dynamic_inputs() {
    let definitions = include_str!("../../../../tests/fixtures/runtime/generic-env-values.xsh");
    for body in [
        "let _ = direct_env_value(1.5)",
        "let _ = direct_env_value(b\"bytes\")",
        "let _ = direct_env_value(null)",
        "let value: Str? = \"maybe\"; let _ = direct_env_value(value)",
        "let value: Any = \"erased\"; let _ = direct_env_value(value)",
        "let _ = direct_env_value([1, 2])",
        "let _ = direct_env_value([\"one\", \"two\"])",
        "let value: Result[Str, Str] = Err(\"opaque\"); let _ = direct_env_value(value)",
        "enum EnvToken { Kept }\nproc rejected(value: EnvToken) [env, error] -> Result[Str] { direct_env_value(value) }",
        "proc rejected(value: FsRoot) [env, error] -> Result[Str] { direct_env_value(value) }",
    ] {
        let source = format!("{definitions}\n{body}\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{body}\n{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, &source, crate::sema::check::CheckOptions::for_execution());
        let errors = checked.diagnostics.iter().filter(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error).collect::<Vec<_>>();
        assert!(!errors.is_empty(), "{body}");
        assert!(errors.iter().all(|diagnostic| diagnostic.code != Some(crate::diagnostic::DiagnosticCode::CheckLocalInference)), "{body}\n{errors:?}");
    }
}

#[test]
fn generic_path_transforms_preserve_canonical_results_on_both_routes() {
    on_scoped_body_test_stack(|| {
        let source = include_str!("../../../../tests/xsh/generic-path-transforms.xsh");
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("generic-path-transforms.xsh", crate::loader::entry_source_from_text("generic-path-transforms.xsh", source.to_owned()), Vec::new());
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = parsed.arena.source_text_source_id().unwrap();
        let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let prepared = Evaluator::new_with_sources(Vec::new(), sources).prepare_test_program(Arc::new(parsed.arena), source_id, checked).unwrap();
        for force_frames in [false, true] {
            let result = prepared.eval_test("generic_path_transforms_preserve_canonical_results", Value::Unit, false, force_frames, Vec::new(), crate::runtime::eval::TestCancellation::default());
            assert_eq!(result.output.status, 0, "frames={force_frames}: {:?} {:?}", result.output.diagnostics, result.output.traceback);
            assert!(result.output.diagnostics.is_empty(), "{:?}", result.output.diagnostics);
            assert!(result.output.traceback.is_none(), "{:?}", result.output.traceback);
            assert_eq!(result.output.stdout, b"receiver\n", "frames={force_frames}");
            assert!(matches!(&result.result, Some(Value::Result(ResultValue::Ok(value))) if matches!(value.as_ref(), Value::Unit)), "frames={force_frames}: {:?}", result.result);
        }
    });
}

#[test]
fn imported_path_method_schemes_publish_separate_result_binders() {
    on_scoped_body_test_stack(|| {
        let directory = tempfile::Builder::new().prefix("path-method-schemes-").tempdir_in(".work/tmp").unwrap();
        std::fs::write(directory.path().join("transforms.xsh"), "##! Canonical path transformations.\n## Returns the canonical parent domain.\nexport pure parent(location) { location.parent() }\n## Returns the canonical normalized domain.\nexport pure normal(location) { location.normalize() }\n").unwrap();
        let source = r#"use transforms

test imported_path_method_results {
  let absolute: Path = p"/usr/share"
  let relative: RelPath = p"usr/share"
  let parent_absolute: Path = transforms.parent(absolute)
  let parent_relative: RelPath = transforms.parent(relative)
  let normalized: RelPath = transforms.normal(relative)
  assert parent_absolute == p"/usr"
  assert parent_relative == p"usr"
  assert normalized == relative
}
"#;
        let entry = directory.path().join("main.xsh");
        std::fs::write(&entry, source).unwrap();
        let (sources, parsed) = crate::loader::parse_script_with_module_roots(entry.to_str().unwrap(), &[directory.path().to_path_buf()]).unwrap();
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = parsed.arena.source_text_source_id().unwrap();
        let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        for family in [xsh_registry::signature::MethodFamilyId::PathParent, xsh_registry::signature::MethodFamilyId::PathNormalize] {
            let scheme = checked.function_schemes.values().find(|scheme| scheme.callable().requirements().methods.iter().any(|method| method.family == family)).unwrap();
            let crate::sema::schemes::SchemeType::Parameter(receiver) = &scheme.params()[0].ty else { panic!("the exported receiver remains a caller-independent parameter") };
            let crate::sema::schemes::SchemeType::Parameter(result) = scheme.return_type() else { panic!("the exported result retains its canonical member relationship") };
            assert_ne!(receiver, result);
        }
        let prepared = Evaluator::new_with_sources(Vec::new(), sources).prepare_test_program(Arc::new(parsed.arena), source_id, checked).unwrap();
        drop(directory);
        for force_frames in [false, true] {
            let result = prepared.eval_test("imported_path_method_results", Value::Unit, false, force_frames, Vec::new(), crate::runtime::eval::TestCancellation::default());
            assert_eq!(result.output.status, 0, "frames={force_frames}: {:?} {:?}", result.output.diagnostics, result.output.traceback);
            assert!(result.output.diagnostics.is_empty(), "{:?}", result.output.diagnostics);
            assert!(result.output.traceback.is_none(), "{:?}", result.output.traceback);
            assert!(matches!(&result.result, Some(Value::Result(ResultValue::Ok(value))) if matches!(value.as_ref(), Value::Unit)), "frames={force_frames}: {:?}", result.result);
        }
    });
}
