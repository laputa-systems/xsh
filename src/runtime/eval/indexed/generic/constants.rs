use super::*;
use crate::runtime::eval::{LoweredValue, require::PreparedSchema};
use crate::sema::check::{DeclarationIdentity, ExpressionIdentity};
use crate::sema::constants::LiteralConstant;
use crate::sema::inference::ScopedRoot;
use std::mem::size_of;

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct OriginalConstantSource {
    pub origin: ExpressionIdentity,
    pub checked: ScopedRoot,
    pub selection: ExpressionIdentity,
    pub lexical_owner: Option<DeclarationIdentity>,
    pub literal: Arc<LiteralConstant>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) struct ConstantSourceId { index: u32, proof: OwnerProof }

/// Constant expansion carries one authored expression and its selected literal
/// allocation. Its closed type comes from that expression's checked root.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedConstantSource {
    pub original: OriginalConstantSource,
    pub instruction: u32,
    pub owner: InstructionOwner,
    pub ty: GroundTypeId,
    pub pool: u32,
    pub value: LoweredValue,
    pub schema: Arc<PreparedSchema>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct ConstantEvidence {
    sources: Vec<Entry<Arc<PreparedConstantSource>>>,
    originals: Vec<Arc<PreparedConstantSource>>,
    instructions: Vec<(u32, ConstantSourceId)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ConstantCheckpoint { sources: usize }

impl ConstantEvidence {
    pub(super) fn checkpoint(&self) -> ConstantCheckpoint { ConstantCheckpoint { sources: self.sources.len() } }
    pub(super) fn validate_checkpoint(&self, checkpoint: ConstantCheckpoint, serial_limit: u64) -> Result<(), IrVerifyError> {
        if checkpoint.sources > self.sources.len() || self.sources.get(checkpoint.sources.wrapping_sub(1)).is_some_and(|entry| entry.serial >= serial_limit) { return Err(failure("constant checkpoint references retired or replaced receipts")); }
        Ok(())
    }
    pub(super) fn rewind_validated(&mut self, checkpoint: ConstantCheckpoint) { self.sources.truncate(checkpoint.sources); self.originals.truncate(checkpoint.sources); self.instructions.clear(); }
    pub(super) fn finish(&mut self, root: u64) {
        self.instructions = self.sources.iter().enumerate().map(|(index, entry)| (entry.value.instruction,
            ConstantSourceId { index: index as u32, proof: OwnerProof { root, serial: entry.serial } })).collect();
        self.instructions.sort_unstable_by_key(|entry| entry.0);
    }
    pub(super) fn shrink_to_fit(&mut self) { self.sources.shrink_to_fit(); self.originals.shrink_to_fit(); self.instructions.shrink_to_fit(); }
    pub(super) fn retained_bytes(&self) -> usize {
        self.sources.capacity() * size_of::<Entry<Arc<PreparedConstantSource>>>() + self.originals.capacity() * size_of::<Arc<PreparedConstantSource>>()
            + self.instructions.capacity() * size_of::<(u32, ConstantSourceId)>()
            + self.sources.iter().map(|entry| size_of::<PreparedConstantSource>() + 2 * size_of::<usize>() + entry.value.schema.retained_bytes()
                + literal_heap_bytes(&entry.value.original.literal) + constant_value_heap_bytes(&entry.value.value)).sum::<usize>()
    }
}

fn constant_value_heap_bytes(value: &LoweredValue) -> usize {
    let mut bytes = 0;
    let mut seen = std::collections::BTreeSet::new();
    let mut pending = vec![value];
    while let Some(value) = pending.pop() {
        match value {
            LoweredValue::Str(value) => { if seen.insert(Arc::as_ptr(value) as *const u8 as usize) { bytes += value.len() + 2 * size_of::<usize>(); } }
            LoweredValue::Bytes(value) => { if seen.insert(Arc::as_ptr(value) as *const u8 as usize) { bytes += value.len() + 2 * size_of::<usize>(); } }
            LoweredValue::Path(value) => { bytes += value.bytes.capacity(); }
            LoweredValue::Regex(value) => { bytes += size_of::<crate::runtime::value::RegexValue>() + value.pattern.capacity(); }
            LoweredValue::List(values) => { bytes += values.capacity() * size_of::<LoweredValue>(); pending.extend(values); }
            LoweredValue::SharedList(values) => {
                if seen.insert(Arc::as_ptr(values) as usize) { bytes += size_of::<Vec<LoweredValue>>() + values.capacity() * size_of::<LoweredValue>() + 2 * size_of::<usize>(); pending.extend(values.iter()); }
            }
            LoweredValue::RecordVec(values) => {
                if seen.insert(Arc::as_ptr(values) as usize) { bytes += size_of::<Vec<(Name, LoweredValue)>>() + values.capacity() * size_of::<(Name, LoweredValue)>() + 2 * size_of::<usize>(); pending.extend(values.iter().map(|(_, value)| value)); }
            }
            LoweredValue::Record(values) => {
                if seen.insert(Arc::as_ptr(values) as usize) {
                    bytes += size_of::<std::collections::BTreeMap<Arc<str>, LoweredValue>>() + values.len() * (size_of::<(Arc<str>, LoweredValue)>() + 3 * size_of::<usize>()) + 2 * size_of::<usize>(); pending.extend(values.values());
                    for key in values.keys() { if seen.insert(Arc::as_ptr(key) as *const u8 as usize) { bytes += key.len() + 2 * size_of::<usize>(); } }
                }
            }
            LoweredValue::Map(values) => {
                if seen.insert(Arc::as_ptr(values) as usize) {
                    bytes += size_of::<std::collections::BTreeMap<crate::map_key::MapKey, LoweredValue>>() + values.len() * (size_of::<(crate::map_key::MapKey, LoweredValue)>() + 3 * size_of::<usize>()) + 2 * size_of::<usize>(); pending.extend(values.values());
                    for key in values.keys() {
                        let allocation = match key { crate::map_key::MapKey::Str(value) => Some((Arc::as_ptr(value) as *const u8 as usize, value.len())), crate::map_key::MapKey::Bytes(value) | crate::map_key::MapKey::Path(value) => Some((Arc::as_ptr(value) as *const u8 as usize, value.len())), _ => None };
                        if let Some((pointer, length)) = allocation && seen.insert(pointer) { bytes += length + 2 * size_of::<usize>(); }
                    }
                }
            }
            LoweredValue::Tag(value) => {
                bytes += size_of::<crate::runtime::eval::LoweredTagValue>() + value.fields.capacity() * size_of::<LoweredValue>(); pending.extend(value.fields.iter());
                if seen.insert(Arc::as_ptr(&value.name) as *const u8 as usize) { bytes += value.name.len() + 2 * size_of::<usize>(); }
            }
            _ => {},
        }
    }
    bytes
}

fn literal_heap_bytes(value: &LiteralConstant) -> usize {
    let mut bytes = size_of::<LiteralConstant>() + 2 * size_of::<usize>();
    let mut seen = std::collections::BTreeSet::new();
    let mut pending = vec![value];
    while let Some(value) = pending.pop() {
        match value {
            LiteralConstant::Str(value) | LiteralConstant::Path(value) => { if seen.insert(Arc::as_ptr(value) as *const u8 as usize) { bytes += value.len() + 2 * size_of::<usize>(); } }
            LiteralConstant::Bytes(value) => { if seen.insert(Arc::as_ptr(value) as *const u8 as usize) { bytes += value.len() + 2 * size_of::<usize>(); } }
            LiteralConstant::List(values) | LiteralConstant::Tag { fields: values, .. } => {
                if seen.insert(Arc::as_ptr(values) as usize) { bytes += size_of::<Vec<LiteralConstant>>() + values.capacity() * size_of::<LiteralConstant>() + 2 * size_of::<usize>(); pending.extend(values.iter()); }
            }
            LiteralConstant::Record(values) => {
                if seen.insert(Arc::as_ptr(values) as usize) { bytes += size_of::<std::collections::BTreeMap<Name, LiteralConstant>>() + values.len() * (size_of::<(Name, LiteralConstant)>() + 3 * size_of::<usize>()) + 2 * size_of::<usize>(); pending.extend(values.values()); }
            }
            LiteralConstant::Map(values) => {
                if seen.insert(Arc::as_ptr(values) as usize) {
                    bytes += size_of::<std::collections::BTreeMap<crate::map_key::MapKey, LiteralConstant>>() + values.len() * (size_of::<(crate::map_key::MapKey, LiteralConstant)>() + 3 * size_of::<usize>()) + 2 * size_of::<usize>(); pending.extend(values.values());
                    for key in values.keys() {
                        let allocation = match key { crate::map_key::MapKey::Str(value) => Some((Arc::as_ptr(value) as *const u8 as usize, value.len())), crate::map_key::MapKey::Bytes(value) | crate::map_key::MapKey::Path(value) => Some((Arc::as_ptr(value) as *const u8 as usize, value.len())), _ => None };
                        if let Some((pointer, length)) = allocation && seen.insert(pointer) { bytes += length + 2 * size_of::<usize>(); }
                    }
                }
            }
            _ => {},
        }
    }
    bytes
}

impl GenericEvidenceStore {
    pub fn constant_source(&self, id: ConstantSourceId) -> Result<&PreparedConstantSource, IrVerifyError> {
        let source = owned(self.root, &self.constants.sources, id.index, id.proof)?;
        if !self.constants.originals.get(id.index as usize).is_some_and(|original| Arc::ptr_eq(source, original)) { return Err(failure("constant source differs from its original receipt")); }
        Ok(source)
    }
    pub fn constant_sources(&self) -> impl Iterator<Item = (ConstantSourceId, &PreparedConstantSource)> {
        self.constants.sources.iter().enumerate().map(|(index, entry)| (ConstantSourceId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } }, entry.value.as_ref()))
    }
    pub fn constant_source_at(&self, instruction: u32) -> Result<Option<&PreparedConstantSource>, IrVerifyError> {
        self.constants.instructions.binary_search_by_key(&instruction, |entry| entry.0).ok().map(|index| self.constant_source(self.constants.instructions[index].1)).transpose()
    }
    pub(super) fn verify_constant_evidence(&self, pools: &SemanticPools, owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        if self.constants.sources.len() != self.constants.originals.len() { return Err(failure("constant original receipt ledger is incomplete")); }
        let mut expected = Vec::new();
        for (id, _) in self.constant_sources() {
            let source = self.constant_source(id)?;
            if owners.get(source.instruction as usize) != Some(&Some(source.owner))
                || self.registered_instruction_origin(source.instruction, false) != Some((OperationSourceOrigin::Expression(source.original.origin), source.owner))
                || !source.schema.valid() || !source.schema.matches_type(&pools.to_type(source.ty)?) {
                return Err(failure("constant source changes its original owner, checked type, or physical schema"));
            }
            expected.push((source.instruction, id));
        }
        expected.sort_unstable_by_key(|entry| entry.0);
        if expected.windows(2).any(|pair| pair[0].0 == pair[1].0) || expected != self.constants.instructions { return Err(failure("constant instruction index is incomplete or ambiguous")); }
        Ok(())
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_constant_source_mut(&mut self, id: ConstantSourceId) -> Result<&mut PreparedConstantSource, IrVerifyError> { self.constant_source(id)?; Ok(Arc::make_mut(&mut self.constants.sources[id.index as usize].value)) }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_remove_constant_sources(&mut self) { self.constants.sources.clear(); self.constants.instructions.clear(); }
}

impl GenericEvidenceBuilder {
    pub fn add_constant_source(&mut self, source: PreparedConstantSource) -> Result<ConstantSourceId, IrVerifyError> {
        if self.store.constants.sources.len() >= 2_000_000 { return Err(failure("constant sources exceed their work limit")); }
        let index = u32::try_from(self.store.constants.sources.len()).map_err(|_| failure("constant source id overflow"))?;
        let serial = self.next_serial; self.next_serial = serial.checked_add(1).ok_or_else(|| failure("constant source serial overflow"))?;
        let source = Arc::new(source); self.store.constants.originals.push(Arc::clone(&source)); self.store.constants.sources.push(Entry { serial, value: source });
        Ok(ConstantSourceId { index, proof: OwnerProof { root: self.store.root, serial } })
    }
}
