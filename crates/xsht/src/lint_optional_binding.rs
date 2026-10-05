use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use xsh::frontend::check::Type;
use xsh::frontend::source::Span;
use xsh::frontend::symbols::Name;
use xsh::frontend::syntax::arena::{
    ArenaBindingTargetKind, ArenaExprKind, ArenaExprOrRun, ArenaStmtKind, ArenaTypeExprTag,
    AstArena, ExprId, StmtId,
};
use xsh::frontend::syntax::node::BinaryOp;

/// A null test that leaves the block, directly followed by a binding that
/// names the tested optional again under its narrowed type:
///
/// ```text
/// guard context.executor != null else { return Err(...) }
/// let executor: Executor = context.executor
/// ```
///
/// is `guard let executor = context.executor else { return Err(...) }`.
///
/// The rewrite is offered only where it cannot change behavior. The subject
/// is a binding or a field path, so reading it once instead of twice yields
/// the same value and has no effect; the two statements are adjacent, so
/// nothing runs between the test and the binding; and the failure code moves
/// unchanged into the `else` block, which already left the continuation. An
/// annotation is dropped only when it is the type the binding gets anyway,
/// and is kept on the guard otherwise.
pub(super) fn lint_null_test_then_binding(linter: &mut super::Linter<'_>, stmts: &[StmtId]) {
    for pair in stmts.windows(2) {
        let Some(diagnostic) = null_test_then_binding(linter, pair[0], pair[1]) else {
            continue;
        };
        linter.diagnostics.push(diagnostic);
    }
}

/// How the null test leaves the block.
enum Failure {
    /// `guard SUBJECT != null else { ... }`: the block is reused as written.
    Block(Span),
    /// `return X when SUBJECT == null` (or `break`, `continue`, and the
    /// `unless SUBJECT != null` spelling): the statement becomes the block.
    Statement(Span),
}

fn null_test_then_binding(
    linter: &super::Linter<'_>,
    test: StmtId,
    binding: StmtId,
) -> Option<Diagnostic> {
    let arena = linter.arena;
    let source = linter.source;
    let test = arena.stmt(test);
    let (subject, failure) = match test.kind {
        ArenaStmtKind::BooleanGuard {
            condition,
            else_block,
        } => (
            null_comparison(arena, condition, BinaryOp::Ne)?,
            Failure::Block(arena.span(arena.block(else_block).span)),
        ),
        ArenaStmtKind::GuardedStmt {
            stmt,
            negate,
            condition,
        } => {
            let inner = arena.stmt(stmt);
            if !matches!(
                inner.kind,
                ArenaStmtKind::Return(_) | ArenaStmtKind::Break { .. } | ArenaStmtKind::Continue
            ) {
                return None;
            }
            let selects_null = if negate { BinaryOp::Ne } else { BinaryOp::Eq };
            (
                null_comparison(arena, condition, selects_null)?,
                Failure::Statement(inner.span),
            )
        }
        _ => return None,
    };
    let subject_path = stable_path(arena, subject)?;

    let binding = arena.stmt(binding);
    let ArenaStmtKind::Let {
        target,
        ty: annotation,
        initializer: ArenaExprOrRun::Expr(value),
    } = binding.kind
    else {
        return None;
    };
    let ArenaBindingTargetKind::Name(name) = arena.binding_target(target).kind else {
        return None;
    };
    // `let _ = x` binds nothing, and an optional annotation asks for the
    // optional itself, which a guard would not bind.
    if name == "_"
        || annotation
            .is_some_and(|ty| arena.type_expr_tags[ty.index()] == ArenaTypeExprTag::Optional)
    {
        return None;
    }
    if stable_path(arena, value)? != subject_path {
        return None;
    }
    // The binding must already see the narrowed subject: the guard binds the
    // non-null type, never the optional.
    let bound = linter.expr_types.get(&arena.expr(value).span)?;
    if matches!(
        bound,
        Type::Optional(_) | Type::Any | Type::Unknown | Type::Invalid
    ) {
        return None;
    }

    let subject_text = source.get(arena.expr(subject).span.range())?;
    // An annotation that is the narrowed type restates it. Any other one (a
    // wider type, or one this comparison cannot resolve) still decides the
    // binding's type, so it moves to the guard.
    let kept_annotation = match annotation {
        Some(ty) => {
            let text = source.get(arena.type_expr_span(ty).range())?;
            let restated = bound.to_string() == text
                || linter.annotation_is_needless(&Type::from_arena(arena, ty), value);
            (!restated).then_some(text)
        }
        None => None,
    };
    let head = match kept_annotation {
        Some(annotation) => format!("guard let {name}: {annotation} = {subject_text} else "),
        None => format!("guard let {name} = {subject_text} else "),
    };
    // A statement's span can run past its last token, so the binding ends
    // where its initializer does.
    let span = Span::new(
        test.span.source_id,
        test.span.start(),
        arena.expr(value).span.end(),
    );
    let part = |start: usize, end: usize| Span::new(span.source_id, start, end);
    // Each edit replaces text this rule rebuilds; a comment inside one would
    // be lost. A guard's block is left in place, comments and all.
    let edits = match failure {
        Failure::Block(block) => vec![
            (part(span.start(), block.start()), head),
            (part(block.end(), span.end()), String::new()),
        ],
        Failure::Statement(statement) => {
            let indent = line_indent(source, span.start());
            let exit = source.get(statement.range())?.trim();
            vec![(
                span,
                format!("{head}{{\n{indent}  {exit}\n{indent}}}"),
            )]
        }
    };
    let mut diagnostic = Diagnostic::warning(format!(
        "bind `{name}` with `guard let` instead of testing `{subject_text}` for null and naming it again"
    ))
    .with_code(DiagnosticCode::LintPreferOptionalBinding)
    .with_label(Label::secondary(
        span,
        "`guard let` tests the optional and binds its value in one step",
    ));
    if edits
        .iter()
        .any(|(edit, _)| super::span_may_contain_comment(source, *edit))
    {
        return Some(diagnostic.with_note(
            "a comment between the null test and the binding needs a manual rewrite",
        ));
    }
    for (edit, replacement) in edits {
        diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
            edit,
            "bind the optional with `guard let`",
            replacement,
        ));
    }
    Some(diagnostic)
}

/// The non-null operand of `SUBJECT <op> null` or `null <op> SUBJECT`.
fn null_comparison(arena: &AstArena, condition: ExprId, op: BinaryOp) -> Option<ExprId> {
    let ArenaExprKind::Binary {
        op: found,
        left,
        right,
    } = arena.expr(condition).kind
    else {
        return None;
    };
    if found != op {
        return None;
    }
    match (arena.expr(left).kind, arena.expr(right).kind) {
        (ArenaExprKind::Null, ArenaExprKind::Null) => None,
        (_, ArenaExprKind::Null) => Some(left),
        (ArenaExprKind::Null, _) => Some(right),
        _ => None,
    }
}

/// The names of a binding or field path (`x`, `x.a.b`): an expression whose
/// evaluation runs no code, so it may be read once or twice alike.
fn stable_path(arena: &AstArena, mut expr: ExprId) -> Option<Vec<Name>> {
    let mut path = Vec::new();
    loop {
        match arena.expr(expr).kind {
            ArenaExprKind::Ident(name) => {
                path.push(name);
                path.reverse();
                return Some(path);
            }
            ArenaExprKind::Field { base, name } => {
                path.push(name);
                expr = base;
            }
            _ => return None,
        }
    }
}

/// The blanks that indent the line containing `offset`.
fn line_indent(source: &str, offset: usize) -> &str {
    let line_start = source[..offset].rfind('\n').map_or(0, |index| index + 1);
    let line = &source[line_start..offset];
    let blanks = line.len() - line.trim_start_matches([' ', '\t']).len();
    &line[..blanks]
}
