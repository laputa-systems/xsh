#![allow(clippy::single_call_fn)]

use std::sync::Arc;

use xsh::diagnostic::Diagnostic;
use xsh::frontend::check::Checker;
use xsh::frontend::source::SourceId;
use xsh::frontend::symbols::{Name, SymbolOwner};
use xsh::frontend::syntax::arena::{ArenaProgram, ArenaProgramBuilder};
use xsh::frontend::syntax::parser::{ArenaParseOutput, Parser};
use xsht::format::Formatter;
use xsht::lint::{LintOptions, Linter};

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
        Some("var names = [item.trim() for item in items if item != \"\"]\n")
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
        Some("var by_key = {bucket.key: bucket.items for bucket in buckets}\n")
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
fn linter_autofixes_unreachable_return_after_all_returning_match() {
    let source = "\
type Tok = TOp(Str) | TEOF

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
        codes.contains(&"lint.unreachable-after-match"),
        "expected lint.unreachable-after-match in {codes:?}"
    );
}

#[test]
fn linter_suggests_multiline_tag_union() {
    let source = "type Tok = A | B | C | D | E\n";
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
        fixed.contains("type Tok =\n"),
        "fixed text should contain multiline type def, got:\n{fixed}"
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
