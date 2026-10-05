use super::super::{LintOptions, Linter};
use xsh::diagnostic::{Diagnostic, DiagnosticCode};
use xsh::frontend::check::Checker;
use xsh::frontend::source::SourceId;
use xsh::frontend::syntax::parser::Parser;

fn lint(source: &str) -> Vec<Diagnostic> {
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let options = LintOptions {
        terminating_call_spans: checked.terminating_call_spans,
        ..LintOptions::default()
    };
    Linter::lint(&parsed.arena, source, options)
        .diagnostics
        .into_iter()
        .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferExit))
        .collect()
}

/// The source after applying every fix the lint offers.
fn fixed(source: &str) -> String {
    let mut edits: Vec<(std::ops::Range<usize>, String)> = lint(source)
        .iter()
        .flat_map(|diagnostic| &diagnostic.fix_hints)
        .filter_map(|hint| Some((hint.span?.range(), hint.replacement.clone()?)))
        .collect();
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut text = source.to_owned();
    for (range, replacement) in edits {
        text.replace_range(range, &replacement);
    }
    text
}

#[test]
fn abort_statements_become_exit_statements() {
    for (source, expected) in [
        ("abort(2)\n", "exit 2\n"),
        ("let code = 3\nabort(code + 1)\n", "let code = 3\nexit code + 1\n"),
        ("abort(status: 4)\n", "exit 4\n"),
        (
            "proc finish(code: Int) {\n  if code > 0 {\n    abort(code)\n  }\n  match code {\n    0 => abort(0)\n    _ => abort( 9 ) # last\n  }\n}\n",
            "proc finish(code: Int) {\n  if code > 0 {\n    exit code\n  }\n  match code {\n    0 => exit 0\n    _ => exit 9 # last\n  }\n}\n",
        ),
        (
            "let ready = false\nguard ready else { abort(1) }\n",
            "let ready = false\nguard ready else { exit 1 }\n",
        ),
    ] {
        assert_eq!(fixed(source), expected, "{source}");
        assert!(lint(expected).is_empty(), "{expected}");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), expected);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, expected);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    }
}

#[test]
fn a_forced_abort_is_left_alone() {
    // `force` skips deferred cleanup; `exit` always runs it.
    for source in ["abort(2, force: true)\n", "abort(2, true)\n", "abort(2, force: false)\n"] {
        assert!(lint(source).is_empty(), "{source}");
    }
}

#[test]
fn an_abort_that_is_not_a_statement_is_reported_without_a_fix() {
    let source = "let done = abort(3)\n";
    let diagnostics = lint(source);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert!(diagnostics[0].fix_hints.is_empty());
    assert_eq!(fixed(source), source);
}
