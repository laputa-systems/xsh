use super::*;
use crate::source::Span;

// Callback parameters are distinct source ports even when their types
// coincide. The sealed callback creation owns the slots and each original
// read, independently of the surrounding function or driver environment.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct OriginalStageBlockCallback {
    pub stage: crate::sema::check::StageIdentity,
    pub block: crate::syntax::arena::BlockId,
    pub parameters: Box<[Option<(Name, Span)>]>,
    pub slots: Box<[u32]>,
    pub types: Box<[GroundTypeId]>,
    pub initial: Option<u32>,
    pub value: u32,
    pub reads: Box<[(u32, crate::sema::check::ExpressionIdentity, u32)]>,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct OriginalPreparedStage {
    pub origin: crate::sema::check::StageIdentity,
    pub authority: PreparedOperationAuthority,
    pub input: GroundTypeId,
    pub result: GroundTypeId,
    pub effects: PreparedOperationEffects,
    pub arguments: Box<[crate::sema::check::SolvedArgumentSource]>,
    pub supplied_slots: Box<[u32]>,
    pub default_slots: Box<[u32]>,
    pub stage: u32,
    pub tag: super::super::full::FullStageTag,
    pub payload: Box<[u32]>,
    pub callback: Option<OriginalStageBlockCallback>,
}

/// The checked pipeline and its emitted stage sequence form one creation.
/// Keeping the original allocation separate prevents coordinated edits to a
/// result receipt and its stage contract from authorizing another pipeline.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct OriginalStagePipeline {
    pub origin: crate::sema::check::ExpressionIdentity,
    pub instruction: u32,
    pub owner: InstructionOwner,
    pub input: u32,
    pub input_source: u32,
    pub input_wrappers: Box<[ValueInitializerWrapper]>,
    pub input_type: GroundTypeId,
    pub result: GroundTypeId,
    pub instruction_payload: Box<[u32]>,
    pub block_flags: u8,
    pub block_payload: Box<[u32]>,
    pub stages: Box<[OriginalPreparedStage]>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct StageEvidence {
    program: Option<u64>,
    receipts: Vec<Entry<Arc<OriginalStagePipeline>>>,
    originals: Vec<Arc<OriginalStagePipeline>>,
    instructions: Vec<(u32, usize)>,
    callback_reads: Vec<(u32, usize, usize, usize)>,
}

impl StageEvidence {
    pub(super) fn checkpoint(&self) -> usize { self.receipts.len() }
    pub(super) fn validate_checkpoint(&self, count: usize, serial: u64) -> Result<(), IrVerifyError> {
        if count > self.receipts.len() || self.receipts.get(count.wrapping_sub(1)).is_some_and(|entry| entry.serial >= serial) { return Err(failure("stage checkpoint references retired entries")); }
        Ok(())
    }
    pub(super) fn rewind_validated(&mut self, count: usize) { self.receipts.truncate(count); self.originals.truncate(count); self.instructions.clear(); self.callback_reads.clear(); }
    pub(super) fn finish_indexes(&mut self) {
        self.instructions = self.receipts.iter().enumerate().map(|(index, entry)| (entry.value.instruction, index)).collect();
        self.instructions.sort_unstable_by_key(|entry| entry.0);
        self.callback_reads.clear();
        for (pipeline, receipt) in self.receipts.iter().enumerate() {
            for (stage, proof) in receipt.value.stages.iter().enumerate() {
                if let Some(callback) = &proof.callback {
                    for (read, &(instruction, _, _)) in callback.reads.iter().enumerate() { self.callback_reads.push((instruction, pipeline, stage, read)); }
                }
            }
        }
        self.callback_reads.sort_unstable_by_key(|entry| entry.0);
    }
    pub(super) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        self.receipts.capacity() * size_of::<Entry<Arc<OriginalStagePipeline>>>() + self.originals.capacity() * size_of::<Arc<OriginalStagePipeline>>() + self.instructions.capacity() * size_of::<(u32, usize)>() + self.callback_reads.capacity() * size_of::<(u32, usize, usize, usize)>()
            + self.receipts.iter().map(|entry| size_of::<OriginalStagePipeline>() + 2 * size_of::<usize>() + (entry.value.instruction_payload.len() + entry.value.block_payload.len()) * size_of::<u32>() + entry.value.input_wrappers.len() * size_of::<ValueInitializerWrapper>() + entry.value.input_wrappers.iter().map(|wrapper| wrapper.payload.len() * size_of::<u32>()).sum::<usize>() + entry.value.stages.iter().map(|stage| size_of::<OriginalPreparedStage>() + stage.authority.retained_bytes() + (stage.effects.inputs.len() * size_of::<(crate::sema::inference::EffectRole, crate::sema::inference::EffectSet)>() + stage.effects.outputs.len() * size_of::<(crate::sema::inference::ProducerRole, crate::sema::inference::EffectSet)>()) + stage.callback.as_ref().map_or(0, |callback| size_of::<OriginalStageBlockCallback>() + callback.parameters.len() * size_of::<Option<(Name, Span)>>() + callback.slots.len() * size_of::<u32>() + callback.types.len() * size_of::<GroundTypeId>() + callback.reads.len() * size_of::<(u32, crate::sema::check::ExpressionIdentity, u32)>()) + stage.arguments.len() * size_of::<crate::sema::check::SolvedArgumentSource>() + (stage.payload.len() + stage.supplied_slots.len() + stage.default_slots.len()) * size_of::<u32>()).sum::<usize>()).sum::<usize>()
    }
    pub(super) fn shrink_to_fit(&mut self) { self.receipts.shrink_to_fit(); self.originals.shrink_to_fit(); self.instructions.shrink_to_fit(); self.callback_reads.shrink_to_fit(); }
    fn receipt(&self, index: usize) -> Result<&OriginalStagePipeline, IrVerifyError> {
        let value = &self.receipts.get(index).ok_or_else(|| failure("stage pipeline is missing"))?.value;
        if !self.originals.get(index).is_some_and(|original| Arc::ptr_eq(value, original)) { return Err(failure("stage pipeline changes its original receipt")); }
        Ok(value)
    }
}

impl GenericEvidenceStore {
    pub(in crate::runtime::eval) fn stage_pipeline_at(&self, instruction: u32) -> Result<Option<&OriginalStagePipeline>, IrVerifyError> {
        if !self.stages.receipts.is_empty() && self.stages.program != Some(self.root) { return Err(failure("stage pipeline belongs to a foreign program")); }
        self.stages.instructions.binary_search_by_key(&instruction, |entry| entry.0).ok().map(|index| self.stages.receipt(self.stages.instructions[index].1)).transpose()
    }
    pub(in crate::runtime::eval) fn stage_block_callback_read_at(&self, instruction: u32) -> Result<Option<(&OriginalStagePipeline, &OriginalStageBlockCallback, u32)>, IrVerifyError> {
        if !self.stages.receipts.is_empty() && self.stages.program != Some(self.root) { return Err(failure("stage block callback belongs to a foreign program")); }
        let Ok(index) = self.stages.callback_reads.binary_search_by_key(&instruction, |entry| entry.0) else { return Ok(None); };
        let (_, pipeline, stage, read) = self.stages.callback_reads[index];
        let pipeline = self.stages.receipt(pipeline)?;
        let callback = pipeline.stages.get(stage).and_then(|stage| stage.callback.as_ref()).ok_or_else(|| failure("stage block callback receipt is missing"))?;
        let &(source, origin, port) = callback.reads.get(read).ok_or_else(|| failure("stage block callback read receipt is missing"))?;
        if source != instruction || self.registered_instruction_origin(source, false) != Some((OperationSourceOrigin::Expression(origin), pipeline.owner)) { return Err(failure("stage block callback read loses its original expression")); }
        Ok(Some((pipeline, callback, port)))
    }
    pub(in crate::runtime::eval) fn stage_pipelines(&self) -> impl Iterator<Item = &OriginalStagePipeline> { self.stages.receipts.iter().map(|entry| entry.value.as_ref()) }
    pub(super) fn verify_stage_evidence(&self, owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        let evidence = &self.stages;
        if !evidence.receipts.is_empty() && evidence.program != Some(self.root) { return Err(failure("stage pipeline belongs to a foreign program")); }
        if evidence.receipts.len() != evidence.originals.len() { return Err(failure("stage pipeline original ledger is incomplete")); }
        let mut expected = Vec::new();
        let mut expected_reads = Vec::new();
        for index in 0..evidence.receipts.len() {
            let source = evidence.receipt(index)?;
            if owners.get(source.instruction as usize) != Some(&Some(source.owner)) || owners.get(source.input as usize) != Some(&Some(source.owner)) || owners.get(source.input_source as usize) != Some(&Some(source.owner)) { return Err(failure("stage pipeline changes its original owner")); }
            if self.registered_instruction_origin(source.instruction, false) != Some((OperationSourceOrigin::Expression(source.origin), source.owner)) { return Err(failure("stage pipeline changes its original expression")); }
            if source.stages.iter().any(|stage| stage.origin.pipeline != source.origin) { return Err(failure("stage pipeline borrows another stage origin")); }
            for (stage_index, stage) in source.stages.iter().enumerate() {
                if let Some(callback) = &stage.callback {
                    if callback.stage != stage.origin || callback.parameters.len() != callback.slots.len() || callback.types.len() != callback.slots.len() || !matches!(callback.slots.len(), 1 | 2) || (callback.slots.len() == 2 && callback.slots[0] == callback.slots[1]) { return Err(failure("stage block callback changes its original source ports")); }
                    for (read, &(instruction, origin, port)) in callback.reads.iter().enumerate() {
                        if port as usize >= callback.slots.len() || owners.get(instruction as usize) != Some(&Some(source.owner)) || self.registered_instruction_origin(instruction, false) != Some((OperationSourceOrigin::Expression(origin), source.owner)) { return Err(failure("stage block callback read changes its original port or owner")); }
                        expected_reads.push((instruction, index, stage_index, read));
                    }
                }
            }
            expected.push((source.instruction, index));
        }
        expected.sort_unstable_by_key(|entry| entry.0);
        if expected.windows(2).any(|pair| pair[0].0 == pair[1].0) || expected != evidence.instructions { return Err(failure("stage pipeline instruction index is incomplete or ambiguous")); }
        expected_reads.sort_unstable_by_key(|entry| entry.0);
        if expected_reads.windows(2).any(|pair| pair[0].0 == pair[1].0) || expected_reads != evidence.callback_reads { return Err(failure("stage block callback read index is incomplete or ambiguous")); }
        Ok(())
    }
}

impl GenericEvidenceBuilder {
    pub(in crate::runtime::eval) fn add_original_stage_pipeline(&mut self, source: OriginalStagePipeline) -> Result<(), IrVerifyError> {
        if self.store.stages.receipts.len() >= 2_000_000 { return Err(failure("stage pipeline limit exceeded")); }
        let serial = self.next_serial;
        self.next_serial = serial.checked_add(1).ok_or_else(|| failure("stage pipeline serial overflow"))?;
        let source = Arc::new(source);
        self.store.stages.program = Some(self.store.root);
        self.store.stages.originals.push(Arc::clone(&source));
        self.store.stages.receipts.push(Entry { serial, value: source });
        Ok(())
    }
}

#[cfg(test)]
impl GenericEvidenceStore {
    pub(in crate::runtime::eval) fn test_remove_stage_pipelines(&mut self) { self.stages.receipts.clear(); self.stages.instructions.clear(); }
    pub(in crate::runtime::eval) fn test_replace_stage_pipelines(&mut self, other: &Self) { self.stages = other.stages.clone(); }
    pub(in crate::runtime::eval) fn test_stage_pipeline_mut(&mut self, instruction: u32) -> Result<&mut OriginalStagePipeline, IrVerifyError> {
        let index = self.stages.instructions.binary_search_by_key(&instruction, |entry| entry.0).map_err(|_| failure("stage pipeline is missing"))?;
        let index = self.stages.instructions[index].1;
        Ok(Arc::make_mut(&mut self.stages.receipts[index].value))
    }
}
