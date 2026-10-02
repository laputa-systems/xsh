use super::*;
use super::super::generic::{LexicalCapture, LexicalCaptureSource, OperationSourceOrigin, graph_ground_type};
use crate::sema::check::{ProducerFlowKind, ProducerFlowSource};

#[cfg(test)]
mod tests;

fn problem(reason: &'static str) -> IrBuildError { IrBuildError::format(reason, None, 0, 0) }

impl FullBuilder {
    pub(super) fn stage_lexical_capture(&mut self, target: IrFunctionId, declaration: Option<crate::sema::check::DeclarationIdentity>, header_index: u32, original: &LoweredTopLevelSlot) -> Result<(), IrBuildError> {
        let Some(binding) = original.lexical_binding else { return Ok(()); };
        let Some(declaration) = declaration else { return Ok(()); };
        let solved = self.solved.clone().ok_or_else(|| problem("lexical_capture_checked_owner_missing"))?;
        let definition = solved.bindings.get(&binding).ok_or_else(|| problem("lexical_capture_original_binding_missing"))?;
        solved.declarations.get(&declaration).ok_or_else(|| problem("lexical_capture_original_declaration_missing"))?;
        let lexical = definition.owner.map(|owner| solved.declarations.get(&owner).map(|definition| definition.scheme).ok_or_else(|| problem("lexical_capture_definition_owner_missing"))).transpose()?;
        let source_type = crate::sema::inference::ScopedRoot { ty: definition.ty, scope: definition.scheme.or(lexical) };
        if !original.source_type.is_some_and(|root| root.ty == source_type.ty && root.scope == source_type.scope)
            || original.mutable != definition.mutable || definition.owner == Some(declaration)
            || original.host_binding.is_some() {
            return Err(problem("lexical_capture_original_binding_changed"));
        }
        solved.graph.validate_scoped(source_type).map_err(|_| problem("lexical_capture_original_scope"))?;
        self.stage_original_callable_capture(target, Some(declaration), header_index, original)?;
        let Ok(original_type) = graph_ground_type(&solved.graph, source_type.ty) else { return Ok(()); };
        let ty = self.intern_generic_ground_type(&original_type)?;
        let slot = u32::try_from(original.slot).map_err(|_| problem("lexical_capture_slot_overflow"))?;
        let header = self.store.captures.get(header_index as usize).ok_or_else(|| problem("lexical_capture_header_missing"))?;
        let name = header.name;
        if header.slot_and_flags != (slot | (u32::from(original.mutable) << 31)) || header.type_id != ty
            || self.store.string(name).map_err(|_| problem("lexical_capture_header_name"))? != original.name.as_str().as_str() {
            return Err(problem("lexical_capture_original_header_changed"));
        }
        self.generic_evidence_mut().add_lexical_capture(LexicalCapture {
            binding, definition_owner: definition.owner, declaration, target, header_index, slot, name,
            mutable: original.mutable, source_type, original_type, ty,
        }).map_err(|_| problem("lexical_capture_original_allocation"))?;
        Ok(())
    }

    pub(super) fn stage_lexical_capture_read(&mut self, instruction: u32, origin: crate::sema::check::ExpressionIdentity, owner: InstructionOwner, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        let Some(original) = scratch.lexical_capture_reads.get(&origin) else { return Ok(()); };
        if !matches!(self.store.tags.get(instruction as usize), Some(FullTag::ExprParam | FullTag::IntSlot | FullTag::BoolSlot))
            || self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| problem("lexical_capture_original_read_payload"))? != [original.slot as u32]
            || !matches!(scratch.expressions.get(original.expression.index()), Some(BuildExprRow::Param(slot)) if *slot == original.slot) {
            return Err(problem("lexical_capture_original_read_changed"));
        }
        self.lexical_capture_read_rows.push((instruction, original.clone(), owner));
        Ok(())
    }

    pub(super) fn prepare_lexical_captures(&mut self) -> Result<(), IrBuildError> {
        if self.lexical_capture_read_rows.is_empty() { return Ok(()); }
        let solved = self.solved.clone().ok_or_else(|| problem("lexical_capture_read_checked_owner_missing"))?;
        for (instruction, original, owner) in self.lexical_capture_read_rows.clone() {
            let InstructionOwner::Function(target) = owner else { return Err(problem("lexical_capture_read_owner_changed")); };
            let (capture, allocation) = self.generic_evidence_mut().lexical_capture_for_slot(target, original.slot as u32)
                .map_err(|_| problem("lexical_capture_read_allocation_invalid"))?.map(|(id, capture)| (id, capture.clone()))
                .ok_or_else(|| problem("lexical_capture_read_allocation_missing"))?;
            let flow = *solved.expression_producer_flows.get(&original.origin).ok_or_else(|| problem("lexical_capture_read_checked_binding_missing"))?;
            let node = solved.producer_flows.node(flow).map_err(|_| problem("lexical_capture_read_checked_binding_invalid"))?;
            let ProducerFlowKind::CapturedBinding { identity, version, input } = node.kind else { return Err(problem("lexical_capture_read_checked_binding_changed")); };
            if node.source != ProducerFlowSource::Expression(original.origin)
                || identity != original.binding || solved.binding_producer_flows.get(&(identity, version)) != Some(&input)
                || allocation.binding != original.binding || allocation.declaration != original.caller
                || solved.expression_owners.get(&original.origin) != Some(&original.caller)
                || solved.expressions.get(&original.origin) != Some(&original.source_type.ty)
                || solved.expression_scope(original.origin, Some(original.caller)).map_err(|_| problem("lexical_capture_read_scope"))? != original.source_type.scope
                || !self.generic_expression_rows.iter().any(|(row, origin, actual_owner)| *row == instruction && *origin == original.origin && *actual_owner == owner) {
                return Err(problem("lexical_capture_read_checked_source_changed"));
            }
            solved.graph.validate_scoped(original.source_type).map_err(|_| problem("lexical_capture_read_scope"))?;
            let original_type = graph_ground_type(&solved.graph, original.source_type.ty).map_err(|_| problem("lexical_capture_read_type_not_ground"))?;
            let ty = self.intern_generic_ground_type(&original_type)?;
            if ty != allocation.ty || original_type != allocation.original_type { return Err(problem("lexical_capture_read_original_type_changed")); }
            let payload = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| problem("lexical_capture_read_payload"))?.to_vec().into_boxed_slice();
            if payload.as_ref() != [allocation.slot] { return Err(problem("lexical_capture_read_slot_changed")); }
            let tag = self.store.tags[instruction as usize];
            if !match tag {
                FullTag::ExprParam => true,
                FullTag::IntSlot => matches!(original_type, Type::Int | Type::UInt),
                FullTag::BoolSlot => original_type == Type::Bool,
                _ => false,
            } { return Err(problem("lexical_capture_read_original_port_type_changed")); }
            self.generic_evidence_mut().add_lexical_capture_source(LexicalCaptureSource {
                capture, binding: original.binding, declaration: original.caller, origin: original.origin,
                owner, instruction, tag, slot: allocation.slot, source_type: original.source_type, original_type, ty, payload,
            }).map_err(|_| problem("lexical_capture_read_original_allocation"))?;
        }
        Ok(())
    }
}

impl FullVerifier {
    fn verify_lexical_capture(store: &FullStore, capture: &LexicalCapture) -> Result<(), IrVerifyError> {
        let function = store.functions.get(capture.target.index()).ok_or_else(|| IrVerifyError::new("lexical capture target is missing"))?;
        let range = function.captures.bounds(store.captures.len()).ok_or_else(|| IrVerifyError::new("lexical capture header range is invalid"))?;
        let header = store.captures.get(capture.header_index as usize).filter(|_| range.contains(&(capture.header_index as usize))).ok_or_else(|| IrVerifyError::new("lexical capture changes its original header owner"))?;
        if header.slot_and_flags != (capture.slot | (u32::from(capture.mutable) << 31)) || header.type_id != capture.ty || header.name != capture.name
            || store.semantic.to_type(capture.ty)? != capture.original_type {
            return Err(IrVerifyError::new("lexical capture changes its original header slot or type"));
        }
        let generic = store.generic.as_deref().ok_or_else(|| IrVerifyError::new("lexical capture loses its original authority"))?;
        if generic.scope_for_function(capture.target).is_none() && generic.checked_function(capture.declaration)?.target != capture.target {
            return Err(IrVerifyError::new("lexical capture changes its checked declaration owner"));
        }
        Ok(())
    }

    pub(super) fn verify_lexical_capture_source(store: &FullStore, generic: &GenericEvidenceStore, source: &LexicalCaptureSource) -> Result<(), IrVerifyError> {
        let capture = generic.lexical_capture(source.capture)?;
        Self::verify_lexical_capture(store, capture)?;
        let range = store.function_instruction_range(capture.target.index())?;
        if source.owner != InstructionOwner::Function(capture.target) || source.declaration != capture.declaration || source.binding != capture.binding
            || source.slot != capture.slot || source.ty != capture.ty || source.original_type != capture.original_type
            || store.tags.get(source.instruction as usize) != Some(&source.tag)
            || !match source.tag {
                FullTag::ExprParam => true,
                FullTag::IntSlot => matches!(source.original_type, Type::Int | Type::UInt),
                FullTag::BoolSlot => source.original_type == Type::Bool,
                _ => false,
            }
            || !range.contains(&(source.instruction as usize))
            || store.payload(store.data[source.instruction as usize].range())? != source.payload.as_ref() || source.payload.as_ref() != [source.slot]
            || generic.registered_instruction_origin(source.instruction, false) != Some((OperationSourceOrigin::Expression(source.origin), source.owner)) {
            return Err(IrVerifyError::new("lexical capture read changes its original slot, binding or owner"));
        }
        Ok(())
    }

    pub(super) fn verify_lexical_captures(store: &FullStore, tree: &super::super::pattern::PatternTree) -> Result<(), IrVerifyError> {
        let Some(generic) = store.generic.as_deref().filter(|generic| generic.has_lexical_captures()) else { return Ok(()); };
        for (id, _) in generic.lexical_captures() { Self::verify_lexical_capture(store, generic.lexical_capture(id)?)?; }
        for (id, _) in generic.lexical_capture_sources() {
            let source = generic.lexical_capture_source(id)?;
            Self::verify_lexical_capture_source(store, generic, source)?;
            tree.parent(source.instruction)?;
        }
        Ok(())
    }

    pub(super) fn verify_lexical_capture_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, expected: &Type) -> Result<bool, IrVerifyError> {
        let Some(id) = generic.lexical_capture_source_at(instruction)? else { return Ok(false); };
        let source = generic.lexical_capture_source(id)?;
        if source.owner != owner || store.semantic.to_type(source.ty)? != *expected { return Err(IrVerifyError::new("lexical capture operand changes its original owner or type")); }
        Self::verify_lexical_capture_source(store, generic, source)?;
        Ok(true)
    }
}
