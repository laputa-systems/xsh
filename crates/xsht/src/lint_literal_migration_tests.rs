use super::{LintOptions, Linter};
use xsh::diagnostic::DiagnosticCode;
use xsh::frontend::check::{Checker, LiteralConstant};
use xsh::frontend::source::SourceId;
use xsh::frontend::syntax::arena::{ArenaExprOrRun, ArenaStmtKind};
use xsh::frontend::syntax::parser::Parser;

#[test]
fn prepared_path_literals_and_exported_data_preserve_comments_and_converge() {
    for source in [
        "# The path remains relative.\nlet destination = p\"../out/非 ascii\" # retained\n",
        "export   let destination: Path = p\"out\" # retained\n",
        "export\tlet version = 1 # retained\n",
        "let locations = {root: p\"out\", search: [p\".\", p\"lib\"]}\n",
    ] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let values = |program: &xsh::frontend::syntax::arena::ArenaProgram| {
            program
                .arena
                .stmt_ids(program.statements)
                .filter_map(|statement| {
                    let mut kind = program.arena.stmt(statement).kind;
                    if let ArenaStmtKind::Export(inner) = kind {
                        kind = program.arena.stmt(inner).kind;
                    }
                    let (ArenaStmtKind::Let {
                        initializer: ArenaExprOrRun::Expr(value),
                        ..
                    }
                    | ArenaStmtKind::Const {
                        initializer: ArenaExprOrRun::Expr(value),
                        ..
                    }) = kind
                    else {
                        return None;
                    };
                    LiteralConstant::analyze(&program.arena, value, &Default::default())
                })
                .collect::<Vec<_>>()
        };
        let before = values(&parsed.arena);
        assert_eq!(before.len(), 1);
        let output = Linter::lint(
            &parsed.arena,
            source,
            LintOptions {
                expr_types: checked.expr_types,
                function_effect_facts_checked: true,
                ..LintOptions::default()
            },
        );
        let fixes = output
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferConst))
            .flat_map(|diagnostic| &diagnostic.fix_hints)
            .collect::<Vec<_>>();
        assert_eq!(fixes.len(), 1, "{source}");
        let fix = fixes[0];
        assert_eq!(fix.replacement.as_deref(), Some("const"));
        let mut fixed = source.to_owned();
        fixed.replace_range(fix.span.unwrap().range(), "const");
        assert_eq!(fixed, source.replacen("let ", "const ", 1));
        let reparsed = Parser::parse_source_arena_only(SourceId::new(0), &fixed);
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
        assert_eq!(before, values(&reparsed.arena));
        let second = Linter::lint(
            &reparsed.arena,
            &fixed,
            LintOptions {
                expr_types: rechecked.expr_types,
                function_effect_facts_checked: true,
                ..LintOptions::default()
            },
        );
        assert!(
            !second
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferConst))
        );
    }
}

#[test]
fn literal_migrations_retain_runtime_initialization_and_computed_arguments() {
    let source = "let root = Path(\"out\")\nlet copied = root\nlet joined = fp\"{root}/file\"\nproc paths() -> Path { let local = p\"out\"; local }\nproc delays() -> List[Duration] {\n  var values = [1s]\n  values = values.push(time.millis(2))\n  values\n}\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let output = Linter::lint(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            function_effect_facts_checked: true,
            ..LintOptions::default()
        },
    );
    assert!(
        !output
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferConst))
    );
    assert!(!output.diagnostics.iter().any(|diagnostic| diagnostic.code
        == Some(DiagnosticCode::LintPreferListCompoundAssignment)
        && !diagnostic.fix_hints.is_empty()));
}

#[test]
fn list_duration_literal_updates_preserve_the_checked_element_domain() {
    let source = "proc delays() -> List[Duration] {\n  var values = [1s]\n  values = values.push(2s)\n  values\n}\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let output = Linter::lint(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            function_effect_facts_checked: true,
            ..LintOptions::default()
        },
    );
    let fixes = output
        .diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.code == Some(DiagnosticCode::LintPreferListCompoundAssignment)
        })
        .flat_map(|diagnostic| &diagnostic.fix_hints)
        .collect::<Vec<_>>();
    assert_eq!(fixes.len(), 1);
    let fix = fixes[0];
    assert_eq!(fix.replacement.as_deref(), Some("values += [2s]"));
    let mut fixed = source.to_owned();
    fixed.replace_range(
        fix.span.unwrap().range(),
        fix.replacement.as_deref().unwrap(),
    );
    let reparsed = Parser::parse_source_arena_only(SourceId::new(0), &fixed);
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
    let second = Linter::lint(
        &reparsed.arena,
        &fixed,
        LintOptions {
            expr_types: rechecked.expr_types,
            function_effect_facts_checked: true,
            ..LintOptions::default()
        },
    );
    assert!(!second.diagnostics.iter().any(
        |diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferListCompoundAssignment)
    ));
}

#[test]
fn literal_origin_byte_suffixes_preserve_bounds_and_converge() {
    let source = "let data = b\"a\\0\\xffbcd\"\nlet copied = data\nlet suffix = data.slice(3, data.len() - 3) # retained\nlet empty = copied.slice(6, length: copied.len() - 6)\nlet rest = copied.slice(2)\nlet bounded = data.slice(1, 2)\nlet whole = copied.slice(0, copied.len() - 0)\nlet saturated = data.slice(1, 9223372036854775807)\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let output = Linter::lint(
        &parsed.arena,
        source,
        LintOptions {
            expr_types: checked.expr_types,
            function_effect_facts_checked: true,
            ..LintOptions::default()
        },
    );
    let fixes = output
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferSlice))
        .flat_map(|diagnostic| &diagnostic.fix_hints)
        .collect::<Vec<_>>();
    assert_eq!(fixes.len(), 6);
    let mut fixed = source.to_owned();
    for fix in fixes.iter().rev() {
        fixed.replace_range(
            fix.span.unwrap().range(),
            fix.replacement.as_deref().unwrap(),
        );
    }
    assert!(fixed.contains("data[3..] # retained"));
    assert!(fixed.contains("copied[6..]"));
    assert!(fixed.contains("copied[2..]"));
    assert!(fixed.contains("data[1..3]"));
    assert!(fixed.contains("copied[0..]"));
    assert!(fixed.contains("data[1..6]"));
    let reparsed = Parser::parse_source_arena_only(SourceId::new(0), &fixed);
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
    let second = Linter::lint(
        &reparsed.arena,
        &fixed,
        LintOptions {
            expr_types: rechecked.expr_types,
            function_effect_facts_checked: true,
            ..LintOptions::default()
        },
    );
    assert!(
        !second
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferSlice))
    );
}

#[test]
fn byte_suffix_migration_retains_unknown_mutable_and_distinct_receivers() {
    for source in [
        "pure suffix(data: Bytes) -> Bytes { data.slice(1, data.len() - 1) }\n",
        "var data = b\"abc\"\nlet suffix = data.slice(1, data.len() - 1)\n",
        "let data = b\"abc\"\nlet suffix = data.slice(4, data.len() - 4)\n",
        "let data = b\"abc\"\nlet other = b\"abcd\"\nlet suffix = data.slice(1, other.len() - 1)\n",
        "let data = b\"abc\"\nlet suffix = data.slice(1, # retain bounds explanation\n  data.len() - 1)\n",
        "let data = b\"abc\"\nlet suffix = data.slice(1, -1)\n",
        "pure produce() -> Bytes { b\"abc\" }\nlet suffix = produce().slice(1, produce().len() - 1)\n",
    ] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let output = Linter::lint(
            &parsed.arena,
            source,
            LintOptions {
                expr_types: checked.expr_types,
                function_effect_facts_checked: true,
                ..LintOptions::default()
            },
        );
        let slices = output
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferSlice))
            .collect::<Vec<_>>();
        assert_eq!(slices.len(), 1, "{source}");
        assert!(slices[0].fix_hints.is_empty(), "{source}");
    }
}

#[cfg(feature = "native-tests")]
#[test]
fn path_literal_promotion_prepares_each_changed_source_without_extra_lint_checks() {
    xsh::frontend::stdlib_preparation::reset();
    let mut source =
        "print tui.red()\nlet target_path = Path(\"/srv/xsh\")\nprint target_path\n".to_owned();
    for (round, code) in [
        Some("lint.path-constructor"),
        Some("lint.prefer-const"),
        None,
    ]
    .into_iter()
    .enumerate()
    {
        let loaded = xsh::frontend::load::parse_load_check_text(
            "fixture.xsh",
            source.clone(),
            Vec::new(),
            Default::default(),
        );
        assert!(
            loaded.parsed.diagnostics.is_empty(),
            "{:?}",
            loaded.parsed.diagnostics
        );
        let checked = loaded.checked.expect("a clean parse is checked");
        assert_eq!(
            checked.diagnostics.len(),
            usize::from(round == 0),
            "{:?}",
            checked.diagnostics
        );
        if round == 0 {
            assert_eq!(
                checked.diagnostics[0].code,
                Some(DiagnosticCode::CheckBarePrintIdent)
            );
        }
        let mut edits = checked
            .diagnostics
            .iter()
            .flat_map(|diagnostic| &diagnostic.fix_hints)
            .map(|fix| {
                (
                    fix.span.unwrap(),
                    fix.replacement.as_deref().unwrap().to_owned(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            xsh::frontend::stdlib_preparation::parsed_modules(),
            round + 1,
            "each changed source prepares its standard module once"
        );
        let output = Linter::lint(
            &loaded.parsed.arena,
            &source,
            LintOptions {
                expr_types: checked.expr_types,
                function_effect_facts: checked.function_effect_facts,
                function_effect_facts_checked: true,
                ..LintOptions::default()
            },
        );
        assert_eq!(
            xsh::frontend::stdlib_preparation::parsed_modules(),
            round + 1,
            "literal rules do not prepare the already checked source again"
        );
        let fixes = output
            .diagnostics
            .iter()
            .filter(|diagnostic| {
                matches!(
                    diagnostic.code,
                    Some(DiagnosticCode::LintPathConstructor | DiagnosticCode::LintPreferConst)
                )
            })
            .collect::<Vec<_>>();
        if let Some(code) = code {
            assert_eq!(fixes.len(), 1);
            assert_eq!(fixes[0].code.map(DiagnosticCode::name), Some(code));
            let [fix] = fixes[0].fix_hints.as_slice() else {
                panic!("one source edit exposes the next rule");
            };
            edits.push((
                fix.span.unwrap(),
                fix.replacement.as_deref().unwrap().to_owned(),
            ));
            edits.sort_by_key(|(span, _)| span.start());
            for (span, replacement) in edits.into_iter().rev() {
                source.replace_range(span.range(), &replacement);
            }
        } else {
            assert!(fixes.is_empty(), "the const Path source has converged");
        }
    }
    assert_eq!(
        source,
        "print tui.red()\nconst target_path = p\"/srv/xsh\"\nprint $target_path\n"
    );
}
