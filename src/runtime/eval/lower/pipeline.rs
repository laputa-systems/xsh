use super::{
    ArenaCallArgKind, ArenaExprKind, ArenaStreamStage, BuildExprId, BuildExprRow, BuildStmtId, BuildStmtRow,
    CompactLowerConstructProbe, LoweredPipelineStage, LoweredRecordEntry, Name, ReduceByOp,
    SlotScope, StreamStageKind, Type, cleanup_pipeline_stage_item_slot,
    compact_call_arg_expr,
};

impl<'p> CompactLowerConstructProbe<'p, '_> {
    pub(super) fn fuse_par_map_flat_map_reduce_by(&self, stages: &mut Vec<LoweredPipelineStage>) {
        let mut fused = Vec::with_capacity(stages.len());
        let mut index = 0;
        while index < stages.len() {
            if let Some((slot, body, jobs, value)) = Self::lowered_par_map_parts(&stages[index]) {
                if index + 2 < stages.len()
                    && self.lowered_flat_map_is_identity(&stages[index + 1])
                    && let Some((reduce_item_slot, reduce_body, reduce_value, op)) =
                        Self::lowered_reduce_by_parts(&stages[index + 2])
                {
                    fused.push(LoweredPipelineStage::ParMapFlatMapReduceBy {
                        slot,
                        body,
                        jobs,
                        value,
                        flatten: true,
                        reduce_item_slot,
                        reduce_body,
                        reduce_value,
                        op,
                    });
                    index += 3;
                    continue;
                }
                if index + 1 < stages.len()
                    && let Some((reduce_item_slot, reduce_body, reduce_value, op)) =
                        Self::lowered_reduce_by_parts(&stages[index + 1])
                {
                    fused.push(LoweredPipelineStage::ParMapFlatMapReduceBy {
                        slot,
                        body,
                        jobs,
                        value,
                        flatten: false,
                        reduce_item_slot,
                        reduce_body,
                        reduce_value,
                        op,
                    });
                    index += 2;
                    continue;
                }
            }
            fused.push(stages[index].clone());
            index += 1;
        }
        *stages = fused;
    }

    fn lowered_par_map_parts(
        stage: &LoweredPipelineStage,
    ) -> Option<(
        usize,
        Option<Vec<BuildStmtId>>,
        Option<BuildExprId>,
        BuildExprId,
    )> {
        match stage {
            LoweredPipelineStage::ParMap { slot, jobs, value } => {
                Some((*slot, None, *jobs, *value))
            }
            LoweredPipelineStage::ParMapBlock {
                slot,
                body,
                jobs,
                value,
            } => Some((*slot, Some(body.clone()), *jobs, *value)),
            _ => None,
        }
    }

    fn lowered_flat_map_is_identity(&self, stage: &LoweredPipelineStage) -> bool {
        let (slot, body, value) = match stage {
            LoweredPipelineStage::FlatMap { slot, value } => (*slot, true, *value),
            LoweredPipelineStage::FlatMapBlock { slot, body, value } => {
                (*slot, body.is_empty(), *value)
            }
            _ => return false,
        };
        if !body {
            return false;
        }
        matches!(
            self.scratch.borrow().expressions.get(value.index()),
            Some(BuildExprRow::Param(param)) if *param == slot
        )
    }

    fn lowered_reduce_by_parts(
        stage: &LoweredPipelineStage,
    ) -> Option<(usize, Vec<BuildStmtId>, BuildExprId, ReduceByOp)> {
        match stage {
            LoweredPipelineStage::ReduceBy {
                item_slot,
                body,
                value,
                op,
                jobs: None,
            } => Some((*item_slot, body.clone(), *value, *op)),
            _ => None,
        }
    }

    pub(super) fn lower_pipeline_stage(
        &mut self,
        stage: &ArenaStreamStage,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_ty: Option<&Type>,
    ) -> Option<LoweredPipelineStage> {
        if stage.block.is_none()
            && let Some(entry) = self
                .bodies
                .argument_bindings
                .get(&self.program.arena.span(stage.span))
                .and_then(|binding| binding.callable_entry)
        {
            let callee =
                compact_call_arg_expr(self.program.arena.call_args(stage.args).get(entry)?)?;
            let slot = slots.reserve("pipeline.item");
            let item = push_build_row!(self, expr, BuildExprRow::Param(slot));
            let value = self.lower_stage_callable_call(callee, item, slots, current_function)?;
            return Some(match stage.kind {
                StreamStageKind::Map => LoweredPipelineStage::Map { slot, value },
                StreamStageKind::Where => LoweredPipelineStage::Where { slot, predicate: value },
                StreamStageKind::FlatMap => LoweredPipelineStage::FlatMap { slot, value },
                StreamStageKind::UniqueBy => LoweredPipelineStage::UniqueBy { slot, key: value },
                StreamStageKind::GroupBy => LoweredPipelineStage::GroupBy { slot, key: value },
                StreamStageKind::Any => LoweredPipelineStage::Any { slot, predicate: value },
                StreamStageKind::All => LoweredPipelineStage::All { slot, predicate: value },
                StreamStageKind::SortBy => return self.lower_configured_pipeline_stage(
                    stage, slots, current_function, item_ty, Some((slot, value)),
                ),
                StreamStageKind::Each | StreamStageKind::Tee => {
                    let span = self.program.arena.expr(callee).span;
                    let body = vec![push_build_row!(self, stmt, BuildStmtRow::Expr { value, span })];
                    if stage.kind == StreamStageKind::Each { LoweredPipelineStage::Each { slot, body } }
                    else { LoweredPipelineStage::Tee { slot, body } }
                }
                StreamStageKind::TextStreamLines | StreamStageKind::BytesChunks
                | StreamStageKind::JsonLines | StreamStageKind::JsonStream
                | StreamStageKind::Enumerate | StreamStageKind::Zip | StreamStageKind::Sort
                | StreamStageKind::Sum | StreamStageKind::Collect | StreamStageKind::First
                | StreamStageKind::Last | StreamStageKind::Min | StreamStageKind::Max
                | StreamStageKind::Count | StreamStageKind::Take | StreamStageKind::Drop
                | StreamStageKind::Repeat | StreamStageKind::Range | StreamStageKind::Batch
                | StreamStageKind::ParMap | StreamStageKind::TablePrint | StreamStageKind::ReduceBy
                | StreamStageKind::Shuffle | StreamStageKind::Fold | StreamStageKind::Reduce => return None,
            });
        }
        if !xsh_registry::stream_parameters::stage_parameters(stage.kind.as_str()).is_empty() {
            return self.lower_configured_pipeline_stage(stage, slots, current_function, item_ty, None);
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
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty)?;
                Some(LoweredPipelineStage::UniqueBy { slot, key })
            }
            StreamStageKind::GroupBy => {
                if let Some((slot, key)) =
                    self.try_lower_pipeline_stage_shorthand(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::GroupBy { slot, key });
                }
                let (slot, key) =
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty)?;
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
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty)?;
                Some(LoweredPipelineStage::CountBy { slot, key })
            }
            StreamStageKind::Where => {
                if let Some((slot, predicate)) =
                    self.try_lower_pipeline_stage_shorthand(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::Where { slot, predicate });
                }
                if let Some((slot, predicate)) =
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::Where { slot, predicate });
                }
                let (slot, body, value) =
                    self.lower_pipeline_stage_block(stage, slots, current_function, item_ty)?;
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
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::Map { slot, value });
                }
                let (slot, body, value) =
                    self.lower_pipeline_stage_block(stage, slots, current_function, item_ty)?;
                Some(LoweredPipelineStage::MapBlock { slot, body, value })
            }
            StreamStageKind::FlatMap => {
                if let Some((slot, value)) =
                    self.try_lower_pipeline_stage_shorthand(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::FlatMap { slot, value });
                }
                if let Some((slot, value)) =
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::FlatMap { slot, value });
                }
                let (slot, body, value) =
                    self.lower_pipeline_stage_block(stage, slots, current_function, item_ty)?;
                Some(LoweredPipelineStage::FlatMapBlock { slot, body, value })
            }
            StreamStageKind::Any => {
                if let Some((slot, predicate)) =
                    self.try_lower_pipeline_stage_shorthand(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::Any { slot, predicate });
                }
                if let Some((slot, predicate)) =
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::Any { slot, predicate });
                }
                let (slot, body, value) =
                    self.lower_pipeline_stage_block(stage, slots, current_function, item_ty)?;
                Some(LoweredPipelineStage::AnyBlock { slot, body, value })
            }
            StreamStageKind::All => {
                if let Some((slot, predicate)) =
                    self.try_lower_pipeline_stage_shorthand(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::All { slot, predicate });
                }
                if let Some((slot, predicate)) =
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::All { slot, predicate });
                }
                let (slot, body, value) =
                    self.lower_pipeline_stage_block(stage, slots, current_function, item_ty)?;
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

    // The first consumed configuration value initializes source-ordered checked
    // temporaries. Later configuration fields read those slots at the same stage
    // boundary; record spreads and effectful expressions are never repeated.
    fn lower_configured_pipeline_stage(
        &mut self,
        stage: &ArenaStreamStage,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_ty: Option<&Type>,
        callable: Option<(usize, BuildExprId)>,
    ) -> Option<LoweredPipelineStage> {
        use crate::sema::arguments::{ArgumentValueSource, expand_named_arguments};
        let params = crate::sema::stage_arguments::stage_argument_params(stage.kind.as_str());
        let span = self.program.arena.span(stage.span);
        let argument_slots = &self.bodies.argument_bindings.get(&span)?.argument_slots;
        let callable_entry = self.bodies.argument_bindings.get(&span)?.callable_entry;
        let arguments = self.program.arena.call_args(stage.args).iter().enumerate()
            .filter(|(entry, _)| Some(*entry) != callable_entry)
            .map(|(_, argument)| argument.clone()).collect::<Vec<_>>();
        let expanded = expand_named_arguments(
            self.program,
            &arguments,
            |expr| {
                self.bodies
                    .expr_types
                    .get(&expr)
                    .cloned()
                    .or_else(|| self.checked_expr_type(expr))
            },
        )
        .ok()?;
        let lowered =
            self.lower_expanded_argument_values(&expanded, slots, current_function, None)?;
        let mut values = vec![None; params.len()];
        let mut types = vec![None; params.len()];
        let mut booleans = vec![Some(false); params.len()];
        for ((argument, &slot), value) in expanded.iter().zip(argument_slots).zip(lowered.values) {
            values[slot] = Some(value);
            types[slot] = Some(argument.ty.clone());
            booleans[slot] = match argument.value {
                ArgumentValueSource::Expression(expr) => match self.program.arena.expr(expr).kind {
                    ArenaExprKind::Bool(value) => Some(value),
                    _ => None,
                },
                _ => None,
            };
        }
        let wrap = |this: &mut Self, value| {
            this.wrap_argument_bindings(value, lowered.bindings.clone(), span)
        };
        let record = |this: &mut Self| {
            let fields = params
                .iter()
                .zip(&values)
                .filter_map(|(parameter, value)| {
                    value.map(|value| LoweredRecordEntry::Field(parameter.name, value))
                })
                .collect();
            let record = push_build_row!(this, expr, BuildExprRow::Record(fields));
            wrap(this, record)
        };
        match stage.kind {
            StreamStageKind::ParMap => {
                let jobs = values[0].map(|value| wrap(self, value));
                if let Some((slot, value)) =
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::ParMap { slot, jobs, value });
                }
                let (slot, body, value) =
                    self.lower_pipeline_stage_block(stage, slots, current_function, item_ty)?;
                Some(LoweredPipelineStage::ParMapBlock {
                    slot,
                    body,
                    jobs,
                    value,
                })
            }
            StreamStageKind::Sort => Some(LoweredPipelineStage::Sort {
                descending: values[0].map(|value| wrap(self, value)),
            }),
            StreamStageKind::SortBy => {
                let (slot, key) = callable.or_else(||
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty))?;
                Some(LoweredPipelineStage::SortBy {
                    slot,
                    key,
                    descending: values[0].map(|value| wrap(self, value)),
                })
            }
            StreamStageKind::Batch => match values.as_slice() {
                [Some(count), None, None] => Some(LoweredPipelineStage::BatchCount {
                    count: wrap(self, *count),
                }),
                [None, Some(max_bytes), None] => Some(LoweredPipelineStage::BatchMaxBytes {
                    max_bytes: wrap(self, *max_bytes),
                }),
                [None, None, Some(_)] if booleans[2] == Some(true) => {
                    Some(LoweredPipelineStage::BatchMaxArgv { max_argv: None })
                }
                _ => Some(LoweredPipelineStage::BatchLimits {
                    configuration: record(self),
                }),
            },
            StreamStageKind::ReduceBy => {
                let (item_slot, value) =
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty)?;
                let body = Vec::new();
                if booleans[..3].iter().all(Option::is_some) {
                    let op = match booleans[..3]
                        .iter()
                        .position(|value| *value == Some(true))?
                    {
                        0 => ReduceByOp::Sum,
                        1 => ReduceByOp::Min,
                        _ => ReduceByOp::Max,
                    };
                    let jobs = values[3].map(|value| wrap(self, value));
                    Some(LoweredPipelineStage::ReduceBy {
                        item_slot,
                        body,
                        value,
                        op,
                        jobs,
                    })
                } else {
                    Some(LoweredPipelineStage::ReduceByConfigured {
                        item_slot,
                        body,
                        value,
                        configuration: record(self),
                    })
                }
            }
            StreamStageKind::Take => Some(LoweredPipelineStage::Take(wrap(self, values[0]?))),
            StreamStageKind::Drop => Some(LoweredPipelineStage::Drop(wrap(self, values[0]?))),
            StreamStageKind::Repeat => Some(LoweredPipelineStage::Repeat {
                count: wrap(self, values[0]?),
            }),
            StreamStageKind::Range => Some(LoweredPipelineStage::Range {
                start: wrap(self, values[0]?),
                end: values[1]?,
            }),
            StreamStageKind::BytesChunks => Some(LoweredPipelineStage::BytesChunks {
                size: wrap(self, values[0]?),
            }),
            StreamStageKind::Zip => Some(LoweredPipelineStage::Zip {
                other: wrap(self, values[0]?),
            }),
            StreamStageKind::Fold | StreamStageKind::Reduce => {
                let initial = wrap(self, values[0]?);
                self.lower_pipeline_stage_fold(
                    stage,
                    slots,
                    current_function,
                    item_ty,
                    initial,
                    types[0].clone(),
                )
            }
            StreamStageKind::Shuffle => Some(LoweredPipelineStage::Shuffle {
                seed: values[0].map(|value| wrap(self, value)),
            }),
            StreamStageKind::TablePrint => Some(match values[0] {
                Some(value) => LoweredPipelineStage::TablePrintConfigured {
                    columns: wrap(self, value),
                },
                None => LoweredPipelineStage::TablePrint { columns: None },
            }),
            _ => None,
        }
    }

    fn try_lower_pipeline_stage_shorthand(
        &mut self,
        stage: &ArenaStreamStage,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        _item_ty: Option<&Type>,
    ) -> Option<(usize, BuildExprId)> {
        if stage.block.is_some() || stage.args.is_empty() {
            return None;
        }
        let args = self.program.arena.call_args(stage.args);
        let [arg] = args else {
            return None;
        };
        let ArenaCallArgKind::Positional(expr) = arg.kind else {
            return None;
        };
        let slot = slots.reserve("pipeline.item");
        let value = self.lower_expr(expr, slots, current_function, Some(slot))?;
        Some((slot, value))
    }

    // The bound initializer type stays on the accumulator slot. Nested pipeline
    // tails resolve their field and stage argument types from that slot, so
    // retaining the checked type is necessary for valid compositions to lower.
    // Callback parameters have their own lexical scope and may shadow outer names.
    fn lower_pipeline_stage_fold(
        &mut self,
        stage: &ArenaStreamStage,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_ty: Option<&Type>,
        initial: BuildExprId,
        acc_ty: Option<Type>,
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
                if slots.is_declared_here(acc.name) {
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
                if slots.is_declared_here(item.name) {
                    slots.exit(saved);
                    return None;
                }
                slots.declare_with_type(item.name, item_ty.cloned())
            }
            _ => slots.reserve("pipeline.item"),
        };
        let body = Vec::new();
        let value =
            match self.lower_block_value_expr(block, slots, current_function, Some(item_slot)) {
                Some(value) => value,
                None => {
                    slots.exit(saved);
                    return None;
                }
            };
        slots.exit(saved);
        Some(LoweredPipelineStage::Fold {
            acc_slot,
            item_slot,
            initial,
            body,
            value,
        })
    }

    fn lower_pipeline_stage_expr(
        &mut self,
        stage: &ArenaStreamStage,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_ty: Option<&Type>,
    ) -> Option<(usize, BuildExprId)> {
        let block = stage.block?;
        let statements = self.program.arena.block(block).statements;
        let statements = self.program.arena.stmt_ids(statements).collect::<Vec<_>>();
        let (slot, cleanup) = self.lower_pipeline_stage_item_slot(stage, slots, item_ty)?;
        let lowered = match statements.as_slice() {
            [stmt] => self
                .lower_tail_stmt_as_expr(*stmt, slots, current_function, Some(slot))
                .or_else(|| {
                    self.lower_block_value_expr(block, slots, current_function, Some(slot))
                }),
            _ => self.lower_block_value_expr(block, slots, current_function, Some(slot)),
        };
        let expr = match lowered {
            Some(expr) => expr,
            None => {
                cleanup_pipeline_stage_item_slot(slots, cleanup, slot);
                return None;
            }
        };
        cleanup_pipeline_stage_item_slot(slots, cleanup, slot);
        Some((slot, expr))
    }

    fn lower_pipeline_stage_block(
        &mut self,
        stage: &ArenaStreamStage,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_ty: Option<&Type>,
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
        slots.exit(saved);
        Some((slot, body, value))
    }

    fn lower_pipeline_stage_item_slot(
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
                // Retirement restores any outer slot and its checked type after
                // the callback, so this temporary binding may shadow a local.
                Some((
                    slots.declare_with_type(param.name, item_ty.cloned()),
                    Some(param.name),
                ))
            }
            _ => None,
        }
    }
}
