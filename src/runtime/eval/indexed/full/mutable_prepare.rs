use super::*;
use super::super::generic::{MutableBindingReceipt, MutableCompoundAssignment, MutableDriverReceipt, MutableNominalInvariant, OperationSourceOrigin, graph_ground_type};
use super::callable_prepare::CallableLexicalIndex;
use super::super::generic::MutableReadRefinement;

fn mutable_problem(reason: &'static str) -> IrBuildError { IrBuildError::format(reason, None, 0, 0) }

impl FullBuilder {
    // Compiler sequencing wrappers own execution structure; only their material body
    // carries the authored initializer identity.
    fn mutable_value_lineage(&self, mut value: u32, owner: InstructionOwner) -> Result<(u32, Vec<(u32, FullTag, Box<[u32]>)>), IrBuildError> {
        let mut wrappers = Vec::new();
        loop {
            let compiler = self.compiler_argument_wrapper(value, owner)?;
            let tag = *self.store.tags.get(value as usize).ok_or_else(|| mutable_problem("mutable_wrapper_instruction"))?;
            if compiler.is_none() && !matches!(tag, FullTag::ExprCheckedValue | FullTag::ExprTry | FullTag::ExprRequire) { break; }
            if compiler.is_none() && tag == FullTag::ExprTry && self.generic_expression_rows.iter().any(|(instruction, _, _)| *instruction == value) { break; }
            if wrappers.len() >= 256 { return Err(mutable_problem("mutable_wrapper_depth")); }
            let words = self.store.payload(self.store.data[value as usize].range()).map_err(|_| mutable_problem("mutable_wrapper_payload"))?.to_vec().into_boxed_slice();
            let child = match compiler { Some(compiler) => compiler.body, None => *words.first().ok_or_else(|| mutable_problem("mutable_wrapper_child"))? };
            if child >= value { return Err(mutable_problem("mutable_wrapper_cycle")); }
            wrappers.push((value, tag, words));
            value = child;
        }
        Ok((value, wrappers))
    }

    pub(super) fn stage_mutable_driver_step(&mut self, row: BuildTopStmtId, step: u32, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        let declaration = scratch.mutable_driver_bindings.values().find(|original| original.row == row);
        let write = scratch.mutable_driver_writes.get(&row);
        let (binding, statement, value_source, value_root, ordinal, assignment) = match (declaration, write) {
            (Some(original), None) => (original.binding, original.statement, original.value_source, original.value_type, 0, None),
            (None, Some(write)) => (write.binding, write.statement, write.value_source, write.value_type, write.ordinal, Some(write.assignment)),
            (None, None) => return Ok(()),
            _ => return Err(mutable_problem("mutable_driver_original_ambiguous")),
        };
        let original = scratch.mutable_driver_bindings.get(&binding).ok_or_else(|| mutable_problem("mutable_driver_original_missing"))?;
        let solved = self.solved.clone().ok_or_else(|| mutable_problem("mutable_driver_solved_missing"))?;
        let definition = solved.bindings.get(&binding).ok_or_else(|| mutable_problem("mutable_driver_definition_missing"))?;
        if !definition.mutable || definition.owner.is_some() || definition.ty != original.source_type.ty || definition.scheme != original.source_type.scope
            || solved.expressions.get(&value_source) != Some(&value_root.ty) || solved.expression_owners.contains_key(&value_source)
            || solved.expression_scope(value_source, None).ok() != Some(value_root.scope) { return Err(mutable_problem("mutable_driver_original_checked_root_changed")); }
        solved.graph.validate_scoped(original.source_type).map_err(|_| mutable_problem("mutable_driver_binding_scope"))?;
        solved.graph.validate_scoped(value_root).map_err(|_| mutable_problem("mutable_driver_value_scope"))?;
        match &scratch.top_statements.get(row.index()).ok_or_else(|| mutable_problem("mutable_driver_original_row"))?.kind {
            BuildTopKind::Let { target, mutable: true, value, .. } if ordinal == 0 && *target == original.name && *value == original.value => {}
            BuildTopKind::Assign { target, op, value, .. } if ordinal != 0 && *target == original.name && Some(*op) == assignment && Some(*value) == write.map(|write| write.value) => {}
            _ => return Err(mutable_problem("mutable_driver_physical_row_changed")),
        }
        let binding_type = self.intern_generic_ground_type(&graph_ground_type(&solved.graph, original.source_type.ty).map_err(|_| mutable_problem("mutable_driver_ground_binding"))?)?;
        let value_type = self.intern_generic_ground_type(&graph_ground_type(&solved.graph, value_root.ty).map_err(|_| mutable_problem("mutable_driver_ground_value"))?)?;
        let compound = match write.and_then(|write| write.compound.as_ref()) {
            Some(operation) => Some(self.prepare_mutable_compound_contract(statement, operation, assignment.ok_or_else(|| mutable_problem("mutable_driver_compound_operator"))?, binding_type, value_type, binding_type)?),
            None => None,
        };
        let encoded = self.store.driver_steps.get(step as usize).ok_or_else(|| mutable_problem("mutable_driver_encoded_step"))?;
        let tag = encoded.tag;
        let payload = self.store.payload(encoded.data.range()).map_err(|_| mutable_problem("mutable_driver_payload"))?.to_vec().into_boxed_slice();
        let value = FullVerifier::mutable_driver_value(&self.store, step, original.name, ordinal == 0).map_err(|_| mutable_problem("mutable_driver_encoded_role"))?;
        let (value, value_wrappers) = self.mutable_value_lineage(value, InstructionOwner::Driver(step))?;
        self.generic_evidence_mut().register_instruction_origin(value, OperationSourceOrigin::Expression(value_source), InstructionOwner::Driver(step)).map_err(|_| mutable_problem("mutable_driver_initializer_source"))?;
        self.generic_evidence_mut().add_mutable_driver_receipt(MutableDriverReceipt { binding, statement, step, name: original.name, tag, payload, binding_type, binding_root: original.source_type, value, value_wrappers: value_wrappers.into_boxed_slice(), value_source, value_type, value_root, ordinal, assignment, compound }).map_err(|_| mutable_problem("mutable_driver_receipt"))
    }

    pub(super) fn stage_mutable_statement(&mut self, row: BuildStmtId, instruction: u32, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        if let Some(&guard) = scratch.mutable_refinement_guards.get(&row) {
            let owner = self.mutable_current_owner()?;
            self.generic_evidence_mut().register_instruction_origin(instruction, OperationSourceOrigin::Statement(guard), owner).map_err(|_| mutable_problem("mutable_refinement_guard_origin"))?;
        }
        let declaration = scratch.mutable_binding_origins.values().find(|original| original.row == row);
        let write = scratch.mutable_binding_writes.get(&row);
        let (binding, statement, value_source, value_root, ordinal) = match (declaration, write) {
            (Some(original), None) => (original.binding, original.statement, original.value_source, original.value_type, 0),
            (None, Some(write)) => (write.binding, write.statement, write.value_source, write.value_type, write.ordinal),
            (None, None) => return Ok(()),
            _ => return Err(mutable_problem("mutable_binding_allocation_ambiguous")),
        };
        let emitted = declaration.map(|original| &original.emitted).or_else(|| write.map(|write| &write.emitted)).ok_or_else(|| mutable_problem("mutable_binding_physical_origin"))?;
        if crate::runtime::eval::lower::mutable_binding::BuildMutableStatement::from_row(scratch.statements.get(row.index()).ok_or_else(|| mutable_problem("mutable_binding_physical_row"))?).as_ref() != Some(emitted) { return Err(mutable_problem("mutable_binding_physical_row_changed")); }
        let original = scratch.mutable_binding_origins.get(&binding);
        let driver = scratch.mutable_driver_bindings.get(&binding);
        let captured = write.and_then(|write| write.capture.as_ref());
        let source_type = captured.map(|capture| capture.binding_root).or_else(|| original.map(|original| original.source_type)).or_else(|| driver.map(|original| original.source_type)).ok_or_else(|| mutable_problem("mutable_binding_original_missing"))?;
        let solved = self.solved.clone().ok_or_else(|| mutable_problem("mutable_binding_solved_missing"))?;
        let definition = solved.bindings.get(&binding).ok_or_else(|| mutable_problem("mutable_binding_definition_missing"))?;
        let scope = definition.scheme.or_else(|| definition.owner.and_then(|owner| solved.declarations.get(&owner).map(|declaration| declaration.scheme)));
        let caller = captured.map(|capture| capture.caller).or(definition.owner);
        if captured.is_some_and(|capture| capture.definition_owner != definition.owner || solved.statement_owners.get(&statement).copied() != Some(capture.caller)) { return Err(mutable_problem("mutable_capture_original_caller")); }
        if !definition.mutable || definition.ty != source_type.ty || scope != source_type.scope
            || solved.expressions.get(&value_source) != Some(&value_root.ty)
            || solved.expression_owners.get(&value_source).copied() != caller
            || solved.expression_scope(value_source, caller).ok() != Some(value_root.scope) { return Err(mutable_problem("mutable_binding_original_changed")); }
        solved.graph.validate_scoped(source_type).map_err(|_| mutable_problem("mutable_binding_scope"))?;
        solved.graph.validate_scoped(value_root).map_err(|_| mutable_problem("mutable_write_scope"))?;
        let binding_type = self.intern_generic_ground_type(&graph_ground_type(&solved.graph, source_type.ty).map_err(|_| mutable_problem("mutable_binding_requires_scope"))?)?;
        let value_type = self.intern_generic_ground_type(&graph_ground_type(&solved.graph, value_root.ty).map_err(|_| mutable_problem("mutable_write_requires_scope"))?)?;
        let nominal = MutableNominalInvariant::from_checked(&solved, source_type).map_err(|_| mutable_problem("mutable_nominal_original_changed"))?;
        if nominal.is_some() && (captured.is_some() || solved.graph.resolved(value_root.ty).ok() != solved.graph.resolved(source_type.ty).ok()) { return Err(mutable_problem("mutable_nominal_assignment_changes_original_family")); }
        let assignment = match emitted {
            crate::runtime::eval::lower::mutable_binding::BuildMutableStatement::Value { assignment, .. } | crate::runtime::eval::lower::mutable_binding::BuildMutableStatement::Integer { assignment, .. } => *assignment,
            crate::runtime::eval::lower::mutable_binding::BuildMutableStatement::Boolean { assignment, .. } => assignment.then_some(AssignOp::Set),
        };
        let compound = match write.and_then(|write| write.compound.as_ref()) {
            Some(operation) => Some(self.prepare_mutable_compound_contract(statement, operation, assignment.ok_or_else(|| mutable_problem("mutable_compound_operator"))?, binding_type, value_type, binding_type)?),
            None => None,
        };
        let owner = self.mutable_current_owner()?;
        let tag = self.store.tags[instruction as usize];
        let payload = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| mutable_problem("mutable_binding_payload"))?.to_vec().into_boxed_slice();
        let offset = if ordinal == 0 { 1 } else { match tag { FullTag::StmtAssignBool => 1, FullTag::StmtAssign | FullTag::StmtAssignInt => 2, _ => return Err(mutable_problem("mutable_write_kind")) } };
        let slot = match emitted { crate::runtime::eval::lower::mutable_binding::BuildMutableStatement::Value { slot, .. } | crate::runtime::eval::lower::mutable_binding::BuildMutableStatement::Integer { slot, .. } | crate::runtime::eval::lower::mutable_binding::BuildMutableStatement::Boolean { slot, .. } => *slot };
        if payload.first().copied() != Some(slot as u32) || (captured.is_none() && original.is_some_and(|original| original.slot != slot)) { return Err(mutable_problem("mutable_binding_slot")); }
        let capture = if let Some(original) = captured {
            let InstructionOwner::Function(target) = owner else { return Err(mutable_problem("mutable_capture_receiving_owner")); };
            let (id, allocation) = self.generic_evidence_mut().lexical_capture_for_slot(target, slot as u32).map_err(|_| mutable_problem("mutable_capture_allocation"))?.ok_or_else(|| mutable_problem("mutable_capture_allocation_missing"))?;
            if !allocation.mutable || allocation.binding != binding || allocation.definition_owner != original.definition_owner || allocation.declaration != original.caller || original.slot != slot
                || allocation.source_type.ty != source_type.ty || allocation.source_type.scope != source_type.scope || allocation.ty != binding_type { return Err(mutable_problem("mutable_capture_allocation_changed")); }
            Some(id)
        } else { None };
        let value = *payload.get(offset).ok_or_else(|| mutable_problem("mutable_binding_value"))?;
        let (value, value_wrappers) = self.mutable_value_lineage(value, owner)?;
        self.generic_evidence_mut().register_instruction_origin(value, OperationSourceOrigin::Expression(value_source), owner).map_err(|_| mutable_problem("mutable_initializer_original_source"))?;
        self.generic_evidence_mut().register_instruction_origin(instruction, OperationSourceOrigin::Statement(statement), owner).map_err(|_| mutable_problem("mutable_binding_origin"))?;
        self.generic_evidence_mut().add_mutable_binding_receipt(MutableBindingReceipt {
            binding, nominal, capture, captured_path: None, statement: Some(statement), read_origin: None, refinement: None, instruction, owner, tag, payload, binding_type,
            binding_root: source_type, value: Some(value), value_wrappers: value_wrappers.into_boxed_slice(), value_source: Some(value_source), value_type: Some(value_type), value_root: Some(value_root), ordinal, assignment, compound,
        }).map_err(|_| mutable_problem("mutable_binding_receipt"))
    }

    pub(super) fn prepare_mutable_compound_contract(&mut self, statement: crate::sema::check::StatementIdentity, operation: &crate::sema::check::SolvedOperation, op: AssignOp, expected_left: TypeId, expected_right: TypeId, expected_result: TypeId) -> Result<MutableCompoundAssignment, IrBuildError> {
        let solved = self.solved.clone().ok_or_else(|| mutable_problem("mutable_compound_solved"))?;
        let checked = solved.statement_operations.get(&statement).ok_or_else(|| mutable_problem("mutable_compound_original_missing"))?;
        if checked.requirement != operation.requirement || checked.actual_arguments != operation.actual_arguments || checked.result != operation.result || checked.effects != operation.effects || checked.caller != operation.caller
            || operation.receiver.is_some() || operation.actual_arguments.len() != 2 || !operation.argument_coercions.is_empty() || operation.binding.supplied_slots != [0, 1]
            || !operation.binding.default_slots.is_empty() || operation.binding.rest_slot.is_some() || operation.binding.dynamic.is_some() { return Err(mutable_problem("mutable_compound_original_changed")); }
        let scope = solved.operation_scope(crate::sema::check::ProducerFlowSource::Statement(statement), operation).map_err(|_| mutable_problem("mutable_compound_scope"))?;
        solved.graph.validate_requirement_scoped(crate::sema::inference::ScopedRequirementRoot { requirement: operation.requirement, scope }).map_err(|_| mutable_problem("mutable_compound_requirement"))?;
        let crate::sema::inference::EffectSummary::Closed(effects) = solved.graph.closed_effect_summary(operation.effects).map_err(|_| mutable_problem("mutable_compound_effect_scope"))? else { return Err(mutable_problem("mutable_compound_effect_scope")); };
        if effects != crate::sema::inference::EffectSet::EMPTY { return Err(mutable_problem("mutable_compound_effect_contract")); }
        let selected = solved.graph.candidate_evidence(operation.requirement).map_err(|_| mutable_problem("mutable_compound_selection"))?.ok_or_else(|| mutable_problem("mutable_compound_selection"))?;
        let crate::sema::check::SolvedOperationAuthority::Language(metadata) = solved.operation_catalog.candidate(&solved.graph, selected.candidate).map_err(|_| mutable_problem("mutable_compound_authority"))? else { return Err(mutable_problem("mutable_compound_authority")); };
        if !matches!(metadata.operation, crate::sema::operation_graph::PreparedLanguageOperation::Compound { op: selected_op, .. } if selected_op == op) { return Err(mutable_problem("mutable_compound_contract")); }
        let mut intern = |ty| {
            solved.graph.validate_scoped(crate::sema::inference::ScopedRoot { ty, scope }).map_err(|_| mutable_problem("mutable_compound_operand_scope"))?;
            self.intern_generic_ground_type(&graph_ground_type(&solved.graph, ty).map_err(|_| mutable_problem("mutable_compound_operand_root"))?)
        };
        let left = intern(operation.actual_arguments[0])?;
        let right = intern(operation.actual_arguments[1])?;
        let result = intern(operation.result)?;
        if left != expected_left || right != expected_right || result != expected_result { return Err(mutable_problem("mutable_compound_operand_relationship")); }
        Ok(MutableCompoundAssignment { operation: metadata.operation, requirement: operation.requirement, scope, effects, left, right, result })
    }

    fn mutable_current_owner(&self) -> Result<InstructionOwner, IrBuildError> {
        let raw = self.current_owner.ok_or_else(|| mutable_problem("mutable_binding_owner"))?;
        if let Some(driver) = driver_owner_index(raw) { return Ok(InstructionOwner::Driver(driver as u32)); }
        IrFunctionId::from_raw(raw).map(InstructionOwner::Function).ok_or_else(|| mutable_problem("mutable_binding_owner"))
    }

    pub(super) fn prepare_mutable_refinements(&mut self) -> Result<(), IrBuildError> {
        let preparations = self.generic_evidence_mut().mutable_refinement_preparations();
        for receipt in preparations {
            let mut refinement = receipt.refinement.ok_or_else(|| mutable_problem("mutable_refinement_source_missing"))?;
            let checked = Arc::clone(&refinement.source);
            let mut resolve = |origin| self.generic_evidence_mut().mutable_original_instruction(origin, receipt.owner).map_err(|_| mutable_problem("mutable_refinement_physical_source_missing"));
            refinement.predicate = resolve(OperationSourceOrigin::Expression(checked.predicate))?;
            refinement.subject = resolve(OperationSourceOrigin::Expression(checked.subject))?;
            refinement.guard = resolve(OperationSourceOrigin::Statement(checked.guard))?;
            refinement.condition = resolve(OperationSourceOrigin::Expression(checked.guard_condition))?;
            let mut aliases = Vec::new();
            for alias in &checked.aliases { aliases.push((resolve(OperationSourceOrigin::Statement(alias.statement))?, resolve(OperationSourceOrigin::Expression(alias.initializer))?)); }
            refinement.aliases = aliases.into_boxed_slice();
            refinement.writes = checked.writes.iter().map(|write| resolve(OperationSourceOrigin::Statement(write.statement))).collect::<Result<Vec<_>, _>>()?.into_boxed_slice();
            let mut instructions = vec![refinement.predicate, refinement.subject, refinement.guard, refinement.condition];
            instructions.extend(refinement.aliases.iter().flat_map(|&(allocation, initializer)| [allocation, initializer]));
            instructions.sort_unstable(); instructions.dedup();
            refinement.rows = instructions.iter().map(|&instruction| {
                let tag = *self.store.tags.get(instruction as usize).ok_or_else(|| mutable_problem("mutable_refinement_row"))?;
                let payload = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| mutable_problem("mutable_refinement_payload"))?.to_vec().into_boxed_slice();
                Ok((instruction, tag, payload))
            }).collect::<Result<Vec<_>, IrBuildError>>()?.into_boxed_slice();
            let payload = self.store.payload(self.store.data[refinement.guard as usize].range()).map_err(|_| mutable_problem("mutable_refinement_guard_payload"))?;
            let [branches, 1, failure] = payload else { return Err(mutable_problem("mutable_refinement_guard_shape")); };
            let branch = IrBlockId::from_raw(*branches).and_then(|id| self.store.blocks.get(id.index())).ok_or_else(|| mutable_problem("mutable_refinement_branches"))?;
            let branch_payload = self.store.payload(branch.instructions).map_err(|_| mutable_problem("mutable_refinement_branch_payload"))?;
            let [1, condition, success] = branch_payload else { return Err(mutable_problem("mutable_refinement_branch_shape")); };
            if *condition != refinement.condition { return Err(mutable_problem("mutable_refinement_condition_source")); }
            refinement.blocks = [*branches, *success, *failure].iter().map(|&raw| {
                let block = IrBlockId::from_raw(raw).and_then(|id| self.store.blocks.get(id.index())).ok_or_else(|| mutable_problem("mutable_refinement_block"))?;
                let words = self.store.payload(block.instructions).map_err(|_| mutable_problem("mutable_refinement_block_payload"))?.to_vec().into_boxed_slice();
                Ok((raw, block.owner, block.flags, words))
            }).collect::<Result<Vec<_>, IrBuildError>>()?.into_boxed_slice();
            self.generic_evidence_mut().complete_mutable_refinement(receipt.instruction, refinement).map_err(|_| mutable_problem("mutable_refinement_publication"))?;
        }
        Ok(())
    }

    pub(super) fn stage_mutable_use(&mut self, instruction: u32, origin: crate::sema::check::ExpressionIdentity, owner: InstructionOwner, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        if scratch.lexical_capture_reads.contains_key(&origin) { return Ok(()); }
        let Some(&binding) = scratch.mutable_binding_uses.get(&origin) else { return Ok(()); };
        self.stage_mutable_read(instruction, OperationSourceOrigin::Expression(origin), binding, owner, scratch)
    }

    pub(super) fn stage_mutable_read(&mut self, instruction: u32, origin: OperationSourceOrigin, binding: crate::sema::check::BindingIdentity, owner: InstructionOwner, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        let tag = self.store.tags[instruction as usize];
        if !matches!(tag, FullTag::ExprParam | FullTag::IntSlot | FullTag::BoolSlot) { return Ok(()); }
        let solved = self.solved.clone().ok_or_else(|| mutable_problem("mutable_read_solved_missing"))?;
        let payload = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| mutable_problem("mutable_read_payload"))?.to_vec().into_boxed_slice();
        let capture = if let InstructionOwner::Function(target) = owner {
            self.generic_evidence_mut().lexical_capture_for_slot(target, *payload.first().ok_or_else(|| mutable_problem("mutable_read_slot"))?).map_err(|_| mutable_problem("mutable_read_capture"))?.map(|(id, allocation)| (id, allocation.clone()))
        } else { None };
        if let Some((_, allocation)) = &capture {
            let caller = match origin { OperationSourceOrigin::Statement(statement) => solved.statement_owners.get(&statement).copied(), OperationSourceOrigin::Expression(expression) => solved.expression_owners.get(&expression).copied(), _ => return Err(mutable_problem("mutable_read_capture_source_kind")) };
            if !allocation.mutable || allocation.binding != binding || caller != Some(allocation.declaration) { return Err(mutable_problem("mutable_read_capture_source")); }
            if let OperationSourceOrigin::Statement(statement) = origin {
                let flow = *solved.statement_producer_flows.get(&statement).ok_or_else(|| mutable_problem("mutable_read_capture_original_flow"))?;
                let node = solved.producer_flows.node(flow).map_err(|_| mutable_problem("mutable_read_capture_original_flow"))?;
                let crate::sema::check::ProducerFlowKind::Join { inputs } = &node.kind else { return Err(mutable_problem("mutable_read_capture_original_flow_kind")); };
                let [input] = inputs.as_slice() else { return Err(mutable_problem("mutable_read_capture_original_flow_arity")); };
                let original = solved.producer_flows.node(*input).map_err(|_| mutable_problem("mutable_read_capture_original_binding"))?;
                let crate::sema::check::ProducerFlowSource::Binding { identity, version } = original.source else { return Err(mutable_problem("mutable_read_capture_original_binding")); };
                if node.source != crate::sema::check::ProducerFlowSource::Statement(statement) || identity != binding || solved.binding_producer_flows.get(&(identity, version)) != Some(input) { return Err(mutable_problem("mutable_read_capture_original_binding_changed")); }
            }
        }
        let source_type = capture.as_ref().map(|(_, allocation)| allocation.source_type).or_else(|| scratch.mutable_binding_origins.get(&binding).map(|original| original.source_type)).or_else(|| scratch.mutable_driver_bindings.get(&binding).map(|original| original.source_type)).ok_or_else(|| mutable_problem("mutable_read_original_missing"))?;
        let binding_type = self.intern_generic_ground_type(&graph_ground_type(&solved.graph, source_type.ty).map_err(|_| mutable_problem("mutable_read_requires_scope"))?)?;
        let nominal = MutableNominalInvariant::from_checked(&solved, source_type).map_err(|_| mutable_problem("mutable_nominal_read_original_changed"))?;
        let refinement = if let OperationSourceOrigin::Expression(read) = origin {
            if solved.refined_reads.contains_key(&read) {
                let checked = solved.checked_refined_read(read).map_err(|_| mutable_problem("mutable_refinement_original_changed"))?;
                if checked.binding != binding || checked.caller != solved.bindings.get(&binding).and_then(|definition| definition.owner) || checked.invariant.ty != source_type.ty || checked.invariant.scope != source_type.scope || !checked.read_path.is_empty() { return Err(mutable_problem("mutable_refinement_storage_relationship")); }
                solved.graph.validate_scoped(checked.narrowed).map_err(|_| mutable_problem("mutable_refinement_narrowed_scope"))?;
                let narrowed_type = self.intern_generic_ground_type(&graph_ground_type(&solved.graph, checked.narrowed.ty).map_err(|_| mutable_problem("mutable_refinement_requires_scope"))?)?;
                Some(MutableReadRefinement { source: Arc::new(checked.clone()), narrowed_type, predicate: u32::MAX, subject: u32::MAX, guard: u32::MAX, condition: u32::MAX, aliases: Box::new([]), writes: Box::new([]), rows: Box::new([]), blocks: Box::new([]) })
            } else { None }
        } else { None };
        let payload = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| mutable_problem("mutable_read_payload"))?.to_vec().into_boxed_slice();
        self.generic_evidence_mut().register_instruction_origin(instruction, origin, owner).map_err(|_| mutable_problem("mutable_read_origin"))?;
        self.generic_evidence_mut().add_mutable_binding_receipt(MutableBindingReceipt { binding, nominal, capture: capture.map(|(id, _)| id), captured_path: None, statement: None, read_origin: Some(origin), refinement, instruction, owner, tag, payload, binding_type, binding_root: source_type, value: None, value_wrappers: Box::new([]), value_source: None, value_type: None, value_root: None, ordinal: 0, assignment: None, compound: None }).map_err(|_| mutable_problem("mutable_read_receipt"))
    }
}

impl FullVerifier {
    fn mutable_driver_program(store: &FullStore, step: u32) -> Result<usize, IrVerifyError> {
        let mut selected = None;
        for (index, program) in store.driver_programs.iter().enumerate() {
            if program.steps.bounds(store.driver_steps.len()).is_some_and(|range| range.contains(&(step as usize))) {
                if selected.replace(index).is_some() { return Err(IrVerifyError::new("mutable driver ordinal has ambiguous program ownership")); }
            }
        }
        selected.ok_or_else(|| IrVerifyError::new("mutable driver ordinal lacks its original program"))
    }

    // Driver slots are rebuilt for each statement. A dense address is authorized
    // by its original binding, named mapping, and containing program order.
    pub(super) fn verify_mutable_driver_dominance(store: &FullStore, definition: &MutableDriverReceipt, owner: InstructionOwner, slot: u32) -> Result<(), IrVerifyError> {
        let InstructionOwner::Driver(step) = owner else { return Err(IrVerifyError::new("mutable driver binding cannot authorize a foreign function")); };
        if step <= definition.step || Self::mutable_driver_program(store, step)? != Self::mutable_driver_program(store, definition.step)? { return Err(IrVerifyError::new("mutable driver read or write is outside its original program order")); }
        let current = store.driver_steps.get(step as usize).ok_or_else(|| IrVerifyError::new("mutable driver current step is invalid"))?;
        let range = current.slots.bounds(store.driver_slots.len()).ok_or_else(|| IrVerifyError::new("mutable driver slot range is invalid"))?;
        let current = store.driver_slots[range].iter().find(|current| current.slot == slot).ok_or_else(|| IrVerifyError::new("mutable driver read lacks its original slot allocation"))?;
        if store.string(current.name)? != definition.name.as_str().as_str() || current.type_id != definition.binding_type || current.flags & DRIVER_SLOT_MUTABLE == 0 { return Err(IrVerifyError::new("mutable driver changes its original named slot mapping")); }
        for previous in definition.step as usize + 1..step as usize {
            if store.driver_steps[previous].tag == FullDriverTag::Let {
                let payload = store.payload(store.driver_steps[previous].data.range())?;
                if payload.first().copied() == Some(definition.name.symbol().raw()) { return Err(IrVerifyError::new("mutable driver read crosses a later binding of its original name")); }
            }
        }
        Ok(())
    }

    fn verify_mutable_value_lineage(store: &FullStore, generic: &GenericEvidenceStore, wrappers: &[(u32, FullTag, Box<[u32]>)], value: u32, owner: InstructionOwner) -> Result<(), IrVerifyError> {
        for (index, (wrapper, tag, payload)) in wrappers.iter().enumerate() {
            if store.tags.get(*wrapper as usize) != Some(tag) || store.payload(store.data[*wrapper as usize].range())? != payload.as_ref() {
                return Err(IrVerifyError::new("mutable initializer changes its original execution wrapper"));
            }
            let next = wrappers.get(index + 1).map_or(value, |(instruction, _, _)| *instruction);
            let child = if *tag == FullTag::ExprMatch {
                Self::original_compiler_argument_wrapper_body(store, generic, *wrapper, owner)?.ok_or_else(|| IrVerifyError::new("mutable initializer loses its original compiler wrapper"))?
            } else if matches!(tag, FullTag::ExprCheckedValue | FullTag::ExprTry | FullTag::ExprRequire) {
                *payload.first().ok_or_else(|| IrVerifyError::new("mutable initializer wrapper lacks its child"))?
            } else { return Err(IrVerifyError::new("mutable initializer changes its original wrapper role")); };
            if child != next || child >= *wrapper { return Err(IrVerifyError::new("mutable initializer changes its original wrapper lineage")); }
        }
        Ok(())
    }

    fn verify_mutable_driver_receipts<'a>(store: &FullStore, generic: &'a GenericEvidenceStore) -> Result<BTreeMap<crate::sema::check::BindingIdentity, &'a MutableDriverReceipt>, IrVerifyError> {
        let mut definitions = BTreeMap::new();
        for receipt in generic.mutable_driver_receipts().filter(|receipt| receipt.ordinal == 0) {
            if definitions.insert(receipt.binding, receipt).is_some() { return Err(IrVerifyError::new("mutable driver duplicates its original allocation")); }
        }
        for receipt in generic.mutable_driver_receipts() {
            generic.mutable_driver_receipt(receipt.step)?.ok_or_else(|| IrVerifyError::new("mutable driver loses its original receipt"))?;
            Self::mutable_driver_program(store, receipt.step)?;
            let step = store.driver_steps.get(receipt.step as usize).ok_or_else(|| IrVerifyError::new("mutable driver step is missing"))?;
            if step.tag != receipt.tag || store.payload(step.data.range())? != receipt.payload.as_ref() { return Err(IrVerifyError::new("mutable driver changes its original encoded allocation or write")); }
            let encoded_value = Self::mutable_driver_value(store, receipt.step, receipt.name, receipt.ordinal == 0)?;
            if encoded_value != receipt.value_wrappers.first().map_or(receipt.value, |(instruction, _, _)| *instruction) { return Err(IrVerifyError::new("mutable driver changes its original checked value")); }
            Self::verify_mutable_value_lineage(store, generic, &receipt.value_wrappers, receipt.value, InstructionOwner::Driver(receipt.step))?;
            if let Some(op) = receipt.assignment {
                if receipt.payload.get(1).and_then(|raw| store.assign_ops.get(*raw as usize)).copied() != Some(op) { return Err(IrVerifyError::new("mutable driver changes its original selected assignment operator")); }
                let definition = definitions.get(&receipt.binding).ok_or_else(|| IrVerifyError::new("mutable driver write lacks its original allocation"))?;
                if definition.binding_type != receipt.binding_type || definition.binding_root.ty != receipt.binding_root.ty || definition.binding_root.scope != receipt.binding_root.scope || definition.name != receipt.name { return Err(IrVerifyError::new("mutable driver write changes its invariant original binding")); }
                let range = step.slots.bounds(store.driver_slots.len()).ok_or_else(|| IrVerifyError::new("mutable driver write slot range is invalid"))?;
                let slot = store.driver_slots[range].iter().find(|slot| store.string(slot.name).ok() == Some(definition.name.as_str().as_str())).ok_or_else(|| IrVerifyError::new("mutable driver write lacks its original lexical slot"))?;
                Self::verify_mutable_driver_dominance(store, definition, InstructionOwner::Driver(receipt.step), slot.slot)?;
            }
            Self::verify_generic_source(store, generic, receipt.value, InstructionOwner::Driver(receipt.step), &store.semantic.to_type(receipt.value_type)?, None, &mut Vec::new())?;
        }
        Ok(definitions)
    }

    fn mutable_driver_value(store: &FullStore, step: u32, name: Name, declaration: bool) -> Result<u32, IrVerifyError> {
        let original = store.driver_steps.get(step as usize).ok_or_else(|| IrVerifyError::new("mutable driver step is missing"))?;
        if original.tag != if declaration { FullDriverTag::Let } else { FullDriverTag::Assign } { return Err(IrVerifyError::new("mutable driver changes its original allocation or write role")); }
        let decoder = FullDecoder { store, owner: driver_owner(step as usize).map_err(|_| IrVerifyError::new("mutable driver owner is invalid"))?, instruction_range: store.driver_instruction_range(step as usize)?, instruction_states: None, block_states: None, slot_count: original.slot_count, pattern_tree: None, pattern_ceiling: Cell::new(usize::MAX), verified: true };
        let mut payload = FullCursor::new(store.payload(original.data.range())?);
        if Name::decode(&decoder, &mut payload)? != name { return Err(IrVerifyError::new("mutable driver changes its original binding name")); }
        if declaration {
            Option::<LoweredType>::decode(&decoder, &mut payload)?; Option::<LoweredTypeCheck>::decode(&decoder, &mut payload)?;
            if !bool::decode(&decoder, &mut payload)? { return Err(IrVerifyError::new("mutable driver loses its original mutable allocation")); }
        } else { AssignOp::decode(&decoder, &mut payload)?; }
        payload.raw()
    }
    pub(super) fn verify_mutable_assignment_operator(store: &FullStore, instruction: u32, expected: AssignOp, offset: usize) -> Result<(), IrVerifyError> {
        let row = store.data.get(instruction as usize).ok_or_else(|| IrVerifyError::new("mutable assignment instruction is missing"))?;
        let payload = store.payload(row.range())?;
        if payload.get(offset).and_then(|index| store.assign_ops.get(*index as usize)).copied() != Some(expected) { return Err(IrVerifyError::new("mutable assignment changes its original selected operator")); }
        Ok(())
    }

    pub(super) fn verify_mutable_binding_dominance(store: &FullStore, tree: &super::super::pattern::PatternTree) -> Result<(), IrVerifyError> {
        let Some(generic) = store.generic.as_deref() else { return Ok(()); };
        let index = CallableLexicalIndex::new(store, tree)?;
        let owners = store.generic_instruction_owners()?;
        let driver_definitions = Self::verify_mutable_driver_receipts(store, generic)?;
        let mut definitions = BTreeMap::new();
        let mut mutable_slots = std::collections::BTreeSet::new();
        let owner_key = |owner| match owner { InstructionOwner::Function(function) => (false, function.raw()), InstructionOwner::Driver(driver) => (true, driver) };
        for receipt in generic.mutable_binding_receipts().filter(|receipt| receipt.read_origin.is_none() && receipt.ordinal == 0 && receipt.captured_path.is_none()) {
            if !matches!(receipt.tag, FullTag::StmtLet | FullTag::StmtLetInt | FullTag::StmtLetBool) || definitions.insert(receipt.binding, receipt).is_some() { return Err(IrVerifyError::new("mutable binding changes its original declaration")); }
        }
        for definition in definitions.values() {
            let slot = *definition.payload.first().ok_or_else(|| IrVerifyError::new("mutable binding slot is missing"))?;
            if !mutable_slots.insert((owner_key(definition.owner), slot)) { return Err(IrVerifyError::new("mutable binding has multiple original storage definitions")); }
        }
        for (_, allocation) in generic.lexical_captures().filter(|(_, allocation)| allocation.mutable) {
            mutable_slots.insert((owner_key(InstructionOwner::Function(allocation.target)), allocation.slot));
        }
        for receipt in generic.mutable_binding_receipts() {
            generic.mutable_binding_receipt(receipt.instruction)?;
            if store.tags.get(receipt.instruction as usize) != Some(&receipt.tag) || store.payload(store.data[receipt.instruction as usize].range())? != receipt.payload.as_ref() { return Err(IrVerifyError::new("mutable binding changes its original encoded contract")); }
            if let Some(op) = receipt.assignment {
                if receipt.tag == FullTag::StmtAssignBool {
                    if op != AssignOp::Set { return Err(IrVerifyError::new("mutable Boolean assignment changes its original operator")); }
                } else { Self::verify_mutable_assignment_operator(store, receipt.instruction, op, if receipt.captured_path.is_some() { 2 } else { 1 })?; }
            }
            if let Some(refinement) = &receipt.refinement {
                Self::verify_mutable_read_refinement(store, generic, tree, &index, receipt, refinement)?;
            }
            if let Some(capture) = receipt.capture {
                let allocation = generic.lexical_capture(capture)?;
                if !allocation.mutable || receipt.owner != InstructionOwner::Function(allocation.target) || receipt.binding != allocation.binding || receipt.binding_type != allocation.ty
                    || receipt.binding_root.ty != allocation.source_type.ty || receipt.binding_root.scope != allocation.source_type.scope || receipt.payload.first() != Some(&allocation.slot)
                    || receipt.refinement.is_some() || (receipt.read_origin.is_none() && receipt.ordinal == 0 && receipt.captured_path.is_none()) { return Err(IrVerifyError::new("mutable capture changes its original allocation or invariant type")); }
            } else if let Some(definition) = definitions.get(&receipt.binding) {
                if definition.owner != receipt.owner || definition.binding_type != receipt.binding_type || (definition.binding_root.ty != receipt.binding_root.ty || definition.binding_root.scope != receipt.binding_root.scope) || definition.payload.first() != receipt.payload.first() { return Err(IrVerifyError::new("mutable binding changes its invariant storage type or slot")); }
                if definition.instruction != receipt.instruction && !index.dominates(tree, definition.instruction, receipt.instruction)? { return Err(IrVerifyError::new("mutable read or write is outside its original binding scope")); }
            } else {
                let definition = driver_definitions.get(&receipt.binding).ok_or_else(|| IrVerifyError::new("mutable read or write lacks its original definition"))?;
                if definition.binding_type != receipt.binding_type || definition.binding_root.ty != receipt.binding_root.ty || definition.binding_root.scope != receipt.binding_root.scope { return Err(IrVerifyError::new("mutable driver read or write changes its invariant original type")); }
                Self::verify_mutable_driver_dominance(store, definition, receipt.owner, *receipt.payload.first().ok_or_else(|| IrVerifyError::new("mutable driver slot is missing"))?)?;
            }
            if let Some(value) = receipt.value {
                Self::verify_mutable_value_lineage(store, generic, &receipt.value_wrappers, value, receipt.owner)?;
            }
            if let (Some(value), Some(ty)) = (receipt.value, receipt.value_type) {
                if generic.registered_instruction_origin(value, false) != receipt.value_source.map(|source| (OperationSourceOrigin::Expression(source), receipt.owner)) { return Err(IrVerifyError::new("mutable value changes its original checked expression")); }
                FullVerifier::verify_generic_source(store, generic, value, receipt.owner, &store.semantic.to_type(ty)?, None, &mut Vec::new())?;
            }
        }
        for path in generic.mutable_paths() {
            if let Some(receipt) = generic.mutable_binding_receipt(path.instruction)?.filter(|receipt| receipt.captured_path == Some(path.instruction)) {
                if receipt.capture.is_none() || receipt.binding != path.binding || receipt.owner != path.owner || receipt.binding_type != path.binding_type || receipt.payload.first() != Some(&path.slot) { return Err(IrVerifyError::new("captured path changes its original receiving cell")); }
            } else if let Some(definition) = definitions.get(&path.binding) {
                if path.owner != definition.owner || path.binding_type != definition.binding_type || path.binding_root.ty != definition.binding_root.ty || path.binding_root.scope != definition.binding_root.scope || definition.payload.first() != Some(&path.slot)
                    || !index.dominates(tree, definition.instruction, path.instruction)? { return Err(IrVerifyError::new("mutable path is outside its original binding scope")); }
            } else {
                let definition = driver_definitions.get(&path.binding).ok_or_else(|| IrVerifyError::new("mutable path lacks its original binding definition"))?;
                if path.binding_type != definition.binding_type || path.binding_root.ty != definition.binding_root.ty || path.binding_root.scope != definition.binding_root.scope { return Err(IrVerifyError::new("mutable driver path changes its invariant original type")); }
                Self::verify_mutable_driver_dominance(store, definition, path.owner, path.slot)?;
            }
            Self::verify_mutable_path_contract(store, generic, path)?;
        }
        for (instruction, tag) in store.tags.iter().enumerate() {
            if !matches!(tag, FullTag::StmtAssign | FullTag::StmtAssignInt | FullTag::StmtAssignBool | FullTag::StmtAssignField | FullTag::StmtAssignFieldInt | FullTag::StmtAssignPath) { continue; }
            let slot = store.payload(store.data[instruction].range())?.first().copied();
            if let Some((InstructionOwner::Function(target), slot)) = owners[instruction].zip(slot) {
                if generic.lexical_capture_for_slot(target, slot)?.is_some_and(|(_, capture)| capture.mutable) && generic.mutable_binding_receipt(instruction as u32)?.is_none() { return Err(IrVerifyError::new("captured mutable write lacks its original receiving cell authority")); }
            }
            if owners[instruction].zip(slot).is_some_and(|(owner, slot)| mutable_slots.contains(&(owner_key(owner), slot))) && generic.mutable_binding_receipt(instruction as u32)?.is_none() && generic.mutable_path_at(instruction as u32)?.is_none() { return Err(IrVerifyError::new("mutable binding has a write without its original checked assignment")); }
            if let Some((InstructionOwner::Driver(step), slot)) = owners[instruction].zip(slot) {
                let current = &store.driver_steps[step as usize];
                let slots = current.slots.bounds(store.driver_slots.len()).ok_or_else(|| IrVerifyError::new("mutable driver slot range is invalid"))?;
                if let Some(mapping) = store.driver_slots[slots].iter().find(|mapping| mapping.slot == slot) {
                    if driver_definitions.values().any(|definition| definition.step < step && Self::mutable_driver_program(store, definition.step).ok() == Self::mutable_driver_program(store, step).ok() && store.string(mapping.name).ok() == Some(definition.name.as_str().as_str())) {
                        if mapping.flags & DRIVER_SLOT_WRITE == 0 { return Err(IrVerifyError::new("mutable driver write loses its original scope synchronization")); }
                        if generic.mutable_binding_receipt(instruction as u32)?.is_none() && generic.mutable_path_at(instruction as u32)?.is_none() { return Err(IrVerifyError::new("mutable driver binding has a write without its original assignment proof")); }
                    }
                }
            }
        }
        for (step, current) in store.driver_steps.iter().enumerate().filter(|(_, current)| current.tag == FullDriverTag::Assign) {
            let payload = store.payload(current.data.range())?;
            if driver_definitions.values().any(|definition| definition.step < step as u32 && Self::mutable_driver_program(store, definition.step).ok() == Self::mutable_driver_program(store, step as u32).ok() && payload.first().copied() == Some(definition.name.symbol().raw())) && generic.mutable_driver_receipt(step as u32)?.is_none() { return Err(IrVerifyError::new("mutable driver direct write lacks its original assignment proof")); }
        }
        Ok(())
    }

    // Allocation addresses do not describe execution order. A nested write may
    // execute before the read when its enclosing statement precedes that read.
    fn mutable_write_may_precede_read(tree: &super::super::pattern::PatternTree, index: &CallableLexicalIndex, mut write: u32, read: u32) -> Result<bool, IrVerifyError> {
        for _ in 0..512 {
            if index.dominates(tree, write, read)? { return Ok(true); }
            let Some(parent) = tree.parent(write)? else { return Ok(false); };
            write = parent;
        }
        Err(IrVerifyError::new("mutable refinement mutation ancestry exceeds its bound"))
    }

    fn verify_mutable_read_refinement(store: &FullStore, generic: &GenericEvidenceStore, tree: &super::super::pattern::PatternTree, index: &CallableLexicalIndex, receipt: &MutableBindingReceipt, refinement: &MutableReadRefinement) -> Result<(), IrVerifyError> {
        let source = &refinement.source;
        if !source.predicate_nonnull_when_true || source.path.is_empty() || !source.read_path.is_empty() {
            return Err(IrVerifyError::new("mutable refinement requires its original positive root guard"));
        }
        for (instruction, tag, payload) in refinement.rows.iter() {
            if store.tags.get(*instruction as usize) != Some(tag) || store.payload(store.data[*instruction as usize].range())? != payload.as_ref() {
                return Err(IrVerifyError::new("mutable refinement changes its original predicate, alias, or guard"));
            }
        }
        for (raw, owner, flags, payload) in refinement.blocks.iter() {
            let block = IrBlockId::from_raw(*raw).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("mutable refinement guard block is missing"))?;
            if block.owner != *owner || block.flags != *flags || store.payload(block.instructions)? != payload.as_ref() {
                return Err(IrVerifyError::new("mutable refinement changes its original guard branches"));
            }
        }
        for (instruction, origin) in [(refinement.predicate, OperationSourceOrigin::Expression(source.predicate)), (refinement.subject, OperationSourceOrigin::Expression(source.subject)), (refinement.guard, OperationSourceOrigin::Statement(source.guard)), (refinement.condition, OperationSourceOrigin::Expression(source.guard_condition))] {
            if generic.registered_instruction_origin(instruction, false) != Some((origin, receipt.owner)) { return Err(IrVerifyError::new("mutable refinement changes its original source identity")); }
        }
        if !tree.is_descendant(refinement.predicate, refinement.subject)? || !tree.is_descendant(refinement.guard, refinement.condition)?
            || !index.dominates(tree, refinement.guard, receipt.instruction)? { return Err(IrVerifyError::new("mutable refinement guard does not dominate its original read")); }
        let mut predicate_allocation = refinement.guard;
        let mut prior_binding = None;
        for (ordinal, (alias, &(allocation, initializer))) in source.aliases.iter().zip(refinement.aliases.iter()).enumerate() {
            let (id, original) = generic.value_binding_sources().find(|(_, original)| original.binding == super::super::generic::ValueBindingIdentity::Named(alias.binding)).ok_or_else(|| IrVerifyError::new("mutable refinement alias lacks its original binding"))?;
            generic.value_binding_source(id)?;
            let contract = &original.expected;
            if original.statement != alias.statement || original.initializer_source != alias.initializer || contract.owner != receipt.owner
                || contract.instruction != allocation || contract.initializer_source_instruction != initializer || !index.dominates(tree, allocation, refinement.guard)? {
                return Err(IrVerifyError::new("mutable refinement alias changes its original allocation or scope"));
            }
            if let Some(prior) = prior_binding {
                let use_ = generic.value_binding_use(initializer)?.ok_or_else(|| IrVerifyError::new("mutable refinement alias loses its original preceding read"))?;
                let application = generic.value_binding(use_.application)?;
                if generic.value_binding_source(application.source)?.binding != super::super::generic::ValueBindingIdentity::Named(prior)
                    || !index.dominates(tree, refinement.aliases[ordinal - 1].0, initializer)? { return Err(IrVerifyError::new("mutable refinement alias changes its original predicate lineage")); }
            } else {
                if initializer != refinement.predicate { return Err(IrVerifyError::new("mutable refinement alias changes its original tested predicate")); }
                predicate_allocation = allocation;
            }
            prior_binding = Some(alias.binding);
        }
        if let Some(binding) = prior_binding {
            let use_ = generic.value_binding_use(refinement.condition)?.ok_or_else(|| IrVerifyError::new("mutable refinement guard loses its original alias read"))?;
            if generic.value_binding_source(generic.value_binding(use_.application)?.source)?.binding != super::super::generic::ValueBindingIdentity::Named(binding) { return Err(IrVerifyError::new("mutable refinement guard changes its original predicate binding")); }
        } else if refinement.condition != refinement.predicate { return Err(IrVerifyError::new("mutable refinement guard changes its original predicate")); }
        let guard_payload = store.payload(store.data[refinement.guard as usize].range())?;
        if !matches!(store.tags[refinement.guard as usize], FullTag::StmtIf | FullTag::StmtIfBool) { return Err(IrVerifyError::new("mutable refinement loses its original exiting guard")); }
        let [branches, 1, failure] = guard_payload else { return Err(IrVerifyError::new("mutable refinement guard changes its original failure branch")); };
        let branches = IrBlockId::from_raw(*branches).ok_or_else(|| IrVerifyError::new("mutable refinement branches are invalid"))?;
        let branch_payload = store.payload(store.blocks[branches.index()].instructions)?;
        let [1, condition, success] = branch_payload else { return Err(IrVerifyError::new("mutable refinement guard changes its original condition branches")); };
        let success = IrBlockId::from_raw(*success).ok_or_else(|| IrVerifyError::new("mutable refinement success block is invalid"))?;
        let failure = IrBlockId::from_raw(*failure).ok_or_else(|| IrVerifyError::new("mutable refinement failure block is invalid"))?;
        if *condition != refinement.condition || store.payload(store.blocks[success.index()].instructions)? != [0]
            || !indexed_block_can_return(store, failure)? { return Err(IrVerifyError::new("mutable refinement guard no longer exits before a nullable read")); }
        let mut accepted = std::collections::BTreeSet::new();
        for (write, &instruction) in source.writes.iter().zip(refinement.writes.iter()) {
            let path = generic.mutable_path_at(instruction)?.ok_or_else(|| IrVerifyError::new("mutable refinement disjoint write loses its original path"))?;
            let actual: Option<Vec<Name>> = path.steps.iter().map(|step| match step { super::super::generic::MutablePathStep::Field { name, .. } => Some(*name), _ => None }).collect();
            if path.binding != receipt.binding || path.owner != receipt.owner || path.statement != write.statement || actual.as_deref() != Some(write.path.as_ref())
                || write.revision <= source.revision || write.path.starts_with(&source.path) || source.path.starts_with(&write.path) {
                return Err(IrVerifyError::new("mutable refinement changes its original disjoint mutation proof"));
            }
            if !index.dominates(tree, instruction, receipt.instruction)? { return Err(IrVerifyError::new("mutable refinement disjoint write leaves its original lexical path")); }
            accepted.insert(instruction);
        }
        for write in generic.mutable_binding_receipts().filter(|write| write.binding == receipt.binding && write.owner == receipt.owner && write.read_origin.is_none() && write.ordinal != 0) {
            if Self::mutable_write_may_precede_read(tree, index, write.instruction, receipt.instruction)? && !index.dominates(tree, write.instruction, predicate_allocation)? { return Err(IrVerifyError::new("mutable refinement crosses an original whole binding write")); }
        }
        for path in generic.mutable_paths().filter(|path| path.binding == receipt.binding && path.owner == receipt.owner) {
            if Self::mutable_write_may_precede_read(tree, index, path.instruction, receipt.instruction)? && !index.dominates(tree, path.instruction, predicate_allocation)? && !accepted.contains(&path.instruction) { return Err(IrVerifyError::new("mutable refinement crosses an unproved original path write")); }
        }
        Ok(())
    }

    // Reads retain the binding invariant. Initializers and each actual write
    // are checked independently, so a self assignment cannot authorize itself.
    pub(super) fn verify_mutable_binding_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, expected: &Type, _active: &mut Vec<u32>) -> Result<bool, IrVerifyError> {
        let Some(receipt) = generic.mutable_binding_receipt(instruction)? else { return Ok(false); };
        if receipt.read_origin.is_none() { return Ok(false); }
        let actual = store.semantic.to_type(receipt.binding_type)?;
        let narrowed = receipt.refinement.as_ref().map(|refinement| store.semantic.to_type(refinement.narrowed_type)).transpose()?;
        if receipt.owner != owner || (actual != *expected && narrowed.as_ref() != Some(expected)) { return Err(IrVerifyError::new(format!("mutable read {instruction} changes its invariant checked type: original {actual:?}, requested {expected:?}, source {:?}", receipt.read_origin))); }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::eval::Evaluator;
    use crate::sema::check::Checker;
    use crate::syntax::parser::Parser;

    fn fixture(source: &str) -> FullProgram {
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("mutable-authority.xsh", source);
        let parsed = Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let symbols = parsed.arena.symbol_owner().clone(); let _symbols = symbols.enter();
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let solved = Arc::downgrade(&checked.solved);
        let declarations = Checker::compact_declarations_from_checked(&parsed.arena, &checked);
        let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
        let prepared = FullBuilder::build_compact(&parsed.arena, &declarations, &bodies, source, Arc::new(sources), source_id);
        drop(parsed); drop(checked); drop(declarations); drop(bodies);
        assert!(solved.upgrade().is_none());
        prepared.unwrap()
    }

    #[test]
    fn mutable_nominal_enum_retains_original_assignment_and_read_after_disposal_both_routes() {
        crate::runtime::eval::run_eval(|| {
            let program = Arc::new(fixture("enum Language { LangUnknown, LangRust }\npure selected() -> Int { var language = LangUnknown; language = LangRust; match language { LangUnknown => 0, LangRust => 1 } }\npure updated() -> Int { selected() }\n"));
            program.symbol_owner().with_current(|| {
                for recursive in [false, true] {
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let work = || evaluator.call_indexed_direct(LoweredFunctionKey::Name(Name::intern("updated")), LoweredFunctionKind::Pure, &[], Span::new(program.store.source_id, 0, 0)).expect("original enum function exists");
                    let result = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(LoweredFunctionKey::Name(Name::intern("selected")), recursive, work);
                    assert_eq!(result.unwrap(), crate::runtime::value::Value::Int(1));
                }
            });
        });
    }

    #[test]
    fn mutable_nominal_enum_refuses_missing_foreign_and_same_typed_assignment_authority() {
        crate::runtime::eval::run_eval(|| {
            let program = fixture("enum Language { LangUnknown, LangRust }\nenum Other { OtherUnknown, OtherReady }\npure selected() -> Int { var first = LangUnknown; var sibling = LangRust; var foreign = OtherUnknown; first = LangRust; sibling = LangUnknown; foreign = OtherReady; match first { LangUnknown => 0, LangRust => 1 } }\n");
            program.symbol_owner().with_current(|| {
                FullVerifier::verify(&program).unwrap();
                let generic = program.generic_evidence().unwrap();
                let write = generic.mutable_binding_receipts().find(|receipt| receipt.nominal.as_ref().is_some_and(|nominal| nominal.family == "Language") && receipt.assignment.is_some()).unwrap();
                let sibling = generic.mutable_binding_receipts().find(|receipt| receipt.nominal.is_some() && receipt.ordinal == 0 && receipt.read_origin.is_none() && receipt.binding_type == write.binding_type && receipt.binding != write.binding).unwrap();
                let foreign = generic.mutable_binding_receipts().find(|receipt| receipt.nominal.is_some() && receipt.ordinal == 0 && receipt.read_origin.is_none() && receipt.binding_type != write.binding_type).unwrap();
                let read = generic.mutable_binding_receipts().find(|receipt| receipt.nominal.is_some() && receipt.read_origin.is_some()).unwrap();
                let mut missing_write = program.clone();
                missing_write.store.generic.as_deref_mut().unwrap().test_remove_mutable_binding_receipt(write.instruction);
                assert!(FullVerifier::verify(&missing_write).is_err());
                let mut missing_read = program.clone();
                missing_read.store.generic.as_deref_mut().unwrap().test_remove_mutable_binding_receipt(read.instruction);
                assert!(FullVerifier::verify(&missing_read).is_err());
                let mut slot = program.clone();
                let range = slot.store.data[write.instruction as usize].range();
                slot.store.extra[range.start as usize] = sibling.payload[0];
                slot.store.generic.as_deref_mut().unwrap().test_mutable_binding_receipt_mut(write.instruction).unwrap().payload[0] = sibling.payload[0];
                assert!(FullVerifier::verify(&slot).is_err(), "same enum storage does not confer the sibling's assignment authority");
                let mut rhs = program.clone();
                let range = rhs.store.data[write.instruction as usize].range();
                rhs.store.extra[range.start as usize + 2] = foreign.value.unwrap();
                assert!(FullVerifier::verify(&rhs).is_err());
                let mut declaration = program.clone();
                declaration.store.generic.as_deref_mut().unwrap().test_mutable_binding_receipt_mut(write.instruction).unwrap().nominal = foreign.nominal.clone();
                assert!(FullVerifier::verify(&declaration).is_err(), "same nullary representation does not confer foreign enum authority");
                let mut member = program.clone();
                let nominal = member.store.generic.as_deref_mut().unwrap().test_mutable_binding_receipt_mut(write.instruction).unwrap().nominal.as_mut().unwrap();
                nominal.members[0] = foreign.nominal.as_ref().unwrap().members[0].clone();
                assert!(FullVerifier::verify(&member).is_err());
                let mut absent = program.clone();
                absent.store.generic.as_deref_mut().unwrap().test_mutable_binding_receipt_mut(write.instruction).unwrap().nominal = None;
                assert!(FullVerifier::verify(&absent).is_err());
            });
        });
    }

    #[test]
    fn mutable_record_shorthand_retains_original_binding_read_after_disposal_both_routes() {
        crate::runtime::eval::run_eval(|| {
            let program = Arc::new(fixture("type Count = {count: Int}\npure selected() -> Count { var count = 0; var spare = 10; count += 1; {count} }\npure updated() -> Int { selected().count }\n"));
            program.symbol_owner().with_current(|| {
                FullVerifier::verify(&program).unwrap();
                for recursive in [false, true] {
                    let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let work = || evaluator.call_indexed_direct(LoweredFunctionKey::Name(Name::intern("updated")), LoweredFunctionKind::Pure, &[], Span::new(program.store.source_id, 0, 0)).expect("original shorthand function exists");
                    let result = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(LoweredFunctionKey::Name(Name::intern("selected")), recursive, work);
                    assert_eq!(result.unwrap(), crate::runtime::value::Value::Int(1));
                }
            });
        });
    }

    #[test]
    fn mutable_byte_scanner_loop_index_retains_original_read_and_write_proofs_both_routes() {
        crate::runtime::eval::run_eval(|| {
            let program = Arc::new(fixture("pure selected(text: Bytes) -> Int { var index = 0; var total = 0; while index < text.len() { let ch = (text.byte_at(index) ?? -1); total += ch; index += 1 }; total }\npure updated() -> Int { selected(b\"abc\") }\n"));
            program.symbol_owner().with_current(|| {
                assert!(program.store.tags.contains(&FullTag::IntStrByteAtSlot));
                let generic = program.generic_evidence().unwrap();
                let (_, native) = generic.native_scalar_sources().find(|(_, source)| source.byte_at_fallback.is_some()).unwrap();
                let index = native.byte_at_fallback.as_ref().unwrap().index_instruction;
                assert!(generic.mutable_binding_receipt(index).unwrap().is_some(), "folded byte index retains its independent original mutable read");
                let mut missing = (*program).clone();
                missing.store.generic.as_deref_mut().unwrap().test_remove_mutable_binding_receipt(index);
                assert!(FullVerifier::verify(&missing).is_err());
                for recursive in [false, true] {
                    let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let work = || evaluator.call_indexed_direct(LoweredFunctionKey::Name(Name::intern("updated")), LoweredFunctionKind::Pure, &[], Span::new(program.store.source_id, 0, 0)).expect("original scanner exists");
                    let result = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(LoweredFunctionKey::Name(Name::intern("selected")), recursive, work);
                    assert_eq!(result.unwrap(), crate::runtime::value::Value::Int(294));
                }
            });
        });
    }

    #[test]
    fn mutable_guarded_record_read_refuses_changed_predicate_guard_alias_and_disjoint_write() {
        crate::runtime::eval::run_eval(|| {
            let program = fixture("type Inner = {value: Str?, count: Int}\ntype Outer = {inner: Inner}\npure selected() -> Str { var report: Outer = {inner: {value: \"ready\", count: 1}}; let available = report.inner.value != null; let retained = available; report.inner.count = 2; guard retained else {return \"missing\"}; report.inner.value ?? \"missing\" }\n");
            program.symbol_owner().with_current(|| {
                FullVerifier::verify(&program).unwrap();
                let read = program.generic_evidence().unwrap().mutable_binding_receipts().find(|receipt| receipt.refinement.is_some()).unwrap();
                let proof = read.refinement.as_ref().unwrap();
                assert_ne!(read.binding_type, proof.narrowed_type);
                let mut predicate = program.clone();
                predicate.store.tags[proof.predicate as usize] = FullTag::BoolBool;
                assert!(FullVerifier::verify(&predicate).is_err());
                let mut condition = program.clone();
                let range = condition.store.data[proof.condition as usize].range();
                let alias = proof.aliases.first().unwrap().0;
                condition.store.extra[range.start as usize] = program.store.payload(program.store.data[alias as usize].range()).unwrap()[0];
                assert!(FullVerifier::verify(&condition).is_err());
                let mut guard = program.clone();
                let range = guard.store.data[proof.guard as usize].range();
                guard.store.extra[range.start as usize + 1] = 0;
                assert!(FullVerifier::verify(&guard).is_err());
                let mut missing = program.clone();
                missing.store.generic.as_deref_mut().unwrap().test_mutable_binding_receipt_mut(read.instruction).unwrap().refinement = None;
                assert!(FullVerifier::verify(&missing).is_err());
                let write = program.generic_evidence().unwrap().mutable_path_at(proof.writes[0]).unwrap().unwrap();
                let mut path = program.clone();
                let range = path.store.data[write.instruction as usize].range();
                path.store.extra[range.start as usize] = u32::MAX;
                assert!(FullVerifier::verify(&path).is_err());
                let mut sibling = program.clone();
                let block = sibling.store.blocks.iter().find(|block| block.flags & BLOCK_SEQUENCE_KIND_MASK == BLOCK_STATEMENTS && sibling.store.payload(block.instructions).unwrap().contains(&proof.guard)).unwrap().instructions;
                let words = sibling.store.payload(block).unwrap();
                let guard_offset = words.iter().position(|&instruction| instruction == proof.guard).unwrap();
                let alias_offset = words.iter().position(|&instruction| instruction == proof.aliases[0].0).unwrap();
                sibling.store.extra.swap(block.start as usize + guard_offset, block.start as usize + alias_offset);
                assert!(FullVerifier::verify(&sibling).is_err());
            });
        });
    }

    #[test]
    fn mutable_guarded_read_refuses_later_overlapping_write_moved_before_actual_read() {
        crate::runtime::eval::run_eval(|| {
            let program = fixture("type Row = {value: Str?}\npure selected() -> Str { var row: Row = {value: \"ready\"}; let available = row.value != null; guard available else {return \"missing\"}; let verified = row.value; row.value = null; verified }\n");
            program.symbol_owner().with_current(|| {
                FullVerifier::verify(&program).unwrap();
                let generic = program.generic_evidence().unwrap();
                let read = generic.mutable_binding_receipts().find(|receipt| receipt.refinement.is_some()).unwrap();
                let write = generic.mutable_paths().find(|write| write.binding == read.binding).unwrap();
                assert!(write.instruction > read.instruction);
                let verified = generic.value_binding_sources().find(|(_, source)| source.expected.instruction > read.instruction && source.expected.instruction < write.instruction).unwrap().1.expected.instruction;
                let mut moved = program.clone();
                let block = moved.store.blocks.iter().find(|block| block.flags & BLOCK_SEQUENCE_KIND_MASK == BLOCK_STATEMENTS && moved.store.payload(block.instructions).unwrap().contains(&verified) && moved.store.payload(block.instructions).unwrap().contains(&write.instruction)).unwrap().instructions;
                let words = moved.store.payload(block).unwrap();
                let read_offset = words.iter().position(|&instruction| instruction == verified).unwrap();
                let write_offset = words.iter().position(|&instruction| instruction == write.instruction).unwrap();
                moved.store.extra.swap(block.start as usize + read_offset, block.start as usize + write_offset);
                assert!(FullVerifier::verify(&moved).is_err(), "actual lexical mutation order must preserve the original guard proof");
            });
        });
    }

    #[test]
    fn mutable_guarded_read_refuses_conditional_overlapping_write_moved_before_actual_read() {
        crate::runtime::eval::run_eval(|| {
            let program = fixture("type Row = {value: Str?}\npure selected(flag: Bool) -> Str { var row: Row = {value: \"ready\"}; let available = row.value != null; guard available else {return \"missing\"}; let verified = row.value; if flag {row.value = null}; verified }\n");
            program.symbol_owner().with_current(|| {
                FullVerifier::verify(&program).unwrap();
                let generic = program.generic_evidence().unwrap();
                let read = generic.mutable_binding_receipts().find(|receipt| receipt.refinement.is_some()).unwrap();
                let proof = read.refinement.as_ref().unwrap();
                let write = generic.mutable_paths().find(|write| write.binding == read.binding).unwrap();
                assert!(write.instruction > read.instruction);
                let verified = generic.value_binding_sources().find(|(_, source)| source.expected.instruction > read.instruction && source.expected.instruction < write.instruction).unwrap().1.expected.instruction;
                let conditional = program.store.tags.iter().enumerate().find_map(|(instruction, tag)| (matches!(tag, FullTag::StmtIf | FullTag::StmtIfBool) && instruction as u32 != proof.guard).then_some(instruction as u32)).unwrap();
                let mut moved = program.clone();
                let block = moved.store.blocks.iter().find(|block| block.flags & BLOCK_SEQUENCE_KIND_MASK == BLOCK_STATEMENTS && moved.store.payload(block.instructions).unwrap().contains(&verified) && moved.store.payload(block.instructions).unwrap().contains(&conditional)).unwrap().instructions;
                let words = moved.store.payload(block).unwrap();
                let read_offset = words.iter().position(|&instruction| instruction == verified).unwrap();
                let write_offset = words.iter().position(|&instruction| instruction == conditional).unwrap();
                moved.store.extra.swap(block.start as usize + read_offset, block.start as usize + write_offset);
                assert!(FullVerifier::verify(&moved).is_err(), "an enclosing conditional can execute its original overlapping write before the read");
            });
        });
    }

    #[test]
    fn mutable_captured_writes_refuse_same_typed_cell_rhs_missing_and_foreign_allocation() {
        crate::runtime::eval::run_eval(|| {
            let program = fixture("var first: Int = 0\nvar second: Int = 0\nproc update() [] -> Unit { first = 1; second = 2 }\n");
            program.symbol_owner().with_current(|| {
                FullVerifier::verify(&program).unwrap();
                let generic = program.generic_evidence().unwrap();
                let (capture, allocation) = generic.lexical_captures().find(|(_, capture)| program.store.string(capture.name).unwrap() == "first").unwrap();
                let (foreign_id, foreign) = generic.lexical_captures().find(|(_, other)| other.target == allocation.target && other.binding != allocation.binding).unwrap();
                assert_eq!(allocation.ty, foreign.ty);
                let instruction = program.store.tags.iter().enumerate().find_map(|(instruction, tag)| (matches!(tag, FullTag::StmtAssign | FullTag::StmtAssignInt | FullTag::StmtAssignBool) && program.store.payload(program.store.data[instruction].range()).unwrap().first() == Some(&allocation.slot)).then_some(instruction as u32)).unwrap();
                let write = generic.mutable_binding_receipt(instruction).unwrap().expect("an original captured write requires its independent authored assignment receipt");
                assert_eq!(write.binding, allocation.binding);
                assert_eq!(write.capture, Some(capture));
                assert_eq!(write.owner, InstructionOwner::Function(allocation.target));
                assert!(write.assignment.is_some());
                let mut swapped = program.clone();
                let range = swapped.store.data[instruction as usize].range();
                swapped.store.extra[range.start as usize] = foreign.slot;
                assert!(FullVerifier::verify(&swapped).is_err());
                let mut missing = program.clone();
                missing.store.generic.as_deref_mut().unwrap().test_remove_mutable_binding_receipt(instruction);
                assert!(FullVerifier::verify(&missing).is_err());
                let mut rhs = program.clone();
                let foreign_write = generic.mutable_binding_receipts().find(|write| write.binding == foreign.binding && write.assignment.is_some()).unwrap();
                rhs.store.extra[range.start as usize + 2] = foreign_write.value.unwrap();
                assert!(FullVerifier::verify(&rhs).is_err());
                let mut changed = program.clone();
                changed.store.generic.as_deref_mut().unwrap().test_lexical_capture_mut(capture).unwrap().binding = foreign.binding;
                assert!(FullVerifier::verify(&changed).is_err());
                let mut foreign_header = program.clone();
                foreign_header.store.generic.as_deref_mut().unwrap().test_lexical_capture_mut(foreign_id).unwrap().target = IrFunctionId::from_raw(u32::MAX - 1).unwrap();
                assert!(FullVerifier::verify(&foreign_header).is_err());
            });
        });
    }

    #[test]
    fn mutable_driver_aggregates_keep_checked_nominal_collection_and_alias_roots_both_routes() {
        crate::runtime::eval::run_eval(|| {
            for (source, stdout) in [
                ("type Inner[T] = {value: T?}\ntype Outer[T] = {inner: Inner[T], anchor: T, items: List[T] = []}\npure observed(value: Int) -> Int { value }\nvar value = Outer(inner: Inner(value: null), anchor: 9)\nlet retained = value\nvalue.anchor = 12\nprint observed(retained.anchor + value.anchor)\n", b"21\n".as_slice()),
                ("pure observed(value: Int) -> Int { value }\nvar entries: List[Int] = [1, 2]\nlet retained = entries\nentries[1] = 9\nentries += [3]\nprint observed(retained[1] + entries[1] + entries[2])\n", b"14\n".as_slice()),
                ("pure observed(value: Int) -> Int { value }\nvar values: Map[Int, Int] = {[3]: 1}\nlet retained = values\nvalues[3] = 9\nprint observed(values[3] + retained[3])\n", b"10\n".as_slice()),
            ] {
                for recursive in [false, true] {
                    let mut sources = SourceMap::new();
                    let source_id = sources.add_file("mutable-driver-aggregate.xsh", source);
                    let parsed = Parser::parse_source_arena_only(source_id, source);
                    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                    let symbols = parsed.arena.symbol_owner().clone();
                    symbols.with_current(|| {
                        let checked = Checker::check_arena(&parsed.arena, source);
                        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
                        let solved = Arc::downgrade(&checked.solved);
                        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
                        let plan = evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
                        drop(checked); drop(parsed);
                        assert!(solved.upgrade().is_none());
                        let work = || evaluator.try_eval_installed_compact_indexed_only_inner(plan).unwrap_or_else(|_| panic!("original driver remains installed"));
                        let output = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(LoweredFunctionKey::Name(Name::intern("observed")), recursive, work);
                        assert_eq!(output.stdout, stdout, "{:?}", output.diagnostics);
                        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
                    });
                }
            }
        });
    }

    #[test]
    fn mutable_driver_aggregate_paths_refuse_original_name_slot_selector_and_missing_writes() {
        crate::runtime::eval::run_eval(|| {
            let program = fixture("var row = {count: 1, spare: 7}\nvar other = {count: 10, spare: 70}\nrow.count = 3\nprint ${row.count}\n");
            program.symbol_owner().with_current(|| {
                FullVerifier::verify(&program).unwrap();
                let path = program.generic_evidence().unwrap().mutable_paths().next().unwrap();
                let InstructionOwner::Driver(step) = path.owner else { panic!("original aggregate driver expected"); };
                let mut name = program.clone();
                let string = IrStringId::from_raw(path.payload[1]).unwrap();
                let range = name.store.strings[string.index()].bounds(name.store.string_bytes.len()).unwrap();
                name.store.string_bytes[range].copy_from_slice(b"spare");
                assert!(FullVerifier::verify(&name).is_err());
                let slots = program.store.driver_steps[step as usize].slots.bounds(program.store.driver_slots.len()).unwrap();
                let foreign = program.store.driver_slots[slots].iter().find(|slot| program.store.string(slot.name).unwrap() == "other").unwrap().slot;
                assert_ne!(foreign, path.slot);
                let mut slot = program.clone();
                let range = slot.store.data[path.instruction as usize].range();
                slot.store.extra[range.start as usize] = foreign;
                assert!(FullVerifier::verify(&slot).is_err());
                let mut missing = program.clone();
                missing.store.generic.as_deref_mut().unwrap().test_remove_mutable_path(path.instruction);
                assert!(FullVerifier::verify_generic_evidence(&missing.store).is_err());
                assert!(FullVerifier::verify(&missing).is_err());
            });
            let list = fixture("var rows = [1, 2]\nrows[1] = 9\nprint ${rows[0]}\n");
            list.symbol_owner().with_current(|| {
                FullVerifier::verify(&list).unwrap();
                let path = list.generic_evidence().unwrap().mutable_paths().next().unwrap();
                let super::super::super::generic::MutablePathStep::Index { instruction, .. } = path.steps[0] else { panic!("original driver selector expected"); };
                let mut selector = list.clone();
                let range = selector.store.data[instruction as usize].range();
                selector.store.extra[range.start as usize] = 0;
                assert!(FullVerifier::verify(&selector).is_err());
            });
        });
    }

    #[test]
    fn mutable_driver_bindings_execute_original_writes_and_shadows_after_frontend_disposal_both_routes() {
        crate::runtime::eval::run_eval(|| {
            let source = "pure observed(value: Int) -> Int { value }\nvar total: Int = 0\nvar index: Int = 0\nwhile index < 3 { total += index; index += 1 }\ntotal = total + 1\n{ var total = 20; total = total - 1; let _ = total }\nprint observed(total)\n";
            for recursive in [false, true] {
                let mut sources = SourceMap::new();
                let source_id = sources.add_file("mutable-driver.xsh", source);
                let parsed = Parser::parse_source_arena_only(source_id, source);
                assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                let symbols = parsed.arena.symbol_owner().clone();
                symbols.with_current(|| {
                    let checked = Checker::check_arena(&parsed.arena, source);
                    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
                    let solved = Arc::downgrade(&checked.solved);
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
                    let plan = evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked).unwrap_or_else(|error| panic!("{error:?}"));
                    drop(checked); drop(parsed);
                    assert!(solved.upgrade().is_none());
                    let work = || evaluator.try_eval_installed_compact_indexed_only_inner(plan).unwrap_or_else(|_| panic!("original driver remains installed"));
                    let output = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(LoweredFunctionKey::Name(Name::intern("observed")), recursive, work);
                    assert_eq!(output.stdout, b"4\n", "{:?}", output.diagnostics);
                    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
                });
            }
        });
    }

    #[test]
    fn mutable_driver_binding_refuses_missing_foreign_changed_write_and_allocation() {
        crate::runtime::eval::run_eval(|| {
            let program = fixture("pure observed(value: Int) -> Int { value }\nvar total: Int = 1\nvar other: Int = 20\ntotal += 2\nprint observed(total)\n");
            program.symbol_owner().with_current(|| {
                FullVerifier::verify(&program).unwrap();
                let generic = program.generic_evidence().unwrap();
                let definition = generic.mutable_driver_receipts().find(|receipt| receipt.name == Name::intern("total") && receipt.ordinal == 0).unwrap();
                let write = generic.mutable_driver_receipts().find(|receipt| receipt.ordinal != 0).unwrap();
                let read = generic.mutable_binding_receipts().find(|receipt| receipt.binding == definition.binding && receipt.read_origin.is_some()).unwrap();
                let mut missing = program.clone();
                missing.store.generic.as_deref_mut().unwrap().test_remove_mutable_driver_receipt(write.step);
                assert!(FullVerifier::verify_generic_evidence(&missing.store).unwrap_err().message.contains("original receipt ledger"));
                assert!(FullVerifier::verify(&missing).is_err());
                let mut missing_allocation = program.clone();
                missing_allocation.store.generic.as_deref_mut().unwrap().test_remove_mutable_driver_receipt(definition.step);
                assert!(FullVerifier::verify_generic_evidence(&missing_allocation.store).unwrap_err().message.contains("original receipt ledger"));
                assert!(FullVerifier::verify(&missing_allocation).is_err());
                let mut allocation = program.clone();
                let range = allocation.store.driver_steps[definition.step as usize].data.range();
                allocation.store.extra[range.start as usize] = Name::intern("other").symbol().raw();
                assert!(FullVerifier::verify(&allocation).is_err());
                let mut op = program.clone();
                op.store.assign_ops[write.payload[1] as usize] = AssignOp::Set;
                assert!(FullVerifier::verify(&op).is_err());
                let mut rhs = program.clone();
                let range = rhs.store.driver_steps[write.step as usize].data.range();
                rhs.store.extra[range.start as usize + 2] = definition.value;
                assert!(FullVerifier::verify(&rhs).is_err());
                let mut foreign = program.clone();
                let InstructionOwner::Driver(read_step) = read.owner else { panic!("original driver read expected"); };
                let slots = program.store.driver_steps[read_step as usize].slots.bounds(program.store.driver_slots.len()).unwrap();
                let foreign_slot = program.store.driver_slots[slots].iter().find(|slot| program.store.string(slot.name).unwrap() == "other").unwrap().slot;
                assert_ne!(foreign_slot, read.payload[0]);
                let range = foreign.store.data[read.instruction as usize].range();
                foreign.store.extra[range.start as usize] = foreign_slot;
                assert!(FullVerifier::verify(&foreign).is_err());
                let mut agreeing = program.clone();
                agreeing.store.generic.as_deref_mut().unwrap().test_mutable_driver_receipt_mut(write.step).unwrap().payload[2] = definition.value;
                let range = agreeing.store.driver_steps[write.step as usize].data.range();
                agreeing.store.extra[range.start as usize + 2] = definition.value;
                assert!(FullVerifier::verify(&agreeing).is_err());
            });
            let loop_program = fixture("var total: Int = 1\nwhile total < 2 { total += 1 }\nprint $total\n");
            loop_program.symbol_owner().with_current(|| {
                FullVerifier::verify(&loop_program).unwrap();
                let write = loop_program.generic_evidence().unwrap().mutable_binding_receipts().find(|receipt| receipt.assignment.is_some()).unwrap();
                let InstructionOwner::Driver(step) = write.owner else { panic!("original driver write expected"); };
                let mut synchronization = loop_program.clone();
                let slots = synchronization.store.driver_steps[step as usize].slots.bounds(synchronization.store.driver_slots.len()).unwrap();
                let mapping = synchronization.store.driver_slots[slots].iter_mut().find(|mapping| mapping.slot == write.payload[0]).unwrap();
                mapping.flags &= !DRIVER_SLOT_WRITE;
                assert!(FullVerifier::verify(&synchronization).is_err(), "a genuine driver slot write must still synchronize its original scope");
            });
        });
    }

    #[test]
    fn mutable_scalar_assignments_execute_original_invariant_after_frontend_disposal_both_routes() {
        crate::runtime::eval::run_eval(|| {
            let source = "pure selected() -> Int { var value = 7; value = value - 2; { var value = 20; value = value - 1; let _ = value }; value = value - 1; value }\nprint selected()\n";
            for (source, status, stdout) in [(source, 0, b"4\n".as_slice()), ("pure selected() -> Bool { var value = true; value = false; value }\nprint selected()\n", 0, b"false\n".as_slice()), ("pure selected() -> UInt { var result: UInt = 1; result -= 2; result }\nprint selected()\nprint \"forbidden\"\n", 0, b"".as_slice()), ("pure selected(value: Int) -> UInt { var result: UInt = 1; result = value; result }\nprint selected(-1)\nprint \"forbidden\"\n", 0, b"".as_slice())] {
            for recursive in [false, true] {
                let mut sources = SourceMap::new();
                let source_id = sources.add_file("mutable-routes.xsh", source);
                let parsed = Parser::parse_source_arena_only(source_id, source);
                assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                let symbols = parsed.arena.symbol_owner().clone();
                symbols.with_current(|| {
                    let checked = Checker::check_arena(&parsed.arena, source);
                    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
                    let solved = Arc::downgrade(&checked.solved);
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
                    let plan = evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked).unwrap();
                    drop(checked); drop(parsed);
                    assert!(solved.upgrade().is_none());
                    let execute = || evaluator.try_eval_installed_compact_indexed_only_inner(plan).unwrap_or_else(|_| panic!("mutable program remains installed"));
                    let output = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(LoweredFunctionKey::Name(Name::intern("selected")), recursive, execute);
                    assert_eq!(output.status, status, "{:?}", output.diagnostics);
                    assert_eq!(output.stdout, stdout);
                    if stdout.is_empty() { assert!(output.diagnostics.iter().any(|diagnostic| diagnostic.message.contains("UInt")), "{:?}", output.diagnostics); } else { assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics); }
                });
            }
            }
        });
    }

    #[test]
    fn mutable_list_accumulation_infers_original_path_items_and_executes_both_routes_after_frontend_disposal() {
        crate::runtime::eval::run_eval(|| {
            let source = "pure selected() { var entries = []; for destination in [p\"first\", p\"second\"] { entries += [destination] }; entries }\nprint selected()[0].display()\n";
            for recursive in [false, true] {
                let mut sources = SourceMap::new();
                let source_id = sources.add_file("mutable-list-accumulation.xsh", source);
                let parsed = Parser::parse_source_arena_only(source_id, source);
                assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                let symbols = parsed.arena.symbol_owner().clone();
                symbols.with_current(|| {
                    let checked = Checker::check_arena(&parsed.arena, source);
                    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
                    let solved = Arc::downgrade(&checked.solved);
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
                    let plan = evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked).unwrap();
                    let program = evaluator.indexed_program.as_ref().unwrap();
                    let definition = program.generic_evidence().unwrap().mutable_binding_receipts().find(|receipt| receipt.read_origin.is_none() && receipt.ordinal == 0).unwrap();
                    assert_eq!(program.store.semantic.to_type(definition.binding_type).unwrap(), Type::List(Box::new(Type::Path)));
                    assert_eq!(program.store.semantic.to_type(definition.value_type.unwrap()).unwrap(), Type::List(Box::new(Type::Path)));
                    drop(checked); drop(parsed);
                    assert!(solved.upgrade().is_none());
                    let work = || evaluator.try_eval_installed_compact_indexed_only_inner(plan).unwrap_or_else(|_| panic!("original List accumulation remains installed"));
                    let output = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(LoweredFunctionKey::Name(Name::intern("selected")), recursive, work);
                    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
                    assert_eq!(output.stdout, b"first\n");
                });
            }
        });
    }

    #[test]
    fn mutable_list_compound_rejects_operator_pool_reinterpretation() {
        crate::runtime::eval::run_eval(|| {
            let program = fixture("pure selected() -> List[Path] { var entries = []; entries += [p\"first\"]; entries }\n");
            program.symbol_owner().with_current(|| {
                FullVerifier::verify(&program).unwrap();
                let write = program.generic_evidence().unwrap().mutable_binding_receipts().find(|receipt| receipt.ordinal > 0).unwrap();
                assert_eq!(write.tag, FullTag::StmtAssign);
                let mut changed = program.clone();
                changed.store.assign_ops[write.payload[1] as usize] = AssignOp::Set;
                assert!(FullVerifier::verify(&changed).is_err(), "List compound keeps its original selected assignment operator");
            });
        });
    }

    #[test]
    fn mutable_assignment_authority_rejects_rewritten_rhs_and_cross_scope_read() {
        crate::runtime::eval::run_eval(|| {
            let program = fixture("pure selected(left: Int) -> Int { var first = left - 1; { var second = left - 2; second = 3; let _ = second }; first = first - 1; first }\n");
            program.symbol_owner().with_current(|| {
                FullVerifier::verify(&program).unwrap();
                let receipts = program.generic_evidence().unwrap();
                let write = receipts.mutable_binding_receipts().find(|receipt| receipt.ordinal > 0).unwrap();
                let mut rewritten = program.clone();
                let range = rewritten.store.data[write.instruction as usize].range();
                rewritten.store.extra[range.start as usize] = u32::MAX;
                assert!(FullVerifier::verify(&rewritten).is_err());
                let mut agreeing = program.clone();
                agreeing.store.generic.as_deref_mut().unwrap().test_mutable_binding_receipt_mut(write.instruction).unwrap().payload[0] = 0;
                let range = agreeing.store.data[write.instruction as usize].range();
                agreeing.store.extra[range.start as usize] = 0;
                assert!(agreeing.generic_evidence().unwrap().mutable_binding_receipt(write.instruction).unwrap_err().message.contains("original receipt"));
                assert!(FullVerifier::verify(&agreeing).is_err());
                let mut missing = program.clone();
                missing.store.generic.as_deref_mut().unwrap().test_remove_mutable_binding_receipt(write.instruction);
                assert!(FullVerifier::verify(&missing).is_err());
                let read = receipts.mutable_binding_receipts().find(|receipt| receipt.read_origin.is_some()).unwrap();
                let other = receipts.mutable_binding_receipts().find(|receipt| receipt.read_origin.is_none() && receipt.ordinal == 0 && receipt.binding != read.binding).unwrap();
                let mut sibling = program.clone();
                let range = sibling.store.data[read.instruction as usize].range();
                sibling.store.extra[range.start as usize] = other.payload[0];
                assert!(FullVerifier::verify(&sibling).is_err());
            });
        });
    }

    #[test]
    fn mutable_original_read_moved_to_sibling_body_loses_lexical_authority() {
        crate::runtime::eval::run_eval(|| {
            let program = fixture("pure selected(left: Int) -> Int { { var first = left - 1; let _ = first }; { var second = left - 2; let _ = second }; left }\n");
            program.symbol_owner().with_current(|| {
                let reads: Vec<_> = program.generic_evidence().unwrap().mutable_binding_receipts().filter(|receipt| receipt.read_origin.is_some()).collect();
                assert_eq!(reads.len(), 2);
                let statements: Vec<_> = reads.iter().map(|read| program.store.tags.iter().enumerate().find_map(|(instruction, tag)| {
                    (*tag == FullTag::StmtExpr && program.store.payload(program.store.data[instruction].range()).unwrap().first() == Some(&read.instruction)).then_some(instruction)
                }).unwrap()).collect();
                let mut sibling = program.clone();
                for index in 0..2 {
                    let range = sibling.store.data[statements[index]].range();
                    sibling.store.extra[range.start as usize] = reads[1 - index].instruction;
                }
                let error = FullVerifier::verify(&sibling).unwrap_err();
                assert!(error.message.contains("outside its original binding scope"), "{}", error.message);
            });
        });
    }

}
