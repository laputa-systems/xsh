use xsh::diagnostic::{Diagnostic, DiagnosticCode, Label, Severity};
use xsh::frontend::source::Span;
use xsh::frontend::symbols::Name;
use xsh::frontend::syntax::arena::{
    ArenaBindingTargetKind, ArenaCallArgKind, ArenaExprKind, ArenaExprOrRun, ArenaStmtKind,
    AstArena, DeferTrigger, ExprId, StmtId,
};

/// A handle opened by `let NAME = OPEN?` whose release is deferred on the
/// next line lives until its block ends:
///
/// ```text
/// let root = fs.open_root(dir)?
/// defer root.close()
/// ```
///
/// `with root = fs.open_root(dir)? { ... }` says the same in one form, over
/// the rest of the block. It is a note with no rewrite: a release that fails
/// after the block reached its end leaves the enclosing function from a
/// `defer` and is the `Err` of the scope, and many programs show the `defer`
/// shape on purpose.
///
/// The handle has to stay in its block. It may be a receiver (`root.read(..)`)
/// or a call argument (`check(root)`); any other spelling of its name in the
/// rest of the block (a `return`, a tail value, an assignment, a field of a
/// record) may hand it on, and such a site is left alone.
pub(super) fn lint_deferred_releases(linter: &mut super::Linter<'_>, stmts: &[StmtId]) {
    let arena = linter.arena;
    for index in 0..stmts.len().saturating_sub(1) {
        let Some((name, open)) = opened_handle(arena, stmts[index]) else {
            continue;
        };
        let Some(release) = deferred_release(arena, stmts[index + 1], name) else {
            continue;
        };
        let rest = &stmts[index + 2..];
        if let (Some(&first), Some(&last)) = (rest.first(), rest.last()) {
            let body = Span::new(
                arena.stmt(first).span.source_id,
                arena.stmt(first).span.start(),
                arena.stmt(last).span.end(),
            );
            let Some(text) = linter.source.get(body.range()) else {
                continue;
            };
            if !stays_in_block(text, name.as_str().as_str()) {
                continue;
            }
        }
        let name = name.as_str();
        linter.diagnostics.push(
            Diagnostic::new(
                Severity::Note,
                format!("`{name}` is opened and its release deferred on the next line"),
            )
            .with_code(DiagnosticCode::LintPreferWithScope)
            .with_label(Label::primary(
                arena.expr(open).span,
                "the resource a `with` scope would bind",
            ))
            .with_label(Label::secondary(
                arena.stmt(stmts[index + 1]).span,
                format!("and release, as {release}, when its block ends"),
            ))
            .with_note(format!(
                "`with {name} = ... {{ ... }}` over the rest of the block binds and releases it in one form, but is not the same program: a release that fails after the block finished is the scope's `Err` instead of a failure raised from the `defer`"
            )),
        );
    }
}

/// `let NAME = OPEN?`: an immutable, unannotated binding of one name to a
/// propagated value.
fn opened_handle(arena: &AstArena, stmt: StmtId) -> Option<(Name, ExprId)> {
    let ArenaStmtKind::Let {
        target,
        ty: None,
        initializer: ArenaExprOrRun::Expr(open),
    } = arena.stmt(stmt).kind
    else {
        return None;
    };
    let ArenaBindingTargetKind::Name(name) = arena.binding_target(target).kind else {
        return None;
    };
    (name.as_str() != "_" && matches!(arena.expr(open).kind, ArenaExprKind::Try(_)))
        .then_some((name, open))
}

/// `defer NAME.close()` or `defer fs.unlock(NAME)`, with or without `?`,
/// as the spelling of the release.
fn deferred_release(arena: &AstArena, stmt: StmtId, name: Name) -> Option<&'static str> {
    let ArenaStmtKind::Defer(ArenaExprOrRun::Expr(action), DeferTrigger::Exit) =
        arena.stmt(stmt).kind
    else {
        return None;
    };
    let call = match arena.expr(action).kind {
        ArenaExprKind::Try(call) => call,
        _ => action,
    };
    let ArenaExprKind::Call { callee, args } = arena.expr(call).kind else {
        return None;
    };
    let ArenaExprKind::Field { base, name: member } = arena.expr(callee).kind else {
        return None;
    };
    let is_name = |expr: ExprId| matches!(arena.expr(expr).kind, ArenaExprKind::Ident(found) if found == name);
    let args = arena.call_args(args);
    if member.as_str() == "close" && args.is_empty() && is_name(base) {
        return Some("`close()`");
    }
    let unlocks = member.as_str() == "unlock"
        && matches!(arena.expr(base).kind, ArenaExprKind::Ident(module) if module == "fs")
        && matches!(args, [arg] if matches!(arg.kind, ArenaCallArgKind::Positional(value) if is_name(value)));
    unlocks.then_some("`fs.unlock`")
}

/// Whether every spelling of `name` in `text` is a receiver or one whole call
/// argument. The test reads the characters around the word, past any spaces
/// and line breaks, so it does not depend on how the code is laid out; a
/// spelling in a comment or a string counts as a use it cannot read.
fn stays_in_block(text: &str, name: &str) -> bool {
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    text.match_indices(name).all(|(at, _)| {
        let before = &text[..at];
        let after = &text[at + name.len()..];
        if before.chars().next_back().is_some_and(is_word)
            || after.chars().next().is_some_and(is_word)
        {
            // Part of a longer word: another name.
            return true;
        }
        let previous = before.trim_end().chars().next_back();
        let next = after.trim_start().chars().next();
        next == Some('.')
            || (matches!(previous, Some('(' | ',')) && matches!(next, Some(')' | ',')))
    })
}

#[cfg(test)]
mod tests {
    use super::super::{LintOptions, Linter};
    use xsh::diagnostic::{Diagnostic, DiagnosticCode, Severity};
    use xsh::frontend::source::SourceId;
    use xsh::frontend::syntax::parser::Parser;

    fn deferred_releases(source: &str) -> Vec<Diagnostic> {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let options = LintOptions {
            prefer_with_scope: true,
            ..LintOptions::default()
        };
        Linter::lint(&parsed.arena, source, options)
            .diagnostics
            .into_iter()
            .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferWithScope))
            .collect()
    }

    #[test]
    fn an_opened_handle_with_its_deferred_release_is_noted_without_a_rewrite() {
        let source = "proc check(dir: Path) [fs, error] -> Result[Bool] {\n  let root = fs.open_root(dir)?\n  defer root.close()\n  inspect(root)?\n  root.exists(p\"data\")\n}\n";
        let diagnostics = deferred_releases(source);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].severity, Severity::Note);
        assert_eq!(
            diagnostics[0].message,
            "`root` is opened and its release deferred on the next line"
        );
        assert_eq!(
            &source[diagnostics[0].labels[0].span.range()],
            "fs.open_root(dir)?"
        );
        assert!(diagnostics[0].notes[0].contains("with root = ... { ... }"));
        assert!(diagnostics[0].fix_hints.is_empty());
    }

    #[test]
    fn the_note_is_opt_in() {
        let source = "let root = fs.open_root(p\".\")?\ndefer root.close()\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        let diagnostics = Linter::lint(&parsed.arena, source, LintOptions::default()).diagnostics;
        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code != Some(DiagnosticCode::LintPreferWithScope)),
            "{diagnostics:?}"
        );
        assert_eq!(deferred_releases(source).len(), 1);
    }

    #[test]
    fn a_lock_and_a_propagated_release_at_the_top_of_a_file_count() {
        for source in [
            "let held = fs.lock(p\"state.lock\")?\ndefer fs.unlock(held)\nprint \"locked\"\n",
            "let held = fs.lock(p\"state.lock\")?\ndefer fs.unlock(held)?\n",
            "let scratch = fs.tempdir()?\ndefer scratch.close()?\nscratch.write(\n  p\"a\",\n  \"b\",\n)\n",
        ] {
            assert_eq!(deferred_releases(source).len(), 1, "{source}");
        }
    }

    #[test]
    fn the_finding_does_not_depend_on_layout() {
        let flat = "let root = fs.open_root(p\".\")?\ndefer root.close()\nuse_root(1, root)?\n";
        let broken =
            "let root = fs.open_root(p\".\")?\ndefer root.close()\nuse_root(\n  1,\n  root\n)?\n";
        assert_eq!(deferred_releases(flat).len(), 1);
        assert_eq!(deferred_releases(broken).len(), 1);
    }

    #[test]
    fn a_handle_that_may_leave_its_block_and_other_shapes_are_left_alone() {
        for source in [
            // Returned, the block's value, stored, or assigned.
            "proc open(dir: Path) [fs, error] -> Result[FsRoot] {\n  let root = fs.open_root(dir)?\n  defer root.close()\n  return root\n}\n",
            "proc open(dir: Path) [fs, error] -> Result[FsRoot] {\n  let root = fs.open_root(dir)?\n  defer root.close()\n  root\n}\n",
            "let root = fs.open_root(p\".\")?\ndefer root.close()\nlet kept = {root: root}\n",
            "var kept = null\nlet root = fs.open_root(p\".\")?\ndefer root.close()\nkept = root\n",
            // The release is not the next statement, or runs only on failure.
            "let root = fs.open_root(p\".\")?\nprint \"opened\"\ndefer root.close()\n",
            "let root = fs.open_root(p\".\")?\nerrdefer root.close()\n",
            // Another handle's release, a block, and a binding that is not a
            // propagated value.
            "let root = fs.open_root(p\".\")?\ndefer other.close()\n",
            "let root = fs.open_root(p\".\")?\ndefer {\n  root.close()\n}\n",
            "let root = opened\ndefer root.close()\n",
            "var root = fs.open_root(p\".\")?\ndefer root.close()\n",
        ] {
            assert!(deferred_releases(source).is_empty(), "{source}");
        }
    }
}
