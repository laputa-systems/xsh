use super::{ConstraintChange, ConstraintResolutionError, TypeConstraints, MAX_TYPE_DEPTH};
use crate::sema::effects::{EffectContract, EffectSet};
use crate::source::Span;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_EFFECT_VARIABLE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct EffectVariableId(u64);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CallableGeneralizationScope {
    pub(super) owner: super::CallableApplicationScope,
    pub(super) types: BTreeMap<super::TypeVariableId, crate::sema::schemes::SchemeTypeParameterId>,
    pub(super) rows: BTreeMap<super::RowVariableId, crate::sema::schemes::SchemeRowParameterId>,
    pub(super) effects: BTreeMap<EffectVariableId, crate::sema::schemes::SchemeEffectParameterId>,
    pub(super) applications: BTreeMap<super::CallableApplicationId, u32>,
}

pub(crate) type EffectPredicate = crate::sema::effects::EffectPredicate<EffectVariableId>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EffectConstraintError {
    Resolution(ConstraintResolutionError),
    Mismatch,
    Unresolved,
}

impl From<ConstraintResolutionError> for EffectConstraintError {
    fn from(error: ConstraintResolutionError) -> Self {
        Self::Resolution(error)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EffectVariable {
    origin: Span,
    level: u32,
    binding: Option<EffectContract<EffectVariableId>>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct EffectConstraints {
    variables: BTreeMap<EffectVariableId, EffectVariable>,
    predicates: Vec<EffectPredicate>,
    watchers: BTreeMap<EffectVariableId, BTreeSet<usize>>,
    pending: VecDeque<usize>,
    queued: BTreeSet<usize>,
}

#[derive(Clone, Debug)]
pub(super) enum EffectChange {
    FreshVariable(EffectVariableId),
    Binding(EffectVariableId, Option<EffectContract<EffectVariableId>>),
    Level(EffectVariableId, u32),
    Predicate,
    Watcher(EffectVariableId, usize),
    Queued(usize),
    Dequeued(usize),
}

impl EffectConstraints {
    pub(super) fn variable_level(&self, id: EffectVariableId) -> Result<u32, ConstraintResolutionError> {
        self.variables.get(&id).map(|variable| variable.level).ok_or(ConstraintResolutionError::ForeignVariable)
    }

    pub(super) fn rollback(&mut self, change: EffectChange) {
        match change {
            EffectChange::FreshVariable(id) => { self.variables.remove(&id); }
            EffectChange::Binding(id, binding) => {
                self.variables.get_mut(&id).expect("effect cell belongs to its journal").binding = binding;
            }
            EffectChange::Level(id, level) => {
                self.variables.get_mut(&id).expect("effect cell belongs to its journal").level = level;
            }
            EffectChange::Predicate => { self.predicates.pop().expect("effect predicate belongs to its journal"); }
            EffectChange::Watcher(variable, predicate) => {
                let watchers = self.watchers.get_mut(&variable).expect("effect watcher belongs to its journal");
                assert!(watchers.remove(&predicate));
                if watchers.is_empty() { self.watchers.remove(&variable); }
            }
            EffectChange::Queued(predicate) => {
                assert_eq!(self.pending.pop_back(), Some(predicate));
                assert!(self.queued.remove(&predicate));
            }
            EffectChange::Dequeued(predicate) => {
                self.pending.push_front(predicate);
                assert!(self.queued.insert(predicate));
            }
        }
    }

    fn watch(&mut self, variable: EffectVariableId, predicate: usize, changes: &mut Vec<ConstraintChange>) {
        if self.watchers.entry(variable).or_default().insert(predicate) {
            changes.push(ConstraintChange::Effect(EffectChange::Watcher(variable, predicate)));
        }
    }

    fn enqueue(&mut self, predicate: usize, changes: &mut Vec<ConstraintChange>) {
        if self.queued.insert(predicate) {
            self.pending.push_back(predicate);
            changes.push(ConstraintChange::Effect(EffectChange::Queued(predicate)));
        }
    }

    pub(super) fn resolve(
        &self,
        contract: &EffectContract<EffectVariableId>,
        depth: usize,
        visiting: &mut BTreeSet<EffectVariableId>,
    ) -> Result<EffectContract<EffectVariableId>, ConstraintResolutionError> {
        self.resolve_with_memo(contract, depth, visiting, &mut BTreeMap::new()).map(|(contract, _)| contract)
    }

    fn resolve_with_memo(
        &self,
        contract: &EffectContract<EffectVariableId>,
        depth: usize,
        visiting: &mut BTreeSet<EffectVariableId>,
        memo: &mut BTreeMap<EffectVariableId, (EffectContract<EffectVariableId>, usize)>,
    ) -> Result<(EffectContract<EffectVariableId>, usize), ConstraintResolutionError> {
        if depth > MAX_TYPE_DEPTH { return Err(ConstraintResolutionError::TypeDepth); }
        let mut height = 0;
        let contract = contract.substitute(|id| {
            let variable = self.variables.get(id).ok_or(ConstraintResolutionError::ForeignVariable)?;
            let Some(binding) = &variable.binding else { return Ok(EffectContract::variable(*id)); };
            let (resolved, variable_height) = if let Some(resolved) = memo.get(id) {
                resolved.clone()
            } else {
                if !visiting.insert(*id) { return Err(ConstraintResolutionError::TypeDepth); }
                let resolved = self.resolve_with_memo(binding, depth + 1, visiting, memo);
                visiting.remove(id);
                let (resolved, nested_height) = resolved?;
                let resolved = (resolved, nested_height + 1);
                memo.insert(*id, resolved.clone());
                resolved
            };
            if depth.saturating_add(variable_height) > MAX_TYPE_DEPTH {
                return Err(ConstraintResolutionError::TypeDepth);
            }
            height = height.max(variable_height);
            Ok(resolved)
        })?;
        Ok((contract, height))
    }

    fn dependencies(contract: &EffectContract<EffectVariableId>) -> impl Iterator<Item = EffectVariableId> + '_ {
        match contract {
            EffectContract::Unknown => None,
            EffectContract::Known(expression) => Some(expression.variables()),
        }.into_iter().flatten().map(|(id, _)| *id)
    }

    fn bind(
        &mut self,
        id: EffectVariableId,
        contract: EffectContract<EffectVariableId>,
        changes: &mut Vec<ConstraintChange>,
    ) -> Result<bool, EffectConstraintError> {
        if Self::dependencies(&contract).any(|dependency| dependency == id) {
            return Ok(false);
        }
        let dependencies = Self::dependencies(&contract).collect::<Vec<_>>();
        let level = self.variables.get(&id).ok_or(ConstraintResolutionError::ForeignVariable)?.level;
        self.lower_levels(&contract, level, changes)?;
        let variable = self.variables.get_mut(&id).ok_or(ConstraintResolutionError::ForeignVariable)?;
        assert!(variable.binding.is_none());
        changes.push(ConstraintChange::Effect(EffectChange::Binding(id, variable.binding.replace(contract))));
        let watchers = self.watchers.get(&id).into_iter().flatten().copied().collect::<Vec<_>>();
        for predicate in watchers {
            for dependency in &dependencies { self.watch(*dependency, predicate, changes); }
            self.enqueue(predicate, changes);
        }
        Ok(true)
    }

    pub(super) fn lower_levels(&mut self, contract: &EffectContract<EffectVariableId>, level: u32, changes: &mut Vec<ConstraintChange>) -> Result<(), ConstraintResolutionError> {
        let mut pending = Self::dependencies(contract).map(|id| (id, 0)).collect::<Vec<_>>();
        let mut visited = BTreeSet::new();
        while let Some((id, depth)) = pending.pop() {
            if depth > MAX_TYPE_DEPTH { return Err(ConstraintResolutionError::TypeDepth); }
            if !visited.insert(id) { continue; }
            let variable = self.variables.get_mut(&id).ok_or(ConstraintResolutionError::ForeignVariable)?;
            if variable.level > level {
                changes.push(ConstraintChange::Effect(EffectChange::Level(id, variable.level)));
                variable.level = level;
            }
            if let Some(binding) = &variable.binding {
                pending.extend(Self::dependencies(binding).map(|id| (id, depth + 1)));
            }
        }
        Ok(())
    }

    fn check(&mut self, predicate: &EffectPredicate, changes: &mut Vec<ConstraintChange>) -> Result<bool, EffectConstraintError> {
        let mut contracts = predicate.contracts().map(|contract| self.resolve(contract, 0, &mut BTreeSet::new()));
        let left = contracts.next().expect("effect predicates have a subject")?;
        let right = contracts.next().transpose()?;
        match (predicate, left, right) {
            (EffectPredicate::Equal(_, _), left, Some(right)) => {
                if left == right { return Ok(true); }
                if let EffectContract::Known(expression) = &left
                    && let Some(id) = expression.single_variable()
                {
                    return self.bind(*id, right, changes);
                }
                if let EffectContract::Known(expression) = &right
                    && let Some(id) = expression.single_variable()
                {
                    return self.bind(*id, left, changes);
                }
                match (&left, &right) {
                    (EffectContract::Unknown, _) | (_, EffectContract::Unknown) => Err(EffectConstraintError::Mismatch),
                    _ => match (left.closed(), right.closed()) {
                        (Some(left), Some(right)) if left != right => Err(EffectConstraintError::Mismatch),
                        (Some(_), Some(_)) => Ok(true),
                        _ => Ok(false),
                    },
                }
            }
            (EffectPredicate::Included(_, _) | EffectPredicate::Covered(_, _), left, Some(right)) => {
                if matches!(right, EffectContract::Unknown) { return Ok(true); }
                if matches!(left, EffectContract::Unknown) { return Err(EffectConstraintError::Mismatch); }
                if left == right { return Ok(true); }
                let (Some(left), Some(right)) = (left.closed(), right.closed()) else { return Ok(false); };
                let fits = match predicate {
                    EffectPredicate::Included(_, _) => left.is_subset(right),
                    EffectPredicate::Covered(_, _) => {
                        let permissions = right.iter().collect::<Vec<_>>();
                        left.iter().all(|effect| crate::sema::check::Checker::effects_covers(&permissions, &effect))
                    }
                    _ => unreachable!(),
                };
                if fits { Ok(true) } else { Err(EffectConstraintError::Mismatch) }
            }
            (EffectPredicate::Excluded(_, excluded), left, None) => {
                let EffectContract::Known(expression) = left else { return Err(EffectConstraintError::Mismatch); };
                let prohibited = excluded.prohibited_requirements();
                if !expression.direct().intersection(prohibited).is_empty() { return Err(EffectConstraintError::Mismatch); }
                Ok(expression.variables().all(|(_, mask)| mask.intersection(prohibited).is_empty()))
            }
            _ => unreachable!("effect predicate arity is fixed"),
        }
    }

    fn drain(&mut self, changes: &mut Vec<ConstraintChange>) -> Result<(), EffectConstraintError> {
        while let Some(predicate) = self.pending.pop_front() {
            assert!(self.queued.remove(&predicate));
            changes.push(ConstraintChange::Effect(EffectChange::Dequeued(predicate)));
            let obligation = self.predicates[predicate].clone();
            self.check(&obligation, changes)?;
        }
        Ok(())
    }
}

impl TypeConstraints {
    pub(super) fn generalize_effect_contract(
        &self,
        contract: &EffectContract<EffectVariableId>,
        environment_level: u32,
        generalization: &mut super::Generalization,
    ) -> Result<EffectContract<crate::sema::schemes::SchemeEffectParameterId>, super::SchemeOperationError> {
        self.resolve_effect(contract)?.substitute(|id| {
            if self.effects.variable_level(*id)? <= environment_level {
                return Err(super::SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedEffects));
            }
            let parameter = if let Some(parameter) = generalization.effect_ids.get(id) {
                *parameter
            } else {
                let parameter = crate::sema::schemes::SchemeEffectParameterId(generalization.effect_parameters.len() as u32);
                generalization.effect_ids.insert(*id, parameter);
                generalization.effect_parameters.push(parameter);
                parameter
            };
            Ok(EffectContract::variable(parameter))
        })
    }

    pub(super) fn generalize_effect_predicates(
        &self,
        environment_level: u32,
        generalization: &mut super::Generalization,
    ) -> Result<Vec<crate::sema::effects::EffectPredicate<crate::sema::schemes::SchemeEffectParameterId>>, super::SchemeOperationError> {
        let mut pending = generalization.effect_ids.keys().copied().collect::<Vec<_>>();
        let mut visited_variables = BTreeSet::new();
        let mut visited_predicates = BTreeSet::new();
        let mut predicates = Vec::new();
        while let Some(variable) = pending.pop() {
            if !visited_variables.insert(variable) { continue; }
            for &index in self.effects.watchers.get(&variable).into_iter().flatten() {
                if !visited_predicates.insert(index) { continue; }
                let predicate = self.effects.predicates[index].try_map_contracts(|contract| {
                    self.generalize_effect_contract(contract, environment_level, generalization)
                })?;
                predicates.push(predicate);
                pending.extend(generalization.effect_ids.keys().filter(|id| !visited_variables.contains(id)).copied());
            }
        }
        Ok(predicates)
    }


    pub(crate) fn instantiate_effect_contract(
        &self,
        effects: &crate::sema::effects::CheckedEffects,
        instance: &super::InstantiatedScheme,
    ) -> Result<crate::sema::effects::CallableEffects, super::SchemeOperationError> {
        let contract = effects.contract().substitute(|id| instance.effect_arguments.get(id).copied()
            .map(EffectContract::variable).ok_or(super::SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnboundEffectParameter)))?;
        Ok(crate::sema::effects::CallableEffects::from_contract(self.resolve_effect(&contract)?))
    }

    pub(crate) fn resolve_generalized_effect(
        &self,
        contract: &EffectContract<EffectVariableId>,
        scope: &CallableGeneralizationScope,
    ) -> Result<EffectContract<crate::sema::schemes::SchemeEffectParameterId>, super::SchemeOperationError> {
        self.resolve_effect(contract)?.substitute(|id| scope.effects.get(id).copied()
            .map(EffectContract::variable).ok_or(super::SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::BodyTypeNotBound)))
    }

    pub(crate) fn fresh_effect_at_level(&mut self, origin: Span, level: u32) -> EffectVariableId {
        let id = EffectVariableId(NEXT_EFFECT_VARIABLE.fetch_add(1, Ordering::Relaxed));
        assert!(self.effects.variables.insert(id, EffectVariable { origin, level, binding: None }).is_none());
        if let Some(changes) = &mut self.trial_changes {
            changes.push(ConstraintChange::Effect(EffectChange::FreshVariable(id)));
        }
        id
    }

    pub(crate) fn resolve_effect(&self, contract: &EffectContract<EffectVariableId>) -> Result<EffectContract<EffectVariableId>, ConstraintResolutionError> {
        self.effects.resolve(contract, 0, &mut BTreeSet::new())
    }

    pub(crate) fn constrain_effect(&mut self, predicate: EffectPredicate) -> Result<(), EffectConstraintError> {
        self.apply_journaled(|this, changes| this.constrain_effect_inner(predicate, changes))
    }

    pub(super) fn constrain_effect_inner(&mut self, predicate: EffectPredicate, changes: &mut Vec<ConstraintChange>) -> Result<(), EffectConstraintError> {
        let dependencies = predicate.contracts().map(|contract| {
            self.effects.resolve(contract, 0, &mut BTreeSet::new()).map(|resolved| {
                EffectConstraints::dependencies(&resolved).collect::<Vec<_>>()
            })
        }).collect::<Result<Vec<_>, _>>()?;
        let id = self.effects.predicates.len();
        self.effects.predicates.push(predicate);
        changes.push(ConstraintChange::Effect(EffectChange::Predicate));
        for dependency in dependencies.into_iter().flatten() { self.effects.watch(dependency, id, changes); }
        self.effects.enqueue(id, changes);
        self.effects.drain(changes)
    }

    pub(crate) fn finish_effect_constraints(&mut self) -> Result<(), EffectConstraintError> {
        self.apply_journaled(|this, changes| {
            for predicate in this.effects.predicates.clone() {
                if !this.effects.check(&predicate, changes)? { return Err(EffectConstraintError::Unresolved); }
            }
            this.effects.drain(changes)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::types::Type;
    use crate::source::SourceId;
    use crate::syntax::node::Effect;

    fn span() -> Span { Span::new(SourceId::new(0), 0, 1) }
    fn finite(effects: impl IntoIterator<Item = Effect>) -> EffectContract<EffectVariableId> {
        EffectContract::finite(EffectSet::new(effects))
    }

    #[test]
    fn symbolic_effect_constraints_wake_alias_dependencies_and_restore_failed_bindings() {
        let mut constraints = TypeConstraints::default();
        let first = constraints.fresh_effect_at_level(span(), 1);
        let second = constraints.fresh_effect_at_level(span(), 1);
        constraints.constrain_effect(EffectPredicate::Covered(EffectContract::variable(first), finite([Effect::Env]))).unwrap();
        constraints.constrain_effect(EffectPredicate::Equal(EffectContract::variable(first), EffectContract::variable(second))).unwrap();
        let before = constraints.effects.clone();
        assert_eq!(constraints.constrain_effect(EffectPredicate::Equal(EffectContract::variable(second), finite([Effect::Time]))), Err(EffectConstraintError::Mismatch));
        assert_eq!(constraints.effects, before);
        constraints.constrain_effect(EffectPredicate::Equal(EffectContract::variable(second), finite([Effect::Env]))).unwrap();
        assert_eq!(constraints.resolve_effect(&EffectContract::variable(first)).unwrap(), finite([Effect::Env]));
        constraints.finish_effect_constraints().unwrap();
        let callable = Type::Callable(std::sync::Arc::new(crate::sema::types::TypedCallable {
            pure: false,
            sig: crate::sema::types::CallableType {
                params: Vec::new(), return_ty: Box::new(Type::Int),
                effects: crate::sema::types::CallableEffects::from_contract(EffectContract::variable(first)),
            },
        }));
        assert!(callable.contains_inference());
        let resolved = constraints.resolve(&callable).unwrap();
        assert!(!resolved.contains_inference());
        assert_eq!(constraints.generalize_types(&[callable], 0).unwrap().types[0].closed_ground_type(), Some(resolved));
        let bound = constraints.fresh_effect_at_level(span(), 1);
        let callable = |effects| Type::Callable(std::sync::Arc::new(crate::sema::types::TypedCallable {
            pure: false,
            sig: crate::sema::types::CallableType {
                params: Vec::new(), return_ty: Box::new(Type::Int),
                effects: crate::sema::types::CallableEffects::from_contract(effects),
            },
        }));
        constraints.constrain(&callable(EffectContract::variable(bound)), &callable(finite([Effect::Env])), span()).unwrap();
        assert_eq!(constraints.finish_effect_constraints(), Err(EffectConstraintError::Unresolved));
        constraints.constrain_effect(EffectPredicate::Equal(EffectContract::variable(bound), finite([Effect::Env]))).unwrap();
        constraints.finish_effect_constraints().unwrap();
    }

    #[test]
    fn symbolic_effect_constraints_keep_coverage_distinct_from_identity_and_exclusions() {
        let mut constraints = TypeConstraints::default();
        constraints.constrain_effect(EffectPredicate::Covered(finite([Effect::Fs]), finite([Effect::Io]))).unwrap();
        assert_eq!(constraints.constrain_effect(EffectPredicate::Equal(finite([Effect::Fs]), finite([Effect::Io]))), Err(EffectConstraintError::Mismatch));
        assert_eq!(constraints.constrain_effect(EffectPredicate::Included(finite([Effect::Fs]), finite([Effect::Io]))), Err(EffectConstraintError::Mismatch));
        assert_eq!(constraints.constrain_effect(EffectPredicate::Excluded(finite([Effect::Io]), EffectSet::singleton(Effect::Fs))), Err(EffectConstraintError::Mismatch));
        constraints.constrain_effect(EffectPredicate::Excluded(finite([Effect::Fs]), EffectSet::singleton(Effect::Io))).unwrap();
        let denied = constraints.fresh_effect_at_level(span(), 1);
        constraints.constrain_effect(EffectPredicate::Excluded(EffectContract::variable(denied), EffectSet::singleton(Effect::Fs))).unwrap();
        assert_eq!(constraints.constrain_effect(EffectPredicate::Equal(EffectContract::variable(denied), finite([Effect::Io]))), Err(EffectConstraintError::Mismatch));
        constraints.constrain_effect(EffectPredicate::Equal(EffectContract::variable(denied), finite([Effect::Time]))).unwrap();
        let allowed = constraints.fresh_effect_at_level(span(), 1);
        constraints.constrain_effect(EffectPredicate::Excluded(EffectContract::variable(allowed), EffectSet::singleton(Effect::Io))).unwrap();
        constraints.constrain_effect(EffectPredicate::Equal(EffectContract::variable(allowed), finite([Effect::Fs]))).unwrap();
        let callback = constraints.fresh_effect_at_level(span(), 1);
        constraints.constrain_effect(EffectPredicate::Excluded(EffectContract::variable(callback).capture_error(), EffectSet::singleton(Effect::Error))).unwrap();
        constraints.constrain_effect(EffectPredicate::Excluded(EffectContract::variable(callback), EffectSet::singleton(Effect::Fs))).unwrap();
        assert_eq!(constraints.finish_effect_constraints(), Err(EffectConstraintError::Unresolved));
        assert_eq!(constraints.constrain_effect(EffectPredicate::Equal(EffectContract::variable(callback), EffectContract::Unknown)), Err(EffectConstraintError::Mismatch));
        constraints.constrain_effect(EffectPredicate::Equal(EffectContract::variable(callback), finite([Effect::Env, Effect::Error]))).unwrap();
        constraints.finish_effect_constraints().unwrap();
    }

    #[test]
    fn symbolic_effect_constraint_trials_restore_fresh_cells_watchers_queue_and_panic() {
        for panic in [false, true] {
            let mut constraints = TypeConstraints::default();
            let variable = constraints.fresh_effect_at_level(span(), 1);
            constraints.constrain_effect(EffectPredicate::Covered(EffectContract::variable(variable), finite([Effect::Env]))).unwrap();
            let before = constraints.effects.clone();
            let mut fresh = None;
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| constraints.trial(|constraints| {
                let id = constraints.fresh_effect_at_level(span(), 2);
                fresh = Some(id);
                let captured = constraints.fresh(span());
                let callback = Type::Callable(std::sync::Arc::new(crate::sema::types::TypedCallable {
                    pure: false,
                    sig: crate::sema::types::CallableType {
                        params: Vec::new(), return_ty: Box::new(Type::Int),
                        effects: crate::sema::types::CallableEffects::from_contract(EffectContract::variable(id)),
                    },
                }));
                constraints.constrain(&captured, &callback, span())?;
                assert_eq!(constraints.effects.variables[&id].level, 0);
                constraints.constrain_effect(EffectPredicate::Equal(EffectContract::variable(variable), EffectContract::variable(id))).unwrap();
                constraints.constrain_effect(EffectPredicate::Equal(EffectContract::variable(id), finite([Effect::Env]))).unwrap();
                if panic { panic!("failed candidate after effect constraints"); }
                constraints.constrain(&Type::Int, &Type::Str, span())
            })));
            if panic { assert!(result.is_err()); } else { assert_eq!(result.unwrap(), Ok(false)); }
            assert_eq!(constraints.effects, before);
            assert_eq!(constraints.resolve_effect(&EffectContract::variable(fresh.unwrap())), Err(ConstraintResolutionError::ForeignVariable));
            let cloned = constraints.clone();
            assert_eq!(cloned.resolve_effect(&EffectContract::variable(variable)), Ok(EffectContract::variable(variable)));
        }
    }

    #[test]
    fn symbolic_effect_constraints_do_not_choose_a_union_contributor_or_io_permission() {
        let mut constraints = TypeConstraints::default();
        let first = constraints.fresh_effect_at_level(span(), 1);
        let second = constraints.fresh_effect_at_level(span(), 1);
        constraints.constrain_effect(EffectPredicate::Included(finite([Effect::Fs]), EffectContract::variable(first).union(EffectContract::variable(second)))).unwrap();
        assert_eq!(constraints.resolve_effect(&EffectContract::variable(first)), Ok(EffectContract::variable(first)));
        assert_eq!(constraints.finish_effect_constraints(), Err(EffectConstraintError::Unresolved));
        constraints.constrain_effect(EffectPredicate::Equal(EffectContract::variable(first), finite([]))).unwrap();
        constraints.constrain_effect(EffectPredicate::Equal(EffectContract::variable(second), finite([Effect::Fs]))).unwrap();
        constraints.finish_effect_constraints().unwrap();
    }
}
