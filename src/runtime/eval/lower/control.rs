use super::{
    ArenaCommand, ArenaExprKind, ArenaStmtKind, AssignOp, BlockId, BuildExprId, BuildExprRow,
    BuildPatternRow, BuildStmtId, BuildStmtRow, CompactLowerConstructProbe, ExprId, FxHashMap,
    LoweredValue, Name, RuntimeOp, SlotScope, Span, StmtId, Type, cleanup_lowered_pattern_slots,
     lowered_arena_run_capture_type,
};

pub(super) enum TempDirLoweringDomain<'a> {
    Ordinary,
    Generalized {
        facts: &'a crate::sema::schemes::SchemeBodyFacts,
        visited: &'a mut super::FxHashSet<ExprId>,
        types: &'a mut super::BTreeMap<BuildExprId, super::SchemeType>,
    },
}

pub(super) enum CollectYieldKind { Item, List }

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum TailBlockPurpose { CheckedPosition, ProcCompletion }

impl TailBlockPurpose {
    pub(super) fn completes_control(self, arena: &crate::syntax::arena::AstArena, tail: StmtId) -> bool {
        self == Self::ProcCompletion && match arena.stmt(tail).kind {
            ArenaStmtKind::If { .. } | ArenaStmtKind::Match { .. } => true,
            ArenaStmtKind::Expr(expression) => matches!(arena.expr(expression).kind, ArenaExprKind::If { .. } | ArenaExprKind::Match { .. }),
            _ => false,
        }
    }
}

#[derive(Clone)]
pub(super) enum CollectSlot {
    Ordinary(usize),
    Generalized { slot: usize, ty: super::SchemeType },
}

impl CollectSlot {
    fn slot(&self) -> usize {
        match self { Self::Ordinary(slot) | Self::Generalized { slot, .. } => *slot }
    }
}

pub(super) enum CollectLoweringDomain<'a> {
    Ordinary,
    Generalized {
        facts: &'a crate::sema::schemes::SchemeBodyFacts,
        visited: &'a mut super::FxHashSet<ExprId>,
        types: &'a mut super::BTreeMap<BuildExprId, super::SchemeType>,
    },
}

impl<'p> CompactLowerConstructProbe<'p, '_> {
    pub(super) fn lower_tail_block_for_purpose(
        &mut self,
        block: BlockId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
        purpose: TailBlockPurpose,
    ) -> Option<Vec<BuildStmtId>> {
        let transfers_resource = self
            .bodies
            .resource_transfer(crate::sema::check::ResourceTransferSite::BlockValue(block))?;
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
        let completion_control = purpose.completes_control(&self.program.arena, tail);
        if !completion_control && self.bodies.statement_positions.get(&tail)
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
            ArenaStmtKind::Expr(expr) => {
                push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::Return {
                        value: self.lower_expr(expr, slots, current_function, item_slot)?,
                    }
                )
            }
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
                let value = self.lower_bare_ident_stmt(tail, name, slots)?;
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
                purpose,
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
            ArenaStmtKind::Match { value, arms } if purpose == TailBlockPurpose::ProcCompletion => {
                self.lower_tail_match_stmt(value, arms, self.program.arena.stmt(tail).span, slots, current_function, item_slot, purpose)?
            }
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
                                purpose,
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
                    lowered.push(self.lower_stmt_with_blocker_guard(
                        tail,
                        slots,
                        current_function,
                        item_slot,
                    )?);
                    push_build_row!(
                        self,
                        stmt,
                        BuildStmtRow::Return {
                            value: push_build_row!(self, expr, BuildExprRow::Unit),
                        }
                    )
                }
            }
            _ => self.lower_stmt_with_blocker_guard(tail, slots, current_function, item_slot)?,
        };
        if transfers_resource {
            self.scratch.borrow_mut().mark_resource_transfer(tail);
        }
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

    pub(super) fn lower_collect_in_domain(
        &mut self, expression: ExprId, block: BlockId, span: Span, slots: &mut SlotScope,
        current_function: Option<Name>, item_slot: Option<usize>, domain: &mut CollectLoweringDomain<'_>,
    ) -> Option<BuildExprId> {
        let expected = match domain {
            CollectLoweringDomain::Ordinary => None,
            CollectLoweringDomain::Generalized { facts, .. } => Some(Self::collect_list_type(facts, expression)?),
        };
        let checked_type = expected.as_ref().and_then(super::SchemeType::closed_ground_type).filter(Type::has_unsigned_constraint);
        let mut raw_item = None;
        let mut transfers_resource = false;
        for (&statement, &collect) in &self.bodies.collect_yields {
            if collect != expression { continue; }
            let Some(transfer) = self.bodies.resource_transfer(crate::sema::check::ResourceTransferSite::Yield(statement)) else {
                self.last_blocker_detail = Some((self.program.arena.stmt(statement).span, "missing checked resource-transfer fact for collected yield".into()));
                return None;
            };
            transfers_resource |= transfer;
            if checked_type.is_some() && let CollectLoweringDomain::Generalized { facts, .. } = domain {
                let (value, delegated) = match self.program.arena.stmt(statement).kind {
                    ArenaStmtKind::Yield(super::ArenaExprOrRun::Expr(value)) => (value, false),
                    ArenaStmtKind::YieldDelegate(value) => (value, true),
                    _ => return None,
                };
                let actual = facts.expression_types().get(&value)?.closed_ground_type()?;
                let actual = if delegated { match actual { Type::List(item) => *item, _ => return None } } else { actual };
                raw_item = Some(match raw_item {
                    None => actual,
                    Some(ref item) => crate::sema::types::merge_list_literal_item_ty(item, &actual)?,
                });
            }
        }
        let storage_type = match raw_item {
            Some(item) => Some(super::SchemeType::List(Box::new(super::SchemeType::Ground(item)))),
            None => expected.clone(),
        };
        let saved = slots.enter();
        let slot = slots.reserve("collect list");
        let storage = match &storage_type {
            Some(ty) => CollectSlot::Generalized { slot, ty: ty.clone() },
            None => CollectSlot::Ordinary(slot),
        };
        let previous = slots.collect_slots.insert(expression, storage);
        let lowered = (|| {
            let empty = push_build_row!(self, expr, BuildExprRow::List(Vec::new()));
            let declaration = push_build_row!(self, stmt, BuildStmtRow::Let { slot, value: empty, annotation: None });
            let statements = match domain {
                CollectLoweringDomain::Ordinary => self.lower_block(block, slots, current_function, item_slot)?,
                CollectLoweringDomain::Generalized { facts, visited, types } => {
                    let ty = storage_type.clone()?;
                    types.insert(empty, ty.clone());
                    slots.scheme_declarations.insert(declaration, super::super::SchemeLocalDeclaration { ty, mutability: super::super::SchemeLocalMutability::Mutable });
                    let mut statements = self.lower_scheme_statement_block(block, slots, current_function?, facts, visited, types)?;
                    let unit = push_build_row!(self, expr, BuildExprRow::Unit);
                    types.insert(unit, super::SchemeType::Ground(Type::Unit));
                    statements.push(push_build_row!(self, stmt, BuildStmtRow::Value { value: unit }));
                    statements
                }
            };
            let inner = push_build_row!(self, expr, BuildExprRow::ValueBlock { body: statements, span });
            let run = push_build_row!(self, stmt, BuildStmtRow::Expr { value: inner, span });
            let list = push_build_row!(self, expr, BuildExprRow::Param(slot));
            if let CollectLoweringDomain::Generalized { types, .. } = domain {
                types.insert(inner, super::SchemeType::Ground(Type::Unit));
                types.insert(list, storage_type.clone()?);
            }
            let value = push_build_row!(self, stmt, BuildStmtRow::Value { value: list });
            if transfers_resource { self.scratch.borrow_mut().mark_resource_transfer(value); }
            let result = push_build_row!(self, expr, BuildExprRow::ValueBlock { body: vec![declaration, run, value], span });
            if let CollectLoweringDomain::Generalized { types, .. } = domain {
                types.insert(result, storage_type.clone()?);
                if let Some(checked_type) = &checked_type {
                    let checked = self.checked_unsigned_value(result, checked_type, span);
                    self.record_scheme_storage_elaboration(checked, expected.as_ref()?, types);
                    return Some(checked);
                }
            }
            Some(result)
        })();
        match previous {
            Some(previous) => { slots.collect_slots.insert(expression, previous); }
            None => { slots.collect_slots.remove(&expression); }
        }
        slots.exit(saved);
        lowered
    }

    fn collect_list_type(facts: &crate::sema::schemes::SchemeBodyFacts, expression: ExprId) -> Option<super::SchemeType> {
        match facts.expression_types().get(&expression)? {
            ty @ super::SchemeType::List(_) => Some(ty.clone()),
            super::SchemeType::Ground(Type::List(item)) => Some(super::SchemeType::List(Box::new(super::SchemeType::Ground((**item).clone())))),
            _ => None,
        }
    }

    pub(super) fn lower_scheme_collect_yield(
        &mut self, statement: StmtId, value: ExprId, kind: CollectYieldKind,
        slots: &mut SlotScope, current_function: Name, facts: &crate::sema::schemes::SchemeBodyFacts,
        visited: &mut super::FxHashSet<ExprId>, types: &mut super::BTreeMap<BuildExprId, super::SchemeType>, body: &mut Vec<BuildStmtId>,
    ) -> Option<BuildStmtId> {
        let delegated = matches!(kind, CollectYieldKind::List);
        let collect = *self.bodies.collect_yields.get(&statement)?;
        let CollectSlot::Generalized { ty, .. } = slots.collect_slots.get(&collect)? else { return None; };
        let ty = ty.clone();
        let span = self.program.arena.stmt(statement).span;
        let value = self.lower_scheme_expression(value, slots, current_function, facts, visited, types, body)?;
        let items = if delegated && types.get(&value) == Some(&ty) { value } else {
            let row = if delegated { BuildExprRow::ListBuild(vec![(true, value, span)]) } else { BuildExprRow::List(vec![value]) };
            let items = push_build_row!(self, expr, row);
            types.insert(items, ty);
            items
        };
        self.lower_collect_yield(statement, items, slots)
    }

    pub(super) fn lower_collect_yield(
        &mut self,
        id: StmtId,
        items: BuildExprId,
        slots: &SlotScope,
    ) -> Option<BuildStmtId> {
        let collect = *self.bodies.collect_yields.get(&id)?;
        let slot = slots.collect_slots.get(&collect)?.slot();
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

    pub(super) fn lower_retry_block(
        &mut self,
        block: BlockId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<Vec<BuildStmtId>> {
        self.lower_retry_block_with_context(block, slots, &mut (),
            |lowerer, statement, slots, (), body| {
                body.push(lowerer.lower_stmt_with_blocker_guard(statement, slots, current_function, item_slot)?);
                Some(())
            },
            |lowerer, statement, slots, (), _| lowerer.lower_tail_stmt_as_expr(statement, slots, current_function, item_slot))
    }

    pub(super) fn lower_retry_block_with_context<C>(
        &mut self, block: BlockId, slots: &mut SlotScope, context: &mut C,
        mut lower_statement: impl FnMut(&mut Self, StmtId, &mut SlotScope, &mut C, &mut Vec<BuildStmtId>) -> Option<()>,
        mut lower_tail: impl FnMut(&mut Self, StmtId, &mut SlotScope, &mut C, &mut Vec<BuildStmtId>) -> Option<BuildExprId>,
    ) -> Option<Vec<BuildStmtId>> {
        if !self.program.arena.block(block).params.is_empty() { return Some(Vec::new()); }
        let transfers_resource = self.bodies.resource_transfer(crate::sema::check::ResourceTransferSite::BlockValue(block))?;
        let saved = slots.enter();
        let result = (|| {
            let ids = self.program.arena.stmt_ids(self.program.arena.block(block).statements).collect::<Vec<_>>();
            let Some((&tail, prefix)) = ids.split_last() else { return (!transfers_resource).then(Vec::new); };
            let mut lowered = Vec::with_capacity(ids.len());
            for &statement in prefix { lower_statement(self, statement, slots, context, &mut lowered)?; }
            if self.bodies.statement_positions.get(&tail) == Some(&crate::sema::check::StatementPosition::Statement) {
                if transfers_resource { return None; }
                lower_statement(self, tail, slots, context, &mut lowered)?;
            } else if let Some(value) = lower_tail(self, tail, slots, context, &mut lowered) {
                let statement = push_build_row!(self, stmt, BuildStmtRow::Value { value });
                if transfers_resource { self.scratch.borrow_mut().mark_resource_transfer(statement); }
                lowered.push(statement);
            } else {
                if transfers_resource { return None; }
                lower_statement(self, tail, slots, context, &mut lowered)?;
            }
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

    fn lower_tail_if_stmt(
        &mut self,
        branches: crate::syntax::arena::ArenaRange,
        else_block: Option<BlockId>,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
        purpose: TailBlockPurpose,
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
            let body = self.lower_tail_block_for_purpose(branch.block, slots, current_function, item_slot, purpose);
            slots.exit(saved);
            lowered.push((condition, body?, captures));
        }
        let else_body = match else_block {
            Some(block) => {
                let saved = slots.enter();
                let body = self.lower_tail_block_for_purpose(block, slots, current_function, item_slot, purpose);
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

    fn lower_tail_match_stmt(
        &mut self,
        value: ExprId,
        arms: crate::syntax::arena::ArenaRange,
        span: Span,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
        purpose: TailBlockPurpose,
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
            let body = self.lower_tail_block_for_purpose(arm.block, slots, current_function, item_slot, purpose);
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

    pub(super) fn lower_tempdir_scope(
        &mut self, path: Option<ExprId>, block: BlockId, span: Span,
        slots: &mut SlotScope, current_function: Option<Name>, item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        self.lower_tempdir_scope_in_domain(path, block, span, slots, current_function, item_slot,
            &mut TempDirLoweringDomain::Ordinary)
    }

    fn tempdir_expression(&mut self, row: BuildExprRow, ty: super::SchemeType,
        domain: &mut TempDirLoweringDomain<'_>) -> BuildExprId {
        let value = super::build_expr(&self.scratch, row);
        if let TempDirLoweringDomain::Generalized { types, .. } = domain { types.insert(value, ty); }
        value
    }

    fn tempdir_declaration(&mut self, slot: usize, value: BuildExprId, ty: super::SchemeType,
        slots: &mut SlotScope, domain: &mut TempDirLoweringDomain<'_>) -> BuildStmtId {
        let declaration = push_build_row!(self, stmt, BuildStmtRow::Let { slot, value, annotation: None });
        if matches!(domain, TempDirLoweringDomain::Generalized { .. }) {
            slots.scheme_declarations.insert(declaration, super::super::SchemeLocalDeclaration {
                ty, mutability: super::super::SchemeLocalMutability::Immutable,
            });
        }
        declaration
    }

    fn tempdir_failure(&mut self, slots: &mut SlotScope, ty: super::SchemeType,
        domain: &mut TempDirLoweringDomain<'_>) -> (super::BuildPatternId, Option<BuildExprId>, BuildExprId) {
        let slot = slots.reserve("tempdir.failed");
        let pattern = push_build_row!(self, pattern, BuildPatternRow::ResultErr { slot: Some(slot), unit_only: false });
        let value = self.tempdir_expression(BuildExprRow::Param(slot), super::SchemeType::Ground(Type::Error), domain);
        let failure = self.tempdir_expression(BuildExprRow::Err { value, cause: None }, ty, domain);
        (pattern, None, failure)
    }

    fn tempdir_path_binding(&mut self, block: BlockId, slots: &mut SlotScope,
        domain: &TempDirLoweringDomain<'_>) -> Option<usize> {
        match self.program.arena.block_params(self.program.arena.block(block).params) {
            [parameter] if parameter.name.as_str() != "_" => {
                if matches!(domain, TempDirLoweringDomain::Generalized { .. }) {
                    Some(slots.declare_scheme_source(self.declaration_binding(self.program.arena.span(parameter.span), parameter.name)?,
                        parameter.name, super::SchemeType::Ground(Type::Path)))
                } else {
                    self.declare_source_at(self.program.arena.span(parameter.span), parameter.name, Some(Type::Path), slots)
                }
            }
            _ => Some(slots.reserve("tempdir.path")),
        }
    }

    fn tempdir_body_in_domain(&mut self, block: BlockId, mut body: Vec<BuildStmtId>, span: Span,
        slots: &mut SlotScope, current_function: Option<Name>, item_slot: Option<usize>,
        domain: &mut TempDirLoweringDomain<'_>) -> Option<BuildExprId> {
        let (tail, ty) = match domain {
            TempDirLoweringDomain::Ordinary => return self.lower_tempdir_body(block, body, span, slots, current_function, item_slot),
            TempDirLoweringDomain::Generalized { facts, visited, types } => {
                let statements = self.program.arena.stmt_ids(self.program.arena.block(block).statements).collect::<Vec<_>>();
                let mut tail = None;
                if let Some((&last, prefix)) = statements.split_last() {
                    for &statement in prefix {
                        self.lower_scheme_statement(statement, slots, current_function?, facts, visited, types, &mut body)?;
                    }
                    if self.bodies.statement_positions.get(&last) != Some(&crate::sema::check::StatementPosition::Statement) {
                        tail = Some(self.lower_scheme_tail_value(last, slots, current_function?, facts, visited, types, &mut body)?);
                    } else {
                        self.lower_scheme_statement(last, slots, current_function?, facts, visited, types, &mut body)?;
                    }
                }
                match tail {
                    Some(tail) => (tail, types.get(&tail)?.clone()),
                    None => {
                        let unit = push_build_row!(self, expr, BuildExprRow::Unit);
                        types.insert(unit, super::SchemeType::Ground(Type::Unit));
                        (unit, super::SchemeType::Ground(Type::Unit))
                    }
                }
            }
        };
        let ty = super::SchemeType::Result(Box::new(ty), Box::new(super::SchemeType::Ground(Type::Error)));
        let value = self.tempdir_expression(BuildExprRow::Ok(tail), ty.clone(), domain);
        body.push(push_build_row!(self, stmt, BuildStmtRow::Value { value }));
        Some(self.tempdir_expression(BuildExprRow::ValueBlock { body, span }, ty, domain))
    }

    pub(super) fn lower_tempdir_scope_in_domain(&mut self, path: Option<ExprId>, block: BlockId, span: Span,
        slots: &mut SlotScope, current_function: Option<Name>, item_slot: Option<usize>,
        domain: &mut TempDirLoweringDomain<'_>) -> Option<BuildExprId> {
        use super::SchemeType;
        let ground = SchemeType::Ground;
        let unit_result = SchemeType::Result(Box::new(ground(Type::Unit)), Box::new(ground(Type::Error)));
        let path = match path {
            Some(expression) => Some(match domain {
                TempDirLoweringDomain::Ordinary => self.lower_expr(expression, slots, current_function, item_slot)?,
                TempDirLoweringDomain::Generalized { facts, visited, types } => self.lower_scheme_lazy_expression(expression, slots, current_function?, facts, visited, types)?,
            }),
            None => None,
        };
        let saved = slots.enter();
        let result = (|| {
            if let Some(path) = path {
                let at_slot = slots.reserve("tempdir.at");
                let at = |lowerer: &mut Self, domain: &mut TempDirLoweringDomain<'_>| lowerer.tempdir_expression(BuildExprRow::Param(at_slot), ground(Type::Path), domain);
                let remove = |lowerer: &mut Self, domain: &mut TempDirLoweringDomain<'_>| {
                    let path = at(lowerer, domain);
                    let missing_ok = lowerer.tempdir_expression(BuildExprRow::Bool(true), ground(Type::Bool), domain);
                    lowerer.tempdir_expression(BuildExprRow::FsRemove { path, missing_ok: Some(missing_ok), span }, unit_result.clone(), domain)
                };
                let inner = slots.enter();
                let scope = (|| {
                    let removal = remove(self, domain);
                    let removal = self.tempdir_expression(BuildExprRow::Try { value: removal, span }, ground(Type::Unit), domain);
                    let mut body = vec![push_build_row!(self, stmt, BuildStmtRow::Defer { value: removal, on_error: false })];
                    let value = at(self, domain);
                    let path_slot = self.tempdir_path_binding(block, slots, domain)?;
                    body.push(self.tempdir_declaration(path_slot, value, ground(Type::Path), slots, domain));
                    self.tempdir_body_in_domain(block, body, span, slots, current_function, item_slot, domain)
                })();
                slots.exit(inner);
                let scope = scope?;
                let ty = match domain { TempDirLoweringDomain::Generalized { types, .. } => types.get(&scope)?.clone(), _ => ground(Type::Unit) };
                let path_value = at(self, domain);
                let created = self.tempdir_expression(BuildExprRow::FsMkdir { path: path_value, parents: None, span }, unit_result.clone(), domain);
                let done = push_build_row!(self, pattern, BuildPatternRow::ResultOk { slot: None, unit_only: false });
                let failure = self.tempdir_failure(slots, ty.clone(), domain);
                let entered = self.tempdir_expression(BuildExprRow::MatchExpr { value: created, arms: vec![(done, None, scope), failure], span }, ty.clone(), domain);
                let cleared = remove(self, domain);
                let done = push_build_row!(self, pattern, BuildPatternRow::ResultOk { slot: None, unit_only: false });
                let failure = self.tempdir_failure(slots, ty.clone(), domain);
                let outcome = self.tempdir_expression(BuildExprRow::MatchExpr { value: cleared, arms: vec![(done, None, entered), failure], span }, ty.clone(), domain);
                let body = vec![self.tempdir_declaration(at_slot, path, ground(Type::Path), slots, domain), push_build_row!(self, stmt, BuildStmtRow::Value { value: outcome })];
                Some(self.tempdir_expression(BuildExprRow::ValueBlock { body, span }, ty, domain))
            } else {
                let root_result = SchemeType::Result(Box::new(ground(Type::FsRoot)), Box::new(ground(Type::Error)));
                let created = self.tempdir_expression(BuildExprRow::FsTempDir { span }, root_result, domain);
                let root_slot = slots.reserve("tempdir.root");
                let opened = push_build_row!(self, pattern, BuildPatternRow::ResultOk { slot: Some(root_slot), unit_only: false });
                let root_method = |lowerer: &mut Self, op: RuntimeOp, ty: Type, domain: &mut TempDirLoweringDomain<'_>| {
                    let receiver = lowerer.tempdir_expression(BuildExprRow::Param(root_slot), ground(Type::FsRoot), domain);
                    let result = SchemeType::Result(Box::new(ground(ty.clone())), Box::new(ground(Type::Error)));
                    let call = lowerer.tempdir_expression(BuildExprRow::ModuleCall { call_plan: None, op, args: vec![Some(receiver)], span }, result, domain);
                    lowerer.tempdir_expression(BuildExprRow::Try { value: call, span }, ground(ty), domain)
                };
                let close = root_method(self, RuntimeOp::FsCloseRoot, Type::Unit, domain);
                let mut body = vec![push_build_row!(self, stmt, BuildStmtRow::Defer { value: close, on_error: false })];
                let path = root_method(self, RuntimeOp::FsRootPath, Type::Path, domain);
                let path_slot = self.tempdir_path_binding(block, slots, domain)?;
                body.push(self.tempdir_declaration(path_slot, path, ground(Type::Path), slots, domain));
                let scope = self.tempdir_body_in_domain(block, body, span, slots, current_function, item_slot, domain)?;
                let ty = match domain { TempDirLoweringDomain::Generalized { types, .. } => types.get(&scope)?.clone(), _ => ground(Type::Unit) };
                let failure = self.tempdir_failure(slots, ty.clone(), domain);
                Some(self.tempdir_expression(BuildExprRow::MatchExpr { value: created, arms: vec![(opened, None, scope), failure], span }, ty, domain))
            }
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
            if !slots.record_checked_slot_type(closing_slot, Type::Bool) { return None; }
            let release = |lowerer: &mut Self,
                           slot: usize,
                           kind: crate::modules::ManagedResource,
                           unless_closing: bool| {
                let value = push_build_row!(lowerer, expr, BuildExprRow::Param(slot));
                let mut release = push_build_row!(
                    lowerer,
                    expr,
                    BuildExprRow::ModuleCall {
                        call_plan: None,
                        op: kind.release_op().expect("checker accepts only resources with a with release"),
                        args: vec![Some(value)],
                        span,
                    }
                );
                if unless_closing {
                    let closing = push_build_row!(lowerer, expr, BuildExprRow::Param(closing_slot));
                    let unit = push_build_row!(lowerer, expr, BuildExprRow::Unit);
                    let nothing = push_build_row!(lowerer, expr, BuildExprRow::Ok(unit));
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
                    value: open,
                    annotation: None,
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
                    self.declare_source_at(self.program.arena.span(binding.span), binding.name, ty, slots)?
                };
                body.push(push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::Let {
                        slot,
                        value,
                        annotation: None,
                    }
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
            let finished_type = self.lower_binding_checked_type(None, scope)?;
            let finished_type = self.canonicalize_checked_type(finished_type);
            if !slots.record_checked_slot_type(finished_slot, finished_type.clone()) { return None; }
            body.push(push_build_row!(
                self,
                stmt,
                BuildStmtRow::Let {
                    slot: finished_slot,
                    value: finished,
                    annotation: None,
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
            if !self.scratch.borrow_mut().record_checked_expr_type(
                outcome,
                finished_type,
                false,
            ) {
                return None;
            }
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

    /// Keep branch statements and their tail in one lexical scope so the
    /// selected value is evaluated before that scope runs cleanup.
    pub(super) fn lower_block_value_expr(
        &mut self,
        block: BlockId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let span = self
            .program
            .arena
            .span(self.program.arena.block(block).span);
        let transfers_resource = self
            .bodies
            .resource_transfer(crate::sema::check::ResourceTransferSite::BlockValue(block))?;
        let statements = self.program.arena.block(block).statements;
        let statements = self.program.arena.stmt_ids(statements).collect::<Vec<_>>();
        let saved = slots.enter();
        let result = (|| {
            let mut body = Vec::with_capacity(statements.len());
            let mut emitted_transfer = false;
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
                    let stmt = push_build_row!(self, stmt, BuildStmtRow::Value { value });
                    if transfers_resource {
                        self.scratch.borrow_mut().mark_resource_transfer(stmt);
                        emitted_transfer = true;
                    }
                    body.push(stmt);
                } else {
                    if transfers_resource {
                        return None;
                    }
                    body.push(self.lower_stmt_with_blocker_guard(
                        tail,
                        slots,
                        current_function,
                        item_slot,
                    )?);
                }
            }
            if transfers_resource && !emitted_transfer {
                return None;
            }
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
