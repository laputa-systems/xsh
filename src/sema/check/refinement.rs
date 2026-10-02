use super::*;
use crate::syntax::arena::{ArenaExprKind, ArenaBindingTargetKind, ArenaExprOrRun, ExprId};
use std::sync::Arc;

impl Checker {
    pub(super) fn null_predicate_source(&self, arena: &ArenaProgram, condition: ExprId, subject: ExprId, name: Name) -> Option<proof::PredicateSource> {
        let binding = self.lookup(name)?.original_binding?;
        Some(proof::PredicateSource {
            binding, predicate: self.expression_identity(arena, condition), subject: self.expression_identity(arena, subject),
            nonnull_when_true: matches!(arena.arena.expr(condition).kind, ArenaExprKind::Binary { op: crate::syntax::node::BinaryOp::Ne, .. }),
            aliases: Vec::new(), guard: None,
        })
    }

    pub(super) fn boolean_alias_source(&self, arena: &ArenaProgram, target: crate::syntax::arena::BindingTargetId, initializer: ArenaExprOrRun, proof: Option<Arc<proof::ConditionNarrowings>>) -> Option<Arc<proof::ConditionNarrowings>> {
        let mut proof = proof?;
        let ArenaBindingTargetKind::Name(_) = arena.arena.binding_target(target).kind else { return Some(proof); };
        let ArenaExprOrRun::Expr(expression) = initializer else { return Some(proof); };
        let Some(statement) = self.current_statement else { return Some(proof); };
        let alias = SolvedRefinementAlias {
            binding: BindingIdentity { source: arena.arena.expr(expression).span.source_id, namespace: self.current_namespace, target },
            statement: StatementIdentity { source: arena.arena.stmt(statement).span.source_id, namespace: self.current_namespace, statement },
            initializer: self.expression_identity(arena, expression),
        };
        let facts = Arc::make_mut(&mut proof);
        for fact in facts.when_true.iter_mut().chain(facts.when_false.iter_mut()) {
            if let Some(source) = &mut fact.source {
                if source.aliases.len() < 128 { Arc::make_mut(source).aliases.push(alias.clone()); }
                else { fact.source = None; }
            }
        }
        Some(proof)
    }

    pub(super) fn apply_exiting_guard_refinements(&mut self, arena: &ArenaProgram, statement: crate::syntax::arena::StmtId, condition: ExprId, facts: &[proof::Narrowing]) {
        let guard = StatementIdentity { source: arena.arena.stmt(statement).span.source_id, namespace: self.current_namespace, statement };
        let condition = self.expression_identity(arena, condition);
        let mut facts = facts.to_vec();
        for fact in &mut facts {
            if let Some(source) = &mut fact.source { Arc::make_mut(source).guard = Some((guard, condition)); }
        }
        self.apply_narrowings(&facts);
    }

    pub(super) fn record_checked_refined_read(&mut self, arena: &ArenaProgram, read: ExprId) {
        if !self.graph_generation { return; }
        let Some((name, read_path, _)) = self.proof_subject_arena(arena, read) else { return; };
        let Some(binding) = self.lookup(name).cloned() else { return; };
        let Some(binding_identity) = binding.original_binding else { return; };
        let identity = self.expression_identity(arena, read);
        let (read_type, scope, invariant) = {
            let state = self.generic.borrow();
            let Some(&ty) = state.facts.expressions.get(&identity) else { return; };
            let scope = state.facts.expression_scope(identity, self.current_generic).ok().flatten();
            let Some(binding) = state.facts.bindings.get(&binding_identity) else { return; };
            let invariant_scope = binding.scheme.or(binding.owner.and_then(|owner| state.facts.declarations.get(&owner).map(|declaration| declaration.scheme)));
            (ty, scope, crate::sema::inference::ScopedRoot { ty: binding.ty, scope: invariant_scope })
        };
        let mut candidates = binding.refinements.iter().filter_map(|fact| {
            let source = fact.source.as_ref()?;
            let (guard, guard_condition) = source.guard?;
            if source.binding != binding_identity || !fact.path.starts_with(&read_path) || !binding.proof.accepts(fact) { return None; }
            let writes = binding.proof.source_writes(fact)?;
            Some(SolvedRefinedRead {
                binding: binding_identity, predicate: source.predicate, subject: source.subject, predicate_nonnull_when_true: source.nonnull_when_true,
                path: Arc::clone(&fact.path), read_path: Arc::from(read_path.clone()), aliases: source.aliases.clone(), guard, guard_condition,
                caller: self.current_generic, invariant, narrowed: crate::sema::inference::ScopedRoot { ty: read_type, scope }, revision: fact.revision(), writes,
            })
        });
        let Some(record) = candidates.next() else { return; };
        if candidates.next().is_some() { return; }
        let result = {
            let mut state = self.generic.borrow_mut();
            let edges = (record.aliases.len() * 3 + record.writes.len() * 2 + record.path.len() + record.read_path.len() + 8) as u64;
            state.facts.graph.charge_source_fact_nodes(1)
                .and_then(|_| state.facts.graph.charge_source_fact_edges(edges))
                .and_then(|_| state.facts.graph.charge_source_fact_work(edges))
                .map(|_| { state.facts.refined_reads.insert(identity, Arc::new(record)); })
        };
        if let Err(error) = result { self.graph_error(arena.arena.expr(read).span, error); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceId;
    use crate::syntax::parser::Parser;

    #[test]
    fn checked_null_guard_reads_keep_original_aliases_and_disjoint_writes_after_frontend_drop() {
        let source = include_str!("../../../tests/fixtures/frontend-indexed/proof-provenance.xsh");
        let parsed = Parser::parse_source_arena_only(SourceId::new(96), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let read = *checked.solved.refined_reads.keys().find(|identity| {
            let span = parsed.arena.arena.expr(identity.expression).span;
            &source[span.range()] == "report" && span.range().start > source.find("guard retained").unwrap()
        }).expect("guard continuation keeps its original mutable storage read");
        let _symbols = checked.solved.symbol_owner().enter();
        let receipt = checked.solved.checked_refined_read(read).unwrap();
        assert_eq!(receipt.aliases.len(), 2);
        assert_eq!(receipt.path.as_ref(), &[Name::intern("inner"), Name::intern("value")]);
        assert!(receipt.read_path.is_empty());
        assert_eq!(receipt.writes.len(), 1);
        assert_eq!(receipt.writes[0].path.as_ref(), &[Name::intern("inner"), Name::intern("count")]);
        assert!(receipt.predicate_nonnull_when_true);
        assert_eq!(&source[parsed.arena.arena.expr(receipt.predicate.expression).span.range()], "report.inner.value != null");
        assert_eq!(&source[parsed.arena.arena.expr(receipt.subject.expression).span.range()], "report.inner.value");
        assert_eq!(&source[parsed.arena.arena.expr(receipt.guard_condition.expression).span.range()], "retained");
        assert_ne!(checked.solved.bindings[&receipt.binding].ty, receipt.narrowed.ty);
        drop(parsed);
        checked.solved.validate().unwrap();
        let mut solved = Arc::try_unwrap(checked.solved).unwrap();
        let original = solved.refined_reads.remove(&read).unwrap();
        assert!(solved.validate().is_err());
        for corruption in 0..5 {
            solved.refined_reads.insert(read, Arc::clone(&original));
            let record = Arc::make_mut(solved.refined_reads.get_mut(&read).unwrap());
            match corruption {
                0 => record.guard_condition.namespace = Some(Name::intern("foreign")),
                1 => record.predicate = record.subject,
                2 => record.aliases.clear(),
                3 => record.path = Arc::from([Name::intern("inner"), Name::intern("count")]),
                _ => record.writes.clear(),
            }
            assert!(solved.validate().is_err(), "altered original guard relationship {corruption}");
        }
        solved.refined_reads.insert(read, original);
        solved.validate().unwrap();
    }
}
