use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ErrorJoinOutcome { Identity(TypeId), Atom(Atom) }

impl InferenceContext {
    pub fn error_join(&self, id: ErrorJoinId) -> Result<&ErrorJoin, InferenceError> {
        slot(&self.error_joins, id.index(), id.generation)
    }

    pub fn require_error_join(&mut self, join: ErrorJoin, reason: ReasonId) -> Result<RequirementId, InferenceError> {
        self.probe(|graph| {
            let join = graph.store_error_join(join)?;
            graph.new_requirement(RequirementTemplate::ErrorJoin { join }, reason)
        })
    }

    pub(super) fn store_error_join(&mut self, join: ErrorJoin) -> Result<ErrorJoinId, InferenceError> {
        if join.inputs.is_empty() { return Err(InferenceError::Boundary("an error join requires a reached failure contribution")); }
        self.work_many(join.inputs.len())?;
        for input in &join.inputs { self.value_type(*input)?; }
        self.value_type(join.result)?;
        if let Some(bound) = join.bound { self.value_type(bound)?; }
        self.constraint()?;
        let id = ErrorJoinId { index: self.error_joins.len() as u32, generation: Self::generation()? };
        self.error_joins.push(Slot { generation: id.generation, value: join });
        Ok(id)
    }

    pub(super) fn replace_error_join(&mut self, id: ErrorJoinId, types: &FxHashMap<TypeId, TypeId>, effects: &FxHashMap<EffectSummary, EffectSummary>, memo: &mut ReplacementMemo) -> Result<ErrorJoinId, InferenceError> {
        self.work()?;
        if let Some(replacement) = memo.error_joins.get(&id) { return Ok(*replacement); }
        self.work_many(self.error_join(id)?.inputs.len())?;
        let join = self.error_join(id)?.clone();
        let inputs = join.inputs.into_iter().map(|input| self.replace(input, types, effects, memo, 0)).collect::<Result<_, _>>()?;
        let result = self.replace(join.result, types, effects, memo, 0)?;
        let bound = join.bound.map(|bound| self.replace(bound, types, effects, memo, 0)).transpose()?;
        let replacement = self.store_error_join(ErrorJoin { inputs, result, bound })?;
        memo.error_joins.insert(id, replacement);
        Ok(replacement)
    }

    pub(super) fn error_join_type_pending(&mut self, ty: TypeId) -> Result<bool, InferenceError> {
        Ok(!self.free_metas(ty)?.is_empty() || !self.rigid_nodes(ty)?.is_empty()
            || !self.free_effects(ty)?.is_empty() || !self.rigid_effects(ty)?.is_empty())
    }

    // Reading the outcome consumes the shared work budget, but never adds a
    // type, constraint, or binding. Publication uses the same calculation.
    pub(super) fn error_join_outcome(&mut self, id: ErrorJoinId) -> Result<Option<ErrorJoinOutcome>, InferenceError> {
        self.work_many(self.error_join(id)?.inputs.len())?;
        let inputs = self.error_join(id)?.inputs.clone();
        if inputs.is_empty() { return Err(InferenceError::InvalidScheme); }
        let first = self.resolved(inputs[0])?;
        for &input in &inputs {
            self.work()?;
            let input = self.resolved(input)?;
            if matches!(self.node(input)?, TypeNode::Poison | TypeNode::NonCompletion) { return Err(InferenceError::Recovery(input)); }
        }
        if inputs.iter().all(|input| self.resolved(*input).is_ok_and(|input| input == first)) {
            return Ok(Some(ErrorJoinOutcome::Identity(first)));
        }
        // A bound on the output cannot decide independent symbolic inputs.
        // Their relationship is retained until an actual instance supplies them.
        for &input in &inputs { if self.error_join_type_pending(input)? { return Ok(None); } }
        let mut identical = true;
        for &input in inputs.iter().skip(1) { identical &= self.same_published_type(first, input)?; }
        if identical { return Ok(Some(ErrorJoinOutcome::Identity(first))); }

        let mut family = None;
        let mut one_family = true;
        for input in inputs {
            self.work()?;
            let input = self.resolved(input)?;
            let next = match self.node(input)? {
                TypeNode::Atom(Atom::ErrorFamily(family) | Atom::ErrorVariant { family, .. }) => Some(*family),
                TypeNode::Atom(Atom::Error | Atom::ProcessError | Atom::ErrorFacet(_)) => None,
                _ => return Err(InferenceError::TypeMismatch { left: first, right: input }),
            };
            if let Some(next) = next {
                if let Some(previous) = family { one_family &= previous == next; }
                else { family = Some(next); }
            } else { one_family = false; }
        }
        Ok(Some(ErrorJoinOutcome::Atom(if one_family { Atom::ErrorFamily(family.ok_or(InferenceError::InvalidScheme)?) } else { Atom::Error })))
    }

    pub(super) fn solve_error_join(&mut self, requirement: RequirementId, id: ErrorJoinId) -> Result<(), InferenceError> {
        if self.requirement(requirement)?.eligibility { return Ok(()); }
        let Some(outcome) = self.error_join_outcome(id)? else { return Ok(()); };
        let result = self.error_join(id)?.result;
        let bound = self.error_join(id)?.bound;
        let reason = self.requirement(requirement)?.reason;
        let joined = match outcome { ErrorJoinOutcome::Identity(ty) => ty, ErrorJoinOutcome::Atom(atom) => self.atom(atom)? };
        let (output_assignability, assignability) = if let Some(bound) = bound {
            if self.error_join_type_pending(joined)? { return Ok(()); }
            let output_origin = self.constraint_origins().len();
            self.assignable(result, joined, reason)?;
            self.trail_requirement(requirement)?;
            self.requirements[requirement.index()].value.error_join_output_assignability = Some(output_origin);
            if self.error_join_type_pending(bound)? { return Ok(()); }
            let origin = self.constraint_origins().len();
            self.assignable(bound, joined, reason)?;
            (Some(output_origin), Some(origin))
        } else { self.unify(result, joined, reason)?; (None, None) };
        self.trail_requirement(requirement)?;
        let state = &mut self.requirements[requirement.index()].value;
        state.error_join_output_assignability = output_assignability;
        state.error_join_assignability = assignability;
        state.eligibility = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceId;

    fn span() -> Span { Span::new(SourceId::new(0), 0, 1) }

    #[test]
    fn nominal_error_joins_preserve_family_and_disjoint_failure_identity() {
        let symbols = crate::symbol::SymbolOwner::new(); let _guard = symbols.enter();
        for disjoint in [false, true] {
            let mut graph = InferenceContext::default();
            let reason = graph.reason(span(), None).unwrap();
            let left = graph.atom(Atom::ErrorVariant { family: Name::intern("FirstFailure"), variant: Name::intern("Missing") }).unwrap();
            let right = graph.atom(Atom::ErrorVariant { family: Name::intern(if disjoint { "SecondFailure" } else { "FirstFailure" }), variant: Name::intern("Denied") }).unwrap();
            let result = graph.fresh(1, span()).unwrap();
            let requirement = graph.require_error_join(ErrorJoin { inputs: vec![left, right], result, bound: None }, reason).unwrap();
            graph.solve().unwrap();
            assert!(graph.eligibility_satisfied(requirement).unwrap());
            assert!(matches!(graph.node(graph.resolved(result).unwrap()).unwrap(), TypeNode::Atom(atom) if *atom == if disjoint { Atom::Error } else { Atom::ErrorFamily(Name::intern("FirstFailure")) }));
            assert_eq!(graph.resolved(left).unwrap(), left);
            assert_eq!(graph.resolved(right).unwrap(), right);
        }
    }

    #[test]
    fn a_declared_bound_does_not_ground_independent_pending_error_inputs() {
        let symbols = crate::symbol::SymbolOwner::new(); let _guard = symbols.enter();
        let mut graph = InferenceContext::default();
        let reason = graph.reason(span(), None).unwrap();
        let left = graph.fresh(1, span()).unwrap();
        let right = graph.fresh(1, span()).unwrap();
        let result = graph.fresh(1, span()).unwrap();
        let bound = graph.atom(Atom::Error).unwrap();
        let requirement = graph.require_error_join(ErrorJoin { inputs: vec![left, right], result, bound: Some(bound) }, reason).unwrap();
        graph.solve().unwrap();
        assert!(!graph.eligibility_satisfied(requirement).unwrap());
        for input in [left, right, result] { assert_eq!(graph.resolved(input).unwrap(), input); }
        let family = graph.atom(Atom::ErrorFamily(Name::intern("FirstFailure"))).unwrap();
        graph.unify(left, family, reason).unwrap(); graph.unify(right, family, reason).unwrap();
        graph.solve().unwrap();
        assert!(graph.eligibility_satisfied(requirement).unwrap());
        assert_eq!(graph.resolved(result).unwrap(), family);

        let widened = graph.require_error_join(ErrorJoin { inputs: vec![family, family], result: bound, bound: Some(bound) }, reason).unwrap();
        graph.solve().unwrap();
        assert!(graph.eligibility_satisfied(widened).unwrap());
        assert_eq!(graph.resolved(bound).unwrap(), bound);
        let state = graph.requirement(widened).unwrap();
        for index in [state.error_join_output_assignability, state.error_join_assignability] {
            let origin = &graph.constraint_origins()[index.expect("directional computed-output proof")];
            assert_eq!(origin.reason, reason);
            assert_eq!(origin.relation, ConstraintRelation::Assignable { expected: bound, actual: family });
        }
    }

    #[test]
    fn a_single_failure_retains_data_identity_and_rejects_an_unrelated_bound() {
        let symbols = crate::symbol::SymbolOwner::new(); let _guard = symbols.enter();
        let mut graph = InferenceContext::default();
        let reason = graph.reason(span(), None).unwrap();
        let input = graph.fresh(1, span()).unwrap();
        let result = graph.fresh(1, span()).unwrap();
        let requirement = graph.require_error_join(ErrorJoin { inputs: vec![input], result, bound: None }, reason).unwrap();
        graph.solve().unwrap();
        assert_eq!(graph.resolved(result).unwrap(), graph.resolved(input).unwrap());
        assert!(graph.eligibility_satisfied(requirement).unwrap());
        let data = graph.atom(Atom::Int).unwrap();
        graph.unify(input, data, reason).unwrap(); graph.solve().unwrap();
        assert_eq!(graph.resolved(result).unwrap(), data);

        let family = graph.atom(Atom::ErrorFamily(Name::intern("FirstFailure"))).unwrap();
        let bound = graph.atom(Atom::ErrorFamily(Name::intern("SecondFailure"))).unwrap();
        let output = graph.fresh(1, span()).unwrap();
        graph.require_error_join(ErrorJoin { inputs: vec![family], result: output, bound: Some(bound) }, reason).unwrap();
        assert!(matches!(graph.solve(), Err(InferenceError::TypeMismatch { .. })));
        assert_eq!(graph.resolved(family).unwrap(), family);
    }
}
