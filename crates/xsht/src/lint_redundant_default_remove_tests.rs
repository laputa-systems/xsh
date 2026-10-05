use super::{LintOptions, Linter};
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
        expr_types: checked.expr_types,
        ..LintOptions::default()
    };
    Linter::lint(&parsed.arena, source, options)
        .diagnostics
        .into_iter()
        .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintRedundantDefault))
        .collect()
}

// While a missing path is an error by default, `missing_ok: true` changes
// what the call does and `missing_ok: false` is what a migration writes
// before the default changes, so neither is reported.
#[test]
fn an_explicit_missing_ok_is_kept_while_the_default_is_false() {
    let source = "proc clean(stale: Path, root: Path) [fs, error] {\n  stale.remove(missing_ok: true)\n  fs.remove(root, missing_ok: true)\n  stale.remove(missing_ok: false)\n  fs.remove(root, missing_ok: false)\n}\n";
    assert!(lint(source).is_empty(), "{:?}", lint(source));
}
