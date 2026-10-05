use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use xsh::frontend::check::{Checker, StatementPosition, Type};
use xsh::frontend::source::Span;
use xsh::frontend::symbols::Name;
use xsh::frontend::syntax::arena::{
    ArenaBindingTargetKind, ArenaExprKind, ArenaExprOrRun, ArenaExprTag, ArenaProgram,
    ArenaStmtKind, ArenaStmtTag, DeferTrigger, ExprId, StmtId,
};
use xsh::frontend::syntax::parser::Parser;

/// `let root = fs.tempdir()?`, `defer root.close()?`, and optionally
/// `let dir = root.host_path()?` open a directory that lives until the
/// enclosing block ends. When the handle is used for nothing but its path,
/// the rest of the block is exactly the body of `tempdir dir { ... }`: the
/// body's defers and owned handles still finish before the directory is
/// removed, and creation still fails the statement.
pub(super) fn lint_tempdir_scopes(linter: &mut super::Linter<'_>, program: &ArenaProgram) {
    if !linter.source.contains("fs.tempdir()") {
        return;
    }
    let arena = linter.arena;
    let statements = program.statement_ids().collect::<Vec<_>>();
    // A workspace lints each module against one arena holding every module.
    let Some(source_id) = statements
        .first()
        .map(|&stmt| arena.stmt(stmt).span.source_id)
    else {
        return;
    };
    let mut lists = vec![(statements, true)];
    for def in &arena.function_defs {
        let body = arena.span(arena.block(def.body).span);
        if body.source_id != source_id {
            continue;
        }
        // A body whose tail may be a value would return the scope's Result.
        if matches!(
            linter.checked_function_returns.get(&body),
            Some(Type::Unit | Type::Stream(_))
        ) || linter
            .checked_function_returns
            .get(&body)
            .is_some_and(Type::is_result_unit)
        {
            lists.push((arena.stmt_ids(arena.block(def.body).statements).collect(), false));
        }
    }
    for (index, tag) in arena.stmt_tags.iter().enumerate() {
        if !matches!(tag, ArenaStmtTag::For | ArenaStmtTag::While | ArenaStmtTag::Loop) {
            continue;
        }
        let statement = arena.stmt(StmtId::from_index(index));
        if statement.span.source_id != source_id {
            continue;
        }
        if let ArenaStmtKind::For { block, .. }
        | ArenaStmtKind::While { block, .. }
        | ArenaStmtKind::Loop { block } = statement.kind
        {
            lists.push((arena.stmt_ids(arena.block(block).statements).collect(), false));
        }
    }
    // Both sides are checked as standalone files: the workspace arena holds
    // other modules, which a reparsed candidate does not.
    let before = std::cell::LazyCell::new(|| {
        let parsed = Parser::parse_source_arena_only(source_id, linter.source);
        diagnostic_keys(&parsed.arena, linter.source)
    });
    let mut found = Vec::new();
    for (statements, script) in &lists {
        for index in 0..statements.len() {
            if let Some(site) = site(linter, statements, index, *script) {
                found.push(site);
            }
        }
    }
    for site in found {
        let fix = rewrite(linter, &site).filter(|(span, replacement)| {
            let mut candidate = linter.source.to_owned();
            candidate.replace_range(span.range(), replacement);
            let parsed = Parser::parse_source_arena_only(source_id, &candidate);
            parsed.diagnostics.is_empty() && diagnostic_keys(&parsed.arena, &candidate) == *before
        });
        let mut diagnostic = Diagnostic::warning(
            "this temporary directory is used only through its path; a `tempdir` scope binds the path and removes the directory",
        )
        .with_code(DiagnosticCode::LintPreferTempdirScope)
        .with_label(Label::primary(
            linter.arena.stmt(site.open).span,
            format!("`tempdir {} {{ ... }}` scopes the rest of this block", site.binder),
        ));
        if let Some((span, replacement)) = fix {
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                span,
                "move the rest of the block into a `tempdir` scope",
                replacement,
            ));
        }
        linter.diagnostics.push(diagnostic);
    }
}

struct Site {
    open: StmtId,
    /// The rest of the enclosing block, which becomes the scope body.
    rest: Vec<StmtId>,
    binder: Name,
    /// `root.host_path()?` expressions in the body that read the binder.
    path_reads: Vec<Span>,
}

fn site(
    linter: &super::Linter<'_>,
    statements: &[StmtId],
    index: usize,
    script: bool,
) -> Option<Site> {
    let arena = linter.arena;
    let ArenaStmtKind::Let {
        target,
        ty: None,
        initializer: ArenaExprOrRun::Expr(open),
    } = arena.stmt(statements[index]).kind
    else {
        return None;
    };
    let ArenaBindingTargetKind::Name(root) = arena.binding_target(target).kind else {
        return None;
    };
    let ArenaExprKind::Try(created) = arena.expr(open).kind else {
        return None;
    };
    if !is_call(arena, created, None, "tempdir")
        || !matches!(arena.expr(callee(arena, created)?).kind, ArenaExprKind::Field { base, .. }
            if matches!(arena.expr(base).kind, ArenaExprKind::Ident(module) if module == "fs"))
        || !matches!(
            linter.expr_types.get(&arena.expr(created).span),
            Some(Type::Result(ok, _)) if **ok == Type::FsRoot
        )
    {
        return None;
    }
    let ArenaStmtKind::Defer(ArenaExprOrRun::Expr(close), DeferTrigger::Exit) =
        arena.stmt(*statements.get(index + 1)?).kind
    else {
        return None;
    };
    if !matches!(arena.expr(close).kind, ArenaExprKind::Try(call) if is_call(arena, call, Some(root), "close"))
    {
        return None;
    }
    let (binder, start) = match statements.get(index + 2).map(|&stmt| arena.stmt(stmt).kind) {
        Some(ArenaStmtKind::Let {
            target,
            ty: None,
            initializer: ArenaExprOrRun::Expr(path),
        }) if path_read(arena, path, root) => match arena.binding_target(target).kind {
            ArenaBindingTargetKind::Name(binder) if binder.as_str() != "_" => (binder, index + 3),
            _ => return None,
        },
        _ => (root, index + 2),
    };
    let rest = statements.get(start..)?.to_vec();
    let (&first, &last) = (rest.first()?, rest.last()?);
    let body = Span::new(
        arena.stmt(first).span.source_id,
        arena.stmt(first).span.start(),
        arena.stmt(last).span.end(),
    );
    let inside = |span: Span| {
        span.source_id == body.source_id && body.start() <= span.start() && span.end() <= body.end()
    };
    // The handle may be read only for its path; any other use (or a later
    // binder, label, or comment spelling its name) keeps the long form.
    let spellings = word_count(linter.source.get(body.range())?, root.as_str().as_str());
    if binder != root && spellings != 0 {
        return None;
    }
    // Tags are read first: a workspace arena holds every module's rows.
    let path_reads = if spellings == 0 {
        Vec::new()
    } else {
        arena
            .expr_tags
            .iter()
            .enumerate()
            .filter(|(_, tag)| matches!(tag, ArenaExprTag::Try))
            .map(|(index, _)| ExprId::from_index(index))
            .filter(|&expr| inside(arena.expr(expr).span) && path_read(arena, expr, root))
            .map(|expr| arena.expr(expr).span)
            .collect::<Vec<_>>()
    };
    if spellings != path_reads.len() {
        return None;
    }
    // Declarations keep their module scope; a script's constants may be read
    // by its functions.
    for &stmt in &rest {
        if (script && matches!(arena.stmt(stmt).kind, ArenaStmtKind::Const { .. }))
            || matches!(
            arena.stmt(stmt).kind,
            ArenaStmtKind::Use(_)
                | ArenaStmtKind::Export(_)
                | ArenaStmtKind::TypeDef(_)
                | ArenaStmtKind::ErrorDef(_)
                | ArenaStmtKind::ProcDef(_)
                | ArenaStmtKind::CliMain(_)
                | ArenaStmtKind::PureDef(_)
                | ArenaStmtKind::StreamDef(_)
                | ArenaStmtKind::SignalHook(_)
        ) {
            return None;
        }
    }
    // The scope is a statement: the block's last statement must not be its value.
    let tail = arena.stmt(last);
    let statement_tail = match tail.kind {
        ArenaStmtKind::Expr(_)
        | ArenaStmtKind::Command(_)
        | ArenaStmtKind::If { .. }
        | ArenaStmtKind::Match { .. }
        | ArenaStmtKind::Loop { .. }
        | ArenaStmtKind::TailBareIdent(_) => {
            linter.statement_positions.get(&tail.span) == Some(&StatementPosition::Statement)
        }
        ArenaStmtKind::Return(_) | ArenaStmtKind::Break { .. } | ArenaStmtKind::Continue => false,
        _ => true,
    };
    if !statement_tail {
        return None;
    }
    // A script's top-level bindings must not be read from outside the moved
    // statements, such as by a function declared earlier.
    if script {
        let moved = rest
            .iter()
            .filter_map(|&stmt| match arena.stmt(stmt).kind {
                ArenaStmtKind::Let { target, .. } | ArenaStmtKind::Var { target, .. } => {
                    match arena.binding_target(target).kind {
                        ArenaBindingTargetKind::Name(name) => Some(name),
                        _ => None,
                    }
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        if arena
            .expr_tags
            .iter()
            .enumerate()
            .filter(|(_, tag)| matches!(tag, ArenaExprTag::Ident))
            .map(|(index, _)| ExprId::from_index(index))
            .any(|expr| {
            let span = arena.expr(expr).span;
            matches!(arena.expr(expr).kind, ArenaExprKind::Ident(name) if moved.contains(&name))
                && span.source_id == body.source_id
                && !inside(span)
        }) {
            return None;
        }
    }
    Some(Site {
        open: statements[index],
        rest,
        binder,
        path_reads,
    })
}

/// Open the scope, indent its body, and close it as one fix so another safe
/// edit cannot apply only part of the transformation.
fn rewrite(linter: &super::Linter<'_>, site: &Site) -> Option<(Span, String)> {
    let source = linter.source;
    let arena = linter.arena;
    let open = arena.stmt(site.open).span;
    let at = |start: usize, end: usize| Span::new(open.source_id, start, end);
    let line_start = source[..open.start()].rfind('\n').map_or(0, |at| at + 1);
    let indent = &source[line_start..open.start()];
    if !indent.chars().all(|c| c == ' ') {
        return None;
    }
    let first = arena.stmt(*site.rest.first()?).span;
    let body_start = source[..first.start()].rfind('\n').map_or(0, |at| at + 1);
    let last = arena.stmt(*site.rest.last()?).span.end();
    let body = source.get(body_start..last)?;
    let end = body_start + body.trim_end_matches(['\n', ';']).len();
    // Comments between the opening statements, and block strings whose lines
    // re-indentation would change, keep the long form.
    if body_start <= open.start()
        || source.get(open.start()..body_start)?.contains('#')
        || body.contains("\"\"\"")
    {
        return None;
    }
    let mut edits = vec![(
        at(open.start(), body_start),
        format!("tempdir {} {{\n", site.binder),
    )];
    let mut line = body_start;
    for text in source.get(body_start..end)?.split('\n') {
        if !text.trim().is_empty() {
            if !text.starts_with(indent) || site.path_reads.iter().any(|span| span.start() == line) {
                return None;
            }
            edits.push((at(line, line), "  ".to_string()));
        }
        line += text.len() + 1;
    }
    for span in &site.path_reads {
        edits.push((*span, site.binder.to_string()));
    }
    edits.push((at(end, end), format!("\n{indent}}}")));
    edits.sort_by_key(|(span, _)| (span.start(), span.end()));
    let replacement_end = arena.stmt(*site.rest.last()?).span.end();
    let mut replacement = source.get(open.start()..replacement_end)?.to_owned();
    for (span, text) in edits.iter().rev() {
        replacement.replace_range(
            (span.start() - open.start())..(span.end() - open.start()),
            text,
        );
    }
    Some((at(open.start(), replacement_end), replacement))
}

fn callee(arena: &xsh::frontend::syntax::arena::AstArena, call: ExprId) -> Option<ExprId> {
    match arena.expr(call).kind {
        ArenaExprKind::Call { callee, .. } => Some(callee),
        _ => None,
    }
}

/// `receiver.method()` with no arguments, where `receiver` is the name `root`
/// when given.
fn is_call(
    arena: &xsh::frontend::syntax::arena::AstArena,
    call: ExprId,
    root: Option<Name>,
    method: &str,
) -> bool {
    let ArenaExprKind::Call { callee, args } = arena.expr(call).kind else {
        return false;
    };
    let ArenaExprKind::Field { base, name } = arena.expr(callee).kind else {
        return false;
    };
    args.is_empty()
        && name.as_str() == method
        && root.is_none_or(|root| matches!(arena.expr(base).kind, ArenaExprKind::Ident(found) if found == root))
}

/// `root.host_path()?`.
fn path_read(arena: &xsh::frontend::syntax::arena::AstArena, expr: ExprId, root: Name) -> bool {
    matches!(arena.expr(expr).kind, ArenaExprKind::Try(call) if is_call(arena, call, Some(root), "host_path"))
}

fn word_count(text: &str, word: &str) -> usize {
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    text.match_indices(word)
        .filter(|(at, _)| {
            !text[..*at].chars().next_back().is_some_and(is_word)
                && !text[at + word.len()..].chars().next().is_some_and(is_word)
        })
        .count()
}

/// Checker diagnostics without positions, so a rewrite that moves text can
/// be compared with the original. A file that only checks with its imports
/// keeps the same unresolved-import diagnostics in both.
fn diagnostic_keys(program: &ArenaProgram, source: &str) -> Vec<String> {
    let mut keys = Checker::check_arena(program, source)
        .diagnostics
        .into_iter()
        .map(|diagnostic| format!("{:?} {}", diagnostic.code, diagnostic.message))
        .collect::<Vec<_>>();
    keys.sort();
    keys
}

#[cfg(test)]
mod tests {
    use crate::xsht::lint::{LintOptions, Linter};
    use xsh::diagnostic::{Diagnostic, DiagnosticCode};
    use xsh::frontend::check::Checker;
    use xsh::frontend::source::SourceId;
    use xsh::frontend::syntax::parser::Parser;

    fn diagnostics(source: &str) -> Vec<Diagnostic> {
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
        .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferTempdirScope))
        .collect()
    }

    fn fixed(source: &str) -> String {
        let output = diagnostics(source);
        assert_eq!(output.len(), 1, "{source}: {output:?}");
        assert_eq!(output[0].fix_hints.len(), 1, "fix must be atomic: {source}");
        let mut fixed = source.to_owned();
        let mut hints = output[0].fix_hints.iter().collect::<Vec<_>>();
        hints.sort_by_key(|hint| (hint.span.unwrap().start(), hint.span.unwrap().end()));
        for hint in hints.into_iter().rev() {
            fixed.replace_range(hint.span.unwrap().range(), hint.replacement.as_deref().unwrap());
        }
        fixed
    }

    #[test]
    fn path_only_temporary_directory_becomes_a_scope() {
        let source = "proc build() [fs, error] {\n  let scratch = fs.tempdir()?\n  defer scratch.close()?\n  let dir = scratch.host_path()?\n  fp\"{dir}/a\".write(\"1\")?\n\n  # the listing\n  print f\"{fs.files(dir)? |> count()}\"\n}\n\nbuild()?\n";
        let output = fixed(source);
        assert_eq!(
            output,
            "proc build() [fs, error] {\n  tempdir dir {\n    fp\"{dir}/a\".write(\"1\")?\n\n    # the listing\n    print f\"{fs.files(dir)? |> count()}\"\n  }\n}\n\nbuild()?\n"
        );
        assert!(diagnostics(&output).is_empty(), "{output}");
    }

    #[test]
    fn path_reads_of_the_handle_become_the_bound_path() {
        let source = "let scratch = fs.tempdir()?\ndefer scratch.close()?\nlet log = fp\"{scratch.host_path()?}/app.log\"\nlog.write(\"ok\\n\")?\nprint $log.name()\n";
        let output = fixed(source);
        assert_eq!(
            output,
            "tempdir scratch {\n  let log = fp\"{scratch}/app.log\"\n  log.write(\"ok\\n\")?\n  print $log.name()\n}\n"
        );
        assert!(diagnostics(&output).is_empty(), "{output}");
    }

    #[test]
    fn handles_used_beyond_their_path_keep_the_long_form() {
        for source in [
            // Rooted operations need the handle.
            "let root = fs.tempdir()?\ndefer root.close()?\nroot.write(p\"a\", \"1\")?\nprint ${root.host_path()?}\n",
            // The handle escapes into a helper.
            "proc touch(root: FsRoot) [fs, error] { root.write(p\"a\", \"1\")? }\nlet root = fs.tempdir()?\ndefer root.close()?\ntouch(root)?\n",
            // Work between creation and cleanup registration is not scoped.
            "let root = fs.tempdir()?\nprint \"made\"\ndefer root.close()?\nprint ${root.host_path()?}\n",
            // The block's tail is its value.
            "proc name() [fs, error] -> Result[Str] {\n  let root = fs.tempdir()?\n  defer root.close()?\n  let dir = root.host_path()?\n  dir.name()\n}\nprint ${name()?}\n",
            // Nothing reads the directory.
            "let root = fs.tempdir()?\ndefer root.close()?\n",
        ] {
            let output = diagnostics(source);
            assert!(output.is_empty(), "{source}: {output:?}");
        }
    }

    #[test]
    fn layouts_that_cannot_move_verbatim_report_without_a_fix() {
        for source in [
            "let root = fs.tempdir()?; defer root.close()?; let dir = root.host_path()?; print $dir\n",
            "let root = fs.tempdir()?\n# keep this note\ndefer root.close()?\nlet dir = root.host_path()?\nprint $dir\n",
            "let root = fs.tempdir()?\ndefer root.close()?\nlet dir = root.host_path()?\nlet text = \"\"\"\n  line\n  \"\"\"\nprint f\"{dir}{text}\"\n",
        ] {
            let output = diagnostics(source);
            assert_eq!(output.len(), 1, "{source}: {output:?}");
            assert!(output[0].fix_hints.is_empty(), "{source}");
        }
    }

    #[test]
    fn scope_rewrite_preserves_cleanup_order_failures_and_removal() {
        use std::fs;
        use xsh::execution::script::{RunOptions, run_script};

        let root = tempfile::tempdir().unwrap();
        for (case, body) in [
            ("success", "print \"body\""),
            ("failure", "fp\"{dir}/missing/file\".write(\"x\")?"),
            ("early-return", "return"),
        ] {
            let source = format!(
                "proc work() [fs, error] {{\n  defer {{ print \"outer cleanup\" }}\n  let scratch = fs.tempdir()?\n  defer scratch.close()?\n  let dir = scratch.host_path()?\n  defer {{ print f\"inner cleanup sees {{dir.exists()?}}\" }}\n  {body}\n  print \"after\"\n}}\nlet outcome = try {{ work()? }}\nprint ${{outcome is Ok(_)}}\n"
            );
            let output = fixed(&source);
            assert!(output.contains("  tempdir dir {\n"), "{output}");
            let run = |version: &str, text: &str| {
                let path = root.path().join(format!("{case}-{version}.xsh"));
                fs::write(&path, text).unwrap();
                run_script(RunOptions {
                    script: path.to_string_lossy().into_owned(),
                    args: Vec::new(),
                    coverage_trace_dir: None,
                })
            };
            let before = run("before", &source);
            let after = run("after", &output);
            assert_eq!(after.status, before.status, "{case}");
            assert_eq!(
                String::from_utf8_lossy(&after.stdout),
                String::from_utf8_lossy(&before.stdout),
                "{case}"
            );
            assert!(
                String::from_utf8_lossy(&before.stdout).contains("inner cleanup sees true\nouter cleanup\n"),
                "{case}: {}",
                String::from_utf8_lossy(&before.stdout)
            );
        }
    }
}
