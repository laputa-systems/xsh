use super::*;
use super::super::generic::{graph_ground_type, OriginalStageBlockCallback, OriginalPreparedStage, OriginalStagePipeline, PreparedOperationAuthority, PreparedOperationEffects};
use crate::sema::check::{ProducerFlowSource, SolvedOperationAuthority};
use crate::sema::inference::{EffectSummary, ScopedRequirementRoot, ScopedRoot};
use crate::syntax::node::StreamStageKind;

fn problem(reason: &'static str) -> IrBuildError { IrBuildError::format(reason, None, 0, 0) }

fn selected_tag(kind: &StreamStageKind, tag: FullStageTag) -> bool {
    use FullStageTag as T;
    use StreamStageKind as K;
    matches!((kind, tag),
        (K::Map, T::Map | T::MapBlock) | (K::Where, T::Where | T::WhereBlock)
        | (K::FlatMap, T::FlatMap | T::FlatMapBlock) | (K::Batch, T::BatchCount | T::BatchMaxArgv | T::BatchMaxBytes | T::BatchLimits)
        | (K::ReduceBy, T::ReduceBy | T::ReduceByConfigured) | (K::TablePrint, T::TablePrint | T::TablePrintConfigured)
        | (K::ParMap, T::ParMap | T::ParMapBlock) | (K::Fold | K::Reduce, T::Fold)
        | (K::TextStreamLines, T::TextLines) | (K::JsonLines | K::JsonStream, T::JsonLines)
        | (K::BytesChunks, T::BytesChunks) | (K::Shuffle, T::Shuffle) | (K::Tee, T::Tee) | (K::Each, T::Each)
        | (K::Enumerate, T::Enumerate) | (K::Zip, T::Zip) | (K::Sort, T::Sort) | (K::SortBy, T::SortBy)
        | (K::GroupBy, T::GroupBy) | (K::Count, T::Count | T::CountBy) | (K::Any, T::Any | T::AnyBlock)
        | (K::All, T::All | T::AllBlock) | (K::UniqueBy, T::UniqueBy) | (K::Sum, T::Sum)
        | (K::Collect, T::Collect) | (K::First, T::First) | (K::Last, T::Last) | (K::Min, T::Min)
        | (K::Max, T::Max) | (K::Take, T::Take) | (K::Drop, T::Drop) | (K::Repeat, T::Repeat) | (K::Range, T::Range))
}

fn block_callback_matches(tag: FullStageTag, payload: &[u32], callback: &OriginalStageBlockCallback) -> bool {
    if tag == FullStageTag::Fold {
        return callback.slots.len() == 2 && payload.len() == 5 && payload[..2] == *callback.slots
            && Some(payload[2]) == callback.initial && payload[4] == callback.value;
    }
    if callback.slots.len() != 1 || callback.initial.is_some() || payload.first() != callback.slots.first() { return false; }
    let value = match tag {
        FullStageTag::Map | FullStageTag::Where | FullStageTag::FlatMap | FullStageTag::GroupBy | FullStageTag::UniqueBy
        | FullStageTag::CountBy | FullStageTag::Any | FullStageTag::All | FullStageTag::SortBy => payload.get(1),
        FullStageTag::MapBlock | FullStageTag::WhereBlock | FullStageTag::FlatMapBlock | FullStageTag::AnyBlock
        | FullStageTag::AllBlock | FullStageTag::ReduceBy | FullStageTag::ReduceByConfigured => payload.get(2),
        FullStageTag::ParMap | FullStageTag::ParMapBlock => payload.last(),
        _ => None,
    };
    value == Some(&callback.value)
}

impl FullBuilder {

    pub(super) fn stage_original_block_callback(&mut self, expression: BuildExprId, instruction: u32, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        let Some(original) = scratch.stage_block_callback_origins.get(&expression) else { return Ok(()); };
        if original.value != expression || self.active_encoded_expressions.get(&expression) != Some(&instruction) { return Err(problem("stage_block_callback_original_value_changed")); }
        let solved = self.solved.clone().ok_or_else(|| problem("stage_block_callback_original_graph_missing"))?;
        let source = solved.stage_operations.get(&original.stage).ok_or_else(|| problem("stage_block_callback_original_stage_missing"))?;
        if !matches!(source.callback, Some(crate::sema::check::StageCallback::Block(block)) if block == original.block) || original.parameters.len() != original.slots.len() || !matches!(original.slots.len(), 1 | 2) || (original.slots.len() == 2 && original.slots[0] == original.slots[1]) { return Err(problem("stage_block_callback_original_block_changed")); }
        let graph = &solved.graph;
        let scope = solved.operation_scope(ProducerFlowSource::Stage(original.stage), &source.operation).map_err(|_| problem("stage_block_callback_original_scope"))?;
        graph.validate_requirement_scoped(ScopedRequirementRoot { requirement: source.operation.requirement, scope }).map_err(|_| problem("stage_block_callback_original_certificate"))?;
        let callback = *source.operation.actual_arguments.last().ok_or_else(|| problem("stage_block_callback_original_signature_missing"))?;
        let crate::sema::inference::TypeNode::Arrow(arrow) = graph.node(graph.resolved(callback).map_err(|_| problem("stage_block_callback_original_signature"))?).map_err(|_| problem("stage_block_callback_original_signature"))? else { return Err(problem("stage_block_callback_original_signature")); };
        if arrow.params.len() != original.slots.len() { return Err(problem("stage_block_callback_original_parameter_count")); }
        let types = arrow.params.iter().map(|parameter| graph_ground_type(graph, parameter.ty).map_err(|_| problem("stage_block_callback_parameter_requires_scope"))).collect::<Result<Vec<_>, _>>()?;
        let initial = original.initial.map(|row| self.active_encoded_expressions.get(&row).copied().ok_or_else(|| problem("stage_callback_original_initial_missing"))).transpose()?;
        let mut reads = Vec::new();
        for &(row, origin, port) in &original.reads {
            if port as usize >= original.slots.len() || !matches!(scratch.expressions.get(row.index()), Some(BuildExprRow::Param(slot)) if *slot == original.slots[port as usize]) { return Err(problem("stage_block_callback_original_read_slot_changed")); }
            let Some(&read) = self.active_encoded_expressions.get(&row) else { continue; };
            if self.active_expression_origins.get(&row) != Some(&origin) { return Err(problem("stage_block_callback_original_read_identity_changed")); }
            let checked = *solved.expressions.get(&origin).ok_or_else(|| problem("stage_block_callback_original_read_type_missing"))?;
            let read_scope = solved.expression_scope(origin, source.operation.caller).map_err(|_| problem("stage_block_callback_original_read_scope"))?;
            graph.validate_scoped(ScopedRoot { ty: checked, scope: read_scope }).map_err(|_| problem("stage_block_callback_original_read_certificate"))?;
            if graph_ground_type(graph, checked).map_err(|_| problem("stage_block_callback_original_read_requires_scope"))? != types[port as usize] { return Err(problem("stage_block_callback_original_read_type_changed")); }
            reads.push((read, origin, port));
        }
        let types = types.iter().map(|ty| self.intern_generic_ground_type(ty)).collect::<Result<Vec<_>, _>>()?.into_boxed_slice();
        self.stage_block_callback_rows.push(OriginalStageBlockCallback { stage: original.stage, block: original.block, parameters: original.parameters.clone(), slots: original.slots.iter().map(|&slot| u32::try_from(slot).map_err(|_| problem("stage_callback_slot_overflow"))).collect::<Result<Vec<_>, _>>()?.into_boxed_slice(), types, initial, value: instruction, reads: reads.into_boxed_slice() });
        Ok(())
    }
    pub(super) fn prepare_stage_pipelines(&mut self) -> Result<(), IrBuildError> {
        let Some(solved) = self.solved.clone() else { return Ok(()); };
        let graph = &solved.graph;
        let origins = self.generic_expression_rows.iter().map(|&(instruction, origin, owner)| (instruction, (origin, owner))).collect::<FxHashMap<_, _>>();
        for (instruction, origin, owner) in self.generic_expression_rows.clone() {
            if self.store.tags[instruction as usize] != FullTag::ExprPipeline { continue; }
            let checked = *solved.expressions.get(&origin).ok_or_else(|| problem("stage_pipeline_original_result_missing"))?;
            let Ok(result_type) = graph_ground_type(graph, checked) else { continue; };
            let caller = solved.expression_owners.get(&origin).copied();
            let scope = solved.expression_scope(origin, caller).map_err(|_| problem("stage_pipeline_original_scope"))?;
            graph.validate_scoped(ScopedRoot { ty: checked, scope }).map_err(|_| problem("stage_pipeline_original_result_certificate"))?;
            let instruction_payload = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| problem("stage_pipeline_payload"))?.to_vec();
            let input = *instruction_payload.first().ok_or_else(|| problem("stage_pipeline_input"))?;
            let (input_source, input_wrappers) = self.argument_initializer_lineage(input, owner)?;
            let &(input_origin, input_owner) = origins.get(&input_source).ok_or_else(|| problem("stage_pipeline_original_input_missing"))?;
            if input_owner != owner { return Err(problem("stage_pipeline_original_input_owner")); }
            let input_checked = *solved.expressions.get(&input_origin).ok_or_else(|| problem("stage_pipeline_original_input_type"))?;
            let input_scope = solved.expression_scope(input_origin, caller).map_err(|_| problem("stage_pipeline_original_input_scope"))?;
            graph.validate_scoped(ScopedRoot { ty: input_checked, scope: input_scope }).map_err(|_| problem("stage_pipeline_original_input_certificate"))?;
            let Ok(input_type) = graph_ground_type(graph, input_checked) else { continue; };
            let block = instruction_payload.get(1).and_then(|raw| IrBlockId::from_raw(*raw)).and_then(|id| self.store.blocks.get(id.index())).copied().ok_or_else(|| problem("stage_pipeline_stage_block"))?;
            let block_payload = self.store.payload(block.instructions).map_err(|_| problem("stage_pipeline_stage_block"))?.to_vec();
            let original_stages = solved.stage_operations.iter().filter(|(identity, _)| identity.pipeline == origin).collect::<Vec<_>>();
            if original_stages.len() != block_payload.len().saturating_sub(1) { continue; }
            if let Some((_, first)) = original_stages.first() {
                let flow = first.input_producer_flow.ok_or_else(|| problem("stage_pipeline_original_input_flow_missing"))?;
                let node = solved.producer_flows.node(flow).map_err(|_| problem("stage_pipeline_original_input_flow"))?;
                if node.source != ProducerFlowSource::Expression(input_origin) { return Err(problem("stage_pipeline_original_input_replaced")); }
            }
            let mut preceding_flow = original_stages.first().and_then(|(_, stage)| stage.input_producer_flow);
            let mut stages = Vec::new();
            let mut previous = input_type.clone();
            for ((&identity, original), &stage) in original_stages.into_iter().zip(block_payload.iter().skip(1)) {
                if original.input_producer_flow != preceding_flow { return Err(problem("stage_pipeline_original_producer_order")); }
                if let Some(flow) = original.result_producer_flow { solved.producer_flows.node(flow).map_err(|_| problem("stage_pipeline_original_result_flow"))?; }
                preceding_flow = original.result_producer_flow;
                let operation = &original.operation;
                let stage_scope = solved.operation_scope(ProducerFlowSource::Stage(identity), operation).map_err(|_| problem("stage_pipeline_operation_scope"))?;
                graph.validate_requirement_scoped(ScopedRequirementRoot { requirement: operation.requirement, scope: stage_scope }).map_err(|_| problem("stage_pipeline_operation_certificate"))?;
                let selected = graph.candidate_evidence(operation.requirement).map_err(|_| problem("stage_pipeline_selection"))?.ok_or_else(|| problem("stage_pipeline_pending_selection"))?;
                let SolvedOperationAuthority::Stage(metadata) = solved.operation_catalog.candidate(graph, selected.candidate).map_err(|_| problem("stage_pipeline_selected_authority"))? else { return Err(problem("stage_pipeline_selected_kind")); };
                let tag = *self.store.stages.get(stage as usize).ok_or_else(|| problem("stage_pipeline_stage_opcode"))?;
                if !selected_tag(&metadata.stage, tag) { return Err(problem("stage_pipeline_selected_opcode_changed")); }
                let receiver = operation.receiver.ok_or_else(|| problem("stage_pipeline_receiver_missing"))?;
                graph.validate_scoped(ScopedRoot { ty: receiver, scope: stage_scope }).map_err(|_| problem("stage_pipeline_receiver_certificate"))?;
                let stage_input = graph_ground_type(graph, receiver).map_err(|_| problem("stage_pipeline_receiver_requires_scope"))?;
                if stage_input != previous { return Err(problem("stage_pipeline_original_sequence_changed")); }
                let stage_result = graph_ground_type(graph, selected.result).map_err(|_| problem("stage_pipeline_result_requires_scope"))?;
                previous = stage_result.clone();
                let crate::sema::inference::RequirementTemplate::Operation { call, .. } = graph.requirement_template(operation.requirement).map_err(|_| problem("stage_pipeline_operation_template"))? else { return Err(problem("stage_pipeline_operation_template")); };
                let call = graph.operation_call(call).map_err(|_| problem("stage_pipeline_operation_call"))?;
                let closed = |summary| match graph.closed_effect_summary(summary).map_err(|_| problem("stage_pipeline_effect_owner"))? { EffectSummary::Closed(bits) => Ok(bits), _ => Err(problem("stage_pipeline_effect_requires_scope")) };
                let effects = PreparedOperationEffects { creation: closed(selected.effects)?, inputs: call.effect_bindings.iter().map(|&(role, summary)| closed(summary).map(|bits| (role, bits))).collect::<Result<Vec<_>, _>>()?.into_boxed_slice(), outputs: call.output_effect_bindings.iter().map(|&(role, summary)| closed(summary).map(|bits| (role, bits))).collect::<Result<Vec<_>, _>>()?.into_boxed_slice() };
                let arguments = solved.stage_argument_sources.get(&identity).ok_or_else(|| problem("stage_pipeline_argument_sources_missing"))?.clone().into_boxed_slice();
                for &actual in &operation.actual_arguments { graph.validate_scoped(ScopedRoot { ty: actual, scope: stage_scope }).map_err(|_| problem("stage_pipeline_argument_certificate"))?; }
                let input = self.intern_generic_ground_type(&stage_input)?;
                let result = self.intern_generic_ground_type(&stage_result)?;
                let payload: Box<[u32]> = self.store.payload(self.store.stage_data[stage as usize].range()).map_err(|_| problem("stage_pipeline_stage_payload"))?.into();
                let callback = self.stage_block_callback_rows.iter().find(|callback| callback.stage == identity).cloned();
                if let Some(callback) = &callback {
                    if !block_callback_matches(tag, &payload, callback) { return Err(problem("stage_block_callback_original_stage_rows_changed")); }
                }
                stages.push(OriginalPreparedStage { origin: identity, authority: PreparedOperationAuthority::Stage { identity: metadata.identity, stage: metadata.stage.clone(), source: metadata.source, form: metadata.form, variant: metadata.variant, callback_slot: metadata.callback_slot, additional_producer: metadata.additional_producer }, input, result, effects, arguments, supplied_slots: operation.binding.supplied_slots.iter().map(|&slot| slot as u32).collect(), default_slots: operation.binding.default_slots.iter().map(|&slot| slot as u32).collect(), stage, tag, payload, callback });
            }
            // The expression consumes the final stage stream into a List.
            let materialized = match previous { Type::Stream(item) => Type::List(item), other => other };
            if materialized != result_type { return Err(problem("stage_pipeline_original_materialization_changed")); }
            let input_type = self.intern_generic_ground_type(&input_type)?;
            let result = self.intern_generic_ground_type(&result_type)?;
            self.generic_evidence_mut().add_original_stage_pipeline(OriginalStagePipeline { origin, instruction, owner, input, input_source, input_wrappers, input_type, result, instruction_payload: instruction_payload.into_boxed_slice(), block_flags: block.flags, block_payload: block_payload.into_boxed_slice(), stages: stages.into_boxed_slice() }).map_err(|_| problem("stage_pipeline_allocation"))?;
        }
        Ok(())
    }
}

impl FullVerifier {

    pub(super) fn verify_stage_block_callback_operand(store: &FullStore, generic: &GenericEvidenceStore, source: u32, owner: InstructionOwner, expected: &Type) -> Result<bool, IrVerifyError> {
        let Some((pipeline, callback, port)) = generic.stage_block_callback_read_at(source)? else { return Ok(false); };
        if pipeline.owner != owner || store.semantic.to_type(callback.types[port as usize])? != *expected { return Err(IrVerifyError::new("stage block callback read changes its original owner or parameter domain")); }
        let words = store.payload(store.data[source as usize].range())?;
        if store.tags[source as usize] != FullTag::ExprParam || words != [callback.slots[port as usize]] { return Err(IrVerifyError::new("stage block callback read changes its original parameter slot")); }
        let stage = pipeline.stages.iter().find(|stage| stage.origin == callback.stage).ok_or_else(|| IrVerifyError::new("stage block callback loses its original stage"))?;
        if store.stages.get(stage.stage as usize) != Some(&stage.tag) || !block_callback_matches(stage.tag, &stage.payload, callback) || store.payload(store.stage_data[stage.stage as usize].range())? != stage.payload.as_ref() { return Err(IrVerifyError::new("stage block callback changes its original callback or parameter binding")); }
        Ok(true)
    }
    pub(super) fn verify_stage_pipeline_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, expected: &Type, instance: Option<InstantiationId>, active: &mut Vec<u32>) -> Result<(), IrVerifyError> {
        let source = generic.stage_pipeline_at(instruction)?.ok_or_else(|| IrVerifyError::new("pipeline lacks its independently prepared stage proof"))?;
        if source.owner != owner || store.semantic.to_type(source.result)? != *expected { return Err(IrVerifyError::new("stage pipeline changes its original owner or result")); }
        if store.tags.get(instruction as usize) != Some(&FullTag::ExprPipeline) || store.payload(store.data[instruction as usize].range())? != source.instruction_payload.as_ref() { return Err(IrVerifyError::new("stage pipeline changes its original input or stage block")); }
        let block = IrBlockId::from_raw(source.instruction_payload[1]).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("stage pipeline block is invalid"))?;
        if block.flags != source.block_flags || store.payload(block.instructions)? != source.block_payload.as_ref() { return Err(IrVerifyError::new("stage pipeline changes its original stage order")); }
        for stage in &source.stages {
            let data = store.stage_data.get(stage.stage as usize).ok_or_else(|| IrVerifyError::new("stage pipeline row is missing"))?;
            let payload = store.payload(data.range())?;
            if matches!(stage.tag, FullStageTag::Map | FullStageTag::Where | FullStageTag::FlatMap | FullStageTag::GroupBy | FullStageTag::UniqueBy | FullStageTag::CountBy | FullStageTag::Any | FullStageTag::All | FullStageTag::SortBy | FullStageTag::ParMap)
                && payload.first() != stage.payload.first() { return Err(IrVerifyError::new("stage pipeline changes its original item slot")); }
            if store.stages.get(stage.stage as usize) != Some(&stage.tag) || payload != stage.payload.as_ref() { return Err(IrVerifyError::new("stage pipeline changes its selected operation or configuration children")); }
        }
        let already_active = active.last() == Some(&instruction);
        if !already_active { active.push(instruction); }
        Self::verify_argument_initializer_lineage(store, generic, source.input, source.input_source, &source.input_wrappers, owner)?;
        Self::verify_generic_source(store, generic, source.input_source, owner, &store.semantic.to_type(source.input_type)?, instance, active)?;
        if !already_active { active.pop(); }
        Ok(())
    }
    pub(super) fn verify_stage_pipelines(store: &FullStore, generic: &GenericEvidenceStore) -> Result<(), IrVerifyError> {
        for pipeline in generic.stage_pipelines() {
            for stage in &pipeline.stages {
                if let Some(callback) = &stage.callback {
                    for &(source, _, port) in &callback.reads { Self::verify_stage_block_callback_operand(store, generic, source, pipeline.owner, &store.semantic.to_type(callback.types[port as usize])?)?; }
                }
            }
        }
        for source in generic.stage_pipelines() { Self::verify_stage_pipeline_operand(store, generic, source.instruction, source.owner, &store.semantic.to_type(source.result)?, None, &mut Vec::new())?; }
        Ok(())
    }
}

impl FullExecution<'_> {
    pub(in crate::runtime::eval) fn stage_pipeline(&self, instruction: u32) -> Result<Option<&OriginalStagePipeline>, IrVerifyError> {
        if !self.decoder.instruction_range.contains(&(instruction as usize)) { return Err(IrVerifyError::new("stage pipeline belongs to another body")); }
        let Some(generic) = self.generic_evidence() else { return Ok(None); };
        let Some(source) = generic.stage_pipeline_at(instruction)? else { return Ok(None); };
        let owner = if let Some(driver) = driver_owner_index(self.decoder.owner) { InstructionOwner::Driver(driver as u32) } else {
            InstructionOwner::Function(IrFunctionId::from_raw(self.decoder.owner).ok_or_else(|| IrVerifyError::new("stage pipeline owner is invalid"))?)
        };
        if source.owner != owner || source.instruction != instruction { return Err(IrVerifyError::new("stage pipeline changes its original execution owner")); }
        let store = self.decoder.store;
        if store.tags.get(instruction as usize) != Some(&FullTag::ExprPipeline) || store.payload(store.data[instruction as usize].range())? != source.instruction_payload.as_ref() { return Err(IrVerifyError::new("stage pipeline changes its original execution operands")); }
        let block = IrBlockId::from_raw(source.instruction_payload[1]).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("stage pipeline execution block is invalid"))?;
        if block.flags != source.block_flags || store.payload(block.instructions)? != source.block_payload.as_ref() { return Err(IrVerifyError::new("stage pipeline changes its original execution order")); }
        for stage in &source.stages {
            if store.stages.get(stage.stage as usize) != Some(&stage.tag) || store.payload(store.stage_data[stage.stage as usize].range())? != stage.payload.as_ref() { return Err(IrVerifyError::new("stage pipeline changes its original execution configuration")); }
        }
        FullVerifier::verify_argument_initializer_lineage(store, generic, source.input, source.input_source, &source.input_wrappers, owner)?;
        for stage in &source.stages {
            if let Some(callback) = &stage.callback {
                for &(read, _, port) in &callback.reads { FullVerifier::verify_stage_block_callback_operand(store, generic, read, owner, &store.semantic.to_type(callback.types[port as usize])?)?; }
            }
        }
        Ok(Some(source))
    }
}
