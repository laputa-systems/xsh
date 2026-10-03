use super::*;
use super::super::generic::{ContextProducerRoot, PreparedContextProducer};
use crate::sema::check::{ExpressionIdentity, ProducerFlowSource};
use crate::sema::inference::ScopedRoot;

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct BuildContextScopeOrigin {
    pub origin: ExpressionIdentity,
    pub input: BuildExprId,
    pub input_source: ExpressionIdentity,
    pub input_type: ScopedRoot,
    pub result_type: ScopedRoot,
    pub body: Vec<BuildStmtId>,
    pub tail: Option<(BuildExprId, ProducerFlowSource, ScopedRoot)>,
}

fn context_problem(reason: &'static str) -> IrBuildError { IrBuildError::format(reason, None, 0, 0) }

impl FullBuilder {
    fn context_ground_root(&mut self, solved: &crate::sema::check::SolvedTypes, root: ScopedRoot) -> Result<TypeId, IrBuildError> {
        solved.graph.validate_scoped(root).map_err(|_| context_problem("context_original_root_scope"))?;
        let reference = self.generic.get_or_insert_with(GenericEvidenceBuilder::default).prepare_closed_reference(&solved.graph, root.ty, &mut self.store.semantic, &mut self.semantic)
            .map_err(|_| context_problem("context_original_root_not_ground"))?;
        let TypeRef::Ground(ty) = reference else { return Err(context_problem("context_original_root_not_ground")); };
        Ok(ty)
    }

    pub(super) fn stage_original_context_scope(&mut self, expression: BuildExprId, instruction: u32, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        let Some(original) = scratch.context_scope_origins.get(&expression) else { return Ok(()); };
        let solved = self.solved.clone().ok_or_else(|| context_problem("context_original_graph_missing"))?;
        let raw = self.current_owner.ok_or_else(|| context_problem("context_original_owner_missing"))?;
        let owner = if let Some(index) = driver_owner_index(raw) { InstructionOwner::Driver(index as u32) } else { InstructionOwner::Function(IrFunctionId::from_raw(raw).ok_or_else(|| context_problem("context_original_owner_invalid"))?) };
        let caller = solved.expression_owners.get(&original.origin).copied();
        if solved.expressions.get(&original.origin) != Some(&original.result_type.ty) || solved.expression_scope(original.origin, caller).ok() != Some(original.result_type.scope)
            || self.active_expression_origins.get(&expression) != Some(&original.origin) { return Err(context_problem("context_original_result_changed")); }
        let BuildExprRow::ContextScope { input, body, .. } = &scratch.expressions[expression.index()] else { return Err(context_problem("context_original_row_changed")); };
        if *input != original.input || *body != original.body { return Err(context_problem("context_original_children_changed")); }
        let prepare_root = |builder: &mut Self, row: BuildExprId, source: ProducerFlowSource, root: ScopedRoot| -> Result<ContextProducerRoot, IrBuildError> {
            let origin = match source {
                ProducerFlowSource::Expression(origin) => {
                    if origin.source != original.origin.source || origin.namespace != original.origin.namespace
                        || solved.expressions.get(&origin) != Some(&root.ty) || solved.expression_scope(origin, caller).ok() != Some(root.scope)
                        || solved.expression_owners.get(&origin).copied() != caller || builder.active_expression_origins.get(&row) != Some(&origin) { return Err(context_problem("context_original_child_changed")); }
                    let flow = *solved.expression_producer_flows.get(&origin).ok_or_else(|| context_problem("context_original_child_producer_missing"))?;
                    if solved.producer_flows.node(flow).map_err(|_| context_problem("context_original_child_producer_owner"))?.source != source { return Err(context_problem("context_original_child_producer_changed")); }
                    super::super::generic::OperationSourceOrigin::Expression(origin)
                }
                ProducerFlowSource::Statement(origin) => {
                    let run = solved.run_operations.values().find(|run| run.parent == source).ok_or_else(|| context_problem("context_original_statement_producer_missing"))?;
                    if origin.source != original.origin.source || origin.namespace != original.origin.namespace
                        || solved.statements.get(&origin) != Some(&crate::sema::check::StatementPosition::Value)
                        || run.operation.caller != caller || run.operation.result != root.ty || solved.operation_scope(source, &run.operation).ok() != Some(root.scope)
                        || solved.producer_flows.node(run.producer_flow).map_err(|_| context_problem("context_original_statement_producer_owner"))?.source != source { return Err(context_problem("context_original_statement_producer_changed")); }
                    super::super::generic::OperationSourceOrigin::Statement(origin)
                }
                _ => return Err(context_problem("context_original_child_source_kind")),
            };
            let instruction = *builder.active_encoded_expressions.get(&row).ok_or_else(|| context_problem("context_original_child_not_encoded"))?;
            let original_type = super::super::generic::graph_ground_type(&solved.graph, root.ty).map_err(|_| context_problem("context_original_child_not_ground"))?;
            Ok(ContextProducerRoot { origin, instruction, ty: builder.context_ground_root(&solved, root)?, original_type })
        };
        let input = prepare_root(self, original.input, ProducerFlowSource::Expression(original.input_source), original.input_type)?;
        let tail = original.tail.map(|(row, origin, root)| prepare_root(self, row, origin, root)).transpose()?;
        let result = self.context_ground_root(&solved, original.result_type)?;
        let payload = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| context_problem("context_original_payload"))?.to_vec().into_boxed_slice();
        let body = payload.get(2).copied().and_then(IrBlockId::from_raw).ok_or_else(|| context_problem("context_original_body"))?;
        let block = self.store.blocks.get(body.index()).ok_or_else(|| context_problem("context_original_body"))?;
        let statements = self.store.payload(block.instructions).map_err(|_| context_problem("context_original_body_payload"))?.to_vec().into_boxed_slice();
        let original_result = super::super::generic::graph_ground_type(&solved.graph, original.result_type.ty).map_err(|_| context_problem("context_original_result_not_ground"))?;
        let receipt = PreparedContextProducer { origin: original.origin, instruction, owner, result, original_result, input, tail, payload, body, statements };
        FullVerifier::validate_context_encoding(&self.store, &receipt).map_err(|_| context_problem("context_original_encoding_changed"))?;
        self.generic_evidence_mut().add_context_producer(receipt).map_err(|_| context_problem("context_original_receipt_capacity"))
    }
}

impl FullVerifier {
    pub(super) fn validate_context_encoding(store: &FullStore, value: &PreparedContextProducer) -> Result<(), IrVerifyError> {
        if store.semantic.to_type(value.result)? != value.original_result || store.semantic.to_type(value.input.ty)? != value.input.original_type
            || value.tail.as_ref().map(|tail| store.semantic.to_type(tail.ty).map(|ty| ty != tail.original_type)).transpose()?.unwrap_or(false) {
            return Err(IrVerifyError::new("context producer changes its original checked types"));
        }
        if store.tags.get(value.instruction as usize) != Some(&FullTag::ExprContextScope)
            || store.payload(store.data[value.instruction as usize].range())? != value.payload.as_ref()
            || value.payload.get(1) != Some(&value.input.instruction) || value.payload.get(2) != Some(&value.body.raw()) { return Err(IrVerifyError::new("context producer changes its original input or body")); }
        let kind = value.payload.first().copied().ok_or_else(|| IrVerifyError::new("context producer kind is missing"))?;
        let input = store.semantic.to_type(value.input.ty)?;
        let valid_input = match (kind, &input) {
            (0, Type::Path | Type::Str) => true,
            (1, Type::Record(fields)) => fields.values().all(Type::can_be_argv_item),
            (1, Type::Map(key, item)) if **key == Type::Str => item.can_be_argv_item(),
            _ => false,
        };
        if !valid_input { return Err(IrVerifyError::new("context producer changes its original input contract")); }
        let block = store.blocks.get(value.body.index()).ok_or_else(|| IrVerifyError::new("context producer body is missing"))?;
        if block.flags != BLOCK_STATEMENTS || store.payload(block.instructions)? != value.statements.as_ref()
            || value.statements.first().copied().map(|count| count as usize) != Some(value.statements.len().saturating_sub(1)) { return Err(IrVerifyError::new("context producer changes its original statement sequence")); }
        if let Some(tail) = &value.tail {
            let statement = *value.statements.last().filter(|_| value.statements.len() > 1).ok_or_else(|| IrVerifyError::new("context producer loses its original completion"))?;
            if store.tags.get(statement as usize) != Some(&FullTag::StmtValue)
                || store.payload(store.data[statement as usize].range())? != [tail.instruction] { return Err(IrVerifyError::new("context producer changes its original completing expression")); }
        }
        Ok(())
    }

    pub(super) fn verify_original_context_scopes(store: &FullStore, tree: &super::super::pattern::PatternTree) -> Result<(), IrVerifyError> {
        let contexts = store.tags.iter().enumerate().filter(|(_, tag)| **tag == FullTag::ExprContextScope).map(|(index, _)| index as u32).collect::<Vec<_>>();
        let Some(generic) = store.generic.as_deref() else {
            if contexts.is_empty() { return Ok(()); }
            return Err(IrVerifyError::new("context producers have no original authority"));
        };
        for instruction in contexts {
            generic.context_producer_at(instruction)?.ok_or_else(|| IrVerifyError::new("context producer loses its original receipt"))?;
        }
        for value in generic.run_producers() {
            let value = generic.run_producer_at(value.capture)?.ok_or_else(|| IrVerifyError::new("run producer original receipt is missing"))?;
            Self::validate_run_producer_encoding(store, value)?;
            if !tree.is_descendant(value.continuation, value.capture)? { return Err(IrVerifyError::new("run capture is outside its original continuation")); }
            for operand in &value.operands { if !tree.is_descendant(value.capture, operand.instruction)? { return Err(IrVerifyError::new("run operand is outside its original capture")); } }
            if let Some(accept) = &value.accept {
                Self::verify_generic_source(store, generic, accept.instruction, value.owner, &accept.original_type, None, &mut Vec::new())?;
            }
        }
        for value in generic.context_producers() {
            let value = generic.context_producer_at(value.instruction)?.ok_or_else(|| IrVerifyError::new("context producer original receipt is missing"))?;
            Self::validate_context_encoding(store, value)?;
            if !tree.is_descendant(value.instruction, value.input.instruction)? { return Err(IrVerifyError::new("context input is outside its original producer")); }
            for &statement in &value.statements[1..] { if !tree.is_descendant(value.instruction, statement)? { return Err(IrVerifyError::new("context statement is outside its original body")); } }
            Self::verify_generic_source(store, generic, value.input.instruction, value.owner, &store.semantic.to_type(value.input.ty)?, None, &mut Vec::new())?;
            if let Some(tail) = &value.tail {
                if !tree.is_descendant(value.instruction, tail.instruction)? || tree.is_descendant(value.input.instruction, tail.instruction)? { return Err(IrVerifyError::new("context completion is outside its original body")); }
                Self::verify_generic_source(store, generic, tail.instruction, value.owner, &store.semantic.to_type(tail.ty)?, None, &mut Vec::new())?;
            }
        }
        Ok(())
    }

    pub(super) fn verify_context_scope_source(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, expected: &Type) -> Result<bool, IrVerifyError> {
        let Some(value) = generic.context_producer_at(instruction)? else { return Ok(false); };
        if value.owner != owner || store.semantic.to_type(value.result)? != *expected { return Err(IrVerifyError::new("context producer changes its checked result or owner")); }
        Self::validate_context_encoding(store, value)?;
        Ok(true)
    }
}

impl<'a> FullExecution<'a> {
    pub(in crate::runtime::eval) fn context_producer(&self, instruction: u32, kind: crate::syntax::arena::ContextScopeKind, input: u32, body: u32) -> Result<&PreparedContextProducer, IrVerifyError> {
        if !self.decoder.instruction_range.contains(&(instruction as usize)) { return Err(IrVerifyError::new("context producer belongs to another body")); }
        let generic = self.generic_evidence().ok_or_else(|| IrVerifyError::new("context producer has no original authority"))?;
        let value = generic.context_producer_at(instruction)?.ok_or_else(|| IrVerifyError::new("context producer has no original receipt"))?;
        let owner = if let Some(driver) = driver_owner_index(self.decoder.owner) { InstructionOwner::Driver(driver as u32) } else {
            InstructionOwner::Function(IrFunctionId::from_raw(self.decoder.owner).ok_or_else(|| IrVerifyError::new("context producer owner is invalid"))?)
        };
        let kind = match kind { crate::syntax::arena::ContextScopeKind::Cwd => 0, crate::syntax::arena::ContextScopeKind::Env => 1 };
        if value.owner != owner || value.input.instruction != input || value.body.raw() != body || value.payload.first() != Some(&kind) {
            return Err(IrVerifyError::new("context producer changes its original owner, input, body or protocol"));
        }
        FullVerifier::validate_context_encoding(self.decoder.store, value)?;
        Ok(value)
    }
}

#[cfg(test)]
mod tests;

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct BuildRunProducerOrigin {
    pub source: ProducerFlowSource,
    pub run: crate::syntax::arena::RunFormId,
    pub capture: BuildExprId,
    pub continuation: BuildExprId,
    pub spawn: Option<BuildSpawnRunOrigin>,
    pub packet: Option<BuildRunPacketSource>,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct BuildRunPacketSource {
    pub environment: Vec<BuildRunEnvironmentSource>,
    pub stdin: Vec<BuildRunStdinSource>,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct BuildRunEnvironmentSource {
    pub name: Name,
    pub span: Span,
    pub argument_span: Span,
    pub text: Arc<str>,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct BuildRunStdinSource {
    pub kind: crate::syntax::node::RedirectionKind,
    pub span: Span,
    pub argument_span: Span,
    pub mode: u32,
    pub origin: ExpressionIdentity,
    pub source_type: ScopedRoot,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct BuildSpawnRunOrigin {
    pub origin: ExpressionIdentity,
    pub source_type: ScopedRoot,
    pub target: crate::sema::check::RunIdentity,
}

impl FullBuilder {
    pub(super) fn stage_original_run_producer(&mut self, expression: BuildExprId, instruction: u32, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        use super::super::generic::{PreparedRunProducer, RunProducerOperand, PreparedOperationAuthority};
        use crate::sema::operation_graph::PreparedLanguageOperation;
        let Some(original) = scratch.run_producer_origins.get(&expression) else { return Ok(()); };
        if original.spawn.is_some() { return self.stage_original_spawn_run(original, expression, instruction, scratch); }
        let solved = self.solved.clone().ok_or_else(|| context_problem("run_original_graph_missing"))?;
        let (source, namespace) = match original.source {
            ProducerFlowSource::Expression(origin) => (origin.source, origin.namespace),
            ProducerFlowSource::Statement(origin) => (origin.source, origin.namespace),
            _ => return Err(context_problem("run_original_parent_kind")),
        };
        let (_, run) = solved.run_operations.iter().find(|(identity, _)| identity.run == original.run && identity.source == source && identity.namespace == namespace)
            .ok_or_else(|| context_problem("run_original_operation_missing"))?;
        if run.arguments.iter().any(|argument| !matches!(argument.source, crate::sema::check::RunArgumentSource::Expression(_)) || !matches!(argument.mode, crate::sema::check::RunArgumentMode::Single | crate::sema::check::RunArgumentMode::Expansion | crate::sema::check::RunArgumentMode::Splice))
            || !matches!(run.kind, RunKind::CaptureText | RunKind::CaptureBytes | RunKind::CaptureTextRecord | RunKind::CaptureBytesRecord) { return Ok(()); }
        let BuildExprRow::RunCapture(capture_row) = &scratch.expressions[original.capture.index()] else { return Err(context_problem("run_original_capture_changed")); };
        if capture_row.timeout.is_some() || capture_row.cpu_max.is_some() || (original.packet.is_none() && (!capture_row.env.is_empty() || !capture_row.redirections.is_empty())) { return Ok(()); }
        if let Some(accept) = capture_row.accept {
            let BuildExprRow::List(items) = &scratch.expressions[accept.index()] else { return Ok(()); };
            if items.is_empty() || !items.iter().all(|item| matches!(scratch.expressions[item.index()], BuildExprRow::Int(value) if (0..=255).contains(&value))) { return Ok(()); }
        }
        if original.continuation != expression || run.parent != original.source || capture_row.kind != run.kind || capture_row.propagate
            || capture_row.accept.is_some() != run.policy {
            return Err(context_problem("run_original_protocol_changed"));
        }
        if run.propagate {
            if !matches!(&scratch.expressions[expression.index()], BuildExprRow::Try(child) if *child == original.capture) { return Err(context_problem("run_original_propagation_changed")); }
        } else if expression != original.capture { return Err(context_problem("run_original_continuation_changed")); }
        let raw = self.current_owner.ok_or_else(|| context_problem("run_original_owner_missing"))?;
        let owner = if let Some(index) = driver_owner_index(raw) { InstructionOwner::Driver(index as u32) } else { InstructionOwner::Function(IrFunctionId::from_raw(raw).ok_or_else(|| context_problem("run_original_owner_invalid"))?) };
        let operation = &run.operation;
        if operation.receiver.is_some() || !operation.actual_arguments.is_empty() || !operation.binding.supplied_slots.is_empty() || !operation.binding.default_slots.is_empty()
            || operation.binding.rest_slot.is_some() || operation.binding.dynamic.is_some() || !operation.argument_coercions.is_empty()
            || solved.producer_flows.node(run.producer_flow).map_err(|_| context_problem("run_original_producer_owner"))?.source != original.source { return Err(context_problem("run_original_operation_changed")); }
        let scope = solved.operation_scope(run.parent, operation).map_err(|_| context_problem("run_original_operation_scope"))?;
        solved.graph.validate_requirement_scoped(crate::sema::inference::ScopedRequirementRoot { requirement: operation.requirement, scope }).map_err(|_| context_problem("run_original_requirement_scope"))?;
        let selected = solved.graph.candidate_evidence(operation.requirement).map_err(|_| context_problem("run_original_selection"))?.ok_or_else(|| context_problem("run_original_selection_missing"))?;
        let crate::sema::check::SolvedOperationAuthority::Language(metadata) = solved.operation_catalog.candidate(&solved.graph, selected.candidate).map_err(|_| context_problem("run_original_authority"))? else { return Err(context_problem("run_original_authority_kind")); };
        if metadata.operation != (PreparedLanguageOperation::Run { kind: run.kind, policy: run.policy, propagate: run.propagate })
            || solved.graph.resolved(selected.result).ok() != solved.graph.resolved(operation.result).ok() { return Err(context_problem("run_original_authority_changed")); }
        let authority = PreparedOperationAuthority::Language { identity: metadata.identity, authority: metadata.authority, operation: metadata.operation, argument_order: metadata.argument_order, statement_result_is_unit: metadata.statement_result_is_unit };
        let original_result = super::super::generic::graph_ground_type(&solved.graph, operation.result).map_err(|_| context_problem("run_original_result_not_ground"))?;
        let original_carrier = if run.propagate { Type::Result(Box::new(original_result.clone()), Box::new(Type::ProcessError)) } else { original_result.clone() };
        let result = self.context_ground_root(&solved, ScopedRoot { ty: operation.result, scope })?;
        let carrier = if run.propagate { self.intern_generic_ground_type(&original_carrier)? } else { result };
        let capture = *self.active_encoded_expressions.get(&original.capture).ok_or_else(|| context_problem("run_original_capture_not_encoded"))?;
        let payload = self.store.payload(self.store.data[capture as usize].range()).map_err(|_| context_problem("run_original_payload"))?.to_vec().into_boxed_slice();
        let continuation_payload = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| context_problem("run_original_continuation_payload"))?.to_vec().into_boxed_slice();
        let (mut operands, mut blocks, mut texts, arguments) = self.prepare_checked_run_operands(capture_row.target.as_ref(), &capture_row.args, &run.arguments, owner, operation.caller, scratch, &solved)?;
        for offset in [4, 5, 6] {
            let id = payload.get(offset).copied().and_then(IrBlockId::from_raw).ok_or_else(|| context_problem("run_original_argument_block"))?;
            let words = self.store.payload(self.store.blocks[id.index()].instructions).map_err(|_| context_problem("run_original_argument_payload"))?.to_vec().into_boxed_slice();
            blocks.push((id, words));
        }
        let accept = self.prepare_literal_run_acceptance(capture_row.accept, owner, operation.caller, scratch, &solved, &mut operands, &mut blocks)?;
        let packet = self.prepare_original_run_packet(original.packet.as_ref(), &capture_row.env, &capture_row.redirections, owner, operation.caller, scratch, &solved, &mut operands, &mut blocks, &mut texts)?;
        let crate::sema::inference::EffectSummary::Closed(effects) = operation.effects else { return Err(context_problem("run_original_effects_not_closed")); };
        let receipt = PreparedRunProducer { source: original.source, run: original.run, capture, continuation: instruction, owner, result, carrier, original_result, original_carrier, authority, effects, accept, spawn: None, payload, continuation_payload, operands, blocks, texts, arguments, packet };
        FullVerifier::validate_run_producer_encoding(&self.store, &receipt).map_err(|error| IrBuildError::verification("run_original_encoding_changed", error))?;
        let source = match original.source {
            ProducerFlowSource::Expression(origin) => super::super::generic::OperationSourceOrigin::Expression(origin),
            ProducerFlowSource::Statement(origin) => super::super::generic::OperationSourceOrigin::Statement(origin),
            _ => unreachable!(),
        };
        self.generic_evidence_mut().register_instruction_origin(instruction, source, owner).map_err(|_| context_problem("run_original_parent_registration"))?;
        self.generic_evidence_mut().add_run_producer(receipt).map_err(|_| context_problem("run_original_receipt_capacity"))
    }
}

impl FullVerifier {
    pub(super) fn validate_run_producer_encoding(store: &FullStore, value: &super::super::generic::PreparedRunProducer) -> Result<(), IrVerifyError> {
        if store.semantic.to_type(value.result)? != value.original_result || store.semantic.to_type(value.carrier)? != value.original_carrier {
            return Err(IrVerifyError::new("run producer changes its original checked result or carrier"));
        }
        if value.spawn.is_some() { return Self::validate_spawn_run_encoding(store, value); }
        if store.tags.get(value.capture as usize) != Some(&FullTag::ExprRunCapture) || store.payload(store.data[value.capture as usize].range())? != value.payload.as_ref()
            || store.payload(store.data[value.continuation as usize].range())? != value.continuation_payload.as_ref() { return Err(IrVerifyError::new("run producer changes its original execution or continuation")); }
        let super::super::generic::PreparedOperationAuthority::Language { operation: crate::sema::operation_graph::PreparedLanguageOperation::Run { kind, policy, propagate }, .. } = value.authority else { return Err(IrVerifyError::new("run producer has an unprepared operation authority")); };
        let completion = if policy { 11 } else { 10 };
        if value.payload.len() != completion + 3 || store.run_kinds.get(value.payload[0] as usize) != Some(&kind) || value.payload[1] != 0
            || value.payload[7..9] != [0, 0] || value.payload[9] != u32::from(policy) || value.payload[completion] != 0 || value.payload[completion + 1] > 1 {
            return Err(IrVerifyError::new("run producer changes its checked capture protocol"));
        }
        Self::validate_run_acceptance_encoding(store, value)?;
        Self::validate_run_argument_encoding(store, value)?;
        Self::validate_run_packet_encoding(store, value)?;
        if propagate {
            if value.capture == value.continuation || store.tags.get(value.continuation as usize) != Some(&FullTag::ExprTry) || value.continuation_payload.as_ref() != [value.capture] { return Err(IrVerifyError::new("run producer changes its original propagation wrapper")); }
        } else if value.capture != value.continuation { return Err(IrVerifyError::new("run producer introduces an unchecked continuation")); }
        for (id, original) in &value.blocks {
            let block = store.blocks.get(id.index()).ok_or_else(|| IrVerifyError::new("run producer argument block is missing"))?;
            if block.flags != BLOCK_LIST || store.payload(block.instructions)? != original.as_ref() { return Err(IrVerifyError::new("run producer changes its original argument sequence")); }
        }
        for operand in &value.operands {
            if store.tags.get(operand.instruction as usize) != Some(&operand.tag) || store.payload(store.data[operand.instruction as usize].range())? != operand.payload.as_ref() { return Err(IrVerifyError::new("run producer changes its original literal operand")); }
        }
        for (id, text) in &value.texts { if store.string(*id)? != text.as_ref() { return Err(IrVerifyError::new("run producer changes its original literal text")); } }
        Ok(())
    }

    pub(super) fn verify_run_producer_source(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, expected: &Type) -> Result<bool, IrVerifyError> {
        let Some(value) = generic.run_producer_at(instruction)? else { return Ok(false); };
        let actual = if instruction == value.continuation { value.result } else { value.carrier };
        if value.owner != owner || store.semantic.to_type(actual)? != *expected { return Err(IrVerifyError::new("run producer changes its original result or owner")); }
        Self::validate_run_producer_encoding(store, value)?;
        Ok(true)
    }

    pub(super) fn context_scope_carrier(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner) -> Result<Option<Type>, IrVerifyError> {
        let Some(value) = generic.context_producer_at(instruction)? else { return Ok(None); };
        if value.owner != owner { return Err(IrVerifyError::new("context carrier changes its original owner")); }
        Self::validate_context_encoding(store, value)?;
        Ok(Some(store.semantic.to_type(value.result)?))
    }

    pub(super) fn run_producer_carrier(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner) -> Result<Option<Type>, IrVerifyError> {
        let Some(value) = generic.run_producer_at(instruction)? else { return Ok(None); };
        if value.owner != owner || value.capture != instruction { return Err(IrVerifyError::new("run carrier changes its original capture or owner")); }
        Self::validate_run_producer_encoding(store, value)?;
        Ok(Some(store.semantic.to_type(value.carrier)?))
    }
}

impl<'a> FullExecution<'a> {
    pub(in crate::runtime::eval) fn run_producer(&self, instruction: u32) -> Result<Option<&super::super::generic::PreparedRunProducer>, IrVerifyError> {
        if !self.decoder.instruction_range.contains(&(instruction as usize)) { return Err(IrVerifyError::new("run producer belongs to another body")); }
        let Some(generic) = self.generic_evidence() else { return Ok(None); };
        let Some(value) = generic.run_producer_at(instruction)? else { return Ok(None); };
        let owner = if let Some(driver) = driver_owner_index(self.decoder.owner) { InstructionOwner::Driver(driver as u32) } else {
            InstructionOwner::Function(IrFunctionId::from_raw(self.decoder.owner).ok_or_else(|| IrVerifyError::new("run producer owner is invalid"))?)
        };
        if value.owner != owner || value.capture != instruction { return Err(IrVerifyError::new("run producer changes its original capture or owner")); }
        FullVerifier::validate_run_producer_encoding(self.decoder.store, value)?;
        Ok(Some(value))
    }
}

#[path = "run_prepare.rs"]
mod run_prepare;
