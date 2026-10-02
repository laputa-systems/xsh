use super::*;
use crate::modules::signature::MethodReceiver;
use crate::sema::inference::{RequirementId, EffectSet};
use crate::sema::registry_graph::RegistryOwner;
use crate::sema::types::Type;
use crate::modules::RuntimeOp;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) struct ScopedNativeMethodSourceId { index: u32, proof: OwnerProof }
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) struct ScopedNativeMethodWitnessId { index: u32, proof: OwnerProof }

/// A pending method retains its original receiver family. Each caller supplies
/// a certificate for one member without choosing a member for the declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct ScopedNativeMethodRequirement {
    pub receiver: TypeRef,
    pub arguments: Box<[TypeRef]>,
    pub result: TypeRef,
    pub candidates: Box<[NativeCallableContract]>,
    pub parameter_labels: [Name; 2],
}
impl ScopedNativeMethodRequirement {
    pub(in crate::runtime::eval) fn references(&self) -> impl Iterator<Item = TypeRef> + '_ { std::iter::once(self.receiver).chain(self.arguments.iter().copied()).chain([self.result]) }
    pub(in crate::runtime::eval) fn retained_bytes(&self) -> usize {
        self.arguments.len() * size_of::<TypeRef>() + self.candidates.iter().map(|candidate| size_of::<NativeCallableContract>() + candidate.authority.retained_bytes()
            + effect_bytes(&candidate.effects) + candidate.argument_relations.len() * size_of::<crate::sema::inference::ArgumentRelation>() + candidate.input_eligibility.len() * size_of::<(usize, crate::sema::inference::Eligibility)>()).sum::<usize>()
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct ScopedNativeMethodReceiver { pub origin: ExpressionIdentity, pub instruction: u32, pub ty: TypeRef }
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct ScopedNativeMethodObligation {
    pub scope: SchemeScopeId, pub requirement: u32, pub immediate_original: RequirementId, pub ancestry: Box<[RequirementId]>, pub expected: ScopedNativeMethodRequirement,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct ScopedNativeMethodSource {
    pub origin: ExpressionIdentity, pub instruction: u32, pub scope: SchemeScopeId, pub requirement: u32, pub original_requirement: RequirementId,
    pub expected: ScopedNativeMethodRequirement, pub receiver: ScopedNativeMethodReceiver, pub arguments: Box<[PreparedInvocationArgument]>,
    pub payload: Box<[u32]>, pub argument_block: IrBlockId, pub argument_payload: Box<[u32]>, pub method_name: Name,
    pub obligations: Box<[ScopedNativeMethodObligation]>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct ScopedNativeMethodWitness {
    pub source: ScopedNativeMethodSourceId, pub candidate: NativeCallableContract, pub receiver: GroundTypeId, pub arguments: Box<[GroundTypeId]>, pub result: GroundTypeId,
}
#[derive(Clone, Debug, Default)]
pub(super) struct ScopedNativeMethodEvidence {
    sources: Vec<Entry<Arc<ScopedNativeMethodSource>>>, originals: Vec<Arc<ScopedNativeMethodSource>>,
    witnesses: Vec<Entry<ScopedNativeMethodWitness>>, instructions: Vec<(u32, ScopedNativeMethodSourceId)>,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct ScopedNativeMethodCheckpoint { sources: usize, witnesses: usize }
impl ScopedNativeMethodEvidence {
    pub(super) fn retained_bytes(&self) -> usize {
        self.sources.capacity() * size_of::<Entry<Arc<ScopedNativeMethodSource>>>() + self.originals.capacity() * size_of::<Arc<ScopedNativeMethodSource>>()
            + self.witnesses.capacity() * size_of::<Entry<ScopedNativeMethodWitness>>() + self.instructions.capacity() * size_of::<(u32, ScopedNativeMethodSourceId)>()
            + self.sources.iter().map(|entry| { let source = &entry.value; size_of::<ScopedNativeMethodSource>() + 2 * size_of::<usize>() + source.expected.retained_bytes()
                + source.arguments.len() * size_of::<PreparedInvocationArgument>() + (source.payload.len() + source.argument_payload.len()) * size_of::<u32>()
                + source.obligations.iter().map(|obligation| size_of::<ScopedNativeMethodObligation>() + obligation.ancestry.len() * size_of::<RequirementId>() + obligation.expected.retained_bytes()).sum::<usize>() }).sum::<usize>()
            + self.witnesses.iter().map(|entry| entry.value.arguments.len() * size_of::<GroundTypeId>() + entry.value.candidate.authority.retained_bytes() + effect_bytes(&entry.value.candidate.effects)
                + entry.value.candidate.argument_relations.len() * size_of::<crate::sema::inference::ArgumentRelation>() + entry.value.candidate.input_eligibility.len() * size_of::<(usize, crate::sema::inference::Eligibility)>()).sum::<usize>()
    }
    pub(super) fn shrink_to_fit(&mut self) { self.sources.shrink_to_fit(); self.originals.shrink_to_fit(); self.witnesses.shrink_to_fit(); self.instructions.shrink_to_fit(); }
    pub(super) fn checkpoint(&self) -> ScopedNativeMethodCheckpoint { ScopedNativeMethodCheckpoint { sources: self.sources.len(), witnesses: self.witnesses.len() } }
    pub(super) fn validate_checkpoint(&self, checkpoint: ScopedNativeMethodCheckpoint, serial: u64) -> Result<(), IrVerifyError> {
        if checkpoint.sources > self.sources.len() || checkpoint.witnesses > self.witnesses.len()
            || self.sources.get(checkpoint.sources.wrapping_sub(1)).is_some_and(|entry| entry.serial >= serial)
            || self.witnesses.get(checkpoint.witnesses.wrapping_sub(1)).is_some_and(|entry| entry.serial >= serial) { return Err(failure("native method checkpoint refers to retired evidence")); }
        Ok(())
    }
    pub(super) fn rewind_validated(&mut self, checkpoint: ScopedNativeMethodCheckpoint) { self.sources.truncate(checkpoint.sources); self.originals.truncate(checkpoint.sources); self.witnesses.truncate(checkpoint.witnesses); self.instructions.clear(); }
    pub(super) fn finish_indexes(&mut self, root: u64) {
        self.instructions = self.sources.iter().enumerate().map(|(index, entry)| (entry.value.instruction, ScopedNativeMethodSourceId { index: index as u32, proof: OwnerProof { root, serial: entry.serial } })).collect();
        self.instructions.sort_unstable_by_key(|entry| entry.0);
    }
}

fn candidate_domain(pools: &SemanticPools, candidate: &NativeCallableContract) -> Result<Type, IrVerifyError> {
    let domain = match (candidate.registry_owner, &candidate.authority) {
        (RegistryOwner::Method(MethodReceiver::Str), PreparedOperationAuthority::Registry { operation: RuntimeOp::TextStartsWith, binding: crate::modules::signature::ImplBinding::Native, argument_check: crate::modules::signature::ApiArgCheck::Standard,
            semantic_rule: crate::modules::signature::SemanticRule::Standard, lifecycle: crate::sema::registry_graph::RegistryLifecycle::None, producer_transfer: crate::sema::registry_graph::RegistryProducerTransferPlan::Empty, .. }) => Type::Str,
        (RegistryOwner::Method(MethodReceiver::Bytes), PreparedOperationAuthority::Registry { operation: RuntimeOp::BytesStartsWith, binding: crate::modules::signature::ImplBinding::Native, argument_check: crate::modules::signature::ApiArgCheck::Standard,
            semantic_rule: crate::modules::signature::SemanticRule::Standard, lifecycle: crate::sema::registry_graph::RegistryLifecycle::None, producer_transfer: crate::sema::registry_graph::RegistryProducerTransferPlan::Empty, .. }) => Type::Bytes,
        _ => return Err(failure("scoped native method changes its canonical receiver family")),
    };
    if candidate.kind != CallableKind::Pure || candidate.effects.creation != EffectSet::EMPTY || !candidate.effects.inputs.is_empty() || !candidate.effects.outputs.is_empty()
        || !candidate.input_eligibility.is_empty() || candidate.argument_relations.len() != 2 || pools.signature_param_count(candidate.signature)? != 2
        || pools.signature_closed_effects(candidate.signature)? != EffectSet::EMPTY || pools.to_type(pools.signature_return_type(candidate.signature)?)? != Type::Bool {
        return Err(failure("scoped native method changes its canonical signature or effects"));
    }
    for index in 0..2 { if pools.signature_parameter_defaulted(candidate.signature, index)? || pools.signature_parameter_rest(candidate.signature, index)?
        || pools.to_type(pools.signature_param(candidate.signature, index)?.1)? != domain { return Err(failure("scoped native method changes its exact receiver and prefix domains")); } }
    Ok(domain)
}
impl GenericEvidenceStore {
    pub(in crate::runtime::eval) fn verify_scoped_native_method_requirement(&self, pools: &SemanticPools, scope: &SchemeScope, requirement: &ScopedNativeMethodRequirement) -> Result<(), IrVerifyError> {
        if requirement.arguments.len() != 1 || requirement.candidates.len() != 2 { return Err(failure("scoped native method loses its original family shape")); }
        let domains = requirement.candidates.iter().map(|candidate| candidate_domain(pools, candidate)).collect::<Result<Vec<_>, _>>()?;
        if domains.iter().filter(|ty| **ty == Type::Str).count() != 1 || domains.iter().filter(|ty| **ty == Type::Bytes).count() != 1 { return Err(failure("scoped native method duplicates a receiver family member")); }
        for candidate in &requirement.candidates { for (index, &label) in requirement.parameter_labels.iter().enumerate() {
            if pools.signature_param(candidate.signature, index)?.0 != label { return Err(failure("scoped native method changes its original parameter labels")); }
        } }
        for reference in requirement.references() { self.verify_reference(pools, scope, reference)?; }
        Ok(())
    }
    pub(in crate::runtime::eval) fn scoped_native_method_source(&self, id: ScopedNativeMethodSourceId) -> Result<&ScopedNativeMethodSource, IrVerifyError> {
        let value = owned(self.root, &self.native_callables.methods.sources, id.index, id.proof)?;
        if !self.native_callables.methods.originals.get(id.index as usize).is_some_and(|original| Arc::ptr_eq(value, original)) { return Err(failure("native method changes its original source receipt")); }
        Ok(value)
    }
    pub(in crate::runtime::eval) fn scoped_native_method_sources(&self) -> impl Iterator<Item = (ScopedNativeMethodSourceId, &ScopedNativeMethodSource)> {
        self.native_callables.methods.sources.iter().enumerate().map(|(index, entry)| (ScopedNativeMethodSourceId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } }, entry.value.as_ref()))
    }
    pub(in crate::runtime::eval) fn scoped_native_method_source_at(&self, instruction: u32) -> Result<Option<ScopedNativeMethodSourceId>, IrVerifyError> {
        self.native_callables.methods.instructions.binary_search_by_key(&instruction, |entry| entry.0).ok().map(|index| { let id = self.native_callables.methods.instructions[index].1; self.scoped_native_method_source(id)?; Ok(id) }).transpose()
    }
    pub(in crate::runtime::eval) fn scoped_native_method_witness(&self, id: ScopedNativeMethodWitnessId) -> Result<&ScopedNativeMethodWitness, IrVerifyError> { owned(self.root, &self.native_callables.methods.witnesses, id.index, id.proof) }
    pub(in crate::runtime::eval) fn verify_scoped_native_method_witness(&self, pools: &SemanticPools, scope: &SchemeScope, requirement: &ScopedNativeMethodRequirement, index: usize, substitutions: &[GroundTypeId], id: ScopedNativeMethodWitnessId) -> Result<(), IrVerifyError> {
        self.verify_scoped_native_method_requirement(pools, scope, requirement)?;
        let witness = self.scoped_native_method_witness(id)?;
        let source = self.scoped_native_method_source(witness.source)?;
        if !source.obligations.iter().any(|obligation| self.scope(obligation.scope).is_ok_and(|member| member.owner == scope.owner)
            && obligation.requirement as usize == index && obligation.expected == *requirement) || !source.expected.candidates.contains(&witness.candidate)
            || !requirement.candidates.contains(&witness.candidate) || witness.arguments.len() != 1 { return Err(failure("native method witness belongs to another original requirement")); }
        let domain = candidate_domain(pools, &witness.candidate)?;
        let references = requirement.references();
        for (reference, actual) in references.zip(std::iter::once(witness.receiver).chain(witness.arguments.iter().copied()).chain([witness.result])) {
            if self.instantiated_normalized_type(pools, scope, reference, substitutions)? != Self::normalized_ground_type(pools, actual)? { return Err(failure("native method witness changes its original scoped operand or result")); }
        }
        if pools.to_type(witness.receiver)? != domain || pools.to_type(witness.arguments[0])? != domain || pools.to_type(witness.result)? != Type::Bool { return Err(failure("native method witness changes its selected canonical domains")); }
        Ok(())
    }
    pub(in crate::runtime::eval) fn scoped_native_method_operation(&self, pools: &SemanticPools, instruction: u32, instance: Option<InstantiationId>) -> Result<Option<RuntimeOp>, IrVerifyError> {
        let Some(id) = self.scoped_native_method_source_at(instruction)? else {
            if self.native_callables.methods.originals.iter().any(|source| source.instruction == instruction) { return Err(failure("native method loses its original source proof")); }
            return Ok(None);
        };
        let source = self.scoped_native_method_source(id)?;
        let instance = self.instance(instance.ok_or_else(|| failure("scoped native method lacks its active instantiation"))?)?;
        if instance.scope != source.scope { return Err(failure("scoped native method uses another declaration frame")); }
        let RequirementWitness::NativeMethod(witness) = *instance.requirements.get(source.requirement as usize).ok_or_else(|| failure("native method requirement witness is missing"))? else { return Err(failure("native method has another witness kind")); };
        self.verify_scoped_native_method_witness(pools, self.scope(source.scope)?, &source.expected, source.requirement as usize, &instance.substitutions, witness)?;
        if self.scoped_native_method_witness(witness)?.source != id { return Err(failure("native method witness changes its original body source")); }
        let PreparedOperationAuthority::Registry { operation, .. } = self.scoped_native_method_witness(witness)?.candidate.authority else { return Err(failure("native method witness lacks selected registry authority")); };
        Ok(Some(operation))
    }
    pub(super) fn verify_scoped_native_method_evidence(&self, pools: &SemanticPools, owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        let methods = &self.native_callables.methods;
        if methods.sources.len() != methods.originals.len() { return Err(failure("native method original ledger is incomplete")); }
        let mut instructions = Vec::new();
        let mut original_obligations = std::collections::BTreeSet::new();
        for (id, _) in self.scoped_native_method_sources() {
            let source = self.scoped_native_method_source(id)?;
            let scope = self.scope(source.scope)?;
            let owner = InstructionOwner::Function(scope.owner);
            self.verify_scoped_native_method_requirement(pools, scope, &source.expected)?;
            if owners.get(source.instruction as usize) != Some(&Some(owner)) || self.registered_instruction_origin(source.instruction, false) != Some((OperationSourceOrigin::Expression(source.origin), owner))
                || scope.requirements.get(source.requirement as usize) != Some(&Requirement::NativeMethod(source.expected.clone())) || source.arguments.len() != 1
                || source.receiver.ty != source.expected.receiver || source.arguments[0].ty != source.expected.arguments[0]
                || owners.get(source.receiver.instruction as usize) != Some(&Some(owner)) || self.registered_instruction_origin(source.receiver.instruction, false) != Some((OperationSourceOrigin::Expression(source.receiver.origin), owner))
                || !self.argument_has_original_source(source.arguments[0].instruction, source.origin, 0, &source.arguments[0].original, owner, source.arguments[0].ty)
                || source.payload.len() != 4 || source.payload[0] != source.receiver.instruction || source.payload[2] != source.argument_block.raw()
                || source.argument_payload.as_ref() != [1, source.arguments[0].instruction] || source.obligations.is_empty() || source.obligations.len() > 2_000_000 {
                return Err(failure("native method source changes its original scope, receiver or operand packet"));
            }
            let mut obligations = std::collections::BTreeSet::new();
            for obligation in &source.obligations {
                let member = self.scope(obligation.scope)?;
                self.verify_scoped_native_method_requirement(pools, member, &obligation.expected)?;
                if !obligations.insert((obligation.scope.index, obligation.requirement)) || !original_obligations.insert((obligation.scope.index, obligation.requirement)) || obligation.expected.candidates != source.expected.candidates || obligation.expected.parameter_labels != source.expected.parameter_labels
                    || member.requirements.get(obligation.requirement as usize) != Some(&Requirement::NativeMethod(obligation.expected.clone()))
                    || obligation.ancestry.is_empty() || obligation.ancestry.len() > 256 || obligation.ancestry.first() != Some(&obligation.immediate_original)
                    || obligation.ancestry.last() != Some(&source.original_requirement)
                    || obligation.ancestry.iter().collect::<rustc_hash::FxHashSet<_>>().len() != obligation.ancestry.len() { return Err(failure("native method changes its original requirement ancestry")); }
            }
            if !source.obligations.iter().any(|obligation| obligation.scope == source.scope && obligation.requirement == source.requirement && obligation.immediate_original == source.original_requirement && obligation.expected == source.expected) { return Err(failure("native method loses its definition owned requirement")); }
            instructions.push((source.instruction, id));
        }
        instructions.sort_unstable_by_key(|entry| entry.0);
        if instructions.windows(2).any(|pair| pair[0].0 == pair[1].0) || instructions != methods.instructions { return Err(failure("native method instruction index is ambiguous or stale")); }
        let mut work = 0usize;
        for (scope, member) in self.scopes() { for (index, requirement) in member.requirements.iter().enumerate() {
            work = work.checked_add(1).ok_or_else(|| failure("native method requirement coverage exceeds its bound"))?;
            if work > 2_000_000 { return Err(failure("native method requirement coverage exceeds its bound")); }
            if matches!(requirement, Requirement::NativeMethod(_)) && !original_obligations.contains(&(scope.index, index as u32)) { return Err(failure("native method requirement loses its original body ancestry")); }
        } }
        for entry in &methods.witnesses {
            let witness = &entry.value;
            let source = self.scoped_native_method_source(witness.source)?;
            let domain = candidate_domain(pools, &witness.candidate)?;
            if !source.expected.candidates.contains(&witness.candidate) || witness.arguments.len() != 1 || pools.to_type(witness.receiver)? != domain
                || pools.to_type(witness.arguments[0])? != domain || pools.to_type(witness.result)? != Type::Bool { return Err(failure("native method witness loses its selected family or domains")); }
        }
        Ok(())
    }
}
impl GenericEvidenceBuilder {
    pub(in crate::runtime::eval) fn add_scoped_native_method_source(&mut self, value: ScopedNativeMethodSource) -> Result<ScopedNativeMethodSourceId, IrVerifyError> {
        if self.store.native_callables.methods.sources.len() >= 2_000_000 || value.obligations.len() > 2_000_000 || value.payload.len() != 4 || value.arguments.len() != 1 || value.argument_payload.len() != 2 || value.expected.arguments.len() != 1 || value.expected.candidates.len() != 2 { return Err(failure("native method source exceeds its evidence bound")); }
        let index = self.store.native_callables.methods.sources.len() as u32;
        let serial = self.next_serial; self.next_serial = serial.checked_add(1).ok_or_else(|| failure("native method source serial overflow"))?;
        let value = Arc::new(value);
        self.store.native_callables.methods.originals.push(Arc::clone(&value)); self.store.native_callables.methods.sources.push(Entry { serial, value });
        Ok(ScopedNativeMethodSourceId { index, proof: OwnerProof { root: self.store.root, serial } })
    }
    pub(in crate::runtime::eval) fn add_scoped_native_method_witness(&mut self, value: ScopedNativeMethodWitness) -> Result<ScopedNativeMethodWitnessId, IrVerifyError> {
        self.store.scoped_native_method_source(value.source)?;
        if self.store.native_callables.methods.witnesses.len() >= 2_000_000 { return Err(failure("native method witnesses exceed their evidence bound")); }
        if let Some((index, entry)) = self.store.native_callables.methods.witnesses.iter().enumerate().find(|(_, entry)| entry.value == value) { return Ok(ScopedNativeMethodWitnessId { index: index as u32, proof: OwnerProof { root: self.store.root, serial: entry.serial } }); }
        let index = self.store.native_callables.methods.witnesses.len() as u32;
        let serial = self.next_serial; self.next_serial = serial.checked_add(1).ok_or_else(|| failure("native method witness serial overflow"))?;
        self.store.native_callables.methods.witnesses.push(Entry { serial, value });
        Ok(ScopedNativeMethodWitnessId { index, proof: OwnerProof { root: self.store.root, serial } })
    }
}
#[cfg(test)]
impl GenericEvidenceStore {
    pub(in crate::runtime::eval) fn test_scoped_native_method_source_mut(&mut self, id: ScopedNativeMethodSourceId) -> Result<&mut ScopedNativeMethodSource, IrVerifyError> { self.scoped_native_method_source(id)?; Ok(Arc::make_mut(&mut self.native_callables.methods.sources[id.index as usize].value)) }
    pub(in crate::runtime::eval) fn test_scoped_native_method_witness_mut(&mut self, id: ScopedNativeMethodWitnessId) -> Result<&mut ScopedNativeMethodWitness, IrVerifyError> { self.scoped_native_method_witness(id)?; Ok(&mut self.native_callables.methods.witnesses[id.index as usize].value) }
    pub(in crate::runtime::eval) fn test_remove_scoped_native_method_sources(&mut self) { self.native_callables.methods.sources.clear(); self.native_callables.methods.instructions.clear(); }
}
