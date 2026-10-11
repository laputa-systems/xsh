use super::{
    ArenaCommand, ArenaExprKind, ArenaStmtKind, AssignOp, BlockId, BuildExprId, BuildExprRow,
    BuildPatternRow, BuildStmtId, BuildStmtRow, CompactLowerConstructProbe, ExprId, FxHashMap,
    LoweredValue, Name, RuntimeOp, SlotScope, Span, StmtId, Type, cleanup_lowered_pattern_slots,
    collect_local, lowered_arena_run_capture_type,
};

impl<'p> CompactLowerConstructProbe<'p, '_> {
    pub(super) fn lower_tail_block(
        &mut self,
        block: BlockId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<Vec<BuildStmtId>> {
        let statements = self.program.arena.block(block).statements;
        let ids = self.program.arena.stmt_ids(statements).collect::<Vec<_>>();
        let Some((&tail, prefix)) = ids.split_last() else {
            return Some(Vec::new());
        };
        let tail = self.program.arena.core_stmt_id(tail);
        let mut lowered = Vec::with_capacity(ids.len());
        for stmt in prefix {
            lowered.push(self.lower_stmt_with_blocker_guard(
                *stmt,
                slots,
                current_function,
                item_slot,
            )?);
        }
        if self.bodies.statement_positions.get(&tail)
            == Some(&crate::sema::check::StatementPosition::Statement)
        {
            lowered.push(self.lower_stmt_with_blocker_guard(
                tail,
                slots,
                current_function,
                item_slot,
            )?);
            return Some(lowered);
        }
        let tail = match self.program.arena.stmt(tail).kind {
            ArenaStmtKind::Expr(expr) => push_build_row!(
                self,
                stmt,
                BuildStmtRow::Return {
                    value: self.lower_expr(expr, slots, current_function, item_slot)?,
                }
            ),
            ArenaStmtKind::Exit(status) => {
                let span = self.program.arena.stmt(tail).span;
                push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::Return {
                        value: self.lower_exit(status, span, slots, current_function, item_slot)?,
                    }
                )
            }
            ArenaStmtKind::TailBareIdent(name) => {
                let value = self
                    .lower_bare_ident_stmt(tail, name, slots)
                    .unwrap_or(push_build_row!(self, expr, BuildExprRow::Unit));
                push_build_row!(self, stmt, BuildStmtRow::Return { value })
            }
            ArenaStmtKind::If {
                branches,
                else_block,
            } => match self.lower_tail_if_stmt(
                branches,
                else_block,
                slots,
                current_function,
                item_slot,
            ) {
                Some(stmt) => stmt,
                None => {
                    return self
                        .lower_stmt_with_blocker_guard(tail, slots, current_function, item_slot)
                        .map(|stmt| {
                            lowered.push(stmt);
                            lowered
                        });
                }
            },
            ArenaStmtKind::Match { value, arms } => push_build_row!(
                self,
                stmt,
                BuildStmtRow::Return {
                    value: match self.lower_match_stmt_as_expr(
                        value,
                        arms,
                        self.program.arena.stmt(tail).span,
                        slots,
                        current_function,
                        item_slot,
                    ) {
                        Some(value) => value,
                        None => {
                            // Retain statement control flow when an arm cannot
                            // supply a lowered value expression.
                            if let Some(stmt) = self.lower_tail_match_stmt(
                                value,
                                arms,
                                self.program.arena.stmt(tail).span,
                                slots,
                                current_function,
                                item_slot,
                            ) {
                                lowered.push(stmt);
                                return Some(lowered);
                            }
                            return self
                                .lower_stmt_with_blocker_guard(
                                    tail,
                                    slots,
                                    current_function,
                                    item_slot,
                                )
                                .map(|stmt| {
                                    lowered.push(stmt);
                                    lowered
                                });
                        }
                    },
                }
            ),
            // A tail `run.text cmd` supplies the body's value.
            ArenaStmtKind::Command(command_id)
                if self.bodies.statement_positions.get(&tail)
                    == Some(&crate::sema::check::StatementPosition::Value) =>
            {
                if matches!(
                    self.program.arena.command_stmt(command_id).command,
                    ArenaCommand::Run(_)
                ) {
                    let value =
                        self.lower_tail_stmt_as_expr(tail, slots, current_function, item_slot)?;
                    push_build_row!(self, stmt, BuildStmtRow::Return { value })
                } else {
                    // Every other command's value is Unit. A return type that
                    // accepts the Unit fallthrough (checked before lowering)
                    // runs the command as the statement it is and completes
                    // with that Unit.
                    self.lower_stmt_with_blocker_guard(tail, slots, current_function, item_slot)?
                }
            }
            _ => self.lower_stmt_with_blocker_guard(tail, slots, current_function, item_slot)?,
        };
        lowered.push(tail);
        Some(lowered)
    }

    pub(super) fn lower_block(
        &mut self,
        block: BlockId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<Vec<BuildStmtId>> {
        if !self.program.arena.block(block).params.is_empty() {
            return Some(Vec::new());
        }
        let saved = slots.enter();
        let lowered = self.lower_block_in_current_scope(block, slots, current_function, item_slot);
        slots.exit(saved);
        lowered
    }

    /// `items`, a list, appended to the local of the `collect` expression
    /// that the checker gave this yield. That expression encloses the yield
    /// in the same function, so its local is in scope.
    pub(super) fn lower_collect_yield(
        &mut self,
        id: StmtId,
        items: BuildExprId,
        slots: &SlotScope,
    ) -> Option<BuildStmtId> {
        let collect = *self.bodies.collect_yields.get(&id)?;
        let slot = slots.resolve(collect_local(collect))?;
        Some(push_build_row!(
            self,
            stmt,
            BuildStmtRow::Assign {
                slot,
                op: AssignOp::Add,
                value: items,
                check: None,
                span: self.program.arena.stmt(id).span,
            }
        ))
    }

    /// A retry attempt has its own lexical scope. Its implicit tail uses the
    /// distinct value flow so explicit returns and loop transfers keep their
    /// enclosing targets, while propagation failures remain retryable.
    pub(super) fn lower_retry_block(
        &mut self,
        block: BlockId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<Vec<BuildStmtId>> {
        if !self.program.arena.block(block).params.is_empty() {
            return Some(Vec::new());
        }
        let saved = slots.enter();
        let result = (|| {
            let statements = self.program.arena.block(block).statements;
            let ids = self.program.arena.stmt_ids(statements).collect::<Vec<_>>();
            let Some((&tail, prefix)) = ids.split_last() else {
                return Some(Vec::new());
            };
            let mut lowered = Vec::with_capacity(ids.len());
            for stmt in prefix {
                lowered.push(self.lower_stmt_with_blocker_guard(
                    *stmt,
                    slots,
                    current_function,
                    item_slot,
                )?);
            }
            let tail_stmt = if self.bodies.statement_positions.get(&tail)
                == Some(&crate::sema::check::StatementPosition::Statement)
            {
                self.lower_stmt_with_blocker_guard(tail, slots, current_function, item_slot)?
            } else if let Some(value) =
                self.lower_tail_stmt_as_expr(tail, slots, current_function, item_slot)
            {
                push_build_row!(self, stmt, BuildStmtRow::Value { value })
            } else {
                self.lower_stmt_with_blocker_guard(tail, slots, current_function, item_slot)?
            };
            lowered.push(tail_stmt);
            Some(lowered)
        })();
        slots.exit(saved);
        result
    }

    pub(super) fn lower_block_in_current_scope(
        &mut self,
        block: BlockId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<Vec<BuildStmtId>> {
        let statements = self.program.arena.block(block).statements;
        let ids = self.program.arena.stmt_ids(statements).collect::<Vec<_>>();
        let mut lowered = Vec::with_capacity(ids.len());
        for stmt in ids {
            lowered.push(self.lower_stmt_with_blocker_guard(
                stmt,
                slots,
                current_function,
                item_slot,
            )?);
        }
        Some(lowered)
    }

    pub(super) fn lower_optional_expr(
        &mut self,
        id: Option<ExprId>,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<Option<BuildExprId>> {
        match id {
            Some(id) => Some(Some(self.lower_expr(
                id,
                slots,
                current_function,
                item_slot,
            )?)),
            None => Some(None),
        }
    }

    pub(super) fn lower_postfix_receiver(
        &mut self,
        base: ExprId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        if let Some(receiver) = slots.postfix_receivers.get(&base) {
            return Some(*receiver);
        }
        let receiver = self.lower_expr(base, slots, current_function, item_slot)?;
        Some(push_build_row!(
            self,
            expr,
            BuildExprRow::Try {
                value: receiver,
                span: self.program.arena.expr(base).span
            }
        ))
    }

    pub(super) fn lower_optional_postfix(
        &mut self,
        id: ExprId,
        base: ExprId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let span = self.program.arena.expr(id).span;
        let receiver = self.lower_expr(base, slots, current_function, item_slot)?;
        let slot = slots.reserve("optional receiver");
        let bound = push_build_row!(self, expr, BuildExprRow::Param(slot));
        let previous = slots.postfix_receivers.insert(base, bound);
        slots.guarded_postfixes.insert(id);
        let selected = self.lower_expr(id, slots, current_function, item_slot);
        slots.guarded_postfixes.remove(&id);
        match previous {
            Some(previous) => {
                slots.postfix_receivers.insert(base, previous);
            }
            None => {
                slots.postfix_receivers.remove(&base);
            }
        }
        let selected = selected?;
        let absent = push_build_row!(self, expr, BuildExprRow::Null);
        let null_pattern =
            push_build_row!(self, pattern, BuildPatternRow::Literal(LoweredValue::Null));
        let present_pattern = push_build_row!(self, pattern, BuildPatternRow::Bind { slot });
        Some(push_build_row!(
            self,
            expr,
            BuildExprRow::MatchExpr {
                value: receiver,
                arms: vec![
                    (null_pattern, None, absent),
                    (present_pattern, None, selected)
                ],
                span,
            }
        ))
    }

    /// Lower an `if` in tail (return) position into a `BuildStmtRow::If`/`IfBool`
    /// whose branch bodies (and else body) are lowered as tail-blocks — each
    /// branch's trailing expression becomes a `Return`. This handles a tail
    /// `if cond { a } else { b }` whose branches produce a value, which a plain
    /// statement-if would discard. Returns `None` if any branch cannot lower.
    fn lower_tail_if_stmt(
        &mut self,
        branches: crate::syntax::arena::ArenaRange,
        else_block: Option<BlockId>,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildStmtId> {
        let branches = self.program.arena.if_branches(branches).to_vec();
        let mut lowered = Vec::with_capacity(branches.len());
        for branch in &branches {
            // Sibling branches restore both ordinary locals and captures.
            let saved = slots.enter();
            let (condition, captures) = self.lower_pattern_condition_parts(
                branch.condition,
                slots,
                current_function,
                item_slot,
            )?;
            let body = self.lower_tail_block(branch.block, slots, current_function, item_slot);
            slots.exit(saved);
            lowered.push((condition, body?, captures));
        }
        let else_body = match else_block {
            Some(block) => {
                let saved = slots.enter();
                let body = self.lower_tail_block(block, slots, current_function, item_slot);
                slots.exit(saved);
                Some(body?)
            }
            None => None,
        };
        let has_pattern = branches.iter().any(|branch| {
            matches!(
                self.program.arena.expr(branch.condition).kind,
                ArenaExprKind::PatternCondition { .. }
            )
        });
        if has_pattern {
            return Some(push_build_row!(
                self,
                stmt,
                BuildStmtRow::PatternIf {
                    branches: lowered,
                    else_body,
                    span: self.program.arena.expr(branches[0].condition).span
                }
            ));
        }
        let lowered = lowered
            .into_iter()
            .map(|(condition, body, _)| (condition, body))
            .collect::<Vec<_>>();
        let mut bool_branches = Vec::with_capacity(lowered.len());
        for (condition, body) in &lowered {
            let Some(condition) = self.lower_bool_expr_candidate(condition) else {
                return Some(push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::If {
                        branches: lowered,
                        else_body,
                    }
                ));
            };
            bool_branches.push((condition, body.clone()));
        }
        Some(push_build_row!(
            self,
            stmt,
            BuildStmtRow::IfBool {
                branches: bool_branches,
                else_body,
            }
        ))
    }

    /// Lower a `match` in tail (return) position into a `BuildStmtRow::Match`
    /// whose arm bodies are lowered as tail-blocks — each arm's trailing
    /// expression becomes a `Return`. This handles arms whose body is a
    /// multi-statement block producing a value (e.g. `P => { let a = ..; a }`),
    /// which cannot be expressed as a `MatchExpr` (there is no block-expression
    /// form). Returns `None` if any arm cannot be lowered.
    fn lower_tail_match_stmt(
        &mut self,
        value: ExprId,
        arms: crate::syntax::arena::ArenaRange,
        span: Span,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildStmtId> {
        let (ok_binding_ty, err_binding_ty) = self.compact_match_scrutinee_result_types(value);
        let value = self.lower_expr(value, slots, current_function, item_slot)?;
        let arms = self.program.arena.match_arms(arms).to_vec();
        let mut lowered_arms = Vec::with_capacity(arms.len());
        for arm in arms {
            if !self.program.arena.block(arm.block).params.is_empty() {
                return None;
            }
            let (pattern, cleanup) = self.lower_pattern(
                arm.pattern,
                slots,
                ok_binding_ty.as_ref(),
                err_binding_ty.as_ref(),
            )?;
            // Each arm body gets its own scope so block-local bindings (e.g.
            // `var parts`) don't leak into sibling arms. (The regular match path
            // gets this from `lower_block`; the tail path uses `lower_tail_block`
            // which doesn't scope on its own.)
            let saved = slots.enter();
            let body = self.lower_tail_block(arm.block, slots, current_function, item_slot);
            slots.exit(saved);
            let body = match body {
                Some(body) => body,
                None => {
                    cleanup_lowered_pattern_slots(slots, cleanup);
                    return None;
                }
            };
            let guard = match arm.guard {
                Some(guard_expr) => {
                    match self.lower_expr(guard_expr, slots, current_function, item_slot) {
                        Some(guard) => Some(guard),
                        None => {
                            cleanup_lowered_pattern_slots(slots, cleanup);
                            return None;
                        }
                    }
                }
                None => None,
            };
            cleanup_lowered_pattern_slots(slots, cleanup);
            lowered_arms.push((pattern, guard, body));
        }
        Some(push_build_row!(
            self,
            stmt,
            BuildStmtRow::Match {
                value,
                arms: lowered_arms,
                span,
            }
        ))
    }

    fn lower_match_stmt_as_expr(
        &mut self,
        value: ExprId,
        arms: crate::syntax::arena::ArenaRange,
        span: Span,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        if let Some(expr) =
            self.lower_str_match_stmt_as_expr(value, arms, span, slots, current_function, item_slot)
        {
            return Some(expr);
        }
        if let Some(expr) =
            self.lower_tag_match_stmt_as_expr(value, arms, span, slots, current_function, item_slot)
        {
            return Some(expr);
        }
        let arms = self.program.arena.match_arms(arms).to_vec();
        let (ok_binding_ty, err_binding_ty) = self.compact_match_scrutinee_result_types(value);
        let mut lowered_arms = Vec::with_capacity(arms.len());
        for arm in arms {
            if !self.program.arena.block(arm.block).params.is_empty() {
                return None;
            }
            let (pattern, cleanup) = self.lower_pattern(
                arm.pattern,
                slots,
                ok_binding_ty.as_ref(),
                err_binding_ty.as_ref(),
            )?;
            let value =
                match self.lower_block_value_expr(arm.block, slots, current_function, item_slot) {
                    Some(value) => value,
                    None => {
                        cleanup_lowered_pattern_slots(slots, cleanup);
                        return None;
                    }
                };
            let guard = match arm.guard {
                Some(guard_expr) => {
                    Some(self.lower_expr(guard_expr, slots, current_function, item_slot)?)
                }
                None => None,
            };
            cleanup_lowered_pattern_slots(slots, cleanup);
            lowered_arms.push((pattern, guard, value));
        }
        Some(push_build_row!(
            self,
            expr,
            BuildExprRow::MatchExpr {
                value: self.lower_expr(value, slots, current_function, item_slot)?,
                arms: lowered_arms,
                span,
            }
        ))
    }

    fn lower_str_match_stmt_as_expr(
        &mut self,
        value: ExprId,
        arms: crate::syntax::arena::ArenaRange,
        span: Span,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let arms = self.program.arena.match_arms(arms).to_vec();
        let mut lowered_arms = FxHashMap::default();
        let mut fallback = None;
        for (index, arm) in arms.iter().enumerate() {
            if arm.guard.is_some() || !self.program.arena.block(arm.block).params.is_empty() {
                return None;
            }
            let statements = self.program.arena.block(arm.block).statements;
            let statements = self.program.arena.stmt_ids(statements).collect::<Vec<_>>();
            let [stmt] = statements.as_slice() else {
                return None;
            };
            match self.pattern_str_literals(arm.pattern)? {
                Some(patterns) => {
                    let value =
                        self.lower_arm_value_expr(*stmt, slots, current_function, item_slot)?;
                    for pattern in patterns {
                        lowered_arms.entry(pattern).or_insert_with(|| value);
                    }
                }
                None => {
                    if index + 1 != arms.len() {
                        return None;
                    }
                    fallback = Some(self.lower_arm_value_expr(
                        *stmt,
                        slots,
                        current_function,
                        item_slot,
                    )?);
                }
            }
        }
        Some(push_build_row!(
            self,
            expr,
            BuildExprRow::StrMatchExpr {
                value: self.lower_expr(value, slots, current_function, item_slot)?,
                arms: lowered_arms,
                fallback,
                span,
            }
        ))
    }

    pub(super) fn lower_str_match_stmt(
        &mut self,
        value: ExprId,
        arms: crate::syntax::arena::ArenaRange,
        span: Span,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildStmtId> {
        let arms = self.program.arena.match_arms(arms).to_vec();
        let mut lowered_arms = FxHashMap::default();
        let mut fallback = None;
        for (index, arm) in arms.iter().enumerate() {
            if arm.guard.is_some() || !self.program.arena.block(arm.block).params.is_empty() {
                return None;
            }
            match self.pattern_str_literals(arm.pattern)? {
                Some(patterns) => {
                    let body = self.lower_block(arm.block, slots, current_function, item_slot)?;
                    for pattern in patterns {
                        lowered_arms.entry(pattern).or_insert_with(|| body.clone());
                    }
                }
                None => {
                    if index + 1 != arms.len() {
                        return None;
                    }
                    fallback =
                        Some(self.lower_block(arm.block, slots, current_function, item_slot)?);
                }
            }
        }
        Some(push_build_row!(
            self,
            stmt,
            BuildStmtRow::StrMatch {
                value: self.lower_expr(value, slots, current_function, item_slot)?,
                arms: lowered_arms,
                fallback,
                span,
            }
        ))
    }

    pub(super) fn lower_str_match_expr(
        &mut self,
        value: ExprId,
        arms: crate::syntax::arena::ArenaRange,
        span: Span,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let arms = self.program.arena.match_expr_arms(arms).to_vec();
        let mut lowered_arms = FxHashMap::default();
        let mut fallback = None;
        for (index, arm) in arms.iter().enumerate() {
            if arm.guard.is_some() {
                return None;
            }
            match self.pattern_str_literals(arm.pattern)? {
                Some(patterns) => {
                    let value = self.lower_expr(arm.value, slots, current_function, item_slot)?;
                    for pattern in patterns {
                        lowered_arms.entry(pattern).or_insert_with(|| value);
                    }
                }
                None => {
                    if index + 1 != arms.len() {
                        return None;
                    }
                    fallback =
                        Some(self.lower_expr(arm.value, slots, current_function, item_slot)?);
                }
            }
        }
        Some(push_build_row!(
            self,
            expr,
            BuildExprRow::StrMatchExpr {
                value: self.lower_expr(value, slots, current_function, item_slot)?,
                arms: lowered_arms,
                fallback,
                span,
            }
        ))
    }

    fn lower_tag_match_stmt_as_expr(
        &mut self,
        value: ExprId,
        arms: crate::syntax::arena::ArenaRange,
        span: Span,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let Type::Tag(type_name) = self.checked_expr_type(value)? else {
            return None;
        };
        if self
            .declarations
            .wire_enums
            .mappings
            .contains_key(&type_name)
        {
            return None;
        }
        let arms = self.program.arena.match_arms(arms).to_vec();
        let mut lowered_arms = FxHashMap::default();
        let mut fallback = None;
        for (index, arm) in arms.iter().enumerate() {
            if arm.guard.is_some() || !self.program.arena.block(arm.block).params.is_empty() {
                return None;
            }
            let statements = self.program.arena.block(arm.block).statements;
            let statements = self.program.arena.stmt_ids(statements).collect::<Vec<_>>();
            let [stmt] = statements.as_slice() else {
                return None;
            };
            match self.pattern_tag_name(arm.pattern)? {
                Some(pattern) => {
                    let value =
                        self.lower_arm_value_expr(*stmt, slots, current_function, item_slot)?;
                    lowered_arms.entry(pattern).or_insert(value);
                }
                None => {
                    if index + 1 != arms.len() {
                        return None;
                    }
                    fallback = Some(self.lower_arm_value_expr(
                        *stmt,
                        slots,
                        current_function,
                        item_slot,
                    )?);
                }
            }
        }
        Some(push_build_row!(
            self,
            expr,
            BuildExprRow::TagMatchExpr {
                value: self.lower_expr(value, slots, current_function, item_slot)?,
                arms: lowered_arms,
                fallback,
                span,
            }
        ))
    }

    pub(super) fn lower_tag_match_stmt(
        &mut self,
        value: ExprId,
        arms: crate::syntax::arena::ArenaRange,
        span: Span,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildStmtId> {
        let Type::Tag(type_name) = self.checked_expr_type(value)? else {
            return None;
        };
        if self
            .declarations
            .wire_enums
            .mappings
            .contains_key(&type_name)
        {
            return None;
        }
        let arms = self.program.arena.match_arms(arms).to_vec();
        let mut lowered_arms = FxHashMap::default();
        let mut fallback = None;
        for (index, arm) in arms.iter().enumerate() {
            if arm.guard.is_some() || !self.program.arena.block(arm.block).params.is_empty() {
                return None;
            }
            match self.pattern_tag_name(arm.pattern)? {
                Some(pattern) => {
                    let body = self.lower_block(arm.block, slots, current_function, item_slot)?;
                    lowered_arms.entry(pattern).or_insert(body);
                }
                None => {
                    if index + 1 != arms.len() {
                        return None;
                    }
                    fallback =
                        Some(self.lower_block(arm.block, slots, current_function, item_slot)?);
                }
            }
        }
        Some(push_build_row!(
            self,
            stmt,
            BuildStmtRow::TagMatch {
                value: self.lower_expr(value, slots, current_function, item_slot)?,
                arms: lowered_arms,
                fallback,
                span,
            }
        ))
    }

    pub(super) fn lower_tag_match_expr(
        &mut self,
        value: ExprId,
        arms: crate::syntax::arena::ArenaRange,
        span: Span,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let Type::Tag(type_name) = self.checked_expr_type(value)? else {
            return None;
        };
        if self
            .declarations
            .wire_enums
            .mappings
            .contains_key(&type_name)
        {
            return None;
        }
        let arms = self.program.arena.match_expr_arms(arms).to_vec();
        let mut lowered_arms = FxHashMap::default();
        let mut fallback = None;
        for (index, arm) in arms.iter().enumerate() {
            if arm.guard.is_some() {
                return None;
            }
            match self.pattern_tag_name(arm.pattern)? {
                Some(pattern) => {
                    let value = self.lower_expr(arm.value, slots, current_function, item_slot)?;
                    lowered_arms.entry(pattern).or_insert(value);
                }
                None => {
                    if index + 1 != arms.len() {
                        return None;
                    }
                    fallback =
                        Some(self.lower_expr(arm.value, slots, current_function, item_slot)?);
                }
            }
        }
        Some(push_build_row!(
            self,
            expr,
            BuildExprRow::TagMatchExpr {
                value: self.lower_expr(value, slots, current_function, item_slot)?,
                arms: lowered_arms,
                fallback,
                span,
            }
        ))
    }

    fn lower_arm_value_expr(
        &mut self,
        stmt: StmtId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let stmt = self.program.arena.core_stmt_id(stmt);
        match self.program.arena.stmt(stmt).kind {
            ArenaStmtKind::Expr(expr) => self.lower_expr(expr, slots, current_function, item_slot),
            ArenaStmtKind::Exit(status) => {
                let span = self.program.arena.stmt(stmt).span;
                self.lower_exit(status, span, slots, current_function, item_slot)
            }
            ArenaStmtKind::TailBareIdent(name) => self.lower_bare_ident_stmt(stmt, name, slots),
            _ => None,
        }
    }

    /// Lowers `exit STATUS` to the expression that ends the script: it
    /// evaluates the status and never yields a value, so it stands wherever
    /// the statement does, as a statement or as the tail of a block.
    pub(super) fn lower_exit(
        &mut self,
        status: ExprId,
        span: Span,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        Some(push_build_row!(
            self,
            expr,
            BuildExprRow::Abort {
                status: self.lower_expr(status, slots, current_function, item_slot)?,
                span,
            }
        ))
    }

    /// The place a `?` reports: itself with its operand, or the operand alone
    /// when it is the `?` of a deferred call.
    pub(super) fn propagation_span(&self, propagation: ExprId, operand: ExprId, slots: &SlotScope) -> Span {
        if slots.deferred_propagation == Some(propagation) {
            self.program.arena.expr(operand).span
        } else {
            self.program.arena.expr(propagation).span
        }
    }

    /// Cleanup bodies use statement position even for their final expression.
    pub(super) fn lower_deferred_expr(
        &mut self,
        value: ExprId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        if let ArenaExprKind::ValueBlock(block) = self.program.arena.expr(value).kind {
            let body = self.lower_block(block, slots, current_function, item_slot)?;
            let span = self.program.arena.expr(value).span;
            Some(push_build_row!(
                self,
                expr,
                BuildExprRow::ValueBlock { body, span }
            ))
        } else {
            let outer = slots.deferred_propagation.replace(value);
            let lowered = self.lower_expr(value, slots, current_function, item_slot);
            slots.deferred_propagation = outer;
            let lowered = lowered?;
            // A deferred `Result[Unit]` fails its action with or without a
            // `?`. Lowering the bare form as the propagation it is gives its
            // failure the action's own place; the evaluator's handling of a
            // deferred `Err` value stays as the fallback.
            let checked = self
                .checked_expr_type(value)
                .or_else(|| self.bodies.expr_types.get(&value).cloned());
            if matches!(checked, Some(Type::Result(..)))
                && !matches!(self.program.arena.expr(value).kind, ArenaExprKind::Try(_))
            {
                let span = self.program.arena.expr(value).span;
                return Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::Try {
                        value: lowered,
                        span
                    }
                ));
            }
            Some(lowered)
        }
    }

    /// `tempdir NAME { body }` is the scope that `let root = fs.tempdir()?`,
    /// `defer root.close()?`, and `let NAME = root.host_path()?` open, so the
    /// directory is removed after the body's own cleanup on every exit, by the
    /// same defer machinery. Only a failure to create the directory becomes the
    /// scope's `Err`; the body's tail (or `Unit`) is its `Ok`.
    ///
    /// `tempdir NAME at PATH { body }` is the same scope over a directory the
    /// program names: `PATH` is evaluated once, whatever is there is removed,
    /// the directory is created, and its removal is deferred around the body.
    /// A failure to clear or create it is the scope's `Err`.
    pub(super) fn lower_tempdir_scope(
        &mut self,
        path: Option<ExprId>,
        block: BlockId,
        span: Span,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let Some(path) = path else {
            return self.lower_fresh_tempdir_scope(block, span, slots, current_function, item_slot);
        };
        // The path is evaluated where the scope is written, before the
        // directory name exists.
        let path = self.lower_expr(path, slots, current_function, item_slot)?;
        let saved = slots.enter();
        let result = (|| {
            let at_slot = slots.reserve("tempdir.at");
            let at =
                |lowerer: &mut Self| push_build_row!(lowerer, expr, BuildExprRow::Param(at_slot));
            let remove = |lowerer: &mut Self| {
                let path = at(lowerer);
                let missing_ok = push_build_row!(lowerer, expr, BuildExprRow::Bool(true));
                push_build_row!(
                    lowerer,
                    expr,
                    BuildExprRow::FsRemove {
                        path,
                        missing_ok: Some(missing_ok),
                        span,
                    }
                )
            };
            let failed_slot = slots.reserve("tempdir.failed");
            // An arm that hands a failed step's `Err` on as the scope's value.
            let failed_arm = |lowerer: &mut Self| {
                let failed = push_build_row!(
                    lowerer,
                    pattern,
                    BuildPatternRow::Bind { slot: failed_slot }
                );
                let failure = push_build_row!(lowerer, expr, BuildExprRow::Param(failed_slot));
                (failed, None, failure)
            };
            let done = |lowerer: &mut Self| {
                push_build_row!(
                    lowerer,
                    pattern,
                    BuildPatternRow::ResultOk {
                        slot: None,
                        unit_only: false
                    }
                )
            };

            let scope = {
                let inner = slots.enter();
                let scope = (|| {
                    let removal = remove(self);
                    let removal = push_build_row!(
                        self,
                        expr,
                        BuildExprRow::Try {
                            value: removal,
                            span
                        }
                    );
                    let mut body = vec![push_build_row!(
                        self,
                        stmt,
                        BuildStmtRow::Defer {
                            value: removal,
                            on_error: false,
                        }
                    )];
                    let value = at(self);
                    let path_slot = match self
                        .program
                        .arena
                        .block_params(self.program.arena.block(block).params)
                    {
                        [param] if param.name.as_str() != "_" => {
                            slots.declare_with_type(param.name, Some(Type::Path))
                        }
                        _ => slots.reserve("tempdir.path"),
                    };
                    body.push(push_build_row!(
                        self,
                        stmt,
                        BuildStmtRow::Let {
                            slot: path_slot,
                            value
                        }
                    ));
                    self.lower_tempdir_body(block, body, span, slots, current_function, item_slot)
                })();
                slots.exit(inner);
                scope?
            };

            let created = {
                let path = at(self);
                push_build_row!(
                    self,
                    expr,
                    BuildExprRow::FsMkdir {
                        path,
                        parents: None,
                        span,
                    }
                )
            };
            let entered = {
                let arms = vec![(done(self), None, scope), failed_arm(self)];
                push_build_row!(
                    self,
                    expr,
                    BuildExprRow::MatchExpr {
                        value: created,
                        arms,
                        span,
                    }
                )
            };
            let cleared = remove(self);
            let outcome = {
                let arms = vec![(done(self), None, entered), failed_arm(self)];
                push_build_row!(
                    self,
                    expr,
                    BuildExprRow::MatchExpr {
                        value: cleared,
                        arms,
                        span,
                    }
                )
            };
            let body = vec![
                push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::Let {
                        slot: at_slot,
                        value: path
                    }
                ),
                push_build_row!(self, stmt, BuildStmtRow::Value { value: outcome }),
            ];
            Some(push_build_row!(
                self,
                expr,
                BuildExprRow::ValueBlock { body, span }
            ))
        })();
        slots.exit(saved);
        result
    }

    /// A managed `with` scope, built from rows that exist: each value is
    /// bound and its release deferred, so every way out of the body releases
    /// what was opened, in reverse. A body that reaches its end is followed
    /// by the same releases under a capture, whose `Err` is the scope's: the
    /// first release that fails, with any later one reported as a cleanup
    /// failure. That capture sets a flag once its own releases are
    /// registered, and the outer ones do nothing from then on, so a release
    /// that failed is not attempted a second time.
    pub(super) fn lower_resource_scope(
        &mut self,
        scope: ExprId,
        bindings: crate::syntax::arena::ArenaRange,
        block: BlockId,
        span: Span,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let bindings = self.program.arena.with_bindings(bindings).to_vec();
        // The checker publishes one kind per binding, or nothing for a scope
        // it rejected.
        let kinds = self
            .bodies
            .resource_scopes
            .get(&scope)
            .filter(|kinds| kinds.len() == bindings.len())?
            .clone();
        let saved = slots.enter();
        let result = (|| {
            let closing_slot = slots.reserve("with.closing");
            let release = |lowerer: &mut Self,
                           slot: usize,
                           kind: crate::modules::ManagedResource,
                           unless_closing: bool| {
                let value = push_build_row!(lowerer, expr, BuildExprRow::Param(slot));
                let call = push_build_row!(
                    lowerer,
                    expr,
                    BuildExprRow::ModuleCall {
                        cli_plan: None,
                        op: kind.release_op().expect("checker accepts only resources with a with release"),
                        args: vec![Some(value)],
                        span,
                    }
                );
                let mut release =
                    push_build_row!(lowerer, expr, BuildExprRow::Try { value: call, span });
                if unless_closing {
                    let closing = push_build_row!(lowerer, expr, BuildExprRow::Param(closing_slot));
                    let nothing = push_build_row!(lowerer, expr, BuildExprRow::Unit);
                    release = push_build_row!(
                        lowerer,
                        expr,
                        BuildExprRow::IfExpr {
                            branches: vec![(closing, nothing)],
                            else_value: release,
                            span,
                        }
                    );
                }
                push_build_row!(
                    lowerer,
                    stmt,
                    BuildStmtRow::Defer {
                        value: release,
                        on_error: false,
                    }
                )
            };
            let mut body = Vec::new();
            let open = push_build_row!(self, expr, BuildExprRow::Bool(false));
            body.push(push_build_row!(
                self,
                stmt,
                BuildStmtRow::Let {
                    slot: closing_slot,
                    value: open
                }
            ));
            let mut held = Vec::with_capacity(bindings.len());
            for (binding, kind) in bindings.iter().zip(kinds) {
                let ty = self.lower_binding_checked_type(None, binding.initializer);
                let value =
                    self.lower_expr(binding.initializer, slots, current_function, item_slot)?;
                let slot = if binding.name.as_str() == "_" {
                    slots.reserve("with.resource")
                } else {
                    slots.declare_with_type(binding.name, ty)
                };
                body.push(push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::Let { slot, value }
                ));
                body.push(release(self, slot, kind, true));
                held.push((slot, kind));
            }
            // The body is a block of its own: its defers run and its handles
            // are cleaned up before anything is released.
            let finished = self.lower_tempdir_body(
                block,
                Vec::new(),
                span,
                slots,
                current_function,
                item_slot,
            )?;
            let finished_slot = slots.reserve("with.finished");
            body.push(push_build_row!(
                self,
                stmt,
                BuildStmtRow::Let {
                    slot: finished_slot,
                    value: finished
                }
            ));
            let mut closing = Vec::with_capacity(held.len() + 2);
            for (slot, kind) in held {
                closing.push(release(self, slot, kind, false));
            }
            let handed_over = push_build_row!(self, expr, BuildExprRow::Bool(true));
            closing.push(push_build_row!(
                self,
                stmt,
                BuildStmtRow::Assign {
                    slot: closing_slot,
                    op: AssignOp::Set,
                    value: handed_over,
                    check: None,
                    span,
                }
            ));
            let finished = push_build_row!(self, expr, BuildExprRow::Param(finished_slot));
            let value = push_build_row!(
                self,
                expr,
                BuildExprRow::Try {
                    value: finished,
                    span
                }
            );
            closing.push(push_build_row!(self, stmt, BuildStmtRow::Value { value }));
            let outcome = push_build_row!(
                self,
                expr,
                BuildExprRow::Capture {
                    body: closing,
                    span
                }
            );
            body.push(push_build_row!(
                self,
                stmt,
                BuildStmtRow::Value { value: outcome }
            ));
            Some(push_build_row!(
                self,
                expr,
                BuildExprRow::ValueBlock { body, span }
            ))
        })();
        slots.exit(saved);
        result
    }

    /// The statements of a `tempdir` body after `prefix`, which holds the
    /// deferred removal and the binding of the directory name, as the value
    /// block whose value is `Ok` of the body's tail.
    fn lower_tempdir_body(
        &mut self,
        block: BlockId,
        prefix: Vec<BuildStmtId>,
        span: Span,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let mut body = prefix;
        let statements = self.program.arena.block(block).statements;
        let statements = self.program.arena.stmt_ids(statements).collect::<Vec<_>>();
        let mut tail = None;
        if let Some((&last, prefix)) = statements.split_last() {
            for &stmt in prefix {
                body.push(self.lower_stmt_with_blocker_guard(
                    stmt,
                    slots,
                    current_function,
                    item_slot,
                )?);
            }
            if self.bodies.statement_positions.get(&last)
                != Some(&crate::sema::check::StatementPosition::Statement)
                && let Some(value) =
                    self.lower_tail_stmt_as_expr(last, slots, current_function, item_slot)
            {
                tail = Some(value);
            } else {
                body.push(self.lower_stmt_with_blocker_guard(
                    last,
                    slots,
                    current_function,
                    item_slot,
                )?);
            }
        }
        let tail = tail.unwrap_or_else(|| push_build_row!(self, expr, BuildExprRow::Unit));
        let value = push_build_row!(self, expr, BuildExprRow::Ok(tail));
        body.push(push_build_row!(self, stmt, BuildStmtRow::Value { value }));
        Some(push_build_row!(
            self,
            expr,
            BuildExprRow::ValueBlock { body, span }
        ))
    }

    fn lower_fresh_tempdir_scope(
        &mut self,
        block: BlockId,
        span: Span,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let created = push_build_row!(self, expr, BuildExprRow::FsTempDir { span });
        let failed_slot = slots.reserve("tempdir.failed");
        let failed = push_build_row!(self, pattern, BuildPatternRow::Bind { slot: failed_slot });
        let failure = push_build_row!(self, expr, BuildExprRow::Param(failed_slot));
        let saved = slots.enter();
        let result = (|| {
            let root_slot = slots.reserve("tempdir.root");
            let opened = push_build_row!(
                self,
                pattern,
                BuildPatternRow::ResultOk {
                    slot: Some(root_slot),
                    unit_only: false
                }
            );
            let root_method = |lowerer: &mut Self, op: RuntimeOp| {
                let receiver = push_build_row!(lowerer, expr, BuildExprRow::Param(root_slot));
                let call = push_build_row!(
                    lowerer,
                    expr,
                    BuildExprRow::ModuleCall {
                        cli_plan: None,
                        op,
                        args: vec![Some(receiver)],
                        span,
                    }
                );
                push_build_row!(lowerer, expr, BuildExprRow::Try { value: call, span })
            };
            let mut body = Vec::new();
            let close = root_method(self, RuntimeOp::FsCloseRoot);
            body.push(push_build_row!(
                self,
                stmt,
                BuildStmtRow::Defer {
                    value: close,
                    on_error: false,
                }
            ));
            let path = root_method(self, RuntimeOp::FsRootPath);
            let path_slot = match self
                .program
                .arena
                .block_params(self.program.arena.block(block).params)
            {
                [param] if param.name.as_str() != "_" => {
                    slots.declare_with_type(param.name, Some(Type::Path))
                }
                _ => slots.reserve("tempdir.path"),
            };
            body.push(push_build_row!(
                self,
                stmt,
                BuildStmtRow::Let {
                    slot: path_slot,
                    value: path
                }
            ));
            let scope =
                self.lower_tempdir_body(block, body, span, slots, current_function, item_slot)?;
            Some((opened, scope))
        })();
        slots.exit(saved);
        let (opened, scope) = result?;
        Some(push_build_row!(
            self,
            expr,
            BuildExprRow::MatchExpr {
                value: created,
                arms: vec![(opened, None, scope), (failed, None, failure)],
                span,
            }
        ))
    }

    /// Keep branch statements and their tail in one lexical scope so the
    /// selected value is evaluated before that scope runs cleanup.
    pub(super) fn lower_block_value_expr(
        &mut self,
        block: BlockId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let statements = self.program.arena.block(block).statements;
        let statements = self.program.arena.stmt_ids(statements).collect::<Vec<_>>();
        let saved = slots.enter();
        let result = (|| {
            let mut body = Vec::with_capacity(statements.len());
            if let Some((&tail, prefix)) = statements.split_last() {
                for &stmt in prefix {
                    body.push(self.lower_stmt_with_blocker_guard(
                        stmt,
                        slots,
                        current_function,
                        item_slot,
                    )?);
                }
                if self.bodies.statement_positions.get(&tail)
                    != Some(&crate::sema::check::StatementPosition::Statement)
                    && let Some(value) =
                        self.lower_tail_stmt_as_expr(tail, slots, current_function, item_slot)
                {
                    body.push(push_build_row!(self, stmt, BuildStmtRow::Value { value }));
                } else {
                    body.push(self.lower_stmt_with_blocker_guard(
                        tail,
                        slots,
                        current_function,
                        item_slot,
                    )?);
                }
            }
            let span = self
                .program
                .arena
                .span(self.program.arena.block(block).span);
            Some(push_build_row!(
                self,
                expr,
                BuildExprRow::ValueBlock { body, span }
            ))
        })();
        slots.exit(saved);
        result
    }

    /// Lower a single value-producing tail statement to an expression: a bare
    /// expression, a captured run, a tail-bare-ident, or a value-producing `if`/`match` whose
    /// branch blocks retain ordinary lexical statements and a checked tail.
    pub(super) fn lower_tail_stmt_as_expr(
        &mut self,
        stmt: StmtId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let stmt = self.program.arena.core_stmt_id(stmt);
        let span = self.program.arena.stmt(stmt).span;
        match self.program.arena.stmt(stmt).kind {
            ArenaStmtKind::Expr(expr) => self.lower_expr(expr, slots, current_function, item_slot),
            ArenaStmtKind::Exit(status) => {
                self.lower_exit(status, span, slots, current_function, item_slot)
            }
            ArenaStmtKind::TailBareIdent(name) => self.lower_bare_ident_stmt(stmt, name, slots),
            ArenaStmtKind::Command(command)
                if self.bodies.statement_positions.get(&stmt)
                    == Some(&crate::sema::check::StatementPosition::Value) =>
            {
                let command = self.program.arena.command_stmt(command);
                let ArenaCommand::Run(run) = command.command else {
                    return None;
                };
                lowered_arena_run_capture_type(&self.program.arena, run)?;
                self.lower_run_binding_value(run, slots, current_function, item_slot)
            }
            ArenaStmtKind::If {
                branches,
                else_block,
            } => {
                // A value-producing `if` needs an `else`.
                let else_block = else_block?;
                let arena_branches = self.program.arena.if_branches(branches).to_vec();
                let has_pattern = arena_branches.iter().any(|branch| {
                    matches!(
                        self.program.arena.expr(branch.condition).kind,
                        ArenaExprKind::PatternCondition { .. }
                    )
                });
                let mut lowered = Vec::with_capacity(arena_branches.len());
                for branch in arena_branches {
                    let saved = slots.enter();
                    let (condition, captures) = self.lower_pattern_condition_parts(
                        branch.condition,
                        slots,
                        current_function,
                        item_slot,
                    )?;
                    let value = self.lower_block_value_expr(
                        branch.block,
                        slots,
                        current_function,
                        item_slot,
                    )?;
                    slots.exit(saved);
                    lowered.push((condition, value, captures));
                }
                let else_value =
                    self.lower_block_value_expr(else_block, slots, current_function, item_slot)?;
                if has_pattern {
                    Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PatternIf {
                            branches: lowered,
                            else_value,
                            span
                        }
                    ))
                } else {
                    let branches = lowered
                        .into_iter()
                        .map(|(condition, value, _)| (condition, value))
                        .collect();
                    Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::IfExpr {
                            branches,
                            else_value,
                            span
                        }
                    ))
                }
            }
            ArenaStmtKind::Match { value, arms } => {
                self.lower_match_stmt_as_expr(value, arms, span, slots, current_function, item_slot)
            }
            _ => None,
        }
    }
}
