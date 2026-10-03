use super::*;
use super::super::generic::{graph_ground_type, TryCaptureSource};
use crate::sema::check::ExpressionIdentity;
use crate::sema::inference::ScopedRoot;

mod error_capture;
pub(in crate::runtime::eval) use error_capture::PreparedCaptureErrorRelation;

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct BuildTryCaptureOrigin {
    pub origin: ExpressionIdentity,
    pub block: crate::syntax::arena::BlockId,
    pub source_type: ScopedRoot,
    pub body: Box<[BuildStmtId]>,
    pub completion: Option<(ExpressionIdentity, ScopedRoot)>,
    pub propagation: Option<(ExpressionIdentity, ScopedRoot)>,
    pub propagation_row: Option<BuildExprId>,
    pub error_capture: Option<Arc<crate::sema::check::SolvedErrorCapture>>,
    pub retry: Option<BuildRetryCapturePolicy>,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct BuildRetryCapturePolicy {
    pub delays: Vec<(BuildExprId, ExpressionIdentity, ScopedRoot)>,
    pub selection: Option<(BuildPatternId, crate::sema::check::PatternIdentity, ScopedRoot)>,
}

fn problem(construct: &'static str) -> IrBuildError { IrBuildError::format(construct, None, 0, 0) }

fn capture_owner(raw: Option<u32>) -> Result<InstructionOwner, IrBuildError> {
    let raw = raw.ok_or_else(|| problem("try_capture_owner_missing"))?;
    if let Some(index) = driver_owner_index(raw) { return Ok(InstructionOwner::Driver(index as u32)); }
    IrFunctionId::from_raw(raw).map(InstructionOwner::Function).ok_or_else(|| problem("try_capture_owner_invalid"))
}

impl FullBuilder {
    pub(super) fn stage_try_capture(&mut self, expression: BuildExprId, instruction: u32, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        let Some(original) = scratch.try_capture_origins.get(&expression) else { return Ok(()); };
        let Some((completion_origin, completion_type)) = original.completion else { return Ok(()); };
        if original.retry.is_none() && original.body.len() != 1 { return Ok(()); }
        let Some(statement) = original.body.last() else { return Ok(()); };
        let Some(BuildStmtRow::Value { value }) = scratch.statements.get(statement.index()) else { return Ok(()); };
        let Some(solved) = self.solved.clone() else { return Err(problem("try_capture_checked_owner_missing")); };
        let owner = capture_owner(self.current_owner)?;
        let declaration = solved.expression_owners.get(&original.origin).copied();
        for (origin, root) in [(original.origin, original.source_type), (completion_origin, completion_type)].into_iter().chain(original.propagation) {
            if solved.expressions.get(&origin) != Some(&root.ty)
                || solved.expression_owners.get(&origin).copied() != declaration
                || root.scope != solved.expression_scope(origin, declaration).map_err(|_| problem("try_capture_source_scope"))? {
                return Err(problem("try_capture_original_source_changed"));
            }
            solved.graph.validate_scoped(root).map_err(|_| problem("try_capture_source_scope"))?;
        }
        if self.active_expression_origins.get(&expression) != Some(&original.origin)
            || self.active_expression_origins.get(value) != Some(&completion_origin) {
            return Err(problem("try_capture_original_completion_changed"));
        }
        if let Some(declaration) = declaration {
            if self.declaration_functions.get(&declaration).copied().map(InstructionOwner::Function) != Some(owner) {
                return Err(problem("try_capture_declaration_owner_changed"));
            }
        } else if !matches!(owner, InstructionOwner::Driver(_)) { return Err(problem("try_capture_declaration_owner_missing")); }
        let Ok(carrier_type) = graph_ground_type(&solved.graph, original.source_type.ty) else { return Ok(()); };
        let Ok(completion_type_tree) = graph_ground_type(&solved.graph, completion_type.ty) else { return Ok(()); };
        let Type::Result(success, error) = &carrier_type else { return Err(problem("try_capture_source_is_not_result")); };
        let completion_is_result = original.retry.is_some() && matches!(completion_type_tree, Type::Result(_, _));
        if if completion_is_result { carrier_type != completion_type_tree } else { **success != completion_type_tree } { return Err(problem("try_capture_original_success_relation")); }
        let propagation_type = original.propagation.map(|(_, root)| graph_ground_type(&solved.graph, root.ty)
            .map_err(|_| problem("try_capture_propagation_requires_ground"))).transpose()?;
        let error_capture = self.prepare_capture_error_relation(original, owner, &solved, &carrier_type)?;
        match &propagation_type {
            Some(Type::Result(ok, failure)) if **ok == completion_type_tree && (failure == error || original.retry.is_some() && failure.matches_expected(error)) => {},
            None if error_capture.is_some() || completion_is_result || **error == Type::Error => {},
            _ => return Err(problem("try_capture_original_error_relation")),
        }
        let body = match (scratch.expressions.get(expression.index()), &original.retry) {
            (Some(BuildExprRow::Capture { body, .. }), None) => body,
            (Some(BuildExprRow::Retry { body, .. }), Some(_)) => body,
            _ => return Err(problem("capture_original_instruction_changed")),
        };
        let tag = if original.retry.is_some() { FullTag::ExprRetry } else { FullTag::ExprCapture };
        if body.as_slice() != original.body.as_ref() || self.store.tags.get(instruction as usize) != Some(&tag) {
            return Err(problem("try_capture_original_body_changed"));
        }
        let tail = *self.active_encoded_expressions.get(value).ok_or_else(|| problem("try_capture_completion_instruction_missing"))?;
        let payload = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| problem("try_capture_instruction_payload"))?.to_vec().into_boxed_slice();
        let body_offset = original.retry.as_ref().map_or(0, |retry| if retry.selection.is_some() { 3 } else { 2 });
        let body = *payload.get(body_offset).ok_or_else(|| problem("try_capture_body_missing"))?;
        let block = IrBlockId::from_raw(body).and_then(|id| self.store.blocks.get(id.index())).ok_or_else(|| problem("try_capture_body_invalid"))?;
        let words = self.store.payload(block.instructions).map_err(|_| problem("try_capture_body_payload"))?;
        if words.first().copied().map(|count| count as usize) != Some(original.body.len()) || words.len() != original.body.len() + 1 { return Err(problem("capture_original_body_sequence_changed")); }
        let body_words = words.to_vec().into_boxed_slice();
        let statement = *words.last().ok_or_else(|| problem("capture_original_completion_missing"))?;
        if block.flags != BLOCK_STATEMENTS || self.store.tags.get(statement as usize) != Some(&FullTag::StmtValue)
            || self.store.payload(self.store.data[statement as usize].range()).map_err(|_| problem("try_capture_completion_payload"))? != [tail] {
            return Err(problem("try_capture_original_completion_changed"));
        }
        let (producer, producer_source) = if let Some((origin, _)) = original.propagation {
            let Some(BuildExprRow::Try(child)) = scratch.expressions.get(value.index()) else { return Err(problem("try_capture_propagation_instruction_changed")); };
            let producer = *self.active_encoded_expressions.get(child).ok_or_else(|| problem("try_capture_producer_instruction_missing"))?;
            let row = original.propagation_row.ok_or_else(|| problem("try_capture_producer_source_missing"))?;
            if self.active_expression_origins.get(&row) != Some(&origin) { return Err(problem("try_capture_producer_source_changed")); }
            let source = *self.active_encoded_expressions.get(&row).ok_or_else(|| problem("try_capture_producer_source_missing"))?;
            (Some(producer), Some(source))
        } else {
            if original.propagation_row.is_some() { return Err(problem("try_capture_unexpected_producer_source")); }
            (None, None)
        };
        let carrier = self.intern_generic_ground_type(&carrier_type)?;
        let completion = self.intern_generic_ground_type(&completion_type_tree)?;
        let propagation = propagation_type.as_ref().map(|ty| self.intern_generic_ground_type(ty)).transpose()?;
        let tail_code = (self.store.tags[tail as usize], self.store.payload(self.store.data[tail as usize].range())
            .map_err(|_| problem("try_capture_completion_payload"))?.to_vec().into_boxed_slice());
        let producer_code = producer_source.map(|instruction| -> Result<_, IrBuildError> {
            let payload = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| problem("try_capture_producer_payload"))?;
            Ok((self.store.tags[instruction as usize], payload.to_vec().into_boxed_slice()))
        }).transpose()?;
        let retry = self.prepare_retry_capture_policy(original, expression, &payload, completion_is_result, owner, scratch, &solved)?;
        self.generic_evidence_mut().add_try_capture_source(TryCaptureSource {
            origin: original.origin, block: original.block, source_type: original.source_type, completion_origin, completion_type,
            propagation_origin: original.propagation, owner, instruction, carrier, completion, propagation,
            body, body_words, statement, tail, tail_code, producer, producer_source, producer_code, payload,
            original_carrier: carrier_type, original_completion: completion_type_tree, original_propagation: propagation_type, retry, error_capture,
        }).map_err(|_| problem("try_capture_source_allocation"))?;
        Ok(())
    }
}

impl FullVerifier {
    fn verify_try_capture_source(store: &FullStore, generic: &GenericEvidenceStore, source: &TryCaptureSource) -> Result<(), IrVerifyError> {
        let tag = if source.retry.is_some() { FullTag::ExprRetry } else { FullTag::ExprCapture };
        let body_offset = source.retry.as_ref().map_or(0, |retry| if retry.selection.is_some() { 3 } else { 2 });
        if store.tags.get(source.instruction as usize) != Some(&tag)
            || store.payload(store.data[source.instruction as usize].range())? != source.payload.as_ref()
            || source.payload.get(body_offset) != Some(&source.body) {
            return Err(IrVerifyError::new("try capture changes its original instruction or body"));
        }
        let body = IrBlockId::from_raw(source.body).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("try capture body is invalid"))?;
        if body.flags != BLOCK_STATEMENTS || store.payload(body.instructions)? != source.body_words.as_ref()
            || source.body_words.last() != Some(&source.statement)
            || store.tags.get(source.statement as usize) != Some(&FullTag::StmtValue)
            || store.payload(store.data[source.statement as usize].range())? != [source.tail]
            || store.tags.get(source.tail as usize) != Some(&source.tail_code.0)
            || store.payload(store.data[source.tail as usize].range())? != source.tail_code.1.as_ref() {
            return Err(IrVerifyError::new("try capture changes its original completion body"));
        }
        Self::verify_retry_capture_policy(store, generic, source)?;
        if let Some(relation) = &source.error_capture { relation.verify(source)?; }
        let mut active = vec![source.instruction];
        if let (Some(producer), Some(material), Some((origin, _)), Some(propagation)) = (source.producer, source.producer_source, source.propagation_origin, source.propagation) {
            if store.tags.get(source.tail as usize) != Some(&FullTag::ExprTry) || store.payload(store.data[source.tail as usize].range())? != [producer] {
                return Err(IrVerifyError::new("try capture changes its original propagation"));
            }
            let mut current = producer;
            let mut wrappers = Vec::new();
            loop {
                let body = if let Some((body, call)) = Self::original_argument_wrapper_body(store, generic, current, source.owner)? {
                    if call != origin { return Err(IrVerifyError::new("try capture producer wrapper changes its original call")); }
                    Some(body)
                } else { Self::original_compiler_argument_wrapper_body(store, generic, current, source.owner)? };
                let Some(body) = body else { break; };
                if wrappers.len() >= 256 || wrappers.contains(&current) {
                    return Err(IrVerifyError::new("try capture producer wrappers change their original call"));
                }
                wrappers.push(current); current = body;
            }
            if current != material { return Err(IrVerifyError::new("try capture producer changes its original source")); }
            let Some((tag, payload)) = &source.producer_code else { return Err(IrVerifyError::new("try capture original producer code is missing")); };
            if store.tags.get(material as usize) != Some(tag) || store.payload(store.data[material as usize].range())? != payload.as_ref() {
                return Err(IrVerifyError::new("try capture changes its original producer code"));
            }
            Self::verify_generic_source(store, generic, producer, source.owner, &store.semantic.to_type(propagation)?, None, &mut active)?;
        }
        Self::verify_generic_source(store, generic, source.tail, source.owner, &store.semantic.to_type(source.completion)?, None, &mut active)
    }
    pub(super) fn verify_try_capture_sources(store: &FullStore, generic: &GenericEvidenceStore) -> Result<(), IrVerifyError> {
        for (id, _) in generic.try_capture_sources() { Self::verify_try_capture_source(store, generic, generic.try_capture_source(id)?)?; }
        Ok(())
    }
    pub(super) fn verify_try_capture_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, expected: &Type) -> Result<bool, IrVerifyError> {
        let Some(id) = generic.try_capture_source_at(instruction)? else { return Ok(false); };
        let source = generic.try_capture_source(id)?;
        if source.owner != owner || store.semantic.to_type(source.carrier)? != *expected {
            return Err(IrVerifyError::new("try capture operand changes its original carrier or owner"));
        }
        Self::verify_try_capture_source(store, generic, source)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests;

impl FullBuilder {
    fn prepare_retry_capture_policy(&mut self, original: &BuildTryCaptureOrigin, expression: BuildExprId, payload: &[u32], completion_is_result: bool, owner: InstructionOwner, scratch: &BuildScratch, solved: &crate::sema::check::SolvedTypes) -> Result<Option<super::super::generic::RetryCapturePolicy>, IrBuildError> {
        use super::super::generic::{RetryCaptureDelay, RetryCapturePolicy};
        let Some(policy) = &original.retry else { return Ok(None); };
        let BuildExprRow::Retry { delays, pattern, .. } = &scratch.expressions[expression.index()] else { return Err(problem("retry_original_row_changed")); };
        if delays.iter().copied().ne(policy.delays.iter().map(|delay| delay.0)) || *pattern != policy.selection.map(|selection| selection.0) {
            return Err(problem("retry_original_policy_changed"));
        }
        let caller = solved.expression_owners.get(&original.origin).copied();
        let mut prepared = Vec::new();
        for &(row, origin, root) in &policy.delays {
            if origin.source != original.origin.source || origin.namespace != original.origin.namespace
                || self.active_expression_origins.get(&row) != Some(&origin) || solved.expressions.get(&origin) != Some(&root.ty)
                || solved.expression_owners.get(&origin).copied() != caller || solved.expression_scope(origin, caller).ok() != Some(root.scope) {
                return Err(problem("retry_original_delay_source_changed"));
            }
            solved.graph.validate_scoped(root).map_err(|_| problem("retry_original_delay_scope"))?;
            let original_type = graph_ground_type(&solved.graph, root.ty).map_err(|_| problem("retry_original_delay_not_ground"))?;
            if original_type != Type::Duration { return Err(problem("retry_original_delay_not_duration")); }
            let instruction = *self.active_encoded_expressions.get(&row).ok_or_else(|| problem("retry_original_delay_not_encoded"))?;
            let ty = self.intern_generic_ground_type(&original_type)?;
            let code = (self.store.tags[instruction as usize], self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| problem("retry_original_delay_payload"))?.to_vec().into_boxed_slice());
            self.generic_evidence_mut().register_instruction_origin(instruction, super::super::generic::OperationSourceOrigin::Expression(origin), owner).map_err(|_| problem("retry_original_delay_registration"))?;
            prepared.push(RetryCaptureDelay { origin, source_type: root, instruction, ty, original_type, code });
        }
        let delays_block = *payload.first().ok_or_else(|| problem("retry_original_delays_missing"))?;
        let delay_block = IrBlockId::from_raw(delays_block).and_then(|id| self.store.blocks.get(id.index())).ok_or_else(|| problem("retry_original_delays_block"))?;
        let delays_words = self.store.payload(delay_block.instructions).map_err(|_| problem("retry_original_delays_payload"))?.to_vec().into_boxed_slice();
        let selection = policy.selection.map(|(row, origin, root)| -> Result<_, IrBuildError> {
            if scratch.pattern_origins.get(&row) != Some(&origin) { return Err(problem("retry_original_selection_source_changed")); }
            let checked = solved.checked_pattern(origin).map_err(|_| problem("retry_original_selection_missing"))?;
            if origin.source != original.origin.source || origin.namespace != original.origin.namespace || checked.caller != caller
                || checked.input != root.ty || solved.checked_pattern_scope(origin).ok() != Some(root.scope) || !checked.captures.is_empty() {
                return Err(problem("retry_original_selection_contract_changed"));
            }
            solved.graph.validate_scoped(root).map_err(|_| problem("retry_original_selection_scope"))?;
            let selected_input = graph_ground_type(&solved.graph, root.ty).map_err(|_| problem("retry_original_selection_not_ground"))?;
            let Type::Result(_, error) = graph_ground_type(&solved.graph, original.source_type.ty).map_err(|_| problem("retry_original_result_not_ground"))? else { return Err(problem("retry_original_result_not_result")); };
            if selected_input != *error { return Err(problem("retry_original_selection_error_changed")); }
            let encoded = *payload.get(2).ok_or_else(|| problem("retry_original_selection_not_encoded"))?;
            if !self.generic_pattern_rows.iter().any(|&(pattern, source, source_owner)| pattern == encoded && source == origin && source_owner == owner) {
                return Err(problem("retry_original_selection_owner_changed"));
            }
            Ok((origin, root, encoded))
        }).transpose()?;
        Ok(Some(RetryCapturePolicy { delays: prepared, delays_block, delays_words, selection, completion_is_result }))
    }
}

impl FullVerifier {
    fn verify_retry_capture_policy(store: &FullStore, generic: &GenericEvidenceStore, source: &TryCaptureSource) -> Result<(), IrVerifyError> {
        let Some(policy) = &source.retry else { return Ok(()); };
        if source.payload.first() != Some(&policy.delays_block) || source.payload.get(1) != Some(&u32::from(policy.selection.is_some())) {
            return Err(IrVerifyError::new("retry capture changes its original delay or selection policy"));
        }
        let block = IrBlockId::from_raw(policy.delays_block).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("retry delays block is missing"))?;
        if block.flags != BLOCK_LIST || store.payload(block.instructions)? != policy.delays_words.as_ref()
            || policy.delays_words.first().copied().map(|count| count as usize) != Some(policy.delays.len())
            || policy.delays_words.get(1..) != Some(policy.delays.iter().map(|delay| delay.instruction).collect::<Vec<_>>().as_slice()) {
            return Err(IrVerifyError::new("retry capture changes its original ordered attempts"));
        }
        for delay in &policy.delays {
            if store.tags.get(delay.instruction as usize) != Some(&delay.code.0)
                || store.payload(store.data[delay.instruction as usize].range())? != delay.code.1.as_ref() {
                return Err(IrVerifyError::new("retry capture changes its original delay expression"));
            }
            Self::verify_generic_source(store, generic, delay.instruction, source.owner, &delay.original_type, None, &mut vec![source.instruction])?;
        }
        if let Some((origin, _, pattern)) = policy.selection {
            if source.payload.get(2) != Some(&pattern) { return Err(IrVerifyError::new("retry capture changes its original error selection")); }
            let (id, _) = generic.pattern_sources().find(|(_, selected)| selected.origin == origin).ok_or_else(|| IrVerifyError::new("retry capture lacks its original checked selection"))?;
            Self::verify_pattern_shape(store, generic, id, pattern, 0)?;
        }
        Ok(())
    }
}

impl<'a> FullExecution<'a> {
    pub(in crate::runtime::eval) fn retry_capture(&self, instruction: u32) -> Result<Option<&TryCaptureSource>, IrVerifyError> {
        if !self.decoder.instruction_range.contains(&(instruction as usize)) { return Err(IrVerifyError::new("retry capture belongs to another body")); }
        let Some(generic) = self.generic_evidence() else { return Ok(None); };
        let Some(id) = generic.try_capture_source_at(instruction)? else { return Ok(None); };
        let source = generic.try_capture_source(id)?;
        if source.retry.is_none() { return Err(IrVerifyError::new("retry instruction has another capture protocol")); }
        let owner = if let Some(index) = driver_owner_index(self.decoder.owner) { InstructionOwner::Driver(index as u32) } else {
            InstructionOwner::Function(IrFunctionId::from_raw(self.decoder.owner).ok_or_else(|| IrVerifyError::new("retry capture owner is invalid"))?)
        };
        if source.owner != owner { return Err(IrVerifyError::new("retry capture changes its original owner")); }
        FullVerifier::verify_try_capture_source(self.decoder.store, generic, source)?;
        Ok(Some(source))
    }
}

#[cfg(test)]
#[path = "capture_prepare/retry_tests.rs"]
mod retry_tests;
