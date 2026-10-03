use super::*;
use crate::syntax::arena::{ArenaExprKind, ArenaStmtKind, ExprId};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub(super) struct FieldPresenceBranch {
    binding: BindingIdentity,
    name: Name,
    predicate: ExpressionIdentity,
    subject: ExpressionIdentity,
    key: ExpressionIdentity,
    field: Name,
    control: StatementIdentity,
    branch: u32,
    caller: Option<DeclarationIdentity>,
    material: crate::sema::inference::TypeId,
    subject_type: crate::sema::inference::TypeId,
    stamp: proof::Narrowing,
}

impl Checker {
    pub(super) fn push_checked_field_presence_branch(&mut self, arena: &ArenaProgram, condition: ExprId, ordinal: u32) -> usize {
        let saved = self.field_presence_branches.len();
        if let Some(branch) = self.checked_field_presence_branch(arena, condition, ordinal) {
            self.field_presence_branches.push(branch);
        }
        saved
    }

    pub(super) fn restore_checked_field_presence_branches(&mut self, saved: usize) {
        self.field_presence_branches.truncate(saved);
    }

    fn checked_field_presence_branch(&self, arena: &ArenaProgram, condition: ExprId, ordinal: u32) -> Option<FieldPresenceBranch> {
        if !self.graph_generation { return None; }
        let statement = self.current_statement?;
        let ArenaStmtKind::If { branches, .. } = arena.arena.stmt(statement).kind else { return None; };
        if arena.arena.if_branches(branches).get(ordinal as usize)?.condition != condition { return None; }
        let ArenaExprKind::Binary { op: crate::syntax::node::BinaryOp::In, left, right } = arena.arena.expr(condition).kind else { return None; };
        let ArenaExprKind::Str(literal) = arena.arena.expr(left).kind else { return None; };
        let ArenaExprKind::Ident(name) = arena.arena.expr(right).kind else { return None; };
        let field = Name::intern(arena.arena.string_literal(literal));
        let lexical = self.lookup(name)?;
        if lexical.mutable { return None; }
        let binding = lexical.original_binding?;
        let predicate = self.expression_identity(arena, condition);
        let subject = self.expression_identity(arena, right);
        let key = self.expression_identity(arena, left);
        let state = self.generic.borrow();
        let solved = &state.facts;
        let original = solved.bindings.get(&binding)?;
        let material = original.ty;
        let subject_type = *solved.expressions.get(&subject)?;
        if original.mutable || original.owner != self.current_generic
            || solved.expression_owners.get(&subject).copied() != self.current_generic { return None; }
        let crate::sema::inference::TypeNode::Record(material_row) = solved.graph.node(solved.graph.resolved(material).ok()?).ok()? else { return None; };
        let material_fields = solved.graph.row_data(*material_row).ok()?;
        let crate::sema::inference::TypeNode::Record(subject_row) = solved.graph.node(solved.graph.resolved(subject_type).ok()?).ok()? else { return None; };
        let subject_fields = solved.graph.row_data(*subject_row).ok()?;
        if material_fields.tail.is_some() || subject_fields.tail.is_some()
            || material_fields.fields.len() != subject_fields.fields.len()
            || material_fields.fields.iter().any(|original| subject_fields.fields.iter().find(|field| field.label == original.label)
                .is_none_or(|field| solved.graph.resolved(field.ty).ok() != solved.graph.resolved(original.ty).ok()))
            || material_fields.fields.iter().any(|original| original.label == field) { return None; }
        // Candidate selection can finish after the branch is checked. Keep the
        // actual predicate and roots so its consumer can authenticate that choice.
        let operation = solved.operations.get(&predicate)?;
        if operation.caller != self.current_generic || operation.receiver.is_none()
            || operation.actual_arguments.len() != 1 || !solved.expressions.contains_key(&key) { return None; }
        let stamp = lexical.proof.fact(name, Vec::new(), lexical.ty.clone());
        Some(FieldPresenceBranch {
            binding, name, predicate, subject, key, field,
            control: StatementIdentity { source: arena.arena.stmt(statement).span.source_id, namespace: self.current_namespace, statement },
            branch: ordinal, caller: self.current_generic, material, subject_type, stamp,
        })
    }

    pub(super) fn record_checked_field_presence_read(&mut self, arena: &ArenaProgram, read: ExprId) {
        if !self.graph_generation { return; }
        let ArenaExprKind::Ident(name) = arena.arena.expr(read).kind else { return; };
        let Some(lexical) = self.lookup(name).cloned() else { return; };
        let Some(branch) = self.field_presence_branches.iter().rev().find(|branch| {
            branch.name == name && lexical.original_binding == Some(branch.binding) && lexical.proof.accepts(&branch.stamp)
        }).cloned() else { return; };
        let Some(writes) = lexical.proof.source_writes(&branch.stamp) else { return; };
        if lexical.mutable || !writes.is_empty() { return; }
        let identity = self.expression_identity(arena, read);
        let record = {
            let state = self.generic.borrow();
            let solved = &state.facts;
            let Some(&narrowed) = solved.expressions.get(&identity) else { return; };
            if solved.expression_owners.get(&identity).copied() != branch.caller { return; }
            let Ok(material_root) = solved.graph.resolved(branch.material) else { return; };
            let Ok(crate::sema::inference::TypeNode::Record(material_row)) = solved.graph.node(material_root) else { return; };
            let Ok(material) = solved.graph.row_data(*material_row) else { return; };
            let Ok(narrowed_root) = solved.graph.resolved(narrowed) else { return; };
            let Ok(crate::sema::inference::TypeNode::Record(narrowed_row)) = solved.graph.node(narrowed_root) else { return; };
            let Ok(narrowed_fields) = solved.graph.row_data(*narrowed_row) else { return; };
            if material.tail.is_some() || narrowed_fields.tail.is_some() || narrowed_fields.fields.len() != material.fields.len() + 1
                || material.fields.iter().any(|original| narrowed_fields.fields.iter().find(|field| field.label == original.label)
                    .is_none_or(|field| solved.graph.resolved(field.ty).ok() != solved.graph.resolved(original.ty).ok()))
                || narrowed_fields.fields.iter().find(|field| field.label == branch.field).is_none_or(|field| {
                    !matches!(solved.graph.resolved(field.ty).ok().and_then(|root| solved.graph.node(root).ok()), Some(crate::sema::inference::TypeNode::Atom(crate::sema::inference::Atom::Any)))
                }) { return; }
            let material_scope = solved.bindings.get(&branch.binding).and_then(|binding| binding.scheme);
            let subject_scope = solved.expression_scope(branch.subject, branch.caller).ok().flatten();
            let narrowed_scope = solved.expression_scope(identity, branch.caller).ok().flatten();
            SolvedFieldPresenceRead {
                read: identity, binding: branch.binding, predicate: branch.predicate, subject: branch.subject, key: branch.key, field: branch.field,
                control: branch.control, branch: branch.branch, caller: branch.caller,
                material: crate::sema::inference::ScopedRoot { ty: branch.material, scope: material_scope },
                subject_type: crate::sema::inference::ScopedRoot { ty: branch.subject_type, scope: subject_scope },
                narrowed: crate::sema::inference::ScopedRoot { ty: narrowed, scope: narrowed_scope },
                revision: branch.stamp.revision(), writes,
            }
        };
        let result = {
            let mut state = self.generic.borrow_mut();
            state.facts.graph.charge_source_fact_nodes(1)
                .and_then(|_| state.facts.graph.charge_source_fact_edges(12))
                .and_then(|_| state.facts.graph.charge_source_fact_work(12))
                .map(|_| { state.facts.field_presence_reads.insert(identity, Arc::new(record)); })
        };
        if let Err(error) = result { self.graph_error(arena.arena.expr(read).span, error); }
    }
}
