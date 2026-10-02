use super::*;
use crate::sema::check::ExpressionIdentity;
use crate::sema::operation_graph::PreparedLanguageOperation;
use crate::sema::types::Type;

/// The authored operands remain independent of the encoded operation packet.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct OriginalIndex {
    pub origin: ExpressionIdentity,
    pub base_origin: ExpressionIdentity,
    pub index_origin: ExpressionIdentity,
    pub requirement: crate::sema::inference::RequirementId,
    pub operation: OperationId,
    pub instruction: u32,
    pub base: u32,
    pub index: u32,
    pub index_material: u32,
    pub uint_key_validation: Option<(u32, Box<[u32]>)>,
    pub owner: InstructionOwner,
    pub base_parameter: Option<(crate::sema::check::DeclarationIdentity, u32)>,
    pub index_parameter: Option<(crate::sema::check::DeclarationIdentity, u32)>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct IndexEvidence {
    program: Option<u64>,
    receipts: Vec<Entry<Arc<OriginalIndex>>>,
    originals: Vec<Arc<OriginalIndex>>,
    instructions: Vec<(u32, usize)>,
}

impl IndexEvidence {
    pub(super) fn checkpoint(&self) -> usize { self.receipts.len() }
    pub(super) fn validate_checkpoint(&self, count: usize, serial: u64) -> Result<(), IrVerifyError> {
        if count > self.receipts.len() || self.receipts.get(count.wrapping_sub(1)).is_some_and(|entry| entry.serial >= serial) { return Err(failure("index checkpoint references retired or replacement entries")); }
        Ok(())
    }
    pub(super) fn rewind_validated(&mut self, count: usize) { self.receipts.truncate(count); self.originals.truncate(count); self.instructions.clear(); }
    pub(super) fn finish_indexes(&mut self) {
        self.instructions = self.receipts.iter().enumerate().map(|(index, entry)| (entry.value.instruction, index)).collect();
        self.instructions.sort_unstable_by_key(|entry| entry.0);
    }
    pub(super) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        self.receipts.capacity() * size_of::<Entry<Arc<OriginalIndex>>>() + self.originals.capacity() * size_of::<Arc<OriginalIndex>>()
            + self.instructions.capacity() * size_of::<(u32, usize)>() + self.receipts.len() * (size_of::<OriginalIndex>() + 2 * size_of::<usize>())
            + self.receipts.iter().filter_map(|entry| entry.value.uint_key_validation.as_ref()).map(|(_, payload)| payload.len() * size_of::<u32>()).sum::<usize>()
    }
    pub(super) fn shrink_to_fit(&mut self) { self.receipts.shrink_to_fit(); self.originals.shrink_to_fit(); self.instructions.shrink_to_fit(); }
    fn receipt(&self, index: usize) -> Result<&OriginalIndex, IrVerifyError> {
        let receipt = &self.receipts.get(index).ok_or_else(|| failure("original index is missing"))?.value;
        if !self.originals.get(index).is_some_and(|original| Arc::ptr_eq(original, receipt)) { return Err(failure("index changes its original prepared receipt")); }
        Ok(receipt)
    }
}

impl GenericEvidenceStore {
    pub(in crate::runtime::eval) fn supports_ground_index(map: bool, base: &Type, index: &Type, result: &Type) -> bool {
        if !Self::supports_list_index_item(result) { return false; }
        match (map, base) {
            (false, Type::List(item)) => **item == *result && matches!(index, Type::Int | Type::UInt),
            (true, Type::Map(key, item)) => **item == *result && key.is_map_key()
                && (**key == *index || **key == Type::UInt && *index == Type::Int),
            _ => false,
        }
    }
    /// Index receipts preserve plain value items; streams and resource handles require their own ownership proof.
    pub(in crate::runtime::eval) fn supports_list_index_item(ty: &Type) -> bool {
        match ty {
            Type::Null | Type::Bool | Type::Int | Type::UInt | Type::Float | Type::Duration | Type::Str | Type::Bytes
            | Type::Digest | Type::Regex | Type::Path | Type::Unit | Type::Status | Type::EnvPathList | Type::Error
            | Type::ProcessError | Type::Tag(_) | Type::ErrorFamily(_) | Type::ErrorVariant { .. } | Type::ErrorFacet(_) => true,
            Type::Optional(item) | Type::List(item) => Self::supports_list_index_item(item),
            Type::Map(key, item) | Type::Result(key, item) => Self::supports_list_index_item(key) && Self::supports_list_index_item(item),
            Type::Record(fields) => fields.values().all(Self::supports_list_index_item),
            _ => false,
        }
    }
    pub(in crate::runtime::eval) fn original_index(&self, instruction: u32) -> Result<Option<&OriginalIndex>, IrVerifyError> {
        self.indices.instructions.binary_search_by_key(&instruction, |entry| entry.0).ok().map(|index| self.indices.receipt(self.indices.instructions[index].1)).transpose()
    }
    pub(in crate::runtime::eval) fn original_indices(&self) -> impl Iterator<Item = &OriginalIndex> { self.indices.receipts.iter().map(|entry| entry.value.as_ref()) }
    pub(in crate::runtime::eval) fn verify_index_operation_contract(pools: &SemanticPools, operation: &PreparedOperation) -> Result<(), IrVerifyError> {
        let PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Index { map }, argument_order: crate::sema::operation_graph::OperationArgumentOrder::SourceOrder, statement_result_is_unit: false, .. } = operation.authority else { return Err(failure("selected index operation has another authority")); };
        if operation.receiver.is_some()
            || operation.arguments.len() != 2 || operation.binding.supplied_slots.as_ref() != [0, 1]
            || !operation.binding.default_slots.is_empty() || operation.binding.rest_slot.is_some() || operation.binding.dynamic.is_some()
            || operation.binding.operands.len() != 2 || operation.fallback_lowering.is_some()
            || operation.effects.creation != crate::sema::inference::EffectSet::EMPTY || !operation.effects.inputs.is_empty() || !operation.effects.outputs.is_empty() {
            return Err(failure("selected List index operation has another authority, argument packet, or effect contract"));
        }
        let [Some(TypeRef::Ground(base)), Some(TypeRef::Ground(index))] = operation.arguments.as_ref() else { return Err(failure("selected List index operands are not ground")); };
        let TypeRef::Ground(result) = operation.result else { return Err(failure("selected List index result is not ground")); };
        if !Self::supports_ground_index(map, &pools.to_type(*base)?, &pools.to_type(*index)?, &pools.to_type(result)?) {
            if !Self::supports_list_index_item(&pools.to_type(result)?) { return Err(failure("selected index lacks a plain value item ownership contract")); }
            return Err(failure("selected index changes its original operand, key domain, or item relationship"));
        }
        if !Self::supports_list_index_item(&pools.to_type(result)?) { return Err(failure("selected List index lacks a plain value item ownership contract")); }
        Ok(())
    }
    pub(super) fn verify_original_indices(&self, pools: &SemanticPools, owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        let evidence = &self.indices;
        if !evidence.receipts.is_empty() && evidence.program != Some(self.root) { return Err(failure("original index belongs to a foreign program")); }
        if evidence.receipts.len() != evidence.originals.len() { return Err(failure("original index ledger is incomplete")); }
        let mut expected = Vec::with_capacity(evidence.receipts.len());
        for index in 0..evidence.receipts.len() {
            let original = evidence.receipt(index)?;
            let operation = self.operation(original.operation)?;
            let source = self.operation_source(operation.source)?;
            Self::verify_index_operation_contract(pools, operation)?;
            if source.origin != OperationSourceOrigin::Expression(original.origin) || source.instruction != original.instruction || source.owner != original.owner
                || operation.binding.operands.as_ref() != [original.base, original.index] {
                return Err(failure("index changes its original selected operation or operand packet"));
            }
            for (instruction, origin) in [(original.instruction, original.origin), (original.base, original.base_origin), (original.index_material, original.index_origin)] {
                if origin.source != original.origin.source || origin.namespace != original.origin.namespace
                    || owners.get(instruction as usize) != Some(&Some(original.owner))
                    || self.registered_instruction_origin(instruction, false) != Some((OperationSourceOrigin::Expression(origin), original.owner)) {
                    return Err(failure("index changes its original source operand or lexical owner"));
                }
            }
            if owners.get(original.index as usize) != Some(&Some(original.owner))
                || original.uint_key_validation.as_ref().is_some_and(|(instruction, _)| owners.get(*instruction as usize) != Some(&Some(original.owner))) {
                return Err(failure("index validation changes its lexical owner"));
            }
            expected.push((original.instruction, index));
        }
        expected.sort_unstable_by_key(|entry| entry.0);
        if expected.windows(2).any(|pair| pair[0].0 == pair[1].0) || expected != evidence.instructions { return Err(failure("original index lookup is incomplete or ambiguous")); }
        for (_, operation) in self.operations() {
            if matches!(operation.authority, PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Index { .. }, .. })
                && self.original_index(self.operation_source(operation.source)?.instruction)?.is_none() {
                return Err(failure("prepared index lacks its original operand authority"));
            }
        }
        Ok(())
    }
}

impl GenericEvidenceBuilder {
    pub(in crate::runtime::eval) fn add_original_index(&mut self, original: OriginalIndex) -> Result<(), IrVerifyError> {
        if self.store.indices.receipts.len() >= 2_000_000 { return Err(failure("original index limit exceeded")); }
        let serial = self.next_serial;
        self.next_serial = serial.checked_add(1).ok_or_else(|| failure("original index serial overflow"))?;
        let original = Arc::new(original);
        self.store.indices.program = Some(self.store.root);
        self.store.indices.originals.push(Arc::clone(&original));
        self.store.indices.receipts.push(Entry { serial, value: original });
        Ok(())
    }
}

#[cfg(test)]
impl GenericEvidenceStore {
    pub(in crate::runtime::eval) fn test_original_index_mut(&mut self, instruction: u32) -> Result<&mut OriginalIndex, IrVerifyError> {
        let index = self.indices.instructions.binary_search_by_key(&instruction, |entry| entry.0).map_err(|_| failure("original index is missing"))?;
        let index = self.indices.instructions[index].1;
        Ok(Arc::make_mut(&mut self.indices.receipts[index].value))
    }
    pub(in crate::runtime::eval) fn test_remove_original_indices(&mut self) { self.indices = IndexEvidence::default(); }
    pub(in crate::runtime::eval) fn test_replace_original_indices(&mut self, other: &Self) { self.indices = other.indices.clone(); }
}
