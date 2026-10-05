use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label, Severity};
use xsh::frontend::check::Type;
use xsh::frontend::source::{SourceId, Span};
use xsh::frontend::symbols::Name;
use xsh::frontend::syntax::arena::{
    ArenaBindingTargetKind, ArenaCallArgKind, ArenaExprKind, ArenaExprOrRun, ArenaFmtPart,
    ArenaStmtKind, ArenaSugar, AstArena, BlockId, DeferTrigger, ExprId, StmtId,
};
use std::fmt::Write as _;
use xsh::frontend::syntax::parser::Parser;

use super::prefer_tempdir::{binds, expr_owns_nothing, text_end_of, tokens_spanning_lines};

/// A file produced at a temporary path and renamed over its destination is
/// what `atomically replace DEST as NAME { ... }` is defined to mean:
///
/// ```text
/// let NAME = fp"{DEST}.tmp"
/// NAME.remove(missing_ok: true)
/// defer NAME.remove(missing_ok: true)
/// ...
/// NAME.rename(to: DEST, overwrite: true)
/// ```
///
/// A statement list is reported when it binds a `Path`, clears it, and later
/// renames it. Each call may be written `fs.OP(path, ...)` or
/// `path.OP(...)`, and may carry a `?`, which says again what statement
/// position already does. `block` is the block whose statements these are;
/// the statements of a file have none and are never rewritten, because a
/// binding moved into a block would stop being a top-level one.
pub(super) fn lint_published_files(
    linter: &mut super::Linter<'_>,
    stmts: &[StmtId],
    block: Option<BlockId>,
) {
    let arena = linter.arena;
    for (rename_index, &rename_stmt) in stmts.iter().enumerate() {
        let ArenaStmtKind::Expr(expr) = arena.stmt(rename_stmt).kind else {
            continue;
        };
        let Some(rename) = renamed_path(arena, expr) else {
            continue;
        };
        let name = rename.temporary;
        let earlier = &stmts[..rename_index];
        let Some(binding_index) = earlier
            .iter()
            .rposition(|stmt| binds(arena, *stmt, |bound| bound == name))
        else {
            continue;
        };
        if !binds_a_path(linter, stmts[binding_index], name) {
            continue;
        }
        let Some(clear_index) = (binding_index + 1..rename_index)
            .find(|index| cleared_path(arena, stmts[*index]) == Some(name))
        else {
            continue;
        };
        let first = arena.stmt(stmts[binding_index]).span;
        let last = arena.stmt(rename_stmt).span;
        let mut diagnostic = Diagnostic::new(
            Severity::Warning,
            "prefer `atomically replace DEST as NAME` for a file renamed into place",
        )
        .with_code(DiagnosticCode::LintPreferAtomicallyReplace)
        .with_label(Label::secondary(
            Span::new(first.source_id, first.start(), last.end()),
            "these statements produce a file at a temporary path and rename it over its destination",
        ));
        let sequence = Sequence {
            binding: binding_index,
            clear: clear_index,
            rename: rename_index,
        };
        diagnostic = match block
            .ok_or("the statements of a file are not moved into a block")
            .and_then(|block| atomically_rewrite(linter, block, stmts, sequence, &rename))
        {
            Ok(fix) => diagnostic.with_fix_hint(fix),
            Err(difference) => diagnostic.with_note(format!(
                "no automatic rewrite: {difference}; `atomically replace` names a fresh hidden path beside the destination, removes it however the block ends, and renames it with `overwrite: true` after the block"
            )),
        };
        linter.diagnostics.push(diagnostic);
    }
}

/// The rename of an `atomically replace` runs only when its body runs to its
/// end. A body that leaves on every path, by `return`, `break`, `continue`,
/// `exit`, or a statement that always fails, never replaces the destination:
/// the statement only makes and discards a temporary file. `statement` is the
/// whole statement and `body` its block.
pub(super) fn lint_body_that_never_finishes(
    linter: &mut super::Linter<'_>,
    statement: Span,
    body: BlockId,
) {
    let body_span = linter.arena.span(linter.arena.block(body).span);
    if !linter.definitely_exiting_block_spans.contains(&body_span) {
        return;
    }
    linter.diagnostics.push(
        Diagnostic::new(
            Severity::Warning,
            "this `atomically replace` never replaces its destination",
        )
        .with_code(DiagnosticCode::LintAtomicallyNeverReplaces)
        .with_label(Label::primary(
            Span::new(statement.source_id, statement.start(), body_span.start()),
            "the body leaves on every path, so the rename after it never runs",
        ))
        .with_note(
            "leaving the body early discards the temporary file and publishes nothing; let the body run to its end on the path that should replace the destination",
        ),
    );
}

/// Where the statements of one publication sit in their statement list.
#[derive(Clone, Copy)]
struct Sequence {
    binding: usize,
    clear: usize,
    rename: usize,
}

struct Rename {
    temporary: Name,
    dest: ExprId,
    overwrite: bool,
}

/// The receiver and the other arguments of `fs.OPERATION(path, ...)` or
/// `path.OPERATION(...)`, under an optional `?`.
fn path_operation(
    arena: &AstArena,
    expr: ExprId,
    operation: &str,
) -> Option<(ExprId, Vec<ArenaCallArgKind>)> {
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
    let mut arguments = arena
        .call_args(args)
        .iter()
        .map(|argument| argument.kind.clone())
        .collect::<Vec<_>>();
    if !matches!(arena.expr(base).kind, ArenaExprKind::Ident(module) if module == "fs") {
        return Some((base, arguments));
    }
    if arguments.is_empty() {
        return None;
    }
    let ArenaCallArgKind::Positional(path) = arguments.remove(0) else {
        return None;
    };
    Some((path, arguments))
}

fn local(arena: &AstArena, expr: ExprId) -> Option<Name> {
    match arena.expr(expr).kind {
        ArenaExprKind::Ident(name) => Some(name),
        _ => None,
    }
}

fn is_true_flag(arena: &AstArena, argument: &ArenaCallArgKind, flag: &str) -> bool {
    matches!(
        argument,
        ArenaCallArgKind::Named { name, value, .. }
            if *name == flag && matches!(arena.expr(*value).kind, ArenaExprKind::Bool(true))
    )
}

/// The local in a removal that accepts a missing path: one that leaves
/// `missing_ok` out, or one that writes the default `missing_ok: true`.
fn removed_path(arena: &AstArena, expr: ExprId) -> Option<Name> {
    let (path, arguments) = path_operation(arena, expr, "remove")?;
    match &arguments[..] {
        [] => local(arena, path),
        [flag] if is_true_flag(arena, flag, "missing_ok") => local(arena, path),
        _ => None,
    }
}

fn cleared_path(arena: &AstArena, stmt: StmtId) -> Option<Name> {
    match arena.stmt(stmt).kind {
        ArenaStmtKind::Expr(expr) => removed_path(arena, expr),
        _ => None,
    }
}

fn deferred_removal(arena: &AstArena, stmt: StmtId) -> Option<Name> {
    match arena.stmt(stmt).kind {
        ArenaStmtKind::Defer(ArenaExprOrRun::Expr(expr), DeferTrigger::Exit) => {
            removed_path(arena, expr)
        }
        _ => None,
    }
}

/// A rename of a local to a destination, with or without `overwrite: true`.
fn renamed_path(arena: &AstArena, expr: ExprId) -> Option<Rename> {
    let (path, arguments) = path_operation(arena, expr, "rename")?;
    let temporary = local(arena, path)?;
    // The destination is the function's second operand, or the method's
    // argument, which is labeled `to:`.
    let destination = |argument: &ArenaCallArgKind| match argument {
        ArenaCallArgKind::Positional(dest) => Some(*dest),
        ArenaCallArgKind::Named { name, value, .. } if *name == "to" => Some(*value),
        _ => None,
    };
    let (dest, overwrite) = match &arguments[..] {
        [dest] => (destination(dest)?, false),
        [dest, flag] if is_true_flag(arena, flag, "overwrite") => (destination(dest)?, true),
        _ => return None,
    };
    Some(Rename {
        temporary,
        dest,
        overwrite,
    })
}

/// Whether `stmt` is `let NAME = PATH` for a `PATH` that is a `Path`. A method
/// named `remove` or `rename` on any other receiver is another operation.
fn binds_a_path(linter: &super::Linter<'_>, stmt: StmtId, name: Name) -> bool {
    let arena = linter.arena;
    let ArenaStmtKind::Let {
        target,
        ty,
        initializer: ArenaExprOrRun::Expr(path),
    } = arena.stmt(stmt).kind
    else {
        return false;
    };
    if !matches!(arena.binding_target(target).kind, ArenaBindingTargetKind::Name(bound) if bound == name)
    {
        return false;
    }
    let declared = match ty {
        Some(ty) => Some(Type::from_arena(arena, ty)),
        None => linter.expr_types.get(&arena.expr(path).span).cloned(),
    };
    declared == Some(Type::Path)
}

/// The rewrite of the publication into `atomically replace DEST as NAME {
/// BODY }`, or the way the sequence differs from that form. It is the same
/// program, but for the name of the temporary file, when:
///
/// - `NAME` is bound to a path written beside `DEST` (`is_sibling_of`), and
///   the removal and a deferred removal are adjacent statements, so the
///   temporary path is a sibling of the destination that is removed however
///   the block ends. The form draws a fresh name and so has nothing to clear. Statements between the binding and the
///   removal stay before the rewritten statement; they may not name `NAME`.
/// - The rename passes `overwrite: true` and ends the block, so nothing after
///   it can see a binding the body would now enclose, and its failure
///   propagates, as the form's rename does, instead of being the block's
///   value.
/// - `DEST` is an immutable local the statements in between do not rebind,
///   so reading it before them reads the value the rename read after them.
/// - No statement in between registers a defer or can own a process handle,
///   a network job, or an open stream. The body is a block that runs its
///   defers and releases what it owns before the rename, and these statements
///   did so after it.
/// - The text can be moved as it is: every statement starts its own line and
///   no comment sits anywhere in the sequence, because a replacement that
///   spans a comment is never applied. A line that begins inside a token,
///   such as a line of a multi-line string, is left where it is.
/// - The result parses to an `atomically replace` whose destination and body
///   are, node for node, the destination and the statements in between.
fn atomically_rewrite(
    linter: &super::Linter<'_>,
    block: BlockId,
    stmts: &[StmtId],
    sequence: Sequence,
    rename: &Rename,
) -> Result<FixHint, &'static str> {
    const NOT_ADJACENT: &str = "the temporary path is not cleared and then given a deferred removal by two adjacent statements";
    const USED_EARLY: &str = "a statement between the binding and the removal names the temporary path";
    const LAYOUT: &str = "the statements do not each start a line of their own, or a comment sits among them";
    const NOT_THE_SAME: &str = "the rewritten statement does not parse to the same destination and body";

    let arena = linter.arena;
    let source = linter.source;
    let name = rename.temporary;
    let defer_index = sequence.clear + 1;
    if defer_index >= sequence.rename || deferred_removal(arena, stmts[defer_index]) != Some(name)
    {
        return Err(NOT_ADJACENT);
    }
    if !rename.overwrite {
        return Err("the rename does not pass `overwrite: true`");
    }
    if sequence.rename + 1 != stmts.len() {
        return Err("statements follow the rename in its block");
    }
    let rename_stmt = arena.stmt(stmts[sequence.rename]);
    let propagates = matches!(
        rename_stmt.kind,
        ArenaStmtKind::Expr(expr) if matches!(arena.expr(expr).kind, ArenaExprKind::Try(_))
    ) || linter.propagating_statements.contains(&rename_stmt.span);
    if !propagates {
        return Err("the result of the rename is the value of its block, not a failure that propagates");
    }
    let dest = local(arena, rename.dest)
        .filter(|dest| is_sibling_of(arena, source, stmts[sequence.binding], *dest))
        .ok_or("the temporary path is not written `fp\"{DEST}SUFFIX\"` or `fp\"{DEST.parent}/NAME\"` for a destination that is a local name")?;
    let body = &stmts[defer_index + 1..sequence.rename];
    if body.is_empty() {
        return Err("nothing is produced between the removal and the rename");
    }
    let earlier = &stmts[..sequence.binding];
    let dest_text = dest.as_str();
    let bound_here = earlier
        .iter()
        .rev()
        .find(|stmt| binds(arena, **stmt, |bound| bound == dest));
    let immutable = match bound_here {
        Some(stmt) => matches!(
            arena.stmt(*stmt).kind,
            ArenaStmtKind::Let { .. } | ArenaStmtKind::Const { .. }
        ),
        None => linter
            .scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(dest_text.as_str()))
            .is_some_and(|binding| !binding.mutable),
    };
    if !immutable
        || stmts[sequence.binding + 1..sequence.rename]
            .iter()
            .any(|stmt| binds(arena, *stmt, |bound| bound == dest || bound == name))
    {
        return Err("the destination is not an immutable local, or a statement in between rebinds it");
    }
    if !body.iter().all(|stmt| leaves_nothing_behind(linter, *stmt)) {
        return Err("a statement in between registers a defer or may own a handle, which a block would release before the rename");
    }

    let text_end = |stmt: StmtId| text_end_of(source, arena.stmt(stmt).span).ok_or(LAYOUT);
    let line_start = |offset: usize| source[..offset].rfind('\n').map_or(0, |at| at + 1);
    let line_end = |offset: usize| source[offset..].find('\n').map_or(source.len(), |at| offset + at);
    let binding_span = arena.stmt(stmts[sequence.binding]).span;
    let indent = source
        .get(line_start(binding_span.start())..binding_span.start())
        .filter(|indent| indent.chars().all(|ch| ch == ' '))
        .ok_or(LAYOUT)?;
    let starts_line = |stmt: &StmtId| {
        let start = arena.stmt(*stmt).span.start();
        source.get(line_start(start)..start) == Some(indent)
    };
    if !stmts[sequence.binding..].iter().all(starts_line) {
        return Err(LAYOUT);
    }
    // The three statements that open the sequence go away, with their lines.
    let head = Span::new(
        binding_span.source_id,
        binding_span.start(),
        text_end(stmts[defer_index])?,
    );
    let head_end = line_end(head.end());
    // What stands between the binding and the removal stays, as written,
    // before the statement. It ran after the binding and now runs before the
    // destination is read, which nothing can tell apart: the destination is
    // immutable and these statements do not name the temporary path.
    let kept = match &stmts[sequence.binding + 1..sequence.clear] {
        [] => "",
        between @ [first, ..] => {
            let start = arena.stmt(*first).span.start();
            let text = source
                .get(start..text_end(between[between.len() - 1])?)
                .ok_or(LAYOUT)?;
            if may_name(text, name).ok_or(LAYOUT)? {
                return Err(USED_EARLY);
            }
            text
        }
    };
    let rename_span = arena.stmt(stmts[sequence.rename]).span;
    let end = text_end(stmts[sequence.rename])?;
    let body_end = line_end(text_end(body[body.len() - 1])?);
    let blank = |from: usize, to: usize| source.get(from..to).is_some_and(|text| text.trim().is_empty());
    let replaced = Span::new(binding_span.source_id, binding_span.start(), line_end(end));
    if super::span_may_contain_comment(source, replaced)
        || !blank(head.end(), head_end)
        || !blank(body_end, rename_span.start())
        || !blank(end, line_end(end))
    {
        return Err(LAYOUT);
    }
    let body_text = source.get(head_end..body_end).ok_or(LAYOUT)?;
    let tokens = tokens_spanning_lines(linter, head_end, body_end).ok_or(LAYOUT)?;
    let mut replacement = String::new();
    if !kept.is_empty() {
        replacement.push_str(kept);
        replacement.push('\n');
        replacement.push_str(indent);
    }
    let statement_start = replacement.len();
    let _ = write!(replacement, "atomically replace {dest_text} as {} {{", name.as_str());
    let mut line_start = head_end;
    let mut leading = true;
    for line in body_text.split('\n').skip(1) {
        line_start += 1;
        let inside_token = tokens
            .iter()
            .any(|token| token.start < line_start && line_start < token.end);
        line_start += line.len();
        // A block does not open with a blank line.
        leading &= line.trim().is_empty();
        if leading {
            continue;
        }
        replacement.push('\n');
        if !inside_token && !line.trim().is_empty() {
            replacement.push_str("  ");
        }
        replacement.push_str(line);
    }
    replacement.push('\n');
    replacement.push_str(indent);
    replacement.push('}');

    // The rewrite is offered only when the replacement, parsed on its own, is
    // one `atomically replace` of the same destination over the same
    // statements.
    let statement_text = &replacement[statement_start..];
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), statement_text);
    if !parsed.diagnostics.is_empty() {
        return Err(NOT_THE_SAME);
    }
    let rewritten = &parsed.arena.arena;
    let mut statements = parsed.arena.statement_ids();
    let (Some(statement), None) = (statements.next(), statements.next()) else {
        return Err(NOT_THE_SAME);
    };
    let ArenaStmtKind::Sugar { form, operands, .. } = rewritten.stmt(statement).kind else {
        return Err(NOT_THE_SAME);
    };
    let ArenaSugar::Atomically {
        dest: new_dest,
        body: new_body,
        ..
    } = rewritten.sugar(form, operands)
    else {
        return Err(NOT_THE_SAME);
    };
    let key = |root| super::super::format::canonical_subtree(arena, source, root);
    let new_key = |root| {
        parsed
            .arena
            .symbol_owner()
            .with_current(|| super::super::format::canonical_subtree(rewritten, statement_text, root))
    };
    // A block's key is its statements' keys in order between an opening and a
    // closing, so the body holds the statements in between exactly when it
    // has as many statements and their keys, in order, are part of the
    // block's key.
    let moved = new_key(Ok(new_body));
    let moved = moved
        .strip_prefix("B(|")
        .and_then(|moved| moved.strip_suffix(')'))
        .ok_or(NOT_THE_SAME)?;
    if rewritten
        .stmt_ids(rewritten.block(new_body).statements)
        .count()
        != body.len()
        || key(Err(rename.dest)) != new_key(Err(new_dest))
        || !key(Ok(block)).contains(moved)
    {
        return Err(NOT_THE_SAME);
    }
    Ok(FixHint::replacement(
        Span::new(replaced.source_id, replaced.start(), end),
        "rewrite as `atomically replace ... as ...`",
        replacement,
    ))
}

/// Whether `binding` binds a path written in the directory of `dest`:
/// `fp"{DEST}SUFFIX"`, or `fp"{DEST.parent}/NAME"` where `NAME` is text and
/// `{DEST.name}` holes. Neither `SUFFIX` nor `NAME` names a further
/// component.
fn is_sibling_of(arena: &AstArena, source: &str, binding: StmtId, dest: Name) -> bool {
    let ArenaStmtKind::Let {
        initializer: ArenaExprOrRun::Expr(path),
        ..
    } = arena.stmt(binding).kind
    else {
        return false;
    };
    let ArenaExprKind::PathFmtString(parts) = arena.expr(path).kind else {
        return false;
    };
    // `DEST.COMPONENT`, as a field or as a call without arguments.
    let component_of_dest = |expr: ExprId, component: &str| {
        let expr = match arena.expr(expr).kind {
            ArenaExprKind::Call { callee, args } if arena.call_args(args).is_empty() => callee,
            _ => expr,
        };
        matches!(
            arena.expr(expr).kind,
            ArenaExprKind::Field { base, name } if name == component && local(arena, base) == Some(dest)
        )
    };
    let parts = arena.fmt_parts(parts).collect::<Vec<_>>();
    fn text<'a>(arena: &'a AstArena, source: &'a str, part: &'a ArenaFmtPart) -> Option<&'a str> {
        match part {
            ArenaFmtPart::Text(text) => arena.text_value(text, source),
            ArenaFmtPart::Expr(..) => None,
        }
    }
    let text = |part| text(arena, source, part);
    match &parts[..] {
        [ArenaFmtPart::Expr(base, None), suffix] if local(arena, *base) == Some(dest) => {
            text(suffix).is_some_and(|suffix| !suffix.is_empty() && !suffix.contains('/'))
        }
        [ArenaFmtPart::Expr(parent, None), separator, rest @ ..]
            if component_of_dest(*parent, "parent") =>
        {
            text(separator)
                .and_then(|separator| separator.strip_prefix('/'))
                .is_some_and(|first| !first.contains('/') && !(first.is_empty() && rest.is_empty()))
                && rest.iter().all(|part| match part {
                    ArenaFmtPart::Text(_) => text(part).is_some_and(|text| !text.contains('/')),
                    ArenaFmtPart::Expr(name, None) => component_of_dest(*name, "name"),
                    ArenaFmtPart::Expr(..) => false,
                })
        }
        _ => false,
    }
}

/// Whether a token of `text` holds the spelling of `name`, or `None` when
/// the text does not lex. A string that interpolates the name is one token,
/// so any token that contains the spelling counts.
fn may_name(text: &str, name: Name) -> Option<bool> {
    let lexed = xsh::frontend::syntax::lexer::Lexer::new(SourceId::new(0), text).lex_compact();
    if !lexed.diagnostics.is_empty() {
        return None;
    }
    let name = name.as_str();
    let tokens = &lexed.token_table;
    Some((0..tokens.len()).any(|index| {
        tokens
            .start_at(index)
            .zip(tokens.end_at(index, text))
            .and_then(|(start, end)| text.get(start..end))
            .is_some_and(|token| token.contains(name.as_str()))
    }))
}

/// Whether `stmt`, moved into a block that ends before the rename, changes
/// nothing by ending there: it registers no defer and leaves the block owning
/// nothing it must release.
fn leaves_nothing_behind(linter: &super::Linter<'_>, stmt: StmtId) -> bool {
    let arena = linter.arena;
    match arena.stmt(stmt).kind {
        ArenaStmtKind::Let {
            initializer: ArenaExprOrRun::Expr(expr),
            ..
        }
        | ArenaStmtKind::Var {
            initializer: ArenaExprOrRun::Expr(expr),
            ..
        }
        | ArenaStmtKind::Expr(expr) => expr_owns_nothing(linter, expr),
        // These own what they start only inside blocks of their own, or
        // start nothing that outlives the statement.
        ArenaStmtKind::Command(_)
        | ArenaStmtKind::Assign { .. }
        | ArenaStmtKind::Assert { .. }
        | ArenaStmtKind::If { .. }
        | ArenaStmtKind::While { .. }
        | ArenaStmtKind::For { .. }
        | ArenaStmtKind::Loop { .. }
        | ArenaStmtKind::Match { .. }
        | ArenaStmtKind::Return(_)
        | ArenaStmtKind::Break { .. }
        | ArenaStmtKind::Continue => true,
        ArenaStmtKind::Sugar { expansion, .. } => leaves_nothing_behind(linter, expansion),
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

    fn published_files(source: &str) -> Vec<Diagnostic> {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        Linter::lint(
            &parsed.arena,
            source,
            LintOptions {
                expr_types: checked.expr_types,
                propagating_statements: checked.propagating_statements,
                ..LintOptions::default()
            },
        )
        .diagnostics
        .into_iter()
        .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferAtomicallyReplace))
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

    /// The one reason a diagnostic gives for offering no rewrite.
    fn difference(source: &str) -> String {
        let diagnostics = published_files(source);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert!(diagnostics[0].fix_hints.is_empty(), "{diagnostics:?}");
        let [note] = &diagnostics[0].notes[..] else {
            panic!("{diagnostics:?}");
        };
        note.clone()
    }

    #[test]
    fn a_publication_becomes_atomically_replace_around_what_produces_the_file() {
        let source = "proc publish(source: Path, cached: Path) [fs, error] {\n  cached.parent().mkdir()\n  let temporary = fp\"{cached}.tmp\"\n  temporary.remove(missing_ok: true)\n  defer temporary.remove(missing_ok: true)?\n\n  source.copy(to: temporary)\n  if temporary.read_text()? == \"\" {\n    return error.fail(\"empty\")\n  }\n\n  temporary.rename(to: cached, overwrite: true)\n}\n";
        let diagnostics = published_files(source);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        let fixed = apply(&diagnostics, source);
        assert_eq!(
            fixed,
            "proc publish(source: Path, cached: Path) [fs, error] {\n  cached.parent().mkdir()\n  atomically replace cached as temporary {\n    source.copy(to: temporary)\n    if temporary.read_text()? == \"\" {\n      return error.fail(\"empty\")\n    }\n  }\n}\n"
        );
        // The fixed program checks, and its expansion is not reported again.
        assert!(published_files(&fixed).is_empty());
    }

    #[test]
    fn method_spellings_and_a_run_are_rewritten() {
        let source = "proc save(tag: Str, saved: Path) [fs, process, error] {\n  let partial = fp\"{saved}.partial\"\n  partial.remove(missing_ok: true)?\n  defer partial.remove(missing_ok: true)\n  run docker save --output $partial $tag\n  partial.rename(to: saved, overwrite: true)?\n}\n";
        let diagnostics = published_files(source);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(
            apply(&diagnostics, source),
            "proc save(tag: Str, saved: Path) [fs, process, error] {\n  atomically replace saved as partial {\n    run docker save --output $partial $tag\n  }\n}\n"
        );
    }

    /// The destination's directory is made between the binding and the
    /// removal, and stays before the statement. The path is the hidden
    /// sibling the form itself would choose.
    #[test]
    fn statements_before_the_removal_stay_before_the_statement() {
        let source = "proc publish(source: Path, dest: Path) [fs, error] {\n  let partial = fp\"{dest.parent}/.{dest.name}.tmp\"\n  dest.parent.mkdir()\n\n  print \"publishing\"\n  partial.remove(missing_ok: true)\n  defer partial.remove(missing_ok: true)?\n  source.copy(to: partial)\n\n  fs.fsync(partial)\n  partial.rename(to: dest, overwrite: true)\n}\n";
        let diagnostics = published_files(source);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        let fixed = apply(&diagnostics, source);
        assert_eq!(
            fixed,
            "proc publish(source: Path, dest: Path) [fs, error] {\n  dest.parent.mkdir()\n\n  print \"publishing\"\n  atomically replace dest as partial {\n    source.copy(to: partial)\n\n    fs.fsync(partial)\n  }\n}\n"
        );
        assert!(published_files(&fixed).is_empty());

        let early = source.replace("print \"publishing\"", "print $partial");
        assert!(difference(&early).contains("names the temporary path"));
    }

    /// A line that begins inside a multi-line string is part of its value.
    #[test]
    fn lines_inside_a_multi_line_string_are_not_indented() {
        let source = "proc publish(dest: Path) [fs, error] {\n  let partial = fp\"{dest}.tmp\"\n  partial.remove(missing_ok: true)\n  defer partial.remove(missing_ok: true)\n  partial.write(\"\"\"first\n  second\n\"\"\")\n  partial.rename(to: dest, overwrite: true)\n}\n";
        let diagnostics = published_files(source);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(
            apply(&diagnostics, source),
            "proc publish(dest: Path) [fs, error] {\n  atomically replace dest as partial {\n    partial.write(\"\"\"first\n  second\n\"\"\")\n  }\n}\n"
        );
    }

    #[test]
    fn a_sequence_that_differs_from_the_form_is_reported_without_a_rewrite() {
        let publish = |opening: &str, body: &str, closing: &str| {
            format!("proc publish(source: Path, dest: Path) [fs, process, error] {{\n{opening}{body}{closing}}}\n")
        };
        let opening = "  let partial = fp\"{dest}.tmp\"\n  partial.remove(missing_ok: true)\n  defer partial.remove(missing_ok: true)\n";
        let body = "  source.copy(to: partial)\n";
        let closing = "  partial.rename(to: dest, overwrite: true)\n";
        assert_eq!(published_files(&publish(opening, body, closing)).len(), 1);
        assert!(!published_files(&publish(opening, body, closing))[0].fix_hints.is_empty());

        for (source, reason) in [
            // No deferred removal: a failure used to leave the file behind.
            (
                publish("  let partial = fp\"{dest}.tmp\"\n  partial.remove(missing_ok: true)\n", body, closing),
                "two adjacent statements",
            ),
            // The rename fails when the destination exists.
            (
                publish(opening, body, "  partial.rename(to: dest)\n"),
                "does not pass `overwrite: true`",
            ),
            (
                publish(opening, body, "  partial.rename(to: dest, overwrite: true)\n  print \"published\"\n"),
                "statements follow the rename",
            ),
            // The temporary path is in another directory.
            (
                publish(&opening.replace("{dest}.tmp", "{dest}/../partial"), body, closing),
                "fp\"{DEST}SUFFIX\"",
            ),
            (
                publish(&opening.replace("fp\"{dest}.tmp\"", "fp\"{dest.parent()}/sub/partial\""), body, closing),
                "fp\"{DEST}SUFFIX\"",
            ),
            (publish(opening, "", closing), "nothing is produced"),
            // A statement between the binding and the removal shadows the
            // destination the rename reads.
            (
                publish(&opening.replace(".tmp\"\n", ".tmp\"\n  let dest = source\n"), body, closing),
                "rebinds it",
            ),
            // The body's defer used to run after the rename.
            (
                publish(opening, "  defer { print \"done\" }\n  source.copy(to: partial)\n", closing),
                "registers a defer or may own a handle",
            ),
            (
                publish(opening, "  let child = spawn run sleep 1 ?\n  print $child.pid\n  source.copy(to: partial)\n", closing),
                "registers a defer or may own a handle",
            ),
            (
                publish(opening, "  let dest = source\n  source.copy(to: partial)\n", closing),
                "rebinds it",
            ),
            (
                publish(opening, "  # copy first\n  source.copy(to: partial)\n", closing),
                "a comment sits among them",
            ),
            (
                publish(opening, body, "  # publish\n  partial.rename(to: dest, overwrite: true)\n"),
                "a comment sits among them",
            ),
            (
                publish(&opening.replace("true)\n  defer", "true) # stale\n  defer"), body, closing),
                "a comment sits among them",
            ),
        ] {
            let note = difference(&source);
            assert!(note.contains(reason), "{source}\n{note}");
        }

        // The rename ends a block whose value is its `Result`: a failure is
        // data there, and the form would propagate it.
        let captured = "proc publish(source: Path, dest: Path) [fs, error] -> Result[Bool] {\n  let outcome = try {\n    let partial = fp\"{dest}.tmp\"\n    partial.remove(missing_ok: true)\n    defer partial.remove(missing_ok: true)\n    source.copy(to: partial)\n    partial.rename(to: dest, overwrite: true)\n  }\n  outcome is Ok(Ok(_))\n}\n";
        assert!(difference(captured).contains("is the value of its block"));

        // A destination that can change between the binding and the rename.
        let reassigned = "proc publish(source: Path, first: Path) [fs, error] {\n  var dest = first\n  let partial = fp\"{dest}.tmp\"\n  partial.remove(missing_ok: true)\n  defer partial.remove(missing_ok: true)\n  source.copy(to: partial)\n  partial.rename(to: dest, overwrite: true)\n}\n";
        assert!(difference(reassigned).contains("not an immutable local"));

        // At the top level of a file the binding stays where it is.
        let top_level = "let dest = p\"/tmp/never/dest\"\nlet partial = fp\"{dest}.tmp\"\npartial.remove(missing_ok: true)\ndefer partial.remove(missing_ok: true)\npartial.write(\"x\")\npartial.rename(to: dest, overwrite: true)\n";
        assert!(difference(top_level).contains("statements of a file"));
    }

    #[test]
    fn a_body_that_always_leaves_is_reported() {
        let never = |source: &str| {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            Linter::lint(
                &parsed.arena,
                source,
                LintOptions {
                    expr_types: checked.expr_types,
                    definitely_exiting_block_spans: checked.definitely_exiting_block_spans,
                    ..LintOptions::default()
                },
            )
            .diagnostics
            .into_iter()
            .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintAtomicallyNeverReplaces))
            .collect::<Vec<_>>()
        };
        let publish = |body: &str| {
            format!("proc publish(dest: Path, ready: Bool) [fs, error] {{\n  for _ in [1] {{\n    atomically replace dest as partial {{\n      partial.write(\"x\")\n{body}    }}\n  }}\n}}\n")
        };
        for body in [
            "      return\n",
            "      break\n",
            "      continue\n",
            "      if ready {\n        return\n      } else {\n        continue\n      }\n",
        ] {
            let source = publish(body);
            let diagnostics = never(&source);
            assert_eq!(diagnostics.len(), 1, "{source}\n{diagnostics:?}");
            let label = diagnostics[0].labels[0].span;
            assert_eq!(&source[label.range()], "atomically replace dest as partial ");
            assert!(diagnostics[0].fix_hints.is_empty());
        }
        // A path that reaches the end of the body publishes.
        for body in ["", "      return when ready\n", "      if ready {\n        break\n      }\n"] {
            assert!(never(&publish(body)).is_empty(), "{body}");
        }
    }

    #[test]
    fn other_renames_and_removals_are_not_reported() {
        for source in [
            // Nothing clears the path first.
            "proc publish(source: Path, dest: Path) [fs, error] {\n  let partial = fp\"{dest}.tmp\"\n  source.copy(to: partial)\n  partial.rename(to: dest, overwrite: true)\n}\n",
            // The renamed path is not a local of this block.
            "proc rotate(current: Path, older: Path) [fs, error] {\n  older.remove(missing_ok: true)\n  current.rename(to: older)\n}\n",
            // The path is not an immutable binding of this block.
            "proc publish(source: Path, dest: Path) [fs, error] {\n  var partial = fp\"{dest}.tmp\"\n  partial = fp\"{dest}.new\"\n  partial.remove(missing_ok: true)\n  source.copy(to: partial)\n  partial.rename(to: dest, overwrite: true)\n}\n",
        ] {
            assert!(published_files(source).is_empty(), "{source}");
        }
    }

    // A removal that leaves `missing_ok` out accepts a missing path, as the
    // statement's own removals do.
    #[test]
    fn a_removal_without_missing_ok_is_the_same_removal() {
        let source = "proc publish(dest: Path) [fs, error] {\n  let partial = fp\"{dest}.tmp\"\n  partial.remove()\n  defer partial.remove()\n  partial.write(\"text\")\n  partial.rename(dest, overwrite: true)\n}\n";
        assert_eq!(published_files(source).len(), 1, "{source}");
    }

    // Each call may still be written through the `fs` module.
    #[test]
    fn the_function_spelling_is_the_same_sequence() {
        let source = "proc publish(source: Path, dest: Path) [fs, error] {\n  let partial = fp\"{dest}.tmp\"\n  fs.remove(partial, missing_ok: true)\n  defer fs.remove(partial, missing_ok: true)\n  fs.copy(source, partial)\n  fs.rename(partial, dest, overwrite: true)\n}\n";
        let diagnostics = published_files(source);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(
            apply(&diagnostics, source),
            "proc publish(source: Path, dest: Path) [fs, error] {\n  atomically replace dest as partial {\n    fs.copy(source, partial)\n  }\n}\n"
        );
    }
}
