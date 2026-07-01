#![allow(clippy::single_call_fn)]

use super::{BTreeMap, BuilderKind, Checker, FxHashSet, Name, Span, Type};
use crate::syntax::arena::{
    ArenaBuilderBlock, ArenaBuilderEntryKind, ArenaCallArg, ArenaExprKind, ArenaProgram,
    BuilderBlockId, ExprId,
};

#[allow(dead_code)]
impl Checker {
    pub(super) fn check_builder_call_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        call: ExprId,
        block: BuilderBlockId,
        span: Span,
    ) -> Type {
        let Some((module, name, args)) = checker_builder_call_parts_arena(arena, call) else {
            self.error(
                span,
                "builder blocks require a module call",
                "check.builder-call",
            );
            return Type::Unknown;
        };
        match (module.as_str(), name.as_str()) {
            ("process", "command") => {
                if !args.is_empty() {
                    self.error(
                        span,
                        "process.command accepts no call arguments",
                        "check.arity",
                    );
                }
                self.check_builder_block_arena(
                    arena,
                    source,
                    arena.arena.builder_block(block),
                    BuilderKind::ProcessCommand,
                );
                Type::Command
            }
            _ => {
                self.error(
                    span,
                    "module API does not accept a builder block",
                    "check.builder-call",
                );
                Type::Unknown
            }
        }
    }

    pub(super) fn check_builder_block_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        block: &ArenaBuilderBlock,
        kind: BuilderKind,
    ) {
        self.check_builder_block_inner_arena(arena, source, block, kind, true);
    }

    fn check_builder_block_inner_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        block: &ArenaBuilderBlock,
        kind: BuilderKind,
        require_complete: bool,
    ) {
        self.push_scope();
        let mut seen_fields = FxHashSet::default();
        let mut seen_entries = FxHashSet::default();
        let mut run_entries = 0usize;
        for entry in arena.arena.builder_entries(block.entries) {
            let entry_span = arena.arena.span(entry.span);
            match &entry.kind {
                ArenaBuilderEntryKind::Field { name, value } => {
                    if !seen_fields.insert(*name) {
                        self.error(entry_span, "duplicate builder field", "check.builder-field");
                    }
                    let expected = builder_field_type(kind, name);
                    if expected.is_none() && !builder_allows_field(kind, name) {
                        self.error(entry_span, "unknown builder field", "check.builder-field");
                    }
                    let actual = self.check_expr_arena(arena, source, *value, expected.as_ref());
                    if let Some(expected) = expected {
                        let value_span = arena.arena.expr(*value).span;
                        self.expect_type(&expected, &actual, value_span);
                    }
                    if kind == BuilderKind::ProcessCommand && *name == "cpu_max" {
                        self.check_static_positive_builder_int_arena(
                            arena,
                            *value,
                            "cpu_max must be positive",
                        );
                    }
                }
                ArenaBuilderEntryKind::Entry { name, args, block } => {
                    seen_entries.insert(*name);
                    if !builder_allows_entry(kind, name) {
                        self.error(entry_span, "unknown builder entry", "check.builder-entry");
                    }
                    for arg in arena.arena.command_args(*args) {
                        self.check_command_arg_arena(arena, source, arg, Some(&Type::Str));
                    }
                    if let Some(block_id) = block {
                        let nested = if kind == BuilderKind::ProcessCommand || *name == "command" {
                            BuilderKind::ProcessCommand
                        } else {
                            kind
                        };
                        self.check_builder_block_inner_arena(
                            arena,
                            source,
                            arena.arena.builder_block(*block_id),
                            nested,
                            nested == BuilderKind::ProcessCommand,
                        );
                    }
                }
                ArenaBuilderEntryKind::Task { block, .. } => {
                    self.error(
                        entry_span,
                        "tasks are not valid in this builder",
                        "check.builder-entry",
                    );
                    let previous_return = self.current_return.clone();
                    self.current_return =
                        Some(Type::Result(Box::new(Type::Unit), Box::new(Type::Error)));
                    self.check_value_block_arena(
                        arena,
                        source,
                        *block,
                        &Type::Result(Box::new(Type::Unit), Box::new(Type::Error)),
                    );
                    self.current_return = previous_return;
                }
                ArenaBuilderEntryKind::Stmt(stmt_id) => {
                    if matches!(kind, BuilderKind::ProcessCommand) {
                        match arena.arena.stmt(*stmt_id).kind {
                            crate::syntax::arena::ArenaStmtKind::Command(command_id)
                                if matches!(
                                    arena.arena.command_stmt(command_id).command,
                                    crate::syntax::arena::ArenaCommand::Run(_)
                                ) =>
                            {
                                run_entries += 1;
                                self.check_stmt_arena(arena, source, *stmt_id);
                            }
                            _ => self.error(
                                arena.arena.stmt(*stmt_id).span,
                                "process.command accepts only run entries and fields",
                                "check.builder-entry",
                            ),
                        }
                    } else {
                        self.check_stmt_arena(arena, source, *stmt_id);
                    }
                }
            }
        }
        if require_complete && kind == BuilderKind::ProcessCommand && run_entries == 0 {
            self.error(
                arena.arena.span(block.span),
                "process.command requires a run entry",
                "check.builder-check",
            );
        }
        let _ = (kind, require_complete, seen_fields, seen_entries);
        self.pop_scope();
    }

    fn check_static_positive_builder_int_arena(
        &mut self,
        arena: &ArenaProgram,
        expr_id: ExprId,
        message: &str,
    ) {
        let expr = arena.arena.expr(expr_id);
        match &expr.kind {
            ArenaExprKind::Int(value)
                if arena
                    .arena
                    .int_literal(*value)
                    .value()
                    .is_some_and(|value| value <= 0) =>
            {
                self.error(expr.span, message, "check.builder-field");
            }
            ArenaExprKind::Unary {
                op: crate::syntax::node::UnaryOp::Neg,
                expr: inner,
            } if matches!(arena.arena.expr(*inner).kind, ArenaExprKind::Int(_)) => {
                self.error(expr.span, message, "check.builder-field");
            }
            _ => {}
        }
    }
}

fn checker_builder_call_parts_arena(
    arena: &ArenaProgram,
    expr: ExprId,
) -> Option<(Name, Name, &[ArenaCallArg])> {
    match &arena.arena.expr(expr).kind {
        ArenaExprKind::Call { callee, args } => {
            let ArenaExprKind::Field { base, name } = arena.arena.expr(*callee).kind else {
                return None;
            };
            let ArenaExprKind::Ident(module) = arena.arena.expr(base).kind else {
                return None;
            };
            Some((module, name, arena.arena.call_args(*args)))
        }
        ArenaExprKind::Field { base, name } => {
            let ArenaExprKind::Ident(module) = arena.arena.expr(*base).kind else {
                return None;
            };
            Some((module, *name, &[]))
        }
        _ => None,
    }
}

pub(super) fn builder_field_type(kind: BuilderKind, name: &str) -> Option<Type> {
    match (kind, name) {
        (BuilderKind::ProcessCommand, "cwd") => Some(Type::Path),
        (BuilderKind::ProcessCommand, "timeout") => Some(Type::Duration),
        (BuilderKind::ProcessCommand, "cpu_max") => Some(Type::Int),
        (BuilderKind::ProcessCommand, "env") => Some(Type::Record(BTreeMap::new())),
        (BuilderKind::ProcessCommand, "detach") => Some(Type::Bool),
        (BuilderKind::ProcessCommand, "new_session") => Some(Type::Bool),
        (BuilderKind::ProcessCommand, "ignore_hup") => Some(Type::Bool),
        _ => None,
    }
}

pub(super) fn builder_allows_field(kind: BuilderKind, name: &str) -> bool {
    builder_field_type(kind, name).is_some()
}

pub(super) fn builder_allows_entry(_kind: BuilderKind, _name: &str) -> bool {
    false
}
