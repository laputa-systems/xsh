//! `lint.prefer-rel-path`: a path handed to a rooted operation
//! (`root.write(rel, data)`) whose type is a plain `Path` may be absolute or
//! climb out with `..`, and says so only when the operation refuses it. Typed
//! `RelPath`, the same path is known to be neither.
//!
//! Which parameters name a path beneath the root is read from the registry:
//! the `path` parameter of an `FsRoot` method. A literal without
//! interpolation is left alone, because its bytes are in view. The lint
//! offers no fix, because the type has to come from somewhere: the binding's
//! annotation, the parameter's type and every caller, or a
//! `.require(RelPath)?` whose failure the author has to place.

use std::collections::BTreeMap;
use xsh::diagnostic::{Diagnostic, DiagnosticCode, Label};
use xsh::frontend::check::Type;
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{ArenaCallArgKind, ArenaExprKind, AstArena, ExprId};
use xsh_registry::signature::{MethodReceiver, api_spec};

/// The name every `FsRoot` method gives the path it resolves beneath the root.
const ROOTED_PARAMETER: &str = "path";

pub(super) fn unvalidated_rooted_path(
    arena: &AstArena,
    expr_types: &BTreeMap<Span, Type>,
    expr: ExprId,
) -> Option<Diagnostic> {
    let ArenaExprKind::Call { callee, args } = arena.expr(expr).kind else {
        return None;
    };
    let ArenaExprKind::Field { base, name } = arena.expr(callee).kind else {
        return None;
    };
    if expr_types.get(&arena.expr(base).span) != Some(&Type::FsRoot) {
        return None;
    }
    // Every overload of a rooted method puts the path in the same place.
    let position = api_spec()
        .methods
        .iter()
        .find(|entry| entry.receiver == MethodReceiver::FsRoot)?
        .methods
        .iter()
        .find(|method| name == method.name)?
        .overloads
        .first()?
        .sig
        .params
        .iter()
        .position(|param| param.name == ROOTED_PARAMETER)?;
    let args = arena.call_args(args);
    let named = args.iter().find_map(|arg| match arg.kind {
        ArenaCallArgKind::Named { name, value, .. } if name == ROOTED_PARAMETER => Some(value),
        _ => None,
    });
    let path = match named {
        Some(path) => path,
        None => {
            // A spread or a name before the position leaves it unknown which
            // argument is the path.
            let leading = args.get(..=position)?;
            if !leading
                .iter()
                .all(|arg| matches!(arg.kind, ArenaCallArgKind::Positional(_)))
            {
                return None;
            }
            match leading[position].kind {
                ArenaCallArgKind::Positional(path) => path,
                _ => return None,
            }
        }
    };
    let path = arena.expr(path);
    if matches!(path.kind, ArenaExprKind::Str(_) | ArenaExprKind::PathStr(_))
        || expr_types.get(&path.span) != Some(&Type::Path)
    {
        return None;
    }
    Some(
        Diagnostic::note(
            "this path may be absolute or leave the root, which fails only when it is resolved",
        )
        .with_code(DiagnosticCode::LintPreferRelPath)
        .with_label(Label::secondary(
            path.span,
            "this is a `Path`; a `RelPath` is known to stay beneath where it starts",
        ))
        .with_note(
            "declare the path `RelPath` where it is built, or validate it with `.require(RelPath)?`",
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::super::lint_redundant_propagation::tests::lint_rule;
    use xsh::diagnostic::{Diagnostic, DiagnosticCode};

    fn lint(source: &str) -> Vec<Diagnostic> {
        lint_rule(source, DiagnosticCode::LintPreferRelPath)
    }

    #[test]
    fn a_plain_path_handed_to_a_root_is_reported_without_a_fix() {
        let source = "proc stage(root: FsRoot, file: Path, link: Path) [fs, error] {\n  root.write(file, \"x\")\n  root.mkdir(fp\"{file}/sub\", parents: true)\n  root.symlink(target: link, path: file)\n  print root.exists(path: file.parent())?\n}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 4, "{diagnostics:?}");
        let reported = diagnostics
            .iter()
            .map(|diagnostic| &source[diagnostic.labels[0].span.range()])
            .collect::<Vec<_>>();
        assert_eq!(
            reported,
            ["file", "fp\"{file}/sub\"", "file", "file.parent()"]
        );
        for diagnostic in &diagnostics {
            assert!(diagnostic.fix_hints.is_empty(), "{diagnostic:?}");
            assert!(
                diagnostic.notes[0].contains("`.require(RelPath)?`"),
                "{diagnostic:?}"
            );
        }
    }

    // A RelPath, a literal, a symlink's target, and a path that reaches no
    // root are none of the rule's business.
    #[test]
    fn a_rel_path_a_literal_and_an_unrooted_path_are_left_alone() {
        let source = "proc stage(root: FsRoot, rel: RelPath, target: Path, file: Path) [fs, error] {\n  root.write(rel, \"x\")\n  root.mkdir(rel.parent(), parents: true)\n  root.write(\"etc/motd\", \"x\")\n  root.write(p\"etc/issue\", \"x\")\n  root.symlink(target: target, path: rel)\n  root.write(fp\"{rel}/more\", \"x\")\n  file.write(\"x\")\n  root.close()\n}\n";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(
            &source[diagnostics[0].labels[0].span.range()],
            "fp\"{rel}/more\""
        );
    }
}
