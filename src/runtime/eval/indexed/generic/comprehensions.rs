use super::*;
use crate::sema::check::{BindingIdentity, ComprehensionIdentity, ExpressionIdentity};
use crate::sema::operation_graph::{IterableDomain, PreparedLanguageOperation};

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct ComprehensionRoot {
    pub origin: ExpressionIdentity,
    pub instruction: u32,
    pub ty: GroundTypeId,
    pub tag: super::super::full::FullTag,
    pub payload: Box<[u32]>,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct ComprehensionGenerator {
    pub origin: ComprehensionIdentity,
    pub iterator: ComprehensionRoot,
    pub item: GroundTypeId,
    pub bindings: Vec<ComprehensionBinding>,
    pub target: ComprehensionTarget,
    pub authority: PreparedOperationAuthority,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct ComprehensionBinding {
    pub identity: BindingIdentity,
    pub ty: GroundTypeId,
    pub slot: u32,
    pub path: Vec<Name>,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) enum ComprehensionTarget {
    Discard,
    Slot(u32),
    Record(Vec<(Name, ComprehensionTarget)>),
}

impl ComprehensionTarget {
    fn retained_bytes(&self) -> usize {
        match self { Self::Record(fields) => fields.capacity() * std::mem::size_of::<(Name, Self)>() + fields.iter().map(|(_, child)| child.retained_bytes()).sum::<usize>(), _ => 0 }
    }
    fn binding_paths(&self, path: &mut Vec<Name>, result: &mut Vec<(u32, Vec<Name>)>) {
        match self {
            Self::Discard => {},
            Self::Slot(index) => result.push((*index, path.clone())),
            Self::Record(fields) => for (name, child) in fields { path.push(*name); child.binding_paths(path, result); path.pop(); },
        }
    }
}

/// The checked generator sequence and its original lexical reads authorize
/// the material list or map producer independently of a receiving local binding.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedComprehension {
    pub origin: ExpressionIdentity,
    pub instruction: u32,
    pub owner: InstructionOwner,
    pub result: GroundTypeId,
    pub value: ComprehensionRoot,
    pub key: Option<ComprehensionRoot>,
    pub generators: Vec<ComprehensionGenerator>,
    pub filters: Vec<(u32, ComprehensionRoot)>,
    pub filter_authorities: Vec<Option<PreparedOperationAuthority>>,
    pub reads: Vec<(ComprehensionRoot, BindingIdentity)>,
    pub payload: Box<[u32]>,
    pub qualifier_block: super::super::IrBlockId,
    pub qualifier_payload: Box<[u32]>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct ComprehensionEvidence {
    entries: Vec<Entry<Arc<PreparedComprehension>>>,
    originals: Vec<Arc<PreparedComprehension>>,
    instructions: Vec<(u32, usize)>,
    reads: Vec<(u32, usize, usize)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ComprehensionCheckpoint { entries: usize }

impl ComprehensionEvidence {
    pub(super) fn checkpoint(&self) -> ComprehensionCheckpoint { ComprehensionCheckpoint { entries: self.entries.len() } }
    pub(super) fn validate_checkpoint(&self, checkpoint: ComprehensionCheckpoint, serial_limit: u64) -> Result<(), IrVerifyError> {
        if checkpoint.entries > self.entries.len() || self.entries.get(checkpoint.entries.wrapping_sub(1)).is_some_and(|entry| entry.serial >= serial_limit) { return Err(failure("comprehension checkpoint references retired or replaced entries")); }
        Ok(())
    }
    pub(super) fn rewind_validated(&mut self, checkpoint: ComprehensionCheckpoint) { self.entries.truncate(checkpoint.entries); self.originals.truncate(checkpoint.entries); self.instructions.clear(); self.reads.clear(); }
    pub(super) fn finish(&mut self) { self.instructions = self.entries.iter().enumerate().map(|(index, entry)| (entry.value.instruction, index)).collect(); self.instructions.sort_unstable_by_key(|entry| entry.0); self.reads = self.entries.iter().enumerate().flat_map(|(index, entry)| entry.value.reads.iter().enumerate().map(move |(read, (root, _))| (root.instruction, index, read))).collect(); self.reads.sort_unstable_by_key(|entry| entry.0); }
    pub(super) fn shrink_to_fit(&mut self) { self.entries.shrink_to_fit(); self.originals.shrink_to_fit(); self.instructions.shrink_to_fit(); self.reads.shrink_to_fit(); }
    pub(super) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        self.entries.capacity() * size_of::<Entry<Arc<PreparedComprehension>>>() + self.originals.capacity() * size_of::<Arc<PreparedComprehension>>() + self.instructions.capacity() * size_of::<(u32, usize)>() + self.reads.capacity() * size_of::<(u32, usize, usize)>()
            + self.entries.iter().map(|entry| { let value = &entry.value; size_of::<PreparedComprehension>() + 2 * size_of::<usize>()
                + value.generators.capacity() * size_of::<ComprehensionGenerator>() + value.filters.capacity() * size_of::<(u32, ComprehensionRoot)>()
                + value.reads.capacity() * size_of::<(ComprehensionRoot, BindingIdentity)>() + value.filter_authorities.capacity() * size_of::<Option<PreparedOperationAuthority>>() + (value.payload.len() + value.qualifier_payload.len()) * size_of::<u32>()
                + std::iter::once(&value.value).chain(value.key.iter()).chain(value.generators.iter().map(|generator| &generator.iterator)).chain(value.filters.iter().map(|(_, root)| root)).chain(value.reads.iter().map(|(root, _)| root)).map(|root| root.payload.len() * size_of::<u32>()).sum::<usize>()
                + value.filter_authorities.iter().flatten().map(|authority| authority.retained_bytes()).sum::<usize>()
                + value.generators.iter().map(|generator| generator.authority.retained_bytes() + generator.bindings.capacity() * size_of::<ComprehensionBinding>() + generator.bindings.iter().map(|binding| binding.path.capacity() * size_of::<Name>()).sum::<usize>() + generator.target.retained_bytes()).sum::<usize>() }).sum::<usize>()
    }
}

impl GenericEvidenceBuilder {
    pub fn add_comprehension(&mut self, value: PreparedComprehension) -> Result<(), IrVerifyError> {
        if self.store.comprehensions.entries.len() >= 2_000_000 { return Err(failure("comprehension receipts exceed their work limit")); }
        let serial = self.next_serial; self.next_serial = serial.checked_add(1).ok_or_else(|| failure("generic evidence serial overflow"))?;
        let value = Arc::new(value); self.store.comprehensions.originals.push(value.clone()); self.store.comprehensions.entries.push(Entry { serial, value });
        Ok(())
    }
}

impl GenericEvidenceStore {
    pub fn has_comprehensions(&self) -> bool { !self.comprehensions.entries.is_empty() || !self.comprehensions.originals.is_empty() }
    pub fn comprehensions(&self) -> impl Iterator<Item = &PreparedComprehension> { self.comprehensions.entries.iter().map(|entry| entry.value.as_ref()) }
    pub fn comprehension_at(&self, instruction: u32) -> Result<Option<&PreparedComprehension>, IrVerifyError> {
        let Some(position) = self.comprehensions.instructions.binary_search_by_key(&instruction, |entry| entry.0).ok() else { return Ok(None); };
        let index = self.comprehensions.instructions[position].1;
        let entry = self.comprehensions.entries.get(index).ok_or_else(|| failure("comprehension instruction index is stale"))?;
        if !self.comprehensions.originals.get(index).is_some_and(|original| Arc::ptr_eq(original, &entry.value)) { return Err(failure("comprehension differs from its original receipt")); }
        Ok(Some(entry.value.as_ref()))
    }
    pub fn comprehension_read_at(&self, instruction: u32) -> Result<Option<(&PreparedComprehension, &ComprehensionRoot, BindingIdentity)>, IrVerifyError> {
        let Some(position) = self.comprehensions.reads.binary_search_by_key(&instruction, |entry| entry.0).ok() else { return Ok(None); };
        let (_, index, read) = self.comprehensions.reads[position];
        let instruction = self.comprehensions.entries.get(index).ok_or_else(|| failure("comprehension read index is stale"))?.value.instruction;
        let value = self.comprehension_at(instruction)?.ok_or_else(|| failure("comprehension read owner is missing"))?;
        let (root, binding) = value.reads.get(read).ok_or_else(|| failure("comprehension read index is stale"))?;
        Ok(Some((value, root, *binding)))
    }
    pub(super) fn verify_comprehension_evidence(&self, pools: &SemanticPools, owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        if self.comprehensions.entries.len() != self.comprehensions.originals.len() { return Err(failure("comprehension original ledger is incomplete")); }
        let mut expected_index = Vec::new();
        let mut expected_reads = Vec::new();
        for (index, entry) in self.comprehensions.entries.iter().enumerate() {
            let value = self.comprehension_at(entry.value.instruction)?.ok_or_else(|| failure("comprehension index is missing"))?;
            if !std::ptr::eq(value, entry.value.as_ref()) || owners.get(value.instruction as usize) != Some(&Some(value.owner))
                || self.registered_instruction_origin(value.instruction, false) != Some((OperationSourceOrigin::Expression(value.origin), value.owner))
                || pools.to_type(value.result)? != match &value.key { Some(key) => crate::sema::types::Type::Map(Box::new(pools.to_type(key.ty)?), Box::new(pools.to_type(value.value.ty)?)), None => crate::sema::types::Type::List(Box::new(pools.to_type(value.value.ty)?)) } { return Err(failure("comprehension changes its source, owner or result relationship")); }
            let roots = std::iter::once(&value.value).chain(value.key.iter()).chain(value.filters.iter().map(|(_, root)| root)).chain(value.generators.iter().map(|generator| &generator.iterator)).chain(value.reads.iter().map(|(root, _)| root));
            for root in roots {
                if root.origin.source != value.origin.source || root.origin.namespace != value.origin.namespace || owners.get(root.instruction as usize) != Some(&Some(value.owner))
                    || self.registered_instruction_origin(root.instruction, false) != Some((OperationSourceOrigin::Expression(root.origin), value.owner)) { return Err(failure("comprehension child changes its original source or owner")); }
                pools.to_type(root.ty)?;
            }
            if value.filters.len() != value.filter_authorities.len() { return Err(failure("comprehension filter authority ledger is incomplete")); }
            let mut ordinals = std::collections::BTreeSet::new();
            let mut bindings = std::collections::BTreeSet::new();
            for generator in &value.generators {
                if generator.origin.expression != value.origin || !ordinals.insert(generator.origin.qualifier) { return Err(failure("comprehension generator changes its original binding relationship")); }
                let item = pools.to_type(generator.item)?;
                let mut paths = Vec::new();
                generator.target.binding_paths(&mut Vec::new(), &mut paths);
                let mut indices = std::collections::BTreeSet::new();
                for (index, path) in paths {
                    let binding = generator.bindings.get(index as usize).ok_or_else(|| failure("comprehension target loses its original leaf binding"))?;
                    if !indices.insert(index) || !bindings.insert(binding.identity) || binding.path != path || binding.identity.source != value.origin.source || binding.identity.namespace != value.origin.namespace { return Err(failure("comprehension target changes its original field or binding")); }
                    let mut projected = &item;
                    for field in &path { let crate::sema::types::Type::Record(fields) = projected else { return Err(failure("comprehension target projects a non-record item")); }; projected = fields.get(field).ok_or_else(|| failure("comprehension target projects another original item field"))?; }
                    if pools.to_type(binding.ty)? != *projected { return Err(failure("comprehension target changes its checked field type")); }
                }
                if indices.len() != generator.bindings.len() { return Err(failure("comprehension target binding ledger is incomplete")); }
                let expected = match generator.authority {
                    PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Iteration { domain: IterableDomain::List, outer_result: false }, .. } => crate::sema::types::Type::List(Box::new(item)),
                    PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Iteration { domain: IterableDomain::Str, outer_result: false }, .. } if item == crate::sema::types::Type::Str => crate::sema::types::Type::Str,
                    PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Iteration { domain: IterableDomain::Bytes, outer_result: false }, .. } if item == crate::sema::types::Type::Int => crate::sema::types::Type::Bytes,
                    PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Iteration { domain: IterableDomain::Map, outer_result: false }, .. } => {
                        let crate::sema::types::Type::Record(fields) = &item else { return Err(failure("comprehension Map iterator loses its original entry record")); };
                        if fields.len() != 2 { return Err(failure("comprehension Map iterator changes its entry fields")); }
                        crate::sema::types::Type::Map(Box::new(fields.get(&Name::intern("key")).ok_or_else(|| failure("comprehension Map entry loses its key type"))?.clone()), Box::new(fields.get(&Name::intern("value")).ok_or_else(|| failure("comprehension Map entry loses its value type"))?.clone()))
                    }
                    _ => return Err(failure("comprehension generator has an unprepared iterable contract")),
                };
                if pools.to_type(generator.iterator.ty)? != expected { return Err(failure("comprehension generator changes its checked input/item relationship")); }
            }
            for (ordinal, filter) in &value.filters { if !ordinals.insert(*ordinal) || pools.to_type(filter.ty)? != crate::sema::types::Type::Bool { return Err(failure("comprehension filter changes its original condition contract")); } }
            if value.generators.first().map(|generator| generator.origin.qualifier) != Some(0) || ordinals.iter().copied().ne(0..ordinals.len() as u32) { return Err(failure("comprehension qualifier sequence is incomplete")); }
            for (read, binding) in &value.reads { let binding = value.generators.iter().flat_map(|generator| &generator.bindings).find(|candidate| candidate.identity == *binding).ok_or_else(|| failure("comprehension read has another original binding"))?; if read.ty != binding.ty { return Err(failure("comprehension read changes its checked item type")); } }
            expected_reads.extend(value.reads.iter().enumerate().map(|(read, (root, _))| (root.instruction, index, read)));
            expected_index.push((value.instruction, index));
        }
        expected_reads.sort_unstable_by_key(|entry| entry.0);
        if expected_reads.windows(2).any(|pair| pair[0].0 == pair[1].0) || expected_reads != self.comprehensions.reads { return Err(failure("comprehension read index is stale or ambiguous")); }
        expected_index.sort_unstable_by_key(|entry| entry.0);
        if expected_index.windows(2).any(|pair| pair[0].0 == pair[1].0) || expected_index != self.comprehensions.instructions { return Err(failure("comprehension instruction index is stale or ambiguous")); }
        Ok(())
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_comprehension_mut(&mut self, instruction: u32) -> Result<&mut PreparedComprehension, IrVerifyError> {
        self.comprehension_at(instruction)?.ok_or_else(|| failure("comprehension receipt is missing"))?;
        let position = self.comprehensions.instructions.binary_search_by_key(&instruction, |entry| entry.0).map_err(|_| failure("comprehension receipt is missing"))?;
        Ok(Arc::make_mut(&mut self.comprehensions.entries[self.comprehensions.instructions[position].1].value))
    }
}
