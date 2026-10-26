use super::{ConstraintChange, ConstraintConflict, ConstraintResolutionError, Generalization, SchemeOperationError, TypeConstraints, TypeVariableId, Variable, MAX_TYPE_DEPTH};
use crate::sema::schemes::{IterationPolicy, IterationRequirement, IterationSourceKind, SchemeType, SchemeTypeParameterId};
use crate::sema::types::Type;
use crate::source::Span;
use bbtree::unsync::BBTreeMap as BTreeMap;

type IterationConflict = (Type, Type, Option<ConstraintResolutionError>);

impl TypeConstraints {
    pub(crate) fn require_iteration(&mut self, source: &Type, required: &IterationRequirement<Type>, origin: Span) -> Result<(), ConstraintConflict> {
        self.apply_journaled(|constraints, changes| constraints.require_iteration_inner(source, required, origin, 0, changes))
            .map_err(|(expected, actual, resolution_error)| ConstraintConflict { expected, actual, initializer: None, established: None, contribution: origin, resolution_error })
    }

    pub(super) fn require_iteration_inner(&mut self, source: &Type, required: &IterationRequirement<Type>, origin: Span, depth: usize, changes: &mut Vec<ConstraintChange>) -> Result<(), IterationConflict> {
        if depth > MAX_TYPE_DEPTH { return Err((source.clone(), source.clone(), Some(ConstraintResolutionError::TypeDepth))); }
        let source = self.resolve_with_changes(source, 0, changes).map_err(|error| (source.clone(), source.clone(), Some(error)))?;
        match &source {
            Type::Inference(id) => {
                let root = self.representative(*id, Some(changes)).map_err(|error| (source.clone(), source.clone(), Some(error)))?;
                if self.record_binding(root).map_err(|error| (source.clone(), source.clone(), Some(error)))?.is_some() { return Err((source.clone(), source.clone(), None)); }
                let previous = self.iteration_requirement(root).map_err(|error| (source.clone(), source.clone(), Some(error)))?;
                let merged = match previous {
                    Some(previous) => self.merge_iteration(&previous, required, origin, depth + 1, changes)?,
                    None => required.clone(),
                };
                let root = self.representative(root, Some(changes)).map_err(|error| (source.clone(), source.clone(), Some(error)))?;
                let level = self.type_level(root).map_err(|error| (source.clone(), source.clone(), Some(error)))?;
                for ty in merged.types() { self.lower_levels_in_type(ty, level, changes).map_err(|error| (source.clone(), source.clone(), Some(error)))?; }
                self.record(root, changes);
                let Some(Variable::Representative(variable)) = self.variables.get_mut(&root) else { unreachable!() };
                variable.iteration = Some(Box::new(merged));
                Ok(())
            }
            Type::Validated(validated) => self.require_iteration_inner(validated.base(), required, origin, depth + 1, changes),
            Type::Stream(item) => {
                match &required.policy {
                    IterationPolicy::IterablePropagates(bound) | IterationPolicy::PipelinePropagates(bound) => self.constrain_inner(bound, &Type::Error, origin, false, depth + 1, changes)?,
                    IterationPolicy::PipelineTransport => {},
                    _ => return Err((required.item.clone(), source.clone(), None)),
                }
                self.constrain_inner(&required.item, item, origin, true, depth + 1, changes)
            }
            Type::Result(ok, error) => {
                if required.source_kind == IterationSourceKind::Direct { return Err((required.item.clone(), source.clone(), None)); }
                match &required.policy {
                    IterationPolicy::IterablePropagates(bound) | IterationPolicy::PipelinePropagates(bound) => self.constrain_inner(bound, error, origin, false, depth + 1, changes)?,
                    IterationPolicy::PipelineTransport => {},
                    _ => return Err((required.item.clone(), source.clone(), None)),
                }
                if matches!(ok.unvalidated(), Type::Result(_, _)) { return Err((required.item.clone(), source.clone(), None)); }
                let mut direct = required.clone();
                direct.source_kind = IterationSourceKind::Direct;
                self.require_iteration_inner(ok, &direct, origin, depth + 1, changes)
            }
            _ => {
                if required.policy.is_pipeline() && !matches!(source.unvalidated(), Type::List(_) | Type::Set(_)) {
                    return Err((required.item.clone(), source.clone(), None));
                }
                let item = source.iteration_item_type().ok_or_else(|| (required.item.clone(), source.clone(), None))?;
                self.constrain_inner(&required.item, &item, origin, true, depth + 1, changes)
            }
        }
    }

    pub(super) fn iteration_requirement(&self, id: TypeVariableId) -> Result<Option<IterationRequirement<Type>>, ConstraintResolutionError> {
        let root = self.representative_root(id)?;
        let Some(Variable::Representative(variable)) = self.variables.get(&root) else { unreachable!() };
        Ok(variable.iteration.as_deref().cloned())
    }

    pub(super) fn merge_iteration(&mut self, first: &IterationRequirement<Type>, second: &IterationRequirement<Type>, origin: Span, depth: usize, changes: &mut Vec<ConstraintChange>) -> Result<IterationRequirement<Type>, IterationConflict> {
        self.constrain_inner(&first.item, &second.item, origin, true, depth, changes)?;
        let policy = match (&first.policy, &second.policy) {
            (IterationPolicy::PipelineTransport, other) | (other, IterationPolicy::PipelineTransport) => other.clone(),
            (IterationPolicy::IterablePropagates(first) | IterationPolicy::PipelinePropagates(first), IterationPolicy::IterablePropagates(second) | IterationPolicy::PipelinePropagates(second)) => {
                let first = self.resolve_with_changes(first, depth, changes).map_err(|error| (first.clone(), second.clone(), Some(error)))?;
                let second = self.resolve_with_changes(second, depth, changes).map_err(|error| (first.clone(), second.clone(), Some(error)))?;
                if first.matches_expected(&second) { IterationPolicy::IterablePropagates(first) }
                else if second.matches_expected(&first) { IterationPolicy::IterablePropagates(second) }
                else if !first.contains_inference() && !second.contains_inference() { IterationPolicy::IterableInfallible }
                else {
                    self.constrain_inner(&first, &second, origin, true, depth, changes)?;
                    IterationPolicy::IterablePropagates(first)
                }
            }
            _ => IterationPolicy::IterableInfallible,
        };
        let source_kind = if matches!((first.source_kind, second.source_kind), (IterationSourceKind::DirectOrResult, IterationSourceKind::DirectOrResult)) { IterationSourceKind::DirectOrResult } else { IterationSourceKind::Direct };
        let policy = if first.policy.is_pipeline() || second.policy.is_pipeline() { policy.into_pipeline() } else { policy };
        Ok(IterationRequirement { item: first.item.clone(), policy, source_kind })
    }

    pub(super) fn generalized_iterations(&self, environment_level: u32, generalization: &mut Generalization) -> Result<BTreeMap<SchemeTypeParameterId, IterationRequirement<SchemeType>>, SchemeOperationError> {
        let mut iterations = BTreeMap::new();
        loop {
            let pending = generalization.type_ids.iter().filter(|(_, parameter)| !iterations.contains_key(*parameter)).map(|(&variable, &parameter)| (variable, parameter)).collect::<Vec<_>>();
            let before = generalization.type_ids.len();
            for (variable, parameter) in pending {
                if let Some(iteration) = self.iteration_requirement(variable)? {
                    let item = self.generalize_type(&iteration.item, environment_level, generalization, 0)?;
                    let policy = iteration.policy.try_map(|error| self.generalize_type(error, environment_level, generalization, 0))?;
                    iterations.insert(parameter, IterationRequirement { item, policy, source_kind: iteration.source_kind });
                }
            }
            if generalization.type_ids.len() == before { return Ok(iterations); }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceId;

    fn span() -> Span { Span::new(SourceId::new(0), 0, 1) }

    fn require(constraints: &mut TypeConstraints, source: &Type, item: Type) {
        constraints.require_iteration(source, &IterationRequirement { item, policy: IterationPolicy::IterableInfallible, source_kind: IterationSourceKind::DirectOrResult }, span()).unwrap();
    }

    #[test]
    fn pipeline_iteration_domain_intersects_without_selecting_a_container() {
        for source_type in [Type::List(Box::new(Type::Int)), Type::Set(Box::new(Type::Int)), Type::Stream(Box::new(Type::Int))] {
            let mut constraints = TypeConstraints::default();
            let source = constraints.fresh(span());
            let item = constraints.fresh(span());
            let mut required = IterationRequirement { item: item.clone(), policy: IterationPolicy::IterablePropagates(Type::Error), source_kind: IterationSourceKind::DirectOrResult };
            constraints.require_iteration(&source, &required, span()).unwrap();
            required.policy = IterationPolicy::PipelineTransport;
            constraints.require_iteration(&source, &required, span()).unwrap();
            assert_eq!(constraints.resolve(&source).unwrap(), source);
            for rejected in [Type::Str, Type::Bytes, Type::Map(Box::new(Type::Str), Box::new(Type::Int))] {
                assert!(constraints.constrain(&source, &rejected, span()).is_err());
                assert_eq!(constraints.resolve(&source).unwrap(), source);
                assert_eq!(constraints.resolve(&item).unwrap(), item);
            }
            constraints.constrain(&source, &Type::Any, span()).unwrap();
            assert_eq!(constraints.resolve(&source).unwrap(), source);
            assert_eq!(constraints.resolve(&item).unwrap(), item);
            assert!(constraints.require_iteration(&Type::Any, &required, span()).is_err());
            constraints.constrain(&source, &source_type, span()).unwrap();
            assert_eq!(constraints.resolve(&item).unwrap(), Type::Int);
        }
    }

    #[test]
    fn pipeline_transport_intersection_retains_for_error_restrictions() {
        for initial in [IterationPolicy::IterableInfallible, IterationPolicy::IterablePropagates(Type::Str)] {
            let mut constraints = TypeConstraints::default();
            let source = constraints.fresh(span());
            let item = constraints.fresh(span());
            constraints.require_iteration(&source, &IterationRequirement { item: item.clone(), policy: initial.clone(), source_kind: IterationSourceKind::DirectOrResult }, span()).unwrap();
            constraints.require_iteration(&source, &IterationRequirement { item: item.clone(), policy: IterationPolicy::PipelineTransport, source_kind: IterationSourceKind::DirectOrResult }, span()).unwrap();
            assert!(constraints.constrain(&source, &Type::Stream(Box::new(Type::Int)), span()).is_err());
            assert!(constraints.constrain(&source, &Type::Result(Box::new(Type::List(Box::new(Type::Int))), Box::new(Type::Error)), span()).is_err());
            assert_eq!(constraints.resolve(&source).unwrap(), source);
            let accepted = if matches!(initial, IterationPolicy::IterableInfallible) { Type::List(Box::new(Type::Int)) } else { Type::Result(Box::new(Type::List(Box::new(Type::Int))), Box::new(Type::Str)) };
            constraints.constrain(&source, &accepted, span()).unwrap();
            assert_eq!(constraints.resolve(&item).unwrap(), Type::Int);
        }
    }

    #[test]
    fn iteration_relationship_merge_rolls_back_descendant_compression() {
        let mut constraints = TypeConstraints::default();
        let first_item = constraints.fresh(span());
        let first_error = constraints.fresh(span());
        constraints.constrain(&first_item, &first_error, span()).unwrap();
        let second_item = constraints.fresh(span());
        let second_child = constraints.fresh(span());
        let second_branch = constraints.fresh(span());
        let second_leaf = constraints.fresh(span());
        constraints.constrain(&second_item, &second_child, span()).unwrap();
        constraints.constrain(&second_branch, &second_leaf, span()).unwrap();
        constraints.constrain(&second_item, &second_branch, span()).unwrap();
        require(&mut constraints, &second_item, Type::Int);
        let first_source = constraints.fresh(span());
        let second_source = constraints.fresh(span());
        for (source, item, error) in [
            (&first_source, first_item.clone(), first_error.clone()),
            (&second_source, second_item.clone(), Type::Bool),
        ] {
            constraints.require_iteration(source, &IterationRequirement { item, policy: IterationPolicy::IterablePropagates(error), source_kind: IterationSourceKind::DirectOrResult }, span()).unwrap();
        }
        let original_error = constraints.resolve(&first_error).unwrap();
        assert!(constraints.constrain(&first_source, &second_source, span()).is_err());
        assert_eq!(constraints.resolve(&first_error).unwrap(), original_error);
        assert_eq!(constraints.resolve(&first_item).unwrap(), original_error);
        assert_ne!(constraints.resolve(&second_item).unwrap(), original_error);
    }

    #[test]
    fn iteration_relationships_preserve_item_domains_and_rollback_invalid_instances() {
        for (source_type, item_type) in [
            (Type::List(Box::new(Type::Int)), Type::Int),
            (Type::Set(Box::new(Type::Str)), Type::Str),
            (Type::Str, Type::Str),
            (Type::Bytes, Type::Int),
        ] {
            let mut constraints = TypeConstraints::default();
            let source = constraints.fresh(span());
            let item = constraints.fresh(span());
            require(&mut constraints, &source, item.clone());
            assert!(constraints.constrain(&source, &Type::Int, span()).is_err());
            assert_eq!(constraints.resolve(&source).unwrap(), source);
            assert_eq!(constraints.resolve(&item).unwrap(), item);
            constraints.constrain(&source, &source_type, span()).unwrap();
            assert_eq!(constraints.resolve(&item).unwrap(), item_type);
        }
    }

    #[test]
    fn iteration_relationships_union_sources_and_reject_incompatible_item_contexts() {
        let mut constraints = TypeConstraints::default();
        let first = constraints.fresh(span());
        let first_item = constraints.fresh(span());
        let second = constraints.fresh(span());
        require(&mut constraints, &first, first_item.clone());
        require(&mut constraints, &second, Type::Str);
        constraints.constrain(&first, &second, span()).unwrap();
        assert_eq!(constraints.resolve(&first_item).unwrap(), Type::Str);
        assert!(constraints.constrain(&second, &Type::List(Box::new(Type::Int)), span()).is_err());
        constraints.constrain(&second, &Type::Str, span()).unwrap();
        assert_eq!(constraints.resolve(&first).unwrap(), Type::Str);
        let item = constraints.fresh(span());
        let source = constraints.fresh(span());
        require(&mut constraints, &source, item.clone());
        require(&mut constraints, &source, source.clone());
        constraints.constrain(&source, &Type::Str, span()).unwrap();
        assert_eq!(constraints.resolve(&item).unwrap(), Type::Str);
    }

    #[test]
    fn iteration_relationships_participate_in_trials_and_reject_foreign_item_cells() {
        let mut constraints = TypeConstraints::default();
        let source = constraints.fresh(span());
        let item = constraints.fresh(span());
        assert_eq!(constraints.trial(|candidate| candidate.require_iteration(&source, &IterationRequirement { item: item.clone(), policy: IterationPolicy::IterableInfallible, source_kind: IterationSourceKind::DirectOrResult }, span())), Ok(true));
        constraints.constrain(&source, &Type::Int, span()).unwrap();
        let source = constraints.fresh(span());
        let foreign = TypeConstraints::default().fresh(span());
        assert!(constraints.require_iteration(&source, &IterationRequirement { item: foreign, policy: IterationPolicy::IterableInfallible, source_kind: IterationSourceKind::DirectOrResult }, span()).is_err());
        constraints.constrain(&source, &Type::Bool, span()).unwrap();
    }

    #[test]
    fn iteration_relationships_preserve_result_error_bounds_and_occurs_checks() {
        let mut constraints = TypeConstraints::default();
        let source = constraints.fresh(span());
        let item = constraints.fresh(span());
        constraints.require_iteration(&source, &IterationRequirement { item: item.clone(), policy: IterationPolicy::IterablePropagates(Type::Str), source_kind: IterationSourceKind::DirectOrResult }, span()).unwrap();
        assert!(constraints.constrain(&source, &Type::Result(Box::new(Type::List(Box::new(Type::Int))), Box::new(Type::Error)), span()).is_err());
        assert_eq!(constraints.resolve(&item).unwrap(), item);
        constraints.constrain(&source, &Type::Result(Box::new(Type::List(Box::new(Type::Int))), Box::new(Type::Str)), span()).unwrap();
        assert_eq!(constraints.resolve(&item).unwrap(), Type::Int);
        let recursive = constraints.fresh(span());
        require(&mut constraints, &recursive, recursive.clone());
        assert!(constraints.constrain(&recursive, &Type::List(Box::new(recursive.clone())), span()).is_err());
        constraints.constrain(&recursive, &Type::Str, span()).unwrap();
        let source = constraints.fresh(span());
        let pending = constraints.fresh(span());
        constraints.require_iteration(&source, &IterationRequirement { item: Type::Int, policy: IterationPolicy::IterablePropagates(Type::Error), source_kind: IterationSourceKind::DirectOrResult }, span()).unwrap();
        constraints.constrain(&source, &Type::Result(Box::new(pending.clone()), Box::new(Type::Error)), span()).unwrap();
        assert!(constraints.constrain(&pending, &Type::Result(Box::new(Type::List(Box::new(Type::Int))), Box::new(Type::Error)), span()).is_err());
        constraints.constrain(&pending, &Type::List(Box::new(Type::Int)), span()).unwrap();
        let first = constraints.fresh(span());
        let second = constraints.fresh(span());
        for (source, error) in [(&first, Type::Str), (&second, Type::Bool)] {
            constraints.require_iteration(source, &IterationRequirement { item: Type::Int, policy: IterationPolicy::IterablePropagates(error), source_kind: IterationSourceKind::DirectOrResult }, span()).unwrap();
        }
        constraints.constrain(&first, &second, span()).unwrap();
        assert!(constraints.constrain(&first, &Type::Result(Box::new(Type::List(Box::new(Type::Int))), Box::new(Type::Str)), span()).is_err());
        constraints.constrain(&second, &Type::List(Box::new(Type::Int)), span()).unwrap();
    }
}
