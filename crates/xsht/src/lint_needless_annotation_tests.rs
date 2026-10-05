use super::{LintOptions, Linter};
use xsh::diagnostic::{Diagnostic, DiagnosticCode};
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
    let options = LintOptions {
        expr_types: checked.expr_types,
        only: Some(vec![DiagnosticCode::LintNeedlessAnnotation]),
        ..LintOptions::default()
    };
    Linter::lint(&parsed.arena, source, options).diagnostics
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
