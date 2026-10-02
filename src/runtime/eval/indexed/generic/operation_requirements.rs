use super::*;
use crate::sema::inference::{EffectSet, RequirementId};
use crate::sema::operation_graph::{PreparedLanguageOperation, ValueConstructor};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) struct ScopedOperationSourceId { index: u32, proof: OwnerProof }

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) struct ScopedOperationWitnessId { index: u32, proof: OwnerProof }

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct ScopedOperationRequirement {
    pub authority: PreparedOperationAuthority,
    pub signature: TypeRef,
    pub arguments: Box<[TypeRef]>,
    pub result: TypeRef,
    pub effects: PreparedOperationEffects,
}

/// The source instruction and its binder relationships survive independently
/// of the concrete certificate supplied by each caller.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct ScopedOperationSource {
    pub origin: crate::sema::check::ExpressionIdentity,
    pub instruction: u32,
    pub scope: SchemeScopeId,
    pub requirement: u32,
    pub original_requirement: RequirementId,
    pub expected: ScopedOperationRequirement,
    pub arguments: Box<[PreparedInvocationArgument]>,
    pub obligations: Box<[ScopedOperationObligation]>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct ScopedOperationObligation {
    pub scope: SchemeScopeId,
    pub requirement: u32,
    pub immediate_original: RequirementId,
    pub ancestry: Box<[RequirementId]>,
    pub expected: ScopedOperationRequirement,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) enum ScopedOperationCode { ResultOk }

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct ScopedOperationWitness {
    pub source: ScopedOperationSourceId,
    pub signature: GroundTypeId,
    pub arguments: Box<[GroundTypeId]>,
    pub result: GroundTypeId,
    pub operation: ScopedOperationCode,
    pub authority: PreparedOperationAuthority,
    pub effects: PreparedOperationEffects,
}

#[derive(Clone, Debug, Default)]
pub(super) struct OperationRequirementEvidence {
    sources: Vec<Entry<Arc<ScopedOperationSource>>>,
    originals: Vec<Arc<ScopedOperationSource>>,
    witnesses: Vec<Entry<ScopedOperationWitness>>,
    instructions: Vec<(u32, ScopedOperationSourceId)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct OperationRequirementCheckpoint { sources: usize, witnesses: usize }

impl OperationRequirementEvidence {
    pub(super) fn checkpoint(&self) -> OperationRequirementCheckpoint { OperationRequirementCheckpoint { sources: self.sources.len(), witnesses: self.witnesses.len() } }
    pub(super) fn validate_checkpoint(&self, checkpoint: OperationRequirementCheckpoint, serial_limit: u64) -> Result<(), IrVerifyError> {
        if checkpoint.sources > self.sources.len() || checkpoint.witnesses > self.witnesses.len() { return Err(failure("scoped operation checkpoint references retired entries")); }
        for serial in [self.sources.get(checkpoint.sources.wrapping_sub(1)).map(|entry| entry.serial), self.witnesses.get(checkpoint.witnesses.wrapping_sub(1)).map(|entry| entry.serial)].into_iter().flatten() {
            if serial >= serial_limit { return Err(failure("scoped operation checkpoint references replacement entries")); }
        }
        Ok(())
    }
    pub(super) fn rewind_validated(&mut self, checkpoint: OperationRequirementCheckpoint) {
        self.sources.truncate(checkpoint.sources); self.originals.truncate(checkpoint.sources); self.witnesses.truncate(checkpoint.witnesses); self.instructions.clear();
    }
    pub(super) fn finish(&mut self, root: u64) {
        self.instructions = self.sources.iter().enumerate().map(|(index, entry)| (entry.value.instruction, ScopedOperationSourceId { index: index as u32, proof: OwnerProof { root, serial: entry.serial } })).collect();
        self.instructions.sort_unstable_by_key(|entry| entry.0);
    }
    pub(super) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        self.sources.capacity() * size_of::<Entry<Arc<ScopedOperationSource>>>() + self.originals.capacity() * size_of::<Arc<ScopedOperationSource>>()
            + self.sources.len() * (size_of::<ScopedOperationSource>() + 2 * size_of::<usize>())
            + self.sources.iter().map(|entry| entry.value.expected.retained_bytes() + entry.value.arguments.len() * size_of::<PreparedInvocationArgument>()
                + entry.value.obligations.len() * size_of::<ScopedOperationObligation>() + entry.value.obligations.iter().map(|obligation| obligation.expected.retained_bytes() + obligation.ancestry.len() * size_of::<RequirementId>()).sum::<usize>()).sum::<usize>()
            + self.witnesses.capacity() * size_of::<Entry<ScopedOperationWitness>>()
            + self.witnesses.iter().map(|entry| entry.value.arguments.len() * size_of::<GroundTypeId>() + entry.value.authority.retained_bytes() + effects_bytes(&entry.value.effects)).sum::<usize>()
            + self.instructions.capacity() * size_of::<(u32, ScopedOperationSourceId)>()
    }
    pub(super) fn shrink_to_fit(&mut self) { self.sources.shrink_to_fit(); self.originals.shrink_to_fit(); self.witnesses.shrink_to_fit(); self.instructions.shrink_to_fit(); }
}

fn effects_bytes(effects: &PreparedOperationEffects) -> usize {
    effects.inputs.len() * std::mem::size_of::<(crate::sema::inference::EffectRole, EffectSet)>() + effects.outputs.len() * std::mem::size_of::<(crate::sema::inference::ProducerRole, EffectSet)>()
}

impl ScopedOperationRequirement {
    pub(super) fn retained_bytes(&self) -> usize { self.arguments.len() * std::mem::size_of::<TypeRef>() + self.authority.retained_bytes() + effects_bytes(&self.effects) }
    pub(in crate::runtime::eval) fn references(&self) -> impl Iterator<Item = TypeRef> + '_ { [self.signature, self.result].into_iter().chain(self.arguments.iter().copied()) }
    pub(in crate::runtime::eval) fn rebase(&self, mut reference: impl FnMut(TypeRef) -> Result<TypeRef, super::super::IrBuildError>) -> Result<Self, super::super::IrBuildError> {
        Ok(Self { authority: self.authority.clone(), signature: reference(self.signature)?, result: reference(self.result)?, arguments: self.arguments.iter().map(|&ty| reference(ty)).collect::<Result<Vec<_>, _>>()?.into_boxed_slice(), effects: self.effects.clone() })
    }
    pub(super) fn verify_supported(&self) -> Result<(), IrVerifyError> {
        if !matches!(self.authority, PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Constructor { kind: ValueConstructor::Ok, arity: 1 }, argument_order: crate::sema::operation_graph::OperationArgumentOrder::SourceOrder, statement_result_is_unit: false, .. })
            || self.arguments.len() != 1 || self.effects.creation != EffectSet::EMPTY || !self.effects.inputs.is_empty() || !self.effects.outputs.is_empty() {
            return Err(failure("scoped operation authority is not prepared"));
        }
        Ok(())
    }
}

impl GenericEvidenceStore {
    pub fn scoped_operation_source(&self, id: ScopedOperationSourceId) -> Result<&ScopedOperationSource, IrVerifyError> {
        let source = owned(self.root, &self.operation_requirements.sources, id.index, id.proof)?;
        if !self.operation_requirements.originals.get(id.index as usize).is_some_and(|original| Arc::ptr_eq(source, original)) { return Err(failure("scoped operation differs from its original receipt")); }
        Ok(source.as_ref())
    }
    pub fn scoped_operation_witness(&self, id: ScopedOperationWitnessId) -> Result<&ScopedOperationWitness, IrVerifyError> { owned(self.root, &self.operation_requirements.witnesses, id.index, id.proof) }
    pub fn scoped_operation_sources(&self) -> impl Iterator<Item = (ScopedOperationSourceId, &ScopedOperationSource)> {
        self.operation_requirements.sources.iter().enumerate().map(|(index, entry)| (ScopedOperationSourceId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } }, entry.value.as_ref()))
    }
    pub fn scoped_operation_source_at(&self, instruction: u32) -> Result<Option<ScopedOperationSourceId>, IrVerifyError> {
        self.operation_requirements.instructions.binary_search_by_key(&instruction, |entry| entry.0).ok().map(|index| {
            let id = self.operation_requirements.instructions[index].1; self.scoped_operation_source(id)?; Ok(id)
        }).transpose()
    }
    /// Execution selects a previously verified numeric operation from its
    /// active frame; no value type search participates in this lookup.
    pub fn scoped_operation_authority(&self, instruction: u32, instance: Option<InstantiationId>) -> Result<Option<ScopedOperationCode>, IrVerifyError> {
        let Some(source_id) = self.scoped_operation_source_at(instruction)? else { return Ok(None); };
        let source = self.scoped_operation_source(source_id)?;
        let instance = self.instance(instance.ok_or_else(|| failure("scoped operation requires its active instance"))?)?;
        if instance.scope != source.scope { return Err(failure("scoped operation uses another declaration's instance")); }
        let Some(RequirementWitness::Operation(id)) = instance.requirements.get(source.requirement as usize) else { return Err(failure("scoped operation witness is missing")); };
        let witness = self.scoped_operation_witness(*id)?;
        if witness.source != source_id { return Err(failure("scoped operation uses another original obligation")); }
        Ok(Some(witness.operation))
    }
    pub(super) fn verify_scoped_operation_evidence(&self, pools: &SemanticPools, owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        if self.operation_requirements.sources.len() != self.operation_requirements.originals.len() { return Err(failure("scoped operation original ledger is incomplete")); }
        let mut instructions = Vec::new();
        for (id, _) in self.scoped_operation_sources() {
            let source = self.scoped_operation_source(id)?;
            let scope = self.scope(source.scope)?;
            source.expected.verify_supported()?;
            if scope.requirements.get(source.requirement as usize) != Some(&Requirement::Operation(source.expected.clone()))
                || owners.get(source.instruction as usize) != Some(&Some(InstructionOwner::Function(scope.owner)))
                || self.registered_instruction_origin(source.instruction, false) != Some((OperationSourceOrigin::Expression(source.origin), InstructionOwner::Function(scope.owner)))
                || source.arguments.len() != source.expected.arguments.len() { return Err(failure("scoped operation changes its original requirement or owner")); }
            for (argument, &expected) in source.arguments.iter().zip(&source.expected.arguments) {
                if argument.ty != expected || argument.original.name.is_some()
                    || owners.get(argument.instruction as usize) != Some(&Some(InstructionOwner::Function(scope.owner))) { return Err(failure("scoped operation changes its original operand")); }
            }
            for reference in source.expected.references() { self.verify_reference(pools, scope, reference)?; }
            let mut obligations = std::collections::BTreeSet::new();
            let mut has_original = false;
            for obligation in &source.obligations {
                let member = self.scope(obligation.scope)?;
                obligation.expected.verify_supported()?;
                if !obligations.insert((member.owner.raw(), obligation.requirement)) || member.requirements.get(obligation.requirement as usize) != Some(&Requirement::Operation(obligation.expected.clone()))
                    || obligation.ancestry.is_empty() || obligation.ancestry.len() > 256 || obligation.ancestry.first() != Some(&obligation.immediate_original) || obligation.ancestry.last() != Some(&source.original_requirement)
                    || obligation.ancestry.iter().collect::<std::collections::HashSet<_>>().len() != obligation.ancestry.len() { return Err(failure("scoped operation forwarding changes its original obligation ancestry")); }
                for reference in obligation.expected.references() { self.verify_reference(pools, member, reference)?; }
                has_original |= obligation.scope == source.scope && obligation.requirement == source.requirement && obligation.expected == source.expected && obligation.immediate_original == source.original_requirement;
            }
            if !has_original { return Err(failure("scoped operation original obligation is missing")); }
            instructions.push((source.instruction, id));
        }
        instructions.sort_unstable_by_key(|entry| entry.0);
        if instructions.windows(2).any(|pair| pair[0].0 == pair[1].0) || instructions != self.operation_requirements.instructions { return Err(failure("scoped operation instruction index is ambiguous or stale")); }
        Ok(())
    }
    pub(super) fn verify_scoped_operation_witness(&self, pools: &SemanticPools, scope: &SchemeScope, requirement: &ScopedOperationRequirement, requirement_index: usize, substitutions: &[GroundTypeId], id: ScopedOperationWitnessId) -> Result<(), IrVerifyError> {
        let witness = self.scoped_operation_witness(id)?;
        let source = self.scoped_operation_source(witness.source)?;
        requirement.verify_supported()?;
        let obligation = source.obligations.iter().find(|obligation| self.scope(obligation.scope).is_ok_and(|member| member.owner == scope.owner) && obligation.requirement as usize == requirement_index).ok_or_else(|| failure("scoped operation witness has no original obligation in this scope"))?;
        if obligation.expected != *requirement || witness.authority != requirement.authority || witness.effects != requirement.effects
            || scope.requirements.get(requirement_index) != Some(&Requirement::Operation(requirement.clone()))
            || witness.operation != ScopedOperationCode::ResultOk || witness.arguments.len() != requirement.arguments.len() { return Err(failure("scoped operation witness changes its original authority")); }
        for (reference, actual) in requirement.references().zip([witness.signature, witness.result].into_iter().chain(witness.arguments.iter().copied())) {
            if self.instantiated_normalized_type(pools, scope, reference, substitutions)? != Self::normalized_ground_type(pools, actual)? { return Err(failure("scoped operation witness changes a declaration binder")); }
        }
        let (kind, signature) = pools.callable_descriptor(witness.signature)?.ok_or_else(|| failure("scoped operation signature is not callable"))?;
        if kind != CallableKind::Pure || pools.signature_closed_effects(signature)? != EffectSet::EMPTY || pools.signature_param_count(signature)? != 1
            || pools.signature_param(signature, 0)?.1 != witness.arguments[0] || pools.signature_return_type(signature)? != witness.result
            || pools.type_tag(witness.result)? != TypeTag::Result || pools.type_children(witness.result)?.map(|children| children.0) != Some(witness.arguments[0]) { return Err(failure("scoped operation concrete signature changes its constructor relation")); }
        Ok(())
    }
}

impl GenericEvidenceBuilder {
    pub fn add_scoped_operation_source(&mut self, value: ScopedOperationSource) -> Result<ScopedOperationSourceId, IrVerifyError> {
        value.expected.verify_supported()?;
        if value.obligations.len() > 2_000_000 || self.store.operation_requirements.sources.len() >= 2_000_000 { return Err(failure("scoped operation sources exceed their work limit")); }
        let index = u32::try_from(self.store.operation_requirements.sources.len()).map_err(|_| failure("scoped operation source id overflow"))?;
        let serial = self.next_serial; self.next_serial = serial.checked_add(1).ok_or_else(|| failure("generic evidence serial overflow"))?;
        let value = Arc::new(value); self.store.operation_requirements.originals.push(Arc::clone(&value)); self.store.operation_requirements.sources.push(Entry { serial, value });
        Ok(ScopedOperationSourceId { index, proof: OwnerProof { root: self.store.root, serial } })
    }
    pub fn add_scoped_operation_witness(&mut self, value: ScopedOperationWitness) -> Result<ScopedOperationWitnessId, IrVerifyError> {
        self.store.scoped_operation_source(value.source)?;
        if value.arguments.len() != 1 || self.store.operation_requirements.witnesses.len() >= 2_000_000 { return Err(failure("scoped operation witnesses exceed their work limit")); }
        if let Some((index, entry)) = self.store.operation_requirements.witnesses.iter().enumerate().find(|(_, entry)| entry.value == value) {
            return Ok(ScopedOperationWitnessId { index: index as u32, proof: OwnerProof { root: self.store.root, serial: entry.serial } });
        }
        let index = u32::try_from(self.store.operation_requirements.witnesses.len()).map_err(|_| failure("scoped operation witness id overflow"))?;
        let serial = self.next_serial; self.next_serial = serial.checked_add(1).ok_or_else(|| failure("generic evidence serial overflow"))?;
        self.store.operation_requirements.witnesses.push(Entry { serial, value });
        Ok(ScopedOperationWitnessId { index, proof: OwnerProof { root: self.store.root, serial } })
    }
    pub(in crate::runtime::eval) fn scoped_operation_sources(&self) -> impl Iterator<Item = (ScopedOperationSourceId, &ScopedOperationSource)> { self.store.scoped_operation_sources() }
}

#[cfg(test)]
impl GenericEvidenceStore {
    pub(in crate::runtime::eval) fn test_scoped_operation_source_mut(&mut self, id: ScopedOperationSourceId) -> Result<&mut ScopedOperationSource, IrVerifyError> {
        self.scoped_operation_source(id)?; Ok(Arc::make_mut(&mut self.operation_requirements.sources[id.index as usize].value))
    }
    pub(in crate::runtime::eval) fn test_scoped_operation_witness_mut(&mut self, id: ScopedOperationWitnessId) -> Result<&mut ScopedOperationWitness, IrVerifyError> {
        self.scoped_operation_witness(id)?; Ok(&mut self.operation_requirements.witnesses[id.index as usize].value)
    }
}
