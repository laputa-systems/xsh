use super::*;
use crate::runtime::eval::lower::constructor_prepare::OriginalRecordConstructorPlan;
use crate::runtime::eval::require::PreparedSchema;
use crate::runtime::value::Value;
use crate::sema::check::ExpressionIdentity;
use std::mem::size_of;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) struct RecordConstructorId { index: u32, proof: OwnerProof }

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct RecordConstructorRow {
    pub instruction: u32,
    pub tag: u16,
    pub payload: Box<[u32]>,
    pub block: Option<(u32, Box<[u32]>)>,
    pub text: Option<Arc<str>>,
    pub bytes: Option<Arc<[u8]>>,
    pub constant: Option<Value>,
    pub schema: Option<Arc<PreparedSchema>>,
}

/// Original schema application and declaration defaults survive independently
/// of the flattened row and the compiler's saved argument reads.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedRecordConstructor {
    pub original: OriginalRecordConstructorPlan,
    pub instruction: u32,
    pub record: u32,
    pub validation: u32,
    pub owner: InstructionOwner,
    pub result: GroundTypeId,
    pub fields: Box<[u32]>,
    pub actuals: Box<[GroundTypeId]>,
    pub rows: Box<[RecordConstructorRow]>,
    pub spreads: Box<[RecordConstructorSpread]>,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct RecordConstructorSpread {
    pub original: crate::runtime::eval::lower::constructor_prepare::OriginalRecordConstructorSpread,
    pub record_type: GroundTypeId,
    pub record_initializer: u32,
    pub record_read: u32,
    pub record_wrapper: u32,
    pub field_initializer: u32,
    pub field_read: u32,
    pub field_wrapper: u32,
}

#[derive(Clone, Debug, Default)]
pub(super) struct RecordConstructorEvidence {
    sources: Vec<Entry<Arc<PreparedRecordConstructor>>>,
    originals: Vec<Arc<PreparedRecordConstructor>>,
    instructions: Vec<(u32, RecordConstructorId)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RecordConstructorCheckpoint { sources: usize }

impl RecordConstructorEvidence {
    pub(super) fn checkpoint(&self) -> RecordConstructorCheckpoint { RecordConstructorCheckpoint { sources: self.sources.len() } }
    pub(super) fn validate_checkpoint(&self, checkpoint: RecordConstructorCheckpoint, serial_limit: u64) -> Result<(), IrVerifyError> {
        if checkpoint.sources > self.sources.len() || self.sources.get(checkpoint.sources.wrapping_sub(1)).is_some_and(|entry| entry.serial >= serial_limit) {
            return Err(failure("record constructor checkpoint references retired or replaced receipts"));
        }
        Ok(())
    }
    pub(super) fn rewind_validated(&mut self, checkpoint: RecordConstructorCheckpoint) {
        self.sources.truncate(checkpoint.sources); self.originals.truncate(checkpoint.sources); self.instructions.clear();
    }
    pub(super) fn finish(&mut self, root: u64) {
        self.instructions = self.sources.iter().enumerate().map(|(index, entry)| (entry.value.instruction,
            RecordConstructorId { index: index as u32, proof: OwnerProof { root, serial: entry.serial } })).collect();
        self.instructions.sort_unstable_by_key(|entry| entry.0);
    }
    pub(super) fn shrink_to_fit(&mut self) { self.sources.shrink_to_fit(); self.originals.shrink_to_fit(); self.instructions.shrink_to_fit(); }
    pub(super) fn retained_bytes(&self) -> usize {
        self.sources.capacity() * size_of::<Entry<Arc<PreparedRecordConstructor>>>()
            + self.originals.capacity() * size_of::<Arc<PreparedRecordConstructor>>()
            + self.instructions.capacity() * size_of::<(u32, RecordConstructorId)>()
            + self.sources.iter().map(|entry| {
                let source = &entry.value;
                size_of::<PreparedRecordConstructor>() + 2 * size_of::<usize>()
                    + source.fields.len() * size_of::<u32>() + source.actuals.len() * size_of::<GroundTypeId>()
                    + source.original.parameters.len() * size_of::<(Name, crate::sema::types::Type)>()
                    + source.original.parameters.iter().map(|(_, ty)| ty.retained_bytes().saturating_sub(size_of::<crate::sema::types::Type>())).sum::<usize>()
                    + source.original.recipes.len() * size_of::<crate::sema::check::SolvedArgumentSource>()
                    + source.original.defaults.len() * size_of::<(usize, crate::sema::check::ConstructorDefaultIdentity, ExpressionIdentity, crate::sema::constants::LiteralConstant)>()
                    + source.original.application.parameters.capacity() * size_of::<crate::sema::check::SolvedConstructorParameter>()
                    + source.original.application.supplied.capacity() * size_of::<crate::sema::check::SolvedConstructorArgument>()
                    + source.original.application.default_slots.capacity() * size_of::<usize>()
                    + original_constructor_payload_bytes(&source.original)
                    + source.rows.len() * size_of::<RecordConstructorRow>()
                    + source.spreads.len() * size_of::<RecordConstructorSpread>()
                    + source.rows.iter().map(|row| row.payload.len() * size_of::<u32>() + row.block.as_ref().map_or(0, |(_, words)| words.len() * size_of::<u32>())
                        + row.text.as_ref().map_or(0, |text| text.len()) + row.bytes.as_ref().map_or(0, |bytes| bytes.len())
                        + row.schema.as_ref().map_or(0, |schema| schema.retained_bytes())).sum::<usize>()
            }).sum::<usize>()
    }
}

fn original_constructor_payload_bytes(original: &OriginalRecordConstructorPlan) -> usize {
    use crate::sema::check::{ConstructorAuthority, SolvedSchemaApplication, SolvedSchemaExpectation};
    use crate::sema::constants::{LiteralConstant, SchemaComponent};
    let mut bytes = match &original.application.authority {
        ConstructorAuthority::Record { application, .. } => application.arguments.capacity() * size_of::<crate::sema::inference::TypeId>(),
        ConstructorAuthority::Nominal(_) => 0,
    };
    let mut contexts = vec![&original.application.expectation];
    while let Some(context) = contexts.pop() {
        bytes += context.applications.capacity() * size_of::<SolvedSchemaApplication>()
            + context.children.len() * (size_of::<SchemaComponent>() + size_of::<SolvedSchemaExpectation>() + 3 * size_of::<usize>())
            + context.applications.iter().map(|application| application.arguments.capacity() * size_of::<crate::sema::inference::TypeId>()).sum::<usize>();
        contexts.extend(context.children.values());
    }
    let mut allocations = std::collections::BTreeSet::new();
    let mut literals = original.defaults.iter().map(|(_, _, _, value)| value).collect::<Vec<_>>();
    while let Some(literal) = literals.pop() {
        match literal {
            LiteralConstant::Str(value) | LiteralConstant::Path(value) => {
                if allocations.insert(Arc::as_ptr(value) as *const u8 as usize) { bytes += value.len() + 2 * size_of::<usize>(); }
            }
            LiteralConstant::Bytes(value) => {
                if allocations.insert(Arc::as_ptr(value) as *const u8 as usize) { bytes += value.len() + 2 * size_of::<usize>(); }
            }
            LiteralConstant::List(values) | LiteralConstant::Tag { fields: values, .. } => {
                if allocations.insert(Arc::as_ptr(values) as usize) {
                    bytes += size_of::<Vec<LiteralConstant>>() + values.capacity() * size_of::<LiteralConstant>() + 2 * size_of::<usize>();
                    literals.extend(values.iter());
                }
            }
            LiteralConstant::Record(values) => {
                if allocations.insert(Arc::as_ptr(values) as usize) {
                    bytes += size_of::<std::collections::BTreeMap<Name, LiteralConstant>>()
                        + values.len() * (size_of::<(Name, LiteralConstant)>() + 3 * size_of::<usize>()) + 2 * size_of::<usize>();
                    literals.extend(values.values());
                }
            }
            LiteralConstant::Map(values) => {
                if allocations.insert(Arc::as_ptr(values) as usize) {
                    bytes += size_of::<std::collections::BTreeMap<crate::map_key::MapKey, LiteralConstant>>()
                        + values.len() * (size_of::<(crate::map_key::MapKey, LiteralConstant)>() + 3 * size_of::<usize>()) + 2 * size_of::<usize>();
                    literals.extend(values.values());
                    for key in values.keys() {
                        let allocation = match key {
                            crate::map_key::MapKey::Str(text) => Some((Arc::as_ptr(text) as *const u8 as usize, text.len())),
                            crate::map_key::MapKey::Bytes(bytes) | crate::map_key::MapKey::Path(bytes) => Some((Arc::as_ptr(bytes) as *const u8 as usize, bytes.len())),
                            _ => None,
                        };
                        if let Some((pointer, length)) = allocation && allocations.insert(pointer) { bytes += length + 2 * size_of::<usize>(); }
                    }
                }
            }
            _ => {}
        }
    }
    bytes
}

impl GenericEvidenceStore {
    pub fn record_constructor(&self, id: RecordConstructorId) -> Result<&PreparedRecordConstructor, IrVerifyError> {
        let source = owned(self.root, &self.record_constructors.sources, id.index, id.proof)?;
        if !self.record_constructors.originals.get(id.index as usize).is_some_and(|original| Arc::ptr_eq(source, original)) {
            return Err(failure("record constructor differs from its original receipt"));
        }
        Ok(source)
    }
    pub fn record_constructors(&self) -> impl Iterator<Item = (RecordConstructorId, &PreparedRecordConstructor)> {
        self.record_constructors.sources.iter().enumerate().map(|(index, entry)| (RecordConstructorId {
            index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial },
        }, entry.value.as_ref()))
    }
    pub fn record_constructor_at(&self, instruction: u32) -> Result<Option<&PreparedRecordConstructor>, IrVerifyError> {
        self.record_constructors.instructions.binary_search_by_key(&instruction, |entry| entry.0).ok()
            .map(|index| self.record_constructor(self.record_constructors.instructions[index].1)).transpose()
    }
    pub(super) fn verify_record_constructor_evidence(&self, pools: &SemanticPools, owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        if self.record_constructors.sources.len() != self.record_constructors.originals.len() { return Err(failure("record constructor original receipt ledger is incomplete")); }
        let mut expected = Vec::new();
        for (id, _) in self.record_constructors() {
            let source = self.record_constructor(id)?;
            if owners.get(source.instruction as usize) != Some(&Some(source.owner))
                || self.registered_instruction_origin(source.instruction, false) != Some((OperationSourceOrigin::Expression(source.original.origin), source.owner))
                || source.rows.iter().any(|row| owners.get(row.instruction as usize) != Some(&Some(source.owner))) {
                return Err(failure("record constructor changes its original instruction or owner"));
            }
            let crate::sema::check::ConstructorAuthority::Record { application, origin } = &source.original.application.authority else { return Err(failure("record constructor changes its original authority")); };
            if source.original.application.expectation.applications.first() != Some(application)
                || !source.original.application.expectation.applications.iter().any(|application| application.declaration == *origin)
                || source.original.application.requirement.is_some()
                || pools.to_type(source.result)? != crate::sema::types::Type::Record(source.original.parameters.iter().cloned().collect())
                || source.fields.len() != source.original.parameters.len() || source.actuals.len() != source.original.application.supplied.len() {
                return Err(failure("record constructor changes its original schema application or field contract"));
            }
            expected.push((source.instruction, id));
        }
        expected.sort_unstable_by_key(|entry| entry.0);
        if expected.windows(2).any(|pair| pair[0].0 == pair[1].0) || expected != self.record_constructors.instructions {
            return Err(failure("record constructor instruction index is incomplete or ambiguous"));
        }
        Ok(())
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_record_constructor_mut(&mut self, id: RecordConstructorId) -> Result<&mut PreparedRecordConstructor, IrVerifyError> {
        self.record_constructor(id)?; Ok(Arc::make_mut(&mut self.record_constructors.sources[id.index as usize].value))
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_remove_record_constructors(&mut self) { self.record_constructors.sources.clear(); self.record_constructors.instructions.clear(); }
}

impl GenericEvidenceBuilder {
    pub fn add_record_constructor(&mut self, source: PreparedRecordConstructor) -> Result<RecordConstructorId, IrVerifyError> {
        if self.store.record_constructors.sources.len() >= 2_000_000 || source.fields.len() > 65536 || source.rows.len() > 65536 { return Err(failure("record constructors exceed their work limit")); }
        let index = u32::try_from(self.store.record_constructors.sources.len()).map_err(|_| failure("record constructor id overflow"))?;
        let serial = self.next_serial;
        self.next_serial = serial.checked_add(1).ok_or_else(|| failure("record constructor serial overflow"))?;
        let source = Arc::new(source);
        self.store.record_constructors.originals.push(Arc::clone(&source));
        self.store.record_constructors.sources.push(Entry { serial, value: source });
        Ok(RecordConstructorId { index, proof: OwnerProof { root: self.store.root, serial } })
    }
}
