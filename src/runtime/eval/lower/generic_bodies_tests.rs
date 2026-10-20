use crate::runtime::eval::{Evaluator, FullBuilder, LoweredFunctionKey, LoweredFunctionKind};
use crate::runtime::value::{ResultValue, Value};
use crate::sema::check::Checker;
use crate::source::{SourceId, SourceMap, Span};
use crate::symbol::Name;
use crate::syntax::parser::Parser;
use std::sync::Arc;

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
            (Arc::new(program), sources, if proc { LoweredFunctionKind::Proc } else { LoweredFunctionKind::Pure })
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
            let actual = evaluator.call_indexed_direct(key, LoweredFunctionKind::Proc, &[input], Span::at(source_id, 0)).unwrap().unwrap();
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
            let actual = evaluator.call_indexed_direct(key, LoweredFunctionKind::Proc, &[input], Span::at(source_id, 0)).unwrap().unwrap();
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
            let actual = evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &[], Span::at(SourceId::new(0), 0)).unwrap().unwrap();
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
            (Arc::new(program), sources, if proc { LoweredFunctionKind::Proc } else { LoweredFunctionKind::Pure })
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
                let actual = evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &[Value::List(vec![Value::Int(item)])], Span::at(source_id, 0)).unwrap();
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
        assert_eq!(evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &[], Span::at(source_id, 0)).unwrap().unwrap(), Value::List(vec![Value::Int(1), Value::Int(1)]));
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
            let actual = evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &[Value::Int(index)], Span::at(source_id, 0)).unwrap();
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
        ("", "List[Int]?", "input?[0]", Value::List(vec![Value::Int(9)].into())),
        ("", "List[Int]?", "input?[-1]", Value::List(vec![Value::Int(9)].into())),

        ("", "List[Int]?", "input?[0..1]", Value::List(vec![Value::Int(9)].into())),
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
                    let actual = evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &[input.clone()], Span::at(source_id, 0)).unwrap().unwrap();
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
                let actual = evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &[input.clone()], Span::at(source_id, 0)).unwrap();
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
            let actual = evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &[input], Span::at(source_id, 0)).unwrap().unwrap();
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
                    let actual = evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &[input.clone()], Span::at(source_id, 0)).unwrap().unwrap();
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
                    let actual = evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &[input.clone()], Span::at(source_id, 0)).unwrap().unwrap();
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
            let actual = evaluator.call_indexed_direct(function, LoweredFunctionKind::Pure, &[items, Value::Int(key)], Span::at(source_id, 0)).unwrap();
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
                let error = evaluator.call_indexed_direct(plain, LoweredFunctionKind::Pure, &[Value::Result(ResultValue::Err(Box::new(Value::Str(Arc::from("plain")))))], Span::at(source_id, 0)).unwrap().unwrap_err();
                assert_eq!(error.kind, "stream-stage-error");
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
                        let actual = evaluator.call_indexed_direct(function, LoweredFunctionKind::Pure, &args, Span::at(source_id, 0)).unwrap().unwrap();
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
                let actual = evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &[], Span::at(source_id, 0)).unwrap().unwrap();
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
            let actual = evaluator.call_indexed_direct(key, LoweredFunctionKind::Proc, &[], Span::at(source_id, 0)).unwrap().unwrap();
            let Value::Result(ResultValue::Ok(actual)) = actual else { panic!("a proc preserves its result envelope") };
            let Value::Record(fields) = *actual else { panic!("the ordered constructor returns its counters") };
            assert_eq!(fields.get("count"), Some(&Value::Int(2)), "{operation}");
            let expected = if operation.starts_with('f') { Value::Str(Arc::from("1:2")) }
                else if operation.starts_with('[') { Value::List(vec![Value::Int(1), Value::Int(2)]) }
                else if operation.starts_with("{first:") { Value::Record([(Arc::from("first"), Value::Int(99)), (Arc::from("second"), Value::Int(2))].into_iter().collect()) }
                else { Value::Map([(crate::runtime::value::MapKey::Int(0), Value::Int(2))].into_iter().collect()) };
            assert_eq!(fields.get("built"), Some(&expected), "{operation}");
        }
    }
    for operation in ["f\"{[1][first]}:{[2][second]}\"", "[@[[1][first]], [2][second]]", "{...{first: [1][first]}, second: [2][second]}", "{[0]: [1][first], [1]: [2][second]}"] {
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
            let error = evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &[], Span::at(source_id, 0)).unwrap().unwrap_err();
            assert_eq!(error.kind, "index-out-of-range");
            assert_eq!(&source[error.span.unwrap().range()], "[1][first]");
        }
    }
}
