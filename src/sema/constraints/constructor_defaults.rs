use super::{CallableApplicationScope, ConstraintChange, ConstraintResolutionError, SchemeOperationError, Type, TypeConstraints, TypeVariableId, Variable};
use crate::sema::check::EffectDeclarationId;
use crate::source::Span;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_CONSTRUCTOR_DEFAULT: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ConstructorArgumentCheckpoint {
    owner: CallableApplicationScope,
    first: u64,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ConstructorErrorDefault {
    variable: TypeVariableId,
    source: Span,
}

#[derive(Clone, Debug, Default)]
pub(super) struct ConstructorErrorDefaults {
    pending: BTreeMap<CallableApplicationScope, BTreeMap<u64, ConstructorErrorDefault>>,
    arguments: Vec<ConstructorArgumentCheckpoint>,
}

#[derive(Clone, Debug)]
pub(super) enum ConstructorDefaultChange {
    Registered(CallableApplicationScope, u64),
    Removed(CallableApplicationScope, u64, ConstructorErrorDefault),
    ArgumentsStarted,
    ArgumentsFinished(ConstructorArgumentCheckpoint),
}

impl ConstructorErrorDefaults {
    pub(super) fn rollback(&mut self, change: ConstructorDefaultChange) {
        match change {
            ConstructorDefaultChange::Registered(owner, id) => { self.remove(owner, id); }
            ConstructorDefaultChange::Removed(owner, id, value) => { assert!(self.pending.entry(owner).or_default().insert(id, value).is_none()); }
            ConstructorDefaultChange::ArgumentsStarted => { self.arguments.pop().expect("constructor argument scope owns its checkpoint"); }
            ConstructorDefaultChange::ArgumentsFinished(checkpoint) => self.arguments.push(checkpoint),
        }
    }

    fn remove(&mut self, owner: CallableApplicationScope, id: u64) -> ConstructorErrorDefault {
        let entries = self.pending.get_mut(&owner).expect("constructor default owns its scope");
        let value = entries.remove(&id).expect("constructor default owns its identity");
        if entries.is_empty() { self.pending.remove(&owner); }
        value
    }

    #[cfg(test)]
    pub(super) fn retained_bytes(&self) -> usize {
        self.arguments.capacity() * std::mem::size_of::<ConstructorArgumentCheckpoint>()
            + self.pending.len() * std::mem::size_of::<(CallableApplicationScope, BTreeMap<u64, ConstructorErrorDefault>)>()
            + self.pending.values().map(|entries| entries.len() * std::mem::size_of::<(u64, ConstructorErrorDefault)>()).sum::<usize>()
    }
}

impl TypeConstraints {
    pub(crate) fn record_constructor_error_default(&mut self, owner: CallableApplicationScope, error: &Type, source: Span) -> Result<(), ConstraintResolutionError> {
        let Type::Inference(variable) = self.resolve(error)? else { return Ok(()) };
        let variable = self.representative_root(variable)?;
        if !matches!(self.variables.get(&variable), Some(Variable::Representative(value)) if value.binding.is_none()) { return Ok(()) }
        let id = NEXT_CONSTRUCTOR_DEFAULT.fetch_add(1, Ordering::Relaxed);
        self.constructor_defaults.pending.entry(owner).or_default().insert(id, ConstructorErrorDefault { variable, source });
        if let Some(changes) = &mut self.trial_changes { changes.push(ConstraintChange::ConstructorDefault(ConstructorDefaultChange::Registered(owner, id))); }
        Ok(())
    }

    pub(crate) fn begin_constructor_arguments(&mut self, owner: CallableApplicationScope) -> ConstructorArgumentCheckpoint {
        let checkpoint = ConstructorArgumentCheckpoint { owner, first: NEXT_CONSTRUCTOR_DEFAULT.fetch_add(1, Ordering::Relaxed) };
        self.constructor_defaults.arguments.push(checkpoint);
        if let Some(changes) = &mut self.trial_changes { changes.push(ConstraintChange::ConstructorDefault(ConstructorDefaultChange::ArgumentsStarted)); }
        checkpoint
    }

    fn end_constructor_arguments(&mut self, checkpoint: ConstructorArgumentCheckpoint) -> Result<(), ConstraintResolutionError> {
        if self.constructor_defaults.arguments.last() != Some(&checkpoint) { return Err(ConstraintResolutionError::ForeignVariable); }
        self.constructor_defaults.arguments.pop();
        if let Some(changes) = &mut self.trial_changes { changes.push(ConstraintChange::ConstructorDefault(ConstructorDefaultChange::ArgumentsFinished(checkpoint))); }
        Ok(())
    }

    pub(crate) fn reject_constructor_arguments(&mut self, checkpoint: ConstructorArgumentCheckpoint) -> Result<(), ConstraintResolutionError> {
        self.end_constructor_arguments(checkpoint)?;
        self.reject_constructor_error_defaults_since(checkpoint);
        Ok(())
    }

    fn reject_constructor_error_defaults_since(&mut self, checkpoint: ConstructorArgumentCheckpoint) {
        let entries = self.constructor_defaults.pending.get(&checkpoint.owner).into_iter().flat_map(|entries| entries.range(checkpoint.first..).map(|(&id, _)| id)).collect::<Vec<_>>();
        for id in entries { self.remove_constructor_error_default(checkpoint.owner, id); }
    }

    pub(crate) fn finish_constructor_arguments(&mut self, checkpoint: ConstructorArgumentCheckpoint) -> Result<(), SchemeOperationError> {
        self.end_constructor_arguments(checkpoint)?;
        if checkpoint.owner == CallableApplicationScope::Invocation && self.constructor_defaults.arguments.is_empty() {
            if let Err(error) = self.complete_constructor_error_defaults_since(checkpoint.owner, checkpoint.first, &BTreeSet::new(), &[]) {
                self.reject_constructor_error_defaults_since(checkpoint);
                return Err(error);
            }
        }
        Ok(())
    }

    pub(crate) fn has_constructor_error_defaults(&self, owner: CallableApplicationScope) -> bool { self.constructor_defaults.pending.contains_key(&owner) }

    pub(crate) fn complete_constructor_error_defaults(&mut self, owner: CallableApplicationScope, component: &BTreeSet<EffectDeclarationId>, inputs: &[Type]) -> Result<(), SchemeOperationError> {
        self.complete_constructor_error_defaults_since(owner, 0, component, inputs)
    }

    fn remove_constructor_error_default(&mut self, owner: CallableApplicationScope, id: u64) {
        let value = self.constructor_defaults.remove(owner, id);
        if let Some(changes) = &mut self.trial_changes { changes.push(ConstraintChange::ConstructorDefault(ConstructorDefaultChange::Removed(owner, id, value))); }
    }

    fn complete_constructor_error_defaults_since(&mut self, owner: CallableApplicationScope, first: u64, component: &BTreeSet<EffectDeclarationId>, inputs: &[Type]) -> Result<(), SchemeOperationError> {
        let entries = self.constructor_defaults.pending.get(&owner).into_iter().flat_map(|entries| entries.range(first..).map(|(&id, &entry)| (id, entry))).collect::<Vec<_>>();
        if entries.is_empty() { return Ok(()) }
        self.commit_constraint_transaction(|constraints| {
            constraints.solve_callable_applications().map_err(SchemeOperationError::Application)?;
            let supported = match owner {
                CallableApplicationScope::Declaration(owner) => constraints.input_bound_variables(owner, component, inputs)?,
                CallableApplicationScope::Invocation => super::input_support::UnresolvedVariables::default(),
            };
            for (id, entry) in entries {
                constraints.counters.occurs_visits.fetch_add(1, Ordering::Relaxed);
                if let Type::Inference(variable) = constraints.resolve(&Type::Inference(entry.variable))? {
                    let root = constraints.representative_root(variable)?;
                    if !supported.contains_type(root) && matches!(constraints.variables.get(&root), Some(Variable::Representative(value)) if value.binding.is_none()) {
                        constraints.constrain(&Type::Inference(root), &Type::Error, entry.source).map_err(SchemeOperationError::Constraint)?;
                    }
                }
                constraints.remove_constructor_error_default(owner, id);
            }
            constraints.solve_callable_applications().map_err(SchemeOperationError::Application)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceId;

    fn span() -> Span { Span::new(SourceId::new(0), 0, 1) }
    fn owner() -> EffectDeclarationId { EffectDeclarationId { namespace: None, body: span() } }

    #[test]
    fn constructor_defaults_wait_for_all_actuals_and_preserve_input_bound_cells() {
        let mut constraints = TypeConstraints::default();
        let scope = CallableApplicationScope::Invocation;
        let outer = constraints.begin_constructor_arguments(scope);
        let actual = constraints.fresh(span());
        constraints.record_constructor_error_default(scope, &actual, span()).unwrap();
        let inner = constraints.begin_constructor_arguments(scope);
        let phantom = constraints.fresh(span());
        constraints.record_constructor_error_default(scope, &phantom, span()).unwrap();
        constraints.finish_constructor_arguments(inner).unwrap();
        assert_eq!(constraints.resolve(&phantom).unwrap(), phantom);
        constraints.constrain(&actual, &Type::Str, span()).unwrap();
        constraints.finish_constructor_arguments(outer).unwrap();
        assert_eq!(constraints.resolve(&actual).unwrap(), Type::Str);
        assert_eq!(constraints.resolve(&phantom).unwrap(), Type::Error);
        assert!(!constraints.has_constructor_error_defaults(scope));

        let scope = CallableApplicationScope::Declaration(owner());
        let input = constraints.fresh(span());
        let alias = constraints.fresh(span());
        constraints.record_constructor_error_default(scope, &alias, span()).unwrap();
        constraints.constrain(&alias, &input, span()).unwrap();
        let phantom = constraints.fresh(span());
        constraints.record_constructor_error_default(scope, &phantom, span()).unwrap();
        let inputs = [Type::Result(Box::new(Type::Int), Box::new(input.clone()))];
        constraints.complete_constructor_error_defaults(scope, &BTreeSet::from([owner()]), &inputs).unwrap();
        assert!(matches!(constraints.resolve(&alias).unwrap(), Type::Inference(_)));
        assert_eq!(constraints.resolve(&phantom).unwrap(), Type::Error);
        constraints.constrain(&input, &Type::Str, span()).unwrap();
        assert_eq!(constraints.resolve(&alias).unwrap(), Type::Str);
    }

    #[test]
    fn constructor_defaults_journal_clones_rejections_and_foreign_boundaries() {
        let mut constraints = TypeConstraints::default();
        let scope = CallableApplicationScope::Invocation;
        let checkpoint = constraints.begin_constructor_arguments(scope);
        let pending = constraints.fresh(span());
        constraints.record_constructor_error_default(scope, &pending, span()).unwrap();
        assert_eq!(constraints.trial(|candidate| {
            candidate.finish_constructor_arguments(checkpoint).unwrap();
            candidate.constrain(&pending, &Type::Error, span())
        }), Ok(true));
        assert_eq!(constraints.resolve(&pending).unwrap(), pending);
        assert!(constraints.has_constructor_error_defaults(scope));
        let mut clone = constraints.clone();
        clone.finish_constructor_arguments(checkpoint).unwrap();
        assert_eq!(clone.resolve(&pending).unwrap(), Type::Error);
        assert_eq!(constraints.resolve(&pending).unwrap(), pending);
        constraints.reject_constructor_arguments(checkpoint).unwrap();
        assert!(!constraints.has_constructor_error_defaults(scope));
        assert_eq!(constraints.resolve(&pending).unwrap(), pending);

        let foreign = TypeConstraints::default().fresh(span());
        assert_eq!(constraints.record_constructor_error_default(scope, &foreign, span()), Err(ConstraintResolutionError::ForeignVariable));
        let foreign_checkpoint = TypeConstraints::default().begin_constructor_arguments(scope);
        assert_eq!(constraints.reject_constructor_arguments(foreign_checkpoint), Err(ConstraintResolutionError::ForeignVariable));
        let mut deep = Type::Int;
        for _ in 0..=super::super::MAX_TYPE_DEPTH { deep = Type::List(Box::new(deep)); }
        assert_eq!(constraints.record_constructor_error_default(scope, &deep, span()), Err(ConstraintResolutionError::TypeDepth));

        let scope = CallableApplicationScope::Declaration(owner());
        constraints.record_constructor_error_default(scope, &pending, span()).unwrap();
        assert!(matches!(constraints.complete_constructor_error_defaults(scope, &BTreeSet::new(), &[]), Err(SchemeOperationError::Resolution(ConstraintResolutionError::ForeignVariable))));
        assert!(constraints.has_constructor_error_defaults(scope));
        assert_eq!(constraints.resolve(&pending).unwrap(), pending);
    }

    #[test]
    fn failed_constructor_defaults_do_not_leak_bindings_or_argument_depth() {
        let mut constraints = TypeConstraints::default();
        let scope = CallableApplicationScope::Invocation;
        let checkpoint = constraints.begin_constructor_arguments(scope);
        let first = constraints.fresh(span());
        let constrained = constraints.fresh(span());
        for value in [&first, &constrained] { constraints.record_constructor_error_default(scope, value, span()).unwrap(); }
        constraints.require_map_key(&constrained, span()).unwrap();
        assert!(constraints.finish_constructor_arguments(checkpoint).is_err());
        assert_eq!(constraints.resolve(&first).unwrap(), first);
        assert_eq!(constraints.resolve(&constrained).unwrap(), constrained);
        assert!(!constraints.has_constructor_error_defaults(scope));
        let checkpoint = constraints.begin_constructor_arguments(scope);
        constraints.record_constructor_error_default(scope, &first, span()).unwrap();
        constraints.finish_constructor_arguments(checkpoint).unwrap();
        assert_eq!(constraints.resolve(&first).unwrap(), Type::Error);
    }
}
