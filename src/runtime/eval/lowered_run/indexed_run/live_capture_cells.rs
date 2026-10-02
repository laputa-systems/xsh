use super::*;
use crate::sema::check::{BindingIdentity, DeclarationIdentity};
use crate::runtime::eval::indexed::{IrFunctionId, TypeId};
use std::sync::{Mutex, Weak};

// A cell belongs to an original allocation and one defining activation. Receiving
// declarations share that cell; returning from a call never publishes a snapshot.
#[derive(Clone)]
pub(in crate::runtime::eval) struct LiveCaptureCell(Arc<LiveCaptureCellData>);

struct LiveCaptureCellData {
    program: Weak<FullProgram>,
    binding: BindingIdentity,
    definition_owner: Option<DeclarationIdentity>,
    ty: TypeId,
    value: Mutex<LoweredValue>,
}

impl std::fmt::Debug for LiveCaptureCell {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("LiveCaptureCell").field("binding", &self.0.binding).field("definition_owner", &self.0.definition_owner).finish()
    }
}

impl LiveCaptureCell {
    pub(in crate::runtime::eval) fn value(&self) -> LoweredValue {
        self.0.value.lock().expect("live capture lock is not poisoned").clone()
    }

    fn set(&self, value: LoweredValue) {
        *self.0.value.lock().expect("live capture lock is not poisoned") = value;
    }

    fn matches_program(&self, program: &Arc<FullProgram>) -> bool {
        self.0.program.ptr_eq(&Arc::downgrade(program))
    }

    pub(in crate::runtime::eval) fn validate_capture(&self, program: &Arc<FullProgram>, target: IrFunctionId, slot: usize, span: Span) -> Result<(), RuntimeError> {
        let evidence = program.generic_evidence().ok_or_else(|| failure("live callable capture has no original evidence", span))?;
        let (_, original) = evidence.lexical_capture_for_slot(target, slot as u32).map_err(|error| indexed_error(error, span))?
            .ok_or_else(|| failure("live callable capture has no original lexical allocation", span))?;
        if !supports_live_capture_type(&original.original_type) {
            return Err(failure("live capture binding requires a supported ground value ownership contract", span));
        }
        if !self.matches_program(program) || !original.mutable || self.0.binding != original.binding
            || self.0.definition_owner != original.definition_owner || self.0.ty != original.ty
            || !super::super::super::lowered_value_matches_static_type(&self.value(), &original.original_type) {
            return Err(failure("live callable capture changes its original program, binding, owner or type", span));
        }
        Ok(())
    }
}

#[derive(Default)]
pub(in crate::runtime::eval) struct LiveCaptureCells {
    roots: Vec<LiveCaptureCell>,
    frames: Vec<LiveCaptureFrame>,
    drivers: Vec<LiveCaptureDriver>,
}

struct LiveCaptureDriver {
    address: usize,
    program: Weak<FullProgram>,
    step: u32,
    slots: Vec<(usize, LiveCaptureCell)>,
}

struct LiveCaptureFrame {
    address: usize,
    program: Weak<FullProgram>,
    declaration: DeclarationIdentity,
    target: IrFunctionId,
    slots: Vec<(usize, LiveCaptureCell)>,
}

pub(in crate::runtime::eval) struct CompletedLiveCaptureFrame {
    address: usize,
    program: Weak<FullProgram>,
    declaration: DeclarationIdentity,
    target: IrFunctionId,
    slots: Vec<(usize, LiveCaptureCell)>,
}

impl CompletedLiveCaptureFrame {
    pub(in crate::runtime::eval) fn validate(&self, header: &FunctionHeader, slots: &[LoweredValue], namespace: Option<Name>, span: Span) -> Result<(), RuntimeError> {
        if self.address != slots.as_ptr() as usize || self.declaration.namespace != namespace { return Err(failure("live capture completion changes its original activation or declaration", span)); }
        let program = self.program.upgrade().ok_or_else(|| failure("live capture completion loses its original program", span))?;
        let original = program.function_view_by_id(self.target).map_err(|error| indexed_error(error, span))?.header().map_err(|error| indexed_error(error, span))?;
        if header.captures.len() != original.captures.len() || !header.captures.iter().zip(&original.captures).all(|(actual, original)| actual.slot == original.slot && actual.name == original.name && actual.kind == original.kind && actual.mutable == original.mutable && actual.host_binding == original.host_binding) {
            return Err(failure("live capture completion changes its original receiving header", span));
        }
        for (slot, cell) in &self.slots { cell.validate_capture(&program, self.target, *slot, span)?; }
        Ok(())
    }

    pub(in crate::runtime::eval) fn local_capture(&self, slot: usize) -> Option<&LiveCaptureCell> {
        self.slots.iter().find_map(|(original, cell)| (*original == slot && cell.0.definition_owner.is_some()).then_some(cell))
    }
}

fn failure(message: &str, span: Span) -> RuntimeError {
    RuntimeError::new("indexed-ir", message).with_span(span)
}

// Producers, host handles and callable values carry separate ownership lifetimes.
// An ordinary live binding cell only owns values whose lifetime is its storage.
fn supports_live_capture_type(ty: &Type) -> bool {
    crate::runtime::eval::lower::mutable_binding::supports_mutable_binding_type(ty)
}

impl LiveCaptureCells {
    pub(in crate::runtime::eval) fn begin_driver(&mut self, program: &Arc<FullProgram>, step: u32, address: usize, span: Span) -> Result<(), RuntimeError> {
        let evidence = program.generic_evidence().ok_or_else(|| failure("live driver frame has no original evidence", span))?;
        if self.drivers.iter().any(|driver| driver.address == address) { return Err(failure("live driver reuses an active slot allocation", span)); }
        let mut slots: Vec<(usize, LiveCaptureCell)> = Vec::new();
        for original in evidence.mutable_binding_receipts().filter(|original| original.owner == crate::runtime::eval::indexed::generic::InstructionOwner::Driver(step)) {
            let original = evidence.mutable_binding_receipt(original.instruction).map_err(|error| indexed_error(error, span))?
                .ok_or_else(|| failure("live driver read loses its original mutable receipt", span))?;
            if !matches!(original.tag, FullTag::ExprParam | FullTag::IntSlot | FullTag::BoolSlot | FullTag::StmtAssign | FullTag::StmtAssignInt | FullTag::StmtAssignBool | FullTag::StmtAssignField | FullTag::StmtAssignFieldInt | FullTag::StmtAssignPath) { continue; }
            let Some(cell) = self.roots.iter().find(|cell| cell.matches_program(program) && cell.0.binding == original.binding) else { continue; };
            if cell.0.ty != original.binding_type { return Err(failure("live driver slot changes its original binding type", span)); }
            let slot = *original.payload.first().ok_or_else(|| failure("live driver read has no original slot", span))? as usize;
            if let Some((_, existing)) = slots.iter().find(|(existing, _)| *existing == slot) {
                if !Arc::ptr_eq(&existing.0, &cell.0) { return Err(failure("live driver slot names multiple original allocations", span)); }
            } else { slots.push((slot, cell.clone())); }
        }
        self.drivers.push(LiveCaptureDriver { address, program: Arc::downgrade(program), step, slots });
        Ok(())
    }

    pub(in crate::runtime::eval) fn finish_driver(&mut self, address: usize) {
        if let Some(index) = self.drivers.iter().rposition(|driver| driver.address == address) { self.drivers.remove(index); }
    }

    pub(in crate::runtime::eval) fn register_driver(&mut self, program: &Arc<FullProgram>, step: u32, value: Option<LoweredValue>, span: Span) -> Result<(), RuntimeError> {
        let evidence = program.generic_evidence().ok_or_else(|| failure("live binding allocation has no original evidence", span))?;
        let source = evidence.mutable_driver_receipt(step).map_err(|error| indexed_error(error, span))?
            .ok_or_else(|| failure("live binding allocation lacks its original driver receipt", span))?;
        let Some((id, _)) = evidence.lexical_captures().find(|(_, capture)| capture.mutable && capture.binding == source.binding && capture.definition_owner.is_none()) else { return Ok(()); };
        let capture = evidence.lexical_capture(id).map_err(|error| indexed_error(error, span))?;
        if !supports_live_capture_type(&capture.original_type) { return Ok(()); }
        let value = value.ok_or_else(|| failure("original live captured binding cannot cross the value boundary", span))?;
        if capture.ty != source.binding_type || !super::super::super::lowered_value_matches_static_type(&value, &capture.original_type) {
            return Err(failure("live driver allocation changes its original captured binding type", span));
        }
        if source.assignment.is_some() {
            let cell = self.roots.iter().find(|cell| cell.matches_program(program) && cell.0.binding == source.binding)
                .ok_or_else(|| failure("live binding assignment has no original allocation", span))?;
            if cell.0.ty != source.binding_type { return Err(failure("live binding assignment changes its original type", span)); }
            cell.set(value);
            return Ok(());
        }
        if self.roots.iter().any(|cell| cell.matches_program(program) && cell.0.binding == source.binding) {
            return Err(failure("live binding driver allocation was executed twice", span));
        }
        self.roots.push(LiveCaptureCell(Arc::new(LiveCaptureCellData {
            program: Arc::downgrade(program), binding: source.binding, definition_owner: None,
            ty: source.binding_type, value: Mutex::new(value),
        })));
        Ok(())
    }

    pub(in crate::runtime::eval) fn begin_frame(&mut self, program: &Arc<FullProgram>, target: IrFunctionId, address: usize, header: &FunctionHeader, span: Span) -> Result<(), RuntimeError> {
        self.begin_frame_with_cells(program, target, address, header, None, span)
    }

    pub(in crate::runtime::eval) fn capture_cell(&self, program: &Arc<FullProgram>, target: IrFunctionId, slot: usize, span: Span) -> Result<LiveCaptureCell, RuntimeError> {
        let evidence = program.generic_evidence().ok_or_else(|| failure("live capture has no original evidence", span))?;
        let (_, original) = evidence.lexical_capture_for_slot(target, slot as u32).map_err(|error| indexed_error(error, span))?
            .ok_or_else(|| failure("mutable capture lacks its original lexical allocation", span))?;
        let cell = self.frames.iter().rev().filter(|frame| frame.program.ptr_eq(&Arc::downgrade(program)))
            .flat_map(|frame| frame.slots.iter()).map(|(_, cell)| cell).chain(self.roots.iter())
            .find(|cell| cell.matches_program(program) && cell.0.binding == original.binding && cell.0.definition_owner == original.definition_owner)
            .ok_or_else(|| failure("mutable capture has no live original defining allocation", span))?;
        cell.validate_capture(program, target, slot, span)?;
        Ok(cell.clone())
    }

    pub(in crate::runtime::eval) fn begin_frame_with_cells(&mut self, program: &Arc<FullProgram>, target: IrFunctionId, address: usize, header: &FunctionHeader, cells: Option<&[(usize, LiveCaptureCell)]>, span: Span) -> Result<(), RuntimeError> {
        let evidence = program.generic_evidence().ok_or_else(|| failure("live capture frame has no original evidence", span))?;
        let declaration = evidence.checked_functions().find_map(|(_, source)| (source.target == target).then_some(source.declaration))
            .or_else(|| evidence.lexical_captures().find_map(|(_, source)| (source.target == target).then_some(source.declaration)))
            .ok_or_else(|| failure("live capture frame has no original declaration", span))?;
        if self.frames.iter().any(|frame| frame.address == address) { return Err(failure("live capture frame reuses an active slot allocation", span)); }
        let mut slots = Vec::new();
        for capture in header.captures.iter().filter(|capture| capture.mutable) {
            let (_, source) = evidence.lexical_capture_for_slot(target, capture.slot as u32).map_err(|error| indexed_error(error, span))?
                .ok_or_else(|| failure("mutable capture lacks its original lexical allocation", span))?;
            if !supports_live_capture_type(&source.original_type) { continue; }
            if !source.mutable || source.declaration != declaration {
                return Err(failure("mutable capture changes its original receiving declaration", span));
            }
            let cell = match cells {
                Some(cells) => cells.iter().find_map(|(slot, cell)| (*slot == capture.slot).then_some(cell.clone()))
                    .ok_or_else(|| failure("stored mutable capture has no original live cell", span))?,
                None => self.capture_cell(program, target, capture.slot, span)?,
            };
            cell.validate_capture(program, target, capture.slot, span)?;
            slots.push((capture.slot, cell));
        }
        if cells.is_some_and(|cells| cells.len() != slots.len()) { return Err(failure("stored mutable environment changes its original capture slots", span)); }
        self.frames.push(LiveCaptureFrame { address, program: Arc::downgrade(program), declaration, target, slots });
        Ok(())
    }

    pub(in crate::runtime::eval) fn register_local(&mut self, program: &Arc<FullProgram>, instruction: u32, address: usize, value: LoweredValue, span: Span) -> Result<(), RuntimeError> {
        let evidence = program.generic_evidence().ok_or_else(|| failure("live local allocation has no original evidence", span))?;
        let source = evidence.mutable_binding_receipt(instruction).map_err(|error| indexed_error(error, span))?
            .ok_or_else(|| failure("live local allocation lacks its original mutable receipt", span))?;
        if source.assignment.is_some() || !matches!(source.tag, FullTag::StmtLet | FullTag::StmtLetInt | FullTag::StmtLetBool) {
            return Err(failure("live local allocation requires its original binding declaration", span));
        }
        let frame = self.frames.iter_mut().rev().find(|frame| frame.address == address && frame.program.ptr_eq(&Arc::downgrade(program)))
            .ok_or_else(|| failure("live local allocation has no defining frame", span))?;
        let target = evidence.checked_function(frame.declaration).map_err(|error| indexed_error(error, span))?.target;
        if source.owner != crate::runtime::eval::indexed::generic::InstructionOwner::Function(target) {
            return Err(failure("live local allocation changes its original defining declaration", span));
        }
        let Some((id, _)) = evidence.lexical_captures().find(|(_, capture)| capture.mutable && capture.binding == source.binding && capture.definition_owner == Some(frame.declaration)) else { return Ok(()); };
        let capture = evidence.lexical_capture(id).map_err(|error| indexed_error(error, span))?;
        if !supports_live_capture_type(&capture.original_type) { return Ok(()); }
        if capture.ty != source.binding_type || !super::super::super::lowered_value_matches_static_type(&value, &capture.original_type) {
            return Err(failure("live local allocation changes its original captured binding type", span));
        }
        let slot = *source.payload.first().ok_or_else(|| failure("live local allocation has no original slot", span))? as usize;
        if let Some(index) = frame.slots.iter().position(|(existing, _)| *existing == slot) {
            if frame.slots[index].1.0.definition_owner != Some(frame.declaration) {
                return Err(failure("live local allocation replaces a receiving capture slot", span));
            }
            // Re-entering a lexical allocation creates a distinct cell. A stored
            // callable can continue owning the previous activation independently.
            frame.slots.remove(index);
        }
        frame.slots.push((slot, LiveCaptureCell(Arc::new(LiveCaptureCellData {
            program: Arc::downgrade(program), binding: source.binding, definition_owner: Some(frame.declaration),
            ty: source.binding_type, value: Mutex::new(value),
        }))));
        Ok(())
    }

    pub(in crate::runtime::eval) fn read_slot(&self, address: usize, slot: usize) -> Option<LoweredValue> {
        self.cell_for_slot(address, slot).map(LiveCaptureCell::value)
    }

    fn write_slot(&self, address: usize, slot: usize, value: LoweredValue) -> bool {
        let Some(cell) = self.cell_for_slot(address, slot) else { return false; };
        cell.set(value);
        true
    }

    pub(in crate::runtime::eval) fn cell_for_slot(&self, address: usize, slot: usize) -> Option<&LiveCaptureCell> {
        let slots = self.frames.iter().rev().find(|frame| frame.address == address).map(|frame| &frame.slots)
            .or_else(|| self.drivers.iter().rev().find(|driver| driver.address == address).map(|driver| &driver.slots))?;
        slots.iter().find_map(|(candidate, cell)| (*candidate == slot).then_some(cell))
    }

    pub(in crate::runtime::eval) fn refresh_slots(&self, slots: &mut [LoweredValue]) {
        if let Some(frame) = self.frames.iter().rev().find(|frame| frame.address == slots.as_ptr() as usize) {
            for (slot, cell) in &frame.slots { slots[*slot] = cell.value(); }
        } else if let Some(driver) = self.drivers.iter().rev().find(|driver| driver.address == slots.as_ptr() as usize) {
            for (slot, cell) in &driver.slots { slots[*slot] = cell.value(); }
        }
    }

    pub(in crate::runtime::eval) fn refresh_slot(&self, slots: &mut [LoweredValue], slot: usize) {
        if let Some(value) = self.read_slot(slots.as_ptr() as usize, slot) { slots[slot] = value; }
    }

    fn publish_slot(&self, program: &Arc<FullProgram>, execution: &FullExecution<'_>, instruction: u32, slots: &[LoweredValue], slot: usize, span: Span) -> Result<(), RuntimeError> {
        let address = slots.as_ptr() as usize;
        let Some(cell) = self.cell_for_slot(address, slot) else { return Ok(()); };
        let evidence = execution.generic_evidence().ok_or_else(|| failure("live mutable write has no original evidence", span))?;
        if !program.generic_evidence().is_some_and(|original| std::ptr::eq(original, evidence)) { return Err(failure("live mutable write belongs to another program", span)); }
        let (original_binding, original_type, original_owner, original_slot, original_capture) = if let Some(original) = evidence.mutable_binding_receipt(instruction).map_err(|error| indexed_error(error, span))? {
            if original.assignment.is_none() { return Err(failure("live mutable write substitutes an original read or allocation", span)); }
            (original.binding, original.binding_type, original.owner, *original.payload.first().ok_or_else(|| failure("live mutable write lacks its original slot", span))?, original.capture)
        } else if let Some(original) = evidence.mutable_path_at(instruction).map_err(|error| indexed_error(error, span))? {
            (original.binding, original.binding_type, original.owner, original.slot, None)
        } else { return Err(failure("live mutable write lacks its original assignment authority", span)); };
        let (owner_program, owner) = if let Some(frame) = self.frames.iter().rev().find(|frame| frame.address == address) {
            if cell.0.definition_owner != Some(frame.declaration) {
                let (id, _) = evidence.lexical_capture_for_slot(frame.target, slot as u32).map_err(|error| indexed_error(error, span))?
                    .ok_or_else(|| failure("live mutable write loses its original receiving capture", span))?;
                if original_capture != Some(id) { return Err(failure("live mutable write substitutes another receiving capture allocation", span)); }
            } else if original_capture.is_some() { return Err(failure("live local write substitutes a receiving capture allocation", span)); }
            (&frame.program, crate::runtime::eval::indexed::generic::InstructionOwner::Function(frame.target))
        } else if let Some(driver) = self.drivers.iter().rev().find(|driver| driver.address == address) {
            (&driver.program, crate::runtime::eval::indexed::generic::InstructionOwner::Driver(driver.step))
        } else { return Err(failure("live mutable write has no original active allocation", span)); };
        if !owner_program.ptr_eq(&Arc::downgrade(program)) || !cell.matches_program(program) || original_owner != owner || original_binding != cell.0.binding || original_type != cell.0.ty || original_slot as usize != slot {
            return Err(failure("live mutable write changes its original program, owner, binding or slot", span));
        }
        cell.set(slots[slot].clone());
        Ok(())
    }

    pub(in crate::runtime::eval) fn retire_local_slots(&mut self, address: usize, slots: &[usize]) {
        if let Some(frame) = self.frames.iter_mut().rev().find(|frame| frame.address == address) {
            frame.slots.retain(|(slot, cell)| !slots.contains(slot) || cell.0.definition_owner != Some(frame.declaration));
        }
    }

    fn has_frame(&self, address: usize) -> bool { self.frames.iter().any(|frame| frame.address == address) }

    pub(in crate::runtime::eval) fn finish_frame(&mut self, slots: &mut [LoweredValue], span: Span) -> Result<CompletedLiveCaptureFrame, RuntimeError> {
        let address = slots.as_ptr() as usize;
        let Some(index) = self.frames.iter().rposition(|frame| frame.address == address) else { return Err(failure("live capture frame completion has no active allocation", span)); };
        self.refresh_slots(slots);
        let frame = self.frames.remove(index);
        let slots = frame.slots.into_iter().filter(|(_, cell)| cell.0.definition_owner != Some(frame.declaration)).collect();
        Ok(CompletedLiveCaptureFrame { address, program: frame.program, declaration: frame.declaration, target: frame.target, slots })
    }
}

impl Evaluator {
    pub(super) fn refresh_indexed_live_slots(&self, slots: &mut [LoweredValue]) {
        self.live_capture_cells.lock().expect("live capture registry is not poisoned").refresh_slots(slots);
    }

    pub(super) fn publish_indexed_live_slot(&self, execution: &FullExecution<'_>, instruction: u32, slots: &[LoweredValue], slot: usize, span: Span) -> Result<(), RuntimeError> {
        let registry = self.live_capture_cells.lock().expect("live capture registry is not poisoned");
        if registry.cell_for_slot(slots.as_ptr() as usize, slot).is_none() { return Ok(()); }
        let program = self.indexed_program.as_ref().ok_or_else(|| failure("live mutable write has no installed program", span))?;
        registry.publish_slot(program, execution, instruction, slots, slot, span)
    }

    pub(super) fn register_indexed_live_local(&self, execution: &FullExecution<'_>, instruction: u32, slots: &[LoweredValue], slot: usize, span: Span) -> Result<(), RuntimeError> {
        let Some(evidence) = execution.generic_evidence() else { return Ok(()); };
        let Some(original) = evidence.mutable_binding_receipt(instruction).map_err(|error| indexed_error(error, span))? else { return Ok(()); };
        if original.assignment.is_some() || !matches!(original.owner, crate::runtime::eval::indexed::generic::InstructionOwner::Function(_)) { return Ok(()); }
        let mut registry = self.live_capture_cells.lock().expect("live capture registry is not poisoned");
        if !registry.has_frame(slots.as_ptr() as usize) { return Ok(()); }
        let program = self.indexed_program.as_ref().ok_or_else(|| failure("live local has no installed program", span))?;
        registry.register_local(program, instruction, slots.as_ptr() as usize, slots[slot].clone(), span)
    }

    pub(super) fn complete_indexed_live_statement(&self, execution: &FullExecution<'_>, instruction: u32, slots: &[LoweredValue], span: Span) -> Result<(), RuntimeError> {
        let Some(evidence) = execution.generic_evidence() else { return Ok(()); };
        let Some(original) = evidence.mutable_binding_receipt(instruction).map_err(|error| indexed_error(error, span))? else { return Ok(()); };
        let slot = *original.payload.first().ok_or_else(|| failure("live mutable statement has no original slot", span))? as usize;
        if original.assignment.is_some() { Ok(()) }
        else if matches!(original.tag, FullTag::StmtLet | FullTag::StmtLetInt | FullTag::StmtLetBool) { self.register_indexed_live_local(execution, instruction, slots, slot, span) }
        else { Ok(()) }
    }

    pub(super) fn begin_indexed_live_frame(&self, view: FullFunctionView<'_>, header: &FunctionHeader, slots: &mut [LoweredValue], captures: Option<&[crate::runtime::eval::callable_value::RuntimeCallableCapture]>, span: Span) -> Result<(), RuntimeError> {
        let program = self.indexed_program.as_ref().ok_or_else(|| failure("live capture frame has no installed program", span))?;
        if !view.belongs_to_program(program.as_ref()) { return Err(failure("live capture frame belongs to another program", span)); }
        let target = view.function_id();
        let Some(evidence) = program.generic_evidence() else {
            if header.captures.iter().any(|capture| capture.mutable) { return Err(failure("live capture frame has no original evidence", span)); }
            return Ok(());
        };
        let declaration = evidence.checked_functions().find_map(|(_, source)| (source.target == target).then_some(source.declaration));
        let receives_mutable = evidence.lexical_captures().any(|(_, capture)| capture.target == target && capture.mutable && supports_live_capture_type(&capture.original_type));
        let defines_captured_mutable = declaration.is_some_and(|declaration| evidence.lexical_captures().any(|(_, capture)| capture.mutable && capture.definition_owner == Some(declaration) && supports_live_capture_type(&capture.original_type)));
        if !receives_mutable && !defines_captured_mutable { return Ok(()); }
        let cells = captures.map(|captures| captures.iter().filter_map(|capture| capture.live_cell.clone().map(|cell| (capture.slot, cell))).collect::<Vec<_>>());
        let mut registry = self.live_capture_cells.lock().expect("live capture registry is not poisoned");
        registry.begin_frame_with_cells(program, target, slots.as_ptr() as usize, header, cells.as_deref(), span)?;
        registry.refresh_slots(slots);
        Ok(())
    }

    pub(super) fn finish_indexed_live_frame(&self, slots: &mut [LoweredValue], span: Span) -> Result<Option<CompletedLiveCaptureFrame>, RuntimeError> {
        let mut registry = self.live_capture_cells.lock().expect("live capture registry is not poisoned");
        if !registry.has_frame(slots.as_ptr() as usize) { return Ok(None); }
        registry.finish_frame(slots, span).map(Some)
    }

    pub(super) fn retire_indexed_live_locals(&self, slots: &[LoweredValue], cleared: &[usize]) {
        self.live_capture_cells.lock().expect("live capture registry is not poisoned").retire_local_slots(slots.as_ptr() as usize, cleared);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::check::Checker;

    fn fixture() -> Arc<FullProgram> {
        let source = "var observed = 0\nvar unrelated = 99\nproc record() [] -> Unit { observed = 1 }\nproc completed() [] -> Int { record(); observed }\nproc unrelated_reader() [] -> Int { unrelated }\n";
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("live-capture.xsh", crate::loader::entry_source_from_text("live-capture.xsh", source.to_owned()), Vec::new());
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = sources.files().first().unwrap().id();
        let declarations = Checker::check_compact_declarations(&parsed.arena);
        let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
        assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
        let solved = Arc::downgrade(&bodies.solved);
        let program = Arc::new(crate::runtime::eval::indexed::full::FullBuilder::build_compact(&parsed.arena, &declarations, &bodies, source, Arc::new(sources), source_id).unwrap());
        drop(parsed); drop(declarations); drop(bodies);
        assert!(solved.upgrade().is_none());
        program
    }

    #[test]
    fn live_capture_cells_share_original_allocation_and_never_restore_outer_snapshots() {
        std::thread::Builder::new().stack_size(64 * 1024 * 1024).spawn(|| {
            let program = fixture();
            let _symbols = program.symbol_owner().enter();
            let evidence = program.generic_evidence().unwrap();
            let original = evidence.mutable_driver_receipts().find(|source| source.assignment.is_none()).unwrap();
            let step = original.step;
            let mut cells = LiveCaptureCells::default();
            for allocation in evidence.mutable_driver_receipts().filter(|source| source.assignment.is_none()) {
                cells.register_driver(&program, allocation.step, Some(LoweredValue::Int(if allocation.step == step { 0 } else { 99 })), Span::new(crate::source::SourceId::new(0), 0, 0)).unwrap();
            }
            let targets = evidence.checked_functions().filter(|(_, source)| evidence.lexical_captures().any(|(_, capture)| capture.target == source.target && capture.binding == original.binding)).take(2).map(|(_, source)| source.target).collect::<Vec<_>>();
            assert_eq!(targets.len(), 2);
            let mut outer = vec![LoweredValue::Int(0); 16];
            let mut inner = vec![LoweredValue::Int(0); 16];
            let outer_header = program.function_view_by_id(targets[0]).unwrap().header().unwrap();
            let inner_header = program.function_view_by_id(targets[1]).unwrap().header().unwrap();
            let outer_slot = evidence.lexical_captures().find(|(_, capture)| capture.target == targets[0] && capture.binding == original.binding).unwrap().1.slot as usize;
            let inner_slot = evidence.lexical_captures().find(|(_, capture)| capture.target == targets[1] && capture.binding == original.binding).unwrap().1.slot as usize;
            cells.begin_frame(&program, targets[0], outer.as_ptr() as usize, &outer_header, Span::new(crate::source::SourceId::new(0), 0, 0)).unwrap();
            cells.begin_frame(&program, targets[1], inner.as_ptr() as usize, &inner_header, Span::new(crate::source::SourceId::new(0), 0, 0)).unwrap();
            assert!(cells.write_slot(inner.as_ptr() as usize, inner_slot, LoweredValue::Int(1)));
            cells.finish_frame(&mut inner, Span::new(crate::source::SourceId::new(0), 0, 0)).unwrap();
            assert_eq!(inner[inner_slot], LoweredValue::Int(1));
            assert_eq!(cells.read_slot(outer.as_ptr() as usize, outer_slot), Some(LoweredValue::Int(1)));
            cells.finish_frame(&mut outer, Span::new(crate::source::SourceId::new(0), 0, 0)).unwrap();
            assert_eq!(outer[outer_slot], LoweredValue::Int(1));
        }).unwrap().join().unwrap();
    }

    #[test]
    fn live_capture_cells_refuse_missing_allocations_and_foreign_program_environments() {
        std::thread::Builder::new().stack_size(64 * 1024 * 1024).spawn(|| {
            let program = fixture();
            let _symbols = program.symbol_owner().enter();
            let evidence = program.generic_evidence().unwrap();
            let allocation = evidence.mutable_driver_receipts().find(|source| source.assignment.is_none()).unwrap();
            let step = allocation.step;
            let target = evidence.checked_functions().next().unwrap().1.target;
            let header = program.function_view_by_id(target).unwrap().header().unwrap();
            let capture_slot = evidence.lexical_captures().find(|(_, capture)| capture.target == target && capture.binding == allocation.binding).unwrap().1.slot as usize;
            let mut cells = LiveCaptureCells::default();
            let slots = vec![LoweredValue::Int(0); header.slot_count];
            assert!(cells.begin_frame(&program, target, slots.as_ptr() as usize, &header, Span::new(crate::source::SourceId::new(0), 0, 0)).unwrap_err().message.contains("original defining allocation"));
            cells.register_driver(&program, step, Some(LoweredValue::Int(0)), Span::new(crate::source::SourceId::new(0), 0, 0)).unwrap();
            let cell = cells.capture_cell(&program, target, capture_slot, Span::new(crate::source::SourceId::new(0), 0, 0)).unwrap();
            let unrelated = evidence.mutable_driver_receipts().find(|source| source.assignment.is_none() && source.step != step).unwrap().step;
            cells.register_driver(&program, unrelated, Some(LoweredValue::Int(99)), Span::new(crate::source::SourceId::new(0), 0, 0)).unwrap();
            assert!(cells.roots.last().unwrap().validate_capture(&program, target, capture_slot, Span::new(crate::source::SourceId::new(0), 0, 0)).is_err());
            let foreign = fixture();
            assert!(cell.validate_capture(&foreign, target, capture_slot, Span::new(crate::source::SourceId::new(0), 0, 0)).is_err());
            assert!(cells.register_driver(&program, step, Some(LoweredValue::Int(99)), Span::new(crate::source::SourceId::new(0), 0, 0)).is_err());
        }).unwrap().join().unwrap();
    }

    #[test]
    fn live_capture_frame_refuses_foreign_physical_views_with_matching_function_indices() {
        std::thread::Builder::new().stack_size(64 * 1024 * 1024).spawn(|| {
            let installed = fixture();
            let foreign = fixture();
            let _symbols = foreign.symbol_owner().enter();
            let target = foreign.generic_evidence().unwrap().checked_functions().next().unwrap().1.target;
            let view = foreign.function_view_by_id(target).unwrap();
            let header = view.header().unwrap();
            let mut slots = vec![LoweredValue::Int(0); header.slot_count];
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), crate::source::SourceMap::new());
            evaluator.indexed_program = Some(installed);
            let error = evaluator.begin_indexed_live_frame(view, &header, &mut slots, None, Span::new(crate::source::SourceId::new(0), 0, 0)).unwrap_err();
            assert!(error.message.contains("another program"), "{}", error.message);
        }).unwrap().join().unwrap();
    }
}
