use super::*;
use crate::sema::check::ExpressionIdentity;
use crate::sema::inference::{EffectSet, ScopedRoot};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct ModuleExportParameter {
    pub label: Arc<str>,
    pub ty: Type,
    pub mode: ParameterMode,
    pub defaulted: bool,
    pub rest: bool,
}

/// An export promises a callable shape without naming an implementation in
/// the loading program. The loaded program supplies that implementation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct ModuleExportContract {
    pub kind: CallableKind,
    pub parameters: Box<[ModuleExportParameter]>,
    pub result: Type,
    pub effects: EffectSet,
}

/// The receiver allocation remains distinct from its export schema. A local
/// read names its authored initializer; a captured read names its own header.
#[derive(Clone, Copy, Debug)]
pub(in crate::runtime::eval) enum ModuleReceiverAllocation {
    Local {
        binding: crate::sema::check::BindingIdentity,
        statement: crate::sema::check::StatementIdentity,
        instruction: u32,
        initializer: u32,
        initializer_origin: ExpressionIdentity,
        slot: u32,
        source_type: ScopedRoot,
        initializer_type: ScopedRoot,
    },
    Capture {
        binding: crate::sema::check::BindingIdentity,
        declaration: crate::sema::check::DeclarationIdentity,
        header_index: u32,
        slot: u32,
        name: u32,
        ty: GroundTypeId,
        source_type: ScopedRoot,
    },
    Parameter { declaration: crate::sema::check::DeclarationIdentity, slot: u32 },
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct ModuleInvocationSource {
    pub origin: ExpressionIdentity,
    pub instruction: u32,
    pub owner: InstructionOwner,
    pub callee_origin: ExpressionIdentity,
    pub callee_instruction: u32,
    pub receiver_origin: ExpressionIdentity,
    pub receiver_instruction: u32,
    pub receiver_root: ScopedRoot,
    pub receiver_allocation: ModuleReceiverAllocation,
    pub export_root: ScopedRoot,
    pub result_root: ScopedRoot,
    pub field: Arc<str>,
    pub signature: SignatureId,
    pub contract: ModuleExportContract,
    pub arguments: Box<[PreparedInvocationArgument]>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct ModuleInvocationEvidence {
    program: Option<u64>,
    sources: Vec<Entry<Arc<ModuleInvocationSource>>>,
    originals: Vec<Arc<ModuleInvocationSource>>,
    instructions: Vec<(u32, usize)>,
}

impl ModuleInvocationEvidence {
    pub(super) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        self.sources.capacity() * size_of::<Entry<Arc<ModuleInvocationSource>>>()
            + self.originals.capacity() * size_of::<Arc<ModuleInvocationSource>>()
            + self.instructions.capacity() * size_of::<(u32, usize)>()
            + self.sources.iter().map(|entry| size_of::<ModuleInvocationSource>() + 2 * size_of::<usize>()
                + entry.value.arguments.len() * size_of::<PreparedInvocationArgument>()
                + entry.value.contract.parameters.len() * size_of::<ModuleExportParameter>()
                + entry.value.field.len()
                + entry.value.contract.result.retained_bytes().saturating_sub(size_of::<Type>())
                + entry.value.contract.parameters.iter().map(|parameter| parameter.label.len()
                    + parameter.ty.retained_bytes().saturating_sub(size_of::<Type>())).sum::<usize>()).sum::<usize>()
    }
    pub(super) fn shrink_to_fit(&mut self) { self.sources.shrink_to_fit(); self.originals.shrink_to_fit(); self.instructions.shrink_to_fit(); }
    pub(super) fn checkpoint(&self) -> usize { self.sources.len() }
    pub(super) fn validate_checkpoint(&self, count: usize, serial: u64) -> Result<(), IrVerifyError> {
        if count > self.sources.len() || self.sources.get(count.wrapping_sub(1)).is_some_and(|entry| entry.serial >= serial) { return Err(failure("module invocation checkpoint references retired entries")); }
        Ok(())
    }
    pub(super) fn rewind_validated(&mut self, count: usize) { self.sources.truncate(count); self.originals.truncate(count); self.instructions.clear(); }
    pub(super) fn finish_indexes(&mut self) {
        self.instructions = self.sources.iter().enumerate().map(|(index, entry)| (entry.value.instruction, index)).collect();
        self.instructions.sort_unstable_by_key(|entry| entry.0);
    }
    fn source(&self, root: u64, index: usize) -> Result<&ModuleInvocationSource, IrVerifyError> {
        if self.program != Some(root) { return Err(failure("module invocation belongs to a foreign program")); }
        let source = &self.sources.get(index).ok_or_else(|| failure("module invocation source is missing"))?.value;
        if !self.originals.get(index).is_some_and(|original| Arc::ptr_eq(source, original)) { return Err(failure("module invocation changes its original source receipt")); }
        Ok(source)
    }
}

impl GenericEvidenceStore {
    pub(in crate::runtime::eval) fn module_invocation_source(&self, instruction: u32) -> Result<Option<&ModuleInvocationSource>, IrVerifyError> {
        let Some(index) = self.module_invocations.instructions.binary_search_by_key(&instruction, |entry| entry.0).ok() else { return Ok(None); };
        self.module_invocations.source(self.root, self.module_invocations.instructions[index].1).map(Some)
    }
    pub(in crate::runtime::eval) fn module_invocation_sources(&self) -> impl Iterator<Item = &ModuleInvocationSource> {
        self.module_invocations.sources.iter().map(|entry| entry.value.as_ref())
    }
    pub(super) fn verify_module_invocations(&self, owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        if self.module_invocations.sources.len() != self.module_invocations.originals.len() { return Err(failure("module invocation original ledger is incomplete")); }
        let mut expected = Vec::new();
        for index in 0..self.module_invocations.sources.len() {
            let source = self.module_invocations.source(self.root, index)?;
            if [source.instruction, source.callee_instruction, source.receiver_instruction].iter().any(|&instruction| owners.get(instruction as usize) != Some(&Some(source.owner)))
                || self.registered_instruction_origin(source.instruction, false) != Some((OperationSourceOrigin::Expression(source.origin), source.owner))
                || self.registered_instruction_origin(source.callee_instruction, false) != Some((OperationSourceOrigin::Expression(source.callee_origin), source.owner))
                || self.registered_instruction_origin(source.receiver_instruction, false) != Some((OperationSourceOrigin::Expression(source.receiver_origin), source.owner)) {
                return Err(failure("module invocation changes its original projection or caller"));
            }
            expected.push((source.instruction, index));
            if let ModuleReceiverAllocation::Local { statement, instruction, initializer, initializer_origin, .. } = source.receiver_allocation {
                if owners.get(instruction as usize) != Some(&Some(source.owner)) || owners.get(initializer as usize) != Some(&Some(source.owner))
                    || self.registered_instruction_origin(instruction, false) != Some((OperationSourceOrigin::Statement(statement), source.owner))
                    || self.registered_instruction_origin(initializer, false) != Some((OperationSourceOrigin::Expression(initializer_origin), source.owner)) {
                    return Err(failure("module receiver changes its original binding allocation owner or initializer"));
                }
            }
        }
        expected.sort_unstable_by_key(|entry| entry.0);
        if expected.windows(2).any(|entries| entries[0].0 == entries[1].0) || expected != self.module_invocations.instructions { return Err(failure("module invocation instruction index is invalid")); }
        Ok(())
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_clear_module_invocations(&mut self) { self.module_invocations.sources.clear(); self.module_invocations.originals.clear(); self.module_invocations.instructions.clear(); }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_module_invocation_mut(&mut self, instruction: u32) -> Result<&mut ModuleInvocationSource, IrVerifyError> {
        let index = self.module_invocations.instructions.binary_search_by_key(&instruction, |entry| entry.0).map_err(|_| failure("module invocation source is missing"))?;
        let index = self.module_invocations.instructions[index].1;
        self.module_invocations.source(self.root, index)?;
        Ok(Arc::make_mut(&mut self.module_invocations.sources[index].value))
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_replace_module_invocations(&mut self, other: &Self) { self.module_invocations = other.module_invocations.clone(); }
}

impl GenericEvidenceBuilder {
    pub(in crate::runtime::eval) fn add_module_invocation_source(&mut self, source: ModuleInvocationSource) -> Result<(), IrVerifyError> {
        let serial = self.next_serial;
        self.next_serial = serial.checked_add(1).ok_or_else(|| failure("module invocation serial overflow"))?;
        let source = Arc::new(source);
        self.store.module_invocations.program = Some(self.store.root);
        self.store.module_invocations.originals.push(Arc::clone(&source));
        self.store.module_invocations.sources.push(Entry { serial, value: source });
        Ok(())
    }
    pub(in crate::runtime::eval) fn has_module_invocation_source(&self, instruction: u32) -> Result<bool, IrVerifyError> {
        let matches = self.store.module_invocations.sources.iter().enumerate().filter(|(_, entry)| entry.value.instruction == instruction).map(|(index, _)| index).collect::<Vec<_>>();
        if matches.len() > 1 { return Err(failure("duplicate original module invocation")); }
        if let Some(&index) = matches.first() { self.store.module_invocations.source(self.store.root, index)?; return Ok(true); }
        Ok(false)
    }
}
