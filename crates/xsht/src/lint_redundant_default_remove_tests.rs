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
            fix.replacement.as_deref().unwrap_or_default(),
        );
    }
    fixed
}

// The row and the runtime must name the same default: the fix deletes the
// argument, which is only the same call while a missing path is accepted
// without it.
#[test]
fn the_default_the_row_removes_is_the_runtime_default() {
    assert!(xsh_registry::signature::REMOVE_MISSING_OK_DEFAULT);
}

#[test]
fn a_written_missing_ok_true_is_removed() {
    let source = "proc clean(stale: Path, root: Path) [fs, error] {\n  stale.remove(missing_ok: true)\n  root.remove(missing_ok: true)\n  root.remove(\n    missing_ok: true,\n  )\n  let _ = fp\"{root}/cache\".remove(missing_ok: true)\n}\n";
    let diagnostics = lint(source);
    assert_eq!(diagnostics.len(), 4, "{diagnostics:?}");
    let fixed = apply(&diagnostics, source);
    assert_eq!(
        fixed,
        "proc clean(stale: Path, root: Path) [fs, error] {\n  stale.remove()\n  root.remove()\n  root.remove()\n  let _ = fp\"{root}/cache\".remove()\n}\n"
    );
    assert!(lint(&fixed).is_empty());
}

// `missing_ok: false` changes what the call does. A map's `remove`, a rooted
// `remove`, a user function named `remove`, and the removals a scratch
// directory or an atomic replacement expands to are other calls.
#[test]
fn other_removals_are_left_alone() {
    let source = "proc remove(stale: Path, missing_ok: Bool = false) [fs, error] {\n  stale.remove(missing_ok: false)\n  stale.remove(missing_ok: missing_ok)\n}\n\nproc clean(stale: Path, root: FsRoot, seen: Map[Int]) [fs, error] {\n  var counts = seen\n  counts = counts.remove(\"stale\")\n  root.remove(p\"cache\", dir: true)\n  remove(stale, missing_ok: true)\n  tempdir scratch at stale {\n    fp\"{scratch}/stamp\".write(\"staged\")\n  }\n  atomically replace stale as partial {\n    partial.write(f\"{counts.len()}\")\n  }\n}\n";
    assert!(lint(source).is_empty(), "{:?}", lint(source));
}
