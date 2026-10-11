use super::{
    Arc, BLOCK_LIST, BLOCK_STATEMENTS, BTreeMap, BinaryOp, ControlFlow, DEFAULT_PAR_MAP_WORKERS,
    Evaluator, FullExecution, FullPayload, FullStageTag, FullTag, IndexedItemPredicate,
    IndexedPipelineItems, LoweredProjectedReduceState, LoweredReduceProjection, LoweredValue,
    MapKey, Name, ParMapItemOutcome, ReduceByOp, RuntimeError, Span, StmtFlow, TraceError,
    TraceKind, TracePayload, Traceback, btree_map, bytes_module, compare_lowered_sort_keys,
    indexed_decode, indexed_error, indexed_finish, indexed_optional_raw, indexed_raw,
    indexed_string, indexed_value, lowered_bytes_value, lowered_count_key, lowered_error_message,
    lowered_inline_stats_field_value, lowered_nonnegative_count, lowered_pipeline_input,
    lowered_pipeline_item_count, lowered_pipeline_record_list, lowered_record_vec_get,
    lowered_reduce_fields_owned, lowered_reduce_group_insert, lowered_reduce_key_value_owned,
    lowered_result_err_value, lowered_result_ok, lowered_sort_key_orderable,
    lowered_stats_field_value, lowered_str_parts, lowered_str_value, lowered_str_view_value,
    lowered_table_print_value, lowered_value_argv_len, lowered_value_from_runtime_any,
    runtime_error_from_value,
};

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

    pub(super) fn eval_indexed_optional_expr(
        &mut self,
        execution: &FullExecution<'_>,
        instruction: Option<u32>,
        slots: &mut [LoweredValue],
        span: Span,
    ) -> Result<ControlFlow<LoweredValue, Option<LoweredValue>>, RuntimeError> {
        let Some(instruction) = instruction else {
            return Ok(ControlFlow::Continue(None));
        };
        self.eval_indexed_expr(execution, instruction, slots, span)
            .map(|flow| flow.map_continue(Some))
    }

    /// Preserve an option expression's effects even when the stage currently
    /// runs serially; worker stages use the same positive-count boundary.
    pub(super) fn eval_indexed_jobs_option(
        &mut self,
        execution: &FullExecution<'_>,
        jobs: Option<u32>,
        slots: &mut [LoweredValue],
        span: Span,
    ) -> Result<ControlFlow<LoweredValue, Option<usize>>, RuntimeError> {
        if let Some(jobs) = jobs {
            let value = match self.eval_indexed_expr(execution, jobs, slots, span)? {
                ControlFlow::Continue(value) => value,
                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
            };
            match value {
                LoweredValue::Int(value) if value > 0 => {
                    return Ok(ControlFlow::Continue(Some(value as usize)));
                }
                LoweredValue::Int(_) => {
                    return Err(RuntimeError::new(
                        "stream-jobs",
                        "stream worker count must be positive",
                    )
                    .with_span(span));
                }
                value => {
                    return Err(RuntimeError::new(
                        "type-error",
                        format!(
                            "stream worker count expected Int, found {}",
                            value.type_name()
                        ),
                    )
                    .with_span(span));
                }
            }
        }
        Ok(ControlFlow::Continue(None))
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

    pub(super) fn eval_indexed_pipeline_descending(
        &mut self,
        execution: &FullExecution<'_>,
        descending: Option<u32>,
        slots: &mut [LoweredValue],
        span: Span,
    ) -> Result<bool, RuntimeError> {
        let Some(descending) = descending else {
            return Ok(false);
        };
        match self.eval_indexed_expr(execution, descending, slots, span)? {
            ControlFlow::Continue(LoweredValue::Bool(value)) => Ok(value),
            ControlFlow::Continue(value) => Err(RuntimeError::new(
                "type-error",
                format!("desc expected Bool, found {}", value.type_name()),
            )
            .with_span(span)),
            ControlFlow::Break(value) => Err(runtime_error_from_value(value.into_value(), span)),
        }
    }

    pub(super) fn indexed_field_projection<'program>(
        execution: &'program FullExecution<'program>,
        instruction: u32,
        item_slot: usize,
        span: Span,
    ) -> Result<Option<&'program str>, RuntimeError> {
        let (tag, mut payload) = indexed_value(execution.instruction_id(instruction), span)?;
        if tag != FullTag::ExprField {
            return Ok(None);
        }
        let base = indexed_raw(&mut payload, span)?;
        let name = indexed_string(&mut payload, execution, span)?;
        indexed_decode::<Span>(&mut payload, execution, span)?;
        indexed_finish(payload, span)?;
        let (base_tag, mut base_payload) = indexed_value(execution.instruction_id(base), span)?;
        if base_tag != FullTag::ExprParam {
            return Ok(None);
        }
        let slot = indexed_decode::<usize>(&mut base_payload, execution, span)?;
        indexed_finish(base_payload, span)?;
        Ok((slot == item_slot).then_some(name))
    }

    pub(super) fn indexed_field_chain_ref<'slots>(
        execution: &FullExecution<'_>,
        instruction: u32,
        slots: &'slots [LoweredValue],
        span: Span,
    ) -> Result<Option<&'slots LoweredValue>, RuntimeError> {
        let (tag, mut payload) = indexed_value(execution.instruction_id(instruction), span)?;
        match tag {
            FullTag::ExprParam => {
                let slot = indexed_decode::<usize>(&mut payload, execution, span)?;
                indexed_finish(payload, span)?;
                slots.get(slot).map(Some).ok_or_else(|| {
                    RuntimeError::new("indexed-ir", "field base slot is out of bounds")
                        .with_span(span)
                })
            }
            FullTag::ExprField => {
                let base = indexed_raw(&mut payload, span)?;
                let name = indexed_string(&mut payload, execution, span)?;
                let field_span = indexed_decode::<Span>(&mut payload, execution, span)?;
                indexed_finish(payload, span)?;
                let Some(base) = Self::indexed_field_chain_ref(execution, base, slots, field_span)?
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

    pub(super) fn indexed_string_literal<'program>(
        execution: &'program FullExecution<'program>,
        instruction: u32,
        span: Span,
    ) -> Result<Option<Arc<str>>, RuntimeError> {
        let (tag, mut payload) = indexed_value(execution.instruction_id(instruction), span)?;
        if tag != FullTag::ExprStr {
            return Ok(None);
        }
        let value = indexed_decode::<Arc<str>>(&mut payload, execution, span)?;
        indexed_finish(payload, span)?;
        Ok(Some(value))
    }

    pub(super) fn indexed_item_predicate<'program>(
        execution: &'program FullExecution<'program>,
        instruction: u32,
        item_slot: usize,
        span: Span,
    ) -> Result<Option<IndexedItemPredicate<'program>>, RuntimeError> {
        let (tag, mut payload) = indexed_value(execution.instruction_id(instruction), span)?;
        if tag != FullTag::ExprBinary {
            return Ok(None);
        }
        let op = indexed_decode::<BinaryOp>(&mut payload, execution, span)?;
        let left = indexed_raw(&mut payload, span)?;
        let right = indexed_raw(&mut payload, span)?;
        indexed_decode::<Span>(&mut payload, execution, span)?;
        indexed_finish(payload, span)?;
        if op == BinaryOp::And || op == BinaryOp::Or {
            let Some(left) = Self::indexed_item_predicate(execution, left, item_slot, span)? else {
                return Ok(None);
            };
            let Some(right) = Self::indexed_item_predicate(execution, right, item_slot, span)?
            else {
                return Ok(None);
            };
            return Ok(Some(if op == BinaryOp::And {
                IndexedItemPredicate::And(Box::new(left), Box::new(right))
            } else {
                IndexedItemPredicate::Or(Box::new(left), Box::new(right))
            }));
        }
        if op != BinaryOp::Eq && op != BinaryOp::Ne {
            return Ok(None);
        }
        if let Some(field) = Self::indexed_field_projection(execution, left, item_slot, span)?
            && let Some(value) = Self::indexed_string_literal(execution, right, span)?
        {
            return Ok(Some(IndexedItemPredicate::StringCompare {
                field,
                op,
                value,
            }));
        }
        if let Some(field) = Self::indexed_field_projection(execution, right, item_slot, span)?
            && let Some(value) = Self::indexed_string_literal(execution, left, span)?
        {
            return Ok(Some(IndexedItemPredicate::StringCompare {
                field,
                op,
                value,
            }));
        }
        Ok(None)
    }

    pub(super) fn eval_indexed_item_predicate(
        &mut self,
        predicate: &IndexedItemPredicate<'_>,
        item: &LoweredValue,
        span: Span,
    ) -> Result<bool, RuntimeError> {
        match predicate {
            IndexedItemPredicate::StringCompare { field, op, value } => {
                let field = self
                    .indexed_borrowed_field_value(item, field, span)?
                    .ok_or_else(|| RuntimeError::new("missing-field", *field).with_span(span))?;
                let equal = lowered_str_value(&field).is_some_and(|text| text == value.as_ref());
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

    pub(super) fn indexed_record_fields(
        execution: &FullExecution<'_>,
        instruction: u32,
        span: Span,
    ) -> Result<Option<Vec<(Name, u32)>>, RuntimeError> {
        let (tag, mut payload) = indexed_value(execution.instruction_id(instruction), span)?;
        if tag != FullTag::ExprRecord {
            return Ok(None);
        }
        let (_, mut entries) = execution
            .block(&mut payload, BLOCK_LIST)
            .map_err(|error| indexed_error(error, span))?;
        let len = indexed_raw(&mut entries, span)? as usize;
        indexed_finish(payload, span)?;
        let mut fields = Vec::with_capacity(len);
        for _ in 0..len {
            if indexed_raw(&mut entries, span)? != 0 {
                return Ok(None);
            }
            fields.push((
                indexed_decode::<Name>(&mut entries, execution, span)?,
                indexed_raw(&mut entries, span)?,
            ));
        }
        indexed_finish(entries, span)?;
        Ok(Some(fields))
    }

    pub(super) fn indexed_reduce_projection<'program>(
        execution: &'program FullExecution<'program>,
        item_slot: usize,
        body: u32,
        value: u32,
        op: ReduceByOp,
        span: Span,
    ) -> Result<Option<LoweredReduceProjection<'program>>, RuntimeError> {
        if op != ReduceByOp::Sum {
            return Ok(None);
        }
        let (_, mut statements) = execution
            .block_id(body, BLOCK_STATEMENTS)
            .map_err(|error| indexed_error(error, span))?;
        if indexed_raw(&mut statements, span)? != 0 {
            return Ok(None);
        }
        indexed_finish(statements, span)?;
        let Some(entries) = Self::indexed_record_fields(execution, value, span)? else {
            return Ok(None);
        };
        let mut key_field = None;
        let mut value_fields = None;
        for (name, expr) in entries {
            match name.as_str().as_str() {
                "key" => {
                    key_field = Self::indexed_field_projection(execution, expr, item_slot, span)?;
                }
                "value" => {
                    let Some(fields) = Self::indexed_record_fields(execution, expr, span)? else {
                        return Ok(None);
                    };
                    let mut projected = Vec::with_capacity(fields.len());
                    for (name, expr) in fields {
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

impl Evaluator {
    pub(super) fn eval_indexed_pipeline<'a>(
        &mut self,
        execution: &FullExecution<'a>,
        mut payload: FullPayload<'a>,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let result = {
        let input = indexed_raw(&mut payload, call_span)?;
        let (_, mut stages) = execution
            .block(&mut payload, BLOCK_LIST)
            .map_err(|error| indexed_error(error, call_span))?;
        let stage_count = indexed_raw(&mut stages, call_span)? as usize;
        let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
        indexed_finish(payload, call_span)?;
        let current = match self.eval_indexed_expr(execution, input, slots, span)? {
            ControlFlow::Continue(value) => value,
            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
        };
        let mut current = lowered_pipeline_input(current, span)?;
        let mut consumed = 0;
        if let Some((flow, count)) = self.eval_indexed_live_serial_prefix(
            execution,
            &mut current,
            stages,
            stage_count,
            slots,
            span,
            call_span,
        )? {
            consumed = count;
            current = match flow {
                ControlFlow::Continue(value) => value,
                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
            };
            for _ in 0..consumed {
                indexed_raw(&mut stages, span)?;
            }
        }
        for _ in consumed..stage_count {
            let stage = indexed_raw(&mut stages, span)?;
            let (tag, mut stage_payload) = execution
                .stage_id(stage)
                .map_err(|error| indexed_error(error, span))?;
            let stage_name = Self::indexed_stage_name(tag);
            self.trace_enter(
                TraceKind::StreamStageEnter,
                Some(span),
                Some(stage_name),
                TracePayload::StreamStage {
                    stage: stage_name.to_string(),
                    item_count: lowered_pipeline_item_count(&current),
                    error: None,
                },
            );
            // Contain early returns so every entered stage closes its trace.
            let stage_result =
                (|| -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
                    let value = match tag {
                        FullStageTag::TextLines => {
                            indexed_finish(stage_payload, span)?;
                            let Some((text, start, end)) = lowered_str_parts(&current)
                            else {
                                return Err(RuntimeError::new(
                                    "type-error",
                                    format!(
                                        "text.lines expected Str, found {}",
                                        current.type_name()
                                    ),
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
                                let view_end =
                                    if line_end > cursor && bytes[line_end - 1] == b'\r' {
                                        line_end - 1
                                    } else {
                                        line_end
                                    };
                                lines.push(lowered_str_view_value(
                                    text.clone(),
                                    cursor,
                                    view_end,
                                ));
                                let Some(newline) = newline else {
                                    break;
                                };
                                cursor = newline + 1;
                            }
                            LoweredValue::List(lines)
                        }
                        FullStageTag::JsonLines => {
                            indexed_finish(stage_payload, span)?;
                            let Some(text) = lowered_str_value(&current) else {
                                return Err(RuntimeError::new(
                                    "type-error",
                                    format!(
                                        "json.lines expected Str, found {}",
                                        current.type_name()
                                    ),
                                )
                                .with_span(span));
                            };
                            let values =
                                crate::modules::json::parse_json_lines(text, span)?;
                            let mut lowered = Vec::with_capacity(values.len());
                            for value in values {
                                let Some(value) = lowered_value_from_runtime_any(&value)
                                else {
                                    return Err(RuntimeError::new(
                                        "type-error",
                                        format!(
                                            "json.lines produced unsupported {}",
                                            value.type_name()
                                        ),
                                    )
                                    .with_span(span));
                                };
                                lowered.push(value);
                            }
                            LoweredValue::List(lowered)
                        }
                        FullStageTag::Enumerate => {
                            indexed_finish(stage_payload, span)?;
                            let items = self.lowered_pipeline_input_items(current, span)?;
                            LoweredValue::List(
                                items
                                    .into_iter()
                                    .enumerate()
                                    .map(|(index, value)| {
                                        LoweredValue::Record(Arc::new(btree_map(vec![
                                            (
                                                Arc::from("index"),
                                                LoweredValue::Int(index as i64),
                                            ),
                                            (Arc::from("value"), value),
                                        ])))
                                    })
                                    .collect(),
                            )
                        }
                        FullStageTag::Zip => {
                            let other = indexed_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let other = match self
                                .eval_indexed_expr(execution, other, slots, span)?
                            {
                                ControlFlow::Continue(value) => value,
                                ControlFlow::Break(value) => {
                                    return Ok(ControlFlow::Break(value));
                                }
                            };
                            let right =
                                self.lowered_list_items(other, span, "zip expected List")?;
                            let mut left = IndexedPipelineItems::new(self, current, span)?;
                            let known_left = match &left {
                                IndexedPipelineItems::Materialized(values) => values.len(),
                                IndexedPipelineItems::Live { prefix, .. } => prefix.len(),
                            };
                            let driven = (|| -> Result<Vec<LoweredValue>, RuntimeError> {
                                let mut pairs =
                                    Vec::with_capacity(known_left.min(right.len()));
                                for right in right {
                                    let Some(item) = left.next(self, span)? else {
                                        break;
                                    };
                                    pairs.push(LoweredValue::Record(Arc::new(btree_map(
                                        vec![
                                            (Arc::from("left"), item),
                                            (Arc::from("right"), right),
                                        ],
                                    ))));
                                }
                                Ok(pairs)
                            })();
                            let close = left.cancel(self, span);
                            match driven {
                                Ok(pairs) => {
                                    close?;
                                    LoweredValue::List(pairs)
                                }
                                Err(error) => {
                                    let _ = close;
                                    return Err(error);
                                }
                            }
                        }
                        FullStageTag::Sort => {
                            let descending =
                                indexed_optional_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let mut items =
                                self.lowered_pipeline_input_items(current, span)?;
                            items.sort_by(compare_lowered_sort_keys);
                            if self.eval_indexed_pipeline_descending(
                                execution, descending, slots, span,
                            )? {
                                items.reverse();
                            }
                            LoweredValue::List(items)
                        }
                        FullStageTag::SortBy => {
                            let slot = indexed_decode::<usize>(
                                &mut stage_payload,
                                execution,
                                span,
                            )?;
                            let key = indexed_raw(&mut stage_payload, span)?;
                            let descending =
                                indexed_optional_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let descending = self.eval_indexed_pipeline_descending(
                                execution, descending, slots, span,
                            )?;
                            let items = self.lowered_pipeline_input_items(current, span)?;
                            let projection =
                                Self::indexed_field_projection(execution, key, slot, span)?;
                            let mut keyed = Vec::with_capacity(items.len());
                            for item in items {
                                if let Some(field) = projection
                                    && let Some(key) = self
                                        .indexed_borrowed_field_value(&item, field, span)?
                                {
                                    keyed.push((key, item));
                                    continue;
                                }
                                slots[slot] = item;
                                let key = match self
                                    .eval_indexed_expr(execution, key, slots, span)?
                                {
                                    ControlFlow::Continue(value) => value,
                                    ControlFlow::Break(value) => {
                                        return Ok(ControlFlow::Break(value));
                                    }
                                };
                                let item =
                                    std::mem::replace(&mut slots[slot], LoweredValue::Unit);
                                keyed.push((key, item));
                            }
                            if let Some(key) = keyed
                                .iter()
                                .find(|(key, _)| !lowered_sort_key_orderable(key))
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
                            keyed.sort_by(|(left, _), (right, _)| {
                                compare_lowered_sort_keys(left, right)
                            });
                            if descending {
                                keyed.reverse();
                            }
                            LoweredValue::List(
                                keyed.into_iter().map(|(_, item)| item).collect(),
                            )
                        }
                        FullStageTag::GroupBy => {
                            let slot = indexed_decode::<usize>(
                                &mut stage_payload,
                                execution,
                                span,
                            )?;
                            let key = indexed_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let projection =
                                Self::indexed_field_projection(execution, key, slot, span)?;
                            let mut items = IndexedPipelineItems::new(self, current, span)?;
                            let driven: Result<
                                ControlFlow<
                                    LoweredValue,
                                    Vec<(LoweredValue, Vec<LoweredValue>)>,
                                >,
                                RuntimeError,
                            > = (|| {
                                let mut groups: Vec<(LoweredValue, Vec<LoweredValue>)> =
                                    Vec::new();
                                while let Some(item) = items.next(self, span)? {
                                    let mut item = Some(item);
                                    let key = if let Some(field) = projection
                                        && let Some(key) = self
                                            .indexed_borrowed_field_value(
                                                item.as_ref()
                                                    .expect("group item is present"),
                                                field,
                                                span,
                                            )? {
                                        key
                                    } else {
                                        slots[slot] =
                                            item.take().expect("group item is present");
                                        match self.eval_indexed_expr(
                                            execution, key, slots, span,
                                        )? {
                                            ControlFlow::Continue(value) => value,
                                            ControlFlow::Break(value) => {
                                                return Ok(ControlFlow::Break(value));
                                            }
                                        }
                                    };
                                    let item = item.unwrap_or_else(|| {
                                        std::mem::replace(
                                            &mut slots[slot],
                                            LoweredValue::Unit,
                                        )
                                    });
                                    if let Some((_, group_items)) = groups
                                        .iter_mut()
                                        .find(|(existing, _)| existing == &key)
                                    {
                                        group_items.push(item);
                                    } else {
                                        groups.push((key, vec![item]));
                                    }
                                }
                                Ok(ControlFlow::Continue(groups))
                            })();
                            let close = items.cancel(self, span);
                            let groups = match driven {
                                Err(error) => {
                                    let _ = close;
                                    return Err(error);
                                }
                                Ok(ControlFlow::Break(value)) => {
                                    close?;
                                    return Ok(ControlFlow::Break(value));
                                }
                                Ok(ControlFlow::Continue(groups)) => {
                                    close?;
                                    groups
                                }
                            };
                            slots[slot] = LoweredValue::Unit;
                            LoweredValue::List(
                                groups
                                    .into_iter()
                                    .map(|(key, items)| {
                                        LoweredValue::Record(Arc::new(btree_map(vec![
                                            (Arc::from("items"), LoweredValue::List(items)),
                                            (Arc::from("key"), key),
                                        ])))
                                    })
                                    .collect(),
                            )
                        }
                        FullStageTag::CountBy => {
                            let slot = indexed_decode::<usize>(
                                &mut stage_payload,
                                execution,
                                span,
                            )?;
                            let key = indexed_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let mut items = IndexedPipelineItems::new(self, current, span)?;
                            let driven: Result<
                                ControlFlow<LoweredValue, BTreeMap<String, LoweredValue>>,
                                RuntimeError,
                            > = (|| {
                                let mut counts = BTreeMap::new();
                                while let Some(item) = items.next(self, span)? {
                                    slots[slot] = item;
                                    let key = match self
                                        .eval_indexed_expr(execution, key, slots, span)?
                                    {
                                        ControlFlow::Continue(value) => {
                                            lowered_count_key(&value, span)?
                                        }
                                        ControlFlow::Break(value) => {
                                            return Ok(ControlFlow::Break(value));
                                        }
                                    };
                                    let entry =
                                        counts.entry(key).or_insert(LoweredValue::Int(0));
                                    let LoweredValue::Int(count) = entry else {
                                        unreachable!("count accumulator only stores ints");
                                    };
                                    *count += 1;
                                }
                                Ok(ControlFlow::Continue(counts))
                            })();
                            let close = items.cancel(self, span);
                            let counts = match driven {
                                Err(error) => {
                                    let _ = close;
                                    return Err(error);
                                }
                                Ok(ControlFlow::Break(value)) => {
                                    close?;
                                    return Ok(ControlFlow::Break(value));
                                }
                                Ok(ControlFlow::Continue(counts)) => {
                                    close?;
                                    counts
                                }
                            };
                            slots[slot] = LoweredValue::Unit;
                            LoweredValue::Map(Arc::new(
                                counts
                                    .into_iter()
                                    .map(|(key, value)| (MapKey::from(key), value))
                                    .collect(),
                            ))
                        }
                        FullStageTag::UniqueBy => {
                            let slot = indexed_decode::<usize>(
                                &mut stage_payload,
                                execution,
                                span,
                            )?;
                            let key = indexed_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let mut items = IndexedPipelineItems::new(self, current, span)?;
                            let known_items = match &items {
                                IndexedPipelineItems::Materialized(values) => values.len(),
                                IndexedPipelineItems::Live { prefix, .. } => prefix.len(),
                            };
                            let driven: Result<
                                ControlFlow<LoweredValue, Vec<LoweredValue>>,
                                RuntimeError,
                            > = (|| {
                                let mut seen = Vec::new();
                                let mut unique = Vec::with_capacity(known_items);
                                while let Some(item) = items.next(self, span)? {
                                    slots[slot] = item;
                                    let key = match self
                                        .eval_indexed_expr(execution, key, slots, span)?
                                    {
                                        ControlFlow::Continue(value) => value,
                                        ControlFlow::Break(value) => {
                                            return Ok(ControlFlow::Break(value));
                                        }
                                    };
                                    let item = std::mem::replace(
                                        &mut slots[slot],
                                        LoweredValue::Unit,
                                    );
                                    if !seen.iter().any(|existing| existing == &key) {
                                        seen.push(key);
                                        unique.push(item);
                                    }
                                }
                                Ok(ControlFlow::Continue(unique))
                            })();
                            let close = items.cancel(self, span);
                            let unique = match driven {
                                Err(error) => {
                                    let _ = close;
                                    return Err(error);
                                }
                                Ok(ControlFlow::Break(value)) => {
                                    close?;
                                    return Ok(ControlFlow::Break(value));
                                }
                                Ok(ControlFlow::Continue(unique)) => {
                                    close?;
                                    unique
                                }
                            };
                            slots[slot] = LoweredValue::Unit;
                            LoweredValue::List(unique)
                        }
                        FullStageTag::Where => {
                            let slot = indexed_decode::<usize>(
                                &mut stage_payload,
                                execution,
                                span,
                            )?;
                            let predicate = indexed_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let items = self.lowered_pipeline_input_items(current, span)?;
                            let item_predicate = Self::indexed_item_predicate(
                                execution, predicate, slot, span,
                            )?;
                            let mut filtered = Vec::new();
                            for item in items {
                                if let Some(predicate) = &item_predicate {
                                    if self.eval_indexed_item_predicate(
                                        predicate, &item, span,
                                    )? {
                                        filtered.push(item);
                                    }
                                    continue;
                                }
                                slots[slot] = item;
                                let keep = match self
                                    .eval_indexed_bool(execution, predicate, slots, span)?
                                {
                                    ControlFlow::Continue(value) => value,
                                    ControlFlow::Break(value) => {
                                        return Ok(ControlFlow::Break(value));
                                    }
                                };
                                let item =
                                    std::mem::replace(&mut slots[slot], LoweredValue::Unit);
                                if keep {
                                    filtered.push(item);
                                }
                            }
                            LoweredValue::List(filtered)
                        }
                        FullStageTag::WhereBlock => {
                            let slot = indexed_decode::<usize>(
                                &mut stage_payload,
                                execution,
                                span,
                            )?;
                            let body = indexed_raw(&mut stage_payload, span)?;
                            let value = indexed_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let items = self.lowered_pipeline_input_items(current, span)?;
                            let mut filtered = Vec::new();
                            for item in items {
                                slots[slot] = item;
                                match self.eval_indexed_statement_block(
                                    execution, body, slots, call_span,
                                )? {
                                    StmtFlow::None => {}
                                    flow => {
                                        return Ok(
                                            self.preserve_lexical_expression_flow(flow)
                                        );
                                    }
                                }
                                let keep = match self
                                    .eval_indexed_bool(execution, value, slots, span)?
                                {
                                    ControlFlow::Continue(value) => value,
                                    ControlFlow::Break(value) => {
                                        return Ok(ControlFlow::Break(value));
                                    }
                                };
                                let item =
                                    std::mem::replace(&mut slots[slot], LoweredValue::Unit);
                                if keep {
                                    filtered.push(item);
                                }
                            }
                            LoweredValue::List(filtered)
                        }
                        FullStageTag::Any | FullStageTag::All => {
                            let slot = indexed_decode::<usize>(
                                &mut stage_payload,
                                execution,
                                span,
                            )?;
                            let predicate = indexed_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let items = self.lowered_pipeline_input_items(current, span)?;
                            let all = tag == FullStageTag::All;
                            let mut matched = all;
                            for item in items {
                                slots[slot] = item;
                                let keep = match self
                                    .eval_indexed_bool(execution, predicate, slots, span)?
                                {
                                    ControlFlow::Continue(value) => value,
                                    ControlFlow::Break(value) => {
                                        return Ok(ControlFlow::Break(value));
                                    }
                                };
                                slots[slot] = LoweredValue::Unit;
                                if keep != all {
                                    matched = !all;
                                    break;
                                }
                            }
                            LoweredValue::Bool(matched)
                        }
                        FullStageTag::AnyBlock | FullStageTag::AllBlock => {
                            let slot = indexed_decode::<usize>(
                                &mut stage_payload,
                                execution,
                                span,
                            )?;
                            let body = indexed_raw(&mut stage_payload, span)?;
                            let value = indexed_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let items = self.lowered_pipeline_input_items(current, span)?;
                            let all = tag == FullStageTag::AllBlock;
                            let mut matched = all;
                            for item in items {
                                slots[slot] = item;
                                match self.eval_indexed_statement_block(
                                    execution, body, slots, call_span,
                                )? {
                                    StmtFlow::None => {}
                                    flow => {
                                        return Ok(
                                            self.preserve_lexical_expression_flow(flow)
                                        );
                                    }
                                }
                                let keep = match self
                                    .eval_indexed_bool(execution, value, slots, span)?
                                {
                                    ControlFlow::Continue(value) => value,
                                    ControlFlow::Break(value) => {
                                        return Ok(ControlFlow::Break(value));
                                    }
                                };
                                slots[slot] = LoweredValue::Unit;
                                if keep != all {
                                    matched = !all;
                                    break;
                                }
                            }
                            LoweredValue::Bool(matched)
                        }
                        FullStageTag::Map => {
                            let slot = indexed_decode::<usize>(
                                &mut stage_payload,
                                execution,
                                span,
                            )?;
                            let value = indexed_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let items = self.lowered_pipeline_input_items(current, span)?;
                            let projection = Self::indexed_field_projection(
                                execution, value, slot, span,
                            )?;
                            let mut mapped = Vec::with_capacity(items.len());
                            for (index, item) in items.into_iter().enumerate() {
                                if let Some(field) = projection
                                    && let Some(value) = self
                                        .indexed_borrowed_field_value(&item, field, span)?
                                {
                                    mapped.push(value);
                                    continue;
                                }
                                slots[slot] = item;
                                let value = match self
                                    .eval_indexed_expr(execution, value, slots, span)
                                {
                                    Ok(ControlFlow::Continue(value)) => value,
                                    Ok(ControlFlow::Break(value)) => {
                                        return Ok(ControlFlow::Break(value));
                                    }
                                    Err(error) => {
                                        return Err(self.stream_item_runtime_error(
                                            "map", index, error,
                                        ));
                                    }
                                };
                                mapped.push(value);
                            }
                            LoweredValue::List(mapped)
                        }
                        FullStageTag::MapBlock | FullStageTag::FlatMapBlock => {
                            let slot = indexed_decode::<usize>(
                                &mut stage_payload,
                                execution,
                                span,
                            )?;
                            let body = indexed_raw(&mut stage_payload, span)?;
                            let value = indexed_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let flat = tag == FullStageTag::FlatMapBlock;
                            let items = self.lowered_pipeline_input_items(current, span)?;
                            let mut mapped = Vec::with_capacity(items.len());
                            for item in items {
                                slots[slot] = item;
                                match self.eval_indexed_statement_block(
                                    execution, body, slots, call_span,
                                )? {
                                    StmtFlow::None => {}
                                    flow => {
                                        return Ok(
                                            self.preserve_lexical_expression_flow(flow)
                                        );
                                    }
                                }
                                let value = match self
                                    .eval_indexed_expr(execution, value, slots, span)?
                                {
                                    ControlFlow::Continue(value) => value,
                                    ControlFlow::Break(value) => {
                                        return Ok(ControlFlow::Break(value));
                                    }
                                };
                                if flat {
                                    mapped.extend(self.lowered_list_items(
                                        value,
                                        span,
                                        "flat-map expected List",
                                    )?);
                                } else {
                                    mapped.push(value);
                                }
                            }
                            LoweredValue::List(mapped)
                        }
                        FullStageTag::FlatMap => {
                            let slot = indexed_decode::<usize>(
                                &mut stage_payload,
                                execution,
                                span,
                            )?;
                            let value = indexed_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let items = self.lowered_pipeline_input_items(current, span)?;
                            let mut mapped = Vec::new();
                            for item in items {
                                slots[slot] = item;
                                let value = match self
                                    .eval_indexed_expr(execution, value, slots, span)?
                                {
                                    ControlFlow::Continue(value) => value,
                                    ControlFlow::Break(value) => {
                                        return Ok(ControlFlow::Break(value));
                                    }
                                };
                                mapped.extend(self.lowered_list_items(
                                    value,
                                    span,
                                    "flat-map expected List",
                                )?);
                            }
                            LoweredValue::List(mapped)
                        }
                        FullStageTag::BytesChunks => {
                            let size = indexed_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let bytes = lowered_bytes_value(&current)
                                .ok_or_else(|| {
                                    RuntimeError::new(
                                        "type-error",
                                        format!(
                                            "bytes.chunks expected Bytes, found {}",
                                            current.type_name()
                                        ),
                                    )
                                    .with_span(span)
                                })?
                                .to_vec();
                            let size = match self
                                .eval_indexed_expr(execution, size, slots, span)?
                            {
                                ControlFlow::Continue(LoweredValue::Int(value))
                                    if value > 0 =>
                                {
                                    value
                                }
                                ControlFlow::Continue(LoweredValue::Int(_)) => {
                                    return Err(RuntimeError::new(
                                        "bytes-chunks",
                                        "chunk size must be positive",
                                    )
                                    .with_span(span));
                                }
                                ControlFlow::Continue(value) => {
                                    return Err(RuntimeError::new(
                                        "type-error",
                                        format!(
                                            "bytes.chunks size expected Int, found {}",
                                            value.type_name()
                                        ),
                                    )
                                    .with_span(span));
                                }
                                ControlFlow::Break(value) => {
                                    return Ok(ControlFlow::Break(value));
                                }
                            };
                            let chunks = bytes_module::chunks(bytes, size, span)?;
                            let mut lowered = Vec::with_capacity(chunks.len());
                            for chunk in chunks {
                                let Some(chunk) = lowered_value_from_runtime_any(&chunk)
                                else {
                                    return Err(RuntimeError::new(
                                        "type-error",
                                        format!(
                                            "bytes.chunks produced unsupported {}",
                                            chunk.type_name()
                                        ),
                                    )
                                    .with_span(span));
                                };
                                lowered.push(chunk);
                            }
                            LoweredValue::List(lowered)
                        }
                        FullStageTag::BatchLimits => {
                            let configuration = indexed_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let configuration = match self.eval_indexed_expr(
                                execution,
                                configuration,
                                slots,
                                span,
                            )? {
                                ControlFlow::Continue(value) => value,
                                ControlFlow::Break(value) => {
                                    return Ok(ControlFlow::Break(value));
                                }
                            };
                            let fields = match configuration {
                                LoweredValue::Record(fields) => fields,
                                LoweredValue::RecordVec(fields) => Arc::new(
                                    fields
                                        .iter()
                                        .map(|(name, value)| {
                                            (
                                                Arc::<str>::from(name.as_str().as_str()),
                                                value.clone(),
                                            )
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
                            let positive_limit =
                                |name: &str| -> Result<Option<usize>, RuntimeError> {
                                    match fields.get(name) {
                                        None => Ok(None),
                                        Some(LoweredValue::Int(value)) if *value > 0 => {
                                            Ok(Some(*value as usize))
                                        }
                                        _ => Err(RuntimeError::new(
                                            "stream-batch",
                                            format!("batch {name} must be a positive Int"),
                                        )
                                        .with_span(span)),
                                    }
                                };
                            let count = positive_limit("count")?;
                            let max_bytes = positive_limit("max_bytes")?;
                            let max_argv = match fields.get("max_argv") {
                                None | Some(LoweredValue::Bool(false)) => None,
                                Some(LoweredValue::Bool(true)) => Some(
                                    super::super::super::stream::platform_arg_max()
                                        .saturating_sub(4096)
                                        .clamp(1, 128 * 1024),
                                ),
                                _ => {
                                    return Err(RuntimeError::new(
                                        "type-error",
                                        "batch max_argv must be Bool",
                                    )
                                    .with_span(span));
                                }
                            };
                            if count.is_none() && max_bytes.is_none() && max_argv.is_none()
                            {
                                return Err(RuntimeError::new(
                                    "stream-batch",
                                    "batch requires an enabled limit",
                                )
                                .with_span(span));
                            }
                            let mut items = IndexedPipelineItems::new(self, current, span)?;
                            let driven = (|| -> Result<Vec<LoweredValue>, RuntimeError> {
                                let mut batches = Vec::new();
                                let mut batch = Vec::new();
                                let mut bytes = 0usize;
                                let mut argv_bytes = 0usize;
                                while let Some(item) = items.next(self, span)? {
                                    let item_bytes =
                                        if max_bytes.is_some() || max_argv.is_some() {
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
                                    let argv_cost = item_bytes
                                        .saturating_add(usize::from(!batch.is_empty()));
                                    let full = count
                                        .is_some_and(|limit| batch.len() >= limit)
                                        || max_bytes.is_some_and(|limit| {
                                            bytes.saturating_add(item_bytes) > limit
                                        })
                                        || max_argv.is_some_and(|limit| {
                                            argv_bytes.saturating_add(argv_cost) > limit
                                        });
                                    if !batch.is_empty() && full {
                                        batches.push(LoweredValue::List(std::mem::take(
                                            &mut batch,
                                        )));
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
                            let close = items.cancel(self, span);
                            match driven {
                                Ok(batches) => {
                                    close?;
                                    LoweredValue::List(batches)
                                }
                                Err(error) => {
                                    let _ = close;
                                    return Err(error);
                                }
                            }
                        }
                        FullStageTag::BatchCount => {
                            let count = indexed_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let count = match self
                                .eval_indexed_expr(execution, count, slots, span)?
                            {
                                ControlFlow::Continue(LoweredValue::Int(value))
                                    if value > 0 =>
                                {
                                    value as usize
                                }
                                ControlFlow::Continue(LoweredValue::Int(_)) => {
                                    return Err(RuntimeError::new(
                                        "stream-stage-option",
                                        "count must be positive",
                                    )
                                    .with_span(span));
                                }
                                ControlFlow::Continue(value) => {
                                    return Err(RuntimeError::new(
                                        "type-error",
                                        format!(
                                            "count expected Int, found {}",
                                            value.type_name()
                                        ),
                                    )
                                    .with_span(span));
                                }
                                ControlFlow::Break(value) => {
                                    return Ok(ControlFlow::Break(value));
                                }
                            };
                            let items = self.lowered_pipeline_input_items(current, span)?;
                            let mut batches = Vec::new();
                            let mut batch = Vec::with_capacity(count);
                            for item in items {
                                batch.push(item);
                                if batch.len() == count {
                                    batches.push(LoweredValue::List(std::mem::take(
                                        &mut batch,
                                    )));
                                    batch = Vec::with_capacity(count);
                                }
                            }
                            if !batch.is_empty() {
                                batches.push(LoweredValue::List(batch));
                            }
                            LoweredValue::List(batches)
                        }
                        FullStageTag::BatchMaxArgv | FullStageTag::BatchMaxBytes => {
                            let limit = if tag == FullStageTag::BatchMaxArgv {
                                let max_argv =
                                    indexed_optional_raw(&mut stage_payload, span)?;
                                match max_argv {
                                    Some(expr) => {
                                        match self.eval_indexed_expr(
                                            execution, expr, slots, span,
                                        )? {
                                            ControlFlow::Continue(value) => {
                                                lowered_nonnegative_count(value, span)?
                                            }
                                            ControlFlow::Break(value) => {
                                                return Ok(ControlFlow::Break(value));
                                            }
                                        }
                                    }
                                    None => super::super::super::stream::platform_arg_max()
                                        .saturating_sub(4096)
                                        .clamp(1, 128 * 1024),
                                }
                            } else {
                                let max_bytes = indexed_raw(&mut stage_payload, span)?;
                                match self
                                    .eval_indexed_expr(execution, max_bytes, slots, span)?
                                {
                                    ControlFlow::Continue(value) => {
                                        lowered_nonnegative_count(value, span)?
                                    }
                                    ControlFlow::Break(value) => {
                                        return Ok(ControlFlow::Break(value));
                                    }
                                }
                            };
                            indexed_finish(stage_payload, span)?;
                            let mut items = IndexedPipelineItems::new(self, current, span)?;
                            let driven = (|| -> Result<Vec<LoweredValue>, RuntimeError> {
                                let mut batches = Vec::new();
                                let mut batch = Vec::new();
                                let mut batch_len = 0usize;
                                while let Some(item) = items.next(self, span)? {
                                    let item_len = lowered_value_argv_len(&item);
                                    if tag == FullStageTag::BatchMaxBytes
                                        && item_len > limit
                                    {
                                        return Err(RuntimeError::new(
                                            "argv-limit",
                                            "batch item exceeds byte budget",
                                        )
                                        .with_span(span));
                                    }
                                    let separator = usize::from(
                                        tag == FullStageTag::BatchMaxArgv
                                            && !batch.is_empty(),
                                    );
                                    if !batch.is_empty()
                                        && batch_len + separator + item_len > limit
                                    {
                                        batches.push(LoweredValue::List(std::mem::take(
                                            &mut batch,
                                        )));
                                        batch_len = 0;
                                    }
                                    let separator = usize::from(
                                        tag == FullStageTag::BatchMaxArgv
                                            && !batch.is_empty(),
                                    );
                                    batch_len += separator + item_len;
                                    batch.push(item);
                                }
                                if !batch.is_empty() {
                                    batches.push(LoweredValue::List(batch));
                                }
                                Ok(batches)
                            })();
                            let close = items.cancel(self, span);
                            match driven {
                                Ok(batches) => {
                                    close?;
                                    LoweredValue::List(batches)
                                }
                                Err(error) => {
                                    let _ = close;
                                    return Err(error);
                                }
                            }
                        }
                        FullStageTag::Shuffle => {
                            let seed = indexed_optional_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let seed = match seed {
                                Some(seed) => {
                                    match self
                                        .eval_indexed_expr(execution, seed, slots, span)?
                                    {
                                        ControlFlow::Continue(LoweredValue::Int(value)) => {
                                            value as u64
                                        }
                                        ControlFlow::Continue(value) => {
                                            return Err(RuntimeError::new(
                                                "type-error",
                                                format!(
                                                    "shuffle seed expected Int, found {}",
                                                    value.type_name()
                                                ),
                                            )
                                            .with_span(span));
                                        }
                                        ControlFlow::Break(value) => {
                                            return Ok(ControlFlow::Break(value));
                                        }
                                    }
                                }
                                None => 0,
                            };
                            let mut items =
                                self.lowered_pipeline_input_items(current, span)?;
                            let mut state = seed
                                ^ (items.len() as u64).wrapping_mul(0x9e3779b97f4a7c15);
                            for index in (1..items.len()).rev() {
                                state =
                                    state.wrapping_mul(6364136223846793005).wrapping_add(1);
                                let swap = (state as usize) % (index + 1);
                                items.swap(index, swap);
                            }
                            LoweredValue::List(items)
                        }
                        FullStageTag::Fold => {
                            let acc_slot = indexed_decode::<usize>(
                                &mut stage_payload,
                                execution,
                                span,
                            )?;
                            let item_slot = indexed_decode::<usize>(
                                &mut stage_payload,
                                execution,
                                span,
                            )?;
                            let initial = indexed_raw(&mut stage_payload, span)?;
                            let body = indexed_raw(&mut stage_payload, span)?;
                            let value = indexed_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let mut acc = match self
                                .eval_indexed_expr(execution, initial, slots, span)?
                            {
                                ControlFlow::Continue(value) => value,
                                ControlFlow::Break(value) => {
                                    return Ok(ControlFlow::Break(value));
                                }
                            };
                            let mut items = IndexedPipelineItems::new(self, current, span)?;
                            let driven = (|| -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
                        while let Some(item) = items.next(self, span)? {
                            slots[acc_slot] = acc;
                            slots[item_slot] = item;
                            match self.eval_indexed_statement_block(
                                execution,
                                body,
                                slots,
                                span,
                            )? {
                                StmtFlow::None => {}
                                flow => return Ok(self.preserve_lexical_expression_flow(flow)),
                            }
                            acc = match self.eval_indexed_expr(execution, value, slots, span)? {
                                ControlFlow::Continue(value) => value,
                                ControlFlow::Break(value) => {
                                    return Ok(ControlFlow::Break(value));
                                }
                            };
                        }
                        Ok(ControlFlow::Continue(acc))
                    })();
                            let close = items.cancel(self, span);
                            let acc = match driven {
                                Err(error) => {
                                    let _ = close;
                                    return Err(error);
                                }
                                Ok(ControlFlow::Break(value)) => {
                                    close?;
                                    return Ok(ControlFlow::Break(value));
                                }
                                Ok(ControlFlow::Continue(acc)) => {
                                    close?;
                                    acc
                                }
                            };
                            slots[acc_slot] = LoweredValue::Unit;
                            slots[item_slot] = LoweredValue::Unit;
                            acc
                        }
                        FullStageTag::ReduceBy | FullStageTag::ReduceByConfigured => {
                            let item_slot = indexed_decode::<usize>(
                                &mut stage_payload,
                                execution,
                                span,
                            )?;
                            let body = indexed_raw(&mut stage_payload, span)?;
                            let value = indexed_raw(&mut stage_payload, span)?;
                            let op = if tag == FullStageTag::ReduceByConfigured {
                                let configuration = indexed_raw(&mut stage_payload, span)?;
                                indexed_finish(stage_payload, span)?;
                                let configuration = match self.eval_indexed_expr(
                                    execution,
                                    configuration,
                                    slots,
                                    span,
                                )? {
                                    ControlFlow::Continue(value) => value,
                                    ControlFlow::Break(value) => {
                                        return Ok(ControlFlow::Break(value));
                                    }
                                };
                                let fields = match configuration {
                                    LoweredValue::Record(fields) => fields,
                                    LoweredValue::RecordVec(fields) => Arc::new(
                                        fields
                                            .iter()
                                            .map(|(name, value)| {
                                                (
                                                    Arc::<str>::from(
                                                        name.as_str().as_str(),
                                                    ),
                                                    value.clone(),
                                                )
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
                                let mut selected = None;
                                for (name, mode) in [
                                    ("sum", ReduceByOp::Sum),
                                    ("min", ReduceByOp::Min),
                                    ("max", ReduceByOp::Max),
                                ] {
                                    match fields.get(name) {
                                        Some(LoweredValue::Bool(true)) => {
                                            if selected.replace(mode).is_some() {
                                                return Err(RuntimeError::new("stream-reduce-mode", "reduce-by requires exactly one enabled reduction mode").with_span(span));
                                            }
                                        }
                                        None | Some(LoweredValue::Bool(false)) => {}
                                        _ => {
                                            return Err(RuntimeError::new(
                                                "type-error",
                                                "reduction modes must be Bool",
                                            )
                                            .with_span(span));
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
                                    .with_span(span));
                                }
                                selected.ok_or_else(|| RuntimeError::new("stream-reduce-mode", "reduce-by requires exactly one enabled reduction mode").with_span(span))?
                            } else {
                                let op = indexed_decode::<ReduceByOp>(
                                    &mut stage_payload,
                                    execution,
                                    span,
                                )?;
                                let jobs = indexed_optional_raw(&mut stage_payload, span)?;
                                indexed_finish(stage_payload, span)?;
                                if let ControlFlow::Break(value) = self
                                    .eval_indexed_jobs_option(
                                        execution, jobs, slots, span,
                                    )?
                                {
                                    return Ok(ControlFlow::Break(value));
                                }
                                op
                            };
                            let mut items = IndexedPipelineItems::new(self, current, span)?;
                            let mut projection = Self::indexed_reduce_projection(
                                execution, item_slot, body, value, op, span,
                            )?
                            .map(LoweredProjectedReduceState::new);
                            let driven = (|| -> Result<ControlFlow<LoweredValue, BTreeMap<_, _>>, RuntimeError> {
                        let mut groups = BTreeMap::new();
                        while let Some(item) = items.next(self, span)? {
                            if let Some(projection) = projection.as_mut() {
                                self.eval_lowered_projected_reduce_by_item(
                                    projection,
                                    item,
                                    &mut groups,
                                    span,
                                )?;
                                continue;
                            }
                            slots[item_slot] = item;
                            match self.eval_indexed_statement_block(
                                execution,
                                body,
                                slots,
                                span,
                            )? {
                                StmtFlow::None => {}
                                flow => return Ok(self.preserve_lexical_expression_flow(flow)),
                            }
                            let output =
                                match self.eval_indexed_expr(execution, value, slots, span)? {
                                    ControlFlow::Continue(value) => value,
                                    ControlFlow::Break(value) => {
                                        return Ok(ControlFlow::Break(value));
                                    }
                                };
                            let (key, value) =
                                lowered_reduce_fields_owned(output, "key", "value", span)?;
                            let key = lowered_reduce_key_value_owned(key, span)?;
                            lowered_reduce_group_insert(&mut groups, key, value, op, span)?;
                        }
                        slots[item_slot] = LoweredValue::Unit;
                        Ok(ControlFlow::Continue(groups))
                    })();
                            let close = items.cancel(self, span);
                            let groups = match driven {
                                Err(error) => {
                                    let _ = close;
                                    return Err(error);
                                }
                                Ok(ControlFlow::Break(value)) => {
                                    close?;
                                    return Ok(ControlFlow::Break(value));
                                }
                                Ok(ControlFlow::Continue(groups)) => {
                                    close?;
                                    groups
                                }
                            };
                            LoweredValue::Map(Arc::new(
                                groups
                                    .into_iter()
                                    .map(|(key, value)| (MapKey::from(key), value))
                                    .collect(),
                            ))
                        }
                        FullStageTag::ParMapFlatMapReduceBy => {
                            let slot = indexed_decode::<usize>(
                                &mut stage_payload,
                                execution,
                                span,
                            )?;
                            let body = indexed_optional_raw(&mut stage_payload, span)?;
                            let jobs = indexed_optional_raw(&mut stage_payload, span)?;
                            let value = indexed_raw(&mut stage_payload, span)?;
                            let flatten = indexed_decode::<bool>(
                                &mut stage_payload,
                                execution,
                                span,
                            )?;
                            let reduce_item_slot = indexed_decode::<usize>(
                                &mut stage_payload,
                                execution,
                                span,
                            )?;
                            let reduce_body = indexed_raw(&mut stage_payload, span)?;
                            let reduce_value = indexed_raw(&mut stage_payload, span)?;
                            let op = indexed_decode::<ReduceByOp>(
                                &mut stage_payload,
                                execution,
                                span,
                            )?;
                            indexed_finish(stage_payload, span)?;
                            let jobs = match self
                                .eval_indexed_jobs_option(execution, jobs, slots, span)?
                            {
                                ControlFlow::Continue(Some(jobs)) => jobs,
                                ControlFlow::Continue(None) => {
                                    std::thread::available_parallelism()
                                        .map_or(1, |count| {
                                            count.get().min(DEFAULT_PAR_MAP_WORKERS)
                                        })
                                }
                                ControlFlow::Break(value) => {
                                    return Ok(ControlFlow::Break(value));
                                }
                            };
                            let items = self.lowered_pipeline_input_items(current, span)?;
                            if self.trace_enabled || jobs <= 1 || items.len() <= 1 {
                                let mut groups = BTreeMap::new();
                                let mut projection = Self::indexed_reduce_projection(
                                    execution,
                                    reduce_item_slot,
                                    reduce_body,
                                    reduce_value,
                                    op,
                                    span,
                                )?
                                .map(LoweredProjectedReduceState::new);
                                for (item_index, item) in items.into_iter().enumerate() {
                                    if self.trace_enabled {
                                        self.trace_lowered_parallel_job(
                                            TraceKind::ParallelJobStart,
                                            "par-map",
                                            item_index,
                                            None,
                                            span,
                                        );
                                    }
                                    let mapped = match self.eval_indexed_par_map_item(
                                        execution, body, value, slots, slot, item, span,
                                    ) {
                                        Ok(value) => value,
                                        Err(error) => {
                                            return Err(self.stream_item_runtime_error(
                                                "par-map", item_index, error,
                                            ));
                                        }
                                    };
                                    if let Some(flow) = self.pending_value_block_flow.take()
                                    {
                                        return Ok(
                                            self.preserve_lexical_expression_flow(flow)
                                        );
                                    }
                                    let rows = if flatten {
                                        self.lowered_flat_map_rows(mapped, span)?
                                    } else {
                                        vec![mapped]
                                    };
                                    self.eval_indexed_reduce_rows(
                                        execution,
                                        rows,
                                        reduce_item_slot,
                                        reduce_body,
                                        reduce_value,
                                        op,
                                        &mut projection,
                                        slots,
                                        &mut groups,
                                        span,
                                    )?;
                                    if let Some(flow) = self.pending_value_block_flow.take()
                                    {
                                        return Ok(
                                            self.preserve_lexical_expression_flow(flow)
                                        );
                                    }
                                    if self.trace_enabled {
                                        self.trace_lowered_parallel_job(
                                            TraceKind::ParallelJobEnd,
                                            "par-map",
                                            item_index,
                                            None,
                                            span,
                                        );
                                    }
                                }
                                LoweredValue::Map(Arc::new(
                                    groups
                                        .into_iter()
                                        .map(|(key, value)| (MapKey::from(key), value))
                                        .collect(),
                                ))
                            } else {
                                let output = self.eval_indexed_par_map_flat_map_reduce_by(
                                    execution,
                                    body,
                                    value,
                                    flatten,
                                    reduce_item_slot,
                                    reduce_body,
                                    reduce_value,
                                    op,
                                    slots,
                                    slot,
                                    items,
                                    jobs,
                                    span,
                                )?;
                                if let Some(flow) = self.pending_value_block_flow.take() {
                                    return Ok(self.preserve_lexical_expression_flow(flow));
                                }
                                output
                            }
                        }
                        FullStageTag::ParMap | FullStageTag::ParMapBlock => {
                            let slot = indexed_decode::<usize>(
                                &mut stage_payload,
                                execution,
                                span,
                            )?;
                            let body = if tag == FullStageTag::ParMapBlock {
                                Some(indexed_raw(&mut stage_payload, span)?)
                            } else {
                                None
                            };
                            let jobs = indexed_optional_raw(&mut stage_payload, span)?;
                            let value = indexed_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let jobs = match self
                                .eval_indexed_jobs_option(execution, jobs, slots, span)?
                            {
                                ControlFlow::Continue(Some(jobs)) => jobs,
                                ControlFlow::Continue(None) => {
                                    std::thread::available_parallelism()
                                        .map_or(1, |count| {
                                            count.get().min(DEFAULT_PAR_MAP_WORKERS)
                                        })
                                }
                                ControlFlow::Break(value) => {
                                    return Ok(ControlFlow::Break(value));
                                }
                            };
                            let items = self.lowered_pipeline_input_items(current, span)?;
                            let results = if self.trace_enabled
                                || jobs <= 1
                                || items.len() <= 1
                            {
                                let mut results = Vec::with_capacity(items.len());
                                for (item_index, item) in items.into_iter().enumerate() {
                                    if self.trace_enabled {
                                        self.trace_lowered_parallel_job(
                                            TraceKind::ParallelJobStart,
                                            "par-map",
                                            item_index,
                                            None,
                                            span,
                                        );
                                    }
                                    let result = self.eval_indexed_par_map_item(
                                        execution, body, value, slots, slot, item, span,
                                    );
                                    if self.trace_enabled {
                                        self.trace_lowered_parallel_job(
                                            TraceKind::ParallelJobEnd,
                                            "par-map",
                                            item_index,
                                            None,
                                            span,
                                        );
                                    }
                                    if let Some(flow) = self.pending_value_block_flow.take()
                                    {
                                        return Ok(
                                            self.preserve_lexical_expression_flow(flow)
                                        );
                                    }
                                    match result {
                                        Ok(value) => results.push(value),
                                        Err(error) => {
                                            return Err(self.stream_item_runtime_error(
                                                "par-map", item_index, error,
                                            ));
                                        }
                                    }
                                }
                                results
                            } else {
                                self.eval_indexed_par_map_parallel(
                                    execution, body, value, slots, slot, items, jobs, span,
                                )?
                            };
                            slots[slot] = LoweredValue::Unit;
                            if let Some(flow) = self.pending_value_block_flow.take() {
                                return Ok(self.preserve_lexical_expression_flow(flow));
                            }
                            LoweredValue::List(results)
                        }
                        FullStageTag::Tee | FullStageTag::Each => {
                            let slot = indexed_decode::<usize>(
                                &mut stage_payload,
                                execution,
                                span,
                            )?;
                            let body = indexed_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let mut items = IndexedPipelineItems::new(self, current, span)?;
                            let tee = tag == FullStageTag::Tee;
                            let driven =
                        (|| -> Result<ControlFlow<LoweredValue, Vec<LoweredValue>>, RuntimeError> {
                            let mut output = Vec::new();
                            while let Some(item) = items.next(self, span)? {
                                if tee {
                                    output.push(item.clone());
                                }
                                slots[slot] = item;
                                let flow = self.eval_indexed_statement_block(
                                    execution,
                                    body,
                                    slots,
                                    span,
                                )?;
                                match flow {
                                    StmtFlow::None => {}
                                    flow => return Ok(self.preserve_lexical_expression_flow(flow)),
                                }
                            }
                            Ok(ControlFlow::Continue(output))
                        })();
                            let close = items.cancel(self, span);
                            let output = match driven {
                                Err(error) => {
                                    let _ = close;
                                    return Err(error);
                                }
                                Ok(ControlFlow::Break(value)) => {
                                    close?;
                                    return Ok(ControlFlow::Break(value));
                                }
                                Ok(ControlFlow::Continue(output)) => {
                                    close?;
                                    output
                                }
                            };
                            slots[slot] = LoweredValue::Unit;
                            if tee {
                                // tee is a pass-through stage: it yields the
                                // items unchanged for later stages. each is a
                                // terminal stage that the checker types as Unit,
                                // so its pipeline value must be Unit rather than
                                // the drained (empty) list.
                                LoweredValue::List(output)
                            } else {
                                LoweredValue::Unit
                            }
                        }
                        FullStageTag::TablePrint | FullStageTag::TablePrintConfigured => {
                            let columns = if tag == FullStageTag::TablePrintConfigured {
                                let expression = indexed_raw(&mut stage_payload, span)?;
                                indexed_finish(stage_payload, span)?;
                                let values = match self
                                    .eval_indexed_expr(execution, expression, slots, span)?
                                {
                                    ControlFlow::Continue(value) => {
                                        self.lowered_pipeline_input_items(value, span)?
                                    }
                                    ControlFlow::Break(value) => {
                                        return Ok(ControlFlow::Break(value));
                                    }
                                };
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
                                let columns = indexed_decode::<Option<Vec<String>>>(
                                    &mut stage_payload,
                                    execution,
                                    span,
                                )?;
                                indexed_finish(stage_payload, span)?;
                                columns
                            };
                            let mut items = IndexedPipelineItems::new(self, current, span)?;
                            let collected =
                                (|| -> Result<Vec<LoweredValue>, RuntimeError> {
                                    let mut records = Vec::new();
                                    while let Some(item) = items.next(self, span)? {
                                        records.push(item);
                                    }
                                    Ok(records)
                                })();
                            let close = items.cancel(self, span);
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
                            let records = lowered_pipeline_record_list(
                                &LoweredValue::List(collected),
                                span,
                            )?;
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
                                            LoweredValue::Int(_)
                                            | LoweredValue::Float(_)
                                            | LoweredValue::Duration(_) => {
                                                crate::terminal::table::TableAlign::Right
                                            }
                                            _ => crate::terminal::table::TableAlign::Left,
                                        })
                                        .unwrap_or(
                                            crate::terminal::table::TableAlign::Left,
                                        );
                                    crate::terminal::table::TextTableColumn::new(
                                        name.clone(),
                                        0,
                                        80,
                                        align,
                                    )
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
                                            crate::terminal::table::sanitize_table_text(
                                                &lowered_table_print_value(&value),
                                            )
                                        })
                                        .collect::<Vec<_>>()
                                })
                                .collect::<Vec<_>>();
                            let mut output = String::new();
                            let width =
                                crate::terminal::table::terminal_table_width_for_stdout(
                                    20, 120,
                                );
                            crate::terminal::table::render_text_table(
                                &table_columns,
                                &rows,
                                width,
                                &mut output,
                            );
                            self.stdout.extend_from_slice(output.as_bytes());
                            LoweredValue::Unit
                        }
                        FullStageTag::Count => {
                            indexed_finish(stage_payload, span)?;
                            if let LoweredValue::Stream(mut stream) = current {
                                let mut count = stream.items.len() as i64;
                                while self.stream_next(&mut stream, span)?.is_some() {
                                    count += 1;
                                }
                                LoweredValue::Int(count)
                            } else {
                                let items =
                                    self.lowered_pipeline_input_items(current, span)?;
                                LoweredValue::Int(items.len() as i64)
                            }
                        }
                        FullStageTag::Sum => {
                            indexed_finish(stage_payload, span)?;
                            let mut items = IndexedPipelineItems::new(self, current, span)?;
                            let summed = (|| -> Result<i64, RuntimeError> {
                                let mut sum = 0i64;
                                while let Some(item) = items.next(self, span)? {
                                    let LoweredValue::Int(value) = item else {
                                        return Err(RuntimeError::new(
                                            "type-error",
                                            "sum expected Int stream",
                                        )
                                        .with_span(span));
                                    };
                                    sum += value;
                                }
                                Ok(sum)
                            })();
                            let close = items.cancel(self, span);
                            let sum = match summed {
                                Ok(sum) => {
                                    close?;
                                    sum
                                }
                                Err(error) => {
                                    let _ = close;
                                    return Err(error);
                                }
                            };
                            LoweredValue::Int(sum)
                        }
                        FullStageTag::First
                        | FullStageTag::Last
                        | FullStageTag::Min
                        | FullStageTag::Max => {
                            indexed_finish(stage_payload, span)?;
                            // A bounded terminal over a producer pulls one item
                            // and stops there: the rest of the body is never
                            // run, and its defers run once.
                            if tag == FullStageTag::First
                                && let LoweredValue::Stream(stream) = &current
                                && stream.script().is_some()
                            {
                                let LoweredValue::Stream(mut stream) = current else {
                                    unreachable!("checked above")
                                };
                                let item = self.stream_next(&mut stream, span)?;
                                self.stream_cancel(&mut stream, span)?;
                                match item {
                                    Some(value) => {
                                        match lowered_value_from_runtime_any(&value) {
                                            Some(item) => lowered_result_ok(item),
                                            None => lowered_result_err_value(
                                                RuntimeError::new(
                                                    "type-error",
                                                    format!(
                                                        "stream produced unsupported {}",
                                                        value.type_name()
                                                    ),
                                                )
                                                .with_span(span),
                                            ),
                                        }
                                    }
                                    None => lowered_result_err_value(
                                        RuntimeError::new(
                                            "empty-stream",
                                            "stream was empty",
                                        )
                                        .with_span(span),
                                    ),
                                }
                            } else if tag == FullStageTag::First {
                                let items =
                                    self.lowered_pipeline_input_items(current, span)?;
                                match items.into_iter().next() {
                                    Some(item) => lowered_result_ok(item),
                                    None => lowered_result_err_value(
                                        RuntimeError::new(
                                            "empty-stream",
                                            "stream was empty",
                                        )
                                        .with_span(span),
                                    ),
                                }
                            } else {
                                let mut items =
                                    IndexedPipelineItems::new(self, current, span)?;
                                let selected =
                                    (|| -> Result<Option<LoweredValue>, RuntimeError> {
                                        let mut selected = None;
                                        while let Some(item) = items.next(self, span)? {
                                            selected = Some(match selected {
                                                None => item,
                                                Some(previous) => match tag {
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
                                let close = items.cancel(self, span);
                                match selected {
                                    Ok(Some(item)) => {
                                        close?;
                                        lowered_result_ok(item)
                                    }
                                    Ok(None) => {
                                        close?;
                                        lowered_result_err_value(
                                            RuntimeError::new(
                                                "empty-stream",
                                                "stream was empty",
                                            )
                                            .with_span(span),
                                        )
                                    }
                                    Err(error) => {
                                        let _ = close;
                                        return Err(error);
                                    }
                                }
                            }
                        }
                        FullStageTag::Collect => {
                            indexed_finish(stage_payload, span)?;
                            if let LoweredValue::Stream(stream) = current {
                                let values = self.collect_stream_values(*stream, span)?;
                                let mut lowered = Vec::with_capacity(values.len());
                                for value in values {
                                    let Some(value) =
                                        lowered_value_from_runtime_any(&value)
                                    else {
                                        return Err(RuntimeError::new(
                                            "type-error",
                                            format!(
                                                "stream produced unsupported {}",
                                                value.type_name()
                                            ),
                                        )
                                        .with_span(span));
                                    };
                                    lowered.push(value);
                                }
                                LoweredValue::List(lowered)
                            } else if matches!(
                                current,
                                LoweredValue::List(_) | LoweredValue::SharedList(_)
                            ) {
                                current
                            } else {
                                return Err(RuntimeError::new(
                                    "type-error",
                                    "pipeline input expected List",
                                )
                                .with_span(span));
                            }
                        }
                        FullStageTag::Take | FullStageTag::Drop => {
                            let count = indexed_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let count = match self
                                .eval_indexed_expr(execution, count, slots, span)?
                            {
                                ControlFlow::Continue(value) => {
                                    lowered_nonnegative_count(value, span)?
                                }
                                ControlFlow::Break(value) => {
                                    return Ok(ControlFlow::Break(value));
                                }
                            };
                            // `take` over a producer pulls only what it keeps
                            // and then stops the producer; `drop` has to read
                            // past the dropped items, so it drains the stream.
                            if tag == FullStageTag::Take
                                && let LoweredValue::Stream(stream) = &current
                                && stream.script().is_some()
                            {
                                let LoweredValue::Stream(mut stream) = current else {
                                    unreachable!("checked above")
                                };
                                let mut kept = Vec::new();
                                while kept.len() < count {
                                    match self.stream_next(&mut stream, span)? {
                                        Some(value) => {
                                            match lowered_value_from_runtime_any(&value) {
                                                Some(item) => kept.push(item),
                                                None => {
                                                    return Err(RuntimeError::new(
                                            "type-error",
                                            format!(
                                                "stream produced unsupported {}",
                                                value.type_name()
                                            ),
                                        )
                                        .with_span(span));
                                                }
                                            }
                                        }
                                        None => break,
                                    }
                                }
                                self.stream_cancel(&mut stream, span)?;
                                LoweredValue::List(kept)
                            } else {
                                let items =
                                    self.lowered_pipeline_input_items(current, span)?;
                                if tag == FullStageTag::Take {
                                    LoweredValue::List(
                                        items.into_iter().take(count).collect(),
                                    )
                                } else {
                                    LoweredValue::List(
                                        items.into_iter().skip(count).collect(),
                                    )
                                }
                            }
                        }
                        FullStageTag::Repeat => {
                            let count = indexed_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let count = match self
                                .eval_indexed_expr(execution, count, slots, span)?
                            {
                                ControlFlow::Continue(value) => {
                                    lowered_nonnegative_count(value, span)?
                                }
                                ControlFlow::Break(value) => {
                                    return Ok(ControlFlow::Break(value));
                                }
                            };
                            if count == 0 {
                                let mut items =
                                    IndexedPipelineItems::new(self, current, span)?;
                                items.cancel(self, span)?;
                                LoweredValue::List(Vec::new())
                            } else {
                                let items =
                                    self.lowered_pipeline_input_items(current, span)?;
                                let mut repeated = Vec::with_capacity(items.len() * count);
                                for _ in 0..count {
                                    repeated.extend(items.iter().cloned());
                                }
                                LoweredValue::List(repeated)
                            }
                        }
                        FullStageTag::Range => {
                            let start = indexed_raw(&mut stage_payload, span)?;
                            let end = indexed_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let start = match self
                                .eval_indexed_expr(execution, start, slots, span)?
                            {
                                ControlFlow::Continue(LoweredValue::Int(value)) => value,
                                ControlFlow::Continue(value) => {
                                    return Err(RuntimeError::new(
                                        "type-error",
                                        format!(
                                            "range start expected Int, found {}",
                                            value.type_name()
                                        ),
                                    )
                                    .with_span(span));
                                }
                                ControlFlow::Break(value) => {
                                    return Ok(ControlFlow::Break(value));
                                }
                            };
                            let end = match self
                                .eval_indexed_expr(execution, end, slots, span)?
                            {
                                ControlFlow::Continue(LoweredValue::Int(value)) => value,
                                ControlFlow::Continue(value) => {
                                    return Err(RuntimeError::new(
                                        "type-error",
                                        format!(
                                            "range end expected Int, found {}",
                                            value.type_name()
                                        ),
                                    )
                                    .with_span(span));
                                }
                                ControlFlow::Break(value) => {
                                    return Ok(ControlFlow::Break(value));
                                }
                            };
                            LoweredValue::List(if start <= end {
                                (start..end).map(LoweredValue::Int).collect()
                            } else {
                                (end + 1..=start).rev().map(LoweredValue::Int).collect()
                            })
                        }
                    };
                    Ok(ControlFlow::Continue(value))
                })();
            let trace_error = stage_result
                .as_ref()
                .err()
                .map(TraceError::from_runtime_error);
            self.trace_exit(
                TraceKind::StreamStageExit,
                Some(span),
                Some(stage_name),
                TracePayload::StreamStage {
                    stage: stage_name.to_string(),
                    item_count: None,
                    error: trace_error,
                },
            );
            current = match stage_result? {
                ControlFlow::Continue(value) => value,
                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
            };
        }
        indexed_finish(stages, span)?;
        ControlFlow::Continue(current)
        };
        Ok(result)
    }
}
