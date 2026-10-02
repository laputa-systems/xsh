use super::*;
use crate::sema::check::{BindingIdentity, ExpressionIdentity, StatementIdentity};
use crate::sema::operation_graph::{IterableDomain, PreparedLanguageOperation};
use super::super::IrBlockId;

/// The original selected iterable owns the item slot independently of its reads.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct OriginalIterationBinding {
    pub statement: StatementIdentity,
    pub binding: BindingIdentity,
    pub iterator_origin: ExpressionIdentity,
    pub authority: PreparedOperationAuthority,
    pub instruction: u32,
    pub iterator: u32,
    pub slot: u32,
    pub body: IrBlockId,
    pub owner: InstructionOwner,
    pub input: GroundTypeId,
    pub item: GroundTypeId,
    pub binding_type: GroundTypeId,
    pub iterator_parameter: Option<(crate::sema::check::DeclarationIdentity, u32)>,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct OriginalIterationUse {
    pub origin: ExpressionIdentity,
    pub binding: IterationBindingId,
    pub instruction: u32,
    pub owner: InstructionOwner,
}

#[derive(Clone, Debug, Default)]
pub(super) struct IterationEvidence {
    bindings: Vec<Entry<Arc<OriginalIterationBinding>>>,
    original_bindings: Vec<Arc<OriginalIterationBinding>>,
    uses: Vec<Entry<Arc<OriginalIterationUse>>>,
    original_uses: Vec<Arc<OriginalIterationUse>>,
    use_instructions: Vec<(u32, usize)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct IterationCheckpoint { bindings: usize, uses: usize }

impl IterationEvidence {
    pub(super) fn checkpoint(&self) -> IterationCheckpoint { IterationCheckpoint { bindings: self.bindings.len(), uses: self.uses.len() } }
    pub(super) fn validate_checkpoint(&self, checkpoint: IterationCheckpoint, serial_limit: u64) -> Result<(), IrVerifyError> {
        if checkpoint.bindings > self.bindings.len() || checkpoint.uses > self.uses.len() { return Err(failure("iteration checkpoint references retired entries")); }
        for serial in [self.bindings.get(checkpoint.bindings.wrapping_sub(1)).map(|entry| entry.serial), self.uses.get(checkpoint.uses.wrapping_sub(1)).map(|entry| entry.serial)].into_iter().flatten() {
            if serial >= serial_limit { return Err(failure("iteration checkpoint references replacement entries")); }
        }
        Ok(())
    }
    pub(super) fn rewind_validated(&mut self, checkpoint: IterationCheckpoint) {
        self.bindings.truncate(checkpoint.bindings); self.original_bindings.truncate(checkpoint.bindings);
        self.uses.truncate(checkpoint.uses); self.original_uses.truncate(checkpoint.uses); self.use_instructions.clear();
    }
    pub(super) fn finish(&mut self) {
        self.use_instructions = self.uses.iter().enumerate().map(|(index, entry)| (entry.value.instruction, index)).collect();
        self.use_instructions.sort_unstable_by_key(|entry| entry.0);
    }
    pub(super) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        self.bindings.capacity() * size_of::<Entry<Arc<OriginalIterationBinding>>>() + self.original_bindings.capacity() * size_of::<Arc<OriginalIterationBinding>>()
            + self.bindings.len() * (size_of::<OriginalIterationBinding>() + 2 * size_of::<usize>())
            + self.bindings.iter().map(|entry| entry.value.authority.retained_bytes()).sum::<usize>()
            + self.uses.capacity() * size_of::<Entry<Arc<OriginalIterationUse>>>() + self.original_uses.capacity() * size_of::<Arc<OriginalIterationUse>>()
            + self.uses.len() * (size_of::<OriginalIterationUse>() + 2 * size_of::<usize>()) + self.use_instructions.capacity() * size_of::<(u32, usize)>()
    }
    pub(super) fn shrink_to_fit(&mut self) { self.bindings.shrink_to_fit(); self.original_bindings.shrink_to_fit(); self.uses.shrink_to_fit(); self.original_uses.shrink_to_fit(); self.use_instructions.shrink_to_fit(); }
}

impl GenericEvidenceStore {
    pub fn has_iteration_bindings(&self) -> bool { !self.iterations.bindings.is_empty() || !self.iterations.uses.is_empty() }
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
        let mut seen = std::collections::BTreeSet::new();
        for (id, _) in self.iteration_bindings() {
            let binding = self.iteration_binding(id)?;
            if !seen.insert(binding.binding) || binding.statement.source != binding.binding.source || binding.statement.namespace != binding.binding.namespace
                || binding.iterator_origin.source != binding.statement.source || binding.iterator_origin.namespace != binding.statement.namespace
                || owners.get(binding.instruction as usize) != Some(&Some(binding.owner)) || owners.get(binding.iterator as usize) != Some(&Some(binding.owner))
                || self.registered_instruction_origin(binding.instruction, false) != Some((OperationSourceOrigin::Statement(binding.statement), binding.owner))
                || self.registered_instruction_origin(binding.iterator, false) != Some((OperationSourceOrigin::Expression(binding.iterator_origin), binding.owner)) {
                return Err(failure("iteration binding changes its original source or owner"));
            }
            if !matches!(binding.authority, PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Iteration { domain: IterableDomain::List, outer_result: false }, .. })
                || pools.to_type(binding.input)? != crate::sema::types::Type::List(Box::new(crate::sema::types::Type::Str))
                || pools.to_type(binding.item)? != crate::sema::types::Type::Str || binding.binding_type != binding.item {
                return Err(failure("iteration binding has an unprepared item contract"));
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
    pub(in crate::runtime::eval) fn test_iteration_binding_mut(&mut self, id: IterationBindingId) -> Result<&mut OriginalIterationBinding, IrVerifyError> {
        self.iteration_binding(id)?;
        Ok(Arc::make_mut(&mut self.iterations.bindings[id.index as usize].value))
    }
}

impl GenericEvidenceBuilder {
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
