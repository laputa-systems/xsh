use super::*;
use crate::sema::check::{BindingIdentity, ExpressionIdentity, StatementIdentity, WithBindingIdentity, GuardErrorBindingIdentity};
use crate::sema::inference::ScopedRoot;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) struct ValueBindingSourceId { index: u32, proof: OwnerProof }

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) struct ValueBindingId { index: u32, proof: OwnerProof }

#[derive(Clone, Debug, Eq, PartialEq)]
/// An initializer wrapper executes before the material source value. Its complete
/// payload retains the original child and validation selection independently.
pub(in crate::runtime::eval) struct ValueInitializerWrapper {
    pub instruction: u32,
    pub payload: Box<[u32]>,
    pub kind: ValueInitializerWrapperKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) enum ValueInitializerWrapperKind {
    CheckedValue,
    CheckedBindingTry,
    CheckedBindingRequire,
    FsRootReceiverTry,
    CompilerArgument { initializer: u32, pattern: u32, body: u32, slot: u32 },
    SavedArgument { call: ExpressionIdentity, initializer: u32, pattern: u32, body: u32 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(in crate::runtime::eval) enum ValueBindingIdentity { Named(BindingIdentity), With(WithBindingIdentity), GuardError(GuardErrorBindingIdentity) }

impl ValueBindingIdentity {
    pub(in crate::runtime::eval) fn source(self) -> crate::source::SourceId { match self { Self::Named(binding) => binding.source, Self::With(binding) => binding.statement.source, Self::GuardError(binding) => binding.statement.source } }
    pub(in crate::runtime::eval) fn namespace(self) -> Option<Name> { match self { Self::Named(binding) => binding.namespace, Self::With(binding) => binding.statement.namespace, Self::GuardError(binding) => binding.statement.namespace } }
    pub(in crate::runtime::eval) fn named(self) -> Option<BindingIdentity> { match self { Self::Named(binding) => Some(binding), Self::With(_) | Self::GuardError(_) => None } }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) enum ValueBindingAllocation {
    Value, Integer, Boolean,
    Guard { error_slot: Option<u32>, failure_body: u32, location: u32 },
    GuardError { success_slot: u32, failure_body: u32, location: u32 },
    With { ordinal: u32, bindings: u32, body: u32, error_slot: Option<u32>, failure_body: u32, captures: u32, location: u32 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct ValueBindingContract {
    pub instruction: u32,
    pub allocation: ValueBindingAllocation,
    pub owner: InstructionOwner,
    pub slot: u32,
    pub initializer: u32,
    pub initializer_source_instruction: u32,
    pub initializer_wrappers: Box<[ValueInitializerWrapper]>,
    pub with_bindings: Box<[u32]>,
    pub binding_type: GroundTypeId,
    pub initializer_type: GroundTypeId,
    pub scope: Option<SchemeScopeId>,
}

/// The checked definition and initializer own the value independently of its
/// encoded allocation and later reads, including definitions with equal types.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct ValueBindingSource {
    pub binding: ValueBindingIdentity,
    pub statement: StatementIdentity,
    pub initializer_source: ExpressionIdentity,
    pub source_type: ScopedRoot,
    pub initializer_type: ScopedRoot,
    pub expected: ValueBindingContract,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedValueBinding {
    pub source: ValueBindingSourceId,
    pub contract: ValueBindingContract,
}

/// The material record and a field-presence read have distinct checked types.
/// The original predicate and its successful branch own that difference.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct ValueFieldPresence {
    pub original: crate::sema::check::SolvedFieldPresenceRead,
    pub material_type: GroundTypeId,
    pub narrowed_type: GroundTypeId,
    pub predicate: u32,
    pub subject: u32,
    pub key: u32,
    pub control: u32,
    pub branch_body: u32,
    pub predicate_payload: Box<[u32]>,
    pub key_payload: Box<[u32]>,
    pub control_payload: Box<[u32]>,
    pub branch_payload: Box<[u32]>,
    pub branch_body_payload: Box<[u32]>,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct ValueBindingUse {
    pub origin: OperationSourceOrigin,
    pub application: ValueBindingId,
    pub instruction: u32,
    pub owner: InstructionOwner,
    pub presence: Option<ValueFieldPresence>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct ValueBindingEvidence {
    sources: Vec<Entry<Arc<ValueBindingSource>>>,
    originals: Vec<Arc<ValueBindingSource>>,
    applications: Vec<Entry<Arc<PreparedValueBinding>>>,
    original_applications: Vec<Arc<PreparedValueBinding>>,
    uses: Vec<Entry<Arc<ValueBindingUse>>>,
    original_uses: Vec<Arc<ValueBindingUse>>,
    use_instructions: Vec<(u32, usize)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ValueBindingCheckpoint { sources: usize, applications: usize, uses: usize }

impl ValueBindingEvidence {
    pub(super) fn checkpoint(&self) -> ValueBindingCheckpoint { ValueBindingCheckpoint { sources: self.sources.len(), applications: self.applications.len(), uses: self.uses.len() } }
    pub(super) fn validate_checkpoint(&self, checkpoint: ValueBindingCheckpoint, serial_limit: u64) -> Result<(), IrVerifyError> {
        if checkpoint.sources > self.sources.len() || checkpoint.applications > self.applications.len() || checkpoint.uses > self.uses.len() { return Err(failure("value binding checkpoint references retired entries")); }
        for serial in [self.sources.get(checkpoint.sources.wrapping_sub(1)).map(|entry| entry.serial), self.applications.get(checkpoint.applications.wrapping_sub(1)).map(|entry| entry.serial), self.uses.get(checkpoint.uses.wrapping_sub(1)).map(|entry| entry.serial)].into_iter().flatten() {
            if serial >= serial_limit { return Err(failure("value binding checkpoint references replacement entries")); }
        }
        Ok(())
    }
    pub(super) fn rewind_validated(&mut self, checkpoint: ValueBindingCheckpoint) {
        self.sources.truncate(checkpoint.sources); self.originals.truncate(checkpoint.sources);
        self.applications.truncate(checkpoint.applications); self.original_applications.truncate(checkpoint.applications);
        self.uses.truncate(checkpoint.uses); self.original_uses.truncate(checkpoint.uses); self.use_instructions.clear();
    }
    pub(super) fn finish(&mut self) {
        self.use_instructions = self.uses.iter().enumerate().map(|(index, entry)| (entry.value.instruction, index)).collect();
        self.use_instructions.sort_unstable_by_key(|entry| entry.0);
    }
    pub(super) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        self.sources.capacity() * size_of::<Entry<Arc<ValueBindingSource>>>() + self.originals.capacity() * size_of::<Arc<ValueBindingSource>>()
            + self.sources.len() * (size_of::<ValueBindingSource>() + 2 * size_of::<usize>())
            + self.applications.capacity() * size_of::<Entry<Arc<PreparedValueBinding>>>() + self.original_applications.capacity() * size_of::<Arc<PreparedValueBinding>>()
            + self.applications.len() * (size_of::<PreparedValueBinding>() + 2 * size_of::<usize>())
            + self.uses.capacity() * size_of::<Entry<Arc<ValueBindingUse>>>() + self.original_uses.capacity() * size_of::<Arc<ValueBindingUse>>()
            + self.uses.len() * (size_of::<ValueBindingUse>() + 2 * size_of::<usize>()) + self.use_instructions.capacity() * size_of::<(u32, usize)>()
            + self.sources.iter().map(|entry| wrapper_bytes(&entry.value.expected)).sum::<usize>()
            + self.applications.iter().map(|entry| wrapper_bytes(&entry.value.contract)).sum::<usize>()
            + self.uses.iter().map(|entry| entry.value.presence.as_ref().map_or(0, |presence| {
                (presence.predicate_payload.len() + presence.key_payload.len() + presence.control_payload.len()
                    + presence.branch_payload.len() + presence.branch_body_payload.len()) * size_of::<u32>()
                    + presence.original.writes.capacity() * size_of::<crate::sema::check::SolvedRefinementWrite>()
            })).sum::<usize>()
    }
    pub(super) fn shrink_to_fit(&mut self) { self.sources.shrink_to_fit(); self.originals.shrink_to_fit(); self.applications.shrink_to_fit(); self.original_applications.shrink_to_fit(); self.uses.shrink_to_fit(); self.original_uses.shrink_to_fit(); self.use_instructions.shrink_to_fit(); }
}

fn wrapper_bytes(contract: &ValueBindingContract) -> usize {
    contract.initializer_wrappers.len() * std::mem::size_of::<ValueInitializerWrapper>()
        + contract.initializer_wrappers.iter().map(|wrapper| wrapper.payload.len() * std::mem::size_of::<u32>()).sum::<usize>()
        + contract.with_bindings.len() * std::mem::size_of::<u32>()
}

impl GenericEvidenceStore {
    pub fn has_value_bindings(&self) -> bool { !self.values.sources.is_empty() || !self.values.applications.is_empty() || !self.values.uses.is_empty() }
    pub fn value_binding_source(&self, id: ValueBindingSourceId) -> Result<&ValueBindingSource, IrVerifyError> {
        let value = owned(self.root, &self.values.sources, id.index, id.proof)?;
        if !self.values.originals.get(id.index as usize).is_some_and(|original| Arc::ptr_eq(value, original)) { return Err(failure("value binding source differs from its original receipt")); }
        Ok(value.as_ref())
    }
    pub fn value_binding(&self, id: ValueBindingId) -> Result<&PreparedValueBinding, IrVerifyError> {
        let value = owned(self.root, &self.values.applications, id.index, id.proof)?;
        if !self.values.original_applications.get(id.index as usize).is_some_and(|original| Arc::ptr_eq(value, original)) { return Err(failure("value binding application differs from its original receipt")); }
        Ok(value.as_ref())
    }
    pub fn value_binding_sources(&self) -> impl Iterator<Item = (ValueBindingSourceId, &ValueBindingSource)> {
        self.values.sources.iter().enumerate().map(|(index, entry)| (ValueBindingSourceId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } }, entry.value.as_ref()))
    }
    pub fn value_bindings(&self) -> impl Iterator<Item = (ValueBindingId, &PreparedValueBinding)> {
        self.values.applications.iter().enumerate().map(|(index, entry)| (ValueBindingId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } }, entry.value.as_ref()))
    }
    pub fn value_binding_uses(&self) -> impl Iterator<Item = &ValueBindingUse> { self.values.uses.iter().map(|entry| entry.value.as_ref()) }
    pub fn value_binding_use(&self, instruction: u32) -> Result<Option<&ValueBindingUse>, IrVerifyError> {
        let Some(index) = self.values.use_instructions.binary_search_by_key(&instruction, |entry| entry.0).ok().map(|index| self.values.use_instructions[index].1) else { return Ok(None); };
        let value = &self.values.uses.get(index).ok_or_else(|| failure("value binding use index is stale"))?.value;
        if !self.values.original_uses.get(index).is_some_and(|original| Arc::ptr_eq(value, original)) { return Err(failure("value binding use differs from its original receipt")); }
        Ok(Some(value.as_ref()))
    }
    pub(super) fn verify_value_binding_evidence(&self, pools: &SemanticPools, owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        if self.values.sources.len() != self.values.originals.len() || self.values.applications.len() != self.values.original_applications.len() || self.values.uses.len() != self.values.original_uses.len() { return Err(failure("value binding original receipt ledger is incomplete")); }
        let mut sources = std::collections::BTreeSet::new();
        let mut definitions = std::collections::BTreeSet::new();
        for (id, _) in self.value_bindings() {
            let application = self.value_binding(id)?;
            let source = self.value_binding_source(application.source)?;
            let contract = &application.contract;
            if !sources.insert(application.source.index) || !definitions.insert(source.binding) || source.expected != *contract
                || source.binding.source() != source.statement.source || source.binding.namespace() != source.statement.namespace
                || source.initializer_source.source != source.statement.source || source.initializer_source.namespace != source.statement.namespace
                || owners.get(contract.instruction as usize) != Some(&Some(contract.owner)) || owners.get(contract.initializer as usize) != Some(&Some(contract.owner))
                || owners.get(contract.initializer_source_instruction as usize) != Some(&Some(contract.owner))
                || contract.initializer_wrappers.len() > 256 || contract.initializer_wrappers.iter().any(|wrapper| owners.get(wrapper.instruction as usize) != Some(&Some(contract.owner))) {
                return Err(failure("value binding changes its original definition, initializer, or owner"));
            }
            match (source.binding, contract.allocation) {
                (ValueBindingIdentity::With(binding), ValueBindingAllocation::With { ordinal, .. }) if binding.statement == source.statement
                    && binding.ordinal == ordinal && !contract.with_bindings.is_empty() => {}
                (ValueBindingIdentity::Named(_), ValueBindingAllocation::Value | ValueBindingAllocation::Integer | ValueBindingAllocation::Boolean | ValueBindingAllocation::Guard { .. })
                    if contract.with_bindings.is_empty() => {}
                (ValueBindingIdentity::GuardError(binding), ValueBindingAllocation::GuardError { .. })
                    if binding.statement == source.statement && contract.with_bindings.is_empty() => {}
                _ => return Err(failure("value binding changes its original binding domain or ordinal")),
            }
            if self.registered_instruction_origin(contract.instruction, false) != Some((OperationSourceOrigin::Statement(source.statement), contract.owner)) { return Err(failure("value binding changes its original allocation source")); }
            if self.registered_instruction_origin(contract.initializer_source_instruction, false) != Some((OperationSourceOrigin::Expression(source.initializer_source), contract.owner)) {
                return Err(IrVerifyError::new(format!("value initializer {} lacks its original source {:?}: {:?}", contract.initializer_source_instruction, source.initializer_source, self.registered_instruction_origin(contract.initializer_source_instruction, false))));
            }
            Self::verify_type(pools, contract.binding_type)?; Self::verify_type(pools, contract.initializer_type)?;
            if let Some(scope) = contract.scope {
                if contract.owner != InstructionOwner::Function(self.scope(scope)?.owner) { return Err(failure("value binding changes its original generic scope")); }
            }
        }
        if sources.len() != self.values.sources.len() { return Err(failure("value binding source lacks its prepared allocation")); }
        let mut expected = Vec::new();
        for (index, entry) in self.values.uses.iter().enumerate() {
            let use_ = self.value_binding_use(entry.value.instruction)?.ok_or_else(|| failure("value binding use is missing from its index"))?;
            let application = self.value_binding(use_.application)?;
            let source = self.value_binding_source(application.source)?;
            let (read_source, read_namespace) = match use_.origin {
                OperationSourceOrigin::Expression(origin) => (origin.source, origin.namespace),
                OperationSourceOrigin::Statement(origin) => (origin.source, origin.namespace),
                _ => return Err(failure("value binding read has no authored value or statement identity")),
            };
            if use_.owner != application.contract.owner || owners.get(use_.instruction as usize) != Some(&Some(use_.owner))
                || read_source != source.binding.source() || read_namespace != source.binding.namespace()
                || self.registered_instruction_origin(use_.instruction, false) != Some((use_.origin, use_.owner)) {
                return Err(failure("value binding read changes its original source or owner"));
            }
            if let Some(presence) = &use_.presence {
                let original = &presence.original;
                if use_.origin != OperationSourceOrigin::Expression(original.read)
                    || source.binding.named() != Some(original.binding)
                    || original.material.ty != source.source_type.ty || original.material.scope != source.source_type.scope
                    || presence.material_type != application.contract.binding_type || !original.writes.is_empty()
                    || [presence.predicate, presence.subject, presence.key, presence.control].iter().any(|&instruction| owners.get(instruction as usize) != Some(&Some(use_.owner)))
                    || self.registered_instruction_origin(presence.predicate, false) != Some((OperationSourceOrigin::Expression(original.predicate), use_.owner))
                    || self.registered_instruction_origin(presence.subject, false) != Some((OperationSourceOrigin::Expression(original.subject), use_.owner))
                    || self.registered_instruction_origin(presence.key, false) != Some((OperationSourceOrigin::Expression(original.key), use_.owner))
                    || self.registered_instruction_origin(presence.control, false) != Some((OperationSourceOrigin::Statement(original.control), use_.owner)) {
                    return Err(failure("field-presence read changes its original material, predicate or branch owner"));
                }
                Self::verify_type(pools, presence.material_type)?;
                Self::verify_type(pools, presence.narrowed_type)?;
            }
            expected.push((use_.instruction, index));
        }
        expected.sort_unstable_by_key(|entry| entry.0);
        if expected.windows(2).any(|pair| pair[0].0 == pair[1].0) || expected != self.values.use_instructions { return Err(failure("value binding read index is ambiguous or incomplete")); }
        Ok(())
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_value_binding_source_mut(&mut self, id: ValueBindingSourceId) -> Result<&mut ValueBindingSource, IrVerifyError> { self.value_binding_source(id)?; Ok(Arc::make_mut(&mut self.values.sources[id.index as usize].value)) }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_value_binding_mut(&mut self, id: ValueBindingId) -> Result<&mut PreparedValueBinding, IrVerifyError> { self.value_binding(id)?; Ok(Arc::make_mut(&mut self.values.applications[id.index as usize].value)) }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_value_binding_use_mut(&mut self, instruction: u32) -> Result<&mut ValueBindingUse, IrVerifyError> {
        self.value_binding_use(instruction)?.ok_or_else(|| failure("value binding use is missing"))?;
        let index = self.values.use_instructions.binary_search_by_key(&instruction, |entry| entry.0).unwrap();
        Ok(Arc::make_mut(&mut self.values.uses[self.values.use_instructions[index].1].value))
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_remove_value_bindings(&mut self) { self.values.applications.clear(); }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_remove_value_binding_uses(&mut self) { self.values.uses.clear(); }
}

impl GenericEvidenceBuilder {
    pub(in crate::runtime::eval) fn value_binding(&self, id: ValueBindingId) -> Result<&PreparedValueBinding, IrVerifyError> { self.store.value_binding(id) }

    pub fn add_value_binding_source(&mut self, value: ValueBindingSource) -> Result<ValueBindingSourceId, IrVerifyError> {
        if self.store.values.sources.len() >= 2_000_000 { return Err(failure("value bindings exceed their work limit")); }
        let index = u32::try_from(self.store.values.sources.len()).map_err(|_| failure("value binding source id overflow"))?;
        let serial = self.next_serial; self.next_serial = serial.checked_add(1).ok_or_else(|| failure("generic evidence serial overflow"))?;
        let value = Arc::new(value); self.store.values.originals.push(Arc::clone(&value)); self.store.values.sources.push(Entry { serial, value });
        Ok(ValueBindingSourceId { index, proof: OwnerProof { root: self.store.root, serial } })
    }
    pub fn add_value_binding(&mut self, value: PreparedValueBinding) -> Result<ValueBindingId, IrVerifyError> {
        self.store.value_binding_source(value.source)?;
        if self.store.values.applications.len() >= 2_000_000 { return Err(failure("value binding applications exceed their work limit")); }
        let index = u32::try_from(self.store.values.applications.len()).map_err(|_| failure("value binding application id overflow"))?;
        let serial = self.next_serial; self.next_serial = serial.checked_add(1).ok_or_else(|| failure("generic evidence serial overflow"))?;
        let value = Arc::new(value); self.store.values.original_applications.push(Arc::clone(&value)); self.store.values.applications.push(Entry { serial, value });
        Ok(ValueBindingId { index, proof: OwnerProof { root: self.store.root, serial } })
    }
    pub fn add_value_binding_use(&mut self, value: ValueBindingUse) -> Result<(), IrVerifyError> {
        self.store.value_binding(value.application)?;
        if self.store.values.uses.len() >= 2_000_000 { return Err(failure("value binding reads exceed their work limit")); }
        let serial = self.next_serial; self.next_serial = serial.checked_add(1).ok_or_else(|| failure("generic evidence serial overflow"))?;
        let value = Arc::new(value); self.store.values.original_uses.push(Arc::clone(&value)); self.store.values.uses.push(Entry { serial, value });
        Ok(())
    }
}
