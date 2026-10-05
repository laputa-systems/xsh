//! `lint.prefer-non-empty-argv`: a spliced command vector (`run @argv`) whose
//! type is a plain `List[T]` names no program when it is empty, and says so
//! only when it runs. Typed `NonEmpty[T]`, the same vector cannot be empty.
//!
//! The checker decides which targets these are: every spliced `run` target
//! whose type is a list that carries no validation. The lint only reports
//! them. It offers no fix, because the type has to come from somewhere: the
//! binding's annotation, the parameter's type and every caller, or a
//! `.require(NonEmpty[T])?` whose failure the author has to place.

use std::collections::BTreeMap;
use xsh::diagnostic::{Diagnostic, DiagnosticCode, Label};
use xsh::frontend::check::Type;
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{ArenaCommandArg, AstArena};

pub(super) fn prefer_non_empty_argv(
    arena: &AstArena,
    unvalidated: &BTreeMap<Span, Type>,
    target: &ArenaCommandArg,
) -> Option<Diagnostic> {
    if unvalidated.is_empty() {
        return None;
    }
    let span = arena.span(target.span);
    let Type::List(item) = unvalidated.get(&span)? else {
        return None;
    };
    Some(
        Diagnostic::warning("this command vector may be empty, which fails only when it runs")
            .with_code(DiagnosticCode::LintPreferNonEmptyArgv)
            .with_label(Label::secondary(
                span,
                format!("this is a `List[{item}]`; a `NonEmpty[{item}]` always names a program"),
            ))
            .with_note(format!(
                "declare the vector `NonEmpty[{item}]` where it is built, or validate it with `.require(NonEmpty[{item}])?`"
            )),
    )
}

#[cfg(test)]
mod tests {
    use super::super::lint_redundant_propagation::tests::lint_rule;
    use xsh::diagnostic::{Diagnostic, DiagnosticCode};

    fn lint(source: &str) -> Vec<Diagnostic> {
        lint_rule(source, DiagnosticCode::LintPreferNonEmptyArgv)
    }

    #[test]
    fn a_plain_list_spliced_as_a_command_is_reported_without_a_fix() {
        let source = "proc launch(argv: List[Str], tool: List[Path]) [process, error] {\n  run @argv\n  let out = run.text @(tool) \"--version\" ?\n  print $out\n  run.status @argv | run wc -l\n}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 3, "{diagnostics:?}");
        let reported = diagnostics
            .iter()
            .map(|diagnostic| &source[diagnostic.labels[0].span.range()])
            .collect::<Vec<_>>();
        assert_eq!(reported, ["@argv", "@(tool)", "@argv"]);
        assert_eq!(
            diagnostics[0].labels[0].message.as_deref(),
            Some("this is a `List[Str]`; a `NonEmpty[Str]` always names a program")
        );
        assert!(
            diagnostics[1].notes[0].contains("`.require(NonEmpty[Path])?`"),
            "{:?}",
            diagnostics[1].notes
        );
        // The type has to be established somewhere the lint cannot choose.
        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| diagnostic.fix_hints.is_empty())
        );
    }

    #[test]
    fn a_validated_vector_and_a_written_target_are_not_reported() {
        let source = "proc launch(argv: NonEmpty[Str], extra: List[Str]) [process, error] -> Result[Unit] {\n  run @argv\n  run @(argv.push(\"-v\"))\n  run git @extra\n  let checked = extra.require(NonEmpty[Str])?\n  run @checked\n  if extra is NonEmpty[Str] {\n    run @extra\n  }\n\n  Ok()\n}\n";
        let diagnostics = lint(source);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    // What the suggested rewrite produces checks and is not reported again.
    #[test]
    fn the_suggested_type_clears_the_report() {
        let source = "proc launch(argv: List[Str]) [process, error] {\n  run @argv\n}\n";
        assert_eq!(lint(source).len(), 1);
        assert!(lint(&source.replace("List[Str]", "NonEmpty[Str]")).is_empty());
    }
}
