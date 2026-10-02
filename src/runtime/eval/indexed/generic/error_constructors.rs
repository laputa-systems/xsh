use super::*;
use crate::runtime::eval::lower::error_constructor::OriginalErrorConstructorPlan;
use crate::sema::check::{ConstructorAuthority, NominalDeclaration, QualifiedNominalIdentity};
use crate::sema::types::Type;

/// The immutable receipt retains canonical declaration metadata independently of encoded strings and fields.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedErrorConstructor {
    pub original: OriginalErrorConstructorPlan,
    pub instruction: u32,
    pub owner: InstructionOwner,
    pub result: GroundTypeId,
    pub parameters: Box<[GroundTypeId]>,
    pub fields: Box<[u32]>,
    pub actuals: Box<[GroundTypeId]>,
    pub payload: Box<[u32]>,
    pub field_block: (u32, Box<[u32]>),
    pub facet_block: (u32, Box<[u32]>),
    pub checked_fields: Box<[(u32, Box<[u32]>)]>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct ErrorConstructorEvidence {
    program: Option<u64>,
    receipts: Vec<Entry<Arc<PreparedErrorConstructor>>>,
    originals: Vec<Arc<PreparedErrorConstructor>>,
    instructions: Vec<(u32, usize)>,
}

impl ErrorConstructorEvidence {
    pub(super) fn checkpoint(&self) -> usize { self.receipts.len() }
    pub(super) fn validate_checkpoint(&self, count: usize, serial: u64) -> Result<(), IrVerifyError> {
        if count > self.receipts.len() || self.receipts.get(count.wrapping_sub(1)).is_some_and(|entry| entry.serial >= serial) { return Err(failure("error constructor checkpoint references retired or replacement entries")); }
        Ok(())
    }
    pub(super) fn rewind_validated(&mut self, count: usize) { self.receipts.truncate(count); self.originals.truncate(count); self.instructions.clear(); }
    pub(super) fn finish_indexes(&mut self) {
        self.instructions = self.receipts.iter().enumerate().map(|(index, entry)| (entry.value.instruction, index)).collect();
        self.instructions.sort_unstable_by_key(|entry| entry.0);
    }
    pub(super) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        self.receipts.capacity() * size_of::<Entry<Arc<PreparedErrorConstructor>>>() + self.originals.capacity() * size_of::<Arc<PreparedErrorConstructor>>()
            + self.instructions.capacity() * size_of::<(u32, usize)>()
            + self.receipts.iter().map(|entry| {
                let value = &entry.value;
                size_of::<PreparedErrorConstructor>() + 2 * size_of::<usize>()
                    + (value.parameters.len() + value.actuals.len()) * size_of::<GroundTypeId>() + value.fields.len() * size_of::<u32>()
                    + (value.payload.len() + value.field_block.1.len() + value.facet_block.1.len()) * size_of::<u32>()
                    + value.original.parameters.len() * size_of::<(Name, Type)>()
                    + value.original.parameters.iter().map(|(_, ty)| ty.retained_bytes().saturating_sub(size_of::<Type>())).sum::<usize>()
                    + value.original.facets.len() * size_of::<Name>() + value.original.recipes.len() * size_of::<crate::sema::check::SolvedArgumentSource>()
                    + value.original.application.parameters.capacity() * size_of::<crate::sema::check::SolvedConstructorParameter>()
                    + value.original.application.supplied.capacity() * size_of::<crate::sema::check::SolvedConstructorArgument>()
                    + value.original.application.default_slots.capacity() * size_of::<usize>()
                    + value.checked_fields.len() * size_of::<(u32, Box<[u32]>)>() + value.checked_fields.iter().map(|(_, payload)| payload.len() * size_of::<u32>()).sum::<usize>()
            }).sum::<usize>()
    }
    pub(super) fn shrink_to_fit(&mut self) { self.receipts.shrink_to_fit(); self.originals.shrink_to_fit(); self.instructions.shrink_to_fit(); }
    fn receipt(&self, index: usize) -> Result<&PreparedErrorConstructor, IrVerifyError> {
        let receipt = &self.receipts.get(index).ok_or_else(|| failure("original error constructor is missing"))?.value;
        if !self.originals.get(index).is_some_and(|original| Arc::ptr_eq(original, receipt)) { return Err(failure("error constructor changes its original receipt")); }
        Ok(receipt)
    }
}

impl GenericEvidenceStore {
    pub(in crate::runtime::eval) fn error_constructor_at(&self, instruction: u32) -> Result<Option<&PreparedErrorConstructor>, IrVerifyError> {
        self.error_constructors.instructions.binary_search_by_key(&instruction, |entry| entry.0).ok()
            .map(|index| self.error_constructors.receipt(self.error_constructors.instructions[index].1)).transpose()
    }
    pub(in crate::runtime::eval) fn error_constructors(&self) -> impl Iterator<Item = &PreparedErrorConstructor> { self.error_constructors.receipts.iter().map(|entry| entry.value.as_ref()) }
    pub(super) fn verify_error_constructor_evidence(&self, pools: &SemanticPools, owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        let evidence = &self.error_constructors;
        if !evidence.receipts.is_empty() && evidence.program != Some(self.root) { return Err(failure("error constructor belongs to a foreign program")); }
        if evidence.receipts.len() != evidence.originals.len() { return Err(failure("error constructor original receipt ledger is incomplete")); }
        let mut expected = Vec::with_capacity(evidence.receipts.len());
        for index in 0..evidence.receipts.len() {
            let source = evidence.receipt(index)?;
            let original = &source.original;
            if !matches!(original.authority, QualifiedNominalIdentity::Source { declaration: NominalDeclaration::Error(_), member: Some(member), .. } if member == original.member)
                || !matches!(original.application.authority, ConstructorAuthority::Nominal(authority) if authority == original.authority)
                || original.application.requirement.is_none() || !original.application.default_slots.is_empty()
                || original.application.parameters.iter().any(|parameter| parameter.default.is_some())
                || source.fields.len() != original.parameters.len() || source.parameters.len() != original.parameters.len()
                || source.actuals.len() != original.application.supplied.len()
                || pools.to_type(source.result)? != original.checked_type
                || !matches!(original.checked_type, Type::ErrorVariant { variant, .. } if variant == original.member)
                || owners.get(source.instruction as usize) != Some(&Some(source.owner))
                || self.registered_instruction_origin(source.instruction, false) != Some((OperationSourceOrigin::Expression(original.origin), source.owner)) {
                return Err(failure("error constructor changes its original declaration, instruction, or field contract"));
            }
            let mut occupied = std::collections::BTreeSet::new();
            if original.recipes.len() != original.application.supplied.len() { return Err(failure("error constructor original recipes are incomplete")); }
            for (ordinal, (argument, recipe)) in original.application.supplied.iter().zip(original.recipes.iter()).enumerate() {
                if !occupied.insert(argument.slot) || source.fields.get(argument.slot).is_none() { return Err(failure("error constructor changes its original field slots")); }
                let field = source.fields[argument.slot];
                if let Some(saved) = self.original_argument_binding(field) {
                    if saved.call != original.origin || saved.owner != source.owner || saved.ordinal as usize != ordinal || saved.recipe != *recipe {
                        return Err(failure("error constructor changes its original saved operand recipe"));
                    }
                }
                if owners.get(field as usize) != Some(&Some(source.owner)) { return Err(failure("error constructor field belongs to another owner")); }
            }
            if occupied.len() != original.parameters.len() { return Err(failure("error constructor lacks its original supplied fields")); }
            for (((name, ty), &prepared), parameter) in original.parameters.iter().zip(source.parameters.iter()).zip(&original.application.parameters) {
                if parameter.label != Some(*name) || pools.to_type(prepared)? != *ty { return Err(failure("error constructor field type differs from its original formal")); }
            }
            expected.push((source.instruction, index));
        }
        expected.sort_unstable_by_key(|entry| entry.0);
        if expected.windows(2).any(|pair| pair[0].0 == pair[1].0) || expected != evidence.instructions { return Err(failure("error constructor lookup is incomplete or ambiguous")); }
        Ok(())
    }
}

impl GenericEvidenceBuilder {
    pub(in crate::runtime::eval) fn add_error_constructor(&mut self, source: PreparedErrorConstructor) -> Result<(), IrVerifyError> {
        if self.store.error_constructors.receipts.len() >= 2_000_000 { return Err(failure("error constructor limit exceeded")); }
        let serial = self.next_serial;
        self.next_serial = serial.checked_add(1).ok_or_else(|| failure("error constructor serial overflow"))?;
        let source = Arc::new(source);
        self.store.error_constructors.program = Some(self.store.root);
        self.store.error_constructors.originals.push(Arc::clone(&source));
        self.store.error_constructors.receipts.push(Entry { serial, value: source });
        Ok(())
    }
}

#[cfg(test)]
impl GenericEvidenceStore {
    pub(in crate::runtime::eval) fn test_remove_error_constructors(&mut self) { self.error_constructors.receipts.clear(); self.error_constructors.instructions.clear(); }
    pub(in crate::runtime::eval) fn test_replace_error_constructors(&mut self, other: &Self) { self.error_constructors = other.error_constructors.clone(); }
    pub(in crate::runtime::eval) fn test_error_constructor_mut(&mut self, instruction: u32) -> Result<&mut PreparedErrorConstructor, IrVerifyError> {
        let index = self.error_constructors.instructions.binary_search_by_key(&instruction, |entry| entry.0).map_err(|_| failure("original error constructor is missing"))?;
        let index = self.error_constructors.instructions[index].1;
        Ok(Arc::make_mut(&mut self.error_constructors.receipts[index].value))
    }
}
