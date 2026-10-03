use super::*;
use crate::sema::check::NativeBridgeInvocation;
use crate::modules::RuntimeOp;
use crate::sema::types::Type;

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedBridgeCall {
    pub original: Arc<NativeBridgeInvocation>,
    pub instruction: u32,
    pub owner: InstructionOwner,
    pub caller_signature: SignatureId,
    pub signature: SignatureId,
    pub formal: GroundTypeId,
    pub actual: GroundTypeId,
    pub result: GroundTypeId,
    pub operand: u32,
    pub payload: Box<[u32]>,
    pub argument_block: (u32, Box<[u32]>),
}

/// Public arguments and the private policy operand retain separate signatures.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedHashPolicyCall {
    pub origin: crate::sema::check::ExpressionIdentity,
    pub instruction: u32,
    pub owner: InstructionOwner,
    pub caller: Option<crate::sema::check::DeclarationIdentity>,
    pub implementation: crate::sema::check::DeclarationIdentity,
    pub target: Option<IrFunctionId>,
    pub authority: PreparedOperationAuthority,
    pub signature: SignatureId,
    pub implementation_signature: SignatureId,
    pub arguments: Box<[PreparedInvocationArgument]>,
    pub argument_lineages: Box<[PreparedNativeArgumentLineage]>,
    pub algorithm: Name,
    pub algorithm_operand: u32,
    pub algorithm_payload: Box<[u32]>,
    pub payload: Box<[u32]>,
    pub argument_block: (u32, Box<[u32]>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct BridgeCheckpoint { native: usize, hash: usize }

#[derive(Clone, Debug, Default)]
pub(super) struct BridgeEvidence {
    program: Option<u64>,
    entries: Vec<Entry<Arc<PreparedBridgeCall>>>,
    originals: Vec<Arc<PreparedBridgeCall>>,
    instructions: Vec<(u32, usize)>,
    hashes: Vec<Entry<Arc<PreparedHashPolicyCall>>>,
    hash_originals: Vec<Arc<PreparedHashPolicyCall>>,
    hash_instructions: Vec<(u32, usize)>,
    hash_original_instructions: Vec<u32>,
}

impl BridgeEvidence {
    pub(super) fn checkpoint(&self) -> BridgeCheckpoint { BridgeCheckpoint { native: self.entries.len(), hash: self.hashes.len() } }
    pub(super) fn validate_checkpoint(&self, checkpoint: BridgeCheckpoint, serial: u64) -> Result<(), IrVerifyError> {
        let count = checkpoint.native;
        if checkpoint.hash > self.hashes.len() || self.hashes.get(checkpoint.hash.wrapping_sub(1)).is_some_and(|entry| entry.serial >= serial) { return Err(failure("hash bridge checkpoint refers to replaced entries")); }
        if count > self.entries.len() || self.entries.get(count.wrapping_sub(1)).is_some_and(|entry| entry.serial >= serial) { return Err(failure("bridge checkpoint refers to replaced entries")); }
        Ok(())
    }
    pub(super) fn rewind_validated(&mut self, checkpoint: BridgeCheckpoint) { self.entries.truncate(checkpoint.native); self.originals.truncate(checkpoint.native); self.instructions.clear(); self.hashes.truncate(checkpoint.hash); self.hash_originals.truncate(checkpoint.hash); self.hash_instructions.clear(); self.hash_original_instructions.clear(); }
    pub(super) fn finish(&mut self, root: u64) { self.program = Some(root); self.instructions = self.entries.iter().enumerate().map(|(index, entry)| (entry.value.instruction, index)).collect(); self.instructions.sort_unstable(); self.hash_instructions = self.hashes.iter().enumerate().map(|(index, entry)| (entry.value.instruction, index)).collect(); self.hash_instructions.sort_unstable(); self.hash_original_instructions = self.hash_originals.iter().map(|source| source.instruction).collect(); self.hash_original_instructions.sort_unstable(); }
    pub(super) fn shrink_to_fit(&mut self) { self.entries.shrink_to_fit(); self.originals.shrink_to_fit(); self.instructions.shrink_to_fit(); self.hashes.shrink_to_fit(); self.hash_originals.shrink_to_fit(); self.hash_instructions.shrink_to_fit(); self.hash_original_instructions.shrink_to_fit(); }
    pub(super) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        let mut declarations = std::collections::BTreeSet::new();
        let mut invocations = std::collections::BTreeSet::new();
        self.hashes.capacity() * size_of::<Entry<Arc<PreparedHashPolicyCall>>>() + self.hash_originals.capacity() * size_of::<Arc<PreparedHashPolicyCall>>() + self.hash_instructions.capacity() * size_of::<(u32, usize)>() + self.hash_original_instructions.capacity() * size_of::<u32>()
            + self.hash_originals.iter().map(|source| size_of::<PreparedHashPolicyCall>() + (source.payload.len() + source.algorithm_payload.len() + source.argument_block.1.len()) * 4 + source.arguments.len() * size_of::<PreparedInvocationArgument>() + source.argument_lineages.iter().map(PreparedNativeArgumentLineage::retained_bytes).sum::<usize>()).sum::<usize>()
            + self.entries.capacity() * size_of::<Entry<Arc<PreparedBridgeCall>>>() + self.originals.capacity() * size_of::<Arc<PreparedBridgeCall>>()
            + self.instructions.capacity() * size_of::<(u32, usize)>() + self.originals.iter().map(|source| size_of::<PreparedBridgeCall>() + source.payload.len() * 4 + source.argument_block.1.len() * 4
                + if invocations.insert(Arc::as_ptr(&source.original) as usize) { source.original.retained_bytes() } else { 0 }
                + if declarations.insert(Arc::as_ptr(source.original.declaration()) as usize) { source.original.declaration().retained_bytes() } else { 0 }).sum::<usize>()
    }
}

impl GenericEvidenceBuilder {
    pub(in crate::runtime::eval) fn add_bridge_call(&mut self, source: PreparedBridgeCall) -> Result<(), IrVerifyError> {
        if self.store.bridges.entries.len() >= 2_000_000 { return Err(failure("native bridge call limit exceeded")); }
        let serial = self.next_serial;
        self.next_serial = serial.checked_add(1).ok_or_else(|| failure("native bridge call serial overflow"))?;
        let source = Arc::new(source);
        self.store.bridges.program = Some(self.store.root);
        self.store.bridges.originals.push(Arc::clone(&source));
        self.store.bridges.entries.push(Entry { serial, value: source });
        Ok(())
    }
}

impl GenericEvidenceStore {
    pub(in crate::runtime::eval) fn bridge_call(&self, instruction: u32) -> Result<Option<&PreparedBridgeCall>, IrVerifyError> {
        let Ok(index) = self.bridges.instructions.binary_search_by_key(&instruction, |entry| entry.0) else { return Ok(None); };
        let index = self.bridges.instructions[index].1;
        let entry = self.bridges.entries.get(index).ok_or_else(|| failure("native bridge receipt index is invalid"))?;
        if self.bridges.program != Some(self.root) || !self.bridges.originals.get(index).is_some_and(|original| Arc::ptr_eq(original, &entry.value)) { return Err(failure("native bridge receipt is foreign or replaced")); }
        Ok(Some(&entry.value))
    }
    pub(in crate::runtime::eval) fn bridge_calls(&self) -> impl Iterator<Item = &PreparedBridgeCall> { self.bridges.entries.iter().map(|entry| entry.value.as_ref()) }
    pub(super) fn verify_bridge_evidence(&self, pools: &SemanticPools, owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        if self.bridges.program != Some(self.root) { return Err(failure("native bridge evidence has a foreign program owner")); }
        let mut expected = Vec::new();
        for (index, entry) in self.bridges.entries.iter().enumerate() {
            let source = entry.value.as_ref();
            if self.bridges.program != Some(self.root) || !self.bridges.originals.get(index).is_some_and(|original| Arc::ptr_eq(original, &entry.value))
                || owners.get(source.instruction as usize) != Some(&Some(source.owner))
                || self.registered_instruction_origin(source.instruction, false) != Some((OperationSourceOrigin::Expression(source.original.origin()), source.owner)) { return Err(failure("native bridge changes its original program, instruction, or owner")); }
            let original = source.original.declaration();
            let catalog = crate::stdlib::find(original.catalog_identity()).ok_or_else(|| failure("native bridge catalog authority is missing"))?;
            if crate::stdlib::bridge_op(catalog, original.function()) != Some(original.op()) || original.op() != RuntimeOp::BridgeTypeName
                || source.original.origin().namespace != original.declaration().namespace || source.original.origin().source != original.declaration().source
                || source.original.call().declaration != Some(original.declaration())
                || source.original.call().binding.supplied_slots != [0] || !source.original.call().binding.default_slots.is_empty()
                || source.original.call().binding.rest_slot.is_some() || source.original.call().binding.dynamic.is_some()
                || source.original.recipes().len() != 1 || source.original.call().actual_arguments.len() != 1
                || pools.signature_closed_effects(source.signature)? != crate::sema::inference::EffectSet::EMPTY
                || pools.signature_param_count(source.signature)? != 1 || pools.signature_return_type(source.signature)? != source.result
                || pools.signature_param(source.signature, 0)? != (Name::intern("value"), source.formal, 0)
                || pools.signature_parameter_defaulted(source.signature, 0)? || pools.signature_parameter_rest(source.signature, 0)?
                || pools.to_type(source.formal)? != Type::Any || pools.to_type(source.result)? != Type::Str { return Err(failure("native bridge changes its declared Any-to-Str signature or binder")); }
            let caller = source.original.call().caller.ok_or_else(|| failure("native bridge has no original caller"))?;
            let checked = self.checked_function(caller)?;
            if source.owner != InstructionOwner::Function(checked.target) || checked.signature != source.caller_signature
                || caller.namespace != original.declaration().namespace || caller.source != original.declaration().source
                || !self.argument_has_original_source(source.operand, source.original.origin(), 0, &source.original.recipes()[0], source.owner, TypeRef::Ground(source.actual)) { return Err(failure("native bridge operand changes its original caller or recipe")); }
            Self::verify_type(pools, source.actual)?;
            expected.push((source.instruction, index));
        }
        expected.sort_unstable();
        if expected.windows(2).any(|entries| entries[0].0 == entries[1].0) || expected != self.bridges.instructions || self.bridges.originals.len() != self.bridges.entries.len() { return Err(failure("native bridge receipt lookup is incomplete or ambiguous")); }
        self.verify_hash_policy_evidence(pools, owners)?;
        Ok(())
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_remove_bridge_calls(&mut self) { self.bridges.entries.clear(); self.bridges.instructions.clear(); }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_replace_bridge_calls(&mut self, foreign: &Self) { self.bridges = foreign.bridges.clone(); }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_bridge_call_mut(&mut self, instruction: u32) -> &mut PreparedBridgeCall {
        let index = self.bridges.instructions.binary_search_by_key(&instruction, |entry| entry.0).unwrap();
        Arc::make_mut(&mut self.bridges.entries[self.bridges.instructions[index].1].value)
    }
}

impl GenericEvidenceBuilder {
    pub(in crate::runtime::eval) fn add_hash_policy_call(&mut self, source: PreparedHashPolicyCall) -> Result<(), IrVerifyError> {
        if self.store.bridges.hashes.len() >= 2_000_000 { return Err(failure("hash policy call limit exceeded")); }
        let serial = self.next_serial;
        self.next_serial = serial.checked_add(1).ok_or_else(|| failure("hash policy call serial overflow"))?;
        let source = Arc::new(source);
        self.store.bridges.program = Some(self.store.root);
        self.store.bridges.hash_originals.push(Arc::clone(&source));
        self.store.bridges.hashes.push(Entry { serial, value: source });
        Ok(())
    }
}

impl GenericEvidenceStore {
    pub(in crate::runtime::eval) fn hash_policy_call(&self, instruction: u32) -> Result<Option<&PreparedHashPolicyCall>, IrVerifyError> {
        let Ok(index) = self.bridges.hash_instructions.binary_search_by_key(&instruction, |entry| entry.0) else { return Ok(None); };
        let index = self.bridges.hash_instructions[index].1;
        let entry = self.bridges.hashes.get(index).ok_or_else(|| failure("hash policy receipt index is invalid"))?;
        if self.bridges.program != Some(self.root) || !self.bridges.hash_originals.get(index).is_some_and(|original| Arc::ptr_eq(original, &entry.value)) { return Err(failure("hash policy receipt is foreign or replaced")); }
        Ok(Some(&entry.value))
    }
    pub(in crate::runtime::eval) fn hash_policy_instruction_originally_prepared(&self, instruction: u32) -> Result<bool, IrVerifyError> {
        if self.bridges.program != Some(self.root) { return Err(failure("hash policy instruction has a foreign evidence owner")); }
        Ok(self.bridges.hash_original_instructions.binary_search(&instruction).is_ok())
    }
    pub(in crate::runtime::eval) fn hash_policy_calls(&self) -> impl Iterator<Item = &PreparedHashPolicyCall> { self.bridges.hashes.iter().map(|entry| entry.value.as_ref()) }
    fn verify_hash_policy_evidence(&self, pools: &SemanticPools, owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        use crate::modules::signature::{ApiArgCheck, ImplBinding, SemanticRule};
        let mut expected = Vec::new();
        for (index, entry) in self.bridges.hashes.iter().enumerate() {
            let source = entry.value.as_ref();
            if self.bridges.program != Some(self.root) || !self.bridges.hash_originals.get(index).is_some_and(|original| Arc::ptr_eq(original, &entry.value))
                || owners.get(source.instruction as usize) != Some(&Some(source.owner))
                || self.registered_instruction_origin(source.instruction, false) != Some((OperationSourceOrigin::Expression(source.origin), source.owner)) { return Err(failure("hash policy changes its original program, instruction, or owner")); }
            let PreparedOperationAuthority::Registry { operation: RuntimeOp::HashVerifyFile, binding: ImplBinding::Script(script), argument_check: ApiArgCheck::HashVerifyFile, semantic_rule: SemanticRule::Standard, .. } = &source.authority else { return Err(failure("hash policy changes its original selected operation")); };
            if script.module != "hash" || script.function != "verify_file"
                || !matches!(source.algorithm.as_str().as_str(), "md5" | "sha1" | "sha256" | "sha512")
                || source.arguments.len() != 2 || source.argument_lineages.len() != 2
                || pools.signature_param_count(source.signature)? != 2
                || pools.signature_param_count(source.implementation_signature)? != 3
                || pools.signature_closed_effects(source.signature)? != crate::sema::inference::EffectSet::FS
                || pools.signature_closed_effects(source.implementation_signature)? != crate::sema::inference::EffectSet::FS
                || pools.to_type(pools.signature_return_type(source.signature)?)? != Type::Result(Box::new(Type::Unit), Box::new(Type::Error))
                || pools.signature_return_type(source.signature)? != pools.signature_return_type(source.implementation_signature)? { return Err(failure("hash policy changes public or implementation signatures")); }
            for (signature, labels) in [(source.signature, &["path", "sha256"][..]), (source.implementation_signature, &["path", "checksum", "algorithm"][..])] {
                for (slot, label) in labels.iter().enumerate() {
                    let (name, ty, flags) = pools.signature_param(signature, slot)?;
                    let expected_type = if slot == 0 { Type::Path } else { Type::Str };
                    let expected_flags = u8::from(signature == source.signature && slot == 1);
                    if name != *label || pools.to_type(ty)? != expected_type || flags != expected_flags.into() { return Err(failure("hash policy changes formal labels, types, or absence policy")); }
                }
            }
            match (source.caller, source.owner) {
                (None, InstructionOwner::Driver(_)) => {},
                (Some(caller), InstructionOwner::Function(target)) if self.checked_function(caller)?.target == target => {},
                _ => return Err(failure("hash policy changes its original caller")),
            }
            if let Some(target) = source.target {
                let implementation = self.checked_function(source.implementation)?;
                if implementation.target != target || implementation.signature != source.implementation_signature { return Err(failure("hash policy changes its original implementation")); }
            }
            for (ordinal, (argument, lineage)) in source.arguments.iter().zip(&source.argument_lineages).enumerate() {
                if lineage.ordinal as usize != ordinal || lineage.instruction != argument.instruction || lineage.material_type != argument.ty
                    || owners.get(lineage.source_instruction as usize) != Some(&Some(source.owner))
                    || !self.argument_has_original_source(lineage.source_instruction, source.origin, ordinal, &argument.original, source.owner, lineage.source_type)
                    || (ordinal == 0 && argument.original.name.is_some_and(|name| name != "path"))
                    || (ordinal == 1 && argument.original.name != Some(source.algorithm)) { return Err(failure("hash policy changes an original public argument")); }
            }
            expected.push((source.instruction, index));
        }
        expected.sort_unstable();
        if expected.windows(2).any(|entries| entries[0].0 == entries[1].0) || expected != self.bridges.hash_instructions || expected.iter().map(|entry| entry.0).collect::<Vec<_>>() != self.bridges.hash_original_instructions || self.bridges.hash_originals.len() != self.bridges.hashes.len() { return Err(failure("hash policy receipt lookup is incomplete or ambiguous")); }
        Ok(())
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_remove_hash_policy_calls(&mut self) { self.bridges.hashes.clear(); self.bridges.hash_instructions.clear(); }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_replace_hash_policy_calls(&mut self, foreign: &Self) { self.bridges = foreign.bridges.clone(); }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_hash_policy_call_mut(&mut self, instruction: u32) -> &mut PreparedHashPolicyCall {
        let index = self.bridges.hash_instructions.binary_search_by_key(&instruction, |entry| entry.0).unwrap();
        Arc::make_mut(&mut self.bridges.hashes[self.bridges.hash_instructions[index].1].value)
    }
}
