use super::{ConstraintChange, ConstraintResolutionError, Generalization, TypeConstraints};
use crate::sema::schemes::{CallableScheme, MethodFamilyRequirement, SchemeType};
use crate::source::Span;
use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_METHOD: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct SealedMethodId(NonZeroU64);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ScopedMethod {
    owner: super::CallableApplicationScope,
    requirement: MethodFamilyRequirement<crate::sema::types::Type>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct SealedMethods {
    entries: BTreeMap<SealedMethodId, ScopedMethod>,
    source_sites: BTreeMap<(super::CallableApplicationScope, Span), SealedMethodId>,
}

#[derive(Clone, Debug)]
pub(super) enum MethodChange {
    Fresh(SealedMethodId, Option<(super::CallableApplicationScope, Span)>),
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
    }

    pub(super) fn rollback(&mut self, change: MethodChange) {
        match change {
            MethodChange::Fresh(id, source) => {
                self.entries.remove(&id).expect("method requirement belongs to its journal");
                if let Some(source) = source {
                    assert_eq!(self.source_sites.remove(&source), Some(id));
                }
            }
        }
    }
}

impl TypeConstraints {
    pub(crate) fn register_method_family(
        &mut self,
        owner: super::CallableApplicationScope,
        requirement: MethodFamilyRequirement<crate::sema::types::Type>,
        source: Option<Span>,
    ) -> Result<SealedMethodId, ConstraintResolutionError> {
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
            assert!(this.methods.entries.insert(id, ScopedMethod { owner, requirement }).is_none());
            if let Some(source) = source {
                assert!(this.methods.source_sites.insert(source, id).is_none());
            }
            changes.push(ConstraintChange::Method(MethodChange::Fresh(id, source)));
            Ok::<_, ConstraintResolutionError>(())
        })?;
        Ok(id)
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
        if entry.owner != super::CallableApplicationScope::Declaration(owner) || scope.owner != entry.owner {
            return Err(ConstraintResolutionError::ForeignVariable.into());
        }
        self.validate_generalized_methods(callable, scope)?;
        Ok(scope.methods.get(&id).copied())
    }

    pub(super) fn generalize_method_families(
        &self,
        owners: &BTreeSet<crate::sema::check::EffectDeclarationId>,
        environment_level: u32,
        generalization: &mut Generalization,
    ) -> Result<Vec<MethodFamilyRequirement<SchemeType>>, super::SchemeOperationError> {
        let mut requirements = Vec::new();
        for (&id, entry) in &self.methods.entries {
            if !matches!(entry.owner, super::CallableApplicationScope::Declaration(owner) if owners.contains(&owner)) {
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
        let mut methods = scope.methods.iter().map(|(&id, &ordinal)| (ordinal, id)).collect::<Vec<_>>();
        methods.sort_unstable();
        if methods.len() != callable.requirements().methods.len() {
            return Err(super::SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedMethodFamily));
        }
        let mut generalization = self.callable_generalization(callable, scope);
        for (index, &(ordinal, id)) in methods.iter().enumerate() {
            let entry = self.methods.entries.get(&id).ok_or(ConstraintResolutionError::ForeignVariable)?;
            if index != ordinal as usize
                || generalize_method_family(self, &entry.requirement, 0, &mut generalization)? != callable.requirements().methods[index]
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
        instance: &super::InstantiatedScheme,
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
    match crate::modules::api_spec().select_method_family(
        requirement.family,
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
        family: requirement.family,
        receiver: constraints.generalize_type(&requirement.receiver, environment_level, generalization, 0)?,
        argument_slots: requirement.argument_slots.clone(),
        arguments: requirement.arguments.iter()
            .map(|ty| constraints.generalize_type(ty, environment_level, generalization, 0))
            .collect::<Result<_, _>>()?,
        result: constraints.generalize_type(&requirement.result, environment_level, generalization, 0)?,
    })
}
