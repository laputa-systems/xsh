use super::*;
use crate::runtime::eval::lower::paths::OriginalFormattedPath;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) struct FormattedPathId { index: u32, proof: OwnerProof }

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) enum FormattedPathPart {
    Text(Arc<str>),
    Expression { instruction: u32, source: u32, ty: TypeRef },
}

/// The result type belongs to the authored formatting expression. Its exact
/// emitted operands remain sealed when the frontend graph is disposed.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedFormattedPath {
    pub original: OriginalFormattedPath,
    pub instruction: u32,
    pub owner: InstructionOwner,
    pub caller: Option<SchemeScopeId>,
    pub ty: GroundTypeId,
    pub parts: Box<[FormattedPathPart]>,
    pub payload: Box<[u32]>,
    pub parts_block: u32,
    pub parts_payload: Box<[u32]>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct PathEvidence {
    sources: Vec<Entry<Arc<PreparedFormattedPath>>>,
    originals: Vec<Arc<PreparedFormattedPath>>,
    instructions: Vec<(u32, usize)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PathCheckpoint { sources: usize }

impl PathEvidence {
    pub(super) fn checkpoint(&self) -> PathCheckpoint { PathCheckpoint { sources: self.sources.len() } }
    pub(super) fn validate_checkpoint(&self, checkpoint: PathCheckpoint, serial_limit: u64) -> Result<(), IrVerifyError> {
        if checkpoint.sources > self.sources.len() || self.sources.get(checkpoint.sources.wrapping_sub(1)).is_some_and(|entry| entry.serial >= serial_limit) { return Err(failure("formatted path checkpoint references retired or replaced receipts")); }
        Ok(())
    }
    pub(super) fn rewind_validated(&mut self, checkpoint: PathCheckpoint) { self.sources.truncate(checkpoint.sources); self.originals.truncate(checkpoint.sources); self.instructions.clear(); }
    pub(super) fn finish(&mut self) {
        self.instructions = self.sources.iter().enumerate().map(|(index, entry)| (entry.value.instruction, index)).collect();
        self.instructions.sort_unstable_by_key(|entry| entry.0);
    }
    pub(super) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        self.sources.capacity() * size_of::<Entry<Arc<PreparedFormattedPath>>>() + self.originals.capacity() * size_of::<Arc<PreparedFormattedPath>>()
            + self.instructions.capacity() * size_of::<(u32, usize)>()
            + self.sources.iter().map(|entry| size_of::<PreparedFormattedPath>() + 2 * size_of::<usize>()
                + (entry.value.payload.len() + entry.value.parts_payload.len()) * size_of::<u32>() + entry.value.parts.len() * size_of::<FormattedPathPart>()
                + entry.value.original.parts.len() * size_of::<crate::runtime::eval::lower::paths::OriginalPathPart>()
                + entry.value.parts.iter().map(|part| match part { FormattedPathPart::Text(text) => text.len() + 2 * size_of::<usize>(), _ => 0 }).sum::<usize>()).sum::<usize>()
    }
    pub(super) fn shrink_to_fit(&mut self) { self.sources.shrink_to_fit(); self.originals.shrink_to_fit(); self.instructions.shrink_to_fit(); }
}

impl GenericEvidenceStore {
    pub fn has_formatted_paths(&self) -> bool { !self.paths.sources.is_empty() || !self.paths.originals.is_empty() }
    pub fn formatted_path(&self, id: FormattedPathId) -> Result<&PreparedFormattedPath, IrVerifyError> {
        let source = owned(self.root, &self.paths.sources, id.index, id.proof)?;
        if !self.paths.originals.get(id.index as usize).is_some_and(|original| Arc::ptr_eq(source, original)) { return Err(failure("formatted path differs from its original authored receipt")); }
        Ok(source.as_ref())
    }
    pub fn formatted_paths(&self) -> impl Iterator<Item = (FormattedPathId, &PreparedFormattedPath)> {
        self.paths.sources.iter().enumerate().map(|(index, entry)| (FormattedPathId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } }, entry.value.as_ref()))
    }
    pub fn formatted_path_at(&self, instruction: u32) -> Result<Option<FormattedPathId>, IrVerifyError> {
        let Ok(position) = self.paths.instructions.binary_search_by_key(&instruction, |entry| entry.0) else { return Ok(None); };
        let index = self.paths.instructions[position].1;
        let entry = self.paths.sources.get(index).ok_or_else(|| failure("formatted path instruction index is stale"))?;
        let id = FormattedPathId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } };
        self.formatted_path(id)?;
        Ok(Some(id))
    }
    pub(super) fn verify_path_evidence(&self, pools: &SemanticPools, owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        if self.paths.sources.len() != self.paths.originals.len() { return Err(failure("formatted path original ledger is incomplete")); }
        let mut instructions = Vec::new();
        for (id, _) in self.formatted_paths() {
            let source = self.formatted_path(id)?;
            if let Some(caller) = source.caller {
                if source.owner != InstructionOwner::Function(self.scope(caller)?.owner) { return Err(failure("formatted path changes its original caller scope")); }
            }
            if pools.to_type(source.ty)? != source.original.target.result_type() || owners.get(source.instruction as usize) != Some(&Some(source.owner))
                || self.registered_instruction_origin(source.instruction, false) != Some((OperationSourceOrigin::Expression(source.original.origin), source.owner))
                || source.parts.len() != source.original.parts.len() { return Err(failure("formatted path changes its original source, owner or type")); }
            for (part, original) in source.parts.iter().zip(source.original.parts.iter()) {
                match (part, original) {
                    (FormattedPathPart::Text(text), crate::runtime::eval::lower::paths::OriginalPathPart::Text(actual)) if text == actual => {},
                    (FormattedPathPart::Expression { instruction, source: original_instruction, ty }, crate::runtime::eval::lower::paths::OriginalPathPart::Expression { origin, .. }) => {
                        if let Some(caller) = source.caller { self.verify_reference(pools, self.scope(caller)?, *ty)?; }
                        else if let TypeRef::Ground(ty) = ty { Self::verify_type(pools, *ty)?; }
                        else { return Err(failure("formatted path interpolation has no original caller scope")); }
                        if owners.get(*instruction as usize) != Some(&Some(source.owner)) || owners.get(*original_instruction as usize) != Some(&Some(source.owner))
                            || self.registered_instruction_origin(*original_instruction, false) != Some((OperationSourceOrigin::Expression(*origin), source.owner)) { return Err(failure("formatted path interpolation changes its original source owner")); }
                    }
                    _ => return Err(failure("formatted path changes its original interpolation recipe")),
                }
            }
            instructions.push((source.instruction, id.index as usize));
        }
        instructions.sort_unstable_by_key(|entry| entry.0);
        if instructions.windows(2).any(|pair| pair[0].0 == pair[1].0) || instructions != self.paths.instructions { return Err(failure("formatted path instruction index is ambiguous or stale")); }
        Ok(())
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_formatted_path_mut(&mut self, id: FormattedPathId) -> Result<&mut PreparedFormattedPath, IrVerifyError> {
        self.formatted_path(id)?;
        Ok(Arc::make_mut(&mut self.paths.sources[id.index as usize].value))
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_clear_formatted_paths(&mut self) { self.paths.sources.clear(); self.paths.instructions.clear(); }
}

impl GenericEvidenceBuilder {
    pub fn add_formatted_path(&mut self, source: PreparedFormattedPath) -> Result<FormattedPathId, IrVerifyError> {
        if self.store.paths.sources.len() >= 2_000_000 { return Err(failure("formatted path evidence exceeds its work limit")); }
        let index = self.store.paths.sources.len() as u32;
        let serial = self.next_serial; self.next_serial = serial.checked_add(1).ok_or_else(|| failure("formatted path serial overflow"))?;
        let source = Arc::new(source);
        self.store.paths.originals.push(Arc::clone(&source)); self.store.paths.sources.push(Entry { serial, value: source });
        Ok(FormattedPathId { index, proof: OwnerProof { root: self.store.root, serial } })
    }
}
