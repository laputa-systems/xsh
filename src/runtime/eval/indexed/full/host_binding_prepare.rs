use super::*;
use super::super::generic::{HostBindingCapture, HostBindingSource, graph_ground_type};

#[cfg(test)]
mod tests;

fn problem(reason: &'static str) -> IrBuildError { IrBuildError::format(reason, None, 0, 0) }

impl FullBuilder {
    pub(super) fn finalize_synthetic_host_captures(&mut self, function: IrFunctionId, body: &FunctionBuild) -> Result<(), IrBuildError> {
        if body.solved_declaration.is_some() || !body.captures.iter().any(|capture| capture.host_binding.is_some()) { return Ok(()); }
        if body.legacy_checked_signature.is_none() || self.current_owner != Some(function.raw())
            || self.current_slot_count as usize != body.slot_count {
            return Err(problem("host_capture_synthetic_owner_missing"));
        }
        let header = *self.store.functions.get(function.index()).ok_or_else(|| problem("host_capture_synthetic_header_missing"))?;
        let range = header.captures.bounds(self.store.captures.len()).ok_or_else(|| problem("host_capture_synthetic_header_invalid"))?;
        let originals = self.store.captures[range.clone()].to_vec();
        if originals.len() != body.captures.len() { return Err(problem("host_capture_synthetic_header_changed")); }
        let mut retained = Vec::with_capacity(originals.len());
        for (index, (capture, original)) in originals.iter().zip(body.captures.iter()).enumerate() {
            let slot = u32::try_from(original.slot).map_err(|_| problem("host_capture_synthetic_slot_overflow"))?;
            if capture.slot_and_flags != (slot | u32::from(original.mutable) << 31)
                || self.store.string(capture.name).map_err(|_| problem("host_capture_synthetic_name"))? != original.name.as_str().as_str() {
                return Err(problem("host_capture_synthetic_original_slot_changed"));
            }
            if let Some(binding) = original.host_binding {
                if original.mutable || original.name != binding.name() || self.store.semantic.to_type(capture.type_id).map_err(|_| problem("host_capture_synthetic_type"))? != binding.ty() {
                    return Err(problem("host_capture_synthetic_original_host_changed"));
                }
                if self.encoded_slot_uses.iter().any(|(owner, used)| *owner == function.raw() && *used == original.slot) {
                    let header_index = u32::try_from(range.start + index).map_err(|_| problem("host_capture_synthetic_header_overflow"))?;
                    self.stage_host_binding_capture(function, None, header_index, original)?;
                    return Err(problem("host_capture_synthetic_used_source_missing"));
                }
            } else { retained.push(*capture); }
        }
        // Other headers and their sealed allocations keep their original
        // indices. Only this wrapper publishes the smaller capture range.
        let start = self.store.captures.len();
        self.store.captures.extend(retained);
        self.store.functions[function.index()].captures = table_range(start, self.store.captures.len())?;
        Ok(())
    }

    pub(super) fn stage_host_binding_capture(&mut self, target: IrFunctionId, declaration: Option<crate::sema::check::DeclarationIdentity>, header_index: u32, original: &LoweredTopLevelSlot) -> Result<(), IrBuildError> {
        let Some(binding) = original.host_binding else { return Ok(()); };
        let declaration = declaration.ok_or_else(|| problem("host_capture_original_declaration_missing"))?;
        let solved = self.solved.as_ref().ok_or_else(|| problem("host_capture_checked_source_missing"))?;
        if !solved.declarations.contains_key(&declaration) || original.mutable || original.name != binding.name() {
            return Err(problem("host_capture_original_allocation_changed"));
        }
        let header = *self.store.captures.get(header_index as usize).ok_or_else(|| problem("host_capture_header_missing"))?;
        let slot = u32::try_from(original.slot).map_err(|_| problem("host_capture_slot_overflow"))?;
        if header.slot_and_flags != slot || self.store.string(header.name).map_err(|_| problem("host_capture_header_name"))? != binding.name().as_str().as_str()
            || self.store.semantic.to_type(header.type_id).map_err(|_| problem("host_capture_header_type"))? != binding.ty() {
            return Err(problem("host_capture_original_header_changed"));
        }
        self.generic_evidence_mut().add_host_binding_capture(HostBindingCapture { binding, declaration, target, header_index, slot, ty: header.type_id, name: header.name })
            .map_err(|_| problem("host_capture_original_receipt_allocation"))?;
        Ok(())
    }
    pub(super) fn stage_host_binding_read(&mut self, instruction: u32, origin: crate::sema::check::ExpressionIdentity, owner: InstructionOwner, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        let Some(original) = scratch.host_binding_reads.get(&origin) else { return Ok(()); };
        if self.store.tags.get(instruction as usize) != Some(&FullTag::ExprParam)
            || self.active_expression_origins.get(&original.expression) != Some(&origin) {
            return Err(problem("host_binding_original_read_changed"));
        }
        self.host_binding_read_rows.push((instruction, original.clone(), owner));
        Ok(())
    }

    pub(super) fn prepare_host_bindings(&mut self) -> Result<(), IrBuildError> {
        if self.host_binding_read_rows.is_empty() { return Ok(()); }
        let solved = self.solved.clone().ok_or_else(|| problem("host_binding_checked_source_missing"))?;
        for (instruction, original, owner) in self.host_binding_read_rows.clone() {
            if original.origin != *self.generic_expression_rows.iter().find_map(|(row, origin, actual_owner)| (*row == instruction && *actual_owner == owner).then_some(origin)).ok_or_else(|| problem("host_binding_original_source_missing"))?
                || solved.expression_owners.get(&original.origin).copied() != original.caller
                || solved.expressions.get(&original.origin) != Some(&original.source_type.ty)
                || solved.expression_scope(original.origin, original.caller).map_err(|_| problem("host_binding_original_scope"))? != original.source_type.scope {
                return Err(problem("host_binding_original_source_changed"));
            }
            solved.graph.validate_scoped(original.source_type).map_err(|_| problem("host_binding_original_type_scope"))?;
            let ty = graph_ground_type(&solved.graph, original.source_type.ty).map_err(|_| problem("host_binding_original_type_not_ground"))?;
            if ty != original.binding.ty() { return Err(problem("host_binding_original_type_changed")); }
            let ty = self.intern_generic_ground_type(&ty)?;
            let slot_index = u32::try_from(original.slot).map_err(|_| problem("host_binding_original_slot_overflow"))?;
            let payload = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| problem("host_binding_original_read_payload"))?.to_vec().into_boxed_slice();
            if payload.as_ref() != [slot_index] { return Err(problem("host_binding_original_read_slot_changed")); }
            if let InstructionOwner::Function(target) = owner {
                let (id, capture) = self.generic_evidence_mut().host_binding_capture_for_slot(target, slot_index).map_err(|_| problem("host_binding_original_capture_lookup"))?
                    .map(|(id, capture)| (id, capture.clone())).ok_or_else(|| problem("host_binding_original_capture_missing"))?;
                if original.caller != Some(capture.declaration) || capture.binding != original.binding || capture.ty != ty {
                    return Err(problem("host_binding_original_capture_changed"));
                }
                self.generic_evidence_mut().add_host_binding_source(HostBindingSource {
                    binding: original.binding, origin: original.origin, instruction, owner, slot: slot_index, ty,
                    source_type: original.source_type, scope_start: 0, scope_end: 0, slot_name: capture.name, slot_flags: 0,
                    payload, capture: Some(id),
                }).map_err(|_| problem("host_binding_original_receipt_allocation"))?;
                continue;
            }
            let InstructionOwner::Driver(step) = owner else { unreachable!() };
            if original.caller.is_some() { return Err(problem("host_binding_original_driver_scope_changed")); }
            let driver = *self.store.driver_steps.get(step as usize).ok_or_else(|| problem("host_binding_driver_owner"))?;
            let slots = driver.slots.bounds(self.store.driver_slots.len()).ok_or_else(|| problem("host_binding_driver_slots"))?;
            let slot = *self.store.driver_slots[slots].iter().find(|slot| slot.slot as usize == original.slot).ok_or_else(|| problem("host_binding_original_slot_missing"))?;
            if self.store.string(slot.name).map_err(|_| problem("host_binding_original_slot_name"))? != original.binding.name().as_str().as_str()
                || slot.type_id != ty || slot.flags != DRIVER_SLOT_READ {
                return Err(problem("host_binding_original_slot_changed"));
            }
            let program = *self.store.driver_programs.iter().find(|program| program.steps.bounds(self.store.driver_steps.len()).is_some_and(|steps| steps.contains(&(step as usize)))).ok_or_else(|| problem("host_binding_initial_scope_missing"))?;
            let scope = program.steps.bounds(self.store.driver_steps.len()).ok_or_else(|| problem("host_binding_initial_scope_invalid"))?;
            self.generic_evidence_mut().add_host_binding_source(HostBindingSource {
                binding: original.binding, origin: original.origin, instruction, owner, slot: slot_index, ty,
                source_type: original.source_type, scope_start: scope.start as u32, scope_end: scope.end as u32,
                slot_name: slot.name, slot_flags: slot.flags, payload, capture: None,
            }).map_err(|_| problem("host_binding_original_receipt_allocation"))?;
        }
        Ok(())
    }
}

impl FullVerifier {
    fn verify_host_binding_capture(store: &FullStore, capture: &HostBindingCapture) -> Result<(), IrVerifyError> {
        let function = store.functions.get(capture.target.index()).ok_or_else(|| IrVerifyError::new("host capture target is missing"))?;
        let range = function.captures.bounds(store.captures.len()).ok_or_else(|| IrVerifyError::new("host capture target header is invalid"))?;
        let header = store.captures.get(capture.header_index as usize).filter(|_| range.contains(&(capture.header_index as usize))).ok_or_else(|| IrVerifyError::new("host capture changes its original header owner"))?;
        if header.slot_and_flags != capture.slot || header.type_id != capture.ty || header.name != capture.name
            || store.string(header.name)? != capture.binding.name().as_str().as_str() || store.semantic.to_type(header.type_id)? != capture.binding.ty() {
            return Err(IrVerifyError::new("host capture changes its original hydrated slot"));
        }
        let generic = store.generic.as_deref().ok_or_else(|| IrVerifyError::new("host capture loses its original authority"))?;
        if generic.scope_for_function(capture.target).is_none() && generic.checked_function(capture.declaration)?.target != capture.target {
            return Err(IrVerifyError::new("host capture changes its checked declaration owner"));
        }
        Ok(())
    }

    fn verify_host_binding_source(store: &FullStore, source: &HostBindingSource) -> Result<(), IrVerifyError> {
        if let InstructionOwner::Function(target) = source.owner {
            let generic = store.generic.as_deref().ok_or_else(|| IrVerifyError::new("host read loses its original capture authority"))?;
            let capture = generic.host_binding_capture(source.capture.ok_or_else(|| IrVerifyError::new("host read has no original capture receipt"))?)?;
            Self::verify_host_binding_capture(store, capture)?;
            let range = store.function_instruction_range(target.index())?;
            if capture.target != target || capture.slot != source.slot || capture.binding != source.binding || capture.ty != source.ty
                || source.scope_start != 0 || source.scope_end != 0 || source.slot_name != capture.name || source.slot_flags != 0
                || store.tags.get(source.instruction as usize) != Some(&FullTag::ExprParam)
                || store.payload(store.data[source.instruction as usize].range())? != source.payload.as_ref()
                || source.payload.as_ref() != [source.slot] || !range.contains(&(source.instruction as usize)) {
                return Err(IrVerifyError::new("host read changes its original captured slot or owner"));
            }
            for instruction in range {
                if matches!(store.tags[instruction], FullTag::StmtAssign | FullTag::StmtAssignInt | FullTag::StmtAssignBool | FullTag::StmtAssignField | FullTag::StmtAssignFieldInt | FullTag::StmtAssignPath)
                    && store.payload(store.data[instruction].range())?.first() == Some(&source.slot) {
                    return Err(IrVerifyError::new("host capture has an unprepared write in its function scope"));
                }
            }
            return Ok(());
        }
        let InstructionOwner::Driver(step) = source.owner else { return Err(IrVerifyError::new("host binding has no driver hydration owner")); };
        if source.capture.is_some() { return Err(IrVerifyError::new("host driver read carries a foreign capture environment")); }
        let driver = store.driver_steps.get(step as usize).ok_or_else(|| IrVerifyError::new("host binding driver owner is missing"))?;
        let slots = driver.slots.bounds(store.driver_slots.len()).ok_or_else(|| IrVerifyError::new("host binding driver slots are invalid"))?;
        let slot = store.driver_slots[slots].iter().find(|slot| slot.slot == source.slot).ok_or_else(|| IrVerifyError::new("host binding loses its original slot"))?;
        let program = store.driver_programs.iter().find(|program| program.steps.bounds(store.driver_steps.len()).is_some_and(|steps| steps.contains(&(step as usize)))).ok_or_else(|| IrVerifyError::new("host binding loses its initial driver scope"))?;
        let scope = program.steps.bounds(store.driver_steps.len()).ok_or_else(|| IrVerifyError::new("host binding initial scope is invalid"))?;
        if scope.start as u32 != source.scope_start || scope.end as u32 != source.scope_end
            || slot.name != source.slot_name || slot.flags != source.slot_flags || slot.flags != DRIVER_SLOT_READ || slot.type_id != source.ty
            || store.string(slot.name)? != source.binding.name().as_str().as_str()
            || store.semantic.to_type(source.ty)? != source.binding.ty()
            || store.tags.get(source.instruction as usize) != Some(&FullTag::ExprParam)
            || store.payload(store.data[source.instruction as usize].range())? != source.payload.as_ref()
            || !store.driver_instruction_range(step as usize)?.contains(&(source.instruction as usize)) {
            return Err(IrVerifyError::new("host binding changes its original read or hydrated slot"));
        }
        for index in scope.start..step as usize {
            let previous = store.driver_steps[index];
            if matches!(previous.tag, FullDriverTag::Let | FullDriverTag::Assign) {
                let words = store.payload(previous.data.range())?;
                if words.first().copied().map(|raw| Name::from_symbol(Symbol::from_raw(raw))) == Some(source.binding.name()) {
                    return Err(IrVerifyError::new("host binding was replaced or written before its read"));
                }
            }
            if previous.tag == FullDriverTag::LetRecord {
                let decoder = FullDecoder { store, owner: driver_owner(index).map_err(|_| IrVerifyError::new("host binding preceding driver owner is invalid"))?, instruction_range: store.driver_instruction_range(index)?, instruction_states: None, block_states: None, slot_count: previous.slot_count, pattern_tree: None, pattern_ceiling: Cell::new(usize::MAX), verified: true };
                let mut words = FullCursor::new(store.payload(previous.data.range())?);
                BuildExprId::decode(&decoder, &mut words)?;
                let fields = Vec::<(Name, usize)>::decode(&decoder, &mut words)?;
                if fields.iter().any(|(name, _)| *name == source.binding.name()) { return Err(IrVerifyError::new("host binding was shadowed by a record binding")); }
            }
        }
        for instruction in store.driver_instruction_range(step as usize)? {
            if matches!(store.tags[instruction], FullTag::StmtAssign | FullTag::StmtAssignInt | FullTag::StmtAssignBool | FullTag::StmtAssignField | FullTag::StmtAssignFieldInt | FullTag::StmtAssignPath)
                && store.payload(store.data[instruction].range())?.first() == Some(&source.slot) {
                return Err(IrVerifyError::new("host binding has an unprepared write in its driver scope"));
            }
        }
        Ok(())
    }

    pub(super) fn verify_host_bindings(store: &FullStore, tree: &super::super::pattern::PatternTree) -> Result<(), IrVerifyError> {
        let Some(generic) = store.generic.as_deref().filter(|generic| generic.has_host_bindings()) else { return Ok(()); };
        for (id, _) in generic.host_binding_captures() { Self::verify_host_binding_capture(store, generic.host_binding_capture(id)?)?; }
        for (id, _) in generic.host_binding_sources() {
            let source = generic.host_binding_source(id)?;
            Self::verify_host_binding_source(store, source)?;
            tree.parent(source.instruction)?;
        }
        Ok(())
    }

    pub(super) fn verify_host_binding_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, expected: &Type) -> Result<bool, IrVerifyError> {
        let Some(id) = generic.host_binding_source_at(instruction)? else { return Ok(false); };
        let source = generic.host_binding_source(id)?;
        if source.owner != owner || store.semantic.to_type(source.ty)? != *expected { return Err(IrVerifyError::new("host binding operand changes its original owner or type")); }
        Self::verify_host_binding_source(store, source)?;
        Ok(true)
    }
}
