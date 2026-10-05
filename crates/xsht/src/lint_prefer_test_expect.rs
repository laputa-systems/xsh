use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use xsh::frontend::source::{SourceId, Span};
use xsh::frontend::syntax::arena::{
    ArenaBindingTargetKind, ArenaCallArgKind, ArenaExprKind, ArenaExprOrRun, ArenaStmtKind,
    AstArena, ExprId, StmtId,
};
use xsh::frontend::syntax::node::BinaryOp;
use xsh::frontend::syntax::token::TokenTag;

use super::prefer_tempdir::text_end_of;

/// A script run bound to a name and then asserted on, field by field:
///
/// ```text
/// let output = test.run_script(ctx, source)?
/// assert output.status == 1, output.stderr
/// assert "unknown option" in output.stderr, output.stderr
/// ```
///
/// is `test.expect(ctx, source, status: 1, stderr: ["unknown option"])?`,
/// which runs the script the same way, passes exactly when every one of the
/// assertions would, and reports a failure with the whole output.
///
/// The assertions taken are the ones directly after the run that say only
/// what `test.expect` says: the exit status (`NAME.success`, or `NAME.status`
/// compared with an integer literal) and fragments of `NAME.stderr` and
/// `NAME.stdout`, each with no message or with one of those two streams as
/// its message. The run is reported only when one of them gives the status,
/// which `test.expect` requires. Later statements are left as they are; when
/// any of them reads the binding, the rewrite keeps it, and otherwise the
/// call stands as a statement, which drops the record.
///
/// A fragment is a literal, a binding, or a field path, so reading it before
/// the script runs instead of after yields the same value. A comment among
/// the statements ends the run of assertions before it, and a comment inside
/// the call leaves the site alone, because the rewrite would lose it.
pub(super) fn lint_script_runs(linter: &mut super::Linter<'_>, stmts: &[StmtId]) {
    // A local named `test` makes the call something else.
    if linter.scopes.iter().any(|scope| scope.contains_key("test")) {
        return;
    }
    for index in 0..stmts.len() {
        if let Some(diagnostic) = script_run(linter, stmts, index) {
            linter.diagnostics.push(diagnostic);
        }
    }
}

/// What the assertions after a run require of it.
#[derive(Default)]
struct Expectation {
    /// The source text of the required exit status.
    status: Option<String>,
    stderr: Vec<String>,
    stdout: Vec<String>,
}

enum Requirement {
    Status(String),
    Stderr(String),
    Stdout(String),
}

/// The parameters of `test.run_script` after the context and the source, in
/// order. `test.expect` takes them by the same names.
const RUN_PARAMETERS: [&str; 4] = ["args", "env", "stdin", "name"];

fn script_run(linter: &super::Linter<'_>, stmts: &[StmtId], index: usize) -> Option<Diagnostic> {
    let arena = linter.arena;
    let source = linter.source;
    let ArenaStmtKind::Let {
        target,
        ty: None,
        initializer: ArenaExprOrRun::Expr(initializer),
    } = arena.stmt(stmts[index]).kind
    else {
        return None;
    };
    let ArenaBindingTargetKind::Name(name) = arena.binding_target(target).kind else {
        return None;
    };
    let name = name.as_str();
    let name: &str = &name;
    let ArenaExprKind::Try(call) = arena.expr(initializer).kind else {
        return None;
    };
    let ArenaExprKind::Call { callee, args } = arena.expr(call).kind else {
        return None;
    };
    let ArenaExprKind::Field { base, name: function } = arena.expr(callee).kind else {
        return None;
    };
    if function != "run_script"
        || !matches!(arena.expr(base).kind, ArenaExprKind::Ident(module) if module == "test")
    {
        return None;
    }
    let text = |expr: ExprId| source.get(arena.expr(expr).span.range());
    let mut arguments = Vec::new();
    for (position, argument) in arena.call_args(args).iter().enumerate() {
        arguments.push(match argument.kind {
            ArenaCallArgKind::Positional(value) if position < 2 => text(value)?.to_string(),
            ArenaCallArgKind::Positional(value) => {
                format!("{}: {}", RUN_PARAMETERS.get(position - 2)?, text(value)?)
            }
            ArenaCallArgKind::Named { name, value, .. } if position >= 2 => {
                format!("{}: {}", name.as_str(), text(value)?)
            }
            _ => return None,
        });
    }
    if arguments.len() < 2 {
        return None;
    }

    let binding = arena.stmt(stmts[index]).span;
    let line_end = |offset: usize| source[offset..].find('\n').map_or(source.len(), |at| offset + at);
    let ends_its_line =
        |end: usize| source.get(end..line_end(end)).is_some_and(|rest| rest.trim().is_empty());
    let mut end = text_end_of(source, binding)?;
    if !ends_its_line(end) || super::span_may_contain_comment(source, binding) {
        return None;
    }
    let mut expectation = Expectation::default();
    let mut taken = 0;
    for stmt in &stmts[index + 1..] {
        let span = arena.stmt(*stmt).span;
        let Some(requirement) = requirement(arena, source, *stmt, name) else {
            break;
        };
        let stmt_end = text_end_of(source, span)?;
        // Only blank space may separate the statements that are replaced.
        if !source.get(end..span.start())?.trim().is_empty() || !ends_its_line(stmt_end) {
            break;
        }
        match requirement {
            Requirement::Status(status) => {
                if expectation.status.as_ref().is_some_and(|known| *known != status) {
                    break;
                }
                expectation.status = Some(status);
            }
            Requirement::Stderr(fragment) => expectation.stderr.push(fragment),
            Requirement::Stdout(fragment) => expectation.stdout.push(fragment),
        }
        end = stmt_end;
        taken += 1;
    }
    let status = expectation.status?;

    let replaced = Span::new(binding.source_id, binding.start(), end);
    let rest = &stmts[index + 1 + taken..];
    let read_later = match (rest.first(), rest.last()) {
        (Some(first), Some(last)) => may_read(
            source.get(arena.stmt(*first).span.start()..arena.stmt(*last).span.end())?,
            name,
        ),
        _ => false,
    };
    // A record nothing reads is dropped by the statement itself, which
    // `test.expect` is registered to allow. As the last statement of its
    // block the call could become the block's value, which only the checker
    // can rule out, so there the record is discarded by a binding and
    // `lint.redundant-discard` removes the binding where it is safe.
    let binding = if read_later {
        format!("let {name} = ")
    } else if rest.is_empty() {
        "let _ = ".to_string()
    } else {
        String::new()
    };
    let mut call = format!(
        "{binding}test.expect({}, {}, status: {status}",
        arguments[0], arguments[1],
    );
    for (stream, fragments) in [("stderr", &expectation.stderr), ("stdout", &expectation.stdout)] {
        if !fragments.is_empty() {
            call.push_str(&format!(", {stream}: [{}]", fragments.join(", ")));
        }
    }
    for argument in &arguments[2..] {
        call.push_str(", ");
        call.push_str(argument);
    }
    call.push_str(")?");
    let replacement = formatted_in_place(source, replaced, &call)?;
    Some(
        Diagnostic::warning("a script run is followed by assertions that `test.expect` states")
            .with_code(DiagnosticCode::LintPreferTestExpect)
            .with_label(Label::secondary(
                replaced,
                "`test.expect` takes the status and the output fragments, and reports the whole output when one is wrong",
            ))
            .with_fix_hint(FixHint::replacement(
                replaced,
                "run the script with `test.expect`",
                replacement,
            )),
    )
}

/// What `stmt` asserts about the script output bound to `output`, when
/// `test.expect` can say the same.
fn requirement(arena: &AstArena, source: &str, stmt: StmtId, output: &str) -> Option<Requirement> {
    let ArenaStmtKind::Assert { condition, message } = arena.stmt(stmt).kind else {
        return None;
    };
    let field = |expr: ExprId| match arena.expr(expr).kind {
        ArenaExprKind::Field { base, name }
            if matches!(arena.expr(base).kind, ArenaExprKind::Ident(local) if local == output) =>
        {
            Some(name)
        }
        _ => None,
    };
    let is_field = |expr: ExprId, name: &str| field(expr).is_some_and(|found| found == name);
    // The whole output is in the failure `test.expect` reports, so a message
    // that is one of its streams adds nothing; any other message would be lost.
    if let Some(message) = message
        && !is_field(message, "stderr")
        && !is_field(message, "stdout")
    {
        return None;
    }
    if is_field(condition, "success") {
        return Some(Requirement::Status("0".to_string()));
    }
    let ArenaExprKind::Binary { op, left, right } = arena.expr(condition).kind else {
        return None;
    };
    match op {
        BinaryOp::Eq if is_field(left, "success") => {
            matches!(arena.expr(right).kind, ArenaExprKind::Bool(true))
                .then(|| Requirement::Status("0".to_string()))
        }
        BinaryOp::Eq if is_field(left, "status") => {
            let ArenaExprKind::Int(_) = arena.expr(right).kind else {
                return None;
            };
            Some(Requirement::Status(
                source.get(arena.expr(right).span.range())?.to_string(),
            ))
        }
        BinaryOp::In if reads_the_same_before_the_run(arena, left, output) => {
            let fragment = source.get(arena.expr(left).span.range())?.to_string();
            if is_field(right, "stderr") {
                Some(Requirement::Stderr(fragment))
            } else if is_field(right, "stdout") {
                Some(Requirement::Stdout(fragment))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// A string literal, or a binding or field path that is not the script
/// output: reading it has no effect and does not depend on the run.
fn reads_the_same_before_the_run(arena: &AstArena, expr: ExprId, output: &str) -> bool {
    match arena.expr(expr).kind {
        ArenaExprKind::Str(_) => true,
        ArenaExprKind::Ident(name) => name != output,
        ArenaExprKind::Field { base, .. } => reads_the_same_before_the_run(arena, base, output),
        _ => false,
    }
}

/// Whether the statements in `text` may read the binding `name`: some token
/// other than a comment spells it. A string literal that interpolates
/// nothing only mentions the name, and text that does not lex is taken to
/// read it.
fn may_read(text: &str, name: &str) -> bool {
    let lexed = xsh::frontend::syntax::lexer::Lexer::new(SourceId::new(0), text).lex_compact();
    if !lexed.diagnostics.is_empty() {
        return super::mentions_identifier(text, name);
    }
    let tokens = &lexed.token_table;
    (0..tokens.len()).any(|index| {
        let Some(tag) = tokens.tag_at(index) else {
            return true;
        };
        let (Some(start), Some(end)) = (tokens.start_at(index), tokens.end_at(index, text)) else {
            return true;
        };
        let Some(token) = text.get(start..end) else {
            return true;
        };
        let inert = match tag {
            TokenTag::Comment => true,
            TokenTag::String
            | TokenTag::PathString
            | TokenTag::GlobString
            | TokenTag::EnvString
            | TokenTag::FmtString
            | TokenTag::PathFmtString => !token.contains(['$', '{']),
            _ => false,
        };
        !inert && super::mentions_identifier(token, name)
    })
}

/// `statement` as the formatter prints it where `replaced` starts, without
/// the indentation of its first line.
///
/// The formatter breaks a line by its width, which depends on how deep the
/// statement is nested, so the statement is formatted inside as many blocks
/// as its indentation says it has around it.
pub(super) fn formatted_in_place(source: &str, replaced: Span, statement: &str) -> Option<String> {
    let line_start = source[..replaced.start()].rfind('\n').map_or(0, |at| at + 1);
    let indent = source.get(line_start..replaced.start())?;
    if !indent.chars().all(|ch| ch == ' ') || indent.len() % 2 != 0 {
        return None;
    }
    let depth = indent.len() / 2;
    let mut opening = String::new();
    let mut closing = String::new();
    for level in 0..depth {
        opening.push_str(&format!("{}if true {{\n", "  ".repeat(level)));
        closing.insert_str(0, &format!("{}}}\n", "  ".repeat(level)));
    }
    let fragment = format!("{opening}{indent}{statement}\n{closing}");
    let formatted =
        super::super::format::Formatter::new().format_source(replaced.source_id, &fragment);
    if !formatted.diagnostics.is_empty() {
        return None;
    }
    let body = formatted
        .formatted
        .strip_prefix(opening.as_str())?
        .strip_suffix(closing.as_str())?;
    Some(body.strip_prefix(indent)?.strip_suffix('\n')?.to_string())
}

#[cfg(test)]
mod tests {
    use super::super::{LintOptions, Linter};
    use xsh::diagnostic::{Diagnostic, DiagnosticCode};
    use xsh::frontend::check::Checker;
    use xsh::frontend::source::SourceId;
    use xsh::frontend::syntax::parser::Parser;

    // The lint runs from the linter's statement traversal, so the tests
    // drive the whole linter restricted to this code.
    fn lint(source: &str) -> Vec<Diagnostic> {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let options = LintOptions {
            expr_types: checked.expr_types,
            only: Some(vec![DiagnosticCode::LintPreferTestExpect]),
            ..LintOptions::default()
        };
        Linter::lint(&parsed.arena, source, options).diagnostics
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
                fix.replacement.as_deref().unwrap_or_default(),
            );
        }
        fixed
    }

    /// The fixed source lints clean, checks, and is what the formatter prints.
    fn fixed(source: &str, sites: usize) -> String {
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), sites, "{diagnostics:?}");
        let fixed = apply(&diagnostics, source);
        assert!(lint(&fixed).is_empty(), "{fixed}");
        let formatted =
            super::super::super::format::Formatter::new().format_source(SourceId::new(0), &fixed);
        assert!(formatted.diagnostics.is_empty(), "{:?}", formatted.diagnostics);
        assert_eq!(formatted.formatted, fixed);
        fixed
    }

    #[test]
    fn status_and_fragment_assertions_become_one_expectation() {
        let source = "test rejects { |ctx|\n  let output = test.run_script(ctx, \"exit 1\")?\n  assert output.status == 1, output.stderr\n  assert \"first\" in output.stderr, output.stderr\n  assert \"second\" in output.stderr\n  assert \"printed\" in output.stdout, output.stdout\n}\n\ntest accepts { |ctx|\n  let source = \"print 1\"\n  let accepted = test.run_script(ctx, source, [\"a\"], stdin: b\"in\")?\n  assert accepted.success, accepted.stderr\n\n  let again = test.run_script(ctx, source)?\n  assert again.success == true\n  assert again.status == 0\n}\n";
        assert_eq!(
            fixed(source, 3),
            "test rejects { |ctx|\n  let _ = test.expect(ctx, \"exit 1\", status: 1, stderr: [\"first\", \"second\"], stdout: [\"printed\"])?\n}\n\ntest accepts { |ctx|\n  let source = \"print 1\"\n  test.expect(ctx, source, status: 0, args: [\"a\"], stdin: b\"in\")?\n\n  let _ = test.expect(ctx, source, status: 0)?\n}\n"
        );
    }

    // The binding stays when a later statement reads it, and an assertion
    // `test.expect` cannot state ends the run of assertions that are taken.
    #[test]
    fn a_binding_that_is_read_later_is_kept() {
        let source = "test rejects { |ctx|\n  let output = test.run_script(ctx, \"exit 1\")?\n  assert output.status == 1\n  assert \"gone\" not in output.stderr\n  assert \"late\" in output.stderr\n}\n";
        assert_eq!(
            fixed(source, 1),
            "test rejects { |ctx|\n  let output = test.expect(ctx, \"exit 1\", status: 1)?\n  assert \"gone\" not in output.stderr\n  assert \"late\" in output.stderr\n}\n"
        );
    }

    // A name that later text only mentions, in a comment or in a string that
    // interpolates nothing, is not read there.
    #[test]
    fn a_later_mention_in_a_comment_or_plain_string_is_not_a_read() {
        let source = "test rejects { |ctx|\n  let cross = test.run_script(ctx, \"exit 1\")?\n  assert cross.status == 1\n  # cross is done.\n  assert ctx.name != \"\", \"cross runs fail early\"\n  let kept = test.run_script(ctx, \"exit 1\")?\n  assert kept.status == 1\n  assert ctx.name != \"\", f\"{kept.stderr}\"\n}\n";
        assert_eq!(
            fixed(source, 2),
            "test rejects { |ctx|\n  test.expect(ctx, \"exit 1\", status: 1)?\n  # cross is done.\n  assert ctx.name != \"\", \"cross runs fail early\"\n  let kept = test.expect(ctx, \"exit 1\", status: 1)?\n  assert ctx.name != \"\", f\"{kept.stderr}\"\n}\n"
        );
    }

    // A call too long for one line is broken the way the formatter breaks
    // it, at the depth where it stands.
    #[test]
    fn a_long_or_multi_line_call_is_laid_out_by_the_formatter() {
        let source = "test rejects { |ctx|\n  if ctx.name != \"\" {\n    let output = test.run_script(\n      ctx,\n      r\"\"\"\nprint 1\nexit 1\n\"\"\",\n      env: {XSH_PREFER_TEST_EXPECT: \"1\"},\n    )?\n    assert output.status == 1, output.stderr\n    assert \"the first of the fragments that the failing script must print\" in output.stderr, output.stderr\n    assert \"the second of the fragments that the failing script must print\" in output.stderr, output.stderr\n  }\n}\n";
        assert_eq!(
            fixed(source, 1),
            "test rejects { |ctx|\n  if ctx.name != \"\" {\n    let _ = test.expect(\n      ctx,\n      r\"\"\"\nprint 1\nexit 1\n\"\"\",\n      status: 1,\n      stderr: [\n        \"the first of the fragments that the failing script must print\",\n        \"the second of the fragments that the failing script must print\",\n      ],\n      env: {XSH_PREFER_TEST_EXPECT: \"1\"},\n    )?\n  }\n}\n"
        );
    }

    // No status, a status that is not one value, two statuses, a message the
    // rewrite would lose, a fragment computed from the output or by a call,
    // a typed binding, and a run that is not propagated are left alone.
    #[test]
    fn runs_without_one_exact_status_are_left_alone() {
        let source = "pure label() -> Str {\n  \"x\"\n}\n\ntest rejects { |ctx|\n  let source = \"exit 1\"\n  let a = test.run_script(ctx, source)?\n  assert \"x\" in a.stderr\n  let b = test.run_script(ctx, source)?\n  assert ! b.success, b.stderr\n  let c = test.run_script(ctx, source)?\n  assert c.status != 0\n  let d = test.run_script(ctx, source)?\n  assert d.status == 1, source\n  let e = test.run_script(ctx, source)?\n  assert e.stdout in e.stderr\n  assert label() in e.stderr\n  let f = test.run_script(ctx, source)\n  let _ = f\n  let g = test.run_script(ctx, source)?\n  let unrelated = 1\n  assert g.status == unrelated\n}\n";
        assert!(lint(source).is_empty(), "{:?}", lint(source));
    }

    // A comment before an assertion ends the run there, and a comment inside
    // the call leaves the site alone.
    #[test]
    fn comments_are_never_dropped() {
        let source = "test rejects { |ctx|\n  let output = test.run_script(ctx, \"exit 1\")?\n  assert output.status == 1\n  # The diagnostic names the field.\n  assert \"field\" in output.stderr\n  let other = test.run_script(\n    ctx,\n    # A script that fails.\n    \"exit 1\",\n  )?\n  assert other.status == 1\n}\n";
        assert_eq!(
            fixed(source, 1),
            "test rejects { |ctx|\n  let output = test.expect(ctx, \"exit 1\", status: 1)?\n  # The diagnostic names the field.\n  assert \"field\" in output.stderr\n  let other = test.run_script(\n    ctx,\n    # A script that fails.\n    \"exit 1\",\n  )?\n  assert other.status == 1\n}\n"
        );
    }
}
