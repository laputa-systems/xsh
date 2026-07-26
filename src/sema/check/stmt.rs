#![allow(clippy::single_call_fn)]

use super::TagVariantInfo;
use super::expr::expr_or_run_span_arena;
use super::pattern::collect_covered_constructors_arena;
use super::{
    AnnotationFact, AnnotationFactKind, BinaryOp, Checker, FxHashSet, Name, Span, Type, UnaryOp,
    call_arg_expr_id_arena, command_stmt_asserts_success_arena, command_ty_auto_propagates,
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

#[derive(Clone, Debug)]
struct Narrowing {
    name: Name,
    ty: Type,
}

#[derive(Clone, Debug, Default)]
struct ConditionNarrowings {
    when_true: Vec<Narrowing>,
    when_false: Vec<Narrowing>,
}

fn annotation_type_is_nontrivial(ty: &Type) -> bool {
    matches!(
        ty,
        Type::List(_)
            | Type::Map(_)
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
        ArenaStmtKind::Guard { else_block, .. } => block_has_exit_point_arena(arena, *else_block),
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
    let has_catch_all = arms.iter().any(|arm| {
        let pattern = arena.arena.pattern(arm.pattern);
        matches!(
            &pattern.kind,
            crate::syntax::arena::ArenaPatternKind::Wildcard
        ) || matches!(
            &pattern.kind,
            crate::syntax::arena::ArenaPatternKind::Binding(name)
                if !tag_variants.contains_key(name)
        )
    });
    if has_catch_all {
        return true;
    }
    if matches!(value_ty, Type::Result(_, _)) {
        let mut covered: FxHashSet<Name> = FxHashSet::default();
        for arm in arms {
            collect_covered_constructors_arena(arena, arm.pattern, &mut covered);
        }
        return covered.contains(&Name::intern("Ok")) && covered.contains(&Name::intern("Err"));
    }
    let Type::Tag(type_name) = value_ty else {
        return false;
    };
    let Some(TypeDefBody::TagUnion(variants)) = type_defs.get(type_name) else {
        return false;
    };
    let mut covered: FxHashSet<Name> = FxHashSet::default();
    for arm in arms {
        collect_covered_constructors_arena(arena, arm.pattern, &mut covered);
    }
    variants
        .iter()
        .all(|variant| covered.contains(&variant.name))
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
                if self.current_scope().contains_key(name) {
                    self.error(span, "duplicate name in scope", "check.duplicate-name");
                }
                self.check_builtin_args_shadow(&name.as_str(), span);
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
                    if self.current_scope().contains_key(&field.name) {
                        self.error(
                            field_span,
                            "duplicate name in scope",
                            "check.duplicate-name",
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
                    let binding = if self.in_pure && mutable {
                        Binding::pure_local_var(field_ty)
                    } else {
                        Binding::new(field_ty, mutable)
                    };
                    self.current_scope_mut().insert(field.name, binding);
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
        if op == AssignOp::Div
            && matches!(left, Type::Path | Type::Unknown)
            && matches!(right, Type::Str | Type::Path | Type::Unknown)
        {
            return Type::Path;
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
        if !matches!(left, Type::Int | Type::Unknown) {
            self.error(
                op_span,
                "compound assignment requires Int or Float operands",
                "check.operator-type",
            );
        }
        if !matches!(right, Type::Int | Type::Unknown) {
            self.error(
                rhs_span,
                "compound assignment requires Int operands",
                "check.operator-type",
            );
        }
        Type::Int
    }

    fn apply_narrowings(&mut self, narrowings: &[Narrowing]) {
        for narrowing in narrowings {
            let Some(binding) = self.lookup(narrowing.name).cloned() else {
                continue;
            };
            self.current_scope_mut().insert(
                narrowing.name,
                if binding.pure_local_mutation {
                    Binding::pure_local_var(narrowing.ty.clone())
                } else {
                    Binding::new(narrowing.ty.clone(), binding.mutable)
                },
            );
        }
    }

    pub(super) fn check_loop_control(&mut self, span: Span, is_break: bool) {
        if self.loop_depth > 0 {
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

    pub(super) fn check_stmt_arena(&mut self, arena: &ArenaProgram, source: &str, id: StmtId) {
        let stmt = arena.arena.stmt(id);
        match stmt.kind {
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
                if let ArenaStmtKind::Let { target, .. } = inner.kind
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
            ArenaStmtKind::ProcDef(def_id) => {
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
            ArenaStmtKind::Expr(expr_id) => {
                let ty = self.check_expr_arena(arena, source, expr_id, None);
                if !expr_ty_auto_propagates(&ty) {
                    let expr_span = arena.arena.expr(expr_id).span;
                    self.reject_ignored_result(&ty, expr_span);
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
                else_param,
                else_block,
            } => {
                self.check_with_arena(
                    arena, source, bindings, body, else_param, else_block, stmt.span,
                );
            }
            ArenaStmtKind::Guard {
                target,
                ty,
                initializer,
                else_param,
                else_block,
            } => {
                self.check_guard_arena(
                    arena,
                    source,
                    target,
                    ty,
                    initializer,
                    else_param,
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
                self.push_scope();
                if negate {
                    self.apply_narrowings(&narrowings.when_false);
                } else {
                    self.apply_narrowings(&narrowings.when_true);
                }
                self.check_stmt_arena(arena, source, inner);
                self.pop_scope();
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
            }
        }
    }

    fn check_condition_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        condition: ExprId,
        code: &'static str,
    ) -> ConditionNarrowings {
        let condition_ty = self.check_expr_arena(arena, source, condition, Some(&Type::Bool));
        if matches!(
            condition_ty,
            Type::Bool | Type::Status | Type::Any | Type::Unknown
        ) {
            return self.infer_condition_narrowings_arena(arena, condition);
        }
        let condition_span = arena.arena.expr(condition).span;
        self.error(condition_span, "condition must be Bool or Status", code);
        ConditionNarrowings::default()
    }

    fn infer_condition_narrowings_arena(
        &self,
        arena: &ArenaProgram,
        condition: ExprId,
    ) -> ConditionNarrowings {
        match arena.arena.expr(condition).kind {
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
                let mut left = self.infer_condition_narrowings_arena(arena, left);
                let right = self.infer_condition_narrowings_arena(arena, right);
                left.when_true.extend(right.when_true);
                left
            }
            ArenaExprKind::Binary {
                op: BinaryOp::Or,
                left,
                right,
            } => {
                let left = self.infer_condition_narrowings_arena(arena, left);
                let mut right = self.infer_condition_narrowings_arena(arena, right);
                right.when_false.extend(left.when_false);
                right
            }
            ArenaExprKind::Binary {
                op: BinaryOp::Eq | BinaryOp::Ne,
                left,
                right,
            } => self.infer_null_comparison_narrowings_arena(arena, condition, left, right),
            ArenaExprKind::Call { callee, args } => {
                self.infer_record_has_narrowing_arena(arena, callee, args)
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
        let Some((name, inner)) = self.null_compared_optional_binding_arena(arena, left, right)
        else {
            return ConditionNarrowings::default();
        };
        let narrowing = Narrowing { name, ty: inner };
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

    fn null_compared_optional_binding_arena(
        &self,
        arena: &ArenaProgram,
        left: ExprId,
        right: ExprId,
    ) -> Option<(Name, Type)> {
        match (arena.arena.expr(left).kind, arena.arena.expr(right).kind) {
            (ArenaExprKind::Ident(name), ArenaExprKind::Null)
            | (ArenaExprKind::Null, ArenaExprKind::Ident(name)) => {
                let Type::Optional(inner) = &self.lookup(name)?.ty else {
                    return None;
                };
                Some((name, inner.as_ref().clone()))
            }
            _ => None,
        }
    }

    fn infer_record_has_narrowing_arena(
        &self,
        arena: &ArenaProgram,
        callee: ExprId,
        args: ArenaRange,
    ) -> ConditionNarrowings {
        let ArenaExprKind::Field { base, name } = arena.arena.expr(callee).kind else {
            return ConditionNarrowings::default();
        };
        if name != "has" {
            return ConditionNarrowings::default();
        }
        let call_args = arena.arena.call_args(args);

        let (record_name, field_expr) = if matches!(arena.arena.expr(base).kind, ArenaExprKind::Ident(module) if module == "record")
            && call_args.len() == 2
        {
            let record_expr_id = call_arg_expr_id_arena(&call_args[0].kind);
            let ArenaExprKind::Ident(record_name) = arena.arena.expr(record_expr_id).kind else {
                return ConditionNarrowings::default();
            };
            (record_name, call_arg_expr_id_arena(&call_args[1].kind))
        } else if call_args.len() == 1 {
            let ArenaExprKind::Ident(record_name) = arena.arena.expr(base).kind else {
                return ConditionNarrowings::default();
            };
            (record_name, call_arg_expr_id_arena(&call_args[0].kind))
        } else {
            return ConditionNarrowings::default();
        };
        if record_name == "record" {
            return ConditionNarrowings::default();
        };
        let ArenaExprKind::Str(field_name_id) = arena.arena.expr(field_expr).kind else {
            return ConditionNarrowings::default();
        };
        let field_name = arena.arena.string_literal(field_name_id).clone();
        let Some(binding) = self.lookup(record_name) else {
            return ConditionNarrowings::default();
        };
        let mut fields = match &binding.ty {
            Type::Record(fields) => fields.clone(),
            Type::Any | Type::Unknown => return ConditionNarrowings::default(),
            _ => return ConditionNarrowings::default(),
        };
        fields.entry(Name::intern(&field_name)).or_insert(Type::Any);
        ConditionNarrowings {
            when_true: vec![Narrowing {
                name: record_name,
                ty: Type::Record(fields),
            }],
            when_false: Vec::new(),
        }
    }

    fn check_if_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        branches: ArenaRange,
        else_block: Option<BlockId>,
    ) {
        let branch_list = arena.arena.if_branches(branches);
        for branch in branch_list {
            let narrowings =
                self.check_condition_arena(arena, source, branch.condition, "check.if-condition");
            self.push_scope();
            self.apply_narrowings(&narrowings.when_true);
            self.check_block_arena(arena, source, branch.block);
            self.pop_scope();
        }
        if let Some(block) = else_block {
            if branch_list.len() == 1 {
                let narrowings =
                    self.infer_condition_narrowings_arena(arena, branch_list[0].condition);
                self.push_scope();
                self.apply_narrowings(&narrowings.when_false);
                self.check_block_arena(arena, source, block);
                self.pop_scope();
                return;
            }
            self.check_block_arena(arena, source, block);
        }
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
        let item_ty = match iter_ty {
            Type::List(item) => *item,
            Type::Stream(item) => *item,
            Type::Any => Type::Any,
            Type::Unknown => Type::Unknown,
            Type::Result(ok, _) => match *ok {
                Type::List(item) => *item,
                Type::Stream(item) => *item,
                _ => Type::Unknown,
            },
            _ => {
                let iter_span = arena.arena.expr(iter).span;
                self.error(
                    iter_span,
                    "`for` iterates over List or Stream values",
                    "check.for-iterator",
                );
                Type::Unknown
            }
        };
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
            if let Some(guard) = arm.guard {
                let guard_ty = self.check_expr_arena(arena, source, guard, Some(&Type::Bool));
                let guard_span = arena.arena.expr(guard).span;
                self.expect_type(&Type::Bool, &guard_ty, guard_span);
            }
            self.check_block_arena(arena, source, arm.block);
            self.pop_scope();
        }
        let value_span = arena.arena.expr(value).span;
        self.check_tag_exhaustiveness_arena(
            arena,
            &value_ty,
            arm_list
                .iter()
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
        else_param: Option<Name>,
        else_block: BlockId,
        span: Span,
    ) {
        self.push_scope();
        for binding in arena.arena.with_bindings(bindings) {
            let ty = self.check_expr_arena(arena, source, binding.initializer, None);
            let value_ty = match ty {
                Type::Result(ok, _) => *ok,
                Type::Unknown => Type::Unknown,
                other => {
                    let binding_span = arena.arena.span(binding.span);
                    self.error(
                        binding_span,
                        "`with` bindings must produce a Result value",
                        "check.with-binding",
                    );
                    other
                }
            };
            let binding_span = arena.arena.span(binding.span);
            self.define(binding.name, Binding::new(value_ty, false), binding_span);
        }
        self.check_block_arena(arena, source, body);
        self.pop_scope();
        self.push_scope();
        if let Some(param) = else_param {
            self.define(param, Binding::new(Type::Error, false), span);
        }
        self.check_block_arena(arena, source, else_block);
        self.pop_scope();
    }

    fn check_guard_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        target: BindingTargetId,
        ty: Option<TypeExprId>,
        initializer: ArenaExprOrRun,
        else_param: Option<Name>,
        else_block: BlockId,
        span: Span,
    ) {
        let init_ty = self.check_expr_or_run_arena(arena, source, initializer, None);
        let ok_ty = match init_ty {
            Type::Result(ok, _) => *ok,
            Type::Unknown => Type::Unknown,
            other => {
                self.error(
                    span,
                    "`guard let` binding must produce a Result value",
                    "check.guard-binding",
                );
                other
            }
        };
        let bind_ty = if let Some(ty_id) = ty {
            let ann = self.type_from_arena(arena, ty_id);
            self.expect_type(&ann, &ok_ty, span);
            ann
        } else {
            ok_ty
        };
        self.push_scope();
        if let Some(param) = else_param {
            self.define(param, Binding::new(Type::Error, false), span);
        }
        self.check_block_arena(arena, source, else_block);
        self.pop_scope();
        self.define_binding_target_arena(arena, target, &bind_ty, false, span);
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
                "block parameters are valid only in stream stages",
                "check.block-params",
            );
        }
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

    pub(super) fn check_function_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        def: &ArenaFunctionDef,
        pure: bool,
    ) {
        let previous_return = self.current_return.clone();
        let previous_pure = self.in_pure;
        let previous_effects = self.current_effects.clone();
        let return_ty = self.type_from_arena(arena, def.return_ty);
        self.current_return = Some(return_ty.clone());
        self.in_pure = pure;
        self.current_effects = if pure {
            None
        } else {
            def.effects
                .map(|effects| arena.arena.effects(effects).collect())
        };
        self.push_scope();
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
            let param_ty = self.type_from_arena(arena, param.ty);
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
            param_types.push((param.name, param_span, param_ty));
        }
        for (name, span, ty) in param_types {
            self.define(name, Binding::new(ty, false), span);
        }
        self.check_value_block_arena(arena, source, def.body, &return_ty);
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
        self.current_return = previous_return;
        self.in_pure = previous_pure;
        self.current_effects = previous_effects;
    }

    pub(super) fn check_stream_function_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        def: &ArenaFunctionDef,
    ) {
        let previous_return = self.current_return.clone();
        let previous_yield = self.current_yield.clone();
        let previous_pure = self.in_pure;
        let previous_effects = self.current_effects.clone();
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
        self.push_scope();
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
            let param_ty = self.type_from_arena(arena, param.ty);
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
            param_types.push((param.name, param_span, param_ty));
        }
        for (name, span, ty) in param_types {
            self.define(name, Binding::new(ty, false), span);
        }
        self.check_value_block_arena(arena, source, def.body, &Type::Unit);
        self.pop_scope();
        self.current_return = previous_return;
        self.current_yield = previous_yield;
        self.in_pure = previous_pure;
        self.current_effects = previous_effects;
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

        let previous_return = self.current_return.clone();
        let previous_pure = self.in_pure;
        let previous_effects = self.current_effects.clone();
        let previous_in_signal_hook = self.in_signal_hook;
        self.current_return = Some(Type::Result(Box::new(Type::Unit), Box::new(Type::Error)));
        self.in_pure = false;
        self.current_effects = Some(arena.arena.effects(hook.effects).collect());
        self.in_signal_hook = true;
        let ty = self.check_tail_block_arena(arena, source, hook.body, None);
        self.current_return = previous_return;
        self.in_pure = previous_pure;
        self.current_effects = previous_effects;
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
        let actual = self.check_expr_or_run_arena(arena, source, initializer, expected.as_ref());
        if let Some(expected) = &expected
            && !contextual_empty_map_initializer_arena(arena, initializer, expected, &actual)
        {
            let init_span = expr_or_run_span_arena(arena, initializer);
            self.expect_type(expected, &actual, init_span);
        }
        let final_ty = expected.unwrap_or(actual);
        if ty.is_none()
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
        self.define_binding_target_arena(arena, target, &final_ty, mutable, span);
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
            self.error(span, "assignment to undefined name", "check.undefined-name");
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
                "assignment to immutable `let` binding",
                "check.assign-let",
            );
        }
        let target_ty = self.assignment_target_type_arena(arena, source, target, &binding.ty, span);
        if op == AssignOp::Set {
            let actual = self.check_expr_or_run_arena(arena, source, value, Some(&target_ty));
            let value_span = expr_or_run_span_arena(arena, value);
            self.expect_type(&target_ty, &actual, value_span);
            return;
        }
        let rhs = self.check_expr_or_run_arena(arena, source, value, None);
        let value_span = expr_or_run_span_arena(arena, value);
        let result = self.check_compound_assignment_op(op, &target_ty, &rhs, span, value_span);
        self.expect_type(&target_ty, &result, span);
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
                    Type::Map(item_ty) => {
                        let index_span = arena.arena.expr(*index).span;
                        self.expect_type(&Type::Str, &index_ty, index_span);
                        item_ty.as_ref().clone()
                    }
                    Type::Unknown => Type::Unknown,
                    _ => {
                        self.error(
                            span,
                            "indexed assignment currently supports Map values",
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
        let actual = value
            .map(|value| self.check_expr_or_run_arena(arena, source, value, None))
            .unwrap_or(Type::Unit);
        if !tail_type_matches_expected(&expected, &actual) {
            let value_span = value.map_or(span, |v| expr_or_run_span_arena(arena, v));
            self.expect_type(&expected, &actual, value_span);
        }
    }

    fn check_yield_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        value: ArenaExprOrRun,
        span: Span,
    ) {
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
                "`yield` does not accept a stream; use `for item in stream { yield item }`",
                "check.yield-stream",
            );
            return;
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
        let ty = self.check_expr_or_run_arena(arena, source, value, None);
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
                "block parameters are valid only in stream stages",
                "check.block-params",
            );
        }
        self.push_scope();
        self.block_depth += 1;
        for stmt_id in arena.arena.stmt_ids(block.statements) {
            self.check_stmt_arena(arena, source, stmt_id);
        }
        self.block_depth -= 1;
        self.pop_scope();
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
                "block parameters are valid only in stream stages",
                "check.block-params",
            );
        }
        self.block_depth += 1;
        let stmt_ids: Vec<StmtId> = arena.arena.stmt_ids(block.statements).collect();
        let result = if let Some((&tail, non_tail)) = stmt_ids.split_last() {
            let tail_producing = matches!(
                arena.arena.stmt(tail).kind,
                ArenaStmtKind::Expr(_)
                    | ArenaStmtKind::Command(_)
                    | ArenaStmtKind::TailBareIdent(_)
                    | ArenaStmtKind::Match { .. }
            );
            for &stmt_id in non_tail {
                self.check_non_tail_stmt_arena(arena, source, stmt_id);
            }
            if tail_producing {
                let ty = self.check_tail_stmt_arena(arena, source, tail, expected);
                if let Some(expected) = expected {
                    let tail_span = arena.arena.stmt(tail).span;
                    self.expect_type(expected, &ty, tail_span);
                }
                ty
            } else {
                self.check_stmt_arena(arena, source, tail);
                Type::Unit
            }
        } else {
            Type::Unit
        };
        self.block_depth -= 1;
        result
    }

    fn check_non_tail_stmt_arena(&mut self, arena: &ArenaProgram, source: &str, id: StmtId) {
        let stmt = arena.arena.stmt(id);
        if let ArenaStmtKind::Expr(expr_id) = stmt.kind {
            let ty = self.check_expr_arena(arena, source, expr_id, None);
            if expr_ty_auto_propagates(&ty) {
                return;
            }
            let expr_span = arena.arena.expr(expr_id).span;
            self.reject_ignored_result(&ty, expr_span);
            if !ty.matches_expected(&Type::Unit) {
                let message = format!(
                    "expression statement must be last to produce a value: expression has type `{ty}`; use `let _ = ...` to discard it"
                );
                self.error(expr_span, &message, "check.non-tail-expression");
            }
            return;
        }
        self.check_stmt_arena(arena, source, id);
    }

    fn check_tail_stmt_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        id: StmtId,
        expected: Option<&Type>,
    ) -> Type {
        let stmt = arena.arena.stmt(id);
        match stmt.kind {
            ArenaStmtKind::Expr(expr_id) => {
                let ctx = tail_expr_context_arena(arena, expr_id, expected);
                self.check_expr_arena(arena, source, expr_id, ctx.as_ref())
            }
            ArenaStmtKind::TailBareIdent(name) => {
                self.check_tail_bare_ident_arena(arena, source, name, stmt.span)
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
                if command_stmt_asserts_success_arena(arena, &command_stmt.command) {
                    return Type::Unit;
                }
                if command_stmt.propagate || command_ty_auto_propagates(&ty) {
                    self.check_propagation(&ty, stmt.span)
                } else {
                    ty
                }
            }
            ArenaStmtKind::Match { value, arms } => {
                self.check_tail_match_arena(arena, source, value, arms)
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
        let mut inferred: Option<Type> = None;
        for arm in arm_list {
            self.push_scope();
            self.check_pattern_arena(arena, source, arm.pattern, &value_ty);
            if let Some(guard) = arm.guard {
                let guard_ty = self.check_expr_arena(arena, source, guard, Some(&Type::Bool));
                let guard_span = arena.arena.expr(guard).span;
                self.expect_type(&Type::Bool, &guard_ty, guard_span);
            }
            let arm_ty = self.check_tail_block_arena(arena, source, arm.block, inferred.as_ref());
            if inferred.is_none() && !matches!(arm_ty, Type::Unknown) {
                inferred = Some(arm_ty);
            }
            self.pop_scope();
        }
        let value_span = arena.arena.expr(value).span;
        self.check_tag_exhaustiveness_arena(
            arena,
            &value_ty,
            arm_list
                .iter()
                .map(|arm| (arm.pattern, arena.arena.span(arm.span)))
                .collect(),
            value_span,
        );
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
        (Type::Map(_), Type::Map(item)) if matches!(item.as_ref(), Type::Any)
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

#[allow(dead_code)]
fn tail_expr_context_arena(
    arena: &ArenaProgram,
    expr_id: ExprId,
    expected: Option<&Type>,
) -> Option<Type> {
    let is_empty_list = matches!(
        arena.arena.expr(expr_id).kind,
        ArenaExprKind::List(range) if range.is_empty()
    );
    if !is_empty_list {
        return None;
    }
    match expected {
        Some(Type::List(_)) => expected.cloned(),
        Some(Type::Result(ok, _)) if matches!(ok.as_ref(), Type::List(_)) => Some(*ok.clone()),
        _ => None,
    }
}
