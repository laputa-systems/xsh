use super::{LintOptions, Linter};
use xsh::diagnostic::DiagnosticCode;
use xsh::frontend::check::Checker;
use xsh::frontend::source::SourceId;
use xsh::frontend::syntax::parser::Parser;

/// The names `lint.unused-type` reports in `source`, in order.
fn unused_types(source: &str) -> Vec<String> {
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let options = LintOptions {
        expr_types: checked.expr_types,
        ..LintOptions::default()
    };
    Linter::lint(&parsed.arena, source, options)
        .diagnostics
        .into_iter()
        .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintUnusedType))
        .map(|diagnostic| diagnostic.message)
        .collect()
}

#[test]
fn type_named_only_as_a_union_member_is_used() {
    let source = "type Named = {name: Str}\n\ntype Sized = {size: Int}\n\npure label(item: Union[Named, Sized]) -> Str {\n  match item {\n    named is Named => named.name\n    sized is Sized => f\"{sized.size}\"\n  }\n}\n\nprint label({name: \"a\"})\n";
    assert_eq!(unused_types(source), Vec::<String>::new());
}

#[test]
fn type_named_only_inside_a_union_in_a_collection_is_used() {
    let source = "type Named = {name: Str}\n\npure count(items: List[Union[Named, Int]]?) -> Int {\n  (items ?? []).len()\n}\n\nprint count(null)\n";
    assert_eq!(unused_types(source), Vec::<String>::new());
}

#[test]
fn type_named_only_inside_a_callable_type_is_used() {
    let source = "type Request = {path: Str}\n\ntype Reply = {status: Int}\n\npure ok(request: Request) -> Reply {\n  {status: request.path.byte_len()}\n}\n\npure serve(handler: pure(request: Request) -> Reply) -> Int {\n  handler({path: \"/\"}).status\n}\n\nprint serve(ok)\n";
    let source_without_other_uses = source.replace(
        "pure ok(request: Request) -> Reply {\n  {status: request.path.byte_len()}\n}\n\n",
        "",
    );
    assert_eq!(unused_types(source), Vec::<String>::new());
    // The callable type alone keeps both names in use.
    let alone = source_without_other_uses.replace("print serve(ok)\n", "");
    assert_eq!(unused_types(&alone), Vec::<String>::new());
}

#[test]
fn type_named_nowhere_is_still_reported() {
    let source = "type Named = {name: Str}\n\ntype Spare = {size: Int}\n\npure count(items: List[Union[Named, Int]]) -> Int {\n  items.len()\n}\n\nprint count([1])\n";
    assert_eq!(
        unused_types(source),
        vec!["unused type declaration `Spare`".to_string()]
    );
}
