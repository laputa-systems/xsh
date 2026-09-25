use super::*;

// Serial stages run on each live source item before the next pull. Bounded
// terminals close a producer when they have enough input.
pub(super) enum IndexedPipelineItems {
    Materialized(std::vec::IntoIter<LoweredValue>),
    Live {
        stream: Box<StreamValue>,
        prefix: std::vec::IntoIter<crate::runtime::value::StreamItem>,
    },
}

impl IndexedPipelineItems {
    pub(super) fn new(
        evaluator: &mut Evaluator,
        current: LoweredValue,
        span: Span,
    ) -> Result<Self, RuntimeError> {
        match current {
            LoweredValue::Stream(mut stream)
                if stream.source.is_some() || stream.script().is_some() =>
            {
                let prefix = std::mem::take(&mut stream.items).into_iter();
                Ok(Self::Live { stream, prefix })
            }
            value => Ok(Self::Materialized(
                evaluator.lowered_pipeline_input_items(value, span)?.into_iter(),
            )),
        }
    }

    pub(super) fn next(
        &mut self,
        evaluator: &mut Evaluator,
        span: Span,
    ) -> Result<Option<LoweredValue>, RuntimeError> {
        match self {
            Self::Materialized(items) => Ok(items.next()),
            Self::Live { stream, prefix } => {
                let value = match prefix.next() {
                    Some(item) => Some(item.value),
                    None => evaluator.stream_next(stream, span)?,
                };
                value
                    .map(|value| {
                        lowered_value_from_runtime_any(&value).ok_or_else(|| {
                            RuntimeError::new(
                                "type-error",
                                format!("stream produced unsupported {}", value.type_name()),
                            )
                            .with_span(span)
                        })
                    })
                    .transpose()
            }
        }
    }

    pub(super) fn cancel(&mut self, evaluator: &mut Evaluator, span: Span) -> Result<(), RuntimeError> {
        if let Self::Live { stream, .. } = self {
            evaluator.stream_cancel(stream, span)?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub(super) enum IndexedLiveSerialStage {
    Tee { slot: usize, body: u32 },
    Where { slot: usize, predicate: u32 },
    WhereBlock { slot: usize, body: u32, value: u32 },
    Map { slot: usize, value: u32, flat: bool },
    MapBlock { slot: usize, body: u32, value: u32, flat: bool },
    Drop { count_expr: u32, count: usize, seen: usize },
    Enumerate { index: usize },
    Take { count_expr: u32, count: usize },
    Any { slot: usize, predicate: u32, all: bool },
    AnyBlock { slot: usize, body: u32, value: u32, all: bool },
    First,
    Count,
    Collect,
}

impl IndexedLiveSerialStage {
    fn tag(self) -> FullStageTag {
        match self {
            Self::Tee { .. } => FullStageTag::Tee,
            Self::Where { .. } => FullStageTag::Where,
            Self::WhereBlock { .. } => FullStageTag::WhereBlock,
            Self::Map { flat: false, .. } => FullStageTag::Map,
            Self::Map { flat: true, .. } => FullStageTag::FlatMap,
            Self::MapBlock { flat: false, .. } => FullStageTag::MapBlock,
            Self::MapBlock { flat: true, .. } => FullStageTag::FlatMapBlock,
            Self::Drop { .. } => FullStageTag::Drop,
            Self::Enumerate { .. } => FullStageTag::Enumerate,
            Self::Take { .. } => FullStageTag::Take,
            Self::Any { all: false, .. } => FullStageTag::Any,
            Self::Any { all: true, .. } => FullStageTag::All,
            Self::AnyBlock { all: false, .. } => FullStageTag::AnyBlock,
            Self::AnyBlock { all: true, .. } => FullStageTag::AllBlock,
            Self::First => FullStageTag::First,
            Self::Count => FullStageTag::Count,
            Self::Collect => FullStageTag::Collect,
        }
    }
}

// Only stages that produce another sequence can hand rows directly to a `for`
// body. A materializing or scalar terminal keeps the ordinary expression path.
pub(super) fn indexed_for_pipeline_input(
    execution: &FullExecution<'_>,
    instruction: u32,
    call_span: Span,
) -> Result<Option<(u32, Span, SmallVec<[IndexedLiveSerialStage; 4]>)>, RuntimeError> {
    let (tag, mut payload) = indexed_value(execution.instruction_id(instruction), call_span)?;
    if tag != FullTag::ExprPipeline {
        return Ok(None);
    }
    let input = indexed_raw(&mut payload, call_span)?;
    let (_, mut stages) = execution
        .block(&mut payload, BLOCK_LIST)
        .map_err(|error| indexed_error(error, call_span))?;
    let stage_count = indexed_raw(&mut stages, call_span)? as usize;
    let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
    indexed_finish(payload, call_span)?;
    let decoded = decode_serial_prefix(execution, stages, stage_count, span)?;
    if decoded.len() != stage_count
        || decoded.is_empty()
        || matches!(
            decoded.last(),
            Some(
                IndexedLiveSerialStage::Any { .. }
                    | IndexedLiveSerialStage::AnyBlock { .. }
                    | IndexedLiveSerialStage::First
                    | IndexedLiveSerialStage::Count
                    | IndexedLiveSerialStage::Collect
            )
        )
    {
        return Ok(None);
    }
    Ok(Some((input, span, decoded)))
}

// The expression runner drains this cursor; a `for` frame keeps it between body
// executions. Both retain the source, stage counters, and flat-map expansion.
pub(super) struct IndexedSerialPipeline {
    items: IndexedPipelineItems,
    stages: SmallVec<[IndexedLiveSerialStage; 4]>,
    pending: SmallVec<[(usize, LoweredValue); 4]>,
    source_index: usize,
    emitted: usize,
    limit: usize,
    outcome: Option<LoweredValue>,
    stopped: bool,
    finished: bool,
    span: Span,
    call_span: Span,
}

impl IndexedSerialPipeline {
    pub(super) fn new(
        evaluator: &mut Evaluator,
        execution: &FullExecution<'_>,
        input: LoweredValue,
        mut stages: SmallVec<[IndexedLiveSerialStage; 4]>,
        slots: &mut [LoweredValue],
        span: Span,
        call_span: Span,
    ) -> Result<ControlFlow<LoweredValue, Self>, RuntimeError> {
        let input = lowered_pipeline_input(input, span)?;
        for stage in &mut stages {
            match stage {
                IndexedLiveSerialStage::Drop { count_expr, count, .. }
                | IndexedLiveSerialStage::Take { count_expr, count } => {
                    match evaluator.eval_indexed_expr(execution, *count_expr, slots, span)? {
                        ControlFlow::Continue(value) => {
                            *count = lowered_nonnegative_count(value, span)?;
                        }
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    }
                }
                _ => {}
            }
        }
        let items = IndexedPipelineItems::new(evaluator, input, span)?;
        for stage in &stages {
            let name = Evaluator::indexed_stage_name(stage.tag());
            evaluator.trace_enter(
                TraceKind::StreamStageEnter,
                Some(span),
                Some(name),
                TracePayload::StreamStage {
                    stage: name.to_string(),
                    item_count: None,
                    error: None,
                },
            );
        }
        let limit = match stages.last() {
            Some(IndexedLiveSerialStage::Take { count, .. }) => *count,
            _ => usize::MAX,
        };
        Ok(ControlFlow::Continue(Self {
            items,
            stages,
            pending: SmallVec::new(),
            source_index: 0,
            emitted: 0,
            limit,
            outcome: None,
            stopped: false,
            finished: false,
            span,
            call_span,
        }))
    }

    pub(super) fn next(
        &mut self,
        evaluator: &mut Evaluator,
        execution: &FullExecution<'_>,
        slots: &mut [LoweredValue],
    ) -> Result<ControlFlow<LoweredValue, Option<LoweredValue>>, RuntimeError> {
        let span = self.span;
        let call_span = self.call_span;
        let block_header = Evaluator::indexed_block_header(slots.len());
        while !self.stopped && self.emitted < self.limit {
            if self.pending.is_empty() {
                let Some(item) = self.items.next(evaluator, span)? else {
                    return Ok(ControlFlow::Continue(None));
                };
                self.pending.push((0, item));
                self.source_index += 1;
            }
            let (stage_index, value) = self.pending.pop().expect("pending pipeline item");
            if stage_index == self.stages.len() {
                self.emitted += 1;
                return Ok(ControlFlow::Continue(Some(value)));
            }
            let stage = &mut self.stages[stage_index];
            let mut item = Some(value);
            let mut expanded = None;
            match stage {
                IndexedLiveSerialStage::Tee { slot, body } => {
                    let Some(value) = item.as_ref() else { continue };
                    slots[*slot] = value.clone();
                    let flow = evaluator.eval_indexed_statement_block(
                        execution, *body, &block_header, slots, call_span,
                    )?;
                    slots[*slot] = LoweredValue::Unit;
                    match flow {
                        StmtFlow::None | StmtFlow::Continue => {}
                        StmtFlow::Return(value) | StmtFlow::Propagate(value) => {
                            return Ok(ControlFlow::Break(value));
                        }
                        StmtFlow::Break(value) => {
                            return Ok(ControlFlow::Break(value.unwrap_or(LoweredValue::Unit)));
                        }
                    }
                }
                IndexedLiveSerialStage::Where { slot, predicate } => {
                    let Some(value) = item.take() else { continue };
                    slots[*slot] = value;
                    let keep = match evaluator.eval_indexed_bool(
                        execution, *predicate, slots, span,
                    )? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    };
                    let value = std::mem::replace(&mut slots[*slot], LoweredValue::Unit);
                    if keep {
                        item = Some(value);
                    }
                }
                IndexedLiveSerialStage::WhereBlock { slot, body, value } => {
                    let Some(input) = item.take() else { continue };
                    slots[*slot] = input;
                    let flow = evaluator.eval_indexed_statement_block(
                        execution, *body, &block_header, slots, call_span,
                    )?;
                    match flow {
                        StmtFlow::None => {}
                        StmtFlow::Return(value) | StmtFlow::Propagate(value) => {
                            return Ok(ControlFlow::Break(value));
                        }
                        StmtFlow::Break(_) | StmtFlow::Continue => {
                            let (kind, message) = if matches!(flow, StmtFlow::Break(_)) {
                                ("break-outside-loop", "break used outside loop")
                            } else {
                                ("continue-outside-loop", "continue used outside loop")
                            };
                            return Err(RuntimeError::new(kind, message).with_span(span));
                        }
                    }
                    let keep = match evaluator.eval_indexed_bool(execution, *value, slots, span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    };
                    let value = std::mem::replace(&mut slots[*slot], LoweredValue::Unit);
                    if keep {
                        item = Some(value);
                    }
                }
                IndexedLiveSerialStage::Map { slot, value, flat } => {
                    let Some(input) = item.take() else { continue };
                    slots[*slot] = input;
                    let projected = match evaluator.eval_indexed_expr(execution, *value, slots, span) {
                        Ok(ControlFlow::Continue(value)) => value,
                        Ok(ControlFlow::Break(value)) => return Ok(ControlFlow::Break(value)),
                        Err(error) => {
                            return Err(evaluator.stream_item_runtime_error("map", self.source_index - 1, error));
                        }
                    };
                    slots[*slot] = LoweredValue::Unit;
                    if *flat {
                        expanded = Some(evaluator.lowered_flat_map_rows(projected, span)?);
                    } else {
                        item = Some(projected);
                    }
                }
                IndexedLiveSerialStage::MapBlock { slot, body, value, flat } => {
                    let Some(input) = item.take() else { continue };
                    slots[*slot] = input;
                    let flow = evaluator.eval_indexed_statement_block(
                        execution, *body, &block_header, slots, call_span,
                    )?;
                    match flow {
                        StmtFlow::None => {}
                        StmtFlow::Return(value) | StmtFlow::Propagate(value) => {
                            return Ok(ControlFlow::Break(value));
                        }
                        StmtFlow::Break(_) | StmtFlow::Continue => {
                            let (kind, message) = if matches!(flow, StmtFlow::Break(_)) {
                                ("break-outside-loop", "break used outside loop")
                            } else {
                                ("continue-outside-loop", "continue used outside loop")
                            };
                            return Err(RuntimeError::new(kind, message).with_span(span));
                        }
                    }
                    let projected = match evaluator.eval_indexed_expr(execution, *value, slots, span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    };
                    slots[*slot] = LoweredValue::Unit;
                    if *flat {
                        expanded = Some(evaluator.lowered_flat_map_rows(projected, span)?);
                    } else {
                        item = Some(projected);
                    }
                }
                IndexedLiveSerialStage::Drop { count, seen, .. } => {
                    if item.is_some() && *seen < *count {
                        *seen += 1;
                        item = None;
                    }
                }
                IndexedLiveSerialStage::Enumerate { index } => {
                    if let Some(value) = item.take() {
                        item = Some(LoweredValue::Record(Arc::new(btree_map(vec![
                            (Arc::from("index"), LoweredValue::Int(*index as i64)),
                            (Arc::from("value"), value),
                        ]))));
                        *index += 1;
                    }
                }
                IndexedLiveSerialStage::Take { .. } => {
                    if let Some(value) = item.take() {
                        self.emitted += 1;
                        self.stopped = self.emitted >= self.limit;
                        return Ok(ControlFlow::Continue(Some(value)));
                    }
                }
                IndexedLiveSerialStage::Any { slot, predicate, all } => {
                    let Some(value) = item.take() else { continue };
                    slots[*slot] = value;
                    let keep = match evaluator.eval_indexed_bool(
                        execution, *predicate, slots, span,
                    )? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    };
                    slots[*slot] = LoweredValue::Unit;
                    if keep != *all {
                        self.outcome = Some(LoweredValue::Bool(!*all));
                        self.stopped = true;
                    }
                }
                IndexedLiveSerialStage::AnyBlock { slot, body, value, all } => {
                    let Some(input) = item.take() else { continue };
                    slots[*slot] = input;
                    let flow = evaluator.eval_indexed_statement_block(
                        execution, *body, &block_header, slots, call_span,
                    )?;
                    match flow {
                        StmtFlow::None => {}
                        StmtFlow::Return(value) | StmtFlow::Propagate(value) => {
                            return Ok(ControlFlow::Break(value));
                        }
                        StmtFlow::Break(_) | StmtFlow::Continue => {
                            let (kind, message) = if matches!(flow, StmtFlow::Break(_)) {
                                ("break-outside-loop", "break used outside loop")
                            } else {
                                ("continue-outside-loop", "continue used outside loop")
                            };
                            return Err(RuntimeError::new(kind, message).with_span(span));
                        }
                    }
                    let keep = match evaluator.eval_indexed_bool(execution, *value, slots, span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    };
                    slots[*slot] = LoweredValue::Unit;
                    if keep != *all {
                        self.outcome = Some(LoweredValue::Bool(!*all));
                        self.stopped = true;
                    }
                }
                IndexedLiveSerialStage::First => {
                    if let Some(value) = item.take() {
                        self.outcome = Some(lowered_result_ok(value));
                        self.stopped = true;
                    }
                }
                IndexedLiveSerialStage::Count => {
                    if item.take().is_some() {
                        self.emitted += 1;
                    }
                }
                IndexedLiveSerialStage::Collect => {
                    if let Some(value) = item.take() {
                        self.emitted += 1;
                        return Ok(ControlFlow::Continue(Some(value)));
                    }
                }
            }
            if let Some(expanded) = expanded {
                self.pending.extend(expanded.into_iter().rev().map(|row| (stage_index + 1, row)));
            } else if let Some(item) = item {
                self.pending.push((stage_index + 1, item));
            }
        }
        Ok(ControlFlow::Continue(None))
    }

    pub(super) fn finish(
        &mut self,
        evaluator: &mut Evaluator,
        error: Option<&RuntimeError>,
    ) -> Result<(), RuntimeError> {
        if self.finished {
            return Ok(());
        }
        self.finished = true;
        let cancel_result = self.items.cancel(evaluator, self.span);
        let trace_error = error
            .or_else(|| cancel_result.as_ref().err())
            .map(|error| TraceError::new(&error.kind, &error.message));
        for stage in self.stages.iter().rev() {
            let name = Evaluator::indexed_stage_name(stage.tag());
            evaluator.trace_exit(
                TraceKind::StreamStageExit,
                Some(self.span),
                Some(name),
                TracePayload::StreamStage {
                    stage: name.to_string(),
                    item_count: None,
                    error: trace_error.clone(),
                },
            );
        }
        cancel_result
    }

    fn into_result(self, output: Vec<LoweredValue>) -> LoweredValue {
        match self.stages.last() {
            Some(IndexedLiveSerialStage::Any { all, .. }
                | IndexedLiveSerialStage::AnyBlock { all, .. }) => {
                self.outcome.unwrap_or(LoweredValue::Bool(*all))
            }
            Some(IndexedLiveSerialStage::First) => self.outcome.unwrap_or_else(|| {
                lowered_result_err_value(
                    RuntimeError::new("empty-stream", "stream was empty").with_span(self.span),
                )
            }),
            Some(IndexedLiveSerialStage::Count) => LoweredValue::Int(self.emitted as i64),
            _ => LoweredValue::List(output),
        }
    }
}

// Decode stage shape before running the input. A `for` loop can keep this
// state across body executions without evaluating the input expression again.
pub(super) fn decode_serial_prefix(
    execution: &FullExecution<'_>,
    mut stages: FullPayload<'_>,
    stage_count: usize,
    span: Span,
) -> Result<SmallVec<[IndexedLiveSerialStage; 4]>, RuntimeError> {
    let mut prefix: SmallVec<[IndexedLiveSerialStage; 4]> = SmallVec::new();
    for _ in 0..stage_count {
        let stage = indexed_raw(&mut stages, span)?;
        let (tag, mut payload) = execution
            .stage_id(stage)
            .map_err(|error| indexed_error(error, span))?;
        let decoded = match tag {
            FullStageTag::Tee => {
                let slot = indexed_decode::<usize>(&mut payload, execution, span)?;
                let body = indexed_raw(&mut payload, span)?;
                indexed_finish(payload, span)?;
                IndexedLiveSerialStage::Tee { slot, body }
            }
            FullStageTag::Where => {
                let slot = indexed_decode::<usize>(&mut payload, execution, span)?;
                let predicate = indexed_raw(&mut payload, span)?;
                indexed_finish(payload, span)?;
                IndexedLiveSerialStage::Where { slot, predicate }
            }
            FullStageTag::WhereBlock => {
                let slot = indexed_decode::<usize>(&mut payload, execution, span)?;
                let body = indexed_raw(&mut payload, span)?;
                let value = indexed_raw(&mut payload, span)?;
                indexed_finish(payload, span)?;
                IndexedLiveSerialStage::WhereBlock { slot, body, value }
            }
            FullStageTag::Map | FullStageTag::FlatMap => {
                let slot = indexed_decode::<usize>(&mut payload, execution, span)?;
                let value = indexed_raw(&mut payload, span)?;
                indexed_finish(payload, span)?;
                IndexedLiveSerialStage::Map { slot, value, flat: tag == FullStageTag::FlatMap }
            }
            FullStageTag::MapBlock | FullStageTag::FlatMapBlock => {
                let slot = indexed_decode::<usize>(&mut payload, execution, span)?;
                let body = indexed_raw(&mut payload, span)?;
                let value = indexed_raw(&mut payload, span)?;
                indexed_finish(payload, span)?;
                IndexedLiveSerialStage::MapBlock { slot, body, value, flat: tag == FullStageTag::FlatMapBlock }
            }
            FullStageTag::Drop => {
                let count_expr = indexed_raw(&mut payload, span)?;
                indexed_finish(payload, span)?;
                IndexedLiveSerialStage::Drop { count_expr, count: 0, seen: 0 }
            }
            FullStageTag::Enumerate => {
                indexed_finish(payload, span)?;
                IndexedLiveSerialStage::Enumerate { index: 0 }
            }
            FullStageTag::Take => {
                let count_expr = indexed_raw(&mut payload, span)?;
                indexed_finish(payload, span)?;
                IndexedLiveSerialStage::Take { count_expr, count: 0 }
            }
            FullStageTag::Any | FullStageTag::All => {
                let slot = indexed_decode::<usize>(&mut payload, execution, span)?;
                let predicate = indexed_raw(&mut payload, span)?;
                indexed_finish(payload, span)?;
                IndexedLiveSerialStage::Any { slot, predicate, all: tag == FullStageTag::All }
            }
            FullStageTag::AnyBlock | FullStageTag::AllBlock => {
                let slot = indexed_decode::<usize>(&mut payload, execution, span)?;
                let body = indexed_raw(&mut payload, span)?;
                let value = indexed_raw(&mut payload, span)?;
                indexed_finish(payload, span)?;
                IndexedLiveSerialStage::AnyBlock {
                    slot, body, value, all: tag == FullStageTag::AllBlock,
                }
            }
            FullStageTag::First => {
                indexed_finish(payload, span)?;
                IndexedLiveSerialStage::First
            }
            FullStageTag::Count => {
                indexed_finish(payload, span)?;
                IndexedLiveSerialStage::Count
            }
            FullStageTag::Collect => {
                indexed_finish(payload, span)?;
                IndexedLiveSerialStage::Collect
            }
            _ => break,
        };
        prefix.push(decoded);
        if matches!(tag, FullStageTag::Take | FullStageTag::Any | FullStageTag::All | FullStageTag::AnyBlock | FullStageTag::AllBlock | FullStageTag::First | FullStageTag::Count | FullStageTag::Collect) {
            break;
        }
    }
    Ok(prefix)
}

impl Evaluator {
    // Keep serial stage effects in item order. Unsupported stages consume the
    // materialized prefix after this path has drained the live source.
    pub(super) fn eval_indexed_live_serial_prefix(
        &mut self,
        execution: &FullExecution<'_>,
        current: &mut LoweredValue,
        stages: FullPayload<'_>,
        stage_count: usize,
        slots: &mut [LoweredValue],
        span: Span,
        call_span: Span,
    ) -> Result<Option<(ControlFlow<LoweredValue, LoweredValue>, usize)>, RuntimeError> {
        if !matches!(current, LoweredValue::Stream(stream) if stream.source.is_some() || stream.script().is_some()) {
            return Ok(None);
        }
        let stages = decode_serial_prefix(execution, stages, stage_count, span)?;
        if stages.is_empty() {
            return Ok(None);
        }
        let consumed = stages.len();
        let input = std::mem::replace(current, LoweredValue::Unit);
        let mut pipeline = match IndexedSerialPipeline::new(
            self, execution, input, stages, slots, span, call_span,
        )? {
            ControlFlow::Continue(pipeline) => pipeline,
            ControlFlow::Break(value) => {
                return Ok(Some((ControlFlow::Break(value), consumed)));
            }
        };
        let mut output = if pipeline.limit == usize::MAX {
            Vec::new()
        } else {
            Vec::with_capacity(pipeline.limit.min(1024))
        };
        let driven = (|| -> Result<Option<LoweredValue>, RuntimeError> {
            loop {
                match pipeline.next(self, execution, slots)? {
                    ControlFlow::Continue(Some(value)) => output.push(value),
                    ControlFlow::Continue(None) => return Ok(None),
                    ControlFlow::Break(value) => return Ok(Some(value)),
                }
            }
        })();
        let close = pipeline.finish(self, driven.as_ref().err());
        match driven {
            Err(error) => {
                let _ = close;
                Err(error)
            }
            Ok(Some(value)) => {
                close?;
                Ok(Some((ControlFlow::Break(value), consumed)))
            }
            Ok(None) => {
                close?;
                Ok(Some((ControlFlow::Continue(pipeline.into_result(output)), consumed)))
            }
        }
    }
}
