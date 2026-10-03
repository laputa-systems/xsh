use super::*;

impl CompactLowerConstructProbe<'_, '_> {
    pub(super) fn record_conditional_result(&self, original: ExprId, row: BuildExprId) -> Option<()> {
        use super::super::indexed::full::{BuildConditionalArm, BuildConditionalBody, BuildConditionalResult};
        use super::super::indexed::generic::ConditionalKind;
        let creation_check = matches!(self.scratch.borrow().expressions.get(row.index()), Some(BuildExprRow::CheckedValue { check, .. }) if check.ty == Type::UInt).then_some(row);
        let row = self.original_source_instruction(row)?;
        let actual = self.scratch.borrow().expressions.get(row.index())?.clone();
        if !matches!(actual, BuildExprRow::MatchExpr { .. } | BuildExprRow::IfExpr { .. }) { return Some(()); }
        let Some(source) = self.pattern_result_source(original) else { return Some(()); };
        let mut subject = None;
        let mut arms = Vec::new();
        let mut fallback = None;
        let kind = match (self.program.arena.expr(original).kind, actual) {
            (ArenaExprKind::Match { value, arms: originals } | ArenaExprKind::PatternTest { value, arms: originals }, BuildExprRow::MatchExpr { value: actual, arms: lowered, .. }) => {
                let pattern_test = matches!(self.program.arena.expr(original).kind, ArenaExprKind::PatternTest { .. });
                let originals = self.program.arena.match_expr_arms(originals);
                if originals.len() != lowered.len() { return None; }
                let Some(original_subject) = self.pattern_result_source(value) else { return Some(()); };
                subject = Some((actual, original_subject));
                for (original, (pattern, guard, body)) in originals.iter().zip(lowered) {
                    let identity = self.original_pattern_identity(original.pattern);
                    if self.scratch.borrow().pattern_origins.get(&pattern) != Some(&identity) { return None; }
                    let guard = match (original.guard, guard) {
                        (None, None) => None,
                        (Some(original), Some(actual)) => {
                            let Some(source) = self.pattern_result_source(original) else { return Some(()); };
                            Some((actual, source))
                        }
                        _ => return None,
                    };
                    let body = if pattern_test {
                        let ArenaExprKind::Bool(value) = self.program.arena.expr(original.value).kind else { return None; };
                        if !matches!(self.scratch.borrow().expressions.get(body.index()), Some(BuildExprRow::Bool(actual)) if *actual == value) { return None; }
                        BuildConditionalBody::Boolean { instruction: body, value }
                    } else {
                        let Some(body) = self.conditional_result_body(original.value, body) else { return Some(()); };
                        body
                    };
                    arms.push(BuildConditionalArm { pattern: Some((pattern, identity)), condition: None, guard, body });
                }
                if pattern_test { ConditionalKind::PatternTest } else { ConditionalKind::Match }
            }
            (ArenaExprKind::If { branches, else_value }, BuildExprRow::IfExpr { branches: actual, else_value: actual_fallback, .. }) => {
                let originals = self.program.arena.if_expr_branches(branches);
                if originals.len() != actual.len() { return None; }
                for (original, (condition, body)) in originals.iter().zip(actual) {
                    let Some(condition_source) = self.pattern_result_source(original.condition) else { return Some(()); };
                    let Some(body) = self.conditional_result_body(original.value, body) else { return Some(()); };
                    arms.push(BuildConditionalArm { pattern: None, condition: Some((condition, condition_source)), guard: None, body });
                }
                let Some(body) = self.conditional_result_body(else_value, actual_fallback) else { return Some(()); };
                fallback = Some(body);
                ConditionalKind::If
            }
            (ArenaExprKind::Unary { op: UnaryOp::Not, expr }, BuildExprRow::IfExpr { branches, else_value, .. }) => {
                let [(condition, body)] = branches.as_slice() else { return None; };
                let Some(condition_source) = self.pattern_result_source(expr) else { return Some(()); };
                let scratch = self.scratch.borrow();
                if !matches!(scratch.expressions.get(body.index()), Some(BuildExprRow::Bool(false)))
                    || !matches!(scratch.expressions.get(else_value.index()), Some(BuildExprRow::Bool(true))) { return None; }
                arms.push(BuildConditionalArm { pattern: None, condition: Some((*condition, condition_source)), guard: None,
                    body: BuildConditionalBody::Boolean { instruction: *body, value: false } });
                fallback = Some(BuildConditionalBody::Boolean { instruction: else_value, value: true });
                ConditionalKind::Not
            }
            _ => return Some(()),
        };
        let result = BuildConditionalResult { instruction: row, source, kind, creation_check, subject, arms: arms.into_boxed_slice(), fallback };
        let mut scratch = self.scratch.borrow_mut();
        if scratch.conditional_result_origins.insert(result.source.origin, result.clone()).is_some_and(|previous| previous != result) { return None; }
        Some(())
    }

    fn conditional_result_body(&self, original: ExprId, instruction: BuildExprId) -> Option<super::super::indexed::full::BuildConditionalBody> {
        use super::super::indexed::full::BuildConditionalBody;
        if let Some(body) = self.pattern_result_body(original, instruction) { return Some(BuildConditionalBody::Authored(body)); }
        let source = self.pattern_result_source(original)?;
        let ArenaExprKind::ValueBlock(block) = self.program.arena.expr(original).kind else { return None; };
        let tail = self.program.arena.stmt_ids(self.program.arena.block(block).statements).last()?;
        let ArenaStmtKind::TailBareIdent(name) = self.program.arena.stmt(tail).kind else { return None; };
        let statement = self.statement_identity(tail);
        let solved = self.solved();
        if solved.statement_owners.get(&statement).copied() != source.caller
            || solved.statements.get(&statement) != Some(&crate::sema::check::StatementPosition::Value) { return None; }
        let scratch = self.scratch.borrow();
        let BuildExprRow::ValueBlock { body, .. } = scratch.expressions.get(instruction.index())? else { return None; };
        let tail_statement = *body.last()?;
        let BuildStmtRow::Value { value: read } = scratch.statements.get(tail_statement.index())? else { return None; };
        let BuildExprRow::Param(slot) = scratch.expressions.get(read.index())? else { return None; };
        let (read_type, read_scope, parameter) = if let Some(&(original_statement, binding)) = scratch.value_statement_reads.get(read) {
            if original_statement != statement { return None; }
            let original = solved.bindings.get(&binding)?;
            if original.mutable || original.owner != source.caller { return None; }
            (original.ty, original.scheme.or_else(|| source.caller.and_then(|caller| solved.declarations.get(&caller).map(|decl| decl.scheme))), None)
        } else {
            let caller = source.caller?;
            let function = self.program.arena.function_def(caller.declaration);
            let original = self.program.arena.params(function.params).get(*slot)?;
            if original.name != name { return None; }
            let declaration = solved.declarations.get(&caller)?;
            let crate::sema::inference::TypeNode::Arrow(signature) = solved.graph.node(solved.graph.resolved(declaration.signature).ok()?).ok()? else { return None; };
            (signature.params.get(*slot)?.ty, Some(declaration.scheme), Some(u32::try_from(*slot).ok()?))
        };
        solved.graph.validate_scoped(crate::sema::inference::ScopedRoot { ty: read_type, scope: read_scope }).ok()?;
        Some(BuildConditionalBody::StatementTail {
            body: super::super::BuildPatternResultBody { instruction, source, condition: None, terminal: None },
            statement, tail_statement, read: *read, read_type, read_scope, parameter,
        })
    }
}
