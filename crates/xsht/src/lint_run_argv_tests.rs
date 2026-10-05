use super::super::{LintOptions, Linter};
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
        standard_call_spans: checked.standard_call_spans,
        ..LintOptions::default()
    };
    Linter::lint(&parsed.arena, source, options)
        .diagnostics
        .into_iter()
        .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferRunArgv))
        .collect()
}

/// The source after applying every fix the lint offers.
fn fixed(source: &str) -> String {
    let mut edits: Vec<(std::ops::Range<usize>, String)> = lint(source)
        .iter()
        .flat_map(|diagnostic| &diagnostic.fix_hints)
        .filter_map(|hint| Some((hint.span?.range(), hint.replacement.clone()?)))
        .collect();
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut text = source.to_owned();
    for (range, replacement) in edits {
        text.replace_range(range, &replacement);
    }
    text
}

const HEAD: &str = "proc launch(argv: List[Str]) [process, error] -> Result[Status, ProcessError] {\n";

#[test]
fn a_rebuilt_vector_that_is_a_whole_value_is_rewritten() {
    for (body, expected) in [
        (
            "  let status = process.run(process.command_argv(argv[0], argv))?\n  Ok(status)\n}\n",
            "  let status = run.status @argv ?\n  Ok(status)\n}\n",
        ),
        (
            "  var status: Status = process.run(process.command_argv(argv[0], argv))?\n  status = process.run(process.command_argv(argv[0], argv))?\n  Ok(status)\n}\n",
            "  var status: Status = run.status @argv ?\n  status = run.status @argv ?\n  Ok(status)\n}\n",
        ),
    ] {
        let source = format!("{HEAD}{body}");
        let expected = format!("{HEAD}{expected}");
        assert_eq!(fixed(&source), expected);
        assert!(lint(&expected).is_empty());
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &expected);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &expected);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    }
}

#[test]
fn a_rebuilt_vector_inside_a_larger_expression_is_reported_without_a_fix() {
    for body in [
        "  Ok(process.run(process.command_argv(argv[0], argv))?)\n}\n",
        "  let both = [process.run(process.command_argv(argv[0], argv))?]\n  Ok(both[0])\n}\n",
    ] {
        let source = format!("{HEAD}{body}");
        let diagnostics = lint(&source);
        assert_eq!(diagnostics.len(), 1, "{source}");
        assert!(diagnostics[0].fix_hints.is_empty(), "{source}");
    }
}

#[test]
fn other_command_plans_are_left_alone() {
    for body in [
        // The executable intentionally differs from `argv[0]`.
        "  let status = process.run(process.command_argv(\"busybox\", argv))?\n  Ok(status)\n}\n",
        "  let status = process.run(process.command_argv(argv[1], argv))?\n  Ok(status)\n}\n",
        // An option changes how the command runs.
        "  let status = process.run(process.command_argv(argv[0], argv, p\"/\"))?\n  Ok(status)\n}\n",
        // The `Result` is a value here, not propagated.
        "  process.run(process.command_argv(argv[0], argv))\n}\n",
        // Another vector supplies the program.
        "  let other = [\"true\"]\n  let status = process.run(process.command_argv(other[0], argv))?\n  Ok(status)\n}\n",
    ] {
        let source = format!("{HEAD}{body}");
        assert!(lint(&source).is_empty(), "{source}");
    }
}
