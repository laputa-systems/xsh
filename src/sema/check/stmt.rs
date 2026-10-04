#![allow(clippy::single_call_fn)]

use super::TagVariantInfo;
use super::expr::expr_or_run_span_arena;
use super::{
    AnnotationFact, AnnotationFactKind, BinaryOp, Checker, Diagnostic, FixHint, FxHashSet, Label, Name, Span, Type, UnaryOp,
    command_stmt_asserts_success_arena, command_ty_auto_propagates,
    expr_ty_auto_propagates, normalize_hook_signal, signal_rejection_message,
};
use super::{Binding, TypeDefBody, tail_type_matches_expected};
use crate::syntax::arena::{
    ArenaAssignTargetKind, ArenaBindingTargetKind, ArenaExprKind, ArenaExprOrRun, ArenaFunctionDef,
    ArenaProgram, ArenaRange, ArenaSignalHook, ArenaStmtKind, AssignTargetId, BindingTargetId,
    BlockId, ExprId, StmtId, TypeExprId,
};
use crate::syntax::node::AssignOp;
use rustc_hash::FxHashMap;

pub(super) use super::proof::{Narrowing, ConditionNarrowings};

fn annotation_type_is_nontrivial(ty: &Type) -> bool {
    matches!(
        ty,
        Type::List(_)
            | Type::Map(_, _)
            | Type::Result(_, _)
            | Type::Optional(_)
            | Type::Command
            | Type::Pure
            | Type::Proc
            | Type::Tag(_)
    )
}

/// Arena-native mirror of `block_always_returns`/`stmt_always_returns` — a
/// pure structural walk, same pattern as `block_has_exit_point_arena`.
#[allow(dead_code)]
pub(super) fn block_always_returns_arena(arena: &ArenaProgram, block_id: BlockId) -> bool {
    let block = arena.arena.block(block_id);
    arena
        .arena
        .stmt_ids(block.statements)
        .any(|id| stmt_always_returns_arena(arena, id))
}

#[allow(dead_code)]
pub(super) fn stmt_always_returns_arena(arena: &ArenaProgram, id: StmtId) -> bool {
    match arena.arena.stmt(id).kind {
        ArenaStmtKind::Return(_) => true,
        ArenaStmtKind::BooleanGuard { condition, else_block } if matches!(arena.arena.expr(condition).kind, ArenaExprKind::Bool(false)) => block_always_returns_arena(arena, else_block),
        ArenaStmtKind::Expr(expr) => match arena.arena.expr(expr).kind {
            ArenaExprKind::ErrorContext { block, .. } => block_always_returns_arena(arena, block),
            _ => false,
        },

        ArenaStmtKind::If {
            branches,
            else_block: Some(else_block),
        } => {
            arena
                .arena
                .if_branches(branches)
                .iter()
                .all(|branch| block_always_returns_arena(arena, branch.block))
                && block_always_returns_arena(arena, else_block)
        }
        ArenaStmtKind::Match { arms, .. } => arena
            .arena
            .match_arms(arms)
            .iter()
            .all(|arm| block_always_returns_arena(arena, arm.block)),
        ArenaStmtKind::With { body, else_block, .. } => {
            block_always_returns_arena(arena, body) && block_always_returns_arena(arena, else_block)
        }
        _ => false,
    }
}

/// Returns true if the block contains any `break` or `return` statement that
/// is not inside a nested `while`/`for`/`loop`. A loop body with at least one
/// such exit point is not statically infinite.
/// Arena-native mirror of `block_has_exit_point`/`stmt_has_exit_point` — a
/// pure structural walk (no type-checking), so it's independent of which
/// `ArenaStmtKind` variants `check_stmt_arena` has native coverage for.
#[allow(dead_code)]
pub(super) fn block_has_exit_point_arena(arena: &ArenaProgram, block_id: BlockId) -> bool {
    let block = arena.arena.block(block_id);
    arena
        .arena
        .stmt_ids(block.statements)
        .any(|id| stmt_has_exit_point_arena(arena, id))
}

#[allow(dead_code)]
pub(super) fn stmt_has_exit_point_arena(arena: &ArenaProgram, id: StmtId) -> bool {
    match &arena.arena.stmt(id).kind {
        ArenaStmtKind::Break { .. } | ArenaStmtKind::Return(_) => true,
        ArenaStmtKind::While { .. } | ArenaStmtKind::For { .. } | ArenaStmtKind::Loop { .. } => {
            false
        }
        ArenaStmtKind::If {
            branches,
            else_block,
        } => {
            arena
                .arena
                .if_branches(*branches)
                .iter()
                .any(|b| block_has_exit_point_arena(arena, b.block))
                || else_block.is_some_and(|b| block_has_exit_point_arena(arena, b))
        }
        ArenaStmtKind::Match { arms, .. } => arena
            .arena
            .match_arms(*arms)
            .iter()
            .any(|a| block_has_exit_point_arena(arena, a.block)),
        ArenaStmtKind::With {
            body, else_block, ..
        } => {
            block_has_exit_point_arena(arena, *body)
                || block_has_exit_point_arena(arena, *else_block)
        }
        ArenaStmtKind::Guard { else_block, .. } | ArenaStmtKind::BooleanGuard { else_block, .. } => block_has_exit_point_arena(arena, *else_block),
        ArenaStmtKind::Expr(expr) => match arena.arena.expr(*expr).kind {
            ArenaExprKind::ErrorContext { block, .. } => block_has_exit_point_arena(arena, block),
            _ => false,
        },
        ArenaStmtKind::GuardedStmt { stmt: inner, .. } => stmt_has_exit_point_arena(arena, *inner),
        _ => false,
    }
}

/// Returns true if a match on `value_ty` with the given arms is exhaustive —
/// i.e. every possible value is matched. This is true when the match has a
/// catch-all (wildcard or non-tag-variant binding) or, for tag unions, when
/// every variant is explicitly covered.
fn match_is_exhaustive_arena(
    arena: &ArenaProgram,
    value_ty: &Type,
    arms: &[crate::syntax::arena::ArenaMatchArm],
    type_defs: &FxHashMap<Name, TypeDefBody>,
    tag_variants: &FxHashMap<Name, TagVariantInfo>,
) -> bool {
    patterns_are_exhaustive_arena(arena, value_ty, arms.iter().filter(|arm| arm.guard.is_none()).map(|arm| arm.pattern), type_defs, tag_variants)
}

pub(super) fn patterns_are_exhaustive_arena(
    arena: &ArenaProgram,
    value_ty: &Type,
    patterns: impl Iterator<Item = crate::syntax::arena::PatternId>,
    type_defs: &FxHashMap<Name, TypeDefBody>,
    tag_variants: &FxHashMap<Name, TagVariantInfo>,
) -> bool {
    use crate::syntax::arena::ArenaPatternKind;
    fn irrefutable(arena: &ArenaProgram, pattern: crate::syntax::arena::PatternId, variants: &FxHashMap<Name, TagVariantInfo>) -> bool {
        match arena.arena.pattern(pattern).kind {
            ArenaPatternKind::Group(child) | ArenaPatternKind::Alias { pattern: child, .. } => irrefutable(arena, child, variants),
            ArenaPatternKind::Alternation(items) => arena.arena.pattern_ids(items).any(|child| irrefutable(arena, child, variants)),
            ArenaPatternKind::Wildcard => true,
            ArenaPatternKind::Binding(name) => !variants.contains_key(&name),
            ArenaPatternKind::Tuple(items) => arena.arena.pattern_ids(items).all(|item| irrefutable(arena, item, variants)),
            // A record shape can reject dynamic payloads even when all fields bind.
            ArenaPatternKind::Record { .. } => false,
            _ => false,
        }
    }
    fn covered(arena: &ArenaProgram, pattern: crate::syntax::arena::PatternId, variants: &FxHashMap<Name, TagVariantInfo>, constructors: &mut FxHashSet<Name>, booleans: &mut [bool; 2]) -> bool {
        match arena.arena.pattern(pattern).kind {
            ArenaPatternKind::Wildcard => return true,
            ArenaPatternKind::Binding(name) if !variants.contains_key(&name) => return true,
            ArenaPatternKind::Binding(name) => { constructors.insert(name); }
            ArenaPatternKind::Constructor { name, arg } if arg.is_none_or(|arg| irrefutable(arena, arg, variants)) => { constructors.insert(name); }
            ArenaPatternKind::Literal(expr) => if let ArenaExprKind::Bool(value) = arena.arena.expr(expr).kind { booleans[usize::from(value)] = true; },
            ArenaPatternKind::Alternation(items) => for item in arena.arena.pattern_ids(items) { if covered(arena, item, variants, constructors, booleans) { return true; } },
            _ => {}
        }
        false
    }
    let mut pending: Vec<_> = patterns.collect();
    let mut patterns = Vec::new();
    while let Some(pattern) = pending.pop() {
        match arena.arena.pattern(pattern).kind {
            ArenaPatternKind::Group(child) | ArenaPatternKind::Alias { pattern: child, .. } => pending.push(child),
            ArenaPatternKind::Alternation(items) => pending.extend(arena.arena.pattern_ids(items)),
            _ => patterns.push(pattern),
        }
    }
    let mut constructors = FxHashSet::default();
    let mut booleans = [false; 2];
    let mut empty_list = false;
    let mut nonempty_list = false;
    for pattern in patterns {
        if covered(arena, pattern, tag_variants, &mut constructors, &mut booleans) { return true; }
        if matches!(value_ty, Type::List(_)) && let ArenaPatternKind::List { elements, rest } = arena.arena.pattern(pattern).kind {
            let count = arena.arena.pattern_ids(elements).count();
            if count == 0 {
                if rest.is_some() { return true; }
                empty_list = true;
            } else if count == 1 && rest.is_some() && arena.arena.pattern_ids(elements).all(|child| matches!(arena.arena.pattern(child).kind, ArenaPatternKind::Wildcard) || matches!(arena.arena.pattern(child).kind, ArenaPatternKind::Binding(name) if !tag_variants.contains_key(&name))) {
                nonempty_list = true;
            }
        }
    }
    match value_ty {
        Type::List(_) => empty_list && nonempty_list,
        Type::Bool => booleans.iter().all(|value| *value),
        Type::Result(_, _) => constructors.contains(&Name::intern("Ok")) && constructors.contains(&Name::intern("Err")),
        Type::Tag(name) => match type_defs.get(name).or_else(|| type_defs.values().find(|body|
            matches!(body, TypeDefBody::TagUnion(variants) if variants.first().is_some_and(|variant| variant.type_name == *name)))) {
            Some(TypeDefBody::TagUnion(variants)) => variants.iter().all(|variant| constructors.contains(&variant.name)),
            _ => false,
        },
        _ => false,
    }
}

/// Arena-native mirror of `check_stmt` and the block/binding/assignment
/// machinery it depends on.
#[allow(dead_code)]
impl Checker {
    pub(super) fn define_binding_target_arena(
        &mut self,
        arena: &ArenaProgram,
        target: BindingTargetId,
        ty: &Type,
        mutable: bool,
        span: Span,
    ) {
        match &arena.arena.binding_target(target).kind {
            ArenaBindingTargetKind::Name(name) => {
                if name.as_str() == "_" {
                    return;
                }
                if self.current_scope().contains_key(name) || self.tag_variants.contains_key(name) {
                    self.error(span, "duplicate name in scope", "check.duplicate-name");
                }
                self.define(
                    *name,
                    if self.in_pure && mutable {
                        Binding::pure_local_var(ty.clone())
                    } else {
                        Binding::new(ty.clone(), mutable)
                    },
                    span,
                );
            }
            ArenaBindingTargetKind::Record { fields, .. } => {
                let record_fields = match ty {
                    Type::Record(fields) => Some(fields),
                    Type::Unknown => None,
                    _ => {
                        self.error(
                            span,
                            "record destructuring requires a record value",
                            "check.destructure-type",
                        );
                        None
                    }
                };
                let mut names = FxHashSet::default();
                for field in arena.arena.destructure_fields(*fields) {
                    let field_span = arena.arena.span(field.span);
                    if !names.insert(field.name) {
                        self.error(
                            field_span,
                            "duplicate destructured field",
                            "check.destructure-field",
                        );
                    }
                    let field_ty = record_fields
                        .and_then(|fields| fields.get(&field.name))
                        .cloned()
                        .unwrap_or(Type::Unknown);
                    if let Some(record_fields) = record_fields
                        && !record_fields.is_empty()
                        && !record_fields.contains_key(&field.name)
                    {
                        self.error(
                            field_span,
                            "unknown destructured field",
                            "check.destructure-field",
                        );
                    }
                    self.define_binding_target_arena(arena, field.target, &field_ty, mutable, field_span);
                }
            }
        }
    }

    pub(super) fn check_compound_assignment_op(
        &mut self,
        op: AssignOp,
        left: &Type,
        right: &Type,
        op_span: Span,
        rhs_span: Span,
    ) -> Type {
        if left == &Type::Duration {
            let valid = matches!((op, right), (AssignOp::Add | AssignOp::Sub, Type::Duration)
                | (AssignOp::Mul | AssignOp::Div, Type::Int));
            if !valid { self.error(rhs_span, "invalid Duration compound assignment dimensions", "check.operator-type"); }
            return Type::Duration;
        }
        if op == AssignOp::Add && matches!(left, Type::List(_) | Type::Str) {
            self.expect_type(left, right, rhs_span);
            return left.clone();
        }
        if matches!(left, Type::Float) && op != AssignOp::Rem {
            if !matches!(right, Type::Float | Type::Unknown) {
                self.error(
                    rhs_span,
                    "compound assignment requires Float operands",
                    "check.operator-type",
                );
            }
            return Type::Float;
        }
        if !matches!(left, Type::Int | Type::UInt | Type::Unknown) {
            // The target keeps its type, so this one report is the whole
            // mistake: neither the operand nor the result is reported again.
            let symbol = match op { AssignOp::Add => "+=", AssignOp::Sub => "-=", AssignOp::Mul => "*=", AssignOp::Div => "/=", _ => "%=" };
            let mut diagnostic = Diagnostic::error(format!("`{symbol}` is not defined for {left}"))
                .with_code("check.operator-type")
                .with_label(Label::primary(op_span, "compound assignment requires Int or Float operands"));
            if *left == Type::Path {
                diagnostic = diagnostic.with_note("operators never join paths; build the path with an `fp\"...\"` literal");
            }
            self.diagnostics.push(diagnostic);
            return left.clone();
        }
        if !matches!(right, Type::Int | Type::UInt | Type::Unknown) {
            self.error(
                rhs_span,
                "compound assignment requires Int operands",
                "check.operator-type",
            );
        }
        Type::Int
    }

    pub(super) fn apply_narrowings(&mut self, narrowings: &[Narrowing]) {
        for narrowing in narrowings {
            let Some(binding) = self.lookup(narrowing.name).cloned() else {
                continue;
            };
            if !binding.proof.accepts(narrowing) { continue; }
            let mut narrowed = binding;
            if narrowed.unrefined_ty.is_none() {
                narrowed.unrefined_ty = Some(narrowed.ty.clone());
            }
            if !super::proof::replace_projection(&mut narrowed.ty, &narrowing.path, narrowing.ty.clone()) { continue; }
            self.current_scope_mut().insert(narrowing.name, narrowed);
        }
    }

    pub(super) fn check_loop_control(&mut self, span: Span, is_break: bool) {
        if self.loop_depth > 0 {
            return;
        }
        if self.in_defer_block {
            self.error(span, "loop control cannot leave a deferred cleanup block", "check.defer-control-flow");
            return;
        }
        let message = if self.stream_item_types.is_empty() {
            if is_break {
                "`break` is valid only inside while or for loops"
            } else {
                "`continue` is valid only inside while or for loops"
            }
        } else if is_break {
            "`break` cannot target a structured stream stage"
        } else {
            "`continue` cannot target a structured stream stage"
        };
        self.error(span, message, "check.loop-control");
    }

    /// Assertion is explicit, so a Bool in statement position is rejected
    /// rather than asserted or silently discarded. Statement use is decided
    /// from the checked type and its consumer, never from a runtime value.
    pub(super) fn reject_bool_statement(&mut self, source: &str, ty: &Type, statement: Span) -> bool {
        if *ty != Type::Bool {
            return false;
        }
        let mut diagnostic = Diagnostic::error("Bool expression statement is not an assertion")
            .with_code("check.bool-statement")
            .with_label(Label::primary(statement, "use `assert <expr>` to assert it, or `let _ = <expr>` to discard it"));
        if let Some(fix) = bool_statement_assert_fix(source, statement) { diagnostic = diagnostic.with_fix_hint(fix); }
        self.diagnostics.push(diagnostic);
        true
    }

    /// Inside a body that produces a value, a non-tail statement must not
    /// produce one: it was probably meant as the tail. Elsewhere (top level
    /// and statement blocks) a discarded value is accepted, except a
    /// discarded copy update such as `items.push(x)`, whose only effect is
    /// the value it returns.
    fn reject_discarded_value(&mut self, arena: &ArenaProgram, expr_id: ExprId, ty: &Type, in_value_body: bool) {
        if ty.is_result() || ty.matches_expected(&Type::Unit) {
            return;
        }
        let span = arena.arena.expr(expr_id).span;
        let copy_update = copy_update_mistake(arena, expr_id, ty);
        let message = match (&copy_update, in_value_body) {
            (_, true) => format!("expression statement must be last to produce a value: expression has type `{ty}`; use `let _ = ...` to discard it"),
            (Some(mistake), false) => mistake.message.clone(),
            (None, false) => return,
        };
        let mut diagnostic = Diagnostic::error(message.clone())
            .with_code("check.non-tail-expression")
            .with_label(Label::primary(span, message));
        if let Some(mistake) = copy_update {
            diagnostic = diagnostic.with_note(if in_value_body { format!("{}; {}", mistake.message, mistake.repair) } else { mistake.repair });
        }
        self.diagnostics.push(diagnostic);
    }

    /// XSH has no truthiness, so a condition names the type it found. A
    /// fallible Bool or Status (`fs.exists(path)`) is the usual cause, and
    /// the hint offers `?`, which propagates the failure instead of guessing.
    /// It changes failure behavior, so `lint --fix` never applies it.
    pub(super) fn report_non_bool_condition(&mut self, ty: &Type, span: Span, message: &str, code: &str) {
        let mut diagnostic = Diagnostic::error(message)
            .with_code(code)
            .with_label(Label::primary(span, format!("found {ty}")));
        if let Some((Type::Bool | Type::Status, _)) = super::types::result_types(ty) {
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                Span::at(span.source_id, span.end()),
                "propagate the failure with `?`, or choose a fallback with `??`",
                "?",
            ).dangerous());
        } else if ty.is_result() {
            diagnostic = diagnostic.with_note("unwrap the Result with `?` or `??`, then compare its value");
        }
        self.diagnostics.push(diagnostic);
    }

    pub(super) fn check_stmt_arena(&mut self, arena: &ArenaProgram, source: &str, id: StmtId) {
        let stmt = arena.arena.stmt(id);
        self.statement_positions.entry(stmt.span).or_insert(super::StatementPosition::Statement);
        // Declarations bind module-level names; inside a body or block nothing
        // could resolve them and preparation has no form for them.
        if matches!(
            stmt.kind,
            ArenaStmtKind::Use(_)
                | ArenaStmtKind::Export(_)
                | ArenaStmtKind::TypeDef(_)
                | ArenaStmtKind::ErrorDef(_)
                | ArenaStmtKind::ProcDef(_)
                | ArenaStmtKind::PureDef(_)
                | ArenaStmtKind::StreamDef(_)
        ) && (self.block_depth > 0 || self.current_return.is_some())
        {
            self.error(
                stmt.span,
                "declarations are allowed only at the top level of a script or module",
                "check.nested-declaration",
            );
        }
        match stmt.kind {
            ArenaStmtKind::BooleanGuard { condition, else_block } => {
                let narrowings = self.check_condition_arena(arena, source, condition, "check.guard-condition");
                let success_scopes = self.scopes.clone();
                self.push_scope();
                self.apply_narrowings(&narrowings.when_false);
                self.check_block_arena(arena, source, else_block);
                self.pop_scope();
                if !self.definitely_exiting_block_spans.contains(&arena.arena.span(arena.arena.block(else_block).span)) {
                    self.error(arena.arena.span(arena.arena.block(else_block).span), "guard failure branch must leave the enclosing continuation on every reachable path", "check.guard-fallthrough");
                }
                self.scopes = success_scopes;
                self.apply_narrowings(&narrowings.when_true);
            }
            ArenaStmtKind::Use(use_id) => {
                let use_stmt = arena.arena.use_stmt(use_id);
                self.check_use_arena(
                    arena,
                    use_stmt.path,
                    use_stmt.alias,
                    use_stmt.resolved.as_deref(),
                    stmt.span,
                );
            }
            ArenaStmtKind::Export(inner_id) => {
                let inner = arena.arena.stmt(inner_id);
                if let ArenaStmtKind::Let { target, .. } | ArenaStmtKind::Const { target, .. } = inner.kind
                    && matches!(
                        arena.arena.binding_target(target).kind,
                        ArenaBindingTargetKind::Record { .. }
                    )
                {
                    self.error(
                        inner.span,
                        "destructured exports are not supported",
                        "check.export-destructure",
                    );
                }
                let previous_exported = self.current_exported;
                self.current_exported = true;
                self.check_stmt_arena(arena, source, inner_id);
                self.current_exported = previous_exported;
            }
            ArenaStmtKind::TypeDef(def_id) => {
                let def = arena.arena.type_def(def_id);
                self.check_type_def_arena(arena, source, def, stmt.span);
            }
            ArenaStmtKind::ErrorDef(def_id) => {
                self.check_error_def_arena(arena, source, def_id);
            }
            ArenaStmtKind::ProcDef(def_id) | ArenaStmtKind::CliMain(def_id) => {
                let def = arena.arena.function_def(def_id).clone();
                self.check_function_arena(arena, source, &def, false);
            }
            ArenaStmtKind::PureDef(def_id) => {
                let def = arena.arena.function_def(def_id).clone();
                self.check_function_arena(arena, source, &def, true);
            }
            ArenaStmtKind::StreamDef(def_id) => {
                let def = arena.arena.function_def(def_id).clone();
                self.check_stream_function_arena(arena, source, &def);
            }
            ArenaStmtKind::SignalHook(hook_id) => {
                let hook = arena.arena.signal_hook(hook_id).clone();
                self.check_signal_hook_arena(arena, source, &hook, stmt.span);
            }
            ArenaStmtKind::Let {
                target,
                ty,
                initializer,
            } | ArenaStmtKind::Const {
                target,
                ty,
                initializer,
            } => {
                self.check_binding_arena(arena, source, target, ty, initializer, false, stmt.span);
            }
            ArenaStmtKind::Var {
                target,
                ty,
                initializer,
            } => {
                self.check_binding_arena(arena, source, target, ty, initializer, true, stmt.span);
            }
            ArenaStmtKind::Assign { target, op, value } => {
                self.check_assignment_arena(arena, source, target, op, value, stmt.span);
            }
            ArenaStmtKind::Return(value) => {
                self.check_return_arena(arena, source, value, stmt.span);
            }
            ArenaStmtKind::YieldDelegate(value) => {
                self.check_yield_delegation_arena(arena, source, value, stmt.span);
            }
            ArenaStmtKind::Yield(value) => {
                self.check_yield_arena(arena, source, value, stmt.span);
            }
            ArenaStmtKind::Defer(value) => {
                self.check_defer_arena(arena, source, value, stmt.span);
            }
            ArenaStmtKind::Break { value } => {
                if self.in_signal_hook {
                    self.error(
                        stmt.span,
                        "`break` is not allowed in signal hooks",
                        "check.signal-hook",
                    );
                }
                self.check_loop_control(stmt.span, true);
                if let Some(expr_id) = value {
                    self.check_expr_arena(arena, source, expr_id, None);
                }
            }
            ArenaStmtKind::Continue => {
                if self.in_signal_hook {
                    self.error(
                        stmt.span,
                        "`continue` is not allowed in signal hooks",
                        "check.signal-hook",
                    );
                }
                self.check_loop_control(stmt.span, false);
            }
            ArenaStmtKind::Assert { condition, message } => {
                if self.retry_attempt_depth == 0 { self.assertion_effect_spans.insert(stmt.span); }
                self.check_propagation(&Type::Result(Box::new(Type::Unit), Box::new(Type::ErrorFamily(Name::intern("AssertionError")))), stmt.span);
                let condition_ty = self.check_expr_with_schema_arena(arena, source, ArenaExprOrRun::Expr(condition), Some(&Type::Bool), None);
                if condition_ty != Type::Bool && !matches!(condition_ty, Type::Unknown | Type::Invalid) {
                    self.report_non_bool_condition(&condition_ty, arena.arena.expr(condition).span, "assert condition requires Bool", "check.assert-condition");
                }
                if let Some(message) = message {
                    let message_ty = self.check_expr_with_schema_arena(arena, source, ArenaExprOrRun::Expr(message), Some(&Type::Str), None);
                    if message_ty != Type::Str && !matches!(message_ty, Type::Unknown | Type::Invalid) {
                        self.error(arena.arena.expr(message).span, "assert message requires Str", "check.assert-message");
                    }
                }
                let facts = self.infer_condition_narrowings_arena(arena, condition);
                self.apply_narrowings(&facts.when_true);
            }
            ArenaStmtKind::Expr(expr_id) if matches!(arena.arena.expr(expr_id).kind, ArenaExprKind::ErrorContext { .. }) => {
                let ArenaExprKind::ErrorContext { message, block } = arena.arena.expr(expr_id).kind else { unreachable!() };
                let ty = self.check_expr_with_schema_arena(arena, source, ArenaExprOrRun::Expr(message), Some(&Type::Str), None);
                self.expect_type(&Type::Str, &ty, arena.arena.expr(message).span);
                self.check_block_arena(arena, source, block);
                self.expr_types.insert(arena.arena.expr(expr_id).span, Type::Unit);
            }
            ArenaStmtKind::Expr(expr_id) => {
                self.statement_expression_spans.insert(arena.arena.expr(expr_id).span);
                let ty = if let ArenaExprKind::ValueBlock(block) = arena.arena.expr(expr_id).kind {
                    self.check_block_arena(arena, source, block);
                    self.expr_types.insert(arena.arena.expr(expr_id).span, Type::Unit);
                    Type::Unit
                } else {
                    self.check_expr_with_schema_arena(arena, source, ArenaExprOrRun::Expr(expr_id), None, None)
                };
                self.record_inert_expression_discard(arena, ArenaExprOrRun::Expr(expr_id));
                if !self.reject_bool_statement(source, &ty, stmt.span) {
                    self.record_statement_error(&ty, stmt.span);
                }
                if !expr_ty_auto_propagates(&ty) {
                    let expr_span = arena.arena.expr(expr_id).span;
                    self.reject_ignored_result(&ty, expr_span);
                    self.reject_discarded_value(arena, expr_id, &ty, false);
                }
            }
            ArenaStmtKind::If {
                branches,
                else_block,
            } => self.check_if_arena(arena, source, branches, else_block),
            ArenaStmtKind::While { condition, block } => {
                self.check_while_arena(arena, source, condition, block);
            }
            ArenaStmtKind::For {
                target,
                iter,
                block,
            } => {
                self.check_for_arena(arena, source, target, iter, block, stmt.span);
            }
            ArenaStmtKind::Loop { block } => {
                self.loop_depth += 1;
                self.check_block_arena(arena, source, block);
                self.loop_depth -= 1;
                if !block_has_exit_point_arena(arena, block) {
                    self.error(
                        stmt.span,
                        "`loop` has no `break` — will run forever",
                        "check.loop-no-break",
                    );
                }
            }
            ArenaStmtKind::With {
                bindings,
                body,
                else_block,
            } => {
                self.check_with_arena(
                    arena, source, bindings, body, else_block, stmt.span,
                );
            }
            ArenaStmtKind::Guard {
                target,
                ty,
                initializer,
                else_block,
            } => {
                self.check_guard_arena(
                    arena,
                    source,
                    target,
                    ty,
                    initializer,
                        else_block,
                    stmt.span,
                );
            }
            ArenaStmtKind::GuardedStmt {
                stmt: inner,
                negate,
                condition,
            } => {
                let narrowings = self.check_condition_arena(
                    arena,
                    source,
                    condition,
                    "check.guarded-stmt-condition",
                );
                let continuing_scopes = self.stmt_definitely_exits_arena(arena, inner).then(|| self.scopes.clone());
                self.push_scope();
                if negate {
                    self.apply_narrowings(&narrowings.when_false);
                } else {
                    self.apply_narrowings(&narrowings.when_true);
                }
                self.check_stmt_arena(arena, source, inner);
                self.pop_scope();
                if let Some(scopes) = continuing_scopes {
                    // An exiting payload cannot mutate bindings on the path
                    // that skips it. That continuation retains the opposite
                    // condition proof against the original lexical bindings.
                    self.scopes = scopes;
                    self.apply_narrowings(if negate { &narrowings.when_true } else { &narrowings.when_false });
                }
            }
            ArenaStmtKind::Match { value, arms } => {
                self.check_match_arena(arena, source, value, arms);
            }
            ArenaStmtKind::Command(command_id) => {
                self.check_command_stmt_arena(arena, source, command_id);
            }
            ArenaStmtKind::TailBareIdent(name) => {
                let ty = self.check_tail_bare_ident_arena(arena, source, name, stmt.span);
                if !command_ty_auto_propagates(&ty) {
                    self.reject_ignored_result(&ty, stmt.span);
                }
                self.reject_bool_statement(source, &ty, stmt.span);
            }
        }
    }

    pub(super) fn bind_pattern_condition_arena(&mut self, arena: &ArenaProgram, source: &str, condition: ExprId) {
        if let ArenaExprKind::PatternCondition { value, arms } = arena.arena.expr(condition).kind {
            let ty = self.expr_types.get(&arena.arena.expr(value).span).cloned().unwrap_or(Type::Unknown);
            self.check_pattern_arena(arena, source, arena.arena.match_expr_arms(arms)[0].pattern, &ty);
        }
    }

    pub(super) fn check_condition_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        condition: ExprId,
        code: &'static str,
    ) -> ConditionNarrowings {
        let condition_ty = self.check_expr_with_schema_arena(arena, source, ArenaExprOrRun::Expr(condition), Some(&Type::Bool), None);
        if matches!(
            condition_ty,
            Type::Bool | Type::Status | Type::Any | Type::Unknown
        ) {
            return self.infer_condition_narrowings_arena(arena, condition);
        }
        let condition_span = arena.arena.expr(condition).span;
        self.report_non_bool_condition(&condition_ty, condition_span, "condition must be Bool or Status", code);
        ConditionNarrowings::default()
    }

    pub(super) fn infer_condition_narrowings_arena(&self, arena: &ArenaProgram, condition: ExprId) -> ConditionNarrowings {
        let mut facts = self.condition_proofs.get(&condition).map(|facts| facts.as_ref().clone())
            .unwrap_or_else(|| self.infer_condition_proof_arena(arena, condition));
        let valid = |fact: &Narrowing| self.lookup(fact.name).is_some_and(|binding| binding.proof.accepts(fact));
        facts.when_true.retain(valid);
        facts.when_false.retain(valid);
        facts
    }

    pub(super) fn proof_subject_arena(&self, arena: &ArenaProgram, mut expr: ExprId) -> Option<(Name, Vec<Name>, Type)> {
        let mut path = Vec::new();
        loop {
            match arena.arena.expr(expr).kind {
                ArenaExprKind::Ident(name) => {
                    path.reverse();
                    let ty = super::proof::projected_type(&self.lookup(name)?.ty, &path)?.clone();
                    return Some((name, path, ty));
                }
                ArenaExprKind::Field { base, name } if path.len() < 128 => { path.push(name); expr = base; }
                _ => return None,
            }
        }
    }

    pub(super) fn infer_condition_proof_arena(
        &self,
        arena: &ArenaProgram,
        condition: ExprId,
    ) -> ConditionNarrowings {
        match arena.arena.expr(condition).kind {
            ArenaExprKind::Ident(name) => self.lookup(name).and_then(|binding| binding.boolean_proof.as_ref())
                .map(|proof| proof.as_ref().clone()).unwrap_or_default(),
            ArenaExprKind::Unary {
                op: UnaryOp::Not,
                expr,
            } => {
                let inner = self.infer_condition_narrowings_arena(arena, expr);
                ConditionNarrowings {
                    when_true: inner.when_false,
                    when_false: inner.when_true,
                }
            }
            ArenaExprKind::Binary {
                op: BinaryOp::And,
                left,
                right,
            } => {
                self.infer_condition_narrowings_arena(arena, left).and(self.infer_condition_narrowings_arena(arena, right))
            }
            ArenaExprKind::Binary {
                op: BinaryOp::Or,
                left,
                right,
            } => {
                self.infer_condition_narrowings_arena(arena, left).or(self.infer_condition_narrowings_arena(arena, right))
            }
            ArenaExprKind::Binary {
                op: BinaryOp::Eq | BinaryOp::Ne,
                left,
                right,
            } => self.infer_null_comparison_narrowings_arena(arena, condition, left, right),
            ArenaExprKind::Binary { op: BinaryOp::In | BinaryOp::NotIn, left, right } => {
                let narrowing = self.infer_record_membership_narrowing_arena(arena, left, right);
                if matches!(arena.arena.expr(condition).kind, ArenaExprKind::Binary { op: BinaryOp::NotIn, .. }) {
                    ConditionNarrowings { when_true: narrowing.when_false, when_false: narrowing.when_true }
                } else { narrowing }
            }
            ArenaExprKind::PatternTest { value, arms } | ArenaExprKind::PatternCondition { value, arms } => {
                let Some((name, path, subject_ty)) = self.proof_subject_arena(arena, value) else { return ConditionNarrowings::default(); };
                let Some(binding) = self.lookup(name) else { return ConditionNarrowings::default(); };
                let pattern = arena.arena.match_expr_arms(arms)[0].pattern;
                let ty = self.pattern_test_narrowed_type(arena, pattern);
                // A facet filters a nominal error without changing its family or
                // variant. Keep that precision when no intersection type is available.
                let ty = ty.map(|ty| {
                    if matches!(ty, Type::ErrorFacet(_))
                        && matches!(
                            subject_ty,
                            Type::ErrorFamily(_) | Type::ErrorVariant { .. } | Type::ProcessError
                        )
                    {
                        subject_ty.clone()
                    } else {
                        ty
                    }
                });
                ConditionNarrowings {
                    when_true: ty.into_iter().map(|ty| binding.proof.fact(name, path.clone(), ty)).collect(),
                    when_false: Vec::new(),
                }
            }
            _ => ConditionNarrowings::default(),
        }
    }

    fn infer_null_comparison_narrowings_arena(
        &self,
        arena: &ArenaProgram,
        condition: ExprId,
        left: ExprId,
        right: ExprId,
    ) -> ConditionNarrowings {
        let Some((name, path, inner)) = self.null_compared_optional_binding_arena(arena, left, right)
        else {
            return ConditionNarrowings::default();
        };
        let narrowing = self.lookup(name).unwrap().proof.fact(name, path, inner);
        if matches!(
            arena.arena.expr(condition).kind,
            ArenaExprKind::Binary {
                op: BinaryOp::Ne,
                ..
            }
        ) {
            ConditionNarrowings {
                when_true: vec![narrowing],
                when_false: Vec::new(),
            }
        } else {
            ConditionNarrowings {
                when_true: Vec::new(),
                when_false: vec![narrowing],
            }
        }
    }

    fn null_compared_optional_binding_arena(&self, arena: &ArenaProgram, left: ExprId, right: ExprId) -> Option<(Name, Vec<Name>, Type)> {
        let subject = match (arena.arena.expr(left).kind, arena.arena.expr(right).kind) {
            (_, ArenaExprKind::Null) => left,
            (ArenaExprKind::Null, _) => right,
            _ => return None,
        };
        let (name, path, ty) = self.proof_subject_arena(arena, subject)?;
        let Type::Optional(inner) = ty else { return None; };
        Some((name, path, *inner))
    }

    fn infer_record_membership_narrowing_arena(
        &self, arena: &ArenaProgram, field_expr: ExprId, record_expr: ExprId,
    ) -> ConditionNarrowings {
        let Some((record_name, path, subject_ty)) = self.proof_subject_arena(arena, record_expr) else {
            return ConditionNarrowings::default();
        };
        let ArenaExprKind::Str(field_name_id) = arena.arena.expr(field_expr).kind else {
            return ConditionNarrowings::default();
        };
        let Some(binding) = self.lookup(record_name) else { return ConditionNarrowings::default(); };
        let Type::Record(mut fields) = subject_ty else { return ConditionNarrowings::default(); };
        let field_name = arena.arena.string_literal(field_name_id);
        fields.entry(Name::intern(field_name)).or_insert(Type::Any);
        ConditionNarrowings {
            when_true: vec![binding.proof.fact(record_name, path, Type::Record(fields))],
            when_false: Vec::new(),
        }
    }

    fn check_if_arena(&mut self, arena: &ArenaProgram, source: &str, branches: ArenaRange, else_block: Option<BlockId>) {
        let initial_scopes = self.scopes.clone();
        let branch_list = arena.arena.if_branches(branches);
        let original = self.scopes.iter().flat_map(|scope| scope.iter()).map(|(name, binding)| (*name, binding.clone())).collect::<FxHashMap<_, _>>();
        let mut reaching = Vec::new();
        let mut previous_failure = Vec::new();
        for branch in branch_list {
            self.push_scope();
            self.apply_narrowings(&previous_failure);
            let facts = self.check_condition_arena(arena, source, branch.condition, "check.if-condition");
            let failure_scopes = self.scopes.clone();
            self.apply_narrowings(&facts.when_true);
            self.bind_pattern_condition_arena(arena, source, branch.condition);
            self.check_block_arena(arena, source, branch.block);
            if !self.definitely_exiting_block_spans.contains(&arena.arena.span(arena.arena.block(branch.block).span)) && let Some(bindings) = self.block_exit_bindings.get(&branch.block) { reaching.push(bindings.clone()); }
            previous_failure.extend(facts.when_false);
            self.scopes = failure_scopes;
            self.pop_scope();
        }
        self.push_scope();
        self.apply_narrowings(&previous_failure);
        if let Some(block) = else_block {
            self.check_block_arena(arena, source, block);
            if !self.definitely_exiting_block_spans.contains(&arena.arena.span(arena.arena.block(block).span)) && let Some(bindings) = self.block_exit_bindings.get(&block) { reaching.push(bindings.clone()); }
        } else {
            reaching.push(self.scopes.iter().flat_map(|scope| scope.iter()).map(|(name, binding)| (*name, binding.clone())).collect());
        }
        self.pop_scope();
        self.scopes = initial_scopes;
        for (name, initial) in &original {
            for bindings in &reaching {
                if let Some(binding) = bindings.get(name).filter(|binding| binding.proof.same_binding(&initial.proof)) {
                    for path in binding.proof.mutation_paths_since(&initial.proof) {
                        for scope in &mut self.scopes {
                            for binding in scope.values_mut().filter(|binding| binding.proof.same_binding(&initial.proof) && binding.mutable) {
                                binding.proof.mutate(&path);
                                if let Some(original) = &binding.unrefined_ty { super::proof::restore_projection(&mut binding.ty, original, &path); }
                                if path.is_empty() { binding.unrefined_ty = None; }
                            }
                        }
                    }
                }
            }
        }
        if reaching.is_empty() { return; }
        let mut facts = Vec::new();
        for (name, initial) in original {
            let Some(current) = self.lookup(name) else { continue; };
            if !initial.proof.same_binding(&current.proof) { continue; }
            let types = reaching.iter().map(|bindings| bindings.get(&name)
                .filter(|binding| binding.proof.same_binding(&initial.proof)).map(|binding| binding.ty.clone())
                .unwrap_or_else(|| initial.unrefined_ty.as_ref().unwrap_or(&initial.ty).clone())).collect::<Vec<_>>();
            let ty = super::proof::intersection_type(initial.unrefined_ty.as_ref().unwrap_or(&initial.ty), &types);
            if ty != current.ty { facts.push(current.proof.fact(name, Vec::new(), ty)); }
        }
        self.apply_narrowings(&facts);
    }

    fn check_while_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        condition: ExprId,
        block: BlockId,
    ) {
        let narrowings =
            self.check_condition_arena(arena, source, condition, "check.while-condition");
        self.push_scope();
        self.apply_narrowings(&narrowings.when_true);
        self.bind_pattern_condition_arena(arena, source, condition);
        self.loop_depth += 1;
        self.check_block_arena(arena, source, block);
        self.loop_depth -= 1;
        self.pop_scope();
    }

    fn check_for_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        target: BindingTargetId,
        iter: ExprId,
        block: BlockId,
        span: Span,
    ) {
        let iter_ty = self.check_expr_arena(arena, source, iter, None);
        if matches!(&iter_ty, Type::Result(ok, _) if matches!(ok.as_ref(), Type::Map(_, _) | Type::Str | Type::Bytes)) {
            self.check_propagation(&iter_ty, arena.arena.expr(iter).span);
        }
        let item_ty = iter_ty.iteration_item_type().unwrap_or_else(|| match iter_ty {
            Type::Any => Type::Any,
            Type::Unknown => Type::Unknown,
            _ => {
                self.error(arena.arena.expr(iter).span, "`for` iterates over List, Stream, Map, Str, or Bytes values", "check.for-iterator");
                Type::Unknown
            }
        });
        self.push_scope();
        self.define_binding_target_arena(arena, target, &item_ty, false, span);
        self.loop_depth += 1;
        self.check_block_arena(arena, source, block);
        self.loop_depth -= 1;
        self.pop_scope();
    }

    fn check_match_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        value: ExprId,
        arms: ArenaRange,
    ) {
        let value_ty = self.check_expr_arena(arena, source, value, None);
        let arm_list = arena.arena.match_arms(arms);
        for arm in arm_list {
            self.push_scope();
            self.check_pattern_arena(arena, source, arm.pattern, &value_ty);
            if let Some(value) = Self::single_error_handler_value_arena(arena, arm.block) {
                self.warn_flattened_error_handler_arena(arena, value, arm.pattern, &value_ty);
            }
            if let Some(guard) = arm.guard {
                let guard_ty = self.check_expr_with_schema_arena(arena, source, ArenaExprOrRun::Expr(guard), Some(&Type::Bool), None);
                let guard_span = arena.arena.expr(guard).span;
                self.expect_type(&Type::Bool, &guard_ty, guard_span);
            }
            self.check_block_arena(arena, source, arm.block);
            self.pop_scope();
        }
        let value_span = arena.arena.expr(value).span;
        self.check_list_match_coverage_arena(arena, &value_ty, arm_list.iter().map(|arm| (arm.pattern, arena.arena.span(arm.span), arm.guard.is_some())), value_span);
        self.check_tag_exhaustiveness_arena(
            arena,
            &value_ty,
            arm_list
                .iter()
                .filter(|arm| arm.guard.is_none())
                .map(|arm| (arm.pattern, arena.arena.span(arm.span)))
                .collect(),
            value_span,
        );
    }

    fn check_with_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        bindings: ArenaRange,
        body: BlockId,
        else_block: BlockId,
        _span: Span,
    ) {
        self.push_scope();
        let mut error_ty = None;
        for binding in arena.arena.with_bindings(bindings) {
            let previous_errors = self.with_initializer_errors.replace(Vec::new());
            let ty = self.check_expr_arena(arena, source, binding.initializer, None);
            let mut errors = self.with_initializer_errors.take().unwrap_or_default();
            self.with_initializer_errors = previous_errors;
            if let Type::Result(_, error) = &ty { errors.push((**error).clone()); }
            for error in errors {
                error_ty = Some(match error_ty {
                    None => error,
                    Some(previous) if previous == error => previous,
                    Some(_) => Type::Error,
                });
            }
            let value_ty = match ty { Type::Result(ok, _) => *ok, other => other };
            let binding_span = arena.arena.span(binding.span);
            if self.current_scope().contains_key(&binding.name) {
                self.error(binding_span, "duplicate name in scope", "check.duplicate-name");
            }
            if binding.name.as_str() != "_" {
                self.define(binding.name, Binding::new(value_ty, false), binding_span);
            }
        }
        self.check_block_arena(arena, source, body);
        self.pop_scope();
        let error_ty = error_ty.unwrap_or(Type::Error);
        self.handler_input_types.insert(arena.arena.span(arena.arena.block(else_block).span), error_ty.clone());
        self.check_error_handler_block_arena(arena, source, else_block, &error_ty);
    }

    fn check_guard_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        target: BindingTargetId,
        ty: Option<TypeExprId>,
        initializer: ArenaExprOrRun,
        else_block: BlockId,
        span: Span,
    ) {
        let expected = ty.map(|id| Type::Result(Box::new(self.type_from_arena(arena, id)), Box::new(Type::Error)));
        let schema = ty.and_then(|id| self.record_constructors.annotation_expectation(&arena.arena, id, self.current_namespace).ok()).map(|schema|
            crate::sema::constants::SchemaExpectation { instances: Vec::new(), children: std::collections::BTreeMap::from([(crate::sema::constants::SchemaComponent::Success, schema)]) });
        let init_ty = self.check_expr_with_schema_arena(arena, source, initializer, expected.as_ref(), schema);
        let (ok_ty, error_ty) = match init_ty {
            Type::Result(ok, error) => (*ok, *error),
            Type::Unknown => (Type::Unknown, Type::Unknown),
            other => {
                self.error(span, "`guard let` binding must produce a Result value", "check.guard-binding");
                (other, Type::Error)
            }
        };
        if record_target_requires_schema_check(arena, target, &ok_ty) {
            self.error(span, "record destructuring of Any requires an explicit schema check", "check.destructure-type");
        }
        let bind_ty = if let Some(ty_id) = ty {
            let ann = self.type_from_arena(arena, ty_id);
            self.expect_type(&ann, &ok_ty, span);
            ann
        } else { ok_ty };
        self.check_error_handler_block_arena(arena, source, else_block, &error_ty);
        self.define_binding_target_arena(arena, target, &bind_ty, false, span);
    }

    fn check_error_handler_block_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        block: BlockId,
        error_ty: &Type,
    ) {
        let params = arena.arena.block_params(arena.arena.block(block).params);
        self.push_scope();
        if params.len() > 1 {
            self.error(arena.arena.span(params[1].span), "an error handler accepts at most one parameter", "check.handler-block-params");
        }
        for param in params {
            if param.name.as_str() == "_" { continue; }
            let span = arena.arena.span(param.span);
            if self.current_scope().contains_key(&param.name) {
                self.error(span, "duplicate name in scope", "check.duplicate-name");
            }
            self.define(param.name, Binding::new(error_ty.clone(), false), span);
        }
        self.check_statement_block_contents_arena(arena, source, block);
        self.pop_scope();
    }

    pub(super) fn check_value_block_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        block_id: BlockId,
        expected: &Type,
    ) {
        let block = arena.arena.block(block_id);
        if let Some(param) = arena.arena.block_params(block.params).first() {
            let param_span = arena.arena.span(param.span);
            self.error(
                param_span,
                "this block does not receive parameters",
                "check.block-params",
            );
        }
        self.check_value_block_contents_arena(arena, source, block_id, expected);
    }

    fn check_value_block_contents_arena(
        &mut self, arena: &ArenaProgram, source: &str, block_id: BlockId, expected: &Type,
    ) {
        let block = arena.arena.block(block_id);
        self.push_scope();
        self.block_depth += 1;
        let stmt_ids: Vec<StmtId> = arena.arena.stmt_ids(block.statements).collect();
        let block_span = arena.arena.span(block.span);
        if let Some((&tail, non_tail)) = stmt_ids.split_last() {
            let tail_producing = matches!(
                arena.arena.stmt(tail).kind,
                ArenaStmtKind::Expr(_)
                    | ArenaStmtKind::Command(_)
                    | ArenaStmtKind::TailBareIdent(_)
                    | ArenaStmtKind::Match { .. }
                    | ArenaStmtKind::If { .. }
                    | ArenaStmtKind::Return(_)
                    | ArenaStmtKind::Break { .. }
                    | ArenaStmtKind::Continue
            );
            let checked_stmts: &[StmtId] = if tail_producing { non_tail } else { &stmt_ids };
            for &stmt_id in checked_stmts {
                self.check_non_tail_stmt_arena(arena, source, stmt_id);
            }
            if tail_producing {
                let actual = self.check_tail_stmt_arena(arena, source, tail, Some(expected));
                if !tail_type_matches_expected(expected, &actual) {
                    let tail_span = arena.arena.stmt(tail).span;
                    self.expect_type(expected, &actual, tail_span);
                }
            } else if expected != &Type::Unit
                && !expected.is_result_unit()
                && !block_always_returns_arena(arena, block_id)
            {
                self.error(
                    block_span,
                    "function can fall through without returning its declared type",
                    "check.missing-return",
                );
            }
        } else if expected != &Type::Unit && !expected.is_result_unit() {
            self.error(
                block_span,
                "function can fall through without returning its declared type",
                "check.missing-return",
            );
        }
        self.block_depth -= 1;
        self.pop_scope();
    }

    // Deferred bodies read mutable captures after the declaration's current proofs can expire.
    pub(super) fn push_deferred_capture_scope(&mut self) {
        let visible = self.scopes.iter().flat_map(|scope| scope.iter())
            .map(|(name, binding)| (*name, binding.clone())).collect::<FxHashMap<_, _>>();
        self.push_scope();
        for (name, mut binding) in visible {
            if binding.mutable {
                binding.proof.mutate(&[]);
                if let Some(original) = binding.unrefined_ty.take() { binding.ty = original; }
                binding.pure_local_mutation = false;
                self.current_scope_mut().insert(name, binding);
            }
        }
    }

    pub(super) fn check_function_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        def: &ArenaFunctionDef,
        pure: bool,
    ) {
        let body_span = arena.arena.span(arena.arena.block(def.body).span);
        if def.test_declaration {
            if self.current_exported {
                self.error(body_span, "test declarations cannot be exported", "check.test-export");
            }
            if self.block_depth != 0 || self.current_return.is_some() {
                self.error(body_span, "test declarations must be top-level", "check.test-nested");
            }
        }
        if pure && def.return_ty_defaulted && self.inferred_returns.is_none()
            && self.function_return_types.contains_key(&body_span) {
            return;
        }
        let saved_capture_scopes = self.scopes.clone();
        self.collect_function_local_constraints(arena, source, def, pure);
        let previous_errors = self.with_initializer_errors.take();
        let previous_defer = std::mem::replace(&mut self.in_defer_block, false);
        let previous_boundary_depth = std::mem::replace(&mut self.retry_attempt_depth, 0);
        let previous_boundary_errors = std::mem::take(&mut self.error_boundary_errors);
        let previous_context_scopes = std::mem::take(&mut self.context_scope_depths);
        let previous_return = self.current_return.clone();
        let previous_return_schema = self.return_schema.clone();
        let previous_expected_schema = self.expected_schema.clone();
        let previous_pure = self.in_pure;
        let previous_effects = self.current_effects.clone();
        let previous_effect_owner = self.effect_owner;
        self.effect_owner = (!pure).then(|| self.effect_declaration_id(arena, def.body));
        // A proc return probe infers only the top-level declaration it targets.
        let inferring = def.return_ty_defaulted && self.inferred_returns.is_some() && (pure || previous_return.is_none());
        let outer_inference = if inferring { None } else { self.inferred_returns.take() };
        let inferred_proc_return = (!pure && def.return_ty_defaulted && !inferring)
            .then(|| self.function_return_types.get(&body_span).cloned()).flatten();
        let return_ty = if inferring { Type::Unknown } else {
            inferred_proc_return.clone().unwrap_or_else(|| self.type_from_arena(arena, def.return_ty))
        };
        if !inferring { self.function_return_types.insert(body_span, return_ty.clone()); }
        self.return_schema = (!inferring && inferred_proc_return.is_none()).then(|| self.record_constructors.annotation_expectation(&arena.arena, def.return_ty, self.current_namespace).ok()).flatten();
        self.expected_schema = self.return_schema.clone();
        self.current_return = Some(return_ty.clone());
        self.in_pure = pure;
        self.current_effects = if pure {
            None
        } else {
            self.effective_function_effects(arena, def)
        };
        self.push_deferred_capture_scope();
        let mut saw_default = false;
        let mut param_types = Vec::new();
        let mut names = FxHashSet::default();
        let params = arena.arena.params(def.params);
        for (index, param) in params.iter().enumerate() {
            let param_span = arena.arena.span(param.span);
            if !names.insert(param.name) {
                self.error(
                    param_span,
                    "duplicate name in scope",
                    "check.duplicate-name",
                );
            }
            if param.rest && index + 1 != params.len() {
                self.error(
                    param_span,
                    "rest parameters must be last",
                    "check.rest-position",
                );
            }
            let param_ty = self.infer_checked_parameter(arena, source, param);
            if param.rest && !matches!(param_ty, Type::List(_)) {
                self.error(
                    arena.arena.type_expr_span(param.ty),
                    "rest parameters require a List type",
                    "check.rest-type",
                );
            }
            if param.default.is_some() {
                saw_default = true;
            } else if saw_default && !param.rest {
                self.error(
                    param_span,
                    "required parameters cannot follow defaulted parameters",
                    "check.default-param",
                );
            }
            if let Some(default) = param.default {
                let actual = self.check_expr_arena(arena, source, default, Some(&param_ty));
                let default_span = arena.arena.expr(default).span;
                self.expect_type(&param_ty, &actual, default_span);
                if param.ty_defaulted && param_ty.annotation_source().is_some() {
                    self.annotation_facts.push(AnnotationFact {
                        kind: AnnotationFactKind::DefaultedParam {
                            span: param_span,
                            default: default_span,
                        },
                        ty: param_ty.clone(),
                    });
                }
            }
            let schema = (!param.ty_defaulted).then(|| self.record_constructors.annotation_expectation(&arena.arena, param.ty, self.current_namespace).ok()).flatten();
            param_types.push((param.name, param_span, param_ty, schema));
        }
        self.publish_parameter_types(arena, def);
        for (name, span, ty, schema) in param_types {
            let mut binding = Binding::new(ty, false);
            binding.schema_expectation = schema;
            self.define(name, binding, span);
        }
        if inferring {
            let tail = self.check_tail_block_arena(arena, source, def.body, None);
            if tail != Type::Unknown {
                self.inferred_returns.as_mut().unwrap().push((tail, body_span));
            }
        } else {
            if def.test_declaration {
                if !cfg!(feature = "native-tests") {
                    self.error(body_span, "test declarations require native-test support; use an xsht build with the native-tests feature", "check.test-feature-disabled");
                }
                self.check_value_block_contents_arena(arena, source, def.body, &return_ty);
            } else {
                self.check_value_block_arena(arena, source, def.body, &return_ty);
            }
        }
        if !pure
            && self.current_exported
            && def.return_ty_defaulted
            && return_ty == Type::Result(Box::new(Type::Unit), Box::new(Type::Error))
        {
            let body_span = arena.arena.span(arena.arena.block(def.body).span);
            self.annotation_facts.push(AnnotationFact {
                kind: AnnotationFactKind::ExportedProcReturn { body: body_span },
                ty: return_ty.clone(),
            });
        }
        self.pop_scope();
        if !inferring { self.inferred_returns = outer_inference; }
        self.scopes = saved_capture_scopes;
        self.current_return = previous_return;
        self.return_schema = previous_return_schema;
        self.expected_schema = previous_expected_schema;
        self.in_pure = previous_pure;
        self.current_effects = previous_effects;
        self.effect_owner = previous_effect_owner;
        self.in_defer_block = previous_defer;
        self.with_initializer_errors = previous_errors;
        self.retry_attempt_depth = previous_boundary_depth;
        self.error_boundary_errors = previous_boundary_errors;
        self.context_scope_depths = previous_context_scopes;
    }

    pub(super) fn check_stream_function_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        def: &ArenaFunctionDef,
    ) {
        let saved_capture_scopes = self.scopes.clone();
        self.collect_stream_local_constraints(arena, source, def);
        let previous_errors = self.with_initializer_errors.take();
        let previous_defer = std::mem::replace(&mut self.in_defer_block, false);
        let previous_boundary_depth = std::mem::replace(&mut self.retry_attempt_depth, 0);
        let previous_boundary_errors = std::mem::take(&mut self.error_boundary_errors);
        let previous_context_scopes = std::mem::take(&mut self.context_scope_depths);
        let previous_return = self.current_return.clone();
        let previous_yield = self.current_yield.clone();
        let previous_pure = self.in_pure;
        let previous_effects = self.current_effects.clone();
        let previous_effect_owner = self.effect_owner;
        self.effect_owner = Some(self.effect_declaration_id(arena, def.body));
        let return_ty = self.type_from_arena(arena, def.return_ty);
        let item_ty = match return_ty {
            Type::Stream(item) => *item,
            Type::Unknown | Type::Invalid => Type::Unknown,
            _ => {
                self.error(
                    arena.arena.type_expr_span(def.return_ty),
                    "stream producer must return Stream[T]",
                    "check.stream-return",
                );
                Type::Unknown
            }
        };
        self.current_return = Some(Type::Unit);
        self.current_yield = Some(item_ty);
        self.in_pure = false;
        self.current_effects = def
            .effects
            .map(|effects| arena.arena.effects(effects).collect());
        self.push_deferred_capture_scope();
        let mut saw_default = false;
        let mut param_types = Vec::new();
        let mut names = FxHashSet::default();
        let params = arena.arena.params(def.params);
        for (index, param) in params.iter().enumerate() {
            let param_span = arena.arena.span(param.span);
            if !names.insert(param.name) {
                self.error(
                    param_span,
                    "duplicate name in scope",
                    "check.duplicate-name",
                );
            }
            if param.rest && index + 1 != params.len() {
                self.error(
                    param_span,
                    "rest parameters must be last",
                    "check.rest-position",
                );
            }
            let param_ty = self.infer_checked_parameter(arena, source, param);
            if param.rest && !matches!(param_ty, Type::List(_)) {
                self.error(
                    arena.arena.type_expr_span(param.ty),
                    "rest parameters require a List type",
                    "check.rest-type",
                );
            }
            if param.default.is_some() {
                saw_default = true;
            } else if saw_default && !param.rest {
                self.error(
                    param_span,
                    "required parameters cannot follow defaulted parameters",
                    "check.default-param",
                );
            }
            if let Some(default) = param.default {
                let actual = self.check_expr_arena(arena, source, default, Some(&param_ty));
                let default_span = arena.arena.expr(default).span;
                self.expect_type(&param_ty, &actual, default_span);
            }
            let schema = (!param.ty_defaulted).then(|| self.record_constructors.annotation_expectation(&arena.arena, param.ty, self.current_namespace).ok()).flatten();
            param_types.push((param.name, param_span, param_ty, schema));
        }
        self.publish_parameter_types(arena, def);
        for (name, span, ty, schema) in param_types {
            let mut binding = Binding::new(ty, false);
            binding.schema_expectation = schema;
            self.define(name, binding, span);
        }
        self.check_value_block_arena(arena, source, def.body, &Type::Unit);
        self.pop_scope();
        self.scopes = saved_capture_scopes;
        self.current_return = previous_return;
        self.current_yield = previous_yield;
        self.in_pure = previous_pure;
        self.current_effects = previous_effects;
        self.effect_owner = previous_effect_owner;
        self.in_defer_block = previous_defer;
        self.with_initializer_errors = previous_errors;
        self.retry_attempt_depth = previous_boundary_depth;
        self.error_boundary_errors = previous_boundary_errors;
        self.context_scope_depths = previous_context_scopes;
    }

    pub(super) fn check_signal_hook_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        hook: &ArenaSignalHook,
        span: Span,
    ) {
        if self.options.interactive_commands.is_some() {
            self.error(
                span,
                "signal hooks are not supported in interactive input",
                "check.signal-hook",
            );
        }
        if self.current_exported {
            self.error(span, "signal hooks are not exported", "check.signal-hook");
        }
        if self.module_depth > 0 {
            self.error(
                span,
                "signal hooks are entry-script-only in v1",
                "check.signal-hook-module",
            );
        } else if self.block_depth > 0 || self.current_return.is_some() {
            self.error(
                span,
                "signal hooks are allowed only at the entry script top level",
                "check.signal-hook",
            );
        }

        match normalize_hook_signal(&hook.signal.as_str(), span) {
            Ok(info) => {
                if self.module_depth == 0
                    && let Some(previous) = self
                        .root_signal_hooks
                        .insert(Name::intern(&info.name), span)
                {
                    self.diagnostics.push(
                        crate::diagnostic::Diagnostic::error("duplicate signal hook")
                            .with_code("check.duplicate-signal-hook")
                            .with_label(crate::diagnostic::Label::primary(
                                span,
                                format!("duplicate hook for `{}`", info.name),
                            ))
                            .with_label(crate::diagnostic::Label::secondary(
                                previous,
                                "first hook declared here",
                            )),
                    );
                }
            }
            Err(rejection) => self.error(
                span,
                &signal_rejection_message(&hook.signal.as_str(), rejection),
                "check.signal-hook",
            ),
        }

        if hook.options.pre_cancel.as_deref().is_some_and(|duration| {
            crate::runtime::value::DurationValue::from_literal(duration).is_none()
        }) {
            self.error(
                span,
                "`--pre-cancel` expects a duration literal",
                "check.signal-hook",
            );
        }
        let body = arena.arena.block(hook.body);
        if let Some(param) = arena.arena.block_params(body.params).first() {
            let param_span = arena.arena.span(param.span);
            self.error(
                param_span,
                "signal hook blocks do not accept parameters",
                "check.signal-hook",
            );
        }

        let saved_capture_scopes = self.scopes.clone();
        self.push_deferred_capture_scope();
        let previous_return = self.current_return.clone();
        let previous_pure = self.in_pure;
        let previous_effects = self.current_effects.clone();
        let previous_effect_owner = self.effect_owner.take();
        let previous_in_signal_hook = self.in_signal_hook;
        self.current_return = Some(Type::Result(Box::new(Type::Unit), Box::new(Type::Error)));
        self.in_pure = false;
        self.current_effects = Some(arena.arena.effects(hook.effects).collect());
        self.in_signal_hook = true;
        let ty = self.check_tail_block_arena(arena, source, hook.body, None);
        self.pop_scope();
        self.scopes = saved_capture_scopes;
        self.current_return = previous_return;
        self.in_pure = previous_pure;
        self.current_effects = previous_effects;
        self.effect_owner = previous_effect_owner;
        self.in_signal_hook = previous_in_signal_hook;

        match ty {
            Type::Unit | Type::Status | Type::Unknown | Type::Invalid => {}
            Type::Result(ok, _) if *ok == Type::Unit => {}
            _ => {
                let body_span = arena.arena.span(body.span);
                self.error(
                    body_span,
                    "signal hook body must produce Unit, Status, or Result[Unit]",
                    "check.signal-hook",
                );
            }
        }
    }

    pub(super) fn check_binding_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        target: BindingTargetId,
        ty: Option<TypeExprId>,
        initializer: ArenaExprOrRun,
        mutable: bool,
        span: Span,
    ) {
        let expected = ty.map(|ty_id| self.type_from_arena(arena, ty_id));
        let schema = ty.and_then(|ty| self.record_constructors.annotation_expectation(&arena.arena, ty, self.current_namespace).ok());
        let local_expected = if ty.is_none() { self.local_binding_expectation(span) } else { None };
        let actual = self.check_expr_with_schema_arena(arena, source, initializer, expected.as_ref().or(local_expected.as_ref()), schema.clone());
        let callable_alias = if !mutable && ty.is_none() {
            match initializer {
                ArenaExprOrRun::Expr(expression) => self.resolve_callable_alias_target(arena, expression),
                _ => None,
            }
        } else { None };
        if ty.is_none() { self.record_inert_local_discard(arena, target, initializer); }
        if let Some(expected) = &expected
            && !contextual_empty_map_initializer_arena(arena, initializer, expected, &actual)
        {
            let init_span = expr_or_run_span_arena(arena, initializer);
            self.expect_type(expected, &actual, init_span);
        }
        if record_target_requires_schema_check(arena, target, &actual) {
            self.error(span, "record destructuring of Any requires an explicit schema check", "check.destructure-type");
        }
        let final_ty = if ty.is_none() { self.infer_local_binding(arena, target, initializer, mutable, span, actual) } else { expected.unwrap_or(actual) };
        if ty.is_none()
            && callable_alias.is_none()
            && should_record_binding_annotation_arena(
                arena,
                target,
                &final_ty,
                self.current_exported,
            )
        {
            let init_span = expr_or_run_span_arena(arena, initializer);
            self.annotation_facts.push(AnnotationFact {
                kind: AnnotationFactKind::Binding {
                    span,
                    initializer: init_span,
                    exported: self.current_exported,
                },
                ty: final_ty.clone(),
            });
        }
        let boolean_proof = if !mutable && final_ty == Type::Bool {
            if let ArenaExprOrRun::Expr(expr) = initializer {
                if let ArenaExprKind::Ident(name) = arena.arena.expr(expr).kind {
                    self.lookup(name).and_then(|binding| binding.boolean_proof.clone())
                } else { Some(std::sync::Arc::new(self.infer_condition_narrowings_arena(arena, expr))) }
            } else { None }
        } else { None };
        let schema = schema.or_else(|| match initializer {
            ArenaExprOrRun::Expr(expression) => self.schema_expectation_for_expr(arena, expression),
            ArenaExprOrRun::Run(_) => None,
        });
        self.define_binding_target_arena(arena, target, &final_ty, mutable, span);
        self.record_checked_local_binding(arena, target, span, &final_ty);
        if let Some(alias) = callable_alias
            && let ArenaBindingTargetKind::Name(name) = arena.arena.binding_target(target).kind {
            if self.current_exported && (alias.signature.definition.is_none() || !alias.signature.explicit_return || (!alias.pure && (alias.signature.inferred_effects || alias.signature.effects.is_none()))) {
                self.error(span, "an exported callable alias requires an explicit return and effect contract on its target", "check.callable-alias-export");
            }
            self.attach_callable_alias(name, alias, expr_or_run_span_arena(arena, initializer));
        }
        if let crate::syntax::arena::ArenaBindingTargetKind::Name(name) = arena.arena.binding_target(target).kind && let Some(binding) = self.current_scope_mut().get_mut(&name) { binding.boolean_proof = boolean_proof; }
        self.set_binding_schema_arena(arena, target, schema);
    }

    fn set_binding_schema_arena(&mut self, arena: &ArenaProgram, target: BindingTargetId, schema: Option<super::super::constants::SchemaExpectation>) {
        match &arena.arena.binding_target(target).kind {
            ArenaBindingTargetKind::Name(name) => {
                if let Some(binding) = self.current_scope_mut().get_mut(name) { binding.schema_expectation = schema; }
            }
            ArenaBindingTargetKind::Record { fields, .. } => {
                for field in arena.arena.destructure_fields(*fields) {
                    let child = schema.as_ref().and_then(|context| context.value_context().children.get(&super::super::constants::SchemaComponent::Field(field.name))).cloned();
                    self.set_binding_schema_arena(arena, field.target, child);
                }
            }
        }
    }

    /// `x = 1` without `let` or `var` is the shell and Python spelling of a
    /// declaration. A plain name gets a `let` fix (never auto-applied: the
    /// name may instead be a typo of an existing one) and is then declared from
    /// its value, so its later uses do not repeat the same mistake as
    /// unresolved names.
    fn report_undeclared_assignment(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        target: AssignTargetId,
        op: AssignOp,
        value: ArenaExprOrRun,
        span: Span,
    ) {
        let name = assign_target_root_name_arena(arena, target);
        let mut diagnostic = Diagnostic::error(format!("assignment to undefined name `{name}`; declare it with `let` or `var`"))
            .with_code("check.undefined-name")
            .with_label(Label::primary(span, "assignment to undefined name"));
        if op == AssignOp::Set && matches!(arena.arena.assign_target(target).kind, ArenaAssignTargetKind::Name(_)) {
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(Span::at(span.source_id, span.start()), "declare it with `let`", "let ").dangerous());
            self.diagnostics.push(diagnostic);
            let ty = self.check_expr_or_run_arena(arena, source, value, None);
            self.define(name, Binding::new(ty, true), span);
            return;
        }
        self.diagnostics.push(diagnostic);
    }

    fn check_assignment_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        target: AssignTargetId,
        op: AssignOp,
        value: ArenaExprOrRun,
        span: Span,
    ) {
        let name = assign_target_root_name_arena(arena, target);
        let Some(binding) = self.lookup(name).cloned() else {
            self.report_undeclared_assignment(arena, source, target, op, value, span);
            return;
        };
        if self.in_pure && !binding.pure_local_mutation {
            self.error(
                span,
                "pure functions can assign only to local `var` bindings declared inside the same pure function",
                "check.pure-assignment",
            );
        }
        if !binding.mutable {
            self.error(
                span,
                &format!("cannot assign to `{name}`: it is not a `var`; declare with `var` to allow reassignment"),
                "check.assign-let",
            );
        }
        let target_ty = self.assignment_target_type_arena(arena, source, target, binding.unrefined_ty.as_ref().unwrap_or(&binding.ty), span);
        if op == AssignOp::Set {
            let actual = self.check_expr_or_run_arena(arena, source, value, Some(&target_ty));
            let value_span = expr_or_run_span_arena(arena, value);
            if !actual.can_escape_context_scope()
                && self.context_scope_depths.last().is_some_and(|depth| self.scopes.iter().rposition(|scope| scope.contains_key(&name)).is_some_and(|owner| owner < *depth)) {
                self.error(value_span, "a live producer or host handle cannot escape through an outer assignment", "check.context-scope-escape");
            }
            self.expect_type(&target_ty, &actual, value_span);
            self.invalidate_binding_projection_arena(arena, target, name);
            return;
        }
        let rhs = self.check_expr_or_run_arena(arena, source, value, Some(&target_ty));
        let value_span = expr_or_run_span_arena(arena, value);
        let result = self.check_compound_assignment_op(op, &target_ty, &rhs, span, value_span);
        self.expect_type(&target_ty, &result, span);
        self.invalidate_binding_projection_arena(arena, target, name);
    }

    fn invalidate_binding_projection_arena(&mut self, arena: &ArenaProgram, mut target: AssignTargetId, name: Name) {
        let mut path = Vec::new();
        loop {
            match arena.arena.assign_target(target).kind {
                ArenaAssignTargetKind::Field { base, name } if path.len() < 128 => { path.push(name); target = base; }
                ArenaAssignTargetKind::Name(_) => { path.reverse(); break; }
                _ => { path.clear(); break; }
            }
        }
        let Some(identity) = self.lookup(name).map(|binding| binding.proof.clone()) else { return; };
        for scope in &mut self.scopes {
            for binding in scope.values_mut().filter(|binding| binding.proof.same_binding(&identity)) {
                if !binding.mutable { continue; }
                binding.proof.mutate(&path);
                if let Some(original) = &binding.unrefined_ty {
                    super::proof::restore_projection(&mut binding.ty, original, &path);
                }
                if path.is_empty() { binding.unrefined_ty = None; }
            }
        }
    }

    pub(super) fn invalidate_mutable_narrowings(&mut self) {
        for scope in &mut self.scopes {
            for binding in scope.values_mut() {
                if !binding.mutable { continue; }
                binding.proof.mutate(&[]);
                if let Some(original) = binding.unrefined_ty.take() { binding.ty = original; }
            }
        }
    }

    fn assignment_target_type_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        target: AssignTargetId,
        root_ty: &Type,
        span: Span,
    ) -> Type {
        match &arena.arena.assign_target(target).kind {
            ArenaAssignTargetKind::Name(_) => root_ty.clone(),
            ArenaAssignTargetKind::Field { base, name } => {
                let base_ty =
                    self.assignment_target_type_arena(arena, source, *base, root_ty, span);
                match base_ty {
                    Type::Record(fields) => fields.get(name).cloned().unwrap_or_else(|| {
                        self.error(
                            span,
                            &format!("unknown record field `{name}`"),
                            "check.unknown-field",
                        );
                        Type::Unknown
                    }),
                    Type::Unknown => Type::Unknown,
                    _ => {
                        self.error(
                            span,
                            "field assignment requires a record value",
                            "check.assign-target",
                        );
                        Type::Unknown
                    }
                }
            }
            ArenaAssignTargetKind::Index { base, index } => {
                let base_ty =
                    self.assignment_target_type_arena(arena, source, *base, root_ty, span);
                let index_ty = self.check_expr_arena(arena, source, *index, None);
                match base_ty {
                    Type::Map(key_ty, item_ty) => {
                        let index_span = arena.arena.expr(*index).span;
                        self.expect_type(&key_ty, &index_ty, index_span);
                        item_ty.as_ref().clone()
                    }
                    Type::List(item_ty) => {
                        self.expect_type(&Type::Int, &index_ty, arena.arena.expr(*index).span);
                        item_ty.as_ref().clone()
                    }
                    Type::Unknown => Type::Unknown,
                    _ => {
                        self.error(
                            span,
                            "indexed assignment requires List or Map values",
                            "check.assign-target",
                        );
                        Type::Unknown
                    }
                }
            }
        }
    }

    fn check_return_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        value: Option<ArenaExprOrRun>,
        span: Span,
    ) {
        // Scripts select an exit status with a final top-level `Int` or
        // `abort`; top-level code has no callable to return from.
        if self.current_return.is_none() {
            self.error(span, "`return` is valid only inside a callable body", "check.return-outside-callable");
        }
        if self.in_defer_block {
            self.error(span, "`return` cannot leave a deferred cleanup block", "check.defer-control-flow");
        }
        if self.in_signal_hook {
            self.error(
                span,
                "`return` is not allowed in signal hooks",
                "check.signal-hook",
            );
        }
        if self.current_yield.is_some() && value.is_some() {
            let value_span = value.map_or(span, |v| expr_or_run_span_arena(arena, v));
            self.error(
                value_span,
                "stream producer return cannot include a value",
                "check.stream-return",
            );
        }
        let expected = self.current_return.clone().unwrap_or(Type::Unit);
        if value.is_none() && expected.is_result_unit() {
            return;
        }
        let context = match value {
            Some(ArenaExprOrRun::Expr(expr)) => tail_expr_context_arena(arena, expr, Some(&expected)),
            _ => None,
        };
        let actual = value
            .map(|value| {
                let schema = self.return_schema.as_ref().map(|schema| {
                    if matches!(expected, Type::Result(_, _)) && !matches!(context, Some(Type::Result(_, _))) {
                        schema.children.get(&crate::sema::constants::SchemaComponent::Success).cloned().unwrap_or_default()
                    } else { schema.clone() }
                });
                self.check_expr_with_schema_arena(arena, source, value, context.as_ref(), schema)
            })
            .unwrap_or(Type::Unit);
        let actual = self.resolve_local_tail_type(actual, Some(&expected), span);
        if !self.context_scope_depths.is_empty() && !actual.can_escape_context_scope() {
            self.error(span, "a live producer or host handle cannot escape through a lexical return", "check.context-scope-escape");
        }
        if self.inference_reachable && let Some(returns) = &mut self.inferred_returns {
            returns.push((actual.clone(), span));
        }
        if !tail_type_matches_expected(&expected, &actual) {
            let value_span = value.map_or(span, |v| expr_or_run_span_arena(arena, v));
            self.expect_type(&expected, &actual, value_span);
        }
    }

    fn check_yield_delegation_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        value: ExprId,
        span: Span,
    ) {
        if self.in_defer_block {
            self.error(span, "`yield` is not allowed in a deferred cleanup block", "check.defer-control-flow");
        }
        self.reject_yield_in_retry(span);
        let expected = self.current_yield.clone();
        if expected.is_none() {
            self.error(span, "`yield` is valid only in stream producers", "check.yield");
        }
        // Fresh list syntax receives an item context for empty and nested
        // literals. Callable sources retain their declared List or Stream kind.
        let collection = match arena.arena.expr(value).kind {
            ArenaExprKind::List(_) | ArenaExprKind::ListComp { .. } => {
                expected.as_ref().map(|ty| Type::List(Box::new(ty.clone())))
            }
            _ => None,
        };
        let actual = self.check_expr_arena(arena, source, value, collection.as_ref());
        let value_span = arena.arena.expr(value).span;
        match actual {
            Type::List(item) | Type::Stream(item) => {
                if !self.context_scope_depths.is_empty() && !item.can_escape_context_scope() {
                    self.error(value_span, "a delegated live producer or host handle cannot escape a context", "check.context-scope-escape");
                }
                if let Some(expected) = expected {
                    self.expect_type(&expected, &item, value_span);
                }
            }
            Type::Unknown => {}
            _ => self.error(
                value_span,
                "yield delegation requires a List or Stream; handle Results explicitly",
                "check.yield-delegation",
            ),
        }
    }

    /// A retry attempt runs outside its producer's frame, so a `yield` there
    /// used to check and then fail at runtime.
    fn reject_yield_in_retry(&mut self, span: Span) {
        if self.retry_block_depth > 0 {
            self.error(span, "`yield` is not allowed inside a retry attempt", "check.yield");
        }
    }

    fn check_yield_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        value: ArenaExprOrRun,
        span: Span,
    ) {
        if self.in_defer_block {
            self.error(span, "`yield` is not allowed in a deferred cleanup block", "check.defer-control-flow");
        }
        self.reject_yield_in_retry(span);
        let expected = match self.current_yield.clone() {
            Some(ty) => ty,
            None => {
                self.error(
                    span,
                    "`yield` is valid only in stream producers",
                    "check.yield",
                );
                self.check_expr_or_run_arena(arena, source, value, None);
                return;
            }
        };
        let actual = self.check_expr_or_run_arena(arena, source, value, Some(&expected));
        let value_span = expr_or_run_span_arena(arena, value);
        if matches!(actual, Type::Stream(_)) {
            self.error(
                value_span,
                "`yield` does not accept a stream; use `yield @stream`",
                "check.yield-stream",
            );
            return;
        }
        if !self.context_scope_depths.is_empty() && !actual.can_escape_context_scope() {
            self.error(value_span, "a live producer or host handle cannot escape through yield", "check.context-scope-escape");
        }
        self.expect_type(&expected, &actual, value_span);
    }

    fn check_defer_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        value: ArenaExprOrRun,
        span: Span,
    ) {
        if self.in_pure {
            self.error(
                span,
                "`defer` is not allowed in pure functions",
                "check.pure-defer",
            );
        }
        if let ArenaExprOrRun::Expr(expr) = value
            && let ArenaExprKind::ValueBlock(block) = arena.arena.expr(expr).kind
        {
            let saved_capture_scopes = self.scopes.clone();
            let previous_errors = self.with_initializer_errors.take();
            let previous_defer = std::mem::replace(&mut self.in_defer_block, true);
            let previous_loop = std::mem::replace(&mut self.loop_depth, 0);
            self.push_scope();
            // Cleanup reads mutable captures later, after current branch refinements may expire.
            let mut captures = FxHashMap::default();
            for scope in &self.scopes {
                for (&name, binding) in scope {
                    let mut binding = binding.clone();
                    if binding.mutable {
                        binding.proof.mutate(&[]);
                        if let Some(original) = binding.unrefined_ty.take() { binding.ty = original; }
                    }
                    captures.insert(name, binding);
                }
            }
            self.current_scope_mut().extend(captures);
            let body = arena.arena.block(block);
            if let Some(param) = arena.arena.block_params(body.params).first() {
                self.error(arena.arena.span(param.span), "deferred cleanup blocks have no parameters", "check.block-params");
            }
            self.push_scope();
            self.block_depth += 1;
            for statement in arena.arena.stmt_ids(body.statements) {
                self.check_non_tail_stmt_arena(arena, source, statement);
                if let ArenaStmtKind::TailBareIdent(name) = arena.arena.stmt(statement).kind {
                    let ty = self.lookup(name).map(|binding| binding.ty.clone()).unwrap_or(Type::Unknown);
                    if !expr_ty_auto_propagates(&ty) && !ty.matches_expected(&Type::Unit) && ty != Type::Bool {
                        self.error(arena.arena.stmt(statement).span, "cleanup statement must produce Unit; use `let _ = ...` to discard a value", "check.defer-type");
                    }
                }
            }
            self.block_depth -= 1;
            self.pop_scope();
            self.pop_scope();
            self.scopes = saved_capture_scopes;
            self.in_defer_block = previous_defer;
            self.with_initializer_errors = previous_errors;
            self.loop_depth = previous_loop;
            self.expr_types.insert(arena.arena.expr(expr).span, Type::Unit);
            return;
        }
        let ty = self.check_expr_or_run_arena(arena, source, value, None);
        self.record_statement_error(&ty, span);
        match ty {
            Type::Unit | Type::Status | Type::Unknown => {}
            Type::Result(ok, _) if *ok == Type::Unit => {}
            _ => {
                let value_span = expr_or_run_span_arena(arena, value);
                self.error(
                    value_span,
                    "deferred cleanup must produce Unit, Status, or Result[Unit]",
                    "check.defer-type",
                );
            }
        }
    }

    pub(super) fn check_block_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        block_id: BlockId,
    ) {
        let block = arena.arena.block(block_id);
        if let Some(param) = arena.arena.block_params(block.params).first() {
            let param_span = arena.arena.span(param.span);
            self.error(
                param_span,
                "this block does not receive parameters",
                "check.block-params",
            );
        }
        self.push_scope();
        self.check_statement_block_contents_arena(arena, source, block_id);
        self.pop_scope();
    }

    fn check_statement_block_contents_arena(&mut self, arena: &ArenaProgram, source: &str, block_id: BlockId) {
        let block = arena.arena.block(block_id);
        self.block_depth += 1;
        let previous_reachable = self.inference_reachable;
        for stmt_id in arena.arena.stmt_ids(block.statements) {
            self.check_stmt_arena(arena, source, stmt_id);
            if self.inferred_returns.is_some() && self.return_inference_stmt_returns(arena, stmt_id) { self.inference_reachable = false; }
        }
        self.inference_reachable = previous_reachable;
        let bindings = self.scopes.iter().flat_map(|scope| scope.iter()).map(|(name, binding)| (*name, binding.clone())).collect();
        self.block_exit_bindings.insert(block_id, bindings);
        if self.block_definitely_exits_arena(arena, block_id) {
            self.definitely_exiting_block_spans.insert(arena.arena.span(block.span));
        }
        self.block_depth -= 1;
    }

    // Only checked exits count. A call that can fail still has a success continuation.
    fn block_definitely_exits_arena(&self, arena: &ArenaProgram, block: BlockId) -> bool {
        arena.arena.stmt_ids(arena.arena.block(block).statements).any(|id| self.stmt_definitely_exits_arena(arena, id))
    }

    fn stmt_definitely_exits_arena(&self, arena: &ArenaProgram, statement: StmtId) -> bool {
        match arena.arena.stmt(statement).kind {
            ArenaStmtKind::Return(_) => self.current_return.is_some(),
            ArenaStmtKind::Break { .. } | ArenaStmtKind::Continue => self.loop_depth > 0,
            ArenaStmtKind::Expr(expr) => self.expr_definitely_exits_arena(arena, expr),
            ArenaStmtKind::Let { initializer: ArenaExprOrRun::Expr(expr), .. }
            | ArenaStmtKind::Var { initializer: ArenaExprOrRun::Expr(expr), .. }
            | ArenaStmtKind::Assign { value: ArenaExprOrRun::Expr(expr), .. } => self.expr_definitely_exits_arena(arena, expr),
            ArenaStmtKind::If { branches, else_block } => {
                for branch in arena.arena.if_branches(branches) {
                    if self.expr_definitely_exits_arena(arena, branch.condition) { return true; }
                    match arena.arena.expr(branch.condition).kind {
                        ArenaExprKind::Bool(false) => continue,
                        ArenaExprKind::Bool(true) => return self.block_definitely_exits_arena(arena, branch.block),
                        _ if !self.block_definitely_exits_arena(arena, branch.block) => return false,
                        _ => {}
                    }
                }
                else_block.is_some_and(|block| self.block_definitely_exits_arena(arena, block))
            }
            ArenaStmtKind::Match { value, arms } => {
                let arms = arena.arena.match_arms(arms);
                let ty = self.expr_types.get(&arena.arena.expr(value).span).cloned().unwrap_or(Type::Unknown);
                match_is_exhaustive_arena(arena, &ty, arms, &self.type_defs, &self.tag_variants)
                    && arms.iter().all(|arm| self.block_definitely_exits_arena(arena, arm.block))
            }
            ArenaStmtKind::With { body, else_block, .. } => self.block_definitely_exits_arena(arena, body) && self.block_definitely_exits_arena(arena, else_block),
            ArenaStmtKind::BooleanGuard { condition, else_block } if matches!(arena.arena.expr(condition).kind, ArenaExprKind::Bool(false)) => self.block_definitely_exits_arena(arena, else_block),
            ArenaStmtKind::Loop { block } => !block_has_exit_point_arena(arena, block),
            ArenaStmtKind::While { condition, block } if matches!(arena.arena.expr(condition).kind, ArenaExprKind::Bool(true)) => !block_has_exit_point_arena(arena, block),
            _ => false,
        }
    }

    fn expr_definitely_exits_arena(&self, arena: &ArenaProgram, expr: ExprId) -> bool {
        if self.terminating_call_spans.contains(&arena.arena.expr(expr).span) { return true; }
        match arena.arena.expr(expr).kind {
            ArenaExprKind::ValueBlock(block) => self.block_definitely_exits_arena(arena, block),
            _ => false,
        }
    }

    pub(super) fn check_tail_block_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        block_id: BlockId,
        expected: Option<&Type>,
    ) -> Type {
        let block = arena.arena.block(block_id);
        if let Some(param) = arena.arena.block_params(block.params).first() {
            let param_span = arena.arena.span(param.span);
            self.error(
                param_span,
                "this block does not receive parameters",
                "check.block-params",
            );
        }
        self.check_tail_block_contents_arena(arena, source, block_id, expected)
    }

    pub(super) fn check_tail_block_contents_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        block_id: BlockId,
        expected: Option<&Type>,
    ) -> Type {
        let block = arena.arena.block(block_id);
        self.block_depth += 1;
        let stmt_ids: Vec<StmtId> = arena.arena.stmt_ids(block.statements).collect();
        let previous_reachable = self.inference_reachable;
        let result = if let Some((&tail, non_tail)) = stmt_ids.split_last() {
            let tail_producing = matches!(
                arena.arena.stmt(tail).kind,
                ArenaStmtKind::Expr(_)
                    | ArenaStmtKind::Command(_)
                    | ArenaStmtKind::TailBareIdent(_)
                    | ArenaStmtKind::Match { .. }
                    | ArenaStmtKind::If { .. }
                    | ArenaStmtKind::Return(_)
                    | ArenaStmtKind::Break { .. }
                    | ArenaStmtKind::Continue
            );
            for &stmt_id in non_tail {
                let previous_tail = std::mem::replace(&mut self.context_scope_tail_value, false);
                self.check_non_tail_stmt_arena(arena, source, stmt_id);
                self.context_scope_tail_value = previous_tail;
                if self.inferred_returns.is_some() && self.return_inference_stmt_returns(arena, stmt_id) { self.inference_reachable = false; }
            }
            if tail_producing {
                let ty = self.check_tail_stmt_arena(arena, source, tail, expected);
                if let Some(expected) = expected {
                    let tail_span = arena.arena.stmt(tail).span;
                    if !tail_type_matches_expected(expected, &ty) { self.expect_type(expected, &ty, tail_span); }
                }
                ty
            } else {
                self.check_stmt_arena(arena, source, tail);
                Type::Unit
            }
        } else {
            Type::Unit
        };
        let reachable = self.inference_reachable;
        self.inference_reachable = previous_reachable;
        self.block_depth -= 1;
        let always_returns = if self.inferred_returns.is_some() { self.return_inference_block_returns(arena, block_id) }
            else { block_always_returns_arena(arena, block_id) };
        if !reachable || always_returns { Type::Unknown } else { result }
    }

    /// A value branch whose block ends without a value (a trailing `let`, or
    /// nothing) completes with Unit. The block checker reports a mismatch only
    /// for a tail value, so such a branch used to satisfy any expected type
    /// and the function then failed preparation.
    fn check_unit_branch_completion(&mut self, arena: &ArenaProgram, block: BlockId, expected: Option<Type>, actual: &Type) {
        if *actual == Type::Unit
            && let Some(expected) = expected
            && !tail_type_matches_expected(&expected, actual)
        {
            self.expect_type(&expected, actual, arena.arena.span(arena.arena.block(block).span));
        }
    }

    fn return_inference_block_returns(&self, arena: &ArenaProgram, block: BlockId) -> bool {
        arena.arena.stmt_ids(arena.arena.block(block).statements)
            .any(|id| self.return_inference_stmt_returns(arena, id))
    }

    fn return_inference_stmt_returns(&self, arena: &ArenaProgram, id: StmtId) -> bool {
        match arena.arena.stmt(id).kind {
            ArenaStmtKind::Return(_) => true,
            ArenaStmtKind::With { body, else_block, .. } => {
                self.return_inference_block_returns(arena, body)
                    && self.return_inference_block_returns(arena, else_block)
            }
            ArenaStmtKind::Expr(expr) => match arena.arena.expr(expr).kind {
                ArenaExprKind::ErrorContext { block, .. } => self.return_inference_block_returns(arena, block),
                _ => false,
            },
            ArenaStmtKind::If { branches, else_block: Some(other) } => {
                arena.arena.if_branches(branches).iter().all(|branch| self.return_inference_block_returns(arena, branch.block))
                    && self.return_inference_block_returns(arena, other)
            }
            ArenaStmtKind::Match { value, arms } => {
                let Some(ty) = self.expr_types.get(&arena.arena.expr(value).span) else { return false; };
                let arms = arena.arena.match_arms(arms);
                patterns_are_exhaustive_arena(arena, ty, arms.iter().filter(|arm| arm.guard.is_none()).map(|arm| arm.pattern), &self.type_defs, &self.tag_variants)
                    && arms.iter().all(|arm| self.return_inference_block_returns(arena, arm.block))
            }
            _ => false,
        }
    }

    fn check_non_tail_stmt_arena(&mut self, arena: &ArenaProgram, source: &str, id: StmtId) {
        let stmt = arena.arena.stmt(id);
        self.statement_positions.insert(stmt.span, super::StatementPosition::Statement);
        if let ArenaStmtKind::Expr(expr_id) = stmt.kind {
            self.statement_expression_spans.insert(arena.arena.expr(expr_id).span);
            let ty = if let ArenaExprKind::ValueBlock(block) = arena.arena.expr(expr_id).kind {
                self.check_block_arena(arena, source, block);
                self.expr_types.insert(arena.arena.expr(expr_id).span, Type::Unit);
                Type::Unit
            } else {
                self.check_expr_with_schema_arena(arena, source, ArenaExprOrRun::Expr(expr_id), None, None)
            };
            self.record_inert_expression_discard(arena, ArenaExprOrRun::Expr(expr_id));
            if self.reject_bool_statement(source, &ty, stmt.span) { return; }
            self.record_statement_error(&ty, stmt.span);
            if expr_ty_auto_propagates(&ty) {
                return;
            }
            let expr_span = arena.arena.expr(expr_id).span;
            self.reject_ignored_result(&ty, expr_span);
            self.reject_discarded_value(arena, expr_id, &ty, true);
            return;
        }
        self.check_stmt_arena(arena, source, id);
    }

    pub(super) fn check_tail_stmt_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        id: StmtId,
        expected: Option<&Type>,
    ) -> Type {
        let stmt = arena.arena.stmt(id);
        self.statement_positions.insert(stmt.span, super::StatementPosition::Value);
        if expected.is_some_and(|ty| ty == &Type::Unit || ty.is_result_unit())
            && !(expected.is_some_and(Type::is_result_unit) && tail_stmt_uses_result_context_arena(arena, id)) {
            if let ArenaStmtKind::Expr(expr_id) = stmt.kind {
                let previous_tail = std::mem::replace(&mut self.context_scope_tail_value, false);
                // A statement's Unit result is produced by consuming its value;
                // it must not constrain a Bool-producing call before Bool statement
                // classification. Blocks and inferred schemas still need their
                // declared success context while checking their contents.
                let context = statement_tail_needs_value_context_arena(arena, expr_id)
                    .then(|| tail_expr_context_arena(arena, expr_id, expected)).flatten();
                let schema = self.expected_schema.as_ref().map(|schema| {
                    if expected.is_some_and(Type::is_result) && !context.as_ref().is_some_and(Type::is_result) {
                        schema.children.get(&crate::sema::constants::SchemaComponent::Success).cloned().unwrap_or_default()
                    } else { schema.clone() }
                });
                let actual = self.check_expr_with_schema_arena(arena, source, ArenaExprOrRun::Expr(expr_id), context.as_ref(), schema);
                self.context_scope_tail_value = previous_tail;
                self.record_inert_expression_discard(arena, ArenaExprOrRun::Expr(expr_id));
                if actual.is_result() {
                    if expected.is_some_and(Type::is_result_unit) { return actual; }
                    if expr_ty_auto_propagates(&actual) {
                        self.statement_positions.insert(stmt.span, super::StatementPosition::Statement);
                        return Type::Unit;
                    }
                    self.reject_ignored_result(&actual, arena.arena.expr(expr_id).span);
                }
                self.statement_expression_spans.insert(arena.arena.expr(expr_id).span);
                if !self.reject_bool_statement(source, &actual, stmt.span) {
                    self.record_statement_error(&actual, stmt.span);
                    if !actual.is_result() && !self.is_inert_expression_discard(arena.arena.expr(expr_id).span) {
                        self.expect_type(&Type::Unit, &actual, arena.arena.expr(expr_id).span);
                    }
                }
                self.statement_positions.insert(stmt.span, super::StatementPosition::Statement);
                return Type::Unit;
            }
            self.statement_positions.insert(stmt.span, super::StatementPosition::Statement);
            self.check_stmt_arena(arena, source, id);
            return Type::Unit;
        }
        match stmt.kind {
            ArenaStmtKind::Assert { .. } => {
                self.statement_positions.insert(stmt.span, super::StatementPosition::Statement);
                self.check_stmt_arena(arena, source, id);
                Type::Unit
            }
            ArenaStmtKind::Expr(expr_id) => {
                if expected.is_some_and(|ty| *ty == Type::Unit || ty.is_result_unit()) {
                    self.statement_expression_spans.insert(arena.arena.expr(expr_id).span);
                }
                let ctx = tail_expr_context_arena(arena, expr_id, expected);
                let previous = std::mem::replace(&mut self.context_scope_tail_value, true);
                let schema = self.expected_schema.as_ref().map(|schema| {
                    if matches!(expected, Some(Type::Result(_, _))) && !matches!(ctx, Some(Type::Result(_, _))) {
                        schema.children.get(&crate::sema::constants::SchemaComponent::Success).cloned().unwrap_or_default()
                    } else { schema.clone() }
                });
                let ty = self.check_expr_with_schema_arena(arena, source, ArenaExprOrRun::Expr(expr_id), ctx.as_ref(), schema);
                self.context_scope_tail_value = previous;
                self.resolve_local_tail_type(ty, expected, stmt.span)
            }
            ArenaStmtKind::TailBareIdent(name) => {
                let ty = self.check_tail_bare_ident_arena(arena, source, name, stmt.span);
                let ty = self.resolve_local_tail_type(ty, expected, stmt.span);
                if expected.is_some_and(|ty| *ty == Type::Unit || ty.is_result_unit())
                    && self.reject_bool_statement(source, &ty, stmt.span)
                { Type::Unit } else { ty }
            }
            ArenaStmtKind::Command(command_id) => {
                let command_stmt = arena.arena.command_stmt(command_id);
                if self.in_pure {
                    self.error(
                        stmt.span,
                        "commands are not allowed in pure functions",
                        "check.pure-command",
                    );
                }
                let ty = self.check_command_arena(arena, source, &command_stmt.command, stmt.span);
                // A consumed capture tail keeps its output. Unit tails have
                // already taken the statement path, which discards that output.
                if let crate::syntax::arena::ArenaCommand::Run(run) = command_stmt.command
                    && super::command::run_capture_result_type_arena(arena, run).is_some()
                {
                    return ty;
                }
                if command_stmt_asserts_success_arena(arena, &command_stmt.command) {
                    self.record_statement_error(&Type::Result(Box::new(Type::Unit), Box::new(Type::ProcessError)), stmt.span);
                    return Type::Unit;
                }
                if command_stmt.propagate || command_ty_auto_propagates(&ty) {
                    self.check_propagation(&ty, stmt.span)
                } else {
                    ty
                }
            }
            ArenaStmtKind::If { branches, else_block } => {
                let infer_branches = self.inferred_returns.is_some() && expected.is_none();
                let mut inferred = None;
                for branch in arena.arena.if_branches(branches) {
                    let narrowings = self.check_condition_arena(arena, source, branch.condition, "check.if-condition");
                    self.push_scope();
                    self.apply_narrowings(&narrowings.when_true);
                    self.bind_pattern_condition_arena(arena, source, branch.condition);
                    let actual = self.check_tail_block_arena(arena, source, branch.block, if infer_branches { None } else { expected.or(inferred.as_ref()) });
                    self.pop_scope();
                    if !infer_branches { self.check_unit_branch_completion(arena, branch.block, expected.or(inferred.as_ref()).cloned(), &actual); }
                    if actual != Type::Unknown {
                        inferred = Some(if infer_branches {
                            inferred.map_or(actual.clone(), |previous| self.unify_inferred_returns(previous, actual, stmt.span))
                        } else { inferred.unwrap_or(actual) });
                    }
                }
                if let Some(block) = else_block {
                    self.push_scope();
                    if arena.arena.if_branches(branches).len() == 1 {
                        let narrowings = self.infer_condition_narrowings_arena(arena, arena.arena.if_branches(branches)[0].condition);
                        self.apply_narrowings(&narrowings.when_false);
                    }
                    let actual = self.check_tail_block_arena(arena, source, block, if infer_branches { None } else { expected.or(inferred.as_ref()) });
                    self.pop_scope();
                    if !infer_branches { self.check_unit_branch_completion(arena, block, expected.or(inferred.as_ref()).cloned(), &actual); }
                    if actual != Type::Unknown {
                        inferred = Some(if infer_branches {
                            inferred.map_or(actual.clone(), |previous| self.unify_inferred_returns(previous, actual, stmt.span))
                        } else { inferred.unwrap_or(actual) });
                    }
                } else {
                    self.error(stmt.span, "value-producing if requires an else branch", "check.if-value-else");
                }
                inferred.unwrap_or(Type::Unknown)
            }
            ArenaStmtKind::Match { value, arms } => {
                self.check_tail_match_arena(arena, source, value, arms, expected)
            }
            ArenaStmtKind::Return(_) | ArenaStmtKind::Break { .. } | ArenaStmtKind::Continue => {
                self.check_stmt_arena(arena, source, id);
                Type::Unknown
            }
            _ => {
                self.check_stmt_arena(arena, source, id);
                Type::Unit
            }
        }
    }

    fn check_tail_match_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        value: ExprId,
        arms: ArenaRange,
        expected: Option<&Type>,
    ) -> Type {
        let value_ty = self.check_expr_arena(arena, source, value, None);
        let arm_list = arena.arena.match_arms(arms);
        let all_arms_return = match_is_exhaustive_arena(
            arena,
            &value_ty,
            arm_list,
            &self.type_defs,
            &self.tag_variants,
        ) && arm_list
            .iter()
            .all(|arm| block_always_returns_arena(arena, arm.block));
        let infer_branches = self.inferred_returns.is_some() && expected.is_none();
        let mut inferred: Option<Type> = None;
        for arm in arm_list {
            self.push_scope();
            self.check_pattern_arena(arena, source, arm.pattern, &value_ty);
            if let Some(value) = Self::single_error_handler_value_arena(arena, arm.block) {
                self.warn_flattened_error_handler_arena(arena, value, arm.pattern, &value_ty);
            }
            if let Some(guard) = arm.guard {
                let guard_ty = self.check_expr_with_schema_arena(arena, source, ArenaExprOrRun::Expr(guard), Some(&Type::Bool), None);
                let guard_span = arena.arena.expr(guard).span;
                self.expect_type(&Type::Bool, &guard_ty, guard_span);
            }
            let arm_ty = self.check_tail_block_arena(arena, source, arm.block, if infer_branches { None } else { expected.or(inferred.as_ref()) });
            if !infer_branches { self.check_unit_branch_completion(arena, arm.block, expected.or(inferred.as_ref()).cloned(), &arm_ty); }
            if arm_ty != Type::Unknown {
                inferred = Some(if infer_branches {
                    inferred.map_or(arm_ty.clone(), |previous| self.unify_inferred_returns(previous, arm_ty, arena.arena.span(arm.span)))
                } else { inferred.unwrap_or(arm_ty) });
            }
            self.pop_scope();
        }
        let value_span = arena.arena.expr(value).span;
        self.check_list_match_coverage_arena(arena, &value_ty, arm_list.iter().map(|arm| (arm.pattern, arena.arena.span(arm.span), arm.guard.is_some())), value_span);
        if !match_is_exhaustive_arena(arena, &value_ty, arm_list, &self.type_defs, &self.tag_variants) {
            let unguarded = arm_list.iter().filter(|arm| arm.guard.is_none()).map(|arm| (arm.pattern, arena.arena.span(arm.span))).collect::<Vec<_>>();
            self.report_value_match_not_exhaustive(arena, &value_ty, &unguarded, value_span);
        }
        if all_arms_return {
            Type::Unknown
        } else {
            inferred.unwrap_or(Type::Unknown)
        }
    }
}

fn assign_target_root_name_arena(arena: &ArenaProgram, target: AssignTargetId) -> Name {
    match &arena.arena.assign_target(target).kind {
        ArenaAssignTargetKind::Name(name) => *name,
        ArenaAssignTargetKind::Field { base, .. } | ArenaAssignTargetKind::Index { base, .. } => {
            assign_target_root_name_arena(arena, *base)
        }
    }
}

#[allow(dead_code)]
fn contextual_empty_map_initializer_arena(
    arena: &ArenaProgram,
    initializer: ArenaExprOrRun,
    expected: &Type,
    actual: &Type,
) -> bool {
    if !matches!(
        (expected, actual),
        (Type::Map(_, _), Type::Map(_, item)) if matches!(item.as_ref(), Type::Any)
    ) {
        return false;
    }
    let ArenaExprOrRun::Expr(expr_id) = initializer else {
        return false;
    };
    let ArenaExprKind::Call { callee, args } = arena.arena.expr(expr_id).kind else {
        return false;
    };
    if !args.is_empty() {
        return false;
    }
    let ArenaExprKind::Field { base, name } = arena.arena.expr(callee).kind else {
        return false;
    };
    matches!(arena.arena.expr(base).kind, ArenaExprKind::Ident(module) if module == "map")
        && name == "empty"
}

#[allow(dead_code)]
fn should_record_binding_annotation_arena(
    arena: &ArenaProgram,
    target: BindingTargetId,
    ty: &Type,
    exported: bool,
) -> bool {
    let simple_name = match &arena.arena.binding_target(target).kind {
        ArenaBindingTargetKind::Name(name) => Some(*name),
        ArenaBindingTargetKind::Record { .. } => None,
    };
    let Some(name) = simple_name else {
        return false;
    };
    if name == "_" || matches!(ty, Type::Unit) || ty.annotation_source().is_none() {
        return false;
    }
    exported || annotation_type_is_nontrivial(ty)
}

// Explicit Result tails consume the complete annotation through branch and
// block boundaries. Their Unit success payload still leaves the Result as data.
pub(super) fn tail_expr_uses_result_context_arena(arena: &ArenaProgram, expr: ExprId) -> bool {
    match arena.arena.expr(expr).kind {
        ArenaExprKind::Capture(_) | ArenaExprKind::ContextScope { .. } => true,
        ArenaExprKind::Call { callee, .. } => matches!(arena.arena.expr(callee).kind,
            ArenaExprKind::Ident(name) if name == "Ok" || name == "Err"),
        ArenaExprKind::ValueBlock(block) | ArenaExprKind::ErrorContext { block, .. } =>
            arena.arena.stmt_ids(arena.arena.block(block).statements).last()
                .is_some_and(|tail| tail_stmt_uses_result_context_arena(arena, tail)),
        ArenaExprKind::Match { arms, .. } => arena.arena.match_expr_arms(arms).iter()
            .any(|arm| tail_expr_uses_result_context_arena(arena, arm.value)),
        ArenaExprKind::If { branches, else_value } =>
            tail_expr_uses_result_context_arena(arena, else_value)
                || arena.arena.if_expr_branches(branches).iter()
                    .any(|branch| tail_expr_uses_result_context_arena(arena, branch.value)),
        _ => false,
    }
}

fn tail_stmt_uses_result_context_arena(arena: &ArenaProgram, stmt: StmtId) -> bool {
    let block_uses_result = |block| arena.arena.stmt_ids(arena.arena.block(block).statements).last()
        .is_some_and(|tail| tail_stmt_uses_result_context_arena(arena, tail));
    match arena.arena.stmt(stmt).kind {
        ArenaStmtKind::Expr(expr) => tail_expr_uses_result_context_arena(arena, expr),
        ArenaStmtKind::Match { arms, .. } => arena.arena.match_arms(arms).iter()
            .any(|arm| block_uses_result(arm.block)),
        ArenaStmtKind::If { branches, else_block } =>
            else_block.is_some_and(block_uses_result)
                || arena.arena.if_branches(branches).iter().any(|branch| block_uses_result(branch.block)),
        _ => false,
    }
}

fn tail_expr_context_arena(
    arena: &ArenaProgram,
    expr_id: ExprId,
    expected: Option<&Type>,
) -> Option<Type> {
    let expected = expected?;
    let explicit_result = tail_expr_uses_result_context_arena(arena, expr_id);
    Some(if explicit_result { expected.clone() } else { expected.result_ok().unwrap_or(expected).clone() })
}

/// `assert ` is inserted before the statement. An unbraced match arm ends at a
/// comma, which `assert` would read as its message separator, so such an arm
/// becomes a braced block.
fn bool_statement_assert_fix(source: &str, statement: Span) -> Option<FixHint> {
    let text = source.get(statement.range())?.trim_end();
    let Some(content) = text.strip_suffix(',') else {
        // Statement-start grouping is not needed after `assert`.
        if let Some(inner) = whole_paren_group(text) {
            return Some(FixHint::replacement(Span::new(statement.source_id, statement.start(), statement.start() + text.len()), "insert `assert`", format!("assert {inner}")));
        }
        return Some(FixHint::replacement(Span::at(statement.source_id, statement.start()), "insert `assert`", "assert "));
    };
    let content = content.trim_end();
    if content.contains('#') { return None; }
    Some(FixHint::replacement(
        Span::new(statement.source_id, statement.start(), statement.start() + content.len()),
        "insert `assert` in a braced match arm",
        format!("{{ assert {content} }}"),
    ))
}

/// The contents of `text` when one pair of parentheses encloses all of it.
fn whole_paren_group(text: &str) -> Option<&str> {
    let tokens = crate::syntax::lexer::lex_spellings(text);
    if tokens.first()?.0 != crate::syntax::token::TokenTag::LParen || tokens.last()?.0 != crate::syntax::token::TokenTag::RParen {
        return None;
    }
    let mut depth = 0usize;
    for (index, (tag, _)) in tokens.iter().enumerate() {
        match tag {
            crate::syntax::token::TokenTag::LParen => depth += 1,
            crate::syntax::token::TokenTag::RParen => {
                depth -= 1;
                if depth == 0 && index + 1 != tokens.len() {
                    return None;
                }
            }
            _ => {}
        }
    }
    Some(text[1..text.len() - 1].trim())
}

fn statement_tail_needs_value_context_arena(arena: &ArenaProgram, expr: ExprId) -> bool {
    match arena.arena.expr(expr).kind {
        ArenaExprKind::ValueBlock(_) | ArenaExprKind::ErrorContext { .. }
        | ArenaExprKind::Require { schema: None, .. } => true,
        ArenaExprKind::Try(inner) => statement_tail_needs_value_context_arena(arena, inner),
        _ => false,
    }
}

fn record_target_requires_schema_check(arena: &ArenaProgram, target: BindingTargetId, ty: &Type) -> bool {
    match arena.arena.binding_target(target).kind {
        ArenaBindingTargetKind::Name(_) => false,
        ArenaBindingTargetKind::Record { fields, .. } => match ty {
            Type::Any => true,
            Type::Record(schema) => arena.arena.destructure_fields(fields).iter().any(|field| {
                schema.get(&field.name).is_some_and(|ty| record_target_requires_schema_check(arena, field.target, ty))
            }),
            _ => false,
        },
    }
}

struct CopyUpdateMistake {
    message: String,
    repair: String,
}

/// Collections are values, so `.push`, `.set`, and `.remove` return an updated
/// copy and leave the receiver unchanged. A discarded call of one of them is
/// almost always a mutation written in another language's style.
fn copy_update_mistake(arena: &ArenaProgram, expr_id: ExprId, ty: &Type) -> Option<CopyUpdateMistake> {
    let ArenaExprKind::Call { callee, .. } = arena.arena.expr(expr_id).kind else { return None; };
    let ArenaExprKind::Field { base, name } = arena.arena.expr(callee).kind else { return None; };
    let name = name.as_str();
    let name: &str = name.as_ref();
    let ArenaExprKind::Ident(receiver) = arena.arena.expr(base).kind else { return None; };
    let collection = match ty {
        Type::List(_) => "list",
        Type::Map(_, _) => "map",
        _ => return None,
    };
    let repair = match (collection, name) {
        ("list", "push") => format!("append in place with `{receiver} += [value]` on a `var`"),
        ("list", "extend") => format!("append in place with `{receiver} += other` on a `var`"),
        ("map", "push" | "set" | "remove") => format!("assign the result back with `{receiver} = {receiver}.{name}(...)` on a `var`"),
        _ => return None,
    };
    Some(CopyUpdateMistake {
        message: format!("`.{name}` returns a new {collection} and leaves `{receiver}` unchanged; this statement discards it"),
        repair,
    })
}
