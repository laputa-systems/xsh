#![allow(clippy::single_call_fn)]

use std::fs;
use std::sync::Arc;

use tempfile::TempDir;
use xsh::diagnostic::Diagnostic;
use xsh::frontend::check::Checker;
use xsh::frontend::load::parse_load_check_text;
use xsh::frontend::source::SourceId;
use xsh::frontend::symbols::{Name, SymbolOwner};
use xsh::frontend::syntax::arena::{ArenaProgram, ArenaProgramBuilder};
use xsh::frontend::syntax::parser::{ArenaParseOutput, Parser};
use xsht::format::Formatter;
use xsht::lint::{LintOptions, Linter};

#[test]
fn boolean_guard_fix_keeps_failure_body_comments_and_converges() {
    let source = "proc validate(jobs: Int) [error] {\n  if jobs <= 0 {\n    # Preserve domain error identity.\n    return error.fail(\"jobs must be positive\")\n  }\n\n  let _ = jobs\n}\n";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let diagnostics = Linter::lint(&parsed.arena, source, LintOptions {
        expr_types: checked.expr_types,
        definitely_exiting_block_spans: checked.definitely_exiting_block_spans,
        ..LintOptions::default()
    }).diagnostics;
    let diagnostic = diagnostics.iter().find(|diagnostic| diagnostic.code.as_deref() == Some("lint.boolean-guard")).expect("checked guard rewrite");
    let fix = &diagnostic.fix_hints[0];
    let mut fixed = source.to_string();
    fixed.replace_range(fix.span.unwrap().range(), fix.replacement.as_ref().unwrap());
    assert!(fixed.contains("guard jobs > 0 else {"));
    assert!(fixed.contains("# Preserve domain error identity."));
    assert_parse_check_standalone("boolean guard", &fixed);
    let formatted = Formatter::new().format_source(SourceId::new(0), &fixed);
    assert_eq!(formatted.formatted, fixed);
    let parsed = parse_lint_source(&fixed);
    let checked = Checker::check_arena(&parsed.arena, &fixed);
    let second = Linter::lint(&parsed.arena, &fixed, LintOptions {
        expr_types: checked.expr_types,
        definitely_exiting_block_spans: checked.definitely_exiting_block_spans,
        ..LintOptions::default()
    }).diagnostics;
    assert!(!second.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("lint.boolean-guard")));
}

#[test]
fn boolean_guard_float_fix_retains_nan_negation() {
    let source = "pure positive(value: Float) -> Bool {\n  if value <= 0.0 { return false }\n  true\n}\n";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let diagnostics = Linter::lint(&parsed.arena, source, LintOptions {
        expr_types: checked.expr_types,
        definitely_exiting_block_spans: checked.definitely_exiting_block_spans,
        ..LintOptions::default()
    }).diagnostics;
    let diagnostic = diagnostics.iter().find(|diagnostic| diagnostic.code.as_deref() == Some("lint.boolean-guard")).unwrap();
    let replacement = diagnostic.fix_hints[0].replacement.as_ref().unwrap();
    assert!(replacement.starts_with("guard ! (value <= 0.0) else"), "{replacement}");
    let mut fixed = source.to_string();
    fixed.replace_range(diagnostic.fix_hints[0].span.unwrap().range(), replacement);
    assert_parse_check_standalone("float boolean guard", &fixed);
}

#[test]
fn boolean_guard_fix_refuses_fallthrough_unchecked_and_binding_forms() {
    for source in [
        "proc validate(ok: Bool) [] { if ! ok { print \"fallthrough\" } }\n",
        "proc validate(ok: Bool) [] { if ! ok { return } else { return } }\n",
        "proc validate(ok: Bool) [] { let _ = ok; if ! ok { return } }\n",
        "proc validate(outcome: Result[Int]) [] { if let Err(failure) = outcome { return } }\n",
    ] {
        let parsed = parse_lint_source(source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        let options = LintOptions { expr_types: checked.expr_types, definitely_exiting_block_spans: checked.definitely_exiting_block_spans, ..LintOptions::default() };
        for options in [options, LintOptions::default()] {
            let diagnostics = Linter::lint(&parsed.arena, source, options).diagnostics;
            assert!(!diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("lint.boolean-guard")), "{source}");
        }
    }
}

fn assert_fmt_stable(source_id: SourceId, label: &str, source: &str) {
    let formatted = Formatter::new().format_source(source_id, source);
    assert!(
        formatted.diagnostics.is_empty(),
        "{label}: formatter produced diagnostics: {:?}",
        formatted.diagnostics
    );
    assert_eq!(
        formatted.formatted, source,
        "{label}: formatter changed source"
    );
}

fn parse_lint_source(source: &str) -> ArenaParseOutput {
    parse_lint_source_with_id(SourceId::new(0), source)
}

fn parse_lint_source_with_id(source_id: SourceId, source: &str) -> ArenaParseOutput {
    Parser::parse_source_arena_only(source_id, source)
}

fn lint_and_assert_fmt_stable(
    program: &ArenaProgram,
    source: &str,
    options: LintOptions,
) -> Vec<Diagnostic> {
    let diagnostics = Linter::lint(program, source, options).diagnostics;
    assert_lint_fixes_fmt_stable(program, source, &diagnostics);
    diagnostics
}

fn assert_parse_check_standalone(label: &str, source: &str) {
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(
        parsed.diagnostics.is_empty(),
        "{label}: fixed source has parse errors:\n---\n{source}\n---\n{:?}",
        parsed.diagnostics
    );
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(
        checked.diagnostics.is_empty(),
        "{label}: fixed source has check errors:\n---\n{source}\n---\n{:?}",
        checked.diagnostics
    );
}

fn assert_lint_fixes_fmt_stable(program: &ArenaProgram, source: &str, diagnostics: &[Diagnostic]) {
    let mut all_fixes = Vec::new();
    for diagnostic in diagnostics {
        for hint in &diagnostic.fix_hints {
            let (Some(span), Some(replacement)) = (hint.span, hint.replacement.as_ref()) else {
                continue;
            };
            if span.source_id != SourceId::new(0) {
                continue;
            }

            let mut fixed = source.to_string();
            fixed.replace_range(span.range(), replacement);
            let label = diagnostic.code.as_deref().unwrap_or("lint fix");
            let formatted = Formatter::new().format_source(span.source_id, &fixed);
            assert!(
                formatted.diagnostics.is_empty(),
                "{label}: formatter produced diagnostics after lint fix: {:?}",
                formatted.diagnostics
            );
            assert_eq!(
                formatted.formatted, fixed,
                "{label}: formatter changed source"
            );
            all_fixes.push((span, replacement.clone()));
        }
    }

    if program.modules.is_empty() && !all_fixes.is_empty() {
        all_fixes.sort_by_key(|(span, _)| span.start());
        let mut merged = Vec::new();
        let mut previous_end = 0usize;
        for (span, replacement) in all_fixes {
            if span.start() < previous_end {
                continue;
            }
            previous_end = span.end();
            merged.push((span, replacement));
        }

        let mut fixed = source.to_string();
        for (span, replacement) in merged.into_iter().rev() {
            fixed.replace_range(span.range(), &replacement);
        }
        assert_parse_check_standalone("all lint fixes", &fixed);
        let formatted = Formatter::new().format_source(SourceId::new(0), &fixed);
        assert!(
            formatted.diagnostics.is_empty(),
            "all lint fixes: formatter produced diagnostics: {:?}",
            formatted.diagnostics
        );
        assert_parse_check_standalone("all lint fixes after fmt", &formatted.formatted);
    }
}

#[test]
fn linter_reports_stage_12_warning_rules_deterministically() {
    let source = "\
proc main(argv: List[Str]) {
  let input = argv[0]
  let src = \"tmp\"
  let root = Path(\"target/lint\")
  let unused = 1
  let p = Path(src)
  fs.mkdir(fp\"${root}/src/lib\", parents: true)?
  run grep ${input} haystack ?

  if true {
    let src = \"other\"
    print ${src} ${argv[0]}
  }
}

main(args)?
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());
    let codes: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_deref().unwrap())
        .collect();

    assert_eq!(
        codes,
        [
            "lint.path-constructor",
            "lint.path-constructor",
            "lint.redundant-default",
            "lint.run-status",
            "lint.shadowing",
            "lint.redundant-command-interpolation",
            "lint.unused-local",
            "lint.unused-local",
            "lint.unannotated-effects",
        ]
    );
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| { diagnostic.labels.iter().any(|label| !label.span.is_empty()) })
    );
}

#[test]
fn linter_reports_missing_declared_effects_with_fix() {
    let source = "\
proc main() [fs] {
  let _ = fs.read_text(Path(\"x\"))?
}
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);

    let diagnostics = lint_and_assert_fmt_stable(
        &parsed.arena,
        source,
        LintOptions {
            callable_effects: checked.callable_effects,
            ..LintOptions::default()
        },
    );
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code.as_deref() == Some("lint.missing-effects"))
        .expect("expected missing effects lint");

    assert_eq!(diagnostic.fix_hints.len(), 1);
    assert_eq!(
        diagnostic.fix_hints[0].replacement.as_deref(),
        Some("[fs, error]")
    );
}

#[test]
fn linter_reports_missing_effects_from_called_restricted_proc() {
    let source = "\
proc timestamp() [time] -> Int {
  time.now()
}

proc main() [] -> Int {
  timestamp()
}
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);

    let diagnostics = lint_and_assert_fmt_stable(
        &parsed.arena,
        source,
        LintOptions {
            callable_effects: checked.callable_effects,
            ..LintOptions::default()
        },
    );
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code.as_deref() == Some("lint.missing-effects"))
        .expect("expected missing effects lint");

    assert_eq!(
        diagnostic.fix_hints[0].replacement.as_deref(),
        Some("[time]")
    );
}

#[test]
fn linter_reports_missing_effects_from_imported_module_proc() {
    SymbolOwner::new().with_current(|| {
        let module_source = "\
##! Kbuild lint fixture module.
## Returns a task status with an environment effect.
export proc image_task() [env] -> Int {
  1
}
";
        let main_source = "\
use kbuild

proc main() [] -> Int {
  kbuild.image_task()
}
";
        // Assemble the multi-module arena the way the loader does: parse the entry
        // and the imported module into one builder, resolve the `use`, and register
        // the module body.
        let mut builder = ArenaProgramBuilder::with_token_capacity(main_source.len() / 4 + 1);
        let root =
            Parser::parse_source_into_arena_builder(SourceId::new(0), main_source, &mut builder);
        assert!(root.diagnostics.is_empty(), "{:?}", root.diagnostics);
        let module =
            Parser::parse_source_into_arena_builder(SourceId::new(1), module_source, &mut builder);
        assert!(module.diagnostics.is_empty(), "{:?}", module.diagnostics);
        for stmt in builder.statement_ids(root.statements) {
            if let Some((use_id, _path, _span)) = builder.use_stmt_for_statement(stmt) {
                builder.set_use_resolved(use_id, Arc::from("kbuild"));
            }
        }
        builder.push_arena_module(
            "kbuild".to_string(),
            Name::intern("kbuild"),
            module.statements,
        );
        let arena = builder.finish_with_statements(root.statements);
        let checked = Checker::check_arena(&arena, main_source);

        let diagnostics = lint_and_assert_fmt_stable(
            &arena,
            main_source,
            LintOptions {
                callable_effects: checked.callable_effects,
                ..LintOptions::default()
            },
        );
        let diagnostic = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code.as_deref() == Some("lint.missing-effects"))
            .expect("expected missing effects lint");

        assert_eq!(
            diagnostic.fix_hints[0].replacement.as_deref(),
            Some("[env]")
        );
    });
}

#[test]
fn linter_reports_named_underscore_locals_but_allows_sink_binding() {
    let source = "\
proc main() {
  let _ = 1
  let _unused = 2
}

main()?
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());
    let messages: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.as_str())
        .collect();

    assert_eq!(messages, ["unused local variable `_unused`"]);
}

#[test]
fn linter_marks_display_string_interpolation_as_used() {
    let source = "\
proc main() {
  let dir = \"tmp\"
  let unused = \"never read\"
  print f\"dir=$dir\"
}
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.code.as_deref() == Some("lint.unused-local")
                && diagnostic.message.contains("`unused`")
        }),
        "genuinely unused local should still be reported: {diagnostics:?}"
    );
    assert!(
        !diagnostics.iter().any(|diagnostic| {
            diagnostic.code.as_deref() == Some("lint.unused-local")
                && diagnostic.message.contains("`dir`")
        }),
        "display-string interpolation should count as a use: {diagnostics:?}"
    );
}

#[test]
fn linter_marks_indexed_assignment_keys_as_used() {
    let source = "\
proc main() {
  let key = \"name\"
  var output: Map[Int] = {}
  output[key] = 1
}
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());

    assert!(
        !diagnostics.iter().any(|diagnostic| {
            diagnostic.code.as_deref() == Some("lint.unused-local")
                && diagnostic.message.contains("`key`")
        }),
        "indexed assignment key should count as a use: {diagnostics:?}"
    );
}

#[test]
fn linter_reports_redundant_result_unit_ceremony() {
    let source = "\
proc helper() -> Result[Unit] {
  return Ok()
}

export proc public() -> Result[Unit] {
  return Ok()
}
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());
    let codes: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_deref().unwrap())
        .collect();

    assert_eq!(
        codes,
        [
            "lint.redundant-result-unit",
            "lint.redundant-ok-return",
            "lint.redundant-ok-return",
            "lint.unused-callable",
        ]
    );
}

#[test]
fn linter_autofixes_redundant_tail_ok_return() {
    let source = "\
proc parsed(value: Int) -> Result[Int] {
  return Ok(value + 1)
}
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    );
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code.as_deref() == Some("lint.redundant-ok-tail"))
        .expect("expected redundant tail Ok diagnostic");

    assert_eq!(
        diagnostic
            .fix_hints
            .first()
            .and_then(|hint| hint.replacement.as_deref()),
        Some("value + 1\n")
    );
}

#[test]
fn linter_warns_for_redundant_tail_ok_return_without_type_info() {
    let source = "\
proc parsed(value: Int) -> Result[Int] {
  return Ok(value + 1)
}
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code.as_deref() == Some("lint.redundant-ok-tail"))
        .expect("expected redundant tail Ok diagnostic");

    assert!(diagnostic.fix_hints.is_empty());
}

#[test]
fn linter_autofixes_redundant_tail_return_binding() {
    let source = "\
proc overlap(left: List[Str], right: List[Str]) -> List[Str] {
  var values = [item for item in left if right.contains(item)]
  return values
}
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code.as_deref() == Some("lint.redundant-tail-return-binding"))
        .expect("expected redundant tail return binding diagnostic");
    let hint = diagnostic
        .fix_hints
        .first()
        .expect("tail return binding has a fix");

    assert_eq!(
        hint.replacement.as_deref(),
        Some("[item for item in left if right.contains(item)]\n")
    );
}

#[test]
fn linter_does_not_autofix_tail_return_binding_across_comment() {
    let source = "\
pure value() -> Int {
  let answer = 42
  # name the value while debugging this calculation
  return answer
}
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code.as_deref() == Some("lint.redundant-tail-return-binding"))
        .expect("expected redundant tail return binding diagnostic");

    assert!(diagnostic.fix_hints.is_empty());
}

#[test]
fn linter_autofixes_typed_empty_list_tail_return_binding() {
    let source = "\
pure values() -> List[Str] {
  let items: List[Str] = []
  return items
}
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code.as_deref() == Some("lint.redundant-tail-return-binding"))
        .expect("expected redundant tail return binding diagnostic");

    assert_eq!(
        diagnostic
            .fix_hints
            .first()
            .and_then(|hint| hint.replacement.as_deref()),
        Some("[]\n")
    );
}

#[test]
fn linter_autofixes_typed_tail_return_bindings_when_initializer_already_matches() {
    let source = "\
pure values(items: List[Str]) -> List[Str] {
  let out: List[Str] = items
  return out
}
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    );
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code.as_deref() == Some("lint.redundant-tail-return-binding"))
        .expect("expected redundant tail return binding diagnostic");

    assert_eq!(
        diagnostic
            .fix_hints
            .first()
            .and_then(|hint| hint.replacement.as_deref()),
        Some("items\n")
    );
}

#[test]
fn linter_counts_a_record_type_annotation_as_a_type_use() {
    let source =
        "type Accum = {total: Int, out: List[Str]}\nlet initial: Accum = {total: 0, out: []}\n";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());
    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code.as_deref() == Some("lint.unused-type") }),
        "record annotation must use its declared type: {diagnostics:?}"
    );
}

#[test]
fn linter_does_not_suggest_unparseable_tail_return_for_typed_records() {
    let source = "\
type Item = {name: Str, active: Bool, count: Int}

proc convert(value: Str) -> Item {
  let item: Item = {name: value, active: true, count: 1}
  return item
}

proc convert_all(values: List[Str]) -> List[Item] {
  return values |> map { |value|
    let item: Item = {name: value, active: true, count: 1}
    item
  } |> collect()
}
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());
    assert!(!diagnostics.iter().any(|diagnostic| {
        diagnostic.code.as_deref() == Some("lint.redundant-tail-return-binding")
    }));
}

#[test]
fn linter_autofixes_single_newline_triple_string() {
    let source = "\
let newline = \"\"\"


\"\"\"

let sample = \"\"\"alpha
beta\"\"\"
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());
    let newline_fixes: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.code.as_deref() == Some("lint.redundant-newline-triple-string")
        })
        .map(|diagnostic| {
            diagnostic.fix_hints[0]
                .replacement
                .as_deref()
                .expect("newline triple-string diagnostic has replacement")
        })
        .collect();

    assert_eq!(newline_fixes, ["\"\\n\""]);
}

#[test]
fn formatter_preserves_single_newline_triple_string_lint_fix() {
    let source = "\
let newline = \"\"\"


\"\"\"
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic.code.as_deref() == Some("lint.redundant-newline-triple-string")
        })
        .expect("expected newline triple-string diagnostic");
    let hint = diagnostic
        .fix_hints
        .first()
        .expect("newline triple-string diagnostic has fix hint");
    let replacement = hint
        .replacement
        .as_ref()
        .expect("newline triple-string diagnostic has replacement");
    let span = hint
        .span
        .expect("newline triple-string diagnostic has replacement span");

    let mut fixed = source.to_string();
    fixed.replace_range(span.range(), replacement);
    assert_eq!(fixed, "let newline = \"\\n\"\n");
    assert_fmt_stable(SourceId::new(0), "newline triple-string lint fix", &fixed);
}

#[test]
fn linter_autofixes_redundant_path_display_parse_roundtrips() {
    let source = "\
proc parsed(root: Path, value: Str) -> Path {
  return Path(fp\"${root}/${value}\".display())
}

proc main(root: Path, value: Str) [error] {
  let direct = Path(fp\"${root}/${value}\".display())
  let nested = Path(fp\"${root}/${value}\".display())
  print ${direct} ${nested}
}
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    );
    let path_parse_diagnostics: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.as_deref() == Some("lint.redundant-path-parse"))
        .collect();

    assert_eq!(
        path_parse_diagnostics.len(),
        3,
        "diagnostics: {diagnostics:?}"
    );

    let replacements: Vec<_> = path_parse_diagnostics
        .iter()
        .map(|diagnostic| {
            diagnostic.fix_hints[0]
                .replacement
                .as_deref()
                .expect("path parse lint has replacement")
        })
        .collect();
    assert_eq!(
        replacements,
        [
            "fp\"${root}/${value}\"",
            "fp\"${root}/${value}\"",
            "fp\"${root}/${value}\"",
        ]
    );
}

#[test]
fn linter_autofixes_redundant_type_driven_roundtrips() {
    let source = "\
type Row = {name: Str}

proc main(root: Path, name: Str, row: Row, count: Int, ratio: Float) [error] {
  let parsed_literal = Path(\"tmp/out\")
  let parsed_fmt = Path(f\"${root}/${name}\")
  let constructed_fmt = Path(f\"${root}/${name}\")
  let same_path = fp\"${root}\"
  let same_name = f\"${name}\"
  let same_row = row.require(Row)?
  let raw: Any = {name}
  let checked_row = raw.require(Row)?
  let same_count = f\"${count}\".parse_int()?
  let same_ratio = f\"${ratio}\".parse_float()?
  print ${parsed_literal} ${parsed_fmt} ${constructed_fmt} ${same_path} ${same_name} ${same_row.name} ${checked_row.name} ${same_count} ${same_ratio}
}
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    );

    let fixes_for = |code: &str| -> Vec<&str> {
        diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code.as_deref() == Some(code))
            .map(|diagnostic| {
                diagnostic.fix_hints[0]
                    .replacement
                    .as_deref()
                    .expect("diagnostic has replacement")
            })
            .collect()
    };

    assert_eq!(
        fixes_for("lint.path-constructor"),
        [
            "p\"tmp/out\"",
            "fp\"${root}/${name}\"",
            "fp\"${root}/${name}\"",
        ]
    );
    assert_eq!(fixes_for("lint.redundant-path-interpolation"), ["root"]);
    assert_eq!(fixes_for("lint.redundant-string-interpolation"), ["name"]);
    assert_eq!(fixes_for("lint.redundant-require"), ["row"]);
    assert_eq!(
        fixes_for("lint.redundant-display-parse"),
        ["count", "ratio"]
    );
}

#[test]
fn linter_autofixes_single_value_command_fstrings() {
    let source = "\
proc main(manifest: Path, name: Str) {
  print f\"${manifest.display()}\"
  print f\"${name}\"
}
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    );
    let replacements: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.as_deref() == Some("lint.redundant-command-fmt"))
        .map(|diagnostic| {
            diagnostic.fix_hints[0]
                .replacement
                .as_deref()
                .expect("command f-string diagnostic has replacement")
        })
        .collect();
    assert_eq!(replacements, ["$manifest", "$name"]);
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code.as_deref()
                != Some("lint.redundant-string-interpolation")),
        "diagnostics: {diagnostics:?}"
    );
}

#[test]
fn linter_autofixes_redundant_command_interpolations_for_run_args() {
    let source = "\
proc main(name: Str) {
  run echo ${name.lower()}
}
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());
    let replacements: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.code.as_deref() == Some("lint.redundant-command-interpolation")
        })
        .map(|diagnostic| {
            diagnostic.fix_hints[0]
                .replacement
                .as_deref()
                .expect("command interpolation diagnostic has replacement")
        })
        .collect();
    assert_eq!(replacements, ["name.lower()"]);
}

#[test]
fn linter_reports_redundant_json_and_stream_roundtrips() {
    let source = "\
proc main() [error] {
  let normalized = json.decode(json.encode({name: \"pkg\"})?)?
  let values = [1, 2, 3] |> where true |> map .
  print ${normalized}
}
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    );
    let json_count = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.as_deref() == Some("lint.json-roundtrip"))
        .count();
    let stream_fixes: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.as_deref() == Some("lint.redundant-pipeline-stage"))
        .map(|diagnostic| {
            diagnostic.fix_hints[0]
                .replacement
                .as_deref()
                .expect("pipeline diagnostic has replacement")
        })
        .collect();

    assert_eq!(json_count, 1, "diagnostics: {diagnostics:?}");
    assert_eq!(stream_fixes, ["", ""]);
}

#[test]
fn linter_reports_unsorted_import_blocks_with_fix() {
    let source = "\
use json
use env
use fs
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code.as_deref() == Some("lint.unsorted-imports"))
        .expect("expected unsorted imports diagnostic");
    let replacement = diagnostic
        .fix_hints
        .first()
        .and_then(|hint| hint.replacement.as_deref())
        .expect("expected import sorting fix");

    assert_eq!(replacement, "use env\nuse fs\nuse json\n");
}

#[test]
fn linter_sorts_multiple_import_groups_independently() {
    let source = "\
use json
use fs
let divider = 1
use time
use env
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());
    let replacements = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.as_deref() == Some("lint.unsorted-imports"))
        .map(|diagnostic| {
            diagnostic.fix_hints[0]
                .replacement
                .as_deref()
                .unwrap()
                .to_string()
        })
        .collect::<Vec<_>>();

    assert_eq!(replacements, ["use fs\nuse json\n", "use env\nuse time\n"]);
}

#[test]
fn linter_warns_for_commented_import_blocks_without_fix() {
    let source = "\
use zeta
# keep this import near zeta for now
use alpha
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code.as_deref() == Some("lint.unsorted-imports"))
        .expect("expected unsorted imports diagnostic");

    assert!(diagnostic.fix_hints.is_empty());
}

#[test]
fn linter_reports_top_level_const_order_without_default_fix() {
    let source = "\
pure helper() -> Int {
  return 1
}

let answer = 42
let dynamic = answer
let status = run.status true
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());
    let const_diagnostics = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.as_deref() == Some("lint.organize-top-level-consts"))
        .collect::<Vec<_>>();

    assert_eq!(const_diagnostics.len(), 1);
    assert!(const_diagnostics[0].fix_hints.is_empty());
}

#[test]
fn linter_suggests_list_comprehension_for_accumulation_loop() {
    let source = "\
type Item = {name: Str}

let items: List[Item] = []
var names: List[Str] = []
for item in items {
  names = names.push(item[\"name\"])
}
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());
    let codes: Vec<_> = diagnostics
        .iter()
        .filter_map(|d| d.code.as_deref())
        .collect();
    assert!(
        codes.contains(&"lint.prefer-list-comp"),
        "expected lint.prefer-list-comp in {codes:?}"
    );
    let hint = diagnostics
        .iter()
        .find(|d| d.code.as_deref() == Some("lint.prefer-list-comp"))
        .and_then(|d| d.fix_hints.first())
        .expect("fix hint present");
    assert!(hint.replacement.is_some(), "fix hint has replacement");
}

#[test]
fn linter_suggests_guarded_list_comprehension_for_guarded_accumulation_loop() {
    let source = "\
let items: List[Str] = []
var names: List[Str] = []
for item in items {
  if item != \"\" {
    names = names.push(item.trim())
  }
}
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());
    let hint = diagnostics
        .iter()
        .find(|d| d.code.as_deref() == Some("lint.prefer-list-comp"))
        .and_then(|d| d.fix_hints.first())
        .expect("guarded accumulation has list-comprehension fix");

    assert_eq!(
        hint.replacement.as_deref(),
        Some("var names: List[Str] = [item.trim() for item in items if item != \"\"]\n")
    );
}

#[test]
fn linter_does_not_rewrite_branching_list_accumulation_loop() {
    let source = "\
let items: List[Str] = []
var names: List[Str] = []
for item in items {
  if item != \"\" {
    names = names.push(item.trim())
  } else {
    names = names.push(\"missing\")
  }
}
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());

    assert!(
        !diagnostics
            .iter()
            .any(|d| d.code.as_deref() == Some("lint.prefer-list-comp")),
        "branching accumulation should not get a list-comprehension fix: {diagnostics:?}"
    );
}

#[test]
fn linter_does_not_rewrite_unique_accumulation_loop() {
    let source = "\
let items: List[Int] = []
var unique: List[Int] = []
for item in items {
  if ! unique.contains(item) {
    unique = unique.push(item)
  }
}
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());

    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-list-comp")),
        "unique accumulation depends on the accumulator: {diagnostics:?}"
    );
}

#[test]
fn linter_suggests_map_comprehension_for_map_building_loop() {
    let source = "\
let buckets = [{key: \"pkg\", items: [\"one\"]}]
var by_key: Map[List[Str]] = map.empty()

for bucket in buckets {
  by_key[bucket.key] = bucket.items
}
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());
    let hint = diagnostics
        .iter()
        .find(|d| d.code.as_deref() == Some("lint.prefer-map-comp"))
        .and_then(|d| d.fix_hints.first())
        .expect("map-building loop has map-comprehension fix");

    assert_eq!(
        hint.replacement.as_deref(),
        Some("var by_key: Map[List[Str]] = {bucket.key: bucket.items for bucket in buckets}\n")
    );
}

#[test]
fn linter_suggests_empty_map_literal_for_map_empty() {
    let source = "\
let counts: Map[Int] = map.empty()
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let diagnostics = lint_and_assert_fmt_stable(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    );
    let hint = diagnostics
        .iter()
        .find(|d| d.code.as_deref() == Some("lint.prefer-empty-map-literal"))
        .and_then(|d| d.fix_hints.first())
        .expect("map.empty has empty-map literal fix");

    assert_eq!(hint.replacement.as_deref(), Some("{}"));
}

#[test]
fn linter_suggests_stream_producer_for_proc_list_accumulator() {
    let source_without_lazy_consumer = "\
proc rows(items: List[Str]) [error] -> Result[List[Str]] {
  var out: List[Str] = []

  for item in items {
    if item != \"\" {
      out = out.push(item)
    }
  }

  return out |> sort-by .
}

pure pure_rows(items: List[Str]) -> List[Str] {
  var out: List[Str] = []

  for item in items {
    out = out.push(item)
  }

  return out
}
";
    let parsed = parse_lint_source(source_without_lazy_consumer);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let diagnostics = lint_and_assert_fmt_stable(
        &parsed.arena,
        source_without_lazy_consumer,
        LintOptions::default(),
    );
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.code.as_deref() == Some("lint.prefer-stream-producer")),
        "definition alone should not warn: {diagnostics:?}"
    );

    let source = format!("{source_without_lazy_consumer}\nlet count = rows([\"a\"])? |> count()\n");
    let parsed = parse_lint_source(&source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, &source, LintOptions::default());
    let stream_warnings = diagnostics
        .iter()
        .filter(|d| d.code.as_deref() == Some("lint.prefer-stream-producer"))
        .count();
    assert_eq!(stream_warnings, 1, "diagnostics: {diagnostics:?}");
}

#[test]
fn linter_suggests_string_concat_over_join_empty() {
    let source = r#"let x = ["a", "b"].join("")
"#;
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());
    let codes: Vec<_> = diagnostics
        .iter()
        .filter_map(|d| d.code.as_deref())
        .collect();
    assert!(
        codes.contains(&"lint.prefer-string-concat"),
        "expected lint.prefer-string-concat in {codes:?}"
    );
    let hint = diagnostics
        .iter()
        .find(|d| d.code.as_deref() == Some("lint.prefer-string-concat"))
        .and_then(|d| d.fix_hints.first())
        .expect("fix hint present");
    let replacement = hint.replacement.as_ref().expect("fix hint has replacement");
    assert_eq!(replacement, "\"a\" + \"b\"");
}

#[test]
fn linter_reports_dead_code_after_all_returning_match() {
    let source = "\
enum Tok { TOp(Str), TEOF }

pure is_op(t: Tok, name: Str) -> Bool {
  match t {
    TOp(s) => return s == name
    _ => return false
  }
  return false
}
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());
    let codes: Vec<_> = diagnostics
        .iter()
        .filter_map(|d| d.code.as_deref())
        .collect();
    assert!(
        codes.contains(&"lint.dead-code"),
        "expected lint.dead-code in {codes:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message == "unreachable code")
    );
}

#[test]
fn linter_reports_dead_code_after_return() {
    let source = "\
proc work() {
  return
  print \"never\"
}
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());
    let dead_code = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.as_deref() == Some("lint.dead-code"))
        .collect::<Vec<_>>();
    assert_eq!(dead_code.len(), 1, "diagnostics: {diagnostics:?}");
    assert_eq!(dead_code[0].message, "unreachable code");
}

#[test]
fn linter_suggests_multiline_tag_union() {
    let source = "enum Tok { A, B, C, D, E }\n";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());
    let codes: Vec<_> = diagnostics
        .iter()
        .filter_map(|d| d.code.as_deref())
        .collect();
    assert!(
        codes.contains(&"lint.multiline-tag-union"),
        "expected lint.multiline-tag-union in {codes:?}"
    );
    let hint = diagnostics
        .iter()
        .find(|d| d.code.as_deref() == Some("lint.multiline-tag-union"))
        .and_then(|d| d.fix_hints.first())
        .expect("fix hint present");
    let replacement = hint.replacement.as_ref().expect("fix hint has replacement");
    assert!(
        replacement.contains('\n'),
        "replacement should be multi-line"
    );
    let fix_span = hint.span.expect("fix hint has span");
    assert!(
        fix_span.start() < fix_span.end(),
        "fix span should be non-empty"
    );
    // The replacement applied to source text should produce the expected result
    let mut fixed = source.to_string();
    fixed.replace_range(fix_span.start()..fix_span.end(), replacement);
    assert!(
        fixed.contains("enum Tok {\n"),
        "fixed text should contain multiline enum declaration, got:\n{fixed}"
    );
}

#[test]
fn linter_autofixes_redundant_path_display_in_command_args() {
    let source = "\
proc main(foo: Path) {
  print (foo.display())
  print $foo.display()
  print ${foo.display()}
  print foo.display()
}
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = Linter::lint(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    )
    .diagnostics;

    let path_display_diagnostics: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.as_deref() == Some("lint.redundant-path-display"))
        .collect();

    assert_eq!(
        path_display_diagnostics.len(),
        4,
        "expected 4 lint.redundant-path-display diagnostics, got {path_display_diagnostics:?}; all: {diagnostics:?}"
    );

    // Check fix replacements
    let fixes: Vec<(&str, &str)> = path_display_diagnostics
        .iter()
        .map(|d| {
            let hint = &d.fix_hints[0];
            let replacement = hint.replacement.as_deref().expect("fix has replacement");
            let span = hint.span.expect("fix has span");
            (&source[span.start()..span.end()], replacement)
        })
        .collect();
    // (foo.display()) → expr span replaced with "foo" (explicit typed, keeps parens)
    assert_eq!(fixes[0].0, "foo.display()");
    assert_eq!(fixes[0].1, "foo");
    // $foo.display() → shorthand expr span includes $, replace with "$foo"
    assert_eq!(fixes[1].0, "$foo.display()");
    assert_eq!(fixes[1].1, "$foo");
    // ${foo.display()} → arg span becomes "$foo" (combined fix)
    assert_eq!(fixes[2].0, "${foo.display()}");
    assert_eq!(fixes[2].1, "$foo");
    // foo.display() → implicit typed arg replaced with "$foo"
    assert_eq!(fixes[3].0, "foo.display()");
    assert_eq!(fixes[3].1, "$foo");
}

#[test]
fn linter_autofixes_needless_str_annotation() {
    let source = "\
let name: Str = \"pkg\"
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    );
    let diagnostic = diagnostics
        .iter()
        .find(|d| d.code.as_deref() == Some("lint.needless-annotation"))
        .expect("expected needless-annotation diagnostic");

    assert_eq!(
        diagnostic
            .fix_hints
            .first()
            .and_then(|hint| hint.replacement.as_deref()),
        Some(""),
        "fix hint should delete the annotation"
    );
}

#[test]
fn linter_autofixes_needless_scalar_annotations() {
    let source = "\
let ok: Bool = true
let count: Int = 1
let ratio: Float = 3.14
let root: Path = p\"src\"
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    );
    let needless: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code.as_deref() == Some("lint.needless-annotation"))
        .collect();
    assert_eq!(needless.len(), 4, "expected 4 needless diagnostics");
    for d in &needless {
        assert!(
            !d.fix_hints.is_empty(),
            "every needless diagnostic should have a fix hint"
        );
    }
}

#[test]
fn linter_autofixes_needless_list_annotations() {
    let source = "\
let deps: List[Str] = [\"musl\"]
var argv: List[Str] = [\"cc\", \"-O2\"]
let paths: List[Path] = [p\"src/main.c\", p\"lib/foo.c\"]
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    );
    let needless: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code.as_deref() == Some("lint.needless-annotation"))
        .collect();
    assert_eq!(needless.len(), 3, "expected 3 needless list diagnostics");
}

#[test]
fn linter_autofixes_needless_export_str_annotation() {
    let source = "\
export let rel: Str = \"1\"
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    );
    let diagnostic = diagnostics
        .iter()
        .find(|d| d.code.as_deref() == Some("lint.needless-annotation"))
        .expect("expected needless-annotation diagnostic for exported binding");
    assert!(!diagnostic.fix_hints.is_empty());
}

#[test]
fn linter_skips_needless_for_method_call_initializer() {
    let source = "\
let name: Str = metadata.get(\"name\")?
";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);

    let diagnostics = lint_and_assert_fmt_stable(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    );
    let needless: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code.as_deref() == Some("lint.needless-annotation"))
        .collect();
    assert!(needless.is_empty(), "should not lint dynamic initializers");
}

#[test]
fn linter_skips_needless_for_module_call_initializer() {
    let source = "\
let rows: List[Record] = json.read(index)?
";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);

    let diagnostics = lint_and_assert_fmt_stable(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    );
    let needless: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code.as_deref() == Some("lint.needless-annotation"))
        .collect();
    assert!(
        needless.is_empty(),
        "should not lint module call initializers"
    );
}

#[test]
fn linter_skips_needless_for_empty_list_initializer() {
    let source = "\
type Entry = {name: Str}
var entries: List[Entry] = []
";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);

    let diagnostics = lint_and_assert_fmt_stable(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    );
    let needless: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code.as_deref() == Some("lint.needless-annotation"))
        .collect();
    assert!(
        needless.is_empty(),
        "should not lint empty list initializers"
    );
}

#[test]
fn linter_skips_needless_for_proc_params() {
    let source = "\
proc main(...argv: List[Str]) [error] {}
";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);

    let diagnostics = lint_and_assert_fmt_stable(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    );
    let needless: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code.as_deref() == Some("lint.needless-annotation"))
        .collect();
    assert!(needless.is_empty(), "should never lint proc parameters");
}

#[test]
fn linter_autofixes_needless_var_annotation() {
    let source = "\
var count: Int = 0
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    );
    let diagnostic = diagnostics
        .iter()
        .find(|d| d.code.as_deref() == Some("lint.needless-annotation"))
        .expect("expected needless-annotation diagnostic for var binding");
    assert!(!diagnostic.fix_hints.is_empty());
}

#[test]
fn linter_skips_needless_for_dynamic_try_initializer() {
    let source = "\
let name: Str = getenv(\"X\")?.display()
";
    // Note: this won't check cleanly, but we just want to verify the lint
    // doesn't fire for expressions involving Try + Field access
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);

    let diagnostics = Linter::lint(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    )
    .diagnostics;
    let needless: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code.as_deref() == Some("lint.needless-annotation"))
        .collect();
    assert!(needless.is_empty(), "should not lint dynamic initializers");
}

#[test]
fn linter_needless_annotation_fix_preserves_source() {
    let source = "\
let name: Str = \"pkg\"
let deps: List[Str] = [\"musl\", \"zlib\"]
var argv: List[Str] = [\"cc\", \"-O2\"]
let source_path: Path = p\"src/main.c\"
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    );
    let needless: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code.as_deref() == Some("lint.needless-annotation"))
        .collect();
    assert_eq!(
        needless.len(),
        4,
        "expected 4 needless annotation diagnostics"
    );
}

#[test]
fn linter_autofixes_contains_membership_to_in() {
    let source = "\
proc main(names: List[Str], name: Str, text: Str, source_path: Path) {
  if names.contains(name) {}
  if ! names.contains(name) {}
  if [\"a\", \"b\"].contains(name) {}
  if text.contains(\"needle\") {}
  if source_path.display().contains(\"/\") {}
}
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    );
    let replacements: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code.as_deref() == Some("lint.prefer-in"))
        .map(|d| {
            d.fix_hints
                .first()
                .and_then(|hint| hint.replacement.as_deref())
                .expect("prefer-in diagnostic has replacement")
        })
        .collect();

    assert_eq!(
        replacements,
        [
            "name in names",
            "name not in names",
            "name in [\"a\", \"b\"]",
            "\"needle\" in text",
            "\"/\" in source_path.display()",
        ]
    );
}

#[test]
fn linter_skips_contains_to_in_when_rewrite_could_reorder_effects() {
    let source = "\
proc main(source_path: Path, names: List[Str]) [fs, error] {
  if fs.read_text(source_path)?.contains(\"needle\") {}
  if names.contains(fs.read_text(source_path)?) {}
}
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    );
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.code.as_deref() == Some("lint.prefer-in")),
        "effectful contains calls should not be autofixed"
    );
}

#[test]
fn linter_warns_for_dollar_lookalike_in_expression_string() {
    let source = "\
let body = \"hello\"
let line = \"tags: $body\"
print $line
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());
    let dollar: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code.as_deref() == Some("lint.dollar-in-expression-string"))
        .collect();
    assert_eq!(dollar.len(), 1);
    assert!(
        dollar[0].message.contains("`$body` is literal text"),
        "unexpected message: {}",
        dollar[0].message
    );
    assert!(
        dollar[0].labels[0]
            .message
            .as_deref()
            .is_some_and(|message| message.contains("interpolate `body`")),
        "unexpected label: {:?}",
        dollar[0].labels[0].message
    );
}

#[test]
fn linter_skips_interpolating_string_and_literal_dollar_contexts() {
    let source = "\
let body = \"hello\"
let escaped = \"literal \\$body\"
let raw = r\"$body\"
let fmt = f\"tags: ${body}\"
print \"tags: $body\" $escaped $raw $fmt
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.code.as_deref() == Some("lint.dollar-in-expression-string")),
        "command-word interpolation, escaped dollars, raw strings, and f-strings must not warn: {diagnostics:?}"
    );
}

#[test]
fn linter_skips_unbound_dollar_lookalikes_in_expression_string() {
    let source = "\
let note = \"home: $HOME cost: $5 template: $unbound and $field.field\"
print $note
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.code.as_deref() == Some("lint.dollar-in-expression-string")),
        "dollar lookalikes that do not name a binding should not warn: {diagnostics:?}"
    );
}

#[test]
fn linter_warns_for_dollar_lookalike_in_triple_quoted_and_parenthesized_expressions() {
    let source = "\
let body = \"hello\"
let block = \"\"\"line one
tags: $body
line three\"\"\"
print (\"tags: $body\") $block
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());
    let dollar: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code.as_deref() == Some("lint.dollar-in-expression-string"))
        .collect();
    assert_eq!(dollar.len(), 2);
    assert!(
        dollar
            .iter()
            .all(|d| d.message.contains("`$body` is literal text"))
    );
}

#[test]
fn linter_reports_one_dead_region_after_loop_branches_exit() {
    let source = "\
proc work(stop: Bool) {
  loop {
    if stop {
      break
    } else {
      return
    }
    print \"unreachable\"
    print \"also unreachable\"
  }
  print \"reachable\"
}
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());
    let dead_code = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.as_deref() == Some("lint.dead-code"))
        .collect::<Vec<_>>();
    assert_eq!(dead_code.len(), 1, "diagnostics: {diagnostics:?}");
    assert_eq!(
        dead_code[0].labels[0].span.start(),
        source.find("print \"unreachable\"").unwrap()
    );
}

#[test]
fn linter_keeps_following_code_reachable_after_conditional_exit_and_zero_iteration_loop() {
    let source = "\
proc work(stop: Bool) {
  return when stop
  while stop {
    return
  }
  print \"reachable\"
}
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());
    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.as_deref() == Some("lint.dead-code")),
        "diagnostics: {diagnostics:?}"
    );
}

#[test]
fn linter_keeps_code_after_loop_break_reachable_but_reports_dead_loop_body() {
    let source = "\
proc main(stop: Bool) {
  loop {
    if stop {
      break
    } else {
      continue
    }
    print \"dead in loop body\"
  }
  print \"reachable after break\"
}
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());
    let dead_code = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.as_deref() == Some("lint.dead-code"))
        .collect::<Vec<_>>();
    assert_eq!(dead_code.len(), 1, "diagnostics: {diagnostics:?}");
    assert!(dead_code.iter().any(|diagnostic| {
        diagnostic.labels[0].span.start() == source.find("print \"dead in loop body\"").unwrap()
    }));
}

#[test]
fn linter_reports_dead_code_after_match_without_a_normal_no_arm_path() {
    let source = "\
proc main(choice: Int) {
  match choice {
    1 => return
  }
  print \"unreachable\"
}
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.as_deref() == Some("lint.dead-code")),
        "diagnostics: {diagnostics:?}"
    );
}

#[test]
fn linter_reports_dead_code_after_all_with_paths_exit() {
    let source = "\
proc work() {
  with value = Ok(1) {
    return
  } else {
    return
  }
  print \"unreachable\"
}
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.as_deref() == Some("lint.dead-code")),
        "diagnostics: {diagnostics:?}"
    );
}

#[test]
fn linter_reports_dead_code_after_checker_proven_abort() {
    let source = "\
proc main() {
  abort(0)
  print \"unreachable\"
}
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(
        &parsed.arena,
        source,
        LintOptions {
            terminating_call_spans: checked.terminating_call_spans,
            ..LintOptions::default()
        },
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.as_deref() == Some("lint.dead-code")),
        "diagnostics: {diagnostics:?}"
    );
}

#[test]
fn linter_reports_unused_callable_but_keeps_main_calls_and_dynamic_references_live() {
    let source = "\
pure direct() -> Str {
  return \"direct\"
}

pure dynamic() -> Str {
  return \"dynamic\"
}

pure unused() -> Str {
  return \"unused\"
}

proc main() {
  let callback = dynamic
  print direct() callback.call()
}
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());
    let unused: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.as_deref() == Some("lint.unused-callable"))
        .collect();
    assert_eq!(unused.len(), 1, "diagnostics: {diagnostics:?}");
    assert!(unused[0].message.contains("`unused`"));
}

#[test]
fn linter_can_disable_dead_code_diagnostics_without_disabling_other_lints() {
    let source = "\
pure unused() -> Str {
  return \"unused\"
}

proc main() {
  let unused = 1
}

proc dead() {
  return
  print \"dead\"
}
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(
        &parsed.arena,
        source,
        LintOptions {
            dead_code: false,
            ..LintOptions::default()
        },
    );
    assert!(
        !diagnostics.iter().any(|diagnostic| {
            matches!(
                diagnostic.code.as_deref(),
                Some("lint.dead-code") | Some("lint.unused-callable")
            )
        }),
        "diagnostics: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.as_deref() == Some("lint.unused-local")),
        "non-dead-code lint should remain active: {diagnostics:?}"
    );
}

#[test]
fn linter_follows_declared_callable_resolution_before_local_bindings() {
    let source = "\
pure helper() -> Str {
  return \"callable\"
}

proc main() {
  let helper = 1
  print helper()
}
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());
    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.as_deref() == Some("lint.unused-callable")),
        "diagnostics: {diagnostics:?}"
    );
}

#[test]
fn linter_keeps_native_tests_exports_and_recursive_callables_live() {
    let source = "\
export pure public_api() -> Int {
  return helper()
}

pure helper() -> Int {
  return recursive_a()
}

pure recursive_a() -> Int {
  return recursive_b()
}

pure recursive_b() -> Int {
  return 1
}

test test_callable_roots {
  print public_api()
}
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());
    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.as_deref() == Some("lint.unused-callable")),
        "diagnostics: {diagnostics:?}"
    );
}

#[test]
fn linter_reports_unused_module_callable_without_reporting_exported_module_api() {
    let root = TempDir::new().expect("create temp project");
    let entry = root.path().join("main.xsh");
    let helper = root.path().join("helper.xsh");
    fs::write(
        &helper,
        "\
##! Helper reachability fixture module.
## Exposes the reachable public API.
export pure public_api() -> Int {
  return private_helper()
}

pure private_helper() -> Int {
  return 1
}

pure unused_helper() -> Int {
  return 2
}
",
    )
    .expect("write helper module");
    let source = "\
use helper

proc main() {
  print helper.public_api()
}
";
    let checked_entry = parse_load_check_text(
        entry.to_str().expect("utf-8 entry path"),
        source.to_string(),
        Vec::new(),
        Default::default(),
    );
    assert!(
        checked_entry.parsed.diagnostics.is_empty(),
        "{:?}",
        checked_entry.parsed.diagnostics
    );
    let checked = checked_entry.checked.expect("checked program");
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics =
        Linter::lint(&checked_entry.parsed.arena, source, LintOptions::default()).diagnostics;
    let unused: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.as_deref() == Some("lint.unused-callable"))
        .collect();
    assert_eq!(unused.len(), 1, "diagnostics: {diagnostics:?}");
    assert!(unused[0].message.contains("`unused_helper`"));
    assert_eq!(unused[0].labels[0].span.source_id, SourceId::new(1));
}

#[test]
fn linter_removes_checked_tail_returns_in_value_branches() {
    let source = "pure label(code: Int) -> Str {\n  match code {\n    0 => return \"ok\"\n    _ => {\n      let detail: Str = f\"exit $code\"\n      return detail # retain this comment\n    }\n  }\n}\n";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let diagnostics = Linter::lint(&parsed.arena, source, LintOptions {
        expr_types: checked.expr_types,
        statement_positions: checked.statement_positions,
        ..LintOptions::default()
    }).diagnostics;
    let fixes = diagnostics.iter().filter(|diagnostic| diagnostic.code.as_deref() == Some("lint.redundant-tail-return")).flat_map(|diagnostic| diagnostic.fix_hints.iter()).collect::<Vec<_>>();
    assert_eq!(fixes.len(), 2);
    let mut candidate = source.to_owned();
    for fix in fixes.iter().rev() { candidate.replace_range(fix.span.unwrap().range(), fix.replacement.as_deref().unwrap()); }
    assert!(candidate.contains("let detail: Str"));
    assert!(candidate.contains("detail # retain this comment"));
    let parsed = parse_lint_source(&candidate);
    let checked = Checker::check_arena(&parsed.arena, &candidate);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let second = Linter::lint(&parsed.arena, &candidate, LintOptions {
        expr_types: checked.expr_types,
        statement_positions: checked.statement_positions,
        ..LintOptions::default()
    }).diagnostics;
    assert!(!second.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("lint.redundant-tail-return")));
}

#[test]
fn linter_tail_return_preserves_grouping_and_unicode_comments() {
    let source = "pure sum() -> Int {\n  return (1 + 2) # café\n}\n";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let diagnostics = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, statement_positions: checked.statement_positions, ..LintOptions::default() }).diagnostics;
    let fix = diagnostics.iter().find(|diagnostic| diagnostic.code.as_deref() == Some("lint.redundant-tail-return")).unwrap().fix_hints.first().unwrap();
    let mut candidate = source.to_owned();
    candidate.replace_range(fix.span.unwrap().range(), fix.replacement.as_deref().unwrap());
    assert!(candidate.contains("(1 + 2) # café"));
    assert_parse_check_standalone("grouped tail", &candidate);
}

#[test]
fn linter_keeps_conditional_and_callback_lexical_returns() {
    let source = "pure conditional(flag: Bool) -> Int {\n  if flag { return 1 }\n  2\n}\npure callback() -> Int {\n  let rows = [1] |> map { |number| return 4 }\n  9\n}\n";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let diagnostics = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, statement_positions: checked.statement_positions, ..LintOptions::default() }).diagnostics;
    assert!(!diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("lint.redundant-tail-return")));
}

#[test]
fn linter_named_argument_pun_fix_preserves_resolution_comments_and_converges() {
    let source = include_str!("../../../tests/fixtures/syntax/valid/named-argument-pun-explicit.xsh");
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let options = LintOptions {
        expr_types: checked.expr_types,
        ..LintOptions::default()
    };
    let diagnostics = Linter::lint(&parsed.arena, source, options).diagnostics;
    let puns: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-named-argument-pun"))
        .collect();
    assert_eq!(
        puns.len(),
        3,
        "different identifiers and field expressions are not puns",
    );
    assert_eq!(puns[0].fix_hints.len(), 1);
    assert_eq!(puns[1].fix_hints.len(), 1);
    assert!(
        puns[2].fix_hints.is_empty(),
        "comments prevent safe replacement",
    );
    let fix = &puns[0].fix_hints[0];
    let span = fix.span.unwrap();
    assert_eq!(&source[span.range()], "value: value");
    assert_eq!(fix.replacement.as_deref(), Some("value:"));
    let grouped_span = puns[1].fix_hints[0].span.unwrap();
    assert_eq!(&source[grouped_span.range()], "value: (value)");
    let mut fixed = source.to_string();
    for diagnostic in puns.iter().rev() {
        for fix in &diagnostic.fix_hints {
            fixed.replace_range(
                fix.span.unwrap().range(),
                fix.replacement.as_ref().unwrap(),
            );
        }
    }
    assert_parse_check_standalone("named argument pun fix", &fixed);
    let formatted = Formatter::new().format_source(SourceId::new(0), &fixed);
    assert!(formatted.diagnostics.is_empty(), "{:?}", formatted.diagnostics);
    assert!(formatted.formatted.contains("accept(value:)"));
    assert!(formatted.formatted.contains("# Preserve this comment."));
    let parsed = parse_lint_source(&formatted.formatted);
    let checked = Checker::check_arena(&parsed.arena, &formatted.formatted);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let second = Linter::lint(
        &parsed.arena,
        &formatted.formatted,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    );
    assert!(
        second.diagnostics.iter()
            .filter(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-named-argument-pun"))
            .all(|diagnostic| diagnostic.fix_hints.is_empty())
    );
}

#[test]
fn linter_named_argument_pun_requires_checked_identifier_resolution() {
    let source = include_str!("../../../tests/fixtures/syntax/valid/named-argument-pun-explicit.xsh");
    let parsed = parse_lint_source(source);
    let diagnostics = Linter::lint(&parsed.arena, source, LintOptions::default()).diagnostics;
    assert!(
        !diagnostics.iter().any(|diagnostic|
            diagnostic.code.as_deref() == Some("lint.prefer-named-argument-pun")
        )
    );
}

#[test]
fn linter_list_compound_assignment_is_checked_and_converges() {
    let source = "# café\nvar names: List[Str] = []\nlet item = \"value\"\nnames = names.push(item) # Keep this reason.\nlet more = [\"second\"]\nnames = names.extend(more)\nprint names.len()\n";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions {
        expr_types: checked.expr_types,
        ..LintOptions::default()
    });
    let updates: Vec<_> = diagnostics.iter().filter(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-list-compound-assignment")).collect();
    assert_eq!(updates.len(), 2);
    let mut fixed = source.to_string();
    for diagnostic in updates.iter().rev() {
        let hint = &diagnostic.fix_hints[0];
        fixed.replace_range(hint.span.unwrap().range(), hint.replacement.as_deref().unwrap());
    }
    assert!(fixed.contains("names += [item] # Keep this reason."));
    assert!(fixed.contains("names += more"));
    let parsed = parse_lint_source(&fixed);
    let checked = Checker::check_arena(&parsed.arena, &fixed);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let second = Linter::lint(&parsed.arena, &fixed, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    assert!(!second.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-list-compound-assignment")));
}

#[test]
fn linter_list_compound_assignment_refuses_unchecked_effectful_and_nested_updates() {
    let source = "pure item() -> Int {\n  return 2\n}\nvar values = [1]\nvalues = values.push(item())\nvar container = {values: [1]}\ncontainer.values = container.values.push(2)\nlet pushed = values.push(3)\nprint ${pushed.len()} ${container.values.len()}\n";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    for options in [LintOptions::default(), LintOptions { expr_types: checked.expr_types, ..LintOptions::default() }] {
        let output = Linter::lint(&parsed.arena, source, options);
        assert!(!output.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-list-compound-assignment")));
    }
}

#[test]
fn linter_list_compound_assignment_retains_multiline_comments() {
    let source = "var values = [1]\nvalues = values.push(\n  2,\n)\nprint ${values.len()}\n";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let output = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    let diagnostic = output.diagnostics.iter().find(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-list-compound-assignment")).expect("checked list update warning");
    assert!(diagnostic.fix_hints.is_empty());
}

#[test]
fn linter_prefer_slice_fixes_proven_byte_bounds_and_converges() {
    let source = "\
let data = b\"abcdef\"
let prefix = data.slice(0, length: 3) # é retained
let suffix = b\"abcdef\".slice(offset: 2)
let whole = data.slice(0, data.len())
let empty = data.slice(0, 0)
print ${prefix.base64()} ${suffix.base64()} ${whole.base64()} ${empty.base64()}
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions {
        expr_types: checked.expr_types,
        ..LintOptions::default()
    });
    let mut fixes = diagnostics.iter()
        .filter(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-slice"))
        .flat_map(|diagnostic| diagnostic.fix_hints.iter())
        .map(|hint| (hint.span.unwrap(), hint.replacement.as_ref().unwrap()))
        .collect::<Vec<_>>();
    assert_eq!(fixes.len(), 4);
    fixes.sort_by_key(|(span, _)| span.start());
    let mut fixed = source.to_string();
    for (span, replacement) in fixes.into_iter().rev() {
        fixed.replace_range(span.range(), replacement);
    }
    assert!(fixed.contains("data[..3] # é retained"));
    assert!(fixed.contains("b\"abcdef\"[2..]"));
    assert!(fixed.contains("data[..]"));
    assert_parse_check_standalone("slice fixes", &fixed);
    let parsed = parse_lint_source(&fixed);
    let checked = Checker::check_arena(&parsed.arena, &fixed);
    let second = Linter::lint(&parsed.arena, &fixed, LintOptions {
        expr_types: checked.expr_types,
        ..LintOptions::default()
    });
    assert!(!second.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-slice")));
}

#[test]
fn linter_prefer_slice_retains_uncertain_offsets_counts_and_overflow() {
    let source = "\
pure count() -> Int {
  return 3
}
let data = b\"abc\"
let negative = data.slice(-1)
let uncertain = data.slice(2)
let arithmetic = data.slice(1, data.len() - 1)
let effect_count = data.slice(0, count())
let overflow = data.slice(1, 9223372036854775807)
print ${negative.base64()} ${uncertain.base64()} ${arithmetic.base64()} ${effect_count.base64()} ${overflow.base64()}
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let diagnostics = Linter::lint(&parsed.arena, source, LintOptions {
        expr_types: checked.expr_types,
        ..LintOptions::default()
    }).diagnostics;
    let slices = diagnostics.iter().filter(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-slice")).collect::<Vec<_>>();
    assert_eq!(slices.len(), 5);
    for diagnostic in slices {
        assert!(diagnostic.fix_hints.is_empty());
        assert!(!diagnostic.notes.is_empty());
    }
}

#[test]
fn linter_prefer_slice_requires_checked_builtin_receiver() {
    let source = "let data = b\"abc\"\nlet part = data.slice(0, 2)\nprint ${part.base64()}\n";
    let parsed = parse_lint_source(source);
    let diagnostics = Linter::lint(&parsed.arena, source, LintOptions::default()).diagnostics;
    assert!(!diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-slice")));
}

#[test]
fn linter_record_destructuring_fix_roundtrips_and_converges() {
    let source = "# 源\nlet config = {root: \"src\", build: {jobs: 3, target: \"native\"}}\nlet root = config.root\nlet jobs = config.build.jobs\nlet target_name = config.build.target\nprint $root $jobs $target_name\n";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let output = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    let diagnostic = output.diagnostics.iter().find(|d| d.code.as_deref() == Some("lint.prefer-record-destructuring")).unwrap();
    let hint = &diagnostic.fix_hints[0];
    let mut fixed = source.to_string();
    fixed.replace_range(hint.span.unwrap().range(), hint.replacement.as_ref().unwrap());
    assert!(fixed.contains("let {root, build: {jobs, target: target_name, ..}, ..} = config"));
    assert_parse_check_standalone("record destructuring", &fixed);
    let formatted = Formatter::new().format_source(SourceId::new(0), &fixed);
    assert!(formatted.diagnostics.is_empty(), "{:?}", formatted.diagnostics);
    assert_eq!(formatted.formatted, fixed);
    let parsed = parse_lint_source(&fixed);
    let checked = Checker::check_arena(&parsed.arena, &fixed);
    let output = Linter::lint(&parsed.arena, &fixed, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    assert!(!output.diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.prefer-record-destructuring")));
}

#[test]
fn linter_record_destructuring_retains_annotations_comments_and_effectful_roots() {
    for source in [
        "let config = {a: 1, b: 2}\nlet a: Int = config.a\nlet b = config.b\nprint $a $b\n",
        "let config = {a: 1, b: 2}\nlet a = config.a # useful\nlet b = config.b\nprint $a $b\n",
        "type Fields = {a: Int, b: Int}\npure source() -> Fields { return {a: 1, b: 2} }\nlet a = source().a\nlet b = source().b\nprint $a $b\n",
    ] {
        let parsed = parse_lint_source(source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        let output = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
        assert!(!output.diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.prefer-record-destructuring")), "{source}");
    }
}

#[test]
fn formatter_preserves_comments_inside_nested_record_binding_targets() {
    let source = "let config = {root: \"src\", build: {jobs: 3, target: \"native\"}}\nlet {root, build: {\n  jobs, # worker count\n  target: target_name, ..\n}, ..} = config\nprint $root $jobs $target_name\n";
    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(formatted.diagnostics.is_empty(), "{:?}", formatted.diagnostics);
    assert_eq!(formatted.formatted, source);
    let again = Formatter::new().format_source(SourceId::new(0), &formatted.formatted);
    assert_eq!(again.formatted, source);
}

#[test]
fn linter_comparison_chain_coalesces_stable_operands_and_converges() {
    let source = include_str!("../../../tests/fixtures/lint/comparison-chain.xsh");
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let diagnostics = Linter::lint(&parsed.arena, source, LintOptions::default()).diagnostics;
    let chains = diagnostics.iter().filter(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-comparison-chain")).collect::<Vec<_>>();
    assert_eq!(chains.len(), 3);
    let mut fixed = source.to_string();
    for diagnostic in chains.into_iter().rev() {
        let hint = &diagnostic.fix_hints[0];
        fixed.replace_range(hint.span.expect("replacement span").range(), hint.replacement.as_deref().unwrap());
    }
    assert!(fixed.contains("lower <= middle < upper"));
    assert!(fixed.contains("0 <= middle < upper <= 20"));
    assert!(fixed.contains("0 < middle <= 10"));
    assert_parse_check_standalone("comparison chains", &fixed);
    let parsed = parse_lint_source(&fixed);
    let second = Linter::lint(&parsed.arena, &fixed, LintOptions::default()).diagnostics;
    assert!(!second.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-comparison-chain")));
    let formatted = Formatter::new().format_source(SourceId::new(0), &fixed);
    assert!(formatted.diagnostics.is_empty(), "{:?}", formatted.diagnostics);
    assert_eq!(formatted.formatted, fixed.replace("λ", "\\u{3bb}"));
    let stable = Formatter::new().format_source(SourceId::new(0), &formatted.formatted);
    assert_eq!(stable.formatted, formatted.formatted);
}

#[test]
fn linter_comparison_chain_preserves_calls_mutable_reads_and_comments() {
    let source = include_str!("../../../tests/fixtures/lint/comparison-chain-unsafe.xsh");
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let diagnostics = Linter::lint(&parsed.arena, source, LintOptions::default()).diagnostics;
    assert!(!diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-comparison-chain")));
}

#[test]
fn formatter_comparison_chain_preserves_grouping_and_precedence() {
    let source = include_str!("../../../tests/fixtures/syntax/comparison-chain.xsh");
    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(formatted.diagnostics.is_empty(), "{:?}", formatted.diagnostics);
    assert!(formatted.formatted.contains("let adjacent = 1 < 2 <= 3"));
    assert!(formatted.formatted.contains("let explicit = (1 < 2 <= 3) == true"));
    assert!(formatted.formatted.contains("let grouped = (1 < 2) < 3"));
    assert!(formatted.formatted.contains("let arithmetic = (1 + 2) * 3 < 10 <= 12"));
    let stable = Formatter::new().format_source(SourceId::new(0), &formatted.formatted);
    assert_eq!(stable.formatted, formatted.formatted);
}

#[test]
fn linter_prefer_guard_supports_value_actions_and_converges() {
    let source = r#"pure cached(value: Str?) -> Str {
  if value != null {
    return value
  }
  return "missing"
}
stream items() [] -> Stream[Int] {
  if !(false or false) {
    yield 1
  }
}
let value = loop {
  if true {
    break 2
  }
}
"#;
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let diagnostics = Linter::lint(&parsed.arena, source, LintOptions::default()).diagnostics;
    let mut edits = diagnostics.iter()
        .filter(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-guard"))
        .flat_map(|diagnostic| diagnostic.fix_hints.iter())
        .map(|hint| (hint.span.unwrap(), hint.replacement.as_ref().unwrap()))
        .collect::<Vec<_>>();
    assert_eq!(edits.len(), 3);
    edits.sort_by_key(|(span, _)| std::cmp::Reverse(span.start()));
    let mut fixed = source.to_string();
    for (span, replacement) in edits {
        fixed.replace_range(span.range(), replacement);
    }
    assert!(fixed.contains("return value when value != null"));
    assert!(fixed.contains("yield 1 unless (false or false)"));
    assert!(fixed.contains("break 2 when true"));
    assert_parse_check_standalone("guarded value fixes", &fixed);
    let formatted = Formatter::new().format_source(SourceId::new(0), &fixed);
    assert!(formatted.diagnostics.is_empty(), "{:?}", formatted.diagnostics);
    assert_parse_check_standalone("formatted guards", &formatted.formatted);
    let reparsed = parse_lint_source(&formatted.formatted);
    let second = Linter::lint(&reparsed.arena, &formatted.formatted, LintOptions::default()).diagnostics;
    assert!(!second.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-guard")));
}

#[test]
fn linter_prefer_guard_preserves_comments_else_and_multiple_actions() {
    for source in [
        "pure value() -> Int { if true { # keep\n return 1 }; return 2 }\n",
        "pure value() -> Int { if true { return 1 } else { return 2 } }\n",
        "proc value() [] -> Int { if true { print 1; return 1 }; return 2 }\n",
    ] {
        let parsed = parse_lint_source(source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let diagnostics = Linter::lint(&parsed.arena, source, LintOptions::default()).diagnostics;
        assert!(!diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-guard")), "{diagnostics:?}");
    }
}

#[test]
fn linter_prefer_guard_groups_external_run_payload() {
    let source = "proc value(selected: Bool) [process] -> Status { if selected { return run.status /usr/bin/true }; return run.status /usr/bin/true }\n";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let diagnostics = Linter::lint(&parsed.arena, source, LintOptions::default()).diagnostics;
    let hint = diagnostics.iter()
        .find(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-guard"))
        .unwrap().fix_hints.first().unwrap();
    assert_eq!(hint.replacement.as_deref(), Some("return (run.status /usr/bin/true) when selected"));
    let mut fixed = source.to_string();
    fixed.replace_range(hint.span.unwrap().range(), hint.replacement.as_ref().unwrap());
    assert_parse_check_standalone("grouped run guard", &fixed);
}

#[test]
fn linter_prefer_guard_keeps_unwieldy_payload_blocks() {
    let source = format!("pure value(selected: Bool) -> Str {{ if selected {{ return \"{}\" }}; return \"fallback\" }}\n", "x".repeat(120));
    let parsed = parse_lint_source(&source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let diagnostics = Linter::lint(&parsed.arena, &source, LintOptions::default()).diagnostics;
    assert!(!diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-guard")));
}

#[test]
fn linter_guarded_return_keeps_following_statements_reachable() {
    let source = "pure value(selected: Bool) -> Int { return 1 when selected; return 2 }\n";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let diagnostics = Linter::lint(&parsed.arena, source, LintOptions::default()).diagnostics;
    assert!(!diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("lint.dead-code")), "{diagnostics:?}");
}

#[test]
fn linter_multi_clause_accumulators_have_safe_idempotent_fixes() {
    let source = "let groups = [[1, 2], [3]]\nvar values: List[Int] = []\n\nfor batch in groups {\n  if batch.len() > 0 {\n    for value in batch {\n      if value > 1 {\n        values = values.push(value)\n      }\n    }\n  }\n}\n";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    let hint = diagnostics.iter().find(|d| d.code.as_deref() == Some("lint.prefer-list-comp")).and_then(|d| d.fix_hints.first()).expect("nested accumulator fix");
    let mut fixed = source.to_owned();
    fixed.replace_range(hint.span.expect("fix span").range(), hint.replacement.as_deref().expect("replacement"));
    assert_parse_check_standalone("multi clause fix", &fixed);
    let reparsed = parse_lint_source(&fixed);
    let second = Linter::lint(&reparsed.arena, &fixed, LintOptions::default());
    assert!(!second.diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.prefer-list-comp")));
}

#[test]
fn linter_multi_clause_map_accumulator_retains_annotation_and_filters() {
    let source = "let entries = [{key: \"a\", values: [1, 2]}]\nvar values: Map[Int] = {}\nfor entry in entries {\n  for value in entry.values {\n    if value > 1 {\n      values[entry.key] = value\n    }\n  }\n}\n";
    let parsed = parse_lint_source(source);
    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());
    let hint = diagnostics.iter().find(|d| d.code.as_deref() == Some("lint.prefer-map-comp")).and_then(|d| d.fix_hints.first()).expect("nested map fix");
    assert!(hint.replacement.as_deref().unwrap().contains("var values: Map[Int] = {\n"));
}

#[test]
fn linter_multi_clause_accumulators_keep_uncertain_loops() {
    for body in [
        "for batch in groups {\n  for value in values {\n    values = values.push(value)\n  }\n}",
        "for batch in groups {\n  for values in batch {\n    values = values.push(1)\n  }\n}",
        "for batch in groups {\n  for value in batch {\n    print $value\n    values = values.push(value)\n  }\n}",
        "for batch in groups {\n  for value in batch {\n    # résumé\n    values = values.push(value)\n  }\n}",
        "for batch in groups {\n  for value in batch {\n    if values.len() == 0 {\n      values = values.push(value)\n    }\n  }\n}",
        "for batch in groups {\n  for value in batch {\n    return\n  }\n}",
    ] {
        let source = format!("let groups = [[1, 2]]\nvar values: List[Int] = []\n{body}\n");
        let parsed = parse_lint_source(&source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let output = Linter::lint(&parsed.arena, &source, LintOptions::default());
        assert!(!output.diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.prefer-list-comp")), "unsafe fix for {source}: {:?}", output.diagnostics);
    }
}

#[test]
fn optional_postfix_fix_preserves_null_fallback_and_converges() {
    let source = "let name: Str? = null\nlet label = if name == null { \"default\" } else { name.trim() }\nprint $label\n";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty());
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let diagnostics = Linter::lint(&parsed.arena, source, LintOptions {
        expr_types: checked.expr_types, ..LintOptions::default()
    }).diagnostics;
    let diagnostic = diagnostics.iter().find(|d| d.code.as_deref() == Some("lint.prefer-optional-postfix")).expect("optional postfix lint");
    let fix = &diagnostic.fix_hints[0];
    let span = fix.span.unwrap();
    let mut fixed = source.to_string();
    fixed.replace_range(span.start()..span.end(), fix.replacement.as_ref().unwrap());
    assert_parse_check_standalone("optional postfix", &fixed);
    let formatted = Formatter::new().format_source(SourceId::new(0), &fixed);
    assert!(formatted.diagnostics.is_empty());
    assert!(formatted.formatted.contains("name?.trim() ??"));
    let reparsed = parse_lint_source(&formatted.formatted);
    let rechecked = Checker::check_arena(&reparsed.arena, &formatted.formatted);
    let second = Linter::lint(&reparsed.arena, &formatted.formatted, LintOptions {
        expr_types: rechecked.expr_types, ..LintOptions::default()
    });
    assert!(!second.diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.prefer-optional-postfix")));
    assert_eq!(Formatter::new().format_source(SourceId::new(0), &formatted.formatted).formatted, formatted.formatted);
}

#[test]
fn optional_postfix_fix_refuses_mutation_comments_and_optional_results() {
    for source in [
        "let name: Str? = null\nlet label = if name == null { \"default\" } else { print \"selected\"; name.trim() }\nprint $label\n",
        "var name: Str? = null\nname = \"x\"\nlet label = if name == null { \"default\" } else { name.trim() }\nprint $label\n",
        "let name: Str? = null\nlet label = if name == null {\n  # retain explanation\n  \"default\"\n} else { name.trim() }\nprint $label\n",
        "type Item = {name: Str?}\nlet item: Item? = null\nlet name: Str? = if item == null { \"default\" } else { item.name }\nprint (name ?? \"\")\n",
    ] {
        let parsed = parse_lint_source(source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let diagnostics = Linter::lint(&parsed.arena, source, LintOptions {
            expr_types: checked.expr_types, ..LintOptions::default()
        }).diagnostics;
        assert!(!diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.prefer-optional-postfix")), "{source}");
    }
}

#[test]
fn formatter_retains_guarded_postfix_and_unicode_spans() {
    let source = "let text: Str? = null\nlet prefix = text?[..2] ?? \"α\"\nlet suffix = text?[1..] ?? \"β\"\nlet whole = text?[..] ?? \"γ\"\nlet values: List[Int]? = null\nlet item = values?[0] ?? 3\nprint (text?.trim() ?? prefix) $suffix $whole $item\n";
    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(formatted.diagnostics.is_empty(), "{:?}", formatted.diagnostics);
    assert!(formatted.formatted.contains("text?[..2]"));
    assert!(formatted.formatted.contains("values?[0]"));
    assert_eq!(Formatter::new().format_source(SourceId::new(0), &formatted.formatted).formatted, formatted.formatted);
    assert_parse_check_standalone("guarded postfix round trip", &formatted.formatted);
}

#[test]
fn linter_map_entry_iteration_fix_preserves_spans_and_converges() {
    let source = "# 源\nproc render(counts: Map[Int]) [error] -> List[Str] {\n  var output: List[Str] = []\n  for key in counts.keys() {\n    let count = counts.get(key)?\n    output += [f\"${key}=${count}\"]\n  }\n\n  return output\n}\n";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let diagnostics = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() }).diagnostics;
    let diagnostic = diagnostics.iter().find(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-map-entry-iteration")).unwrap();
    let hint = &diagnostic.fix_hints[0];
    let mut fixed = source.to_string();
    fixed.replace_range(hint.span.unwrap().range(), hint.replacement.as_ref().unwrap());
    assert!(fixed.contains("for {key, value: count} in counts"));
    assert!(!fixed.contains("counts.get(key)"));
    assert_parse_check_standalone("map iteration fix", &fixed);
    let formatted = Formatter::new().format_source(SourceId::new(0), &fixed);
    assert!(formatted.diagnostics.is_empty(), "{:?}", formatted.diagnostics);
    assert_eq!(formatted.formatted, fixed);
    let parsed = parse_lint_source(&fixed);
    let checked = Checker::check_arena(&parsed.arena, &fixed);
    let again = Linter::lint(&parsed.arena, &fixed, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    assert!(!again.diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.prefer-map-entry-iteration")));
}

#[test]
fn linter_map_entry_iteration_keeps_mutation_annotations_comments_and_unknown_methods() {
    for body in [
        "let count = counts.get(key)?\n    counts[\"other\"] = count",
        "let count: Int = counts.get(key)?\n    print $count",
        "let count = counts.get(key)? # explains lookup\n    print $count",
        "let count = counts.get(key, 0)\n    print $count",
    ] {
        let source = format!("var counts = map.empty().set(\"one\", 1)\nfor key in counts.keys() {{\n    {body}\n}}\n");
        let parsed = parse_lint_source(&source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        let output = Linter::lint(&parsed.arena, &source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
        assert!(!output.diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.prefer-map-entry-iteration")), "{source}");
    }
}

#[test]
fn linter_list_splicing_rechecks_preserves_unicode_and_converges() {
    let source = "# café\nlet flags = [\"-g\"]\nlet names = [\"main.xsh\"]\nlet argv = [\"cc\"].extend(flags).extend([\"-o\", \"app\"]).extend(names)\nprint argv.len()\n";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let output = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    let hint = output.diagnostics.iter().filter(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-list-splicing"))
        .flat_map(|diagnostic| &diagnostic.fix_hints).max_by_key(|hint| hint.span.unwrap().range().len()).expect("list construction fix");
    let mut fixed = source.to_string();
    fixed.replace_range(hint.span.unwrap().range(), hint.replacement.as_deref().unwrap());
    assert!(fixed.contains("[\"cc\", @flags, \"-o\", \"app\", @names]"));
    assert_parse_check_standalone("spliced construction", &fixed);
    let formatted = Formatter::new().format_source(SourceId::new(0), &fixed);
    assert!(formatted.diagnostics.is_empty());
    assert_eq!(Formatter::new().format_source(SourceId::new(0), &formatted.formatted).formatted, formatted.formatted);
    let parsed = parse_lint_source(&formatted.formatted);
    let checked = Checker::check_arena(&parsed.arena, &formatted.formatted);
    let second = Linter::lint(&parsed.arena, &formatted.formatted, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    assert!(!second.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-list-splicing")));
}

#[test]
fn linter_list_splicing_retains_nested_elements_and_local_update_policy() {
    let source = "let groups = [[1]].extend([[2]]).extend([[3]])\nvar values = [1]\nvalues = values.extend([2])\nlet nested = groups.push([4])\nprint groups.len() nested.len()\n";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty());
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let output = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    let updates: Vec<_> = output.diagnostics.iter().filter(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-list-splicing")).collect();
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].fix_hints[0].replacement.as_deref(), Some("[[1], [2], [3]]"));
    let unchecked = Linter::lint(&parsed.arena, source, LintOptions::default());
    assert!(!unchecked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-list-splicing")));
}

#[test]
fn linter_list_splicing_refuses_annotation_conversions() {
    let source = "type Row = {value: Int}\nlet left: List[Row] = [{value: 1}]\nlet right: List[Row] = [{value: 2}]\nlet combined = left.extend(right).extend(left)\nprint combined.len()\n";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let output = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    assert!(!output.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-list-splicing")));
}

#[test]
fn linter_list_splicing_preserves_comments_without_a_fix() {
    let source = "let argv = [\"head\"] + (if true { # retain this explanation\n  [\"tail\"]\n} else { [\"other\"] })\nprint argv.len()\n";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let output = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    let diagnostic = output.diagnostics.iter().find(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-list-splicing")).expect("construction warning");
    assert!(diagnostic.fix_hints.is_empty());
}

#[test]
fn formatter_list_pattern_nested_rest_and_comments_are_stable() {
    let source = include_str!("../../../tests/fixtures/syntax/list-pattern.xsh");
    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(formatted.diagnostics.is_empty(), "{:?}", formatted.diagnostics);
    assert!(formatted.formatted.contains("[\"build\", _, ..]"));
    assert!(formatted.formatted.contains("# Keep the selected command explanation."));
    let again = Formatter::new().format_source(SourceId::new(0), &formatted.formatted);
    assert_eq!(again.formatted, formatted.formatted);
}

#[test]
fn linter_list_pattern_preserves_unsafe_bounds_mutability_annotations_and_comments() {
    let source = include_str!("../../../tests/fixtures/lint/list-pattern-unsafe.xsh");
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let diagnostics = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() }).diagnostics;
    assert!(!diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-list-pattern")));
}

#[test]
fn linter_list_pattern_rewrites_stable_bounded_extraction_and_converges() {
    let source = include_str!("../../../tests/fixtures/lint/list-pattern.xsh");
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let diagnostics = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() }).diagnostics;
    let selected: Vec<_> = diagnostics.iter().filter(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-list-pattern")).collect();
    assert_eq!(selected.len(), 2);
    let mut fixed = source.to_string();
    for diagnostic in selected.into_iter().rev() {
        let hint = &diagnostic.fix_hints[0];
        fixed.replace_range(hint.span.unwrap().range(), hint.replacement.as_deref().unwrap());
    }
    assert!(fixed.contains("if let [\"build\", target] = values"));
    assert!(fixed.contains("if let [7, target, ..] = values"));
    assert_parse_check_standalone("list patterns", &fixed);
    let parsed = parse_lint_source(&fixed);
    let diagnostics = Linter::lint(&parsed.arena, &fixed, LintOptions::default()).diagnostics;
    assert!(!diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-list-pattern")));
}

#[test]
fn checker_list_pattern_reachability_uses_unguarded_coverage() {
    let source = include_str!("../../../tests/fixtures/frontend-indexed/list-pattern-unreachable.xsh");
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert_eq!(checked.diagnostics.iter().filter(|diagnostic| diagnostic.code.as_deref() == Some("check.unreachable-match-arm")).count(), 1);
}

#[test]
fn linter_defer_block_helper_fix_is_checked_and_idempotent() {
    let source = "proc cleanup() [] -> Unit {\n  print \"café\"\n}\ndefer cleanup()\nprint \"body\"\n";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let output = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    let diagnostic = output.diagnostics.iter().find(|d| d.code.as_deref() == Some("lint.prefer-defer-block")).expect("safe helper suggestion");
    assert_eq!(diagnostic.fix_hints.len(), 2);
    let mut edits = diagnostic.fix_hints.clone();
    edits.sort_by_key(|hint| std::cmp::Reverse(hint.span.unwrap().start()));
    let mut fixed = source.to_string();
    for edit in edits { fixed.replace_range(edit.span.unwrap().range(), edit.replacement.as_deref().unwrap()); }
    assert!(fixed.contains("defer {\n  print \"café\"\n}"));
    assert_parse_check_standalone("defer block helper", &fixed);
    let parsed = parse_lint_source(&fixed);
    let checked = Checker::check_arena(&parsed.arena, &fixed);
    let second = Linter::lint(&parsed.arena, &fixed, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    assert!(!second.diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.prefer-defer-block")));
}

#[test]
fn linter_defer_block_helper_refuses_captures_failures_comments_and_multiple_uses() {
    for source in [
        "let message = \"captured\"\nproc cleanup() [] -> Unit { print $message }\ndefer cleanup()\n",
        "proc cleanup() [error] { let _ = \"bad\".parse_int()? }\ndefer cleanup()\n",
        "# preserve helper docs\nproc cleanup() [] -> Unit { print \"done\" }\ndefer cleanup()\n",
        "proc cleanup() [] -> Unit { print \"done\" }\ndefer cleanup()\ncleanup()\n",
        "proc cleanup() [] -> Unit { print \"done\" }\nproc caller() [] { defer cleanup() }\ncaller()\n",
    ] {
        let parsed = parse_lint_source(source);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
        for options in [LintOptions::default(), LintOptions { expr_types: checked.expr_types.clone(), ..LintOptions::default() }] {
            let output = Linter::lint(&parsed.arena, source, options);
            assert!(!output.diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.prefer-defer-block")), "{source}");
        }
    }
}

#[test]
fn linter_regex_literals_decode_patterns_preserve_comments_and_converge() {
    let source = "let escaped = regex.compile(\"^\\\\s*[A-Z]+$\")? # retained\nlet quoted = regex.compile(\"^\\\".*\\\"$\")?\nlet raw = regex.compile(r\"\\$\\{literal\\}\")?\nprint ${escaped.matches(\"WORD\")} ${quoted.matches(\"\\\"word\\\"\")} ${raw.matches(r\"${literal}\")}\n";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions {
        expr_types: checked.expr_types, ..LintOptions::default()
    });
    let mut edits = diagnostics.iter().filter(|d| d.code.as_deref() == Some("lint.prefer-regex-literal"))
        .flat_map(|d| &d.fix_hints).map(|h| (h.span.unwrap(), h.replacement.as_ref().unwrap())).collect::<Vec<_>>();
    assert_eq!(edits.len(), 3);
    edits.sort_by_key(|(span, _)| span.start());
    let mut fixed = source.to_string();
    for (span, replacement) in edits.into_iter().rev() { fixed.replace_range(span.range(), replacement); }
    assert!(fixed.contains("rx\"^\\s*[A-Z]+$\" # retained"));
    assert!(fixed.contains("rx\"\"\"^\".*\"$\"\"\""));
    assert_parse_check_standalone("regex literal fixes", &fixed);
    let parsed = parse_lint_source(&fixed);
    let checked = Checker::check_arena(&parsed.arena, &fixed);
    let second = Linter::lint(&parsed.arena, &fixed, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    assert!(!second.diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.prefer-regex-literal")));
}

#[test]
fn linter_regex_literals_retain_invalid_dynamic_results_contexts_and_recovery() {
    let source = "let pattern = \"[a-z]+\"\nlet invalid = regex.compile(\"(\")\nlet dynamic = regex.compile(pattern)?\nlet consumed = regex.compile(\"[a-z]+\")\nlet recovered = regex.compile(\"[a-z]+\") ?? rx\".*\"\nlet contextual = regex.compile(\"[a-z]+\").context(\"user pattern\")?\nlet unrepresentable = regex.compile(\"\\\"\\\"\\\"\")?\nlet commented = regex.compile(\n  # explanation\n  \"[a-z]+\",\n)?\nprint ${dynamic.matches(\"x\")} ${recovered.matches(\"x\")} ${contextual.matches(\"x\")} ${unrepresentable.matches(\"x\")} ${commented.matches(\"x\")}\n";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let diagnostics = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() }).diagnostics;
    let regex = diagnostics.iter().filter(|d| d.code.as_deref() == Some("lint.prefer-regex-literal")).collect::<Vec<_>>();
    assert_eq!(regex.len(), 1);
    assert!(regex[0].fix_hints.is_empty());
    assert!(!regex[0].notes.is_empty());
    assert!(!Linter::lint(&parsed.arena, source, LintOptions::default()).diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.prefer-regex-literal")));
}

#[test]
fn regex_literal_formatting_retains_source_delimiters_and_raw_contents() {
    let source = "let single=rx\"^\\s*\\$\\{literal\\}$\"\nlet multiline=rx\"\"\"(?x)\n  ^ [a-z]+ # raw comment\n  $\n\"\"\"\n";
    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(formatted.diagnostics.is_empty(), "{:?}", formatted.diagnostics);
    assert!(formatted.formatted.contains("rx\"^\\s*\\$\\{literal\\}$\""));
    assert!(formatted.formatted.contains("rx\"\"\"(?x)\n  ^ [a-z]+ # raw comment\n  $\n\"\"\""));
    let before = parse_lint_source(source);
    let after = parse_lint_source(&formatted.formatted);
    assert!(after.diagnostics.is_empty(), "{:?}", after.diagnostics);
    assert_eq!(before.arena.arena.regex_literals.iter().map(|l| l.pattern.clone()).collect::<Vec<_>>(), after.arena.arena.regex_literals.iter().map(|l| l.pattern.clone()).collect::<Vec<_>>());
    assert_parse_check_standalone("formatted regex literals", &formatted.formatted);
    let again = Formatter::new().format_source(SourceId::new(0), &formatted.formatted);
    assert_eq!(formatted.formatted, again.formatted);
}

#[test]
fn linter_regex_literals_retains_compile_calls_in_result_recovery_branches() {
    let source = "with value = regex.compile(\"(\") { print ${value.matches(\"x\")} } else { let fallback = regex.compile(\".*\")?; print ${fallback.matches(\"x\")} }\nlet recovered = regex.compile(\"(\") ?? regex.compile(\".*\")?\nlet matched = match regex.compile(\"(\") {\n  Ok(value) => value,\n  Err(_) => regex.compile(\".*\")?,\n}\nmatch regex.compile(\"(\") {\n  Ok(value) => { print ${value.matches(\"x\")} },\n  Err(_) => { let fallback = regex.compile(\".*\")?; print ${fallback.matches(\"x\")} },\n}\nprint ${recovered.matches(\"x\")} ${matched.matches(\"x\")}\n";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let output = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    assert!(!output.diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.prefer-regex-literal")));
}

#[test]
fn yield_delegation_forwarding_fix_is_checked_and_idempotent() {
    for iterable in ["values", "(Ok(values)?)"] {
        let source = format!("stream rows(values: List[Int]) [error] -> Stream[Int] {{\n  for item in {iterable} {{\n    yield item\n  }}\n}}\n");
        let parsed = parse_lint_source(&source);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let diagnostics = Linter::lint(&parsed.arena, &source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() }).diagnostics;
        let hint = diagnostics.iter().find(|d| d.code.as_deref() == Some("lint.prefer-yield-delegation")).and_then(|d| d.fix_hints.first()).expect("forwarding fix");
        let mut fixed = source.clone();
        fixed.replace_range(hint.span.unwrap().range(), hint.replacement.as_deref().unwrap());
        assert!(fixed.contains("yield @"), "{fixed}");
        if iterable.contains('?') { assert!(fixed.contains("Ok(values)?"), "{fixed}"); }
        assert_parse_check_standalone("yield delegation fix", &fixed);
        let parsed = parse_lint_source(&fixed);
        let again = Linter::lint(&parsed.arena, &fixed, LintOptions::default());
        assert!(!again.diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.prefer-yield-delegation")));
    }
}

#[test]
fn yield_delegation_fix_preserves_nontransparent_forwarding_loops() {
    for body in ["yield item * 2", "if item > 0 { yield item }", "print $item\n    yield item", "defer close()\n    yield item", "yield item\n    break", "# current item\n    yield item"] {
        let source = format!("proc close() [io] {{ print \"close\" }}\nstream rows(values: List[Int]) [io] -> Stream[Int] {{\n  for item in values {{\n    {body}\n  }}\n}}\n");
        let parsed = parse_lint_source(&source);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
        let output = Linter::lint(&parsed.arena, &source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
        assert!(!output.diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.prefer-yield-delegation")), "unsafe fix: {source}");
    }
    for source in [
        "stream rows(values: Result[List[Int]]) [error] -> Stream[Int] { for item in values { yield item } }\n",
        "stream rows(values: Stream[Int]) [] -> Stream[Int] { for item in values |> map { |number| number + 1 } { yield item } }\n",
    ] {
        let parsed = parse_lint_source(source);
        let checked = Checker::check_arena(&parsed.arena, source);
        let output = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
        assert!(!output.diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.prefer-yield-delegation")));
    }
}

#[test]
fn formatter_preserves_yield_delegation_and_postfix_guards() {
    let source = "stream rows() -> Stream[Int] {\n  yield @[1, 2]\n  yield @([3]) when true\n}\n";
    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(formatted.diagnostics.is_empty(), "{:?}", formatted.diagnostics);
    assert_parse_check_standalone("formatted delegation", &formatted.formatted);
    let again = Formatter::new().format_source(SourceId::new(0), &formatted.formatted);
    assert_eq!(again.formatted, formatted.formatted);
}

#[test]
fn error_fallback_fix_preserves_handler_effects_and_converges() {
    let source = "pure recover(outcome: Result[Str]) -> Str {\n  let selected = match outcome { Ok(value) => value, Err(failure) => failure.message }\n  selected\n}\n".to_string();
    let parsed = parse_lint_source(&source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, &source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let output = Linter::lint(&parsed.arena, &source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    let diagnostic = output.diagnostics.iter().find(|diagnostic| diagnostic.code.as_deref() == Some("lint.error-fallback-block")).expect("identity fallback fix");
    let hint = &diagnostic.fix_hints[0];
    let mut fixed = source.clone();
    fixed.replace_range(hint.span.unwrap().range(), hint.replacement.as_ref().unwrap());
    assert_parse_check_standalone("error fallback block", &fixed);
    let formatted = Formatter::new().format_source(SourceId::new(0), &fixed);
    assert!(formatted.diagnostics.is_empty());
    assert!(formatted.formatted.contains("outcome ?? { |failure|"));
    assert!(formatted.formatted.contains("failure.message"));
    let reparsed = parse_lint_source(&formatted.formatted);
    let rechecked = Checker::check_arena(&reparsed.arena, &formatted.formatted);
    let second = Linter::lint(&reparsed.arena, &formatted.formatted, LintOptions { expr_types: rechecked.expr_types, ..LintOptions::default() });
    assert!(!second.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("lint.error-fallback-block")));
    assert_eq!(Formatter::new().format_source(SourceId::new(0), &formatted.formatted).formatted, formatted.formatted);
}

#[test]
fn error_fallback_fix_refuses_success_transforms_guards_and_error_patterns() {
    for source in [
        "pure recover(outcome: Result[Str]) -> Str { let selected = match outcome { Ok(value) => value.trim(), Err(failure) => failure.message }; selected }\n",
        "pure recover(outcome: Result[Str]) -> Str { let selected = match outcome { Ok(value) if true => value, _ => \"other\" }; selected }\n",
        "pure recover(outcome: Result[Str]) -> Str { let selected = match outcome { Ok(value) => value, Err(is NotFound) => \"missing\", _ => \"other\" }; selected }\n",
        "pure recover(outcome: Result[Str]) -> Str { let selected = match outcome { Ok(value) => value, Err(failure) => {\n# retain this explanation\nfailure.message\n} }; selected }\n",
    ] {
        let parsed = parse_lint_source(source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let output = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
        assert!(!output.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("lint.error-fallback-block")), "{source}");
    }
}

#[test]
fn error_fallback_flow_keeps_success_path_reachable() {
    let source = "pure choose(outcome: Result[Int]) -> Int {\n  let selected = outcome ?? { |_| return 7 }\n  selected\n}\n";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty());
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let output = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    assert!(!output.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("lint.unreachable")), "{:?}", output.diagnostics);
}

#[test]
fn linter_record_constructor_preserves_annotation_comments_and_converges() {
    let source = include_str!("../../../tests/fixtures/syntax/valid/record-constructor-explicit.xsh");
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let diagnostics = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() }).diagnostics;
    let constructors: Vec<_> = diagnostics.iter().filter(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-record-constructor")).collect();
    assert_eq!(constructors.len(), 4);
    assert_eq!(constructors[0].fix_hints.len(), 1);
    assert!(constructors[1].fix_hints.is_empty());
    let fix = &constructors[0].fix_hints[0];
    assert_eq!(fix.replacement.as_deref(), Some("Config(name:)"));
    let mut fixed = source.to_string();
    for diagnostic in constructors.iter().rev() {
        for fix in &diagnostic.fix_hints {
            fixed.replace_range(fix.span.unwrap().range(), fix.replacement.as_ref().unwrap());
        }
    }
    assert!(fixed.contains("let config: Config = Config("));
    assert!(fixed.contains("enabled: observed_default()"));
    assert!(fixed.contains("let lookup: Lookup = Lookup(value: empty)"));
    assert_parse_check_standalone("record constructor", &fixed);
    let formatted = Formatter::new().format_source(SourceId::new(0), &fixed);
    assert_eq!(formatted.formatted, fixed);
    let parsed = parse_lint_source(&fixed);
    let checked = Checker::check_arena(&parsed.arena, &fixed);
    let second = Linter::lint(&parsed.arena, &fixed, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() }).diagnostics;
    let constructors: Vec<_> = second.iter().filter(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-record-constructor")).collect();
    assert_eq!(constructors.len(), 1);
    assert!(constructors[0].fix_hints.is_empty());
}

#[test]
fn linter_record_constructor_requires_static_schema_and_preserves_constant_bits() {
    let source = "type Signed = {value: Float = -0.0}\nlet same: Signed = {value: -0.0}\nlet different: Signed = {value: 0.0}\nlet source = {value: -0.0}\nlet spread: Signed = {...source}\ntype Lookup = {value: Map[Int]}\nlet contextual: Lookup = {value: {}}\nlet dynamic: Record = {value: -0.0}\n";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let unchecked = Linter::lint(&parsed.arena, source, LintOptions::default()).diagnostics;
    assert!(!unchecked.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-record-constructor")));
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let diagnostics = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() }).diagnostics;
    let constructors: Vec<_> = diagnostics.iter().filter(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-record-constructor")).collect();
    assert_eq!(constructors.len(), 4);
    assert_eq!(constructors[0].fix_hints[0].replacement.as_deref(), Some("Signed()"));
    assert_eq!(constructors[1].fix_hints[0].replacement.as_deref(), Some("Signed(value: 0.0)"));
    assert!(constructors[2].fix_hints.is_empty());
    assert!(constructors[3].fix_hints.is_empty());
}

#[test]
fn private_pure_return_removal_is_opt_in_exact_and_convergent() {
    let source = "pure label(name: Str) -> Str { name.trim() }\nprint label(\"ready\")\n";
    let parsed = parse_lint_source(source);
    let disabled = Linter::lint(&parsed.arena, source, LintOptions::default());
    assert!(!disabled.diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.prefer-inferred-pure-return")));
    let enabled = Linter::lint(&parsed.arena, source, LintOptions { prefer_inferred_pure_returns: true, ..LintOptions::default() });
    let diagnostic = enabled.diagnostics.iter().find(|d| d.code.as_deref() == Some("lint.prefer-inferred-pure-return")).expect("safe inferred return fix");
    let span = diagnostic.fix_hints[0].span.unwrap();
    let mut fixed = source.to_string();
    fixed.replace_range(span.start()..span.end(), "");
    assert_parse_check_standalone("inferred return", &fixed);
    let parsed = parse_lint_source(&fixed);
    let second = Linter::lint(&parsed.arena, &fixed, LintOptions { prefer_inferred_pure_returns: true, ..LintOptions::default() });
    assert!(!second.diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.prefer-inferred-pure-return")));
}

#[test]
fn private_pure_return_removal_retains_context_and_result_boundaries() {
    for source in [
        "pure empty() -> List[Int] { [] }\nlet values = empty()\n",
        "type Item = {name: Str}\npure item() -> Item { {name: \"ready\"} }\nlet selected = item()\n",
        "type Item = {name: Str}\npure items() -> List[Item] { [{name: \"ready\"}] }\nlet selected = items()\n",
        "pure structural() -> Record { {name: \"ready\"} }\nlet selected = structural()\n",
        "pure wrapped() -> Result[Int] { 1 }\nlet selected = wrapped()\n",
        "pure converted() -> Path { \".\" }\nlet selected = converted()\n",
        "pure assertion() -> Unit { false }\nassertion()\n",
        "pure recursive(value: Int) -> Int { recursive(value) }\nlet selected = recursive(1)\n",
        "export pure visible() -> Int { 1 }\n",
    ] {
        let parsed = parse_lint_source(source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let linted = Linter::lint(&parsed.arena, source, LintOptions { prefer_inferred_pure_returns: true, ..LintOptions::default() });
        assert!(!linted.diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.prefer-inferred-pure-return")), "{source}");
    }
}

#[test]
fn field_label_fixes_preserve_key_bytes_conversions_comments_and_converge() {
    let source = "let row = {\"type\": \"file\", r\"in\": 2, \"a.b\": 3, \"x-y\": 4, \"size\": 5} # retained\nlet label: Str = row.get(\"type\")?\nprint $label ${row.in}\n";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let output = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    let mut edits = output.diagnostics.iter().filter(|d| matches!(d.code.as_deref(), Some("lint.prefer-bare-field-label" | "lint.prefer-known-field-access")))
        .flat_map(|d| &d.fix_hints).map(|h| (h.span.unwrap(), h.replacement.as_ref().unwrap())).collect::<Vec<_>>();
    assert_eq!(edits.len(), 4, "{:?}", output.diagnostics);
    edits.sort_by_key(|(span, _)| span.start());
    let mut fixed = source.to_string();
    for (span, replacement) in edits.into_iter().rev() { fixed.replace_range(span.range(), replacement); }
    assert!(fixed.contains("{type: \"file\", in: 2, \"a.b\": 3, \"x-y\": 4, size: 5} # retained"), "{fixed}");
    assert!(fixed.contains("let label: Str = row.type"), "{fixed}");
    assert_parse_check_standalone("field label fixes", &fixed);
    let formatted = Formatter::new().format_source(SourceId::new(0), &fixed);
    assert!(formatted.diagnostics.is_empty());
    assert_eq!(formatted.formatted, Formatter::new().format_source(SourceId::new(0), &formatted.formatted).formatted);
    let parsed = parse_lint_source(&fixed);
    let checked = Checker::check_arena(&parsed.arena, &fixed);
    let again = Linter::lint(&parsed.arena, &fixed, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    assert!(!again.diagnostics.iter().any(|d| matches!(d.code.as_deref(), Some("lint.prefer-bare-field-label" | "lint.prefer-known-field-access"))));
}

#[test]
fn field_label_access_fixes_retain_dynamic_results_context_recovery_and_consumers() {
    let source = "let row = {type: \"file\", in: 2}\nlet unknown_consumer = row.get(\"type\")?\nlet consumed = row.get(\"type\")\nlet contextual: Str = row.get(\"type\").context(\"wire\")?\nlet missing = row.get(\"absent\")\nlet recovered = row.get(\"type\") ?? \"none\"\nlet commented: Str = row.get(\n  # keep\n  \"type\",\n)?\nlet dynamic: Record = {}\nlet selected = dynamic.get(\"type\")?\nprint $unknown_consumer $contextual $recovered $commented $selected\n";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let diagnostics = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() }).diagnostics;
    let fields = diagnostics.iter().filter(|d| d.code.as_deref() == Some("lint.prefer-known-field-access")).collect::<Vec<_>>();
    assert_eq!(fields.len(), 2);
    assert!(fields.iter().all(|d| d.fix_hints.is_empty() && !d.notes.is_empty()));
    assert!(!Linter::lint(&parsed.arena, source, LintOptions::default()).diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.prefer-known-field-access")));
}

#[test]
fn linter_map_literal_chains_recheck_and_preserve_unicode_order() {
    let source = "# café\nlet key = \"β\"\nlet counts = {[key]: 1}.set(\"alpha\", 2).set(key, 3)\nprint counts.len()\n";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let output = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    let hint = output.diagnostics.iter().filter(|d| d.code.as_deref() == Some("lint.prefer-map-literal")).flat_map(|d| &d.fix_hints).max_by_key(|h| h.span.unwrap().range().len()).expect("Map construction fix");
    let mut fixed = source.to_string();
    fixed.replace_range(hint.span.unwrap().range(), hint.replacement.as_ref().unwrap());
    assert!(fixed.contains("{[key]: 1, [\"alpha\"]: 2, [key]: 3}"));
    assert_parse_check_standalone("computed Map chain", &fixed);
    let formatted = Formatter::new().format_source(SourceId::new(0), &fixed);
    assert!(formatted.diagnostics.is_empty());
    assert_eq!(Formatter::new().format_source(SourceId::new(0), &formatted.formatted).formatted, formatted.formatted);
    let parsed = parse_lint_source(&formatted.formatted);
    let checked = Checker::check_arena(&parsed.arena, &formatted.formatted);
    let second = Linter::lint(&parsed.arena, &formatted.formatted, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    assert!(!second.diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.prefer-map-literal")));
}

#[test]
fn linter_fresh_map_initialization_retains_annotations_and_refuses_observations() {
    let source = "let name = \"entry\"\nvar counts: Map[Int] = {}\ncounts = counts.set(name, 1)\ncounts = counts.set(\"total\", 2)\nprint counts.len()\n";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let output = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    let hint = output.diagnostics.iter().find(|d| d.code.as_deref() == Some("lint.prefer-map-literal")).expect("fresh initialization fix").fix_hints.first().unwrap();
    let mut fixed = source.to_string();
    fixed.replace_range(hint.span.unwrap().range(), hint.replacement.as_ref().unwrap());
    assert!(fixed.contains("var counts: Map[Int] = {[name]: 1, [\"total\"]: 2}"));
    assert_parse_check_standalone("fresh Map", &fixed);
    let formatted = Formatter::new().format_source(SourceId::new(0), &fixed);
    assert_eq!(Formatter::new().format_source(SourceId::new(0), &formatted.formatted).formatted, formatted.formatted);
    for source in [
        "var counts: Map[Int] = {}\nprint counts.len()\ncounts = counts.set(\"one\", 1)\n",
        "var counts: Map[Int] = {}\ncounts = counts.set(\"one\", counts.len())\n",
        "var counts: Map[Int] = {}\nlet alias = counts\ncounts = counts.set(\"one\", 1)\n",
        "type Row = {value: Int}\nlet values = map.empty().set(\"one\", Row(value: 1))\n",
    ] {
        let parsed = parse_lint_source(source);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let output = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
        assert!(!output.diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.prefer-map-literal")), "{source}");
    }
    let unchecked = Linter::lint(&parsed.arena, source, LintOptions::default());
    assert!(!unchecked.diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.prefer-map-literal")));
}

#[test]
fn linter_map_literal_comment_spans_have_guidance_without_fixes() {
    let source = "var counts: Map[Int] = {}\n# retain initialization note\ncounts = counts.set(\"one\", 1)\nprint counts.len()\n";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty());
    let output = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    let diagnostic = output.diagnostics.iter().find(|d| d.code.as_deref() == Some("lint.prefer-map-literal")).expect("commented initialization warning");
    assert!(diagnostic.fix_hints.is_empty());
}

#[test]
fn linter_list_element_assignment_exact_bounds_rechecks_and_converges() {
    let source = "# café\nvar values: List[Int] = [1, 2, 3]\nvalues = [@values[..1], 8, @values[2..]] # keep\nprint values.len()\n";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let output = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    let diagnostic = output.diagnostics.iter().find(|d| d.code.as_deref() == Some("lint.prefer-list-element-assignment")).unwrap();
    let hint = &diagnostic.fix_hints[0];
    let mut fixed = source.to_string();
    fixed.replace_range(hint.span.unwrap().range(), hint.replacement.as_deref().unwrap());
    assert!(fixed.contains("values[1] = 8 # keep"));
    assert!(fixed.contains("var values: List[Int] = [1, 2, 3]"));
    assert_parse_check_standalone("element assignment", &fixed);
    let formatted = Formatter::new().format_source(SourceId::new(0), &fixed);
    assert!(formatted.diagnostics.is_empty());
    assert_eq!(Formatter::new().format_source(SourceId::new(0), &formatted.formatted).formatted, formatted.formatted);
    let parsed = parse_lint_source(&formatted.formatted);
    let checked = Checker::check_arena(&parsed.arena, &formatted.formatted);
    let second = Linter::lint(&parsed.arena, &formatted.formatted, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    assert!(!second.diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.prefer-list-element-assignment")));
}

#[test]
fn linter_list_element_assignment_refuses_clipped_bounds_effects_and_comments() {
    for source in [
        "var values = [1]\nvalues = [@values[..1], 8, @values[2..]]\n",
        "var values = [1, 2, 3]\nprint values.len()\nvalues = [@values[..1], 8, @values[2..]]\n",
        "pure replacement() -> Int { return 8 }\nvar values = [1, 2, 3]\nvalues = [@values[..1], replacement(), @values[2..]]\n",
        "var values = [1, 2, 3]\nvalues = [@values[..1], # reason\n  8, @values[2..]]\n",
    ] {
        let parsed = parse_lint_source(source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let output = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
        let diagnostic = output.diagnostics.iter().find(|d| d.code.as_deref() == Some("lint.prefer-list-element-assignment")).unwrap();
        assert!(diagnostic.fix_hints.is_empty(), "{source}");
        let unchecked = Linter::lint(&parsed.arena, source, LintOptions::default());
        assert!(!unchecked.diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.prefer-list-element-assignment")));
    }
}

#[test]
fn formatter_list_element_assignment_preserves_nested_selectors_and_comments() {
    let source = "var rows = [{count: 1}]\nrows[if true { # selector\n  0\n} else { 0 }].count += 1 # update\n";
    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(formatted.diagnostics.is_empty(), "{:?}", formatted.diagnostics);
    assert!(formatted.formatted.contains("# selector"));
    assert!(formatted.formatted.contains("# update"));
    assert_parse_check_standalone("formatted nested assignment", &formatted.formatted);
    assert_eq!(Formatter::new().format_source(SourceId::new(0), &formatted.formatted).formatted, formatted.formatted);
}

#[test]
fn linter_nested_record_update_fix_rechecks_and_converges() {
    let source = "let base = {a: {b: 1, c: 2}}\nlet after = {...base, a: {...base.a, b: 3}}\nprint $after.a.b\n";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let output = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    let diagnostic = output.diagnostics.iter().find(|d| d.code.as_deref() == Some("lint.prefer-nested-record-update")).expect("safe nested spread fix");
    let hint = &diagnostic.fix_hints[0];
    let mut fixed = source.to_string();
    fixed.replace_range(hint.span.unwrap().range(), hint.replacement.as_ref().unwrap());
    assert!(fixed.contains("{...base, a.b: 3}"));
    assert_parse_check_standalone("nested record update", &fixed);
    let formatted = Formatter::new().format_source(SourceId::new(0), &fixed);
    assert!(formatted.diagnostics.is_empty());
    let again = Formatter::new().format_source(SourceId::new(0), &formatted.formatted);
    assert_eq!(again.formatted, formatted.formatted);
    let parsed = parse_lint_source(&fixed);
    let checked = Checker::check_arena(&parsed.arena, &fixed);
    let output = Linter::lint(&parsed.arena, &fixed, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    assert!(!output.diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.prefer-nested-record-update")));
}

#[test]
fn linter_nested_record_update_retains_unstable_reads_comments_and_new_fields() {
    for source in [
        "pure change(value: Any) -> Unit { let base = {a: {b: 1}}; let after = {...base, a: {...base.a, b: value}} }\n",
        "var base = {a: {b: 1}}\nlet after = {...base, a: {...base.a, b: 3}}\n",
        "let base = {a: {b: 1}}\nlet after = {...base, a: {...base.a, b: 3 # worker count\n}}\n",
        "let base = {a: {b: 1}}\nlet after = {...base, a: {...base.a, new: 3}}\n",
        "let base = {a: {b: 1}}\nlet after = {...base, a: {...base.a, b: 3}, ...base}\n",
    ] {
        let parsed = parse_lint_source(source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        let output = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
        assert!(!output.diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.prefer-nested-record-update")), "{source}");
    }
}

#[test]
fn pattern_alternatives_adjacent_arm_fix_is_checked_and_idempotent() {
    let source = "enum Event { Added(Str), Changed(Str), Deleted(Str) }\nlet event = Added(\"café\")\nlet selected = match event {\n  Added(name) => name.upper()\n  Changed(name) => name.upper()\n  Deleted(name) => name.upper()\n}\nprint $selected\n";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let output = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    let hints: Vec<_> = output.diagnostics.iter().filter(|d| d.code.as_deref() == Some("lint.identical-match-arms")).flat_map(|d| &d.fix_hints).collect();
    assert_eq!(hints.len(), 1);
    let mut fixed = source.to_string();
    fixed.replace_range(hints[0].span.unwrap().range(), hints[0].replacement.as_deref().unwrap());
    assert!(fixed.contains("Added(name) | Changed(name) | Deleted(name) => name.upper()"));
    assert_parse_check_standalone("combined alternatives", &fixed);
    let formatted = Formatter::new().format_source(SourceId::new(0), &fixed);
    assert!(formatted.diagnostics.is_empty());
    assert_eq!(Formatter::new().format_source(SourceId::new(0), &formatted.formatted).formatted, formatted.formatted);
    let parsed = parse_lint_source(&fixed);
    assert!(!Linter::lint(&parsed.arena, &fixed, LintOptions::default()).diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.identical-match-arms")));
}

#[test]
fn pattern_alternatives_adjacent_arm_fix_retains_guards_comments_and_capture_types() {
    for source in [
        "enum Event { Number(Int), Text(Str) }\nlet result = match Number(1) { Number(value) => 0 Text(value) => 0 }\n",
        "let result = match [1] {\n [left] => 0\n [right, ..] => 0\n _ => 1\n}\n",
        "let result = match 1 { 1 if false => 0 2 => 0 _ => 1 }\n",
        "let result = match 1 { 1 => 0 # keep reason\n 2 => 0 _ => 1 }\n",
    ] {
        let parsed = parse_lint_source(source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let output = Linter::lint(&parsed.arena, source, LintOptions::default());
        assert!(!output.diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.identical-match-arms")), "{source}");
    }
}

#[test]
fn pattern_aliases_formatter_preserves_group_precedence_and_comments() {
    let source = "let selected = match [1] {\n ([name] | [name, ..]) as original => { # whole value\n name + original.len()\n }\n _ => 0\n}\n";
    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(formatted.diagnostics.is_empty(), "{:?}", formatted.diagnostics);
    assert!(formatted.formatted.contains("([name] | [name, ..]) as original"));
    assert!(formatted.formatted.contains("# whole value"));
    assert_parse_check_standalone("formatted alias", &formatted.formatted);
    assert_eq!(Formatter::new().format_source(SourceId::new(0), &formatted.formatted).formatted, formatted.formatted);
}

#[test]
fn value_pipeline_hole_lint_rewrites_safe_nested_and_linear_calls() {
    for source in [
        "pure inner(value: Int) -> Int { value + 1 }\npure outer(prefix: Int, value: Int) -> Int { prefix + value }\nlet selected = outer(10, inner(2))\n",
        "pure inner(value: Int) -> Int { value + 1 }\npure outer(prefix: Int, value: Int) -> Int { prefix + value }\nlet temporary = inner(2)\nlet selected = outer(10, value: temporary)\n",
    ] {
        let parsed = parse_lint_source(source);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let linted = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
        let diagnostic = linted.diagnostics.iter().find(|d| d.code.as_deref() == Some("lint.prefer-value-pipeline")).expect("pipeline suggestion");
        let fix = &diagnostic.fix_hints[0];
        let mut fixed = source.to_string();
        fixed.replace_range(fix.span.unwrap().range(), fix.replacement.as_ref().unwrap());
        assert!(fixed.contains("inner(2) |> outer(10,"), "{fixed}");
        assert_parse_check_standalone("value pipeline fix", &fixed);
        let formatted = Formatter::new().format_source(SourceId::new(0), &fixed);
        assert!(formatted.diagnostics.is_empty());
        assert_eq!(Formatter::new().format_source(SourceId::new(0), &formatted.formatted).formatted, formatted.formatted);
        let parsed = parse_lint_source(&fixed);
        let checked = Checker::check_arena(&parsed.arena, &fixed);
        let again = Linter::lint(&parsed.arena, &fixed, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
        assert!(!again.diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.prefer-value-pipeline")), "{fixed}");
    }
}

#[test]
fn value_pipeline_hole_lint_retains_effect_order_optional_calls_and_context() {
    for source in [
        "proc mark(value: Int) [] -> Int { print $value; value }\npure outer(prefix: Int, value: Int) -> Int { prefix + value }\nlet selected = outer(mark(1), mark(2))\n",
        "pure inner(value: Int) -> Int { value + 1 }\npure outer(prefix: Int, value: Int) -> Int { prefix + value }\nvar prefix = 1\nlet selected = outer(prefix, inner(2))\n",
        "pure other() -> Str { \"other\" }\nlet receiver: Str? = null\nlet selected = receiver?.replace(\"x\", other())\n",
        "pure inner() -> Str { \".\" }\npure converted(value: Path) -> Path { value }\nlet selected = converted(inner())\n",
        "pure inner(value: Int) -> Int { value + 1 }\npure outer(prefix: Int, value: Int) -> Int { prefix + value }\nlet temporary: Int = inner(2)\nlet selected = outer(10, temporary)\n",
    ] {
        let parsed = parse_lint_source(source);
        let checked = Checker::check_arena(&parsed.arena, source);
        let linted = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
        assert!(!linted.diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.prefer-value-pipeline")), "{source}");
    }
    let source = "pure inner(value: Int) -> Int { value + 1 }\npure outer(prefix: Int, value: Int) -> Int { prefix + value }\nlet selected = outer(10, # keep this explanation\n  inner(2))\n";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);
    let linted = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    let diagnostic = linted.diagnostics.iter().find(|d| d.code.as_deref() == Some("lint.prefer-value-pipeline")).expect("manual pipeline suggestion");
    assert!(diagnostic.fix_hints.is_empty());
}

#[test]
fn core_assert_formatter_retains_statement_and_message_comments() {
    let source = "proc check(value: Int) [error] {\n  # café context\n  assert value == 2, f\"value ${value}\" # useful context\n}\n";
    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(formatted.diagnostics.is_empty(), "{:?}", formatted.diagnostics);
    assert_eq!(formatted.formatted, source);
    assert_parse_check_standalone("core assertion", &formatted.formatted);
    let second = Formatter::new().format_source(SourceId::new(0), &formatted.formatted);
    assert_eq!(second.formatted, formatted.formatted);
}

#[test]
#[cfg(feature = "native-tests")]
fn core_assert_lint_fixes_literal_context_and_refuses_eager_or_consumed_results() {
    let source = r#"proc context() [io] -> Str { print "context"; "detail" }
proc assertions(dynamic: Any) [io, error] {
  test.ok(true, "café")?
  test.eq(1 + 1, 2, message: "equality")?
  test.ne("left", "right", "inequality")?
  test.ok(true, context())?
  test.eq(dynamic, 1, "dynamic")?
  let consumed = test.ok(true, "consumed")
  consumed?
  let captured: Result[Unit] = try { test.ok(true, "captured")? }
  let retried: Result[Unit] = retry [] { test.eq(1, 1, "retried")? }
}
"#;
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let options = LintOptions {
        expr_types: checked.expr_types,
        statement_positions: checked.statement_positions,
        ..LintOptions::default()
    };
    let diagnostics = Linter::lint(&parsed.arena, source, options).diagnostics;
    let assertions = diagnostics.iter().filter(|diagnostic| diagnostic.code.as_deref() == Some("lint.core-assert")).collect::<Vec<_>>();
    assert_eq!(assertions.len(), 4);
    assert!(assertions[3].fix_hints.is_empty(), "eager context must retain its effects");
    let mut fixed = source.to_string();
    for diagnostic in assertions[..3].iter().rev() {
        let hint = &diagnostic.fix_hints[0];
        fixed.replace_range(hint.span.unwrap().range(), hint.replacement.as_deref().unwrap());
    }
    assert!(fixed.contains("assert true, \"café\""));
    assert!(fixed.contains("test.ok(true, context())?"));
    assert!(fixed.contains("test.eq(dynamic, 1, \"dynamic\")?"));
    assert!(fixed.contains("let consumed = test.ok(true, \"consumed\")"));
    assert!(fixed.contains("try { test.ok(true, \"captured\")? }"));
    assert!(fixed.contains("retry [] { test.eq(1, 1, \"retried\")? }"));
    assert_parse_check_standalone("core assertion migration", &fixed);
    let parsed = parse_lint_source(&fixed);
    let checked = Checker::check_arena(&parsed.arena, &fixed);
    let second = Linter::lint(&parsed.arena, &fixed, LintOptions {
        expr_types: checked.expr_types,
        statement_positions: checked.statement_positions,
        ..LintOptions::default()
    }).diagnostics;
    let assertions = second.iter().filter(|diagnostic| diagnostic.code.as_deref() == Some("lint.core-assert")).collect::<Vec<_>>();
    assert_eq!(assertions.len(), 1);
    assert!(assertions[0].fix_hints.is_empty());
}

#[test]
fn native_test_declaration_migration_preserves_context_effects_and_is_idempotent() {
    let source = "proc test_exact_name(ctx: TestContext) [error] -> Result[Unit] {\n  test.eq(ctx.name, ctx.name)?\n  return Ok()\n}\n";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let options = LintOptions { native_test_file: true, function_return_types: checked.function_return_types.clone(), ..LintOptions::default() };
    let linted = Linter::lint(&parsed.arena, source, options);
    let diagnostic = linted.diagnostics.iter().find(|d| d.code.as_deref() == Some("lint.legacy-test-proc")).expect("migration diagnostic");
    let mut edits = diagnostic.fix_hints.iter().map(|fix| (fix.span.unwrap(), fix.replacement.as_ref().unwrap())).collect::<Vec<_>>();
    edits.sort_by_key(|(span, _)| span.start());
    let mut rewritten = source.to_owned();
    for (span, replacement) in edits.into_iter().rev() { rewritten.replace_range(span.range(), replacement); }
    assert!(rewritten.starts_with("test test_exact_name [error] { |ctx|"), "{rewritten}");
    assert!(rewritten.contains("return Ok()"));
    let parsed = parse_lint_source(&rewritten);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, &rewritten);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let second = Linter::lint(&parsed.arena, &rewritten, LintOptions { native_test_file: true, function_return_types: checked.function_return_types, ..LintOptions::default() });
    assert!(!second.diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.legacy-test-proc")));
    let formatted = Formatter::new().format_source(SourceId::new(0), &rewritten);
    assert!(formatted.diagnostics.is_empty());
    assert!(formatted.formatted.contains("test test_exact_name [error] { |ctx|"));
    let second = Formatter::new().format_source(SourceId::new(0), &formatted.formatted);
    assert_eq!(formatted.formatted, second.formatted);
}

#[test]
fn native_test_declaration_migration_declines_callers_and_ordinary_files() {
    for source in [
        "proc test_called() {}\nproc caller() { test_called() }\n",
    ] {
        let parsed = parse_lint_source(source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let linted = Linter::lint(&parsed.arena, source, LintOptions { native_test_file: true, function_return_types: checked.function_return_types.clone(), ..LintOptions::default() });
        let migration = linted.diagnostics.iter().find(|d| d.code.as_deref() == Some("lint.legacy-test-proc")).expect("manual migration diagnostic");
        assert!(migration.fix_hints.is_empty());
        let ordinary = Linter::lint(&parsed.arena, source, LintOptions { function_return_types: checked.function_return_types, ..LintOptions::default() });
        assert!(!ordinary.diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.legacy-test-proc")));
    }
}

#[test]
fn formatter_enum_declarations_are_canonical_and_idempotent() {
    let source = "enum Token { Present(Str), }\ntype Alias = Token\n";
    let first = Formatter::new().format_source(SourceId::new(0), source);
    assert!(first.diagnostics.is_empty(), "{:?}", first.diagnostics);
    assert!(first.formatted.contains("enum Token { Present(Str) }"));
    assert!(first.formatted.contains("type Alias = Token"));
    let second = Formatter::new().format_source(SourceId::new(0), &first.formatted);
    assert_eq!(first.formatted, second.formatted);
}

#[test]
fn formatter_enum_comments_remain_with_their_variants() {
    let declaration = "export enum Choice {\n  Selected(Int), # payload café\n  # absent choice\n  Empty,\n}";
    let source = format!("{declaration}\ntype Alias = Choice\n");
    let first = Formatter::new().format_source(SourceId::new(0), &source);
    assert!(first.diagnostics.is_empty(), "{:?}", first.diagnostics);
    assert!(first.formatted.contains(declaration), "{}", first.formatted);
    let second = Formatter::new().format_source(SourceId::new(0), &first.formatted);
    assert_eq!(first.formatted, second.formatted);
}

#[test]
fn linter_does_not_add_selective_filters_to_unconditional_retry() {
    let source = "error FetchError = Busy(message: Str) | Fatal(message: Str)\nproc attempt() -> Result[Str, FetchError] { Err(FetchError.Busy(message: \"busy\")) }\nlet result = retry [0ms] { attempt()? }\n";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(!checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == xsh::diagnostic::Severity::Error), "{:?}", checked.diagnostics);
    let diagnostics = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() }).diagnostics;
    assert!(diagnostics.iter().flat_map(|diagnostic| &diagnostic.fix_hints).filter_map(|hint| hint.replacement.as_ref()).all(|replacement| !replacement.contains(" on (")));
}

#[test]
fn linter_preserves_manual_selective_loop_with_observable_counter_and_delay() {
    let source = "error FetchError = Busy(message: Str) | Fatal(message: Str)\nproc attempt() -> Result[Str, FetchError] { Err(FetchError.Busy(message: \"busy\")) }\nvar attempts = 0\nlet result = loop {\n  attempts += 1\n  let result = attempt()\n  match result {\n    Ok(value) => break Ok(value)\n    Err(error) => {\n      break Err(error) unless error is FetchError.Busy\n      break Err(error) when attempts == 2\n      time.sleep(0ms)\n    }\n  }\n}\nprint ${attempts}\n";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(!checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == xsh::diagnostic::Severity::Error), "{:?}", checked.diagnostics);
    let diagnostics = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() }).diagnostics;
    assert!(diagnostics.iter().flat_map(|diagnostic| &diagnostic.fix_hints).filter_map(|hint| hint.replacement.as_ref()).all(|replacement| !replacement.contains("retry")));
}

#[test]
fn duration_arithmetic_conversion_fix_rechecks_and_converges() {
    let source = "let pause = time.millis(250)\nlet budget = time.seconds(2)\n";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let output = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    let mut fixes = output.diagnostics.iter().filter(|d| d.code.as_deref() == Some("lint.duration-arithmetic"))
        .map(|d| &d.fix_hints[0]).collect::<Vec<_>>();
    assert_eq!(fixes.len(), 2);
    fixes.sort_by_key(|fix| std::cmp::Reverse(fix.span.unwrap().start()));
    let mut fixed = source.to_string();
    for fix in fixes { fixed.replace_range(fix.span.unwrap().range(), fix.replacement.as_ref().unwrap()); }
    assert_parse_check_standalone("Duration conversion", &fixed);
    assert!(fixed.contains("250 * 1ms"));
    assert!(fixed.contains("2 * 1s"));
    let formatted = Formatter::new().format_source(SourceId::new(0), &fixed);
    let parsed = parse_lint_source(&formatted.formatted);
    let checked = Checker::check_arena(&parsed.arena, &formatted.formatted);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let output = Linter::lint(&parsed.arena, &formatted.formatted, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    assert!(!output.diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.duration-arithmetic")));
    assert_eq!(Formatter::new().format_source(SourceId::new(0), &formatted.formatted).formatted, formatted.formatted);
}

#[test]
fn duration_arithmetic_conversion_retains_clamping_saturation_unknowns_and_comments() {
    let source = "pure convert(value: Int) -> Duration { time.millis(value) }\nlet negative = time.millis(-1)\nlet saturated = time.seconds(9223372036854775807)\nlet commented = time.seconds(\n  # retain conversion annotation\n  2\n)\n";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let output = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    assert!(!output.diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.duration-arithmetic")));
    let output = Linter::lint(&parsed.arena, source, LintOptions::default());
    assert!(!output.diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.duration-arithmetic")));
}

#[test]
fn duration_arithmetic_conversion_refuses_custom_module_alias() {
    let root = TempDir::new().unwrap();
    let entry = root.path().join("entry.xsh");
    fs::write(root.path().join("helper.xsh"), "##! Custom duration conversion.\n## Returns a fixed duration independent of the count.\nexport pure millis(count: Int) -> Duration { let _ = count; 2s }\n").unwrap();
    let source = "use helper as time\nlet pause = time.millis(250)\n";
    let loaded = parse_load_check_text(entry.to_str().unwrap(), source.to_string(), Vec::new(), Default::default());
    assert!(loaded.parsed.diagnostics.is_empty(), "{:?}", loaded.parsed.diagnostics);
    let checked = loaded.checked.unwrap();
    assert!(checked.diagnostics.iter().any(|d| d.code.as_deref() == Some("check.standard-module-shadow")), "{:?}", checked.diagnostics);
    let output = Linter::lint(&loaded.parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    assert!(!output.diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.duration-arithmetic")));
}

#[test]
fn block_string_concatenation_fix_rechecks_exact_bytes_and_converges() {
    let source = "let value = \"first\\n\" + \"  café\\n\"\nprint $value\n";
    let parsed = parse_lint_source(source);
    let diagnostics = Linter::lint(&parsed.arena, source, LintOptions::default()).diagnostics;
    let fix = diagnostics.iter().find(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-block-string")).unwrap().fix_hints.first().unwrap();
    let mut fixed = source.to_string();
    fixed.replace_range(fix.span.unwrap().range(), fix.replacement.as_ref().unwrap());
    assert_parse_check_standalone("block string fix", &fixed);
    let parsed = parse_lint_source(&fixed);
    let statement = parsed.arena.statement_ids().next().unwrap();
    let xsh::frontend::syntax::arena::ArenaStmtKind::Let { initializer: xsh::frontend::syntax::arena::ArenaExprOrRun::Expr(value), .. } = parsed.arena.arena.stmt(statement).kind else { panic!("binding"); };
    let xsh::frontend::syntax::arena::ArenaExprKind::Str(text) = parsed.arena.arena.expr(value).kind else { panic!("block string"); };
    assert_eq!(parsed.arena.arena.string_literal(text).as_ref(), "first\n  café\n");
    assert!(!Linter::lint(&parsed.arena, &fixed, LintOptions::default()).diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-block-string")));
    let formatted = Formatter::new().format_source(SourceId::new(0), &fixed);
    assert!(formatted.diagnostics.is_empty());
    assert_parse_check_standalone("formatted block string fix", &formatted.formatted);
    assert_eq!(formatted.formatted, Formatter::new().format_source(SourceId::new(0), &formatted.formatted).formatted);
}

#[test]
fn block_string_concatenation_fix_retains_dynamic_interpolation_comments_crlf_and_consumers() {
    for source in [
        "let value = \"first\\n\" + dynamic\n",
        "let value = \"first\\n\" + f\"${dynamic}\"\n",
        "let value = \"first\\n\" + \"second\" # retain\n",
        "let value = \"first\\r\\n\" + \"second\"\n",
        "print (\"first\\n\" + \"second\")\n",
    ] {
        let parsed = parse_lint_source(source);
        assert!(!Linter::lint(&parsed.arena, source, LintOptions::default()).diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-block-string")), "{source}");
    }
}

#[test]
fn try_capture_migration_does_not_erase_retry_metadata_or_lexical_returns() {
    let source = "proc outer() -> Result[Int] {\n  let value = retry [] {\n    return Ok(7)\n  }?\n  value\n}\nlet nested = retry [] { Ok(7) }\n";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let diagnostics = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() }).diagnostics;
    for diagnostic in diagnostics {
        for fix in diagnostic.fix_hints {
            assert!(!fix.replacement.as_deref().is_some_and(|replacement| replacement.contains("try ")), "unsafe local capture migration: {fix:?}");
        }
    }
}

#[test]
fn prepared_constant_fix_preserves_comments_and_converges() {
    let source = "# protocol\nlet version = 1 # stable\nlet values: List[Int] = []\nlet runtime = 1 / 0\nlet ordinary = version\npure helper(input: Int) -> Int { let local = 2; input + local }\n";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let output = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    let fixes: Vec<_> = output.diagnostics.iter().filter(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-const")).collect();
    assert_eq!(fixes.len(), 2);
    let mut fixed = source.to_string();
    for diagnostic in fixes.iter().rev() {
        let fix = &diagnostic.fix_hints[0];
        fixed.replace_range(fix.span.unwrap().range(), fix.replacement.as_deref().unwrap());
    }
    assert!(fixed.contains("const version = 1 # stable"));
    assert!(fixed.contains("let runtime = 1 / 0"));
    assert!(fixed.contains("let ordinary = version"));
    assert!(fixed.contains("let local = 2"));
    let formatted = Formatter::new().format_source(SourceId::new(0), &fixed);
    assert!(formatted.diagnostics.is_empty());
    assert!(formatted.formatted.contains("const version = 1 # stable"));
    let second = parse_lint_source(&fixed);
    let checked = Checker::check_arena(&second.arena, &fixed);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let output = Linter::lint(&second.arena, &fixed, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    assert!(!output.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-const")));
}

#[test]
fn linter_named_argument_spread_requires_exact_stable_visible_fields_and_converges() {
    let source = include_str!("../../../tests/fixtures/syntax/valid/named-argument-forwarding.xsh");
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let diagnostics = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() }).diagnostics;
    let forwards = diagnostics.iter().filter(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-named-argument-spread")).collect::<Vec<_>>();
    assert_eq!(forwards.len(), 3, "partial records and effectful receivers must not forward");
    assert_eq!(forwards[0].fix_hints.len(), 1);
    assert!(forwards[1].fix_hints.is_empty(), "comments remain intact");
    assert!(forwards[2].fix_hints.is_empty(), "mutable receivers retain repeated reads");
    let fix = &forwards[0].fix_hints[0];
    assert_eq!(fix.replacement.as_deref(), Some("...options"));
    let mut candidate = source.to_string();
    candidate.replace_range(fix.span.unwrap().range(), fix.replacement.as_ref().unwrap());
    assert_parse_check_standalone("named argument spreading", &candidate);
    let formatted = Formatter::new().format_source(SourceId::new(0), &candidate);
    assert!(formatted.diagnostics.is_empty(), "{:?}", formatted.diagnostics);
    let again = Formatter::new().format_source(SourceId::new(0), &formatted.formatted);
    assert_eq!(again.formatted, formatted.formatted);
    let parsed = parse_lint_source(&formatted.formatted);
    let checked = Checker::check_arena(&parsed.arena, &formatted.formatted);
    let second = Linter::lint(&parsed.arena, &formatted.formatted, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() }).diagnostics;
    assert!(second.iter().filter(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-named-argument-spread")).all(|diagnostic| diagnostic.fix_hints.is_empty()));
}

#[test]
fn linter_named_argument_spread_requires_checked_record_facts() {
    let source = include_str!("../../../tests/fixtures/syntax/valid/named-argument-forwarding.xsh");
    let parsed = parse_lint_source(source);
    let diagnostics = Linter::lint(&parsed.arena, source, LintOptions::default()).diagnostics;
    assert!(!diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-named-argument-spread")));
}

#[test]
fn lexical_ctx_formatter_preserves_value_and_label_and_converges() {
    let source = "let value = ctx f\"operation ${1 + 2}\" {\n  # retain region explanation\n  7\n}\nprint $value\n";
    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(formatted.diagnostics.is_empty(), "{:?}", formatted.diagnostics);
    assert!(formatted.formatted.contains("ctx f\"operation ${1 + 2}\""));
    assert!(formatted.formatted.contains("# retain region explanation"));
    assert_parse_check_standalone("context block", &formatted.formatted);
    assert_eq!(Formatter::new().format_source(SourceId::new(0), &formatted.formatted).formatted, formatted.formatted);
}

#[test]
fn lexical_ctx_linter_visits_label_effects_and_body_bindings() {
    let source = "proc label() [io] -> Str { print \"label\"; \"operation\" }\nproc operation() [io] -> Unit { ctx label() { print \"body\" } }\noperation()\n";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let output = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    assert!(!output.diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.unused-binding")), "{:?}", output.diagnostics);
}

#[test]
fn typed_map_keys_formatter_and_checked_literal_fix_converge() {
    let source = "type Identifier = Int\nvar values: Map[Identifier, Str] = {}\nvalues = values.set(20, \"twenty\")\nvalues = values.set(3, \"three\")\nprint values.len()\n";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let output = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    let hint = output.diagnostics.iter().filter(|d| d.code.as_deref() == Some("lint.prefer-map-literal")).flat_map(|d| &d.fix_hints).max_by_key(|h| h.span.unwrap().range().len()).expect("typed Map literal fix");
    let mut fixed = source.to_string();
    fixed.replace_range(hint.span.unwrap().range(), hint.replacement.as_ref().unwrap());
    assert!(fixed.contains("var values: Map[Identifier, Str] = {[20]: \"twenty\", [3]: \"three\"}"));
    assert_parse_check_standalone("typed Map construction", &fixed);
    let formatted = Formatter::new().format_source(SourceId::new(0), &fixed);
    assert!(formatted.diagnostics.is_empty());
    assert_eq!(Formatter::new().format_source(SourceId::new(0), &formatted.formatted).formatted, formatted.formatted);
    let parsed = parse_lint_source(&formatted.formatted);
    let checked = Checker::check_arena(&parsed.arena, &formatted.formatted);
    let second = Linter::lint(&parsed.arena, &formatted.formatted, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    assert!(!second.diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.prefer-map-literal")));
    let source = "let values = {[key + 1]: value for {key, value} in {[1]: 2}}\n";
    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(formatted.diagnostics.is_empty());
    assert_parse_check_standalone("computed comprehension key", &formatted.formatted);
    assert_eq!(Formatter::new().format_source(SourceId::new(0), &formatted.formatted).formatted, formatted.formatted);
}

#[test]
fn parametric_record_constructor_fix_keeps_concrete_alias_and_converges() {
    let source = "type Box[T] = {value: T}\ntype Count = Box[Int]\nlet count: Count = {value: 7}\nprint ${count.value + 1}\n";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let output = Linter::lint(&parsed.arena, source, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    let diagnostic = output.diagnostics.iter().find(|d| d.code.as_deref() == Some("lint.prefer-record-constructor")).unwrap();
    let hint = &diagnostic.fix_hints[0];
    let mut fixed = source.to_string();
    fixed.replace_range(hint.span.unwrap().range(), hint.replacement.as_deref().unwrap());
    assert!(fixed.contains("let count: Count = Count(value: 7)"));
    assert_parse_check_standalone("concrete schema alias", &fixed);
    let formatted = Formatter::new().format_source(SourceId::new(0), &fixed);
    assert!(formatted.diagnostics.is_empty());
    assert_eq!(Formatter::new().format_source(SourceId::new(0), &formatted.formatted).formatted, formatted.formatted);
    let parsed = parse_lint_source(&fixed);
    let checked = Checker::check_arena(&parsed.arena, &fixed);
    let repeated = Linter::lint(&parsed.arena, &fixed, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    assert!(!repeated.diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.prefer-record-constructor")));
    let direct = "type Box[T] = {value: T}\nlet count: Box[Int] = {value: 7}\n";
    let parsed = parse_lint_source(direct);
    let checked = Checker::check_arena(&parsed.arena, direct);
    let output = Linter::lint(&parsed.arena, direct, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() });
    assert!(!output.diagnostics.iter().any(|d| d.code.as_deref() == Some("lint.prefer-record-constructor")));
}

#[test]
fn private_proc_effects_lint_does_not_reinsert_inferred_annotations() {
    let source = "proc clock() -> Int { let _ = time.now(); 42 }\nproc forwarding() -> Int { clock() }\n";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let linted = Linter::lint(&parsed.arena, source, LintOptions {
        function_effect_facts: checked.function_effect_facts,
        ..LintOptions::default()
    });
    assert!(!linted.diagnostics.iter().any(|diagnostic| matches!(diagnostic.code.as_deref(), Some("lint.unannotated-effects" | "lint.missing-effects"))));
}

#[test]
fn private_proc_effects_removal_is_opt_in_checked_and_convergent() {
    let source = "proc clock() [time] -> Int { let _ = time.now(); 42 }\nlet value = clock()\n";
    let parsed = parse_lint_source(source);
    let disabled = Linter::lint(&parsed.arena, source, LintOptions::default());
    assert!(!disabled.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-inferred-private-effects")));
    let enabled = Linter::lint(&parsed.arena, source, LintOptions { prefer_inferred_private_effects: true, ..LintOptions::default() });
    let diagnostic = enabled.diagnostics.iter().find(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-inferred-private-effects")).expect("equivalent private effect removal");
    let span = diagnostic.fix_hints[0].span.unwrap();
    let mut fixed = source.to_string();
    fixed.replace_range(span.start()..span.end(), "");
    assert_parse_check_standalone("inferred effects", &fixed);
    let parsed = parse_lint_source(&fixed);
    let second = Linter::lint(&parsed.arena, &fixed, LintOptions { prefer_inferred_private_effects: true, ..LintOptions::default() });
    assert!(!second.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-inferred-private-effects")));
}

#[test]
fn private_proc_effects_removal_retains_bounds_docs_and_entry_contracts() {
    for source in [
        "proc deliberate() [time] -> Int { 42 }\n",
        "# Checked clock boundary.\nproc documented() [time] -> Int { let _ = time.now(); 42 }\n",
        "proc main() [time] -> Int { let _ = time.now(); 42 }\n",
        "test registered [error] { assert true, \"checked\" }\n",
        "##! Public boundary.\n## Clock.\nexport proc published() [time] -> Int { let _ = time.now(); 42 }\n",
        "proc dynamic(callback: Proc) [io] -> Int { let _ = callback.call(); 42 }\n",
    ] {
        let parsed = parse_lint_source(source);
        let linted = Linter::lint(&parsed.arena, source, LintOptions { prefer_inferred_private_effects: true, ..LintOptions::default() });
        assert!(!linted.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-inferred-private-effects")), "{source}");
    }

}
