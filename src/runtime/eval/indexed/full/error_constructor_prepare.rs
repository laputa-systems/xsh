use super::*;
use super::super::generic::{PreparedErrorConstructor, graph_ground_type};
use crate::runtime::eval::lower::error_constructor::OriginalErrorConstructorSource;
use crate::sema::check::{ConstructorAuthority, NominalMemberKind};
use crate::sema::inference::{ScopedRequirementRoot, ScopedRoot};

pub(super) type StagedErrorConstructor = (OriginalErrorConstructorSource, u32, InstructionOwner, Box<[u32]>);
fn problem(reason: &'static str) -> IrBuildError { IrBuildError::format(reason, None, 0, 0) }

impl FullBuilder {
    pub(super) fn stage_original_error_constructor(&mut self, row: BuildExprId, instruction: u32, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        let Some(source) = scratch.error_constructor_sources.get(&row) else { return Ok(()); };
        let raw = self.current_owner.ok_or_else(|| problem("error_constructor_owner"))?;
        let owner = if let Some(driver) = driver_owner_index(raw) { InstructionOwner::Driver(driver as u32) }
            else { InstructionOwner::Function(IrFunctionId::from_raw(raw).ok_or_else(|| problem("error_constructor_owner"))?) };
        if self.active_expression_origins.get(&row) != Some(&source.plan.origin)
            || self.store.tags.get(instruction as usize) != Some(&FullTag::ExprError) { return Err(problem("error_constructor_original_instruction")); }
        let fields = source.fields.iter().map(|row| self.active_encoded_expressions.get(row).copied().ok_or_else(|| problem("error_constructor_original_field_missing"))).collect::<Result<Box<[_]>, _>>()?;
        self.error_constructor_rows.push((source.clone(), instruction, owner, fields));
        Ok(())
    }

    pub(super) fn prepare_error_constructors(&mut self) -> Result<(), IrBuildError> {
        let Some(solved) = self.solved.clone() else { return Ok(()); };
        for (source, instruction, owner, fields) in self.error_constructor_rows.clone() {
            let plan = &source.plan;
            let application = solved.constructor_applications.get(&plan.origin).ok_or_else(|| problem("error_constructor_original_application_missing"))?;
            if !matches!(application.authority, ConstructorAuthority::Nominal(authority) if authority == plan.authority)
                || application.caller != plan.application.caller || application.result != plan.checked.ty
                || application.requirement != plan.application.requirement || !application.default_slots.is_empty()
                || application.parameters.len() != plan.application.parameters.len() || application.supplied.len() != plan.application.supplied.len()
                || solved.expressions.get(&plan.origin) != Some(&application.result)
                || solved.argument_sources.get(&plan.origin).map(Vec::as_slice) != Some(plan.recipes.as_ref())
                || application.parameters.iter().zip(&plan.application.parameters).any(|(left, right)| left.label != right.label || left.ty != right.ty || left.default != right.default)
                || application.supplied.iter().zip(&plan.application.supplied).any(|(left, right)| left.value != right.value || left.actual != right.actual
                    || left.slot != right.slot || left.assignability != right.assignability || left.projection != right.projection) {
                return Err(problem("error_constructor_original_application_changed"));
            }
            let scope = solved.expression_scope(plan.origin, application.caller).map_err(|_| problem("error_constructor_original_scope"))?;
            if scope != plan.checked.scope { return Err(problem("error_constructor_original_scope_changed")); }
            solved.graph.validate_scoped(plan.checked).map_err(|_| problem("error_constructor_original_result_scope"))?;
            let requirement = application.requirement.ok_or_else(|| problem("error_constructor_original_requirement_missing"))?;
            solved.graph.validate_requirement_scoped(ScopedRequirementRoot { requirement, scope }).map_err(|_| problem("error_constructor_original_requirement_scope"))?;
            let selected = solved.graph.candidate_evidence(requirement).map_err(|_| problem("error_constructor_original_requirement"))?.ok_or_else(|| problem("error_constructor_original_requirement"))?;
            let crate::sema::check::SolvedOperationAuthority::Language(metadata) = solved.operation_catalog.candidate(&solved.graph, selected.candidate).map_err(|_| problem("error_constructor_original_candidate"))? else { return Err(problem("error_constructor_original_candidate")); };
            if !matches!(metadata.operation, crate::sema::operation_graph::PreparedLanguageOperation::Declaration { authority: "language.constructor.error_variant", identity }
                if &*identity.as_str() == format!("{:?}", plan.authority)) || metadata.authority != "language.constructor.error_variant" { return Err(problem("error_constructor_original_candidate_changed")); }
            let member = solved.checked_nominal_member(plan.authority).map_err(|_| problem("error_constructor_original_nominal_missing"))?;
            if member.kind != NominalMemberKind::Error || member.scope.is_some() || member.family != plan.family || member.member != plan.member
                || member.facets.as_slice() != plan.facets.as_ref() || member.fields.len() != plan.parameters.len() || fields.len() != plan.parameters.len() {
                return Err(problem("error_constructor_original_nominal_changed"));
            }
            match (owner, application.caller) {
                (InstructionOwner::Function(owner), Some(caller)) if self.declaration_functions.get(&caller) == Some(&owner) => {},
                (InstructionOwner::Driver(_), None) => {},
                _ => return Err(problem("error_constructor_original_caller_changed")),
            }
            let mut parameters = Vec::with_capacity(plan.parameters.len());
            for ((name, ty), parameter) in plan.parameters.iter().zip(&application.parameters) {
                if parameter.label != Some(*name) { return Err(problem("error_constructor_original_formal_changed")); }
                solved.graph.validate_scoped(ScopedRoot { ty: parameter.ty, scope }).map_err(|_| problem("error_constructor_original_formal_scope"))?;
                let declared = member.fields.iter().find_map(|&(label, ty)| (label == Some(*name)).then_some(ty)).ok_or_else(|| problem("error_constructor_original_declared_field_missing"))?;
                if graph_ground_type(&solved.graph, parameter.ty).map_err(|_| problem("error_constructor_original_formal_type"))? != *ty
                    || graph_ground_type(&solved.graph, declared).map_err(|_| problem("error_constructor_original_declared_type"))? != *ty {
                    return Err(problem("error_constructor_original_formal_type_changed"));
                }
                parameters.push(self.intern_generic_ground_type(ty)?);
            }
            let result_type = graph_ground_type(&solved.graph, application.result).map_err(|_| problem("error_constructor_original_result_type"))?;
            if solved.nominals.get(&solved.graph.resolved(application.result).map_err(|_| problem("error_constructor_original_result_owner"))?) != Some(&plan.authority)
                || result_type != plan.checked_type {
                return Err(problem("error_constructor_original_result_changed"));
            }
            let result = self.intern_generic_ground_type(&result_type)?;
            let mut actuals = Vec::with_capacity(application.supplied.len());
            let mut checked_fields = Vec::new();
            for (ordinal, (argument, recipe)) in application.supplied.iter().zip(plan.recipes.iter()).enumerate() {
                let mut field = *fields.get(argument.slot).ok_or_else(|| problem("error_constructor_original_slot"))?;
                while self.store.tags.get(field as usize) == Some(&FullTag::ExprCheckedValue) {
                    let payload = self.store.payload(self.store.data[field as usize].range()).map_err(|_| problem("error_constructor_original_checked_payload"))?.to_vec().into_boxed_slice();
                    let child = *payload.first().ok_or_else(|| problem("error_constructor_original_checked_payload"))?;
                    checked_fields.push((field, payload)); field = child;
                }
                let saved = self.prepared_saved_argument_bindings.get(&field).ok_or_else(|| problem("error_constructor_original_saved_field_missing"))?;
                if saved.call != plan.origin || saved.ordinal as usize != ordinal || saved.recipe != *recipe || saved.owner != owner {
                    return Err(problem("error_constructor_original_saved_field_changed"));
                }
                let actual = graph_ground_type(&solved.graph, argument.actual).map_err(|_| problem("error_constructor_original_actual_type"))?;
                actuals.push(self.intern_generic_ground_type(&actual)?);
            }
            let (payload, field_block, facet_block) = structured_error_allocation(&self.store, instruction).map_err(|_| problem("error_constructor_original_allocation"))?;
            self.generic_evidence_mut().add_error_constructor(PreparedErrorConstructor { original: plan.clone(), instruction, owner, result,
                parameters: parameters.into_boxed_slice(), fields, actuals: actuals.into_boxed_slice(), payload, field_block, facet_block, checked_fields: checked_fields.into_boxed_slice() }).map_err(|_| problem("error_constructor_receipt_allocation"))?;
        }
        Ok(())
    }
}

type ErrorAllocation = (Box<[u32]>, (u32, Box<[u32]>), (u32, Box<[u32]>));
fn structured_error_allocation(store: &FullStore, instruction: u32) -> Result<ErrorAllocation, IrVerifyError> {
    if store.tags.get(instruction as usize) != Some(&FullTag::ExprError) { return Err(IrVerifyError::new("error constructor has another opcode")); }
    let payload = store.payload(store.data.get(instruction as usize).ok_or_else(|| IrVerifyError::new("error constructor instruction is missing"))?.range())?;
    if payload.len() != 5 || payload[0] != 1 { return Err(IrVerifyError::new("error constructor has another structured payload")); }
    let block = |raw| {
        let block = IrBlockId::from_raw(raw).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("error constructor block is invalid"))?;
        if block.flags & BLOCK_SEQUENCE_KIND_MASK != BLOCK_LIST { return Err(IrVerifyError::new("error constructor block has another kind")); }
        Ok((raw, store.payload(block.instructions)?.to_vec().into_boxed_slice()))
    };
    Ok((payload.to_vec().into_boxed_slice(), block(payload[3])?, block(payload[4])?))
}

impl FullVerifier {
    pub(super) fn verify_error_constructors(store: &FullStore, generic: &GenericEvidenceStore) -> Result<(), IrVerifyError> {
        for source in generic.error_constructors() {
            Self::verify_error_constructor_operand(store, generic, source.instruction, source.owner, &store.semantic.to_type(source.result)?, &mut Vec::new())?;
        }
        for (instruction, tag) in store.tags.iter().enumerate() {
            if *tag == FullTag::ExprError && store.payload(store.data[instruction].range())?.first() == Some(&1)
                && generic.registered_instruction_origin(instruction as u32, false).is_some() && generic.error_constructor_at(instruction as u32)?.is_none() {
                return Err(IrVerifyError::new("structured error constructor lacks its original declaration receipt"));
            }
        }
        Ok(())
    }

    pub(super) fn verify_error_constructor_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, expected: &Type, active: &mut Vec<u32>) -> Result<bool, IrVerifyError> {
        let Some(source) = generic.error_constructor_at(instruction)? else { return Ok(false); };
        let original = &source.original;
        let Type::ErrorVariant { family: checked_family, variant: checked_member } = original.checked_type else { return Err(IrVerifyError::new("error constructor loses its original checked nominal type")); };
        let mut expected = expected;
        while let Type::Optional(item) = expected { expected = item; }
        if source.owner != owner || !match expected {
            Type::Error => true,
            Type::ErrorFamily(family) => *family == checked_family || *family == original.family,
            Type::ErrorVariant { family, variant } => (*family == checked_family || *family == original.family) && *variant == checked_member,
            Type::ErrorFacet(facet) => original.facets.contains(facet),
            _ => false,
        } { return Err(IrVerifyError::new("error constructor changes its original nominal carrier or lexical owner")); }
        let (payload, fields, facets) = structured_error_allocation(store, instruction)?;
        if payload != source.payload || fields != source.field_block || facets != source.facet_block
            || store.string(payload[1])? != original.family.as_str().as_str() || store.string(payload[2])? != original.member.as_str().as_str()
            || fields.1.first().copied() != Some(original.parameters.len() as u32) || fields.1.len() != 1 + original.parameters.len() * 2
            || facets.1.first().copied() != Some(original.facets.len() as u32) || facets.1.len() != 1 + original.facets.len() {
            return Err(IrVerifyError::new("error constructor changes its original canonical family, fields, or facets"));
        }
        for ((entry, (name, _)), &field) in fields.1[1..].chunks_exact(2).zip(original.parameters.iter()).zip(source.fields.iter()) {
            if store.string(entry[0])? != name.as_str().as_str() || entry[1] != field { return Err(IrVerifyError::new("error constructor changes its original payload label or operand")); }
        }
        for (&raw, &facet) in facets.1[1..].iter().zip(original.facets.iter()) {
            if raw != facet.symbol().raw() { return Err(IrVerifyError::new("error constructor changes its original facet authority")); }
        }
        for (field, payload) in source.checked_fields.iter() {
            if store.tags.get(*field as usize) != Some(&FullTag::ExprCheckedValue) || store.payload(store.data[*field as usize].range())? != payload.as_ref() {
                return Err(IrVerifyError::new("error constructor changes its original checked payload boundary"));
            }
        }
        if active.len() >= 256 || active.contains(&instruction) { return Err(IrVerifyError::new("error constructor payload is cyclic or too deep")); }
        active.push(instruction);
        for (argument, &actual) in original.application.supplied.iter().zip(source.actuals.iter()) {
            let mut field = source.fields[argument.slot];
            while store.tags.get(field as usize) == Some(&FullTag::ExprCheckedValue) { field = store.payload(store.data[field as usize].range())?[0]; }
            let saved = generic.original_argument_binding(field).ok_or_else(|| IrVerifyError::new("error constructor loses its original payload argument"))?;
            if saved.ty != TypeRef::Ground(actual) { return Err(IrVerifyError::new("error constructor changes its original actual payload type")); }
            Self::verify_generic_source(store, generic, field, owner, &store.semantic.to_type(actual)?, None, active)?;
        }
        active.pop();
        Ok(true)
    }
}

#[cfg(test)]
#[path = "error_constructor_prepare/tests.rs"]
mod tests;
