use super::*;
use crate::sema::check::{BindingIdentity, ExpressionIdentity, StatementIdentity};
use crate::sema::inference::{ScopedRoot, TypeNode};

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) enum BuildMutablePathStep {
    Field { name: Name, input: ScopedRoot, output: ScopedRoot },
    Index { expression: BuildExprId, source: ExpressionIdentity, checked: ScopedRoot, input: ScopedRoot, output: ScopedRoot },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) enum BuildMutablePathValue {
    Value(BuildExprId),
    Integer(BuildIntId),
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct BuildMutablePathWrite {
    pub binding: BindingIdentity,
    pub statement: StatementIdentity,
    pub target: crate::syntax::arena::AssignTargetId,
    pub slot: usize,
    pub op: AssignOp,
    pub value: BuildMutablePathValue,
    pub value_source: ExpressionIdentity,
    pub value_type: ScopedRoot,
    pub selected_type: ScopedRoot,
    pub check: Option<Type>,
    pub compound: Option<crate::sema::check::SolvedOperation>,
    pub steps: Box<[BuildMutablePathStep]>,
}

// Aggregate mutation authority requires every original child to have an inert
// ground carrier. Resource and callable roles need their own storage protocol.
pub(super) fn inert_mutable_aggregate(ty: &Type) -> bool {
    fn inert(ty: &Type) -> bool {
        match ty {
            Type::Int | Type::UInt | Type::Bool | Type::Str | Type::Float | Type::Duration | Type::Bytes | Type::Path | Type::Regex | Type::Null | Type::Unit => true,
            Type::List(item) | Type::Optional(item) => inert(item),
            Type::Map(key, value) => inert(key) && inert(value),
            Type::Record(fields) => fields.values().all(inert),
            _ => false,
        }
    }
    matches!(ty, Type::List(_) | Type::Map(_, _) | Type::Record(_)) && inert(ty)
}

impl CompactLowerConstructProbe<'_, '_> {
    pub(super) fn record_original_mutable_path(&mut self, statement: StmtId, row: BuildStmtId, slots: &SlotScope) -> Option<()> {
        let ArenaStmtKind::Assign { target, op, value: ArenaExprOrRun::Expr(value_source), .. } = self.program.arena.stmt(statement).kind else { return Some(()); };
        let name = self.assign_target_root_name(target)?;
        let Some(&binding) = slots.mutable_binding_authorities.get(&name) else { return Some(()); };
        let scratch = self.scratch.borrow();
        let local = scratch.mutable_binding_origins.get(&binding);
        let driver = scratch.mutable_driver_bindings.get(&binding);
        let source_type = local.map(|original| original.source_type).or_else(|| driver.map(|original| original.source_type))?;
        let (slot, path, emitted_op, value, check) = match scratch.statements.get(row.index())? {
            BuildStmtRow::AssignPath { slot, path, op, value, check, .. } => (*slot, path.0.clone(), *op, BuildMutablePathValue::Value(*value), check.as_ref().map(|check| check.ty.clone())),
            BuildStmtRow::AssignField { slot, field, op, value, .. } => (*slot, vec![LoweredAssignStep::Field(Name::intern(field))], *op, BuildMutablePathValue::Value(*value), None),
            BuildStmtRow::AssignFieldInt { slot, field, op, value, .. } => (*slot, vec![LoweredAssignStep::Field(Name::intern(field))], *op, BuildMutablePathValue::Integer(*value), None),
            _ => return None,
        };
        if local.is_some_and(|original| original.slot != slot) || driver.is_some_and(|original| original.name != name) || slots.resolve(name) != Some(slot) || emitted_op != op || path.is_empty() || path.len() > 128 { return None; }
        let mut targets = Vec::new();
        let mut current = target;
        loop {
            match self.program.arena.assign_target(current).kind {
                ArenaAssignTargetKind::Name(root) if root == name => break,
                ArenaAssignTargetKind::Field { base, name } => { targets.push((Some(name), None)); current = base; }
                ArenaAssignTargetKind::Index { base, index } => { targets.push((None, Some(index))); current = base; }
                _ => return None,
            }
            if targets.len() > 128 { return None; }
        }
        targets.reverse();
        if targets.len() != path.len() { return None; }
        let solved = self.solved();
        let owner = solved.bindings.get(&binding)?.owner;
        let mut selected_type = source_type;
        let mut steps = Vec::new();
        for (authored, emitted) in targets.into_iter().zip(&path) {
            let input = selected_type;
            let resolved = solved.graph.resolved(input.ty).ok()?;
            let (step, child) = match (authored, emitted, solved.graph.node(resolved).ok()?) {
                ((Some(name), None), LoweredAssignStep::Field(actual), TypeNode::Record(record)) if *actual == name => {
                    let record = solved.graph.row_data(*record).ok()?;
                    if record.tail.is_some() { return None; }
                    let child = record.fields.iter().find(|field| field.label == name)?.ty;
                    (BuildMutablePathStep::Field { name, input, output: ScopedRoot { ty: child, scope: input.scope } }, child)
                }
                ((None, Some(index)), LoweredAssignStep::Index(expression), node @ (TypeNode::List(_) | TypeNode::Map(_, _))) => {
                    let (key, child) = match node { TypeNode::List(child) => (None, *child), TypeNode::Map(key, value) => (Some(*key), *value), _ => unreachable!() };
                    let source = self.expression_identity(index);
                    let checked = ScopedRoot { ty: *solved.expressions.get(&source)?, scope: solved.expression_scope(source, owner).ok()? };
                    if solved.expression_owners.get(&source).copied() != owner { return None; }
                    solved.graph.validate_scoped(checked).ok()?;
                    let actual = super::super::indexed::generic::graph_ground_type(&solved.graph, checked.ty).ok()?;
                    if let Some(key) = key {
                        let expected = super::super::indexed::generic::graph_ground_type(&solved.graph, key).ok()?;
                        if !actual.matches_expected(&expected) { return None; }
                    } else if !matches!(actual, Type::Int | Type::UInt) { return None; }
                    if self.expression_origins.get(expression) != Some(&source) { return None; }
                    (BuildMutablePathStep::Index { expression: *expression, source, checked, input, output: ScopedRoot { ty: child, scope: input.scope } }, child)
                }
                _ => return None,
            };
            selected_type = ScopedRoot { ty: child, scope: input.scope };
            solved.graph.validate_scoped(selected_type).ok()?;
            steps.push(step);
        }
        let value_source = self.expression_identity(value_source);
        if match value {
            BuildMutablePathValue::Value(value) => self.expression_origins.get(&value) != Some(&value_source),
            BuildMutablePathValue::Integer(value) => scratch.int_expression_origins.get(&value) != Some(&value_source),
        } { return None; }
        let value_type = ScopedRoot { ty: *solved.expressions.get(&value_source)?, scope: solved.expression_scope(value_source, owner).ok()? };
        solved.graph.validate_scoped(value_type).ok()?;
        if solved.expression_owners.get(&value_source).copied() != owner { return None; }
        let compound = if op == AssignOp::Set { None } else { Some(solved.statement_operations.get(&self.statement_identity(statement))?.clone()) };
        let receipt = BuildMutablePathWrite { binding, statement: self.statement_identity(statement), target, slot, op, value, value_source, value_type, selected_type, check, compound, steps: steps.into_boxed_slice() };
        drop(scratch);
        self.scratch.borrow_mut().mutable_path_writes.insert(row, receipt);
        Some(())
    }
}
