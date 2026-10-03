use super::*;

impl CompactLowerConstructProbe<'_, '_> {
    fn record_pattern_admission(
        &mut self, condition: ExprId, matcher: BuildExprId, control: super::super::BuildPatternControlRow,
        control_origin: super::super::indexed::generic::OperationSourceOrigin, branch: usize,
        body: super::super::BuildPatternAdmissionBody,
    ) -> Option<()> {
        let ArenaExprKind::PatternCondition { arms, .. } = self.program.arena.expr(condition).kind else { return Some(()); };
        let original = self.program.arena.match_expr_arms(arms).first()?.pattern;
        let original = self.original_pattern_identity(original);
        let condition_origin = self.expression_identity(condition);
        let mut scratch = self.scratch.borrow_mut();
        let BuildExprRow::MatchExpr { arms, .. } = scratch.expressions.get(matcher.index())? else { return None; };
        if arms.len() != 2 || scratch.pattern_origins.get(&arms[0].0) != Some(&original) { return None; }
        let admission = super::super::BuildPatternAdmission {
            control, control_origin, condition_origin, branch: u32::try_from(branch).ok()?, body,
            result: None,
        };
        if scratch.pattern_admissions.insert(matcher, admission.clone()).is_some_and(|previous| previous != admission) { return None; }
        drop(scratch);
        self.expression_origins.insert(matcher, condition_origin);
        Some(())
    }

    pub(super) fn record_pattern_expression_admissions(
        &mut self, row: BuildExprId, control_origin: super::super::indexed::generic::OperationSourceOrigin,
        conditions: &[ExprId],
    ) -> Option<BuildExprId> {
        let branches = {
            let scratch = self.scratch.borrow();
            let BuildExprRow::PatternIf { branches, .. } = scratch.expressions.get(row.index())? else { return None; };
            branches.clone()
        };
        if branches.len() != conditions.len() { return None; }
        for (branch, (&condition, (matcher, body, _))) in conditions.iter().zip(branches).enumerate() {
            self.record_pattern_admission(condition, matcher, super::super::BuildPatternControlRow::Expression(row), control_origin, branch,
                super::super::BuildPatternAdmissionBody::Expression(body))?;
        }
        if let super::super::indexed::generic::OperationSourceOrigin::Expression(origin) = control_origin {
            // Capture admission remains independent when the value has no
            // retained result receipt. Typed consumers require that receipt.
            let _ = self.record_pattern_result(row, origin);
        }
        Some(row)
    }

    pub(super) fn pattern_result_source(&self, expression: ExprId) -> Option<super::super::BuildPatternResultSource> {
        let origin = self.expression_identity(expression);
        let solved = self.solved();
        if solved.non_completing_expressions.contains(&origin) { return None; }
        let ty = *solved.expressions.get(&origin)?;
        let caller = solved.expression_owners.get(&origin).copied();
        let scope = solved.expression_scope(origin, caller).ok()?;
        solved.graph.validate_scoped(crate::sema::inference::ScopedRoot { ty, scope }).ok()?;
        Some(super::super::BuildPatternResultSource { origin, ty, scope, caller })
    }

    pub(super) fn pattern_result_body(&self, original: ExprId, instruction: BuildExprId) -> Option<super::super::BuildPatternResultBody> {
        let source = self.pattern_result_source(original)?;
        let terminal = if let ArenaExprKind::ValueBlock(block) = self.program.arena.expr(original).kind {
            let tail = self.program.arena.stmt_ids(self.program.arena.block(block).statements).last()?;
            let scratch = self.scratch.borrow();
            let BuildExprRow::ValueBlock { body, .. } = scratch.expressions.get(instruction.index())? else { return None; };
            let statement = *body.last()?;
            let BuildStmtRow::Value { value } = scratch.statements.get(statement.index())? else { return None; };
            let identity = self.statement_identity(tail);
            let tail_source = match self.program.arena.stmt(tail).kind {
                ArenaStmtKind::Expr(expression) => {
                    let original = self.pattern_result_source(expression)?;
                    if self.expression_origins.get(value) != Some(&original.origin) { return None; }
                    super::super::BuildPatternResultTerminalSource::Expression(original)
                }
                ArenaStmtKind::TailBareIdent(name) => {
                    let &(row, capture) = scratch.pattern_statement_use_origins.get(&identity)?;
                    if row != super::super::BuildPatternUseRow::Expression(*value) || capture.name != name { return None; }
                    let solved = self.solved();
                    let pattern = solved.checked_pattern(capture.pattern).ok()?;
                    if pattern.caller != source.caller || !pattern.captures.iter().any(|original| original.identity == capture) { return None; }
                    super::super::BuildPatternResultTerminalSource::PatternCapture(capture)
                }
                _ => return None,
            };
            Some((statement, *value, tail_source, identity))
        } else { None };
        Some(super::super::BuildPatternResultBody { instruction, source, condition: None, terminal })
    }

    fn record_pattern_result(&self, row: BuildExprId, origin: ExpressionIdentity) -> Option<()> {
        let ArenaExprKind::If { branches, else_value } = self.program.arena.expr(origin.expression).kind else { return None; };
        let originals = self.program.arena.if_expr_branches(branches);
        let (actual, fallback) = {
            let scratch = self.scratch.borrow();
            let BuildExprRow::PatternIf { branches, else_value, .. } = scratch.expressions.get(row.index())? else { return None; };
            (branches.clone(), *else_value)
        };
        if originals.len() != actual.len() { return None; }
        let source = self.pattern_result_source(origin.expression)?;
        let branches = originals.iter().zip(&actual).map(|(original, (matcher, body, _))| {
            let mut body = self.pattern_result_body(original.value, *body)?;
            body.condition = Some((*matcher, self.pattern_result_source(original.condition)?));
            Some(body)
        }).collect::<Option<Vec<_>>>()?;
        let fallback = self.pattern_result_body(else_value, fallback)?;
        let matcher = actual.iter().find_map(|(matcher, _, _)| self.scratch.borrow().pattern_admissions.contains_key(matcher).then_some(*matcher))?;
        self.scratch.borrow_mut().pattern_admissions.get_mut(&matcher)?.result = Some(Box::new(super::super::BuildPatternConditionalResult {
            source, branches: branches.into_boxed_slice(), fallback,
        }));
        Some(())
    }

    pub(super) fn record_pattern_statement_admissions(
        &mut self, row: BuildStmtId, statement: StmtId, conditions: &[ExprId],
    ) -> Option<BuildStmtId> {
        let branches = {
            let scratch = self.scratch.borrow();
            let BuildStmtRow::PatternIf { branches, .. } = scratch.statements.get(row.index())? else { return None; };
            branches.clone()
        };
        if branches.len() != conditions.len() { return None; }
        let origin = super::super::indexed::generic::OperationSourceOrigin::Statement(self.statement_identity(statement));
        for (branch, (&condition, (matcher, body, _))) in conditions.iter().zip(branches).enumerate() {
            self.record_pattern_admission(condition, matcher, super::super::BuildPatternControlRow::Statement(row), origin, branch,
                super::super::BuildPatternAdmissionBody::Statements(body.into_boxed_slice()))?;
        }
        Some(row)
    }

    pub(super) fn record_pattern_loop_admission(&mut self, row: BuildStmtId, statement: StmtId, condition: ExprId) -> Option<BuildStmtId> {
        let (matcher, body) = {
            let scratch = self.scratch.borrow();
            let BuildStmtRow::PatternWhile { condition, body, .. } = scratch.statements.get(row.index())? else { return None; };
            (*condition, body.clone())
        };
        self.record_pattern_admission(condition, matcher, super::super::BuildPatternControlRow::Statement(row),
            super::super::indexed::generic::OperationSourceOrigin::Statement(self.statement_identity(statement)), 0,
            super::super::BuildPatternAdmissionBody::Statements(body.into_boxed_slice()))?;
        Some(row)
    }
}
