use super::*;
use crate::sema::check::ExpressionIdentity;
use crate::sema::inference::ScopedRoot;
use crate::sema::types::Type;
use std::collections::BTreeMap;
use std::mem::size_of;
pub(in crate::runtime::eval) use crate::runtime::eval::lower::record_binding::OriginalRecordEntryKind as RecordEntryKind;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) struct RecordSourceId { index: u32, proof: OwnerProof }

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct RecordChildWrapper {
    pub instruction: u32,
    pub payload: Box<[u32]>,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedRecordEntry {
    pub kind: RecordEntryKind,
    pub origin: ExpressionIdentity,
    pub checked: ScopedRoot,
    pub instruction: u32,
    pub source_instruction: u32,
    pub ty: GroundTypeId,
    pub wrappers: Box<[RecordChildWrapper]>,
}

/// The original source owns every supplied value and spread. The flattened
/// semantic row proves the result type but supplies no physical field slots.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedRecordSource {
    pub origin: ExpressionIdentity,
    pub checked: ScopedRoot,
    pub instruction: u32,
    pub owner: InstructionOwner,
    pub result: GroundTypeId,
    pub layout: PhysicalLayoutId,
    pub schema: Arc<crate::runtime::eval::require::PreparedSchema>,
    pub entries: Box<[PreparedRecordEntry]>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct RecordEvidence {
    sources: Vec<Entry<Arc<PreparedRecordSource>>>,
    originals: Vec<Arc<PreparedRecordSource>>,
    instructions: Vec<(u32, RecordSourceId)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RecordCheckpoint { sources: usize }

impl RecordEvidence {
    pub(super) fn checkpoint(&self) -> RecordCheckpoint { RecordCheckpoint { sources: self.sources.len() } }
    pub(super) fn validate_checkpoint(&self, checkpoint: RecordCheckpoint, serial_limit: u64) -> Result<(), IrVerifyError> {
        if checkpoint.sources > self.sources.len() || self.sources.get(checkpoint.sources.wrapping_sub(1)).is_some_and(|entry| entry.serial >= serial_limit) {
            return Err(failure("record source checkpoint references retired or replaced entries"));
        }
        Ok(())
    }
    pub(super) fn rewind_validated(&mut self, checkpoint: RecordCheckpoint) {
        self.sources.truncate(checkpoint.sources); self.originals.truncate(checkpoint.sources); self.instructions.clear();
    }
    pub(super) fn finish(&mut self, root: u64) {
        self.instructions = self.sources.iter().enumerate().map(|(index, entry)| (entry.value.instruction, RecordSourceId {
            index: index as u32, proof: OwnerProof { root, serial: entry.serial },
        })).collect();
        self.instructions.sort_unstable_by_key(|entry| entry.0);
    }
    pub(super) fn retained_bytes(&self) -> usize {
        self.sources.capacity() * size_of::<Entry<Arc<PreparedRecordSource>>>()
            + self.originals.capacity() * size_of::<Arc<PreparedRecordSource>>()
            + self.instructions.capacity() * size_of::<(u32, RecordSourceId)>()
            + self.sources.iter().map(|entry| size_of::<PreparedRecordSource>() + 2 * size_of::<usize>()
                + entry.value.schema.retained_bytes()
                + entry.value.entries.len() * size_of::<PreparedRecordEntry>()
                + entry.value.entries.iter().map(|entry| entry.wrappers.len() * size_of::<RecordChildWrapper>()
                    + entry.wrappers.iter().map(|wrapper| wrapper.payload.len() * size_of::<u32>()).sum::<usize>()).sum::<usize>()).sum::<usize>()
    }
    pub(super) fn shrink_to_fit(&mut self) { self.sources.shrink_to_fit(); self.originals.shrink_to_fit(); self.instructions.shrink_to_fit(); }
}

impl GenericEvidenceStore {
    pub fn record_source(&self, id: RecordSourceId) -> Result<&PreparedRecordSource, IrVerifyError> {
        let source = owned(self.root, &self.records.sources, id.index, id.proof)?;
        if !self.records.originals.get(id.index as usize).is_some_and(|original| Arc::ptr_eq(source, original)) {
            return Err(failure("record source differs from its original receipt"));
        }
        Ok(source.as_ref())
    }
    pub fn record_sources(&self) -> impl Iterator<Item = (RecordSourceId, &PreparedRecordSource)> {
        self.records.sources.iter().enumerate().map(|(index, entry)| (RecordSourceId {
            index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial },
        }, entry.value.as_ref()))
    }
    pub fn record_source_at(&self, instruction: u32) -> Result<Option<&PreparedRecordSource>, IrVerifyError> {
        self.records.instructions.binary_search_by_key(&instruction, |entry| entry.0).ok()
            .map(|index| self.record_source(self.records.instructions[index].1)).transpose()
    }
    pub(super) fn verify_record_evidence(&self, pools: &SemanticPools, owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        if self.records.sources.len() != self.records.originals.len() { return Err(failure("record original receipt ledger is incomplete")); }
        let mut expected = Vec::new();
        for (id, _) in self.record_sources() {
            let source = self.record_source(id)?;
            if owners.get(source.instruction as usize) != Some(&Some(source.owner))
                || self.registered_instruction_origin(source.instruction, false) != Some((OperationSourceOrigin::Expression(source.origin), source.owner)) {
                return Err(failure("record source changes its original instruction or owner"));
            }
            let Type::Record(result) = pools.to_type(source.result)? else { return Err(failure("record source result is not a closed record")); };
            let layout = self.layout(source.layout)?;
            let crate::runtime::eval::require::PreparedSchema::Record(required) = source.schema.as_ref() else {
                return Err(failure("record source has no physical record schema"));
            };
            if layout.record_type != source.result || !source.schema.valid() || !source.schema.matches_type(&Type::Record(result.clone()))
                || !source.schema.visit_wire_mappings(&mut |_| false)
                || required.len() != layout.fields.len() || required.iter().zip(layout.fields.iter()).any(|((name, _), (field, _))| name != field) {
                return Err(failure("record source changes its original result layout or physical schema"));
            }
            let mut supplied = BTreeMap::new();
            let mut spread = false;
            for entry in source.entries.iter() {
                if entry.origin.source != source.origin.source || entry.origin.namespace != source.origin.namespace
                    || owners.get(entry.instruction as usize) != Some(&Some(source.owner))
                    || owners.get(entry.source_instruction as usize) != Some(&Some(source.owner))
                    || entry.wrappers.len() > 256 || entry.wrappers.iter().any(|wrapper| owners.get(wrapper.instruction as usize) != Some(&Some(source.owner)))
                    || self.registered_instruction_origin(entry.source_instruction, false) != Some((OperationSourceOrigin::Expression(entry.origin), source.owner)) {
                    return Err(failure("record entry changes its original source or owner"));
                }
                let ty = pools.to_type(entry.ty)?;
                match entry.kind {
                    RecordEntryKind::Field(name) => { supplied.insert(name, ty); }
                    RecordEntryKind::Spread => {
                        spread = true;
                        let Type::Record(fields) = ty else { return Err(failure("record spread source is not a closed record")); };
                        supplied.extend(fields);
                    }
                }
            }
            if !spread || supplied != result { return Err(failure("record source result differs from its original supplied entries")); }
            expected.push((source.instruction, id));
        }
        expected.sort_unstable_by_key(|entry| entry.0);
        if expected.windows(2).any(|pair| pair[0].0 == pair[1].0) || expected != self.records.instructions {
            return Err(failure("record source instruction index is incomplete or ambiguous"));
        }
        Ok(())
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_record_source_mut(&mut self, id: RecordSourceId) -> Result<&mut PreparedRecordSource, IrVerifyError> {
        self.record_source(id)?; Ok(Arc::make_mut(&mut self.records.sources[id.index as usize].value))
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_record_layout_mut(&mut self, id: RecordSourceId) -> Result<&mut PhysicalLayout, IrVerifyError> {
        let layout = self.record_source(id)?.layout;
        self.layout(layout)?;
        Ok(&mut self.layouts[layout.index as usize].value)
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_remove_record_sources(&mut self) { self.records.sources.clear(); self.records.originals.clear(); self.records.instructions.clear(); }
}

impl GenericEvidenceBuilder {
    pub fn add_record_source(&mut self, source: PreparedRecordSource) -> Result<RecordSourceId, IrVerifyError> {
        if self.store.records.sources.len() >= 2_000_000 || source.entries.len() > 65536 { return Err(failure("record sources exceed their work limit")); }
        let index = u32::try_from(self.store.records.sources.len()).map_err(|_| failure("record source id overflow"))?;
        let serial = self.next_serial;
        self.next_serial = serial.checked_add(1).ok_or_else(|| failure("generic evidence serial overflow"))?;
        let source = Arc::new(source);
        self.store.records.originals.push(Arc::clone(&source));
        self.store.records.sources.push(Entry { serial, value: source });
        Ok(RecordSourceId { index, proof: OwnerProof { root: self.store.root, serial } })
    }
}
