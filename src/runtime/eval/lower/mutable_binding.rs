use super::*;
use crate::sema::check::{BindingIdentity, ExpressionIdentity, StatementIdentity};
use crate::sema::inference::ScopedRoot;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) enum BuildMutableStatement {
    Value { slot: usize, value: BuildExprId, assignment: Option<AssignOp>, check: Option<Type> },
    Integer { slot: usize, value: BuildIntId, assignment: Option<AssignOp> },
    Boolean { slot: usize, value: BuildBoolId, assignment: bool },
}
impl BuildMutableStatement {
    pub(in crate::runtime::eval) fn from_row(row: &BuildStmtRow) -> Option<Self> {
        Some(match row {
            BuildStmtRow::Let { slot, value } => Self::Value { slot: *slot, value: *value, assignment: None, check: None },
            BuildStmtRow::LetInt { slot, value } => Self::Integer { slot: *slot, value: *value, assignment: None },
            BuildStmtRow::LetBool { slot, value } => Self::Boolean { slot: *slot, value: *value, assignment: false },
            BuildStmtRow::Assign { slot, value, op, check, .. } => Self::Value { slot: *slot, value: *value, assignment: Some(*op), check: check.as_ref().map(|check| check.ty.clone()) },
            BuildStmtRow::AssignInt { slot, value, op, .. } => Self::Integer { slot: *slot, value: *value, assignment: Some(*op) },
            BuildStmtRow::AssignBool { slot, value } => Self::Boolean { slot: *slot, value: *value, assignment: true },
            _ => return None,
        })
    }
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct BuildMutableBindingOrigin {
    pub binding: BindingIdentity,
    pub statement: StatementIdentity,
    pub row: BuildStmtId,
    write_count: u32,
    pub slot: usize,
    pub source_type: ScopedRoot,
    pub value_source: ExpressionIdentity,
    pub value_type: ScopedRoot,
    pub emitted: BuildMutableStatement,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct BuildMutableCaptureWrite {
    pub caller: crate::sema::check::DeclarationIdentity,
    pub definition_owner: Option<crate::sema::check::DeclarationIdentity>,
    pub binding_root: ScopedRoot,
    pub slot: usize,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct BuildMutableBindingWrite {
    pub binding: BindingIdentity,
    pub statement: StatementIdentity,
    pub value_source: ExpressionIdentity,
    pub value_type: ScopedRoot,
    pub emitted: BuildMutableStatement,
    pub capture: Option<BuildMutableCaptureWrite>,
    pub compound: Option<crate::sema::check::SolvedOperation>,
    pub ordinal: u32,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct BuildMutableDriverBinding {
    pub binding: BindingIdentity,
    pub statement: StatementIdentity,
    pub row: BuildTopStmtId,
    pub name: Name,
    pub source_type: ScopedRoot,
    pub value: BuildExprId,
    pub value_source: ExpressionIdentity,
    pub value_type: ScopedRoot,
    pub write_count: u32,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct BuildMutableDriverWrite {
    pub binding: BindingIdentity,
    pub statement: StatementIdentity,
    pub value: BuildExprId,
    pub value_source: ExpressionIdentity,
    pub value_type: ScopedRoot,
    pub assignment: AssignOp,
    pub compound: Option<crate::sema::check::SolvedOperation>,
    pub ordinal: u32,
}

pub(in crate::runtime::eval) fn supports_mutable_binding_type(ty: &Type) -> bool {
    matches!(ty, Type::Int | Type::UInt | Type::Bool | Type::Str | Type::Float) || super::mutable_path::inert_mutable_aggregate(ty)
}

pub(in crate::runtime::eval) fn supports_mutable_driver_binding_type(ty: &Type) -> bool {
    supports_mutable_binding_type(ty)
}

impl CompactLowerConstructProbe<'_, '_> {
    pub(super) fn seed_original_mutable_capture(&self, name: Name, slot: usize, binding: BindingIdentity, source_type: ScopedRoot, slots: &mut SlotScope) -> Option<()> {
        let known = self.top_level_known.get(&name)?;
        if !known.mutable || known.lexical_binding != Some(binding) || !known.source_type.is_some_and(|root| root.ty == source_type.ty && root.scope == source_type.scope)
            || !slots.captures.contains(&name) || slots.resolve(name) != Some(slot) { return None; }
        let solved = self.solved();
        let definition = solved.bindings.get(&binding)?;
        let scope = definition.scheme.or_else(|| definition.owner.and_then(|owner| solved.declarations.get(&owner).map(|definition| definition.scheme)));
        if !definition.mutable || definition.ty != source_type.ty || scope != source_type.scope { return None; }
        solved.graph.validate_scoped(source_type).ok()?;
        let Ok(ground) = super::super::indexed::generic::graph_ground_type(&solved.graph, source_type.ty) else { return Some(()); };
        if supports_mutable_binding_type(&ground) { slots.mutable_binding_authorities.insert(name, binding); }
        Some(())
    }

    pub(super) fn record_original_mutable_refinement_guard(&mut self, statement: StmtId, row: BuildStmtId) -> Option<()> {
        let guard = self.statement_identity(statement);
        if self.solved().refined_reads.values().any(|read| read.guard == guard) {
            self.scratch.borrow_mut().mutable_refinement_guards.insert(row, guard);
        }
        Some(())
    }

    pub(super) fn record_original_mutable_driver_statement(&mut self, statement: StmtId, row: BuildTopStmtId, known: &FxHashMap<Name, LoweredTopLevelBinding>) -> Option<()> {
        let origin = self.statement_identity(statement);
        let solved = self.solved();
        match self.program.arena.stmt(statement).kind {
            ArenaStmtKind::Export(inner) => return self.record_original_mutable_driver_statement(inner, row, known),
            ArenaStmtKind::Var { target, initializer: ArenaExprOrRun::Expr(value), .. } => {
                let ArenaBindingTargetKind::Name(name) = self.program.arena.binding_target(target).kind else { return Some(()); };
                let binding = BindingIdentity { source: origin.source, namespace: origin.namespace, target };
                let definition = solved.bindings.get(&binding)?;
                let source_type = ScopedRoot { ty: definition.ty, scope: definition.scheme };
                solved.graph.validate_scoped(source_type).ok()?;
                let ground = super::super::indexed::generic::graph_ground_type(&solved.graph, definition.ty).ok()?;
                if !supports_mutable_driver_binding_type(&ground) { return Some(()); }
                if !definition.mutable || definition.owner.is_some() { return None; }
                let value_source = self.expression_identity(value);
                let value_type = ScopedRoot { ty: *solved.expressions.get(&value_source)?, scope: solved.expression_scope(value_source, None).ok()? };
                solved.graph.validate_scoped(value_type).ok()?;
                let scratch = self.scratch.borrow();
                let BuildTopKind::Let { target, mutable: true, value, .. } = &scratch.top_statements.get(row.index())?.kind else { return None; };
                if *target != name { return None; }
                let original = BuildMutableDriverBinding { binding, statement: origin, row, name, source_type, value: *value, value_source, value_type, write_count: 0 };
                drop(scratch);
                self.scratch.borrow_mut().mutable_driver_bindings.insert(binding, original);
            }
            ArenaStmtKind::Assign { target, op, value: ArenaExprOrRun::Expr(value), .. } => {
                let ArenaAssignTargetKind::Name(name) = self.program.arena.assign_target(target).kind else { return Some(()); };
                let Some(binding) = known.get(&name).filter(|binding| binding.mutable).and_then(|binding| binding.lexical_binding) else { return Some(()); };
                if !self.scratch.borrow().mutable_driver_bindings.contains_key(&binding) { return Some(()); }
                let value_source = self.expression_identity(value);
                let value_type = ScopedRoot { ty: *solved.expressions.get(&value_source)?, scope: solved.expression_scope(value_source, None).ok()? };
                solved.graph.validate_scoped(value_type).ok()?;
                let compound = if op == AssignOp::Set { None } else { Some(solved.statement_operations.get(&origin)?.clone()) };
                let mut scratch = self.scratch.borrow_mut();
                if matches!(scratch.top_statements.get(row.index())?.kind, BuildTopKind::Stmt(_)) { return Some(()); }
                let BuildTopKind::Assign { target, op: actual, value, .. } = &scratch.top_statements.get(row.index())?.kind else { return None; };
                if *target != name || *actual != op { return None; }
                let value = *value;
                let original = scratch.mutable_driver_bindings.get_mut(&binding)?;
                original.write_count = original.write_count.checked_add(1)?;
                let ordinal = original.write_count;
                scratch.mutable_driver_writes.insert(row, BuildMutableDriverWrite { binding, statement: origin, value, value_source, value_type, assignment: op, compound, ordinal });
            }
            _ => {}
        }
        Some(())
    }

    pub(super) fn record_original_mutable_statement(&mut self, statement: StmtId, row: BuildStmtId, slots: &mut SlotScope) -> Option<()> {
        let origin = self.statement_identity(statement);
        match self.program.arena.stmt(statement).kind {
            ArenaStmtKind::Var { target, initializer: ArenaExprOrRun::Expr(value), .. } => {
                let ArenaBindingTargetKind::Name(name) = self.program.arena.binding_target(target).kind else { return Some(()); };
                let binding = BindingIdentity { source: origin.source, namespace: origin.namespace, target };
                let solved = self.solved();
                let definition = solved.bindings.get(&binding)?;
                let scope = definition.scheme.or_else(|| definition.owner.and_then(|owner| solved.declarations.get(&owner).map(|declaration| declaration.scheme)));
                let source_type = ScopedRoot { ty: definition.ty, scope };
                solved.graph.validate_scoped(source_type).ok()?;
                let Ok(ground) = super::super::indexed::generic::graph_ground_type(&solved.graph, definition.ty) else { return Some(()); };
                if !supports_mutable_binding_type(&ground) { return Some(()); }
                if !definition.mutable { return None; }
                let slot = slots.resolve(name)?;
                let value_source = self.expression_identity(value);
                let value_type = ScopedRoot { ty: *solved.expressions.get(&value_source)?, scope: solved.expression_scope(value_source, definition.owner).ok()? };
                solved.graph.validate_scoped(value_type).ok()?;
                let emitted = BuildMutableStatement::from_row(self.scratch.borrow().statements.get(row.index())?)?;
                self.scratch.borrow_mut().mutable_binding_origins.insert(binding, BuildMutableBindingOrigin { binding, statement: origin, row, write_count: 0, slot, source_type, value_source, value_type, emitted });
                slots.mutable_binding_authorities.insert(name, binding);
            }
            ArenaStmtKind::Assign { target, op, value: ArenaExprOrRun::Expr(value), .. } => {
                let ArenaAssignTargetKind::Name(name) = self.program.arena.assign_target(target).kind else { return self.record_original_mutable_path(statement, row, slots); };
                let Some(&binding) = slots.mutable_binding_authorities.get(&name) else { return Some(()); };
                let solved = self.solved();
                let definition = solved.bindings.get(&binding)?;
                let value_source = self.expression_identity(value);
                let value_type = ScopedRoot { ty: *solved.expressions.get(&value_source)?, scope: solved.expression_scope(value_source, definition.owner).ok()? };
                solved.graph.validate_scoped(value_type).ok()?;
                let compound = if op == AssignOp::Set { None } else { Some(solved.statement_operations.get(&origin)?.clone()) };
                let mut scratch = self.scratch.borrow_mut();
                let ordinal = if let Some(original) = scratch.mutable_binding_origins.get_mut(&binding) {
                    original.write_count = original.write_count.checked_add(1)?; original.write_count
                } else {
                    let original = scratch.mutable_driver_bindings.get_mut(&binding)?;
                    original.write_count = original.write_count.checked_add(1)?; original.write_count
                };
                let emitted = BuildMutableStatement::from_row(scratch.statements.get(row.index())?)?;
                scratch.mutable_binding_writes.insert(row, BuildMutableBindingWrite { binding, statement: origin, value_source, value_type, ordinal, emitted, capture: None, compound });
            }
            _ => {}
        }
        Some(())
    }
}
