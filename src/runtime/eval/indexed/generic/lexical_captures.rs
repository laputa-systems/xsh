use super::*;
use crate::sema::check::{BindingIdentity, DeclarationIdentity, ExpressionIdentity};
use crate::sema::inference::ScopedRoot;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) struct LexicalCaptureId { index: u32, proof: OwnerProof }

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) struct LexicalCaptureSourceId { index: u32, proof: OwnerProof }

/// The declaration receives this original lexical binding in one header slot.
/// The checked binding root and definition owner survive independently of reads.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct LexicalCapture {
    pub binding: BindingIdentity,
    pub definition_owner: Option<DeclarationIdentity>,
    pub declaration: DeclarationIdentity,
    pub target: IrFunctionId,
    pub header_index: u32,
    pub slot: u32,
    pub name: u32,
    pub mutable: bool,
    pub source_type: ScopedRoot,
    pub original_type: crate::sema::types::Type,
    pub ty: GroundTypeId,
}

/// A read belongs to its original expression and receiving capture allocation.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct LexicalCaptureSource {
    pub capture: LexicalCaptureId,
    pub binding: BindingIdentity,
    pub declaration: DeclarationIdentity,
    pub origin: ExpressionIdentity,
    pub owner: InstructionOwner,
    pub instruction: u32,
    pub slot: u32,
    pub source_type: ScopedRoot,
    pub original_type: crate::sema::types::Type,
    pub ty: GroundTypeId,
    pub payload: Box<[u32]>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct LexicalCaptureEvidence {
    captures: Vec<Entry<Arc<LexicalCapture>>>,
    original_captures: Vec<Arc<LexicalCapture>>,
    capture_slots: Vec<(u32, u32, usize)>,
    sources: Vec<Entry<Arc<LexicalCaptureSource>>>,
    originals: Vec<Arc<LexicalCaptureSource>>,
    instructions: Vec<(u32, usize)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct LexicalCaptureCheckpoint { captures: usize, sources: usize }

impl LexicalCaptureEvidence {
    pub(super) fn checkpoint(&self) -> LexicalCaptureCheckpoint { LexicalCaptureCheckpoint { captures: self.captures.len(), sources: self.sources.len() } }
    pub(super) fn validate_checkpoint(&self, checkpoint: LexicalCaptureCheckpoint, serial_limit: u64) -> Result<(), IrVerifyError> {
        if checkpoint.captures > self.captures.len() || checkpoint.sources > self.sources.len()
            || self.captures.get(checkpoint.captures.wrapping_sub(1)).is_some_and(|entry| entry.serial >= serial_limit)
            || self.sources.get(checkpoint.sources.wrapping_sub(1)).is_some_and(|entry| entry.serial >= serial_limit) {
            return Err(failure("lexical capture checkpoint references retired or replaced entries"));
        }
        Ok(())
    }
    pub(super) fn rewind_validated(&mut self, checkpoint: LexicalCaptureCheckpoint) {
        self.captures.truncate(checkpoint.captures); self.original_captures.truncate(checkpoint.captures);
        self.sources.truncate(checkpoint.sources); self.originals.truncate(checkpoint.sources);
        self.instructions.clear(); self.index_capture_slots();
    }
    fn index_capture_slots(&mut self) {
        self.capture_slots = self.captures.iter().enumerate().map(|(index, entry)| (entry.value.target.raw(), entry.value.slot, index)).collect();
        self.capture_slots.sort_unstable();
    }
    pub(super) fn finish(&mut self) {
        self.index_capture_slots();
        self.instructions = self.sources.iter().enumerate().map(|(index, entry)| (entry.value.instruction, index)).collect();
        self.instructions.sort_unstable_by_key(|entry| entry.0);
    }
    pub(super) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        self.captures.capacity() * size_of::<Entry<Arc<LexicalCapture>>>() + self.original_captures.capacity() * size_of::<Arc<LexicalCapture>>()
            + self.captures.len() * (size_of::<LexicalCapture>() + 2 * size_of::<usize>())
            + self.sources.capacity() * size_of::<Entry<Arc<LexicalCaptureSource>>>() + self.originals.capacity() * size_of::<Arc<LexicalCaptureSource>>()
            + self.sources.len() * (size_of::<LexicalCaptureSource>() + 2 * size_of::<usize>())
            + self.capture_slots.capacity() * size_of::<(u32, u32, usize)>() + self.instructions.capacity() * size_of::<(u32, usize)>()
            + self.captures.iter().map(|entry| entry.value.original_type.retained_bytes().saturating_sub(size_of::<crate::sema::types::Type>())).sum::<usize>()
            + self.sources.iter().map(|entry| entry.value.payload.len() * size_of::<u32>() + entry.value.original_type.retained_bytes().saturating_sub(size_of::<crate::sema::types::Type>())).sum::<usize>()
    }
    pub(super) fn shrink_to_fit(&mut self) { self.captures.shrink_to_fit(); self.original_captures.shrink_to_fit(); self.capture_slots.shrink_to_fit(); self.sources.shrink_to_fit(); self.originals.shrink_to_fit(); self.instructions.shrink_to_fit(); }
}

impl GenericEvidenceStore {
    pub fn has_lexical_captures(&self) -> bool { !self.lexical_captures.captures.is_empty() || !self.lexical_captures.original_captures.is_empty() || !self.lexical_captures.sources.is_empty() || !self.lexical_captures.originals.is_empty() }
    pub fn lexical_capture(&self, id: LexicalCaptureId) -> Result<&LexicalCapture, IrVerifyError> {
        let capture = owned(self.root, &self.lexical_captures.captures, id.index, id.proof)?;
        if !self.lexical_captures.original_captures.get(id.index as usize).is_some_and(|original| Arc::ptr_eq(capture, original)) { return Err(failure("lexical capture differs from its original allocation receipt")); }
        Ok(capture.as_ref())
    }
    pub fn lexical_captures(&self) -> impl Iterator<Item = (LexicalCaptureId, &LexicalCapture)> {
        self.lexical_captures.captures.iter().enumerate().map(|(index, entry)| (LexicalCaptureId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } }, entry.value.as_ref()))
    }
    pub fn lexical_capture_for_slot(&self, target: IrFunctionId, slot: u32) -> Result<Option<(LexicalCaptureId, &LexicalCapture)>, IrVerifyError> {
        let Ok(index) = self.lexical_captures.capture_slots.binary_search_by_key(&(target.raw(), slot), |entry| (entry.0, entry.1)) else { return Ok(None); };
        let index = self.lexical_captures.capture_slots[index].2;
        let entry = self.lexical_captures.captures.get(index).ok_or_else(|| failure("lexical capture slot index is stale"))?;
        let id = LexicalCaptureId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } };
        let capture = self.lexical_capture(id)?;
        if capture.target != target || capture.slot != slot { return Err(failure("lexical capture slot index changes its allocation")); }
        Ok(Some((id, capture)))
    }
    pub fn lexical_capture_source(&self, id: LexicalCaptureSourceId) -> Result<&LexicalCaptureSource, IrVerifyError> {
        let source = owned(self.root, &self.lexical_captures.sources, id.index, id.proof)?;
        if !self.lexical_captures.originals.get(id.index as usize).is_some_and(|original| Arc::ptr_eq(source, original)) { return Err(failure("lexical capture read differs from its original source receipt")); }
        Ok(source.as_ref())
    }
    pub fn lexical_capture_sources(&self) -> impl Iterator<Item = (LexicalCaptureSourceId, &LexicalCaptureSource)> {
        self.lexical_captures.sources.iter().enumerate().map(|(index, entry)| (LexicalCaptureSourceId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } }, entry.value.as_ref()))
    }
    pub fn lexical_capture_source_at(&self, instruction: u32) -> Result<Option<LexicalCaptureSourceId>, IrVerifyError> {
        let Some(index) = self.lexical_captures.instructions.binary_search_by_key(&instruction, |entry| entry.0).ok().map(|index| self.lexical_captures.instructions[index].1) else { return Ok(None); };
        let entry = self.lexical_captures.sources.get(index).ok_or_else(|| failure("lexical capture instruction index is stale"))?;
        let id = LexicalCaptureSourceId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } };
        self.lexical_capture_source(id)?; Ok(Some(id))
    }
    pub(super) fn verify_lexical_capture_evidence(&self, pools: &SemanticPools, owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        if self.lexical_captures.captures.len() != self.lexical_captures.original_captures.len() || self.lexical_captures.sources.len() != self.lexical_captures.originals.len() { return Err(failure("lexical capture original ledger is incomplete")); }
        let mut slots = Vec::new();
        for (id, _) in self.lexical_captures() {
            let capture = self.lexical_capture(id)?;
            if capture.definition_owner == Some(capture.declaration) || pools.to_type(capture.ty)? != capture.original_type { return Err(failure("lexical capture changes its original binding type or definition owner")); }
            slots.push((capture.target.raw(), capture.slot, id.index as usize));
        }
        slots.sort_unstable();
        if slots != self.lexical_captures.capture_slots || slots.windows(2).any(|pair| (pair[0].0, pair[0].1) == (pair[1].0, pair[1].1)) { return Err(failure("lexical capture slot index is ambiguous or stale")); }
        let mut instructions = Vec::new();
        for (id, _) in self.lexical_capture_sources() {
            let source = self.lexical_capture_source(id)?;
            let capture = self.lexical_capture(source.capture)?;
            if source.owner != InstructionOwner::Function(capture.target) || source.declaration != capture.declaration || source.binding != capture.binding
                || source.slot != capture.slot || source.ty != capture.ty || source.original_type != capture.original_type
                || pools.to_type(source.ty)? != source.original_type || source.payload.as_ref() != [source.slot]
                || owners.get(source.instruction as usize) != Some(&Some(source.owner))
                || self.registered_instruction_origin(source.instruction, false) != Some((OperationSourceOrigin::Expression(source.origin), source.owner)) {
                return Err(failure("lexical capture read changes its original allocation, owner or type"));
            }
            instructions.push((source.instruction, id.index as usize));
        }
        instructions.sort_unstable_by_key(|entry| entry.0);
        if instructions != self.lexical_captures.instructions || instructions.windows(2).any(|pair| pair[0].0 == pair[1].0) { return Err(failure("lexical capture instruction index is ambiguous or stale")); }
        Ok(())
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_lexical_capture_mut(&mut self, id: LexicalCaptureId) -> Result<&mut LexicalCapture, IrVerifyError> { self.lexical_capture(id)?; Ok(Arc::make_mut(&mut self.lexical_captures.captures[id.index as usize].value)) }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_lexical_capture_source_mut(&mut self, id: LexicalCaptureSourceId) -> Result<&mut LexicalCaptureSource, IrVerifyError> { self.lexical_capture_source(id)?; Ok(Arc::make_mut(&mut self.lexical_captures.sources[id.index as usize].value)) }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_clear_lexical_captures(&mut self) { self.lexical_captures.captures.clear(); self.lexical_captures.capture_slots.clear(); }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_clear_lexical_capture_sources(&mut self) { self.lexical_captures.sources.clear(); self.lexical_captures.instructions.clear(); }
}

impl GenericEvidenceBuilder {
    pub fn add_lexical_capture(&mut self, capture: LexicalCapture) -> Result<LexicalCaptureId, IrVerifyError> {
        if self.store.lexical_captures.captures.len() >= 2_000_000 { return Err(failure("lexical capture evidence exceeds its work limit")); }
        if self.store.lexical_capture_for_slot(capture.target, capture.slot)?.is_some() { return Err(failure("lexical capture allocation is duplicated")); }
        let index = self.store.lexical_captures.captures.len() as u32;
        let serial = self.next_serial; self.next_serial = serial.checked_add(1).ok_or_else(|| failure("lexical capture serial overflow"))?;
        let capture = Arc::new(capture);
        let position = self.store.lexical_captures.capture_slots.binary_search_by_key(&(capture.target.raw(), capture.slot), |entry| (entry.0, entry.1)).unwrap_err();
        self.store.lexical_captures.capture_slots.insert(position, (capture.target.raw(), capture.slot, index as usize));
        self.store.lexical_captures.original_captures.push(Arc::clone(&capture)); self.store.lexical_captures.captures.push(Entry { serial, value: capture });
        Ok(LexicalCaptureId { index, proof: OwnerProof { root: self.store.root, serial } })
    }
    pub fn lexical_capture_for_slot(&self, target: IrFunctionId, slot: u32) -> Result<Option<(LexicalCaptureId, &LexicalCapture)>, IrVerifyError> { self.store.lexical_capture_for_slot(target, slot) }
    pub fn add_lexical_capture_source(&mut self, source: LexicalCaptureSource) -> Result<LexicalCaptureSourceId, IrVerifyError> {
        if self.store.lexical_captures.sources.len() >= 2_000_000 { return Err(failure("lexical capture read evidence exceeds its work limit")); }
        self.store.lexical_capture(source.capture)?;
        let index = self.store.lexical_captures.sources.len() as u32;
        let serial = self.next_serial; self.next_serial = serial.checked_add(1).ok_or_else(|| failure("lexical capture read serial overflow"))?;
        let source = Arc::new(source);
        self.store.lexical_captures.originals.push(Arc::clone(&source)); self.store.lexical_captures.sources.push(Entry { serial, value: source });
        Ok(LexicalCaptureSourceId { index, proof: OwnerProof { root: self.store.root, serial } })
    }
}
