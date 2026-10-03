use super::*;
use crate::runtime::eval::lower::tag_constructor::OriginalTagConstructorPlan;
use crate::sema::check::{ConstructorAuthority, NominalDeclaration, QualifiedNominalIdentity};
use crate::sema::types::Type;

/// The immutable receipt retains canonical declaration metadata independently of encoded strings and fields.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedTagConstructor {
    pub original: OriginalTagConstructorPlan,
    pub instruction: u32,
    pub owner: InstructionOwner,
    pub result: GroundTypeId,
    pub parameters: Box<[GroundTypeId]>,
    pub fields: Box<[u32]>,
    pub actuals: Box<[TypeRef]>,
    pub scope: Option<SchemeScopeId>,
    pub requirement: Option<u32>,
    pub scoped_requirement: Option<ScopedTagConstructorRequirement>,
    pub payload: Box<[u32]>,
    pub field_block: (u32, Box<[u32]>),
    pub argument_wrappers: Box<[u32]>,
    pub checked_fields: Box<[(u32, Box<[u32]>)]>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct TagConstructorEvidence {
    program: Option<u64>,
    receipts: Vec<Entry<Arc<PreparedTagConstructor>>>,
    originals: Vec<Arc<PreparedTagConstructor>>,
    instructions: Vec<(u32, usize)>,
    wrappers: Vec<(u32, u32)>,
}

impl TagConstructorEvidence {
    pub(super) fn checkpoint(&self) -> usize { self.receipts.len() }
    pub(super) fn validate_checkpoint(&self, count: usize, serial: u64) -> Result<(), IrVerifyError> {
        if count > self.receipts.len() || self.receipts.get(count.wrapping_sub(1)).is_some_and(|entry| entry.serial >= serial) { return Err(failure("tag constructor checkpoint references retired or replacement entries")); }
        Ok(())
    }
    pub(super) fn rewind_validated(&mut self, count: usize) { self.receipts.truncate(count); self.originals.truncate(count); self.instructions.clear(); self.wrappers.clear(); }
    pub(super) fn finish_indexes(&mut self) {
        self.instructions = self.receipts.iter().enumerate().map(|(index, entry)| (entry.value.instruction, index)).collect();
        self.instructions.sort_unstable_by_key(|entry| entry.0);
        self.wrappers = self.originals.iter().flat_map(|source| source.argument_wrappers.iter().map(move |&wrapper| (wrapper, source.instruction))).collect();
        self.wrappers.sort_unstable_by_key(|entry| entry.0);
    }
    pub(super) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        self.receipts.capacity() * size_of::<Entry<Arc<PreparedTagConstructor>>>() + self.originals.capacity() * size_of::<Arc<PreparedTagConstructor>>()
            + self.instructions.capacity() * size_of::<(u32, usize)>() + self.wrappers.capacity() * size_of::<(u32, u32)>()
            + self.receipts.iter().map(|entry| {
                let value = &entry.value;
                size_of::<PreparedTagConstructor>() + 2 * size_of::<usize>()
                    + value.scoped_requirement.as_ref().map_or(0, ScopedTagConstructorRequirement::retained_bytes)
                    + value.parameters.len() * size_of::<GroundTypeId>() + value.actuals.len() * size_of::<TypeRef>() + (value.fields.len() + value.argument_wrappers.len()) * size_of::<u32>()
                    + (value.payload.len() + value.field_block.1.len()) * size_of::<u32>()
                    + value.original.parameters.len() * size_of::<Type>()
                    + value.original.parameters.iter().map(|ty| ty.retained_bytes().saturating_sub(size_of::<Type>())).sum::<usize>()
                    + value.original.recipes.len() * size_of::<crate::sema::check::SolvedArgumentSource>()
                    + value.original.application.parameters.capacity() * size_of::<crate::sema::check::SolvedConstructorParameter>()
                    + value.original.application.supplied.capacity() * size_of::<crate::sema::check::SolvedConstructorArgument>()
                    + value.original.application.default_slots.capacity() * size_of::<usize>()
                    + value.checked_fields.len() * size_of::<(u32, Box<[u32]>)>() + value.checked_fields.iter().map(|(_, payload)| payload.len() * size_of::<u32>()).sum::<usize>()
            }).sum::<usize>()
    }
    pub(super) fn shrink_to_fit(&mut self) { self.receipts.shrink_to_fit(); self.originals.shrink_to_fit(); self.instructions.shrink_to_fit(); self.wrappers.shrink_to_fit(); }
    fn receipt(&self, index: usize) -> Result<&PreparedTagConstructor, IrVerifyError> {
        let receipt = &self.receipts.get(index).ok_or_else(|| failure("original tag constructor is missing"))?.value;
        if !self.originals.get(index).is_some_and(|original| Arc::ptr_eq(original, receipt)) { return Err(failure("tag constructor changes its original receipt")); }
        Ok(receipt)
    }
}

impl GenericEvidenceStore {
    pub(in crate::runtime::eval) fn tag_constructor_at(&self, instruction: u32) -> Result<Option<&PreparedTagConstructor>, IrVerifyError> {
        if !self.tag_constructors.originals.is_empty() && self.tag_constructors.program != Some(self.root) { return Err(failure("tag constructor belongs to a foreign program")); }
        self.tag_constructors.instructions.binary_search_by_key(&instruction, |entry| entry.0).ok()
            .map(|index| self.tag_constructors.receipt(self.tag_constructors.instructions[index].1)).transpose()
    }
    pub(in crate::runtime::eval) fn tag_constructor_wrapper_target(&self, instruction: u32) -> Option<u32> {
        self.tag_constructors.wrappers.binary_search_by_key(&instruction, |entry| entry.0).ok().map(|index| self.tag_constructors.wrappers[index].1)
    }
    pub(in crate::runtime::eval) fn tag_constructor_originally_prepared(&self, instruction: u32) -> bool {
        self.tag_constructors.originals.iter().any(|source| source.instruction == instruction)
    }
    pub(in crate::runtime::eval) fn tag_constructors(&self) -> impl Iterator<Item = &PreparedTagConstructor> { self.tag_constructors.receipts.iter().map(|entry| entry.value.as_ref()) }
    pub(super) fn verify_tag_constructor_evidence(&self, pools: &SemanticPools, owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        let evidence = &self.tag_constructors;
        if !evidence.receipts.is_empty() && evidence.program != Some(self.root) { return Err(failure("tag constructor belongs to a foreign program")); }
        if evidence.receipts.len() != evidence.originals.len() { return Err(failure("tag constructor original receipt ledger is incomplete")); }
        let mut expected = Vec::with_capacity(evidence.receipts.len());
        for index in 0..evidence.receipts.len() {
            let source = evidence.receipt(index)?;
            let original = &source.original;
            if !matches!(original.authority, QualifiedNominalIdentity::Source { declaration: NominalDeclaration::Type(_), member: Some(member), .. } if member == original.member)
                || !matches!(original.application.authority, ConstructorAuthority::Nominal(authority) if authority == original.authority)
                || original.application.requirement.is_none() || !original.application.default_slots.is_empty()
                || original.application.parameters.iter().any(|parameter| parameter.default.is_some())
                || source.fields.len() != original.parameters.len() || source.parameters.len() != original.parameters.len()
                || source.actuals.len() != original.application.supplied.len() || source.argument_wrappers.len() != original.application.supplied.len()
                || pools.to_type(source.result)? != original.checked_type
                || !matches!(original.checked_type, Type::Tag(family) if family == original.family)
                || owners.get(source.instruction as usize) != Some(&Some(source.owner))
                || self.registered_instruction_origin(source.instruction, false) != Some((original.origin, source.owner)) {
                return Err(failure("tag constructor changes its original declaration, instruction, or field contract"));
            }
            if let Some(scope) = source.scope {
                let scope = self.scope(scope)?;
                if source.owner != InstructionOwner::Function(scope.owner) || original.checked.scope.is_none() || original.application.caller.is_none() { return Err(failure("tag constructor changes its original quantified owner")); }
                for &actual in source.actuals.iter() { self.verify_reference(pools, scope, actual)?; }
                if let Some(index) = source.requirement {
                    let Some(Requirement::TagConstructor(requirement)) = scope.requirements.get(index as usize) else { return Err(failure("tag constructor changes its original scoped requirement")); };
                    if source.scoped_requirement.as_ref() != Some(requirement) { return Err(failure("tag constructor changes its sealed original scoped requirement class")); }
                    self.verify_scoped_tag_constructor_requirement(pools, scope, requirement)?;
                    if requirement.nominal != original.authority || requirement.arguments.as_ref() != source.actuals.as_ref() || requirement.result != source.result { return Err(failure("tag constructor changes its original scoped nominal or payload relationships")); }
                } else if source.actuals.iter().any(|actual| !matches!(actual, TypeRef::Ground(_))) { return Err(failure("symbolic tag payload lacks its original scoped constructor requirement")); }
            } else if source.actuals.iter().any(|actual| !matches!(actual, TypeRef::Ground(_))) { return Err(failure("tag constructor loses its original actual type scope")); }
            let mut occupied = std::collections::BTreeSet::new();
            if original.recipes.len() != original.application.supplied.len() { return Err(failure("tag constructor original recipes are incomplete")); }
            for (ordinal, (argument, recipe)) in original.application.supplied.iter().zip(original.recipes.iter()).enumerate() {
                if !occupied.insert(argument.slot) || source.fields.get(argument.slot).is_none() { return Err(failure("tag constructor changes its original field slots")); }
                let field = source.fields[argument.slot];
                if let Some(saved) = self.original_argument_binding(field) {
                    if !matches!(original.origin, OperationSourceOrigin::Expression(expression) if saved.call == expression) || saved.owner != source.owner || saved.ordinal as usize != ordinal || saved.recipe != *recipe || source.argument_wrappers[ordinal] != saved.wrapper {
                        return Err(failure("tag constructor changes its original saved operand recipe"));
                    }
                }
                if owners.get(field as usize) != Some(&Some(source.owner)) { return Err(failure("tag constructor field belongs to another owner")); }
            }
            if occupied.len() != original.parameters.len() { return Err(failure("tag constructor lacks its original supplied fields")); }
            for ((ty, &prepared), parameter) in original.parameters.iter().zip(source.parameters.iter()).zip(&original.application.parameters) {
                if parameter.label.is_some() || pools.to_type(prepared)? != *ty { return Err(failure("tag constructor field type differs from its original formal")); }
            }
            expected.push((source.instruction, index));
        }
        expected.sort_unstable_by_key(|entry| entry.0);
        if expected.windows(2).any(|pair| pair[0].0 == pair[1].0) || expected != evidence.instructions { return Err(failure("tag constructor lookup is incomplete or ambiguous")); }
        let mut wrappers = evidence.originals.iter().flat_map(|source| source.argument_wrappers.iter().map(move |&wrapper| (wrapper, source.instruction))).collect::<Vec<_>>();
        wrappers.sort_unstable_by_key(|entry| entry.0);
        if wrappers.windows(2).any(|pair| pair[0].0 == pair[1].0) || wrappers != evidence.wrappers { return Err(failure("tag constructor original argument wrapper lookup is incomplete or ambiguous")); }
        Ok(())
    }
}

impl GenericEvidenceBuilder {
    pub(in crate::runtime::eval) fn add_tag_constructor(&mut self, source: PreparedTagConstructor) -> Result<(), IrVerifyError> {
        if self.store.tag_constructors.receipts.len() >= 2_000_000 { return Err(failure("tag constructor limit exceeded")); }
        let serial = self.next_serial;
        self.next_serial = serial.checked_add(1).ok_or_else(|| failure("tag constructor serial overflow"))?;
        let source = Arc::new(source);
        self.store.tag_constructors.program = Some(self.store.root);
        self.store.tag_constructors.originals.push(Arc::clone(&source));
        self.store.tag_constructors.receipts.push(Entry { serial, value: source });
        Ok(())
    }
}

#[cfg(test)]
impl GenericEvidenceStore {
    pub(in crate::runtime::eval) fn test_remove_tag_constructors(&mut self) { self.tag_constructors.receipts.clear(); self.tag_constructors.instructions.clear(); }
    pub(in crate::runtime::eval) fn test_replace_tag_constructors(&mut self, other: &Self) { self.tag_constructors = other.tag_constructors.clone(); }
    pub(in crate::runtime::eval) fn test_tag_constructor_mut(&mut self, instruction: u32) -> Result<&mut PreparedTagConstructor, IrVerifyError> {
        let index = self.tag_constructors.instructions.binary_search_by_key(&instruction, |entry| entry.0).map_err(|_| failure("original tag constructor is missing"))?;
        let index = self.tag_constructors.instructions[index].1;
        Ok(Arc::make_mut(&mut self.tag_constructors.receipts[index].value))
    }
}

/// The declaration admits each supplied binder through its authored Any slot.
/// The selected nominal member and result remain fixed for every substitution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct ScopedTagConstructorRequirement {
    pub authority: PreparedOperationAuthority,
    pub nominal: QualifiedNominalIdentity,
    pub signature: TypeRef,
    pub arguments: Box<[TypeRef]>,
    pub result: GroundTypeId,
}

impl ScopedTagConstructorRequirement {
    pub(in crate::runtime::eval) fn references(&self) -> impl Iterator<Item = TypeRef> + '_ {
        [self.signature, TypeRef::Ground(self.result)].into_iter().chain(self.arguments.iter().copied())
    }
    pub(in crate::runtime::eval) fn rebase(&self, mut reference: impl FnMut(TypeRef) -> Result<TypeRef, super::super::IrBuildError>) -> Result<Self, super::super::IrBuildError> {
        Ok(Self { authority: self.authority.clone(), nominal: self.nominal, signature: reference(self.signature)?,
            arguments: self.arguments.iter().map(|&ty| reference(ty)).collect::<Result<Vec<_>, _>>()?.into_boxed_slice(), result: self.result })
    }
    pub(super) fn retained_bytes(&self) -> usize { self.authority.retained_bytes() + self.arguments.len() * std::mem::size_of::<TypeRef>() }
    pub(in crate::runtime::eval) fn verify_supported(&self) -> Result<(), IrVerifyError> {
        if !matches!(self.nominal, QualifiedNominalIdentity::Source { declaration: NominalDeclaration::Type(_), member: Some(_), .. })
            || !matches!(self.authority, PreparedOperationAuthority::Language {
                operation: crate::sema::operation_graph::PreparedLanguageOperation::Declaration { authority: "language.constructor.tag", identity },
                authority: "language.constructor.tag", argument_order: crate::sema::operation_graph::OperationArgumentOrder::SourceOrder, statement_result_is_unit: false, ..
            } if &*identity.as_str() == format!("{:?}", self.nominal)) { return Err(failure("scoped tag constructor changes its fixed original member authority")); }
        Ok(())
    }
}

impl GenericEvidenceStore {
    pub(in crate::runtime::eval) fn verify_tag_constructor_frame(&self, instruction: u32, instance: Option<InstantiationId>) -> Result<(), IrVerifyError> {
        let Some(source) = self.tag_constructor_at(instruction)? else { return Ok(()); };
        let Some(scope) = source.scope else { return Ok(()); };
        let instance = self.instance(instance.ok_or_else(|| failure("tag constructor lacks its original quantified frame"))?)?;
        if instance.scope != scope || source.owner != InstructionOwner::Function(self.scope(scope)?.owner) {
            return Err(failure("tag constructor belongs to another quantified frame"));
        }
        if let Some(index) = source.requirement {
            let expected = source.scoped_requirement.as_ref().ok_or_else(|| failure("tag constructor loses its original scoped requirement class"))?;
            if self.scope(scope)?.requirements.get(index as usize) != Some(&Requirement::TagConstructor(expected.clone())) {
                return Err(failure("tag constructor changes its sealed original scoped requirement class"));
            }
            if instance.requirements.get(index as usize) != Some(&RequirementWitness::TagConstructor) {
                return Err(failure("tag constructor lacks its original scoped requirement witness"));
            }
        } else if source.actuals.iter().any(|actual| !matches!(actual, TypeRef::Ground(_))) {
            return Err(failure("symbolic tag constructor loses its original scoped requirement"));
        }
        Ok(())
    }

    pub(super) fn verify_scoped_tag_constructor_requirement(&self, pools: &SemanticPools, scope: &SchemeScope, requirement: &ScopedTagConstructorRequirement) -> Result<(), IrVerifyError> {
        requirement.verify_supported()?;
        for reference in requirement.references() { self.verify_reference(pools, scope, reference)?; }
        let NormalizedType::Arrow(kind, parameters, output, effects) = self.normalized(pools, scope, requirement.signature, None, &mut Vec::new())? else {
            return Err(failure("scoped tag constructor loses its original callable signature"));
        };
        if kind != CallableKind::Pure || !effects.is_empty() || parameters.len() != requirement.arguments.len()
            || parameters.iter().any(|parameter| parameter.2 || parameter.3 || parameter.4 != NormalizedType::Scalar(Type::Any))
            || *output != Self::normalized_ground_type(pools, requirement.result)? || !matches!(pools.to_type(requirement.result)?, Type::Tag(_)) {
            return Err(failure("scoped tag constructor changes its declared Any slots or fixed nominal result"));
        }
        Ok(())
    }

    pub(super) fn verify_scoped_tag_constructor_witness(&self, pools: &SemanticPools, scope: &SchemeScope, requirement: &ScopedTagConstructorRequirement, requirement_index: usize) -> Result<(), IrVerifyError> {
        if scope.requirements.get(requirement_index) != Some(&Requirement::TagConstructor(requirement.clone())) { return Err(failure("scoped tag constructor witness belongs to another original requirement")); }
        self.verify_scoped_tag_constructor_requirement(pools, scope, requirement)
    }
}
