use super::{CallableApplicationScope, ConstraintChange, ConstraintResolutionError, SchemeOperationError, SealedOperationError, Type, TypeConstraints, TypeVariableId, Variable};
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
    constructor_owned: bool,
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
    fn fresh_constructor_error(&self, owner: CallableApplicationScope, error: &Type) -> Result<bool, ConstraintResolutionError> {
        let Type::Inference(variable) = self.resolve(error)? else { return Ok(false) };
        let Some(entries) = self.constructor_defaults.pending.get(&owner) else { return Ok(false) };
        let owns = |variable| entries.values().any(|entry| entry.variable == variable && entry.constructor_owned);
        let Some(Variable::Representative(value)) = self.variables.get(&variable) else { return Ok(false) };
        if !owns(variable) || value.binding.is_some() { return Ok(false); }
        if value.rank == 0 { return Ok(true); }
        for &member in self.variables.keys() {
            self.counters.occurs_visits.fetch_add(1, Ordering::Relaxed);
            if self.representative_root(member)? == variable && !owns(member) { return Ok(false); }
        }
        Ok(true)
    }

    pub(super) fn constrain_store_constructor_errors(&mut self, owner: CallableApplicationScope, storage: &Type, replacement: &Type, span: Span, changes: &mut Vec<ConstraintChange>) -> Result<(), SealedOperationError> {
        if !self.has_constructor_error_defaults(owner) { return Ok(()); }
        let mut pending = vec![(storage.clone(), replacement.clone(), 0)];
        while let Some((storage, replacement, depth)) = pending.pop() {
            if depth > super::MAX_TYPE_DEPTH { return Err(ConstraintResolutionError::TypeDepth.into()); }
            self.counters.occurs_visits.fetch_add(1, Ordering::Relaxed);
            let storage = self.resolve_with_changes(&storage, 0, changes)?;
            let mut replacement = self.resolve_with_changes(&replacement, 0, changes)?;
            if let Type::Result(_, error) = &storage
                && self.fresh_constructor_error(owner, error)?
            {
                if let Type::Inference(variable) = replacement {
                    let level = self.type_level(variable)?;
                    let payload = self.fresh_type_with_changes(span, level, changes);
                    let error = self.fresh_type_with_changes(span, level, changes);
                    let shape = Type::Result(Box::new(payload), Box::new(error));
                    self.constrain_operation_type(&Type::Inference(variable), &shape, span, changes)?;
                    replacement = self.resolve_with_changes(&replacement, 0, changes)?;
                }
                if let Type::Result(_, replacement_error) = &replacement
                    && !replacement_error.contains_any() && !replacement_error.contains_recovery()
                {
                    self.constrain_operation_type(error, replacement_error, span, changes)?;
                }
            }
            match (storage, replacement) {
                (Type::Result(left, left_error), Type::Result(right, right_error))
                | (Type::Map(left, left_error), Type::Map(right, right_error)) => {
                    pending.push((*left, *right, depth + 1));
                    pending.push((*left_error, *right_error, depth + 1));
                }
                (Type::List(left), Type::List(right)) | (Type::Set(left), Type::Set(right))
                | (Type::Stream(left), Type::Stream(right)) | (Type::Optional(left), Type::Optional(right)) => pending.push((*left, *right, depth + 1)),
                (Type::Optional(left), right) if !matches!(right, Type::Inference(_)) => pending.push((*left, right, depth + 1)),
                (Type::Record(left), Type::Record(right)) => {
                    for (name, left) in left.iter() {
                        if let Some(right) = right.get(name) { pending.push((left.clone(), right.clone(), depth + 1)); }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub(crate) fn record_constructor_error_default(&mut self, owner: CallableApplicationScope, error: &Type, source: Span) -> Result<(), ConstraintResolutionError> {
        let Type::Inference(variable) = self.resolve(error)? else { return Ok(()) };
        let variable = self.representative_root(variable)?;
        let Some(Variable::Representative(value)) = self.variables.get(&variable) else { return Ok(()) };
        if value.binding.is_some() { return Ok(()) }
        let constructor_owned = value.rank == 0 && value.origin == source && !value.annotation;
        let id = NEXT_CONSTRUCTOR_DEFAULT.fetch_add(1, Ordering::Relaxed);
        self.constructor_defaults.pending.entry(owner).or_default().insert(id, ConstructorErrorDefault { variable, source, constructor_owned });
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
        if checkpoint.owner == CallableApplicationScope::Invocation && self.constructor_defaults.arguments.is_empty()
            && let Err(error) = self.complete_constructor_error_defaults_since(checkpoint.owner, checkpoint.first, &BTreeSet::new(), &[])
        {
            self.reject_constructor_error_defaults_since(checkpoint);
            return Err(error);
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
                CallableApplicationScope::Guard(_) => return Err(ConstraintResolutionError::ForeignVariable.into()),
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
    fn constructor_storage_context_preserves_foreign_error_cells() {
        use crate::sema::sealed_operation::SealedOperation;
        let mut constraints = TypeConstraints::default();
        let scope = CallableApplicationScope::Invocation;
        let error = constraints.fresh(span());
        constraints.record_constructor_error_default(scope, &error, span()).unwrap();
        let foreign_span = Span::new(SourceId::new(0), 2, 3);
        let foreign = constraints.fresh(foreign_span);
        let foreign_owner = CallableApplicationScope::Declaration(EffectDeclarationId { namespace: None, body: Span::new(SourceId::new(0), 2, 4) });
        constraints.record_constructor_error_default(foreign_owner, &foreign, foreign_span).unwrap();
        constraints.constrain(&error, &foreign, span()).unwrap();
        let before = constraints.resolve(&foreign).unwrap();
        constraints.register_sealed_operation(scope, SealedOperation::StoreValue {
            storage: Type::Result(Box::new(Type::Int), Box::new(error)),
            replacement: Type::Result(Box::new(Type::Int), Box::new(Type::Bool)), span: span(),
        }).unwrap();
        constraints.solve_sealed_operations().unwrap();
        assert_eq!(constraints.resolve(&foreign).unwrap(), before);
        assert!(matches!(before, Type::Inference(_)));
    }

    #[test]
    fn failed_constructor_storage_context_restores_inference_state() {
        use crate::sema::sealed_operation::SealedOperation;
        let mut constraints = TypeConstraints::default();
        let scope = CallableApplicationScope::Invocation;
        let error = constraints.fresh(span());
        constraints.record_constructor_error_default(scope, &error, span()).unwrap();
        let replacement = constraints.fresh(Span::new(SourceId::new(0), 2, 3));
        constraints.register_sealed_operation(scope, SealedOperation::StoreValue {
            storage: Type::Result(Box::new(Type::Int), Box::new(error.clone())),
            replacement: replacement.clone(), span: span(),
        }).unwrap();
        let variables = constraints.variables.len();
        let operations = constraints.operations.clone();
        let rejected = constraints.commit_constraint_transaction(|trial| {
            trial.solve_sealed_operations()?;
            trial.constrain(&replacement, &Type::Result(Box::new(Type::Str), Box::new(Type::Bool)), span()).map_err(SealedOperationError::Constraint)?;
            trial.solve_sealed_operations()
        });
        assert!(matches!(rejected, Err(SealedOperationError::StorageConstraint(_))));
        assert_eq!(constraints.resolve(&replacement).unwrap(), replacement);
        assert_eq!(constraints.resolve(&error).unwrap(), error);
        assert_eq!(constraints.variables.len(), variables);
        assert_eq!(constraints.operations, operations);
        constraints.constrain(&replacement, &Type::Result(Box::new(Type::Int), Box::new(Type::Bool)), span()).unwrap();
        constraints.solve_sealed_operations().unwrap();
        assert_eq!(constraints.resolve(&error).unwrap(), Type::Bool);
    }

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
