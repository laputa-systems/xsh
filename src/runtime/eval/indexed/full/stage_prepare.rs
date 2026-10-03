use super::*;
use super::super::generic::{graph_ground_type, OriginalStageBlockCallback, OriginalPreparedStage, OriginalStagePipeline, OriginalStageFusion, OriginalStageIdentityFlatMap, PreparedOperationAuthority, PreparedOperationEffects, PreparedStageResultRecord};
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
    if tag == FullStageTag::ParMapFlatMapReduceBy {
        let Some((map_slot, map_value, _, reduce_slot, reduce_value)) = fused_callback_ports(payload) else { return false; };
        return callback.slots.len() == 1 && callback.initial.is_none()
            && ((callback.slots[0] == map_slot && callback.value == map_value) || (callback.slots[0] == reduce_slot && callback.value == reduce_value));
    }
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

fn fused_callback_ports(payload: &[u32]) -> Option<(u32, u32, bool, u32, u32)> {
    let mut words = payload.iter().copied();
    let slot = words.next()?;
    for _ in 0..2 {
        match words.next()? { 0 => {}, 1 => { words.next()?; }, _ => return None }
    }
    let value = words.next()?;
    let flatten = match words.next()? { 0 => false, 1 => true, _ => return None };
    let reduce_slot = words.next()?;
    words.next()?;
    let reduce_value = words.next()?;
    words.next()?;
    if words.next().is_some() { return None; }
    Some((slot, value, flatten, reduce_slot, reduce_value))
}

fn fusion_selected_tag(kind: &StreamStageKind, tag: FullStageTag, identity: crate::sema::check::StageIdentity, fusion: Option<&OriginalStageFusion>) -> bool {
    if tag != FullStageTag::ParMapFlatMapReduceBy { return fusion.is_none() && selected_tag(kind, tag); }
    let Some(fusion) = fusion else { return false; };
    match kind {
        StreamStageKind::ParMap => fusion.stages.first() == Some(&identity),
        StreamStageKind::ReduceBy => fusion.stages.last() == Some(&identity),
        StreamStageKind::FlatMap => fusion.stages.len() == 3 && fusion.stages[1] == identity && fusion.identity_flat_map.is_some(),
        _ => false,
    }
}

impl FullBuilder {

    pub(super) fn stage_original_pipeline_fusions(&mut self, expression: BuildExprId, instruction: u32, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        let Some(BuildExprRow::ListPipeline { stages, .. }) = scratch.expressions.get(expression.index()) else { return Ok(()); };
        let Some(origin) = self.active_expression_origins.get(&expression).copied() else { return Ok(()); };
        let Some(solved) = self.solved.clone() else { return Ok(()); };
        if !solved.stage_operations.keys().any(|stage| stage.pipeline == origin) { return Ok(()); }
        for (ordinal, stage) in stages.iter().enumerate() {
            let LoweredPipelineStage::ParMapFlatMapReduceBy { slot, value, flatten, reduce_item_slot, reduce_value, .. } = stage else { continue; };
            let original = scratch.stage_fusion_origins.get(value).ok_or_else(|| problem("stage_fusion_original_source_missing"))?;
            if original.stages.len() != if *flatten { 3 } else { 2 } || original.identity_flat_map.is_some() != *flatten || original.identity_read.is_some() != *flatten
                || original.stages.iter().any(|stage| stage.pipeline != origin)
                || original.stages.windows(2).any(|pair| pair[0].index.checked_add(1) != Some(pair[1].index)) { return Err(problem("stage_fusion_original_sequence_changed")); }
            let callback_identity = |value, slot| scratch.stage_block_callback_origins.get(&value).filter(|callback| callback.slots.as_ref() == [slot]).map(|callback| callback.stage)
                .or_else(|| self.active_stage_call_origins.get(&value).copied());
            if callback_identity(*value, *slot) != original.stages.first().copied() || callback_identity(*reduce_value, *reduce_item_slot) != original.stages.last().copied() { return Err(problem("stage_fusion_original_callback_changed")); }
            let identity_flat_map = if let Some(callback) = &original.identity_flat_map {
                if callback.stage != original.stages[1] || callback.slots.len() != 1 || callback.parameters.len() != 1 || callback.initial.is_some()
                    || !matches!(scratch.expressions.get(callback.value.index()), Some(BuildExprRow::Param(slot)) if *slot == callback.slots[0]) { return Err(problem("stage_fusion_original_identity_port_changed")); }
                let source = solved.stage_operations.get(&callback.stage).ok_or_else(|| problem("stage_fusion_original_identity_stage_missing"))?;
                if !matches!(source.callback, Some(crate::sema::check::StageCallback::Block(block)) if block == callback.block) { return Err(problem("stage_fusion_original_identity_block_changed")); }
                let graph = &solved.graph;
                let scope = solved.operation_scope(ProducerFlowSource::Stage(callback.stage), &source.operation).map_err(|_| problem("stage_fusion_original_identity_scope"))?;
                graph.validate_requirement_scoped(ScopedRequirementRoot { requirement: source.operation.requirement, scope }).map_err(|_| problem("stage_fusion_original_identity_certificate"))?;
                let signature = *source.operation.actual_arguments.last().ok_or_else(|| problem("stage_fusion_original_identity_signature"))?;
                let crate::sema::inference::TypeNode::Arrow(arrow) = graph.node(graph.resolved(signature).map_err(|_| problem("stage_fusion_original_identity_signature"))?).map_err(|_| problem("stage_fusion_original_identity_signature"))? else { return Err(problem("stage_fusion_original_identity_signature")); };
                if arrow.params.len() != 1 { return Err(problem("stage_fusion_original_identity_parameter_count")); }
                if graph.closed_effect_summary(arrow.effects).map_err(|_| problem("stage_fusion_original_identity_effects"))? != EffectSummary::Closed(crate::sema::inference::EffectSet::EMPTY) { return Err(problem("stage_fusion_original_identity_not_effect_free")); }
                let parameter = graph_ground_type(graph, arrow.params[0].ty).map_err(|_| problem("stage_fusion_original_identity_parameter_scope"))?;
                let result = graph_ground_type(graph, arrow.result).map_err(|_| problem("stage_fusion_original_identity_result_scope"))?;
                let parameter_root = ScopedRoot { ty: arrow.params[0].ty, scope };
                graph.validate_scoped(parameter_root).map_err(|_| problem("stage_fusion_original_identity_parameter_certificate"))?;
                graph.validate_scoped(ScopedRoot { ty: arrow.result, scope }).map_err(|_| problem("stage_fusion_original_identity_result_certificate"))?;
                let read = original.identity_read.ok_or_else(|| problem("stage_fusion_original_identity_read_missing"))?;
                let checked = match read {
                    super::super::generic::OperationSourceOrigin::Expression(read) => {
                        if callback.reads.as_ref() != [(callback.value, read, 0)] { return Err(problem("stage_fusion_original_identity_read_changed")); }
                        let checked = *solved.expressions.get(&read).ok_or_else(|| problem("stage_fusion_original_identity_read_type"))?;
                        let read_scope = solved.expression_scope(read, source.operation.caller).map_err(|_| problem("stage_fusion_original_identity_read_scope"))?;
                        graph.validate_scoped(ScopedRoot { ty: checked, scope: read_scope }).map_err(|_| problem("stage_fusion_original_identity_read_certificate"))?;
                        checked
                    }
                    super::super::generic::OperationSourceOrigin::Statement(read) => {
                        if !callback.reads.is_empty() || solved.statements.get(&read) != Some(&crate::sema::check::StatementPosition::Value)
                            || solved.statement_owners.get(&read).copied() != source.operation.caller { return Err(problem("stage_fusion_original_identity_statement_owner")); }
                        parameter_root.ty
                    }
                    _ => return Err(problem("stage_fusion_original_identity_read_kind")),
                };
                if parameter != result || parameter != graph_ground_type(graph, checked).map_err(|_| problem("stage_fusion_original_identity_read_type"))? { return Err(problem("stage_fusion_original_identity_type_changed")); }
                Some(OriginalStageIdentityFlatMap { stage: callback.stage, block: callback.block, parameter: callback.parameters[0], slot: u32::try_from(callback.slots[0]).map_err(|_| problem("stage_fusion_original_identity_slot_overflow"))?, read, parameter_root, parameter_type: self.intern_generic_ground_type(&parameter)? })
            } else { None };
            self.stage_fusion_rows.push(OriginalStageFusion { instruction, ordinal: u32::try_from(ordinal).map_err(|_| problem("stage_fusion_ordinal_overflow"))?, stages: original.stages.clone(), identity_flat_map });
        }
        Ok(())
    }

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
            if original_stages.is_empty() { continue; }
            let mut physical_stages = Vec::with_capacity(original_stages.len());
            let mut original_index = 0;
            for (ordinal, &stage) in block_payload.iter().skip(1).enumerate() {
                let tag = *self.store.stages.get(stage as usize).ok_or_else(|| problem("stage_pipeline_stage_opcode"))?;
                let sources = self.stage_fusion_rows.iter().filter(|fusion| fusion.instruction == instruction && fusion.ordinal as usize == ordinal).collect::<Vec<_>>();
                if tag == FullStageTag::ParMapFlatMapReduceBy {
                    let [source] = sources.as_slice() else { return Err(problem("stage_pipeline_original_fusion_source_missing")); };
                    let fusion = Arc::new((**source).clone());
                    if !matches!(fusion.stages.len(), 2 | 3) || fusion.identity_flat_map.is_some() != (fusion.stages.len() == 3) { return Err(problem("stage_pipeline_original_fusion_sequence")); }
                    for &identity in &fusion.stages {
                        if original_stages.get(original_index).map(|(original, _)| **original) != Some(identity) { return Err(problem("stage_pipeline_original_fusion_order")); }
                        physical_stages.push((stage, Some(Arc::clone(&fusion))));
                        original_index += 1;
                    }
                } else {
                    if !sources.is_empty() || original_index >= original_stages.len() { return Err(problem("stage_pipeline_original_physical_order")); }
                    physical_stages.push((stage, None));
                    original_index += 1;
                }
            }
            if original_index != original_stages.len() { return Err(problem("stage_pipeline_original_stage_sequence_missing")); }
            if let Some((_, first)) = original_stages.first() {
                let flow = first.input_producer_flow.ok_or_else(|| problem("stage_pipeline_original_input_flow_missing"))?;
                let node = solved.producer_flows.node(flow).map_err(|_| problem("stage_pipeline_original_input_flow"))?;
                if node.source != ProducerFlowSource::Expression(input_origin) { return Err(problem("stage_pipeline_original_input_replaced")); }
            }
            let mut preceding_flow = original_stages.first().and_then(|(_, stage)| stage.input_producer_flow);
            let mut stages = Vec::new();
            let mut previous = input_type.clone();
            for ((&identity, original), (stage, fusion)) in original_stages.into_iter().zip(physical_stages) {
                if original.input_producer_flow != preceding_flow { return Err(problem("stage_pipeline_original_producer_order")); }
                if let Some(flow) = original.result_producer_flow { solved.producer_flows.node(flow).map_err(|_| problem("stage_pipeline_original_result_flow"))?; }
                preceding_flow = original.result_producer_flow;
                let operation = &original.operation;
                let stage_scope = solved.operation_scope(ProducerFlowSource::Stage(identity), operation).map_err(|_| problem("stage_pipeline_operation_scope"))?;
                graph.validate_requirement_scoped(ScopedRequirementRoot { requirement: operation.requirement, scope: stage_scope }).map_err(|_| problem("stage_pipeline_operation_certificate"))?;
                let selected = graph.candidate_evidence(operation.requirement).map_err(|_| problem("stage_pipeline_selection"))?.ok_or_else(|| problem("stage_pipeline_pending_selection"))?;
                let SolvedOperationAuthority::Stage(metadata) = solved.operation_catalog.candidate(graph, selected.candidate).map_err(|_| problem("stage_pipeline_selected_authority"))? else { return Err(problem("stage_pipeline_selected_kind")); };
                let tag = *self.store.stages.get(stage as usize).ok_or_else(|| problem("stage_pipeline_stage_opcode"))?;
                if !fusion_selected_tag(&metadata.stage, tag, identity, fusion.as_deref()) { return Err(problem("stage_pipeline_selected_opcode_changed")); }
                let receiver = operation.receiver.ok_or_else(|| problem("stage_pipeline_receiver_missing"))?;
                graph.validate_scoped(ScopedRoot { ty: receiver, scope: stage_scope }).map_err(|_| problem("stage_pipeline_receiver_certificate"))?;
                let stage_input = graph_ground_type(graph, receiver).map_err(|_| problem("stage_pipeline_receiver_requires_scope"))?;
                if stage_input != previous { return Err(problem("stage_pipeline_original_sequence_changed")); }
                graph.validate_scoped(ScopedRoot { ty: selected.result, scope: stage_scope }).map_err(|_| problem("stage_pipeline_result_certificate"))?;
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
                let result_record_layout = if tag == FullStageTag::GroupBy {
                    let Type::Stream(record) = &stage_result else { return Err(problem("stage_group_by_original_result_carrier")); };
                    let schema = crate::runtime::eval::require::PreparedSchema::compile_record_layout(record).ok_or_else(|| problem("stage_group_by_original_result_row"))?;
                    Some(PreparedStageResultRecord { record: self.intern_generic_ground_type(record)?, schema })
                } else { None };
                let payload: Box<[u32]> = self.store.payload(self.store.stage_data[stage as usize].range()).map_err(|_| problem("stage_pipeline_stage_payload"))?.into();
                let callback = self.stage_block_callback_rows.iter().find(|callback| callback.stage == identity).cloned();
                if let Some(callback) = &callback {
                    if !block_callback_matches(tag, &payload, callback) { return Err(problem("stage_block_callback_original_stage_rows_changed")); }
                }
                stages.push(OriginalPreparedStage { origin: identity, authority: PreparedOperationAuthority::Stage { identity: metadata.identity, stage: metadata.stage.clone(), source: metadata.source, form: metadata.form, variant: metadata.variant, callback_slot: metadata.callback_slot, additional_producer: metadata.additional_producer }, input, result, effects, arguments, supplied_slots: operation.binding.supplied_slots.iter().map(|&slot| slot as u32).collect(), default_slots: operation.binding.default_slots.iter().map(|&slot| slot as u32).collect(), stage, tag, payload, callback, result_record_layout, fusion });
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

    fn verify_stage_fusions(store: &FullStore, pipeline: &OriginalStagePipeline) -> Result<(), IrVerifyError> {
        let mut logical = 0;
        for (ordinal, &physical) in pipeline.block_payload.iter().skip(1).enumerate() {
            let stage = pipeline.stages.get(logical).ok_or_else(|| IrVerifyError::new("stage fusion loses its original operation sequence"))?;
            if stage.stage != physical { return Err(IrVerifyError::new("stage fusion changes its original physical order")); }
            if stage.tag != FullStageTag::ParMapFlatMapReduceBy {
                if stage.fusion.is_some() { return Err(IrVerifyError::new("ordinary stage borrows fused source authority")); }
                logical += 1;
                continue;
            }
            let fusion = stage.fusion.as_ref().ok_or_else(|| IrVerifyError::new("fused stage loses its original source authority"))?;
            if fusion.instruction != pipeline.instruction || fusion.ordinal as usize != ordinal || !matches!(fusion.stages.len(), 2 | 3)
                || fusion.identity_flat_map.is_some() != (fusion.stages.len() == 3) { return Err(IrVerifyError::new("fused stage changes its original source grouping")); }
            let (_, _, flatten, _, _) = fused_callback_ports(&stage.payload).ok_or_else(|| IrVerifyError::new("fused stage callback payload is invalid"))?;
            if flatten != fusion.identity_flat_map.is_some() { return Err(IrVerifyError::new("fused stage changes its original flattening action")); }
            for &origin in &fusion.stages {
                let source = pipeline.stages.get(logical).ok_or_else(|| IrVerifyError::new("fused stage source operation is missing"))?;
                if source.origin != origin || source.stage != physical || source.tag != stage.tag || source.payload != stage.payload
                    || !source.fusion.as_ref().is_some_and(|original| Arc::ptr_eq(original, fusion)) { return Err(IrVerifyError::new("fused stage changes an original composed operation")); }
                let PreparedOperationAuthority::Stage { stage: kind, .. } = &source.authority else { return Err(IrVerifyError::new("fused stage has another operation authority")); };
                if !fusion_selected_tag(kind, source.tag, origin, Some(fusion)) { return Err(IrVerifyError::new("fused stage changes its selected original operation order")); }
                logical += 1;
            }
            if let Some(identity) = &fusion.identity_flat_map {
                let (source_id, namespace) = match identity.read {
                    super::super::generic::OperationSourceOrigin::Expression(read) => (read.source, read.namespace),
                    super::super::generic::OperationSourceOrigin::Statement(read) => (read.source, read.namespace),
                    _ => return Err(IrVerifyError::new("fused identity callback has another original source kind")),
                };
                if identity.stage != fusion.stages[1] || source_id != pipeline.origin.source || namespace != pipeline.origin.namespace { return Err(IrVerifyError::new("fused identity callback borrows another source port")); }
                let source = &pipeline.stages[logical - 2];
                let parameter = store.semantic.to_type(identity.parameter_type)?;
                let Type::List(item) = &parameter else { return Err(IrVerifyError::new("fused identity callback changes its original list domain")); };
                if source.callback.is_some() || store.semantic.to_type(source.input)? != Type::Stream(Box::new(parameter.clone()))
                    || store.semantic.to_type(source.result)? != Type::Stream(item.clone()) { return Err(IrVerifyError::new("fused identity callback changes its original input or result port")); }
            }
        }
        if logical != pipeline.stages.len() { return Err(IrVerifyError::new("stage fusion drops an original source operation")); }
        Ok(())
    }

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
        Self::verify_stage_fusions(store, source)?;
        if source.owner != owner || store.semantic.to_type(source.result)? != *expected { return Err(IrVerifyError::new("stage pipeline changes its original owner or result")); }
        if store.tags.get(instruction as usize) != Some(&FullTag::ExprPipeline) || store.payload(store.data[instruction as usize].range())? != source.instruction_payload.as_ref() { return Err(IrVerifyError::new("stage pipeline changes its original input or stage block")); }
        let block = IrBlockId::from_raw(source.instruction_payload[1]).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("stage pipeline block is invalid"))?;
        if block.flags != source.block_flags || store.payload(block.instructions)? != source.block_payload.as_ref() { return Err(IrVerifyError::new("stage pipeline changes its original stage order")); }
        for stage in &source.stages {
            stage.verify_result_record_layout(&store.semantic)?;
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
    pub(in crate::runtime::eval) fn materialize_stage_result_record(&self, pipeline_instruction: u32, stage_row: u32, value: LoweredValue, span: Span) -> Result<LoweredValue, crate::runtime::value::RuntimeError> {
        let layout = || -> Result<&PreparedStageResultRecord, IrVerifyError> {
            let pipeline = self.stage_pipeline(pipeline_instruction)?.ok_or_else(|| IrVerifyError::new("structured stage result has no original pipeline authority"))?;
            let stage = pipeline.stages.iter().find(|stage| stage.stage == stage_row).ok_or_else(|| IrVerifyError::new("structured stage result belongs to another pipeline"))?;
            stage.verify_result_record_layout(&self.decoder.store.semantic)?;
            stage.result_record_layout.as_ref().ok_or_else(|| IrVerifyError::new("structured stage result has no original record row layout"))
        };
        let layout = layout().map_err(|error| crate::runtime::value::RuntimeError::new("indexed-ir", error.message).with_span(span))?;
        match value {
            LoweredValue::List(items) => items.into_iter().map(|item| layout.materialize_item(item, span)).collect::<Result<Vec<_>, _>>().map(LoweredValue::List),
            _ => Err(crate::runtime::value::RuntimeError::new("indexed-ir", "group-by changes its original physical list result").with_span(span)),
        }
    }

    pub(in crate::runtime::eval) fn stage_pipeline(&self, instruction: u32) -> Result<Option<&OriginalStagePipeline>, IrVerifyError> {
        if !self.decoder.instruction_range.contains(&(instruction as usize)) { return Err(IrVerifyError::new("stage pipeline belongs to another body")); }
        let Some(generic) = self.generic_evidence() else { return Ok(None); };
        let Some(source) = generic.stage_pipeline_at(instruction)? else { return Ok(None); };
        let owner = if let Some(driver) = driver_owner_index(self.decoder.owner) { InstructionOwner::Driver(driver as u32) } else {
            InstructionOwner::Function(IrFunctionId::from_raw(self.decoder.owner).ok_or_else(|| IrVerifyError::new("stage pipeline owner is invalid"))?)
        };
        if source.owner != owner || source.instruction != instruction { return Err(IrVerifyError::new("stage pipeline changes its original execution owner")); }
        let store = self.decoder.store;
        FullVerifier::verify_stage_fusions(store, source)?;
        if store.tags.get(instruction as usize) != Some(&FullTag::ExprPipeline) || store.payload(store.data[instruction as usize].range())? != source.instruction_payload.as_ref() { return Err(IrVerifyError::new("stage pipeline changes its original execution operands")); }
        let block = IrBlockId::from_raw(source.instruction_payload[1]).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("stage pipeline execution block is invalid"))?;
        if block.flags != source.block_flags || store.payload(block.instructions)? != source.block_payload.as_ref() { return Err(IrVerifyError::new("stage pipeline changes its original execution order")); }
        for stage in &source.stages {
            stage.verify_result_record_layout(&store.semantic)?;
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
