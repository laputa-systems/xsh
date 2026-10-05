use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use xsh::frontend::check::{Checker, Type};
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{
    ArenaAssignTargetKind, ArenaBindingTargetKind, ArenaCommand, ArenaCommandArg,
    ArenaCommandArgKind, ArenaExprKind, ArenaExprOrRun, ArenaStmtKind, ArenaWordPart, ExprId,
    StmtId,
};
use xsh::frontend::syntax::node::{AssignOp, CoreCommand, RunKind};
use xsh::frontend::syntax::parser::Parser;

/// A single propagated text capture may become the scope tail only when nothing
/// can observe the placeholder before restoration. Keep its mutable binding and
/// the body's propagation destination; the added outer `?` consumes entry errors.
pub(super) fn lint_command_scope_scaffolds(linter: &mut super::Linter<'_>, statements: &[StmtId]) {
    if !linter.arena.signal_hooks.is_empty() {
        return;
    }
    for pair in statements.windows(2) {
        let declaration = linter.arena.stmt(pair[0]);
        let ArenaStmtKind::Var {
            target,
            initializer: ArenaExprOrRun::Expr(initial),
            ..
        } = declaration.kind
        else {
            continue;
        };
        let ArenaBindingTargetKind::Name(name) = linter.arena.binding_target(target).kind else {
            continue;
        };
        if !matches!(linter.arena.expr(initial).kind, ArenaExprKind::Str(_))
            || linter.expr_types.get(&linter.arena.expr(initial).span) != Some(&Type::Str)
        {
            continue;
        }
        let statement = linter.arena.stmt(pair[1]);
        let ArenaStmtKind::Command(command) = statement.kind else {
            continue;
        };
        let command = linter.arena.command_stmt(command);
        let ArenaCommand::Core {
            name: CoreCommand::Cd,
            args,
            env,
            block: Some(block),
        } = command.command
        else {
            continue;
        };
        if !env.is_empty() {
            continue;
        }
        let [argument] = linter.arena.command_args(args) else {
            continue;
        };
        let Some(input) = path_input(linter, argument) else {
            continue;
        };
        if super::expr_references_name(linter.arena, input, name) {
            continue;
        }
        let body = linter
            .arena
            .stmt_ids(linter.arena.block(block).statements)
            .collect::<Vec<_>>();
        let [assignment] = body.as_slice() else {
            continue;
        };
        let assignment = linter.arena.stmt(*assignment);
        let ArenaStmtKind::Assign {
            target,
            op: AssignOp::Set,
            value: ArenaExprOrRun::Run(run),
        } = assignment.kind
        else {
            continue;
        };
        if !matches!(linter.arena.assign_target(target).kind, ArenaAssignTargetKind::Name(found) if found == name)
        {
            continue;
        }
        let run = linter.arena.run_form(run);
        let [segment] = linter.arena.run_segments(run.segments) else {
            continue;
        };
        // An unconsumed capture returns Result[Str] and would add another Result
        // layer. Streams and handles also have different lifetime obligations.
        if !run.propagate
            || segment.kind != RunKind::CaptureText
            || segment.timeout.is_some()
            || segment.cpu_max.is_some()
            || segment.accept.is_some()
            || !segment.env.is_empty()
            || !segment.redirections.is_empty()
            || segment.grouped
        {
            continue;
        }
        if !simple_argument(linter, &segment.target)
            || super::command_arg_references_name(linter.arena, &segment.target, name)
            || !linter
                .arena
                .command_args(segment.args)
                .iter()
                .all(|argument| {
                    simple_argument(linter, argument)
                        && !super::command_arg_references_name(linter.arena, argument, name)
                })
        {
            continue;
        }
        let edit = Span::new(
            declaration.span.source_id,
            declaration.span.start(),
            statement.span.end(),
        );
        let Some(original) = linter.source.get(edit.range()) else {
            continue;
        };
        if original.contains('#') || linter.source.contains("defer") {
            continue;
        }
        let input = match linter.arena.expr(input).kind {
            ArenaExprKind::Ident(name) => name.to_string(),
            ArenaExprKind::PathStr(_) => {
                let Some(text) = linter.source.get(linter.arena.expr(input).span.range()) else {
                    continue;
                };
                text.to_owned()
            }
            _ => continue,
        };
        let run = linter.arena.span(run.span);
        let Some(prefix) = linter
            .source
            .get(declaration.span.start()..linter.arena.expr(initial).span.start())
        else {
            continue;
        };
        let Some(run) = linter.source.get(run.range()) else {
            continue;
        };
        let suffix = if original.ends_with('\n') {
            "\n"
        } else if original.ends_with(';') {
            ";"
        } else {
            ""
        };
        // The capture propagates as the scope's tail. A statement cd already
        // propagates entry failures; the outer `?` keeps that same destination.
        let replacement = format!("{prefix}cd ({input}) {{ {run} }}?{suffix}");
        let mut candidate = linter.source.to_owned();
        candidate.replace_range(edit.range(), &replacement);
        let parsed = Parser::parse_source_arena_only(edit.source_id, &candidate);
        if !parsed.diagnostics.is_empty()
            || !Checker::check_arena(&parsed.arena, &candidate)
                .diagnostics
                .is_empty()
        {
            continue;
        }
        linter.diagnostics.push(
            Diagnostic::warning("a fresh text placeholder can consume the scope value")
                .with_code(DiagnosticCode::LintPreferContextScopeValue)
                .with_label(Label::secondary(
                    edit,
                    "retain the capture and initialize after restoring the context",
                ))
                .with_fix_hint(FixHint::replacement(
                    edit,
                    "consume the propagated text capture as the scope tail",
                    replacement,
                )),
        );
    }
}

fn path_input(linter: &super::Linter<'_>, argument: &ArenaCommandArg) -> Option<ExprId> {
    let input = match argument.kind {
        ArenaCommandArgKind::Typed(input) => input,
        ArenaCommandArgKind::Word(parts) => {
            let mut parts = linter.arena.word_parts(parts);
            let (ArenaWordPart::Shorthand(input) | ArenaWordPart::Interpolation(input)) =
                parts.next()?
            else {
                return None;
            };
            if parts.next().is_some() {
                return None;
            }
            input
        }
        _ => return None,
    };
    (matches!(
        linter.arena.expr(input).kind,
        ArenaExprKind::Ident(_) | ArenaExprKind::PathStr(_)
    ) && linter.expr_types.get(&linter.arena.expr(input).span) == Some(&Type::Path))
    .then_some(input)
}

fn simple_argument(linter: &super::Linter<'_>, argument: &ArenaCommandArg) -> bool {
    let scalar = |expr| {
        matches!(
            linter.arena.expr(expr).kind,
            ArenaExprKind::Ident(_)
                | ArenaExprKind::Int(_)
                | ArenaExprKind::Str(_)
                | ArenaExprKind::Bool(_)
                | ArenaExprKind::PathStr(_)
        ) && matches!(
            linter.expr_types.get(&linter.arena.expr(expr).span),
            Some(Type::Int | Type::Str | Type::Bool | Type::Path)
        )
    };
    match argument.kind {
        ArenaCommandArgKind::Word(parts) => linter.arena.word_parts(parts).all(|part| match part {
            ArenaWordPart::Bare(_) | ArenaWordPart::Quoted(_) => true,
            ArenaWordPart::Shorthand(expr) | ArenaWordPart::Interpolation(expr) => scalar(expr),
        }),
        ArenaCommandArgKind::Typed(expr) => scalar(expr),
        _ => false,
    }
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
                ..LintOptions::default()
            },
        )
        .diagnostics
    }

    #[test]
    fn command_scope_run_text_initializer_preserves_mutability_and_converges() {
        let source = "proc inspect(repo: Path) [env, process, error] -> Result[Str] {\n  var revision = \"\"\n  cd $repo {\n    revision = run.text git rev-parse HEAD ?\n  }\n  revision = revision.trim()\n  revision\n}\n";
        let output = diagnostics(source);
        let diagnostic = output
            .iter()
            .find(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferContextScopeValue))
            .expect("command scope initializer migration");
        assert_eq!(diagnostic.fix_hints.len(), 1);
        let fix = &diagnostic.fix_hints[0];
        let mut fixed = source.to_owned();
        fixed.replace_range(fix.span.unwrap().range(), fix.replacement.as_ref().unwrap());
        assert!(
            fixed.contains("var revision = cd (repo) { run.text git rev-parse HEAD }?"),
            "{fixed}"
        );
        assert!(fixed.contains("revision = revision.trim()"));
        assert!(
            !diagnostics(&fixed)
                .iter()
                .any(|diagnostic| diagnostic.code
                    == Some(DiagnosticCode::LintPreferContextScopeValue))
        );
    }
    fn rewrite(source: &str) -> String {
        let output = diagnostics(source);
        let fixes = output
            .iter()
            .filter(|diagnostic| {
                diagnostic.code == Some(DiagnosticCode::LintPreferContextScopeValue)
            })
            .flat_map(|diagnostic| &diagnostic.fix_hints)
            .collect::<Vec<_>>();
        assert_eq!(fixes.len(), 1, "{output:?}");
        let mut fixed = source.to_owned();
        fixed.replace_range(
            fixes[0].span.unwrap().range(),
            fixes[0].replacement.as_deref().unwrap(),
        );
        fixed
    }

    #[test]
    fn command_scope_run_text_initializer_retains_written_local_type() {
        let source = "proc inspect(repo: Path) [env, process, error] -> Result[Str] { var revision: Str = \"\"; cd $repo { revision = run.text /bin/pwd ? }; revision = revision.trim(); revision }\n";
        let fixed = rewrite(source);
        assert!(fixed.contains("var revision: Str = cd (repo)"), "{fixed}");
        assert!(fixed.contains("revision = revision.trim()"));
        assert!(
            !diagnostics(&fixed)
                .iter()
                .any(|diagnostic| diagnostic.code
                    == Some(DiagnosticCode::LintPreferContextScopeValue))
        );
    }

    #[test]
    fn command_scope_run_text_initializer_refuses_observed_placeholders_and_cleanup() {
        for source in [
            "proc inspect(repo: Path) [env, process, error] -> Result[Str] { var revision = \"\"; cd $repo { revision = run.text /usr/bin/printf $revision ? }; revision }\n",
            "proc inspect(repo: Path) [env, process, error] -> Result[Str] { var revision = \"\"; defer { print $revision }; cd $repo { revision = run.text /bin/pwd ? }; revision }\n",
            "on TERM [io] { print signal }\nproc inspect(repo: Path) [env, process, error] -> Result[Str] { var revision = \"\"; cd $repo { revision = run.text /bin/pwd ? }; revision }\n",
            "proc inspect(repo: Path) [env, process, error] -> Result[Str] { var revision = \"\"; cd $repo { print before; revision = run.text /bin/pwd ? }; revision }\n",
            "proc inspect(repo: Path) [env, process, error] -> Result[Str] { var revision = \"\"; cd $repo { # preserve assignment timing\n revision = run.text /bin/pwd ? }; revision }\n",
            "proc inspect(repo: Path) [env, process, error] -> Result[Str] { var revision = \"\"; let observed = revision; cd $repo { revision = run.text /bin/pwd ? }; revision }\n",
            "proc inspect(repo: Path) [env, process, error] -> Result[Result[Str]] { var revision: Result[Str] = Ok(\"\"); cd $repo { revision = try run.text /bin/pwd }; Ok(revision) }\n",
        ] {
            let output = diagnostics(source);
            assert!(
                output
                    .iter()
                    .filter(|diagnostic| diagnostic.code
                        == Some(DiagnosticCode::LintPreferContextScopeValue))
                    .all(|diagnostic| diagnostic.fix_hints.is_empty()),
                "{source}"
            );
        }
    }

    #[test]
    fn command_scope_run_text_initializer_preserves_process_results_and_restoration() {
        use std::fs;
        use xsh::execution::script::{RunOptions, run_script};

        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("work tree");
        fs::create_dir(&directory).unwrap();
        let directory = fs::canonicalize(directory).unwrap();
        let outer = root.path().join("outer");
        fs::create_dir(&outer).unwrap();
        let outer = fs::canonicalize(outer).unwrap();
        let missing = fs::canonicalize(root.path()).unwrap().join("missing");
        for (case, command, target, nested, failed) in [
            ("success", "/bin/pwd", &directory, "", false),
            ("entry-failure", "/bin/pwd", &missing, "", true),
            ("process-failure", "/bin/false", &directory, "", true),
            ("nested-success", "/bin/pwd", &directory, "cwd", false),
            ("nested-failure", "/bin/false", &directory, "cwd", true),
            (
                "env-success",
                "/bin/sh -c r\"\"\"printf '%s' \"$XSH_SCOPE_CAPTURE_SENTINEL\" \"\"\"",
                &directory,
                "env",
                false,
            ),
            ("env-failure", "/bin/false", &directory, "env", true),
        ] {
            let invocation = if nested == "cwd" {
                "cd (outer) { let result = inspect(repo); assert fs.cwd()? == outer, \"nested restoration\"; result }?"
            } else if nested == "env" {
                "env ({XSH_SCOPE_CAPTURE_SENTINEL: \"scoped\"}) { inspect(repo) }?"
            } else {
                "inspect(repo)"
            };
            let source = format!(
                "proc inspect(repo: Path) [env, process, error] -> Result[Str] {{\n  var revision = \"\"\n  cd $repo {{ revision = run.text {command} ? }}\n  revision = revision.trim()\n  revision\n}}\nlet repo = Path(args[0])\nlet outer = Path(args[1])\nlet before = fs.cwd()?\nlet env_before = env.get_or(\"XSH_SCOPE_CAPTURE_SENTINEL\", \"absent\")?\nlet outcome = {invocation}\nprint ${{outcome is Err(_)}}\nprint ${{fs.cwd()? == before}}\nprint ${{env.get_or(\"XSH_SCOPE_CAPTURE_SENTINEL\", \"absent\")? == env_before}}\nmatch outcome {{ Ok(value) => print $value; Err(_) => print failed }}\n"
            );
            let fixed = rewrite(&source);
            let run = |version: &str, text: &str| {
                let path = root.path().join(format!("{case}-{version}.xsh"));
                fs::write(&path, text).unwrap();
                run_script(RunOptions {
                    script: path.to_string_lossy().into_owned(),
                    args: vec![
                        target.to_string_lossy().into_owned(),
                        outer.to_string_lossy().into_owned(),
                    ],
                    coverage_trace_dir: None,
                })
            };
            let before = run("before", &source);
            let after = run("after", &fixed);
            assert_eq!(
                before.status,
                0,
                "{case}: {}",
                String::from_utf8_lossy(&before.stderr)
            );
            assert_eq!(
                after.status,
                before.status,
                "{case}: {}",
                String::from_utf8_lossy(&after.stderr)
            );
            assert_eq!(after.stdout, before.stdout, "{case}");
            assert_eq!(after.stderr, before.stderr, "{case}");
            let value = if failed {
                "failed".to_owned()
            } else if nested == "env" {
                "scoped".to_owned()
            } else {
                target.to_string_lossy().into_owned()
            };
            assert_eq!(
                before.stdout,
                format!("{failed}\ntrue\ntrue\n{value}\n").into_bytes(),
                "{case}"
            );
        }
    }
}
