use super::{
    Arc, BTreeMap, ControlFlow, DEFAULT_PAR_MAP_WORKERS, Evaluator, FullExecution,
    FullStageTag, FullTag,
    LoweredValue, RuntimeError, SmallVec, Span, StreamValue, TraceError, TraceKind,
    TracePayload, ReduceByOp, btree_map, bytes_module, compare_lowered_sort_keys,
    indexed_allocation_error, indexed_error, indexed_range_values, indexed_value,
    lowered_bytes_value, lowered_count_key, lowered_nonnegative_count, lowered_pipeline_input,
    lowered_pipeline_item_count, lowered_pipeline_record_list,
    lowered_reduce_fields_owned, lowered_reduce_group_insert, lowered_reduce_key_value_owned,
    lowered_result_err_value, lowered_result_ok, lowered_str_parts, lowered_str_value,
    lowered_str_view_value, lowered_table_print_value, lowered_value_argv_len,
    lowered_value_from_runtime_any,
};
use crate::runtime::eval::indexed::full::decode::ListView;
use crate::runtime::eval::indexed::full::schema::ExprView;
use crate::runtime::eval::indexed::full::{LoweredPipelineStage, StageView};
use super::pipeline::IndexedItemPredicate;
use crate::map_key::MapKey;

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
        if let LoweredValue::Stream(stream) = &current {
            evaluator.validate_stream_owner(stream, span)?;
        }
        match current {
            LoweredValue::Stream(mut stream)
                if stream.source.is_some() || stream.script().is_some() =>
            {
                let prefix = std::mem::take(&mut stream.items).into_iter();
                Ok(Self::Live { stream, prefix })
            }
            value => Ok(Self::Materialized(
                evaluator
                    .lowered_pipeline_input_items(value, span)?
                    .into_iter(),
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

    pub(super) fn cancel(
        &mut self,
        evaluator: &mut Evaluator,
        span: Span,
    ) -> Result<(), RuntimeError> {
        let escape = evaluator.pending_value_block_flow.take();
        let result = if let Self::Live { stream, .. } = self {
            evaluator.stream_cancel(stream, span)
        } else {
            Ok(())
        };
        if result.is_ok() {
            evaluator.pending_value_block_flow = escape;
        }
        result
    }

    fn visit_retained_values(
        &self,
        visitor: &mut dyn for<'a> FnMut(crate::runtime::eval::RetainedValueRef<'a>),
    ) {
        match self {
            Self::Materialized(items) => {
                crate::runtime::eval::visit_retained_lowered_values(items.as_slice(), visitor);
            }
            Self::Live { stream, prefix } => {
                visitor(crate::runtime::eval::RetainedValueRef::Stream(stream));
                for item in prefix.as_slice() {
                    visitor(crate::runtime::eval::RetainedValueRef::Runtime(&item.value));
                }
            }
        }
    }
}

fn indexed_batch_rows(
    evaluator: &mut Evaluator,
    input: LoweredValue,
    count: Option<usize>,
    max_bytes: Option<usize>,
    max_argv: Option<usize>,
    span: Span,
) -> Result<Vec<LoweredValue>, RuntimeError> {
    let mut items = IndexedPipelineItems::new(evaluator, input, span)?;
    let driven = (|| -> Result<Vec<LoweredValue>, RuntimeError> {
        let mut batches = Vec::new();
        let mut batch = Vec::new();
        let mut bytes = 0usize;
        let mut argv_bytes = 0usize;
        while let Some(item) = items.next(evaluator, span)? {
            let item_bytes = if max_bytes.is_some() || max_argv.is_some() {
                lowered_value_argv_len(&item)
            } else {
                0
            };
            if max_bytes.is_some_and(|limit| item_bytes > limit) {
                return Err(RuntimeError::new(
                    "argv-limit",
                    "batch item exceeds byte budget",
                )
                .with_span(span));
            }
            let argv_cost = item_bytes.saturating_add(usize::from(!batch.is_empty()));
            let full = count.is_some_and(|limit| batch.len() >= limit)
                || max_bytes.is_some_and(|limit| bytes.saturating_add(item_bytes) > limit)
                || max_argv.is_some_and(|limit| {
                    argv_bytes.saturating_add(argv_cost) > limit
                });
            if !batch.is_empty() && full {
                batches.push(LoweredValue::List(std::mem::take(&mut batch)));
                bytes = 0;
                argv_bytes = 0;
            }
            bytes = bytes.saturating_add(item_bytes);
            argv_bytes = argv_bytes
                .saturating_add(item_bytes)
                .saturating_add(usize::from(!batch.is_empty()));
            batch.push(item);
        }
        if !batch.is_empty() {
            batches.push(LoweredValue::List(batch));
        }
        Ok(batches)
    })();
    let close = items.cancel(evaluator, span);
    match driven {
        Ok(batches) => {
            close?;
            Ok(batches)
        }
        Err(error) => {
            let _ = close;
            Err(error)
        }
    }
}

#[derive(Clone)]
pub(super) struct IndexedLiveSerialStage {
    instruction: u32,
    tag: FullStageTag,
    state: IndexedSerialState,
    map_projection: Option<(Arc<str>, Span)>,
    where_predicate: Option<Arc<IndexedItemPredicate>>,
}

#[derive(Clone, Copy)]
enum IndexedSerialState {
    Materialized,
    Stateless,
    Drop { count: usize, seen: usize },
    Enumerate { index: usize },
    Take { count: usize },
    First,
    Count,
    Collect,
}

impl IndexedLiveSerialStage {
    fn ends_group(&self) -> bool {
        matches!(
            self.state,
            IndexedSerialState::Materialized
                | IndexedSerialState::Take { .. }
                | IndexedSerialState::First
                | IndexedSerialState::Count
                | IndexedSerialState::Collect
        ) || matches!(
            self.tag,
            FullStageTag::Any
                | FullStageTag::All
                | FullStageTag::AnyBlock
                | FullStageTag::AllBlock
                | FullStageTag::Each
        )
    }
}

// Only stages that produce another sequence can hand rows directly to a `for`
// body. A materializing or scalar terminal keeps the ordinary expression path.
pub(super) fn indexed_for_pipeline_input(
    execution: &FullExecution<'_>,
    instruction: u32,
    call_span: Span,
) -> Result<Option<(u32, Span, SmallVec<[IndexedLiveSerialStage; 4]>)>, RuntimeError> {
    let (tag, payload) = indexed_value(execution.instruction_id(instruction), call_span)?;
    if tag != FullTag::ExprPipeline {
        return Ok(None);
    }
    let ExprView::ListPipeline { input, stages, span } =
        ExprView::decode(execution, tag, payload).map_err(|error| indexed_error(error, call_span))?
    else {
        unreachable!("pipeline tag has a matching borrowed view")
    };
    let input = input.raw();
    let stage_count = stages.len();
    let decoded = decode_serial_prefix(execution, stages, span)?;
    if decoded.len() != stage_count
        || decoded.is_empty()
        || decoded
            .iter()
            .any(|stage| matches!(stage.state, IndexedSerialState::Materialized))
        || decoded.last().is_some_and(|stage| {
            matches!(
                stage.tag,
                FullStageTag::Any
                    | FullStageTag::All
                    | FullStageTag::AnyBlock
                    | FullStageTag::AllBlock
                    | FullStageTag::Each
                    | FullStageTag::First
                    | FullStageTag::Count
                    | FullStageTag::Collect
            )
        })
    {
        return Ok(None);
    }
    Ok(Some((input, span, decoded)))
}

pub(super) fn indexed_value_pipeline_input(
    execution: &FullExecution<'_>,
    instruction: u32,
    call_span: Span,
) -> Result<Option<(u32, Span, SmallVec<[IndexedLiveSerialStage; 4]>)>, RuntimeError> {
    let (tag, payload) = indexed_value(execution.instruction_id(instruction), call_span)?;
    if tag != FullTag::ExprPipeline {
        return Ok(None);
    }
    let ExprView::ListPipeline { input, stages, span } =
        ExprView::decode(execution, tag, payload).map_err(|error| indexed_error(error, call_span))?
    else {
        unreachable!("pipeline tag has a matching borrowed view")
    };
    let input = input.raw();
    let stage_count = stages.len();
    let decoded = decode_serial_complete(execution, stages, span)?;
    if decoded.len() != stage_count {
        return Ok(None);
    }
    Ok(Some((input, span, decoded)))
}

// The expression runner drains this cursor; a `for` frame keeps it between body
// executions. Both retain the source, stage counters, and flat-map expansion.
pub(super) struct IndexedSerialPipeline {
    items: Option<IndexedPipelineItems>,
    input: Option<LoweredValue>,
    stages: SmallVec<[IndexedLiveSerialStage; 4]>,
    preflight: SmallVec<[usize; 2]>,
    preflight_position: usize,
    awaiting_preflight: bool,
    pending: SmallVec<[(usize, LoweredValue); 4]>,
    source_index: usize,
    emitted: usize,
    limit: usize,
    outcome: Option<LoweredValue>,
    stopped: bool,
    finished: bool,
    traces_started: bool,
    span: Span,
    awaiting: Option<SerialAwait>,
    callback_result: Option<ControlFlow<LoweredValue, LoweredValue>>,
}

pub(super) struct IndexedStageEngine {
    current: Option<LoweredValue>,
    stages: SmallVec<[IndexedLiveSerialStage; 4]>,
    stage_position: usize,
    active_stage_count: usize,
    active: Option<Box<IndexedSerialPipeline>>,
    materialized: Option<Box<IndexedMaterializedStage>>,
    output: Vec<LoweredValue>,
    span: Span,
}

pub(super) enum IndexedStageStep {
    Expr(u32),
    Block(u32),
    Break(LoweredValue),
    Done(LoweredValue),
}

type IndexedMaterializedStep = IndexedStageStep;

enum IndexedKeyedStage {
    SortBy {
        slot: usize,
        key: u32,
        descending: bool,
        values: Vec<(LoweredValue, LoweredValue)>,
    },
    GroupBy {
        slot: usize,
        key: u32,
        groups: Vec<(LoweredValue, Vec<LoweredValue>)>,
    },
    CountBy {
        slot: usize,
        key: u32,
        counts: std::collections::BTreeMap<String, i64>,
    },
    UniqueBy {
        slot: usize,
        key: u32,
        seen: Vec<LoweredValue>,
        values: Vec<LoweredValue>,
    },
}

struct IndexedFoldStage {
    accumulator_slot: usize,
    item_slot: usize,
    body: u32,
    value: u32,
    accumulator: Option<LoweredValue>,
    awaiting_body: bool,
    awaiting_value: bool,
}

struct IndexedReduceByStage {
    item_slot: usize,
    body: u32,
    value: u32,
    op: Option<ReduceByOp>,
    configured: bool,
    prepared: bool,
    groups: BTreeMap<String, LoweredValue>,
    awaiting_body: bool,
    awaiting_value: bool,
}

struct IndexedParMapStage {
    slot: usize,
    body: Option<u32>,
    value: u32,
    flatten: Option<bool>,
    reduce: Option<(usize, u32, u32, ReduceByOp)>,
    jobs: Option<usize>,
    inputs: Option<Vec<LoweredValue>>,
    position: usize,
    results: Vec<LoweredValue>,
    awaiting_body: bool,
    awaiting_value: bool,
    active_item: Option<usize>,
}

impl IndexedKeyedStage {
    fn slot_and_key(&self) -> (usize, u32) {
        match self {
            Self::SortBy { slot, key, .. }
            | Self::GroupBy { slot, key, .. }
            | Self::CountBy { slot, key, .. }
            | Self::UniqueBy { slot, key, .. } => (*slot, *key),
        }
    }

    fn add(&mut self, key: LoweredValue, item: LoweredValue, span: Span) -> Result<(), RuntimeError> {
        match self {
            Self::SortBy { values, .. } => values.push((key, item)),
            Self::GroupBy { groups, .. } => {
                if let Some((_, values)) = groups.iter_mut().find(|(existing, _)| *existing == key) {
                    values.push(item);
                } else {
                    groups.push((key, vec![item]));
                }
            }
            Self::CountBy { counts, .. } => {
                let key = lowered_count_key(&key, span)?;
                *counts.entry(key).or_default() += 1;
            }
            Self::UniqueBy { seen, values, .. } => {
                if !seen.iter().any(|existing| existing == &key) {
                    seen.push(key);
                    values.push(item);
                }
            }
        }
        Ok(())
    }

    fn into_value(self, span: Span) -> Result<LoweredValue, RuntimeError> {
        match self {
            Self::SortBy {
                mut values,
                descending,
                ..
            } => {
                if let Some(key) = values
                    .iter()
                    .find(|(key, _)| !super::lowered_sort_key_orderable(key))
                    .map(|(key, _)| key)
                {
                    return Err(RuntimeError::new(
                        "stream-sort-key",
                        format!(
                            "sort-by keys must be Int, Str, Bool, Path, or Records of supported keys; found {}",
                            key.type_name()
                        ),
                    )
                    .with_span(span));
                }
                values.sort_by(|(left, _), (right, _)| compare_lowered_sort_keys(left, right));
                if descending {
                    values.reverse();
                }
                Ok(LoweredValue::List(
                    values.into_iter().map(|(_, item)| item).collect(),
                ))
            }
            Self::GroupBy { groups, .. } => Ok(LoweredValue::List(
                groups
                    .into_iter()
                    .map(|(key, items)| {
                        LoweredValue::Record(Arc::new(btree_map(vec![
                            (Arc::from("items"), LoweredValue::List(items)),
                            (Arc::from("key"), key),
                        ])))
                    })
                    .collect(),
            )),
            Self::CountBy { counts, .. } => Ok(LoweredValue::Map(Arc::new(
                counts
                    .into_iter()
                    .map(|(key, count)| (MapKey::from(key), LoweredValue::Int(count)))
                    .collect(),
            ))),
            Self::UniqueBy { values, .. } => Ok(LoweredValue::List(values)),
        }
    }

    fn visit_retained_values(
        &self,
        visitor: &mut dyn for<'a> FnMut(crate::runtime::eval::RetainedValueRef<'a>),
    ) {
        match self {
            Self::SortBy { values, .. } => {
                for (key, item) in values {
                    crate::runtime::eval::visit_retained_lowered_value(key, visitor);
                    crate::runtime::eval::visit_retained_lowered_value(item, visitor);
                }
            }
            Self::GroupBy { groups, .. } => {
                for (key, items) in groups {
                    crate::runtime::eval::visit_retained_lowered_value(key, visitor);
                    crate::runtime::eval::visit_retained_lowered_values(items, visitor);
                }
            }
            Self::CountBy { .. } => {}
            Self::UniqueBy { seen, values, .. } => {
                crate::runtime::eval::visit_retained_lowered_values(seen, visitor);
                crate::runtime::eval::visit_retained_lowered_values(values, visitor);
            }
        }
    }
}

struct IndexedMaterializedStage {
    tag: FullStageTag,
    input: Option<LoweredValue>,
    static_columns: Option<Option<Vec<String>>>,
    input_count: Option<usize>,
    arguments: SmallVec<[u32; 2]>,
    argument_position: usize,
    argument_values: SmallVec<[LoweredValue; 2]>,
    awaiting_argument: bool,
    callback_result: Option<ControlFlow<LoweredValue, LoweredValue>>,
    keyed: Option<IndexedKeyedStage>,
    key_projection: Option<(String, Span)>,
    fold: Option<IndexedFoldStage>,
    reduce: Option<IndexedReduceByStage>,
    par_map: Option<IndexedParMapStage>,
    items: Option<IndexedPipelineItems>,
    awaiting_key: bool,
    keyed_preflight_done: bool,
    span: Span,
    finished: bool,
}

impl IndexedMaterializedStage {
    fn new_frame(
        evaluator: &mut Evaluator,
        execution: &FullExecution<'_>,
        input: LoweredValue,
        instruction: u32,
        span: Span,
    ) -> Result<Self, RuntimeError> {
        let view = execution
            .stage_view(instruction)
            .map_err(|error| indexed_error(error, span))?;
        let tag = view.tag();
        let mut arguments = SmallVec::new();
        let mut static_columns = None;
        let mut keyed = None;
        let mut key_projection = None;
        let mut fold = None;
        let mut reduce = None;
        let mut par_map = None;
        match view {
            StageView::TextLines | StageView::JsonLines => {}
            StageView::BytesChunks { size } => arguments.push(size.raw()),
            StageView::BatchCount { count } => arguments.push(count.raw()),
            StageView::BatchMaxArgv { max_argv } => {
                if let Some(expression) = max_argv {
                    arguments.push(expression.raw());
                }
            }
            StageView::BatchMaxBytes { max_bytes } => arguments.push(max_bytes.raw()),
            StageView::BatchLimits { configuration } => arguments.push(configuration.raw()),
            StageView::Zip { other } => arguments.push(other.raw()),
            StageView::Sort { descending } => {
                if let Some(expression) = descending {
                    arguments.push(expression.raw());
                }
            }
            StageView::Shuffle { seed } => {
                if let Some(expression) = seed {
                    arguments.push(expression.raw());
                }
            }
            StageView::Repeat { count } => arguments.push(count.raw()),
            StageView::Range { start, end } => {
                arguments.push(start.raw());
                arguments.push(end.raw());
            }
            StageView::TablePrint { columns } => {
                static_columns = Some(match columns {
                    Some(columns) => Some(
                        columns
                            .map(|column| {
                                column
                                    .map(str::to_owned)
                                    .map_err(|error| indexed_error(error, span))
                            })
                            .collect::<Result<Vec<_>, _>>()?,
                    ),
                    None => None,
                });
            }
            StageView::TablePrintConfigured { columns } => {
                arguments.push(columns.raw());
            }
            StageView::SortBy { slot, key, descending } => {
                if let Some(descending) = descending {
                    arguments.push(descending.raw());
                }
                key_projection = Evaluator::indexed_field_projection_with_span(
                    execution,
                    key.raw(),
                    slot,
                    span,
                )?
                .map(|(field, field_span)| (field.to_owned(), field_span));
                keyed = Some(IndexedKeyedStage::SortBy {
                    slot,
                    key: key.raw(),
                    descending: false,
                    values: Vec::new(),
                });
            }
            StageView::GroupBy { slot, key } => {
                key_projection = Evaluator::indexed_field_projection_with_span(
                    execution,
                    key.raw(),
                    slot,
                    span,
                )?
                .map(|(field, field_span)| (field.to_owned(), field_span));
                keyed = Some(IndexedKeyedStage::GroupBy {
                    slot,
                    key: key.raw(),
                    groups: Vec::new(),
                });
            }
            StageView::CountBy { slot, key } => {
                keyed = Some(IndexedKeyedStage::CountBy {
                    slot,
                    key: key.raw(),
                    counts: std::collections::BTreeMap::new(),
                });
            }
            StageView::UniqueBy { slot, key } => {
                keyed = Some(IndexedKeyedStage::UniqueBy {
                    slot,
                    key: key.raw(),
                    seen: Vec::new(),
                    values: Vec::new(),
                });
            }
            StageView::Fold { acc_slot, item_slot, initial, body, value } => {
                arguments.push(initial.raw());
                fold = Some(IndexedFoldStage {
                    accumulator_slot: acc_slot,
                    item_slot,
                    body: body.block_id(),
                    value: value.raw(),
                    accumulator: None,
                    awaiting_body: false,
                    awaiting_value: false,
                });
            }
            StageView::ReduceBy { item_slot, body, value, op, jobs } => {
                if let Some(jobs) = jobs {
                    arguments.push(jobs.raw());
                }
                reduce = Some(IndexedReduceByStage {
                    item_slot,
                    body: body.block_id(),
                    value: value.raw(),
                    op: Some(op),
                    configured: false,
                    prepared: false,
                    groups: BTreeMap::new(),
                    awaiting_body: false,
                    awaiting_value: false,
                });
            }
            StageView::ReduceByConfigured { item_slot, body, value, configuration } => {
                arguments.push(configuration.raw());
                reduce = Some(IndexedReduceByStage {
                    item_slot,
                    body: body.block_id(),
                    value: value.raw(),
                    op: None,
                    configured: true,
                    prepared: false,
                    groups: BTreeMap::new(),
                    awaiting_body: false,
                    awaiting_value: false,
                });
            }
            StageView::ParMap { slot, jobs, value } => {
                if let Some(jobs) = jobs {
                    arguments.push(jobs.raw());
                }
                par_map = Some(IndexedParMapStage {
                    slot,
                    body: None,
                    value: value.raw(),
                    flatten: None,
                    reduce: None,
                    jobs: None,
                    inputs: None,
                    position: 0,
                    results: Vec::new(),
                    awaiting_body: false,
                    awaiting_value: false,
                    active_item: None,
                });
            }
            StageView::ParMapBlock { slot, body, jobs, value } => {
                if let Some(jobs) = jobs {
                    arguments.push(jobs.raw());
                }
                par_map = Some(IndexedParMapStage {
                    slot,
                    body: Some(body.block_id()),
                    value: value.raw(),
                    flatten: None,
                    reduce: None,
                    jobs: None,
                    inputs: None,
                    position: 0,
                    results: Vec::new(),
                    awaiting_body: false,
                    awaiting_value: false,
                    active_item: None,
                });
            }
            StageView::ParMapFlatMapReduceBy {
                slot,
                body,
                jobs,
                value,
                flatten,
                reduce_item_slot,
                reduce_body,
                reduce_value,
                op,
            } => {
                if let Some(jobs) = jobs {
                    arguments.push(jobs.raw());
                }
                par_map = Some(IndexedParMapStage {
                    slot,
                    body: body.map(|body| body.block_id()),
                    value: value.raw(),
                    flatten: Some(flatten),
                    reduce: Some((reduce_item_slot, reduce_body.block_id(), reduce_value.raw(), op)),
                    jobs: None,
                    inputs: None,
                    position: 0,
                    results: Vec::new(),
                    awaiting_body: false,
                    awaiting_value: false,
                    active_item: None,
                });
            }
            StageView::Sum | StageView::Last | StageView::Min | StageView::Max => {}
            _ => {
                return Err(RuntimeError::new(
                    "frame-scheduler",
                    format!("pipeline stage {:?} has no materializing frame handler", tag),
                )
                .with_span(span));
            }
        }
        let input_count = lowered_pipeline_item_count(&input);
        let name = Evaluator::indexed_stage_name(tag);
        evaluator.trace_enter(
            TraceKind::StreamStageEnter,
            Some(span),
            Some(name),
            TracePayload::StreamStage {
                stage: name.to_string(),
                item_count: input_count,
                error: None,
            },
        );
        Ok(Self {
            tag,
            input: Some(input),
            static_columns,
            input_count,
            arguments,
            argument_position: 0,
            argument_values: SmallVec::new(),
            awaiting_argument: false,
            callback_result: None,
            keyed,
            key_projection,
            fold,
            reduce,
            par_map,
            items: None,
            awaiting_key: false,
            keyed_preflight_done: false,
            span,
            finished: false,
        })
    }

    fn resume(&mut self, result: ControlFlow<LoweredValue, LoweredValue>) {
        assert!(
            self.awaiting_argument
                || self.awaiting_key
                || self.fold.as_ref().is_some_and(|fold| fold.awaiting_value)
                || self.reduce.as_ref().is_some_and(|reduce| reduce.awaiting_value)
                || self.par_map.as_ref().is_some_and(|par_map| par_map.awaiting_value),
            "stage callback has an expression continuation"
        );
        assert!(self.callback_result.replace(result).is_none());
    }

    fn visit_retained_values(
        &self,
        visitor: &mut dyn for<'a> FnMut(crate::runtime::eval::RetainedValueRef<'a>),
    ) {
        if let Some(input) = &self.input {
            crate::runtime::eval::visit_retained_lowered_value(input, visitor);
        }
        if let Some(items) = &self.items {
            items.visit_retained_values(visitor);
        }
        if let Some(keyed) = &self.keyed {
            keyed.visit_retained_values(visitor);
        }
        if let Some(fold) = &self.fold
            && let Some(accumulator) = &fold.accumulator
        {
            crate::runtime::eval::visit_retained_lowered_value(accumulator, visitor);
        }
        if let Some(reduce) = &self.reduce {
            crate::runtime::eval::visit_retained_lowered_values(
                reduce.groups.values(),
                visitor,
            );
        }
        if let Some(par_map) = &self.par_map {
            if let Some(inputs) = &par_map.inputs {
                crate::runtime::eval::visit_retained_lowered_values(inputs, visitor);
            }
            crate::runtime::eval::visit_retained_lowered_values(&par_map.results, visitor);
        }
        crate::runtime::eval::visit_retained_lowered_values(&self.argument_values, visitor);
    }

    fn pending_item_error(&self) -> Option<(&'static str, usize)> {
        self.par_map.as_ref().and_then(|par_map| {
            par_map
                .active_item
                .map(|item_index| ("par-map", item_index))
        })
    }

    fn next_frame(
        &mut self,
        evaluator: &mut Evaluator,
        execution: &FullExecution<'_>,
        slots: &mut [LoweredValue],
    ) -> Result<IndexedMaterializedStep, RuntimeError> {
        if self.awaiting_argument {
            self.awaiting_argument = false;
            match self
                .callback_result
                .take()
                .expect("stage argument expression has resumed")
            {
                ControlFlow::Continue(value) => self.argument_values.push(value),
                ControlFlow::Break(value) => return Ok(IndexedMaterializedStep::Break(value)),
            }
            self.argument_position += 1;
        }
        if let Some(expression) = self.arguments.get(self.argument_position).copied() {
            if self.tag == FullStageTag::BytesChunks
                && self.argument_position == 0
                && lowered_bytes_value(
                    self.input
                        .as_ref()
                        .expect("bytes stage input remains through preflight"),
                )
                .is_none()
            {
                let input = self.input.as_ref().expect("checked above");
                return Err(RuntimeError::new(
                    "type-error",
                    format!("bytes.chunks expected Bytes, found {}", input.type_name()),
                )
                .with_span(self.span));
            }
            self.awaiting_argument = true;
            return Ok(IndexedMaterializedStep::Expr(expression));
        }
        if self.keyed.is_some() {
            if !self.keyed_preflight_done {
                if let Some(IndexedKeyedStage::SortBy { descending, .. }) = &mut self.keyed {
                    if let Some(value) = self.argument_values.first() {
                        let LoweredValue::Bool(value) = value else {
                            return Err(RuntimeError::new(
                                "type-error",
                                format!("desc expected Bool, found {}", value.type_name()),
                            )
                            .with_span(self.span));
                        };
                        *descending = *value;
                    }
                }
                self.keyed_preflight_done = true;
            }
            return self.next_keyed(evaluator, slots);
        }
        if self.fold.is_some() {
            return self.next_fold(evaluator, slots);
        }
        if self.reduce.is_some() {
            return self.next_reduce(evaluator, slots);
        }
        if self.par_map.is_some() {
            return self.next_par_map(evaluator, execution, slots);
        }
        if self.tag == FullStageTag::BytesChunks
            && self.argument_position == 0
            && lowered_bytes_value(
                self.input
                    .as_ref()
                    .expect("bytes stage input remains through preflight"),
            )
            .is_none()
        {
            let input = self.input.as_ref().expect("checked above");
            return Err(RuntimeError::new(
                "type-error",
                format!("bytes.chunks expected Bytes, found {}", input.type_name()),
            )
            .with_span(self.span));
        }
        let value = self.apply(evaluator, execution, slots)?;
        Ok(IndexedMaterializedStep::Done(value))
    }

    fn next_keyed(
        &mut self,
        evaluator: &mut Evaluator,
        slots: &mut [LoweredValue],
    ) -> Result<IndexedMaterializedStep, RuntimeError> {
        if self.awaiting_key {
            self.awaiting_key = false;
            let key = match self
                .callback_result
                .take()
                .expect("key expression has resumed")
            {
                ControlFlow::Continue(key) => key,
                ControlFlow::Break(value) => return Ok(IndexedMaterializedStep::Break(value)),
            };
            let (slot, _) = self
                .keyed
                .as_ref()
                .expect("keyed stage remains active")
                .slot_and_key();
            let item = std::mem::replace(&mut slots[slot], LoweredValue::Unit);
            self.keyed
                .as_mut()
                .expect("keyed stage remains active")
                .add(key, item, self.span)?;
        }
        if self.items.is_none() {
            let input = self.input.take().expect("keyed stage input is evaluated once");
            self.items = Some(IndexedPipelineItems::new(evaluator, input, self.span)?);
        }
        loop {
            let next = self
                .items
                .as_mut()
                .expect("keyed stage input initialized")
                .next(evaluator, self.span)?;
            let Some(item) = next else {
                let mut items = self.items.take().expect("keyed source remains active");
                items.cancel(evaluator, self.span)?;
                let keyed = self.keyed.take().expect("keyed accumulator remains active");
                return Ok(IndexedMaterializedStep::Done(
                    keyed.into_value(self.span)?,
                ));
            };
            let (slot, key) = self
                .keyed
                .as_ref()
                .expect("keyed stage remains active")
                .slot_and_key();
            if let Some((field, field_span)) = self.key_projection.as_ref()
                && let Some(key_value) =
                    evaluator.indexed_borrowed_field_value(&item, field, *field_span)?
            {
                self.keyed
                    .as_mut()
                    .expect("keyed stage remains active")
                    .add(key_value, item, self.span)?;
                continue;
            }
            slots[slot] = item;
            self.awaiting_key = true;
            return Ok(IndexedMaterializedStep::Expr(key));
        }
    }

    fn next_fold(
        &mut self,
        evaluator: &mut Evaluator,
        slots: &mut [LoweredValue],
    ) -> Result<IndexedMaterializedStep, RuntimeError> {
        if let Some(fold) = &mut self.fold {
            if fold.awaiting_body {
                fold.awaiting_body = false;
                fold.awaiting_value = true;
                return Ok(IndexedMaterializedStep::Expr(fold.value));
            }
            if fold.awaiting_value {
                fold.awaiting_value = false;
                match self.callback_result.take().expect("fold value has resumed") {
                    ControlFlow::Continue(accumulator) => {
                        fold.accumulator = Some(accumulator);
                    }
                    ControlFlow::Break(value) => {
                        if let Some(mut items) = self.items.take() {
                            items.cancel(evaluator, self.span)?;
                        }
                        slots[fold.accumulator_slot] = LoweredValue::Unit;
                        slots[fold.item_slot] = LoweredValue::Unit;
                        return Ok(IndexedMaterializedStep::Break(value));
                    }
                }
            }
        }
        if self
            .fold
            .as_ref()
            .is_some_and(|fold| fold.accumulator.is_none())
        {
            let accumulator = std::mem::replace(
                self.argument_values
                    .first_mut()
                    .expect("fold initializer was evaluated"),
                LoweredValue::Unit,
            );
            self.fold
                .as_mut()
                .expect("fold state remains active")
                .accumulator = Some(accumulator);
        }
        if self.items.is_none() {
            let input = self.input.take().expect("fold input is evaluated once");
            self.items = Some(IndexedPipelineItems::new(evaluator, input, self.span)?);
        }
        let next = self
            .items
            .as_mut()
            .expect("fold source initialized")
            .next(evaluator, self.span)?;
        let Some(item) = next else {
            let mut items = self.items.take().expect("fold source remains active");
            items.cancel(evaluator, self.span)?;
            let fold = self.fold.as_mut().expect("fold state remains active");
            slots[fold.accumulator_slot] = LoweredValue::Unit;
            slots[fold.item_slot] = LoweredValue::Unit;
            return Ok(IndexedMaterializedStep::Done(
                fold.accumulator.take().expect("fold initialized its accumulator"),
            ));
        };
        let fold = self.fold.as_mut().expect("fold state remains active");
        slots[fold.accumulator_slot] = fold
            .accumulator
            .as_ref()
            .expect("fold initialized its accumulator")
            .clone();
        slots[fold.item_slot] = item;
        fold.awaiting_body = true;
        Ok(IndexedMaterializedStep::Block(fold.body))
    }

    fn next_reduce(
        &mut self,
        evaluator: &mut Evaluator,
        slots: &mut [LoweredValue],
    ) -> Result<IndexedMaterializedStep, RuntimeError> {
        if let Some(reduce) = &mut self.reduce {
            if reduce.awaiting_body {
                reduce.awaiting_body = false;
                reduce.awaiting_value = true;
                return Ok(IndexedMaterializedStep::Expr(reduce.value));
            }
            if reduce.awaiting_value {
                reduce.awaiting_value = false;
                let output = match self
                    .callback_result
                    .take()
                    .expect("reduce-by value has resumed")
                {
                    ControlFlow::Continue(output) => output,
                    ControlFlow::Break(value) => {
                        if let Some(mut items) = self.items.take() {
                            items.cancel(evaluator, self.span)?;
                        }
                        slots[reduce.item_slot] = LoweredValue::Unit;
                        return Ok(IndexedMaterializedStep::Break(value));
                    }
                };
                let (key, value) = lowered_reduce_fields_owned(output, "key", "value", self.span)?;
                let key = lowered_reduce_key_value_owned(key, self.span)?;
                lowered_reduce_group_insert(
                    &mut reduce.groups,
                    key,
                    value,
                    reduce.op.expect("reduce-by operation was prepared"),
                    self.span,
                )?;
                slots[reduce.item_slot] = LoweredValue::Unit;
            }
            if !reduce.prepared {
                if reduce.configured {
                    let configuration = self
                        .argument_values
                        .first()
                        .expect("configured reduce-by argument was evaluated");
                    let fields = match configuration {
                        LoweredValue::Record(fields) => Arc::clone(fields),
                        LoweredValue::RecordVec(fields) => Arc::new(
                            fields
                                .iter()
                                .map(|(name, value)| {
                                    (Arc::<str>::from(name.as_str().as_str()), value.clone())
                                })
                                .collect(),
                        ),
                        _ => {
                            return Err(RuntimeError::new(
                                "indexed-ir",
                                "stage configuration must be a record",
                            )
                            .with_span(self.span));
                        }
                    };
                    let mut selected = None;
                    for (name, mode) in [
                        ("sum", ReduceByOp::Sum),
                        ("min", ReduceByOp::Min),
                        ("max", ReduceByOp::Max),
                    ] {
                        match fields.get(name) {
                            Some(LoweredValue::Bool(true)) => {
                                if selected.replace(mode).is_some() {
                                    return Err(RuntimeError::new(
                                        "stream-reduce-mode",
                                        "reduce-by requires exactly one enabled reduction mode",
                                    )
                                    .with_span(self.span));
                                }
                            }
                            None | Some(LoweredValue::Bool(false)) => {}
                            _ => {
                                return Err(RuntimeError::new(
                                    "type-error",
                                    "reduction modes must be Bool",
                                )
                                .with_span(self.span));
                            }
                        }
                    }
                    if let Some(jobs) = fields.get("jobs")
                        && !matches!(jobs, LoweredValue::Int(value) if *value > 0)
                    {
                        return Err(RuntimeError::new(
                            "stream-jobs",
                            "stream worker count must be a positive Int",
                        )
                        .with_span(self.span));
                    }
                    reduce.op = Some(selected.ok_or_else(|| {
                        RuntimeError::new(
                            "stream-reduce-mode",
                            "reduce-by requires exactly one enabled reduction mode",
                        )
                        .with_span(self.span)
                    })?);
                } else if let Some(jobs) = self.argument_values.first() {
                    match jobs {
                        LoweredValue::Int(value) if *value > 0 => {}
                        LoweredValue::Int(_) => {
                            return Err(RuntimeError::new(
                                "stream-jobs",
                                "stream worker count must be positive",
                            )
                            .with_span(self.span));
                        }
                        value => {
                            return Err(RuntimeError::new(
                                "type-error",
                                format!(
                                    "stream worker count expected Int, found {}",
                                    value.type_name()
                                ),
                            )
                            .with_span(self.span));
                        }
                    }
                }
                reduce.prepared = true;
            }
        }
        if self.items.is_none() {
            let input = self
                .input
                .take()
                .expect("reduce-by input is evaluated once");
            self.items = Some(IndexedPipelineItems::new(evaluator, input, self.span)?);
        }
        let next = self
            .items
            .as_mut()
            .expect("reduce-by source initialized")
            .next(evaluator, self.span)?;
        let Some(item) = next else {
            let mut items = self
                .items
                .take()
                .expect("reduce-by source remains active");
            items.cancel(evaluator, self.span)?;
            let reduce = self.reduce.as_mut().expect("reduce-by state remains active");
            slots[reduce.item_slot] = LoweredValue::Unit;
            let values = std::mem::take(&mut reduce.groups)
                .into_iter()
                .map(|(key, value)| (MapKey::from(key), value))
                .collect();
            return Ok(IndexedMaterializedStep::Done(LoweredValue::Map(Arc::new(values))));
        };
        let reduce = self.reduce.as_mut().expect("reduce-by state remains active");
        slots[reduce.item_slot] = item;
        reduce.awaiting_body = true;
        Ok(IndexedMaterializedStep::Block(reduce.body))
    }

    fn next_par_map(
        &mut self,
        evaluator: &mut Evaluator,
        execution: &FullExecution<'_>,
        slots: &mut [LoweredValue],
    ) -> Result<IndexedMaterializedStep, RuntimeError> {
        let par_map = self.par_map.as_mut().expect("par-map state remains active");
        if par_map.awaiting_body {
            par_map.awaiting_body = false;
            par_map.awaiting_value = true;
            return Ok(IndexedMaterializedStep::Expr(par_map.value));
        }
        if par_map.awaiting_value {
            par_map.awaiting_value = false;
            let value = match self
                .callback_result
                .take()
                .expect("par-map value has resumed")
            {
                ControlFlow::Continue(value) => value,
                ControlFlow::Break(value) => {
                    slots[par_map.slot] = LoweredValue::Unit;
                    par_map.active_item = None;
                    return Ok(IndexedMaterializedStep::Break(value));
                }
            };
            slots[par_map.slot] = LoweredValue::Unit;
            let item_index = par_map.active_item.take().expect("par-map item is active");
            if evaluator.trace_enabled {
                evaluator.trace_lowered_parallel_job(
                    TraceKind::ParallelJobEnd,
                    "par-map",
                    item_index,
                    None,
                    self.span,
                );
            }
            par_map.results.push(value);
            par_map.position += 1;
        }
        if par_map.inputs.is_none() {
            if let Some(value) = self.argument_values.first() {
                match value {
                    LoweredValue::Int(value) if *value > 0 => {
                        par_map.jobs = Some(*value as usize);
                    }
                    LoweredValue::Int(_) => {
                        return Err(RuntimeError::new(
                            "stream-jobs",
                            "stream worker count must be positive",
                        )
                        .with_span(self.span));
                    }
                    value => {
                        return Err(RuntimeError::new(
                            "type-error",
                            format!(
                                "stream worker count expected Int, found {}",
                                value.type_name()
                            ),
                        )
                        .with_span(self.span));
                    }
                }
            }
            let input = self.input.take().expect("par-map input is evaluated once");
            par_map.inputs = Some(evaluator.lowered_pipeline_input_items(input, self.span)?);
            let jobs = par_map.jobs.unwrap_or_else(|| {
                std::thread::available_parallelism()
                    .map_or(1, |count| count.get().min(DEFAULT_PAR_MAP_WORKERS))
            });
            let item_count = par_map.inputs.as_ref().expect("par-map input collected").len();
            if par_map.flatten.is_some()
                || !evaluator.trace_enabled && jobs > 1 && item_count > 1
            {
                let inputs = par_map.inputs.take().expect("par-map input was collected");
                let output = if let Some(flatten) = par_map.flatten {
                    let Some((reduce_slot, reduce_body, reduce_value, op)) = par_map.reduce else {
                        unreachable!("fused stage has reduce metadata")
                    };
                    if inputs.is_empty() {
                        LoweredValue::Map(Arc::new(BTreeMap::new()))
                    } else {
                        evaluator.eval_indexed_par_map_flat_map_reduce_by(
                            execution,
                            par_map.body,
                            par_map.value,
                            flatten,
                            reduce_slot,
                            reduce_body,
                            reduce_value,
                            op,
                            slots,
                            par_map.slot,
                            inputs,
                            jobs,
                            self.span,
                        )?
                    }
                } else {
                    let values = if inputs.is_empty() {
                        Vec::new()
                    } else {
                        evaluator.eval_indexed_par_map_parallel(
                            execution,
                            par_map.body,
                            par_map.value,
                            slots,
                            par_map.slot,
                            inputs,
                            jobs,
                            self.span,
                        )?
                    };
                    LoweredValue::List(values)
                };
                if let Some(flow) = evaluator.pending_value_block_flow.take() {
                    return Ok(match evaluator.preserve_lexical_expression_flow::<()>(flow) {
                        ControlFlow::Break(value) => IndexedMaterializedStep::Break(value),
                        ControlFlow::Continue(()) => unreachable!(),
                    });
                }
                return Ok(IndexedMaterializedStep::Done(output));
            }
        }
        let input_len = par_map
            .inputs
            .as_ref()
            .expect("par-map input was collected")
            .len();
        if par_map.position == input_len {
            return if let Some(_) = par_map.flatten {
                Err(RuntimeError::new(
                    "frame-scheduler",
                    "sequential fused par-map path requires a worker stage",
                )
                .with_span(self.span))
            } else {
                Ok(IndexedMaterializedStep::Done(LoweredValue::List(
                    std::mem::take(&mut par_map.results),
                )))
            };
        }
        let item_index = par_map.position;
        let item = par_map
            .inputs
            .as_mut()
            .expect("par-map input was collected")
            .get_mut(item_index)
            .map(|item| std::mem::replace(item, LoweredValue::Unit))
            .expect("par-map item index is in bounds");
        slots[par_map.slot] = item;
        par_map.active_item = Some(item_index);
        if evaluator.trace_enabled {
            evaluator.trace_lowered_parallel_job(
                TraceKind::ParallelJobStart,
                "par-map",
                item_index,
                None,
                self.span,
            );
        }
        if let Some(body) = par_map.body {
            par_map.awaiting_body = true;
            Ok(IndexedMaterializedStep::Block(body))
        } else {
            par_map.awaiting_value = true;
            Ok(IndexedMaterializedStep::Expr(par_map.value))
        }
    }

    fn apply(
        &mut self,
        evaluator: &mut Evaluator,
        _execution: &FullExecution<'_>,
        _slots: &mut [LoweredValue],
    ) -> Result<LoweredValue, RuntimeError> {
        let input = self.input.take().expect("stage input is evaluated once");
        let span = self.span;
        match self.tag {
            FullStageTag::TextLines => {
                let Some((text, start, end)) = lowered_str_parts(&input) else {
                    return Err(RuntimeError::new(
                        "type-error",
                        format!("text.lines expected Str, found {}", input.type_name()),
                    )
                    .with_span(span));
                };
                let bytes = text.as_bytes();
                let mut cursor = start;
                let mut lines = Vec::new();
                while cursor < end {
                    let newline = bytes[cursor..end]
                        .iter()
                        .position(|byte| *byte == b'\n')
                        .map(|offset| cursor + offset);
                    let line_end = newline.unwrap_or(end);
                    let view_end = if line_end > cursor && bytes[line_end - 1] == b'\r' {
                        line_end - 1
                    } else {
                        line_end
                    };
                    lines.push(lowered_str_view_value(text.clone(), cursor, view_end));
                    let Some(newline) = newline else { break };
                    cursor = newline + 1;
                }
                Ok(LoweredValue::List(lines))
            }
            FullStageTag::JsonLines => {
                let Some(text) = lowered_str_value(&input) else {
                    return Err(RuntimeError::new(
                        "type-error",
                        format!("json.lines expected Str, found {}", input.type_name()),
                    )
                    .with_span(span));
                };
                let values = crate::modules::json::parse_json_lines(text, span)?;
                let mut lowered = Vec::with_capacity(values.len());
                for value in values {
                    let Some(value) = lowered_value_from_runtime_any(&value) else {
                        return Err(RuntimeError::new(
                            "type-error",
                            format!("json.lines produced unsupported {}", value.type_name()),
                        )
                        .with_span(span));
                    };
                    lowered.push(value);
                }
                Ok(LoweredValue::List(lowered))
            }
            FullStageTag::BytesChunks => {
                let LoweredValue::Int(size) = &self.argument_values[0] else {
                    return Err(RuntimeError::new(
                        "type-error",
                        format!(
                            "bytes.chunks size expected Int, found {}",
                            self.argument_values[0].type_name()
                        ),
                    )
                    .with_span(span));
                };
                if *size <= 0 {
                    return Err(RuntimeError::new("bytes-chunks", "chunk size must be positive")
                        .with_span(span));
                }
                let bytes = lowered_bytes_value(&input)
                    .expect("bytes.chunks input type checked before its argument")
                    .to_vec();
                let chunks = bytes_module::chunks(bytes, *size, span)?;
                let mut lowered = Vec::with_capacity(chunks.len());
                for chunk in chunks {
                    let Some(chunk) = lowered_value_from_runtime_any(&chunk) else {
                        return Err(RuntimeError::new(
                            "type-error",
                            format!("bytes.chunks produced unsupported {}", chunk.type_name()),
                        )
                        .with_span(span));
                    };
                    lowered.push(chunk);
                }
                Ok(LoweredValue::List(lowered))
            }
            FullStageTag::Zip => {
                let other = std::mem::replace(
                    &mut self.argument_values[0],
                    LoweredValue::Unit,
                );
                let right = evaluator.lowered_pipeline_input_items(other, span)?;
                let mut left = IndexedPipelineItems::new(evaluator, input, span)?;
                let driven = (|| -> Result<Vec<LoweredValue>, RuntimeError> {
                    let mut pairs = Vec::with_capacity(right.len());
                    for right in right {
                        let Some(value) = left.next(evaluator, span)? else { break };
                        pairs.push(LoweredValue::Record(Arc::new(btree_map(vec![
                            (Arc::from("left"), value),
                            (Arc::from("right"), right),
                        ]))));
                    }
                    Ok(pairs)
                })();
                let close = left.cancel(evaluator, span);
                match driven {
                    Ok(pairs) => {
                        close?;
                        Ok(LoweredValue::List(pairs))
                    }
                    Err(error) => {
                        let _ = close;
                        Err(error)
                    }
                }
            }
            FullStageTag::BatchCount => {
                let count = match self.argument_values.first() {
                    Some(LoweredValue::Int(value)) if *value > 0 => *value as usize,
                    Some(LoweredValue::Int(_)) => {
                        return Err(RuntimeError::new(
                            "stream-stage-option",
                            "count must be positive",
                        )
                        .with_span(span));
                    }
                    Some(value) => {
                        return Err(RuntimeError::new(
                            "type-error",
                            format!("count expected Int, found {}", value.type_name()),
                        )
                        .with_span(span));
                    }
                    None => unreachable!("batch count preflight is decoded"),
                };
                let rows = indexed_batch_rows(evaluator, input, Some(count), None, None, span)?;
                Ok(LoweredValue::List(rows))
            }
            FullStageTag::BatchMaxArgv | FullStageTag::BatchMaxBytes => {
                let limit = match (self.tag, self.argument_values.first()) {
                    (FullStageTag::BatchMaxArgv, None) => {
                        super::super::super::stream::platform_arg_max()
                            .saturating_sub(4096)
                            .clamp(1, 128 * 1024)
                    }
                    (_, Some(value)) => lowered_nonnegative_count(value.clone(), span)?,
                    _ => unreachable!("batch limit expression is decoded"),
                };
                let max_argv = (self.tag == FullStageTag::BatchMaxArgv).then_some(limit);
                let max_bytes = (self.tag == FullStageTag::BatchMaxBytes).then_some(limit);
                let rows = indexed_batch_rows(evaluator, input, None, max_bytes, max_argv, span)?;
                Ok(LoweredValue::List(rows))
            }
            FullStageTag::BatchLimits => {
                let configuration = match self.argument_values.first() {
                    Some(LoweredValue::Record(fields)) => fields.clone(),
                    Some(LoweredValue::RecordVec(fields)) => Arc::new(
                        fields
                            .iter()
                            .map(|(name, value)| {
                                (Arc::<str>::from(name.as_str().as_str()), value.clone())
                            })
                            .collect(),
                    ),
                    _ => {
                        return Err(RuntimeError::new(
                            "indexed-ir",
                            "stage configuration must be a record",
                        )
                        .with_span(span));
                    }
                };
                let positive_limit = |name: &str| -> Result<Option<usize>, RuntimeError> {
                    match configuration.get(name) {
                        None => Ok(None),
                        Some(LoweredValue::Int(value)) if *value > 0 => Ok(Some(*value as usize)),
                        _ => Err(RuntimeError::new(
                            "stream-batch",
                            format!("batch {name} must be a positive Int"),
                        )
                        .with_span(span)),
                    }
                };
                let count = positive_limit("count")?;
                let max_bytes = positive_limit("max_bytes")?;
                let max_argv = match configuration.get("max_argv") {
                    None | Some(LoweredValue::Bool(false)) => None,
                    Some(LoweredValue::Bool(true)) => Some(
                        super::super::super::stream::platform_arg_max()
                            .saturating_sub(4096)
                            .clamp(1, 128 * 1024),
                    ),
                    _ => {
                        return Err(RuntimeError::new("type-error", "batch max_argv must be Bool")
                            .with_span(span));
                    }
                };
                if count.is_none() && max_bytes.is_none() && max_argv.is_none() {
                    return Err(RuntimeError::new(
                        "stream-batch",
                        "batch requires an enabled limit",
                    )
                    .with_span(span));
                }
                let rows = indexed_batch_rows(evaluator, input, count, max_bytes, max_argv, span)?;
                Ok(LoweredValue::List(rows))
            }
            FullStageTag::TablePrint | FullStageTag::TablePrintConfigured => {
                let columns = if self.tag == FullStageTag::TablePrintConfigured {
                    let values = evaluator.lowered_pipeline_input_items(
                        std::mem::replace(&mut self.argument_values[0], LoweredValue::Unit),
                        span,
                    )?;
                    Some(
                        values
                            .into_iter()
                            .map(|value| match value {
                                LoweredValue::Str(text) => Ok(text.to_string()),
                                _ => Err(RuntimeError::new(
                                    "type-error",
                                    "table columns must be Str",
                                )
                                .with_span(span)),
                            })
                            .collect::<Result<Vec<_>, _>>()?,
                    )
                } else {
                    self.static_columns
                        .clone()
                        .expect("static table columns are decoded")
                };
                indexed_table_print(evaluator, input, columns, span)
            }
            FullStageTag::Sort => {
                let descending = self
                    .argument_values
                    .first()
                    .map(|value| match value {
                        LoweredValue::Bool(value) => Ok(*value),
                        value => Err(RuntimeError::new(
                            "type-error",
                            format!("desc expected Bool, found {}", value.type_name()),
                        )
                        .with_span(span)),
                    })
                    .transpose()?
                    .unwrap_or(false);
                let mut items = evaluator.lowered_pipeline_input_items(input, span)?;
                items.sort_by(compare_lowered_sort_keys);
                if descending {
                    items.reverse();
                }
                Ok(LoweredValue::List(items))
            }
            FullStageTag::Shuffle => {
                let seed = match self.argument_values.first() {
                    None => 0,
                    Some(LoweredValue::Int(value)) => *value as u64,
                    Some(value) => {
                        return Err(RuntimeError::new(
                            "type-error",
                            format!("shuffle seed expected Int, found {}", value.type_name()),
                        )
                        .with_span(span));
                    }
                };
                let mut items = evaluator.lowered_pipeline_input_items(input, span)?;
                let mut state = seed ^ (items.len() as u64).wrapping_mul(0x9e3779b97f4a7c15);
                for index in (1..items.len()).rev() {
                    state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                    let swap = (state as usize) % (index + 1);
                    items.swap(index, swap);
                }
                Ok(LoweredValue::List(items))
            }
            FullStageTag::Repeat => {
                let count = lowered_nonnegative_count(
                    std::mem::replace(&mut self.argument_values[0], LoweredValue::Unit),
                    span,
                )?;
                let items = if count == 0 {
                    let mut items = IndexedPipelineItems::new(evaluator, input, span)?;
                    items.cancel(evaluator, span)?;
                    Vec::new()
                } else {
                    evaluator.lowered_pipeline_input_items(input, span)?
                };
                let capacity = items
                    .len()
                    .checked_mul(count)
                    .ok_or_else(|| indexed_allocation_error("repeat", span))?;
                let mut repeated = Vec::new();
                repeated
                    .try_reserve_exact(capacity)
                    .map_err(|_| indexed_allocation_error("repeat", span))?;
                for _ in 0..count {
                    repeated.extend(items.iter().cloned());
                }
                Ok(LoweredValue::List(repeated))
            }
            FullStageTag::Range => {
                let [LoweredValue::Int(start), LoweredValue::Int(end)] =
                    self.argument_values.as_slice()
                else {
                    let index = self
                        .argument_values
                        .iter()
                        .position(|value| !matches!(value, LoweredValue::Int(_)))
                        .unwrap_or(0);
                    let label = if index == 0 { "start" } else { "end" };
                    return Err(RuntimeError::new(
                        "type-error",
                        format!(
                            "range {label} expected Int, found {}",
                            self.argument_values[index].type_name()
                        ),
                    )
                    .with_span(span));
                };
                Ok(LoweredValue::List(indexed_range_values(*start, *end, span)?))
            }
            FullStageTag::Sum => {
                let mut items = IndexedPipelineItems::new(evaluator, input, span)?;
                let driven = (|| -> Result<i64, RuntimeError> {
                    let mut sum = 0i64;
                    while let Some(item) = items.next(evaluator, span)? {
                        let LoweredValue::Int(value) = item else {
                            return Err(RuntimeError::new("type-error", "sum expected Int stream")
                                .with_span(span));
                        };
                        sum += value;
                    }
                    Ok(sum)
                })();
                let close = items.cancel(evaluator, span);
                match driven {
                    Ok(sum) => {
                        close?;
                        Ok(LoweredValue::Int(sum))
                    }
                    Err(error) => {
                        let _ = close;
                        Err(error)
                    }
                }
            }
            FullStageTag::Last | FullStageTag::Min | FullStageTag::Max => {
                let mut items = IndexedPipelineItems::new(evaluator, input, span)?;
                let selected = (|| -> Result<Option<LoweredValue>, RuntimeError> {
                    let mut selected = None;
                    while let Some(item) = items.next(evaluator, span)? {
                        selected = Some(match selected {
                            None => item,
                            Some(previous) => match self.tag {
                                FullStageTag::Last => item,
                                FullStageTag::Min => std::cmp::min_by(
                                    previous,
                                    item,
                                    compare_lowered_sort_keys,
                                ),
                                FullStageTag::Max => std::cmp::max_by(
                                    previous,
                                    item,
                                    compare_lowered_sort_keys,
                                ),
                                _ => unreachable!(),
                            },
                        });
                    }
                    Ok(selected)
                })();
                let close = items.cancel(evaluator, span);
                match selected {
                    Ok(Some(value)) => {
                        close?;
                        Ok(lowered_result_ok(value))
                    }
                    Ok(None) => {
                        close?;
                        Ok(lowered_result_err_value(
                            RuntimeError::new("empty-stream", "stream was empty").with_span(span),
                        ))
                    }
                    Err(error) => {
                        let _ = close;
                        Err(error)
                    }
                }
            }
            _ => unreachable!("only supported materialized stages reach apply"),
        }
    }

    fn finish(&mut self, evaluator: &mut Evaluator, error: Option<&RuntimeError>) -> Result<(), RuntimeError> {
        if self.finished {
            return Ok(());
        }
        self.finished = true;
        if evaluator.trace_enabled
            && let Some(item_index) = self
                .par_map
                .as_mut()
                .and_then(|par_map| par_map.active_item.take())
        {
            evaluator.trace_lowered_parallel_job(
                TraceKind::ParallelJobEnd,
                "par-map",
                item_index,
                None,
                self.span,
            );
        }
        let cancel_result = self
            .items
            .take()
            .map_or(Ok(()), |mut items| items.cancel(evaluator, self.span));
        let trace_error = error
            .or_else(|| cancel_result.as_ref().err())
            .map(TraceError::from_runtime_error);
        let name = Evaluator::indexed_stage_name(self.tag);
        evaluator.trace_exit(
            TraceKind::StreamStageExit,
            Some(self.span),
            Some(name),
            TracePayload::StreamStage {
                stage: name.to_string(),
                item_count: self.input_count,
                error: trace_error,
            },
        );
        cancel_result
    }
}

pub(super) fn indexed_table_print(
    evaluator: &mut Evaluator,
    input: LoweredValue,
    columns: Option<Vec<String>>,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    let mut items = IndexedPipelineItems::new(evaluator, input, span)?;
    let collected = (|| -> Result<Vec<LoweredValue>, RuntimeError> {
        let mut records = Vec::new();
        while let Some(item) = items.next(evaluator, span)? {
            records.push(item);
        }
        Ok(records)
    })();
    let close = items.cancel(evaluator, span);
    let collected = match collected {
        Ok(records) => {
            close?;
            records
        }
        Err(error) => {
            let _ = close;
            return Err(error);
        }
    };
    let records = lowered_pipeline_record_list(&LoweredValue::List(collected), span)?;
    let columns = columns.unwrap_or_else(|| {
        let mut seen = std::collections::BTreeSet::new();
        let mut columns = Vec::new();
        for record in &records {
            for key in record.keys() {
                if seen.insert(key.clone()) {
                    columns.push(key.to_string());
                }
            }
        }
        columns
    });
    let table_columns = columns
        .iter()
        .map(|name| {
            let align = records
                .first()
                .and_then(|record| record.get(name.as_str()))
                .map(|value| match value {
                    LoweredValue::Int(_) | LoweredValue::Float(_) | LoweredValue::Duration(_) => {
                        crate::terminal::table::TableAlign::Right
                    }
                    _ => crate::terminal::table::TableAlign::Left,
                })
                .unwrap_or(crate::terminal::table::TableAlign::Left);
            crate::terminal::table::TextTableColumn::new(name.clone(), 0, 80, align)
        })
        .collect::<Vec<_>>();
    let rows = records
        .iter()
        .map(|record| {
            columns
                .iter()
                .map(|column| {
                    let value = record
                        .get(column.as_str())
                        .cloned()
                        .unwrap_or(LoweredValue::Null);
                    crate::terminal::table::sanitize_table_text(&lowered_table_print_value(&value))
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let mut output = String::new();
    let width = crate::terminal::table::terminal_table_width_for_stdout(20, 120);
    crate::terminal::table::render_text_table(&table_columns, &rows, width, &mut output);
    evaluator.stdout.extend_from_slice(output.as_bytes());
    Ok(LoweredValue::Unit)
}

pub(super) enum IndexedSerialStep {
    Item(Option<LoweredValue>),
    Expr(u32),
    Block(u32),
    Break(LoweredValue),
}

#[derive(Clone, Copy)]
enum SerialExprKind {
    Where,
    Map { flat: bool, wrap_error: bool },
    Any { all: bool },
}

enum SerialAfterBlock {
    Tee,
    Each,
    Where { value: u32 },
    Map { value: u32, flat: bool },
    Any { value: u32, all: bool },
}

enum SerialAwaitPhase {
    Block(SerialAfterBlock),
    Expr(SerialExprKind),
}

struct SerialAwait {
    stage_index: usize,
    slot: usize,
    item: LoweredValue,
    phase: SerialAwaitPhase,
}

impl IndexedSerialPipeline {
    pub(super) fn new_frame(
        input: LoweredValue,
        stages: SmallVec<[IndexedLiveSerialStage; 4]>,
        span: Span,
    ) -> Result<ControlFlow<LoweredValue, Self>, RuntimeError> {
        let input = lowered_pipeline_input(input, span)?;
        let mut preflight = SmallVec::new();
        for (index, stage) in stages.iter().enumerate() {
            match stage.state {
                IndexedSerialState::Drop { .. } | IndexedSerialState::Take { .. } => {
                    preflight.push(index);
                }
                IndexedSerialState::Materialized => {
                    unreachable!("materialized stages do not run in the serial cursor")
                }
                _ => {}
            }
        }
        Ok(ControlFlow::Continue(Self {
            items: None,
            input: Some(input),
            stages,
            preflight,
            preflight_position: 0,
            awaiting_preflight: false,
            pending: SmallVec::new(),
            source_index: 0,
            emitted: 0,
            limit: usize::MAX,
            outcome: None,
            stopped: false,
            finished: false,
            traces_started: false,
            span,
            awaiting: None,
            callback_result: None,
        }))
    }

    pub(super) fn resume(
        &mut self,
        result: ControlFlow<LoweredValue, LoweredValue>,
    ) {
        assert!(
            self.awaiting_preflight
                || self.awaiting.as_ref().is_some_and(|awaiting| matches!(
                    &awaiting.phase,
                    SerialAwaitPhase::Expr(_)
                )),
            "pipeline callback result has an expression continuation"
        );
        assert!(self.callback_result.replace(result).is_none());
    }

    pub(super) fn pending_map_error_item(&self) -> Option<usize> {
        self.awaiting.as_ref().and_then(|awaiting| match &awaiting.phase {
            SerialAwaitPhase::Expr(SerialExprKind::Map {
                wrap_error: true,
                ..
            }) => Some(self.source_index.saturating_sub(1)),
            _ => None,
        })
    }

    pub(super) fn next_frame(
        &mut self,
        evaluator: &mut Evaluator,
        execution: &FullExecution<'_>,
        slots: &mut [LoweredValue],
    ) -> Result<IndexedSerialStep, RuntimeError> {
        if self.awaiting_preflight {
            self.awaiting_preflight = false;
            let result = self
                .callback_result
                .take()
                .expect("pipeline preflight expression has resumed");
            let value = match result {
                ControlFlow::Continue(value) => value,
                ControlFlow::Break(value) => return Ok(IndexedSerialStep::Break(value)),
            };
            let stage_index = self.preflight[self.preflight_position];
            let count = lowered_nonnegative_count(value, self.span)?;
            match &mut self.stages[stage_index].state {
                IndexedSerialState::Drop { count: target, .. }
                | IndexedSerialState::Take { count: target } => *target = count,
                _ => unreachable!("pipeline preflight owns a count stage"),
            }
            self.preflight_position += 1;
        }
        if let Some(stage_index) = self.preflight.get(self.preflight_position).copied() {
            let view = execution
                .stage_view(self.stages[stage_index].instruction)
                .map_err(|error| indexed_error(error, self.span))?;
            let expression = match view {
                StageView::Drop(expression) | StageView::Take(expression) => expression.raw(),
                _ => unreachable!("pipeline preflight owns a count stage"),
            };
            self.awaiting_preflight = true;
            return Ok(IndexedSerialStep::Expr(expression));
        }
        if let Some(input) = self.input.take() {
            let items = IndexedPipelineItems::new(evaluator, input, self.span)?;
            for stage in &self.stages {
                let name = Evaluator::indexed_stage_name(stage.tag);
                evaluator.trace_enter(
                    TraceKind::StreamStageEnter,
                    Some(self.span),
                    Some(name),
                    TracePayload::StreamStage {
                        stage: name.to_string(),
                        item_count: None,
                        error: None,
                    },
                );
            }
            self.items = Some(items);
            self.traces_started = true;
            self.limit = match self.stages.last() {
                Some(IndexedLiveSerialStage { state: IndexedSerialState::Take { count }, .. }) => *count,
                _ => usize::MAX,
            };
        }
        if let Some(awaiting) = self.awaiting.take() {
            match awaiting.phase {
                SerialAwaitPhase::Block(after) => match after {
                    SerialAfterBlock::Tee => {
                        slots[awaiting.slot] = LoweredValue::Unit;
                        self.pending
                            .push((awaiting.stage_index + 1, awaiting.item));
                    }
                    SerialAfterBlock::Each => {
                        slots[awaiting.slot] = LoweredValue::Unit;
                    }
                    SerialAfterBlock::Where { value } => {
                        self.awaiting = Some(SerialAwait {
                            phase: SerialAwaitPhase::Expr(SerialExprKind::Where),
                            ..awaiting
                        });
                        return Ok(IndexedSerialStep::Expr(value));
                    }
                    SerialAfterBlock::Map { value, flat } => {
                        self.awaiting = Some(SerialAwait {
                            phase: SerialAwaitPhase::Expr(SerialExprKind::Map {
                                flat,
                                wrap_error: false,
                            }),
                            ..awaiting
                        });
                        return Ok(IndexedSerialStep::Expr(value));
                    }
                    SerialAfterBlock::Any { value, all } => {
                        self.awaiting = Some(SerialAwait {
                            phase: SerialAwaitPhase::Expr(SerialExprKind::Any { all }),
                            ..awaiting
                        });
                        return Ok(IndexedSerialStep::Expr(value));
                    }
                },
                SerialAwaitPhase::Expr(kind) => {
                    let result = self
                        .callback_result
                        .take()
                        .expect("pipeline expression has resumed");
                    let value = match result {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => {
                            return Ok(IndexedSerialStep::Break(value));
                        }
                    };
                    match kind {
                        SerialExprKind::Where => {
                            let keep = super::lowered_condition_bool(value, self.span)?;
                            let item = std::mem::replace(
                                &mut slots[awaiting.slot],
                                LoweredValue::Unit,
                            );
                            if keep {
                                self.pending.push((awaiting.stage_index + 1, item));
                            }
                        }
                        SerialExprKind::Map { flat, .. } => {
                            slots[awaiting.slot] = LoweredValue::Unit;
                            if flat {
                                let rows = evaluator.lowered_flat_map_rows(value, self.span)?;
                                self.pending.extend(
                                    rows.into_iter()
                                        .rev()
                                        .map(|row| (awaiting.stage_index + 1, row)),
                                );
                            } else {
                                self.pending.push((awaiting.stage_index + 1, value));
                            }
                        }
                        SerialExprKind::Any { all } => {
                            slots[awaiting.slot] = LoweredValue::Unit;
                            let keep = super::lowered_condition_bool(value, self.span)?;
                            if keep != all {
                                self.outcome = Some(LoweredValue::Bool(!all));
                                self.stopped = true;
                            }
                        }
                    }
                }
            }
        }

        let span = self.span;
        while !self.stopped && self.emitted < self.limit {
            if self.pending.is_empty() {
                let Some(item) = self
                    .items
                    .as_mut()
                    .expect("pipeline source initialized before pulling")
                    .next(evaluator, span)?
                else {
                    return Ok(IndexedSerialStep::Item(None));
                };
                self.pending.push((0, item));
                self.source_index += 1;
            }
            let (stage_index, value) = self.pending.pop().expect("pending pipeline item");
            if stage_index == self.stages.len() {
                self.emitted += 1;
                return Ok(IndexedSerialStep::Item(Some(value)));
            }
            let stage = &mut self.stages[stage_index];
            let view = execution
                .stage_view(stage.instruction)
                .map_err(|error| indexed_error(error, self.span))?;
            let flat = matches!(&view, StageView::FlatMap { .. });
            let flat_block = matches!(&view, StageView::FlatMapBlock { .. });
            let all = matches!(&view, StageView::All { .. });
            let all_block = matches!(&view, StageView::AllBlock { .. });
            let mut item = Some(value);
            match view {
                StageView::Tee { slot, body } => {
                    let value = item.take().expect("tee stage receives an item");
                    slots[slot] = value.clone();
                    self.awaiting = Some(SerialAwait {
                        stage_index,
                        slot,
                        item: value,
                        phase: SerialAwaitPhase::Block(SerialAfterBlock::Tee),
                    });
                    return Ok(IndexedSerialStep::Block(body.block_id()));
                }
                StageView::Each { slot, body } => {
                    let value = item.take().expect("each stage receives an item");
                    slots[slot] = value.clone();
                    self.awaiting = Some(SerialAwait {
                        stage_index,
                        slot,
                        item: value,
                        phase: SerialAwaitPhase::Block(SerialAfterBlock::Each),
                    });
                    return Ok(IndexedSerialStep::Block(body.block_id()));
                }
                StageView::Where { slot, predicate } => {
                    let input = item.take().expect("where stage receives an item");
                    let keep = stage
                        .where_predicate
                        .as_ref()
                        .map(|fast_path| {
                            evaluator.eval_indexed_item_predicate(fast_path, &input, self.span)
                        })
                        .transpose()?;
                    match keep {
                        Some(true) => item = Some(input),
                        Some(false) => {}
                        None => {
                            slots[slot] = input.clone();
                            self.awaiting = Some(SerialAwait {
                                stage_index,
                                slot,
                                item: input,
                                phase: SerialAwaitPhase::Expr(SerialExprKind::Where),
                            });
                            return Ok(IndexedSerialStep::Expr(predicate.raw()));
                        }
                    }
                }
                StageView::WhereBlock { slot, body, value } => {
                    let input = item.take().expect("where block receives an item");
                    slots[slot] = input.clone();
                    self.awaiting = Some(SerialAwait {
                        stage_index,
                        slot,
                        item: input,
                        phase: SerialAwaitPhase::Block(SerialAfterBlock::Where {
                            value: value.raw(),
                        }),
                    });
                    return Ok(IndexedSerialStep::Block(body.block_id()));
                }
                StageView::Map { slot, value } | StageView::FlatMap { slot, value } => {
                    let input = item.take().expect("map stage receives an item");
                    let projected = if !flat {
                        if let Some((field, field_span)) = stage.map_projection.as_ref() {
                            match evaluator.indexed_borrowed_field_value(&input, field, *field_span) {
                                Ok(projected) => projected,
                                Err(error) => {
                                    return Err(evaluator.stream_item_runtime_error(
                                        "map",
                                        self.source_index.saturating_sub(1),
                                        error,
                                    ));
                                }
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    if let Some(projected) = projected {
                        item = Some(projected);
                    } else {
                        slots[slot] = input.clone();
                        self.awaiting = Some(SerialAwait {
                            stage_index,
                            slot,
                            item: input,
                            phase: SerialAwaitPhase::Expr(SerialExprKind::Map {
                                flat,
                                wrap_error: true,
                            }),
                        });
                        return Ok(IndexedSerialStep::Expr(value.raw()));
                    }
                }
                StageView::MapBlock { slot, body, value }
                | StageView::FlatMapBlock { slot, body, value } => {
                    let input = item.take().expect("map block receives an item");
                    slots[slot] = input.clone();
                    self.awaiting = Some(SerialAwait {
                        stage_index,
                        slot,
                        item: input,
                        phase: SerialAwaitPhase::Block(SerialAfterBlock::Map {
                            value: value.raw(),
                            flat: flat_block,
                        }),
                    });
                    return Ok(IndexedSerialStep::Block(body.block_id()));
                }
                StageView::Drop(_) => {
                    let IndexedSerialState::Drop { count, seen } = &mut stage.state else {
                        unreachable!("Drop stage state follows its verified tag")
                    };
                    if item.is_some() && *seen < *count {
                        *seen += 1;
                        item = None;
                    }
                }
                StageView::Enumerate => {
                    let IndexedSerialState::Enumerate { index } = &mut stage.state else {
                        unreachable!("Enumerate stage state follows its verified tag")
                    };
                    if let Some(value) = item.take() {
                        item = Some(LoweredValue::Record(Arc::new(btree_map(vec![
                            (Arc::from("index"), LoweredValue::Int(*index as i64)),
                            (Arc::from("value"), value),
                        ]))));
                        *index += 1;
                    }
                }
                StageView::Take(_) => {
                    if !matches!(stage.state, IndexedSerialState::Take { .. }) {
                        unreachable!("Take stage state follows its verified tag")
                    }
                    if let Some(value) = item.take() {
                        self.emitted += 1;
                        self.stopped = self.emitted >= self.limit;
                        return Ok(IndexedSerialStep::Item(Some(value)));
                    }
                }
                StageView::Any { slot, predicate } | StageView::All { slot, predicate } => {
                    let value = item.take().expect("any stage receives an item");
                    slots[slot] = value.clone();
                    self.awaiting = Some(SerialAwait {
                        stage_index,
                        slot,
                        item: value,
                        phase: SerialAwaitPhase::Expr(SerialExprKind::Any { all }),
                    });
                    return Ok(IndexedSerialStep::Expr(predicate.raw()));
                }
                StageView::AnyBlock { slot, body, value }
                | StageView::AllBlock { slot, body, value } => {
                    let input = item.take().expect("any block receives an item");
                    slots[slot] = input.clone();
                    self.awaiting = Some(SerialAwait {
                        stage_index,
                        slot,
                        item: input,
                        phase: SerialAwaitPhase::Block(SerialAfterBlock::Any {
                            value: value.raw(),
                            all: all_block,
                        }),
                    });
                    return Ok(IndexedSerialStep::Block(body.block_id()));
                }
                StageView::First => {
                    if !matches!(stage.state, IndexedSerialState::First) {
                        unreachable!("First stage state follows its verified tag")
                    }
                    if let Some(value) = item.take() {
                        self.outcome = Some(lowered_result_ok(value));
                        self.stopped = true;
                    }
                }
                StageView::Count => {
                    if !matches!(stage.state, IndexedSerialState::Count) {
                        unreachable!("Count stage state follows its verified tag")
                    }
                    if item.take().is_some() {
                        self.emitted += 1;
                    }
                }
                StageView::Collect => {
                    if !matches!(stage.state, IndexedSerialState::Collect) {
                        unreachable!("Collect stage state follows its verified tag")
                    }
                    if let Some(value) = item.take() {
                        self.emitted += 1;
                        return Ok(IndexedSerialStep::Item(Some(value)));
                    }
                }
                _ => unreachable!("materialized stage reached the serial cursor"),
            }
            if let Some(item) = item {
                self.pending.push((stage_index + 1, item));
            }
        }
        Ok(IndexedSerialStep::Item(None))
    }

    pub(super) fn visit_retained_values(
        &self,
        visitor: &mut dyn for<'a> FnMut(crate::runtime::eval::RetainedValueRef<'a>),
    ) {
        if let Some(items) = &self.items {
            items.visit_retained_values(visitor);
        }
        if let Some(input) = &self.input {
            crate::runtime::eval::visit_retained_lowered_value(input, visitor);
        }
        crate::runtime::eval::visit_retained_lowered_values(
            self.pending.iter().map(|(_, value)| value),
            visitor,
        );
        if let Some(value) = &self.outcome {
            crate::runtime::eval::visit_retained_lowered_value(value, visitor);
        }
        if let Some(awaiting) = &self.awaiting {
            crate::runtime::eval::visit_retained_lowered_value(&awaiting.item, visitor);
        }
        if let Some(result) = &self.callback_result {
            match result {
                ControlFlow::Continue(value) | ControlFlow::Break(value) => {
                    crate::runtime::eval::visit_retained_lowered_value(value, visitor);
                }
            }
        }
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
        let cancel_result = self
            .items
            .as_mut()
            .map_or(Ok(()), |items| items.cancel(evaluator, self.span));
        let trace_error = error
            .or_else(|| cancel_result.as_ref().err())
            .map(TraceError::from_runtime_error);
        if self.traces_started {
            for stage in self.stages.iter().rev() {
                let name = Evaluator::indexed_stage_name(stage.tag);
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
        }
        cancel_result
    }

    pub(super) fn into_result(self, output: Vec<LoweredValue>) -> LoweredValue {
        match self.stages.last().map(|stage| stage.tag) {
            Some(FullStageTag::Each) => LoweredValue::Unit,
            Some(FullStageTag::Any | FullStageTag::All | FullStageTag::AnyBlock | FullStageTag::AllBlock) => {
                self.outcome.unwrap_or(LoweredValue::Bool(matches!(
                    self.stages.last().map(|stage| stage.tag),
                    Some(FullStageTag::All | FullStageTag::AllBlock)
                )))
            }
            Some(FullStageTag::First) => self.outcome.unwrap_or_else(|| {
                lowered_result_err_value(
                    RuntimeError::new("empty-stream", "stream was empty").with_span(self.span),
                )
            }),
            Some(FullStageTag::Count) => LoweredValue::Int(self.emitted as i64),
            _ => LoweredValue::List(output),
        }
    }
}

impl IndexedStageEngine {
    pub(super) fn new_frame(
        input: LoweredValue,
        stages: SmallVec<[IndexedLiveSerialStage; 4]>,
        span: Span,
    ) -> Result<Self, RuntimeError> {
        Ok(Self {
            current: Some(lowered_pipeline_input(input, span)?),
            stages,
            stage_position: 0,
            active_stage_count: 0,
            active: None,
            materialized: None,
            output: Vec::new(),
            span,
        })
    }

    pub(super) fn resume(&mut self, result: ControlFlow<LoweredValue, LoweredValue>) {
        if let Some(materialized) = &mut self.materialized {
            materialized.resume(result);
        } else {
            self.active
                .as_mut()
                .expect("stage callback has an active serial group")
                .resume(result);
        }
    }

    pub(super) fn pending_map_error_item(&self) -> Option<usize> {
        self.active
            .as_ref()
            .and_then(|active| active.pending_map_error_item())
    }

    pub(super) fn pending_item_error(&self) -> Option<(&'static str, usize)> {
        self.pending_map_error_item()
            .map(|item_index| ("map", item_index))
            .or_else(|| {
                self.materialized
                    .as_ref()
                    .and_then(|materialized| materialized.pending_item_error())
            })
    }

    pub(super) fn visit_retained_values(
        &self,
        visitor: &mut dyn for<'a> FnMut(crate::runtime::eval::RetainedValueRef<'a>),
    ) {
        if let Some(value) = &self.current {
            crate::runtime::eval::visit_retained_lowered_value(value, visitor);
        }
        if let Some(active) = &self.active {
            active.visit_retained_values(visitor);
        }
        if let Some(materialized) = &self.materialized {
            materialized.visit_retained_values(visitor);
        }
        crate::runtime::eval::visit_retained_lowered_values(&self.output, visitor);
    }

    pub(super) fn next_frame(
        &mut self,
        evaluator: &mut Evaluator,
        execution: &FullExecution<'_>,
        slots: &mut [LoweredValue],
    ) -> Result<IndexedStageStep, RuntimeError> {
        loop {
            if let Some(materialized) = &mut self.materialized {
                match materialized.next_frame(evaluator, execution, slots)? {
                    IndexedMaterializedStep::Expr(expression) => {
                        return Ok(IndexedStageStep::Expr(expression));
                    }
                    IndexedMaterializedStep::Block(block) => {
                        return Ok(IndexedStageStep::Block(block));
                    }
                    IndexedMaterializedStep::Break(value) => {
                        let mut materialized = self
                            .materialized
                            .take()
                            .expect("materialized stage remains active");
                        materialized.finish(evaluator, None)?;
                        return Ok(IndexedStageStep::Break(value));
                    }
                    IndexedMaterializedStep::Done(value) => {
                        let mut materialized = self
                            .materialized
                            .take()
                            .expect("materialized stage remains active");
                        materialized.finish(evaluator, None)?;
                        self.stage_position += 1;
                        self.current = Some(value);
                        continue;
                    }
                }
            }
            if self.active.is_none() {
                if self.stage_position == self.stages.len() {
                    let value = self.current.take().unwrap_or(LoweredValue::Unit);
                    return Ok(IndexedStageStep::Done(value));
                }
                let start = self.stage_position;
                if matches!(self.stages[start].state, IndexedSerialState::Materialized) {
                    let input = self.current.take().expect("materialized stage input");
                    self.materialized = Some(Box::new(IndexedMaterializedStage::new_frame(
                        evaluator,
                        execution,
                        input,
                        self.stages[start].instruction,
                        self.span,
                    )?));
                    continue;
                }
                let group_limit = self.stages[start..]
                    .iter()
                    .position(|stage| matches!(stage.state, IndexedSerialState::Materialized))
                    .map_or(self.stages.len(), |offset| start + offset);
                let end = self.stages[start..group_limit]
                    .iter()
                    .position(|stage| stage.ends_group())
                    .map_or(group_limit, |offset| start + offset + 1);
                let active_stages = self.stages[start..end].iter().cloned().collect();
                self.active_stage_count = end - start;
                let input = self.current.take().expect("serial stage group input");
                let active = IndexedSerialPipeline::new_frame(input, active_stages, self.span)?;
                let active = match active {
                    ControlFlow::Continue(active) => active,
                    ControlFlow::Break(value) => return Ok(IndexedStageStep::Break(value)),
                };
                self.active = Some(Box::new(active));
            }

            let pulled = self
                .active
                .as_mut()
                .expect("serial group active")
                .next_frame(evaluator, execution, slots)?;
            match pulled {
                IndexedSerialStep::Item(Some(value)) => self.output.push(value),
                IndexedSerialStep::Item(None) => {
                    let mut active = self.active.take().expect("serial group active");
                    active.finish(evaluator, None)?;
                    let value = active.into_result(std::mem::take(&mut self.output));
                    self.stage_position += self.active_stage_count;
                    self.active_stage_count = 0;
                    if self.stage_position == self.stages.len() {
                        return Ok(IndexedStageStep::Done(value));
                    }
                    self.current = Some(value);
                }
                IndexedSerialStep::Expr(expression) => {
                    return Ok(IndexedStageStep::Expr(expression));
                }
                IndexedSerialStep::Block(block) => {
                    return Ok(IndexedStageStep::Block(block));
                }
                IndexedSerialStep::Break(value) => {
                    let mut active = self.active.take().expect("serial group active");
                    active.finish(evaluator, None)?;
                    return Ok(IndexedStageStep::Break(value));
                }
            }
        }
    }

    pub(super) fn finish(
        &mut self,
        evaluator: &mut Evaluator,
        error: Option<&RuntimeError>,
    ) -> Result<(), RuntimeError> {
        match self.active.as_mut() {
            Some(active) => active.finish(evaluator, error),
            None => match self.materialized.as_mut() {
                Some(materialized) => materialized.finish(evaluator, error),
                None => Ok(()),
            },
        }
    }
}

// Decode stage shape before running the input. A `for` loop can keep this
// state across body executions without evaluating the input expression again.
pub(super) fn decode_serial_prefix(
    execution: &FullExecution<'_>,
    stages: ListView<'_, LoweredPipelineStage>,
    span: Span,
) -> Result<SmallVec<[IndexedLiveSerialStage; 4]>, RuntimeError> {
    decode_serial_stages(execution, stages, span, true)
}

pub(super) fn decode_serial_complete(
    execution: &FullExecution<'_>,
    stages: ListView<'_, LoweredPipelineStage>,
    span: Span,
) -> Result<SmallVec<[IndexedLiveSerialStage; 4]>, RuntimeError> {
    decode_serial_stages(execution, stages, span, false)
}

fn decode_serial_stages(
    execution: &FullExecution<'_>,
    mut stages: ListView<'_, LoweredPipelineStage>,
    span: Span,
    stop_after_terminal: bool,
) -> Result<SmallVec<[IndexedLiveSerialStage; 4]>, RuntimeError> {
    let mut prefix: SmallVec<[IndexedLiveSerialStage; 4]> = SmallVec::new();
    let stage_count = stages.len();
    for _ in 0..stage_count {
        let stage = stages
            .next_with_id()
            .expect("stage cursor length matches its remaining entries")
            .map_err(|error| indexed_error(error, span))?;
        let instruction = stage.id.raw();
        let view = stage.view;
        let tag = view.tag();
        let mut map_projection = None;
        let mut where_predicate = None;
        let state = match view {
            StageView::Tee { .. }
            | StageView::Each { .. }
            | StageView::WhereBlock { .. }
            | StageView::FlatMap { .. }
            | StageView::MapBlock { .. }
            | StageView::FlatMapBlock { .. }
            | StageView::Any { .. }
            | StageView::All { .. }
            | StageView::AnyBlock { .. }
            | StageView::AllBlock { .. } => IndexedSerialState::Stateless,
            StageView::Where { slot, predicate } => {
                where_predicate = Evaluator::indexed_item_predicate(
                    execution,
                    predicate.raw(),
                    slot,
                    span,
                )?
                .map(Arc::new);
                IndexedSerialState::Stateless
            }
            StageView::Map { slot, value } => {
                map_projection = Evaluator::indexed_field_projection_with_span(
                    execution,
                    value.raw(),
                    slot,
                    span,
                )?
                .map(|(field, field_span)| (Arc::from(field), field_span));
                IndexedSerialState::Stateless
            }
            StageView::Drop(_) => IndexedSerialState::Drop { count: 0, seen: 0 },
            StageView::Enumerate => IndexedSerialState::Enumerate { index: 0 },
            StageView::Take(_) => IndexedSerialState::Take { count: 0 },
            StageView::First => IndexedSerialState::First,
            StageView::Count => IndexedSerialState::Count,
            StageView::Collect => IndexedSerialState::Collect,
            _ if stop_after_terminal => break,
            _ => IndexedSerialState::Materialized,
        };
        let decoded = IndexedLiveSerialStage {
            instruction,
            tag,
            state,
            map_projection,
            where_predicate,
        };
        prefix.push(decoded);
        if stop_after_terminal
            && matches!(tag,
                FullStageTag::Take
                    | FullStageTag::Any
                    | FullStageTag::All
                    | FullStageTag::AnyBlock
                    | FullStageTag::AllBlock
                    | FullStageTag::Each
                    | FullStageTag::First
                    | FullStageTag::Count
                    | FullStageTag::Collect)
        {
            break;
        }
    }
    Ok(prefix)
}
