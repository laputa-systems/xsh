use super::*;
use super::super::generic::{FormattedPathPart, PreparedFormattedPath, graph_ground_type};
use crate::runtime::eval::lower::paths::{FormattedTarget, OriginalFormattedPath, OriginalPathPart};

#[cfg(test)]
mod tests;

#[derive(Clone)]
pub(super) struct PathSourceRow {
    original: OriginalFormattedPath,
    instruction: u32,
    owner: InstructionOwner,
    operands: Vec<Option<(u32, u32)>>,
}

fn problem(reason: &'static str) -> IrBuildError { IrBuildError::format(reason, None, 0, 0) }

fn formatted_tag(target: FormattedTarget) -> FullTag {
    match target { FormattedTarget::Path => FullTag::ExprPathFmtString, FormattedTarget::Str => FullTag::ExprFmtString }
}

impl FullBuilder {
    pub(super) fn stage_formatted_path(&mut self, expression: BuildExprId, instruction: u32, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        let Some(original) = scratch.formatted_paths.get(&expression) else { return Ok(()); };
        if self.store.tags.get(instruction as usize) != Some(&formatted_tag(original.target)) || self.active_expression_origins.get(&expression) != Some(&original.origin) {
            return Err(problem("formatted_path_original_instruction_changed"));
        }
        let raw = self.current_owner.ok_or_else(|| problem("formatted_path_original_owner_missing"))?;
        let owner = if let Some(index) = driver_owner_index(raw) { InstructionOwner::Driver(index as u32) }
            else { InstructionOwner::Function(IrFunctionId::from_raw(raw).ok_or_else(|| problem("formatted_path_original_owner_invalid"))?) };
        let mut operands = Vec::with_capacity(original.parts.len());
        for part in original.parts.iter() {
            operands.push(match part {
                OriginalPathPart::Text(_) => None,
                OriginalPathPart::Expression { value, source, .. } => Some((
                    *self.active_encoded_expressions.get(value).ok_or_else(|| problem("formatted_path_original_operand_missing"))?,
                    *self.active_encoded_expressions.get(source).ok_or_else(|| problem("formatted_path_original_operand_source_missing"))?,
                )),
            });
        }
        self.path_source_rows.push(PathSourceRow { original: original.clone(), instruction, owner, operands });
        Ok(())
    }

    pub(super) fn prepare_formatted_paths(&mut self) -> Result<(), IrBuildError> {
        if self.path_source_rows.is_empty() { return Ok(()); }
        let solved = self.solved.clone().ok_or_else(|| problem("formatted_path_checked_graph_missing"))?;
        for row in self.path_source_rows.clone() {
            let caller = row.original.caller.and_then(|declaration| self.generic_declarations.get(&declaration).copied());
            let expected_owner = match row.original.caller {
                Some(declaration) => InstructionOwner::Function(*self.declaration_functions.get(&declaration).ok_or_else(|| problem("formatted_path_original_declaration_missing"))?),
                None => row.owner,
            };
            if expected_owner != row.owner || row.original.caller.is_none() && !matches!(row.owner, InstructionOwner::Driver(_))
                || solved.expressions.get(&row.original.origin) != Some(&row.original.checked.ty)
                || solved.expression_owners.get(&row.original.origin).copied() != row.original.caller
                || solved.expression_scope(row.original.origin, row.original.caller).map_err(|_| problem("formatted_path_original_scope"))? != row.original.checked.scope {
                return Err(problem("formatted_path_original_source_changed"));
            }
            solved.graph.validate_scoped(row.original.checked).map_err(|_| problem("formatted_path_original_root"))?;
            if graph_ground_type(&solved.graph, row.original.checked.ty).map_err(|_| problem("formatted_path_original_result"))? != row.original.target.result_type() { return Err(problem("formatted_path_original_result_changed")); }
            let ty = self.intern_generic_ground_type(&row.original.target.result_type())?;
            let mut parts = Vec::with_capacity(row.original.parts.len());
            for (original, operand) in row.original.parts.iter().zip(row.operands) {
                parts.push(match (original, operand) {
                    (OriginalPathPart::Text(text), None) => FormattedPathPart::Text(Arc::clone(text)),
                    (OriginalPathPart::Expression { origin, checked, .. }, Some((instruction, source))) => {
                        if solved.expressions.get(origin) != Some(&checked.ty) || solved.expression_owners.get(origin).copied() != row.original.caller
                            || solved.expression_scope(*origin, row.original.caller).map_err(|_| problem("formatted_path_original_operand_scope"))? != checked.scope
                            || !self.generic_expression_rows.iter().any(|(actual, source_origin, owner)| *actual == source && *source_origin == *origin && *owner == row.owner) {
                            return Err(problem("formatted_path_original_operand_source_changed"));
                        }
                        solved.graph.validate_scoped(*checked).map_err(|_| problem("formatted_path_original_operand_root"))?;
                        let ty = self.call_reference(&solved, caller, checked.ty, false)?;
                        FormattedPathPart::Expression { instruction, source, ty }
                    }
                    _ => return Err(problem("formatted_path_original_parts_changed")),
                });
            }
            let payload = self.store.payload(self.store.data[row.instruction as usize].range()).map_err(|_| problem("formatted_path_original_payload"))?.to_vec().into_boxed_slice();
            let parts_block = *payload.first().ok_or_else(|| problem("formatted_path_original_parts_block"))?;
            let block = IrBlockId::from_raw(parts_block).and_then(|id| self.store.blocks.get(id.index())).ok_or_else(|| problem("formatted_path_original_parts_block"))?;
            let parts_payload = self.store.payload(block.instructions).map_err(|_| problem("formatted_path_original_parts_payload"))?.to_vec().into_boxed_slice();
            let prepared = PreparedFormattedPath { original: row.original, instruction: row.instruction, owner: row.owner, caller, ty, parts: parts.into_boxed_slice(), payload, parts_block, parts_payload };
            FullVerifier::verify_formatted_path_source(&self.store, &prepared).map_err(|error| IrBuildError::verification("formatted_path_original_emission_changed", error))?;
            self.generic_evidence_mut().add_formatted_path(prepared).map_err(|_| problem("formatted_path_original_receipt_allocation"))?;
        }
        Ok(())
    }
}

impl FullVerifier {
    fn verify_formatted_path_source(store: &FullStore, source: &PreparedFormattedPath) -> Result<(), IrVerifyError> {
        let (owner, range, slot_count) = match source.owner {
            InstructionOwner::Function(target) => (target.raw(), store.function_instruction_range(target.index())?, store.functions[target.index()].slot_count),
            InstructionOwner::Driver(step) => (driver_owner(step as usize).map_err(|_| IrVerifyError::new("formatted path driver owner is invalid"))?, store.driver_instruction_range(step as usize)?, store.driver_steps[step as usize].slot_count),
        };
        if store.tags.get(source.instruction as usize) != Some(&formatted_tag(source.original.target))
            || !range.contains(&(source.instruction as usize)) || store.semantic.to_type(source.ty)? != source.original.target.result_type()
            || store.payload(store.data[source.instruction as usize].range())? != source.payload.as_ref() { return Err(IrVerifyError::new("formatted path changes its original instruction or result type")); }
        let decoder = FullDecoder { store, owner, instruction_range: range, instruction_states: None, block_states: None, slot_count, pattern_tree: None, pattern_ceiling: Cell::new(usize::MAX), verified: true };
        let mut words = FullCursor::new(source.payload.as_ref());
        if words.raw()? != source.parts_block { return Err(IrVerifyError::new("formatted path changes its original formatting block")); }
        if source.original.target == FormattedTarget::Path { Span::decode(&decoder, &mut words)?; }
        words.finish()?;
        let block = IrBlockId::from_raw(source.parts_block).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("formatted path formatting block is missing"))?;
        if block.owner != owner || block.flags != BLOCK_LIST || block.result != IR_NONE || block.reserved != [0; 3] || store.payload(block.instructions)? != source.parts_payload.as_ref() {
            return Err(IrVerifyError::new("formatted path changes its original formatting block or operands"));
        }
        let mut words = FullCursor::new(source.parts_payload.as_ref());
        if words.raw()? as usize != source.parts.len() || source.parts.len() != source.original.parts.len() { return Err(IrVerifyError::new("formatted path changes its original parts")); }
        // BuildExprId::decode validates children but deliberately discards their
        // indices. Read the raw operands to compare the original recipe.
        for (part, original) in source.parts.iter().zip(source.original.parts.iter()) {
            match (words.raw()?, part, original) {
                (0, FormattedPathPart::Text(prepared), OriginalPathPart::Text(original)) if store.string(words.raw()?)? == prepared.as_ref() && prepared == original => {},
                (1, FormattedPathPart::Expression { instruction, .. }, OriginalPathPart::Expression { format: original, .. }) => {
                    if words.raw()? != *instruction { return Err(IrVerifyError::new("formatted path changes its original interpolation operand")); }
                    Span::decode(&decoder, &mut words)?;
                    if Option::<FormatSpec>::decode(&decoder, &mut words)? != *original { return Err(IrVerifyError::new("formatted path changes its original interpolation format")); }
                }
                _ => return Err(IrVerifyError::new("formatted path changes its original interpolation or format")),
            }
        }
        words.finish()?;
        Ok(())
    }

    pub(super) fn verify_formatted_paths(store: &FullStore, generic: &GenericEvidenceStore, tree: &super::super::pattern::PatternTree) -> Result<(), IrVerifyError> {
        for (id, _) in generic.formatted_paths() {
            let source = generic.formatted_path(id)?;
            Self::verify_formatted_path_source(store, source)?;
            tree.parent(source.instruction)?;
            let mut active = vec![source.instruction];
            for part in source.parts.iter() {
                if let FormattedPathPart::Expression { instruction, ty, .. } = part {
                    Self::verify_formatted_path_interpolation(store, generic, source, *instruction, *ty, None, &mut active)?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn verify_formatted_path_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, expected: &Type, instance: Option<InstantiationId>, active: &mut Vec<u32>) -> Result<bool, IrVerifyError> {
        let Some(id) = generic.formatted_path_at(instruction)? else { return Ok(false); };
        let source = generic.formatted_path(id)?;
        let expected = match expected { Type::Optional(inner) => inner.as_ref(), _ => expected };
        if source.owner != owner || store.semantic.to_type(source.ty)? != *expected { return Err(IrVerifyError::new("formatted path operand changes its original owner or type")); }
        Self::verify_formatted_path_source(store, source)?;
        for part in source.parts.iter() {
            if let FormattedPathPart::Expression { instruction, ty, .. } = part {
                Self::verify_formatted_path_interpolation(store, generic, source, *instruction, *ty, instance, active)?;
            }
        }
        Ok(true)
    }

    fn verify_formatted_path_interpolation(store: &FullStore, generic: &GenericEvidenceStore, source: &PreparedFormattedPath, instruction: u32, ty: TypeRef, instance: Option<InstantiationId>, active: &mut Vec<u32>) -> Result<(), IrVerifyError> {
        if let Some(caller) = source.caller {
            if source.owner != InstructionOwner::Function(generic.scope(caller)?.owner) { return Err(IrVerifyError::new("formatted path interpolation changes its original caller owner")); }
            if let Some(instance) = instance {
                let instance_value = generic.instance(instance)?;
                if instance_value.scope != caller { return Err(IrVerifyError::new("formatted path interpolation uses a foreign caller instance")); }
                let ty = generic.expand(&store.semantic, ty, &instance_value.substitutions, &mut rustc_hash::FxHashMap::default())?;
                if !crate::sema::inference::Eligibility::Display.accepts_closed_display(&ty) { return Err(IrVerifyError::new("formatted interpolation has no concrete Display eligibility")); }
                Self::verify_generic_source(store, generic, instruction, source.owner, &ty, Some(instance), active)
            } else {
                Self::verify_formatted_display_requirement(store, generic, caller, ty)?;
                Self::verify_generic_symbolic_source(store, generic, instruction, caller, ty, active)
            }
        } else if let TypeRef::Ground(ty) = ty {
            let ty = store.semantic.to_type(ty)?;
            if !crate::sema::inference::Eligibility::Display.accepts_closed_display(&ty) { return Err(IrVerifyError::new("formatted interpolation has no concrete Display eligibility")); }
            Self::verify_generic_source(store, generic, instruction, source.owner, &ty, instance, active)
        } else { Err(IrVerifyError::new("formatted path interpolation has no original caller scope")) }
    }

    fn verify_formatted_display_requirement(store: &FullStore, generic: &GenericEvidenceStore, caller: SchemeScopeId, ty: TypeRef) -> Result<(), IrVerifyError> {
        if let TypeRef::Ground(ty) = ty {
            if crate::sema::inference::Eligibility::Display.accepts_closed_display(&store.semantic.to_type(ty)?) { return Ok(()); }
            return Err(IrVerifyError::new("formatted interpolation has no concrete Display eligibility"));
        }
        for requirement in &generic.scope(caller)?.requirements {
            if let Requirement::Eligibility { predicate: crate::sema::inference::Eligibility::Display, ty: required } = requirement
                && generic.references_equal(&store.semantic, caller, ty, *required)? { return Ok(()); }
        }
        Err(IrVerifyError::new("formatted interpolation loses its original scoped Display obligation"))
    }

    pub(super) fn verify_formatted_path_symbolic_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, caller: SchemeScopeId, expected: TypeRef, active: &mut Vec<u32>) -> Result<bool, IrVerifyError> {
        let Some(id) = generic.formatted_path_at(instruction)? else { return Ok(false); };
        let source = generic.formatted_path(id)?;
        if source.caller != Some(caller) || source.owner != InstructionOwner::Function(generic.scope(caller)?.owner)
            || !generic.references_equal(&store.semantic, caller, expected, TypeRef::Ground(source.ty))? {
            return Err(IrVerifyError::new("symbolic formatted path changes its original caller or result type"));
        }
        Self::verify_formatted_path_source(store, source)?;
        for part in source.parts.iter() {
            if let FormattedPathPart::Expression { instruction, ty, .. } = part {
                Self::verify_formatted_path_interpolation(store, generic, source, *instruction, *ty, None, active)?;
            }
        }
        Ok(true)
    }
}
