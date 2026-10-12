use super::lint_try_capture_helpers;
use xsh::diagnostic::{Diagnostic, DiagnosticCode};
use xsh::frontend::check::{CheckBundleView, Checker};
use xsh::frontend::source::SourceId;
use xsh::frontend::syntax::parser::Parser;

fn migration(source: &str) -> Vec<Diagnostic> {
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let view = CheckBundleView::for_root(
        &parsed.arena,
        parsed.arena.statements,
        None,
        Some(SourceId::new(0)),
    );
    let baseline = std::sync::Arc::new(checked);
    parsed
        .arena
        .symbol_owner()
        .with_current(|| lint_try_capture_helpers(&view, source, Some(baseline)))
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
fn single_use_read_port_helper_becomes_a_checked_local_capture() {
    let source = "proc read_port() [fs, error] -> Result[Int] {\n  let content = p\"port\".read_text()?\n  content.trim().parse_int()?\n}\nlet port = read_port() ?? 8080\nprint $port\n";
    let diagnostics = migration(source);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(
        diagnostics[0].code,
        Some(DiagnosticCode::LintPreferTryCapture)
    );
    let fixed = apply(&diagnostics, source);
    assert_eq!(
        fixed,
        "\nlet port = try {\n  let content = p\"port\".read_text()?\n  content.trim().parse_int()?\n} ?? 8080\nprint $port\n"
    );
    assert!(migration(&fixed).is_empty());
}

#[test]
fn capture_proof_compares_the_rewritten_source_when_other_module_facts_are_present() {
    use xsh::frontend::check::{EffectDeclarationId, FunctionEffectFact, StatementPosition, Type};
    use xsh::frontend::source::Span;
    use xsh::frontend::symbols::Name;

    let source = "proc read_port() [error] -> Result[Int] { \"7\".parse_int()? }\nlet port = read_port() ?? 8080\nprint $port\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let mut checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let imported_span = Span::new(SourceId::new(1), 0, 1);
    checked.expr_types.insert(imported_span, Type::Int);
    checked.local_binding_types.insert(imported_span, Type::Int);
    checked
        .statement_positions
        .insert(imported_span, StatementPosition::Statement);
    checked.callable_effects.insert("imported".to_owned(), None);
    parsed.arena.symbol_owner().with_current(|| {
        checked.function_effect_facts.insert(
            EffectDeclarationId {
                namespace: Some(Name::intern("imported")),
                body: imported_span,
            },
            FunctionEffectFact::default(),
        );
    });
    let view = CheckBundleView::for_root(
        &parsed.arena,
        parsed.arena.statements,
        None,
        Some(SourceId::new(0)),
    );
    let diagnostics = parsed.arena.symbol_owner().with_current(|| {
        lint_try_capture_helpers(&view, source, Some(std::sync::Arc::new(checked)))
    });

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, Some(DiagnosticCode::LintPreferTryCapture));
    assert_eq!(diagnostics[0].fix_hints.len(), 2);
}

#[test]
fn capture_proof_still_requires_the_complete_original_bundle_to_be_clean() {
    let source = "proc read_port() [error] -> Result[Int] { \"7\".parse_int()? }\nlet port = read_port() ?? 8080\nprint $port\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let mut checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    checked.diagnostics.push(
        xsh::diagnostic::Diagnostic::warning("diagnostic from another loaded source")
            .with_span(xsh::frontend::source::Span::new(SourceId::new(1), 0, 0)),
    );
    let view = CheckBundleView::for_root(
        &parsed.arena,
        parsed.arena.statements,
        None,
        Some(SourceId::new(0)),
    );
    let diagnostics = parsed.arena.symbol_owner().with_current(|| {
        lint_try_capture_helpers(&view, source, Some(std::sync::Arc::new(checked)))
    });

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn local_capture_helper_fix_preserves_success_parse_failure_and_missing_file() {
    use xsh::execution::evaluator::Evaluator;
    use xsh::frontend::source::SourceMap;

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("port");
    let source = format!(
        "proc read_port() [fs, error] -> Result[Int] {{\n  let content = p{path:?}.read_text()?\n  content.trim().parse_int()?\n}}\nlet port = read_port() ?? 8080\nprint $port\n"
    );
    let fixed = apply(&migration(&source), &source);
    assert_ne!(source, fixed);
    let evaluate = |source: &str| {
        let mut sources = SourceMap::new();
        let id = sources.add_file("port-helper.xsh", source);
        let parsed = Parser::parse_source_arena_only(id, source);
        Evaluator::new_with_sources(Vec::new(), sources).eval(&parsed.arena, id)
    };
    for (contents, expected) in [
        (Some(" 9090\n"), b"9090\n".as_slice()),
        (Some("invalid"), b"8080\n".as_slice()),
        (None, b"8080\n".as_slice()),
    ] {
        if let Some(contents) = contents {
            std::fs::write(&path, contents).unwrap();
        } else {
            std::fs::remove_file(&path).unwrap();
        }
        let before = evaluate(&source);
        let after = evaluate(&fixed);
        assert_eq!(before.status, 0, "{:?}", before.diagnostics);
        assert_eq!(after.status, before.status, "{:?}", after.diagnostics);
        assert_eq!(before.stdout, expected);
        assert_eq!(after.stdout, before.stdout);
        assert_eq!(after.stderr, before.stderr);
    }
}

#[test]
fn local_capture_helpers_decline_references_captures_control_and_promises() {
    let body = "proc read_port() [error] -> Result[Int] { \"7\".parse_int()? }\n";
    let use_site = "let port = read_port() ?? 8080\n";
    for source in [
        format!("{body}{use_site}let other = read_port()\n"),
        format!("{body}{use_site}let other = {{read_port}}\n"),
        format!("{body}{use_site}let alias = read_port\n"),
        // An export spells its error type; it is still not a local helper.
        format!("export proc read_port() [error] -> Result[Int, Error] {{ \"7\".parse_int()? }}\n{use_site}"),
        format!("{body}print before\n{use_site}"),
        format!("# Keep the callable boundary.\n{body}{use_site}"),
        format!("proc read_port() [error] -> Result[Int] {{ # explain parsing\n \"7\".parse_int()? }}\n{use_site}"),
        format!("let content = \"7\"\nproc read_port() [error] -> Result[Int] {{ content.parse_int()? }}\n{use_site}"),
        format!("proc read_port(value: Str = \"7\") [error] -> Result[Int] {{ value.parse_int()? }}\n{use_site}"),
        format!("proc read_port() [error] -> Result[Int] {{ return \"7\".parse_int()? }}\n{use_site}"),
        format!("proc read_port() [error] -> Result[Int] {{ let value = retry [] {{ \"7\".parse_int()? }}?; value }}\n{use_site}"),
        format!("proc read_port() [error] -> Result[Int] {{ let value = try {{ \"7\".parse_int()? }}?; value }}\n{use_site}"),
        format!("proc read_port() [error] -> Result[Int] {{ defer {{ print cleanup }}; \"7\".parse_int()? }}\n{use_site}"),
        format!("proc read_port() [fs, error] -> Result[Int] {{ \"7\".parse_int()? }}\n{use_site}"),
        format!("{body}let port = read_port() ?? {{ |_| 8080 }}\n"),
        "proc read_port() [error] -> Result[Result[Int]] { let value = \"7\".parse_int()?; Ok(Ok(value)) }\nlet port: Result[Int] = read_port() ?? Ok(8080)\n".to_owned(),
    ] {
        assert!(migration(&source).is_empty(), "unsafe helper migration: {source}");
    }
}

#[test]
fn local_capture_helper_migration_keeps_unrelated_comments_and_byte_offsets() {
    let source = "let label = \"préfix\"\n\nproc read_port() [error] -> Result[Int] { \"7\".parse_int()? }\nlet port = read_port() ?? 8080 # fallback remains\n# Report the observed value.\nprint $label $port\n";
    let diagnostics = migration(source);
    assert_eq!(diagnostics.len(), 1);
    let fixed = apply(&diagnostics, source);
    assert!(fixed.contains("# fallback remains\n# Report the observed value."));
    assert!(fixed.contains("let label = \"préfix\""));
    assert!(migration(&fixed).is_empty());
    assert!(diagnostics[0].labels.iter().any(|label| {
        label
            .message
            .as_deref()
            .is_some_and(|message| message.contains("traceback frame"))
    }));
}

#[test]
fn local_capture_helpers_leave_native_tests_and_statement_boundaries_unchanged() {
    for source in [
        "proc outer() -> Result[Int] { let value = retry [] { return Ok(7) }?; value }\nlet nested = retry [] { Ok(7) }\n",
        "pure empty() -> Unit {}\nproc consume() [error] -> Result[Unit] { assert false }\nlet outcome: Unit = consume() ?? empty()\n",
        "test helper [error] { assert false }\n",
    ] {
        assert!(migration(source).is_empty());
    }
}

#[test]
fn local_capture_helpers_decline_closed_but_non_scalar_expression_trees() {
    for initializer in [
        "let content = if true { \"7\" } else { \"8\" }\n  content.parse_int()?",
        "let contents = [\"7\"]\n  \"7\".parse_int()?",
    ] {
        let source = format!(
            "proc read_port() [error] -> Result[Int] {{\n  {initializer}\n}}\nlet port = read_port() ?? 8080\n"
        );
        assert!(migration(&source).is_empty(), "{source}");
    }
}

#[test]
fn checked_local_capture_rule_is_exposed_by_the_linter_library() {
    let source = "proc read_port() [error] -> Result[Int] { \"7\".parse_int()? }\nlet port = read_port() ?? 8080\nprint $port\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let output = super::super::Linter::lint(
        &parsed.arena,
        source,
        super::super::LintOptions {
            checked_output: Some(std::sync::Arc::new(checked)),
            ..super::super::LintOptions::default()
        },
    );
    let diagnostics = output
        .diagnostics
        .into_iter()
        .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferTryCapture))
        .collect::<Vec<_>>();
    assert_eq!(diagnostics.len(), 1);
    let fixed = apply(&diagnostics, source);
    assert!(fixed.contains("let port = try { \"7\".parse_int()? } ?? 8080"));
    assert!(migration(&fixed).is_empty());
}

// One chain cannot be long enough to exhaust the walk: the parser's nesting
// limit ends it first. Three bindings that each stay under that limit do,
// and the same body with short chains is still rewritten.
#[test]
fn local_capture_helpers_decline_bodies_beyond_the_bounded_expression_walk() {
    let helper = |calls: usize| {
        let trims = ".trim()".repeat(calls);
        format!(
            "proc read_port() [error] -> Result[Int] {{\n  let first = \"7\"{trims}\n  let second = first{trims}\n  second{trims}.parse_int()?\n}}\n\nlet port = read_port() ?? 8080\n"
        )
    };
    assert_eq!(migration(&helper(2)).len(), 1);
    assert!(migration(&helper(45)).is_empty());
}
