#![allow(clippy::single_call_fn)]

use std::fs;
use std::sync::Arc;
use xsh::diagnostic::DiagnosticCode;

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
fn linter_owns_qualified_enum_symbols_without_a_caller_scope() {
    let source =
        "enum Choice: Str { Selected = \"selected\", Empty = \"\" }\nexport type Alias = Choice\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty());
    let mut program = parsed.arena;
    program.root_nominal_namespace = Some(
        program
            .symbol_owner()
            .with_current(|| Name::intern("linter_enum_scope")),
    );
    assert!(SymbolOwner::current().is_none());
    for _ in 0..2 {
        let output = Linter::lint(&program, source, LintOptions::default());
        assert!(
            output
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.severity != xsh::diagnostic::Severity::Error)
        );
        let output = Linter::lint_module(&program, source, LintOptions::default());
        assert!(
            output
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.severity != xsh::diagnostic::Severity::Error)
        );
        assert!(SymbolOwner::current().is_none());
    }
    assert_eq!(
        program
            .symbol_owner()
            .with_current(|| Name::intern("linter_enum_scope.Choice"))
            .as_str(),
        "linter_enum_scope.Choice"
    );
}

#[test]
fn callable_alias_forwarder_fix_preserves_signature_and_converges() {
    let source = "pure render(value: Str, prefix: Str = \"label:\") -> Str { prefix + value }\nexport pure format(value: Str, prefix: Str = \"label:\") -> Str { render(value, prefix) }\nprint format(value: \"one\")\n";
    let parsed = parse_lint_source(source);
    let output = Linter::lint(&parsed.arena, source, LintOptions::default());
    let diagnostic = output
        .diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.prefer-callable-alias")
        })
        .expect("exact forwarder");
    let fix = &diagnostic.fix_hints[0];
    let mut fixed = source.to_owned();
    fixed.replace_range(fix.span.unwrap().range(), fix.replacement.as_ref().unwrap());
    assert!(fixed.contains("export let format = render"));
    assert_parse_check_standalone("callable alias", &fixed);
    let parsed = parse_lint_source(&fixed);
    assert!(
        !Linter::lint(&parsed.arena, &fixed, LintOptions::default())
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                == Some("lint.prefer-callable-alias"))
    );
}

#[test]
fn callable_alias_forwarder_fix_retains_policy_comments_and_argument_order() {
    for source in [
        "pure render(value: Str) -> Str { value }; pure format(value: Str) -> Str { # preserve context\n render(value) }\n",
        "pure render(left: Str, right: Str) -> Str { left + right }; pure format(left: Str, right: Str) -> Str { render(right, left) }\n",
        "pure render(value: Str) -> Str { value }; pure format(value: Str) -> Str { render(value.trim()) }\n",
        "proc render(value: Str) [error] -> Result[Str] { Ok(value) }; proc format(value: Str) [error] -> Result[Str] { render(value)? }\n",
    ] {
        let parsed = parse_lint_source(source);
        let output = Linter::lint(&parsed.arena, source, LintOptions::default());
        assert!(
            output
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                    == Some("lint.prefer-callable-alias"))
                .all(|diagnostic| diagnostic.fix_hints.is_empty()),
            "{source}"
        );
    }
}

#[test]
fn fs_root_receiver_cli_fix_checks_an_isolated_fixture_and_converges() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("root-receiver.xsh");
    fs::write(&path, "proc old(root: FsRoot) [fs, error] {\n  fs.root_mkdir(root, p\"nested\", parents: true)?\n}\n").unwrap();
    let files = vec![path.to_string_lossy().into_owned()];
    let result = xsht::lint_files(&files, true, false, None);
    assert_eq!(
        result.status,
        0,
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let first = fs::read_to_string(&path).unwrap();
    // A later round removes the `?`: a statement-position `Result[Unit]`
    // already propagates its failure.
    assert!(
        first.contains("  root.mkdir(p\"nested\", parents: true)\n"),
        "{first}"
    );
    let second = xsht::lint_files(&files, true, false, None);
    assert!(
        second.status <= 1,
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert!(!String::from_utf8_lossy(&second.stderr).contains("lint.fs-root-receiver"));
    assert_eq!(fs::read_to_string(&path).unwrap(), first);
}

#[test]
fn fs_root_receiver_fix_preserves_named_argument_text_and_refuses_reordered_receiver() {
    for (source, expected) in [
        (
            "proc old(root: FsRoot) [fs, error] {\n  fs.root_mkdir(root, p\"nested\", parents: true)?\n}\n",
            Some("root.mkdir(p\"nested\", parents: true)"),
        ),
        (
            "proc old(root: FsRoot) [fs, error] {\n  fs.root_write(data: \"text\", root: root, path: p\"data\")?\n}\n",
            None,
        ),
        (
            "proc old(root: FsRoot) [fs, error] {\n  fs.root_write(\n    root,\n    p\"data\",\n    b\"#bytes\",\n  )?\n}\n",
            Some("root.write(\n    p\"data\",\n    b\"#bytes\",\n  )"),
        ),
        (
            "proc old(root: FsRoot) [fs, error] {\n  fs.root_mkdir(root, # retain this ownership comment\n    p\"nested\")?\n}\n",
            None,
        ),
    ] {
        let parsed = parse_lint_source(source);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(
            checked
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                    == Some("check.unsupported-api"))
        );
        let diagnostics = Linter::lint(
            &parsed.arena,
            source,
            LintOptions {
                expr_types: checked.expr_types,
                ..LintOptions::default()
            },
        )
        .diagnostics;
        let fixes = diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.code.map(DiagnosticCode::name) == Some("lint.fs-root-receiver")
            })
            .flat_map(|diagnostic| diagnostic.fix_hints.iter())
            .collect::<Vec<_>>();
        if let Some(expected) = expected {
            assert_eq!(fixes.len(), 1);
            assert_eq!(fixes[0].replacement.as_deref(), Some(expected));
            let mut fixed = source.to_string();
            fixed.replace_range(fixes[0].span.unwrap().range(), expected);
            assert_parse_check_standalone("root receiver", &fixed);
            let parsed = parse_lint_source(&fixed);
            let checked = Checker::check_arena(&parsed.arena, &fixed);
            assert!(
                !Linter::lint(
                    &parsed.arena,
                    &fixed,
                    LintOptions {
                        expr_types: checked.expr_types,
                        ..LintOptions::default()
                    }
                )
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                    == Some("lint.fs-root-receiver"))
            );
        } else {
            assert!(fixes.is_empty());
        }
    }
}

#[test]
fn fs_root_receiver_refuses_user_record_methods_and_forged_capabilities() {
    for source in [
        "proc old(root: {id: Int}) [fs, error] { fs.root_read(root, p\"data\")? }\n",
        "let fs = {root_read: pure(root: Int, path: Path) -> Int { root }}\nlet _ = fs.root_read(1, p\"data\")\n",
    ] {
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
        assert!(
            !diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                    == Some("lint.fs-root-receiver"))
        );
    }
}

#[test]
fn stage_callable_wrapper_fix_rechecks_and_converges() {
    let source = "pure increment(value: Int) -> Int { value + 1 }\nlet values = [1, 2] |> map { |item| increment(item) }\nprint values.len()\n";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let diagnostics = Linter::lint(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            statically_resolved_call_spans: checked.statically_resolved_call_spans,
            ..LintOptions::default()
        },
    )
    .diagnostics;
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code.map(DiagnosticCode::name) == Some("lint.stage-callable"))
        .expect("transparent wrapper");
    let fix = &diagnostic.fix_hints[0];
    let mut fixed = source.to_string();
    fixed.replace_range(fix.span.unwrap().range(), fix.replacement.as_ref().unwrap());
    assert!(fixed.contains("|> map(increment)"));
    assert_parse_check_standalone("stage callable", &fixed);
    let formatted = Formatter::new().format_source(SourceId::new(0), &fixed);
    assert_eq!(
        Formatter::new()
            .format_source(SourceId::new(0), &formatted.formatted)
            .formatted,
        formatted.formatted
    );
    let parsed = parse_lint_source(&fixed);
    let checked = Checker::check_arena(&parsed.arena, &fixed);
    let diagnostics = Linter::lint(
        &parsed.arena,
        &fixed,
        LintOptions {
            statically_resolved_call_spans: checked.statically_resolved_call_spans,
            ..LintOptions::default()
        },
    )
    .diagnostics;
    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                == Some("lint.stage-callable"))
    );
}

#[test]
fn stage_callable_wrapper_fix_requires_exact_item_stable_name_and_no_propagation() {
    for source in [
        "pure f(value: Int, amount: Int = 1) -> Int { value + amount }\nlet _ = [1] |> map { |item| f(item, 2) }\n",
        "pure f(value: Int) -> Int { value }\nlet _ = [1] |> map { |item| f(item + 1) }\n",
        "pure f(value: Int) -> Int { value }\nlet _ = [1] |> map { |item| # preserve explanation\n f(item) }\n",
        "pure f(value: Int) -> Int { value }\nlet _ = [1] |> map { |item| let copy = item; f(copy) }\n",
        "pure f(value: Int) -> Result[Int] { Ok(value) }\nproc main() [error] { let _ = [1] |> map { |item| f(item)? } }\n",
        "pure f(value: Int) -> Int { value }\nproc apply(f: Pure) [] { let _ = [1] |> map { |item| f(item) } }\n",
    ] {
        let parsed = parse_lint_source(source);
        let checked = Checker::check_arena(&parsed.arena, source);
        let diagnostics = Linter::lint(
            &parsed.arena,
            source,
            LintOptions {
                statically_resolved_call_spans: checked.statically_resolved_call_spans,
                ..LintOptions::default()
            },
        )
        .diagnostics;
        assert!(
            !diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                    == Some("lint.stage-callable")),
            "{source}"
        );
    }
}

#[test]
fn boolean_guard_fix_keeps_failure_body_comments_and_converges() {
    let source = "proc validate(jobs: Int) [error] {\n  if jobs <= 0 {\n    # Preserve domain error identity.\n    return error.fail(\"jobs must be positive\")\n  }\n\n  let _ = jobs\n}\n";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let diagnostics = Linter::lint(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            definitely_exiting_block_spans: checked.definitely_exiting_block_spans,
            redundant_variant_qualifiers: checked.redundant_variant_qualifiers,
            ..LintOptions::default()
        },
    )
    .diagnostics;
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code.map(DiagnosticCode::name) == Some("lint.boolean-guard"))
        .expect("checked guard rewrite");
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
    let second = Linter::lint(
        &parsed.arena,
        &fixed,
        LintOptions {
            expr_types: checked.expr_types,
            definitely_exiting_block_spans: checked.definitely_exiting_block_spans,
            redundant_variant_qualifiers: checked.redundant_variant_qualifiers,
            ..LintOptions::default()
        },
    )
    .diagnostics;
    assert!(
        !second
            .iter()
            .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                == Some("lint.boolean-guard"))
    );
}

#[test]
fn boolean_guard_float_fix_retains_nan_negation() {
    let source =
        "pure positive(value: Float) -> Bool {\n  if value <= 0.0 { return false }\n  true\n}\n";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let diagnostics = Linter::lint(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            definitely_exiting_block_spans: checked.definitely_exiting_block_spans,
            redundant_variant_qualifiers: checked.redundant_variant_qualifiers,
            ..LintOptions::default()
        },
    )
    .diagnostics;
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code.map(DiagnosticCode::name) == Some("lint.boolean-guard"))
        .unwrap();
    let replacement = diagnostic.fix_hints[0].replacement.as_ref().unwrap();
    assert!(
        replacement.starts_with("guard ! (value <= 0.0) else"),
        "{replacement}"
    );
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
        let options = LintOptions {
            expr_types: checked.expr_types,
            definitely_exiting_block_spans: checked.definitely_exiting_block_spans,
            redundant_variant_qualifiers: checked.redundant_variant_qualifiers,
            ..LintOptions::default()
        };
        for options in [options, LintOptions::default()] {
            let diagnostics = Linter::lint(&parsed.arena, source, options).diagnostics;
            assert!(
                !diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                        == Some("lint.boolean-guard")),
                "{source}"
            );
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
            let label = diagnostic
                .code
                .map(DiagnosticCode::name)
                .unwrap_or("lint fix");
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
  fp\"{root}/src/lib\".mkdir(parents: true)?
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
        .map(|diagnostic| diagnostic.code.map(DiagnosticCode::name).unwrap())
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
proc load() [fs] {
  let _ = Path(\"x\").read_text()?
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
        .find(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.missing-effects")
        })
        .expect("expected missing effects lint");

    assert_eq!(diagnostic.fix_hints.len(), 1);
    assert_eq!(
        diagnostic.fix_hints[0].replacement.as_deref(),
        Some("[fs, error]")
    );
}

fn effect_annotation_lints(source: &str) -> Vec<(String, String)> {
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    Linter::lint(
        &parsed.arena,
        source,
        LintOptions {
            native_test_file: true,
            ..LintOptions::default()
        },
    )
    .diagnostics
    .into_iter()
    .filter(|diagnostic| {
        diagnostic.code.map(DiagnosticCode::name) == Some("lint.missing-effects")
    })
    .map(|diagnostic| {
        (
            diagnostic.code.unwrap().name().to_owned(),
            diagnostic.message,
        )
    })
    .collect()
}

#[test]
fn effect_annotation_lints_skip_unrestricted_entrypoints() {
    let test_and_main = "\
test test_reads_clock { |_ctx|
  let _ = time.now()
}

proc main() {
  let _ = time.now()
}
";
    assert_eq!(effect_annotation_lints(test_and_main), []);
    let partial_clause = "test test_reads_clock [] { |_ctx|\n  let _ = time.now()\n}\n";
    let parsed = parse_lint_source(partial_clause);
    assert!(
        !Checker::check_arena(&parsed.arena, partial_clause)
            .diagnostics
            .is_empty(),
        "present clauses remain upper bounds"
    );
    let cli_main = "cli main(count: Int) {\n  let _ = time.now()\n  print $count\n}\n";
    assert_eq!(effect_annotation_lints(cli_main), []);
}

#[test]
fn effect_annotation_lints_leave_inferred_exports_streams_and_main_alone() {
    let source = "\
##! Effect contract fixture.
## Reads the clock.
export proc stamp() -> Int {
  let _ = time.now()
  1
}

stream ticks() -> Stream[Int] {
  let _ = time.now()
  yield 1
}

## Reads the clock under the conventional entry name.
export proc main() {
  let _ = time.now()
}

for tick in ticks() {
  print ${stamp() + tick}
}
";
    assert_eq!(effect_annotation_lints(source), []);
}

#[test]
fn linter_reports_missing_effects_from_called_restricted_proc() {
    let source = "\
proc timestamp() [time] -> Int {
  time.now()
}

proc stamp() [] -> Int {
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
        .find(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.missing-effects")
        })
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

proc build() [] -> Int {
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
            .find(|diagnostic| {
                diagnostic.code.map(DiagnosticCode::name) == Some("lint.missing-effects")
            })
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
  print f\"dir={dir}\"
}
";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, source, LintOptions::default());

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.unused-local")
                && diagnostic.message.contains("`unused`")
        }),
        "genuinely unused local should still be reported: {diagnostics:?}"
    );
    assert!(
        !diagnostics.iter().any(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.unused-local")
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
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.unused-local")
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

export proc public() -> Result[Unit, Error] {
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
        .map(|diagnostic| diagnostic.code.map(DiagnosticCode::name).unwrap())
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
        .find(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.redundant-ok-tail")
        })
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
        .find(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.redundant-ok-tail")
        })
        .expect("expected redundant tail Ok diagnostic");

    assert!(diagnostic.fix_hints.is_empty());
}

#[test]
fn linter_autofixes_redundant_tail_return_binding() {
    let source = "\
proc overlap(left: List[Str], right: List[Str]) -> List[Str] {
  var values = [item for item in left if item in right]
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
        .find(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.redundant-tail-return-binding")
        })
        .expect("expected redundant tail return binding diagnostic");
    let hint = diagnostic
        .fix_hints
        .first()
        .expect("tail return binding has a fix");

    assert_eq!(
        hint.replacement.as_deref(),
        Some("[item for item in left if item in right]\n")
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
        .find(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.redundant-tail-return-binding")
        })
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
        .find(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.redundant-tail-return-binding")
        })
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
        .find(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.redundant-tail-return-binding")
        })
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
        !diagnostics.iter().any(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.unused-type")
        }),
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
        diagnostic.code.map(DiagnosticCode::name) == Some("lint.redundant-tail-return-binding")
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
            diagnostic.code.map(DiagnosticCode::name)
                == Some("lint.redundant-newline-triple-string")
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
            diagnostic.code.map(DiagnosticCode::name)
                == Some("lint.redundant-newline-triple-string")
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
fn linter_path_constructor_owns_display_roundtrips_without_utf8_proof() {
    let source = "\
proc parsed(root: Path, value: Str) -> Path {
  return Path(fp\"{root}/{value}\".display())
}

proc main(root: Path, value: Str) [error] {
  let direct = Path(fp\"{root}/{value}\".display())
  let nested = Path(fp\"{root}/{value}\".display())
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
        .filter(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.redundant-path-parse")
        })
        .collect();

    assert!(
        path_parse_diagnostics.is_empty(),
        "diagnostics: {diagnostics:?}"
    );
    let constructor_fixes = diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.path-constructor")
        })
        .flat_map(|diagnostic| &diagnostic.fix_hints)
        .map(|hint| hint.replacement.as_deref().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        constructor_fixes, ["fp\"{root}/{value}\""; 3],
        "diagnostics: {diagnostics:?}"
    );
}

#[test]
fn linter_autofixes_redundant_type_driven_roundtrips() {
    let source = "\
type Row = {name: Str}

proc main(root: Path, name: Str, row: Row, count: Int, ratio: Float) [error] {
  let parsed_literal = Path(\"tmp/out\")
  let parsed_fmt = Path(f\"{root}/{name}\")
  let constructed_fmt = Path(f\"{root}/{name}\")
  let same_path = fp\"{root}\"
  let same_name = f\"{name}\"
  let same_row = row.require(Row)?
  let raw: Any = {name}
  let checked_row = raw.require(Row)?
  let same_count = f\"{count}\".parse_int()?
  let same_ratio = f\"{ratio}\".parse_float()?
  print ${parsed_literal} ${parsed_fmt} ${constructed_fmt} ${same_path} ${same_name} \\
    ${same_row.name} ${checked_row.name} ${same_count} ${same_ratio}
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
            .filter(|diagnostic| diagnostic.code.map(DiagnosticCode::name) == Some(code))
            .filter(|diagnostic| !diagnostic.fix_hints.is_empty())
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
        ["p\"tmp/out\"", "fp\"{root}/{name}\"", "fp\"{root}/{name}\""]
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
  print f\"{manifest.display()}\"
  print f\"{name}\"
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
        .filter(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.redundant-command-fmt")
        })
        .map(|diagnostic| {
            diagnostic.fix_hints[0]
                .replacement
                .as_deref()
                .expect("command f-string diagnostic has replacement")
        })
        .collect();
    assert_eq!(replacements, ["$name"]);
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
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
            diagnostic.code.map(DiagnosticCode::name)
                == Some("lint.redundant-command-interpolation")
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
  let _ = normalized
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
        .filter(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.json-roundtrip")
        })
        .count();
    let stream_fixes: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.redundant-pipeline-stage")
        })
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
        .find(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.unsorted-imports")
        })
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
        .filter(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.unsorted-imports")
        })
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
        .find(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.unsorted-imports")
        })
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
        .filter(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.organize-top-level-consts")
        })
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
        .filter_map(|d| d.code.map(DiagnosticCode::name))
        .collect();
    assert!(
        codes.contains(&"lint.prefer-list-comp"),
        "expected lint.prefer-list-comp in {codes:?}"
    );
    let hint = diagnostics
        .iter()
        .find(|d| d.code.map(DiagnosticCode::name) == Some("lint.prefer-list-comp"))
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
        .find(|d| d.code.map(DiagnosticCode::name) == Some("lint.prefer-list-comp"))
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
            .any(|d| d.code.map(DiagnosticCode::name) == Some("lint.prefer-list-comp")),
        "branching accumulation should not get a list-comprehension fix: {diagnostics:?}"
    );
}

#[test]
fn linter_does_not_rewrite_unique_accumulation_loop() {
    let source = "\
let items: List[Int] = []
var unique: List[Int] = []
for item in items {
  if item not in unique {
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
            .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                == Some("lint.prefer-list-comp")),
        "unique accumulation depends on the accumulator: {diagnostics:?}"
    );
}

/// The comprehension offered for a loop over `items` whose body is `body`.
fn list_comprehension_for_loop_body(body: &str) -> Option<String> {
    let source = format!(
        "\
let items: List[Str] = []
let skipped = false
var names: List[Str] = []
for item in items {{
{body}}}
"
    );
    let parsed = parse_lint_source(&source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, &source, LintOptions::default());
    diagnostics
        .iter()
        .find(|d| d.code.map(DiagnosticCode::name) == Some("lint.prefer-list-comp"))
        .map(|d| d.fix_hints[0].replacement.clone().unwrap())
}

#[test]
fn linter_turns_leading_continue_guards_into_comprehension_filters() {
    // `continue unless c` keeps the items `if c` keeps; `continue when c`
    // keeps the others, spelled without new grouping.
    for (body, expected) in [
        (
            "  continue unless item != \"\"\n  names += [item.trim()]\n",
            "var names: List[Str] = [item.trim() for item in items if item != \"\"]\n",
        ),
        (
            "  continue when item == \"\"\n  names += [item.trim()]\n",
            "var names: List[Str] = [item.trim() for item in items if item != \"\"]\n",
        ),
        (
            "  continue when skipped\n  names = names.push(item)\n",
            "var names: List[Str] = [item for item in items if ! skipped]\n",
        ),
        (
            "  continue when ! skipped\n  names += [item]\n",
            "var names: List[Str] = [item for item in items if skipped]\n",
        ),
        // Guards filter in the order they ran, before a nested condition.
        (
            "  continue unless item != \"\"\n  continue when item == \"-\"\n  if ! skipped {\n    names += [item]\n  }\n",
            "var names: List[Str] = [\n  item\n  for item in items\n  if item != \"\"\n  if item != \"-\"\n  if ! skipped\n]\n",
        ),
    ] {
        assert_eq!(
            list_comprehension_for_loop_body(body).as_deref(),
            Some(expected),
            "{body}"
        );
    }
}

#[test]
fn linter_keeps_loops_whose_continue_guards_are_not_plain_filters() {
    for body in [
        // Negating a compound condition needs grouping the lint does not add.
        "  continue when item == \"\" or skipped\n  names += [item]\n",
        // The filter reads the list being built.
        "  continue unless names.len() < 3\n  names += [item]\n",
        // A guard after the accumulation skips nothing, and `break` ends the loop.
        "  names += [item]\n  continue unless item != \"\"\n",
        "  break unless item != \"\"\n  names += [item]\n",
        // Two elements at once are not one projection.
        "  continue unless item != \"\"\n  names += [item, item]\n",
    ] {
        assert_eq!(list_comprehension_for_loop_body(body), None, "{body}");
    }
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
        .find(|d| d.code.map(DiagnosticCode::name) == Some("lint.prefer-map-comp"))
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
        .find(|d| d.code.map(DiagnosticCode::name) == Some("lint.prefer-empty-map-literal"))
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
            .any(|d| d.code.map(DiagnosticCode::name) == Some("lint.prefer-stream-producer")),
        "definition alone should not warn: {diagnostics:?}"
    );

    let source = format!("{source_without_lazy_consumer}\nlet count = rows([\"a\"])? |> count()\n");
    let parsed = parse_lint_source(&source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let diagnostics = lint_and_assert_fmt_stable(&parsed.arena, &source, LintOptions::default());
    let stream_warnings = diagnostics
        .iter()
        .filter(|d| d.code.map(DiagnosticCode::name) == Some("lint.prefer-stream-producer"))
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
        .filter_map(|d| d.code.map(DiagnosticCode::name))
        .collect();
    assert!(
        codes.contains(&"lint.prefer-string-concat"),
        "expected lint.prefer-string-concat in {codes:?}"
    );
    let hint = diagnostics
        .iter()
        .find(|d| d.code.map(DiagnosticCode::name) == Some("lint.prefer-string-concat"))
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
        .filter_map(|d| d.code.map(DiagnosticCode::name))
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
        .filter(|diagnostic| diagnostic.code.map(DiagnosticCode::name) == Some("lint.dead-code"))
        .collect::<Vec<_>>();
    assert_eq!(dead_code.len(), 1, "diagnostics: {diagnostics:?}");
    assert_eq!(dead_code[0].message, "unreachable code");
}

#[test]
fn linter_drops_path_display_from_command_words() {
    let source = "\
proc main(foo: Path) {
  print $foo.display()
  print ${foo.display()}
  print foo.display()
  print ${foo.parent().display()}
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
    let mut fixes = diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.redundant-path-display")
        })
        .map(|diagnostic| {
            let [hint] = diagnostic.fix_hints.as_slice() else {
                panic!("one fix per display: {diagnostic:?}")
            };
            (hint.span.unwrap(), hint.replacement.clone().unwrap())
        })
        .collect::<Vec<_>>();
    assert_eq!(fixes.len(), 4, "{diagnostics:?}");
    fixes.sort_by_key(|(span, _)| std::cmp::Reverse(span.start()));
    let mut fixed = source.to_owned();
    for (span, replacement) in fixes {
        fixed.replace_range(span.range(), &replacement);
    }
    assert_eq!(
        fixed,
        "\
proc main(foo: Path) {
  print $foo
  print ${foo}
  print $foo
  print ${foo.parent()}
}
"
    );
}

#[test]
fn linter_command_path_display_fixes_pass_native_bytes() {
    let source = r#"let raw = Path.parse_bytes(b"raw\xff name")?
run printf "%s" "--target=${raw.display()}" ?
run printf "%s" ${raw.display()} ?
run printf "%s" (raw.display()) ?
run printf "%s" f"{raw.display()}" ?
run printf "%s" f"{raw}" ?
"#;
    let parsed = parse_lint_source(source);
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
    // Dropping `.display()` passes the native bytes the text would replace.
    let display_fixes = diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.redundant-path-display")
        })
        .flat_map(|diagnostic| &diagnostic.fix_hints)
        .map(|hint| hint.replacement.as_deref().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        display_fixes,
        ["raw", "raw", "$raw", "raw"],
        "{diagnostics:?}"
    );
    // An f-string word asks for text, so it is not unwrapped without proof
    // that the Path's bytes are UTF-8.
    assert!(
        diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                == Some("lint.redundant-command-fmt"))
            .all(|diagnostic| diagnostic.fix_hints.is_empty()),
        "{diagnostics:?}"
    );
}

#[test]
fn linter_path_constructor_turns_displayed_path_text_into_native_pieces() {
    let source = r#"let raw = Path.parse_bytes(b"raw\xff name")?
let displayed = Path(raw.display())
let formatted = Path(f"{raw}")
let compound = Path(f"{raw}/child")
print $displayed $formatted $compound
"#;
    let parsed = parse_lint_source(source);
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
    let fixes = diagnostics
        .iter()
        .filter(|diagnostic| {
            matches!(
                diagnostic.code.map(DiagnosticCode::name),
                Some("lint.redundant-path-parse" | "lint.path-constructor")
            )
        })
        .flat_map(|diagnostic| &diagnostic.fix_hints)
        .map(|hint| hint.replacement.as_deref().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        fixes,
        ["raw", "raw", "fp\"{raw}/child\""],
        "{diagnostics:?}"
    );
}

#[test]
fn linter_path_constructor_utf8_text_fix_rechecks_and_converges() {
    let source = "pure known(name: Str, count: Int) -> Path { Path(f\"{name}/{count}\") }\nlet literal = Path(p\"known\".display())\nprint known(\"name\", 2) $literal\n";
    let parsed = parse_lint_source(source);
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
    let mut fixes = diagnostics
        .iter()
        .filter(|diagnostic| {
            matches!(
                diagnostic.code.map(DiagnosticCode::name),
                Some("lint.path-constructor" | "lint.redundant-path-parse")
            )
        })
        .flat_map(|diagnostic| &diagnostic.fix_hints)
        .collect::<Vec<_>>();
    fixes.sort_by_key(|hint| std::cmp::Reverse(hint.span.unwrap().start()));
    fixes.dedup_by_key(|hint| hint.span.unwrap());
    assert_eq!(fixes.len(), 2, "{diagnostics:?}");
    let mut fixed = source.to_owned();
    for hint in fixes {
        fixed.replace_range(
            hint.span.unwrap().range(),
            hint.replacement.as_deref().unwrap(),
        );
    }
    assert!(fixed.contains("fp\"{name}/{count}\""));
    assert_parse_check_standalone("UTF-8 path construction", &fixed);
    let parsed = parse_lint_source(&fixed);
    let checked = Checker::check_arena(&parsed.arena, &fixed);
    assert!(
        Linter::lint(
            &parsed.arena,
            &fixed,
            LintOptions {
                expr_types: checked.expr_types,
                ..LintOptions::default()
            }
        )
        .diagnostics
        .iter()
        .filter(|diagnostic| matches!(
            diagnostic.code.map(DiagnosticCode::name),
            Some("lint.path-constructor" | "lint.redundant-path-parse")
        ))
        .all(|diagnostic| diagnostic.fix_hints.is_empty())
    );
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
        .find(|d| d.code.map(DiagnosticCode::name) == Some("lint.needless-annotation"))
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
        .filter(|d| d.code.map(DiagnosticCode::name) == Some("lint.needless-annotation"))
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
        .filter(|d| d.code.map(DiagnosticCode::name) == Some("lint.needless-annotation"))
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
        .find(|d| d.code.map(DiagnosticCode::name) == Some("lint.needless-annotation"))
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
        .filter(|d| d.code.map(DiagnosticCode::name) == Some("lint.needless-annotation"))
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
        .filter(|d| d.code.map(DiagnosticCode::name) == Some("lint.needless-annotation"))
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
        .filter(|d| d.code.map(DiagnosticCode::name) == Some("lint.needless-annotation"))
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
        .filter(|d| d.code.map(DiagnosticCode::name) == Some("lint.needless-annotation"))
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
        .find(|d| d.code.map(DiagnosticCode::name) == Some("lint.needless-annotation"))
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
        .filter(|d| d.code.map(DiagnosticCode::name) == Some("lint.needless-annotation"))
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
        .filter(|d| d.code.map(DiagnosticCode::name) == Some("lint.needless-annotation"))
        .collect();
    assert_eq!(
        needless.len(),
        4,
        "expected 4 needless annotation diagnostics"
    );
}

fn context_safety_lints(source: &str) -> Vec<Diagnostic> {
    let parsed = parse_lint_source(source);
    assert!(
        parsed.diagnostics.is_empty(),
        "{source}: {:?}",
        parsed.diagnostics
    );
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(
        checked.diagnostics.is_empty(),
        "{source}: {:?}",
        checked.diagnostics
    );
    Linter::lint(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            requirement_targets: checked.requirement_targets,
            requirement_expected_targets: checked.requirement_expected_targets,
            statement_expression_spans: checked.statement_expression_spans,
            assertion_effect_spans: checked.assertion_effect_spans,
            standard_call_spans: checked.standard_call_spans,
            ..LintOptions::default()
        },
    )
    .diagnostics
}

#[test]
fn needless_annotation_retains_contextual_collection_element_domains() {
    for source in [
        "let values: List[Int?] = [null, 3]\nlet _ = values\n",
        "let expected: List[Int?] = [0, 2]\npure compare(actual: List[Int?]) { actual == expected }\n",
        "let expected: List[UInt] = [0, 2]\npure compare(actual: List[UInt]) { actual == expected }\n",
        "let expected: List[Int?] = [item for item in [0, 2]]\npure compare(actual: List[Int?]) { actual == expected }\n",
        "type Row = {value: Str?}\npure take(rows: List[Row]) {}\nlet rows: List[Row] = [{value: null}]\ntake(rows)\n",
    ] {
        let diagnostics = context_safety_lints(source);
        assert!(
            !diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                    == Some("lint.needless-annotation")),
            "{source}: {diagnostics:?}"
        );
    }
}

#[test]
fn needless_annotation_retains_independent_inferred_require_boundary() {
    for source in [
        "let raw: Any = Ok(3)\nlet inner: Result[Int] = raw.require()?\nlet _ = inner\n",
        "let raw: Any = Ok(Ok(3))\nlet inner: Result[Result[Int]] = raw.require()?\nlet _ = inner\n",
        "let raw: Any = 3\nlet captured: Result[Int] = try { raw.require()? }\nlet _ = captured\n",
        "let raw: Any = 3\nlet value: Int = raw.require(Int)?\nlet _ = value\n",
    ] {
        let diagnostics = context_safety_lints(source);
        assert!(
            !diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                    == Some("lint.needless-annotation")),
            "{source}: {diagnostics:?}"
        );
        for diagnostic in diagnostics.iter().filter(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.inferred-require-target")
        }) {
            let hint = &diagnostic.fix_hints[0];
            let mut fixed = source.to_string();
            fixed.replace_range(
                hint.span.unwrap().range(),
                hint.replacement.as_deref().unwrap(),
            );
            assert_parse_check_standalone("independent require boundary", &fixed);
        }
    }
}

#[test]
fn needless_annotation_retains_empty_splice_element_anchor() {
    for source in [
        "let empty: List[Str] = [@[], @[]]\npure consume(values: List[Str]) {}\nconsume(empty)\n",
        "let empty: List[Str] = [@[@[]], @[]]\npure consume(values: List[Str]) {}\nconsume(empty)\n",
    ] {
        let diagnostics = context_safety_lints(source);
        assert!(
            !diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                    == Some("lint.needless-annotation")),
            "{source}: {diagnostics:?}"
        );
    }
}

#[test]
fn needless_annotation_retains_result_constructor_contract() {
    let temp = TempDir::new().unwrap();
    for source in [
        "error Failure = Missing(message: Str)\nlet failed: Result[Str, Failure] = Err(Failure.Missing(message: \"missing\"))\nlet recovered = failed ?? { |_error| \"fallback\" }\nlet _ = recovered\n",
        "error Failure = Missing(message: Str)\nlet failed: Result[Bool, Failure] = Err(Failure.Missing(message: \"missing\"))\nlet recovered = failed ?? { |_error| false }\nlet _ = recovered\n",
        "error Failure = Missing(message: Str)\nlet failed: Result[Unit, Failure] = Err(Failure.Missing(message: \"missing\"))\nlet _ = failed\n",
        "error Failure = Missing(message: Str)\nlet success: Result[Str, Failure] = Ok(\"present\")\nlet _ = success\n",
    ] {
        let (declaration, body) = source.split_once('\n').unwrap();
        for source in [
            source.to_owned(),
            format!("{declaration}\ntest result [error] {{ {body} }}\n"),
        ] {
            let diagnostics = context_safety_lints(&source);
            assert!(
                !diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                        == Some("lint.needless-annotation")),
                "{source}: {diagnostics:?}"
            );
            let path = temp.path().join("result-constructor.xsh");
            fs::write(&path, &source).unwrap();
            let result =
                xsht::lint_files(&[path.to_string_lossy().into_owned()], false, false, None);
            assert!(
                !String::from_utf8_lossy(&result.stderr).contains("lint.needless-annotation"),
                "{source}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
    }
}

#[test]
fn needless_annotation_retains_applied_nominal_element_anchor() {
    let source = "type Row[T] = {value: T}\nproc gather() -> List[Row[Int]] { var rows: List[Row[Int]] = []; rows = rows.push(Row(value: 1)); rows }\n";
    let diagnostics = context_safety_lints(source);
    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                == Some("lint.needless-annotation")),
        "{source}: {diagnostics:?}"
    );
}

#[test]
fn needless_annotation_retains_imported_nominal_element_anchor() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("rows.xsh"), "##! Row contracts.\n## A fixed integer row.\nexport type Row = {value: Int}\n## A row with a concrete value domain.\nexport type Box[T] = {value: T}\n").unwrap();
    for schema in ["rows.Row", "rows.Box[Int]"] {
        let constructor = if schema == "rows.Row" {
            "rows.Row"
        } else {
            "rows.Box"
        };
        let source = format!(
            "use rows\nproc gather() -> List[{schema}] {{ var values: List[{schema}] = []; values = values.push({constructor}(value: 1)); values }}\n"
        );
        let entry = temp.path().join("entry.xsh");
        fs::write(&entry, &source).unwrap();
        let loaded = parse_load_check_text(
            entry.to_str().unwrap(),
            source.clone(),
            Vec::new(),
            Default::default(),
        );
        assert!(
            loaded.parsed.diagnostics.is_empty(),
            "{:?}",
            loaded.parsed.diagnostics
        );
        let checked = loaded.checked.unwrap();
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let result = xsht::lint_files(&[entry.to_string_lossy().into_owned()], false, false, None);
        assert!(
            !String::from_utf8_lossy(&result.stderr).contains("lint.needless-annotation"),
            "{source}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(fs::read_to_string(entry).unwrap(), source);
    }
}

#[test]
fn redundant_require_retains_unsigned_validation() {
    let temp = TempDir::new().unwrap();
    for (index, source) in [
        "let value = (-1).require(UInt)?\nlet _ = value\n",
        "let value = [1, -1].require(List[UInt])?\nlet _ = value\n",
    ]
    .into_iter()
    .enumerate()
    {
        let diagnostics = context_safety_lints(source);
        assert!(
            !diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                    == Some("lint.redundant-require")),
            "{source}: {diagnostics:?}"
        );
        let path = temp.path().join(format!("unsigned-{index}.xsh"));
        fs::write(&path, source).unwrap();
        let output = std::process::Command::new(release_bin!("xsht"))
            .arg("trace")
            .arg(&path)
            .output()
            .unwrap();
        assert!(!output.status.success(), "{source}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("schema"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn assertion_helper_fix_retains_contextual_collection_domains() {
    for source in [
        "proc compare(actual: List[UInt]) { test.eq(actual, [3, 20])? }\n",
        "proc compare(actual: List[Int?]) { test.eq(actual, [3, 20])? }\n",
        "proc compare(actual: List[UInt]) { test.ne(right: [3, 20], left: actual)? }\n",
        "test unsigned [error] { let values: Map[UInt, Str] = {[3]: \"three\", [20]: \"twenty\"}; test.eq(values.keys(), [3, 20])? }\n",
    ] {
        let diagnostics = context_safety_lints(source);
        let diagnostics = diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.code.map(DiagnosticCode::name) == Some("lint.core-assert")
            })
            .collect::<Vec<_>>();
        assert_eq!(diagnostics.len(), 1, "{source}");
        for hint in &diagnostics[0].fix_hints {
            let mut fixed = source.to_string();
            fixed.replace_range(
                hint.span.unwrap().range(),
                hint.replacement.as_deref().unwrap(),
            );
            assert_parse_check_standalone("contextual assertion operand", &fixed);
        }
    }
}

fn membership_lints(source: &str) -> Vec<Diagnostic> {
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(
        checked
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                == Some("check.removed-membership")),
        "{:?}",
        checked.diagnostics
    );
    Linter::lint(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            callable_effects: checked.callable_effects,
            statement_expression_spans: checked.statement_expression_spans,
            assertion_effect_spans: checked.assertion_effect_spans,
            membership_migration_spans: checked.membership_migration_spans,
            standard_call_spans: checked.standard_call_spans,
            ..LintOptions::default()
        },
    )
    .diagnostics
    .into_iter()
    .filter(|diagnostic| {
        matches!(
            diagnostic.code.map(DiagnosticCode::name),
            Some("lint.prefer-in" | "lint.core-assert")
        )
    })
    .collect()
}

fn membership_fixed(source: &str) -> String {
    let parsed = parse_lint_source(source);
    let mut edits: Vec<_> = membership_lints(source)
        .into_iter()
        .flat_map(|diagnostic| diagnostic.fix_hints)
        .filter_map(|hint| Some((hint.span?, hint.replacement?)))
        .filter(|(span, _)| !parsed.cst.get().contains_comment(*span))
        .collect();
    edits.sort_by_key(|(span, _)| (span.start(), std::cmp::Reverse(span.end())));
    let mut end = 0;
    let edits: Vec<_> = edits
        .into_iter()
        .filter(|(span, _)| {
            if span.start() < end {
                false
            } else {
                end = span.end();
                true
            }
        })
        .collect();
    let mut fixed = source.to_string();
    for (span, replacement) in edits.into_iter().rev() {
        fixed.replace_range(span.range(), &replacement);
    }
    let formatted = Formatter::new().format_source(SourceId::new(0), &fixed);
    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_parse_check_standalone("membership migration", &formatted.formatted);
    assert!(
        membership_lints(&formatted.formatted)
            .iter()
            .all(|diagnostic| diagnostic.fix_hints.is_empty()),
        "migration should be idempotent: {}",
        formatted.formatted
    );
    formatted.formatted
}

#[test]
fn linter_autofixes_removed_membership_using_checked_identity() {
    let fixed = membership_fixed(
        r#"type Row = {present: Int}
proc main(names: List[Str], name: Str, text: Str, source_path: Path, mapping: Map[Int], fields: Row) {
  if names.contains(name) {}
  if ! names.contains(name) {}
  if text.contains("needle") {}
  if source_path.display().contains("/") {}
  if mapping.has("key") {}
  if fields.has("present") {}
}
"#,
    );
    for expression in [
        "name in names",
        "name not in names",
        "\"needle\" in text",
        "\"/\" in source_path.display()",
        "\"key\" in mapping",
        "\"present\" in fields",
    ] {
        assert!(fixed.contains(expression), "missing {expression}: {fixed}");
    }
}

#[test]
fn linter_preserves_membership_operand_order_with_statement_bindings() {
    let source = r#"proc container() -> Result[Str] { "abc" }
proc item() -> Result[Str] { "b" }
proc main() {
  test.contains(container()?, item()?)?
}
"#;
    let fixed = membership_fixed(source);
    let container = fixed.find("= container()?").expect("receiver binding");
    let item = fixed.find("= item()?").expect("needle binding");
    assert!(container < item, "{fixed}");
    assert!(fixed.contains(" in membership_argument_0_"), "{fixed}");
}

#[test]
fn linter_diagnoses_unsafe_membership_inside_conditions_without_hoisting() {
    let diagnostics = membership_lints(
        r#"proc main(source_path: Path, names: List[Str]) [fs, error] {
  if source_path.read_text()?.contains("needle") {}
  if names.contains(source_path.read_text()?) {}
}
"#,
    );
    assert_eq!(diagnostics.len(), 2);
    assert!(
        diagnostics[0].fix_hints.is_empty(),
        "null-safe consumption requires an explicit manual decision"
    );
    assert!(
        diagnostics[1].fix_hints.is_empty(),
        "mutable receiver read must retain its order"
    );
}

#[test]
fn linter_migrates_assertion_helpers_only_in_statement_use() {
    let fixed = membership_fixed(
        r#"proc main(text: Str) {
  test.ok(true)?
  test.eq(1, 2)?
  test.ne(1, 2)
  let consumed = test.contains(text, "é")
  test.contains(text, "é", message: "custom")?
  let retained = test.eq(1, 2)
  let _ = consumed
  let _ = retained
}
"#,
    );
    assert!(fixed.contains("1 == 2"), "{fixed}");
    assert!(fixed.contains("1 != 2"), "{fixed}");
    assert!(fixed.contains(r#"test.ok("é" in text)"#), "{fixed}");
    assert!(fixed.contains("message: \"custom\""), "{fixed}");
    assert!(fixed.contains("let retained = test.eq(1, 2)"), "{fixed}");
}

#[test]
fn linter_composes_nested_unicode_membership_and_grouped_receivers() {
    let fixed = membership_fixed(
        r#"proc main(text: Str) {
  test.ok(! text.contains("é"))?
  test.eq((-3.5).abs(), 3.5)?
}
"#,
    );
    assert!(fixed.contains(r#""é" not in text"#), "{fixed}");
    assert!(fixed.contains("(-3.5).abs() == 3.5"), "{fixed}");
}

#[test]
fn linter_migrates_package_nested_assertions_and_multiline_match_membership() {
    let fixed = membership_fixed(
        r#"proc main(configured: Str, result: Result[Str], expected: Str) {
  test.eq(configured.contains("menucmd"), false)?
  test.ok(configured.contains("termcmd"))?
  match result {
    Ok(_) => test.fail("unexpected success")?
    Err(problem) => test.contains(
      problem.message,
      f"repeats {expected}",
    )?
  }
  match result {
    Ok(_) => assert true
    Err(problem) => assert problem.message.contains("repeats")
  }
}
"#,
    );
    assert!(!fixed.contains(".contains("), "{fixed}");
    assert!(fixed.contains("\"menucmd\" in configured"), "{fixed}");
    assert!(fixed.contains("\"termcmd\" in configured"), "{fixed}");
    assert!(fixed.contains("problem.message"), "{fixed}");
    assert!(fixed.contains("repeats {expected}"), "{fixed}");
}

#[test]
fn linter_migrates_explicitly_propagated_read_membership_without_null_safe_guessing() {
    let fixed = membership_fixed(
        r#"proc main(source_path: Path) [fs, error] {
  test.contains(source_path.read_text()?, "needle")?
  if (source_path.read_text()?).contains("needle") {}
}
"#,
    );
    assert!(!fixed.contains(".contains("), "{fixed}");
    assert_eq!(
        fixed.matches("source_path.read_text()?").count(),
        2,
        "{fixed}"
    );
    let diagnostics = membership_lints(
        r#"proc main(source_path: Path) [fs, error] {
  if source_path.read_text()?.contains("needle") {}
}
"#,
    );
    assert_eq!(diagnostics.len(), 1);
    assert!(
        diagnostics[0].fix_hints.is_empty(),
        "null-safe consumption must remain explicit"
    );
}

#[test]
fn linter_match_membership_snapshots_preserve_effects_and_failure() {
    let temp = TempDir::new().unwrap();
    for (needle, succeeds) in [("b", true), ("missing", false)] {
        let source = format!(
            r#"proc haystack() [io] -> Str {{ print 1; "abc" }}
proc needle() [io] -> Str {{ print 2; "{needle}" }}
proc message() [io] -> Str {{ print 3; "custom membership failure" }}
proc skipped() [io] -> Str {{ print 9; "abc" }}
proc main() {{
  match true {{
    true => test.contains(haystack(), needle(), message: message())?
    false => test.contains(skipped(), needle())?
  }}
  print 4
}}
"#
        );
        let fixed = membership_fixed(&source);
        let path = temp.path().join(format!("membership-{needle}.xsh"));
        fs::write(&path, &fixed).unwrap();
        let output = std::process::Command::new(release_bin!("xsht"))
            .arg("trace")
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            succeeds,
            "{fixed}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            output.stdout,
            if succeeds {
                b"1\n2\n3\n4\n".as_slice()
            } else {
                b"1\n2\n3\n".as_slice()
            },
            "{fixed}"
        );
        if !succeeds {
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("custom membership failure"),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

#[test]
fn linter_preserves_named_argument_order_and_hygiene() {
    let fixed = membership_fixed(
        r#"proc left() -> Result[Int] { 1 }
proc right() -> Result[Int] { 2 }
proc main() {
  let membership_argument_0_130 = 1
  test.eq(right: right()?, left: left()?)?
  let _ = membership_argument_0_130
}
"#,
    );
    assert!(
        fixed.find("= right()?").unwrap() < fixed.find("= left()?").unwrap(),
        "{fixed}"
    );
}

#[test]
fn linter_named_assertion_snapshots_preserve_runtime_source_order() {
    let temp = TempDir::new().unwrap();
    for (operation, right_value) in [("eq", 7), ("ne", 8)] {
        let source = format!(
            "proc left() [io] -> Int {{ print 1; 7 }}\nproc right() [io] -> Int {{ print 2; {right_value} }}\nproc main() {{ test.{operation}(right: right(), left: left())? }}\n"
        );
        let original = temp.path().join(format!("{operation}-original.xsh"));
        fs::write(&original, &source).unwrap();
        let before = std::process::Command::new(release_bin!("xsht"))
            .arg("trace")
            .arg(&original)
            .output()
            .unwrap();
        assert!(
            before.status.success(),
            "{}",
            String::from_utf8_lossy(&before.stderr)
        );
        assert_eq!(before.stdout, b"2\n1\n", "{source}");
        let fixed = membership_fixed(&source);
        assert!(fixed.contains("membership_argument_0_"), "{fixed}");
        let rewritten = temp.path().join(format!("{operation}-fixed.xsh"));
        fs::write(&rewritten, &fixed).unwrap();
        let after = std::process::Command::new(release_bin!("xsht"))
            .arg("trace")
            .arg(&rewritten)
            .output()
            .unwrap();
        assert!(
            after.status.success(),
            "{}",
            String::from_utf8_lossy(&after.stderr)
        );
        assert_eq!(after.stdout, before.stdout, "{fixed}");
    }
}

#[test]
fn linter_leaves_dynamic_and_comment_bearing_migrations_actionable() {
    let dynamic = membership_lints("proc main(value: Any) { test.contains(value, 1)? }\n");
    assert_eq!(dynamic.len(), 1);
    assert!(dynamic[0].fix_hints.is_empty());
    let source = "proc main() { test.ok(retry [] { # keep this reason\n true }?)? }\n";
    let parsed = parse_lint_source(source);
    let diagnostics = membership_lints(source);
    assert!(
        diagnostics
            .iter()
            .flat_map(|diagnostic| &diagnostic.fix_hints)
            .any(|hint| parsed.cst.get().contains_comment(hint.span.unwrap()))
    );
}

#[test]
fn linter_migrates_set_negation_and_stream_item_membership() {
    let fixed = membership_fixed(
        r#"proc main(mapping: Map[Int], keys: List[Str]) {
  assert ! set.has(set.empty(), "missing")
  let present = keys |> where mapping.has(.)
  let absent = keys |> where ! mapping.has(.) and ! mapping.has("other")
  let _ = present
  let _ = absent
}
"#,
    );
    assert!(fixed.contains("\"missing\" not in set.empty()"), "{fixed}");
    assert!(fixed.contains("(.) in mapping"), "{fixed}");
    assert!(fixed.contains("(.) not in mapping"), "{fixed}");
}

#[test]
fn linter_does_not_rewrite_user_fields_named_contains_or_has() {
    let source = r#"pure contains(value: Str) -> Bool { true }
proc main() {
  let custom = {contains: contains, has: contains}
  let _ = custom.contains
  let _ = custom.has
  assert contains("value")
}
"#;
    let diagnostics = membership_lints(source);
    assert!(diagnostics.is_empty());
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
        .filter(|d| d.code.map(DiagnosticCode::name) == Some("lint.dollar-in-expression-string"))
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
let fmt = f\"tags: {body}\"
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
            .any(|d| d.code.map(DiagnosticCode::name) == Some("lint.dollar-in-expression-string")),
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
            .any(|d| d.code.map(DiagnosticCode::name) == Some("lint.dollar-in-expression-string")),
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
        .filter(|d| d.code.map(DiagnosticCode::name) == Some("lint.dollar-in-expression-string"))
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
        .filter(|diagnostic| diagnostic.code.map(DiagnosticCode::name) == Some("lint.dead-code"))
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
            .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name) == Some("lint.dead-code")),
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
        .filter(|diagnostic| diagnostic.code.map(DiagnosticCode::name) == Some("lint.dead-code"))
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
            .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name) == Some("lint.dead-code")),
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
            .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name) == Some("lint.dead-code")),
        "diagnostics: {diagnostics:?}"
    );
}

#[test]
fn linter_reports_dead_code_after_exit() {
    let source = "\
proc main() {
  exit 0
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
            ..LintOptions::default()
        },
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name) == Some("lint.dead-code")),
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
        .filter(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.unused-callable")
        })
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
                diagnostic.code.map(DiagnosticCode::name),
                Some("lint.dead-code") | Some("lint.unused-callable")
            )
        }),
        "diagnostics: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(
            |diagnostic| diagnostic.code.map(DiagnosticCode::name) == Some("lint.unused-local")
        ),
        "non-dead-code lint should remain active: {diagnostics:?}"
    );
}

#[test]
fn linter_follows_declared_callable_resolution_past_local_bindings() {
    // A call inside the local's scope is `check.call-target` (the runtime
    // would call the local); after the scope ends the call is the function's.
    let source = "\
pure helper() -> Str {
  return \"callable\"
}

proc main() {
  if true {
    let helper = 1
    print $helper
  }
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
            .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                == Some("lint.unused-callable")),
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
            .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                == Some("lint.unused-callable")),
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
        .filter(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.unused-callable")
        })
        .collect();
    assert_eq!(unused.len(), 1, "diagnostics: {diagnostics:?}");
    assert!(unused[0].message.contains("`unused_helper`"));
    assert_eq!(unused[0].labels[0].span.source_id, SourceId::new(1));
}

#[test]
fn linter_removes_checked_tail_returns_in_value_branches() {
    let source = "pure label(code: Int) -> Str {\n  match code {\n    0 => return \"ok\"\n    _ => {\n      let detail: Str = f\"exit {code}\"\n      return detail # retain this comment\n    }\n  }\n}\n";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let diagnostics = Linter::lint(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            statement_positions: checked.statement_positions,
            ..LintOptions::default()
        },
    )
    .diagnostics;
    let fixes = diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.redundant-tail-return")
        })
        .flat_map(|diagnostic| diagnostic.fix_hints.iter())
        .collect::<Vec<_>>();
    assert_eq!(fixes.len(), 2);
    let mut candidate = source.to_owned();
    for fix in fixes.iter().rev() {
        candidate.replace_range(
            fix.span.unwrap().range(),
            fix.replacement.as_deref().unwrap(),
        );
    }
    assert!(candidate.contains("let detail: Str"));
    assert!(candidate.contains("detail # retain this comment"));
    let parsed = parse_lint_source(&candidate);
    let checked = Checker::check_arena(&parsed.arena, &candidate);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let second = Linter::lint(
        &parsed.arena,
        &candidate,
        LintOptions {
            expr_types: checked.expr_types,
            statement_positions: checked.statement_positions,
            ..LintOptions::default()
        },
    )
    .diagnostics;
    assert!(
        !second
            .iter()
            .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                == Some("lint.redundant-tail-return"))
    );
}

#[test]
fn linter_tail_return_keeps_match_arm_record_an_expression() {
    let source = "type Row = {value: Int}\npure row(code: Int) -> Row {\n  match code {\n    0 => return {value: 1}\n    _ => return {value: 2}\n  }\n}\n";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let diagnostics = Linter::lint(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            statement_positions: checked.statement_positions,
            ..LintOptions::default()
        },
    )
    .diagnostics;
    let mut fixes = diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.redundant-tail-return")
        })
        .flat_map(|diagnostic| &diagnostic.fix_hints)
        .collect::<Vec<_>>();
    assert_eq!(fixes.len(), 2);
    fixes.sort_by_key(|fix| std::cmp::Reverse(fix.span.unwrap().start()));
    let mut fixed = source.to_owned();
    for fix in fixes {
        fixed.replace_range(
            fix.span.unwrap().range(),
            fix.replacement.as_deref().unwrap(),
        );
    }
    assert_parse_check_standalone("record match arm tail", &fixed);
}

#[test]
fn linter_tail_return_preserves_grouping_and_unicode_comments() {
    let source = "pure sum() -> Int {\n  return (1 + 2) * 3 # café\n}\n";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let diagnostics = Linter::lint(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            statement_positions: checked.statement_positions,
            ..LintOptions::default()
        },
    )
    .diagnostics;
    let fix = diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.redundant-tail-return")
        })
        .unwrap()
        .fix_hints
        .first()
        .unwrap();
    let mut candidate = source.to_owned();
    candidate.replace_range(
        fix.span.unwrap().range(),
        fix.replacement.as_deref().unwrap(),
    );
    assert!(candidate.contains("(1 + 2) * 3 # café"));
    assert_parse_check_standalone("grouped tail", &candidate);
}

#[test]
fn linter_keeps_conditional_and_callback_lexical_returns() {
    let source = "pure conditional(flag: Bool) -> Int {\n  if flag { return 1 }\n  2\n}\npure callback() -> Int {\n  let rows = [1] |> map { |number| return 4 }\n  9\n}\n";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let diagnostics = Linter::lint(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            statement_positions: checked.statement_positions,
            ..LintOptions::default()
        },
    )
    .diagnostics;
    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                == Some("lint.redundant-tail-return"))
    );
}

#[test]
fn linter_named_argument_pun_fix_preserves_resolution_comments_and_converges() {
    let source =
        include_str!("../../../tests/fixtures/syntax/valid/named-argument-pun-explicit.xsh");
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
        .filter(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.prefer-named-argument-pun")
        })
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
    let mut fixed = source.to_string();
    for diagnostic in puns.iter().rev() {
        for fix in &diagnostic.fix_hints {
            fixed.replace_range(fix.span.unwrap().range(), fix.replacement.as_ref().unwrap());
        }
    }
    assert_parse_check_standalone("named argument pun fix", &fixed);
    let formatted = Formatter::new().format_source(SourceId::new(0), &fixed);
    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
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
        second
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                == Some("lint.prefer-named-argument-pun"))
            .all(|diagnostic| diagnostic.fix_hints.is_empty())
    );
}

#[test]
fn linter_named_argument_pun_requires_checked_identifier_resolution() {
    let source =
        include_str!("../../../tests/fixtures/syntax/valid/named-argument-pun-explicit.xsh");
    let parsed = parse_lint_source(source);
    let diagnostics = Linter::lint(&parsed.arena, source, LintOptions::default()).diagnostics;
    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                == Some("lint.prefer-named-argument-pun"))
    );
}

#[test]
fn linter_list_compound_assignment_is_checked_and_converges() {
    let source = "# café\nvar names: List[Str] = []\nlet item = \"value\"\nnames = names.push(item) # Keep this reason.\nlet more = [\"second\"]\nnames = names.extend(more)\nprint names.len()\n";
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
    let updates: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name)
                == Some("lint.prefer-list-compound-assignment")
        })
        .collect();
    assert_eq!(updates.len(), 2);
    let mut fixed = source.to_string();
    for diagnostic in updates.iter().rev() {
        let hint = &diagnostic.fix_hints[0];
        fixed.replace_range(
            hint.span.unwrap().range(),
            hint.replacement.as_deref().unwrap(),
        );
    }
    assert!(fixed.contains("names += [item] # Keep this reason."));
    assert!(fixed.contains("names += more"));
    let parsed = parse_lint_source(&fixed);
    let checked = Checker::check_arena(&parsed.arena, &fixed);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let second = Linter::lint(
        &parsed.arena,
        &fixed,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    );
    assert!(
        !second
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                == Some("lint.prefer-list-compound-assignment"))
    );
}

#[test]
fn linter_list_compound_assignment_refuses_unchecked_effectful_and_nested_updates() {
    let source = "pure item() -> Int {\n  return 2\n}\nvar values = [1]\nvalues = values.push(item())\nvar container = {values: [1]}\ncontainer.values = container.values.push(2)\nlet pushed = values.push(3)\nprint ${pushed.len()} ${container.values.len()}\n";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    for options in [
        LintOptions::default(),
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    ] {
        let output = Linter::lint(&parsed.arena, source, options);
        assert!(
            !output
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                    == Some("lint.prefer-list-compound-assignment"))
        );
    }
}

#[test]
fn linter_list_compound_assignment_retains_multiline_comments() {
    let source = "var values = [1]\nvalues = values.push(\n  2, # keep\n)\nprint ${values.len()}\n";
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let output = Linter::lint(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    );
    let diagnostic = output
        .diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name)
                == Some("lint.prefer-list-compound-assignment")
        })
        .expect("checked list update warning");
    assert!(diagnostic.fix_hints.is_empty());
}

/// The `+=` updates the compound-assignment lint reports for a checked
/// source, each with its replacement (`None` when only a manual rewrite
/// exists) and its notes.
fn list_compound_updates(source: &str) -> Vec<(Option<String>, Vec<String>)> {
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    Linter::lint(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    )
    .diagnostics
    .into_iter()
    .filter(|diagnostic| {
        diagnostic.code.map(DiagnosticCode::name) == Some("lint.prefer-list-compound-assignment")
    })
    .map(|diagnostic| {
        (
            diagnostic
                .fix_hints
                .first()
                .map(|hint| hint.replacement.clone().expect("replacement")),
            diagnostic.notes.clone(),
        )
    })
    .collect()
}

#[test]
fn linter_list_compound_assignment_reaches_every_argument_that_leaves_the_local_alone() {
    // The argument is evaluated after the receiver read in `push` and before
    // the read of the current value in `+=`; none of these can assign `names`.
    let source = r##"type Row = {name: Str, size: UInt}

pure label(item: Str) -> Str {
  return item.upper()
}

pure shout(items: List[Str]) -> List[Str] {
  return items |> map { |item| item.upper() } |> collect()
}

proc collect_names(items: List[Str], row: Row) [] -> List[Str] {
  var names: List[Str] = []
  var rows: List[Row] = []
  for item in items {
    names = names.push(label(item))
    names = names.push(f"{item}!")
    names = names.push(row.name)
    names = names.push(items[0])
    names = names.push((items |> map { |entry| entry.upper() }).join(","))
    names = names.push("#define X")
    names = names.extend(shout(items))
    rows = rows.push({name: item, size: 3})
  }

  print ${rows.len()}
  names
}
"##;
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
        .filter(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name)
                == Some("lint.prefer-list-compound-assignment")
        })
        .map(|diagnostic| diagnostic.fix_hints[0].replacement.as_deref().unwrap())
        .collect();
    assert_eq!(
        replacements,
        [
            "names += [label(item)]",
            "names += [f\"{item}!\"]",
            "names += [row.name]",
            "names += [items[0]]",
            "names += [(items |> map { |entry| entry.upper() }).join(\",\")]",
            "names += [\"#define X\"]",
            "names += shout(items)",
            "rows += [{name: item, size: 3}]",
        ]
    );
}

#[test]
fn linter_list_compound_assignment_keeps_an_argument_that_assigns_the_target() {
    // Evaluating the callback overwrites `names` after `push` has already read
    // it, so `names += [...]` would keep the overwrite and `push` would not.
    let source = r##"proc collect_names(items: List[Str]) [] -> List[Str] {
  var names: List[Str] = []
  names = names.push(items |> map { |entry| names = [entry]; entry } |> join(","))
  names = names.push((items |> map { |entry| names = [entry]; entry }).join(","))
  names = names.push(items |> map { |entry| entry.upper() } |> join(","))
  names = names.push((items |> map { |entry| entry.upper() }).join(","))
  names
}
"##;
    // The unformatted pipeline and its formatted method-call spelling parse
    // differently and must be judged alike.
    let updates = list_compound_updates(source);
    assert_eq!(
        updates
            .iter()
            .map(|(replacement, _)| replacement.as_deref())
            .collect::<Vec<_>>(),
        [
            Some("names += [items |> map { |entry| entry.upper() } |> join(\",\")]"),
            Some("names += [(items |> map { |entry| entry.upper() }).join(\",\")]"),
        ]
    );
}

#[test]
fn linter_list_compound_assignment_keeps_module_variables_with_calling_arguments() {
    // Any proc may assign a module-level variable, so a call between the two
    // reads could change which one the update sees.
    let source = r##"var seen: List[Str] = []

proc remember(item: Str) [] -> Str {
  seen = []
  return item
}

proc collect_names(item: Str) [] -> List[Str] {
  seen = seen.push(remember(item))
  seen = seen.push(item)
  seen
}
"##;
    let updates = list_compound_updates(source);
    assert_eq!(updates.len(), 1, "{updates:?}");
    assert_eq!(updates[0].0.as_deref(), Some("seen += [item]"));
}

#[test]
fn linter_list_compound_assignment_offers_a_fix_around_hash_strings_and_call_layout() {
    // A `#` inside a string is not a comment, and a call laid out over several
    // lines around a one-line argument still becomes one `+=` statement.
    let source = r##"proc build() [] -> List[Str] {
  var lines: List[Str] = []
  lines = lines.push("#endif /* GUARD_H */")
  lines = lines.push(
    "tail",
  )
  lines
}
"##;
    assert_eq!(
        list_compound_updates(source)
            .into_iter()
            .map(|(replacement, notes)| (replacement.unwrap(), notes))
            .collect::<Vec<_>>(),
        [
            ("lines += [\"#endif /* GUARD_H */\"]".to_string(), vec![]),
            ("lines += [\"tail\"]".to_string(), vec![]),
        ]
    );
}

#[test]
fn linter_list_compound_assignment_copies_multiline_arguments_but_not_comments() {
    let source = r##"type Row = {name: Str, size: Int}

proc build() [] -> List[Row] {
  var rows: List[Row] = []
  rows = rows.push(
    {name: "a", size: 1}, # first
  )
  rows = rows.push({
    name: "b",
    size: 2,
  })
  rows
}
"##;
    let updates = list_compound_updates(source);
    assert_eq!(
        updates,
        [
            (
                None,
                vec!["comments inside the update require a manual rewrite".to_string()]
            ),
            (
                Some("rows += [{\n    name: \"b\",\n    size: 2,\n  }]".to_string()),
                vec![]
            ),
        ]
    );
    // Copying the argument verbatim keeps the continuation lines' own
    // indentation, so the result is valid source that `fmt` then re-lays out.
    let fixed = source.replace(
        "rows = rows.push({\n    name: \"b\",\n    size: 2,\n  })",
        "rows += [{\n    name: \"b\",\n    size: 2,\n  }]",
    );
    assert_parse_check_standalone("multiline compound update", &fixed);
}

#[test]
fn linter_list_compound_assignment_wraps_an_update_that_overflows_the_line() {
    // The formatter lays an overflowing list out one element per line, so the
    // fix does too and leaves already formatted source formatted.
    let source = r##"proc build(items: List[Str]) [] -> List[Str] {
  var lines: List[Str] = []
  for item in items {
    lines = lines.push(
      f"dependency\t{item.upper()}\t{item.lower()}\t{item.trim()}\t{item.upper()}\t{item.lower()}\t{item.trim()}\t{item}",
    )
    lines = lines.push("short")
  }

  lines
}
"##;
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
        .filter(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name)
                == Some("lint.prefer-list-compound-assignment")
        })
        .map(|diagnostic| diagnostic.fix_hints[0].replacement.as_deref().unwrap())
        .collect();
    assert_eq!(
        replacements,
        [
            "lines += [\n      f\"dependency\\t{item.upper()}\\t{item.lower()}\\t{item.trim()}\\t{item.upper()}\\t{item.lower()}\\t{item.trim()}\\t{item}\",\n    ]",
            "lines += [\"short\"]",
        ]
    );
}

#[test]
fn linter_list_compound_assignment_rewrites_push_chains_only() {
    let source = r##"proc build(more: List[Str]) [] -> List[Str] {
  var lines: List[Str] = []
  lines = lines.push("a").push("b")
  lines = lines.push("c").extend(more)
  lines = lines.extend(more).extend(more)
  var index: Map[List[Str]] = {}
  index = index.push("key", "value")
  print ${index.len()}
  lines
}
"##;
    let updates = list_compound_updates(source);
    assert_eq!(
        updates,
        [(Some("lines += [\"a\", \"b\"]".to_string()), vec![])]
    );
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
    let diagnostics = lint_and_assert_fmt_stable(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    );
    let mut fixes = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.map(DiagnosticCode::name) == Some("lint.prefer-slice"))
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
    let second = Linter::lint(
        &parsed.arena,
        &fixed,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    );
    assert!(
        !second.diagnostics.iter().any(
            |diagnostic| diagnostic.code.map(DiagnosticCode::name) == Some("lint.prefer-slice")
        )
    );
}

#[test]
fn linter_prefer_slice_retains_uncertain_offsets_counts_and_overflow() {
    let source = "\
pure count() -> Int {
  return 3
}
pure selected(data: Bytes) -> List[Bytes] {
  let negative = data.slice(-1)
  let uncertain = data.slice(2)
  let arithmetic = data.slice(1, data.len() - 1)
  let effect_count = data.slice(0, count())
  let overflow = data.slice(1, 9223372036854775807)
  [negative, uncertain, arithmetic, effect_count, overflow]
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
    let slices = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.map(DiagnosticCode::name) == Some("lint.prefer-slice"))
        .collect::<Vec<_>>();
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
    assert!(
        !diagnostics.iter().any(
            |diagnostic| diagnostic.code.map(DiagnosticCode::name) == Some("lint.prefer-slice")
        )
    );
}

#[test]
fn linter_record_destructuring_fix_roundtrips_and_converges() {
    let source = "# 源\nlet config = {root: \"src\", build: {jobs: 3, target: \"native\"}}\nlet root = config.root\nlet jobs = config.build.jobs\nlet target_name = config.build.target\nprint $root $jobs $target_name\n";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let output = Linter::lint(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    );
    let diagnostic = output
        .diagnostics
        .iter()
        .find(|d| d.code.map(DiagnosticCode::name) == Some("lint.prefer-record-destructuring"))
        .unwrap();
    let hint = &diagnostic.fix_hints[0];
    let mut fixed = source.to_string();
    fixed.replace_range(
        hint.span.unwrap().range(),
        hint.replacement.as_ref().unwrap(),
    );
    assert!(fixed.contains("let {root, build: {jobs, target: target_name, ..}, ..} = config"));
    assert_parse_check_standalone("record destructuring", &fixed);
    let formatted = Formatter::new().format_source(SourceId::new(0), &fixed);
    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_eq!(formatted.formatted, fixed);
    let parsed = parse_lint_source(&fixed);
    let checked = Checker::check_arena(&parsed.arena, &fixed);
    let output = Linter::lint(
        &parsed.arena,
        &fixed,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    );
    assert!(
        !output
            .diagnostics
            .iter()
            .any(|d| d.code.map(DiagnosticCode::name) == Some("lint.prefer-record-destructuring"))
    );
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
        let output = Linter::lint(
            &parsed.arena,
            source,
            LintOptions {
                expr_types: checked.expr_types,
                ..LintOptions::default()
            },
        );
        assert!(
            !output
                .diagnostics
                .iter()
                .any(|d| d.code.map(DiagnosticCode::name)
                    == Some("lint.prefer-record-destructuring")),
            "{source}"
        );
    }
}

#[test]
fn formatter_preserves_comments_inside_nested_record_binding_targets() {
    let source = "let config = {root: \"src\", build: {jobs: 3, target: \"native\"}}\nlet {root, build: {\n  jobs, # worker count\n  target: target_name, ..\n}, ..} = config\nprint $root $jobs $target_name\n";
    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_eq!(formatted.formatted, source);
    let again = Formatter::new().format_source(SourceId::new(0), &formatted.formatted);
    assert_eq!(again.formatted, source);
}

#[test]
fn formatter_comparison_chain_preserves_grouping_and_precedence() {
    let source = include_str!("../../../tests/fixtures/syntax/comparison-chain.xsh");
    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert!(formatted.formatted.contains("let adjacent = 1 < 2 <= 3"));
    assert!(
        formatted
            .formatted
            .contains("let explicit = (1 < 2 <= 3) == true")
    );
    assert!(formatted.formatted.contains("let grouped = (1 < 2) < 3"));
    assert!(
        formatted
            .formatted
            .contains("let arithmetic = (1 + 2) * 3 < 10 <= 12")
    );
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
    let mut edits = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.map(DiagnosticCode::name) == Some("lint.prefer-guard"))
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
    assert!(fixed.contains("yield 1 unless false or false"), "{fixed}");
    assert!(fixed.contains("break 2 when true"));
    assert_parse_check_standalone("guarded value fixes", &fixed);
    let formatted = Formatter::new().format_source(SourceId::new(0), &fixed);
    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_parse_check_standalone("formatted guards", &formatted.formatted);
    let reparsed = parse_lint_source(&formatted.formatted);
    let second = Linter::lint(
        &reparsed.arena,
        &formatted.formatted,
        LintOptions::default(),
    )
    .diagnostics;
    assert!(
        !second.iter().any(
            |diagnostic| diagnostic.code.map(DiagnosticCode::name) == Some("lint.prefer-guard")
        )
    );
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
        assert!(
            !output
                .diagnostics
                .iter()
                .any(|d| d.code.map(DiagnosticCode::name) == Some("lint.prefer-list-comp")),
            "unsafe fix for {source}: {:?}",
            output.diagnostics
        );
    }
}

#[test]
fn linter_map_entry_iteration_keeps_mutation_annotations_comments_and_unknown_methods() {
    for body in [
        "let count = counts.get(key)?\n    counts[\"other\"] = count",
        "let count: Int = counts.get(key)?\n    print $count",
        "let count = counts.get(key)? # explains lookup\n    print $count",
        "let count = counts.get(key, 0)\n    print $count",
    ] {
        let source = format!(
            "var counts = map.empty().set(\"one\", 1)\nfor key in counts.keys() {{\n    {body}\n}}\n"
        );
        let parsed = parse_lint_source(&source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        let output = Linter::lint(
            &parsed.arena,
            &source,
            LintOptions {
                expr_types: checked.expr_types,
                ..LintOptions::default()
            },
        );
        assert!(
            !output.diagnostics.iter().any(
                |d| d.code.map(DiagnosticCode::name) == Some("lint.prefer-map-entry-iteration")
            ),
            "{source}"
        );
    }
}

#[test]
fn regex_literal_formatting_retains_source_delimiters_and_raw_contents() {
    let source = "let single=rx\"^\\s*\\$\\{literal\\}$\"\nlet multiline=rx\"\"\"(?x)\n  ^ [a-z]+ # raw comment\n  $\n\"\"\"\n";
    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert!(formatted.formatted.contains("rx\"^\\s*\\$\\{literal\\}$\""));
    assert!(
        formatted
            .formatted
            .contains("rx\"\"\"(?x)\n  ^ [a-z]+ # raw comment\n  $\n\"\"\"")
    );
    let before = parse_lint_source(source);
    let after = parse_lint_source(&formatted.formatted);
    assert!(after.diagnostics.is_empty(), "{:?}", after.diagnostics);
    assert_eq!(
        before
            .arena
            .arena
            .regex_literals
            .iter()
            .map(|l| l.pattern.clone())
            .collect::<Vec<_>>(),
        after
            .arena
            .arena
            .regex_literals
            .iter()
            .map(|l| l.pattern.clone())
            .collect::<Vec<_>>()
    );
    assert_parse_check_standalone("formatted regex literals", &formatted.formatted);
    let again = Formatter::new().format_source(SourceId::new(0), &formatted.formatted);
    assert_eq!(formatted.formatted, again.formatted);
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
        let linted = Linter::lint(
            &parsed.arena,
            source,
            LintOptions {
                prefer_inferred_pure_returns: true,
                ..LintOptions::default()
            },
        );
        assert!(
            !linted
                .diagnostics
                .iter()
                .any(|d| d.code.map(DiagnosticCode::name)
                    == Some("lint.prefer-inferred-pure-return")),
            "{source}"
        );
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
        let linted = Linter::lint(
            &parsed.arena,
            source,
            LintOptions {
                expr_types: checked.expr_types,
                ..LintOptions::default()
            },
        );
        assert!(
            !linted
                .diagnostics
                .iter()
                .any(|d| d.code.map(DiagnosticCode::name) == Some("lint.prefer-value-pipeline")),
            "{source}"
        );
    }
    let source = "pure inner(value: Int) -> Int { value + 1 }\npure outer(prefix: Int, value: Int) -> Int { prefix + value }\nlet selected = outer(10, # keep this explanation\n  inner(2))\n";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);
    let linted = Linter::lint(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    );
    let diagnostic = linted
        .diagnostics
        .iter()
        .find(|d| d.code.map(DiagnosticCode::name) == Some("lint.prefer-value-pipeline"))
        .expect("manual pipeline suggestion");
    assert!(diagnostic.fix_hints.is_empty());
}

#[test]
fn duration_arithmetic_conversion_refuses_custom_module_alias() {
    let root = TempDir::new().unwrap();
    let entry = root.path().join("entry.xsh");
    fs::write(root.path().join("helper.xsh"), "##! Custom duration conversion.\n## Returns a fixed duration independent of the count.\nexport pure millis(count: Int) -> Duration { let _ = count; 2s }\n").unwrap();
    let source = "use helper as time\nlet pause = time.millis(250)\n";
    let loaded = parse_load_check_text(
        entry.to_str().unwrap(),
        source.to_string(),
        Vec::new(),
        Default::default(),
    );
    assert!(
        loaded.parsed.diagnostics.is_empty(),
        "{:?}",
        loaded.parsed.diagnostics
    );
    let checked = loaded.checked.unwrap();
    assert!(
        checked
            .diagnostics
            .iter()
            .any(|d| d.code.map(DiagnosticCode::name) == Some("check.standard-module-shadow")),
        "{:?}",
        checked.diagnostics
    );
    let output = Linter::lint(
        &loaded.parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    );
    assert!(
        !output
            .diagnostics
            .iter()
            .any(|d| d.code.map(DiagnosticCode::name) == Some("lint.duration-arithmetic"))
    );
}

#[test]
fn block_string_concatenation_fix_retains_dynamic_interpolation_comments_crlf_and_consumers() {
    for source in [
        "let value = \"first\\n\" + dynamic\n",
        "let value = \"first\\n\" + f\"{dynamic}\"\n",
        "let value = \"first\\r\\n\" + \"second\"\n",
    ] {
        let parsed = parse_lint_source(source);
        assert!(
            !Linter::lint(&parsed.arena, source, LintOptions::default())
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                    == Some("lint.prefer-block-string")),
            "{source}"
        );
    }
    // Comments and expression consumers are layout: the constant chain is
    // still reported, once at its outermost node, but no rewrite is offered.
    // Redundant grouping is absorbed by the rewrite instead.
    for (source, fixed) in [
        ("let value = \"first\\n\" + \"second\" # retain\n", None),
        (
            "let size = (\"first\\n\" + \"second\").count_chars()\n",
            None,
        ),
        (
            "assert value == (\"first\\n\" + \"second\" + \"third\")\n",
            Some("assert value == \"\"\"\n  first\n  secondthird\n  \"\"\"\n"),
        ),
        (
            "let value = \"\"\"\n  first\n  \"\"\" + \"second\" + \"\\n\"\n",
            Some("let value = \"\"\"\n  \n    first\n    second\n  \n  \"\"\"\n"),
        ),
    ] {
        let parsed = parse_lint_source(source);
        let diagnostics = Linter::lint(&parsed.arena, source, LintOptions::default()).diagnostics;
        let reported = diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.code.map(DiagnosticCode::name) == Some("lint.prefer-block-string")
            })
            .collect::<Vec<_>>();
        assert_eq!(reported.len(), 1, "{source}: {diagnostics:?}");
        let rewritten = reported[0].fix_hints.first().map(|fix| {
            let mut text = source.to_owned();
            text.replace_range(fix.span.unwrap().range(), fix.replacement.as_ref().unwrap());
            text
        });
        assert_eq!(rewritten.as_deref(), fixed, "{source}");
    }
}

#[test]
fn linter_named_argument_spread_requires_checked_record_facts() {
    let source = include_str!("../../../tests/fixtures/syntax/valid/named-argument-forwarding.xsh");
    let parsed = parse_lint_source(source);
    let diagnostics = Linter::lint(&parsed.arena, source, LintOptions::default()).diagnostics;
    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                == Some("lint.prefer-named-argument-spread"))
    );
}

fn private_effects_lints(source: &str, enabled: bool) -> Vec<Diagnostic> {
    let parsed = parse_lint_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    Linter::lint(
        &parsed.arena,
        source,
        LintOptions {
            prefer_inferred_private_effects: enabled,
            ..LintOptions::default()
        },
    )
    .diagnostics
    .into_iter()
    .filter(|diagnostic| {
        diagnostic.code.map(DiagnosticCode::name) == Some("lint.prefer-inferred-private-effects")
    })
    .collect()
}

#[test]
fn private_effects_removal_retains_bounds_entry_points_exports_and_recursion() {
    for source in [
        // Narrower than the clause: deleting it would narrow callers' view.
        "proc deliberate() [time] -> Int { 42 }\n",
        "proc main() [time] -> Int { let _ = time.now(); 42 }\n",
        "cli main(count: Int) [time] { let _ = time.now(); print $count }\n",
        "test registered [error] { assert true, \"checked\" }\n",
        "##! Public boundary.\n## Clock.\nexport proc published() [time] -> Int { let _ = time.now(); 42 }\n",
        "proc dynamic(callback: Proc) [io] -> Int { let _ = callback.call(); 42 }\n",
        // Only the clause's own recursive use supplies `time`; without the
        // clause the cycle infers nothing.
        "proc spin(count: Int) [time] -> Int {\n  if count == 0 { return 0 }\n  spin(count - 1)\n}\n",
        "proc ping(count: Int) [time] -> Int {\n  if count == 0 { return 0 }\n  pong(count - 1)\n}\nproc pong(count: Int) -> Int { ping(count) }\n",
    ] {
        assert!(
            private_effects_lints(source, true).is_empty(),
            "{source}"
        );
    }
}

#[test]
fn signature_cli_migration_retains_comments_and_advanced_cli_policy() {
    let simple = "type Options = {jobs: Int}\nproc main(...argv: List[Str]) [error] {\n  # Preserve the options explanation.\n  let {jobs}: Options = cli.parse(argv, {jobs: {kind: \"Int\", default: 4, help: \"Int, default: 4\"}})?\n  print $jobs\n}\n";
    let parsed = parse_lint_source(simple);
    let diagnostics = Linter::lint(&parsed.arena, simple, LintOptions::default()).diagnostics;
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.prefer-signature-cli")
        })
        .unwrap();
    assert!(diagnostic.fix_hints.is_empty());
    for source in [
        simple
            .replace("# Preserve the options explanation.\n  ", "")
            .replace("kind: \"Int\"", "kind: \"Int\", short: \"j\""),
        simple.replace("print $jobs", "print ${argv.len()} $jobs"),
        format!("let initialized = time.now()\n{simple}"),
        simple.replace("Int, default: 4", "Number of workers"),
        simple.replace("default: 4", "default: cpu.count()"),
    ] {
        let parsed = parse_lint_source(&source);
        let diagnostics = Linter::lint(&parsed.arena, &source, LintOptions::default()).diagnostics;
        assert!(
            !diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                    == Some("lint.prefer-signature-cli")),
            "{source}: {diagnostics:?}"
        );
    }
}

#[test]
fn absence_lookup_literal_fallback_fix_rechecks_and_converges() {
    let source = "let entries: Map[Int] = {one: 1}\nlet value = entries.get(\"missing\", 7)\nlet octet = \"a\".byte_at(2, -1)\n";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);
    let output = Linter::lint(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    );
    let mut fixes = output
        .diagnostics
        .iter()
        .filter(|d| d.code.map(DiagnosticCode::name) == Some("lint.lookup-fallback"))
        .map(|d| &d.fix_hints[0])
        .collect::<Vec<_>>();
    assert_eq!(fixes.len(), 2);
    fixes.sort_by_key(|fix| std::cmp::Reverse(fix.span.unwrap().start()));
    let mut fixed = source.to_string();
    for fix in fixes {
        fixed.replace_range(fix.span.unwrap().range(), fix.replacement.as_ref().unwrap());
    }
    assert_parse_check_standalone("absence lookup fallback", &fixed);
    let formatted = Formatter::new().format_source(SourceId::new(0), &fixed);
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
        !second
            .diagnostics
            .iter()
            .any(|d| d.code.map(DiagnosticCode::name) == Some("lint.lookup-fallback"))
    );
    assert_eq!(
        Formatter::new()
            .format_source(SourceId::new(0), &formatted.formatted)
            .formatted,
        formatted.formatted
    );
}

#[test]
fn absence_lookup_path_literal_fallback_fix_rechecks_and_converges() {
    let source = "let paths: List[Path] = [p\"one\"]\nlet selected = paths.get(4, p\".\")\nprint $selected\n";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);
    let output = Linter::lint(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    );
    let fix = output
        .diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.lookup-fallback")
        })
        .flat_map(|diagnostic| &diagnostic.fix_hints)
        .next()
        .expect("inert Path fallback must be fixable");
    let mut fixed = source.to_owned();
    fixed.replace_range(fix.span.unwrap().range(), fix.replacement.as_ref().unwrap());
    assert_parse_check_standalone("Path lookup fallback", &fixed);
    let parsed = parse_lint_source(&fixed);
    let checked = Checker::check_arena(&parsed.arena, &fixed);
    assert!(!Linter::lint(&parsed.arena, &fixed, LintOptions { expr_types: checked.expr_types, ..LintOptions::default() }).diagnostics
        .iter().any(|diagnostic| diagnostic.code.map(DiagnosticCode::name) == Some("lint.lookup-fallback")));
}

#[test]
fn absence_lookup_immutable_fallback_fix_preserves_typed_parameter_and_alias() {
    let source = "pure pick(values: List[Str], fallback: Str) -> Str {\n  let default_value = fallback\n  values.get(4, default_value)\n}\n";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);
    let output = Linter::lint(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    );
    let fix = output
        .diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.lookup-fallback")
        })
        .flat_map(|diagnostic| &diagnostic.fix_hints)
        .next()
        .expect("immutable typed fallback must be fixable");
    let mut fixed = source.to_owned();
    fixed.replace_range(fix.span.unwrap().range(), fix.replacement.as_ref().unwrap());
    assert_parse_check_standalone("immutable lookup fallback", &fixed);
    assert!(fixed.contains("?? default_value"));
}

#[test]
fn absence_lookup_fallback_fix_refuses_eager_effects_failure_comments_and_named_order() {
    for source in [
        "pure fallback() -> Int { 1 / 0 }\nlet values = [1]\nlet value = values.get(0, fallback())\n",
        "let values = [1]\nlet value = values.get(0, 1 / 0)\n",
        "let values = [1]\nlet value = values.get(\n  0, # preserve this explanation\n  7\n)\n",
        "let values = [1]\nlet value = values.get(fallback: 7, index: 0)\n",
        "let values = [1]\nvar fallback = 7\nlet value = values.get(0, fallback)\nfallback = 8\n",
    ] {
        let parsed = parse_lint_source(source);
        let checked = Checker::check_arena(&parsed.arena, source);
        let output = Linter::lint(
            &parsed.arena,
            source,
            LintOptions {
                expr_types: checked.expr_types,
                ..LintOptions::default()
            },
        );
        assert!(
            !output
                .diagnostics
                .iter()
                .filter(|d| d.code.map(DiagnosticCode::name) == Some("lint.lookup-fallback"))
                .any(|d| !d.fix_hints.is_empty())
        );
    }
}

#[test]
fn scalar_iteration_fixes_recheck_and_converge_with_comments_and_scopes() {
    let source = "let text = \"éx\"\nfor character in text.split(\"\") { print $character }\nlet characters = [character for character in text.split(separator: \"\")]\nlet payload = b\"\\x00\\xff\"\nfor index in range(payload.len()) {\n  let octet = payload.byte_at(index)\n  # preserve this body comment\n  let _ = octet\n}\nfor character in [part for part in text.split(\"\")].join(\"\").split(\"\") { let _ = character }\nfor character in \"ab\".split(\"\") { let _ = character }\nprint ${characters.len()}\n";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let output = Linter::lint(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    );
    let mut fixes: Vec<_> = output
        .diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.prefer-scalar-iteration")
        })
        .flat_map(|diagnostic| &diagnostic.fix_hints)
        .collect();
    assert_eq!(fixes.len(), 6, "{:?}", output.diagnostics);
    fixes.sort_by_key(|fix| fix.span.unwrap().start());
    let mut fixed = source.to_string();
    for fix in fixes.into_iter().rev() {
        fixed.replace_range(
            fix.span.unwrap().range(),
            fix.replacement.as_deref().unwrap(),
        );
    }
    assert!(fixed.contains("for character in text {"));
    assert!(fixed.contains("[character for character in text]"));
    assert!(fixed.contains("for octet in payload {\n  # preserve this body comment"));
    assert_parse_check_standalone("direct scalar iteration", &fixed);
    let formatted = Formatter::new().format_source(SourceId::new(0), &fixed);
    assert!(formatted.diagnostics.is_empty());
    assert_eq!(
        Formatter::new()
            .format_source(SourceId::new(0), &formatted.formatted)
            .formatted,
        formatted.formatted
    );
    let parsed = parse_lint_source(&fixed);
    let checked = Checker::check_arena(&parsed.arena, &fixed);
    let output = Linter::lint(
        &parsed.arena,
        &fixed,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    );
    assert!(
        !output
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                == Some("lint.prefer-scalar-iteration"))
    );
}

#[test]
fn scalar_iteration_fixes_refuse_used_adapters_offsets_mutation_and_partial_ranges() {
    for source in [
        "let text = \"ab\"\nlet parts = text.split(\"\")\nfor part in parts { print $part }\nprint ${parts.len()}\n",
        "for character in \"ab\".split(\"\", maxsplit: 1) { print $character }\n",
        "let payload = b\"ab\"\nfor index in range(payload.len()) {\n  let octet = payload.byte_at(index)\n  print $index\n  let _ = octet\n}\n",
        "var payload = b\"ab\"\nfor index in range(payload.len()) {\n  let octet = payload.byte_at(index)\n  payload = b\"xy\"\n  let _ = octet\n}\n",
        "let payload = b\"ab\"\nfor index in range(1, payload.len()) {\n  let octet = payload.byte_at(index)\n  let _ = octet\n}\n",
        "let text = \"é\"\nfor index in range(text.byte_len()) {\n  let octet = text.byte_at(index)\n  let _ = octet\n}\n",
        "let payload = b\"ab\"\nfor index in range(payload.len()) {\n  let octet = payload.byte_at(index) # preserve extraction\n  let _ = octet\n}\n",
    ] {
        let parsed = parse_lint_source(source);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(
            checked.diagnostics.is_empty(),
            "{source}: {:?}",
            checked.diagnostics
        );
        let output = Linter::lint(
            &parsed.arena,
            source,
            LintOptions {
                expr_types: checked.expr_types,
                ..LintOptions::default()
            },
        );
        assert!(
            !output
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                    == Some("lint.prefer-scalar-iteration")),
            "{source}"
        );
    }
}

#[test]
fn explicit_accept_policy_keeps_propagation_and_custom_status_handlers() {
    let source = "proc main() [process, error] {\n  run --accept=[0,1] grep pattern file ?\n  let status = run.status --accept=[0,1] grep pattern file\n  if status.exited_with(1) { print \"no rows\" }\n}\n";
    let parsed = parse_lint_source(source);
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
    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name) == Some("lint.run-status"))
    );
    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(
        formatted
            .formatted
            .contains("--accept=[0, 1] grep pattern file ?")
    );
    assert!(formatted.formatted.contains("if status.exited_with(1)"));
    assert_parse_check_standalone("accept policy", &formatted.formatted);
    let again = Formatter::new().format_source(SourceId::new(0), &formatted.formatted);
    assert_eq!(again.formatted, formatted.formatted);
}

#[test]
fn record_proof_fallback_fix_requires_checked_presence_and_inert_data() {
    let source = "pure select(value: Str?) -> Str {\n  let available = value != null\n  guard available else { return \"missing\" }\n  value ?? \"fallback\"\n}\n";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let linted = Linter::lint(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            proven_nonnull_fallback_receivers: checked.proven_nonnull_fallback_receivers,
            ..LintOptions::default()
        },
    );
    let fix = &linted
        .diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.redundant-optional-fallback")
        })
        .unwrap()
        .fix_hints[0];
    let mut fixed = source.to_string();
    fixed.replace_range(fix.span.unwrap().range(), fix.replacement.as_ref().unwrap());
    assert!(fixed.contains("  value\n"));
    assert_parse_check_standalone("proved fallback", &fixed);
    let parsed = parse_lint_source(&fixed);
    let checked = Checker::check_arena(&parsed.arena, &fixed);
    let again = Linter::lint(
        &parsed.arena,
        &fixed,
        LintOptions {
            expr_types: checked.expr_types,
            proven_nonnull_fallback_receivers: checked.proven_nonnull_fallback_receivers,
            ..LintOptions::default()
        },
    );
    assert!(
        !again
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                == Some("lint.redundant-optional-fallback"))
    );
    for source in [
        "pure select(value: Str?) -> Str { value ?? \"fallback\" }\n",
        "pure fallback() -> Str { \"fallback\" }\npure select(value: Str?) -> Str { guard value != null else { return \"missing\" }; value ?? fallback() }\n",
    ] {
        let parsed = parse_lint_source(source);
        let checked = Checker::check_arena(&parsed.arena, source);
        let linted = Linter::lint(
            &parsed.arena,
            source,
            LintOptions {
                expr_types: checked.expr_types,
                proven_nonnull_fallback_receivers: checked.proven_nonnull_fallback_receivers,
                ..LintOptions::default()
            },
        );
        assert!(
            !linted
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                    == Some("lint.redundant-optional-fallback"))
        );
    }
}

#[test]
fn constant_key_projection_identity_require_fix_preserves_boundaries() {
    let source = "type Config = {workers: Int}\n# Preserve worker contract α.\nproc read(config: Config) [error] -> Int { config.get(\n# Preserve the selected field.\n\"workers\")?.require(Int)? }\n";
    let parsed = parse_lint_source(source);
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
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.redundant-require")
        })
        .expect("identity validation fix");
    let fix = &diagnostic.fix_hints[0];
    let mut fixed = source.to_string();
    fixed.replace_range(fix.span.unwrap().range(), fix.replacement.as_ref().unwrap());
    assert!(fixed.contains("# Preserve worker contract α."));
    assert!(fixed.contains("# Preserve the selected field."));
    assert!(!fixed.contains("require(Int)"));
    assert_parse_check_standalone("typed field require", &fixed);
    let reparsed = parse_lint_source(&fixed);
    let checked = Checker::check_arena(&reparsed.arena, &fixed);
    let second = Linter::lint(
        &reparsed.arena,
        &fixed,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    );
    assert!(
        !second
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                == Some("lint.redundant-require"))
    );
    for source in [
        "proc read(config: Record, key: Str) [error] -> Int { config.get(key)?.require(Int)? }\n",
        "type Config = {path: Str}\nproc read(config: Config) [error] -> Path { config.get(\"path\")?.require(Path)? }\n",
        "type Config = {count: UInt}\nproc read(config: Config) [error] -> UInt { config.get(\"count\")?.require(UInt)? }\n",
        "type Wide = {name: Str, extra: Int}\ntype Narrow = {name: Str}\ntype Config = {entry: Wide}\nproc read(config: Config) [error] -> Narrow { config.get(\"entry\")?.require(Narrow)? }\n",
    ] {
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
        assert!(
            !diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                    == Some("lint.redundant-require")),
            "{source}"
        );
    }
}

#[test]
fn wire_enum_mapping_expression_walk_preserves_wire_bytes_in_safe_edits() {
    let source = "enum State: Str {\n  Ready = \"ready\\n\" + \"empty\\n\"\n}\nlet state: State = Ready\nprint json.encode(state)?\n";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let diagnostics = Linter::lint(&parsed.arena, source, LintOptions::default()).diagnostics;
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.prefer-block-string")
        })
        .expect("mapping expression is visited");
    let fix = &diagnostic.fix_hints[0];
    let mut fixed = source.to_string();
    fixed.replace_range(fix.span.unwrap().range(), fix.replacement.as_ref().unwrap());
    let reparsed = parse_lint_source(&fixed);
    assert!(
        reparsed.diagnostics.is_empty(),
        "{:?}",
        reparsed.diagnostics
    );
    let rechecked = Checker::check_arena(&reparsed.arena, &fixed);
    assert!(
        rechecked.diagnostics.is_empty(),
        "{:?}",
        rechecked.diagnostics
    );
    let original = Checker::check_compact_declarations(&parsed.arena).wire_enums;
    let rewritten = Checker::check_compact_declarations(&reparsed.arena).wire_enums;
    assert_eq!(
        original
            .mappings
            .values()
            .next()
            .unwrap()
            .variants
            .values()
            .next(),
        rewritten
            .mappings
            .values()
            .next()
            .unwrap()
            .variants
            .values()
            .next()
    );
}

#[test]
fn context_scope_scaffold_fix_preserves_checked_value_type_and_converges() {
    let source = "proc example() [env, error] {\n  var selected = \"\"\n  env ({XSH_SCOPE: \"inner\"}) { selected = env.get(\"XSH_SCOPE\")? }?\n  print $selected\n}\n";
    let parsed = parse_lint_source(source);
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
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.prefer-context-scope-value")
        })
        .expect("fresh scaffold");
    assert_eq!(diagnostic.fix_hints.len(), 1);
    let fix = &diagnostic.fix_hints[0];
    let mut fixed = source.to_string();
    fixed.replace_range(fix.span.unwrap().range(), fix.replacement.as_ref().unwrap());
    assert_parse_check_standalone("context scope", &fixed);
    let formatted = Formatter::new().format_source(SourceId::new(0), &fixed);
    assert!(formatted.diagnostics.is_empty());
    let again = Formatter::new().format_source(SourceId::new(0), &formatted.formatted);
    assert_eq!(again.formatted, formatted.formatted);
    let parsed = parse_lint_source(&formatted.formatted);
    let checked = Checker::check_arena(&parsed.arena, &formatted.formatted);
    let second = Linter::lint(
        &parsed.arena,
        &formatted.formatted,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    )
    .diagnostics;
    assert!(
        !second
            .iter()
            .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                == Some("lint.prefer-context-scope-value"))
    );
}

#[test]
fn context_scope_scaffold_declines_cleanup_comments_and_placeholder_reads() {
    for source in [
        "on TERM [env, error] { let ignored = env.get(\"X\")?; }\nproc example() [env, error] { var selected = \"\"; env ({X: \"inner\"}) { selected = env.get(\"X\")? }?; print $selected }\n",
        "proc example() [env, error] { var selected = \"\"; defer { print $selected }; env ({X: \"inner\"}) { selected = env.get(\"X\")? }?; print $selected }\n",
        "proc example() [env, error] { var selected = \"\"; env ({X: \"inner\"}) { # assignment timing\n selected = env.get(\"X\")? }?; print $selected }\n",
        "proc example() [env, error] { var selected = \"\"; env ({X: selected}) { selected = env.get(\"X\")? }?; print $selected }\n",
    ] {
        let parsed = parse_lint_source(source);
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
        assert!(
            diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                    == Some("lint.prefer-context-scope-value"))
                .all(|diagnostic| diagnostic.fix_hints.is_empty())
        );
    }
}

#[test]
fn context_scope_environment_migration_preserves_comments_and_rechecks() {
    let source =
        "env {\n  X = \"one\" # selected once\n  Y = 2;\n} { print ${env.get(\"X\")?} }?\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert_eq!(parsed.diagnostics.len(), 1, "{:?}", parsed.diagnostics);
    let diagnostic = &parsed.diagnostics[0];
    assert_eq!(
        diagnostic.code.map(DiagnosticCode::name),
        Some("parse.env-scope-migration")
    );
    let mut edits = diagnostic.fix_hints.iter().collect::<Vec<_>>();
    edits.sort_by_key(|hint| std::cmp::Reverse(hint.span.unwrap().start()));
    let mut fixed = source.to_string();
    for hint in edits {
        fixed.replace_range(
            hint.span.unwrap().range(),
            hint.replacement.as_ref().unwrap(),
        );
    }
    assert!(fixed.contains("# selected once"));
    assert_parse_check_standalone("explicit environment overlay", &fixed);
    let formatted = Formatter::new().format_source(SourceId::new(0), &fixed);
    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    let again = Formatter::new().format_source(SourceId::new(0), &formatted.formatted);
    assert_eq!(again.formatted, formatted.formatted);
}

#[test]
fn generic_record_constructor_alias_fix_rechecks_and_converges() {
    let source = "type Box[T] = {value: T}\ntype Count = Box[Int]\nlet count = Count(value: 7)\nprint ${count.value + 1}\n";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let output = Linter::lint(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    );
    let diagnostic = output
        .diagnostics
        .iter()
        .find(|d| {
            d.code.map(DiagnosticCode::name) == Some("lint.prefer-generic-record-constructor")
        })
        .unwrap();
    assert_eq!(diagnostic.fix_hints.len(), 1);
    let hint = &diagnostic.fix_hints[0];
    let mut fixed = source.to_string();
    fixed.replace_range(
        hint.span.unwrap().range(),
        hint.replacement.as_deref().unwrap(),
    );
    assert!(fixed.contains("type Count = Box[Int]"));
    assert!(fixed.contains("let count = Box(value: 7)"));
    assert_parse_check_standalone("inferred alias constructor", &fixed);
    let parsed = parse_lint_source(&fixed);
    let checked = Checker::check_arena(&parsed.arena, &fixed);
    let repeated = Linter::lint(
        &parsed.arena,
        &fixed,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    );
    assert!(!repeated.diagnostics.iter().any(
        |d| d.code.map(DiagnosticCode::name) == Some("lint.prefer-generic-record-constructor")
    ));
}

#[test]
fn generic_record_constructor_alias_fix_preserves_conversion_and_ambiguous_evidence() {
    for source in [
        "type Box[T] = {value: T?}\ntype Count = Box[Int]\nlet count = Count(value: null)\n",
        "type Box[T] = {value: List[T]}\ntype Count = Box[Int]\nlet count = Count(value: [])\n",
        "type Box[T] = {value: T}\ntype Count = Box[UInt]\nlet count = Count(value: 7)\n",
        "type Box[T] = {value: T}\ntype Count = Box[Int]\nlet count = Count(...{value: 7})\n",
        "type Box[T] = {value: T}\ntype Count = Box[Int]\nlet count = Count(\n# preserve this argument comment\nvalue: 7)\n",
    ] {
        let parsed = parse_lint_source(source);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(
            checked.diagnostics.is_empty(),
            "{source}: {:?}",
            checked.diagnostics
        );
        let output = Linter::lint(
            &parsed.arena,
            source,
            LintOptions {
                expr_types: checked.expr_types,
                ..LintOptions::default()
            },
        );
        let diagnostic = output
            .diagnostics
            .iter()
            .find(|d| {
                d.code.map(DiagnosticCode::name) == Some("lint.prefer-generic-record-constructor")
            })
            .unwrap();
        assert!(diagnostic.fix_hints.is_empty(), "{source}");
    }
}

#[test]
fn default_parameter_annotation_fixes_recheck_preserve_comments_and_converge() {
    let source = "# café precedes every edit.\nconst defaults = {jobs: 4}\npure next() -> Int { 3 }\npure choose(jobs: Int = defaults.jobs + 1, value: Int = next()) -> Int {\n  # café remains attached to the body.\n  jobs + value\n}\nlet result = choose(value: 7)\n";
    let parsed = parse_lint_source(source);
    let output = Linter::lint(&parsed.arena, source, LintOptions::default());
    let fixes = output
        .diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.default-param-type")
        })
        .collect::<Vec<_>>();
    assert_eq!(fixes.len(), 2, "{:?}", output.diagnostics);
    let mut fixed = source.to_string();
    for diagnostic in fixes.into_iter().rev() {
        let hint = &diagnostic.fix_hints[0];
        fixed.replace_range(
            hint.span.unwrap().range(),
            hint.replacement.as_deref().unwrap(),
        );
    }
    assert!(fixed.contains("jobs = defaults.jobs + 1, value = next()"));
    assert!(fixed.contains("# café remains attached to the body."));
    assert_parse_check_standalone("semantic default types", &fixed);
    let parsed = parse_lint_source(&fixed);
    assert!(
        !Linter::lint(&parsed.arena, &fixed, LintOptions::default())
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                == Some("lint.default-param-type"))
    );
}

#[test]
fn default_parameter_annotation_keeps_domains_context_and_ambiguous_defaults() {
    for source in [
        "pure choose(value: UInt = 4) -> UInt { value }\n",
        "pure choose(value: Int? = 4) -> Int? { value }\n",
        "pure choose(value: Int? = null) -> Int? { value }\n",
        "pure choose(value: List[Int] = []) -> List[Int] { value }\n",
        "type Config { jobs: Int }\npure choose(value: Config = {jobs: 4}) -> Config { value }\n",
        "pure choose(value:\n# keep this contract comment\nInt = 4) -> Int { value }\n",
    ] {
        let parsed = parse_lint_source(source);
        assert!(
            !Linter::lint(&parsed.arena, source, LintOptions::default())
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                    == Some("lint.default-param-type")),
            "{source}"
        );
    }
}

#[test]
fn inferred_require_target_fix_preserves_validation_and_converges() {
    let source = "type Manifest = {jobs: UInt}\nlet raw: Any = {jobs: 4}\nlet value: Manifest = raw.require(Manifest)?\n";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let output = Linter::lint(
        &parsed.arena,
        source,
        LintOptions {
            requirement_targets: checked.requirement_targets,
            requirement_expected_targets: checked.requirement_expected_targets,
            ..LintOptions::default()
        },
    );
    let hint = &output
        .diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("lint.inferred-require-target")
        })
        .expect("same anchored schema")
        .fix_hints[0];
    let mut fixed = source.to_string();
    fixed.replace_range(
        hint.span.unwrap().range(),
        hint.replacement.as_ref().unwrap(),
    );
    assert!(fixed.contains("raw.require()?"));
    assert_parse_check_standalone("inferred require", &fixed);
    let parsed = parse_lint_source(&fixed);
    let checked = Checker::check_arena(&parsed.arena, &fixed);
    let second = Linter::lint(
        &parsed.arena,
        &fixed,
        LintOptions {
            requirement_targets: checked.requirement_targets,
            requirement_expected_targets: checked.requirement_expected_targets,
            ..LintOptions::default()
        },
    );
    assert!(
        !second
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                == Some("lint.inferred-require-target"))
    );
}

#[test]
fn inferred_require_target_fix_rejects_unanchored_and_different_instances() {
    for source in [
        "type Manifest = {jobs: UInt}\nlet raw: Any = {jobs: 4}\nlet value = raw.require(Manifest)?\n",
        "type Marker[T] = {name: Str}\nlet raw: Any = {name: \"ready\"}\nlet value: Marker[Int] = raw.require(Marker[Str])?\n",
    ] {
        let parsed = parse_lint_source(source);
        let checked = Checker::check_arena(&parsed.arena, source);
        let output = Linter::lint(
            &parsed.arena,
            source,
            LintOptions {
                requirement_targets: checked.requirement_targets,
                requirement_expected_targets: checked.requirement_expected_targets,
                ..LintOptions::default()
            },
        );
        assert!(
            !output
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                    == Some("lint.inferred-require-target")),
            "{source}"
        );
    }
}

#[test]
fn inferred_require_formatting_round_trip_and_comments_preserve_the_operation() {
    let source = "type Row = {name: Str}\nlet raw: Any = {name: \"ready\"}\nlet inferred: Row = raw.require()?\nlet explicit: Row = raw.require(\n  # Preserve the boundary explanation.\n  Row\n)?\n";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let output = Linter::lint(
        &parsed.arena,
        source,
        LintOptions {
            requirement_targets: checked.requirement_targets,
            requirement_expected_targets: checked.requirement_expected_targets,
            ..LintOptions::default()
        },
    );
    assert!(
        !output
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                == Some("lint.inferred-require-target"))
    );
    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert!(formatted.formatted.contains("raw.require()?"));
    assert!(
        formatted
            .formatted
            .contains("# Preserve the boundary explanation.")
    );
    assert_parse_check_standalone("formatted inferred require", &formatted.formatted);
    assert_eq!(
        Formatter::new()
            .format_source(SourceId::new(0), &formatted.formatted)
            .formatted,
        formatted.formatted
    );
}

#[test]
fn local_inference_annotation_fix_preserves_all_checked_expression_types_and_converges() {
    for source in [
        "proc gather() -> List[Path] {\n  # preserve initializer evidence\n  var entries: List[Path] = []\n  for destination in [p\"one\"] {\n    entries += [destination]\n  }\n\n  entries\n}\n",
        "proc choose() -> Path? {\n  var selected: Path? = null\n  for destination in [p\"one\"] {\n    selected = destination\n  }\n\n  selected\n}\n",
        "pure size(items: List[Path]) -> Int { items.len() }\nproc count() -> Int { let entries: List[Path] = []; size(entries) }\n",
    ] {
        let parsed = parse_lint_source(source);
        let output = Linter::lint(&parsed.arena, source, LintOptions::default());
        let diagnostic = output
            .diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.code.map(DiagnosticCode::name) == Some("lint.needless-annotation")
            })
            .expect(source);
        let hint = diagnostic
            .fix_hints
            .first()
            .expect("proved local annotation deletion");
        let mut fixed = source.to_owned();
        fixed.replace_range(
            hint.span.unwrap().range(),
            hint.replacement.as_deref().unwrap_or(""),
        );
        if source.contains("# preserve initializer evidence") {
            assert!(fixed.contains("# preserve initializer evidence"));
        }
        assert_parse_check_standalone("local annotation inference", &fixed);
        let parsed = parse_lint_source(&fixed);
        assert!(
            !Linter::lint(&parsed.arena, &fixed, LintOptions::default())
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                    == Some("lint.needless-annotation")),
            "{fixed}"
        );
        let formatted = Formatter::new().format_source(SourceId::new(0), &fixed);
        assert_parse_check_standalone("formatted local inference", &formatted.formatted);
        assert_eq!(
            Formatter::new()
                .format_source(SourceId::new(0), &formatted.formatted)
                .formatted,
            formatted.formatted
        );
    }
}

#[test]
fn local_inference_annotation_fix_requires_identical_material_contract_and_preserves_comments() {
    for source in [
        "proc inspect() -> Unit { let entries: List[Path] = []; print entries.len() }\n",
        "proc choose() -> Unit { var selected: Path? = null; print selected }\n",
        "proc choose() -> Any { var selected: Any = null; selected = 12; selected }\n",
        "proc gather() -> List[UInt] { var entries: List[UInt] = []; entries += [12]; entries }\n",
        "proc counts() -> Map[Int] { let entries: Map[Int] = map.empty(); entries }\n",
        "let entries: List[Path] = []\nprint entries.len()\n",
    ] {
        let parsed = parse_lint_source(source);
        assert!(
            parsed.diagnostics.is_empty(),
            "{source}: {:?}",
            parsed.diagnostics
        );
        let output = Linter::lint(&parsed.arena, source, LintOptions::default());
        assert!(
            !output
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                    == Some("lint.needless-annotation")
                    && !diagnostic.fix_hints.is_empty()),
            "{source}: {:?}",
            output.diagnostics
        );
    }
}

fn bool_statement_fixed(source: &str) -> String {
    let parsed = parse_lint_source(source);
    assert!(
        parsed.diagnostics.is_empty(),
        "{source}: {:?}",
        parsed.diagnostics
    );
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(
        checked
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                == Some("check.bool-statement")),
        "{source}: {:?}",
        checked.diagnostics
    );
    let mut hints = checked
        .diagnostics
        .iter()
        .flat_map(|diagnostic| &diagnostic.fix_hints)
        .collect::<Vec<_>>();
    hints.sort_by_key(|hint| hint.span.unwrap().start());
    assert_eq!(
        hints.len(),
        checked.diagnostics.len(),
        "every Bool statement carries one fix: {:?}",
        checked.diagnostics
    );
    assert!(
        hints.windows(2).all(|pair| pair[0].span != pair[1].span),
        "each statement is reported once: {hints:?}"
    );
    let mut fixed = source.to_owned();
    for hint in hints.iter().rev() {
        fixed.replace_range(
            hint.span.unwrap().range(),
            hint.replacement.as_deref().unwrap(),
        );
    }
    fixed
}

#[test]
fn bool_statement_fix_prefixes_every_checked_bool_statement() {
    let source = r#"let flag = true
let xs = [1, 2]
# a comment before
xs == [1, 2] # a trailing comment
(flag)
xs.len() ==
    2
pure positive(n: Int) -> Bool { n > 0 }
proc check(n: Int) {
    if n > 0 {
        n > 0
    }
    for item in xs {
        item > 0
    }
    match n {
        1 => n == 1,
        _ => n >= 0,
    }
    let _ = n == 3
    assert n < 9, "bounded"
    positive(n)
}
check(1)?
flag
assert flag
"#;
    let fixed = bool_statement_fixed(source);
    assert_eq!(
        fixed,
        r#"let flag = true
let xs = [1, 2]
# a comment before
assert xs == [1, 2] # a trailing comment
assert flag
assert xs.len() ==
    2
pure positive(n: Int) -> Bool { n > 0 }
proc check(n: Int) {
    if n > 0 {
        assert n > 0
    }
    for item in xs {
        assert item > 0
    }
    match n {
        1 => { assert n == 1 },
        _ => { assert n >= 0 },
    }
    let _ = n == 3
    assert n < 9, "bounded"
    assert positive(n)
}
check(1)?
assert flag
assert flag
"#
    );
    assert_parse_check_standalone("bool statement fix", &fixed);
}

#[test]
fn bool_statement_fix_covers_unit_tails_and_test_declarations() {
    let source = "proc ready(flag: Bool) [error] -> Result[Unit] {\n    flag\n}\nproc done(n: Int) [error] -> Result[Unit] {\n    n == 1\n}\ntest arithmetic {\n    1 + 1 == 2\n}\nready(true)?\ndone(1)?\n";
    let fixed = bool_statement_fixed(source);
    assert_eq!(
        fixed,
        "proc ready(flag: Bool) [error] -> Result[Unit] {\n    assert flag\n}\nproc done(n: Int) [error] -> Result[Unit] {\n    assert n == 1\n}\ntest arithmetic {\n    assert 1 + 1 == 2\n}\nready(true)?\ndone(1)?\n"
    );
    assert_parse_check_standalone("bool tail fix", &fixed);
}

#[test]
fn assertion_helper_migration_targets_assert() {
    let source = "proc compare(actual: Int) {\n    test.eq(actual, 3)?\n    match actual {\n        3 => test.ok(actual > 2)?,\n        _ => {},\n    }\n}\n";
    let parsed = parse_lint_source(source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let diagnostics = Linter::lint(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            statement_positions: checked.statement_positions,
            statement_expression_spans: checked.statement_expression_spans,
            assertion_effect_spans: checked.assertion_effect_spans,
            standard_call_spans: checked.standard_call_spans,
            ..LintOptions::default()
        },
    )
    .diagnostics;
    let hints = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.map(DiagnosticCode::name) == Some("lint.core-assert"))
        .flat_map(|diagnostic| &diagnostic.fix_hints)
        .collect::<Vec<_>>();
    assert_eq!(
        hints
            .iter()
            .filter_map(|hint| hint.replacement.as_deref())
            .collect::<Vec<_>>(),
        ["assert actual == 3", "{ assert actual > 2 }"],
        "{diagnostics:?}"
    );
    let mut fixed = source.to_owned();
    for hint in hints.iter().rev() {
        fixed.replace_range(
            hint.span.unwrap().range(),
            hint.replacement.as_deref().unwrap(),
        );
    }
    assert_parse_check_standalone("assertion helper migration", &fixed);
}

#[test]
fn linter_env_strings_replace_literal_reads_and_keep_other_lookups() {
    let source = "let home = env.get(\"HOME\")?\nlet login = env.Str.USER ?? \"nobody\"\nlet named = env.get(name: \"SHELL\") ?? \"sh\"\nlet nested = f\"{env.get(\"TERM\") ?? \"dumb\"}\"\nlet dashed = env.get(\"NOT-AN-IDENT\") ?? \"\"\nlet key = \"HOME\"\nlet computed = env.get(key) ?? \"\"\nlet fallback = env.get_or(\"HOME\", \"/\")?\nlet dir = env.Path.HOME ?? /\nprint $home $login $named $nested $dashed $computed $fallback $dir\n";
    let parsed = parse_lint_source(source);
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
    let mut edits = diagnostics
        .iter()
        .filter(|d| d.code.map(DiagnosticCode::name) == Some("lint.prefer-env-string"))
        .flat_map(|d| &d.fix_hints)
        .map(|h| (h.span.unwrap(), h.replacement.as_ref().unwrap()))
        .collect::<Vec<_>>();
    // Only the four literal identifier reads; `env.get_or` fails on non-UTF-8
    // values where `??` would fall back, so it is not an equivalent rewrite.
    assert_eq!(edits.len(), 4, "{diagnostics:?}");
    edits.sort_by_key(|(span, _)| span.start());
    let mut fixed = source.to_string();
    for (span, replacement) in edits.into_iter().rev() {
        fixed.replace_range(span.range(), replacement);
    }
    assert_eq!(
        fixed,
        "let home = e\"HOME\"?\nlet login = e\"USER\" ?? \"nobody\"\nlet named = e\"SHELL\" ?? \"sh\"\nlet nested = f\"{e\"TERM\" ?? \"dumb\"}\"\nlet dashed = env.get(\"NOT-AN-IDENT\") ?? \"\"\nlet key = \"HOME\"\nlet computed = env.get(key) ?? \"\"\nlet fallback = env.get_or(\"HOME\", \"/\")?\nlet dir = env.Path.HOME ?? /\nprint $home $login $named $nested $dashed $computed $fallback $dir\n"
    );
    assert_parse_check_standalone("env string fixes", &fixed);
    let parsed = parse_lint_source(&fixed);
    let checked = Checker::check_arena(&parsed.arena, &fixed);
    let second = Linter::lint(
        &parsed.arena,
        &fixed,
        LintOptions {
            expr_types: checked.expr_types,
            ..LintOptions::default()
        },
    );
    assert!(
        !second
            .diagnostics
            .iter()
            .any(|d| d.code.map(DiagnosticCode::name) == Some("lint.prefer-env-string"))
    );
}

#[test]
fn linter_env_strings_ignore_shadowed_env_bindings() {
    let source = "let env = {Str: {USER: \"me\"}}\nprint $env.Str.USER\n";
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
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.code.map(DiagnosticCode::name) == Some("lint.prefer-env-string")),
        "{diagnostics:?}"
    );
}
