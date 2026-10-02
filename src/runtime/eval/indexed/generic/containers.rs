use super::*;
use crate::sema::check::ExpressionIdentity;
use crate::source::Span;
use std::mem::size_of;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) struct GroundContainerSourceId { index: u32, proof: OwnerProof }

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) struct NamedMapKeySourceId { index: u32, proof: OwnerProof }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) enum ContainerKind { List, Map }

/// A splice consumes a checked finite list, while a scalar entry contributes
/// one item; both retain their authored position in the evaluation sequence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) enum ContainerOperandRole { ListItem(u32), ListSplice(u32), MapKey(u32), MapValue(u32), MapSpread(u32) }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) enum ContainerOperandOrigin { Expression(ExpressionIdentity), NamedMapKey(NamedMapKeySourceId) }

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct GroundContainerOperand {
    pub origin: ContainerOperandOrigin,
    pub role: ContainerOperandRole,
    pub instruction: u32,
    pub source_instruction: u32,
    pub source_wrappers: Box<[ValueInitializerWrapper]>,
    pub source_type: GroundTypeId,
    pub ty: GroundTypeId,
}

/// The authored container is validated as a whole before its result escapes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct GroundContainerCreationCheck {
    pub instruction: u32,
    pub payload: Box<[u32]>,
}

pub(in crate::runtime::eval) fn checked_container_boundary_accepts(expected: &crate::sema::types::Type, actual: &crate::sema::types::Type) -> bool {
    use crate::sema::types::Type;
    if expected == actual { return true; }
    match (expected, actual) {
        (Type::UInt, Type::Int) => true,
        (Type::List(left), Type::List(right)) | (Type::Optional(left), Type::Optional(right)) => checked_container_boundary_accepts(left, right),
        (Type::Optional(inner), actual) => actual == &Type::Null || checked_container_boundary_accepts(inner, actual),
        (Type::Map(lk, lv), Type::Map(rk, rv)) | (Type::Result(lk, lv), Type::Result(rk, rv)) => checked_container_boundary_accepts(lk, rk) && checked_container_boundary_accepts(lv, rv),
        (Type::Record(left), Type::Record(right)) => left.len() == right.len() && left.iter().all(|(name, ty)| right.get(name).is_some_and(|actual| checked_container_boundary_accepts(ty, actual))),
        _ => false,
    }
}

/// A literal retains its checked result and original operands independently
/// of the container type expected by a later consumer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct GroundContainerSource {
    pub origin: ExpressionIdentity,
    pub instruction: u32,
    pub owner: InstructionOwner,
    pub scope: Option<SchemeScopeId>,
    pub kind: ContainerKind,
    pub result: GroundTypeId,
    pub creation_check: Option<GroundContainerCreationCheck>,
    pub operands: Box<[GroundContainerOperand]>,
    pub instruction_payload: Box<[u32]>,
    pub block_flags: u8,
    pub block_payload: Box<[u32]>,
}

/// An authored named key is a source entry, not an authored expression.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct OriginalNamedMapKey {
    pub container: ExpressionIdentity,
    pub entry_index: u32,
    pub name: Name,
    pub span: Span,
    pub instruction: u32,
    pub owner: InstructionOwner,
    pub checked: GroundTypeId,
}

#[derive(Clone, Debug, Default)]
pub(super) struct ContainerEvidence {
    sources: Vec<Entry<Arc<GroundContainerSource>>>,
    originals: Vec<Arc<GroundContainerSource>>,
    keys: Vec<Entry<Arc<OriginalNamedMapKey>>>,
    original_keys: Vec<Arc<OriginalNamedMapKey>>,
    instructions: Vec<(u32, usize)>,
    key_instructions: Vec<(u32, usize)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ContainerCheckpoint { sources: usize, keys: usize }

impl ContainerEvidence {
    pub(super) fn checkpoint(&self) -> ContainerCheckpoint { ContainerCheckpoint { sources: self.sources.len(), keys: self.keys.len() } }
    pub(super) fn validate_checkpoint(&self, checkpoint: ContainerCheckpoint, serial_limit: u64) -> Result<(), IrVerifyError> {
        if checkpoint.sources > self.sources.len() || checkpoint.keys > self.keys.len() { return Err(failure("container checkpoint references retired entries")); }
        for serial in [self.sources.get(checkpoint.sources.wrapping_sub(1)).map(|entry| entry.serial), self.keys.get(checkpoint.keys.wrapping_sub(1)).map(|entry| entry.serial)].into_iter().flatten() {
            if serial >= serial_limit { return Err(failure("container checkpoint references replacement entries")); }
        }
        Ok(())
    }
    pub(super) fn rewind_validated(&mut self, checkpoint: ContainerCheckpoint) {
        self.sources.truncate(checkpoint.sources); self.originals.truncate(checkpoint.sources);
        self.keys.truncate(checkpoint.keys); self.original_keys.truncate(checkpoint.keys);
        self.instructions.clear(); self.key_instructions.clear();
    }
    pub(super) fn finish(&mut self) {
        self.instructions = self.sources.iter().enumerate().map(|(index, entry)| (entry.value.instruction, index)).collect();
        self.key_instructions = self.keys.iter().enumerate().map(|(index, entry)| (entry.value.instruction, index)).collect();
        self.instructions.sort_unstable_by_key(|entry| entry.0); self.key_instructions.sort_unstable_by_key(|entry| entry.0);
    }
    pub(super) fn retained_bytes(&self) -> usize {
        self.sources.capacity() * size_of::<Entry<Arc<GroundContainerSource>>>() + self.originals.capacity() * size_of::<Arc<GroundContainerSource>>()
            + self.sources.len() * (size_of::<GroundContainerSource>() + 2 * size_of::<usize>())
            + self.sources.iter().map(|entry| entry.value.operands.len() * size_of::<GroundContainerOperand>() + (entry.value.instruction_payload.len() + entry.value.block_payload.len() + entry.value.creation_check.as_ref().map_or(0, |check| check.payload.len())) * size_of::<u32>()).sum::<usize>()
            + self.sources.iter().flat_map(|entry| entry.value.operands.iter()).map(|operand| operand.source_wrappers.len() * size_of::<ValueInitializerWrapper>() + operand.source_wrappers.iter().map(|wrapper| wrapper.payload.len() * size_of::<u32>()).sum::<usize>()).sum::<usize>()
            + self.keys.capacity() * size_of::<Entry<Arc<OriginalNamedMapKey>>>() + self.original_keys.capacity() * size_of::<Arc<OriginalNamedMapKey>>()
            + self.keys.len() * (size_of::<OriginalNamedMapKey>() + 2 * size_of::<usize>())
            + (self.instructions.capacity() + self.key_instructions.capacity()) * size_of::<(u32, usize)>()
    }
    pub(super) fn shrink_to_fit(&mut self) { self.sources.shrink_to_fit(); self.originals.shrink_to_fit(); self.keys.shrink_to_fit(); self.original_keys.shrink_to_fit(); self.instructions.shrink_to_fit(); self.key_instructions.shrink_to_fit(); }
}

impl GenericEvidenceStore {
    pub fn ground_container_source(&self, id: GroundContainerSourceId) -> Result<&GroundContainerSource, IrVerifyError> {
        let value = owned(self.root, &self.containers.sources, id.index, id.proof)?;
        if !self.containers.originals.get(id.index as usize).is_some_and(|original| Arc::ptr_eq(value, original)) { return Err(failure("container differs from its original receipt")); }
        Ok(value.as_ref())
    }
    pub fn ground_containers(&self) -> impl Iterator<Item = (GroundContainerSourceId, &GroundContainerSource)> {
        self.containers.sources.iter().enumerate().map(|(index, entry)| (GroundContainerSourceId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } }, entry.value.as_ref()))
    }
    pub fn ground_container_at(&self, instruction: u32) -> Result<Option<GroundContainerSourceId>, IrVerifyError> {
        let Some(index) = self.containers.instructions.binary_search_by_key(&instruction, |entry| entry.0).ok().map(|index| self.containers.instructions[index].1) else { return Ok(None); };
        let entry = self.containers.sources.get(index).ok_or_else(|| failure("container instruction index is stale"))?;
        let id = GroundContainerSourceId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } };
        self.ground_container_source(id)?; Ok(Some(id))
    }
    pub fn named_map_key_source(&self, id: NamedMapKeySourceId) -> Result<&OriginalNamedMapKey, IrVerifyError> {
        let value = owned(self.root, &self.containers.keys, id.index, id.proof)?;
        if !self.containers.original_keys.get(id.index as usize).is_some_and(|original| Arc::ptr_eq(value, original)) { return Err(failure("named map key differs from its original receipt")); }
        Ok(value.as_ref())
    }
    pub(super) fn verify_container_evidence(&self, pools: &SemanticPools, owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        if self.containers.sources.len() != self.containers.originals.len() || self.containers.keys.len() != self.containers.original_keys.len() { return Err(failure("container original ledger is incomplete")); }
        let mut key_index = Vec::new();
        for (index, entry) in self.containers.keys.iter().enumerate() {
            let id = NamedMapKeySourceId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } };
            let key = self.named_map_key_source(id)?;
            if owners.get(key.instruction as usize) != Some(&Some(key.owner)) || key.span.source_id != key.container.source || pools.to_type(key.checked)? != crate::sema::types::Type::Str
                || self.registered_instruction_origin(key.instruction, false).is_some() { return Err(failure("named map key changes its original source entry or checked type")); }
            key_index.push((key.instruction, index));
        }
        key_index.sort_unstable_by_key(|entry| entry.0);
        if key_index.windows(2).any(|pair| pair[0].0 == pair[1].0) || key_index != self.containers.key_instructions { return Err(failure("named map key index is ambiguous or stale")); }
        let mut container_index = Vec::new();
        for (id, _) in self.ground_containers() {
            let source = self.ground_container_source(id)?;
            if owners.get(source.instruction as usize) != Some(&Some(source.owner)) || self.registered_instruction_origin(source.instruction, false) != Some((OperationSourceOrigin::Expression(source.origin), source.owner)) {
                return Err(failure("container changes its original instruction or owner"));
            }
            if let Some(scope) = source.scope && source.owner != InstructionOwner::Function(self.scope(scope)?.owner) { return Err(failure("container belongs to another declaration scope")); }
            let result = pools.to_type(source.result)?;
            if !matches!((&result, source.kind), (crate::sema::types::Type::List(_), ContainerKind::List) | (crate::sema::types::Type::Map(_, _), ContainerKind::Map)) { return Err(failure("container changes its checked result kind")); }
            if let Some(check) = &source.creation_check {
                if owners.get(check.instruction as usize) != Some(&Some(source.owner)) || self.registered_instruction_origin(check.instruction, false).is_some() || check.payload.len() < 2 || check.payload[0] != source.instruction
                    || check.payload[1] != source.result.raw() { return Err(failure("container creation check changes its original validation boundary")); }
            }
            let mut roles = std::collections::BTreeSet::new();
            for operand in &source.operands {
                if [operand.instruction, operand.source_instruction].into_iter().any(|instruction| owners.get(instruction as usize) != Some(&Some(source.owner))) { return Err(failure("container operand belongs to another body")); }
                match operand.origin {
                    ContainerOperandOrigin::Expression(origin) => {
                        if origin.source != source.origin.source || origin.namespace != source.origin.namespace || self.registered_instruction_origin(operand.source_instruction, false) != Some((OperationSourceOrigin::Expression(origin), source.owner)) { return Err(failure("container operand lost its original expression")); }
                    }
                    ContainerOperandOrigin::NamedMapKey(id) => {
                        let key = self.named_map_key_source(id)?;
                        if key.container != source.origin || key.instruction != operand.instruction || operand.source_instruction != operand.instruction || !operand.source_wrappers.is_empty() || operand.source_type != operand.ty || key.owner != source.owner || operand.role != ContainerOperandRole::MapKey(key.entry_index) || operand.ty != key.checked { return Err(failure("container named key changes its original entry")); }
                    }
                }
                let actual = pools.to_type(operand.ty)?;
                let original = pools.to_type(operand.source_type)?;
                if original != actual && !(original == crate::sema::types::Type::Int && actual == crate::sema::types::Type::UInt && operand.source_wrappers.iter().any(|wrapper| wrapper.kind == ValueInitializerWrapperKind::CheckedValue)) {
                    return Err(failure("container operand changes its original checked conversion"));
                }
                let (role, expected) = match (&result, operand.role) {
                    (crate::sema::types::Type::List(item), ContainerOperandRole::ListItem(index)) => ((0, index), item.as_ref()),
                    (crate::sema::types::Type::List(_), ContainerOperandRole::ListSplice(index)) if matches!(actual, crate::sema::types::Type::List(_)) => ((0, index), &result),
                    (crate::sema::types::Type::Map(key, _), ContainerOperandRole::MapKey(index)) => ((1, index), key.as_ref()),
                    (crate::sema::types::Type::Map(_, value), ContainerOperandRole::MapValue(index)) => ((2, index), value.as_ref()),
                    (crate::sema::types::Type::Map(_, _), ContainerOperandRole::MapSpread(index)) => ((3, index), &result),
                    _ => return Err(failure("container operand has another checked role")),
                };
                let admitted = expected == &crate::sema::types::Type::Any || parameter_accepts(expected, &actual)
                    || matches!(expected, crate::sema::types::Type::Optional(inner) if actual == crate::sema::types::Type::Null || parameter_accepts(inner, &actual));
                let admitted = admitted || source.creation_check.is_some() && checked_container_boundary_accepts(expected, &actual);
                if !roles.insert(role) || !admitted { return Err(failure("container operand changes its checked type relationship")); }
            }
            container_index.push((source.instruction, id.index as usize));
        }
        container_index.sort_unstable_by_key(|entry| entry.0);
        if container_index.windows(2).any(|pair| pair[0].0 == pair[1].0) || container_index != self.containers.instructions { return Err(failure("container instruction index is ambiguous or stale")); }
        Ok(())
    }
}

impl GenericEvidenceBuilder {
    pub fn add_ground_container(&mut self, value: GroundContainerSource) -> Result<GroundContainerSourceId, IrVerifyError> {
        if self.store.containers.sources.len() >= 2_000_000 || value.operands.len() > 2_000_000 || value.block_payload.len() > 16_000_000 { return Err(failure("container proof exceeds its work limit")); }
        let index = self.store.containers.sources.len() as u32;
        let serial = self.next_serial; self.next_serial = serial.checked_add(1).ok_or_else(|| failure("container serial overflow"))?;
        let value = Arc::new(value); self.store.containers.originals.push(Arc::clone(&value)); self.store.containers.sources.push(Entry { serial, value });
        Ok(GroundContainerSourceId { index, proof: OwnerProof { root: self.store.root, serial } })
    }
    pub fn add_named_map_key(&mut self, value: OriginalNamedMapKey) -> Result<NamedMapKeySourceId, IrVerifyError> {
        if self.store.containers.keys.len() >= 2_000_000 { return Err(failure("named map key proof exceeds its work limit")); }
        let index = self.store.containers.keys.len() as u32;
        let serial = self.next_serial; self.next_serial = serial.checked_add(1).ok_or_else(|| failure("container serial overflow"))?;
        let value = Arc::new(value); self.store.containers.original_keys.push(Arc::clone(&value)); self.store.containers.keys.push(Entry { serial, value });
        Ok(NamedMapKeySourceId { index, proof: OwnerProof { root: self.store.root, serial } })
    }
    pub(in crate::runtime::eval) fn named_map_key_sources(&self) -> impl Iterator<Item = (NamedMapKeySourceId, &OriginalNamedMapKey)> {
        self.store.containers.keys.iter().enumerate().map(|(index, entry)| (
            NamedMapKeySourceId { index: index as u32, proof: OwnerProof { root: self.store.root, serial: entry.serial } }, entry.value.as_ref()))
    }
}

#[cfg(test)]
impl GenericEvidenceStore {
    pub(in crate::runtime::eval) fn test_ground_container_mut(&mut self, id: GroundContainerSourceId) -> Result<&mut GroundContainerSource, IrVerifyError> {
        self.ground_container_source(id)?; Ok(Arc::make_mut(&mut self.containers.sources[id.index as usize].value))
    }
    pub(in crate::runtime::eval) fn test_remove_ground_containers(&mut self) { self.containers.sources.clear(); self.containers.instructions.clear(); }
}
