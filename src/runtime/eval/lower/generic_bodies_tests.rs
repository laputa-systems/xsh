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
