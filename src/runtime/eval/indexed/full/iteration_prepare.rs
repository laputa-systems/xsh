use super::*;
use super::super::generic::{OriginalIterationBinding, OriginalIterationUse, OriginalIterationProducer, OriginalLineIterationSource, OriginalLineScan, OriginalLineScanCheck, OriginalLineScanOperation, OperationSourceOrigin, PreparedOperationAuthority};
use crate::sema::inference::ScopedRoot;
use crate::sema::operation_graph::{IterableDomain, PreparedLanguageOperation};

fn problem(reason: &'static str) -> IrBuildError { IrBuildError::format(reason, None, 0, 0) }

#[derive(Clone, Debug)]
pub(super) struct StagedLineScan {
    pub original: super::super::super::lower::BuildLineScanOrigin,
    pub instruction: u32,
    pub owner: InstructionOwner,
}

fn iteration_driver_initializer(store: &FullStore, step: u32) -> Result<(Name, u32), IrVerifyError> {
    let entry = *store.driver_steps.get(step as usize).ok_or_else(|| IrVerifyError::new("iteration initializer driver step is missing"))?;
    if entry.tag != FullDriverTag::Let { return Err(IrVerifyError::new("iteration initializer changes its binding operation")); }
    let decoder = FullDecoder { store, owner: driver_owner(step as usize).map_err(|_| IrVerifyError::new("iteration initializer owner is invalid"))?, instruction_range: store.driver_instruction_range(step as usize)?, instruction_states: None, block_states: None, slot_count: entry.slot_count, pattern_tree: None, pattern_ceiling: Cell::new(usize::MAX), verified: true };
    let mut payload = FullCursor::new(store.payload(entry.data.range())?);
    let name = Name::decode(&decoder, &mut payload)?;
    Option::<LoweredType>::decode(&decoder, &mut payload)?;
    Option::<LoweredTypeCheck>::decode(&decoder, &mut payload)?;
    if bool::decode(&decoder, &mut payload)? { return Err(IrVerifyError::new("iteration initializer becomes mutable")); }
    Ok((name, payload.raw()?))
}

impl FullExecution<'_> {
    pub(in crate::runtime::eval) fn line_scan_counter_binding(&self, instruction: u32, ordinal: usize, slot: usize) -> Result<crate::sema::check::BindingIdentity, IrVerifyError> {
        let generic = self.generic_evidence().ok_or_else(|| IrVerifyError::new("line scan lacks prepared source evidence"))?;
        let scan = generic.line_scan(instruction)?.ok_or_else(|| IrVerifyError::new("line scan lacks its original source receipt"))?;
        FullVerifier::verify_line_scan_physical(&self.decoder, generic, scan)?;
        let check = scan.checks.get(ordinal).ok_or_else(|| IrVerifyError::new("line scan counter check is missing"))?;
        if check.slot as usize != slot { return Err(IrVerifyError::new("line scan counter differs from its original slot")); }
        Ok(check.counter)
    }
    pub(in crate::runtime::eval) fn line_scan_counter_span(&self, instruction: u32, ordinal: usize, slot: usize) -> Result<Span, IrVerifyError> {
        self.line_scan_counter_binding(instruction, ordinal, slot)?;
        let scan = self.generic_evidence().unwrap().line_scan(instruction)?.unwrap();
        Ok(scan.checks[ordinal].increment_span)
    }
}

impl FullBuilder {
    fn iteration_ground_root(&mut self, solved: &crate::sema::check::SolvedTypes, root: ScopedRoot) -> Result<TypeId, IrBuildError> {
        solved.graph.validate_scoped(root).map_err(|_| problem("iteration_original_root_scope"))?;
        let reference = self.generic.get_or_insert_with(GenericEvidenceBuilder::default)
            .prepare_closed_reference(&solved.graph, root.ty, &mut self.store.semantic, &mut self.semantic)
            .map_err(|_| problem("iteration_original_root_not_ground"))?;
        let TypeRef::Ground(ty) = reference else { return Err(problem("iteration_original_root_not_ground")); };
        Ok(ty)
    }

    pub(super) fn stage_original_iteration_statement(&mut self, row: BuildStmtId, instruction: u32, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        if let Some(original) = scratch.line_scan_origins.get(&row) {
            if original.iteration.row != row || self.store.tags.get(instruction as usize) != Some(&FullTag::StmtScanLines) { return Err(problem("line_scan_original_statement_changed")); }
            let raw = self.current_owner.ok_or_else(|| problem("line_scan_owner_missing"))?;
            let owner = if let Some(index) = driver_owner_index(raw) { InstructionOwner::Driver(index as u32) } else { InstructionOwner::Function(IrFunctionId::from_raw(raw).ok_or_else(|| problem("line_scan_owner_invalid"))?) };
            self.line_scan_rows.push(StagedLineScan { original: original.clone(), instruction, owner });
            return Ok(());
        }
        let Some(original) = self.active_iteration_bindings.get(&row).cloned() else { return Ok(()); };
        let statement = scratch.statements.get(original.row.index()).ok_or_else(|| problem("iteration_binding_statement_missing"))?;
        let lines = matches!(original.producer, super::super::super::lower::BuildIterationProducer::Lines { .. });
        let (slot, iterator) = match (lines, statement) {
            (false, BuildStmtRow::For { slot, iter, .. }) => (*slot, *iter),
            (true, BuildStmtRow::ForStrLines { slot, text, .. }) => (*slot, *text),
            _ => return Err(problem("iteration_binding_statement_changed")),
        };
        if slot != original.slot || iterator != original.iterator { return Err(problem("iteration_binding_operand_changed")); }
        let tag = if lines { FullTag::StmtForStrLines } else { FullTag::StmtFor };
        if self.store.tags.get(instruction as usize) != Some(&tag) { return Err(problem("iteration_binding_encoded_statement_changed")); }
        let words = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| problem("iteration_binding_encoded_payload_missing"))?;
        let slot = u32::try_from(original.slot).map_err(|_| problem("iteration_binding_slot_overflow"))?;
        if words.len() != 4 || words[0] != slot { return Err(problem("iteration_binding_encoded_operand_changed")); }
        let iterator = words[1];
        let body = IrBlockId::from_raw(words[2]).ok_or_else(|| problem("iteration_binding_encoded_body_missing"))?;
        let raw = self.current_owner.ok_or_else(|| problem("iteration_binding_owner_missing"))?;
        let owner = if let Some(index) = driver_owner_index(raw) { InstructionOwner::Driver(index as u32) } else { InstructionOwner::Function(IrFunctionId::from_raw(raw).ok_or_else(|| problem("iteration_binding_owner_invalid"))?) };
        self.iteration_binding_rows.push((original, instruction, iterator, slot, body, owner));
        Ok(())
    }

    pub(super) fn prepare_original_iteration_bindings(&mut self) -> Result<(), IrBuildError> {
        let Some(solved) = self.solved.clone() else { return Ok(()); };
        let mut bindings = BTreeMap::new();
        let origins: FxHashMap<_, _> = self.generic_expression_rows.iter().map(|&(instruction, origin, owner)| (instruction, (origin, owner))).collect();
        for (original, instruction, iterator, slot, body, owner) in self.iteration_binding_rows.clone() {
            let iterator_carrier = if original.carrier.is_some() {
                if self.store.tags.get(iterator as usize) != Some(&FullTag::ExprTry) { return Err(problem("iteration_original_carrier_projection_kind")); }
                let words = self.store.payload(self.store.data[iterator as usize].range()).map_err(|_| problem("iteration_original_carrier_projection_payload"))?;
                let [carrier] = words else { return Err(problem("iteration_original_carrier_projection_payload")); };
                Some(*carrier)
            } else { None };
            let authored_iterator = iterator_carrier.unwrap_or(iterator);
            let operation = solved.statement_operations.get(&original.statement).ok_or_else(|| problem("iteration_original_operation_missing"))?;
            let binding = solved.bindings.get(&original.binding).ok_or_else(|| problem("iteration_original_binding_missing"))?;
            let input = ScopedRoot { ty: *solved.expressions.get(&original.iterator_source).ok_or_else(|| problem("iteration_original_iterator_missing"))?, scope: solved.expression_scope(original.iterator_source, operation.caller).map_err(|_| problem("iteration_original_iterator_scope"))? };
            let item = ScopedRoot { ty: operation.result, scope: solved.operation_scope(crate::sema::check::ProducerFlowSource::Statement(original.statement), operation).map_err(|_| problem("iteration_original_item_scope"))? };
            let binding_type = ScopedRoot { ty: binding.ty, scope: binding.scheme.or(item.scope) };
            let iterator_parameter = super::super::super::BuildIterationBindingOrigin::original_parameter(&solved, original.iterator_source, operation.caller).ok_or_else(|| problem("iteration_original_parameter_port"))?;
            if original.caller != operation.caller || binding.owner != operation.caller || binding.mutable
                || original.input.ty != input.ty || original.input.scope != input.scope
                || original.item.ty != item.ty || original.item.scope != item.scope
                || original.binding_type.ty != binding_type.ty || original.binding_type.scope != binding_type.scope
                || original.iterator_parameter != iterator_parameter
                || original.slot != slot as usize || origins.get(&authored_iterator) != Some(&(match original.producer { super::super::super::lower::BuildIterationProducer::Lines { receiver, .. } => receiver, _ => original.iterator_source }, owner))
                || operation.receiver.is_some() || operation.actual_arguments.len() != 1 || operation.binding.supplied_slots != [0]
                || !operation.binding.default_slots.is_empty() || operation.binding.rest_slot.is_some() || operation.binding.dynamic.is_some()
                || solved.graph.resolved(operation.actual_arguments[0]).map_err(|_| problem("iteration_original_operand"))? != solved.graph.resolved(input.ty).map_err(|_| problem("iteration_original_operand"))? {
                return Err(problem("iteration_original_source_changed"));
            }
            let producer = match &original.producer {
                super::super::super::lower::BuildIterationProducer::Parameter => None,
                super::super::super::lower::BuildIterationProducer::Literal => {
                    if matches!(solved.graph.export_type(item.ty), Ok(Type::Record(_))) {
                        let tag = *self.store.tags.get(authored_iterator as usize).ok_or_else(|| problem("iteration_original_literal_instruction"))?;
                        let words = self.store.payload(self.store.data[authored_iterator as usize].range()).map_err(|_| problem("iteration_original_literal_payload"))?.to_vec();
                        if tag != FullTag::ExprList || iterator_parameter.is_some() || iterator_carrier.is_some() { return Err(problem("iteration_original_record_literal_source")); }
                        Some(OriginalIterationProducer { tag, words, declaration: None, initializer: None, lines: None })
                    } else { None }
                }
                super::super::super::lower::BuildIterationProducer::Lines { receiver, receiver_type, receiver_scope, selected, parameter } => {
                    if super::super::super::BuildIterationBindingOrigin::original_lines(&solved, original.iterator_source, *receiver, operation.caller) != Some(*selected)
                        || super::super::super::BuildIterationBindingOrigin::original_parameter(&solved, *receiver, operation.caller) != Some(Some(*parameter)) { return Err(problem("iteration_original_line_source")); }
                    let receiver_root = ScopedRoot { ty: *solved.expressions.get(receiver).ok_or_else(|| problem("iteration_original_line_receiver"))?, scope: solved.expression_scope(*receiver, operation.caller).map_err(|_| problem("iteration_original_line_scope"))? };
                    if *receiver_type != receiver_root.ty || *receiver_scope != receiver_root.scope { return Err(problem("iteration_original_line_root_changed")); }
                    let crate::sema::check::SolvedOperationAuthority::Registry(metadata) = solved.operation_catalog.candidate(&solved.graph, *selected).map_err(|_| problem("iteration_original_line_authority"))? else { return Err(problem("iteration_original_line_authority_kind")); };
                    let authority = PreparedOperationAuthority::Registry { identity: metadata.identity, operation: metadata.operation, binding: metadata.binding, argument_check: metadata.argument_check, semantic_rule: metadata.semantic_rule, lifecycle: metadata.lifecycle, producer_transfer: metadata.producer_transfer.clone() };
                    let receiver_type = self.iteration_ground_root(&solved, receiver_root)?;
                    let tag = *self.store.tags.get(authored_iterator as usize).ok_or_else(|| problem("iteration_original_line_instruction"))?;
                    let words = self.store.payload(self.store.data[authored_iterator as usize].range()).map_err(|_| problem("iteration_original_line_payload"))?.to_vec();
                    if tag != FullTag::ExprParam || words != [parameter.1] { return Err(problem("iteration_original_line_parameter")); }
                    Some(OriginalIterationProducer { tag, words, declaration: None, initializer: None, lines: Some(OriginalLineIterationSource { origin: *receiver, receiver_type, authority, parameter: *parameter }) })
                }
                super::super::super::lower::BuildIterationProducer::NativeStreamCall { selected } => {
                    if super::super::super::BuildIterationBindingOrigin::original_fs_children(&solved, original.iterator_source, operation.caller) != Some(*selected) { return Err(problem("iteration_original_fs_children_authority")); }
                    let tag = *self.store.tags.get(authored_iterator as usize).ok_or_else(|| problem("iteration_original_native_instruction"))?;
                    let words = self.store.payload(self.store.data[authored_iterator as usize].range()).map_err(|_| problem("iteration_original_native_payload"))?.to_vec();
                    if !matches!(tag, FullTag::ExprModuleCall | FullTag::ExprFsList) || words.first().and_then(|index| self.store.runtime_ops.get(*index as usize)) != Some(&RuntimeOp::FsChildren) { return Err(problem("iteration_original_native_operation")); }
                    Some(OriginalIterationProducer { tag, words, declaration: None, initializer: None, lines: None })
                }
                super::super::super::lower::BuildIterationProducer::UserStreamCall { declaration } => {
                    if super::super::super::lower::original_user_stream_call(&solved, original.iterator_source, operation.caller) != Some(*declaration) { return Err(problem("iteration_original_stream_call")); }
                    let tag = *self.store.tags.get(authored_iterator as usize).ok_or_else(|| problem("iteration_original_stream_instruction"))?;
                    let words = self.store.payload(self.store.data[authored_iterator as usize].range()).map_err(|_| problem("iteration_original_stream_payload"))?.to_vec();
                    if !matches!(tag, FullTag::ExprCall | FullTag::ExprDirectPureCall) || words.first().copied() != self.declaration_functions.get(declaration).map(|function| function.raw()) { return Err(problem("iteration_original_stream_target")); }
                    Some(OriginalIterationProducer { tag, words, declaration: Some(*declaration), initializer: None, lines: None })
                }
                super::super::super::lower::BuildIterationProducer::TopLevelBinding { identity, initializer, name, slot } => {
                    if super::super::super::lower::original_top_level_iterator_binding(&solved, original.iterator_source) != Some((*identity, *initializer)) || !matches!(owner, InstructionOwner::Driver(_)) { return Err(problem("iteration_original_top_binding")); }
                    let matches: Vec<_> = origins.iter().filter_map(|(&instruction, &(origin, owner))| (origin == *initializer).then_some((instruction, owner))).collect();
                    let [(initializer_instruction, InstructionOwner::Driver(step))] = matches.as_slice() else { return Err(problem("iteration_original_initializer_ambiguous")); };
                    let InstructionOwner::Driver(current) = owner else { return Err(problem("iteration_original_initializer_owner")); };
                    if *step >= current || iteration_driver_initializer(&self.store, *step).map_err(|_| problem("iteration_original_initializer_payload"))? != (*name, *initializer_instruction) { return Err(problem("iteration_original_initializer_source")); }
                    let tag = *self.store.tags.get(authored_iterator as usize).ok_or_else(|| problem("iteration_original_binding_instruction"))?;
                    let words = self.store.payload(self.store.data[authored_iterator as usize].range()).map_err(|_| problem("iteration_original_binding_payload"))?.to_vec();
                    if tag != FullTag::ExprParam || words != [*slot as u32] { return Err(problem("iteration_original_binding_slot")); }
                    Some(OriginalIterationProducer { tag, words, declaration: None, initializer: Some((*initializer, *step, *initializer_instruction, *name)), lines: None })
                }
            };
            match iterator_parameter {
                Some((_, index)) if self.store.tags.get(authored_iterator as usize) == Some(&FullTag::ExprParam)
                    && self.store.payload(self.store.data[authored_iterator as usize].range()).map_err(|_| problem("iteration_original_parameter_payload"))? == [index] => {},
                None if iterator_carrier.is_some() && self.store.tags.get(authored_iterator as usize) == Some(&FullTag::ExprOk) => {},
                None if iterator_carrier.is_none() && matches!((solved.graph.export_type(input.ty), self.store.tags.get(authored_iterator as usize)),
                    (Ok(Type::List(_)), Some(FullTag::ExprList)) | (Ok(Type::Str), Some(FullTag::ExprStr)) | (Ok(Type::Bytes), Some(FullTag::ExprBytes))) => {},
                None if producer.is_some() => {},
                _ => return Err(problem("iteration_original_parameter_changed")),
            }
            let selected = solved.graph.candidate_evidence(operation.requirement).map_err(|_| problem("iteration_original_selected_owner"))?.ok_or_else(|| problem("iteration_original_selected_missing"))?;
            if selected.candidate != original.selected { return Err(problem("iteration_original_selected_changed")); }
            let crate::sema::check::SolvedOperationAuthority::Language(metadata) = solved.operation_catalog.candidate(&solved.graph, selected.candidate).map_err(|_| problem("iteration_original_authority"))? else { return Err(problem("iteration_original_authority_kind")); };
            if !matches!((metadata.operation, iterator_carrier),
                (PreparedLanguageOperation::Iteration { domain: IterableDomain::List | IterableDomain::Str | IterableDomain::Bytes | IterableDomain::Stream, outer_result: false }, None)
                | (PreparedLanguageOperation::Iteration { domain: IterableDomain::Bytes, outer_result: true }, Some(_))
                | (PreparedLanguageOperation::Iteration { domain: IterableDomain::Stream, outer_result: true }, None)) { return Err(problem("iteration_item_protocol_not_prepared")); }
            let authority = PreparedOperationAuthority::Language { identity: metadata.identity, authority: metadata.authority, operation: metadata.operation, argument_order: metadata.argument_order, statement_result_is_unit: metadata.statement_result_is_unit };
            let input = self.iteration_ground_root(&solved, input)?;
            let item = self.iteration_ground_root(&solved, item)?;
            let binding_type = self.iteration_ground_root(&solved, binding_type)?;
            let id = self.generic_evidence_mut().add_iteration_binding(OriginalIterationBinding { statement: original.statement, binding: original.binding, iterator_origin: original.iterator_source, authority, instruction, iterator, iterator_carrier, slot, body, owner, input, item, binding_type, iterator_parameter, producer }).map_err(|_| problem("iteration_original_binding_capacity"))?;
            if bindings.insert(original.binding, (id, owner)).is_some() { return Err(problem("iteration_original_binding_ambiguous")); }
        }
        for (origin, binding, instruction, owner) in self.iteration_use_rows.clone() {
            let &(binding, original_owner) = bindings.get(&binding).ok_or_else(|| problem("iteration_original_use_binding_missing"))?;
            if owner != original_owner || origins.get(&instruction) != Some(&(origin, owner)) { return Err(problem("iteration_original_use_owner_changed")); }
            let tag = *self.store.tags.get(instruction as usize).ok_or_else(|| problem("iteration_original_use_instruction_missing"))?;
            self.generic_evidence_mut().add_iteration_use(OriginalIterationUse { origin, binding, instruction, owner, tag }).map_err(|_| problem("iteration_original_use_capacity"))?;
        }
        self.prepare_original_line_scans(&solved)
    }

    fn prepare_line_scan_operation(&mut self, solved: &crate::sema::check::SolvedTypes, original: &super::super::super::lower::BuildLineScanOperation, caller: Option<crate::sema::check::DeclarationIdentity>) -> Result<OriginalLineScanOperation, IrBuildError> {
        original.validate(solved, caller).ok_or_else(|| problem("line_scan_original_operation_changed"))?;
        let authority = self.line_scan_authority(solved, original.selected)?;
        let roots = original.roots.iter().map(|&root| self.iteration_ground_root(solved, root)).collect::<Result<Vec<_>, _>>()?;
        let result = self.iteration_ground_root(solved, original.result)?;
        Ok(OriginalLineScanOperation { origin: original.origin, authority, receiver: original.receiver, arguments: original.arguments.clone(), roots, result })
    }

    fn line_scan_authority(&self, solved: &crate::sema::check::SolvedTypes, candidate: crate::sema::inference::CandidateId) -> Result<PreparedOperationAuthority, IrBuildError> {
        Ok(match solved.operation_catalog.candidate(&solved.graph, candidate).map_err(|_| problem("line_scan_original_authority"))? {
            crate::sema::check::SolvedOperationAuthority::Registry(metadata) => PreparedOperationAuthority::Registry { identity: metadata.identity, operation: metadata.operation, binding: metadata.binding, argument_check: metadata.argument_check, semantic_rule: metadata.semantic_rule, lifecycle: metadata.lifecycle, producer_transfer: metadata.producer_transfer.clone() },
            crate::sema::check::SolvedOperationAuthority::Language(metadata) => PreparedOperationAuthority::Language { identity: metadata.identity, authority: metadata.authority, operation: metadata.operation, argument_order: metadata.argument_order, statement_result_is_unit: metadata.statement_result_is_unit },
            _ => return Err(problem("line_scan_original_authority_kind")),
        })
    }

    fn prepare_original_line_scans(&mut self, solved: &crate::sema::check::SolvedTypes) -> Result<(), IrBuildError> {
        for staged in self.line_scan_rows.clone() {
            let original = &staged.original.iteration;
            let super::super::super::lower::BuildIterationProducer::Lines { receiver, receiver_type, receiver_scope, selected, parameter } = original.producer else { return Err(problem("line_scan_original_line_source")); };
            let operation = solved.statement_operations.get(&original.statement).ok_or_else(|| problem("line_scan_original_iteration"))?;
            let input = ScopedRoot { ty: *solved.expressions.get(&original.iterator_source).ok_or_else(|| problem("line_scan_original_input"))?, scope: solved.expression_scope(original.iterator_source, operation.caller).map_err(|_| problem("line_scan_original_input_scope"))? };
            let item = ScopedRoot { ty: operation.result, scope: solved.operation_scope(crate::sema::check::ProducerFlowSource::Statement(original.statement), operation).map_err(|_| problem("line_scan_original_item_scope"))? };
            let binding = solved.bindings.get(&original.binding).ok_or_else(|| problem("line_scan_original_binding"))?;
            let binding_root = ScopedRoot { ty: binding.ty, scope: binding.scheme.or(item.scope) };
            if original.caller != operation.caller || binding.owner != operation.caller || binding.mutable
                || original.input != input || original.item != item || original.binding_type != binding_root
                || solved.graph.candidate_evidence(operation.requirement).map_err(|_| problem("line_scan_original_iteration_selection"))?.map(|evidence| evidence.candidate) != Some(original.selected)
                || super::super::super::BuildIterationBindingOrigin::original_lines(solved, original.iterator_source, receiver, operation.caller) != Some(selected)
                || super::super::super::BuildIterationBindingOrigin::original_parameter(solved, receiver, operation.caller) != Some(Some(parameter))
                || self.declaration_functions.get(&parameter.0).copied().map(InstructionOwner::Function) != Some(staged.owner) { return Err(problem("line_scan_original_source_changed")); }
            let receiver_root = ScopedRoot { ty: *solved.expressions.get(&receiver).ok_or_else(|| problem("line_scan_original_receiver"))?, scope: solved.expression_scope(receiver, operation.caller).map_err(|_| problem("line_scan_original_receiver_scope"))? };
            if receiver_root.ty != receiver_type || receiver_root.scope != receiver_scope { return Err(problem("line_scan_original_receiver_changed")); }
            let input = self.iteration_ground_root(solved, input)?;
            let item = self.iteration_ground_root(solved, item)?;
            let binding_type = self.iteration_ground_root(solved, binding_root)?;
            let receiver_type = self.iteration_ground_root(solved, receiver_root)?;
            let iteration = self.line_scan_authority(solved, original.selected)?;
            let lines = OriginalLineIterationSource { origin: receiver, receiver_type, authority: self.line_scan_authority(solved, selected)?, parameter };
            let trim = staged.original.trim.as_ref().map(|trim| self.prepare_line_scan_operation(solved, trim, operation.caller)).transpose()?;
            let mut checks = Vec::new();
            for check in &staged.original.checks {
                let definition = solved.bindings.get(&check.counter.binding).ok_or_else(|| problem("line_scan_original_counter"))?;
                if definition.owner != operation.caller || !definition.mutable || check.increment.capture.is_some() || check.counter.binding != check.increment.binding
                    || check.counter.source_type.ty != definition.ty || check.increment.statement.source != original.statement.source || check.increment.statement.namespace != original.statement.namespace { return Err(problem("line_scan_original_counter_changed")); }
                let allocation = self.generic_evidence_mut().line_scan_counter_allocation(check.counter.binding, check.counter.statement, staged.owner).map_err(|_| problem("line_scan_original_counter_allocation"))?;
                let counter_type = self.iteration_ground_root(solved, check.counter.source_type)?;
                let value_type = self.iteration_ground_root(solved, check.increment.value_type)?;
                if allocation.payload.first() != Some(&(check.counter.slot as u32)) || allocation.binding_type != counter_type { return Err(problem("line_scan_original_counter_slot_changed")); }
                let compound = check.increment.compound.as_ref().ok_or_else(|| problem("line_scan_original_increment_operation"))?;
                let compound = self.prepare_mutable_compound_contract(check.increment.statement, compound, AssignOp::Add, counter_type, value_type, counter_type)?;
                let producer = solved.producer_flows.node(*solved.binding_producer_flows.get(&(check.increment.binding, check.increment.ordinal)).ok_or_else(|| problem("line_scan_original_counter_version"))?).map_err(|_| problem("line_scan_original_counter_producer"))?;
                if producer.source != (crate::sema::check::ProducerFlowSource::Binding { identity: check.increment.binding, version: check.increment.ordinal }) { return Err(problem("line_scan_original_counter_version_changed")); }
                let crate::sema::check::ProducerFlowKind::Join { inputs } = &producer.kind else { return Err(problem("line_scan_original_counter_producer_kind")); };
                let [input] = inputs.as_slice() else { return Err(problem("line_scan_original_counter_producer_inputs")); };
                let producer = solved.producer_flows.node(*input).map_err(|_| problem("line_scan_original_counter_result"))?;
                if producer.source != crate::sema::check::ProducerFlowSource::Statement(check.increment.statement) || !matches!(producer.kind, crate::sema::check::ProducerFlowKind::Operation { requirement, .. } if requirement == compound.requirement) { return Err(problem("line_scan_original_counter_result_changed")); }
                if check.increment_span.source_id != check.increment.statement.source { return Err(problem("line_scan_original_increment_span")); }
                checks.push(OriginalLineScanCheck { predicate: self.prepare_line_scan_operation(solved, &check.predicate, operation.caller)?, condition: check.condition.clone(), counter: check.counter.binding, allocation_origin: check.counter.statement, allocation: allocation.instruction, slot: u32::try_from(check.counter.slot).map_err(|_| problem("line_scan_counter_slot_overflow"))?, counter_type, increment: check.increment.statement, increment_span: check.increment_span, value: check.increment.value_source, value_type, ordinal: check.increment.ordinal, compound });
            }
            let payload = self.store.payload(self.store.data[staged.instruction as usize].range()).map_err(|_| problem("line_scan_encoded_payload"))?.to_vec();
            let [text_slot, line_slot, block, _] = payload.as_slice() else { return Err(problem("line_scan_encoded_payload_shape")); };
            if *text_slot != parameter.1 || *line_slot as usize != original.slot { return Err(problem("line_scan_encoded_slots_changed")); }
            let block_id = IrBlockId::from_raw(*block).ok_or_else(|| problem("line_scan_encoded_checks_block"))?;
            let block_data = self.store.blocks.get(block_id.index()).ok_or_else(|| problem("line_scan_encoded_checks_missing"))?;
            let checks_block = (*block, block_data.flags, self.store.payload(block_data.instructions).map_err(|_| problem("line_scan_encoded_checks_payload"))?.to_vec());
            self.generic_evidence_mut().add_line_scan(OriginalLineScan { statement: original.statement, binding: original.binding, iterator_origin: original.iterator_source, instruction: staged.instruction, owner: staged.owner, input, item, binding_type, iteration, lines, text_slot: *text_slot, line_slot: *line_slot, payload, span: staged.original.span, checks_block, trim, checks }).map_err(|_| problem("line_scan_original_capacity"))?;
        }
        Ok(())
    }
}

impl FullVerifier {
    fn verify_line_scan_physical(decoder: &FullDecoder<'_>, generic: &GenericEvidenceStore, scan: &OriginalLineScan) -> Result<(), IrVerifyError> {
        let store = decoder.store;
        if store.semantic.to_type(scan.input)? != Type::List(Box::new(Type::Bytes)) || store.semantic.to_type(scan.item)? != Type::Bytes
            || store.semantic.to_type(scan.binding_type)? != Type::Bytes || store.semantic.to_type(scan.lines.receiver_type)? != Type::Bytes { return Err(IrVerifyError::new("line scan changes its original closed line types")); }
        if !decoder.instruction_range.contains(&(scan.instruction as usize)) || store.tags.get(scan.instruction as usize) != Some(&FullTag::StmtScanLines)
            || store.payload(store.data[scan.instruction as usize].range())? != scan.payload { return Err(IrVerifyError::new("line scan changes its original instruction or payload")); }
        let [text_slot, line_slot, block, _] = scan.payload.as_slice() else { return Err(IrVerifyError::new("line scan original payload is invalid")); };
        if *text_slot != scan.text_slot || *line_slot != scan.line_slot || *block != scan.checks_block.0 { return Err(IrVerifyError::new("line scan changes its original text or item storage")); }
        let owner = if let Some(step) = driver_owner_index(decoder.owner) { InstructionOwner::Driver(step as u32) } else { InstructionOwner::Function(IrFunctionId::from_raw(decoder.owner).ok_or_else(|| IrVerifyError::new("line scan owner is invalid"))?) };
        if owner != scan.owner || generic.registered_instruction_origin(scan.instruction, false) != Some((OperationSourceOrigin::Statement(scan.statement), owner)) { return Err(IrVerifyError::new("line scan changes its original source or owner")); }
        let block = store.blocks.get(IrBlockId::from_raw(*block).ok_or_else(|| IrVerifyError::new("line scan checks block is invalid"))?.index()).ok_or_else(|| IrVerifyError::new("line scan checks block is missing"))?;
        if block.owner != decoder.owner || block.flags != BLOCK_LIST || block.flags != scan.checks_block.1 || store.payload(block.instructions)? != scan.checks_block.2 { return Err(IrVerifyError::new("line scan changes its original checks block")); }
        let mut payload = FullCursor::new(&scan.payload);
        let text = usize::decode(decoder, &mut payload)?;
        let line = usize::decode(decoder, &mut payload)?;
        let checks = Vec::<ScanCheck>::decode(decoder, &mut payload)?;
        if Span::decode(decoder, &mut payload)? != scan.span { return Err(IrVerifyError::new("line scan changes its original source span")); }
        payload.finish()?;
        if text >= decoder.slot_count as usize || line >= decoder.slot_count as usize || checks.len() != scan.checks.len() { return Err(IrVerifyError::new("line scan changes its original slots or number of checks")); }
        let InstructionOwner::Function(function) = owner else { return Err(IrVerifyError::new("line scan has no original formal receiver")); };
        let callable = store.functions.get(function.index()).ok_or_else(|| IrVerifyError::new("line scan function is missing"))?;
        if text >= callable.params.len as usize { return Err(IrVerifyError::new("line scan receiver is not its original formal parameter")); }
        let formal = if let Some(scope) = generic.scope_for_function(function) {
            let TypeRef::Ground(formal) = generic.scope(scope)?.parameters[text] else { return Err(IrVerifyError::new("line scan receiver has no closed formal proof")); };
            formal
        } else {
            let range = callable.params.bounds(store.params.len()).ok_or_else(|| IrVerifyError::new("line scan formal range is invalid"))?;
            TypeId::from_raw(store.params[range.start + text].type_id).ok_or_else(|| IrVerifyError::new("line scan formal type is invalid"))?
        };
        if formal != scan.lines.receiver_type { return Err(IrVerifyError::new("line scan changes its original receiver type")); }
        for (actual, original) in checks.iter().zip(&scan.checks) {
            if store.semantic.to_type(original.counter_type)? != Type::Int || store.semantic.to_type(original.value_type)? != Type::Int { return Err(IrVerifyError::new("line scan changes its original counter types")); }
            let condition_matches = match (&actual.condition, &original.condition) {
                (ScanCondition::TrimEmpty, ScanCondition::TrimEmpty) => true,
                (ScanCondition::TrimStartsWith(actual), ScanCondition::TrimStartsWith(original)) | (ScanCondition::StartsWith(actual), ScanCondition::StartsWith(original)) => actual == original,
                _ => false,
            };
            if !condition_matches || actual.counter_slot != original.slot as usize || actual.counter_slot >= decoder.slot_count as usize { return Err(IrVerifyError::new("line scan changes its original condition, prefix, or counter")); }
            let allocation = generic.mutable_binding_receipt(original.allocation)?.ok_or_else(|| IrVerifyError::new("line scan counter lacks its original mutable allocation"))?;
            if allocation.binding != original.counter || allocation.statement != Some(original.allocation_origin) || allocation.owner != scan.owner || allocation.ordinal != 0
                || allocation.read_origin.is_some() || allocation.capture.is_some() || allocation.binding_type != original.counter_type
                || allocation.payload.first() != Some(&original.slot) || store.tags.get(allocation.instruction as usize) != Some(&allocation.tag)
                || store.payload(store.data[allocation.instruction as usize].range())? != allocation.payload.as_ref() { return Err(IrVerifyError::new("line scan counter changes its original binding allocation")); }
        }
        Ok(())
    }

    pub(super) fn verify_original_iteration_bindings(store: &FullStore, tree: &super::super::pattern::PatternTree) -> Result<(), IrVerifyError> {
        let Some(generic) = store.generic.as_deref() else {
            if store.tags.contains(&FullTag::StmtScanLines) { return Err(IrVerifyError::new("line scan lacks original prepared evidence")); }
            return Ok(());
        };
        let owners = store.generic_instruction_owners()?;
        let lexical = super::callable_prepare::CallableLexicalIndex::new(store, tree)?;
        for (instruction, tag) in store.tags.iter().enumerate() {
            if *tag != FullTag::StmtScanLines { continue; }
            let scan = generic.line_scan(instruction as u32)?.ok_or_else(|| IrVerifyError::new("line scan lacks its original prepared source"))?;
            let InstructionOwner::Function(function) = scan.owner else { return Err(IrVerifyError::new("line scan has another original owner")); };
            let callable = store.functions.get(function.index()).ok_or_else(|| IrVerifyError::new("line scan function is missing"))?;
            let decoder = FullDecoder { store, owner: function.raw(), instruction_range: store.function_instruction_range(function.index())?, instruction_states: None, block_states: None, slot_count: callable.slot_count, pattern_tree: None, pattern_ceiling: Cell::new(usize::MAX), verified: true };
            Self::verify_line_scan_physical(&decoder, generic, scan)?;
            for check in &scan.checks {
                if !lexical.dominates(tree, check.allocation, scan.instruction)? { return Err(IrVerifyError::new("line scan counter has no dominating original allocation")); }
            }
        }
        let mut assigned = std::collections::BTreeSet::new();
        for (instruction, owner) in owners.iter().copied().enumerate() {
            if !matches!(store.tags[instruction], FullTag::StmtAssign | FullTag::StmtAssignField | FullTag::StmtAssignFieldInt | FullTag::StmtAssignPath | FullTag::StmtAssignInt | FullTag::StmtAssignBool) { continue; }
            if let (Some(owner), Some(&slot)) = (owner, store.payload(store.data[instruction].range())?.first()) {
                let owner_key = match owner { InstructionOwner::Function(function) => (false, function.raw()), InstructionOwner::Driver(step) => (true, step) };
                assigned.insert((owner_key, slot));
            }
        }
        let mut body_roots = FxHashMap::default();
        for (id, _) in generic.iteration_bindings() {
            let binding = generic.iteration_binding(id)?;
            let line_source = binding.producer.as_ref().and_then(|producer| producer.lines.as_ref());
            let statement_tag = if line_source.is_some() { FullTag::StmtForStrLines } else { FullTag::StmtFor };
            if store.tags.get(binding.instruction as usize) != Some(&statement_tag) { return Err(IrVerifyError::new("iteration original statement changes its operation")); }
            let words = store.payload(store.data[binding.instruction as usize].range())?;
            if words.len() != 4 || words[0] != binding.slot || words[1] != binding.iterator || words[2] != binding.body.raw() { return Err(IrVerifyError::new("iteration original statement changes its operand, slot, or body")); }
            let block = store.blocks.get(binding.body.index()).ok_or_else(|| IrVerifyError::new("iteration body is missing"))?;
            if block.flags != BLOCK_STATEMENTS { return Err(IrVerifyError::new("iteration body has another structural kind")); }
            let roots = store.payload(block.instructions)?.get(1..).ok_or_else(|| IrVerifyError::new("iteration body roots are missing"))?;
            for &root in roots {
                if body_roots.insert(root, id).is_some() { return Err(IrVerifyError::new("iteration bodies share a structural root")); }
            }
            let owner_key = match binding.owner { InstructionOwner::Function(function) => (false, function.raw()), InstructionOwner::Driver(step) => (true, step) };
            if assigned.contains(&(owner_key, binding.slot)) { return Err(IrVerifyError::new("iteration item is mutable without an assignment proof")); }
            let authored_iterator = if let Some(carrier) = binding.iterator_carrier {
                if store.tags.get(binding.iterator as usize) != Some(&FullTag::ExprTry)
                    || store.payload(store.data[binding.iterator as usize].range())? != [carrier]
                    || !tree.is_descendant(binding.iterator, carrier)? { return Err(IrVerifyError::new("iteration carrier changes its original success projection")); }
                carrier
            } else { binding.iterator };
            if let Some(producer) = &binding.producer {
                if store.tags.get(authored_iterator as usize) != Some(&producer.tag) || store.payload(store.data[authored_iterator as usize].range())? != producer.words { return Err(IrVerifyError::new("iteration producer changes its original operation or operands")); }
                if let Some(lines) = &producer.lines {
                    if producer.tag != FullTag::ExprParam || producer.words != [lines.parameter.1] { return Err(IrVerifyError::new("line iteration changes its original receiver parameter")); }
                    Self::verify_generic_source(store, generic, authored_iterator, binding.owner, &store.semantic.to_type(lines.receiver_type)?, None, &mut Vec::new())?;
                }
                if let Some((origin, step, initializer, name)) = producer.initializer {
                    let (actual_name, actual) = iteration_driver_initializer(store, step)?;
                    if actual_name != name || actual != initializer || generic.registered_instruction_origin(initializer, false) != Some((OperationSourceOrigin::Expression(origin), InstructionOwner::Driver(step))) { return Err(IrVerifyError::new("iteration binding changes its original initializer")); }
                    let InstructionOwner::Driver(current) = binding.owner else { return Err(IrVerifyError::new("iteration immutable binding changes its driver owner")); };
                    let program = store.driver_programs.iter().find(|program| program.steps.bounds(store.driver_steps.len()).is_some_and(|range| range.contains(&(current as usize)))).ok_or_else(|| IrVerifyError::new("iteration immutable binding has no driver program"))?;
                    if current <= step || !program.steps.bounds(store.driver_steps.len()).is_some_and(|range| range.contains(&(step as usize))) { return Err(IrVerifyError::new("iteration immutable binding crosses original program order")); }
                    let slots = store.driver_steps[current as usize].slots.bounds(store.driver_slots.len()).ok_or_else(|| IrVerifyError::new("iteration immutable slot range is invalid"))?;
                    let lexical = store.driver_slots[slots].iter().find(|slot| producer.words.first() == Some(&slot.slot)).ok_or_else(|| IrVerifyError::new("iteration immutable binding loses its lexical slot"))?;
                    if lexical.flags & DRIVER_SLOT_MUTABLE != 0 || lexical.type_id != binding.input || store.string(lexical.name)? != name.as_str().as_str() { return Err(IrVerifyError::new("iteration immutable binding changes its original lexical mapping")); }
                    for previous in step as usize + 1..current as usize {
                        if matches!(store.driver_steps[previous].tag, FullDriverTag::Let | FullDriverTag::Assign) && store.payload(store.driver_steps[previous].data.range())?.first() == Some(&name.symbol().raw()) { return Err(IrVerifyError::new("iteration immutable read crosses another binding or assignment")); }
                    }
                    Self::verify_generic_source(store, generic, initializer, InstructionOwner::Driver(step), &store.semantic.to_type(binding.input)?, None, &mut Vec::new())?;
                }
            }
            match binding.iterator_parameter {
                Some((_, index)) if store.tags.get(authored_iterator as usize) == Some(&FullTag::ExprParam)
                    && store.payload(store.data[authored_iterator as usize].range())? == [index] => {},
                None if binding.iterator_carrier.is_some() && store.tags.get(authored_iterator as usize) == Some(&FullTag::ExprOk) => {},
                None if binding.iterator_carrier.is_none() && matches!((store.semantic.to_type(binding.input)?, store.tags.get(authored_iterator as usize)),
                    (Type::List(_), Some(FullTag::ExprList)) | (Type::Str, Some(FullTag::ExprStr)) | (Type::Bytes, Some(FullTag::ExprBytes))) => {},
                None if binding.producer.is_some() => {},
                _ => return Err(IrVerifyError::new("iteration iterator changes its original parameter source")),
            }
            if !binding.producer.as_ref().is_some_and(|producer| producer.initializer.is_some() || producer.lines.is_some()) { Self::verify_generic_source(store, generic, authored_iterator, binding.owner, &store.semantic.to_type(binding.input)?, None, &mut Vec::new())?; }
        }
        for use_ in generic.iteration_uses() {
            let binding = generic.iteration_binding(use_.binding)?;
            if generic.iteration_use(use_.instruction)?.is_none() || store.tags.get(use_.instruction as usize) != Some(&use_.tag)
                || store.payload(store.data[use_.instruction as usize].range())? != [binding.slot] {
                return Err(IrVerifyError::new("iteration item read changes its original slot"));
            }
            let mut visible = false;
            let mut ancestor = Some(use_.instruction);
            for _ in 0..=512 {
                let Some(instruction) = ancestor else { break; };
                if body_roots.get(&instruction) == Some(&use_.binding) { visible = true; break; }
                ancestor = tree.parent(instruction)?;
            }
            if !visible || tree.is_descendant(binding.iterator, use_.instruction)? { return Err(IrVerifyError::new("iteration item read is outside its original loop body")); }
        }
        Ok(())
    }

    pub(super) fn verify_iteration_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, expected: &Type) -> Result<bool, IrVerifyError> {
        let Some(use_) = generic.iteration_use(instruction)? else { return Ok(false); };
        let binding = generic.iteration_binding(use_.binding)?;
        if use_.owner != owner || binding.owner != owner || store.tags.get(instruction as usize) != Some(&use_.tag)
            || store.payload(store.data[instruction as usize].range())? != [binding.slot]
            || generic.registered_instruction_origin(instruction, false) != Some((OperationSourceOrigin::Expression(use_.origin), owner))
            || store.semantic.to_type(binding.binding_type)? != *expected { return Err(IrVerifyError::new(format!("iteration operand changes its original item contract: instruction {instruction} ({:?}), owner {owner:?}, original owner {:?}, original slot {}, checked type {:?}, requested type {expected:?}", store.tags.get(instruction as usize), binding.owner, binding.slot, store.semantic.to_type(binding.binding_type)?))); }
        Ok(true)
    }
}

#[cfg(test)]
mod tests;
