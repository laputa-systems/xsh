use super::*;
use crate::runtime::eval::indexed::generic::OperationSourceOrigin;

// Stage parameters belong to the authored callback block and ordinal. They
// have no ordinary declaration binding target; retain their actual allocated
// slots and source reads before the block's lexical names leave scope.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct BuildStageBlockCallbackOrigin {
    pub stage: crate::sema::check::StageIdentity,
    pub block: BlockId,
    pub parameters: Box<[Option<(Name, Span)>]>,
    pub slots: Box<[usize]>,
    pub initial: Option<BuildExprId>,
    pub value: BuildExprId,
    pub reads: Box<[(BuildExprId, ExpressionIdentity, u32)]>,
}

// Fusion retains the exact authored operation sequence and the callback it
// erases. An omitted identity callback must remain tied to its original port.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct BuildStageFusionOrigin {
    pub stages: Box<[crate::sema::check::StageIdentity]>,
    pub identity_flat_map: Option<BuildStageBlockCallbackOrigin>,
    pub identity_read: Option<OperationSourceOrigin>,
}

impl CompactLowerConstructProbe<'_, '_> {
    pub(super) fn original_stage_fusion(&self, map_value: BuildExprId, flat_map: Option<&LoweredPipelineStage>, reduce_value: BuildExprId) -> Option<BuildStageFusionOrigin> {
        let scratch = self.scratch.borrow();
        let identity = |value| scratch.stage_block_callback_origins.get(&value).map(|callback| callback.stage)
            .or_else(|| self.stage_call_origins.get(&value).copied());
        let map = identity(map_value)?;
        let reduce = identity(reduce_value)?;
        let identity_flat_map = if let Some(stage) = flat_map { Some((|| {
            let (slot, value) = match stage {
                LoweredPipelineStage::FlatMap { slot, value } => (*slot, *value),
                LoweredPipelineStage::FlatMapBlock { slot, body, value } if body.is_empty() => (*slot, *value),
                _ => return None,
            };
            let original = scratch.stage_block_callback_origins.get(&value)?;
            if original.slots.as_ref() != [slot] || original.initial.is_some() || !matches!(scratch.expressions.get(value.index()), Some(BuildExprRow::Param(read)) if *read == slot) { return None; }
            let read = if let [(row, origin, 0)] = original.reads.as_ref() {
                if *row != value { return None; }
                OperationSourceOrigin::Expression(*origin)
            } else if original.reads.is_empty() {
                let statements = self.program.arena.stmt_ids(self.program.arena.block(original.block).statements).collect::<Vec<_>>();
                let [statement] = statements.as_slice() else { return None; };
                let ArenaStmtKind::TailBareIdent(name) = self.program.arena.stmt(*statement).kind else { return None; };
                if original.parameters.len() != 1 || original.parameters[0].map(|(parameter, _)| parameter) != Some(name) { return None; }
                OperationSourceOrigin::Statement(self.statement_identity(*statement))
            } else { return None; };
            Some((original.clone(), read))
        })()?) } else { None };
        let mut stages = vec![map];
        if let Some((callback, _)) = &identity_flat_map { stages.push(callback.stage); }
        stages.push(reduce);
        if stages.windows(2).any(|pair| pair[0].pipeline != pair[1].pipeline || pair[0].index.checked_add(1) != Some(pair[1].index)) { return None; }
        let (identity_flat_map, identity_read) = match identity_flat_map { Some((callback, read)) => (Some(callback), Some(read)), None => (None, None) };
        Some(BuildStageFusionOrigin { stages: stages.into_boxed_slice(), identity_flat_map, identity_read })
    }

    // Legacy output-shape inspection can export only a closed declaration.
    // Callback lowering instead consumes the checked invocation instance.
    pub(super) fn stage_callable_return_type(&self, callee: ExprId) -> Option<Type> {
        let key = match self.program.arena.expr(callee).kind {
            ArenaExprKind::Ident(name) => self.compact_unqualified_function_key(name),
            ArenaExprKind::Field { base, name } => match self.program.arena.expr(base).kind {
                ArenaExprKind::Ident(namespace) => Some(LoweredFunctionKey::Qualified(self.compact_qualified_function_key(namespace, name))),
                _ => None,
            },
            _ => None,
        };
        let definitions = self.function_index();
        if let Some(function) = key.and_then(|key| definitions.definition(key)) {
            let body = self.program.arena.function_def(function.id).body;
            let identity = DeclarationIdentity {
                source: self.program.arena.span(self.program.arena.block(body).span).source_id,
                namespace: function.namespace, declaration: function.id,
            };
            if let Some(callable) = self.solved().declarations.get(&identity) {
                let graph = &self.solved().graph;
                let scheme = graph.scheme(callable.scheme).ok()?;
                if !scheme.quantifiers.is_empty() || !scheme.effect_quantifiers.is_empty() { return None; }
                for requirement in &scheme.requirements {
                    match requirement {
                        crate::sema::inference::RequirementTemplate::Add { left, right, result } => {
                            graph.export_type(*left).ok()?;
                            graph.export_type(*right).ok()?;
                            graph.export_type(*result).ok()?;
                        }
                        crate::sema::inference::RequirementTemplate::Eligibility { .. }
                        | crate::sema::inference::RequirementTemplate::Operation { .. }
                        | crate::sema::inference::RequirementTemplate::EffectInclusion { .. }
                        | crate::sema::inference::RequirementTemplate::CallableInvocation { .. }
                        | crate::sema::inference::RequirementTemplate::ErrorJoin { .. }
                        | crate::sema::inference::RequirementTemplate::EqualityCompatible { .. } => return None,
                    }
                }
                let signature = graph.resolved(callable.signature).ok()?;
                let crate::sema::inference::TypeNode::Arrow(arrow) = graph.node(signature).ok()? else { return None; };
                for parameter in &arrow.params { graph.export_type(parameter.ty).ok()?; }
                return graph.export_type(arrow.result).ok();
            }
        }
        self.bodies.stage_callable_types.get(&callee).cloned()
    }

    pub(super) fn lower_pipeline_stage(
        &mut self,
        stage: &ArenaStreamStage,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_ty: Option<&Type>,
        identity: Option<crate::sema::check::StageIdentity>,
    ) -> Option<LoweredPipelineStage> {
        let descriptor = identity.and_then(|identity| self.solved().stage_operations.get(&identity)).and_then(|source| match source.callback.as_ref() {
            Some(crate::sema::check::StageCallback::Callable { expression, .. } | crate::sema::check::StageCallback::Protocol { expression, .. }) => Some(*expression),
            _ => None,
        });
        if let Some(callee) = descriptor {
            let identity = identity?;
            let (slot, value) = self.lower_checked_stage_callback(callee, identity, slots, current_function)?;
            if !xsh_registry::stream_parameters::stage_parameters(stage.kind.as_str()).is_empty() {
                return self.lower_configured_pipeline_stage(stage, slots, current_function, item_ty, Some((slot, value)), Some(identity));
            }
            if !self.solved().stage_argument_sources.get(&identity)?.is_empty() { return None; }
            return Some(match stage.kind {
                StreamStageKind::Where => LoweredPipelineStage::Where { slot, predicate: value },
                StreamStageKind::Map => LoweredPipelineStage::Map { slot, value },
                StreamStageKind::FlatMap => LoweredPipelineStage::FlatMap { slot, value },
                StreamStageKind::GroupBy => LoweredPipelineStage::GroupBy { slot, key: value },
                StreamStageKind::UniqueBy => LoweredPipelineStage::UniqueBy { slot, key: value },
                StreamStageKind::Count => LoweredPipelineStage::CountBy { slot, key: value },
                StreamStageKind::Any => LoweredPipelineStage::Any { slot, predicate: value },
                StreamStageKind::All => LoweredPipelineStage::All { slot, predicate: value },
                StreamStageKind::Each | StreamStageKind::Tee => {
                    let body = vec![push_build_row!(self, stmt, BuildStmtRow::Expr { value, span: self.program.arena.span(stage.span) })];
                    if stage.kind == StreamStageKind::Each { LoweredPipelineStage::Each { slot, body } }
                    else { LoweredPipelineStage::Tee { slot, body } }
                }
                _ => return None,
            });
        }
        if !xsh_registry::stream_parameters::stage_parameters(stage.kind.as_str()).is_empty() {
            return self.lower_configured_pipeline_stage(stage, slots, current_function, item_ty, None, identity);
        }
        match stage.kind {
            StreamStageKind::TextStreamLines => {

                if stage.block.is_some() || !stage.args.is_empty() {
                    return None;
                }
                Some(LoweredPipelineStage::TextLines)
            }
            StreamStageKind::JsonLines | StreamStageKind::JsonStream => {

                if stage.block.is_some() || !stage.args.is_empty() {
                    return None;
                }
                Some(LoweredPipelineStage::JsonLines)
            }
            StreamStageKind::Enumerate => {

                if stage.block.is_some() || !stage.args.is_empty() {
                    return None;
                }
                Some(LoweredPipelineStage::Enumerate)
            }
            StreamStageKind::Zip => None,
            StreamStageKind::Sort => None,
            StreamStageKind::Sum => {

                if stage.block.is_some() || !stage.args.is_empty() {
                    return None;
                }
                Some(LoweredPipelineStage::Sum)
            }
            StreamStageKind::Collect => {

                if stage.block.is_some() || !stage.args.is_empty() {
                    return None;
                }
                Some(LoweredPipelineStage::Collect)
            }
            StreamStageKind::First => {

                if stage.block.is_some() || !stage.args.is_empty() {
                    return None;
                }
                Some(LoweredPipelineStage::First)
            }
            StreamStageKind::Last => {

                if stage.block.is_some() || !stage.args.is_empty() {
                    return None;
                }
                Some(LoweredPipelineStage::Last)
            }
            StreamStageKind::Min => {

                if stage.block.is_some() || !stage.args.is_empty() {
                    return None;
                }
                Some(LoweredPipelineStage::Min)
            }
            StreamStageKind::Max => {

                if stage.block.is_some() || !stage.args.is_empty() {
                    return None;
                }
                Some(LoweredPipelineStage::Max)
            }
            StreamStageKind::SortBy => None,
            StreamStageKind::UniqueBy => {

                if let Some((slot, key)) =
                    self.try_lower_pipeline_stage_shorthand(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::UniqueBy { slot, key });
                }
                let (slot, key) =
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty, identity)?;
                Some(LoweredPipelineStage::UniqueBy { slot, key })
            }
            StreamStageKind::GroupBy => {
                if let Some((slot, key)) =
                    self.try_lower_pipeline_stage_shorthand(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::GroupBy { slot, key });
                }
                let (slot, key) =
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty, identity)?;
                Some(LoweredPipelineStage::GroupBy { slot, key })
            }
            StreamStageKind::Count => {
                if !stage.args.is_empty() {
                    if stage.block.is_some() {
                        return None;
                    }
                    if let Some((slot, key)) = self.try_lower_pipeline_stage_shorthand(
                        stage,
                        slots,
                        current_function,
                        item_ty,
                    ) {
                        return Some(LoweredPipelineStage::CountBy { slot, key });
                    }
                    return None;
                }
                if stage.block.is_none() {
                    return Some(LoweredPipelineStage::Count);
                }
                let (slot, key) =
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty, identity)?;
                Some(LoweredPipelineStage::CountBy { slot, key })
            }
            StreamStageKind::Where => {

                if let Some((slot, predicate)) =
                    self.try_lower_pipeline_stage_shorthand(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::Where { slot, predicate });
                }
                if let Some((slot, predicate)) =
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty, identity)
                {
                    return Some(LoweredPipelineStage::Where { slot, predicate });
                }
                let (slot, body, value) =
                    self.lower_pipeline_stage_block(stage, slots, current_function, item_ty, identity)?;
                Some(LoweredPipelineStage::WhereBlock { slot, body, value })
            }
            StreamStageKind::Map => {

                if !stage.args.is_empty() {
                    if stage.block.is_some() {
                        return None;
                    }
                    let args = self.program.arena.call_args(stage.args);
                    let [arg] = args else {
                        return None;
                    };
                    let ArenaCallArgKind::Positional(expr) = arg.kind else {
                        return None;
                    };
                    let (slot, _cleanup) =
                        self.lower_pipeline_stage_item_slot(stage, slots, item_ty)?;
                    let value = self.lower_expr(expr, slots, current_function, Some(slot))?;
                    cleanup_pipeline_stage_item_slot(slots, _cleanup, slot);
                    return Some(LoweredPipelineStage::Map { slot, value });
                }
                if let Some((slot, value)) =
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty, identity)
                {
                    return Some(LoweredPipelineStage::Map { slot, value });
                }
                let (slot, body, value) =
                    self.lower_pipeline_stage_block(stage, slots, current_function, item_ty, identity)?;
                Some(LoweredPipelineStage::MapBlock { slot, body, value })
            }
            StreamStageKind::FlatMap => {

                if let Some((slot, value)) =
                    self.try_lower_pipeline_stage_shorthand(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::FlatMap { slot, value });
                }
                if let Some((slot, value)) =
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty, identity)
                {
                    return Some(LoweredPipelineStage::FlatMap { slot, value });
                }
                let (slot, body, value) =
                    self.lower_pipeline_stage_block(stage, slots, current_function, item_ty, identity)?;
                Some(LoweredPipelineStage::FlatMapBlock { slot, body, value })
            }
            StreamStageKind::Any => {

                if let Some((slot, predicate)) =
                    self.try_lower_pipeline_stage_shorthand(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::Any { slot, predicate });
                }
                if let Some((slot, predicate)) =
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty, identity)
                {
                    return Some(LoweredPipelineStage::Any { slot, predicate });
                }
                let (slot, body, value) =
                    self.lower_pipeline_stage_block(stage, slots, current_function, item_ty, identity)?;
                Some(LoweredPipelineStage::AnyBlock { slot, body, value })
            }
            StreamStageKind::All => {

                if let Some((slot, predicate)) =
                    self.try_lower_pipeline_stage_shorthand(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::All { slot, predicate });
                }
                if let Some((slot, predicate)) =
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty, identity)
                {
                    return Some(LoweredPipelineStage::All { slot, predicate });
                }
                let (slot, body, value) =
                    self.lower_pipeline_stage_block(stage, slots, current_function, item_ty, identity)?;
                Some(LoweredPipelineStage::AllBlock { slot, body, value })
            }
            StreamStageKind::Take | StreamStageKind::Drop => None,
            StreamStageKind::Repeat => None,
            StreamStageKind::Range => None,
            StreamStageKind::BytesChunks => None,
            StreamStageKind::Batch => None,
            StreamStageKind::ParMap => None,
            StreamStageKind::Each => {
                let block = stage.block?;
                let (slot, cleanup) = self.lower_pipeline_stage_item_slot(stage, slots, item_ty)?;
                let saved = slots.enter();
                let body =
                    self.lower_block_in_current_scope(block, slots, current_function, Some(slot))?;
                slots.exit(saved);
                cleanup_pipeline_stage_item_slot(slots, cleanup, slot);
                Some(LoweredPipelineStage::Each { slot, body })
            }
            StreamStageKind::Tee => {
                let block = stage.block?;
                let (slot, cleanup) = self.lower_pipeline_stage_item_slot(stage, slots, item_ty)?;
                let saved = slots.enter();
                let body =
                    self.lower_block_in_current_scope(block, slots, current_function, Some(slot))?;
                slots.exit(saved);
                cleanup_pipeline_stage_item_slot(slots, cleanup, slot);
                Some(LoweredPipelineStage::Tee { slot, body })
            }
            StreamStageKind::TablePrint => None,
            StreamStageKind::ReduceBy => None,
            StreamStageKind::Shuffle => None,
            StreamStageKind::Fold | StreamStageKind::Reduce => None,
        }
    }

    // The source invocation owns argument destinations and omitted defaults.
    // Lowering creates runtime rows for those destinations without inventing
    // source expressions or evaluating a default before an item is produced.
    fn lower_checked_stage_callback(
        &mut self,
        callee: ExprId,
        identity: crate::sema::check::StageIdentity,
        slots: &mut SlotScope,
        current_function: Option<Name>,
    ) -> Option<(usize, BuildExprId)> {
        let source = self.solved().stage_operations.get(&identity)?;
        let crate::sema::check::StageCallback::Callable {
            expression, requirement, declaration: Some(declaration), ..
        } = source.callback.as_ref()? else { return None; };
        if *expression != callee { return None; }
        let declaration = *declaration;
        let evidence = self.solved().graph.invocation_evidence(*requirement).ok()??;
        let (_, binding, timing) = evidence.unique_plan()?;
        if timing != crate::sema::inference::InvocationDefaultTiming::AtCall
            || binding.dynamic.is_some() || binding.supplied_slots.len() != 1 { return None; }
        let binding = binding.clone();
        let original = self.program.arena.function_def(declaration.declaration);
        let key = compact_function_key(declaration.namespace, original.name);
        let functions = self.function_index();
        let function = functions.definition(key)?;
        if function.id != declaration.declaration || function.namespace != declaration.namespace
            || function.definition_span.source_id != declaration.source { return None; }
        let direct = !self.declarations.static_callable_aliases.contains_key(&self.program.arena.expr(callee).span)
            && match self.program.arena.expr(callee).kind {
                ArenaExprKind::Ident(name) => slots.resolve(name).is_none() && self.compact_unqualified_function_key(name) == Some(key),
                ArenaExprKind::Field { base, name } => matches!(self.program.arena.expr(base).kind,
                    ArenaExprKind::Ident(namespace) if slots.resolve(namespace).is_none()
                        && LoweredFunctionKey::Qualified(self.compact_qualified_function_key(namespace, name)) == key),
                _ => false,
            };
        let slot = slots.reserve("pipeline.item");
        let item = push_build_row!(self, expr, BuildExprRow::Param(slot));
        let supplied = binding.supplied_slots[0];
        let mut destinations = binding.default_slots.iter().copied().map(|slot| (slot, LoweredCallArg::Default(slot))).collect::<Vec<_>>();
        destinations.push((supplied, LoweredCallArg::Single(item)));
        destinations.sort_by_key(|(slot, _)| *slot);
        let args = destinations.into_iter().map(|(_, value)| value).collect();
        let span = self.program.arena.expr(callee).span;
        let row = if direct {
            // Omitted defaults stay in ordinary callee binding; the direct pure
            // path evaluates supplied expressions only.
            if binding.default_slots.is_empty() && self.compact_direct_pure_call_candidate(key) { BuildExprRow::DirectPureCall { function: key, args, span } }
            else { BuildExprRow::Call { function: key, args, span } }
        } else {
            let handle = self.lower_expr(callee, slots, current_function, None)?;
            BuildExprRow::DynamicCall { callee: handle, args, span }
        };
        let value = push_build_row!(self, expr, row);
        self.stage_call_origins.insert(value, identity);
        cleanup_pipeline_stage_item_slot(slots, None, slot);
        Some((slot, value))
    }

    pub(super) fn lower_pipeline_stage_fold(
        &mut self,
        stage: &ArenaStreamStage,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_ty: Option<&Type>,
        initial: BuildExprId,
        acc_ty: Option<Type>,
        identity: Option<crate::sema::check::StageIdentity>,
    ) -> Option<LoweredPipelineStage> {
        let block = stage.block?;
        let saved = slots.enter();
        let params = self
            .program
            .arena
            .block_params(self.program.arena.block(block).params);
        let acc_slot = match params {
            [] => slots.reserve("pipeline.acc"),
            [acc] | [acc, _] => {
                if slots.is_bound_non_capture(acc.name) {
                    slots.exit(saved);
                    return None;
                }
                slots.declare_with_type(acc.name, acc_ty)
            }
            _ => {
                slots.exit(saved);
                return None;
            }
        };
        let item_slot = match params {
            [_, item] => {
                if slots.is_bound_non_capture(item.name) {
                    slots.exit(saved);
                    return None;
                }
                slots.declare_with_type(item.name, item_ty.cloned())
            }
            _ => slots.reserve("pipeline.item"),
        };
        let first_callback_row = self.scratch.borrow().expressions.len();
        let body = Vec::new();
        let value = match self.lower_block_value_expr(block, slots, current_function, Some(item_slot)) {
            Some(value) => value,
            None => { slots.exit(saved); return None; }
        };
        self.record_original_stage_block_callback(identity, block, &[acc_slot, item_slot], Some(initial), value, first_callback_row)?;
        slots.exit(saved);
        Some(LoweredPipelineStage::Fold {
            acc_slot,
            item_slot,
            initial,
            body,
            value,
        })
    }

    fn record_original_stage_block_callback(
        &mut self, identity: Option<crate::sema::check::StageIdentity>, block: BlockId,
        ports: &[usize], initial: Option<BuildExprId>, value: BuildExprId, first_row: usize,
    ) -> Option<()> {
        let Some(identity) = identity else { return Some(()); };
        let params = self.program.arena.block_params(self.program.arena.block(block).params);
        let parameters = if params.is_empty() && ports.len() == 1 {
            vec![None]
        } else if params.len() == ports.len() {
            params.iter().map(|parameter| Some((parameter.name, self.program.arena.span(parameter.span)))).collect()
        } else { return Some(()); };
        let mut reads = Vec::new();
        for (&row, &origin) in &self.expression_origins {
            if row.index() < first_row { continue; }
            let Some(BuildExprRow::Param(slot)) = self.scratch.borrow().expressions.get(row.index()).cloned() else { continue; };
            let Some(port) = ports.iter().position(|&port| port == slot) else { continue; };
            let belongs = match (parameters[port], self.program.arena.expr(origin.expression).kind) {
                (Some((parameter, _)), ArenaExprKind::Ident(name)) => parameter == name,
                (None, ArenaExprKind::Item) => true,
                _ => false,
            };
            if !belongs { return None; }
            reads.push((row, origin, port as u32));
        }
        reads.sort_by_key(|(row, _, _)| row.index());
        self.scratch.borrow_mut().stage_block_callback_origins.insert(value, BuildStageBlockCallbackOrigin {
            stage: identity, block, parameters: parameters.into_boxed_slice(), slots: ports.into(),
            initial, value, reads: reads.into_boxed_slice(),
        });
        Some(())
    }

    pub(super) fn lower_pipeline_stage_expr(
        &mut self,
        stage: &ArenaStreamStage,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_ty: Option<&Type>,
        identity: Option<crate::sema::check::StageIdentity>,
    ) -> Option<(usize, BuildExprId)> {
        let block = stage.block?;
        let statements = self.program.arena.block(block).statements;
        let statements = self.program.arena.stmt_ids(statements).collect::<Vec<_>>();
        let (slot, cleanup) = self.lower_pipeline_stage_item_slot(stage, slots, item_ty)?;
        let first_callback_row = self.scratch.borrow().expressions.len();
        let lowered = match statements.as_slice() {
            [stmt] => self.lower_tail_stmt_as_expr(*stmt, slots, current_function, Some(slot))
                .or_else(|| self.lower_block_value_expr(block, slots, current_function, Some(slot))),
            _ => self.lower_block_value_expr(block, slots, current_function, Some(slot)),
        };
        let expr = match lowered {
            Some(expr) => expr,
            None => {
                cleanup_pipeline_stage_item_slot(slots, cleanup, slot);
                return None;
            }
        };
        self.record_original_stage_block_callback(identity, block, &[slot], None, expr, first_callback_row)?;
        cleanup_pipeline_stage_item_slot(slots, cleanup, slot);
        Some((slot, expr))
    }

    pub(super) fn lower_pipeline_stage_block(
        &mut self,
        stage: &ArenaStreamStage,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_ty: Option<&Type>,
        identity: Option<crate::sema::check::StageIdentity>,
    ) -> Option<(usize, Vec<BuildStmtId>, BuildExprId)> {
        let block = stage.block?;
        let statements = self.program.arena.block(block).statements;
        let ids = self.program.arena.stmt_ids(statements).collect::<Vec<_>>();
        let (&tail, prefix) = ids.split_last()?;
        let saved = slots.enter();
        let (slot, _cleanup) = match self.lower_pipeline_stage_item_slot(stage, slots, item_ty) {
            Some(value) => value,
            None => {
                slots.exit(saved);
                return None;
            }
        };
        let first_callback_row = self.scratch.borrow().expressions.len();
        let mut body = Vec::with_capacity(prefix.len());
        for stmt in prefix {
            let Some(lowered) =
                self.lower_stmt_with_blocker_guard(*stmt, slots, current_function, Some(slot))
            else {
                slots.exit(saved);
                return None;
            };
            body.push(lowered);
        }
        let value = match self.lower_tail_stmt_as_expr(tail, slots, current_function, Some(slot)) {
            Some(value) => value,
            None => {
                slots.exit(saved);
                return None;
            }
        };
        self.record_original_stage_block_callback(identity, block, &[slot], None, value, first_callback_row)?;
        slots.exit(saved);
        Some((slot, body, value))
    }

    pub(super) fn lower_pipeline_stage_item_slot(
        &self,
        stage: &ArenaStreamStage,
        slots: &mut SlotScope,
        item_ty: Option<&Type>,
    ) -> Option<(usize, Option<Name>)> {
        let block = stage.block?;
        let params = self
            .program
            .arena
            .block_params(self.program.arena.block(block).params);
        match params {
            [] => Some((slots.reserve("pipeline.item"), None)),
            [param] => {
                if slots.is_bound_non_capture(param.name) {
                    return None;
                }
                Some((
                    slots.declare_with_type(param.name, item_ty.cloned()),
                    Some(param.name),
                ))
            }
            _ => None,
        }
    }
    pub(super) fn lower_configured_pipeline_stage(
        &mut self,
        stage: &ArenaStreamStage,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_ty: Option<&Type>,
        callback: Option<(usize, BuildExprId)>,
        identity: Option<crate::sema::check::StageIdentity>,
    ) -> Option<LoweredPipelineStage> {
        use crate::sema::arguments::{ArgumentValueSource, ExpandedArgument};
        let identity = identity?;
        let source = self.solved().stage_operations.get(&identity)?;
        let evidence = self.solved().graph.candidate_evidence(source.operation.requirement).ok()??;
        let crate::sema::check::SolvedOperationAuthority::Stage(metadata) = self.solved().operation_catalog.candidate(&self.solved().graph, evidence.candidate).ok()? else { return None; };
        if metadata.stage != stage.kind { return None; }
        let params = metadata.parameters;
        let sources = self.solved().stage_argument_sources.get(&identity)?;
        let binding = source.operation.binding.clone();
        let count = sources.len();
        let has_callback = source.callback.is_some();
        if binding.dynamic.is_some() || binding.rest_slot.is_some()
            || binding.supplied_slots.len() != count + usize::from(has_callback)
            || source.operation.actual_arguments.len() != binding.supplied_slots.len()
            || has_callback && binding.supplied_slots.last() != Some(&params.len()) { return None; }
        let expanded = sources.iter().zip(&source.operation.actual_arguments).map(|(argument, ty)| {
            Some(ExpandedArgument { entry_index: argument.entry_index, name: argument.name, value: argument.value, span: argument.span, ty: self.solved_type(*ty)? })
        }).collect::<Option<Vec<_>>>()?;
        if binding.supplied_slots[..count].iter().any(|slot| *slot >= params.len()) { return None; }
        let destinations = binding.supplied_slots[..count].to_vec();
        let lowered = self.lower_expanded_argument_values(&expanded, slots, current_function, None)?;
        let mut values = vec![None; params.len()];
        let mut types = vec![None; params.len()];
        let mut booleans = vec![Some(false); params.len()];
        for ((argument, slot), value) in expanded.iter().zip(destinations).zip(lowered.values) {
            values[slot] = Some(value);
            types[slot] = Some(argument.ty.clone());
            booleans[slot] = match argument.value {
                ArgumentValueSource::Expression(expr) => match self.program.arena.expr(expr).kind { ArenaExprKind::Bool(value) => Some(value), _ => None },
                _ => None,
            };
        }
        let span = self.program.arena.span(stage.span);
        let wrap = |this: &mut Self, value| this.wrap_argument_bindings(value, lowered.bindings.clone(), span);
        let record = |this: &mut Self| {
            let fields = params.iter().zip(&values).filter_map(|(parameter, value)| value.map(|value| LoweredRecordEntry::Field(Name::intern(parameter.name), value))).collect();
            let record = push_build_row!(this, expr, BuildExprRow::Record(fields));
            wrap(this, record)
        };
        match stage.kind {
            StreamStageKind::ParMap => {
                let jobs = values[0].map(|value| wrap(self, value));
                if let Some((slot, value)) = callback {
                    return Some(LoweredPipelineStage::ParMap { slot, jobs, value });
                }
                if let Some((slot, value)) = self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty, Some(identity)) {
                    return Some(LoweredPipelineStage::ParMap { slot, jobs, value });
                }
                let (slot, body, value) = self.lower_pipeline_stage_block(stage, slots, current_function, item_ty, Some(identity))?;
                Some(LoweredPipelineStage::ParMapBlock { slot, body, jobs, value })
            }
            StreamStageKind::Sort => Some(LoweredPipelineStage::Sort { descending: values[0].map(|value| wrap(self, value)) }),
            StreamStageKind::SortBy => {
                let (slot, key) = callback.or_else(|| self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty, Some(identity)))?;
                Some(LoweredPipelineStage::SortBy { slot, key, descending: values[0].map(|value| wrap(self, value)) })
            }
            StreamStageKind::Batch => match values.as_slice() {
                [Some(count), None, None] => Some(LoweredPipelineStage::BatchCount { count: wrap(self, *count) }),
                [None, Some(max_bytes), None] => Some(LoweredPipelineStage::BatchMaxBytes { max_bytes: wrap(self, *max_bytes) }),
                [None, None, Some(_)] if booleans[2] == Some(true) => Some(LoweredPipelineStage::BatchMaxArgv { max_argv: None }),
                _ => Some(LoweredPipelineStage::BatchLimits { configuration: record(self) }),
            },
            StreamStageKind::ReduceBy => {
                let (item_slot, value) = callback.or_else(|| self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty, Some(identity)))?;
                let body = Vec::new();
                if booleans[..3].iter().all(Option::is_some) {
                    let op = match booleans[..3].iter().position(|value| *value == Some(true))? { 0 => ReduceByOp::Sum, 1 => ReduceByOp::Min, _ => ReduceByOp::Max };
                    let jobs = values[3].map(|value| wrap(self, value));
                    Some(LoweredPipelineStage::ReduceBy { item_slot, body, value, op, jobs })
                } else {
                    Some(LoweredPipelineStage::ReduceByConfigured { item_slot, body, value, configuration: record(self) })
                }
            }
            StreamStageKind::Take => Some(LoweredPipelineStage::Take(wrap(self, values[0]?))),
            StreamStageKind::Drop => Some(LoweredPipelineStage::Drop(wrap(self, values[0]?))),
            StreamStageKind::Repeat => Some(LoweredPipelineStage::Repeat { count: wrap(self, values[0]?) }),
            StreamStageKind::Range => Some(LoweredPipelineStage::Range { start: wrap(self, values[0]?), end: values[1]? }),
            StreamStageKind::BytesChunks => Some(LoweredPipelineStage::BytesChunks { size: wrap(self, values[0]?) }),
            StreamStageKind::Zip => Some(LoweredPipelineStage::Zip { other: wrap(self, values[0]?) }),
            StreamStageKind::Fold | StreamStageKind::Reduce => {
                let initial = wrap(self, values[0]?);
                self.lower_pipeline_stage_fold(stage, slots, current_function, item_ty, initial, types[0].clone(), Some(identity))
            }
            StreamStageKind::Shuffle => Some(LoweredPipelineStage::Shuffle { seed: values[0].map(|value| wrap(self, value)) }),
            StreamStageKind::TablePrint => Some(match values[0] {
                Some(value) => LoweredPipelineStage::TablePrintConfigured { columns: wrap(self, value) },
                None => LoweredPipelineStage::TablePrint { columns: None },
            }),
            _ => None,
        }
    }
}
