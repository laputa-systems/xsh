use super::*;
use crate::sema::check::{BindingIdentity, ExpressionIdentity, StatementIdentity};
use crate::sema::operation_graph::{IterableDomain, PreparedLanguageOperation};
use super::super::IrBlockId;

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct OriginalLineIterationSource {
    pub origin: ExpressionIdentity,
    pub receiver_type: GroundTypeId,
    pub authority: PreparedOperationAuthority,
    pub parameter: (crate::sema::check::DeclarationIdentity, u32),
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct OriginalIterationProducer {
    pub tag: super::super::full::FullTag,
    pub words: Vec<u32>,
    pub declaration: Option<crate::sema::check::DeclarationIdentity>,
    pub initializer: Option<(ExpressionIdentity, u32, u32, Name)>,
    pub lines: Option<OriginalLineIterationSource>,
}

/// The original selected iterable owns the item slot independently of its reads.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct OriginalIterationBinding {
    pub statement: StatementIdentity,
    pub binding: BindingIdentity,
    pub iterator_origin: ExpressionIdentity,
    pub authority: PreparedOperationAuthority,
    pub instruction: u32,
    pub iterator: u32,
    // Result scalar iteration unwraps an authored carrier through a generated
    // projection; the projection has no authored expression identity.
    pub iterator_carrier: Option<u32>,
    pub slot: u32,
    pub body: IrBlockId,
    pub owner: InstructionOwner,
    pub input: GroundTypeId,
    pub item: GroundTypeId,
    pub binding_type: GroundTypeId,
    pub producer: Option<OriginalIterationProducer>,
    pub iterator_parameter: Option<(crate::sema::check::DeclarationIdentity, u32)>,
}

/// A read retains the original binding and emitted storage operation; typed
/// arithmetic may specialize an Int read without changing its producer.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct OriginalIterationUse {
    pub origin: ExpressionIdentity,
    pub binding: IterationBindingId,
    pub instruction: u32,
    pub owner: InstructionOwner,
    pub tag: super::super::full::FullTag,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct OriginalLineScanOperation {
    pub origin: ExpressionIdentity,
    pub authority: PreparedOperationAuthority,
    pub receiver: Option<ExpressionIdentity>,
    pub arguments: Vec<ExpressionIdentity>,
    pub roots: Vec<GroundTypeId>,
    pub result: GroundTypeId,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct OriginalLineScanCheck {
    pub predicate: OriginalLineScanOperation,
    pub condition: super::super::super::ScanCondition,
    pub counter: BindingIdentity,
    pub allocation_origin: StatementIdentity,
    pub allocation: u32,
    pub slot: u32,
    pub counter_type: GroundTypeId,
    pub increment: StatementIdentity,
    pub increment_span: crate::source::Span,
    pub value: ExpressionIdentity,
    pub value_type: GroundTypeId,
    pub ordinal: u32,
    pub compound: super::mutable_bindings::MutableCompoundAssignment,
}

/// A fused scanner retains the original selected members and counter writes;
/// its removed body cannot lend authority through an emitted instruction.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct OriginalLineScan {
    pub statement: StatementIdentity,
    pub binding: BindingIdentity,
    pub iterator_origin: ExpressionIdentity,
    pub instruction: u32,
    pub owner: InstructionOwner,
    pub input: GroundTypeId,
    pub item: GroundTypeId,
    pub binding_type: GroundTypeId,
    pub iteration: PreparedOperationAuthority,
    pub lines: OriginalLineIterationSource,
    pub text_slot: u32,
    pub line_slot: u32,
    pub payload: Vec<u32>,
    pub span: crate::source::Span,
    pub checks_block: (u32, u8, Vec<u32>),
    pub trim: Option<OriginalLineScanOperation>,
    pub checks: Vec<OriginalLineScanCheck>,
}

impl OriginalLineScanOperation {
    fn retained_bytes(&self) -> usize {
        self.authority.retained_bytes() + self.arguments.capacity() * std::mem::size_of::<ExpressionIdentity>() + self.roots.capacity() * std::mem::size_of::<GroundTypeId>()
    }
}

impl OriginalLineScan {
    fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        size_of::<Self>() + 2 * size_of::<usize>() + self.iteration.retained_bytes() + self.lines.authority.retained_bytes()
            + (self.payload.capacity() + self.checks_block.2.capacity()) * size_of::<u32>()
            + self.trim.as_ref().map_or(0, OriginalLineScanOperation::retained_bytes)
            + self.checks.capacity() * size_of::<OriginalLineScanCheck>()
            + self.checks.iter().map(|check| check.predicate.retained_bytes() + match &check.condition { super::super::super::ScanCondition::TrimEmpty => 0, super::super::super::ScanCondition::TrimStartsWith(bytes) | super::super::super::ScanCondition::StartsWith(bytes) => bytes.capacity() }).sum::<usize>()
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct IterationEvidence {
    bindings: Vec<Entry<Arc<OriginalIterationBinding>>>,
    original_bindings: Vec<Arc<OriginalIterationBinding>>,
    uses: Vec<Entry<Arc<OriginalIterationUse>>>,
    original_uses: Vec<Arc<OriginalIterationUse>>,
    use_instructions: Vec<(u32, usize)>,
    scans: Vec<Entry<Arc<OriginalLineScan>>>,
    original_scans: Vec<Arc<OriginalLineScan>>,
    scan_instructions: Vec<(u32, usize)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct IterationCheckpoint { bindings: usize, uses: usize, scans: usize }

impl IterationEvidence {
    pub(super) fn checkpoint(&self) -> IterationCheckpoint { IterationCheckpoint { bindings: self.bindings.len(), uses: self.uses.len(), scans: self.scans.len() } }
    pub(super) fn validate_checkpoint(&self, checkpoint: IterationCheckpoint, serial_limit: u64) -> Result<(), IrVerifyError> {
        if checkpoint.bindings > self.bindings.len() || checkpoint.uses > self.uses.len() || checkpoint.scans > self.scans.len() { return Err(failure("iteration checkpoint references retired entries")); }
        for serial in [self.bindings.get(checkpoint.bindings.wrapping_sub(1)).map(|entry| entry.serial), self.uses.get(checkpoint.uses.wrapping_sub(1)).map(|entry| entry.serial), self.scans.get(checkpoint.scans.wrapping_sub(1)).map(|entry| entry.serial)].into_iter().flatten() {
            if serial >= serial_limit { return Err(failure("iteration checkpoint references replacement entries")); }
        }
        Ok(())
    }
    pub(super) fn rewind_validated(&mut self, checkpoint: IterationCheckpoint) {
        self.bindings.truncate(checkpoint.bindings); self.original_bindings.truncate(checkpoint.bindings);
        self.uses.truncate(checkpoint.uses); self.original_uses.truncate(checkpoint.uses); self.use_instructions.clear();
        self.scans.truncate(checkpoint.scans); self.original_scans.truncate(checkpoint.scans); self.scan_instructions.clear();
    }
    pub(super) fn finish(&mut self) {
        self.use_instructions = self.uses.iter().enumerate().map(|(index, entry)| (entry.value.instruction, index)).collect();
        self.use_instructions.sort_unstable_by_key(|entry| entry.0);
        self.scan_instructions = self.scans.iter().enumerate().map(|(index, entry)| (entry.value.instruction, index)).collect();
        self.scan_instructions.sort_unstable_by_key(|entry| entry.0);
    }
    pub(super) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        self.bindings.capacity() * size_of::<Entry<Arc<OriginalIterationBinding>>>() + self.original_bindings.capacity() * size_of::<Arc<OriginalIterationBinding>>()
            + self.bindings.len() * (size_of::<OriginalIterationBinding>() + 2 * size_of::<usize>())
            + self.bindings.iter().map(|entry| entry.value.authority.retained_bytes() + entry.value.producer.as_ref().map_or(0, |producer| producer.words.capacity() * size_of::<u32>() + producer.lines.as_ref().map_or(0, |lines| lines.authority.retained_bytes()))).sum::<usize>()
            + self.uses.capacity() * size_of::<Entry<Arc<OriginalIterationUse>>>() + self.original_uses.capacity() * size_of::<Arc<OriginalIterationUse>>()
            + self.uses.len() * (size_of::<OriginalIterationUse>() + 2 * size_of::<usize>()) + self.use_instructions.capacity() * size_of::<(u32, usize)>()
            + self.scans.capacity() * size_of::<Entry<Arc<OriginalLineScan>>>() + self.original_scans.capacity() * size_of::<Arc<OriginalLineScan>>()
            + self.scan_instructions.capacity() * size_of::<(u32, usize)>()
            + self.scans.iter().map(|entry| entry.value.retained_bytes()).sum::<usize>()
    }
    pub(super) fn shrink_to_fit(&mut self) { self.bindings.shrink_to_fit(); self.original_bindings.shrink_to_fit(); self.uses.shrink_to_fit(); self.original_uses.shrink_to_fit(); self.use_instructions.shrink_to_fit(); self.scans.shrink_to_fit(); self.original_scans.shrink_to_fit(); self.scan_instructions.shrink_to_fit(); }
}

impl GenericEvidenceStore {
    fn verify_line_scans(&self, pools: &SemanticPools, owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        use crate::sema::types::Type;
        use super::super::super::ScanCondition;
        let mut instructions = std::collections::BTreeSet::new();
        for scan in self.line_scans() {
            self.line_scan(scan.instruction)?.ok_or_else(|| failure("line scan original receipt is missing"))?;
            if !instructions.insert(scan.instruction) || owners.get(scan.instruction as usize) != Some(&Some(scan.owner))
                || self.registered_instruction_origin(scan.instruction, false) != Some((OperationSourceOrigin::Statement(scan.statement), scan.owner))
                || scan.binding.source != scan.statement.source || scan.binding.namespace != scan.statement.namespace
                || scan.iterator_origin.source != scan.statement.source || scan.iterator_origin.namespace != scan.statement.namespace
                || scan.lines.origin.source != scan.statement.source || scan.lines.origin.namespace != scan.statement.namespace
                || scan.text_slot != scan.lines.parameter.1 || scan.line_slot == scan.text_slot
                || pools.to_type(scan.input)? != Type::List(Box::new(Type::Bytes)) || pools.to_type(scan.item)? != Type::Bytes
                || pools.to_type(scan.binding_type)? != Type::Bytes || pools.to_type(scan.lines.receiver_type)? != Type::Bytes
                || !matches!(scan.iteration, PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Iteration { domain: IterableDomain::List, outer_result: false }, .. })
                || !matches!(scan.lines.authority, PreparedOperationAuthority::Registry { operation: crate::modules::RuntimeOp::BytesStreamLines, binding: crate::modules::signature::ImplBinding::Native, semantic_rule: crate::modules::signature::SemanticRule::Standard, .. }) { return Err(failure("line scan changes its original source, owner, or line contract")); }
            if let Some(trim) = &scan.trim {
                if !matches!(trim.authority, PreparedOperationAuthority::Registry { operation: crate::modules::RuntimeOp::BytesTrim, binding: crate::modules::signature::ImplBinding::Native, semantic_rule: crate::modules::signature::SemanticRule::Standard, .. })
                    || trim.receiver.is_none() || !trim.arguments.is_empty() || trim.roots.len() != 1 || pools.to_type(trim.result)? != Type::Bytes { return Err(failure("line scan changes its original trim contract")); }
            }
            if scan.checks.is_empty() { return Err(failure("line scan has no original counter checks")); }
            for check in &scan.checks {
                let predicate = &check.predicate;
                if predicate.origin.source != scan.statement.source || predicate.origin.namespace != scan.statement.namespace
                    || predicate.roots.iter().any(|&root| pools.to_type(root).ok() != Some(Type::Bytes)) || pools.to_type(predicate.result)? != Type::Bool
                    || check.counter.source != scan.statement.source || check.counter.namespace != scan.statement.namespace
                    || check.increment.source != scan.statement.source || check.increment.namespace != scan.statement.namespace
                    || check.increment_span.source_id != check.increment.source || scan.span.source_id != scan.statement.source
                    || check.allocation_origin.source != scan.statement.source || check.allocation_origin.namespace != scan.statement.namespace
                    || check.value.source != scan.statement.source || check.value.namespace != scan.statement.namespace
                    || check.slot == scan.line_slot || check.slot == scan.text_slot || check.ordinal == 0
                    || pools.to_type(check.counter_type)? != Type::Int || pools.to_type(check.value_type)? != Type::Int
                    || check.compound.left != check.counter_type || check.compound.right != check.value_type || check.compound.result != check.counter_type
                    || !matches!(check.compound.operation, PreparedLanguageOperation::Compound { op: crate::syntax::node::AssignOp::Add, domain: crate::sema::operation_graph::ArithmeticDomain::Integer { left: crate::sema::inference::Atom::Int, right: crate::sema::inference::Atom::Int } }) { return Err(failure("line scan changes its original predicate or counter write")); }
                match (&check.condition, &predicate.authority) {
                    (ScanCondition::TrimEmpty, PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Equality { op: crate::syntax::node::BinaryOp::Eq }, .. }) if scan.trim.is_some() && predicate.receiver.is_none() && predicate.arguments.len() == 2 && predicate.roots.len() == 2 => {},
                    (ScanCondition::TrimStartsWith(_), PreparedOperationAuthority::Registry { operation: crate::modules::RuntimeOp::BytesStartsWith, binding: crate::modules::signature::ImplBinding::Native, semantic_rule: crate::modules::signature::SemanticRule::Standard, .. }) if scan.trim.is_some() && predicate.receiver.is_some() && predicate.arguments.len() == 1 && predicate.roots.len() == 2 => {},
                    (ScanCondition::StartsWith(_), PreparedOperationAuthority::Registry { operation: crate::modules::RuntimeOp::BytesStartsWith, binding: crate::modules::signature::ImplBinding::Native, semantic_rule: crate::modules::signature::SemanticRule::Standard, .. }) if scan.trim.is_none() && predicate.receiver.is_some() && predicate.arguments.len() == 1 && predicate.roots.len() == 2 => {},
                    _ => return Err(failure("line scan changes its original selected condition")),
                }
            }
        }
        Ok(())
    }
    pub fn has_iteration_bindings(&self) -> bool { !self.iterations.bindings.is_empty() || !self.iterations.uses.is_empty() || !self.iterations.scans.is_empty() }
    pub fn line_scans(&self) -> impl Iterator<Item = &OriginalLineScan> { self.iterations.scans.iter().map(|entry| entry.value.as_ref()) }
    pub fn line_scan(&self, instruction: u32) -> Result<Option<&OriginalLineScan>, IrVerifyError> {
        let Some(index) = self.iterations.scan_instructions.binary_search_by_key(&instruction, |entry| entry.0).ok().map(|position| self.iterations.scan_instructions[position].1) else { return Ok(None); };
        let entry = self.iterations.scans.get(index).ok_or_else(|| failure("line scan index is stale"))?;
        if !self.iterations.original_scans.get(index).is_some_and(|original| Arc::ptr_eq(original, &entry.value)) { return Err(failure("line scan differs from its original receipt")); }
        Ok(Some(entry.value.as_ref()))
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_line_scan_mut(&mut self, instruction: u32) -> Result<&mut OriginalLineScan, IrVerifyError> {
        self.line_scan(instruction)?.ok_or_else(|| failure("line scan receipt is missing"))?;
        let index = self.iterations.scan_instructions.binary_search_by_key(&instruction, |entry| entry.0).unwrap();
        Ok(Arc::make_mut(&mut self.iterations.scans[self.iterations.scan_instructions[index].1].value))
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_remove_line_scans(&mut self) { self.iterations.scans.clear(); self.iterations.scan_instructions.clear(); }
    pub fn iteration_binding(&self, id: IterationBindingId) -> Result<&OriginalIterationBinding, IrVerifyError> {
        let value = owned(self.root, &self.iterations.bindings, id.index, id.proof)?;
        if !self.iterations.original_bindings.get(id.index as usize).is_some_and(|original| Arc::ptr_eq(value, original)) { return Err(failure("iteration binding differs from its original receipt")); }
        Ok(value.as_ref())
    }
    pub fn iteration_bindings(&self) -> impl Iterator<Item = (IterationBindingId, &OriginalIterationBinding)> {
        self.iterations.bindings.iter().enumerate().map(|(index, entry)| (IterationBindingId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } }, entry.value.as_ref()))
    }
    pub fn iteration_use(&self, instruction: u32) -> Result<Option<&OriginalIterationUse>, IrVerifyError> {
        let Some(index) = self.iterations.use_instructions.binary_search_by_key(&instruction, |entry| entry.0).ok().map(|index| self.iterations.use_instructions[index].1) else { return Ok(None); };
        let value = &self.iterations.uses.get(index).ok_or_else(|| failure("iteration use index is stale"))?.value;
        if !self.iterations.original_uses.get(index).is_some_and(|original| Arc::ptr_eq(value, original)) { return Err(failure("iteration use differs from its original receipt")); }
        Ok(Some(value.as_ref()))
    }
    pub fn iteration_uses(&self) -> impl Iterator<Item = &OriginalIterationUse> { self.iterations.uses.iter().map(|entry| entry.value.as_ref()) }
    pub(super) fn verify_iteration_evidence(&self, pools: &SemanticPools, owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        if self.iterations.bindings.len() != self.iterations.original_bindings.len() || self.iterations.uses.len() != self.iterations.original_uses.len() { return Err(failure("iteration original receipt ledger is incomplete")); }
        if self.iterations.scans.len() != self.iterations.original_scans.len() { return Err(failure("line scan original receipt ledger is incomplete")); }
        self.verify_line_scans(pools, owners)?;
        let mut seen = std::collections::BTreeSet::new();
        for (id, _) in self.iteration_bindings() {
            let binding = self.iteration_binding(id)?;
            if !seen.insert(binding.binding) || binding.statement.source != binding.binding.source || binding.statement.namespace != binding.binding.namespace
                || binding.iterator_origin.source != binding.statement.source || binding.iterator_origin.namespace != binding.statement.namespace
                || owners.get(binding.instruction as usize) != Some(&Some(binding.owner)) || owners.get(binding.iterator as usize) != Some(&Some(binding.owner))
                || self.registered_instruction_origin(binding.instruction, false) != Some((OperationSourceOrigin::Statement(binding.statement), binding.owner))
                || self.registered_instruction_origin(binding.iterator_carrier.unwrap_or(binding.iterator), false) != Some((OperationSourceOrigin::Expression(binding.producer.as_ref().and_then(|producer| producer.lines.as_ref().map(|lines| lines.origin)).unwrap_or(binding.iterator_origin)), binding.owner)) {
                return Err(failure("iteration binding changes its original source or owner"));
            }
            let item = pools.to_type(binding.item)?;
            let actual_input = pools.to_type(binding.input)?;
            let expected_input = match binding.authority {
                PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Iteration { domain: IterableDomain::List, outer_result: false }, .. } if matches!(item, crate::sema::types::Type::Str | crate::sema::types::Type::Bytes | crate::sema::types::Type::Int | crate::sema::types::Type::Path) || matches!(item, crate::sema::types::Type::Record(_)) && binding.producer.as_ref().is_some_and(|producer| producer.initializer.is_some()
                    || producer.tag == super::super::full::FullTag::ExprList && producer.declaration.is_none() && producer.lines.is_none() && binding.iterator_parameter.is_none() && binding.iterator_carrier.is_none()) => crate::sema::types::Type::List(Box::new(item.clone())),
                PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Iteration { domain: IterableDomain::List, outer_result: false }, .. } if binding.producer.as_ref().and_then(|producer| producer.lines.as_ref()).is_some_and(|lines| {
                    matches!((&lines.authority, &item),
                        (PreparedOperationAuthority::Registry { operation: crate::modules::RuntimeOp::TextStreamLines, binding: crate::modules::signature::ImplBinding::Native, semantic_rule: crate::modules::signature::SemanticRule::Standard, .. }, crate::sema::types::Type::Str)
                        | (PreparedOperationAuthority::Registry { operation: crate::modules::RuntimeOp::BytesStreamLines, binding: crate::modules::signature::ImplBinding::Native, semantic_rule: crate::modules::signature::SemanticRule::Standard, .. }, crate::sema::types::Type::Bytes))
                        && pools.to_type(lines.receiver_type).ok().as_ref() == Some(&item)
                }) => crate::sema::types::Type::List(Box::new(item.clone())),
                PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Iteration { domain: IterableDomain::Stream, outer_result: true }, .. } if matches!(item, crate::sema::types::Type::Record(_)) && binding.producer.as_ref().is_some_and(|producer| matches!(producer.tag, super::super::full::FullTag::ExprModuleCall | super::super::full::FullTag::ExprFsList) && producer.declaration.is_none() && producer.initializer.is_none()) => {
                    let crate::sema::types::Type::Result(success, _) = &actual_input else { return Err(failure("native stream iterator loses its original Result type")); };
                    if **success != crate::sema::types::Type::Stream(Box::new(item.clone())) || binding.iterator_carrier.is_some() { return Err(failure("native stream iterator changes its original item or transport")); }
                    actual_input.clone()
                }
                PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Iteration { domain: IterableDomain::Stream, outer_result: false }, .. } if item == crate::sema::types::Type::Int => crate::sema::types::Type::Stream(Box::new(item.clone())),
                PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Iteration { domain: IterableDomain::Str, outer_result: false }, .. } if item == crate::sema::types::Type::Str => crate::sema::types::Type::Str,
                PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Iteration { domain: IterableDomain::Bytes, outer_result: false }, .. } if item == crate::sema::types::Type::Int => crate::sema::types::Type::Bytes,
                PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Iteration { domain: IterableDomain::Bytes, outer_result: true }, .. } if item == crate::sema::types::Type::Int && binding.iterator_carrier.is_some() => {
                    let crate::sema::types::Type::Result(success, _) = &actual_input else { return Err(failure("iteration carrier loses its original Result type")); };
                    if **success != crate::sema::types::Type::Bytes { return Err(failure("iteration carrier changes its checked Bytes success type")); }
                    actual_input.clone()
                }
                _ => return Err(failure("iteration binding has an unprepared iterable contract")),
            };
            if actual_input != expected_input
                || binding.binding_type != binding.item {
                return Err(failure("iteration binding has an unprepared item contract"));
            }
            if let Some(lines) = binding.producer.as_ref().and_then(|producer| producer.lines.as_ref()) {
                if lines.origin.source != binding.statement.source || lines.origin.namespace != binding.statement.namespace
                    || lines.parameter.0.source != binding.statement.source || lines.parameter.0.namespace != binding.statement.namespace
                    || !matches!(binding.owner, InstructionOwner::Function(_)) || binding.producer.as_ref().is_some_and(|producer| producer.declaration.is_some() || producer.initializer.is_some() || producer.tag != super::super::full::FullTag::ExprParam || producer.words != [lines.parameter.1]) || binding.iterator_parameter.is_some() || binding.iterator_carrier.is_some() { return Err(failure("line iteration changes its original receiver declaration or transport")); }
            }
            if let Some(carrier) = binding.iterator_carrier
                && (owners.get(carrier as usize) != Some(&Some(binding.owner))
                    || self.registered_instruction_origin(carrier, false) != Some((OperationSourceOrigin::Expression(binding.iterator_origin), binding.owner))) {
                return Err(failure("iteration carrier changes its original source or owner"));
            }
            if let Some((declaration, _)) = binding.iterator_parameter
                && (declaration.source != binding.statement.source || declaration.namespace != binding.statement.namespace
                    || !matches!(binding.owner, InstructionOwner::Function(_))) {
                return Err(failure("iteration parameter belongs to another original declaration"));
            }
        }
        let mut expected_index = Vec::new();
        for (index, entry) in self.iterations.uses.iter().enumerate() {
            let use_ = self.iteration_use(entry.value.instruction)?.ok_or_else(|| failure("iteration use index is missing"))?;
            if !std::ptr::eq(use_, entry.value.as_ref()) { return Err(failure("iteration use index identifies another read")); }
            let binding = self.iteration_binding(use_.binding)?;
            if !matches!((use_.tag, pools.to_type(binding.binding_type)?),
                (super::super::full::FullTag::ExprParam, _) | (super::super::full::FullTag::IntSlot, crate::sema::types::Type::Int)) {
                return Err(failure("iteration read changes its checked storage contract"));
            }
            if use_.owner != binding.owner || owners.get(use_.instruction as usize) != Some(&Some(use_.owner))
                || self.registered_instruction_origin(use_.instruction, false) != Some((OperationSourceOrigin::Expression(use_.origin), use_.owner)) {
                return Err(failure("iteration read changes its original binding or owner"));
            }
            expected_index.push((use_.instruction, index));
        }
        expected_index.sort_unstable_by_key(|entry| entry.0);
        if expected_index.windows(2).any(|pair| pair[0].0 == pair[1].0) || expected_index != self.iterations.use_instructions { return Err(failure("iteration read index is ambiguous or stale")); }
        Ok(())
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_iteration_use_mut(&mut self, instruction: u32) -> Result<&mut OriginalIterationUse, IrVerifyError> {
        self.iteration_use(instruction)?.ok_or_else(|| failure("iteration use is missing"))?;
        let index = self.iterations.use_instructions.binary_search_by_key(&instruction, |entry| entry.0).map_err(|_| failure("iteration use is missing"))?;
        let entry = self.iterations.use_instructions[index].1;
        Ok(Arc::make_mut(&mut self.iterations.uses[entry].value))
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_remove_iteration_uses(&mut self) { self.iterations.uses.clear(); self.iterations.use_instructions.clear(); }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_remove_iteration_bindings(&mut self) { self.iterations.bindings.clear(); }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_remove_iteration_record_constructor_layouts(&mut self) { self.constructors.clear(); }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_iteration_binding_mut(&mut self, id: IterationBindingId) -> Result<&mut OriginalIterationBinding, IrVerifyError> {
        self.iteration_binding(id)?;
        Ok(Arc::make_mut(&mut self.iterations.bindings[id.index as usize].value))
    }
}

impl GenericEvidenceBuilder {
    pub fn line_scan_counter_allocation(&self, binding: BindingIdentity, statement: StatementIdentity, owner: InstructionOwner) -> Result<super::mutable_bindings::MutableBindingReceipt, IrVerifyError> {
        let mut entries = self.store.mutable_binding_receipts().filter(|entry| entry.binding == binding && entry.statement == Some(statement) && entry.owner == owner && entry.ordinal == 0 && entry.read_origin.is_none() && entry.capture.is_none());
        let original = entries.next().ok_or_else(|| failure("line scan counter has no original mutable allocation"))?.clone();
        if entries.next().is_some() { return Err(failure("line scan counter has multiple original allocations")); }
        Ok(original)
    }
    pub fn add_line_scan(&mut self, value: OriginalLineScan) -> Result<(), IrVerifyError> {
        if self.store.iterations.scans.len() >= 2_000_000 { return Err(failure("line scans exceed their work limit")); }
        let serial = self.next_serial; self.next_serial = serial.checked_add(1).ok_or_else(|| failure("generic evidence serial overflow"))?;
        let value = Arc::new(value); self.store.iterations.original_scans.push(Arc::clone(&value)); self.store.iterations.scans.push(Entry { serial, value });
        Ok(())
    }
    pub fn add_iteration_binding(&mut self, value: OriginalIterationBinding) -> Result<IterationBindingId, IrVerifyError> {
        if self.store.iterations.bindings.len() >= 2_000_000 { return Err(failure("iteration bindings exceed their work limit")); }
        let index = u32::try_from(self.store.iterations.bindings.len()).map_err(|_| failure("iteration binding id overflow"))?;
        let serial = self.next_serial; self.next_serial = serial.checked_add(1).ok_or_else(|| failure("generic evidence serial overflow"))?;
        let value = Arc::new(value); self.store.iterations.original_bindings.push(Arc::clone(&value)); self.store.iterations.bindings.push(Entry { serial, value });
        Ok(IterationBindingId { index, proof: OwnerProof { root: self.store.root, serial } })
    }
    pub fn add_iteration_use(&mut self, value: OriginalIterationUse) -> Result<(), IrVerifyError> {
        self.store.iteration_binding(value.binding)?;
        if self.store.iterations.uses.len() >= 2_000_000 { return Err(failure("iteration reads exceed their work limit")); }
        let serial = self.next_serial; self.next_serial = serial.checked_add(1).ok_or_else(|| failure("generic evidence serial overflow"))?;
        let value = Arc::new(value); self.store.iterations.original_uses.push(Arc::clone(&value)); self.store.iterations.uses.push(Entry { serial, value });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::eval::indexed::full::FullBuilder;
    use crate::sema::check::Checker;
    use crate::source::SourceMap;

    #[test]
    fn iteration_receipts_reject_foreign_and_rewound_handles_and_count_shared_payloads_once() {
        crate::runtime::eval::run_eval(|| {
            let source = "proc quoted(values: List[Str]) [io] { for value in values { print ${shlex.quote(value)} } }\nquoted([\"one\"])\n";
            let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
                "iteration-lifetime.xsh", crate::loader::entry_source_from_text("iteration-lifetime.xsh", source.to_owned()), Vec::new());
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let declarations = Checker::compact_declarations_from_checked(&parsed.arena, &checked);
            let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
            let source_id = SourceMap::files(&sources).first().unwrap().id();
            let program = FullBuilder::build_compact(&parsed.arena, &declarations, &bodies, source, Arc::new(sources), source_id).unwrap();
            drop(parsed); drop(checked); drop(declarations); drop(bodies);
            let _symbols = program.symbol_owner().enter();
            let generic = program.generic_evidence().unwrap();
            let (original_id, original) = generic.iteration_bindings().next().unwrap();
            let mut original = original.clone();
            let mut pools = SemanticPools::default();
            let mut semantic = super::super::super::semantic::SemanticPoolBuilder::default();
            original.input = semantic.intern_type(&mut pools, &crate::sema::types::Type::List(Box::new(crate::sema::types::Type::Str))).unwrap();
            original.item = semantic.intern_type(&mut pools, &crate::sema::types::Type::Str).unwrap();
            original.binding_type = original.item;
            let original_read = generic.iteration_uses().next().unwrap();
            let mut builder = GenericEvidenceBuilder::default();
            assert!(builder.rewind(GenericEvidenceBuilder::default().checkpoint()).is_err());
            let base = builder.checkpoint();
            let retired = builder.add_iteration_binding(original.clone()).unwrap();
            builder.add_iteration_use(OriginalIterationUse { binding: retired, ..original_read.clone() }).unwrap();
            let stale = builder.checkpoint();
            builder.rewind(base).unwrap();
            let replacement = builder.add_iteration_binding(original.clone()).unwrap();
            assert_ne!(retired, replacement);
            assert!(builder.store.iteration_binding(retired).is_err());
            assert!(builder.store.iteration_binding(original_id).is_err());
            assert!(builder.rewind(stale).is_err());
            let mut outsider = GenericEvidenceBuilder::default();
            let foreign = outsider.add_iteration_binding(original.clone()).unwrap();
            assert!(builder.store.iteration_binding(foreign).is_err());
            assert!(builder.add_iteration_use(OriginalIterationUse { binding: foreign, ..original_read.clone() }).is_err());
            builder.add_iteration_use(OriginalIterationUse { binding: replacement, ..original_read.clone() }).unwrap();
            for (instruction, origin) in [
                (original.instruction, OperationSourceOrigin::Statement(original.statement)),
                (original.iterator, OperationSourceOrigin::Expression(original.iterator_origin)),
                (original_read.instruction, OperationSourceOrigin::Expression(original_read.origin)),
            ] { builder.register_instruction_origin(instruction, origin, original.owner).unwrap(); }
            let owners = vec![Some(original.owner); program.instruction_count()];
            let mut store = builder.finish(&pools, program.function_count(), &owners).unwrap();
            assert!(store.iteration_binding(retired).is_err());
            assert_eq!(store.iteration_use(original_read.instruction).unwrap().unwrap().binding, replacement);
            let evidence = &store.iterations;
            assert!(Arc::ptr_eq(&evidence.bindings[0].value, &evidence.original_bindings[0]));
            assert!(Arc::ptr_eq(&evidence.uses[0].value, &evidence.original_uses[0]));
            assert_eq!(Arc::strong_count(&evidence.bindings[0].value), 2);
            assert_eq!(Arc::strong_count(&evidence.uses[0].value), 2);
            let shallow = evidence.bindings.capacity() * std::mem::size_of::<Entry<Arc<OriginalIterationBinding>>>()
                + evidence.original_bindings.capacity() * std::mem::size_of::<Arc<OriginalIterationBinding>>()
                + evidence.uses.capacity() * std::mem::size_of::<Entry<Arc<OriginalIterationUse>>>()
                + evidence.original_uses.capacity() * std::mem::size_of::<Arc<OriginalIterationUse>>()
                + evidence.use_instructions.capacity() * std::mem::size_of::<(u32, usize)>();
            let unique_payloads = std::mem::size_of::<OriginalIterationBinding>() + std::mem::size_of::<OriginalIterationUse>()
                + 4 * std::mem::size_of::<usize>() + original.authority.retained_bytes();
            assert_eq!(evidence.retained_bytes(), shallow + unique_payloads, "the private receipt and visible entry share each allocation");
            let retained = store.retained_bytes();
            store.shrink_to_fit();
            assert!(store.retained_bytes() <= retained);
            assert_eq!(store.iteration_use(original_read.instruction).unwrap().unwrap().binding, replacement);
        });
    }
}
