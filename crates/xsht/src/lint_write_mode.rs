use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use xsh::frontend::check::Type;
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{
    ArenaCallArgKind, ArenaExprKind, ArenaFmtPart, ArenaStmtKind, AstArena, ExprId, StmtId,
};

/// A write directly followed by a `chmod` of the same path:
///
/// ```text
/// key.write(secret)?
/// key.chmod(0o600)?
/// ```
///
/// is `key.write(secret, mode: 0o600)?`, which sets the bits before any of
/// the data is in the file, so the data is never readable under the mode a
/// plain write leaves.
///
/// When both calls succeed the file ends with the same contents and bits. The
/// rewrite is offered only where nothing else can tell the two apart:
///
/// - the statements are adjacent and handle failure the same way, so no
///   other code runs between them and a failure leaves the same way;
/// - the path is a binding, a field path, or a literal, written the same in
///   both, so reading it once yields the same file;
/// - the mode is a literal in range, a binding, or a field path, so reading
///   it before the write instead of after yields the same value.
///
/// A failure to set the bits is still reported as `fs-chmod`, but it now
/// happens before the write: the pair left the new contents in place, and
/// the merged call leaves the file as it was.
pub(super) fn lint_write_then_chmod(linter: &mut super::Linter<'_>, stmts: &[StmtId]) {
    for pair in stmts.windows(2) {
        let Some(diagnostic) = write_then_chmod(linter, pair[0], pair[1]) else {
            continue;
        };
        linter.diagnostics.push(diagnostic);
    }
}

/// `PATH.NAME(args)` as a statement.
struct PathCall {
    path: ExprId,
    /// The arguments, all positional.
    args: Vec<ExprId>,
    /// Whether the statement is the call under `?`.
    propagated: bool,
}

fn write_then_chmod(
    linter: &super::Linter<'_>,
    first: StmtId,
    second: StmtId,
) -> Option<Diagnostic> {
    let arena = linter.arena;
    let source = linter.source;
    let write = path_call(linter, first, "write")?;
    let chmod = path_call(linter, second, "chmod")?;
    let ([data], [mode]) = (write.args.as_slice(), chmod.args.as_slice()) else {
        return None;
    };
    if write.propagated != chmod.propagated
        || !reads_the_same_each_time(arena, write.path)
        || source.get(arena.expr(write.path).span.range())
            != source.get(arena.expr(chmod.path).span.range())
    {
        return None;
    }
    let in_range = match arena.expr(*mode).kind {
        ArenaExprKind::Int(literal) => arena
            .int_literal(literal)
            .value()
            .is_some_and(|bits| (0..=0o7777).contains(&bits)),
        ArenaExprKind::Ident(_) | ArenaExprKind::Field { .. } => {
            reads_the_same_each_time(arena, *mode)
        }
        _ => false,
    };
    if !in_range {
        return None;
    }

    let first = arena.stmt(first).span;
    let second = arena.stmt(second).span;
    let pair = Span::new(first.source_id, first.start(), second.end());
    let mut diagnostic = Diagnostic::warning("a file is written and then given its mode")
        .with_code(DiagnosticCode::LintPreferWriteMode)
        .with_label(Label::secondary(
            pair,
            "`write(..., mode: M)` sets the mode before any data is in the file",
        ));
    // The mode goes after the data, inside the write as it is written. A
    // comment in the text that is dropped or shifted would be lost.
    let data_end = arena.expr(*data).span.end();
    let mode_text = source.get(arena.expr(*mode).span.range())?;
    let head = source.get(first.start()..data_end)?;
    let tail = source.get(data_end..first.end())?;
    let dropped = source.get(first.end()..second.end())?;
    if !tail.contains('#') && !dropped.contains('#') {
        diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
            pair,
            "write the file with its mode",
            format!("{head}, mode: {mode_text}{tail}"),
        ));
    }
    Some(diagnostic)
}

fn path_call(linter: &super::Linter<'_>, stmt: StmtId, function: &str) -> Option<PathCall> {
    let arena = linter.arena;
    let ArenaStmtKind::Expr(statement) = arena.stmt(stmt).kind else {
        return None;
    };
    let (call, propagated) = match arena.expr(statement).kind {
        ArenaExprKind::Try(inner) => (inner, true),
        _ => (statement, false),
    };
    let ArenaExprKind::Call { callee, args } = arena.expr(call).kind else {
        return None;
    };
    let ArenaExprKind::Field { base, name } = arena.expr(callee).kind else {
        return None;
    };
    if name != function {
        return None;
    }
    let positional = arena
        .call_args(args)
        .iter()
        .map(|argument| match argument.kind {
            ArenaCallArgKind::Positional(value) => Some(value),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    if linter.expr_types.get(&arena.expr(base).span) != Some(&Type::Path) {
        return None;
    }
    Some(PathCall {
        path: base,
        args: positional,
        propagated,
    })
}

/// A binding, a field path, a literal, or an interpolation of those: reading
/// it has no effect, and two adjacent reads yield the same value.
fn reads_the_same_each_time(arena: &AstArena, expr: ExprId) -> bool {
    match arena.expr(expr).kind {
        ArenaExprKind::Ident(_) | ArenaExprKind::Str(_) | ArenaExprKind::PathStr(_) => true,
        ArenaExprKind::Field { base, .. } => reads_the_same_each_time(arena, base),
        ArenaExprKind::FmtString(parts) | ArenaExprKind::PathFmtString(parts) => {
            arena.fmt_parts(parts).all(|part| match part {
                ArenaFmtPart::Text(_) => true,
                ArenaFmtPart::Expr(value, _) => reads_the_same_each_time(arena, value),
            })
        }
        _ => false,
    }
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
            only: Some(vec![DiagnosticCode::LintPreferWriteMode]),
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

    #[test]
    fn a_write_and_its_chmod_merge_into_the_write() {
        let source = "type Service = {path: Path, mode: Int}\n\nproc install(key: Path, unit: Service, root: Path, secret: Str, bits: Int) [fs, error] {\n  key.write(secret)?\n  key.chmod(0o600)?\n  unit.path.write(b\"unit\")?\n  unit.path.chmod(unit.mode)?\n  fp\"{root}/run\".write(secret)?\n  fp\"{root}/run\".chmod(bits)?\n  p\"/tmp/plain\".write(secret)\n  p\"/tmp/plain\".chmod(0o644)\n}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 4, "{diagnostics:?}");
        let fixed = apply(&diagnostics, source);
        assert_eq!(
            fixed,
            "type Service = {path: Path, mode: Int}\n\nproc install(key: Path, unit: Service, root: Path, secret: Str, bits: Int) [fs, error] {\n  key.write(secret, mode: 0o600)?\n  unit.path.write(b\"unit\", mode: unit.mode)?\n  fp\"{root}/run\".write(secret, mode: bits)?\n  p\"/tmp/plain\".write(secret, mode: 0o644)\n}\n"
        );
        assert!(lint(&fixed).is_empty());
    }

    // Another path, statements that fail differently, a statement between
    // them, a mode read from the file just written, a mode out of range, and
    // a write that already has a mode are not this pair.
    #[test]
    fn other_pairs_are_left_alone() {
        let source = "proc install(key: Path, other: Path, secret: Str) [fs, error] {\n  key.write(secret)?\n  other.chmod(0o600)?\n  key.write(secret)?\n  let _ = key.chmod(0o600)\n  key.write(secret)?\n  other.write(secret)?\n  key.chmod(0o600)?\n  key.write(secret)?\n  key.chmod(key.metadata()?.mode % 512)?\n  key.write(secret)?\n  key.chmod(0o10000)?\n  key.write(secret, mode: 0o644)?\n  key.chmod(0o600)?\n  key.write_atomic(bytes.from_text(secret))?\n  key.chmod(0o600)?\n}\n";
        assert!(lint(source).is_empty(), "{:?}", lint(source));
    }

    // A comment between or inside the statements would be lost, so the pair
    // is reported without a rewrite.
    #[test]
    fn a_comment_in_the_pair_blocks_the_rewrite() {
        let source = "proc install(key: Path, secret: Str) [fs, error] {\n  key.write(secret)?\n  # Only the owner may read it.\n  key.chmod(0o600)?\n}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert!(diagnostics[0].fix_hints.is_empty());
    }
}
