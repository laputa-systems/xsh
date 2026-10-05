#![allow(clippy::single_call_fn)]

use std::fs;
use xsh::diagnostic::DiagnosticCode;

use tempfile::TempDir;
use xsh::diagnostic::Diagnostic;
use xsh::frontend::check::Checker;
use xsh::frontend::load::parse_load_check_text;
use xsh::frontend::source::SourceId;
use xsh::frontend::symbols::{Name, SymbolOwner};
use xsh::frontend::syntax::arena::ArenaProgram;
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
