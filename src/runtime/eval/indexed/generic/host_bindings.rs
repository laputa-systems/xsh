use super::*;
use crate::runtime::eval::lower::host_bindings::HostBinding;
use crate::sema::check::ExpressionIdentity;
use crate::sema::inference::ScopedRoot;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) struct HostBindingSourceId { index: u32, proof: OwnerProof }

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) struct HostBindingCaptureId { index: u32, proof: OwnerProof }

/// A capture is selected from the original host seed during slot allocation.
/// Its declaration and header position identify the receiving environment.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct HostBindingCapture {
    pub binding: HostBinding,
    pub declaration: crate::sema::check::DeclarationIdentity,
    pub target: IrFunctionId,
    pub header_index: u32,
    pub slot: u32,
    pub ty: GroundTypeId,
    pub name: u32,
}

/// A read retains its original entry scope or receiving capture allocation.
/// Equal names and types do not authorize another read or a shadowing binding.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct HostBindingSource {
    pub binding: HostBinding,
    pub origin: ExpressionIdentity,
    pub instruction: u32,
    pub owner: InstructionOwner,
    pub slot: u32,
    pub ty: GroundTypeId,
    pub source_type: ScopedRoot,
    pub scope_start: u32,
    pub scope_end: u32,
    pub slot_name: u32,
    pub slot_flags: u8,
    pub payload: Box<[u32]>,
    pub capture: Option<HostBindingCaptureId>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct HostBindingEvidence {
    sources: Vec<Entry<Arc<HostBindingSource>>>,
    originals: Vec<Arc<HostBindingSource>>,
    instructions: Vec<(u32, usize)>,
    captures: Vec<Entry<Arc<HostBindingCapture>>>,
    original_captures: Vec<Arc<HostBindingCapture>>,
    capture_slots: Vec<(u32, u32, usize)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct HostBindingCheckpoint { sources: usize, captures: usize }

impl HostBindingEvidence {
    pub(super) fn checkpoint(&self) -> HostBindingCheckpoint { HostBindingCheckpoint { sources: self.sources.len(), captures: self.captures.len() } }
    pub(super) fn validate_checkpoint(&self, checkpoint: HostBindingCheckpoint, serial_limit: u64) -> Result<(), IrVerifyError> {
        if checkpoint.sources > self.sources.len() || self.sources.get(checkpoint.sources.wrapping_sub(1)).is_some_and(|entry| entry.serial >= serial_limit)
            || checkpoint.captures > self.captures.len() || self.captures.get(checkpoint.captures.wrapping_sub(1)).is_some_and(|entry| entry.serial >= serial_limit) {
            return Err(failure("host binding checkpoint references retired or replaced entries"));
        }
        Ok(())
    }
    pub(super) fn rewind_validated(&mut self, checkpoint: HostBindingCheckpoint) {
        self.sources.truncate(checkpoint.sources); self.originals.truncate(checkpoint.sources); self.instructions.clear();
        self.captures.truncate(checkpoint.captures); self.original_captures.truncate(checkpoint.captures);
        self.capture_slots = self.captures.iter().enumerate().map(|(index, entry)| (entry.value.target.raw(), entry.value.slot, index)).collect();
        self.capture_slots.sort_unstable();
    }
    pub(super) fn finish(&mut self) {
        self.instructions = self.sources.iter().enumerate().map(|(index, entry)| (entry.value.instruction, index)).collect();
        self.instructions.sort_unstable_by_key(|entry| entry.0);
    }
    pub(super) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        self.sources.capacity() * size_of::<Entry<Arc<HostBindingSource>>>() + self.originals.capacity() * size_of::<Arc<HostBindingSource>>()
            + self.sources.len() * (size_of::<HostBindingSource>() + 2 * size_of::<usize>())
            + self.sources.iter().map(|entry| entry.value.payload.len() * size_of::<u32>()).sum::<usize>()
            + self.instructions.capacity() * size_of::<(u32, usize)>()
            + self.captures.capacity() * size_of::<Entry<Arc<HostBindingCapture>>>() + self.original_captures.capacity() * size_of::<Arc<HostBindingCapture>>()
            + self.captures.len() * (size_of::<HostBindingCapture>() + 2 * size_of::<usize>())
            + self.capture_slots.capacity() * size_of::<(u32, u32, usize)>()
    }
    pub(super) fn shrink_to_fit(&mut self) { self.sources.shrink_to_fit(); self.originals.shrink_to_fit(); self.instructions.shrink_to_fit(); self.captures.shrink_to_fit(); self.original_captures.shrink_to_fit(); self.capture_slots.shrink_to_fit(); }
}

impl GenericEvidenceStore {
    pub fn has_host_bindings(&self) -> bool { !self.host_bindings.sources.is_empty() || !self.host_bindings.originals.is_empty() || !self.host_bindings.captures.is_empty() || !self.host_bindings.original_captures.is_empty() }
    pub fn host_binding_capture(&self, id: HostBindingCaptureId) -> Result<&HostBindingCapture, IrVerifyError> {
        let capture = owned(self.root, &self.host_bindings.captures, id.index, id.proof)?;
        if !self.host_bindings.original_captures.get(id.index as usize).is_some_and(|original| Arc::ptr_eq(capture, original)) { return Err(failure("host capture differs from its original allocation receipt")); }
        Ok(capture.as_ref())
    }
    pub fn host_binding_captures(&self) -> impl Iterator<Item = (HostBindingCaptureId, &HostBindingCapture)> {
        self.host_bindings.captures.iter().enumerate().map(|(index, entry)| (HostBindingCaptureId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } }, entry.value.as_ref()))
    }
    pub fn host_binding_capture_for_slot(&self, target: IrFunctionId, slot: u32) -> Result<Option<(HostBindingCaptureId, &HostBindingCapture)>, IrVerifyError> {
        let Ok(index) = self.host_bindings.capture_slots.binary_search_by_key(&(target.raw(), slot), |entry| (entry.0, entry.1)) else { return Ok(None); };
        let index = self.host_bindings.capture_slots[index].2;
        let entry = self.host_bindings.captures.get(index).ok_or_else(|| failure("host capture slot index is stale"))?;
        let id = HostBindingCaptureId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } };
        let capture = self.host_binding_capture(id)?;
        if capture.target != target || capture.slot != slot { return Err(failure("host capture slot index changes its allocation")); }
        Ok(Some((id, capture)))
    }
    pub fn host_binding_source(&self, id: HostBindingSourceId) -> Result<&HostBindingSource, IrVerifyError> {
        let source = owned(self.root, &self.host_bindings.sources, id.index, id.proof)?;
        if !self.host_bindings.originals.get(id.index as usize).is_some_and(|original| Arc::ptr_eq(source, original)) { return Err(failure("host binding differs from its original hydration receipt")); }
        Ok(source.as_ref())
    }
    pub fn host_binding_sources(&self) -> impl Iterator<Item = (HostBindingSourceId, &HostBindingSource)> {
        self.host_bindings.sources.iter().enumerate().map(|(index, entry)| (HostBindingSourceId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } }, entry.value.as_ref()))
    }
    pub fn host_binding_source_at(&self, instruction: u32) -> Result<Option<HostBindingSourceId>, IrVerifyError> {
        let Some(index) = self.host_bindings.instructions.binary_search_by_key(&instruction, |entry| entry.0).ok().map(|index| self.host_bindings.instructions[index].1) else { return Ok(None); };
        let entry = self.host_bindings.sources.get(index).ok_or_else(|| failure("host binding instruction index is stale"))?;
        let id = HostBindingSourceId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } };
        self.host_binding_source(id)?; Ok(Some(id))
    }
    pub(super) fn verify_host_binding_evidence(&self, pools: &SemanticPools, owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        if self.host_bindings.sources.len() != self.host_bindings.originals.len() { return Err(failure("host binding original ledger is incomplete")); }
        if self.host_bindings.captures.len() != self.host_bindings.original_captures.len() { return Err(failure("host capture original ledger is incomplete")); }
        let mut capture_slots = Vec::new();
        for (id, _) in self.host_binding_captures() {
            let capture = self.host_binding_capture(id)?;
            if pools.to_type(capture.ty)? != capture.binding.ty() { return Err(failure("host capture changes its original type")); }
            self.host_binding_capture_for_slot(capture.target, capture.slot)?;
            capture_slots.push((capture.target.raw(), capture.slot, id.index as usize));
        }
        capture_slots.sort_unstable();
        if capture_slots.windows(2).any(|pair| (pair[0].0, pair[0].1) == (pair[1].0, pair[1].1)) || capture_slots != self.host_bindings.capture_slots { return Err(failure("host capture slot index is ambiguous or stale")); }
        let mut instructions = Vec::new();
        for (id, _) in self.host_binding_sources() {
            let source = self.host_binding_source(id)?;
            let valid_hydration = match (source.owner, source.capture) {
                (InstructionOwner::Driver(step), None) => source.scope_start <= step && step < source.scope_end,
                (InstructionOwner::Function(target), Some(id)) => {
                    let capture = self.host_binding_capture(id)?;
                    capture.target == target && capture.slot == source.slot && capture.binding == source.binding && capture.ty == source.ty
                        && source.scope_start == 0 && source.scope_end == 0 && source.slot_name == capture.name && source.slot_flags == 0
                }
                _ => false,
            };
            if owners.get(source.instruction as usize) != Some(&Some(source.owner))
                || self.registered_instruction_origin(source.instruction, false) != Some((OperationSourceOrigin::Expression(source.origin), source.owner))
                || !valid_hydration
                || source.payload.as_ref() != [source.slot] || pools.to_type(source.ty)? != source.binding.ty() {
                return Err(failure("host binding changes its original read, type or hydration scope"));
            }
            instructions.push((source.instruction, id.index as usize));
        }
        instructions.sort_unstable_by_key(|entry| entry.0);
        if instructions.windows(2).any(|pair| pair[0].0 == pair[1].0) || instructions != self.host_bindings.instructions { return Err(failure("host binding instruction index is ambiguous or stale")); }
        Ok(())
    }

    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_host_binding_source_mut(&mut self, id: HostBindingSourceId) -> Result<&mut HostBindingSource, IrVerifyError> {
        owned(self.root, &self.host_bindings.sources, id.index, id.proof)?;
        Ok(Arc::make_mut(&mut self.host_bindings.sources[id.index as usize].value))
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_clear_host_bindings(&mut self) { self.host_bindings.sources.clear(); self.host_bindings.instructions.clear(); }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_host_binding_capture_mut(&mut self, id: HostBindingCaptureId) -> Result<&mut HostBindingCapture, IrVerifyError> {
        self.host_binding_capture(id)?;
        Ok(Arc::make_mut(&mut self.host_bindings.captures[id.index as usize].value))
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_clear_host_binding_captures(&mut self) { self.host_bindings.captures.clear(); self.host_bindings.capture_slots.clear(); }
}

impl GenericEvidenceBuilder {
    pub fn add_host_binding_capture(&mut self, capture: HostBindingCapture) -> Result<HostBindingCaptureId, IrVerifyError> {
        if self.store.host_bindings.captures.len() >= 2_000_000 { return Err(failure("host capture evidence exceeds its work limit")); }
        if self.store.host_binding_capture_for_slot(capture.target, capture.slot)?.is_some() { return Err(failure("host capture allocation is duplicated")); }
        let index = self.store.host_bindings.captures.len() as u32;
        let serial = self.next_serial; self.next_serial = serial.checked_add(1).ok_or_else(|| failure("host capture serial overflow"))?;
        let capture = Arc::new(capture);
        let position = self.store.host_bindings.capture_slots.binary_search_by_key(&(capture.target.raw(), capture.slot), |entry| (entry.0, entry.1)).unwrap_err();
        self.store.host_bindings.capture_slots.insert(position, (capture.target.raw(), capture.slot, index as usize));
        self.store.host_bindings.original_captures.push(Arc::clone(&capture)); self.store.host_bindings.captures.push(Entry { serial, value: capture });
        Ok(HostBindingCaptureId { index, proof: OwnerProof { root: self.store.root, serial } })
    }
    pub fn host_binding_capture_for_slot(&self, target: IrFunctionId, slot: u32) -> Result<Option<(HostBindingCaptureId, &HostBindingCapture)>, IrVerifyError> { self.store.host_binding_capture_for_slot(target, slot) }
    pub fn add_host_binding_source(&mut self, source: HostBindingSource) -> Result<HostBindingSourceId, IrVerifyError> {
        if self.store.host_bindings.sources.len() >= 2_000_000 { return Err(failure("host binding evidence exceeds its work limit")); }
        let index = self.store.host_bindings.sources.len() as u32;
        let serial = self.next_serial; self.next_serial = serial.checked_add(1).ok_or_else(|| failure("host binding serial overflow"))?;
        let source = Arc::new(source);
        self.store.host_bindings.originals.push(Arc::clone(&source)); self.store.host_bindings.sources.push(Entry { serial, value: source });
        Ok(HostBindingSourceId { index, proof: OwnerProof { root: self.store.root, serial } })
    }
}
