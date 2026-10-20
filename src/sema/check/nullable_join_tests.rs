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
fn nullable_branch_inference_flattens_existing_optional_candidates() {
    for expression in [
        "if first { value } else { if second { null } else { value } }",
        "if first { null } else { match second { true => value; false => null } }",
        "match first { true => value; false => { if second { null } else { value } } }",
    ] {
        accepts(&format!("pure maybe(first: Bool, second: Bool, value: Int?) {{ let result = {expression}; result }}\nlet result: Int? = maybe(true, false, 7)\n"));
    }
}
