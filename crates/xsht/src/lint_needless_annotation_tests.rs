use super::{LintOptions, Linter};
use xsh::diagnostic::{Diagnostic, DiagnosticCode, Severity};
use xsh::frontend::check::Checker;
use xsh::frontend::source::SourceId;
use xsh::frontend::syntax::parser::Parser;

// The lint runs from the linter's statement traversal, so the tests drive
// the whole linter restricted to this code.
fn lint(source: &str) -> Vec<Diagnostic> {
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let checked_output = std::sync::Arc::new(checked.clone());
    let options = LintOptions {
        checked_output: Some(checked_output),
        expr_types: checked.expr_types,
        only: Some(vec![DiagnosticCode::LintNeedlessAnnotation]),
        ..LintOptions::default()
    };
    Linter::lint(&parsed.arena, source, options).diagnostics
}

#[test]
fn syntax_findings_remain_when_checked_proof_facts_are_absent() {
    let source = "proc greet(name: Str = \"world\") {\n  return\n  print $name\n}\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let output = Linter::lint(
        &parsed.arena,
        source,
        LintOptions {
            only: Some(vec![
                DiagnosticCode::LintNeedlessAnnotation,
                DiagnosticCode::LintDeadCode,
            ]),
            ..LintOptions::default()
        },
    );
    assert!(
        output.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == Some(DiagnosticCode::LintDeadCode)
        }),
        "the syntax-only finding does not need checker facts: {:?}",
        output.diagnostics
    );
    assert!(
        !output.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == Some(DiagnosticCode::LintNeedlessAnnotation)
        }),
        "removal proof requires the caller's checked baseline: {:?}",
        output.diagnostics
    );
}

fn flagged(source: &str) -> Vec<String> {
    lint(source)
        .iter()
        .flat_map(|diagnostic| &diagnostic.labels)
        .map(|label| source[label.span.range()].to_string())
        .collect()
}

// An annotation that restates the initializer's type is the one the lint
// removes; the cases below differ from it in one respect each.
#[test]
fn an_annotation_that_restates_the_initializer_is_needless() {
    let source = "proc show(count: Int, name: Str) {\n  let total: Int = count + 1\n  let label: Str = name.trim()\n  print $total $label\n}\n";
    assert_eq!(flagged(source), ["Int", "Str"]);
}

// A function's name is a callable with a checked signature. `Proc` and
// `Pure` make it a dynamic handle, which is another type: the binding then
// takes any function and a `.call` on it is checked at run time.
#[test]
fn an_annotation_that_makes_a_function_dynamic_is_not_needless() {
    let source = "proc greet() {\n  print \"hi\"\n}\n\npure twice(count: Int) -> Int {\n  count * 2\n}\n\nlet action: Proc = greet\nlet scale: Pure = twice\nlet _ = action.call()\nlet _ = scale.call(2)\n";
    assert!(lint(source).is_empty(), "{:?}", lint(source));
}

// An annotation wider than the initializer's type is how a value is given
// the wider type: a validated value its base, at any depth, and a value an
// optional or a union that holds it.
#[test]
fn an_annotation_that_widens_the_initializer_is_not_needless() {
    let source = "type Port = Int range 1..=65535\n\nproc show(port: Port, ports: List[Port], names: NonEmpty[Str], rel: RelPath) {\n  let number: Int = port\n  let numbers: List[Int] = ports\n  let listed: List[Str] = names\n  let place: Path = rel\n  let maybe: Int? = number\n  let either: Union[Int, Str] = number\n  print $number (numbers.len()) (listed.len()) $place (maybe ?? 0) (either is Int)\n}\n";
    assert!(lint(source).is_empty(), "{:?}", lint(source));
}

#[test]
fn type_annotated_discards_keep_static_assertions() {
    let source = "let names = [\"printf\"]\nlet paths = [p\"printf\"]\nlet _: Command = process.command_argv(\"printf\", names)\nlet _: Command = process.command_argv(p\"printf\", paths)\n";
    assert!(lint(source).is_empty(), "{:?}", lint(source));
}

#[test]
fn type_annotated_named_bindings_remain_removable() {
    let source = "let names = [\"printf\"]\nlet command: Command = process.command_argv(\"printf\", names)\n";
    assert_eq!(flagged(source), ["Command"]);
}

#[test]
fn a_dynamic_list_annotation_stays_when_removing_it_breaks_a_typed_boundary() {
    let source = "pure row() -> Record { {value: 1} }\npure measure(rows: List[Any]) -> Int { rows.len() }\npure lookup() -> Int {\n  let raw: Any = 1\n  let one: List[Any] = [raw]\n  let rows: List[Any] = [row()]\n  measure(rows) + one.len()\n}\n";
    let diagnostics = lint(source);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(
        &source[diagnostics[0].labels[0].span.range()],
        "List[Any]"
    );
    assert_eq!(
        diagnostics[0].labels[0].span.start(),
        source.find("one: List[Any]").unwrap() + "one: ".len(),
        "only the independently inferred list remains removable"
    );

    let rewritten = source.replace("let rows: List[Any]", "let rows");
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), &rewritten);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, &rewritten);
    assert!(
        checked
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error),
        "the unannotated List[Record] must fail at the List[Any] call boundary: {:?}",
        checked.diagnostics
    );
}
