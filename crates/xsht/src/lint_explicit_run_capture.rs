//! `lint.explicit-run-capture`: a `run.text`, `run.bytes`, `run.capture`, or
//! `run.stream` form in value position without a `?` keeps its failure as a
//! `Result` value. `try run...` says so, and means exactly the same.
//!
//! The checker decides which run forms these are: every value-position form
//! whose value is a `Result`, except the operand of a `?`. The lint only
//! writes the word in front of them.

use std::collections::BTreeSet;
use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{AstArena, RunFormId};

pub(super) fn explicit_run_capture(
    arena: &AstArena,
    implicitly_captured: &BTreeSet<Span>,
    run: RunFormId,
) -> Option<Diagnostic> {
    if implicitly_captured.is_empty() {
        return None;
    }
    let span = arena.span(arena.run_form(run).span);
    implicitly_captured.contains(&span).then(|| {
        Diagnostic::warning("this run form keeps its failure as a value; say so with `try`")
            .with_code(DiagnosticCode::LintExplicitRunCapture)
            .with_label(Label::secondary(
                span,
                "its value is a `Result`: write `try run...` to keep it, or end it with `?` to propagate",
            ))
            .with_fix_hint(FixHint::replacement(
                Span::at(span.source_id, span.start()),
                "capture the result with `try`",
                "try ",
            ))
    })
}

#[cfg(test)]
mod tests {
    use super::super::lint_redundant_propagation::tests::lint_rule;
    use xsh::diagnostic::{Diagnostic, DiagnosticCode};

    fn lint(source: &str) -> Vec<Diagnostic> {
        lint_rule(source, DiagnosticCode::LintExplicitRunCapture)
    }

    fn apply(diagnostics: &[Diagnostic], source: &str) -> String {
        let mut fixes = diagnostics
            .iter()
            .flat_map(|diagnostic| &diagnostic.fix_hints)
            .collect::<Vec<_>>();
        fixes.sort_by_key(|fix| std::cmp::Reverse(fix.span.unwrap().start()));
        let mut fixed = source.to_owned();
        for fix in fixes {
            fixed.replace_range(
                fix.span.unwrap().range(),
                fix.replacement.as_deref().unwrap(),
            );
        }
        fixed
    }

    #[test]
    fn every_captured_run_form_gains_try_and_keeps_its_type() {
        let source = "proc show(outcome: Result[Str, ProcessError]) -> Str {\n  outcome ?? \"none\"\n}\n\nproc describe() -> Result[Str] {\n  let bound = run.text vcs describe\n  var later = run.bytes vcs describe\n  later = run.bytes vcs status\n  let fallback = (run.text vcs describe) ?? \"unknown\"\n  let shown = show((run.text vcs describe))\n  let captured = run.capture --text vcs describe\n  guard let branch = (run.text vcs branch) else {\n    return Ok(\"detached\")\n  }\n\n  match run.text vcs describe {\n    Ok(text) => print $text\n    Err(_) => print $fallback $shown $branch\n  }\n  let _ = [bound is Ok(_), later is Ok(_), captured is Ok(_)]\n  return run.text vcs describe\n}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 9, "{diagnostics:?}");
        let fixed = apply(&diagnostics, source);
        assert_eq!(
            fixed,
            "proc show(outcome: Result[Str, ProcessError]) -> Str {\n  outcome ?? \"none\"\n}\n\nproc describe() -> Result[Str] {\n  let bound = try run.text vcs describe\n  var later = try run.bytes vcs describe\n  later = try run.bytes vcs status\n  let fallback = (try run.text vcs describe) ?? \"unknown\"\n  let shown = show((try run.text vcs describe))\n  let captured = try run.capture --text vcs describe\n  guard let branch = (try run.text vcs branch) else {\n    return Ok(\"detached\")\n  }\n\n  match try run.text vcs describe {\n    Ok(text) => print $text\n    Err(_) => print $fallback $shown $branch\n  }\n  let _ = [bound is Ok(_), later is Ok(_), captured is Ok(_)]\n  return try run.text vcs describe\n}\n"
        );
        // The fixed program checks as the original did, and is clean.
        assert!(lint(&fixed).is_empty(), "{fixed}");
    }

    #[test]
    fn a_run_form_that_propagates_or_yields_a_status_is_left_alone() {
        let source = "proc describe() -> Result[Str] {\n  let text = run.text vcs describe ?\n  let lines = run.stream --text vcs log ? |> take(2) |> collect()\n  let status = run vcs diff --quiet\n  let other = run.status vcs diff --quiet\n  run vcs fetch\n  run.status vcs gc\n  let already = try run.text vcs describe\n  print $text ${lines.len()} ${status.success} ${other.success} ${already is Ok(_)}\n  Ok(text)\n}\n";
        assert!(lint(source).is_empty());
    }
}
