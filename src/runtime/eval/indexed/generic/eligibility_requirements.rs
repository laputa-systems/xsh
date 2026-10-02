use super::*;
use crate::sema::inference::{Eligibility, ScopedRequirementRoot};

/// The original declaration fixes the predicate and binder independently of
/// each concrete caller's eligibility witness.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct OriginalScopedEligibility {
    pub scope: SchemeScopeId,
    pub requirement: u32,
    pub original: ScopedRequirementRoot,
    pub predicate: Eligibility,
    pub ty: TypeRef,
}

#[derive(Clone, Debug, Default)]
pub(super) struct EligibilityEvidence {
    sources: Vec<Entry<Arc<OriginalScopedEligibility>>>,
    originals: Vec<Arc<OriginalScopedEligibility>>,
}

impl EligibilityEvidence {
    pub(super) fn checkpoint(&self) -> usize { self.sources.len() }
    pub(super) fn validate_checkpoint(&self, count: usize, serial_limit: u64) -> Result<(), IrVerifyError> {
        if count > self.sources.len() { return Err(failure("eligibility checkpoint references retired entries")); }
        if self.sources.get(count.wrapping_sub(1)).is_some_and(|entry| entry.serial >= serial_limit) {
            return Err(failure("eligibility checkpoint references replacement entries"));
        }
        Ok(())
    }
    pub(super) fn rewind_validated(&mut self, count: usize) { self.sources.truncate(count); self.originals.truncate(count); }
    pub(super) fn shrink_to_fit(&mut self) { self.sources.shrink_to_fit(); self.originals.shrink_to_fit(); }
    pub(super) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        self.sources.capacity() * size_of::<Entry<Arc<OriginalScopedEligibility>>>()
            + self.originals.capacity() * size_of::<Arc<OriginalScopedEligibility>>()
            + self.originals.len() * (size_of::<OriginalScopedEligibility>() + 2 * size_of::<usize>())
    }
}

impl GenericEvidenceBuilder {
    pub(in crate::runtime::eval) fn add_original_eligibility(&mut self, value: OriginalScopedEligibility) -> Result<(), IrVerifyError> {
        if value.predicate != Eligibility::Display || self.store.eligibility.sources.len() >= 2_000_000 {
            return Err(failure("original eligibility is not prepared"));
        }
        let serial = self.next_serial;
        self.next_serial = serial.checked_add(1).ok_or_else(|| failure("generic evidence serial overflow"))?;
        let value = Arc::new(value);
        self.store.eligibility.originals.push(Arc::clone(&value));
        self.store.eligibility.sources.push(Entry { serial, value });
        Ok(())
    }
}

impl GenericEvidenceStore {
    pub(super) fn verify_original_eligibility(&self, pools: &SemanticPools) -> Result<(), IrVerifyError> {
        if self.eligibility.sources.len() != self.eligibility.originals.len() {
            return Err(failure("original eligibility ledger is incomplete"));
        }
        let mut actual = std::collections::BTreeSet::new();
        for (entry, original) in self.eligibility.sources.iter().zip(&self.eligibility.originals) {
            if !Arc::ptr_eq(&entry.value, original) { return Err(failure("eligibility differs from its original receipt")); }
            let source = entry.value.as_ref();
            let scope = self.scope(source.scope)?;
            if source.predicate != Eligibility::Display || source.original.scope.is_none()
                || scope.requirements.get(source.requirement as usize) != Some(&Requirement::Eligibility { predicate: source.predicate, ty: source.ty })
                || !actual.insert((source.scope.index, source.requirement)) {
                return Err(failure("eligibility changes its original scoped requirement"));
            }
            self.verify_reference(pools, scope, source.ty)?;
        }
        let expected = self.scopes().flat_map(|(id, scope)| scope.requirements.iter().enumerate().filter_map(move |(index, requirement)| {
            matches!(requirement, Requirement::Eligibility { .. }).then_some((id.index, index as u32))
        })).collect::<std::collections::BTreeSet<_>>();
        if actual != expected { return Err(failure("scoped eligibility has no original authority")); }
        Ok(())
    }

    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_eligibility_instance_mut(&mut self, id: InstantiationId) -> Result<&mut Instantiation, IrVerifyError> {
        self.instance(id)?;
        Ok(&mut self.instances[id.index as usize].value)
    }

    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_remove_eligibility_sources(&mut self) { self.eligibility.sources.clear(); }
}
