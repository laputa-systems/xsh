use super::*;
use crate::sema::inference::{ConstraintRelation, InferenceContext, InferenceError, RequirementId, ScopedRoot, TypeId, TypeNode};

/// Constant field selection refines the producer while preserving the selected
/// registry method's fallible, erased result contract.
#[derive(Clone, Debug)]
pub(crate) struct SolvedRecordGetProjection {
    pub receiver: ExpressionIdentity,
    pub key: ExpressionIdentity,
    pub key_type: TypeId,
    pub requirement: RequirementId,
    pub contribution: usize,
    pub field: Name,
    pub receiver_type: TypeId,
    pub field_type: TypeId,
    pub registry_result: TypeId,
    pub producer_result: TypeId,
    pub caller: Option<DeclarationIdentity>,
}

impl Checker {
    pub(super) fn graph_record_get_projection(&mut self, arena: &ArenaProgram,
        projection: &CheckedProjection, receiver_type: &Type, span: Span,
    ) -> Type {
        let Some(expression) = self.current_expression else { return Type::Invalid; };
        let identity = self.expression_identity(arena, expression);
        let existing = self.generic.borrow().facts.record_get_projection(identity)
            .map(|fact| fact.map(|fact| fact.producer_result));
        match existing {
            Ok(Some(result)) => return self.graph_view(result),
            Err(error) => { self.graph_error(span, error); return Type::Invalid; }
            Ok(None) => {}
        }
        if !self.graph_generation {
            return Type::Invalid;
        }
        let receiver = self.expression_identity(arena, projection.receiver);
        let key = self.expression_identity(arena, projection.key);
        let source_type = self.generic.borrow().facts.expressions.get(&receiver).copied();
        let source_type = match source_type {
            Some(source_type) => source_type,
            None => match self.graph_type(receiver_type, span) {
                Ok(source_type) => source_type,
                Err(error) => { self.graph_error(span, error); return Type::Invalid; }
            },
        };
        let outcome = (|| {
            let mut state = self.generic.borrow_mut();
            let operation = state.facts.operations.get(&identity).cloned().ok_or(InferenceError::InvalidScheme)?;
            let evidence = state.facts.graph.candidate_evidence(operation.requirement)?
                .ok_or(InferenceError::InvalidScheme)?;
            let metadata = state.registry.metadata(&state.facts.graph, evidence.candidate)?;
            if metadata.owner != crate::sema::registry_graph::RegistryOwner::Method(MethodReceiver::Record)
                || metadata.operation != crate::modules::RuntimeOp::RecordGet
                || metadata.semantic_rule != crate::modules::signature::SemanticRule::ConstantKeyProjection {
                return Err(InferenceError::InvalidScheme);
            }
            let registry_result = evidence.result;
            let key_type = *state.facts.expressions.get(&key).ok_or(InferenceError::InvalidScheme)?;
            let TypeNode::Result(_, error) = state.facts.graph.node(state.facts.graph.resolved(registry_result)?)? else {
                return Err(InferenceError::InvalidScheme);
            };
            let error = *error;
            let reason = state.facts.graph.reason(span, None)?;
            let contribution = state.facts.graph.constraint_origins().len();
            let field_type = state.facts.graph.require_field(source_type, projection.field,
                if self.current_generic.is_some() { 1 } else { 0 }, reason)?;
            let producer_result = state.facts.graph.result(field_type, error)?;
            state.facts.graph.charge_source_fact_nodes(1)?;
            state.facts.graph.charge_source_fact_edges(8)?;
            state.facts.graph.charge_source_fact_work(1)?;
            state.facts.graph.solve()?;
            state.facts.expressions.insert(receiver, source_type);
            state.facts.expressions.insert(identity, producer_result);
            if let Some(owner) = self.current_generic {
                state.facts.expression_owners.insert(receiver, owner);
                state.facts.expression_owners.insert(identity, owner);
            }
            let fact = Arc::new(SolvedRecordGetProjection {
                receiver, key, key_type, requirement: operation.requirement, contribution,
                field: projection.field, receiver_type: source_type,
                field_type, registry_result, producer_result, caller: self.current_generic,
            });
            state.facts.record_get_projections.insert(identity, Arc::clone(&fact));
            state.facts.original_record_get_projections.insert(identity, fact);
            Ok::<_, InferenceError>(producer_result)
        })();
        match outcome {
            Ok(result) => self.graph_view(result),
            Err(error) => { self.graph_error(span, error); Type::Invalid }
        }
    }
}

impl<Graph> SolvedTypes<Graph> {
    pub(crate) fn record_get_projection(&self, identity: ExpressionIdentity) -> Result<Option<&SolvedRecordGetProjection>, InferenceError> {
        match (self.record_get_projections.get(&identity), self.original_record_get_projections.get(&identity)) {
            (None, None) => Ok(None),
            (Some(fact), Some(original)) if Arc::ptr_eq(fact, original) => Ok(Some(fact)),
            _ => Err(InferenceError::Boundary("record get refinement differs from its original source receipt")),
        }
    }

    pub(super) fn record_get_projection_roots(&self, graph: &InferenceContext) -> Result<Vec<ScopedRoot>, InferenceError> {
        self.validate_record_get_projections(graph)?;
        let mut roots = Vec::with_capacity(self.record_get_projections.len() * 5);
        for (identity, fact) in &self.record_get_projections {
            let scope = self.expression_scope(*identity, fact.caller)?;
            roots.extend([ScopedRoot { ty: fact.receiver_type, scope: self.expression_scope(fact.receiver, fact.caller)? },
                ScopedRoot { ty: fact.key_type, scope: self.expression_scope(fact.key, fact.caller)? },
                ScopedRoot { ty: fact.field_type, scope }, ScopedRoot { ty: fact.registry_result, scope },
                ScopedRoot { ty: fact.producer_result, scope }]);
        }
        Ok(roots)
    }

    pub(super) fn validate_record_get_projections(&self, graph: &InferenceContext) -> Result<(), InferenceError> {
        if self.record_get_projections.len() != self.original_record_get_projections.len() { return Err(InferenceError::Boundary("record get refinement source ledger is incomplete")); }
        for (&identity, fact) in &self.record_get_projections {
            self.record_get_projection(identity)?;
            let operation = self.operations.get(&identity).ok_or(InferenceError::InvalidScheme)?;
            if operation.requirement != fact.requirement || operation.caller != fact.caller
                || self.expression_owners.get(&identity).copied() != fact.caller
                || self.expression_owners.get(&fact.receiver).copied() != fact.caller
                || self.expression_owners.get(&fact.key).copied() != fact.caller
                || identity.source != fact.receiver.source || identity.namespace != fact.receiver.namespace
                || identity.source != fact.key.source || identity.namespace != fact.key.namespace {
                return Err(InferenceError::Boundary("record get refinement changes its original source owner or requirement"));
            }
            for (expression, ty) in [(identity, fact.producer_result), (fact.receiver, fact.receiver_type), (fact.key, fact.key_type)] {
                let original = *self.expressions.get(&expression).ok_or(InferenceError::InvalidScheme)?;
                if graph.resolved(original)? != graph.resolved(ty)? { return Err(InferenceError::Boundary("record get refinement changes its original expression root")); }
            }
            let evidence = graph.candidate_evidence(fact.requirement)?.ok_or(InferenceError::InvalidScheme)?;
            if graph.resolved(evidence.result)? != graph.resolved(fact.registry_result)? {
                return Err(InferenceError::Boundary("record get refinement changes its original selected result root"));
            }
            if graph.resolved(operation.result)? != graph.resolved(fact.registry_result)? {
                return Err(InferenceError::Boundary("record get refinement operation result differs from its selected result root"));
            }
            if graph.export_type(fact.key_type)? != Type::Str {
                return Err(InferenceError::Boundary("record get refinement key is not a string"));
            }
            let origin = graph.constraint_origins().get(fact.contribution).ok_or(InferenceError::InvalidScheme)?;
            if origin.relation != (ConstraintRelation::Projection { record: fact.receiver_type, label: fact.field, result: fact.field_type }) {
                return Err(InferenceError::Boundary("record get refinement changes its original field constraint"));
            }
            let TypeNode::Result(_, original_error) = graph.node(graph.resolved(fact.registry_result)?)? else { return Err(InferenceError::InvalidScheme); };
            let TypeNode::Result(success, error) = graph.node(graph.resolved(fact.producer_result)?)? else { return Err(InferenceError::InvalidScheme); };
            if graph.resolved(*success)? != graph.resolved(fact.field_type)? || graph.resolved(*error)? != graph.resolved(*original_error)? {
                return Err(InferenceError::Boundary("record get refinement changes its original Result producer relationship"));
            }
        }
        Ok(())
    }

    pub(super) fn record_get_projection_payload_bytes(&self) -> usize {
        self.record_get_projections.len() * (std::mem::size_of::<SolvedRecordGetProjection>() + 4 * std::mem::size_of::<usize>())
    }

    pub(super) fn record_get_projection_work(&self) -> usize { self.record_get_projections.len() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_get_projection_refuses_same_typed_field_and_foreign_requirement_rewrites() {
        let source = "type Config = {workers: Int, limit: Int}\npure workers(config: Config) -> Int { config.get(\"workers\") ?? 0 }\npure limit(config: Config) -> Int { config.get(\"limit\") ?? 0 }\n";
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(73), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let _symbols = parsed.arena.symbol_owner().enter();
        let mut checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let solved = Arc::get_mut(&mut checked.solved).unwrap();
        assert_eq!(solved.record_get_projections.len(), 2);
        solved.validate_record_get_projections(&solved.graph).unwrap();
        let identities = solved.record_get_projections.keys().copied().collect::<Vec<_>>();
        let identity = identities[0];
        let original = Arc::clone(&solved.record_get_projections[&identity]);
        Arc::make_mut(solved.record_get_projections.get_mut(&identity).unwrap()).field = Name::intern("limit");
        assert!(solved.validate_record_get_projections(&solved.graph).is_err());
        solved.record_get_projections.insert(identity, Arc::clone(&original));
        let original_requirement = solved.operations[&identity].requirement;
        solved.operations.get_mut(&identity).unwrap().requirement = solved.record_get_projections[&identities[1]].requirement;
        assert!(solved.validate_record_get_projections(&solved.graph).is_err());
        solved.operations.get_mut(&identity).unwrap().requirement = original_requirement;
        solved.record_get_projections.remove(&identity);
        assert!(solved.validate_record_get_projections(&solved.graph).is_err());
        solved.record_get_projections.insert(identity, original);
        solved.validate_record_get_projections(&solved.graph).unwrap();
    }
}
