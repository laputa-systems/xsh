use super::*;
use crate::sema::inference::{CallableDomain, InvocationArgumentKind, InvocationDefaultTiming, RequirementId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct TemplateInvocationArgument {
    pub kind: InvocationArgumentKind,
    pub ty: TypeRef,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct ScopedInvocationObligation {
    pub scope: SchemeScopeId,
    pub requirement: u32,
    pub immediate_original: RequirementId,
    pub ancestry: Box<[RequirementId]>,
    pub expected: Requirement,
}

/// The original body obligation is independent of every concrete callback target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct ScopedInvocationSource {
    pub origin: crate::sema::check::ExpressionIdentity,
    pub instruction: u32,
    pub scope: SchemeScopeId,
    pub requirement: u32,
    pub original_requirement: RequirementId,
    pub callee_instruction: u32,
    pub callee_origin: crate::sema::check::ExpressionIdentity,
    pub callable_parameter: u32,
    pub expected: Requirement,
    pub arguments: Box<[PreparedInvocationArgument]>,
    pub obligations: Box<[ScopedInvocationObligation]>,
}

/// A contextual signature binds operands; the actual value supplies its target and environment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct ScopedInvocationWitness {
    pub source: ScopedInvocationSourceId,
    pub signature: SignatureId,
    pub descriptor: GroundTypeId,
    pub kind: CallableKind,
    pub binding: PreparedOperationBinding,
    pub timing: InvocationDefaultTiming,
    pub effects: crate::sema::inference::EffectSet,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) enum UserInvocationAuthority {
    Ground(InvocationPlanId),
    Scoped { source: ScopedInvocationSourceId, instance: InstantiationId, witness: ScopedInvocationWitnessId },
}

impl GenericEvidenceStore {
    pub fn scoped_invocation_source(&self, id: ScopedInvocationSourceId) -> Result<&ScopedInvocationSource, IrVerifyError> {
        let source = owned(self.root, &self.scoped_invocation_sources, id.index, id.proof)?;
        let original = self.original_scoped_invocation_sources.get(id.index as usize).ok_or_else(|| failure("original scoped invocation receipt is missing"))?;
        if !Arc::ptr_eq(source, original) { return Err(failure("scoped invocation differs from its original receipt")); }
        Ok(source.as_ref())
    }
    pub fn scoped_invocation_witness(&self, id: ScopedInvocationWitnessId) -> Result<&ScopedInvocationWitness, IrVerifyError> {
        owned(self.root, &self.scoped_invocation_witnesses, id.index, id.proof)
    }
    pub fn scoped_invocation_sources(&self) -> impl Iterator<Item = (ScopedInvocationSourceId, &ScopedInvocationSource)> {
        self.scoped_invocation_sources.iter().enumerate().map(|(index, entry)| (ScopedInvocationSourceId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } }, entry.value.as_ref()))
    }
    pub fn scoped_invocation_source_at(&self, instruction: u32) -> Result<Option<ScopedInvocationSourceId>, IrVerifyError> {
        self.scoped_invocation_instructions.binary_search_by_key(&instruction, |entry| entry.0).ok().map(|index| {
            let id = self.scoped_invocation_instructions[index].1;
            self.scoped_invocation_source(id)?;
            Ok(id)
        }).transpose()
    }
    pub fn scoped_invocation_authority(&self, instruction: u32, instance: InstantiationId) -> Result<Option<UserInvocationAuthority>, IrVerifyError> {
        let Some(source) = self.scoped_invocation_source_at(instruction)? else { return Ok(None); };
        let original = self.scoped_invocation_source(source)?;
        let instance_value = self.instance(instance)?;
        if instance_value.scope != original.scope { return Err(failure("scoped invocation uses another declaration's instance")); }
        let Some(RequirementWitness::Invocation(witness)) = instance_value.requirements.get(original.requirement as usize) else { return Err(failure("scoped invocation witness is missing")); };
        if self.scoped_invocation_witness(*witness)?.source != source { return Err(failure("scoped invocation uses another original obligation")); }
        Ok(Some(UserInvocationAuthority::Scoped { source, instance, witness: *witness }))
    }
    pub(super) fn verify_scoped_invocation_witness(&self, pools: &SemanticPools, scope: &SchemeScope, requirement: &Requirement, requirement_index: usize, substitutions: &[GroundTypeId], parameters: &[GroundTypeId], id: ScopedInvocationWitnessId) -> Result<(), IrVerifyError> {
        let witness = self.scoped_invocation_witness(id)?;
        let source = self.scoped_invocation_source(witness.source)?;
        let obligation = source.obligations.iter().find(|obligation| self.scope(obligation.scope).is_ok_and(|original| original.owner == scope.owner) && obligation.requirement as usize == requirement_index && &obligation.expected == requirement)
            .ok_or_else(|| failure("scoped invocation witness changes its original requirement"))?;
        if scope.requirements.get(obligation.requirement as usize) != Some(requirement) { return Err(failure("scoped invocation obligation index changed")); }
        let Requirement::Invocation { callable, arguments, result, domain } = requirement else { return Err(failure("scoped invocation witness has another requirement kind")); };
        if !matches!(domain, CallableDomain::Pure | CallableDomain::Exact(crate::sema::inference::CallableKind::Pure))
            || witness.kind != CallableKind::Pure || witness.effects != crate::sema::inference::EffectSet::EMPTY
            || pools.signature_closed_effects(witness.signature)? != witness.effects
            || witness.timing != InvocationDefaultTiming::AtCall || witness.binding.dynamic.is_some() || witness.binding.rest_slot.is_some() {
            return Err(failure("scoped callback kind, effects, or dynamic binding is not prepared"));
        }
        let expected = self.instantiated_normalized_type(pools, scope, *callable, substitutions)?;
        let actual = Self::normalized_ground_type(pools, witness.descriptor)?;
        if expected != actual || pools.callable_descriptor(witness.descriptor)? != Some((CallableKind::Pure, witness.signature)) {
            return Err(failure("scoped invocation changes its actual callback descriptor"));
        }
        if obligation.scope == source.scope && parameters.get(source.callable_parameter as usize) != Some(&witness.descriptor) { return Err(failure("scoped invocation changes its body callback parameter")); }
        if self.instantiated_normalized_type(pools, scope, *result, substitutions)? != Self::normalized_ground_type(pools, pools.signature_return_type(witness.signature)?)?
            || arguments.len() != source.arguments.len() || arguments.len() != witness.binding.supplied_slots.len()
            || witness.binding.operands.as_ref() != source.arguments.iter().map(|argument| argument.instruction).collect::<Vec<_>>().as_slice() {
            return Err(failure("scoped invocation changes its result or original operands"));
        }
        let count = pools.signature_param_count(witness.signature)?;
        let mut occupied = std::collections::BTreeSet::new();
        let mut next = 0;
        for ((argument, original), &slot) in arguments.iter().zip(&source.arguments).zip(&witness.binding.supplied_slots) {
            if slot as usize >= count || !occupied.insert(slot) || pools.signature_parameter_rest(witness.signature, slot as usize)? {
                return Err(failure("scoped invocation has an invalid supplied slot"));
            }
            let (label, formal, _) = pools.signature_param(witness.signature, slot as usize)?;
            match argument.kind {
                InvocationArgumentKind::Named(name) if name == label && original.original.name == Some(name) => {},
                InvocationArgumentKind::Positional if original.original.name.is_none() => {
                    while next < count && (occupied.contains(&(next as u32)) && next != slot as usize || pools.signature_parameter_mode(witness.signature, next)? == ParameterMode::NamedOnly) { next += 1; }
                    if next != slot as usize { return Err(failure("scoped invocation positional order changed")); }
                    next += 1;
                },
                _ => return Err(failure("scoped invocation argument mode is not prepared")),
            }
            if self.instantiated_normalized_type(pools, scope, argument.ty, substitutions)? != Self::normalized_ground_type(pools, formal)? {
                return Err(failure("scoped invocation operand descriptor differs from its checked signature"));
            }
        }
        let defaults = (0..count as u32).filter(|slot| !occupied.contains(slot)).collect::<Vec<_>>();
        if witness.binding.default_slots.as_ref() != defaults.as_slice() { return Err(failure("scoped invocation default order changed")); }
        for &slot in &witness.binding.default_slots {
            if slot as usize >= count || !occupied.insert(slot) || !pools.signature_parameter_defaulted(witness.signature, slot as usize)? { return Err(failure("scoped invocation default slot changed")); }
        }
        if occupied.len() != count { return Err(failure("scoped invocation omits a required parameter")); }
        Ok(())
    }
    pub(super) fn verify_scoped_invocation_sources(&self, owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        if self.scoped_invocation_sources.len() != self.original_scoped_invocation_sources.len() { return Err(failure("scoped invocation original source ledger is incomplete")); }
        let mut expected = Vec::new();
        let mut seen_obligations = std::collections::BTreeSet::new();
        for (id, _) in self.scoped_invocation_sources() {
            let source = self.scoped_invocation_source(id)?;
            let scope = self.scope(source.scope)?;
            let owner = InstructionOwner::Function(scope.owner);
            if scope.requirements.get(source.requirement as usize) != Some(&source.expected)
                || !matches!(&source.expected, Requirement::Invocation { callable, .. } if scope.parameters.get(source.callable_parameter as usize) == Some(callable)) {
                return Err(failure("scoped invocation source changes its declaration-owned obligation"));
            }
            if !source.obligations.iter().any(|obligation| obligation.scope == source.scope && obligation.requirement == source.requirement && obligation.immediate_original == source.original_requirement && obligation.expected == source.expected) { return Err(failure("scoped invocation body obligation is missing")); }
            for obligation in &source.obligations {
                let member = self.scope(obligation.scope)?;
                if member.requirements.get(obligation.requirement as usize) != Some(&obligation.expected)
                    || obligation.ancestry.first() != Some(&obligation.immediate_original) || obligation.ancestry.last() != Some(&source.original_requirement)
                    || obligation.ancestry.len() > 256 || !seen_obligations.insert((member.owner, obligation.requirement)) {
                    return Err(failure("scoped invocation member ancestry is missing or ambiguous"));
                }
            }
            for (instruction, origin) in [(source.instruction, source.origin), (source.callee_instruction, source.callee_origin)] {
                if owners.get(instruction as usize) != Some(&Some(owner)) || self.registered_instruction_origin(instruction, false) != Some((OperationSourceOrigin::Expression(origin), owner)) {
                    return Err(failure("scoped invocation original source or owner changed"));
                }
            }
            for (ordinal, argument) in source.arguments.iter().enumerate() {
                if owners.get(argument.instruction as usize) != Some(&Some(owner)) || !self.argument_has_original_source(argument.instruction, source.origin, ordinal, &argument.original, owner, argument.ty) {
                    return Err(failure("scoped invocation operand lost its original source"));
                }
            }
            expected.push((source.instruction, id));
        }
        expected.sort_unstable_by_key(|entry| entry.0);
        if expected.windows(2).any(|pair| pair[0].0 == pair[1].0) || expected != self.scoped_invocation_instructions { return Err(failure("scoped invocation instruction index is ambiguous or stale")); }
        Ok(())
    }
}

impl GenericEvidenceBuilder {
    pub fn add_scoped_invocation_source(&mut self, value: ScopedInvocationSource) -> Result<ScopedInvocationSourceId, IrVerifyError> {
        if value.arguments.len() > 65536 || value.obligations.len() > 2_000_000 || self.store.scoped_invocation_sources.len() >= 2_000_000 { return Err(failure("scoped invocation source exceeds its work limit")); }
        let index = u32::try_from(self.store.scoped_invocation_sources.len()).map_err(|_| failure("scoped invocation source id overflow"))?;
        let serial = self.next_serial;
        self.next_serial = serial.checked_add(1).ok_or_else(|| failure("generic evidence serial overflow"))?;
        let value = Arc::new(value);
        self.store.original_scoped_invocation_sources.push(Arc::clone(&value));
        self.store.scoped_invocation_sources.push(Entry { serial, value });
        Ok(ScopedInvocationSourceId { index, proof: OwnerProof { root: self.store.root, serial } })
    }
    pub fn add_scoped_invocation_witness(&mut self, value: ScopedInvocationWitness) -> Result<ScopedInvocationWitnessId, IrVerifyError> {
        if self.store.scoped_invocation_witnesses.len() >= 2_000_000 || value.binding.supplied_slots.len() > 65536 || value.binding.default_slots.len() > 65536 { return Err(failure("scoped invocation witness exceeds its work limit")); }
        if let Some((index, entry)) = self.store.scoped_invocation_witnesses.iter().enumerate().find(|(_, entry)| entry.value == value) {
            return Ok(ScopedInvocationWitnessId { index: index as u32, proof: OwnerProof { root: self.store.root, serial: entry.serial } });
        }
        let index = u32::try_from(self.store.scoped_invocation_witnesses.len()).map_err(|_| failure("scoped invocation witness id overflow"))?;
        let serial = self.next_serial;
        self.next_serial = serial.checked_add(1).ok_or_else(|| failure("generic evidence serial overflow"))?;
        self.store.scoped_invocation_witnesses.push(Entry { serial, value });
        Ok(ScopedInvocationWitnessId { index, proof: OwnerProof { root: self.store.root, serial } })
    }
}

impl GenericEvidenceBuilder {
    pub(in crate::runtime::eval) fn scoped_invocation_sources(&self) -> impl Iterator<Item = (ScopedInvocationSourceId, &ScopedInvocationSource)> { self.store.scoped_invocation_sources() }
}

#[cfg(test)]
impl GenericEvidenceStore {
    pub(in crate::runtime::eval) fn test_scoped_invocation_source_mut(&mut self, id: ScopedInvocationSourceId) -> Result<&mut ScopedInvocationSource, IrVerifyError> {
        self.scoped_invocation_source(id)?;
        Ok(Arc::make_mut(&mut self.scoped_invocation_sources[id.index as usize].value))
    }
    pub(in crate::runtime::eval) fn test_scoped_invocation_witness_mut(&mut self, id: ScopedInvocationWitnessId) -> Result<&mut ScopedInvocationWitness, IrVerifyError> {
        self.scoped_invocation_witness(id)?;
        Ok(&mut self.scoped_invocation_witnesses[id.index as usize].value)
    }
    pub(in crate::runtime::eval) fn test_scoped_invocation_instance_mut(&mut self, id: InstantiationId) -> Result<&mut Instantiation, IrVerifyError> {
        self.instance(id)?;
        Ok(&mut self.instances[id.index as usize].value)
    }
    pub(in crate::runtime::eval) fn test_remove_scoped_invocation_sources(&mut self) { self.scoped_invocation_sources.clear(); self.scoped_invocation_instructions.clear(); }
}
#[cfg(test)]
impl GenericEvidenceBuilder {
    pub(in crate::runtime::eval) fn store_retained_bytes(&self) -> usize { self.store.retained_bytes() }
    pub(in crate::runtime::eval) fn test_scoped_invocation_source(&self, id: ScopedInvocationSourceId) -> Result<&ScopedInvocationSource, IrVerifyError> { self.store.scoped_invocation_source(id) }
    pub(in crate::runtime::eval) fn test_scoped_invocation_witness(&self, id: ScopedInvocationWitnessId) -> Result<&ScopedInvocationWitness, IrVerifyError> { self.store.scoped_invocation_witness(id) }
}

impl ScopedInvocationSource {
    pub(super) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        let requirement_bytes = |requirement: &Requirement| match requirement { Requirement::Invocation { arguments, .. } => arguments.len() * size_of::<TemplateInvocationArgument>(), _ => 0 };
        size_of::<Self>() + 2 * size_of::<usize>() + self.arguments.len() * size_of::<PreparedInvocationArgument>() + requirement_bytes(&self.expected)
            + self.obligations.len() * size_of::<ScopedInvocationObligation>()
            + self.obligations.iter().map(|obligation| obligation.ancestry.len() * size_of::<RequirementId>() + requirement_bytes(&obligation.expected)).sum::<usize>()
    }
}
