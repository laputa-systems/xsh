use super::*;
use crate::sema::check::{ExpressionIdentity, ProducerFlowSource};

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct ContextProducerRoot {
    pub origin: OperationSourceOrigin,
    pub instruction: u32,
    pub ty: GroundTypeId,
    pub original_type: crate::sema::types::Type,
}

/// A context owns the executed input and statement sequence until its result
/// has passed the resource escape check and the previous context is restored.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedContextProducer {
    pub origin: ExpressionIdentity,
    pub instruction: u32,
    pub owner: InstructionOwner,
    pub result: GroundTypeId,
    pub original_result: crate::sema::types::Type,
    pub input: ContextProducerRoot,
    pub tail: Option<ContextProducerRoot>,
    pub payload: Box<[u32]>,
    pub body: super::super::IrBlockId,
    pub statements: Box<[u32]>,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedRunProducer {
    pub source: ProducerFlowSource,
    pub run: crate::syntax::arena::RunFormId,
    pub capture: u32,
    pub continuation: u32,
    pub owner: InstructionOwner,
    pub result: GroundTypeId,
    pub carrier: GroundTypeId,
    pub original_result: crate::sema::types::Type,
    pub original_carrier: crate::sema::types::Type,
    pub authority: PreparedOperationAuthority,
    pub effects: crate::sema::inference::EffectSet,
    pub accept: Option<ContextProducerRoot>,
    pub spawn: Option<PreparedSpawnRunSource>,
    pub arguments: Vec<RunProducerArgument>,
    pub payload: Box<[u32]>,
    pub continuation_payload: Box<[u32]>,
    pub operands: Vec<RunProducerOperand>,
    pub blocks: Vec<(super::super::IrBlockId, Box<[u32]>)>,
    pub texts: Vec<(u32, Arc<str>)>,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedSpawnRunSource {
    pub origin: ExpressionIdentity,
    pub source_type: crate::sema::inference::ScopedRoot,
    pub target: crate::sema::check::RunIdentity,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct RunProducerOperand {
    pub instruction: u32,
    pub tag: super::super::full::FullTag,
    pub payload: Box<[u32]>,
}

/// A command word retains the checked rendering requirement for its authored
/// expression separately from the executable row that reads that expression.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct RunProducerArgument {
    pub word: u32,
    pub mode: crate::sema::check::RunArgumentMode,
    pub requirement: crate::sema::inference::RequirementId,
    pub source: crate::sema::inference::ScopedRoot,
    pub operand: GroundTypeId,
    pub original_operand: crate::sema::types::Type,
    pub root: ContextProducerRoot,
}

#[derive(Clone, Debug)]
enum PreparedProducer { Context(PreparedContextProducer), Run(PreparedRunProducer) }

impl PreparedProducer {
    fn instructions(&self) -> Vec<u32> {
        match self {
            Self::Context(value) => vec![value.instruction],
            Self::Run(value) if value.capture != value.continuation => vec![value.capture, value.continuation],
            Self::Run(value) => vec![value.capture],
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct ContextProducerEvidence {
    entries: Vec<Entry<Arc<PreparedProducer>>>,
    originals: Vec<Arc<PreparedProducer>>,
    instructions: Vec<(u32, usize)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ContextProducerCheckpoint { entries: usize }

impl ContextProducerEvidence {
    pub(super) fn checkpoint(&self) -> ContextProducerCheckpoint { ContextProducerCheckpoint { entries: self.entries.len() } }
    pub(super) fn validate_checkpoint(&self, checkpoint: ContextProducerCheckpoint, serial_limit: u64) -> Result<(), IrVerifyError> {
        if checkpoint.entries > self.entries.len() || self.entries.get(checkpoint.entries.wrapping_sub(1)).is_some_and(|entry| entry.serial >= serial_limit) { return Err(failure("context producer checkpoint references retired or replaced entries")); }
        Ok(())
    }
    pub(super) fn rewind_validated(&mut self, checkpoint: ContextProducerCheckpoint) { self.entries.truncate(checkpoint.entries); self.originals.truncate(checkpoint.entries); self.instructions.clear(); }
    pub(super) fn finish(&mut self) { self.instructions = self.entries.iter().enumerate().flat_map(|(index, entry)| entry.value.instructions().into_iter().map(move |instruction| (instruction, index))).collect(); self.instructions.sort_unstable_by_key(|entry| entry.0); }
    pub(super) fn shrink_to_fit(&mut self) { self.entries.shrink_to_fit(); self.originals.shrink_to_fit(); self.instructions.shrink_to_fit(); }
    pub(super) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        self.entries.capacity() * size_of::<Entry<Arc<PreparedProducer>>>() + self.originals.capacity() * size_of::<Arc<PreparedProducer>>() + self.instructions.capacity() * size_of::<(u32, usize)>()
            + self.entries.iter().map(|entry| size_of::<PreparedProducer>() + 2 * size_of::<usize>() + match entry.value.as_ref() {
                PreparedProducer::Context(value) => (value.payload.len() + value.statements.len()) * size_of::<u32>()
                    + value.original_result.retained_bytes().saturating_sub(size_of::<crate::sema::types::Type>())
                    + value.input.original_type.retained_bytes().saturating_sub(size_of::<crate::sema::types::Type>())
                    + value.tail.as_ref().map_or(0, |tail| tail.original_type.retained_bytes().saturating_sub(size_of::<crate::sema::types::Type>())),
                PreparedProducer::Run(value) => (value.payload.len() + value.continuation_payload.len()) * size_of::<u32>() + value.authority.retained_bytes()
                    + value.arguments.capacity() * size_of::<RunProducerArgument>()
                    + value.arguments.iter().map(|argument| argument.root.original_type.retained_bytes().saturating_sub(size_of::<crate::sema::types::Type>()) + argument.original_operand.retained_bytes().saturating_sub(size_of::<crate::sema::types::Type>())).sum::<usize>()
                    + value.accept.as_ref().map_or(0, |root| root.original_type.retained_bytes().saturating_sub(size_of::<crate::sema::types::Type>()))
                    + value.original_result.retained_bytes().saturating_sub(size_of::<crate::sema::types::Type>())
                    + value.original_carrier.retained_bytes().saturating_sub(size_of::<crate::sema::types::Type>())
                    + value.operands.capacity() * size_of::<RunProducerOperand>() + value.operands.iter().map(|operand| operand.payload.len() * size_of::<u32>()).sum::<usize>()
                    + value.blocks.capacity() * size_of::<(super::super::IrBlockId, Box<[u32]>)>() + value.blocks.iter().map(|(_, payload)| payload.len() * size_of::<u32>()).sum::<usize>()
                    + value.texts.capacity() * size_of::<(u32, Arc<str>)>() + value.texts.iter().map(|(_, text)| text.len() + 2 * size_of::<usize>()).sum::<usize>(),
            }).sum::<usize>()
    }
}

impl GenericEvidenceBuilder {
    pub fn add_context_producer(&mut self, value: PreparedContextProducer) -> Result<(), IrVerifyError> {
        if self.store.context_producers.entries.len() >= 2_000_000 { return Err(failure("context producer receipts exceed their work limit")); }
        let serial = self.next_serial; self.next_serial = serial.checked_add(1).ok_or_else(|| failure("generic evidence serial overflow"))?;
        let value = Arc::new(PreparedProducer::Context(value)); self.store.context_producers.originals.push(value.clone()); self.store.context_producers.entries.push(Entry { serial, value });
        Ok(())
    }
}

impl GenericEvidenceStore {
    pub fn has_context_producers(&self) -> bool { !self.context_producers.entries.is_empty() || !self.context_producers.originals.is_empty() }
    pub fn context_producers(&self) -> impl Iterator<Item = &PreparedContextProducer> { self.context_producers.entries.iter().filter_map(|entry| match entry.value.as_ref() { PreparedProducer::Context(value) => Some(value), _ => None }) }
    pub fn context_producer_at(&self, instruction: u32) -> Result<Option<&PreparedContextProducer>, IrVerifyError> {
        let Some(position) = self.context_producers.instructions.binary_search_by_key(&instruction, |entry| entry.0).ok() else { return Ok(None); };
        let index = self.context_producers.instructions[position].1;
        let entry = self.context_producers.entries.get(index).ok_or_else(|| failure("context producer instruction index is stale"))?;
        if !self.context_producers.originals.get(index).is_some_and(|original| Arc::ptr_eq(original, &entry.value)) { return Err(failure("context producer differs from its original receipt")); }
        Ok(match entry.value.as_ref() { PreparedProducer::Context(value) => Some(value), _ => None })
    }
    pub(super) fn verify_context_producer_evidence(&self, pools: &SemanticPools, owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        if self.context_producers.entries.len() != self.context_producers.originals.len() { return Err(failure("context producer original ledger is incomplete")); }
        let mut expected_index = Vec::new();
        for (index, entry) in self.context_producers.entries.iter().enumerate() {
            if let PreparedProducer::Run(value) = entry.value.as_ref() {
                self.verify_run_producer_evidence(value, pools, owners)?;
                expected_index.extend(entry.value.instructions().into_iter().map(|instruction| (instruction, index)));
                continue;
            }
            let PreparedProducer::Context(original) = entry.value.as_ref() else { unreachable!() };
            let value = self.context_producer_at(original.instruction)?.ok_or_else(|| failure("context producer index is missing"))?;
            if !std::ptr::eq(value, original) || owners.get(value.instruction as usize) != Some(&Some(value.owner))
                || self.registered_instruction_origin(value.instruction, false) != Some((OperationSourceOrigin::Expression(value.origin), value.owner)) { return Err(failure("context producer changes its original source or owner")); }
            if pools.to_type(value.result)? != value.original_result { return Err(failure("context producer result differs from its original checked type")); }
            let crate::sema::types::Type::Result(ok, error) = pools.to_type(value.result)? else { return Err(failure("context producer loses its checked Result carrier")); };
            if *error != crate::sema::types::Type::Error || !ok.can_escape_context_scope() { return Err(failure("context producer changes its checked error or resource boundary")); }
            match &value.tail {
                Some(tail) if pools.to_type(tail.ty)? == *ok => {},
                None if *ok == crate::sema::types::Type::Unit => {},
                _ => return Err(IrVerifyError::new(format!("context producer {:?} changes its original completion relationship: checked result {:?}, tail {:?}", value.origin, value.original_result, value.tail.as_ref().map(|tail| &tail.original_type)))),
            }
            for root in std::iter::once(&value.input).chain(value.tail.iter()) {
                let (source, namespace) = match root.origin {
                    OperationSourceOrigin::Expression(origin) => (origin.source, origin.namespace),
                    OperationSourceOrigin::Statement(origin) => (origin.source, origin.namespace),
                    _ => return Err(failure("context producer child has no original expression or statement")),
                };
                if source != value.origin.source || namespace != value.origin.namespace || owners.get(root.instruction as usize) != Some(&Some(value.owner))
                    || self.registered_instruction_origin(root.instruction, false) != Some((root.origin, value.owner)) { return Err(failure("context producer child changes its original source or owner")); }
                if pools.to_type(root.ty)? != root.original_type { return Err(failure("context producer child differs from its original checked type")); }
            }
            expected_index.push((value.instruction, index));
        }
        expected_index.sort_unstable_by_key(|entry| entry.0);
        if expected_index.windows(2).any(|pair| pair[0].0 == pair[1].0) || expected_index != self.context_producers.instructions { return Err(failure("context producer instruction index is stale or ambiguous")); }
        Ok(())
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_remove_context_producers(&mut self) { self.context_producers.entries.clear(); self.context_producers.instructions.clear(); }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_context_producer_mut(&mut self, instruction: u32) -> Result<&mut PreparedContextProducer, IrVerifyError> {
        self.context_producer_at(instruction)?.ok_or_else(|| failure("context producer receipt is missing"))?;
        let position = self.context_producers.instructions.binary_search_by_key(&instruction, |entry| entry.0).map_err(|_| failure("context producer receipt is missing"))?;
        let PreparedProducer::Context(value) = Arc::make_mut(&mut self.context_producers.entries[self.context_producers.instructions[position].1].value) else { return Err(failure("producer receipt has another protocol")); };
        Ok(value)
    }
}

impl GenericEvidenceBuilder {
    pub fn add_run_producer(&mut self, value: PreparedRunProducer) -> Result<(), IrVerifyError> {
        if self.store.context_producers.entries.len() >= 2_000_000 { return Err(failure("producer receipts exceed their work limit")); }
        let serial = self.next_serial; self.next_serial = serial.checked_add(1).ok_or_else(|| failure("generic evidence serial overflow"))?;
        let value = Arc::new(PreparedProducer::Run(value)); self.store.context_producers.originals.push(value.clone()); self.store.context_producers.entries.push(Entry { serial, value });
        Ok(())
    }
}

impl GenericEvidenceStore {
    pub fn run_producers(&self) -> impl Iterator<Item = &PreparedRunProducer> { self.context_producers.entries.iter().filter_map(|entry| match entry.value.as_ref() { PreparedProducer::Run(value) => Some(value), _ => None }) }
    pub fn run_producer_at(&self, instruction: u32) -> Result<Option<&PreparedRunProducer>, IrVerifyError> {
        let Some(position) = self.context_producers.instructions.binary_search_by_key(&instruction, |entry| entry.0).ok() else {
            if self.context_producers.entries.len() != self.context_producers.originals.len() {
                return Err(failure("run producer original receipt is missing"));
            }
            return Ok(None);
        };
        let index = self.context_producers.instructions[position].1;
        let entry = self.context_producers.entries.get(index).ok_or_else(|| failure("run producer instruction index is stale"))?;
        if !self.context_producers.originals.get(index).is_some_and(|original| Arc::ptr_eq(original, &entry.value)) { return Err(failure("run producer differs from its original receipt")); }
        Ok(match entry.value.as_ref() { PreparedProducer::Run(value) => Some(value), _ => None })
    }
    fn verify_run_producer_evidence(&self, value: &PreparedRunProducer, pools: &SemanticPools, owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        let original = self.run_producer_at(value.capture)?.ok_or_else(|| failure("run producer original receipt is missing"))?;
        if !std::ptr::eq(value, original) || self.run_producer_at(value.continuation)?.map(|receipt| std::ptr::eq(receipt, original)) != Some(true)
            || owners.get(value.capture as usize) != Some(&Some(value.owner)) || owners.get(value.continuation as usize) != Some(&Some(value.owner)) { return Err(failure("run producer changes its original capture, continuation or owner")); }
        let origin = match value.source {
            ProducerFlowSource::Expression(origin) => OperationSourceOrigin::Expression(origin),
            ProducerFlowSource::Statement(origin) => OperationSourceOrigin::Statement(origin),
            _ => return Err(failure("run producer has no original expression or statement parent")),
        };
        if self.registered_instruction_origin(value.continuation, false) != Some((origin, value.owner)) { return Err(failure("run producer changes its original source parent")); }
        self.verify_run_argument_roots(value, pools, owners)?;
        if value.spawn.is_some() { return self.verify_spawn_run_evidence(value, pools); }
        let PreparedOperationAuthority::Language { operation: crate::sema::operation_graph::PreparedLanguageOperation::Run { kind, policy, propagate }, .. } = value.authority else { return Err(failure("run producer changes its original operation authority")); };
        if !matches!(kind, crate::syntax::node::RunKind::CaptureText | crate::syntax::node::RunKind::CaptureBytes | crate::syntax::node::RunKind::CaptureTextRecord | crate::syntax::node::RunKind::CaptureBytesRecord) { return Err(failure("run producer has an unprepared completion protocol")); }
        self.verify_run_acceptance_root(value, policy, propagate, pools, owners)?;
        let result = pools.to_type(value.result)?;
        let carrier = pools.to_type(value.carrier)?;
        if result != value.original_result || carrier != value.original_carrier { return Err(failure("run producer differs from its original checked result or carrier")); }
        let expected = if propagate { crate::sema::types::Type::Result(Box::new(result), Box::new(crate::sema::types::Type::ProcessError)) } else { result };
        if carrier != expected || (value.capture != value.continuation) != propagate { return Err(failure("run producer changes its checked propagation relationship")); }
        Ok(())
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_run_producer_mut(&mut self, instruction: u32) -> Result<&mut PreparedRunProducer, IrVerifyError> {
        self.run_producer_at(instruction)?.ok_or_else(|| failure("run producer receipt is missing"))?;
        let position = self.context_producers.instructions.binary_search_by_key(&instruction, |entry| entry.0).map_err(|_| failure("run producer receipt is missing"))?;
        let PreparedProducer::Run(value) = Arc::make_mut(&mut self.context_producers.entries[self.context_producers.instructions[position].1].value) else { return Err(failure("producer receipt has another protocol")); };
        Ok(value)
    }
}

#[path = "run_calls.rs"]
mod run_calls;
