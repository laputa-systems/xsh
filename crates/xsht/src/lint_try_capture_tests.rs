use super::lint_try_capture_helpers;
use xsh::diagnostic::Diagnostic;
use xsh::frontend::check::Checker;
use xsh::frontend::source::SourceId;
use xsh::frontend::syntax::parser::Parser;

fn migration(source: &str) -> Vec<Diagnostic> {
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    parsed.arena.symbol_owner().with_current(|| lint_try_capture_helpers(&parsed.arena, source))
}

fn apply(diagnostics: &[Diagnostic], source: &str) -> String {
    let mut fixes = diagnostics.iter().flat_map(|diagnostic| &diagnostic.fix_hints).collect::<Vec<_>>();
    fixes.sort_by_key(|fix| std::cmp::Reverse(fix.span.unwrap().start()));
    let mut fixed = source.to_owned();
    for fix in fixes { fixed.replace_range(fix.span.unwrap().range(), fix.replacement.as_deref().unwrap()); }
    fixed
}

#[test]
fn single_use_read_port_helper_becomes_a_checked_local_capture() {
    let source = "proc read_port() [fs, error] -> Result[Int] {\n  let content = p\"port\".read_text()?\n  content.trim().parse_int()?\n}\nlet port = read_port() ?? 8080\nprint $port\n";
    let diagnostics = migration(source);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code.as_deref(), Some("lint.prefer-try-capture"));
    let fixed = apply(&diagnostics, source);
    assert_eq!(fixed, "\nlet port = try {\n  let content = p\"port\".read_text()?\n  content.trim().parse_int()?\n} ?? 8080\nprint $port\n");
    assert!(migration(&fixed).is_empty());
}

#[test]
fn local_capture_helper_fix_preserves_success_parse_failure_and_missing_file() {
    use xsh::execution::evaluator::Evaluator;
    use xsh::frontend::source::SourceMap;

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("port");
    let source = format!("proc read_port() [fs, error] -> Result[Int] {{\n  let content = p{path:?}.read_text()?\n  content.trim().parse_int()?\n}}\nlet port = read_port() ?? 8080\nprint $port\n");
    let fixed = apply(&migration(&source), &source);
    assert_ne!(source, fixed);
    let evaluate = |source: &str| {
        let mut sources = SourceMap::new();
        let id = sources.add_file("port-helper.xsh", source);
        let parsed = Parser::parse_source_arena_only(id, source);
        Evaluator::new_with_sources(Vec::new(), sources).eval(&parsed.arena, id)
    };
    for (contents, expected) in [(Some(" 9090\n"), b"9090\n".as_slice()), (Some("invalid"), b"8080\n".as_slice()), (None, b"8080\n".as_slice())] {
        if let Some(contents) = contents { std::fs::write(&path, contents).unwrap(); }
        else { std::fs::remove_file(&path).unwrap(); }
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
        format!("export {body}{use_site}"),
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
    assert!(diagnostics[0].labels.iter().any(|label| label.message.as_deref().is_some_and(|message| message.contains("traceback frame"))));
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
fn checked_local_capture_rule_is_exposed_by_the_linter_library() {
    let source = "proc read_port() [error] -> Result[Int] { \"7\".parse_int()? }\nlet port = read_port() ?? 8080\nprint $port\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    let output = super::super::Linter::lint(&parsed.arena, source, super::super::LintOptions::default());
    let diagnostics = output.diagnostics.into_iter().filter(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-try-capture")).collect::<Vec<_>>();
    assert_eq!(diagnostics.len(), 1);
    let fixed = apply(&diagnostics, source);
    assert!(fixed.contains("let port = try { \"7\".parse_int()? } ?? 8080"));
    assert!(migration(&fixed).is_empty());
}

#[test]
fn local_capture_helpers_decline_bodies_beyond_the_bounded_expression_walk() {
    let expression = format!("\"7\"{}.parse_int()?", ".trim()".repeat(128));
    let source = format!("proc read_port() [error] -> Result[Int] {{ {expression} }}\nlet port = read_port() ?? 8080\n");
    assert!(migration(&source).is_empty());
}
