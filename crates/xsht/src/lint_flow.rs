use super::{
    ArenaAssignTargetKind, ArenaBuilderEntryKind, ArenaCallArg, ArenaCallArgKind, ArenaCommand,
    ArenaCommandArg, ArenaCommandArgKind, ArenaEnvAssignmentValue, ArenaExprKind, ArenaExprOrRun,
    ArenaFmtPart, ArenaPipeStage, ArenaPipeStageKind, ArenaRange, ArenaRecordFieldKind,
    ArenaRedirectionTarget, ArenaSpawnTarget, ArenaStmtKind, ArenaStreamStage, ArenaSugar,
    ArenaWordPart, AssignTargetId, AstArena, BinaryOp, BlockId, BuilderBlockId, CommandStmtId,
    ExprId, RunFormId, StmtId,
};

/// The normal and abrupt exits reachable from a statement or expression.
///
/// `fallthrough` alone decides whether the next source statement can run. The
/// other bits are retained until the enclosing construct consumes them: a loop
/// consumes its body's `break` and `continue`, while a function boundary
/// consumes `return`. `terminates` denotes a proven path with no normal
/// successor, such as `match-no-arm`, `abort`, or a looping path.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct FlowSummary {
    pub(super) fallthrough: bool,
    pub(super) returns: bool,
    pub(super) breaks: bool,
    pub(super) continues: bool,
    pub(super) terminates: bool,
}

impl FlowSummary {
    pub(super) const fn fallthrough() -> Self {
        Self {
            fallthrough: true,
            returns: false,
            breaks: false,
            continues: false,
            terminates: false,
        }
    }

    pub(super) const fn returning() -> Self {
        Self {
            fallthrough: false,
            returns: true,
            breaks: false,
            continues: false,
            terminates: false,
        }
    }

    pub(super) const fn breaking() -> Self {
        Self {
            fallthrough: false,
            returns: false,
            breaks: true,
            continues: false,
            terminates: false,
        }
    }

    pub(super) const fn continuing() -> Self {
        Self {
            fallthrough: false,
            returns: false,
            breaks: false,
            continues: true,
            terminates: false,
        }
    }

    pub(super) const fn terminating() -> Self {
        Self {
            fallthrough: false,
            returns: false,
            breaks: false,
            continues: false,
            terminates: true,
        }
    }

    pub(super) fn union(mut self, other: Self) -> Self {
        self.fallthrough |= other.fallthrough;
        self.returns |= other.returns;
        self.breaks |= other.breaks;
        self.continues |= other.continues;
        self.terminates |= other.terminates;
        self
    }

    /// Compose `next` after the normal path in `self`, retaining every abrupt
    /// exit that escaped earlier statements in the sequence.
    pub(super) fn then(mut self, next: Self) -> Self {
        if self.fallthrough {
            self.fallthrough = next.fallthrough;
            self.returns |= next.returns;
            self.breaks |= next.breaks;
            self.continues |= next.continues;
            self.terminates |= next.terminates;
        }
        self
    }
}

pub(super) fn block_flow(arena: &AstArena, block: BlockId) -> FlowSummary {
    let mut flow = FlowSummary::fallthrough();
    for stmt in arena.stmt_ids(arena.block(block).statements) {
        if !flow.fallthrough {
            break;
        }
        flow = flow.then(stmt_flow(arena, stmt));
    }
    flow
}

pub(super) fn stmt_flow(arena: &AstArena, stmt: StmtId) -> FlowSummary {
    match arena.stmt(stmt).kind {
        ArenaStmtKind::Export(inner) => stmt_flow(arena, inner),
        // Control flow is the form's meaning, which only its expansion states.
        // The exception is a guard on a literal: `guard true` never runs its
        // else block and `guard false` always does, which the expansion's
        // `if !CONDITION` does not say to a reader of its condition's flow.
        ArenaStmtKind::Sugar {
            form,
            operands,
            expansion,
        } => match arena.sugar(form, operands) {
            ArenaSugar::Guard {
                condition,
                else_block,
            } => match arena.expr(condition).kind {
                ArenaExprKind::Bool(true) => FlowSummary::fallthrough(),
                ArenaExprKind::Bool(false) => block_flow(arena, else_block),
                _ => stmt_flow(arena, expansion),
            },
            _ => stmt_flow(arena, expansion),
        },
        ArenaStmtKind::Let { initializer, .. }
        | ArenaStmtKind::Const { initializer, .. }
        | ArenaStmtKind::Var { initializer, .. } => expr_or_run_flow(arena, &initializer),
        ArenaStmtKind::Assign { target, value, .. } => {
            assign_target_flow(arena, target).then(expr_or_run_flow(arena, &value))
        }
        ArenaStmtKind::Return(value) => value
            .as_ref()
            .map(|value| expr_or_run_flow(arena, value))
            .unwrap_or_else(FlowSummary::fallthrough)
            .then(FlowSummary::returning()),
        // A deferred expression is registered now and runs only during unwind.
        // It must still be linted, but it cannot make following source dead.
        ArenaStmtKind::Defer(..) => FlowSummary::fallthrough(),
        ArenaStmtKind::Yield(value) => expr_or_run_flow(arena, &value),
        ArenaStmtKind::If {
            branches,
            else_block,
        } => if_stmt_flow(arena, branches, else_block),
        ArenaStmtKind::While { condition, block } => {
            let condition = expr_flow(arena, condition);
            let body = block_flow(arena, block);
            // The condition may be false before the first iteration.
            FlowSummary {
                fallthrough: condition.fallthrough,
                returns: condition.returns | body.returns,
                breaks: false,
                continues: false,
                terminates: condition.terminates | body.terminates,
            }
        }
        ArenaStmtKind::For { iter, block, .. } => {
            let iter = expr_flow(arena, iter);
            let body = block_flow(arena, block);
            // A successful iterator may be empty.
            FlowSummary {
                fallthrough: iter.fallthrough,
                returns: iter.returns | body.returns,
                breaks: false,
                continues: false,
                terminates: iter.terminates | body.terminates,
            }
        }
        ArenaStmtKind::With {
            bindings,
            body,
            else_block,
            ..
        } => with_stmt_flow(arena, bindings, body, else_block),
        ArenaStmtKind::Loop { block } => loop_flow(arena, block),
        ArenaStmtKind::Guard {
            initializer,
            else_block,
            ..
        } => {
            let initializer = expr_or_run_flow(arena, &initializer);
            let else_flow = block_flow(arena, else_block);
            // A successful guard always continues after the statement.
            initializer.then(FlowSummary::fallthrough().union(else_flow))
        }
        ArenaStmtKind::Assert { condition, message } => {
            expr_flow(arena, condition).then(FlowSummary::fallthrough().union(match message {
                Some(message) => expr_flow(arena, message).then(FlowSummary::terminating()),
                None => FlowSummary::terminating(),
            }))
        }
        ArenaStmtKind::Break { value } => value
            .map(|value| expr_flow(arena, value))
            .unwrap_or_else(FlowSummary::fallthrough)
            .then(FlowSummary::breaking()),
        ArenaStmtKind::Continue => FlowSummary::continuing(),
        ArenaStmtKind::Match { value, arms } => {
            let value = expr_flow(arena, value);
            if !value.fallthrough {
                return value;
            }
            // Every unmatched or guard-rejected path produces the guaranteed
            // `match-no-arm` runtime error rather than falling through.
            let mut arms_flow = FlowSummary::terminating();
            for arm in arena.match_arms(arms) {
                arms_flow = arms_flow.union(block_flow(arena, arm.block));
            }
            value.then(arms_flow)
        }
        ArenaStmtKind::Command(command) => command_flow(arena, command),
        ArenaStmtKind::TailBareIdent(_) => FlowSummary::fallthrough(),
        ArenaStmtKind::Expr(expr) | ArenaStmtKind::YieldDelegate(expr) => expr_flow(arena, expr),
        // The status is evaluated, and then nothing after the statement runs.
        ArenaStmtKind::Exit(status) => expr_flow(arena, status).then(FlowSummary::terminating()),
        ArenaStmtKind::Use(_)
        | ArenaStmtKind::TypeDef(_)
        | ArenaStmtKind::ErrorDef(_)
        | ArenaStmtKind::ProcDef(_)
        | ArenaStmtKind::CliMain(_)
        | ArenaStmtKind::PureDef(_)
        | ArenaStmtKind::StreamDef(_)
        | ArenaStmtKind::SignalHook(_) => FlowSummary::fallthrough(),
    }
}

pub(super) fn if_stmt_flow(
    arena: &AstArena,
    branches: ArenaRange,
    else_block: Option<BlockId>,
) -> FlowSummary {
    let mut flow = FlowSummary::default();
    let mut next_condition_reachable = true;
    for branch in arena.if_branches(branches) {
        if !next_condition_reachable {
            break;
        }
        let condition = expr_flow(arena, branch.condition);
        flow = flow.union(FlowSummary {
            fallthrough: false,
            returns: condition.returns,
            breaks: condition.breaks,
            continues: condition.continues,
            terminates: condition.terminates,
        });
        if condition.fallthrough {
            flow = flow.union(block_flow(arena, branch.block));
        }
        next_condition_reachable = condition.fallthrough;
    }
    if next_condition_reachable {
        flow = flow.union(match else_block {
            Some(block) => block_flow(arena, block),
            None => FlowSummary::fallthrough(),
        });
    }
    flow
}

pub(super) fn with_stmt_flow(
    arena: &AstArena,
    bindings: ArenaRange,
    body: BlockId,
    else_block: BlockId,
) -> FlowSummary {
    let mut bindings_flow = FlowSummary::fallthrough();
    for binding in arena.with_bindings(bindings) {
        bindings_flow = bindings_flow.then(expr_flow(arena, binding.initializer));
    }
    bindings_flow.then(block_flow(arena, body).union(block_flow(arena, else_block)))
}

pub(super) fn loop_flow(arena: &AstArena, block: BlockId) -> FlowSummary {
    let body = block_flow(arena, block);
    FlowSummary {
        fallthrough: body.breaks,
        returns: body.returns,
        breaks: false,
        continues: false,
        terminates: body.terminates || (!body.breaks && (body.fallthrough || body.continues)),
    }
}

pub(super) fn expr_or_run_flow(arena: &AstArena, value: &ArenaExprOrRun) -> FlowSummary {
    match value {
        ArenaExprOrRun::Expr(expr) => expr_flow(arena, *expr),
        ArenaExprOrRun::Run(run) => run_flow(arena, *run),
    }
}

pub(super) fn expr_flow(arena: &AstArena, expr: ExprId) -> FlowSummary {
    let arena_expr = arena.expr(expr);
    match arena_expr.kind {
        ArenaExprKind::ValuePipelineCall { input, call, .. } => {
            expr_flow(arena, input).then(expr_flow(arena, call))
        }

        ArenaExprKind::FmtString(parts) | ArenaExprKind::PathFmtString(parts) => arena
            .fmt_parts(parts)
            .fold(FlowSummary::fallthrough(), |flow, part| match part {
                ArenaFmtPart::Expr(expr, _) => flow.then(expr_flow(arena, expr)),
                ArenaFmtPart::Text(_) => flow,
            }),
        ArenaExprKind::List(items) | ArenaExprKind::Set(items) => arena
            .list_element_exprs(items)
            .fold(FlowSummary::fallthrough(), |flow, item| {
                flow.then(expr_flow(arena, item))
            }),
        ArenaExprKind::ListComp { qualifiers, .. }
        | ArenaExprKind::SetComp { qualifiers, .. }
        | ArenaExprKind::MapComp { qualifiers, .. } => {
            let first = arena
                .comp_qualifiers(qualifiers)
                .first()
                .expect("comprehension has initial for");
            expr_flow(arena, first.expr())
        }
        ArenaExprKind::Record(fields) => {
            arena
                .record_fields(fields)
                .iter()
                .fold(FlowSummary::fallthrough(), |flow, field| match field.kind {
                    ArenaRecordFieldKind::Computed { key, value, .. } => flow
                        .then(expr_flow(arena, key))
                        .then(expr_flow(arena, value)),
                    ArenaRecordFieldKind::Named { value, .. }
                    | ArenaRecordFieldKind::Path { value, .. } => {
                        flow.then(expr_flow(arena, value))
                    }
                    ArenaRecordFieldKind::Spread { expr, .. } => flow.then(expr_flow(arena, expr)),
                    ArenaRecordFieldKind::Shorthand { .. } => flow,
                })
        }
        ArenaExprKind::If {
            branches,
            else_value,
        } => if_expr_flow(arena, branches, else_value),
        ArenaExprKind::Match { value, arms }
        | ArenaExprKind::PatternTest { value, arms }
        | ArenaExprKind::PatternCondition { value, arms } => {
            let value = expr_flow(arena, value);
            if !value.fallthrough {
                return value;
            }
            let mut arms_flow = FlowSummary::terminating();
            for arm in arena.match_expr_arms(arms) {
                let arm_flow = arm
                    .guard
                    .map(|guard| expr_flow(arena, guard))
                    .unwrap_or_else(FlowSummary::fallthrough)
                    .then(expr_flow(arena, arm.value));
                arms_flow = arms_flow.union(arm_flow);
            }
            value.then(arms_flow)
        }
        ArenaExprKind::Unary { expr, .. } | ArenaExprKind::Try(expr) => expr_flow(arena, expr),
        ArenaExprKind::ComparisonChain(pairs) => {
            let mut operands = arena.comparison_chain_operands(pairs);
            let first = operands
                .next()
                .map(|operand| expr_flow(arena, operand))
                .unwrap_or_else(FlowSummary::fallthrough);
            let second = operands
                .next()
                .map(|operand| expr_flow(arena, operand))
                .unwrap_or_else(FlowSummary::fallthrough);
            let mut flow = first.then(second);
            for operand in operands {
                flow = flow.then(FlowSummary::fallthrough().union(expr_flow(arena, operand)));
            }
            flow
        }
        ArenaExprKind::Binary {
            op: BinaryOp::ResultFallback,
            left,
            right,
        } => expr_flow(arena, left).then(expr_flow(arena, right).union(FlowSummary::fallthrough())),
        ArenaExprKind::Binary { left, right, .. } => {
            expr_flow(arena, left).then(expr_flow(arena, right))
        }
        ArenaExprKind::Call { callee, args } => {
            let receiver_flow = expr_flow(arena, callee);
            let guarded = matches!(arena.expr(callee).kind, ArenaExprKind::NullSafeField { .. });
            let mut flow = FlowSummary::fallthrough();
            for arg in arena.call_args(args) {
                flow = flow.then(call_arg_flow(arena, arg));
            }
            receiver_flow.then(if guarded {
                flow.union(FlowSummary::fallthrough())
            } else {
                flow
            })
        }
        ArenaExprKind::Field { base, .. } | ArenaExprKind::NullSafeField { base, .. } => {
            expr_flow(arena, base)
        }
        ArenaExprKind::Index {
            base,
            index,
            guarded,
        } => {
            let selected = expr_flow(arena, index);
            expr_flow(arena, base).then(if guarded {
                selected.union(FlowSummary::fallthrough())
            } else {
                selected
            })
        }
        ArenaExprKind::Slice {
            base,
            start,
            end,
            guarded,
        } => {
            let selected = start
                .map(|start| expr_flow(arena, start))
                .unwrap_or_else(FlowSummary::fallthrough)
                .then(
                    end.map(|end| expr_flow(arena, end))
                        .unwrap_or_else(FlowSummary::fallthrough),
                );
            expr_flow(arena, base).then(if guarded {
                selected.union(FlowSummary::fallthrough())
            } else {
                selected
            })
        }
        ArenaExprKind::Pipeline { input, stages } => {
            let mut flow = expr_flow(arena, input);
            for stage in arena.pipe_stages(stages) {
                flow = flow.then(pipe_stage_flow(arena, stage));
            }
            flow
        }
        ArenaExprKind::StructuredPipeline { input, stages } => {
            let mut flow = expr_flow(arena, input);
            for stage in arena.stream_stages(stages) {
                flow = flow.then(stream_stage_flow(arena, stage));
            }
            flow
        }
        ArenaExprKind::Run(run) => run_flow(arena, run),
        ArenaExprKind::Spawn(form) => match form.target {
            ArenaSpawnTarget::Run(run) => run_flow(arena, run),
            ArenaSpawnTarget::Command(command) => expr_flow(arena, command),
        },
        ArenaExprKind::Wait(form) => expr_flow(arena, form.target),
        ArenaExprKind::BuilderCall { call, block } => {
            expr_flow(arena, call).then(builder_block_setup_flow(arena, block))
        }
        ArenaExprKind::Require { value, .. } | ArenaExprKind::Convert { value, .. } => {
            expr_flow(arena, value)
        }
        ArenaExprKind::ContextScope { input, block, .. } => {
            expr_flow(arena, input).then(FlowSummary::fallthrough().union(block_flow(arena, block)))
        }
        // Creating the directory may fail before the body runs.
        ArenaExprKind::TempDirScope { path, block, .. } => {
            let entered = FlowSummary::fallthrough().union(block_flow(arena, block));
            match path {
                Some(path) => expr_flow(arena, path).then(entered),
                None => entered,
            }
        }
        // The body runs once every value is bound.
        ArenaExprKind::ResourceScope {
            bindings, block, ..
        } => arena
            .with_bindings(bindings)
            .iter()
            .fold(FlowSummary::fallthrough(), |flow, binding| {
                flow.then(expr_flow(arena, binding.initializer))
            })
            .then(block_flow(arena, block)),
        ArenaExprKind::ErrorContext { message, block } => {
            expr_flow(arena, message).then(block_flow(arena, block))
        }
        ArenaExprKind::Capture(block)
        | ArenaExprKind::ValueBlock(block)
        | ArenaExprKind::Collect { block } => block_flow(arena, block),
        ArenaExprKind::Loop { block } => loop_flow(arena, block),
        // A retry retries failed attempts, but a normally-completing attempt
        // produces the expression's `Result`; it is not an infinite loop.
        ArenaExprKind::Retry { delays, block, .. } => arena
            .expr_ids(delays)
            .fold(FlowSummary::fallthrough(), |flow, delay| {
                flow.then(expr_flow(arena, delay))
            })
            .then(block_flow(arena, block)),
        ArenaExprKind::Null
        | ArenaExprKind::Bool(_)
        | ArenaExprKind::Int(_)
        | ArenaExprKind::Float(_)
        | ArenaExprKind::Duration(_)
        | ArenaExprKind::Str(_)
        | ArenaExprKind::PathStr(_)
        | ArenaExprKind::GlobStr(_)
        | ArenaExprKind::Bytes(_)
        | ArenaExprKind::Regex(_)
        | ArenaExprKind::Ident(_)
        | ArenaExprKind::Item
        | ArenaExprKind::LastStatus
        | ArenaExprKind::EnvString(_)
        | ArenaExprKind::EnvPathList => FlowSummary::fallthrough(),
    }
}

pub(super) fn if_expr_flow(arena: &AstArena, branches: ArenaRange, else_value: ExprId) -> FlowSummary {
    let mut flow = FlowSummary::default();
    let mut next_condition_reachable = true;
    for branch in arena.if_expr_branches(branches) {
        if !next_condition_reachable {
            break;
        }
        let condition = expr_flow(arena, branch.condition);
        flow = flow.union(FlowSummary {
            fallthrough: false,
            returns: condition.returns,
            breaks: condition.breaks,
            continues: condition.continues,
            terminates: condition.terminates,
        });
        if condition.fallthrough {
            flow = flow.union(expr_flow(arena, branch.value));
        }
        next_condition_reachable = condition.fallthrough;
    }
    if next_condition_reachable {
        flow = flow.union(expr_flow(arena, else_value));
    }
    flow
}

pub(super) fn call_arg_flow(arena: &AstArena, arg: &ArenaCallArg) -> FlowSummary {
    match arg.kind {
        ArenaCallArgKind::Positional(expr)
        | ArenaCallArgKind::Named { value: expr, .. }
        | ArenaCallArgKind::Splice { value: expr, .. }
        | ArenaCallArgKind::NamedSpread { value: expr, .. } => expr_flow(arena, expr),
    }
}

pub(super) fn pipe_stage_flow(arena: &AstArena, stage: &ArenaPipeStage) -> FlowSummary {
    match &stage.kind {
        ArenaPipeStageKind::Expr(expr) => expr_flow(arena, *expr),
        ArenaPipeStageKind::Stream(stage) => stream_stage_flow(arena, stage),
    }
}

pub(super) fn stream_stage_flow(arena: &AstArena, stage: &ArenaStreamStage) -> FlowSummary {
    let mut flow = FlowSummary::fallthrough();
    for arg in arena.call_args(stage.args) {
        flow = flow.then(call_arg_flow(arena, arg));
    }
    // Stream-stage blocks execute in their own item region. Their flow does
    // not determine whether the enclosing pipeline expression returns.
    flow
}

pub(super) fn builder_block_setup_flow(arena: &AstArena, block: BuilderBlockId) -> FlowSummary {
    let mut flow = FlowSummary::fallthrough();
    for entry in arena.builder_entries(arena.builder_block(block).entries) {
        match &entry.kind {
            ArenaBuilderEntryKind::Field { value, .. } => {
                flow = flow.then(expr_flow(arena, *value));
            }
            ArenaBuilderEntryKind::Entry { args, block, .. } => {
                for arg in arena.command_args(*args) {
                    flow = flow.then(command_arg_flow(arena, arg));
                }
                if let Some(block) = block {
                    flow = flow.then(builder_block_setup_flow(arena, *block));
                }
            }
            // Task blocks are independent regions. They are analyzed for their
            // own diagnostics by the linter, not as eager builder setup.
            ArenaBuilderEntryKind::Task { .. } | ArenaBuilderEntryKind::Stmt(_) => {}
        }
    }
    flow
}

pub(super) fn assign_target_flow(arena: &AstArena, target: AssignTargetId) -> FlowSummary {
    match arena.assign_target(target).kind {
        ArenaAssignTargetKind::Name(_) | ArenaAssignTargetKind::Env(_) => {
            FlowSummary::fallthrough()
        }
        ArenaAssignTargetKind::Field { base, .. } => assign_target_flow(arena, base),
        ArenaAssignTargetKind::Index { base, index } => {
            assign_target_flow(arena, base).then(expr_flow(arena, index))
        }
    }
}

pub(super) fn command_flow(arena: &AstArena, command: CommandStmtId) -> FlowSummary {
    match arena.command_stmt(command).command.clone() {
        ArenaCommand::Proc { args, .. } => arena
            .command_args(args)
            .iter()
            .fold(FlowSummary::fallthrough(), |flow, arg| {
                flow.then(command_arg_flow(arena, arg))
            }),
        ArenaCommand::Core {
            args, env, block, ..
        } => {
            let mut flow = FlowSummary::fallthrough();
            for arg in arena.command_args(args) {
                flow = flow.then(command_arg_flow(arena, arg));
            }
            for assignment in arena.env_assignments(env) {
                let assignment_flow = match &assignment.value {
                    ArenaEnvAssignmentValue::CommandArg(arg) => command_arg_flow(arena, arg),
                    ArenaEnvAssignmentValue::Expr(expr) => expr_flow(arena, *expr),
                };
                flow = flow.then(assignment_flow);
            }
            if let Some(block) = block {
                flow.then(block_flow(arena, block))
            } else {
                flow
            }
        }
        ArenaCommand::Run(run) => run_flow(arena, run),
    }
}

pub(super) fn run_flow(arena: &AstArena, run: RunFormId) -> FlowSummary {
    let mut flow = FlowSummary::fallthrough();
    for segment in arena.run_segments(arena.run_form(run).segments) {
        if let Some(timeout) = segment.timeout {
            flow = flow.then(expr_flow(arena, timeout));
        }
        if let Some(cpu_max) = segment.cpu_max {
            flow = flow.then(expr_flow(arena, cpu_max));
        }
        if let Some(accept) = segment.accept {
            flow = flow.then(expr_flow(arena, accept));
        }
        for assignment in arena.env_assignments(segment.env) {
            let assignment_flow = match &assignment.value {
                ArenaEnvAssignmentValue::CommandArg(arg) => command_arg_flow(arena, arg),
                ArenaEnvAssignmentValue::Expr(expr) => expr_flow(arena, *expr),
            };
            flow = flow.then(assignment_flow);
        }
        flow = flow.then(command_arg_flow(arena, &segment.target));
        for arg in arena.command_args(segment.args) {
            flow = flow.then(command_arg_flow(arena, arg));
        }
        for redirection in arena.redirections(segment.redirections) {
            let target = match &redirection.target {
                ArenaRedirectionTarget::Path(arg) | ArenaRedirectionTarget::Fd(arg) => arg,
            };
            flow = flow.then(command_arg_flow(arena, target));
        }
    }
    flow
}

pub(super) fn command_arg_flow(arena: &AstArena, arg: &ArenaCommandArg) -> FlowSummary {
    match &arg.kind {
        ArenaCommandArgKind::Word(parts) => {
            arena
                .word_parts(*parts)
                .fold(FlowSummary::fallthrough(), |flow, part| match part {
                    ArenaWordPart::Interpolation(expr) | ArenaWordPart::Shorthand(expr) => {
                        flow.then(expr_flow(arena, expr))
                    }
                    ArenaWordPart::Bare(_) | ArenaWordPart::Quoted(_) => flow,
                })
        }
        ArenaCommandArgKind::SpliceExpr(expr) | ArenaCommandArgKind::Typed(expr) => {
            expr_flow(arena, *expr)
        }
        ArenaCommandArgKind::SpliceName(_) => FlowSummary::fallthrough(),
    }
}
