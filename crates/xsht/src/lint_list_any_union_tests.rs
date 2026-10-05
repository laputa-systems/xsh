use super::super::{LintOptions, Linter};
use xsh::diagnostic::{Diagnostic, DiagnosticCode};
use xsh::frontend::check::Checker;
use xsh::frontend::source::SourceId;
use xsh::frontend::syntax::parser::Parser;

/// Runs the whole linter, as `xsht lint` does, and keeps this rule's reports:
/// the rule is reached only through the linter's statement traversal.
fn lint(source: &str) -> Vec<Diagnostic> {
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
        .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintListAnyUnion))
        .collect()
}

#[test]
fn literal_list_of_a_few_types_names_its_union() {
    let source = "proc build(root: Path) [process, error] {\n  let argv: List[Any] = [\"make\", \"-C\", root, 4]\n  print ${argv.len()}\n}\n";
    let diagnostics = lint(source);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    let diagnostic = &diagnostics[0];
    assert_eq!(
        diagnostic.message,
        "every element of this `List[Any]` is one of 3 types"
    );
    let label = &diagnostic.labels[0];
    assert_eq!(&source[label.span.range()], "List[Any]");
    assert_eq!(
        label.message.as_deref(),
        Some("the closed type is `List[Union[Str, Path, Int]]`")
    );
    // The rewrite changes the binding's type, so it is left to the author.
    assert!(diagnostic.fix_hints.is_empty());

    // The suggested annotation checks and is not reported again.
    let rewritten = source.replace("List[Any]", "List[Union[Str, Path, Int]]");
    assert!(lint(&rewritten).is_empty());
}

#[test]
fn top_level_and_nested_bindings_are_visited() {
    let source = "let defaults: List[Any] = [1, \"one\"]\n\npure pick(flag: Bool) -> Int {\n  if flag {\n    let inner: List[Any] = [1.5, \"x\"]\n    return inner.len()\n  }\n\n  defaults.len()\n}\n";
    let diagnostics = lint(source);
    assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
    let suggested = |diagnostic: &Diagnostic| diagnostic.labels[0].message.clone().unwrap();
    assert!(suggested(&diagnostics[0]).contains("List[Union[Str, Int]]"));
    assert!(suggested(&diagnostics[1]).contains("List[Union[Str, Float]]"));
}

// A `var` is provable when every write through its name is a typed list
// literal, wherever the write is; a spliced `List[T]` contributes `T`.
#[test]
fn mutable_list_filled_only_from_typed_literals_names_its_union() {
    let source = "proc compile(cc: Path, flags: List[Str], levels: List[Int], debug: Bool) [error] {\n  var argv: List[Any] = [cc, \"-c\"]\n  argv = [@argv, @flags]\n  if debug {\n    for level in levels {\n      argv += [\"-g\", level]\n    }\n  } else {\n    argv = [cc, \"-E\"]\n  }\n\n  print ${argv.len()}\n}\n";
    let diagnostics = lint(source);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(
        diagnostics[0].labels[0].message.as_deref(),
        Some("the closed type is `List[Union[Str, Path, Int]]`")
    );
    assert_eq!(
        &source[diagnostics[0].labels[0].span.range()],
        "List[Any]"
    );
}

// One write that is not a typed list literal leaves the element types
// unknown: a computed list, an element assignment, a dynamic element, a
// spliced dynamic list, and a replacement by a computed list.
#[test]
fn mutable_list_with_an_unproven_write_is_left_alone() {
    let source = "pure more() -> List[Any] {\n  [1.5]\n}\n\nproc compile(cc: Path, raw: Any, rest: List[Any]) [error] {\n  var computed: List[Any] = [cc, \"-c\"]\n  computed += more()\n  var indexed: List[Any] = [cc, \"-c\"]\n  indexed[0] = 1.5\n  var dynamic: List[Any] = [cc, \"-c\"]\n  dynamic += [raw]\n  var spliced: List[Any] = [cc, \"-c\"]\n  spliced += [@rest]\n  var replaced: List[Any] = [cc, \"-c\"]\n  replaced = more()\n  print ${computed.len() + indexed.len() + dynamic.len() + spliced.len() + replaced.len()}\n}\n";
    assert!(lint(source).is_empty(), "{:?}", lint(source));
}

// Each of these lists is not provably a small closed set: one element type,
// a dynamic element, a spliced dynamic list, members that would not form a
// union, a member with no portable spelling, and too many types.
#[test]
fn lists_without_a_provable_small_union_are_left_alone() {
    let source = "type Named = {name: Str}\n\nproc build(root: Path, raw: Any, rest: List[Any], named: Named) [error] {\n  let same: List[Any] = [\"a\", \"b\"]\n  let dynamic: List[Any] = [\"a\", raw]\n  let spliced: List[Any] = [\"a\", root, @rest]\n  let overlapping: List[Any] = [1, \"a\".byte_len()]\n  let record: List[Any] = [\"a\", named]\n  let many: List[Any] = [1, 1.5, \"a\", root, true]\n  let empty: List[Any] = []\n  let typed: List[Union[Str, Path]] = [\"a\", root]\n  print ${same.len() + dynamic.len() + spliced.len() + overlapping.len()}\n  print ${record.len() + many.len() + empty.len() + typed.len()}\n}\n";
    assert!(lint(source).is_empty(), "{:?}", lint(source));
}
