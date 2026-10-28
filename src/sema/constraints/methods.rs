use super::{ConstraintChange, ConstraintResolutionError, Generalization, TypeConstraints};
use super::application_constraints::ApplicationDependency;
use crate::sema::schemes::{CallableScheme, MethodFamilyRequirement, SchemeType};
use crate::source::Span;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_METHOD: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct SealedMethodId(NonZeroU64);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ScopedMethod {
    pub(super) owner: super::CallableApplicationScope,
    pub(super) requirement: MethodFamilyRequirement<crate::sema::types::Type>,
    span: Span,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct SealedMethods {
    pub(super) entries: BTreeMap<SealedMethodId, ScopedMethod>,
    watchers: BTreeMap<ApplicationDependency, BTreeSet<SealedMethodId>>,
    pending: VecDeque<SealedMethodId>,
    queued: BTreeSet<SealedMethodId>,
    source_sites: BTreeMap<(super::CallableApplicationScope, Span), SealedMethodId>,
}

#[derive(Clone, Debug)]
pub(super) enum MethodChange {
    Fresh(SealedMethodId, Option<(super::CallableApplicationScope, Span)>),
    Watcher(ApplicationDependency, SealedMethodId),
    Queued(SealedMethodId),
    Dequeued(SealedMethodId),
}

impl SealedMethods {
    #[cfg(test)]
    pub(super) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        self.entries.len() * size_of::<(SealedMethodId, ScopedMethod)>()
            + self.entries.values().map(|entry| {
                entry.requirement.argument_slots.capacity() * size_of::<u32>()
                    + entry.requirement.arguments.capacity() * size_of::<crate::sema::types::Type>()
                    + std::iter::once(&entry.requirement.receiver)
                        .chain(entry.requirement.arguments.iter())
                        .chain(std::iter::once(&entry.requirement.result))
                        .map(super::type_owned_payload_bytes)
                        .sum::<usize>()
            }).sum::<usize>()
            + self.source_sites.len() * size_of::<((super::CallableApplicationScope, Span), SealedMethodId)>()
            + self.watchers.len() * size_of::<(ApplicationDependency, BTreeSet<SealedMethodId>)>()
            + self.watchers.values().map(|ids| ids.len() * size_of::<SealedMethodId>()).sum::<usize>()
            + self.pending.capacity() * size_of::<SealedMethodId>()
            + self.queued.len() * size_of::<SealedMethodId>()
    }

    pub(super) fn rollback(&mut self, change: MethodChange) {
        match change {
            MethodChange::Queued(id) => { assert_eq!(self.pending.pop_back(), Some(id)); assert!(self.queued.remove(&id)); }
            MethodChange::Dequeued(id) => { self.pending.push_front(id); assert!(self.queued.insert(id)); }
            MethodChange::Watcher(dependency, id) => {
                let watchers = self.watchers.get_mut(&dependency).expect("method watcher belongs to its journal");
                assert!(watchers.remove(&id));
                if watchers.is_empty() { self.watchers.remove(&dependency); }
            }
            MethodChange::Fresh(id, source) => {
                self.entries.remove(&id).expect("method requirement belongs to its journal");
                if let Some(source) = source {
                    assert_eq!(self.source_sites.remove(&source), Some(id));
                }
            }
        }
    }

    fn enqueue(&mut self, id: SealedMethodId, changes: &mut Vec<ConstraintChange>) {
        if self.queued.insert(id) {
            self.pending.push_back(id);
            changes.push(ConstraintChange::Method(MethodChange::Queued(id)));
        }
    }

    fn watch(&mut self, dependency: ApplicationDependency, id: SealedMethodId, changes: &mut Vec<ConstraintChange>) {
        if self.watchers.entry(dependency).or_default().insert(id) { changes.push(ConstraintChange::Method(MethodChange::Watcher(dependency, id))); }
    }

    pub(super) fn inherit_type_watchers(&mut self, source: super::TypeVariableId, target: super::TypeVariableId, changes: &mut Vec<ConstraintChange>) {
        let ids = self.watchers.get(&ApplicationDependency::Type(source)).into_iter().flatten().copied().collect::<Vec<_>>();
        for id in ids { self.watch(ApplicationDependency::Type(target), id, changes); }
    }

    pub(super) fn enqueue_changes(&mut self, from: usize, changes: &mut Vec<ConstraintChange>) {
        let dependencies = changes[from..].iter().filter_map(|change| match change {
            ConstraintChange::Type(id, _) | ConstraintChange::FreshType(id) => Some(ApplicationDependency::Type(*id)),
            ConstraintChange::Row(id, _) => Some(ApplicationDependency::Row(*id)),
            _ => None,
        }).collect::<BTreeSet<_>>();
        for dependency in dependencies {
            for id in self.watchers.get(&dependency).into_iter().flatten().copied().collect::<Vec<_>>() { self.enqueue(id, changes); }
        }
    }

    pub(super) fn is_pending(&self) -> bool { !self.pending.is_empty() }

}

impl TypeConstraints {
    pub(crate) fn register_method_family(
        &mut self,
        owner: super::CallableApplicationScope,
        requirement: MethodFamilyRequirement<crate::sema::types::Type>,
        source: Option<Span>,
        span: Span,
    ) -> Result<SealedMethodId, ConstraintResolutionError> {
        let dependencies = self.application_type_dependencies(std::iter::once(&requirement.receiver).chain(requirement.arguments.iter()).chain(std::iter::once(&requirement.result)))?;
        let source = source.map(|span| (owner, span));
        if let Some(source) = source
            && let Some(id) = self.methods.source_sites.get(&source).copied()
        {
            return if self.methods.entries.get(&id).is_some_and(|entry| entry.requirement == requirement) {
                Ok(id)
            } else {
                Err(ConstraintResolutionError::ForeignVariable)
            };
        }
        let id = SealedMethodId(NonZeroU64::new(NEXT_METHOD.fetch_add(1, Ordering::Relaxed)).expect("method identities are nonzero"));
        self.apply_journaled(|this, changes| {
            assert!(this.methods.entries.insert(id, ScopedMethod { owner, requirement, span }).is_none());
            if let Some(source) = source {
                assert!(this.methods.source_sites.insert(source, id).is_none());
            }
            changes.push(ConstraintChange::Method(MethodChange::Fresh(id, source)));
            for dependency in dependencies { this.methods.watch(dependency, id, changes); }
            this.methods.enqueue(id, changes);
            Ok::<_, ConstraintResolutionError>(())
        })?;
        Ok(id)
    }

    pub(crate) fn unresolved_method_result(&self, ty: &crate::sema::types::Type) -> Result<bool, ConstraintResolutionError> {
        let crate::sema::types::Type::Inference(id) = self.resolve(ty)? else { return Ok(false); };
        let root = self.representative_root(id)?;
        for id in self.methods.watchers.get(&ApplicationDependency::Type(root)).into_iter().flatten() {
            let entry = self.methods.entries.get(id).ok_or(ConstraintResolutionError::ForeignVariable)?;
            if !matches!(crate::modules::api_spec().method_requirement_result(entry.requirement.kind), Some(crate::modules::signature::MethodFamilyResult::SelectedSignature)) { continue; }
            if let crate::sema::types::Type::Inference(result) = self.resolve(&entry.requirement.result)?
                && self.representative_root(result)? == root { return Ok(true); }
        }
        Ok(false)
    }

    pub(crate) fn owned_method_result_is_path(&self, owner: super::CallableApplicationScope, ty: &crate::sema::types::Type) -> Result<bool, ConstraintResolutionError> {
        let crate::sema::types::Type::Inference(id) = self.resolve(ty)? else { return Ok(false); };
        let root = self.representative_root(id)?;
        for id in self.methods.watchers.get(&ApplicationDependency::Type(root)).into_iter().flatten() {
            let entry = self.methods.entries.get(id).ok_or(ConstraintResolutionError::ForeignVariable)?;
            if entry.owner != owner || !crate::modules::api_spec().method_requirement_returns_paths(entry.requirement.kind) { continue; }
            if let crate::sema::types::Type::Inference(result) = self.resolve(&entry.requirement.result)?
                && self.representative_root(result)? == root { return Ok(true); }
        }
        Ok(false)
    }

    pub(super) fn solve_method_families(&mut self) -> Result<(), MethodConstraintError> {
        self.apply_journaled(|this, changes| {
            while let Some(id) = this.methods.pending.pop_front() {
                assert!(this.methods.queued.remove(&id));
                changes.push(ConstraintChange::Method(MethodChange::Dequeued(id)));
                let entry = this.methods.entries.get(&id).ok_or(ConstraintResolutionError::ForeignVariable)?.clone();
                let types = std::iter::once(&entry.requirement.receiver).chain(entry.requirement.arguments.iter()).chain(std::iter::once(&entry.requirement.result));
                for dependency in this.application_type_dependencies(types)? { this.methods.watch(dependency, id, changes); }
                let receiver = this.resolve(&entry.requirement.receiver)?;
                let arguments = entry.requirement.arguments.iter().map(|ty| this.resolve(ty)).collect::<Result<Vec<_>, _>>()?;
                if receiver.contains_inference() || arguments.iter().any(crate::sema::types::Type::contains_inference) { continue; }
                let api = crate::modules::api_spec();
                let crate::modules::signature::MethodFamilySelection::Selected(selected) = api.select_method_requirement(entry.requirement.kind, &receiver, &entry.requirement.argument_slots, &arguments) else {
                    return Err(MethodConstraintError::Unsupported { kind: entry.requirement.kind, receiver });
                };
                let (_, _, signature) = api.registered_method(selected).ok_or(ConstraintResolutionError::ForeignVariable)?;
                this.constrain_inner(&entry.requirement.result, &signature.sig.return_ty, entry.span, false, 0, changes)
                    .map_err(|(expected, actual, resolution_error)| MethodConstraintError::Constraint(super::ConstraintConflict {
                        expected, actual, contribution: entry.span, initializer: None, established: None, resolution_error,
                    }))?;
                if this.resolve(&entry.requirement.result)? != signature.sig.return_ty {
                    return Err(MethodConstraintError::Unsupported { kind: entry.requirement.kind, receiver });
                }
            }
            Ok(())
        })
    }

    pub(crate) fn source_method_family(&self, owner: super::CallableApplicationScope, span: Span) -> Option<SealedMethodId> {
        self.methods.source_sites.get(&(owner, span)).copied()
    }

    pub(crate) fn generalized_method_ordinal(
        &self,
        id: SealedMethodId,
        owner: crate::sema::check::EffectDeclarationId,
        callable: &CallableScheme,
        scope: &super::CallableGeneralizationScope,
    ) -> Result<Option<u32>, super::SchemeOperationError> {
        let entry = self.methods.entries.get(&id).ok_or(ConstraintResolutionError::ForeignVariable)?;
        if scope.owner != super::CallableApplicationScope::Declaration(owner) { return Err(ConstraintResolutionError::ForeignVariable.into()); }
        let view = self.generalized_requirement_view(entry.owner, callable, scope)?;
        self.validate_generalized_methods_view(callable, scope, &view)?;
        Ok(view.methods.get(&id).copied())
    }

    pub(super) fn generalize_method_families(
        &self,
        owners: &BTreeSet<super::CallableApplicationScope>,
        environment_level: u32,
        generalization: &mut Generalization,
    ) -> Result<Vec<MethodFamilyRequirement<SchemeType>>, super::SchemeOperationError> {
        let mut requirements = Vec::new();
        for (&id, entry) in &self.methods.entries {
            if !owners.contains(&entry.owner) {
                continue;
            }
            let ordinal = u32::try_from(requirements.len()).map_err(|_| ConstraintResolutionError::TypeDepth)?;
            generalization.methods.insert(id, ordinal);
            requirements.push(generalize_method_family(self, &entry.requirement, environment_level, generalization)?);
        }
        Ok(requirements)
    }

    pub(super) fn validate_generalized_methods(
        &self,
        callable: &CallableScheme,
        scope: &super::CallableGeneralizationScope,
    ) -> Result<Vec<SealedMethodId>, super::SchemeOperationError> {
        let view = self.generalized_requirement_view(scope.owner, callable, scope)?;
        self.validate_generalized_methods_view(callable, scope, &view)
    }

    pub(super) fn validate_generalized_methods_view(
        &self,
        callable: &CallableScheme,
        scope: &super::CallableGeneralizationScope,
        view: &super::GeneralizedRequirementView<'_>,
    ) -> Result<Vec<SealedMethodId>, super::SchemeOperationError> {
        let mut methods = view.methods.iter().map(|(&id, &ordinal)| (ordinal, id)).collect::<Vec<_>>();
        methods.sort_unstable();
        if methods.len() != view.requirements.methods.len() {
            return Err(super::SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedMethodFamily));
        }
        let mut generalization = self.requirement_generalization(callable, scope, view);
        for (index, &(ordinal, id)) in methods.iter().enumerate() {
            let entry = self.methods.entries.get(&id).ok_or(ConstraintResolutionError::ForeignVariable)?;
            if index != ordinal as usize
                || generalize_method_family(self, &entry.requirement, 0, &mut generalization)? != view.requirements.methods[index]
            {
                return Err(super::SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedMethodFamily));
            }
        }
        if generalization.type_ids != scope.types || generalization.row_ids != scope.rows {
            return Err(super::SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::BodyTypeNotBound));
        }
        Ok(methods.into_iter().map(|(_, id)| id).collect())
    }

    pub(super) fn sealed_method_bindings(
        &self,
        instance: &super::InstantiatedRequirements,
        generalization: &mut Generalization,
        caller_methods: &[MethodFamilyRequirement<SchemeType>],
    ) -> Result<Vec<crate::sema::schemes::SealedMethodBinding>, super::SchemeOperationError> {
        instance.methods.iter().map(|id| {
            let entry = self.methods.entries.get(id).ok_or(ConstraintResolutionError::ForeignVariable)?;
            match select_registered_method(self, &entry.requirement)? {
                MethodSelection::Selected(method) => Ok(crate::sema::schemes::SealedMethodBinding::Selected(method)),
                MethodSelection::Unresolved => {
                    let ordinal = generalization.methods.get(id).copied()
                        .ok_or(super::SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedMethodFamily))?;
                    let actual = generalize_method_family(self, &entry.requirement, 0, generalization)?;
                    if caller_methods.get(ordinal as usize) != Some(&actual) {
                        return Err(super::SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedMethodFamily));
                    }
                    Ok(crate::sema::schemes::SealedMethodBinding::Forwarded(ordinal))
                }
                MethodSelection::Dynamic | MethodSelection::Recovery => Err(super::SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedMethodFamily)),
                MethodSelection::Unsupported => Err(super::SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnsupportedMethodFamily)),
                MethodSelection::Ambiguous => Err(super::SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::AmbiguousMethodFamily)),
            }
        }).collect()
    }
}

enum MethodSelection {
    Selected(xsh_registry::signature::RegisteredMethodId),
    Unresolved,
    Dynamic,
    Recovery,
    Unsupported,
    Ambiguous,
}

fn select_registered_method(
    constraints: &TypeConstraints,
    requirement: &MethodFamilyRequirement<crate::sema::types::Type>,
) -> Result<MethodSelection, super::SchemeOperationError> {
    let receiver = constraints.resolve(&requirement.receiver)?;
    let arguments = requirement.arguments.iter().map(|ty| constraints.resolve(ty)).collect::<Result<Vec<_>, _>>()?;
    if receiver.contains_inference() || arguments.iter().any(crate::sema::types::Type::contains_inference) {
        return Ok(MethodSelection::Unresolved);
    }
    if receiver.contains_any() || arguments.iter().any(crate::sema::types::Type::contains_any) {
        return Ok(MethodSelection::Dynamic);
    }
    if receiver.contains_recovery() || arguments.iter().any(crate::sema::types::Type::contains_recovery) {
        return Ok(MethodSelection::Recovery);
    }
    match crate::modules::api_spec().select_method_requirement(
        requirement.kind,
        &receiver,
        &requirement.argument_slots,
        &arguments,
    ) {
        crate::modules::signature::MethodFamilySelection::Selected(method) => Ok(MethodSelection::Selected(method)),
        crate::modules::signature::MethodFamilySelection::Unsupported => Ok(MethodSelection::Unsupported),
        crate::modules::signature::MethodFamilySelection::Ambiguous => Ok(MethodSelection::Ambiguous),
    }
}

fn generalize_method_family(
    constraints: &TypeConstraints,
    requirement: &MethodFamilyRequirement<crate::sema::types::Type>,
    environment_level: u32,
    generalization: &mut Generalization,
) -> Result<MethodFamilyRequirement<SchemeType>, super::SchemeOperationError> {
    Ok(MethodFamilyRequirement {
        kind: requirement.kind,
        receiver: constraints.generalize_type(&requirement.receiver, environment_level, generalization, 0)?,
        argument_slots: requirement.argument_slots.clone(),
        arguments: requirement.arguments.iter()
            .map(|ty| constraints.generalize_type(ty, environment_level, generalization, 0))
            .collect::<Result<_, _>>()?,
        result: constraints.generalize_type(&requirement.result, environment_level, generalization, 0)?,
    })
}

#[derive(Clone, Debug)]
pub(crate) enum MethodConstraintError {
    Resolution(ConstraintResolutionError),
    Constraint(super::ConstraintConflict),
    Unsupported { kind: super::super::schemes::MethodRequirementKind, receiver: crate::sema::types::Type },
}

impl From<ConstraintResolutionError> for MethodConstraintError {
    fn from(error: ConstraintResolutionError) -> Self { Self::Resolution(error) }
}

impl std::fmt::Display for MethodConstraintError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Resolution(error) => write!(formatter, "method type resolution failed: {error:?}"),
            Self::Constraint(error) => write!(formatter, "method requires {}, found {}", error.expected, error.actual),
            Self::Unsupported { kind, receiver } => write!(formatter, "{kind} has no canonical result for {receiver}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::types::Type;

    #[test]
    fn method_results_follow_late_roots_and_restore_failed_transactions() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let span = Span::at(crate::source::SourceId::new(0), 0);
            let mut constraints = TypeConstraints::default();
            let receiver = constraints.fresh_at_level(span, 1);
            let result = constraints.fresh_at_level(span, 1);
            let anchor = constraints.fresh_at_level(span, 1);
            let peer = constraints.fresh_at_level(span, 1);
            constraints.constrain(&anchor, &peer, span).unwrap();
            let id = constraints.register_method_family(super::super::CallableApplicationScope::Invocation, MethodFamilyRequirement {
                kind: crate::sema::schemes::MethodRequirementKind::Family(xsh_registry::signature::MethodFamilyId::PathParent),
                receiver: receiver.clone(), result: result.clone(), argument_slots: Vec::new(), arguments: Vec::new(),
            }, None, span).unwrap();
            constraints.solve_callable_applications().unwrap();
            assert!(constraints.unresolved_method_result(&result).unwrap());
            constraints.constrain(&receiver, &anchor, span).unwrap();
            let Type::Inference(root) = constraints.resolve(&anchor).unwrap() else { unreachable!() };
            assert!(constraints.methods.watchers[&ApplicationDependency::Type(root)].contains(&id));
            let before = constraints.clone();
            let rejected = constraints.commit_constraint_transaction(|this| {
                this.constrain(&receiver, &Type::Path, span).map_err(super::super::CallableApplicationError::Constraint)?;
                this.constrain(&result, &Type::Bool, span).map_err(super::super::CallableApplicationError::Constraint)?;
                this.solve_callable_applications()
            });
            assert!(rejected.is_err());
            assert_eq!(constraints.methods, before.methods);
            assert_eq!(constraints.resolve(&receiver).unwrap(), before.resolve(&receiver).unwrap());
            assert_eq!(constraints.resolve(&result).unwrap(), before.resolve(&result).unwrap());
            constraints.constrain(&receiver, &Type::rel_path(), span).unwrap();
            constraints.solve_callable_applications().unwrap();
            assert_eq!(constraints.resolve(&result).unwrap(), Type::rel_path());
            assert!(!constraints.unresolved_method_result(&result).unwrap());
        });
    }
}
