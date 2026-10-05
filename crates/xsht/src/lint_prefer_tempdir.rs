use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label, Severity};
use xsh::frontend::source::{SourceId, Span};
use xsh::frontend::symbols::Name;
use xsh::frontend::syntax::arena::{
    ArenaBindingTargetKind, ArenaCallArgKind, ArenaExprKind, ArenaExprOrRun, ArenaStmtKind,
    AstArena, BlockId, DeferTrigger, ExprId, StmtId,
};
use xsh::frontend::syntax::parser::Parser;
use xsh::frontend::syntax::token::TokenTag;
use xsh::frontend::check::{StatementPosition, Type};

/// Three adjacent statements that clear a path, create a directory there, and
/// defer its removal are what `tempdir NAME at PATH { ... }` is defined to
/// mean:
///
/// ```text
/// NAME.remove(missing_ok: true)
/// NAME.mkdir()
/// defer NAME.remove(missing_ok: true)
/// ```
///
/// Each call may carry a `?`, which says again what statement position
/// already does. `block` is the block whose statements these are; the statements of a
/// file have none and are never rewritten, because a declaration or binding
/// moved into a block would stop being a top-level one.
pub(super) fn lint_scratch_directories(
    linter: &mut super::Linter<'_>,
    stmts: &[StmtId],
    block: Option<BlockId>,
) {
    let arena = linter.arena;
    for index in 0..stmts.len().saturating_sub(2) {
        let Some(cleared) = cleared_path(arena, stmts[index]) else {
            continue;
        };
        let name = cleared.name;
        let (Some(created), Some(removed)) = (
            created_path(arena, stmts[index + 1]),
            deferred_removal(arena, stmts[index + 2]),
        ) else {
            continue;
        };
        if created.name != name || removed.name != name {
            continue;
        }
        let calls = [cleared, created, removed];
        // A method named `remove` or `mkdir` on anything but a `Path` is
        // another operation.
        if calls
            .iter()
            .any(|call| linter.expr_types.get(&call.span) != Some(&Type::Path))
        {
            continue;
        }
        let first = arena.stmt(stmts[index]).span;
        let last = arena.stmt(stmts[index + 2]).span;
        let mut diagnostic = Diagnostic::new(
            Severity::Warning,
            "prefer `tempdir NAME at PATH` for a scratch directory",
        )
        .with_code(DiagnosticCode::LintPreferTempdir)
        .with_label(Label::secondary(
            Span::new(first.source_id, first.start(), last.end()),
            "these three statements are `tempdir NAME at PATH { ... }` around the rest of the block",
        ));
        if let Some(block) = block
            && let Some(fix) = tempdir_rewrite(linter, block, stmts, index, name)
        {
            diagnostic = diagnostic.with_fix_hint(fix);
        }
        linter.diagnostics.push(diagnostic);
    }
}

/// One of the three calls.
struct PathCall {
    /// The path operand: the method's receiver.
    path: ExprId,
    arguments: Vec<ArenaCallArgKind>,
}

/// `path.OPERATION(...)`, under an optional `?`.
fn path_call(arena: &AstArena, expr: ExprId, operation: &str) -> Option<PathCall> {
    let expr = match arena.expr(expr).kind {
        ArenaExprKind::Try(inner) => inner,
        _ => expr,
    };
    let ArenaExprKind::Call { callee, args } = arena.expr(expr).kind else {
        return None;
    };
    let ArenaExprKind::Field { base, name } = arena.expr(callee).kind else {
        return None;
    };
    if name != operation {
        return None;
    }
    let arguments = arena
        .call_args(args)
        .iter()
        .map(|argument| argument.kind.clone())
        .collect::<Vec<_>>();
    Some(PathCall {
        path: base,
        arguments,
    })
}

/// A local operand of one of the three calls.
#[derive(Clone, Copy)]
struct Operand {
    name: Name,
    /// The span of the name where the call reads it.
    span: Span,
}

fn local(arena: &AstArena, call: &PathCall) -> Option<Operand> {
    match arena.expr(call.path).kind {
        ArenaExprKind::Ident(name) => Some(Operand {
            name,
            span: arena.expr(call.path).span,
        }),
        _ => None,
    }
}

/// The local in `NAME.remove()` or `NAME.remove(missing_ok: true)`, which are
/// the same call: a missing path is accepted by default.
fn removed_path(arena: &AstArena, expr: ExprId) -> Option<Operand> {
    let call = path_call(arena, expr, "remove")?;
    match &call.arguments[..] {
        [] => local(arena, &call),
        [ArenaCallArgKind::Named { name, value, .. }]
            if *name == "missing_ok"
                && matches!(arena.expr(*value).kind, ArenaExprKind::Bool(true)) =>
        {
            local(arena, &call)
        }
        _ => None,
    }
}

fn cleared_path(arena: &AstArena, stmt: StmtId) -> Option<Operand> {
    match arena.stmt(stmt).kind {
        ArenaStmtKind::Expr(expr) => removed_path(arena, expr),
        _ => None,
    }
}

/// The local in `NAME.mkdir()`.
fn created_path(arena: &AstArena, stmt: StmtId) -> Option<Operand> {
    let ArenaStmtKind::Expr(expr) = arena.stmt(stmt).kind else {
        return None;
    };
    let call = path_call(arena, expr, "mkdir")?;
    if !call.arguments.is_empty() {
        return None;
    }
    local(arena, &call)
}

fn deferred_removal(arena: &AstArena, stmt: StmtId) -> Option<Operand> {
    match arena.stmt(stmt).kind {
        ArenaStmtKind::Defer(ArenaExprOrRun::Expr(expr), DeferTrigger::Exit) => {
            removed_path(arena, expr)
        }
        _ => None,
    }
}

pub(super) fn binds(arena: &AstArena, stmt: StmtId, is: impl Fn(Name) -> bool) -> bool {
    let (ArenaStmtKind::Let { target, .. }
    | ArenaStmtKind::Var { target, .. }
    | ArenaStmtKind::Const { target, .. }) = arena.stmt(stmt).kind
    else {
        return false;
    };
    match arena.binding_target(target).kind {
        ArenaBindingTargetKind::Name(name) => is(name),
        // Which names a destructuring binds is not worth reading here.
        ArenaBindingTargetKind::Record { .. } => true,
    }
}

/// The rewrite of `let NAME = PATH`, the three statements at `index`, and the
/// rest of the block into `tempdir NAME at PATH { REST }`, when that is the
/// same program:
///
/// - `let NAME = PATH` is the statement before, and `PATH` is a `Path`, as the
///   form's `NAME: Path` requires.
/// - Nothing earlier in the block can own a process handle, a network job, or
///   an open stream. The block releases those before its defers run, and the
///   rewrite would move the removal into an inner block that exits first.
/// - The text can be moved as it is: every statement starts its own line and no
///   comment sits among the replaced statements. A line that begins inside a
///   token, such as a line of a multi-line string, is left where it is.
/// - The result parses to a `tempdir` whose path and body are, node for node,
///   the path and the rest of the block.
fn tempdir_rewrite(
    linter: &super::Linter<'_>,
    block: BlockId,
    stmts: &[StmtId],
    index: usize,
    name: Name,
) -> Option<FixHint> {
    let arena = linter.arena;
    let source = linter.source;
    let binding = *stmts.get(index.checked_sub(1)?)?;
    let ArenaStmtKind::Let {
        target,
        ty,
        initializer: ArenaExprOrRun::Expr(path),
    } = arena.stmt(binding).kind
    else {
        return None;
    };
    if !matches!(arena.binding_target(target).kind, ArenaBindingTargetKind::Name(bound) if bound == name)
    {
        return None;
    }
    let binding_span = arena.stmt(binding).span;
    let path_span = arena.expr(path).span;
    let path_text = source.get(path_span.range())?;
    let declared = match ty {
        Some(ty) => Some(Type::from_arena(arena, ty)),
        None => linter.expr_types.get(&path_span).cloned(),
    };
    if declared != Some(Type::Path) {
        return None;
    }
    if text_end_of(source, binding_span)? != path_span.end() {
        return None;
    }
    if !stmts[..index - 1]
        .iter()
        .all(|stmt| owns_nothing(linter, *stmt))
    {
        return None;
    }
    let rest = &stmts[index + 3..];
    if rest
        .last()
        .is_some_and(|tail| !tail_keeps_its_meaning(linter, block, *tail))
    {
        return None;
    }
    let text_end = |stmt: StmtId| text_end_of(source, arena.stmt(stmt).span);

    let line_start = |offset: usize| source[..offset].rfind('\n').map_or(0, |at| at + 1);
    let line_end = |offset: usize| source[offset..].find('\n').map_or(source.len(), |at| offset + at);
    let indent = source.get(line_start(binding_span.start())..binding_span.start())?;
    if !indent.chars().all(|ch| ch == ' ') {
        return None;
    }
    // Every statement involved starts its own line at the block's indent.
    let starts_line = |stmt: &StmtId| {
        let start = arena.stmt(*stmt).span.start();
        source.get(line_start(start)..start) == Some(indent)
    };
    if !stmts[index..].iter().all(starts_line) {
        return None;
    }
    let replaced = Span::new(
        binding_span.source_id,
        binding_span.start(),
        text_end(stmts[index + 2])?,
    );
    if super::span_may_contain_comment(source, replaced) {
        return None;
    }
    let head_end = line_end(replaced.end());
    if source.get(replaced.end()..head_end)?.trim() != "" {
        return None;
    }
    let end = match rest.last() {
        Some(last) => {
            let text_end = text_end(*last)?;
            let end = line_end(text_end);
            let tail = source.get(text_end..end)?.trim_start();
            if !(tail.is_empty() || tail.starts_with('#')) {
                return None;
            }
            end
        }
        None => head_end,
    };
    let body = source.get(head_end..end)?;
    let tokens = tokens_spanning_lines(linter, head_end, end)?;
    let mut replacement = format!("tempdir {} at {path_text} {{", name.as_str());
    let mut line_start = head_end;
    for line in body.split('\n').skip(1) {
        line_start += 1;
        replacement.push('\n');
        let inside_token = tokens
            .iter()
            .any(|token| token.start < line_start && line_start < token.end);
        if !inside_token && !line.trim().is_empty() {
            replacement.push_str("  ");
        }
        replacement.push_str(line);
        line_start += line.len();
    }
    replacement.push('\n');
    replacement.push_str(indent);
    replacement.push('}');

    // The rewrite is offered only when the replacement, parsed on its own, is
    // one `tempdir` over the same path and the same rest of the block.
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), &replacement);
    if !parsed.diagnostics.is_empty() {
        return None;
    }
    let rewritten = &parsed.arena.arena;
    let mut statements = parsed.arena.statement_ids();
    let (Some(statement), None) = (statements.next(), statements.next()) else {
        return None;
    };
    let ArenaStmtKind::Expr(scope) = rewritten.stmt(statement).kind else {
        return None;
    };
    let ArenaExprKind::TempDirScope {
        path: Some(new_path),
        block: new_body,
        ..
    } = rewritten.expr(scope).kind
    else {
        return None;
    };
    if rewritten
        .stmt_ids(rewritten.block(new_body).statements)
        .count()
        != rest.len()
    {
        return None;
    }
    let key = |root| super::super::format::canonical_subtree(arena, source, root);
    let new_key = |root| {
        parsed
            .arena
            .symbol_owner()
            .with_current(|| super::super::format::canonical_subtree(rewritten, &replacement, root))
    };
    if !matches!(
        rewritten.block_params(rewritten.block(new_body).params),
        [bound] if bound.name == name
    ) {
        return None;
    }
    // A block's key is its statements' keys in order after its opening, which
    // ends at the first `|`: the body's opening holds the directory name, an
    // identifier. So the body holds the rest of the block exactly when its
    // statements' keys end the block's key.
    let moved = new_key(Ok(new_body));
    let (_, moved) = moved.strip_prefix("B(")?.split_once('|')?;
    if key(Err(path)) != new_key(Err(new_path)) || !key(Ok(block)).ends_with(moved) {
        return None;
    }
    Some(FixHint::replacement(
        Span::new(binding_span.source_id, binding_span.start(), end),
        "rewrite as `tempdir ... at ...`",
        replacement,
    ))
}

/// Whether `tail`, the last statement of `block`, means the same as the last
/// statement of a `tempdir` scope that is itself the last statement of
/// `block`.
///
/// A scope whose value is used is a `Result` of its body's tail. So a tail
/// that is only a statement moves freely, and a tail that is the block's
/// value moves only where a `Result` of it is what the block may produce: in
/// the body of a function that returns `Result[T]`, for a tail that is a `T`.
fn tail_keeps_its_meaning(linter: &super::Linter<'_>, block: BlockId, tail: StmtId) -> bool {
    let arena = linter.arena;
    let mut core = tail;
    while let ArenaStmtKind::Sugar { expansion, .. } = arena.stmt(core).kind {
        core = expansion;
    }
    let statement = arena.stmt(core);
    let produces_value = match statement.kind {
        ArenaStmtKind::Expr(_)
        | ArenaStmtKind::Command(_)
        | ArenaStmtKind::If { .. }
        | ArenaStmtKind::Match { .. }
        | ArenaStmtKind::Loop { .. }
        | ArenaStmtKind::TailBareIdent(_) => {
            linter.statement_positions.get(&statement.span) != Some(&StatementPosition::Statement)
        }
        _ => false,
    };
    if !produces_value {
        return true;
    }
    linter.function_bodies.last() == Some(&block)
        && matches!(linter.function_return_types.last(), Some(Type::Result(..)))
        && !tail_may_be_a_result(linter, tail)
}

/// Whether `stmt`, as the last statement of a block, may give the block a
/// `Result` value other than `Result[Unit]`.
///
/// The tail of a function that returns `Result[T]` may be a `T` or a
/// `Result[T]`, but the tail of a `tempdir` scope there must be a `T`: the
/// scope's value is a `Result` of it.
fn tail_may_be_a_result(linter: &super::Linter<'_>, stmt: StmtId) -> bool {
    let arena = linter.arena;
    let block_tail = |block| {
        arena
            .stmt_ids(arena.block(block).statements)
            .last()
            .is_some_and(|tail| tail_may_be_a_result(linter, tail))
    };
    match arena.stmt(stmt).kind {
        ArenaStmtKind::Expr(expr) => match arena.expr(expr).kind {
            ArenaExprKind::ValueBlock(block) => block_tail(block),
            _ => !matches!(
                linter.expr_types.get(&arena.expr(expr).span),
                Some(ty) if !matches!(ty, Type::Result(ok, _) if **ok != Type::Unit)
            ),
        },
        ArenaStmtKind::If {
            branches,
            else_block,
        } => {
            arena
                .if_branches(branches)
                .iter()
                .any(|branch| block_tail(branch.block))
                || else_block.is_some_and(block_tail)
        }
        ArenaStmtKind::Match { arms, .. } => {
            arena.match_arms(arms).iter().any(|arm| block_tail(arm.block))
        }
        // A bare name's type is not recorded where the linter can read it.
        ArenaStmtKind::TailBareIdent(_) => true,
        ArenaStmtKind::Sugar { expansion, .. } => tail_may_be_a_result(linter, expansion),
        _ => false,
    }
}

/// Where a statement's text ends. Its span runs on through its terminator.
pub(super) fn text_end_of(source: &str, statement: Span) -> Option<usize> {
    let text = source.get(statement.range())?;
    Some(statement.start() + text.trim_end().len())
}

/// Whether running `stmt` leaves its block owning nothing it must release: a
/// binding or expression statement made only of names, literals, operators,
/// and calls whose checked result is plain data.
fn owns_nothing(linter: &super::Linter<'_>, stmt: StmtId) -> bool {
    let arena = linter.arena;
    let expr = match arena.stmt(stmt).kind {
        ArenaStmtKind::Let {
            initializer: ArenaExprOrRun::Expr(expr),
            ..
        }
        | ArenaStmtKind::Var {
            initializer: ArenaExprOrRun::Expr(expr),
            ..
        }
        | ArenaStmtKind::Const {
            initializer: ArenaExprOrRun::Expr(expr),
            ..
        }
        | ArenaStmtKind::Expr(expr) => expr,
        _ => return false,
    };
    expr_owns_nothing(linter, expr)
}

pub(super) fn expr_owns_nothing(linter: &super::Linter<'_>, expr: ExprId) -> bool {
    let arena = linter.arena;
    let inert = match arena.expr(expr).kind {
        ArenaExprKind::Null
        | ArenaExprKind::Bool(_)
        | ArenaExprKind::Int(_)
        | ArenaExprKind::Float(_)
        | ArenaExprKind::Duration(_)
        | ArenaExprKind::Str(_)
        | ArenaExprKind::PathStr(_)
        | ArenaExprKind::FmtString(_)
        | ArenaExprKind::PathFmtString(_)
        | ArenaExprKind::Bytes(_)
        | ArenaExprKind::Ident(_)
        | ArenaExprKind::List(_)
        | ArenaExprKind::Record(_)
        | ArenaExprKind::Unary { .. }
        | ArenaExprKind::Binary { .. }
        | ArenaExprKind::Field { .. }
        | ArenaExprKind::Index { .. } => true,
        // A call hands a resource to its caller only in the value it returns.
        ArenaExprKind::Call { .. } | ArenaExprKind::Try(_) => linter
            .expr_types
            .get(&arena.expr(expr).span)
            .is_some_and(is_plain_data),
        _ => false,
    };
    inert
        && super::expr_child_exprs(arena, expr)
            .into_iter()
            .all(|child| expr_owns_nothing(linter, child))
}

fn is_plain_data(ty: &Type) -> bool {
    match ty {
        Type::Unit
        | Type::Null
        | Type::Bool
        | Type::Int
        | Type::UInt
        | Type::Float
        | Type::Duration
        | Type::Str
        | Type::Bytes
        | Type::Path
        | Type::Error
        | Type::ErrorFamily(_)
        | Type::ErrorVariant { .. } => true,
        Type::Result(ok, error) => is_plain_data(ok) && is_plain_data(error),
        Type::Optional(inner) | Type::List(inner) => is_plain_data(inner),
        _ => false,
    }
}

/// The source ranges of the tokens in `start..end` that span lines, or `None`
/// when the text does not lex. A line that begins inside one is part of a
/// literal, and indenting it could change the literal's value.
pub(super) fn tokens_spanning_lines(
    linter: &super::Linter<'_>,
    start: usize,
    end: usize,
) -> Option<Vec<std::ops::Range<usize>>> {
    let text = linter.source.get(start..end)?;
    let lexed = xsh::frontend::syntax::lexer::Lexer::new(SourceId::new(0), text).lex_compact();
    if !lexed.diagnostics.is_empty() {
        return None;
    }
    let tokens = &lexed.token_table;
    let mut spanning = Vec::new();
    for index in 0..tokens.len() {
        if matches!(
            tokens.tag_at(index),
            Some(TokenTag::Newline | TokenTag::Comment | TokenTag::Eof)
        ) {
            continue;
        }
        let (token_start, token_end) = (tokens.start_at(index)?, tokens.end_at(index, text)?);
        if text.get(token_start..token_end)?.contains('\n') {
            spanning.push(start + token_start..start + token_end);
        }
    }
    Some(spanning)
}

#[cfg(test)]
mod tests {
    use super::super::{LintOptions, Linter};
    use xsh::diagnostic::{Diagnostic, DiagnosticCode};
    use xsh::frontend::check::Checker;
    use xsh::frontend::source::SourceId;
    use xsh::frontend::syntax::parser::Parser;

    fn scratch_directories(source: &str) -> Vec<Diagnostic> {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        Linter::lint(
            &parsed.arena,
            source,
            LintOptions {
                expr_types: checked.expr_types,
                statement_positions: checked.statement_positions,
                function_return_types: checked.function_return_types,
                ..LintOptions::default()
            },
        )
        .diagnostics
            .into_iter()
            .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferTempdir))
            .collect()
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
    fn a_scratch_directory_becomes_tempdir_around_the_rest_of_the_block() {
        let source = "proc stage(root: Path) [fs, error] -> Result[Str] {\n  let label = \"stage\"\n  root.mkdir()?\n  let scratch = fp\"{root}/{label}\"\n  scratch.remove(missing_ok: true)?\n  scratch.mkdir()\n  defer scratch.remove(missing_ok: true)?\n\n  let stamp = fp\"{scratch}/stamp\"\n  stamp.write(label)? # mark\n  stamp.read_text()?\n}\n";
        let diagnostics = scratch_directories(source);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        let fixed = apply(&diagnostics, source);
        assert_eq!(
            fixed,
            "proc stage(root: Path) [fs, error] -> Result[Str] {\n  let label = \"stage\"\n  root.mkdir()?\n  tempdir scratch at fp\"{root}/{label}\" {\n\n    let stamp = fp\"{scratch}/stamp\"\n    stamp.write(label)? # mark\n    stamp.read_text()?\n  }\n}\n"
        );
        // The fixed program checks and is not reported again.
        assert!(scratch_directories(&fixed).is_empty());
    }

    /// A tail that is only a statement moves into the scope from any block.
    #[test]
    fn a_statement_tail_moves_from_a_nested_block() {
        let source = "proc stage(roots: List[Path]) [fs, error] {\n  for root in roots {\n    let scratch = fp\"{root}/s\"\n    scratch.remove(missing_ok: true)\n    scratch.mkdir()\n    defer scratch.remove(missing_ok: true)\n    fp\"{scratch}/stamp\".write(\"x\")\n  }\n}\n";
        let diagnostics = scratch_directories(source);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        let fixed = apply(&diagnostics, source);
        assert_eq!(
            fixed,
            "proc stage(roots: List[Path]) [fs, error] {\n  for root in roots {\n    tempdir scratch at fp\"{root}/s\" {\n      fp\"{scratch}/stamp\".write(\"x\")\n    }\n  }\n}\n"
        );
        assert!(scratch_directories(&fixed).is_empty());
    }

    #[test]
    fn an_annotated_path_and_an_empty_rest_are_rewritten() {
        let source = "proc stage(root: Path) [fs, error] {\n  if root.exists()? {\n    let scratch: Path = root\n    scratch.remove(missing_ok: true)\n    scratch.mkdir()\n    defer scratch.remove(missing_ok: true)\n  }\n}\n";
        let diagnostics = scratch_directories(source);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(
            apply(&diagnostics, source),
            "proc stage(root: Path) [fs, error] {\n  if root.exists()? {\n    tempdir scratch at root {\n    }\n  }\n}\n"
        );
    }

    /// A line that begins inside a multi-line string is part of its value, or
    /// of the margin a block string is read against, so it stays where it is.
    #[test]
    fn lines_inside_a_multi_line_string_are_not_indented() {
        let source = "proc stage(root: Path) [fs, error] -> Result[Unit] {\n  let scratch = fp\"{root}/s\"\n  scratch.remove(missing_ok: true)\n  scratch.mkdir()\n  defer scratch.remove(missing_ok: true)\n  fp\"{scratch}/exact\".write(\"\"\"first\n  second\n\"\"\")?\n  fp\"{scratch}/block\".write(\n    \"\"\"\n    first\n      second\n    \"\"\",\n  )?\n}\n";
        let diagnostics = scratch_directories(source);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        let fixed = apply(&diagnostics, source);
        assert_eq!(
            fixed,
            "proc stage(root: Path) [fs, error] -> Result[Unit] {\n  tempdir scratch at fp\"{root}/s\" {\n    fp\"{scratch}/exact\".write(\"\"\"first\n  second\n\"\"\")?\n    fp\"{scratch}/block\".write(\n      \"\"\"\n    first\n      second\n    \"\"\",\n    )?\n  }\n}\n"
        );
        assert!(scratch_directories(&fixed).is_empty());
    }

    fn reported_without_a_fix(source: &str) {
        let diagnostics = scratch_directories(source);
        assert_eq!(diagnostics.len(), 1, "{source}\n{diagnostics:?}");
        assert!(diagnostics[0].fix_hints.is_empty(), "{source}");
    }

    #[test]
    fn a_rewrite_that_is_not_provably_the_same_program_is_only_reported() {
        let triple = "  scratch.remove(missing_ok: true)\n  scratch.mkdir()\n  defer scratch.remove(missing_ok: true)\n";
        for source in [
            // A parameter: there is no binding to fold into the head.
            format!("proc stage(scratch: Path) [fs, error] {{\n{triple}}}\n"),
            // Something between the binding and the three statements.
            format!(
                "proc stage(root: Path) [fs, error] {{\n  let scratch = fp\"{{root}}/s\"\n  print \"staging\"\n{triple}}}\n"
            ),
            // A handle the block owns is released before its defers.
            format!(
                "proc stage(root: Path) [fs, process, error] {{\n  let job = spawn run sleep 1 ?\n  let scratch = fp\"{{root}}/s\"\n{triple}  job.cancel()?\n}}\n"
            ),
            // A `Result` tail would nest in the scope's own `Result`.
            format!(
                "proc stage(root: Path) [fs, error] -> Result[Str] {{\n  let scratch = fp\"{{root}}/s\"\n{triple}  fp\"{{scratch}}/stamp\".read_text()\n}}\n"
            ),
            // The value of a block that is not a function's body would become
            // a `Result` of it.
            "proc stage(root: Path) [fs, error] -> Result[Str] {\n  let text = {\n    let scratch = fp\"{root}/s\"\n    scratch.remove(missing_ok: true)\n    scratch.mkdir()\n    defer scratch.remove(missing_ok: true)\n    fp\"{scratch}/stamp\".read_text()?\n  }\n  text\n}\n".to_owned(),
            // A comment among the replaced statements would be dropped.
            format!(
                "proc stage(root: Path) [fs, error] {{\n  let scratch = fp\"{{root}}/s\" # where\n{triple}}}\n"
            ),
        ] {
            reported_without_a_fix(&source);
        }
        // The statements of a file are never moved into a block.
        reported_without_a_fix(
            "let scratch = p\"/tmp/s\"\nscratch.remove(missing_ok: true)\nscratch.mkdir()\ndefer scratch.remove(missing_ok: true)\n",
        );
    }

    #[test]
    fn other_cleanup_shapes_are_left_alone() {
        for source in [
            // The removal is registered before the directory exists.
            "proc stage(scratch: Path) [fs, error] {\n  scratch.remove(missing_ok: true)\n  defer scratch.remove(missing_ok: true)\n  scratch.mkdir()\n}\n",
            // A missing directory is an error for one of the removals.
            "proc stage(scratch: Path) [fs, error] {\n  scratch.remove(missing_ok: true)\n  scratch.mkdir()\n  defer scratch.remove(missing_ok: false)\n}\n",
            // Two different paths.
            "proc stage(scratch: Path, other: Path) [fs, error] {\n  scratch.remove(missing_ok: true)\n  other.mkdir()\n  defer scratch.remove(missing_ok: true)\n}\n",
            "proc stage(root: Path) [fs, error] {\n  tempdir scratch at root {\n    print $scratch\n  }\n}\n",
        ] {
            assert!(scratch_directories(source).is_empty(), "{source}");
        }
    }

    // A removal that leaves `missing_ok` out accepts a missing path, as the
    // statement's own removals do.
    #[test]
    fn a_removal_without_missing_ok_is_the_same_removal() {
        let source = "proc stage(scratch: Path) [fs, error] {\n  scratch.remove()\n  scratch.mkdir()\n  defer scratch.remove()\n  print $scratch\n}\n";
        assert_eq!(scratch_directories(source).len(), 1, "{source}");
    }

    // A removal accepts a missing path by default, so the argument that says
    // so may be left out of either removal.
    #[test]
    fn a_removal_without_its_default_argument_is_the_same_sequence() {
        let explicit = "proc stage(root: Path) [fs, error] {\n  let scratch = fp\"{root}/s\"\n  scratch.remove(missing_ok: true)\n  scratch.mkdir()\n  defer scratch.remove(missing_ok: true)\n  print $scratch\n}\n";
        let defaulted = "proc stage(root: Path) [fs, error] {\n  let scratch = fp\"{root}/s\"\n  scratch.remove()\n  scratch.mkdir()\n  defer scratch.remove()\n  print $scratch\n}\n";
        let expected = "proc stage(root: Path) [fs, error] {\n  tempdir scratch at fp\"{root}/s\" {\n    print $scratch\n  }\n}\n";
        for source in [explicit, defaulted] {
            let diagnostics = scratch_directories(source);
            assert_eq!(diagnostics.len(), 1, "{source}");
            assert_eq!(apply(&diagnostics, source), expected, "{source}");
        }
    }

    // `mkdir` with a parent policy is not the bare creation the form does.
    #[test]
    fn a_creation_with_an_argument_is_left_alone() {
        let source = "proc stage(scratch: Path) [fs, error] {\n  scratch.remove()\n  scratch.mkdir(parents: false)\n  defer scratch.remove()\n}\n";
        assert!(scratch_directories(source).is_empty(), "{source}");
    }
}
