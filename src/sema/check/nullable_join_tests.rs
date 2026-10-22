use crate::diagnostic::{DiagnosticCode, Severity};
use crate::sema::check::Checker;
use crate::source::SourceId;
use crate::syntax::parser::Parser;

fn accepts(source: &str) {
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    let errors = checked.diagnostics.iter().filter(|diagnostic| diagnostic.severity == Severity::Error).collect::<Vec<_>>();
    assert!(errors.is_empty(), "{source}\n{errors:?}");
}

#[test]
fn nullable_branch_inference_joins_concrete_values_in_both_orders() {
    for (ty, value) in [("Str", "\"value\""), ("Int", "7"), ("Bool", "true"), ("List[Int]", "[7]"), ("Row", "{item: 7}")] {
        for (left, right) in [(value, "null"), ("null", value)] {
            for expression in [format!("if true {{ {left} }} else {{ {right} }}"), format!("match true {{ true => {left}; false => {right} }}")] {
                accepts(&format!("type Row = {{item: Int}}\nlet joined = {expression}\nlet result: {ty}? = joined\n"));
            }
        }
    }
}

#[test]
fn nullable_branch_inference_preserves_one_symbolic_payload_binder() {
    for expression in [
        "if flag { value } else { null }",
        "if flag { null } else { value }",
        "match flag { true => value; false => null }",
        "match flag { true => null; false => value }",
    ] {
        accepts(&format!("pure maybe(flag: Bool, value) {{ let joined = {expression}; joined }}\nlet number: Int? = maybe(true, 7)\nlet text: Str? = maybe(false, \"value\")\n"));
    }
}

#[test]
fn nullable_branch_inference_preserves_producer_and_schema_relationships() {
    accepts("pure preload_for(configured: Str?) -> Int { let _ = configured; 7 }\nproc detect() [env, error] -> Result[Int] { let configured = if let Ok(value) = env.get(\"XSH_DEV_BUILD_PRELOAD\") { value } else { null }; preload_for(configured) }\n");
    accepts("type Row = {users: Int?}\npure number(value: Str) -> Result[Int] { value as Int }\npure parse(raw: Str) -> Result[Row] { let users = if raw == \"-\" { null } else { number(raw)? }; {users: users} }\n");
}

#[test]
fn nullable_branch_inference_retains_explicit_and_nonnull_domain_errors() {
    for source in [
        "let result: Int = if true { 7 } else { null }\n",
        "let result: Int = if true { null } else { 7 }\n",
        "let result = if true { null } else if false { 7 } else { \"wrong\" }\n",
        "let result = if true { [null] } else { [7] }\n",
        "let only = if true { null } else { null }\nlet result: Int = only\n",
    ] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckTypeMismatch)), "{source}\n{:?}", checked.diagnostics);
    }
}

#[test]
fn nullable_branch_inference_keeps_dynamic_return_inference_underdetermined() {
    let source = "pure maybe(flag: Bool, value: Any) { if flag { null } else { value } }\nlet _ = maybe(true, 7)\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    let errors = checked.diagnostics.iter().filter(|diagnostic| diagnostic.severity == Severity::Error).collect::<Vec<_>>();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, Some(DiagnosticCode::CheckInferReturn));
    assert!(errors[0].message.contains("return shape is underdetermined"), "{errors:?}");
    for returned in ["Any", "Any?"] {
        accepts(&format!("pure maybe(flag: Bool, value: {returned}) -> {returned} {{ if flag {{ null }} else {{ value }} }}\n"));
    }
    let source = "pure maybe(flag: Bool, value: Any) -> Any? { if flag { null } else { value } }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckDynamicBoundary)), "{:?}", checked.diagnostics);
}

#[test]
fn nullable_branch_inference_preserves_statement_tail_contexts() {
    for expression in [
        "if flag { value } else { null }",
        "if flag { null } else { value }",
        "match flag { true => value; false => null }",
        "match flag { true => null; false => value }",
    ] {
        accepts(&format!("pure maybe(flag: Bool, value) {{ {expression} }}\nlet number: Int? = maybe(true, 7)\nlet text: Str? = maybe(false, \"value\")\n"));
    }
}

#[test]
fn proc_nested_if_inside_list_preserves_bytes_item_type() {
    let source = "proc visit(data: Bytes, mark: Bytes) {\n  let records = if data.is_empty() { [] } else { [if data[..1] == mark { data[..1] } else { data }] }\n  for raw_line in records {\n    let line = raw_line\n    let end = line.len()\n  }\n}\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    let errors = checked.diagnostics.iter().filter(|diagnostic| diagnostic.severity == Severity::Error).collect::<Vec<_>>();
    assert!(errors.is_empty(), "{source}\n{errors:?}");

    let source_id = SourceId::new(0);
    let nested_if = source.find("if data[..1] == mark { data[..1] } else { data }").unwrap();
    let nested_if = crate::source::Span::new(source_id, nested_if, nested_if + "if data[..1] == mark { data[..1] } else { data }".len());
    let branch_bytes = source.match_indices("data[..1]").nth(1).unwrap().0;
    let branch_bytes = crate::source::Span::new(source_id, branch_bytes, branch_bytes + "data[..1]".len());
    let branch_data = source.rfind("data }").unwrap();
    let branch_data = crate::source::Span::new(source_id, branch_data, branch_data + "data".len());
    let records = source.find("if data.is_empty() { [] } else { [if data[..1] == mark { data[..1] } else { data }] }").unwrap();
    let records = crate::source::Span::new(source_id, records, records + "if data.is_empty() { [] } else { [if data[..1] == mark { data[..1] } else { data }] }".len());
    let line_len = source.find("line.len()").unwrap();
    let line_len = crate::source::Span::new(source_id, line_len, line_len + "line.len()".len());

    assert_eq!(checked.expr_types.get(&branch_bytes), Some(&crate::sema::check::Type::Bytes));
    assert_eq!(checked.expr_types.get(&branch_data), Some(&crate::sema::check::Type::Bytes));
    assert_eq!(checked.expr_types.get(&nested_if), Some(&crate::sema::check::Type::Bytes));
    assert_eq!(checked.expr_types.get(&records), Some(&crate::sema::check::Type::List(Box::new(crate::sema::check::Type::Bytes))));
    assert_eq!(checked.expr_types.get(&line_len), Some(&crate::sema::check::Type::Int));
}

#[test]
fn branch_join_does_not_invent_empty_list_item_type() {
    let source = "let values = if true { [] } else { [] }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    let errors = checked.diagnostics.iter().filter(|diagnostic| diagnostic.severity == Severity::Error).collect::<Vec<_>>();
    assert!(errors.is_empty(), "{source}\n{errors:?}");

    let expression = source.find("if true { [] } else { [] }").unwrap();
    let expression = crate::source::Span::new(SourceId::new(0), expression, expression + "if true { [] } else { [] }".len());
    assert_eq!(checked.expr_types.get(&expression), Some(&crate::sema::check::Type::List(Box::new(crate::sema::check::Type::Unknown))));
}

#[test]
fn branch_join_keeps_stored_collection_domains_strict() {
    for source in [
        "pure keep(flag: Bool, values: List[Int], data: Bytes) -> List[Int] { if flag { values } else { [data] } }\n",
        "proc keep(flag: Bool, data: Bytes) { let empty = []; let bytes = if flag { empty } else { [data] }; let ints: List[Int] = empty }\n",
    ] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        let errors = checked.diagnostics.iter().filter(|diagnostic| diagnostic.severity == Severity::Error).collect::<Vec<_>>();
        assert!(errors.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckTypeMismatch)), "{source}\n{errors:?}");
    }
}

#[test]
fn nullable_branch_inference_flattens_existing_optional_candidates() {
    for expression in [
        "if first { value } else { if second { null } else { value } }",
        "if first { null } else { match second { true => value; false => null } }",
        "match first { true => value; false => { if second { null } else { value } } }",
    ] {
        accepts(&format!("pure maybe(first: Bool, second: Bool, value: Int?) {{ let result = {expression}; result }}\nlet result: Int? = maybe(true, false, 7)\n"));
    }
}
