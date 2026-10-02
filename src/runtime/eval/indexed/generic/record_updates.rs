use super::*;
use crate::runtime::eval::require::PreparedSchema;
use crate::sema::check::{ExpressionIdentity, RecordUpdateValueSource};
use crate::sema::inference::ScopedRoot;
use crate::sema::types::Type;
use super::super::full::FullTag;
use std::mem::size_of;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) struct RecordUpdateSourceId { index: u32, proof: OwnerProof }

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedUpdateValue {
    pub origin: ExpressionIdentity,
    pub checked: ScopedRoot,
    pub instruction: u32,
    pub source_instruction: u32,
    pub ty: GroundTypeId,
    pub wrappers: Box<[PreparedUpdateWrapper]>,
    pub source_tag: FullTag,
    pub source_payload: Box<[u32]>,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedUpdateWrapper { pub instruction: u32, pub tag: FullTag, pub payload: Box<[u32]>, pub child: u32 }

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedUpdateProjection {
    pub receiver: GroundTypeId,
    pub receiver_root: ScopedRoot,
    pub field: Name,
    pub result: GroundTypeId,
    pub result_root: ScopedRoot,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedRecordReplacement {
    pub path: Box<[Name]>,
    pub projections: Box<[PreparedUpdateProjection]>,
    pub assignability: usize,
    pub supplied_source: RecordUpdateValueSource,
    pub producer_flow: crate::sema::check::ProducerFlowId,
    pub value: PreparedUpdateValue,
}

/// The selected paths constrain replacements while the receiver owns the
/// complete result row and its canonical physical layout.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedRecordUpdateSource {
    pub origin: ExpressionIdentity,
    pub checked: ScopedRoot,
    pub instruction: u32,
    pub owner: InstructionOwner,
    pub result: GroundTypeId,
    pub layout: PhysicalLayoutId,
    pub schema: Arc<PreparedSchema>,
    pub base: PreparedUpdateValue,
    pub replacements: Box<[PreparedRecordReplacement]>,
}

fn value_bytes(value: &PreparedUpdateValue) -> usize {
    value.source_payload.len() * size_of::<u32>() + value.wrappers.len() * size_of::<PreparedUpdateWrapper>()
        + value.wrappers.iter().map(|wrapper| wrapper.payload.len() * size_of::<u32>()).sum::<usize>()
}

#[derive(Clone, Debug, Default)]
pub(super) struct RecordUpdateEvidence {
    sources: Vec<Entry<Arc<PreparedRecordUpdateSource>>>,
    originals: Vec<Arc<PreparedRecordUpdateSource>>,
    instructions: Vec<(u32, RecordUpdateSourceId)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RecordUpdateCheckpoint { sources: usize }

impl RecordUpdateEvidence {
    pub(super) fn checkpoint(&self) -> RecordUpdateCheckpoint { RecordUpdateCheckpoint { sources: self.sources.len() } }
    pub(super) fn validate_checkpoint(&self, checkpoint: RecordUpdateCheckpoint, serial_limit: u64) -> Result<(), IrVerifyError> {
        if checkpoint.sources > self.sources.len() || self.sources.get(checkpoint.sources.wrapping_sub(1)).is_some_and(|entry| entry.serial >= serial_limit) {
            return Err(failure("record update source checkpoint references retired or replaced entries"));
        }
        Ok(())
    }
    pub(super) fn rewind_validated(&mut self, checkpoint: RecordUpdateCheckpoint) {
        self.sources.truncate(checkpoint.sources); self.originals.truncate(checkpoint.sources); self.instructions.clear();
    }
    pub(super) fn finish(&mut self, root: u64) {
        self.instructions = self.sources.iter().enumerate().map(|(index, entry)| (entry.value.instruction, RecordUpdateSourceId {
            index: index as u32, proof: OwnerProof { root, serial: entry.serial },
        })).collect();
        self.instructions.sort_unstable_by_key(|entry| entry.0);
    }
    pub(super) fn retained_bytes(&self) -> usize {
        self.sources.capacity() * size_of::<Entry<Arc<PreparedRecordUpdateSource>>>()
            + self.originals.capacity() * size_of::<Arc<PreparedRecordUpdateSource>>()
            + self.instructions.capacity() * size_of::<(u32, RecordUpdateSourceId)>()
            + self.sources.iter().map(|entry| {
                let source = &entry.value;
                size_of::<PreparedRecordUpdateSource>() + 2 * size_of::<usize>() + source.schema.retained_bytes()
                    + value_bytes(&source.base) + source.replacements.len() * size_of::<PreparedRecordReplacement>()
                    + source.replacements.iter().map(|replacement| replacement.path.len() * size_of::<Name>()
                        + replacement.projections.len() * size_of::<PreparedUpdateProjection>() + value_bytes(&replacement.value)).sum::<usize>()
            }).sum::<usize>()
    }
    pub(super) fn shrink_to_fit(&mut self) { self.sources.shrink_to_fit(); self.originals.shrink_to_fit(); self.instructions.shrink_to_fit(); }
}

impl GenericEvidenceStore {
    pub fn record_update_source(&self, id: RecordUpdateSourceId) -> Result<&PreparedRecordUpdateSource, IrVerifyError> {
        let source = owned(self.root, &self.record_updates.sources, id.index, id.proof)?;
        if !self.record_updates.originals.get(id.index as usize).is_some_and(|original| Arc::ptr_eq(source, original)) {
            return Err(failure("record update source differs from its original receipt"));
        }
        Ok(source.as_ref())
    }
    pub fn record_update_sources(&self) -> impl Iterator<Item = (RecordUpdateSourceId, &PreparedRecordUpdateSource)> {
        self.record_updates.sources.iter().enumerate().map(|(index, entry)| (RecordUpdateSourceId {
            index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial },
        }, entry.value.as_ref()))
    }
    pub fn record_update_source_at(&self, instruction: u32) -> Result<Option<&PreparedRecordUpdateSource>, IrVerifyError> {
        self.record_updates.instructions.binary_search_by_key(&instruction, |entry| entry.0).ok()
            .map(|index| self.record_update_source(self.record_updates.instructions[index].1)).transpose()
    }
    pub(super) fn verify_record_update_evidence(&self, pools: &SemanticPools, owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        if self.record_updates.sources.len() != self.record_updates.originals.len() { return Err(failure("record update original receipt ledger is incomplete")); }
        let mut expected = Vec::new();
        for (id, _) in self.record_update_sources() {
            let source = self.record_update_source(id)?;
            if owners.get(source.instruction as usize) != Some(&Some(source.owner))
                || self.registered_instruction_origin(source.instruction, false) != Some((OperationSourceOrigin::Expression(source.origin), source.owner)) {
                return Err(failure("record update changes its original instruction or owner"));
            }
            let ty = pools.to_type(source.result)?;
            if !matches!(ty, Type::Record(_)) || source.base.ty != source.result { return Err(failure("record update does not preserve its complete original receiver row")); }
            let layout = self.layout(source.layout)?;
            let PreparedSchema::Record(required) = source.schema.as_ref() else { return Err(failure("record update has no canonical physical row schema")); };
            if layout.record_type != source.result || !source.schema.valid() || !source.schema.matches_type(&ty)
                || !source.schema.visit_wire_mappings(&mut |_| false) || required.len() != layout.fields.len()
                || required.iter().zip(layout.fields.iter()).any(|((name, _), (field, _))| name != field) {
                return Err(failure("record update changes its original canonical row layout"));
            }
            self.verify_record_update_value(pools, owners, source, &source.base)?;
            for (index, replacement) in source.replacements.iter().enumerate() {
                if replacement.path.is_empty() || replacement.path.len() != replacement.projections.len()
                    || source.replacements[..index].iter().any(|other| replacement.path.starts_with(&other.path) || other.path.starts_with(&replacement.path))
                    || replacement.supplied_source != RecordUpdateValueSource::Expression(replacement.value.origin) {
                    return Err(failure("record update changes its original disjoint paths or supplied replacement source"));
                }
                let mut selected = source.result;
                for (&field, projection) in replacement.path.iter().zip(replacement.projections.iter()) {
                    if projection.receiver != selected || projection.field != field { return Err(failure("record update changes its original projection chain")); }
                    let Type::Record(fields) = pools.to_type(selected)? else { return Err(failure("record update projection receiver is not a record")); };
                    if fields.get(&field) != Some(&pools.to_type(projection.result)?) { return Err(failure("record update projection selects another field type")); }
                    selected = projection.result;
                }
                self.verify_record_update_value(pools, owners, source, &replacement.value)?;
            }
            expected.push((source.instruction, id));
        }
        expected.sort_unstable_by_key(|entry| entry.0);
        if expected.windows(2).any(|pair| pair[0].0 == pair[1].0) || expected != self.record_updates.instructions { return Err(failure("record update source instruction index is incomplete or ambiguous")); }
        Ok(())
    }
    fn verify_record_update_value(&self, pools: &SemanticPools, owners: &[Option<InstructionOwner>], source: &PreparedRecordUpdateSource, value: &PreparedUpdateValue) -> Result<(), IrVerifyError> {
        Self::verify_type(pools, value.ty)?;
        if value.origin.source != source.origin.source || value.origin.namespace != source.origin.namespace
            || owners.get(value.instruction as usize) != Some(&Some(source.owner)) || owners.get(value.source_instruction as usize) != Some(&Some(source.owner))
            || value.wrappers.len() > 256 || value.wrappers.iter().any(|wrapper| owners.get(wrapper.instruction as usize) != Some(&Some(source.owner)))
            || self.registered_instruction_origin(value.source_instruction, false) != Some((OperationSourceOrigin::Expression(value.origin), source.owner)) {
            return Err(failure("record update value changes its original expression source or owner"));
        }
        Ok(())
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_record_update_source_mut(&mut self, id: RecordUpdateSourceId) -> Result<&mut PreparedRecordUpdateSource, IrVerifyError> {
        self.record_update_source(id)?; Ok(Arc::make_mut(&mut self.record_updates.sources[id.index as usize].value))
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_remove_record_update_sources(&mut self) { self.record_updates.sources.clear(); self.record_updates.instructions.clear(); }
}

impl GenericEvidenceBuilder {
    pub fn add_record_update_source(&mut self, source: PreparedRecordUpdateSource) -> Result<RecordUpdateSourceId, IrVerifyError> {
        if self.store.record_updates.sources.len() >= 2_000_000 || source.replacements.len() > 65536 { return Err(failure("record update sources exceed their work limit")); }
        let index = u32::try_from(self.store.record_updates.sources.len()).map_err(|_| failure("record update source id overflow"))?;
        let serial = self.next_serial;
        self.next_serial = serial.checked_add(1).ok_or_else(|| failure("generic evidence serial overflow"))?;
        let source = Arc::new(source);
        self.store.record_updates.originals.push(Arc::clone(&source));
        self.store.record_updates.sources.push(Entry { serial, value: source });
        Ok(RecordUpdateSourceId { index, proof: OwnerProof { root: self.store.root, serial } })
    }
}
