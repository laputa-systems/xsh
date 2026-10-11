use super::{
    ArenaAssignTargetKind, ArenaBindingTargetKind, ArenaBuilderEntryKind, ArenaCallArgKind,
    ArenaCommand, ArenaCommandArg, ArenaCommandArgKind, ArenaEnvAssignmentValue, ArenaExprKind,
    ArenaExprOrRun, ArenaRange, ArenaRecordFieldKind, ArenaRedirectionTarget, ArenaSpawnTarget,
    ArenaStmtKind, ArenaSugar, ArenaSugarOperand, ArenaTypeExprKind, ArenaWordPart, AssignOp,
    AssignTargetId, AstArena, BinaryOp, BindingTargetId, BlockId, BuilderBlockId, CommandStmtId,
    CoreCommand, Diagnostic, DiagnosticCode, ExprId, FixHint, FlowSummary, FxHashSet, Label, Linter,
    Name, RunFormId, Severity, Span, StmtId, SugarForm, Type, TypeExprId, UnaryOp,
    binding_target_contains_name, context_scope, direct_call_name, expr_child_blocks,
    expr_child_exprs, expr_may_assign_local, expr_may_have_effects, expr_references_name,
    format_binding_target, is_map_empty_call, lint_empty_sentinel, lint_optional_binding,
    lint_prefer_test_expect, lint_write_mode, list_splice_element_type_is_precise,
    list_update_argument_stable, map_comp_key_can_be_bare, pipeline_argument_expr,
    span_end_after_following_newlines, span_may_contain_comment, stmt_flow, type_expr_kind,
};

pub(super) fn return_list_type_expr(arena: &AstArena, ty: TypeExprId) -> bool {
    match type_expr_kind(arena, ty) {
        ArenaTypeExprKind::List(_) => true,
        ArenaTypeExprKind::Result { ok, .. } => {
            matches!(type_expr_kind(arena, ok), ArenaTypeExprKind::List(_))
        }
        _ => false,
    }
}

#[derive(Clone, Debug)]
pub(super) struct StreamProducerCandidate {
    function_name: xsh::frontend::symbols::Name,
    accumulator_name: xsh::frontend::symbols::Name,
    span: Span,
}

pub(super) fn collect_stream_producer_candidates(
    arena: &AstArena,
    stmts: &[StmtId],
    out: &mut Vec<StreamProducerCandidate>,
) {
    for &stmt_id in stmts {
        let inner = match arena.stmt(stmt_id).kind {
            ArenaStmtKind::Export(inner) => arena.stmt(inner).kind,
            other => other,
        };
        if let ArenaStmtKind::ProcDef(def_id) = inner {
            let def = arena.function_def(def_id);
            if return_list_type_expr(arena, def.return_ty)
                && let Some((accumulator_name, span)) = stream_producer_candidate(arena, def.body)
            {
                out.push(StreamProducerCandidate {
                    function_name: def.name,
                    accumulator_name,
                    span,
                });
            }
        }
    }
}

pub(super) fn collect_lazy_consumed_calls(
    arena: &AstArena,
    stmts: &[StmtId],
    out: &mut FxHashSet<xsh::frontend::symbols::Name>,
) {
    for &stmt_id in stmts {
        lazy_visit_stmt(arena, stmt_id, out);
    }
}

pub(super) fn lazy_visit_stmt(
    arena: &AstArena,
    stmt_id: StmtId,
    out: &mut FxHashSet<xsh::frontend::symbols::Name>,
) {
    match arena.stmt(stmt_id).kind {
        ArenaStmtKind::Use(_)
        | ArenaStmtKind::TypeDef(_)
        | ArenaStmtKind::ErrorDef(_)
        | ArenaStmtKind::Continue
        | ArenaStmtKind::TailBareIdent(_)
        | ArenaStmtKind::Return(None)
        // Old visitor::walk_stmt treats Break (with or without value) as a leaf.
        | ArenaStmtKind::Break { .. } => {}
        ArenaStmtKind::Export(inner) => lazy_visit_stmt(arena, inner, out),
        ArenaStmtKind::Let { initializer, .. } | ArenaStmtKind::Const { initializer, .. } | ArenaStmtKind::Var { initializer, .. } => {
            lazy_visit_expr_or_run(arena, &initializer, out);
        }
        ArenaStmtKind::Assign { target, value, .. } => {
            lazy_visit_assign_target(arena, target, out);
            lazy_visit_expr_or_run(arena, &value, out);
        }
        ArenaStmtKind::Return(Some(v)) | ArenaStmtKind::Defer(v, _) | ArenaStmtKind::Yield(v) => {
            lazy_visit_expr_or_run(arena, &v, out);
        }
        ArenaStmtKind::ProcDef(def) | ArenaStmtKind::CliMain(def) | ArenaStmtKind::PureDef(def) | ArenaStmtKind::StreamDef(def) => {
            lazy_visit_block(arena, arena.function_def(def).body, out);
        }
        ArenaStmtKind::SignalHook(hook) => {
            lazy_visit_block(arena, arena.signal_hook(hook).body, out);
        }
        ArenaStmtKind::If { branches, else_block } => {
            for branch in arena.if_branches(branches).to_vec() {
                lazy_visit_expr(arena, branch.condition, out);
                lazy_visit_block(arena, branch.block, out);
            }
            if let Some(block) = else_block {
                lazy_visit_block(arena, block, out);
            }
        }
        ArenaStmtKind::While { condition, block } => {
            lazy_visit_expr(arena, condition, out);
            lazy_visit_block(arena, block, out);
        }
        ArenaStmtKind::For { iter, block, .. } => {
            if let Some(name) = direct_call_name(arena, iter) {
                out.insert(name);
            }
            lazy_visit_expr(arena, iter, out);
            lazy_visit_block(arena, block, out);
        }
        ArenaStmtKind::Loop { block } => lazy_visit_block(arena, block, out),
        ArenaStmtKind::Sugar { operands, .. } => {
            for operand in arena.sugar_operands(operands) {
                match *operand {
                    ArenaSugarOperand::Expr(expr) => lazy_visit_expr(arena, expr, out),
                    ArenaSugarOperand::Block(block) => lazy_visit_block(arena, block, out),
                    ArenaSugarOperand::Stmt(stmt) => lazy_visit_stmt(arena, stmt, out),
                    _ => {}
                }
            }
        }
        ArenaStmtKind::Guard { initializer, else_block, .. } => {
            lazy_visit_expr_or_run(arena, &initializer, out);
            lazy_visit_block(arena, else_block, out);
        }
        ArenaStmtKind::Assert { condition, message } => {
            lazy_visit_expr(arena, condition, out);
            if let Some(message) = message { lazy_visit_expr(arena, message, out); }
        }
        ArenaStmtKind::With { bindings, body, else_block, .. } => {
            for binding in arena.with_bindings(bindings).to_vec() {
                lazy_visit_expr(arena, binding.initializer, out);
            }
            lazy_visit_block(arena, body, out);
            lazy_visit_block(arena, else_block, out);
        }
        ArenaStmtKind::Match { value, arms } => {
            lazy_visit_expr(arena, value, out);
            for arm in arena.match_arms(arms).to_vec() {
                if let Some(guard) = arm.guard {
                    lazy_visit_expr(arena, guard, out);
                }
                lazy_visit_block(arena, arm.block, out);
            }
        }
        ArenaStmtKind::Expr(expr)
        | ArenaStmtKind::YieldDelegate(expr)
        | ArenaStmtKind::Exit(expr) => {
            lazy_visit_expr(arena, expr, out);
        }
        ArenaStmtKind::Command(cmd_id) => {
            lazy_visit_command(arena, cmd_id, out);
        }
    }
}

pub(super) fn lazy_visit_assign_target(
    arena: &AstArena,
    target: AssignTargetId,
    out: &mut FxHashSet<xsh::frontend::symbols::Name>,
) {
    match arena.assign_target(target).kind.clone() {
        ArenaAssignTargetKind::Name(_) | ArenaAssignTargetKind::Env(_) => {}
        ArenaAssignTargetKind::Field { base, .. } => lazy_visit_assign_target(arena, base, out),
        ArenaAssignTargetKind::Index { base, index } => {
            lazy_visit_assign_target(arena, base, out);
            lazy_visit_expr(arena, index, out);
        }
    }
}

pub(super) fn lazy_visit_command(
    arena: &AstArena,
    cmd_id: CommandStmtId,
    out: &mut FxHashSet<xsh::frontend::symbols::Name>,
) {
    match arena.command_stmt(cmd_id).command.clone() {
        ArenaCommand::Proc { args, .. } => {
            for arg in arena.command_args(args).to_vec() {
                lazy_visit_command_arg(arena, &arg, out);
            }
        }
        ArenaCommand::Core {
            args, env, block, ..
        } => {
            for arg in arena.command_args(args).to_vec() {
                lazy_visit_command_arg(arena, &arg, out);
            }
            for assignment in arena.env_assignments(env).to_vec() {
                match assignment.value {
                    ArenaEnvAssignmentValue::CommandArg(arg) => {
                        lazy_visit_command_arg(arena, &arg, out);
                    }
                    ArenaEnvAssignmentValue::Expr(e) => lazy_visit_expr(arena, e, out),
                }
            }
            if let Some(block) = block {
                lazy_visit_block(arena, block, out);
            }
        }
        ArenaCommand::Run(run) => lazy_visit_run(arena, run, out),
    }
}

pub(super) fn lazy_visit_run(
    arena: &AstArena,
    run: RunFormId,
    out: &mut FxHashSet<xsh::frontend::symbols::Name>,
) {
    let segments = arena.run_form(run).segments;
    for segment in arena.run_segments(segments).to_vec() {
        if let Some(timeout) = segment.timeout {
            lazy_visit_expr(arena, timeout, out);
        }
        if let Some(cpu_max) = segment.cpu_max {
            lazy_visit_expr(arena, cpu_max, out);
        }
        if let Some(accept) = segment.accept {
            lazy_visit_expr(arena, accept, out);
        }
        for assignment in arena.env_assignments(segment.env).to_vec() {
            match assignment.value {
                ArenaEnvAssignmentValue::CommandArg(arg) => {
                    lazy_visit_command_arg(arena, &arg, out);
                }
                ArenaEnvAssignmentValue::Expr(e) => lazy_visit_expr(arena, e, out),
            }
        }
        lazy_visit_command_arg(arena, &segment.target, out);
        for arg in arena.command_args(segment.args).to_vec() {
            lazy_visit_command_arg(arena, &arg, out);
        }
        for redirection in arena.redirections(segment.redirections).to_vec() {
            match redirection.target {
                ArenaRedirectionTarget::Path(arg) | ArenaRedirectionTarget::Fd(arg) => {
                    lazy_visit_command_arg(arena, &arg, out);
                }
            }
        }
    }
}

pub(super) fn lazy_visit_command_arg(
    arena: &AstArena,
    arg: &ArenaCommandArg,
    out: &mut FxHashSet<xsh::frontend::symbols::Name>,
) {
    match arg.kind {
        ArenaCommandArgKind::Word(parts) => {
            for part in arena.word_parts(parts).collect::<Vec<_>>() {
                if let ArenaWordPart::Interpolation(e) | ArenaWordPart::Shorthand(e) = part {
                    lazy_visit_expr(arena, e, out);
                }
            }
        }
        ArenaCommandArgKind::SpliceExpr(e) | ArenaCommandArgKind::Typed(e) => {
            lazy_visit_expr(arena, e, out);
        }
        ArenaCommandArgKind::SpliceName(_) => {}
    }
}

pub(super) fn lazy_visit_block(
    arena: &AstArena,
    block: BlockId,
    out: &mut FxHashSet<xsh::frontend::symbols::Name>,
) {
    for stmt in arena
        .stmt_ids(arena.block(block).statements)
        .collect::<Vec<_>>()
    {
        lazy_visit_stmt(arena, stmt, out);
    }
}

pub(super) fn lazy_visit_expr_or_run(
    arena: &AstArena,
    value: &ArenaExprOrRun,
    out: &mut FxHashSet<xsh::frontend::symbols::Name>,
) {
    if let ArenaExprOrRun::Expr(expr) = value {
        lazy_visit_expr(arena, *expr, out);
    }
}

pub(super) fn lazy_visit_expr(
    arena: &AstArena,
    expr: ExprId,
    out: &mut FxHashSet<xsh::frontend::symbols::Name>,
) {
    match arena.expr(expr).kind {
        ArenaExprKind::StructuredPipeline { input, .. } => {
            if let Some(name) = direct_call_name(arena, input) {
                out.insert(name);
            }
        }
        ArenaExprKind::Run(run) => lazy_visit_run(arena, run, out),
        ArenaExprKind::Spawn(form) => {
            if let ArenaSpawnTarget::Run(run) = form.target {
                lazy_visit_run(arena, run, out);
            }
        }
        ArenaExprKind::BuilderCall { block, .. } => {
            lazy_visit_builder_block(arena, block, out);
        }
        _ => {}
    }
    for child in expr_child_exprs(arena, expr) {
        lazy_visit_expr(arena, child, out);
    }
    for block in expr_child_blocks(arena, expr) {
        lazy_visit_block(arena, block, out);
    }
}

pub(super) fn lazy_visit_builder_block(
    arena: &AstArena,
    block: BuilderBlockId,
    out: &mut FxHashSet<xsh::frontend::symbols::Name>,
) {
    for entry in arena
        .builder_entries(arena.builder_block(block).entries)
        .to_vec()
    {
        match entry.kind {
            ArenaBuilderEntryKind::Field { value, .. } => lazy_visit_expr(arena, value, out),
            ArenaBuilderEntryKind::Entry { args, block, .. } => {
                for arg in arena.command_args(args).to_vec() {
                    lazy_visit_command_arg(arena, &arg, out);
                }
                if let Some(block) = block {
                    lazy_visit_builder_block(arena, block, out);
                }
            }
            ArenaBuilderEntryKind::Task { block, .. } => lazy_visit_block(arena, block, out),
            ArenaBuilderEntryKind::Stmt(stmt) => lazy_visit_stmt(arena, stmt, out),
        }
    }
}

pub(super) fn stream_producer_candidate(
    arena: &AstArena,
    body: BlockId,
) -> Option<(xsh::frontend::symbols::Name, Span)> {
    let stmts: Vec<StmtId> = arena.stmt_ids(arena.block(body).statements).collect();
    let &final_stmt = stmts.last()?;
    stmts.iter().enumerate().find_map(|(index, &stmt)| {
        let (name, span) = empty_list_var(arena, stmt)?;
        let rest = &stmts[index + 1..];
        if !rest.iter().any(|&stmt| stmt_pushes_to(arena, stmt, name)) {
            return None;
        }
        if rest
            .iter()
            .any(|&stmt| stmt_assigns_non_push_to(arena, stmt, name))
        {
            return None;
        }
        if !stmt_returns_value_from(arena, final_stmt, name) {
            return None;
        }
        Some((name, span))
    })
}

pub(super) fn empty_list_var(arena: &AstArena, stmt: StmtId) -> Option<(xsh::frontend::symbols::Name, Span)> {
    let arena_stmt = arena.stmt(stmt);
    let ArenaStmtKind::Var {
        target,
        initializer: ArenaExprOrRun::Expr(init),
        ..
    } = arena_stmt.kind
    else {
        return None;
    };
    let ArenaBindingTargetKind::Name(name) = arena.binding_target(target).kind else {
        return None;
    };
    let ArenaExprKind::List(items) = arena.expr(init).kind else {
        return None;
    };
    if items.is_empty() {
        Some((name, arena_stmt.span))
    } else {
        None
    }
}

pub(super) fn stmt_pushes_to(arena: &AstArena, stmt: StmtId, name: xsh::frontend::symbols::Name) -> bool {
    if stmt_is_push_assignment(arena, stmt, name) {
        return true;
    }
    match arena.stmt(stmt).kind {
        ArenaStmtKind::If {
            branches,
            else_block,
        } => {
            arena
                .if_branches(branches)
                .iter()
                .any(|branch| block_pushes_to(arena, branch.block, name))
                || else_block.is_some_and(|block| block_pushes_to(arena, block, name))
        }
        ArenaStmtKind::While { block, .. }
        | ArenaStmtKind::For { block, .. }
        | ArenaStmtKind::Loop { block } => block_pushes_to(arena, block, name),
        ArenaStmtKind::Sugar { operands, .. } => {
            arena
                .sugar_operands(operands)
                .iter()
                .any(|operand| match *operand {
                    ArenaSugarOperand::Block(block) => block_pushes_to(arena, block, name),
                    ArenaSugarOperand::Stmt(stmt) => stmt_pushes_to(arena, stmt, name),
                    _ => false,
                })
        }
        ArenaStmtKind::Guard { else_block, .. } => block_pushes_to(arena, else_block, name),
        ArenaStmtKind::Export(stmt) => stmt_pushes_to(arena, stmt, name),
        ArenaStmtKind::With {
            body, else_block, ..
        } => block_pushes_to(arena, body, name) || block_pushes_to(arena, else_block, name),
        ArenaStmtKind::Match { arms, .. } => arena
            .match_arms(arms)
            .iter()
            .any(|arm| block_pushes_to(arena, arm.block, name)),
        _ => false,
    }
}

pub(super) fn block_pushes_to(arena: &AstArena, block: BlockId, name: xsh::frontend::symbols::Name) -> bool {
    arena
        .stmt_ids(arena.block(block).statements)
        .any(|stmt| stmt_pushes_to(arena, stmt, name))
}

pub(super) fn stmt_assigns_non_push_to(
    arena: &AstArena,
    stmt: StmtId,
    name: xsh::frontend::symbols::Name,
) -> bool {
    match arena.stmt(stmt).kind {
        ArenaStmtKind::Assign { target, .. }
            if assign_target_root_name(arena, target) == Some(name) =>
        {
            !stmt_is_push_assignment(arena, stmt, name)
        }
        ArenaStmtKind::If {
            branches,
            else_block,
        } => {
            arena
                .if_branches(branches)
                .iter()
                .any(|branch| block_assigns_non_push_to(arena, branch.block, name))
                || else_block.is_some_and(|block| block_assigns_non_push_to(arena, block, name))
        }
        ArenaStmtKind::While { block, .. }
        | ArenaStmtKind::For { block, .. }
        | ArenaStmtKind::Loop { block } => block_assigns_non_push_to(arena, block, name),
        ArenaStmtKind::Sugar { operands, .. } => {
            arena
                .sugar_operands(operands)
                .iter()
                .any(|operand| match *operand {
                    ArenaSugarOperand::Block(block) => {
                        block_assigns_non_push_to(arena, block, name)
                    }
                    ArenaSugarOperand::Stmt(stmt) => stmt_assigns_non_push_to(arena, stmt, name),
                    _ => false,
                })
        }
        ArenaStmtKind::Guard { else_block, .. } => {
            block_assigns_non_push_to(arena, else_block, name)
        }
        ArenaStmtKind::Export(stmt) => stmt_assigns_non_push_to(arena, stmt, name),
        ArenaStmtKind::With {
            body, else_block, ..
        } => {
            block_assigns_non_push_to(arena, body, name)
                || block_assigns_non_push_to(arena, else_block, name)
        }
        ArenaStmtKind::Match { arms, .. } => arena
            .match_arms(arms)
            .iter()
            .any(|arm| block_assigns_non_push_to(arena, arm.block, name)),
        _ => false,
    }
}

/// The binding an assignment writes through; an environment variable target
/// has none.
pub(super) fn assign_target_root_name(
    arena: &AstArena,
    target: AssignTargetId,
) -> Option<xsh::frontend::symbols::Name> {
    match arena.assign_target(target).kind.clone() {
        ArenaAssignTargetKind::Name(name) => Some(name),
        ArenaAssignTargetKind::Env(_) => None,
        ArenaAssignTargetKind::Field { base, .. } | ArenaAssignTargetKind::Index { base, .. } => {
            assign_target_root_name(arena, base)
        }
    }
}

pub(super) fn block_assigns_non_push_to(
    arena: &AstArena,
    block: BlockId,
    name: xsh::frontend::symbols::Name,
) -> bool {
    arena
        .stmt_ids(arena.block(block).statements)
        .any(|stmt| stmt_assigns_non_push_to(arena, stmt, name))
}

pub(super) fn stmt_is_push_assignment(
    arena: &AstArena,
    stmt: StmtId,
    name: xsh::frontend::symbols::Name,
) -> bool {
    let ArenaStmtKind::Assign {
        target,
        op: AssignOp::Set,
        value: ArenaExprOrRun::Expr(rhs),
    } = arena.stmt(stmt).kind
    else {
        return false;
    };
    let ArenaAssignTargetKind::Name(assign_name) = arena.assign_target(target).kind else {
        return false;
    };
    if assign_name != name {
        return false;
    }
    let ArenaExprKind::Call { callee, args } = arena.expr(rhs).kind else {
        return false;
    };
    let ArenaExprKind::Field {
        base,
        name: method_name,
    } = arena.expr(callee).kind
    else {
        return false;
    };
    if method_name != "push" || args.len() != 1 {
        return false;
    }
    matches!(arena.expr(base).kind, ArenaExprKind::Ident(base_name) if base_name == name)
}

pub(super) fn stmt_returns_value_from(
    arena: &AstArena,
    stmt: StmtId,
    name: xsh::frontend::symbols::Name,
) -> bool {
    match arena.stmt(stmt).kind {
        ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(expr))) => {
            expr_is_ident_or_pipeline_from(arena, expr, name)
        }
        ArenaStmtKind::Expr(expr) => expr_is_ident_or_pipeline_from(arena, expr, name),
        _ => false,
    }
}

pub(super) fn expr_is_ident_or_pipeline_from(
    arena: &AstArena,
    expr: ExprId,
    name: xsh::frontend::symbols::Name,
) -> bool {
    match arena.expr(expr).kind {
        ArenaExprKind::Ident(ident) => ident == name,
        ArenaExprKind::StructuredPipeline { input, .. } | ArenaExprKind::Pipeline { input, .. } => {
            matches!(arena.expr(input).kind, ArenaExprKind::Ident(ident) if ident == name)
        }
        _ => false,
    }
}

pub(super) fn return_value_is_ok_unit(arena: &AstArena, value: &ArenaExprOrRun) -> bool {
    let ArenaExprOrRun::Expr(expr) = value else {
        return false;
    };
    let ArenaExprKind::Call { callee, args } = arena.expr(*expr).kind else {
        return false;
    };
    matches!(arena.expr(callee).kind, ArenaExprKind::Ident(name) if name == "Ok") && args.is_empty()
}

pub(super) fn ok_call_arg(arena: &AstArena, expr: ExprId) -> Option<ExprId> {
    let ArenaExprKind::Call { callee, args } = arena.expr(expr).kind else {
        return None;
    };
    let [arg] = arena.call_args(args) else {
        return None;
    };
    let ArenaCallArgKind::Positional(arg) = arg.kind else {
        return None;
    };
    matches!(arena.expr(callee).kind, ArenaExprKind::Ident(name) if name == "Ok").then_some(arg)
}

impl<'a> Linter<'a> {
    pub(super) fn lint_record_destructuring(&mut self, stmts: &[StmtId]) {
        fn extraction(linter: &Linter<'_>, stmt: StmtId) -> Option<(Name, Vec<Name>, Name)> {
            let ArenaStmtKind::Let {
                target,
                ty: None,
                initializer: ArenaExprOrRun::Expr(mut value),
            } = linter.arena.stmt(stmt).kind
            else {
                return None;
            };
            let ArenaBindingTargetKind::Name(binding) = linter.arena.binding_target(target).kind
            else {
                return None;
            };
            if binding.as_str() == "_" {
                return None;
            }
            let mut path = Vec::new();
            while let ArenaExprKind::Field { base, name } = linter.arena.expr(value).kind {
                let Some(Type::Record(schema)) =
                    linter.expr_types.get(&linter.arena.expr(base).span)
                else {
                    return None;
                };
                if !schema.contains_key(&name) {
                    return None;
                }
                path.push(name);
                value = base;
            }
            let ArenaExprKind::Ident(root) = linter.arena.expr(value).kind else {
                return None;
            };
            if path.is_empty() || root == binding {
                return None;
            }
            path.reverse();
            Some((root, path, binding))
        }
        fn target(entries: &[(Vec<Name>, Name)], depth: usize) -> Option<String> {
            type Entries = Vec<(Vec<Name>, Name)>;
            let mut fields: Vec<(Name, Entries)> = Vec::new();
            for (path, binding) in entries {
                let field = *path.get(depth)?;
                if let Some((_, children)) = fields.iter_mut().find(|(name, _)| *name == field) {
                    if path.len() == depth + 1
                        || children.iter().any(|(path, _)| path.len() == depth + 1)
                    {
                        return None;
                    }
                    children.push((path.clone(), *binding));
                } else {
                    fields.push((field, vec![(path.clone(), *binding)]));
                }
            }
            let mut output = Vec::new();
            for (field, children) in fields {
                if children[0].0.len() == depth + 1 {
                    let binding = children[0].1;
                    output.push(if field == binding {
                        field.to_string()
                    } else {
                        format!("{field}: {binding}")
                    });
                } else {
                    output.push(format!("{field}: {}", target(&children, depth + 1)?));
                }
            }
            output.push("..".to_string());
            Some(format!("{{{}}}", output.join(", ")))
        }
        let mut index = 0;
        while index < stmts.len() {
            let Some((root, path, binding)) = extraction(self, stmts[index]) else {
                index += 1;
                continue;
            };
            let start = index;
            let mut entries = vec![(path, binding)];
            index += 1;
            while index < stmts.len() {
                let Some((next_root, path, binding)) = extraction(self, stmts[index]) else {
                    break;
                };
                if next_root != root
                    || entries
                        .iter()
                        .any(|(_, name)| *name == next_root || *name == binding)
                {
                    break;
                }
                entries.push((path, binding));
                index += 1;
            }
            if entries.len() < 2 {
                continue;
            }
            let first = self.arena.stmt(stmts[start]).span;
            let last = self.arena.stmt(stmts[index - 1]).span;
            let span = Span::new(first.source_id, first.start(), last.end());
            if self
                .source
                .get(span.range())
                .is_none_or(|source| source.contains('#'))
            {
                continue;
            }
            let Some(pattern) = target(&entries, 0) else {
                continue;
            };
            let suffix = if self
                .source
                .get(span.range())
                .is_some_and(|source| source.ends_with('\n'))
            {
                "\n"
            } else {
                ""
            };
            self.diagnostics.push(
                Diagnostic::new(
                    Severity::Warning,
                    "adjacent record field bindings can destructure their source",
                )
                .with_code(DiagnosticCode::LintPreferRecordDestructuring)
                .with_label(Label::secondary(
                    span,
                    "these fields come from the same checked record binding",
                ))
                .with_fix_hint(FixHint::replacement(
                    span,
                    "bind the selected record fields together",
                    format!("let {pattern} = {root}{suffix}"),
                )),
            );
        }
    }

    /// A fresh placeholder can disappear only when no cleanup or body work
    /// observes its earlier initialization. Keep mutation semantics for later use.
    pub(super) fn lint_context_scope_scaffolds(&mut self, stmts: &[StmtId]) {
        context_scope::lint_command_scope_scaffolds(self, stmts);
        for pair in stmts.windows(2) {
            let declaration = self.arena.stmt(pair[0]);
            let ArenaStmtKind::Var {
                target,
                initializer: ArenaExprOrRun::Expr(initial),
                ..
            } = declaration.kind
            else {
                continue;
            };
            let ArenaBindingTargetKind::Name(name) = self.arena.binding_target(target).kind else {
                continue;
            };
            if !matches!(
                self.arena.expr(initial).kind,
                ArenaExprKind::Int(_)
                    | ArenaExprKind::Str(_)
                    | ArenaExprKind::Bool(_)
                    | ArenaExprKind::PathStr(_)
            ) {
                continue;
            }
            let statement = self.arena.stmt(pair[1]);
            let (input, block, cwd) = match statement.kind {
                ArenaStmtKind::Expr(expr) => {
                    let ArenaExprKind::Try(scope) = self.arena.expr(expr).kind else {
                        continue;
                    };
                    let ArenaExprKind::ContextScope {
                        input,
                        block,
                        kind,
                        value_body: false,
                    } = self.arena.expr(scope).kind
                    else {
                        continue;
                    };
                    (
                        input,
                        block,
                        kind == xsh::frontend::syntax::arena::ContextScopeKind::Cwd,
                    )
                }
                ArenaStmtKind::Command(command) => {
                    let command = self.arena.command_stmt(command);
                    let ArenaCommand::Core {
                        name: CoreCommand::Cd,
                        args,
                        block: Some(block),
                        ..
                    } = command.command
                    else {
                        continue;
                    };
                    let [argument] = self.arena.command_args(args) else {
                        continue;
                    };
                    let ArenaCommandArgKind::Typed(input) = argument.kind else {
                        continue;
                    };
                    if !command.propagate {
                        continue;
                    }
                    (input, block, true)
                }
                _ => continue,
            };
            let body: Vec<_> = self
                .arena
                .stmt_ids(self.arena.block(block).statements)
                .collect();
            let [assignment] = body.as_slice() else {
                continue;
            };
            let assignment = self.arena.stmt(*assignment);
            let ArenaStmtKind::Assign {
                target,
                op: AssignOp::Set,
                value: ArenaExprOrRun::Expr(value),
            } = assignment.kind
            else {
                continue;
            };
            if !matches!(self.arena.assign_target(target).kind, ArenaAssignTargetKind::Name(found) if found == name)
                || expr_references_name(self.arena, input, name)
                || expr_references_name(self.arena, value, name)
            {
                continue;
            }
            if !self.expr_types.contains_key(&self.arena.expr(initial).span)
                || self.expr_types.get(&self.arena.expr(initial).span)
                    != self.expr_types.get(&self.arena.expr(value).span)
            {
                continue;
            }
            let edit = Span::new(
                declaration.span.source_id,
                declaration.span.start(),
                statement.span.end(),
            );
            let mut diagnostic =
                Diagnostic::warning("a fresh placeholder can consume the scope value")
                    .with_code(DiagnosticCode::LintPreferContextScopeValue)
                    .with_label(Label::secondary(
                        edit,
                        "initialize directly from the restored context",
                    ));
            // Cleanup captures, signal handlers, and comments can observe or explain the original
            // assignment timing; leave those scaffolds for an explicit edit.
            fn scalar_atom(arena: &AstArena, expr: ExprId) -> bool {
                matches!(
                    arena.expr(expr).kind,
                    ArenaExprKind::Ident(_)
                        | ArenaExprKind::Int(_)
                        | ArenaExprKind::Str(_)
                        | ArenaExprKind::Bool(_)
                        | ArenaExprKind::PathStr(_)
                )
            }
            let stable_input = scalar_atom(self.arena, input)
                || match self.arena.expr(input).kind {
                    ArenaExprKind::Record(fields) => {
                        self.arena
                            .record_fields(fields)
                            .iter()
                            .all(|field| match field.kind {
                                ArenaRecordFieldKind::Named { value, .. } => {
                                    scalar_atom(self.arena, value)
                                }
                                ArenaRecordFieldKind::Shorthand { .. } => true,
                                _ => false,
                            })
                    }
                    _ => false,
                };
            let mut selected_value = value;
            if let ArenaExprKind::Try(inner) = self.arena.expr(selected_value).kind {
                selected_value = inner;
            }
            let stable_value = scalar_atom(self.arena, selected_value)
                || match self.arena.expr(selected_value).kind {
                    ArenaExprKind::Call { callee, args } => match self.arena.expr(callee).kind {
                        ArenaExprKind::Field { base, name: method } => {
                            match self.arena.expr(base).kind {
                                ArenaExprKind::Ident(module) => {
                                    ["env", "fs"].contains(&module.as_str().as_str())
                                        && !self.scopes.iter().any(|scope| {
                                            scope.contains_key(module.as_str().as_str())
                                        })
                                        && xsh::api::api_spec()
                                            .module_overloads(&module.as_str(), &method.as_str())
                                            .is_some()
                                        && self.arena.call_args(args).iter().all(|arg| {
                                            match arg.kind {
                                                ArenaCallArgKind::Positional(value)
                                                | ArenaCallArgKind::Named { value, .. } => {
                                                    scalar_atom(self.arena, value)
                                                }
                                                _ => false,
                                            }
                                        })
                                }
                                _ => false,
                            }
                        }
                        _ => false,
                    },
                    _ => false,
                };
            if stable_input
                && stable_value
                && self.arena.signal_hooks.is_empty()
                && !self.source.contains("defer")
                && !self.source[edit.range()].contains('#')
            {
                let prefix =
                    &self.source[declaration.span.start()..self.arena.expr(initial).span.start()];
                let input = &self.source[self.arena.expr(input).span.range()];
                let value = &self.source[self.arena.expr(value).span.range()];
                let replacement = format!(
                    "{prefix}{} ({input}) {{ {value} }}?\n",
                    if cwd { "cd" } else { "env" }
                );
                let mut candidate = self.source.to_string();
                candidate.replace_range(edit.range(), &replacement);
                let parsed = xsh::frontend::syntax::parser::Parser::parse_source_arena_only(
                    edit.source_id,
                    &candidate,
                );
                if parsed.diagnostics.is_empty()
                    && xsh::frontend::check::Checker::check_arena(&parsed.arena, &candidate)
                        .diagnostics
                        .is_empty()
                {
                    diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                        edit,
                        "consume the scope tail directly",
                        replacement,
                    ));
                }
            }
            self.diagnostics.push(diagnostic);
        }
    }

    pub(super) fn lint_statement_sequence(&mut self, stmts: &[StmtId]) {
        self.lint_context_scope_scaffolds(stmts);
        self.lint_record_destructuring(stmts);
        self.lint_fresh_map_initializations(stmts);
        self.lint_list_element_reconstruction(stmts);
        if let Some(&first) = stmts.first() {
            self.lint_negative_if_as_boolean_guard(first);
        }
        lint_optional_binding::lint_null_test_then_binding(self, stmts);
        lint_write_mode::lint_write_then_chmod(self, stmts);
        lint_prefer_test_expect::lint_script_runs(self, stmts);
        lint_empty_sentinel::lint_empty_fallback_then_test(self, stmts);
        let mut flow = FlowSummary::fallthrough();
        let mut reported_dead_region = false;
        for (index, &stmt) in stmts.iter().enumerate() {
            if index > 0 {
                self.lint_linear_value_pipeline(stmts[index - 1], stmt, stmts);
            }
            if self.dead_code && !flow.fallthrough && !reported_dead_region {
                self.warning(
                    self.arena.stmt(stmt).span,
                    "unreachable code",
                    DiagnosticCode::LintDeadCode,
                    "this statement can never execute",
                );
                reported_dead_region = true;
            }
            self.lint_stmt(stmt, false);
            if flow.fallthrough {
                flow = flow.then(stmt_flow(self.arena, stmt));
            }
        }
    }

    pub(super) fn lint_list_element_reconstruction(&mut self, stmts: &[StmtId]) {
        fn integer(arena: &AstArena, expr: ExprId) -> Option<usize> {
            let ArenaExprKind::Int(value) = arena.expr(expr).kind else {
                return None;
            };
            usize::try_from(arena.int_literal(value).value()?).ok()
        }
        for (position, &stmt) in stmts.iter().enumerate() {
            let node = self.arena.stmt(stmt);
            let ArenaStmtKind::Assign {
                target,
                op: AssignOp::Set,
                value: ArenaExprOrRun::Expr(value),
            } = node.kind
            else {
                continue;
            };
            let ArenaAssignTargetKind::Name(name) = self.arena.assign_target(target).kind else {
                continue;
            };
            let ArenaExprKind::List(items) = self.arena.expr(value).kind else {
                continue;
            };
            let elements: Vec<_> = self.arena.list_elements(items).collect();
            let [prefix, replacement, suffix] = elements.as_slice() else {
                continue;
            };
            if prefix.splice_span.is_none()
                || replacement.splice_span.is_some()
                || suffix.splice_span.is_none()
            {
                continue;
            }
            let ArenaExprKind::Slice {
                base: first,
                start: None,
                end: Some(end),
                guarded: false,
            } = self.arena.expr(prefix.value).kind
            else {
                continue;
            };
            let ArenaExprKind::Slice {
                base: last,
                start: Some(start),
                end: None,
                guarded: false,
            } = self.arena.expr(suffix.value).kind
            else {
                continue;
            };
            if ![first, last].into_iter().all(|expr| matches!(self.arena.expr(expr).kind, ArenaExprKind::Ident(found) if found == name)) { continue; }
            let (Some(index), Some(after)) = (integer(self.arena, end), integer(self.arena, start))
            else {
                continue;
            };
            if index.checked_add(1) != Some(after) {
                continue;
            }
            let Some(list_ty @ Type::List(element)) =
                self.expr_types.get(&self.arena.expr(first).span)
            else {
                continue;
            };
            if !list_splice_element_type_is_precise(element)
                || self.expr_types.get(&self.arena.expr(value).span) != Some(list_ty)
                || self
                    .expr_types
                    .get(&self.arena.expr(replacement.value).span)
                    != Some(element.as_ref())
            {
                continue;
            }
            let mut diagnostic = Diagnostic::warning(
                "prefer an element assignment when the list index is known valid",
            )
            .with_code(DiagnosticCode::LintPreferListElementAssignment)
            .with_label(Label::secondary(node.span, "replace one existing element"));
            // Only an immediately preceding literal declaration proves a current
            // length without removing a read across intervening effects.
            let length = position.checked_sub(1).and_then(|previous| {
                let ArenaStmtKind::Var { target, initializer: ArenaExprOrRun::Expr(initializer), .. } = self.arena.stmt(stmts[previous]).kind else { return None; };
                if !matches!(self.arena.binding_target(target).kind, ArenaBindingTargetKind::Name(found) if found == name) { return None; }
                let ArenaExprKind::List(elements) = self.arena.expr(initializer).kind else { return None; };
                let elements: Vec<_> = self.arena.list_elements(elements).collect();
                elements.iter().all(|element| element.splice_span.is_none()).then_some(elements.len())
            });
            let edit_span = Span::new(
                node.span.source_id,
                node.span.start(),
                self.arena.expr(value).span.end(),
            );
            // An expression's own span can omit the input of a pipeline, so the
            // element text is read back from between its two commas.
            let element_source = suffix
                .splice_span
                .and_then(|splice| {
                    self.source.get(
                        self.arena.expr(prefix.value).span.end()..self.arena.span(splice).start(),
                    )
                })
                .and_then(|between| between.trim().strip_prefix(',')?.strip_suffix(','))
                .map(str::trim);
            // The list literal reads the list, evaluates the element, and
            // reads the list again; the element assignment evaluates the
            // element and then stores into the list. The two agree exactly
            // when the element leaves the list alone. A statement list in
            // the module scope declares a module-level variable, which any
            // proc may assign, so only a call-free element is known to.
            let module_level = self.scopes.len() == 1;
            let element_leaves_list_alone = element_source.is_some_and(|element_source| {
                if module_level {
                    list_update_argument_stable(self.arena, replacement.value)
                } else {
                    !expr_may_assign_local(
                        self.arena,
                        self.source,
                        replacement.value,
                        name.as_str().as_str(),
                        element_source,
                    )
                }
            });
            if let Some(rhs) = element_source
                && length.is_some_and(|length| index < length)
                && element_leaves_list_alone
                && !span_may_contain_comment(self.source, edit_span)
            {
                diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                    edit_span,
                    "update the existing list element",
                    format!("{name}[{index}] = {rhs}"),
                ));
            } else {
                diagnostic = diagnostic.with_note("slice bounds clip but element indices must exist; an unproved length, an element that may assign the list, and comments require manual review");
            }
            self.diagnostics.push(diagnostic);
        }
    }

    pub(super) fn lint_scalar_split_iteration(&mut self, iter: ExprId) {
        let ArenaExprKind::Call { callee, args } = self.arena.expr(iter).kind else {
            return;
        };
        let ArenaExprKind::Field { base, name } = self.arena.expr(callee).kind else {
            return;
        };
        let checked_str = self.expr_types.get(&self.arena.expr(base).span) == Some(&Type::Str)
            || (matches!(self.arena.expr(base).kind, ArenaExprKind::Str(_))
                && matches!(self.expr_types.get(&self.arena.expr(iter).span), Some(Type::List(item)) if **item == Type::Str));
        if name != "split" || !checked_str {
            return;
        }
        let arguments = self.arena.call_args(args);
        if arguments.len() != 1 {
            return;
        }
        let separator = match arguments[0].kind {
            ArenaCallArgKind::Positional(value) => value,
            ArenaCallArgKind::Named { name, value, .. } if name == "separator" => value,
            _ => return,
        };
        let ArenaExprKind::Str(text) = self.arena.expr(separator).kind else {
            return;
        };
        if !self.arena.string_literal(text).is_empty() {
            return;
        }
        let span = self.arena.expr(iter).span;
        if self
            .source
            .get(span.range())
            .is_none_or(|text| text.contains('#'))
        {
            return;
        }
        let callee_span = self.arena.expr(callee).span;
        let Some(dot) = self
            .source
            .get(callee_span.range())
            .and_then(|source| source.rfind('.'))
        else {
            return;
        };
        let suffix = Span::new(span.source_id, callee_span.start() + dot, span.end());
        self.diagnostics.push(
            Diagnostic::warning("iterate over Unicode scalars without a split List")
                .with_code(DiagnosticCode::LintPreferScalarIteration)
                .with_label(Label::secondary(
                    span,
                    "the checked Str source is retained once",
                ))
                .with_fix_hint(FixHint::deletion(suffix, "iterate over the Str directly")),
        );
    }

    pub(super) fn lint_byte_iteration(
        &mut self,
        stmt_id: StmtId,
        target: BindingTargetId,
        iter: ExprId,
        block: BlockId,
    ) {
        let ArenaBindingTargetKind::Name(index) = self.arena.binding_target(target).kind else {
            return;
        };
        let ArenaExprKind::Call { callee, args } = self.arena.expr(iter).kind else {
            return;
        };
        if !matches!(self.arena.expr(callee).kind, ArenaExprKind::Ident(name) if name == "range")
            || self.is_binding_in_scope_or_assigned("range")
        {
            return;
        }
        let arguments = self.arena.call_args(args);
        let [argument] = arguments else {
            return;
        };
        let ArenaCallArgKind::Positional(length) = argument.kind else {
            return;
        };
        let ArenaExprKind::Call { callee, args } = self.arena.expr(length).kind else {
            return;
        };
        if !self.arena.call_args(args).is_empty() {
            return;
        }
        let ArenaExprKind::Field { base, name } = self.arena.expr(callee).kind else {
            return;
        };
        if name != "len" {
            return;
        }
        let ArenaExprKind::Ident(source) = self.arena.expr(base).kind else {
            return;
        };
        let Some(binding) = self
            .scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(source.as_str().as_str()))
        else {
            return;
        };
        if self.assigned_names.contains(&source)
            || source == index
            || binding.mutable
            || !binding.comparison_stable
        {
            return;
        }
        // Constant preparation may replace the length expression before recording its receiver.
        // The resolved immutable initializer supplies the same checked Bytes proof in that case.
        let source_type = self
            .expr_types
            .get(&self.arena.expr(base).span)
            .or_else(|| {
                (0..self.arena.stmt_tags.len()).find_map(|index| {
                    let statement = self.arena.stmt(StmtId::from_index(index));
                    if statement.span != binding.span {
                        return None;
                    }
                    match statement.kind {
                        ArenaStmtKind::Let {
                            initializer: ArenaExprOrRun::Expr(value),
                            ..
                        }
                        | ArenaStmtKind::Const {
                            initializer: ArenaExprOrRun::Expr(value),
                            ..
                        } => self.expr_types.get(&self.arena.expr(value).span),
                        _ => None,
                    }
                })
            });
        if source_type != Some(&Type::Bytes) {
            return;
        }
        let statements: Vec<_> = self
            .arena
            .stmt_ids(self.arena.block(block).statements)
            .collect();
        let Some((&first, remaining)) = statements.split_first() else {
            return;
        };
        let first = self.arena.stmt(first);
        let ArenaStmtKind::Let {
            target,
            ty: None,
            initializer: ArenaExprOrRun::Expr(mut access),
        } = first.kind
        else {
            return;
        };
        let ArenaBindingTargetKind::Name(octet) = self.arena.binding_target(target).kind else {
            return;
        };
        if octet == source || octet == index || octet == "_" {
            return;
        }
        if let ArenaExprKind::Binary {
            op: BinaryOp::ResultFallback,
            left,
            right,
        } = self.arena.expr(access).kind
        {
            if !matches!(
                self.arena.expr(right).kind,
                ArenaExprKind::Int(_)
                    | ArenaExprKind::Unary {
                        op: UnaryOp::Neg,
                        ..
                    }
            ) {
                return;
            }
            if let ArenaExprKind::Unary { expr: value, .. } = self.arena.expr(right).kind
                && !matches!(self.arena.expr(value).kind, ArenaExprKind::Int(_))
            {
                return;
            }
            access = left;
        }
        let ArenaExprKind::Call { callee, args } = self.arena.expr(access).kind else {
            return;
        };
        let ArenaExprKind::Field { base, name } = self.arena.expr(callee).kind else {
            return;
        };
        if name != "byte_at"
            || !matches!(self.arena.expr(base).kind, ArenaExprKind::Ident(name) if name == source)
        {
            return;
        }
        let [argument] = self.arena.call_args(args) else {
            return;
        };
        if !matches!(argument.kind, ArenaCallArgKind::Positional(value) if matches!(self.arena.expr(value).kind, ArenaExprKind::Ident(name) if name == index))
        {
            return;
        }
        // Textual occurrence checks also cover command interpolation and shadowed names.
        // Rejecting harmless strings or comments is preferable to losing a byte offset.
        let mentions_index = |text: &str| {
            text.match_indices(index.as_str().as_str())
                .any(|(offset, matched)| {
                    let identifier_char = |ch: char| ch.is_alphanumeric() || ch == '_';
                    !text[..offset]
                        .chars()
                        .next_back()
                        .is_some_and(identifier_char)
                        && !text[offset + matched.len()..]
                            .chars()
                            .next()
                            .is_some_and(identifier_char)
                })
        };
        if remaining.iter().any(|id| {
            self.source
                .get(self.arena.stmt(*id).span.range())
                .is_none_or(mentions_index)
        }) {
            return;
        }
        let header = Span::new(
            first.span.source_id,
            self.arena.stmt(stmt_id).span.start(),
            self.arena.expr(iter).span.end(),
        );
        let line_start = self.source[..first.span.start()]
            .rfind('\n')
            .map_or(0, |index| index + 1);
        if !self.source[line_start..first.span.start()]
            .trim()
            .is_empty()
        {
            return;
        }
        let deletion = Span::new(
            first.span.source_id,
            line_start,
            span_end_after_following_newlines(self.source, first.span.end()),
        );
        if [header, deletion].iter().any(|span| {
            self.source
                .get(span.range())
                .is_none_or(|source| source.contains('#'))
        }) {
            return;
        }
        let line_end = self.source[first.span.start()..]
            .find('\n')
            .map_or(self.source.len(), |offset| first.span.start() + offset);
        if self.source[first.span.start()..line_end].contains('#') {
            return;
        }
        let edit = Span::new(header.source_id, header.start(), deletion.end());
        let replacement = format!(
            "for {octet} in {source}{}",
            &self.source[header.end()..line_start]
        );
        let mut candidate = self.source.to_owned();
        candidate.replace_range(edit.range(), &replacement);
        let parsed = xsh::frontend::syntax::parser::Parser::parse_source_arena_only(
            edit.source_id,
            &candidate,
        );
        if !parsed.diagnostics.is_empty()
            || !xsh::frontend::check::Checker::check_arena(&parsed.arena, &candidate)
                .diagnostics
                .is_empty()
        {
            return;
        }
        self.diagnostics.push(
            Diagnostic::warning("iterate over bytes instead of constructing unused offsets")
                .with_code(DiagnosticCode::LintPreferScalarIteration)
                .with_label(Label::secondary(
                    header,
                    "the immutable Bytes source covers the exact full range",
                ))
                .with_fix_hint(FixHint::replacement(
                    edit,
                    "bind each byte directly",
                    replacement,
                )),
        );
    }

    pub(super) fn lint_map_entry_iteration(
        &mut self,
        stmt_id: StmtId,
        target: BindingTargetId,
        iter: ExprId,
        block: BlockId,
    ) {
        let ArenaBindingTargetKind::Name(key) = self.arena.binding_target(target).kind else {
            return;
        };
        let ArenaExprKind::Call { callee, args } = self.arena.expr(iter).kind else {
            return;
        };
        if !self.arena.call_args(args).is_empty() {
            return;
        }
        let ArenaExprKind::Field { base, name } = self.arena.expr(callee).kind else {
            return;
        };
        if name.as_str() != "keys" {
            return;
        }
        let ArenaExprKind::Ident(map) = self.arena.expr(base).kind else {
            return;
        };
        // Entry iteration retains a value snapshot. A mutable map may change
        // inside a value block before a later key lookup reads its current value.
        if !self
            .scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(map.as_str().as_str()))
            .is_some_and(|binding| !binding.mutable)
        {
            return;
        }
        if self.assigned_names.contains(&map)
            || key == map
            || !matches!(
                self.expr_types.get(&self.arena.expr(base).span),
                Some(Type::Map(_, _))
            )
        {
            return;
        }
        let statements: Vec<_> = self
            .arena
            .stmt_ids(self.arena.block(block).statements)
            .collect();
        if statements.len() < 2 {
            return;
        }
        let first = self.arena.stmt(statements[0]);
        let ArenaStmtKind::Let {
            target: value_target,
            ty: None,
            initializer: ArenaExprOrRun::Expr(lookup),
        } = first.kind
        else {
            return;
        };
        let ArenaBindingTargetKind::Name(value) = self.arena.binding_target(value_target).kind
        else {
            return;
        };
        if value == map || value == key || value.as_str() == "_" {
            return;
        }
        let ArenaExprKind::Try(lookup) = self.arena.expr(lookup).kind else {
            return;
        };
        let ArenaExprKind::Call { callee, args } = self.arena.expr(lookup).kind else {
            return;
        };
        let ArenaExprKind::Field { base, name } = self.arena.expr(callee).kind else {
            return;
        };
        if name.as_str() != "get"
            || !matches!(self.arena.expr(base).kind, ArenaExprKind::Ident(name) if name == map)
        {
            return;
        }
        let arguments = self.arena.call_args(args);
        if arguments.len() != 1
            || !matches!(arguments[0].kind, ArenaCallArgKind::Positional(expr) if matches!(self.arena.expr(expr).kind, ArenaExprKind::Ident(name) if name == key))
        {
            return;
        }
        let header = Span::new(
            first.span.source_id,
            self.arena.stmt(stmt_id).span.start(),
            self.arena.expr(iter).span.end(),
        );
        let line_start = self.source[..first.span.start()]
            .rfind('\n')
            .map_or(0, |index| index + 1);
        if !self.source[line_start..first.span.start()]
            .trim()
            .is_empty()
        {
            return;
        }
        let deletion = Span::new(
            first.span.source_id,
            line_start,
            span_end_after_following_newlines(self.source, first.span.end()),
        );
        if [header, deletion].iter().any(|span| {
            self.source
                .get(span.range())
                .is_none_or(|source| source.contains('#'))
        }) {
            return;
        }
        let line_end = self.source[first.span.start()..]
            .find('\n')
            .map_or(self.source.len(), |offset| first.span.start() + offset);
        if self.source[first.span.start()..line_end].contains('#') {
            return;
        }
        let key_field = if key.as_str() == "key" {
            "key".to_string()
        } else {
            format!("key: {key}")
        };
        let value_field = if value.as_str() == "value" {
            "value".to_string()
        } else {
            format!("value: {value}")
        };
        let edit = Span::new(header.source_id, header.start(), deletion.end());
        let replacement = format!(
            "for {{{key_field}, {value_field}}} in {map}{}",
            &self.source[header.end()..line_start]
        );
        self.diagnostics.push(
            Diagnostic::warning("iterate over map entries instead of keys followed by a lookup")
                .with_code(DiagnosticCode::LintPreferMapEntryIteration)
                .with_label(Label::secondary(
                    header,
                    "this checked map stays stable across the loop",
                ))
                .with_fix_hint(FixHint::replacement(
                    edit,
                    "bind the map entry and remove its redundant lookup",
                    replacement,
                )),
        );
    }

    pub(super) fn lint_list_comp_suggestions(&mut self, stmts: &[StmtId]) {
        for pair in stmts.windows(2) {
            self.lint_suggest_list_comp(pair[0], pair[1]);
            self.lint_suggest_map_comp(pair[0], pair[1]);
        }
    }

    pub(super) fn lint_stream_producer_suggestions(&mut self, stmts: &[StmtId]) {
        let mut candidates = Vec::new();
        collect_stream_producer_candidates(self.arena, stmts, &mut candidates);
        if candidates.is_empty() {
            return;
        }
        let mut consumed = FxHashSet::default();
        collect_lazy_consumed_calls(self.arena, stmts, &mut consumed);
        for candidate in candidates {
            if !consumed.contains(&candidate.function_name) {
                continue;
            }
            self.diagnostics.push(
                Diagnostic::new(
                    Severity::Warning,
                    format!(
                        "proc `{}` builds list `{}` item-by-item and is consumed lazily; consider a `stream` producer",
                        candidate.function_name, candidate.accumulator_name,
                    ),
                )
                .with_code(DiagnosticCode::LintPreferStreamProducer)
                .with_label(Label::secondary(
                    candidate.span,
                    "`yield` can avoid materializing this list for direct stream consumers",
                )),
            );
        }
    }

    pub(super) fn accumulator_qualifiers(
        &self,
        mut stmt: StmtId,
        accumulator: Name,
    ) -> Option<(Vec<String>, StmtId)> {
        let mut qualifiers = Vec::new();
        loop {
            let (block, loop_body) = match self.arena.stmt(stmt).kind {
                ArenaStmtKind::For {
                    target,
                    iter,
                    block,
                } => {
                    if binding_target_contains_name(self.arena, target, accumulator)
                        || expr_references_name(self.arena, iter, accumulator)
                    {
                        return None;
                    }
                    // Result iterables in a statement loop require explicit propagation;
                    // that spelling survives in the comprehension's iterable expression.
                    let iter_src = self.source.get(self.arena.expr(iter).span.range())?;
                    qualifiers.push(format!(
                        "for {} in {iter_src}",
                        format_binding_target(self.arena, target)
                    ));
                    (block, true)
                }
                ArenaStmtKind::If {
                    branches,
                    else_block: None,
                } => {
                    let [branch] = self.arena.if_branches(branches) else {
                        return None;
                    };
                    if matches!(
                        self.arena.expr(branch.condition).kind,
                        ArenaExprKind::PatternCondition { .. }
                    ) {
                        return None;
                    }
                    if expr_references_name(self.arena, branch.condition, accumulator) {
                        return None;
                    }
                    let condition = self
                        .source
                        .get(self.arena.expr(branch.condition).span.range())?;
                    qualifiers.push(format!("if {condition}"));
                    (branch.block, false)
                }
                _ => break,
            };
            let block = self.arena.block(block);
            if !block.params.is_empty() {
                return None;
            }
            let mut statements = self.arena.stmt_ids(block.statements).peekable();
            // A loop body may skip items before it accumulates. Each leading
            // `continue unless c` keeps exactly the items `if c` keeps, and
            // the guards run in the order the qualifiers do.
            while loop_body
                && let Some(kept) = statements
                    .peek()
                    .and_then(|&guard| self.kept_item_condition(guard, accumulator))
            {
                qualifiers.push(format!("if {kept}"));
                statements.next();
            }
            stmt = statements.next()?;
            if statements.next().is_some() {
                return None;
            }
        }
        if qualifiers
            .first()
            .is_none_or(|clause| !clause.starts_with("for "))
        {
            return None;
        }
        Some((qualifiers, stmt))
    }

    /// The condition under which a loop body goes on past `guard`, when
    /// `guard` is `continue unless CONDITION` or `continue when CONDITION`:
    /// the condition as written, or its negation.
    pub(super) fn kept_item_condition(&self, guard: StmtId, accumulator: Name) -> Option<String> {
        let guard = self.arena.stmt(guard);
        let ArenaStmtKind::Sugar {
            form: form @ (SugarForm::When | SugarForm::Unless),
            operands,
            ..
        } = guard.kind
        else {
            return None;
        };
        let ArenaSugar::Guarded {
            stmt,
            negate,
            condition,
        } = self.arena.sugar(form, operands)
        else {
            return None;
        };
        if !matches!(self.arena.stmt(stmt).kind, ArenaStmtKind::Continue)
            || expr_references_name(self.arena, condition, accumulator)
        {
            return None;
        }
        // The statement's text keeps grouping that the condition's span can omit.
        let text = self.source.get(guard.span.range())?.trim_end();
        let written = text
            .strip_prefix("continue")?
            .trim_start()
            .strip_prefix(if negate { "unless" } else { "when" })?;
        if !written.starts_with(char::is_whitespace) {
            return None;
        }
        let written = written.trim_start();
        if negate {
            return Some(written.to_string());
        }
        let end = guard.span.start() + text.len();
        self.negated_condition(
            condition,
            Span::new(guard.span.source_id, end - written.len(), end),
        )
    }

    /// `condition` negated in the spelling the formatter prints, for the
    /// shapes whose negation needs no new grouping; `written` is its text.
    pub(super) fn negated_condition(&self, condition: ExprId, written: Span) -> Option<String> {
        let text = self.source.get(written.range())?;
        match self.arena.expr(condition).kind {
            ArenaExprKind::Unary {
                op: UnaryOp::Not, ..
            } => {
                let operand = text.strip_prefix('!')?.trim_start();
                (!operand.starts_with('(')).then(|| operand.to_string())
            }
            ArenaExprKind::Ident(_)
            | ArenaExprKind::Field { .. }
            | ArenaExprKind::Call { .. }
            | ArenaExprKind::Index { .. }
                if !text.starts_with('(') =>
            {
                Some(format!("! {text}"))
            }
            ArenaExprKind::Binary {
                op: op @ (BinaryOp::Eq | BinaryOp::Ne),
                left,
                right,
            } => {
                let (original, negated) = if op == BinaryOp::Eq {
                    ("==", "!=")
                } else {
                    ("!=", "==")
                };
                let between = self.arena.expr(left).span.end()..self.arena.expr(right).span.start();
                let operator = between.start + self.source.get(between.clone())?.find(original)?;
                Some(format!(
                    "{}{negated}{}",
                    self.source.get(written.start()..operator)?,
                    self.source.get(operator + original.len()..written.end())?
                ))
            }
            _ => None,
        }
    }

    /// The element one accumulating statement appends to the list `name`:
    /// `name = name.push(ELEMENT)` or `name += [ELEMENT]`.
    pub(super) fn appended_list_element(&self, stmt: StmtId, name: Name) -> Option<ExprId> {
        let ArenaStmtKind::Assign {
            target,
            op,
            value: ArenaExprOrRun::Expr(value),
        } = self.arena.stmt(stmt).kind
        else {
            return None;
        };
        if !matches!(
            self.arena.assign_target(target).kind,
            ArenaAssignTargetKind::Name(found) if found == name
        ) {
            return None;
        }
        match (op, self.arena.expr(value).kind) {
            (AssignOp::Set, ArenaExprKind::Call { callee, args }) => {
                let ArenaExprKind::Field { base, name: method } = self.arena.expr(callee).kind
                else {
                    return None;
                };
                if method != "push"
                    || !matches!(
                        self.arena.expr(base).kind,
                        ArenaExprKind::Ident(found) if found == name
                    )
                {
                    return None;
                }
                match self.arena.call_args(args) {
                    [arg] => match arg.kind {
                        ArenaCallArgKind::Positional(element) => Some(element),
                        _ => None,
                    },
                    _ => None,
                }
            }
            (AssignOp::Add, ArenaExprKind::List(items)) => {
                let mut items = self.arena.list_elements(items);
                match (items.next(), items.next()) {
                    (Some(item), None) if item.splice_span.is_none() => Some(item.value),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    pub(super) fn accumulator_annotation(&self, ty: Option<TypeExprId>) -> String {
        ty.and_then(|ty| self.source.get(self.arena.type_expr_span(ty).range()))
            .map(|text| format!(": {text}"))
            .unwrap_or_default()
    }

    pub(super) fn accumulator_replacement(
        &self,
        span: Span,
        name: Name,
        annotation: &str,
        open: &str,
        projection: &str,
        close: &str,
        qualifiers: &[String],
    ) -> String {
        if qualifiers
            .iter()
            .filter(|clause| clause.starts_with("for "))
            .count()
            == 1
            && qualifiers.len() <= 2
        {
            let one_line = format!(
                "var {name}{annotation} = {open}{projection} {}{close}\n",
                qualifiers.join(" ")
            );
            // The formatter breaks a comprehension that overflows the line.
            let line_start = self.source[..span.start()]
                .rfind('\n')
                .map_or(0, |offset| offset + 1);
            let width = self.source[line_start..span.start()].chars().count()
                + one_line.trim_end().chars().count();
            if width <= super::super::format::DEFAULT_LINE_WIDTH && !one_line.trim_end().contains('\n') {
                return one_line;
            }
        }
        let line_start = self.source[..span.start()]
            .rfind('\n')
            .map_or(0, |offset| offset + 1);
        let indent = &self.source[line_start..span.start()];
        let mut text = format!("var {name}{annotation} = {open}\n{indent}  {projection}\n");
        for qualifier in qualifiers {
            text.push_str(&format!("{indent}  {qualifier}\n"));
        }
        text.push_str(&format!("{indent}{close}\n"));
        text
    }

    /// A forwarding loop can be replaced only when no body work or binder
    /// conversion is lost and the checked source already has iterable type.
    pub(super) fn lint_yield_delegation(
        &mut self,
        span: Span,
        target: BindingTargetId,
        iter: ExprId,
        block: BlockId,
    ) {
        let Some(Type::Stream(expected)) = self.function_return_types.last() else {
            return;
        };
        let ArenaBindingTargetKind::Name(binding) = self.arena.binding_target(target).kind else {
            return;
        };
        let statements: Vec<_> = self
            .arena
            .stmt_ids(self.arena.block(block).statements)
            .collect();
        if statements.len() != 1 {
            return;
        }
        let ArenaStmtKind::Yield(ArenaExprOrRun::Expr(value)) = self.arena.stmt(statements[0]).kind
        else {
            return;
        };
        if !matches!(self.arena.expr(value).kind, ArenaExprKind::Ident(name) if name == binding) {
            return;
        }
        // Direct loop pipelines can fuse into a lazy cursor while expression
        // pipelines collect first. Keep that consumer boundary explicit.
        if matches!(
            self.arena.expr(iter).kind,
            ArenaExprKind::Pipeline { .. } | ArenaExprKind::StructuredPipeline { .. }
        ) {
            return;
        }
        let Some(Type::List(item) | Type::Stream(item)) =
            self.expr_types.get(&self.arena.expr(iter).span)
        else {
            return;
        };
        if item != expected || matches!(item.as_ref(), Type::Any | Type::Unknown) {
            return;
        }
        if self
            .source
            .get(span.range())
            .is_none_or(|text| text.contains('#'))
        {
            return;
        }
        let Some(source) = self.source.get(self.arena.expr(iter).span.range()) else {
            return;
        };
        let source = if source.contains('\n') {
            format!("({source})")
        } else {
            source.to_string()
        };
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "delegate a transparent forwarding loop with `yield @`",
            )
            .with_code(DiagnosticCode::LintPreferYieldDelegation)
            .with_label(Label::secondary(
                span,
                "this loop only yields its current item",
            ))
            .with_fix_hint(FixHint::replacement(
                span,
                "delegate the iterable",
                format!("yield @{source}"),
            )),
        );
    }

    pub(super) fn lint_suggest_list_comp(&mut self, var_id: StmtId, for_id: StmtId) {
        let var_stmt = self.arena.stmt(var_id);
        let for_stmt = self.arena.stmt(for_id);
        // Match: var <name> = []
        let ArenaStmtKind::Var {
            target,
            ty,
            initializer: ArenaExprOrRun::Expr(init),
        } = var_stmt.kind
        else {
            return;
        };
        let ArenaBindingTargetKind::Name(var_name) = self.arena.binding_target(target).kind else {
            return;
        };
        let ArenaExprKind::List(items) = self.arena.expr(init).kind else {
            return;
        };
        if !items.is_empty() {
            return;
        }
        let Some((qualifiers, push_stmt_id)) = self.accumulator_qualifiers(for_id, var_name) else {
            return;
        };
        let Some(push_expr) = self.appended_list_element(push_stmt_id, var_name) else {
            return;
        };
        if expr_references_name(self.arena, push_expr, var_name) {
            return;
        }
        if let Some(ty) = ty
            && !matches!(Type::from_arena(self.arena, ty), Type::List(_))
        {
            return;
        }
        let push_span = self.arena.expr(push_expr).span;
        let Some(push_src) = self.source.get(push_span.range()) else {
            return;
        };
        let annotation = self.accumulator_annotation(ty);
        let replacement = self.accumulator_replacement(
            var_stmt.span,
            var_name,
            &annotation,
            "[",
            push_src,
            "]",
            &qualifiers,
        );
        let combined = Span::new(
            var_stmt.span.source_id,
            var_stmt.span.start(),
            span_end_after_following_newlines(self.source, for_stmt.span.end()),
        );
        if self
            .source
            .get(combined.range())
            .is_none_or(|text| text.contains('#'))
        {
            return;
        }
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                format!(
                    "use a list comprehension instead of building `{var_name}` with a for loop"
                ),
            )
            .with_code(DiagnosticCode::LintPreferListComp)
            .with_label(Label::secondary(
                for_stmt.span,
                "this for loop only builds a list",
            ))
            .with_fix_hint(FixHint::replacement(
                combined,
                "convert to list comprehension",
                replacement,
            )),
        );
    }

    pub(super) fn lint_suggest_map_comp(&mut self, var_id: StmtId, for_id: StmtId) {
        let var_stmt = self.arena.stmt(var_id);
        let for_stmt = self.arena.stmt(for_id);
        let ArenaStmtKind::Var {
            target,
            initializer: ArenaExprOrRun::Expr(init),
            ty,
        } = var_stmt.kind
        else {
            return;
        };
        let ArenaBindingTargetKind::Name(var_name) = self.arena.binding_target(target).kind else {
            return;
        };
        let empty_literal = matches!(self.arena.expr(init).kind, ArenaExprKind::Record(fields) if fields.is_empty());
        let map_context = ty
            .is_some_and(|ty| matches!(Type::from_arena(self.arena, ty), Type::Map(_, _)))
            || matches!(
                self.expr_types.get(&self.arena.expr(init).span),
                Some(Type::Map(_, _))
            );
        if !(is_map_empty_call(self.arena, init) || empty_literal && map_context) {
            return;
        }
        let Some((qualifiers, assign_stmt_id)) = self.accumulator_qualifiers(for_id, var_name)
        else {
            return;
        };
        let ArenaStmtKind::Assign {
            target: assign_target,
            op: AssignOp::Set,
            value: ArenaExprOrRun::Expr(value),
        } = self.arena.stmt(assign_stmt_id).kind
        else {
            return;
        };
        let ArenaAssignTargetKind::Index { base, index } =
            self.arena.assign_target(assign_target).kind
        else {
            return;
        };
        let ArenaAssignTargetKind::Name(assign_name) = self.arena.assign_target(base).kind else {
            return;
        };
        if assign_name != var_name {
            return;
        }
        if expr_references_name(self.arena, index, var_name)
            || expr_references_name(self.arena, value, var_name)
        {
            return;
        }
        let index_span = self.arena.expr(index).span;
        let value_span = self.arena.expr(value).span;
        let Some(key_src) = self.source.get(index_span.start()..index_span.end()) else {
            return;
        };
        let Some(value_src) = self.source.get(value_span.start()..value_span.end()) else {
            return;
        };
        let annotation = self.accumulator_annotation(ty);
        let projection = if map_comp_key_can_be_bare(self.arena, index) {
            format!("{key_src}: {value_src}")
        } else {
            format!("[{key_src}]: {value_src}")
        };
        let replacement = self.accumulator_replacement(
            var_stmt.span,
            var_name,
            &annotation,
            "{",
            &projection,
            "}",
            &qualifiers,
        );
        let combined = Span::new(
            var_stmt.span.source_id,
            var_stmt.span.start(),
            span_end_after_following_newlines(self.source, for_stmt.span.end()),
        );
        if self
            .source
            .get(combined.range())
            .is_none_or(|text| text.contains('#'))
        {
            return;
        }
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                format!("use a map comprehension instead of building `{var_name}` with a for loop"),
            )
            .with_code(DiagnosticCode::LintPreferMapComp)
            .with_label(Label::secondary(
                for_stmt.span,
                "this for loop only builds a map",
            ))
            .with_fix_hint(FixHint::replacement(
                combined,
                "convert to map comprehension",
                replacement,
            )),
        );
    }

    pub(super) fn lint_linear_value_pipeline(
        &mut self,
        previous: StmtId,
        current: StmtId,
        statements: &[StmtId],
    ) {
        let first = self.arena.stmt(previous);
        let second = self.arena.stmt(current);
        let ArenaStmtKind::Let {
            target,
            ty: None,
            initializer: ArenaExprOrRun::Expr(input),
        } = first.kind
        else {
            return;
        };
        let ArenaBindingTargetKind::Name(name) = self.arena.binding_target(target).kind else {
            return;
        };
        if (self.pipeline_ordinary_call(input).is_none()
            && !matches!(
                self.arena.expr(input).kind,
                ArenaExprKind::ValuePipelineCall { .. }
            ))
            || self.assigned_names.contains(&name)
        {
            return;
        }
        let value = match second.kind {
            ArenaStmtKind::Let {
                initializer: ArenaExprOrRun::Expr(value),
                ..
            }
            | ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(value)))
            | ArenaStmtKind::Expr(value) => value,
            _ => return,
        };
        let Some((_, args)) = self.pipeline_ordinary_call(value) else {
            return;
        };
        let holes = self.arena.call_args(args).iter().filter_map(|arg| {
            let expr = pipeline_argument_expr(arg).unwrap();
            matches!(self.arena.expr(expr).kind, ArenaExprKind::Ident(candidate) if candidate == name).then_some(expr)
        }).collect::<Vec<_>>();
        let [hole] = holes.as_slice() else {
            return;
        };
        let Some(last) = statements.last() else {
            return;
        };
        let end = self.arena.stmt(*last).span.end();
        let references = (0..self.arena.expr_tags.len())
            .filter(|raw| {
                let expr = self.arena.expr(ExprId::from_index(*raw));
                expr.span.source_id == first.span.source_id
                    && expr.span.start() >= first.span.end()
                    && expr.span.end() <= end
                    && matches!(expr.kind, ArenaExprKind::Ident(candidate) if candidate == name)
            })
            .count();
        let shorthand = self.arena.record_fields.iter().any(|field| matches!(field.kind, ArenaRecordFieldKind::Shorthand { name: candidate, span }
            if candidate == name && self.arena.span(span).source_id == first.span.source_id && self.arena.span(span).start() >= first.span.end() && self.arena.span(span).end() <= end));
        if references != 1 || shorthand {
            return;
        }
        let span = Span::new(first.span.source_id, first.span.start(), second.span.end());
        let value_span = self.arena.expr(value).span;
        let input_span = self.arena.expr(input).span;
        let hole_span = self.arena.expr(*hole).span;
        let Some(input_text) = self.source.get(input_span.range()) else {
            return;
        };
        let Some(value_text) = self.source.get(value_span.range()) else {
            return;
        };
        let mut stage = value_text.to_string();
        stage.replace_range(
            hole_span.start() - value_span.start()..hole_span.end() - value_span.start(),
            "_",
        );
        let pipeline = format!("{input_text} |> {stage}");
        let Some(second_text) = self.source.get(second.span.range()) else {
            return;
        };
        let mut replacement = second_text.to_string();
        replacement.replace_range(
            value_span.start() - second.span.start()..value_span.end() - second.span.start(),
            &pipeline,
        );
        let new_start = first.span.start() + value_span.start() - second.span.start();
        if !self.pipeline_rewrite_preserves_types(
            span,
            &replacement,
            value,
            input,
            new_start,
            new_start,
        ) {
            return;
        }
        let mut diagnostic = Diagnostic::new(
            Severity::Warning,
            "single-use temporary forms a value pipeline",
        )
        .with_code(DiagnosticCode::LintPreferValuePipeline)
        .with_label(Label::secondary(
            span,
            "retain the input directly in the following ordinary call",
        ));
        if self
            .source
            .get(span.range())
            .is_some_and(|source| !source.contains('#'))
        {
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                span,
                "collapse the single-use temporary",
                replacement,
            ));
        }
        self.diagnostics.push(diagnostic);
    }

    pub(super) fn lint_fresh_map_initializations(&mut self, statements: &[StmtId]) {
        for (index, &statement) in statements.iter().enumerate() {
            let initializer_stmt = self.arena.stmt(statement);
            let ArenaStmtKind::Var {
                target,
                initializer: ArenaExprOrRun::Expr(initializer),
                ..
            } = initializer_stmt.kind
            else {
                continue;
            };
            let ArenaBindingTargetKind::Name(name) = self.arena.binding_target(target).kind else {
                continue;
            };
            let Some(Type::Map(_, element)) = self
                .expr_types
                .get(&self.arena.expr(initializer).span)
                .cloned()
            else {
                continue;
            };
            if !list_splice_element_type_is_precise(&element)
                || !self.is_empty_map_literal_source(initializer)
            {
                continue;
            }
            let mut entries = Vec::new();
            let mut end = initializer_stmt.span.end();
            for &next in &statements[index + 1..] {
                let stmt = self.arena.stmt(next);
                let ArenaStmtKind::Assign {
                    target,
                    op: AssignOp::Set,
                    value: ArenaExprOrRun::Expr(value),
                } = stmt.kind
                else {
                    break;
                };
                if !matches!(self.arena.assign_target(target).kind, ArenaAssignTargetKind::Name(target) if target == name)
                {
                    break;
                }
                let Some((base, key, value)) = self.map_set_parts(value, &element) else {
                    break;
                };
                if !matches!(self.arena.expr(base).kind, ArenaExprKind::Ident(base) if base == name)
                    || expr_references_name(self.arena, key, name)
                    || expr_references_name(self.arena, value, name)
                    || expr_may_have_effects(self.arena, key)
                    || expr_may_have_effects(self.arena, value)
                {
                    break;
                }
                entries.push((key, value));
                end = stmt.span.end();
            }
            if entries.is_empty() {
                continue;
            }
            let Some(literal) = self.map_literal_replacement(&entries) else {
                continue;
            };
            let span = Span::new(
                initializer_stmt.span.source_id,
                initializer_stmt.span.start(),
                end,
            );
            let prefix = &self.source
                [initializer_stmt.span.start()..self.arena.expr(initializer).span.start()];
            let original = &self.source[span.range()];
            let trailing_layout = &original[original.trim_end().len()..];
            self.map_literal_diagnostic(span, format!("{prefix}{literal}{trailing_layout}"));
        }
    }

    pub(super) fn lint_negative_if_as_boolean_guard(&mut self, statement: StmtId) {
        let stmt = self.arena.stmt(statement);
        let ArenaStmtKind::If {
            branches,
            else_block: None,
        } = stmt.kind
        else {
            return;
        };
        let [branch] = self.arena.if_branches(branches) else {
            return;
        };
        let block_span = self.arena.span(self.arena.block(branch.block).span);
        if !self.definitely_exiting_block_spans.contains(&block_span) {
            return;
        }
        let condition = self.arena.expr(branch.condition);
        if self.source[stmt.span.start()..block_span.start()].contains('#') {
            return;
        }
        let source_expr = |expr: ExprId| {
            let span = self.arena.expr(expr).span;
            self.source[span.start()..span.end()].trim()
        };
        let inverse = match condition.kind {
            ArenaExprKind::Unary {
                op: UnaryOp::Not,
                expr,
            } => source_expr(expr).to_string(),
            ArenaExprKind::Binary { op, left, right }
                if matches!(op, BinaryOp::Lt | BinaryOp::Le | BinaryOp::Ne)
                    || (op == BinaryOp::Eq
                        && matches!(self.arena.expr(right).kind, ArenaExprKind::Null)) =>
            {
                let integer_order = matches!(
                    self.expr_types.get(&self.arena.expr(left).span),
                    Some(Type::Int)
                ) && matches!(
                    self.expr_types.get(&self.arena.expr(right).span),
                    Some(Type::Int)
                );
                let operator = match op {
                    BinaryOp::Lt if integer_order => Some(">="),
                    BinaryOp::Le if integer_order => Some(">"),
                    BinaryOp::Ne => Some("=="),
                    BinaryOp::Eq => Some("!="),
                    _ => None,
                };
                if let Some(operator) = operator {
                    let between =
                        self.arena.expr(left).span.end()..self.arena.expr(right).span.start();
                    let original = match op {
                        BinaryOp::Lt => "<",
                        BinaryOp::Le => "<=",
                        BinaryOp::Ne => "!=",
                        BinaryOp::Eq => "==",
                        _ => unreachable!(),
                    };
                    let Some(offset) = self.source[between.clone()].find(original) else {
                        return;
                    };
                    let token_start = between.start + offset;
                    format!(
                        "{}{}{}",
                        &self.source[condition.span.start()..token_start],
                        operator,
                        &self.source[token_start + original.len()..condition.span.end()]
                    )
                } else {
                    format!("! ({})", source_expr(branch.condition))
                }
            }
            _ => return,
        };
        let replacement = format!(
            "guard {inverse} else {}",
            &self.source[block_span.start()..block_span.end()]
        );
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "leading failure branch can use a Boolean guard",
            )
            .with_code(DiagnosticCode::LintBooleanGuard)
            .with_label(Label::secondary(
                stmt.span,
                "continue only when the condition succeeds",
            ))
            .with_fix_hint(FixHint::replacement(
                stmt.span,
                "use explicit guard failure branch",
                replacement,
            )),
        );
    }

    /// A match arm whose braces hold one postfix-guarded statement is printed
    /// without them, `0 => return x when c`. When the guard fix replaces the
    /// only statement of such a block, it replaces the braces too.
    pub(super) fn widen_guard_fix_to_arm_block(&mut self, block: BlockId) {
        let block = self.arena.block(block);
        let mut statements = self.arena.stmt_ids(block.statements);
        let (Some(only), None) = (statements.next(), statements.next()) else {
            return;
        };
        if !block.params.is_empty() {
            return;
        }
        let stmt_span = self.arena.stmt(only).span;
        let block_span = self.arena.span(block.span);
        let braces_only = self
            .source
            .get(block_span.start()..stmt_span.start())
            .zip(self.source.get(stmt_span.end()..block_span.end()))
            .is_some_and(|(open, close)| open.trim() == "{" && close.trim() == "}");
        if !braces_only {
            return;
        }
        for hint in self
            .diagnostics
            .iter_mut()
            .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferGuard))
            .flat_map(|diagnostic| diagnostic.fix_hints.iter_mut())
            .filter(|hint| hint.span == Some(stmt_span))
        {
            hint.span = Some(block_span);
        }
    }

    pub(super) fn lint_if_as_guard(&mut self, branches: ArenaRange, else_block: Option<BlockId>, span: Span) {
        if self.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == Some(DiagnosticCode::LintBooleanGuard)
                && diagnostic
                    .fix_hints
                    .iter()
                    .any(|fix| fix.span == Some(span))
        }) {
            return;
        }
        // Only single-branch if with no else
        if branches.len() != 1 || else_block.is_some() {
            return;
        }
        let branch = self.arena.if_branches(branches)[0].clone();
        if matches!(
            self.arena.expr(branch.condition).kind,
            ArenaExprKind::PatternCondition { .. }
        ) {
            return;
        }
        // Only single-statement body
        let branch_stmts: Vec<StmtId> = self
            .arena
            .stmt_ids(self.arena.block(branch.block).statements)
            .collect();
        let [only_stmt] = branch_stmts.as_slice() else {
            return;
        };
        let keyword = match self.arena.stmt(*only_stmt).kind {
            ArenaStmtKind::Break { .. } => "break",
            ArenaStmtKind::Continue => "continue",
            ArenaStmtKind::Return(_) => "return",
            ArenaStmtKind::YieldDelegate(_) => "yield",
            ArenaStmtKind::Yield(_) => "yield",
            _ => return,
        };
        // Preserve grouping that expression spans can omit around pipelines and runs.
        let Some(condition) = self
            .source
            .get(span.start() + 2..self.arena.span(self.arena.block(branch.block).span).start())
            .map(str::trim)
        else {
            return;
        };
        let (guard_word, condition) = if matches!(
            self.arena.expr(branch.condition).kind,
            ArenaExprKind::Unary {
                op: UnaryOp::Not,
                ..
            }
        ) && condition.starts_with('!')
        {
            ("unless", condition[1..].trim())
        } else {
            ("when", condition)
        };
        let Some(action) = self.source.get(self.arena.stmt(*only_stmt).span.range()) else {
            return;
        };
        let action = action.trim().trim_end_matches(';').trim_end();
        let payload = action
            .strip_prefix(keyword)
            .unwrap_or_default()
            .trim_start();
        let action = if payload.starts_with("run ") || payload.starts_with("run.") {
            format!("{keyword} ({payload})")
        } else {
            action.to_string()
        };
        let replacement = format!("{action} {guard_word} {condition}");
        // Measure the formatter's spelling so redundant grouping never decides
        // whether the guard is reported; a condition whose line breaks the
        // formatter keeps also keeps its block.
        let canonical = super::super::format::Formatter::new().format_source(span.source_id, &replacement);
        let replacement = canonical.formatted.trim_end();
        // Keep blocks whose condition or payload needs a readable multiline layout.
        if !canonical.diagnostics.is_empty()
            || replacement.contains('\n')
            || replacement.chars().count() > 88
        {
            return;
        }
        let diagnostic = Diagnostic::new(
            Severity::Warning,
            format!("use `{keyword} {guard_word}` instead of a single-action `if`"),
        )
        .with_code(DiagnosticCode::LintPreferGuard)
        .with_label(Label::secondary(span, "replace with postfix guard"));
        // Replacing the whole branch would discard comments attached to its body.
        self.diagnostics
            .push(if span_may_contain_comment(self.source, span) {
                diagnostic.with_note("comments inside the branch need a manual rewrite")
            } else {
                diagnostic.with_fix_hint(FixHint::replacement(
                    span,
                    format!("use `{keyword} {guard_word}`"),
                    replacement,
                ))
            });
    }
}
