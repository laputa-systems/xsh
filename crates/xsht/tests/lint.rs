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
