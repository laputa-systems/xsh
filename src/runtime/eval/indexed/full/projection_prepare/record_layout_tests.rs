use super::*;
use crate::runtime::eval::Evaluator;
use crate::runtime::value::{RecordMap, ResultValue, Value};
use crate::sema::check::Checker;

#[test]
fn validated_record_layout_projects_after_host_round_trip_on_both_routes_without_frontend() {
    std::thread::Builder::new().stack_size(16 * 1024 * 1024).spawn(|| {
        let source = "type Payload = {required: Int}\ntype Nested = {payload: Payload, required: Int}\npure validate(raw: Any) -> Result[Nested] { raw.require(Nested) }\npure project(value: Nested) -> Int { value.payload.required + value.required }\n";
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
            "record-layout.xsh", crate::loader::entry_source_from_text("record-layout.xsh", source.to_owned()), Vec::new());
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = SourceMap::files(&sources).first().unwrap().id();
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let weak = Arc::downgrade(&checked.solved);
        let mut preparing = Evaluator::new_with_sources(Vec::new(), sources);
        preparing.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked).unwrap();
        let program = preparing.indexed_program.take().unwrap();
        drop(preparing); drop(checked); drop(parsed);
        assert!(weak.upgrade().is_none(), "execution cannot retain frontend authority");
        FullVerifier::verify(&program).unwrap();
        assert_eq!(program.generic_evidence().unwrap().ground_projections().count(), 3, "every nested selection must execute its prepared numeric projection");
        for recursive in [false, true] {
            program.symbol_owner().with_current(|| {
                let raw = Value::Record(RecordMap::from([
                    (Arc::from("aardvark"), Value::Bool(true)),
                    (Arc::from("required"), Value::Int(7)),
                    (Arc::from("payload"), Value::Record(RecordMap::from([
                        (Arc::from("aardvark"), Value::Str(Arc::from("extra"))),
                        (Arc::from("required"), Value::Int(9)),
                    ]))),
                ]));
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let mut execute = || {
                    assert_eq!(crate::runtime::eval::lowered_run::recursive_fast_path_forced(), recursive);
                    let validated = evaluator.call_indexed_direct(
                        LoweredFunctionKey::Name(Name::intern("validate")), LoweredFunctionKind::Pure,
                        &[raw.clone()], Span::new(source_id, 0, 0)).unwrap().unwrap();
                    let Value::Result(ResultValue::Ok(record)) = validated else { panic!("valid record must succeed"); };
                    assert_eq!(*record, raw, "validation must preserve all extra fields");
                    let projected = evaluator.call_indexed_direct(
                        LoweredFunctionKey::Name(Name::intern("project")), LoweredFunctionKind::Pure,
                        &[*record], Span::new(source_id, 0, 0)).unwrap().unwrap();
                    assert_eq!(projected, Value::Int(16), "recursive={recursive}");
                    for invalid in [Value::Record(RecordMap::new()), Value::Record(RecordMap::from([
                        (Arc::from("required"), Value::Int(7)),
                        (Arc::from("payload"), Value::Record(RecordMap::from([(Arc::from("required"), Value::Str(Arc::from("wrong")))]))),
                    ]))] {
                        let rejected = evaluator.call_indexed_direct(
                            LoweredFunctionKey::Name(Name::intern("validate")), LoweredFunctionKind::Pure,
                            &[invalid], Span::new(source_id, 0, 0)).unwrap().unwrap();
                        let Value::Result(ResultValue::Err(error)) = rejected else { panic!("invalid record must fail schema validation"); };
                        let Value::Error(error) = error.as_ref() else { panic!("schema failure must remain structured"); };
                        assert_eq!(error.kind, "schema");
                    }
                };
                if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(execute) } else { execute() }
            });
        }
    }).unwrap().join().unwrap_or_else(|payload| std::panic::resume_unwind(payload));
}
