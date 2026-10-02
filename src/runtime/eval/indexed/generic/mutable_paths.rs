use super::*;
use super::super::full::FullTag;
use crate::sema::check::{BindingIdentity, ExpressionIdentity, StatementIdentity};
use crate::sema::inference::ScopedRoot;

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) enum MutablePathStep {
    Field { name: Name, input: GroundTypeId, output: GroundTypeId, input_root: ScopedRoot, output_root: ScopedRoot },
    Index { instruction: u32, source: ExpressionIdentity, checked: GroundTypeId, checked_root: ScopedRoot, input: GroundTypeId, output: GroundTypeId, input_root: ScopedRoot, output_root: ScopedRoot, tag: FullTag, payload: Box<[u32]> },
}

pub(in crate::runtime::eval) type MutablePathCompound = super::mutable_bindings::MutableCompoundAssignment;

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) enum MutablePathEncoding {
    Block { block: u32, owner: u32, flags: u8, payload: Box<[u32]> },
    Field { name: Name, integer: bool },
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct MutablePathReceipt {
    pub binding: BindingIdentity,
    pub statement: StatementIdentity,
    pub target: crate::syntax::arena::AssignTargetId,
    pub instruction: u32,
    pub owner: InstructionOwner,
    pub slot: u32,
    pub payload: Box<[u32]>,
    pub encoding: MutablePathEncoding,
    pub steps: Box<[MutablePathStep]>,
    pub binding_type: GroundTypeId,
    pub binding_root: ScopedRoot,
    pub selected_type: GroundTypeId,
    pub selected_root: ScopedRoot,
    pub value: u32,
    pub value_tag: FullTag,
    pub value_payload: Box<[u32]>,
    pub value_source: ExpressionIdentity,
    pub value_type: GroundTypeId,
    pub value_root: ScopedRoot,
    pub compound: Option<MutablePathCompound>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct MutablePathEvidence {
    entries: Vec<Entry<Arc<MutablePathReceipt>>>,
    originals: Vec<Arc<MutablePathReceipt>>,
    instructions: Vec<(u32, usize)>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct MutablePathCheckpoint { entries: usize }
impl MutablePathEvidence {
    pub(super) fn checkpoint(&self) -> MutablePathCheckpoint { MutablePathCheckpoint { entries: self.entries.len() } }
    pub(super) fn validate_checkpoint(&self, checkpoint: MutablePathCheckpoint, serial_limit: u64) -> Result<(), IrVerifyError> {
        if checkpoint.entries > self.entries.len() || self.entries.get(checkpoint.entries.wrapping_sub(1)).is_some_and(|entry| entry.serial >= serial_limit) { return Err(failure("mutable path checkpoint references retired or replacement entries")); }
        Ok(())
    }
    pub(super) fn rewind_validated(&mut self, checkpoint: MutablePathCheckpoint) { self.entries.truncate(checkpoint.entries); self.originals.truncate(checkpoint.entries); self.instructions.clear(); }
    pub(super) fn finish(&mut self) { self.instructions = self.entries.iter().enumerate().map(|(index, entry)| (entry.value.instruction, index)).collect(); self.instructions.sort_unstable_by_key(|entry| entry.0); }
    pub(super) fn shrink_to_fit(&mut self) { self.entries.shrink_to_fit(); self.originals.shrink_to_fit(); self.instructions.shrink_to_fit(); }
    pub(super) fn retained_bytes(&self) -> usize {
        self.entries.capacity() * std::mem::size_of::<Entry<Arc<MutablePathReceipt>>>() + self.originals.capacity() * std::mem::size_of::<Arc<MutablePathReceipt>>() + self.instructions.capacity() * std::mem::size_of::<(u32, usize)>()
            + self.entries.iter().map(|entry| std::mem::size_of::<MutablePathReceipt>() + 2 * std::mem::size_of::<usize>() + (entry.value.payload.len() + match &entry.value.encoding { MutablePathEncoding::Block { payload, .. } => payload.len(), MutablePathEncoding::Field { .. } => 0 } + entry.value.value_payload.len()) * std::mem::size_of::<u32>() + entry.value.steps.len() * std::mem::size_of::<MutablePathStep>() + entry.value.steps.iter().map(|step| match step { MutablePathStep::Index { payload, .. } => payload.len() * std::mem::size_of::<u32>(), _ => 0 }).sum::<usize>()).sum::<usize>()
    }
}
impl GenericEvidenceStore {
    pub fn has_mutable_paths(&self) -> bool { !self.mutable_paths.entries.is_empty() || !self.mutable_paths.originals.is_empty() }
    pub fn mutable_paths(&self) -> impl Iterator<Item = &MutablePathReceipt> { self.mutable_paths.entries.iter().map(|entry| entry.value.as_ref()) }
    pub fn mutable_path_at(&self, instruction: u32) -> Result<Option<&MutablePathReceipt>, IrVerifyError> {
        let Some(index) = self.mutable_paths.instructions.binary_search_by_key(&instruction, |entry| entry.0).ok().map(|position| self.mutable_paths.instructions[position].1) else { return Ok(None); };
        let entry = self.mutable_paths.entries.get(index).ok_or_else(|| failure("mutable path index is stale"))?;
        if !self.mutable_paths.originals.get(index).is_some_and(|original| Arc::ptr_eq(original, &entry.value)) { return Err(failure("mutable path differs from its original receipt")); }
        Ok(Some(entry.value.as_ref()))
    }
    pub(super) fn verify_mutable_path_evidence(&self, pools: &SemanticPools, owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        if self.mutable_paths.entries.len() != self.mutable_paths.originals.len() { return Err(failure("mutable path original receipt ledger is incomplete")); }
        let mut expected = Vec::new();
        for (index, receipt) in self.mutable_paths().enumerate() {
            self.mutable_path_at(receipt.instruction)?;
            if owners.get(receipt.instruction as usize) != Some(&Some(receipt.owner))
                || self.registered_instruction_origin(receipt.instruction, false) != Some((OperationSourceOrigin::Statement(receipt.statement), receipt.owner))
                || receipt.binding.source != receipt.statement.source || receipt.binding.namespace != receipt.statement.namespace
                || receipt.value_source.source != receipt.statement.source || receipt.value_source.namespace != receipt.statement.namespace
                || self.registered_instruction_origin(receipt.value, false) != Some((OperationSourceOrigin::Expression(receipt.value_source), receipt.owner))
                || owners.get(receipt.value as usize) != Some(&Some(receipt.owner)) || receipt.steps.is_empty() || receipt.steps.len() > 128 { return Err(failure("mutable path changes its original source or owner")); }
            for ty in [receipt.binding_type, receipt.selected_type, receipt.value_type] { Self::verify_type(pools, ty)?; }
            if let Some(compound) = &receipt.compound { for ty in [compound.left, compound.right, compound.result] { Self::verify_type(pools, ty)?; } }
            for step in receipt.steps.iter() {
                match step {
                    MutablePathStep::Field { input, output, .. } => { Self::verify_type(pools, *input)?; Self::verify_type(pools, *output)?; }
                    MutablePathStep::Index { instruction, source, checked, input, output, .. } => {
                        if owners.get(*instruction as usize) != Some(&Some(receipt.owner)) || self.registered_instruction_origin(*instruction, false) != Some((OperationSourceOrigin::Expression(*source), receipt.owner)) { return Err(failure("mutable selector changes its original source or owner")); }
                        for ty in [checked, input, output] { Self::verify_type(pools, *ty)?; }
                    }
                }
            }
            expected.push((receipt.instruction, index));
        }
        expected.sort_unstable_by_key(|entry| entry.0);
        if expected.windows(2).any(|pair| pair[0].0 == pair[1].0) || expected != self.mutable_paths.instructions { return Err(failure("mutable path index is incomplete or ambiguous")); }
        Ok(())
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_mutable_path_mut(&mut self, instruction: u32) -> Result<&mut MutablePathReceipt, IrVerifyError> {
        self.mutable_path_at(instruction)?.ok_or_else(|| failure("mutable path receipt is missing"))?;
        let position = self.mutable_paths.instructions.binary_search_by_key(&instruction, |entry| entry.0).unwrap();
        let index = self.mutable_paths.instructions[position].1;
        Ok(Arc::make_mut(&mut self.mutable_paths.entries[index].value))
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_remove_mutable_path(&mut self, instruction: u32) {
        if let Some(index) = self.mutable_paths.entries.iter().position(|entry| entry.value.instruction == instruction) { self.mutable_paths.entries.remove(index); }
        self.mutable_paths.finish();
    }
}
impl GenericEvidenceBuilder {
    pub fn add_mutable_path(&mut self, receipt: MutablePathReceipt) -> Result<(), IrVerifyError> {
        if self.store.mutable_paths.entries.len() >= 2_000_000 { return Err(failure("mutable paths exceed their work limit")); }
        let serial = self.next_serial; self.next_serial = serial.checked_add(1).ok_or_else(|| failure("mutable path serial overflow"))?;
        let value = Arc::new(receipt); self.store.mutable_paths.originals.push(Arc::clone(&value)); self.store.mutable_paths.entries.push(Entry { serial, value });
        Ok(())
    }
}
