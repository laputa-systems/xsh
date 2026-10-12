use super::{
    Arc, BTreeMap, BinaryOp, ControlFlow, Evaluator, FullExecution,
    FullStageTag, FullTag, LoweredProjectedReduceState, LoweredReduceProjection, LoweredValue,
    MapKey, ParMapItemOutcome, ReduceByOp, RuntimeError, Span, StmtFlow, Traceback,
    ExprView, indexed_error, indexed_value,
    lowered_error_message, lowered_inline_stats_field_value, lowered_record_vec_get,
    lowered_reduce_fields_owned, lowered_reduce_group_insert, lowered_reduce_key_value_owned,
    lowered_stats_field_value, lowered_str_value, lowered_value_from_runtime_any,
};
use crate::runtime::eval::indexed::full::decode::{ListView, RecordEntryView};
use crate::runtime::eval::LoweredRecordEntry;

/// Whether a `par-map` item failed: by a runtime error, by a failure that is
/// propagating out of its callback, or by returning an `Err` from the
/// enclosing function, which is what `fail` does. Any other `return`, a
/// `break`, and a `continue` that leave the callback are not failures.
pub(super) fn par_map_item_failed(
    result: &Result<LoweredValue, RuntimeError>,
    flow: Option<&StmtFlow>,
) -> bool {
    result.is_err()
        || matches!(
            flow,
            Some(StmtFlow::Propagate(_) | StmtFlow::Return(LoweredValue::ResultErr(_)))
        )
}

#[derive(Debug)]
pub(super) enum IndexedItemPredicate {
    StringCompare {
        field: Arc<str>,
        field_span: Span,
        op: BinaryOp,
        value: Arc<str>,
    },
    And(Box<Self>, Box<Self>),
    Or(Box<Self>, Box<Self>),
}

impl Evaluator {
    /// Returned Result values stay in-band. A `?` failure leaves its
    /// `Propagate` flow pending, as in `map`, so the caller stops the stage and
    /// the enclosing function returns the `Err` instead of raising a runtime
    /// error.
    pub(super) fn eval_indexed_par_map_item(
        &mut self,
        execution: &FullExecution<'_>,
        body: Option<u32>,
        value: u32,
        slots: &mut [LoweredValue],
        slot: usize,
        item: LoweredValue,
        span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        slots[slot] = item;
        let item_result = if let Some(body) = body {
            match self.eval_indexed_statement_block(execution, body, slots, span) {
                Ok(StmtFlow::None) => self.eval_indexed_expr(execution, value, slots, span),
                Ok(flow) => Ok(self.preserve_lexical_expression_flow(flow)),
                Err(error) => Err(error),
            }
        } else {
            self.eval_indexed_expr(execution, value, slots, span)
        };
        match item_result? {
            ControlFlow::Continue(value) | ControlFlow::Break(value) => Ok(value),
        }
    }

    /// Runs `items` on `jobs` workers and returns the mapped values in input
    /// order. Once an item fails or leaves a control flow, workers start no
    /// further items. An item that fails also stops the items still running:
    /// their workers stop the children they run and fail at their next
    /// checkpoint. Of the items that ended on their own, the earliest that
    /// failed or left a flow decides the stage, and its flow and traceback
    /// are handed to this evaluator.
    pub(super) fn eval_indexed_par_map_parallel(
        &mut self,
        execution: &FullExecution<'_>,
        body: Option<u32>,
        value: u32,
        slots: &[LoweredValue],
        slot: usize,
        items: Vec<LoweredValue>,
        jobs: usize,
        span: Span,
    ) -> Result<Vec<LoweredValue>, RuntimeError> {
        let worker_count = jobs.min(items.len()).max(1);
        let item_count = items.len();
        let mut partitions: Vec<Vec<(usize, LoweredValue)>> =
            (0..worker_count).map(|_| Vec::new()).collect();
        for (index, item) in items.into_iter().enumerate() {
            partitions[index % worker_count].push((index, item));
        }
        let shared = self.lowered_shared_state();
        let symbols = shared
            .indexed_program
            .as_ref()
            .expect("verified lowered par-map execution has an indexed program")
            .symbol_owner()
            .clone();
        let base_slots = slots.to_vec();
        let call_stack = self.call_stack.clone();
        // Set once any item fails or leaves a control flow, so no worker
        // starts another item.
        let stopped = std::sync::atomic::AtomicBool::new(false);
        // Set when the stage stops before its workers are done: when an item
        // fails, and when this evaluator gives the stage up at a deadline, on
        // cancellation, or while shutting down. A worker then stops the child
        // it is running and fails its item.
        let abandoned = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (chunks, stderr) = std::thread::scope(|scope| {
            let (sender, receiver) = std::sync::mpsc::sync_channel(worker_count);
            let mut workers = Vec::with_capacity(worker_count);
            for (chunk_index, chunk) in partitions.into_iter().enumerate() {
                let shared = &shared;
                let stopped = &stopped;
                let sender = sender.clone();
                let symbols = symbols.clone();
                let base_slots = base_slots.clone();
                let call_stack = call_stack.clone();
                let abandoned = Arc::clone(&abandoned);
                let execution = execution.thread_local();
                let worker = std::thread::Builder::new()
                    .stack_size(super::super::super::debug_test_eval_stack_size(12 * 1024 * 1024))
                    .spawn_scoped(scope, move || {
                        let allocation_stage = crate::mem_track::begin_worker_stage();
                        let _symbols = symbols.enter();
                        let (mut worker, mut worker_slots) = {
                            let _setup = allocation_stage
                                .scope(crate::mem_track::WorkerAllocationScope::Setup);
                            let mut worker = Evaluator::new_lowered_worker(shared);
                            worker.obey_stage_abandonment(Arc::clone(&abandoned));
                            // A traceback built in the callback names the
                            // functions that are running the stage.
                            worker.call_stack = call_stack;
                            (worker, base_slots)
                        };
                        let mut results = {
                            let _results = allocation_stage
                                .scope(crate::mem_track::WorkerAllocationScope::ParMapResults);
                            Vec::with_capacity(chunk.len())
                        };
                        {
                            let _items = allocation_stage
                                .scope(crate::mem_track::WorkerAllocationScope::ParMapItem);
                            for (item_index, item) in chunk {
                                if stopped.load(std::sync::atomic::Ordering::Relaxed) {
                                    break;
                                }
                                let result = worker.eval_indexed_par_map_item(
                                    &execution,
                                    body,
                                    value,
                                    &mut worker_slots,
                                    slot,
                                    item,
                                    span,
                                );
                                let flow = worker.pending_value_block_flow.take();
                                let traceback = worker.pending_traceback.take();
                                let interrupted = worker.take_stage_stop_observed();
                                if result.is_err() || flow.is_some() {
                                    stopped.store(true, std::sync::atomic::Ordering::Relaxed);
                                }
                                if par_map_item_failed(&result, flow.as_ref()) {
                                    abandoned.store(true, std::sync::atomic::Ordering::Relaxed);
                                }
                                results.push((
                                    item_index,
                                    ParMapItemOutcome {
                                        result,
                                        flow,
                                        traceback,
                                        interrupted,
                                    },
                                ));
                            }
                        }
                        // The stage waits for every worker, so the receiver
                        // is there. Were it gone, there would be no one to
                        // report to and nothing to do about it.
                        if let Some(error) = worker.finish_owned_resources() {
                            worker.report_cleanup_error(&error, span);
                        }
                        let _ =
                            sender.send((chunk_index, results, std::mem::take(&mut worker.stderr)));
                    })
                    .expect("failed to spawn lowered par-map worker");
                workers.push((chunk_index, worker));
            }
            drop(sender);
            let mut completed: Vec<Option<(Vec<(usize, ParMapItemOutcome)>, Vec<u8>)>> =
                (0..workers.len()).map(|_| None).collect();
            let mut remaining = workers.len();
            // The stage never returns before its workers do: they borrow this
            // frame, report on this channel, and own children that must be
            // gone when the stage is. When this evaluator has to give the
            // stage up, it raises `abandoned`, keeps waiting, and fails with
            // the reason once every worker is back.
            let mut gave_up: Option<RuntimeError> = None;
            while remaining > 0 {
                match receiver.recv_timeout(std::time::Duration::from_millis(1)) {
                    Ok((chunk_index, results, worker_stderr)) => {
                        completed[chunk_index] = Some((results, worker_stderr));
                        remaining -= 1;
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        return Err(RuntimeError::new(
                            "par-map",
                            "worker exited without returning its results",
                        )
                        .with_span(span));
                    }
                }
                if gave_up.is_some() {
                    continue;
                }
                let reason = match self.service_pending_signal(span) {
                    Err(error) => Some(error),
                    Ok(()) if self.shutting_down() => Some(
                        RuntimeError::abort(
                            self.signal_state.shutdown_status.unwrap_or(3),
                            self.signal_state.shutdown_force,
                        )
                        .with_span(span),
                    ),
                    Ok(()) => None,
                };
                if let Some(reason) = reason {
                    stopped.store(true, std::sync::atomic::Ordering::Relaxed);
                    abandoned.store(true, std::sync::atomic::Ordering::Relaxed);
                    gave_up = Some(reason);
                }
            }
            for (_, worker) in workers {
                worker
                    .join()
                    .expect("lowered par-map worker thread panicked");
            }
            if let Some(reason) = gave_up {
                return Err(reason);
            }
            let mut ordered: Vec<Option<ParMapItemOutcome>> =
                (0..item_count).map(|_| None).collect();
            let mut stderr = Vec::new();
            for completed in completed {
                let (mut results, worker_stderr) = completed.expect("par-map worker missing");
                for (item_index, result) in results.drain(..) {
                    ordered[item_index] = Some(result);
                }
                stderr.extend(worker_stderr);
            }
            // Items skipped after a failure have no outcome, and an item the
            // stage interrupted did not end on its own. Of the others, the
            // earliest that failed or left a flow decides the stage.
            let mut results = Vec::with_capacity(item_count);
            for (item_index, outcome) in ordered.into_iter().enumerate() {
                let Some(ParMapItemOutcome {
                    result,
                    flow,
                    traceback,
                    interrupted,
                }) = outcome
                else {
                    continue;
                };
                if interrupted && (result.is_err() || flow.is_some()) {
                    continue;
                }
                let value = result.map_err(|error| {
                    self.stream_item_runtime_error("par-map", item_index, error)
                })?;
                if let Some(flow) = flow {
                    self.pending_value_block_flow = Some(flow);
                    self.pending_traceback = traceback;
                    return Ok((results, stderr));
                }
                results.push(value);
            }
            // Every stop has a cause: an item that ended on its own, found
            // above, or this evaluator giving the stage up, returned earlier.
            if results.len() != item_count {
                return Err(
                    RuntimeError::new("par-map", "an item was skipped without a failure")
                        .with_span(span),
                );
            }
            Ok((results, stderr))
        })?;
        self.stderr.extend(stderr);
        Ok(chunks)
    }

    // Fused workers use the same projection as the ordinary reduce-by handler:
    // simple record sums update accumulators without rebuilding each output
    // record.
    pub(super) fn eval_indexed_reduce_rows(
        &mut self,
        execution: &FullExecution<'_>,
        rows: Vec<LoweredValue>,
        reduce_item_slot: usize,
        reduce_body: u32,
        reduce_value: u32,
        op: ReduceByOp,
        projection: &mut Option<LoweredProjectedReduceState<'_>>,
        slots: &mut [LoweredValue],
        groups: &mut BTreeMap<String, LoweredValue>,
        span: Span,
    ) -> Result<(), RuntimeError> {
        for row in rows {
            if let Some(projection) = projection.as_mut() {
                self.eval_lowered_projected_reduce_by_item(projection, row, groups, span)?;
                continue;
            }
            slots[reduce_item_slot] = row;
            match self.eval_indexed_statement_block(execution, reduce_body, slots, span)? {
                StmtFlow::None => {}
                flow => {
                    self.pending_value_block_flow = Some(flow);
                    return Ok(());
                }
            }
            let output = match self.eval_indexed_expr(execution, reduce_value, slots, span)? {
                ControlFlow::Continue(value) => value,
                ControlFlow::Break(value) => {
                    if self.pending_value_block_flow.is_some() {
                        return Ok(());
                    }
                    return Err(
                        RuntimeError::new("par-map-reduce", lowered_error_message(&value))
                            .with_span(span),
                    );
                }
            };
            let (key, value) = lowered_reduce_fields_owned(output, "key", "value", span)?;
            let key = lowered_reduce_key_value_owned(key, span)?;
            lowered_reduce_group_insert(groups, key, value, op, span)?;
        }
        slots[reduce_item_slot] = LoweredValue::Unit;
        Ok(())
    }

    pub(super) fn lowered_flat_map_rows(
        &mut self,
        value: LoweredValue,
        span: Span,
    ) -> Result<Vec<LoweredValue>, RuntimeError> {
        match value {
            LoweredValue::List(values) => Ok(values),
            LoweredValue::SharedList(values) => Ok((*values).clone()),
            LoweredValue::Stream(stream) => self
                .collect_stream_values(*stream, span)?
                .into_iter()
                .map(|value| {
                    lowered_value_from_runtime_any(&value).ok_or_else(|| {
                        RuntimeError::new(
                            "type-error",
                            format!("flat-map produced unsupported {}", value.type_name()),
                        )
                        .with_span(span)
                    })
                })
                .collect(),
            other => Err(RuntimeError::new(
                "type-error",
                format!(
                    "flat-map expected List or Stream, found {}",
                    other.type_name()
                ),
            )
            .with_span(span)),
        }
    }

    pub(super) fn eval_indexed_par_map_flat_map_reduce_by(
        &mut self,
        execution: &FullExecution<'_>,
        body: Option<u32>,
        value: u32,
        flatten: bool,
        reduce_item_slot: usize,
        reduce_body: u32,
        reduce_value: u32,
        op: ReduceByOp,
        slots: &[LoweredValue],
        slot: usize,
        items: Vec<LoweredValue>,
        jobs: usize,
        span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        let worker_count = jobs.min(items.len()).max(1);
        let chunk_size = items.len().div_ceil(worker_count);
        let shared = self.lowered_shared_state();
        let symbols = shared
            .indexed_program
            .as_ref()
            .expect("verified fused par-map has an indexed program")
            .symbol_owner()
            .clone();
        let base_slots = slots.to_vec();
        let call_stack = self.call_stack.clone();
        // Set once a chunk fails or leaves a control flow, so no worker starts
        // another item. Chunks are contiguous, so the earliest chunk that
        // failed on its own holds the earliest such item that ran.
        let stopped = std::sync::atomic::AtomicBool::new(false);
        // Set when the stage stops before its workers are done: when a chunk
        // fails, and when this evaluator gives the stage up. A worker then
        // stops the child it is running and fails.
        let abandoned = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let completed = std::thread::scope(|scope| {
            let mut workers = Vec::with_capacity(worker_count);
            for (chunk_index, chunk) in items.chunks(chunk_size).enumerate() {
                let chunk = chunk.to_vec();
                let shared = &shared;
                let stopped = &stopped;
                let symbols = symbols.clone();
                let base_slots = base_slots.clone();
                let call_stack = call_stack.clone();
                let abandoned = Arc::clone(&abandoned);
                let execution = execution.thread_local();
                let worker = std::thread::Builder::new()
                    .stack_size(super::super::super::debug_test_eval_stack_size(12 * 1024 * 1024))
                    .spawn_scoped(scope, move || {
                        let allocation_stage = crate::mem_track::begin_worker_stage();
                        let _symbols = symbols.enter();
                        let (mut worker, mut worker_slots, mut groups) = {
                            let _setup = allocation_stage
                                .scope(crate::mem_track::WorkerAllocationScope::Setup);
                            let mut worker = Evaluator::new_lowered_worker(shared);
                            worker.obey_stage_abandonment(Arc::clone(&abandoned));
                            worker.call_stack = call_stack;
                            (worker, base_slots, BTreeMap::new())
                        };
                        let result = (|| {
                            let mut projection = Self::indexed_reduce_projection(
                                &execution,
                                reduce_item_slot,
                                reduce_body,
                                reduce_value,
                                op,
                                span,
                            )?
                            .map(LoweredProjectedReduceState::new);
                            for item in chunk {
                                if stopped.load(std::sync::atomic::Ordering::Relaxed) {
                                    break;
                                }
                                let mapped = {
                                    let _item = allocation_stage
                                        .scope(crate::mem_track::WorkerAllocationScope::ParMapItem);
                                    worker.eval_indexed_par_map_item(
                                        &execution,
                                        body,
                                        value,
                                        &mut worker_slots,
                                        slot,
                                        item,
                                        span,
                                    )?
                                };
                                if worker.pending_value_block_flow.is_some() {
                                    break;
                                }
                                {
                                    let _reduce = allocation_stage.scope(
                                        crate::mem_track::WorkerAllocationScope::FusedReduceItem,
                                    );
                                    let rows = if flatten {
                                        worker.lowered_flat_map_rows(mapped, span)?
                                    } else {
                                        vec![mapped]
                                    };
                                    worker.eval_indexed_reduce_rows(
                                        &execution,
                                        rows,
                                        reduce_item_slot,
                                        reduce_body,
                                        reduce_value,
                                        op,
                                        &mut projection,
                                        &mut worker_slots,
                                        &mut groups,
                                        span,
                                    )?;
                                }
                                if worker.pending_value_block_flow.is_some() {
                                    break;
                                }
                            }
                            Ok::<_, RuntimeError>(groups)
                        })();
                        let flow = worker.pending_value_block_flow.take();
                        let interrupted = worker.take_stage_stop_observed();
                        if result.is_err() || flow.is_some() {
                            stopped.store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                        let failed = match &result {
                            Ok(_) => par_map_item_failed(&Ok(LoweredValue::Unit), flow.as_ref()),
                            Err(_) => true,
                        };
                        if failed {
                            abandoned.store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                        let traceback = worker.pending_traceback.take();
                        if let Some(error) = worker.finish_owned_resources() {
                            worker.report_cleanup_error(&error, span);
                        }
                        (
                            chunk_index,
                            result,
                            std::mem::take(&mut worker.stderr),
                            flow,
                            traceback,
                            interrupted,
                        )
                    })
                    .expect("failed to spawn fused par-map worker");
                workers.push(worker);
            }
            let mut completed: Vec<
                Option<(
                    Result<BTreeMap<String, LoweredValue>, RuntimeError>,
                    Vec<u8>,
                    Option<StmtFlow>,
                    Option<Traceback>,
                    bool,
                )>,
            > = (0..workers.len()).map(|_| None).collect();
            // As in `par-map`, the stage waits for every worker even when it
            // has to give the stage up, and fails with the reason afterwards.
            let mut gave_up: Option<RuntimeError> = None;
            while !workers.is_empty() {
                let mut index = 0;
                let mut progress = false;
                while index < workers.len() {
                    if workers[index].is_finished() {
                        let worker = workers.swap_remove(index);
                        let (chunk_index, result, worker_stderr, flow, traceback, interrupted) =
                            worker.join().expect("fused par-map worker thread panicked");
                        completed[chunk_index] =
                            Some((result, worker_stderr, flow, traceback, interrupted));
                        progress = true;
                    } else {
                        index += 1;
                    }
                }
                if gave_up.is_none() {
                    let reason = match self.service_pending_signal(span) {
                        Err(error) => Some(error),
                        Ok(()) if self.shutting_down() => Some(
                            RuntimeError::abort(
                                self.signal_state.shutdown_status.unwrap_or(3),
                                self.signal_state.shutdown_force,
                            )
                            .with_span(span),
                        ),
                        Ok(()) => None,
                    };
                    if let Some(reason) = reason {
                        stopped.store(true, std::sync::atomic::Ordering::Relaxed);
                        abandoned.store(true, std::sync::atomic::Ordering::Relaxed);
                        gave_up = Some(reason);
                    }
                }
                if !progress {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            }
            match gave_up {
                Some(reason) => Err(reason),
                None => Ok(completed),
            }
        })?;
        self.stderr.extend(
            completed
                .iter()
                .filter_map(|entry| entry.as_ref())
                .flat_map(|(_, stderr, _, _, _)| stderr.iter().copied()),
        );
        let mut groups = BTreeMap::new();
        let mut decided = false;
        let mut interrupted_chunks = false;
        for completed in completed {
            let (result, _, flow, traceback, interrupted) =
                completed.expect("fused par-map worker missing");
            // A chunk the stage interrupted did not end on its own: the
            // chunk that stopped the stage decides it.
            if interrupted && (result.is_err() || flow.is_some()) {
                interrupted_chunks = true;
                continue;
            }
            if result.is_err() || flow.is_some() {
                decided = true;
            }
            let result = result?;
            if let Some(flow) = flow {
                self.pending_value_block_flow = Some(flow);
                self.pending_traceback = traceback;
                break;
            }
            for (key, value) in result {
                lowered_reduce_group_insert(&mut groups, key, value, op, span)?;
            }
        }
        // Every stop has a cause: a chunk that ended on its own, found above,
        // or this evaluator giving the stage up, returned earlier.
        if interrupted_chunks && !decided {
            return Err(
                RuntimeError::new("par-map", "a chunk was stopped without a failure")
                    .with_span(span),
            );
        }
        Ok(LoweredValue::Map(Arc::new(
            groups
                .into_iter()
                .map(|(key, value)| (MapKey::from(key), value))
                .collect(),
        )))
    }

    pub(super) fn indexed_stage_name(tag: FullStageTag) -> &'static str {
        match tag {
            FullStageTag::TextLines => "text.lines",
            FullStageTag::JsonLines => "json.lines",
            FullStageTag::Where | FullStageTag::WhereBlock => "where",
            FullStageTag::Map | FullStageTag::MapBlock => "map",
            FullStageTag::FlatMap | FullStageTag::FlatMapBlock => "flat-map",
            FullStageTag::BytesChunks => "bytes.chunks",
            FullStageTag::BatchCount
            | FullStageTag::BatchMaxArgv
            | FullStageTag::BatchMaxBytes
            | FullStageTag::BatchLimits => "batch",
            FullStageTag::Shuffle => "shuffle",
            FullStageTag::Fold => "fold",
            FullStageTag::ReduceBy | FullStageTag::ReduceByConfigured => "reduce-by",
            FullStageTag::ParMap | FullStageTag::ParMapBlock => "par-map",
            FullStageTag::ParMapFlatMapReduceBy => "par-map",
            FullStageTag::Tee => "tee",
            FullStageTag::Each => "each",
            FullStageTag::TablePrint | FullStageTag::TablePrintConfigured => "table.print",
            FullStageTag::Enumerate => "enumerate",
            FullStageTag::Zip => "zip",
            FullStageTag::Sort => "sort",
            FullStageTag::SortBy => "sort-by",
            FullStageTag::GroupBy => "group-by",
            FullStageTag::CountBy | FullStageTag::Count => "count",
            FullStageTag::Any | FullStageTag::AnyBlock => "any",
            FullStageTag::All | FullStageTag::AllBlock => "all",
            FullStageTag::UniqueBy => "unique-by",
            FullStageTag::Sum => "sum",
            FullStageTag::Collect => "collect",
            FullStageTag::First => "first",
            FullStageTag::Last => "last",
            FullStageTag::Min => "min",
            FullStageTag::Max => "max",
            FullStageTag::Take => "take",
            FullStageTag::Drop => "drop",
            FullStageTag::Repeat => "repeat",
            FullStageTag::Range => "range",
        }
    }

    pub(super) fn indexed_field_projection<'execution>(
        execution: &'execution FullExecution<'_>,
        instruction: u32,
        item_slot: usize,
        span: Span,
    ) -> Result<Option<&'execution str>, RuntimeError> {
        Ok(Self::indexed_field_projection_with_span(execution, instruction, item_slot, span)?
            .map(|(name, _)| name))
    }

    pub(super) fn indexed_field_projection_with_span<'execution>(
        execution: &'execution FullExecution<'_>,
        instruction: u32,
        item_slot: usize,
        span: Span,
    ) -> Result<Option<(&'execution str, Span)>, RuntimeError> {
        let (tag, payload) = indexed_value(execution.instruction_id(instruction), span)?;
        if tag != FullTag::ExprField {
            return Ok(None);
        }
        let ExprView::Field {
            base,
            name,
            span: field_span,
        } =
            ExprView::decode(execution, tag, payload).map_err(|error| indexed_error(error, span))?
        else {
            unreachable!("field tag has a matching borrowed view")
        };
        let (base_tag, base_payload) = indexed_value(execution.instruction_id(base.raw()), span)?;
        if base_tag != FullTag::ExprParam {
            return Ok(None);
        }
        let ExprView::Param(slot) = ExprView::decode(execution, base_tag, base_payload)
            .map_err(|error| indexed_error(error, span))?
        else {
            unreachable!("parameter tag has a matching borrowed view")
        };
        Ok((slot == item_slot).then_some((name, field_span)))
    }

    fn indexed_binary_parts(
        execution: &FullExecution<'_>,
        instruction: u32,
        span: Span,
    ) -> Result<Option<(BinaryOp, u32, u32)>, RuntimeError> {
        let (tag, payload) = indexed_value(execution.instruction_id(instruction), span)?;
        if tag != FullTag::ExprBinary {
            return Ok(None);
        }
        let ExprView::Binary {
            op, left, right, ..
        } = ExprView::decode(execution, tag, payload).map_err(|error| indexed_error(error, span))?
        else {
            unreachable!("binary tag has a matching borrowed view")
        };
        Ok(Some((op, left.raw(), right.raw())))
    }

    fn indexed_string_literal<'execution>(
        execution: &'execution FullExecution<'_>,
        instruction: u32,
        span: Span,
    ) -> Result<Option<&'execution str>, RuntimeError> {
        let (tag, payload) = indexed_value(execution.instruction_id(instruction), span)?;
        if tag != FullTag::ExprStr {
            return Ok(None);
        }
        let ExprView::Str(value) =
            ExprView::decode(execution, tag, payload).map_err(|error| indexed_error(error, span))?
        else {
            unreachable!("string tag has a matching borrowed view")
        };
        Ok(Some(value))
    }

    fn indexed_string_comparison<'execution>(
        execution: &'execution FullExecution<'_>,
        instruction: u32,
        item_slot: usize,
        span: Span,
    ) -> Result<Option<(&'execution str, Span, BinaryOp, &'execution str)>, RuntimeError> {
        let Some((op @ (BinaryOp::Eq | BinaryOp::Ne), left, right)) =
            Self::indexed_binary_parts(execution, instruction, span)?
        else {
            return Ok(None);
        };
        if let Some((field, field_span)) =
            Self::indexed_field_projection_with_span(execution, left, item_slot, span)?
            && let Some(value) = Self::indexed_string_literal(execution, right, span)?
        {
            return Ok(Some((field, field_span, op, value)));
        }
        if let Some((field, field_span)) =
            Self::indexed_field_projection_with_span(execution, right, item_slot, span)?
            && let Some(value) = Self::indexed_string_literal(execution, left, span)?
        {
            return Ok(Some((field, field_span, op, value)));
        }
        Ok(None)
    }

    pub(super) fn indexed_item_predicate(
        execution: &FullExecution<'_>,
        instruction: u32,
        item_slot: usize,
        span: Span,
    ) -> Result<Option<IndexedItemPredicate>, RuntimeError> {
        let Some((op, left, right)) = Self::indexed_binary_parts(execution, instruction, span)?
        else {
            return Ok(None);
        };
        match op {
            BinaryOp::And | BinaryOp::Or => {
                let Some(left) = Self::indexed_item_predicate(execution, left, item_slot, span)?
                else {
                    return Ok(None);
                };
                let Some(right) = Self::indexed_item_predicate(execution, right, item_slot, span)?
                else {
                    return Ok(None);
                };
                Ok(Some(if op == BinaryOp::And {
                    IndexedItemPredicate::And(Box::new(left), Box::new(right))
                } else {
                    IndexedItemPredicate::Or(Box::new(left), Box::new(right))
                }))
            }
            BinaryOp::Eq | BinaryOp::Ne => {
                let Some((field, field_span, op, value)) =
                    Self::indexed_string_comparison(execution, instruction, item_slot, span)?
                else {
                    return Ok(None);
                };
                Ok(Some(IndexedItemPredicate::StringCompare {
                    field: Arc::from(field),
                    field_span,
                    op,
                    value: Arc::from(value),
                }))
            }
            _ => Ok(None),
        }
    }

    pub(super) fn eval_indexed_item_predicate(
        &mut self,
        predicate: &IndexedItemPredicate,
        item: &LoweredValue,
        span: Span,
    ) -> Result<bool, RuntimeError> {
        match predicate {
            IndexedItemPredicate::StringCompare {
                field,
                field_span,
                op,
                value,
            } => {
                let field_value = self
                    .indexed_borrowed_field_value(item, field, *field_span)?
                    .ok_or_else(|| {
                        RuntimeError::new("missing-field", field.as_ref()).with_span(*field_span)
                    })?;
                let equal = lowered_str_value(&field_value).is_some_and(|text| text == value.as_ref());
                Ok(if *op == BinaryOp::Eq { equal } else { !equal })
            }
            IndexedItemPredicate::And(left, right) => Ok(self
                .eval_indexed_item_predicate(left, item, span)?
                && self.eval_indexed_item_predicate(right, item, span)?),
            IndexedItemPredicate::Or(left, right) => Ok(self
                .eval_indexed_item_predicate(left, item, span)?
                || self.eval_indexed_item_predicate(right, item, span)?),
        }
    }

    pub(super) fn indexed_field_chain_ref<'slots>(
        execution: &FullExecution<'_>,
        instruction: u32,
        slots: &'slots [LoweredValue],
        span: Span,
    ) -> Result<Option<&'slots LoweredValue>, RuntimeError> {
        let (tag, payload) = indexed_value(execution.instruction_id(instruction), span)?;
        match tag {
            FullTag::ExprParam => {
                let ExprView::Param(slot) = ExprView::decode(execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("parameter tag has a matching borrowed view")
                };
                slots.get(slot).map(Some).ok_or_else(|| {
                    RuntimeError::new("indexed-ir", "field base slot is out of bounds")
                        .with_span(span)
                })
            }
            FullTag::ExprField => {
                let ExprView::Field {
                    base,
                    name,
                    span: field_span,
                } = ExprView::decode(execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("field tag has a matching borrowed view")
                };
                let Some(base) =
                    Self::indexed_field_chain_ref(execution, base.raw(), slots, field_span)?
                else {
                    return Ok(None);
                };
                match base {
                    LoweredValue::Record(record) | LoweredValue::Module(record) => {
                        record.get(name).map(Some).ok_or_else(|| {
                            RuntimeError::new("missing-field", name).with_span(field_span)
                        })
                    }
                    LoweredValue::RecordVec(record) => {
                        lowered_record_vec_get(record.as_slice(), name)
                            .map(Some)
                            .ok_or_else(|| {
                                RuntimeError::new("missing-field", name).with_span(field_span)
                            })
                    }
                    LoweredValue::Stats {
                        blanks,
                        code,
                        comments,
                    } => lowered_inline_stats_field_value(*blanks, *code, *comments, name)
                        .map(|_| None)
                        .ok_or_else(|| {
                            RuntimeError::new("missing-field", name).with_span(field_span)
                        }),
                    LoweredValue::StatsBlob(stats) => lowered_stats_field_value(stats, name)
                        .map(|_| None)
                        .ok_or_else(|| {
                            RuntimeError::new("missing-field", name).with_span(field_span)
                        }),
                    _ => Ok(None),
                }
            }
            _ => Ok(None),
        }
    }

    pub(super) fn indexed_record_fields<'execution>(
        execution: &'execution FullExecution<'_>,
        instruction: u32,
        span: Span,
    ) -> Result<Option<ListView<'execution, LoweredRecordEntry>>, RuntimeError> {
        let (tag, payload) = indexed_value(execution.instruction_id(instruction), span)?;
        if tag != FullTag::ExprRecord {
            return Ok(None);
        }
        let ExprView::Record(entries) =
            ExprView::decode(execution, tag, payload).map_err(|error| indexed_error(error, span))?
        else {
            unreachable!("record tag has a matching borrowed view")
        };
        Ok(Some(entries))
    }

    pub(super) fn indexed_reduce_projection<'execution>(
        execution: &'execution FullExecution<'_>,
        item_slot: usize,
        body: u32,
        value: u32,
        op: ReduceByOp,
        span: Span,
    ) -> Result<Option<LoweredReduceProjection<'execution>>, RuntimeError> {
        if op != ReduceByOp::Sum {
            return Ok(None);
        }
        let statements = execution
            .statement_block_view(body)
            .map_err(|error| indexed_error(error, span))?;
        if statements.len() != 0 {
            return Ok(None);
        }
        let Some(entries) = Self::indexed_record_fields(execution, value, span)? else {
            return Ok(None);
        };
        let mut key_field = None;
        let mut value_fields = None;
        for entry in entries {
            let (name, expr) = match entry.map_err(|error| indexed_error(error, span))? {
                RecordEntryView::Field(name, value) => (name, value.raw()),
                RecordEntryView::Spread(_) => return Ok(None),
            };
            match name.as_str().as_str() {
                "key" => {
                    key_field = Self::indexed_field_projection(execution, expr, item_slot, span)?;
                }
                "value" => {
                    let Some(fields) = Self::indexed_record_fields(execution, expr, span)? else {
                        return Ok(None);
                    };
                    let mut projected = Vec::with_capacity(fields.len());
                    for field in fields {
                        let (name, expr) = match field.map_err(|error| indexed_error(error, span))? {
                            RecordEntryView::Field(name, value) => (name, value.raw()),
                            RecordEntryView::Spread(_) => return Ok(None),
                        };
                        let Some(source) =
                            Self::indexed_field_projection(execution, expr, item_slot, span)?
                        else {
                            return Ok(None);
                        };
                        projected.push((name, source));
                    }
                    value_fields = Some(projected);
                }
                _ => return Ok(None),
            }
        }
        Ok(key_field
            .zip(value_fields)
            .map(|(key_field, value_fields)| LoweredReduceProjection {
                key_field,
                value_fields,
            }))
    }
}
