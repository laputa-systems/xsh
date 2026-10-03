use super::super::{LintOptions, Linter};
use xsh::frontend::source::SourceId;
use xsh::frontend::syntax::parser::Parser;
use xsh::execution::script::{RunOptions, ScriptOutput, run_script};
use crate::xsht::format::Formatter;

fn observations(source: &str) -> ScriptOutput {
    let directory = tempfile::TempDir::new().unwrap();
    let path = directory.path().join("literal.xsh");
    std::fs::write(&path, source).unwrap();
    run_script(RunOptions { script: path.to_str().unwrap().to_owned(), args: Vec::new(), coverage_trace_dir: None })
}

fn migrate(source: &str) -> String {
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let output = Linter::lint(&parsed.arena, source, LintOptions::default());
    let fixes = output.diagnostics.iter().filter(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-block-string"))
        .flat_map(|diagnostic| &diagnostic.fix_hints).collect::<Vec<_>>();
    assert_eq!(fixes.len(), 1, "expected one proven literal edit: {source}\n{:?}", output.diagnostics);
    let mut fixed = source.to_owned();
    fixed.replace_range(fixes[0].span.unwrap().range(), fixes[0].replacement.as_deref().unwrap());
    let before = observations(source);
    let after = observations(&fixed);
    assert_eq!(before.status, 0, "{}", String::from_utf8_lossy(&before.stderr));
    assert_eq!((after.status, after.stdout, after.stderr), (before.status, before.stdout, before.stderr));
    let reparsed = Parser::parse_source_arena_only(SourceId::new(0), &fixed);
    assert!(reparsed.diagnostics.is_empty(), "{:?}", reparsed.diagnostics);
    let second = Linter::lint(&reparsed.arena, &fixed, LintOptions::default());
    assert!(!second.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-block-string")));
    fixed
}

#[test]
fn formatted_service_block_string_preserves_exact_runtime_bytes() {
    let source = "let name = \"worker\"\nlet executable = \"/usr/bin/service\"\nlet unit = f\"[service]\\nname=$name\\nexec=$executable\"\nprint $unit\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let output = Linter::lint(&parsed.arena, source, LintOptions::default());
    let diagnostic = output.diagnostics.iter().find(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-block-string")).expect("single formatted escaped-newline literal should migrate");
    let fix = diagnostic.fix_hints.first().expect("formatted service literal should have a safe edit");
    assert_eq!(fix.replacement.as_deref(), Some("f\"\"\"\n  [service]\n  name=$name\n  exec=$executable\n  \"\"\""));
    let fixed = migrate(source);
    assert_eq!(observations(&fixed).stdout, b"[service]\nname=worker\nexec=/usr/bin/service\n");
}

#[test]
fn formatted_block_strings_preserve_blank_lines_escapes_and_final_newline() {
    for body in [
        r"\n",
        r"head\n\n  \n$name\nlast",
        r"\n$name\n",
        r"\\n\n\$literal\t$name\0\nend",
        r#"\"quoted\"\n$name\n\u{e9}\r"#,
        r"head\r\n$name\r\nlast\r\n",
        r"head\n${name:>8}\n${false}",
        r#"\"\"\"\n$name"#,
        r###"${if true { r"""nested
  exact""" } else { "other" }}\n$name"###,
    ] {
        let source = format!("let name = \"one\\n  inserted\"\nlet value = f\"{body}\"\nprint $value\n");
        migrate(&source);
    }
}

#[test]
fn formatted_block_strings_preserve_crlf_source_and_tab_margins() {
    let source = "proc value() -> Str {\r\n\tlet name = \"café\"\r\n\tlet result = f\"head\\r\\n$name\\nlast\\n\"\r\n\tresult\r\n}\r\nprint ${value()}\r\n";
    let fixed = migrate(source);
    assert!(fixed.contains("f\"\"\"\r\n\t  head\r\n\t  $name\n\t  last\n\t  \r\n\t  \"\"\""));
    assert_eq!(observations(&fixed).stdout, "head\r\ncafé\nlast\n\n".as_bytes());
}

#[test]
fn formatted_block_strings_preserve_effect_order_and_multiline_interpolation_source() {
    let source = "proc part(label: Str) [io] -> Str { print $label; label + \"\\ninserted\" }\n# This comment remains attached to the literal.\nlet value = f\"${part(\"left\")}\\n${if true {\n  # retain the interpolation comment\n  part(\"right\")\n} else { \"other\" }}\\n${part(\"left\")}\"\nprint $value\n";
    let fixed = migrate(source);
    assert!(fixed.contains("# This comment remains attached to the literal."));
    assert!(fixed.contains("${if true {\n  # retain the interpolation comment\n  part(\"right\")\n} else { \"other\" }}"));
    assert_eq!(observations(&fixed).stdout, b"left\nright\nleft\nleft\ninserted\nright\ninserted\nleft\ninserted\n");
}

#[test]
fn formatted_block_string_library_formatting_preserves_runtime_bytes_and_converges() {
    let source = "let name = \"one\\n  inserted\"\nlet value = f\"head\\r\\n$name\\n\\nlast\\n\"\nprint $value\n";
    let fixed = migrate(source);
    let formatted = Formatter::new().format_source(SourceId::new(0), &fixed);
    assert!(formatted.diagnostics.is_empty(), "{:?}", formatted.diagnostics);
    let before = observations(source);
    let after = observations(&formatted.formatted);
    assert_eq!((after.status, after.stdout, after.stderr), (before.status, before.stdout, before.stderr));
    assert_eq!(formatted.formatted, Formatter::new().format_source(SourceId::new(0), &formatted.formatted).formatted);
}

#[test]
fn formatted_block_string_refusal_preserves_comments_consumers_and_concat_effect_boundaries() {
    for expression in [
        "f\"head\\n$name\" # keep this trailing comment",
        "f\"head\\n$name\".len()",
        "f\"head\\n$name\" + f\"tail\\n$name\"",
    ] {
        let source = format!("let name = \"worker\"\nlet value = {expression}\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let output = Linter::lint(&parsed.arena, &source, LintOptions::default());
        let diagnostics = output.diagnostics.iter().filter(|diagnostic| diagnostic.code.as_deref() == Some("lint.prefer-block-string")).collect::<Vec<_>>();
        assert!(!diagnostics.is_empty());
        assert!(diagnostics.iter().all(|diagnostic| diagnostic.fix_hints.is_empty()), "{source}");
        assert!(diagnostics.iter().all(|diagnostic| !diagnostic.notes.is_empty()));
    }
}
