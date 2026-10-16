use xsh::frontend::syntax::arena::ArenaRunTailOperand;

use super::{
    ArenaAssignTargetKind, ArenaBuilderEntryKind, ArenaCallArg, ArenaCallArgKind, ArenaCommand,
    ArenaCommandArg, ArenaCommandArgKind, ArenaEnvAssignmentValue, ArenaExprKind, ArenaExprOrRun,
    ArenaFmtPart, ArenaPipeStage, ArenaPipeStageKind, ArenaRange, ArenaRecordFieldKind,
    ArenaRedirectionTarget, ArenaSpawnTarget, ArenaStmtKind, ArenaStreamStage, ArenaSugar,
    ArenaWordPart, AssignTargetId, AstArena, BinaryOp, BlockId,
    BuilderBlockId, CommandStmtId, ExprId, FxHashMap, RunFormId, StmtId,
};

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

#[derive(Default)]
pub(super) struct FlowFacts {
    expressions: FxHashMap<ExprId, FlowSummary>,
    statements: FxHashMap<StmtId, FlowSummary>,
    blocks: FxHashMap<BlockId, FlowSummary>,
    assign_targets: FxHashMap<AssignTargetId, FlowSummary>,
    builder_blocks: FxHashMap<BuilderBlockId, FlowSummary>,
    commands: FxHashMap<CommandStmtId, FlowSummary>,
    runs: FxHashMap<RunFormId, FlowSummary>,
}

impl FlowFacts {
    fn expression(&self, expr: ExprId) -> FlowSummary {
        *self
            .expressions
            .get(&expr)
            .expect("flow expression child was not visited")
    }

    pub(super) fn statement(&self, stmt: StmtId) -> FlowSummary {
        *self
            .statements
            .get(&stmt)
            .expect("flow statement child was not visited")
    }

    fn block(&self, block: BlockId) -> FlowSummary {
        *self.blocks.get(&block).expect("flow block child was not visited")
    }

    fn assign_target(&self, target: AssignTargetId) -> FlowSummary {
        *self
            .assign_targets
            .get(&target)
            .expect("flow assignment target child was not visited")
    }

    fn builder_block(&self, block: BuilderBlockId) -> FlowSummary {
        *self
            .builder_blocks
            .get(&block)
            .expect("flow builder block child was not visited")
    }

    fn command(&self, command: CommandStmtId) -> FlowSummary {
        *self
            .commands
            .get(&command)
            .expect("flow command child was not visited")
    }

    fn run(&self, run: RunFormId) -> FlowSummary {
        *self.runs.get(&run).expect("flow run child was not visited")
    }

    pub(super) fn record_expression(&mut self, arena: &AstArena, expr: ExprId) {
        let flow = expr_flow(arena, self, expr);
        assert!(
            self.expressions.insert(expr, flow).is_none(),
            "flow expression was summarized more than once"
        );
    }

    pub(super) fn record_statement(&mut self, arena: &AstArena, stmt: StmtId) {
        let flow = stmt_flow(arena, self, stmt);
        assert!(
            self.statements.insert(stmt, flow).is_none(),
            "flow statement was summarized more than once"
        );
    }

    pub(super) fn record_block(&mut self, block: BlockId, flow: FlowSummary) {
        assert!(
            self.blocks.insert(block, flow).is_none(),
            "flow block was summarized more than once"
        );
    }

    pub(super) fn record_assign_target(&mut self, arena: &AstArena, target: AssignTargetId) {
        let flow = assign_target_flow(arena, self, target);
        assert!(
            self.assign_targets.insert(target, flow).is_none(),
            "flow assignment target was summarized more than once"
        );
    }

    pub(super) fn record_builder_block(&mut self, arena: &AstArena, block: BuilderBlockId) {
        let flow = builder_block_setup_flow(arena, self, block);
        assert!(
            self.builder_blocks.insert(block, flow).is_none(),
            "flow builder block was summarized more than once"
        );
    }

    pub(super) fn record_command(&mut self, arena: &AstArena, command: CommandStmtId) {
        let flow = command_flow(arena, self, command);
        assert!(
            self.commands.insert(command, flow).is_none(),
            "flow command was summarized more than once"
        );
    }

    pub(super) fn record_run(&mut self, arena: &AstArena, run: RunFormId) {
        let flow = run_flow(arena, self, run);
        assert!(
            self.runs.insert(run, flow).is_none(),
            "flow run was summarized more than once"
        );
    }
}

fn stmt_flow(arena: &AstArena, facts: &FlowFacts, stmt: StmtId) -> FlowSummary {
    match arena.stmt(stmt).kind {
        ArenaStmtKind::Export(inner) => facts.statement(inner),
        ArenaStmtKind::Sugar { form, operands, .. } => match arena.sugar(form, operands) {
            ArenaSugar::Repeat { count, body } => {
                zero_or_more_loop(facts.expression(count), facts.block(body))
            }
            ArenaSugar::Guarded {
                stmt,
                condition,
                ..
            } => guarded_statement(facts.expression(condition), facts.statement(stmt)),
            ArenaSugar::Guard {
                condition,
                else_block,
            } => match arena.expr(condition).kind {
                ArenaExprKind::Bool(true) => FlowSummary::fallthrough(),
                ArenaExprKind::Bool(false) => facts.block(else_block),
                _ => facts
                    .expression(condition)
                    .then(FlowSummary::fallthrough().union(facts.block(else_block))),
            },
            ArenaSugar::Fail { failure, cause } => {
                let mut flow = facts.expression(failure);
                if let Some(cause) = cause {
                    flow = flow.then(facts.expression(cause));
                }
                flow.then(FlowSummary::returning())
            }
            ArenaSugar::Atomically { dest, body, .. } => {
                facts.expression(dest).then(facts.block(body))
            }
            ArenaSugar::ForIndex { source, body, .. } => {
                zero_or_more_loop(facts.expression(source), facts.block(body))
            }
            ArenaSugar::GuardFail { condition, fail } => {
                guarded_statement(facts.expression(condition), facts.statement(fail))
            }
            ArenaSugar::WaitUntil {
                condition,
                limit,
                pace,
            } => {
                let pace_flow = match pace {
                    super::WaitPace::Default => FlowSummary::fallthrough(),
                    super::WaitPace::Every(interval) => facts.expression(interval),
                    super::WaitPace::Backoff { first, cap } => facts
                        .expression(first)
                        .then(facts.expression(cap)),
                };
                let iteration = facts
                    .expression(condition)
                    .then(FlowSummary::fallthrough().union(FlowSummary::breaking()));
                pace_flow
                    .then(facts.expression(limit))
                    .then(loop_summary(iteration))
            }
        },
        ArenaStmtKind::Let { initializer, .. }
        | ArenaStmtKind::Const { initializer, .. }
        | ArenaStmtKind::Var { initializer, .. } => expr_or_run_flow(facts, &initializer),
        ArenaStmtKind::Assign { target, value, .. } => {
            facts
                .assign_target(target)
                .then(expr_or_run_flow(facts, &value))
        }
        ArenaStmtKind::Return(value) => value
            .as_ref()
            .map(|value| expr_or_run_flow(facts, value))
            .unwrap_or_else(FlowSummary::fallthrough)
            .then(FlowSummary::returning()),
        ArenaStmtKind::Defer(..) => FlowSummary::fallthrough(),
        ArenaStmtKind::Yield(value) => expr_or_run_flow(facts, &value),
        ArenaStmtKind::If {
            branches,
            else_block,
        } => if_stmt_flow(arena, facts, branches, else_block),
        ArenaStmtKind::While { condition, block } => {
            let condition = facts.expression(condition);
            let body = facts.block(block);
            FlowSummary {
                fallthrough: condition.fallthrough,
                returns: condition.returns | body.returns,
                breaks: false,
                continues: false,
                terminates: condition.terminates | body.terminates,
            }
        }
        ArenaStmtKind::For { iter, block, .. } => {
            zero_or_more_loop(facts.expression(iter), facts.block(block))
        }
        ArenaStmtKind::With {
            bindings,
            body,
            else_block,
            ..
        } => with_stmt_flow(arena, facts, bindings, body, else_block),
        ArenaStmtKind::Loop { block } => loop_summary(facts.block(block)),
        ArenaStmtKind::Guard {
            initializer,
            else_block,
            ..
        } => {
            let initializer = expr_or_run_flow(facts, &initializer);
            let else_flow = facts.block(else_block);
            initializer.then(FlowSummary::fallthrough().union(else_flow))
        }
        ArenaStmtKind::Assert { condition, message } => {
            facts.expression(condition).then(FlowSummary::fallthrough().union(match message {
                Some(message) => facts
                    .expression(message)
                    .then(FlowSummary::terminating()),
                None => FlowSummary::terminating(),
            }))
        }
        ArenaStmtKind::Break { value } => value
            .map(|value| facts.expression(value))
            .unwrap_or_else(FlowSummary::fallthrough)
            .then(FlowSummary::breaking()),
        ArenaStmtKind::Continue => FlowSummary::continuing(),
        ArenaStmtKind::Match { value, arms } => {
            let value = facts.expression(value);
            if !value.fallthrough {
                return value;
            }
            let mut arms_flow = FlowSummary::terminating();
            for arm in arena.match_arms(arms) {
                arms_flow = arms_flow.union(facts.block(arm.block));
            }
            value.then(arms_flow)
        }
        ArenaStmtKind::Command(command) => facts.command(command),
        ArenaStmtKind::TailBareIdent(_) => FlowSummary::fallthrough(),
        ArenaStmtKind::Expr(expr) | ArenaStmtKind::YieldDelegate(expr) => facts.expression(expr),
        ArenaStmtKind::Exit(status) => facts
            .expression(status)
            .then(FlowSummary::terminating()),
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

fn zero_or_more_loop(iter: FlowSummary, body: FlowSummary) -> FlowSummary {
    FlowSummary {
        fallthrough: iter.fallthrough,
        returns: iter.returns | body.returns,
        breaks: false,
        continues: false,
        terminates: iter.terminates | body.terminates,
    }
}

fn guarded_statement(condition: FlowSummary, stmt: FlowSummary) -> FlowSummary {
    condition.then(stmt.union(FlowSummary::fallthrough()))
}

pub(super) fn if_stmt_flow(
    arena: &AstArena,
    facts: &FlowFacts,
    branches: ArenaRange,
    else_block: Option<BlockId>,
) -> FlowSummary {
    let mut flow = FlowSummary::default();
    let mut next_condition_reachable = true;
    for branch in arena.if_branches(branches) {
        if !next_condition_reachable {
            break;
        }
        let condition = facts.expression(branch.condition);
        flow = flow.union(FlowSummary {
            fallthrough: false,
            returns: condition.returns,
            breaks: condition.breaks,
            continues: condition.continues,
            terminates: condition.terminates,
        });
        if condition.fallthrough {
            flow = flow.union(facts.block(branch.block));
        }
        next_condition_reachable = condition.fallthrough;
    }
    if next_condition_reachable {
        flow = flow.union(match else_block {
            Some(block) => facts.block(block),
            None => FlowSummary::fallthrough(),
        });
    }
    flow
}

pub(super) fn with_stmt_flow(
    arena: &AstArena,
    facts: &FlowFacts,
    bindings: ArenaRange,
    body: BlockId,
    else_block: BlockId,
) -> FlowSummary {
    let mut bindings_flow = FlowSummary::fallthrough();
    for binding in arena.with_bindings(bindings) {
        bindings_flow = bindings_flow.then(facts.expression(binding.initializer));
    }
    bindings_flow.then(facts.block(body).union(facts.block(else_block)))
}

fn loop_summary(body: FlowSummary) -> FlowSummary {
    FlowSummary {
        fallthrough: body.breaks,
        returns: body.returns,
        breaks: false,
        continues: false,
        terminates: body.terminates || (!body.breaks && (body.fallthrough || body.continues)),
    }
}

fn expr_or_run_flow(facts: &FlowFacts, value: &ArenaExprOrRun) -> FlowSummary {
    match value {
        ArenaExprOrRun::Expr(expr) => facts.expression(*expr),
        ArenaExprOrRun::Run(run) => facts.run(*run),
    }
}

fn expr_flow(arena: &AstArena, facts: &FlowFacts, expr: ExprId) -> FlowSummary {
    match arena.expr(expr).kind {
        ArenaExprKind::ValuePipelineCall { input, call, .. } => {
            facts.expression(input).then(facts.expression(call))
        }
        ArenaExprKind::FmtString(parts) | ArenaExprKind::PathFmtString(parts) => arena
            .fmt_parts(parts)
            .fold(FlowSummary::fallthrough(), |flow, part| match part {
                ArenaFmtPart::Expr(expr, _) => flow.then(facts.expression(expr)),
                ArenaFmtPart::Text(_) => flow,
            }),
        ArenaExprKind::List(items) | ArenaExprKind::Set(items) => arena
            .list_element_exprs(items)
            .fold(FlowSummary::fallthrough(), |flow, item| {
                flow.then(facts.expression(item))
            }),
        ArenaExprKind::ListComp { qualifiers, .. }
        | ArenaExprKind::SetComp { qualifiers, .. }
        | ArenaExprKind::MapComp { qualifiers, .. } => {
            let first = arena
                .comp_qualifiers(qualifiers)
                .first()
                .expect("comprehension has initial for");
            facts.expression(first.expr())
        }
        ArenaExprKind::Record(fields) => arena
            .record_fields(fields)
            .iter()
            .fold(FlowSummary::fallthrough(), |flow, field| match field.kind {
                ArenaRecordFieldKind::Computed { key, value, .. } => flow
                    .then(facts.expression(key))
                    .then(facts.expression(value)),
                ArenaRecordFieldKind::Named { value, .. }
                | ArenaRecordFieldKind::Path { value, .. } => flow.then(facts.expression(value)),
                ArenaRecordFieldKind::Spread { expr, .. } => flow.then(facts.expression(expr)),
                ArenaRecordFieldKind::Shorthand { .. } => flow,
            }),
        ArenaExprKind::If {
            branches,
            else_value,
        } => if_expr_flow(arena, facts, branches, else_value),
        ArenaExprKind::Match { value, arms } | ArenaExprKind::PatternTest { value, arms } => {
            match_expr_flow(arena, facts, value, arms)
        }
        ArenaExprKind::PatternCondition { value, .. } => pattern_condition_flow(facts.expression(value)),
        ArenaExprKind::Unary { expr, .. } | ArenaExprKind::Try(expr) => facts.expression(expr),
        ArenaExprKind::ComparisonChain(pairs) => {
            let mut operands = arena.comparison_chain_operands(pairs);
            let first = operands
                .next()
                .map(|operand| facts.expression(operand))
                .unwrap_or_else(FlowSummary::fallthrough);
            let second = operands
                .next()
                .map(|operand| facts.expression(operand))
                .unwrap_or_else(FlowSummary::fallthrough);
            let mut flow = first.then(second);
            for operand in operands {
                flow = flow.then(FlowSummary::fallthrough().union(facts.expression(operand)));
            }
            flow
        }
        ArenaExprKind::Binary {
            op: BinaryOp::ResultFallback,
            left,
            right,
        } => facts
            .expression(left)
            .then(facts.expression(right).union(FlowSummary::fallthrough())),
        ArenaExprKind::Binary { left, right, .. } => {
            facts.expression(left).then(facts.expression(right))
        }
        ArenaExprKind::Call { callee, args } => {
            let receiver_flow = facts.expression(callee);
            let guarded = matches!(arena.expr(callee).kind, ArenaExprKind::NullSafeField { .. });
            let mut flow = FlowSummary::fallthrough();
            for arg in arena.call_args(args) {
                flow = flow.then(call_arg_flow(facts, arg));
            }
            receiver_flow.then(if guarded {
                flow.union(FlowSummary::fallthrough())
            } else {
                flow
            })
        }
        ArenaExprKind::Field { base, .. } | ArenaExprKind::NullSafeField { base, .. } => {
            facts.expression(base)
        }
        ArenaExprKind::Index {
            base,
            index,
            guarded,
        } => {
            let selected = facts.expression(index);
            facts.expression(base).then(if guarded {
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
                .map(|start| facts.expression(start))
                .unwrap_or_else(FlowSummary::fallthrough)
                .then(
                    end.map(|end| facts.expression(end))
                        .unwrap_or_else(FlowSummary::fallthrough),
                );
            facts.expression(base).then(if guarded {
                selected.union(FlowSummary::fallthrough())
            } else {
                selected
            })
        }
        ArenaExprKind::Pipeline { input, stages } => {
            let mut flow = facts.expression(input);
            for stage in arena.pipe_stages(stages) {
                flow = flow.then(pipe_stage_flow(arena, facts, stage));
            }
            flow
        }
        ArenaExprKind::StructuredPipeline { input, stages } => {
            let mut flow = facts.expression(input);
            for stage in arena.stream_stages(stages) {
                flow = flow.then(stream_stage_flow(arena, facts, stage));
            }
            flow
        }
        ArenaExprKind::Run(run) => facts.run(run),
        ArenaExprKind::Spawn(form) => match form.target {
            ArenaSpawnTarget::Run(run) => facts.run(run),
            ArenaSpawnTarget::Command(command) => facts.expression(command),
        },
        ArenaExprKind::Wait(form) => facts.expression(form.target),
        ArenaExprKind::BuilderCall { call, block } => {
            facts.expression(call).then(facts.builder_block(block))
        }
        ArenaExprKind::Require { value, .. } | ArenaExprKind::Convert { value, .. } => {
            facts.expression(value)
        }
        ArenaExprKind::ContextScope { input, block, .. } => facts
            .expression(input)
            .then(FlowSummary::fallthrough().union(facts.block(block))),
        ArenaExprKind::TempDirScope { path, block, .. } => {
            let entered = FlowSummary::fallthrough().union(facts.block(block));
            match path {
                Some(path) => facts.expression(path).then(entered),
                None => entered,
            }
        }
        ArenaExprKind::ResourceScope {
            bindings, block, ..
        } => arena
            .with_bindings(bindings)
            .iter()
            .fold(FlowSummary::fallthrough(), |flow, binding| {
                flow.then(facts.expression(binding.initializer))
            })
            .then(facts.block(block)),
        ArenaExprKind::ErrorContext { message, block } => {
            facts.expression(message).then(facts.block(block))
        }
        ArenaExprKind::Capture(block)
        | ArenaExprKind::ValueBlock(block)
        | ArenaExprKind::Collect { block } => facts.block(block),
        ArenaExprKind::Loop { block } => loop_summary(facts.block(block)),
        ArenaExprKind::Retry { delays, block, .. } => arena
            .expr_ids(delays)
            .fold(FlowSummary::fallthrough(), |flow, delay| {
                flow.then(facts.expression(delay))
            })
            .then(facts.block(block)),
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

fn pattern_condition_flow(value: FlowSummary) -> FlowSummary {
    if value.fallthrough {
        value.then(FlowSummary::terminating().union(FlowSummary::fallthrough()))
    } else {
        value
    }
}

fn match_expr_flow(
    arena: &AstArena,
    facts: &FlowFacts,
    value: ExprId,
    arms: ArenaRange,
) -> FlowSummary {
    let value = facts.expression(value);
    if !value.fallthrough {
        return value;
    }
    let mut arms_flow = FlowSummary::terminating();
    for arm in arena.match_expr_arms(arms) {
        let arm_flow = arm
            .guard
            .map(|guard| facts.expression(guard))
            .unwrap_or_else(FlowSummary::fallthrough)
            .then(facts.expression(arm.value));
        arms_flow = arms_flow.union(arm_flow);
    }
    value.then(arms_flow)
}

fn if_expr_flow(
    arena: &AstArena,
    facts: &FlowFacts,
    branches: ArenaRange,
    else_value: ExprId,
) -> FlowSummary {
    let mut flow = FlowSummary::default();
    let mut next_condition_reachable = true;
    for branch in arena.if_expr_branches(branches) {
        if !next_condition_reachable {
            break;
        }
        let condition = facts.expression(branch.condition);
        flow = flow.union(FlowSummary {
            fallthrough: false,
            returns: condition.returns,
            breaks: condition.breaks,
            continues: condition.continues,
            terminates: condition.terminates,
        });
        if condition.fallthrough {
            flow = flow.union(facts.expression(branch.value));
        }
        next_condition_reachable = condition.fallthrough;
    }
    if next_condition_reachable {
        flow = flow.union(facts.expression(else_value));
    }
    flow
}

fn call_arg_flow(facts: &FlowFacts, arg: &ArenaCallArg) -> FlowSummary {
    match arg.kind {
        ArenaCallArgKind::Positional(expr)
        | ArenaCallArgKind::Named { value: expr, .. }
        | ArenaCallArgKind::Splice { value: expr, .. }
        | ArenaCallArgKind::NamedSpread { value: expr, .. } => facts.expression(expr),
    }
}

fn pipe_stage_flow(arena: &AstArena, facts: &FlowFacts, stage: &ArenaPipeStage) -> FlowSummary {
    match &stage.kind {
        ArenaPipeStageKind::Expr(expr) => facts.expression(*expr),
        ArenaPipeStageKind::Stream(stage) => stream_stage_flow(arena, facts, stage),
    }
}

fn stream_stage_flow(arena: &AstArena, facts: &FlowFacts, stage: &ArenaStreamStage) -> FlowSummary {
    let mut flow = FlowSummary::fallthrough();
    for arg in arena.call_args(stage.args) {
        flow = flow.then(call_arg_flow(facts, arg));
    }
    flow
}

fn assign_target_flow(arena: &AstArena, facts: &FlowFacts, target: AssignTargetId) -> FlowSummary {
    match arena.assign_target(target).kind {
        ArenaAssignTargetKind::Name(_) | ArenaAssignTargetKind::Env(_) => {
            FlowSummary::fallthrough()
        }
        ArenaAssignTargetKind::Field { base, .. } => facts.assign_target(base),
        ArenaAssignTargetKind::Index { base, index } => {
            facts.assign_target(base).then(facts.expression(index))
        }
    }
}

fn builder_block_setup_flow(
    arena: &AstArena,
    facts: &FlowFacts,
    block: BuilderBlockId,
) -> FlowSummary {
    let mut flow = FlowSummary::fallthrough();
    for entry in arena.builder_entries(arena.builder_block(block).entries) {
        match &entry.kind {
            ArenaBuilderEntryKind::Field { value, .. } => {
                flow = flow.then(facts.expression(*value));
            }
            ArenaBuilderEntryKind::Entry { args, block, .. } => {
                for arg in arena.command_args(*args) {
                    flow = flow.then(command_arg_flow(arena, facts, arg));
                }
                if let Some(block) = block {
                    flow = flow.then(facts.builder_block(*block));
                }
            }
            ArenaBuilderEntryKind::Task { .. } | ArenaBuilderEntryKind::Stmt(_) => {}
        }
    }
    flow
}

fn command_flow(arena: &AstArena, facts: &FlowFacts, command: CommandStmtId) -> FlowSummary {
    match arena.command_stmt(command).command.clone() {
        ArenaCommand::Proc { args, .. } => arena
            .command_args(args)
            .iter()
            .fold(FlowSummary::fallthrough(), |flow, arg| {
                flow.then(command_arg_flow(arena, facts, arg))
            }),
        ArenaCommand::Core {
            args, env, block, ..
        } => {
            let mut flow = FlowSummary::fallthrough();
            for arg in arena.command_args(args) {
                flow = flow.then(command_arg_flow(arena, facts, arg));
            }
            for assignment in arena.env_assignments(env) {
                let assignment_flow = match &assignment.value {
                    ArenaEnvAssignmentValue::CommandArg(arg) => command_arg_flow(arena, facts, arg),
                    ArenaEnvAssignmentValue::Expr(expr) => facts.expression(*expr),
                };
                flow = flow.then(assignment_flow);
            }
            if let Some(block) = block {
                flow.then(facts.block(block))
            } else {
                flow
            }
        }
        ArenaCommand::Run(run) => facts.run(run),
    }
}

fn run_flow(arena: &AstArena, facts: &FlowFacts, run: RunFormId) -> FlowSummary {
    let mut flow = FlowSummary::fallthrough();
    for segment in arena.run_segments(arena.run_form(run).segments) {
        for option in arena.run_options(segment.options) {
            flow = flow.then(facts.expression(option.value));
        }
        for assignment in arena.env_assignments(segment.env) {
            let assignment_flow = match &assignment.value {
                ArenaEnvAssignmentValue::CommandArg(arg) => command_arg_flow(arena, facts, arg),
                ArenaEnvAssignmentValue::Expr(expr) => facts.expression(*expr),
            };
            flow = flow.then(assignment_flow);
        }
        flow = flow.then(command_arg_flow(arena, facts, &segment.target));
        for operand in arena.run_tail_operands(segment.tail) {
            match operand {
                ArenaRunTailOperand::Argument(id) => {
                    flow = flow.then(command_arg_flow(arena, facts, arena.command_arg(*id)));
                }
                ArenaRunTailOperand::Redirection(id) => {
                    let target = match &arena.redirection(*id).target {
                        ArenaRedirectionTarget::Path(arg) | ArenaRedirectionTarget::Fd(arg) => arg,
                    };
                    flow = flow.then(command_arg_flow(arena, facts, target));
                }
            }
        }
    }
    flow
}

fn command_arg_flow(arena: &AstArena, facts: &FlowFacts, arg: &ArenaCommandArg) -> FlowSummary {
    match &arg.kind {
        ArenaCommandArgKind::Word(parts) => arena.word_parts(*parts).fold(
            FlowSummary::fallthrough(),
            |flow, part| match part {
                ArenaWordPart::Interpolation(expr) | ArenaWordPart::Shorthand(expr) => {
                    flow.then(facts.expression(expr))
                }
                ArenaWordPart::Bare(_) | ArenaWordPart::Quoted(_) => flow,
            },
        ),
        ArenaCommandArgKind::SpliceExpr(expr) | ArenaCommandArgKind::Typed(expr) => {
            facts.expression(*expr)
        }
        ArenaCommandArgKind::SpliceName(_) => FlowSummary::fallthrough(),
    }
}

#[cfg(test)]
mod tests {
    use super::FlowFacts;
    use xsh::frontend::source::SourceId;
    use xsh::frontend::syntax::arena::{ArenaExprOrRun, ArenaStmtKind};
    use xsh::frontend::syntax::parser::Parser;

    #[test]
    #[should_panic(expected = "flow expression was summarized more than once")]
    fn flow_expression_summaries_are_unique() {
        let source = "let value = 1\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let statement = parsed
            .arena
            .arena
            .stmt_ids(parsed.arena.statements)
            .next()
            .unwrap();
        let ArenaStmtKind::Let {
            initializer: ArenaExprOrRun::Expr(expression),
            ..
        } = parsed.arena.arena.stmt(statement).kind
        else {
            panic!("expected a let expression initializer");
        };
        let mut facts = FlowFacts::default();
        facts.record_expression(&parsed.arena.arena, expression);
        facts.record_expression(&parsed.arena.arena, expression);
    }
}
