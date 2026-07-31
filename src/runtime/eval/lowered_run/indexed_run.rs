use super::{
    Arc, AssignOp, BTreeMap, BinaryOp, Binding, CommandPlan, ControlFlow, Duration, DurationValue,
    Evaluator, FileRedirectionMode, Flow, FormatSpec, FsRootHandle, FunctionHeader, FunctionName,
    LoweredCompTarget, LoweredFunctionKey, LoweredFunctionKind, LoweredModuleExportKind,
    LoweredProjectedReduceState, LoweredReduceProjection, LoweredRetryAttemptValue,
    LoweredReturnKind, LoweredStrPredicate, LoweredTagValue, LoweredType, LoweredValue, Name,
    PathValue, ProcessEnd, ProcessInvocation, ProcessRedirection, ProcessStatus, QualifiedName,
    RecordMap, RedirectionKind, RedirectionStream, ReduceByOp, RegexValue, RunError, RunKind,
    RuntimeError, RuntimeOp, ScanCondition, Span, SpawnOptions, StmtFlow, StreamValue, TraceArg,
    TraceError, TraceKind, TracePayload, Traceback, TracebackFrame, TracebackFrameKind, Type,
    Value, api_spec, assign_lowered_bytes_view, assign_lowered_str_view, bind_lowered_comp_target,
    btree_map, bytes_contains, bytes_module, check_env_name, compare_lowered_sort_keys,
    compound_assignment_value, error_constructor, execute_run_with_policy, exit_status, fs_module,
    fs_root_record, hash_module, json_module, lowered_assign_value, lowered_binary_value,
    lowered_bool_arg_or, lowered_bool_builder_field, lowered_bytes_or_str_owned,
    lowered_bytes_parts, lowered_bytes_value, lowered_command_plan_value,
    lowered_command_redirections, lowered_contains_value, lowered_count_key, lowered_duration_arg,
    lowered_encode_json, lowered_env_record_arg, lowered_error_message,
    lowered_freeze_large_slot_list, lowered_fs_root_dir, lowered_index_value,
    lowered_inline_stats_field_value, lowered_inline_stats_to_record_vec, lowered_int_arg,
    lowered_match_no_arm, lowered_nonnegative_count, lowered_parse_command_values,
    lowered_path_arg, lowered_path_from_value, lowered_path_like_arg, lowered_path_list_arg,
    lowered_path_method_value, lowered_pipeline_input, lowered_pipeline_item_count,
    lowered_pipeline_record_list, lowered_process_run_error, lowered_record_field_value,
    lowered_record_vec_append_or_replace_unsorted, lowered_record_vec_get,
    lowered_record_vec_insert, lowered_record_vec_or_stats, lowered_reduce_fields_owned,
    lowered_reduce_group_insert, lowered_reduce_key_value_owned, lowered_result_err_value,
    lowered_result_ok, lowered_return_value, lowered_root_id, lowered_slice_value,
    lowered_splice_arg_items, lowered_stats_field_value, lowered_status_segment_record,
    lowered_stmt_flow_to_flow, lowered_str_arg, lowered_str_arg_owned, lowered_str_byte_at_value,
    lowered_str_byte_len_value, lowered_str_count_lines_value, lowered_str_key,
    lowered_str_list_arg, lowered_str_parts, lowered_str_predicate_text,
    lowered_str_predicate_value, lowered_str_value, lowered_str_view_value,
    lowered_table_print_value, lowered_tag_key, lowered_trace_error_from_value,
    lowered_trim_is_empty_value, lowered_trim_str_predicate_value, lowered_type_name,
    lowered_unit_result, lowered_value_argv_len, lowered_value_from_runtime,
    lowered_value_from_runtime_any, lowered_value_matches_static_type,
    lowered_value_satisfies_require, path_bytes, push_lowered_display, push_lowered_fmt_value,
    read_host_path_bytes, read_host_path_bytes_vec, root_path_from_dir,
    run_pipeline_inherit_with_policy, runtime_error_from_value, splice_to_argv,
    structured_error_constructor, value_matches_static_type, value_to_argv_bytes,
    with_indexed_eval_depth,
};
use crate::modules::hash::HashAlgorithm;
use crate::runtime::eval::indexed::IrVerifyError;
use crate::runtime::eval::indexed::full::{
    BLOCK_LIST, BLOCK_STATEMENTS, FullDriverTag, FullExecution, FullFunctionView, FullPatternTag,
    FullPayload, FullProgram, FullStageTag, FullTag,
};
use crate::runtime::eval::lower::{lowered_error_value_has_facet, lowered_error_variant_matches};
use crate::runtime::eval::{
    LoweredModuleExport, LoweredTopLevelSlot, LoweredTypeCheck, Propagation, ScanBytes, ScanCheck,
    process_handle,
};
use smallvec::SmallVec;

mod explicit_run;

const DEFAULT_PAR_MAP_WORKERS: usize = 6;

#[derive(Clone)]
struct RunArg {
    mode: u32,
    value: u32,
    span: Span,
}

#[derive(Clone)]
struct RunEnv {
    name: Name,
    value: RunArg,
}

#[derive(Clone)]
struct RunRedirection {
    kind: RedirectionKind,
    target: RunArg,
    span: Span,
}

struct RunSegment {
    target: RunArg,
    args: Vec<RunArg>,
    env: Vec<RunEnv>,
    redirections: Vec<RunRedirection>,
    timeout: Option<u32>,
    cpu_max: Option<u32>,
}

enum BinaryWork {
    Expr(u32),
    Apply { op: BinaryOp, span: Span },
}

enum IndexedItemPredicate<'a> {
    StringCompare {
        field: &'a str,
        op: BinaryOp,
        value: Arc<str>,
    },
    And(Box<IndexedItemPredicate<'a>>, Box<IndexedItemPredicate<'a>>),
    Or(Box<IndexedItemPredicate<'a>>, Box<IndexedItemPredicate<'a>>),
}

enum ProcessCommandEntry {
    Field {
        name: Name,
        value: u32,
        span: Span,
    },
    Run {
        target: RunArg,
        args: Vec<RunArg>,
        env: Vec<RunEnv>,
        timeout: Option<u32>,
        cpu_max: Option<u32>,
        span: Span,
    },
}

fn indexed_error(error: IrVerifyError, span: Span) -> RuntimeError {
    RuntimeError::new(
        "indexed-ir",
        format!("indexed IR verification failed: {}", error.message),
    )
    .with_span(span)
}

#[inline(always)]
fn indexed_value(
    value: Result<(FullTag, FullPayload<'_>), IrVerifyError>,
    span: Span,
) -> Result<(FullTag, FullPayload<'_>), RuntimeError> {
    value.map_err(|error| indexed_error(error, span))
}

#[inline(always)]
fn indexed_decode<'a, T: crate::runtime::eval::indexed::full::FullCodec>(
    payload: &mut FullPayload<'a>,
    execution: &FullExecution<'a>,
    span: Span,
) -> Result<T, RuntimeError> {
    payload
        .decode(execution)
        .map_err(|error| indexed_error(error, span))
}

#[inline(always)]
fn indexed_raw(payload: &mut FullPayload<'_>, span: Span) -> Result<u32, RuntimeError> {
    payload.raw().map_err(|error| indexed_error(error, span))
}

fn indexed_string<'payload, 'program>(
    payload: &mut FullPayload<'payload>,
    execution: &'program FullExecution<'program>,
    span: Span,
) -> Result<&'program str, RuntimeError> {
    execution
        .string(indexed_raw(payload, span)?)
        .map_err(|error| indexed_error(error, span))
}

#[inline(always)]
fn indexed_finish(payload: FullPayload<'_>, span: Span) -> Result<(), RuntimeError> {
    payload.finish().map_err(|error| indexed_error(error, span))
}

fn indexed_optional_raw(
    payload: &mut FullPayload<'_>,
    span: Span,
) -> Result<Option<u32>, RuntimeError> {
    match indexed_raw(payload, span)? {
        0 => Ok(None),
        1 => indexed_raw(payload, span).map(Some),
        _ => Err(RuntimeError::new("indexed-ir", "invalid optional value tag").with_span(span)),
    }
}

impl Evaluator {
    fn indexed_function_index(
        &mut self,
        program: &FullProgram,
        function: LoweredFunctionKey,
        kind: LoweredFunctionKind,
    ) -> Result<Option<usize>, IrVerifyError> {
        let cache_key = (function, kind);
        if let Some(index) = self.indexed_function_cache.get(&cache_key).copied() {
            return Ok(Some(index));
        }
        let view = program.function_view(function, kind)?;
        if let Some(view) = view {
            let index = view.index();
            self.indexed_function_cache.insert(cache_key, index);
            return Ok(Some(index));
        }
        Ok(None)
    }

    fn indexed_block_header(slot_count: usize) -> FunctionHeader {
        FunctionHeader {
            params: Default::default(),
            param_kinds: Default::default(),
            param_checks: Default::default(),
            param_rest: Default::default(),
            param_defaults: Default::default(),
            captures: Default::default(),
            return_kind: LoweredReturnKind::Plain(LoweredType::Unit),
            slot_count,
        }
    }

    fn eval_indexed_par_map_item(
        &mut self,
        execution: &FullExecution<'_>,
        body: Option<u32>,
        value: u32,
        block_header: &FunctionHeader,
        slots: &mut [LoweredValue],
        slot: usize,
        item: LoweredValue,
        span: Span,
    ) -> LoweredValue {
        slots[slot] = item;
        let item_result = if let Some(body) = body {
            match self.eval_indexed_statement_block(execution, body, block_header, slots, span) {
                Ok(StmtFlow::None) | Ok(StmtFlow::Continue) => {
                    self.eval_indexed_expr(execution, value, slots, span)
                }
                Ok(StmtFlow::Return(value)) | Ok(StmtFlow::Propagate(value)) => {
                    Ok(ControlFlow::Break(value))
                }
                Ok(StmtFlow::Break(value)) => {
                    Ok(ControlFlow::Break(value.unwrap_or(LoweredValue::Unit)))
                }
                Err(error) => Err(error),
            }
        } else {
            self.eval_indexed_expr(execution, value, slots, span)
        };
        let item_result = match item_result {
            Ok(ControlFlow::Continue(value)) => value,
            Ok(ControlFlow::Break(value)) => {
                self.stderr.extend_from_slice(
                    format!("par-map error: {}\n", lowered_error_message(&value)).as_bytes(),
                );
                LoweredValue::List(Vec::new())
            }
            Err(error) => {
                self.stderr
                    .extend_from_slice(format!("par-map error: {error:?}\n").as_bytes());
                LoweredValue::List(Vec::new())
            }
        };
        match item_result {
            LoweredValue::ResultOk(value) => *value,
            value @ LoweredValue::ResultErr(_) => {
                self.stderr.extend_from_slice(
                    format!("par-map error: {}\n", lowered_error_message(&value)).as_bytes(),
                );
                LoweredValue::List(Vec::new())
            }
            value => value,
        }
    }

    fn eval_indexed_par_map_parallel(
        &mut self,
        execution: &FullExecution<'_>,
        body: Option<u32>,
        value: u32,
        block_header: &FunctionHeader,
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
        let (chunks, stderr) = std::thread::scope(|scope| {
            let (sender, receiver) = std::sync::mpsc::sync_channel(worker_count);
            let mut workers = Vec::with_capacity(worker_count);
            for (chunk_index, chunk) in partitions.into_iter().enumerate() {
                let shared = &shared;
                let sender = sender.clone();
                let symbols = symbols.clone();
                let base_slots = base_slots.clone();
                let block_header = block_header.clone();
                let execution = execution.thread_local();
                let worker = std::thread::Builder::new()
                    .stack_size(super::super::debug_test_eval_stack_size(12 * 1024 * 1024))
                    .spawn_scoped(scope, move || {
                        let _symbols = symbols.enter();
                        let mut worker = Evaluator::new_lowered_worker(shared);
                        let mut worker_slots = base_slots;
                        let mut results = Vec::with_capacity(chunk.len());
                        for (item_index, item) in chunk {
                            results.push((
                                item_index,
                                worker.eval_indexed_par_map_item(
                                    &execution,
                                    body,
                                    value,
                                    &block_header,
                                    &mut worker_slots,
                                    slot,
                                    item,
                                    span,
                                ),
                            ));
                        }
                        sender
                            .send((chunk_index, results, worker.stderr))
                            .expect("lowered par-map receiver dropped");
                    })
                    .expect("failed to spawn lowered par-map worker");
                workers.push((chunk_index, worker));
            }
            drop(sender);
            let mut completed: Vec<Option<(Vec<(usize, LoweredValue)>, Vec<u8>)>> =
                (0..workers.len()).map(|_| None).collect();
            let mut remaining = workers.len();
            while remaining > 0 {
                match receiver.recv_timeout(std::time::Duration::from_millis(1)) {
                    Ok((chunk_index, results, worker_stderr)) => {
                        completed[chunk_index] = Some((results, worker_stderr));
                        remaining -= 1;
                        self.service_pending_signal(span)?;
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        self.service_pending_signal(span)?;
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        return Err(RuntimeError::new(
                            "par-map",
                            "worker exited without returning its results",
                        )
                        .with_span(span));
                    }
                }
            }
            for (_, worker) in workers {
                worker
                    .join()
                    .expect("lowered par-map worker thread panicked");
            }
            let mut ordered: Vec<Option<LoweredValue>> = (0..item_count).map(|_| None).collect();
            let mut stderr = Vec::new();
            for completed in completed {
                let (mut results, worker_stderr) = completed.expect("par-map worker missing");
                for (item_index, result) in results.drain(..) {
                    ordered[item_index] = Some(result);
                }
                stderr.extend(worker_stderr);
            }
            let results = ordered
                .into_iter()
                .map(|result| result.expect("par-map result missing"))
                .collect();
            Ok((results, stderr))
        })?;
        self.stderr.extend(stderr);
        Ok(chunks)
    }

    fn eval_indexed_reduce_rows(
        &mut self,
        execution: &FullExecution<'_>,
        rows: Vec<LoweredValue>,
        reduce_item_slot: usize,
        reduce_body: u32,
        reduce_value: u32,
        op: ReduceByOp,
        slots: &mut [LoweredValue],
        groups: &mut BTreeMap<String, LoweredValue>,
        span: Span,
    ) -> Result<(), RuntimeError> {
        let block_header = Self::indexed_block_header(slots.len());
        for row in rows {
            slots[reduce_item_slot] = row;
            match self.eval_indexed_statement_block(
                execution,
                reduce_body,
                &block_header,
                slots,
                span,
            )? {
                StmtFlow::None | StmtFlow::Continue => {}
                StmtFlow::Propagate(value) | StmtFlow::Return(value) => {
                    return Err(
                        RuntimeError::new("par-map-reduce", lowered_error_message(&value))
                            .with_span(span),
                    );
                }
                StmtFlow::Break(value) => {
                    return Err(RuntimeError::new(
                        "par-map-reduce",
                        lowered_error_message(&value.unwrap_or(LoweredValue::Unit)),
                    )
                    .with_span(span));
                }
            }
            let output = match self.eval_indexed_expr(execution, reduce_value, slots, span)? {
                ControlFlow::Continue(value) => value,
                ControlFlow::Break(value) => {
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

    fn lowered_flat_map_rows(
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

    fn eval_indexed_par_map_flat_map_reduce_by(
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
        let completed = std::thread::scope(|scope| {
            let mut workers = Vec::with_capacity(worker_count);
            for (chunk_index, chunk) in items.chunks(chunk_size).enumerate() {
                let chunk = chunk.to_vec();
                let shared = &shared;
                let symbols = symbols.clone();
                let base_slots = base_slots.clone();
                let map_header = Self::indexed_block_header(slots.len());
                let execution = execution.thread_local();
                let worker = std::thread::Builder::new()
                    .stack_size(super::super::debug_test_eval_stack_size(12 * 1024 * 1024))
                    .spawn_scoped(scope, move || {
                        let _symbols = symbols.enter();
                        let mut worker = Evaluator::new_lowered_worker(shared);
                        let mut worker_slots = base_slots;
                        let mut groups = BTreeMap::new();
                        let result = (|| {
                            for item in chunk {
                                let mapped = worker.eval_indexed_par_map_item(
                                    &execution,
                                    body,
                                    value,
                                    &map_header,
                                    &mut worker_slots,
                                    slot,
                                    item,
                                    span,
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
                                    &mut worker_slots,
                                    &mut groups,
                                    span,
                                )?;
                            }
                            Ok::<_, RuntimeError>(groups)
                        })();
                        (chunk_index, result, worker.stderr)
                    })
                    .expect("failed to spawn fused par-map worker");
                workers.push(worker);
            }
            let mut completed: Vec<
                Option<(
                    Result<BTreeMap<String, LoweredValue>, RuntimeError>,
                    Vec<u8>,
                )>,
            > = (0..workers.len()).map(|_| None).collect();
            while !workers.is_empty() {
                let mut index = 0;
                let mut progress = false;
                while index < workers.len() {
                    if workers[index].is_finished() {
                        let worker = workers.swap_remove(index);
                        let (chunk_index, result, worker_stderr) =
                            worker.join().expect("fused par-map worker thread panicked");
                        completed[chunk_index] = Some((result, worker_stderr));
                        progress = true;
                    } else {
                        index += 1;
                    }
                }
                self.service_pending_signal(span)?;
                if !progress {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            }
            Ok(completed)
        })?;
        self.stderr.extend(
            completed
                .iter()
                .filter_map(|entry| entry.as_ref())
                .flat_map(|(_, stderr)| stderr.iter().copied()),
        );
        let mut groups = BTreeMap::new();
        for completed in completed {
            let (result, _) = completed.expect("fused par-map worker missing");
            for (key, value) in result? {
                lowered_reduce_group_insert(&mut groups, key, value, op, span)?;
            }
        }
        Ok(LoweredValue::Map(groups))
    }

    fn decode_indexed_run_arg<'a>(
        payload: &mut FullPayload<'a>,
        execution: &FullExecution<'a>,
        span: Span,
    ) -> Result<RunArg, RuntimeError> {
        let mode = indexed_raw(payload, span)?;
        if mode > 2 {
            return Err(
                RuntimeError::new("indexed-ir", "invalid indexed run argument tag").with_span(span),
            );
        }
        Ok(RunArg {
            mode,
            value: indexed_raw(payload, span)?,
            span: indexed_decode::<Span>(payload, execution, span)?,
        })
    }

    fn decode_indexed_run_args<'a>(
        payload: &mut FullPayload<'a>,
        execution: &FullExecution<'a>,
        span: Span,
    ) -> Result<Vec<RunArg>, RuntimeError> {
        let (_, mut values) = execution
            .block(payload, BLOCK_LIST)
            .map_err(|error| indexed_error(error, span))?;
        let len = indexed_raw(&mut values, span)? as usize;
        let mut decoded = Vec::with_capacity(len);
        for _ in 0..len {
            decoded.push(Self::decode_indexed_run_arg(&mut values, execution, span)?);
        }
        indexed_finish(values, span)?;
        Ok(decoded)
    }

    fn decode_indexed_run_env<'a>(
        payload: &mut FullPayload<'a>,
        execution: &FullExecution<'a>,
        span: Span,
    ) -> Result<Vec<RunEnv>, RuntimeError> {
        let (_, mut values) = execution
            .block(payload, BLOCK_LIST)
            .map_err(|error| indexed_error(error, span))?;
        let len = indexed_raw(&mut values, span)? as usize;
        let mut decoded = Vec::with_capacity(len);
        for _ in 0..len {
            decoded.push(RunEnv {
                name: indexed_decode::<Name>(&mut values, execution, span)?,
                value: Self::decode_indexed_run_arg(&mut values, execution, span)?,
            });
        }
        indexed_finish(values, span)?;
        Ok(decoded)
    }

    fn decode_indexed_run_redirections<'a>(
        payload: &mut FullPayload<'a>,
        execution: &FullExecution<'a>,
        span: Span,
    ) -> Result<Vec<RunRedirection>, RuntimeError> {
        let (_, mut values) = execution
            .block(payload, BLOCK_LIST)
            .map_err(|error| indexed_error(error, span))?;
        let len = indexed_raw(&mut values, span)? as usize;
        let mut decoded = Vec::with_capacity(len);
        for _ in 0..len {
            decoded.push(RunRedirection {
                kind: indexed_decode::<RedirectionKind>(&mut values, execution, span)?,
                target: Self::decode_indexed_run_arg(&mut values, execution, span)?,
                span: indexed_decode::<Span>(&mut values, execution, span)?,
            });
        }
        indexed_finish(values, span)?;
        Ok(decoded)
    }

    fn decode_indexed_run_segments<'a>(
        payload: &mut FullPayload<'a>,
        execution: &FullExecution<'a>,
        span: Span,
    ) -> Result<Vec<RunSegment>, RuntimeError> {
        let (_, mut values) = execution
            .block(payload, BLOCK_LIST)
            .map_err(|error| indexed_error(error, span))?;
        let len = indexed_raw(&mut values, span)? as usize;
        let mut decoded = Vec::with_capacity(len);
        for _ in 0..len {
            let _kind = indexed_decode::<RunKind>(&mut values, execution, span)?;
            decoded.push(RunSegment {
                target: Self::decode_indexed_run_arg(&mut values, execution, span)?,
                args: Self::decode_indexed_run_args(&mut values, execution, span)?,
                env: Self::decode_indexed_run_env(&mut values, execution, span)?,
                redirections: Self::decode_indexed_run_redirections(&mut values, execution, span)?,
                timeout: indexed_optional_raw(&mut values, span)?,
                cpu_max: indexed_optional_raw(&mut values, span)?,
            });
        }
        indexed_finish(values, span)?;
        Ok(decoded)
    }

    fn decode_indexed_process_command_entries<'a>(
        payload: &mut FullPayload<'a>,
        execution: &FullExecution<'a>,
        span: Span,
    ) -> Result<Vec<ProcessCommandEntry>, RuntimeError> {
        let (_, mut values) = execution
            .block(payload, BLOCK_LIST)
            .map_err(|error| indexed_error(error, span))?;
        let len = indexed_raw(&mut values, span)? as usize;
        let mut decoded = Vec::with_capacity(len);
        for _ in 0..len {
            decoded.push(match indexed_raw(&mut values, span)? {
                0 => ProcessCommandEntry::Field {
                    name: indexed_decode::<Name>(&mut values, execution, span)?,
                    value: indexed_raw(&mut values, span)?,
                    span: indexed_decode::<Span>(&mut values, execution, span)?,
                },
                1 => ProcessCommandEntry::Run {
                    target: Self::decode_indexed_run_arg(&mut values, execution, span)?,
                    args: Self::decode_indexed_run_args(&mut values, execution, span)?,
                    env: Self::decode_indexed_run_env(&mut values, execution, span)?,
                    timeout: indexed_optional_raw(&mut values, span)?,
                    cpu_max: indexed_optional_raw(&mut values, span)?,
                    span: indexed_decode::<Span>(&mut values, execution, span)?,
                },
                _ => {
                    return Err(RuntimeError::new(
                        "indexed-ir",
                        "invalid indexed process command entry tag",
                    )
                    .with_span(span));
                }
            });
        }
        indexed_finish(values, span)?;
        Ok(decoded)
    }

    fn eval_indexed_run_arg(
        &mut self,
        execution: &FullExecution<'_>,
        arg: &RunArg,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<ControlFlow<LoweredValue, Vec<Vec<u8>>>, RuntimeError> {
        let value = match self.eval_indexed_expr(execution, arg.value, slots, call_span)? {
            ControlFlow::Continue(value) => value.into_value(),
            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
        };
        match arg.mode {
            0 => Ok(ControlFlow::Continue(vec![value_to_argv_bytes(
                value, arg.span,
            )?])),
            1 => match value {
                Value::List(_) => splice_to_argv(value, arg.span).map(ControlFlow::Continue),
                value => Ok(ControlFlow::Continue(vec![value_to_argv_bytes(
                    value, arg.span,
                )?])),
            },
            2 => splice_to_argv(value, arg.span).map(ControlFlow::Continue),
            _ => unreachable!("indexed run argument tag was checked"),
        }
    }

    fn eval_indexed_run_env(
        &mut self,
        execution: &FullExecution<'_>,
        env: &[RunEnv],
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<ControlFlow<LoweredValue, BTreeMap<Vec<u8>, Vec<u8>>>, RuntimeError> {
        let mut overlay = BTreeMap::new();
        for assignment in env {
            let items =
                match self.eval_indexed_run_arg(execution, &assignment.value, slots, call_span)? {
                    ControlFlow::Continue(items) => items,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
            let [value]: [Vec<u8>; 1] = items.try_into().map_err(|_| {
                RuntimeError::new("env-value", "environment values must be one value")
                    .with_span(assignment.value.span)
            })?;
            overlay.insert(assignment.name.as_str().as_bytes().to_vec(), value);
        }
        Ok(ControlFlow::Continue(overlay))
    }

    fn eval_indexed_run_redirections(
        &mut self,
        execution: &FullExecution<'_>,
        redirections: &[RunRedirection],
        slots: &mut [LoweredValue],
    ) -> Result<ControlFlow<LoweredValue, Vec<ProcessRedirection>>, RuntimeError> {
        let mut out = Vec::with_capacity(redirections.len());
        for redirection in redirections {
            let target = match self.eval_indexed_run_arg(
                execution,
                &redirection.target,
                slots,
                redirection.span,
            )? {
                ControlFlow::Continue(items) => items,
                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
            };
            let [target]: [Vec<u8>; 1] = target.try_into().map_err(|_| {
                RuntimeError::new(
                    "redirection-target",
                    "redirection target must produce one path",
                )
                .with_span(redirection.span)
            })?;
            if matches!(
                redirection.kind,
                RedirectionKind::StdoutDup | RedirectionKind::StdinDup
            ) {
                let text = String::from_utf8(target).map_err(|_| {
                    RuntimeError::new(
                        "redirection-target",
                        "fd redirection target must be a number",
                    )
                    .with_span(redirection.span)
                })?;
                let fd = text.trim().parse::<i32>().map_err(|_| {
                    RuntimeError::new(
                        "redirection-target",
                        "fd redirection target must be a number",
                    )
                    .with_span(redirection.span)
                })?;
                out.push(ProcessRedirection::Dup {
                    stream: if redirection.kind == RedirectionKind::StdinDup {
                        RedirectionStream::Stdin
                    } else {
                        RedirectionStream::Stdout
                    },
                    fd,
                });
                continue;
            }
            let path = PathValue::new(target).map_err(|error| error.with_span(redirection.span))?;
            out.push(ProcessRedirection::File {
                stream: match redirection.kind {
                    RedirectionKind::StdinRead => RedirectionStream::Stdin,
                    RedirectionKind::StderrWrite | RedirectionKind::StderrAppend => {
                        RedirectionStream::Stderr
                    }
                    _ => RedirectionStream::Stdout,
                },
                mode: match redirection.kind {
                    RedirectionKind::StdinRead => FileRedirectionMode::Read,
                    RedirectionKind::StdoutAppend | RedirectionKind::StderrAppend => {
                        FileRedirectionMode::Append
                    }
                    _ => FileRedirectionMode::Write,
                },
                path: self.host_path(&path),
            });
        }
        Ok(ControlFlow::Continue(out))
    }

    fn indexed_process_invocation(
        &mut self,
        execution: &FullExecution<'_>,
        target: &RunArg,
        args: &[RunArg],
        env: &[RunEnv],
        redirections: &[RunRedirection],
        timeout: Option<u32>,
        cpu_max: Option<u32>,
        slots: &mut [LoweredValue],
        span: Span,
    ) -> Result<ControlFlow<LoweredValue, ProcessInvocation>, RuntimeError> {
        let target_items = match self.eval_indexed_run_arg(execution, target, slots, span)? {
            ControlFlow::Continue(items) => items,
            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
        };
        let [target_value]: [Vec<u8>; 1] = target_items.try_into().map_err(|_| {
            RuntimeError::new("argv-conversion", "run target must produce one argv item")
                .with_span(target.span)
        })?;
        let mut argv = Vec::new();
        for arg in args {
            match self.eval_indexed_run_arg(execution, arg, slots, span)? {
                ControlFlow::Continue(items) => argv.extend(items),
                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
            }
        }
        let env_overlay = match self.eval_indexed_run_env(execution, env, slots, span)? {
            ControlFlow::Continue(value) => value,
            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
        };
        let redirections =
            match self.eval_indexed_run_redirections(execution, redirections, slots)? {
                ControlFlow::Continue(value) => value,
                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
            };
        let timeout = match self.eval_indexed_optional_expr(execution, timeout, slots, span)? {
            ControlFlow::Continue(Some(LoweredValue::Duration(duration))) => {
                Some(Duration::from_millis(duration.millis))
            }
            ControlFlow::Continue(Some(other)) => {
                return Err(RuntimeError::new(
                    "type-error",
                    format!("run timeout expected Duration, found {}", other.type_name()),
                )
                .with_span(span));
            }
            ControlFlow::Continue(None) => None,
            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
        };
        let cpu_max = match self.eval_indexed_optional_expr(execution, cpu_max, slots, span)? {
            ControlFlow::Continue(Some(LoweredValue::Int(value))) => Some(value),
            ControlFlow::Continue(Some(other)) => {
                return Err(RuntimeError::new(
                    "type-error",
                    format!("run cpumax expected Int, found {}", other.type_name()),
                )
                .with_span(span));
            }
            ControlFlow::Continue(None) => None,
            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
        };
        let mut full_env = self.env.snapshot_clone();
        full_env.extend(env_overlay.clone());
        Ok(ControlFlow::Continue(ProcessInvocation {
            target: target_value,
            argv,
            cwd: self.cwd.clone(),
            env: full_env,
            env_overlay,
            redirections,
            timeout,
            cpu_max,
        }))
    }

    fn indexed_pattern_matches(
        execution: &FullExecution<'_>,
        pattern: u32,
        value: &LoweredValue,
        slots: &mut [LoweredValue],
        span: Span,
    ) -> Result<bool, RuntimeError> {
        let (tag, mut payload) = execution
            .pattern(pattern)
            .map_err(|error| indexed_error(error, span))?;
        let matched = match tag {
            FullPatternTag::Wildcard => true,
            FullPatternTag::Bind => {
                let slot = indexed_decode::<usize>(&mut payload, execution, span)?;
                slots[slot] = value.clone();
                true
            }
            FullPatternTag::Type => {
                let ty = indexed_decode::<Type>(&mut payload, execution, span)?;
                let slot = indexed_decode::<Option<usize>>(&mut payload, execution, span)?;
                if !lowered_value_matches_static_type(value, &ty) {
                    false
                } else {
                    if let Some(slot) = slot {
                        slots[slot] = value.clone();
                    }
                    true
                }
            }
            FullPatternTag::Literal => {
                indexed_decode::<LoweredValue>(&mut payload, execution, span)? == *value
            }
            FullPatternTag::ResultOk => {
                let slot = indexed_decode::<Option<usize>>(&mut payload, execution, span)?;
                let unit_only = indexed_decode::<bool>(&mut payload, execution, span)?;
                if let LoweredValue::ResultOk(inner) = value {
                    if unit_only && !matches!(inner.as_ref(), LoweredValue::Unit) {
                        false
                    } else {
                        if let Some(slot) = slot {
                            slots[slot] = inner.as_ref().clone();
                        }
                        true
                    }
                } else {
                    false
                }
            }
            FullPatternTag::ResultErr => {
                let slot = indexed_decode::<Option<usize>>(&mut payload, execution, span)?;
                let unit_only = indexed_decode::<bool>(&mut payload, execution, span)?;
                if let LoweredValue::ResultErr(inner) = value {
                    if unit_only && !matches!(inner.as_ref(), Value::Unit) {
                        false
                    } else if let Some(slot) = slot {
                        let Some(inner) = lowered_value_from_runtime_any(inner.as_ref()) else {
                            indexed_finish(payload, span)?;
                            return Ok(false);
                        };
                        slots[slot] = inner;
                        true
                    } else {
                        true
                    }
                } else {
                    false
                }
            }
            FullPatternTag::ErrorVariant => {
                let family = indexed_decode::<Name>(&mut payload, execution, span)?;
                let variant = indexed_decode::<Name>(&mut payload, execution, span)?;
                let fields = indexed_decode::<Box<SmallVec<[(Name, Option<usize>); 4]>>>(
                    &mut payload,
                    execution,
                    span,
                )?;
                let result_wrapped = indexed_decode::<bool>(&mut payload, execution, span)?;
                let error = if result_wrapped {
                    let LoweredValue::ResultErr(error) = value else {
                        indexed_finish(payload, span)?;
                        return Ok(false);
                    };
                    error.as_ref()
                } else {
                    let LoweredValue::Error(error) = value else {
                        indexed_finish(payload, span)?;
                        return Ok(false);
                    };
                    error.as_ref()
                };
                lowered_error_variant_matches(&family, &variant, &fields, error, slots)
            }
            FullPatternTag::Facet => {
                let facet = indexed_decode::<Name>(&mut payload, execution, span)?;
                let result_wrapped = indexed_decode::<bool>(&mut payload, execution, span)?;
                let error = if result_wrapped {
                    let LoweredValue::ResultErr(error) = value else {
                        indexed_finish(payload, span)?;
                        return Ok(false);
                    };
                    error.as_ref()
                } else {
                    let LoweredValue::Error(error) = value else {
                        indexed_finish(payload, span)?;
                        return Ok(false);
                    };
                    error.as_ref()
                };
                lowered_error_value_has_facet(error, &facet.as_str())
            }
            FullPatternTag::Tag => {
                let name = indexed_decode::<Name>(&mut payload, execution, span)?;
                let field_count = indexed_raw(&mut payload, span)? as usize;
                let mut field_slots = SmallVec::<[Option<usize>; 2]>::with_capacity(field_count);
                for _ in 0..field_count {
                    field_slots.push(indexed_decode::<Option<usize>>(
                        &mut payload,
                        execution,
                        span,
                    )?);
                }
                let LoweredValue::Tag(value) = value else {
                    indexed_finish(payload, span)?;
                    return Ok(false);
                };
                if value.name.as_ref() != name.as_str() || value.fields.len() != field_slots.len() {
                    false
                } else {
                    for (slot, field) in field_slots.iter().zip(&value.fields) {
                        if let Some(slot) = slot {
                            slots[*slot] = field.clone();
                        }
                    }
                    true
                }
            }
        };
        indexed_finish(payload, span)?;
        Ok(matched)
    }

    fn eval_indexed_optional_expr(
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

    pub(in crate::runtime::eval) fn eval_indexed_driver_step(
        &mut self,
        index: usize,
        call_span: Span,
    ) -> Option<Result<Option<Flow>, RuntimeError>> {
        let program = Arc::clone(self.indexed_program.as_ref()?);
        let _symbols = program.symbol_owner().enter();
        let view = match program.driver_step_view(index) {
            Ok(view) => view,
            Err(error) => return Some(Err(indexed_error(error, call_span))),
        };
        if !matches!(
            view.tag(),
            FullDriverTag::Skip
                | FullDriverTag::Use
                | FullDriverTag::Let
                | FullDriverTag::LetRecord
                | FullDriverTag::Assign
                | FullDriverTag::Discard
                | FullDriverTag::Stmt
                | FullDriverTag::Expr
                | FullDriverTag::Defer
                | FullDriverTag::SignalHook
        ) {
            return None;
        }
        Some(self.eval_indexed_driver_step_inner(view, call_span))
    }

    fn eval_indexed_driver_step_inner(
        &mut self,
        view: crate::runtime::eval::indexed::full::FullDriverStepView<'_>,
        call_span: Span,
    ) -> Result<Option<Flow>, RuntimeError> {
        let execution = view
            .execution()
            .map_err(|error| indexed_error(error, call_span))?;
        let mut payload = view
            .payload()
            .map_err(|error| indexed_error(error, call_span))?;
        let top_level_slots = view
            .slots()
            .map_err(|error| indexed_error(error, call_span))?;
        let mut slots = vec![LoweredValue::Unit; view.slot_count()];
        for slot in &top_level_slots {
            let Some(binding) = self.lookup(slot.name) else {
                return Ok(None);
            };
            let Some(value) = lowered_value_from_runtime(&binding.value, slot.kind)
                .or_else(|| lowered_value_from_runtime_any(&binding.value))
            else {
                return Ok(None);
            };
            slots[slot.slot] = value;
        }
        let header = Self::indexed_block_header(view.slot_count());
        let flow = match view.tag() {
            FullDriverTag::Skip => {
                indexed_finish(payload, call_span)?;
                Flow::Continue(Value::Unit)
            }
            FullDriverTag::Use => {
                let key = indexed_decode::<Arc<str>>(&mut payload, &execution, call_span)?;
                let alias = indexed_decode::<Option<Name>>(&mut payload, &execution, call_span)?;
                let path = indexed_decode::<Vec<Name>>(&mut payload, &execution, call_span)?;
                let namespace = indexed_decode::<Name>(&mut payload, &execution, call_span)?;
                let exports = indexed_decode::<Vec<LoweredModuleExport>>(
                    &mut payload,
                    &execution,
                    call_span,
                )?;
                let child = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, &execution, call_span)?;
                indexed_finish(payload, call_span)?;
                if path.is_empty() {
                    return Err(
                        RuntimeError::new("unknown-module", "empty module path").with_span(span)
                    );
                }
                let import_name = alias.unwrap_or(namespace);
                let program = Arc::clone(
                    self.indexed_program
                        .as_ref()
                        .expect("indexed driver retains its program"),
                );
                let child_steps = program
                    .driver_program_step_views(child)
                    .map_err(|error| indexed_error(error, span))?;
                for child_view in child_steps {
                    if child_view.tag() == FullDriverTag::Defer {
                        continue;
                    }
                    let child_span = child_view
                        .source_span()
                        .map_err(|error| indexed_error(error, span))?;
                    let mut modules_before = Vec::new();
                    if let Some(scope) = self.scopes.last() {
                        for (&name, binding) in scope {
                            if let Value::Module(record) = &binding.value {
                                modules_before.push((name, record.clone()));
                            }
                        }
                    }
                    match self.eval_indexed_driver_step_inner(child_view, child_span)? {
                        Some(Flow::Continue(_)) | None => {}
                        Some(Flow::Propagate(propagation)) => {
                            return Err(runtime_error_from_value(propagation.error, child_span));
                        }
                        Some(_) => {
                            return Err(RuntimeError::new(
                                "module-load",
                                format!("invalid control flow while importing {key}"),
                            )
                            .with_span(child_span));
                        }
                    }
                    for (name, record) in &modules_before {
                        if let Some(binding) = self.lookup(*name)
                            && !matches!(&binding.value, Value::Module(_))
                        {
                            self.define(
                                *name,
                                Binding {
                                    value: Value::Module(record.clone()),
                                    mutable: false,
                                },
                            );
                        }
                    }
                }
                let mut modules_protected = Vec::new();
                if let Some(scope) = self.scopes.last() {
                    for (&name, binding) in scope {
                        if let Value::Module(record) = &binding.value {
                            modules_protected.push((name, record.clone()));
                        }
                    }
                }
                let mut record_fields = Vec::with_capacity(exports.len());
                for export in exports {
                    let value = match export.kind {
                        LoweredModuleExportKind::Value => self
                            .lookup(export.name)
                            .map(|binding| binding.value.clone())
                            .ok_or_else(|| {
                                RuntimeError::new(
                                    "missing-field",
                                    format!("module export `{}` was not materialized", export.name),
                                )
                                .with_span(span)
                            })?,
                        LoweredModuleExportKind::Pure => {
                            let owner = export.function_namespace.unwrap_or(namespace);
                            Value::Pure(QualifiedName::new(owner, export.name).into())
                        }
                        LoweredModuleExportKind::Proc => {
                            let owner = export.function_namespace.unwrap_or(namespace);
                            Value::Proc(QualifiedName::new(owner, export.name).into())
                        }
                    };
                    record_fields.push((export.name, value.clone()));
                    if alias.is_none() {
                        self.define(
                            export.name,
                            Binding {
                                value,
                                mutable: false,
                            },
                        );
                    }
                }
                for (name, module_record) in modules_protected {
                    if let Some(binding) = self.lookup(name)
                        && !matches!(&binding.value, Value::Module(_))
                    {
                        self.define(
                            name,
                            Binding {
                                value: Value::Module(module_record),
                                mutable: false,
                            },
                        );
                    }
                }
                self.define(
                    import_name,
                    Binding {
                        value: Value::Module(RecordMap::from_name_values(record_fields)),
                        mutable: false,
                    },
                );
                Flow::Continue(Value::Unit)
            }
            FullDriverTag::Let => {
                let target = indexed_decode::<Name>(&mut payload, &execution, call_span)?;
                let ty =
                    indexed_decode::<Option<LoweredType>>(&mut payload, &execution, call_span)?;
                let validation = indexed_decode::<Option<super::super::LoweredTypeCheck>>(
                    &mut payload,
                    &execution,
                    call_span,
                )?;
                let mutable = indexed_decode::<bool>(&mut payload, &execution, call_span)?;
                let value = indexed_raw(&mut payload, call_span)?;
                let value_span = indexed_decode::<Span>(&mut payload, &execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let mut value =
                    match self.eval_indexed_expr(&execution, value, &mut slots, call_span)? {
                        ControlFlow::Continue(value) => value.into_value(),
                        ControlFlow::Break(value) => {
                            return Ok(Some(self.question_flow(value.into_value(), call_span)));
                        }
                    };
                if let Some(check) = &validation {
                    if matches!(&check.ty, Type::Map(_))
                        && let Value::Record(record) = &value
                        && record.is_empty()
                    {
                        value = Value::Map(Default::default());
                    }
                    if !value_matches_static_type(&value, &check.ty) {
                        return Err(RuntimeError::new(
                            "type-error",
                            format!("expected {}, found {}", check.name, value.type_name()),
                        )
                        .with_span(value_span));
                    }
                } else if let Some(ty) = ty
                    && lowered_value_from_runtime(&value, ty).is_none()
                {
                    return Err(RuntimeError::new(
                        "type-error",
                        format!("expected {}", lowered_type_name(ty)),
                    )
                    .with_span(value_span));
                }
                if validation.is_none()
                    && ty == Some(LoweredType::Map)
                    && let Value::Record(record) = &value
                    && record.is_empty()
                {
                    value = Value::Map(Default::default());
                }
                self.define(target, Binding { value, mutable });
                Flow::Continue(Value::Unit)
            }
            FullDriverTag::Assign => {
                let target = indexed_decode::<Name>(&mut payload, &execution, call_span)?;
                let op = indexed_decode::<AssignOp>(&mut payload, &execution, call_span)?;
                let value = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, &execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let value =
                    match self.eval_indexed_expr(&execution, value, &mut slots, call_span)? {
                        ControlFlow::Continue(value) => value.into_value(),
                        ControlFlow::Break(value) => {
                            return Ok(Some(self.question_flow(value.into_value(), call_span)));
                        }
                    };
                let value = if op == AssignOp::Set {
                    value
                } else {
                    let current = self
                        .lookup(target)
                        .map(|binding| binding.value.clone())
                        .ok_or_else(|| {
                            RuntimeError::new("unresolved-name", target).with_span(span)
                        })?;
                    compound_assignment_value(op, current, value, span)?
                };
                self.assign(&target.as_str(), value, span)?;
                Flow::Continue(Value::Unit)
            }
            FullDriverTag::LetRecord => {
                let source = indexed_raw(&mut payload, call_span)?;
                let fields = indexed_decode::<Vec<Name>>(&mut payload, &execution, call_span)?;
                let mutable = indexed_decode::<bool>(&mut payload, &execution, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, &execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let source =
                    match self.eval_indexed_expr(&execution, source, &mut slots, call_span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => {
                            return Ok(Some(self.question_flow(value.into_value(), call_span)));
                        }
                    };
                for name in fields {
                    let Some(value) = lowered_record_field_value(&source, &name.as_str()) else {
                        return Err(RuntimeError::new(
                            "field-access",
                            format!("record has no field `{}`", name.as_str()),
                        )
                        .with_span(span));
                    };
                    self.define(
                        name,
                        Binding {
                            value: value.into_value(),
                            mutable,
                        },
                    );
                }
                Flow::Continue(Value::Unit)
            }
            FullDriverTag::Discard => {
                let value = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, &execution, call_span)?;
                indexed_finish(payload, call_span)?;
                match self.eval_indexed_expr(&execution, value, &mut slots, span)? {
                    ControlFlow::Continue(_) => Flow::Continue(Value::Unit),
                    ControlFlow::Break(value) => {
                        return Ok(Some(self.question_flow(value.into_value(), span)));
                    }
                }
            }
            FullDriverTag::Stmt => {
                let statement = indexed_raw(&mut payload, call_span)?;
                indexed_finish(payload, call_span)?;
                let flow =
                    self.eval_indexed_stmt(&execution, statement, &header, &mut slots, call_span)?;
                for slot in &top_level_slots {
                    if slot.mutable {
                        self.assign(
                            &slot.name.as_str(),
                            slots[slot.slot].clone().into_value(),
                            call_span,
                        )?;
                    }
                }
                match flow {
                    StmtFlow::Propagate(value) => self.question_flow(value.into_value(), call_span),
                    flow => lowered_stmt_flow_to_flow(flow),
                }
            }
            FullDriverTag::Expr => {
                let value = indexed_raw(&mut payload, call_span)?;
                indexed_finish(payload, call_span)?;
                let value =
                    match self.eval_indexed_expr(&execution, value, &mut slots, call_span)? {
                        ControlFlow::Continue(value) => value.into_value(),
                        ControlFlow::Break(value) => {
                            return Ok(Some(self.question_flow(value.into_value(), call_span)));
                        }
                    };
                if matches!(value, Value::Result(_)) {
                    self.question_flow(value, call_span)
                } else {
                    Flow::Continue(value)
                }
            }
            FullDriverTag::Defer => {
                let value = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, &execution, call_span)?;
                indexed_finish(payload, call_span)?;
                match self.eval_indexed_expr(&execution, value, &mut slots, span)? {
                    ControlFlow::Continue(_) => Flow::Continue(Value::Unit),
                    ControlFlow::Break(value) => {
                        return Ok(Some(self.question_flow(value.into_value(), span)));
                    }
                }
            }
            FullDriverTag::SignalHook => {
                let signal = indexed_decode::<Name>(&mut payload, &execution, call_span)?;
                let pre_cancel =
                    indexed_decode::<Option<String>>(&mut payload, &execution, call_span)?;
                let body = indexed_raw(&mut payload, call_span)?;
                let hook_slots = indexed_decode::<Vec<LoweredTopLevelSlot>>(
                    &mut payload,
                    &execution,
                    call_span,
                )?;
                let slot_count = indexed_raw(&mut payload, call_span)? as usize;
                let span = indexed_decode::<Span>(&mut payload, &execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let program = Arc::clone(
                    self.indexed_program
                        .as_ref()
                        .expect("indexed driver retains its program"),
                );
                self.register_indexed_signal_hook(
                    &signal.as_str(),
                    pre_cancel.as_deref(),
                    program,
                    view.index(),
                    body,
                    hook_slots,
                    slot_count,
                    span,
                )?;
                Flow::Continue(Value::Unit)
            }
        };
        Ok(Some(flow))
    }

    pub(in crate::runtime::eval) fn call_indexed_direct(
        &mut self,
        function: LoweredFunctionKey,
        kind: LoweredFunctionKind,
        args: &[Value],
        call_span: Span,
    ) -> Option<Result<Value, RuntimeError>> {
        let program = Arc::clone(self.indexed_program.as_ref()?);
        let _symbols = program.symbol_owner().enter();
        self.call_indexed_direct_in_program(program, function, kind, args, call_span)
    }

    fn call_indexed_direct_in_program(
        &mut self,
        program: Arc<FullProgram>,
        function: LoweredFunctionKey,
        kind: LoweredFunctionKind,
        args: &[Value],
        call_span: Span,
    ) -> Option<Result<Value, RuntimeError>> {
        let view = match program.function_view(function, kind) {
            Ok(Some(view)) => view,
            Ok(None) => {
                let LoweredFunctionKey::Qualified(qualified) = function else {
                    return None;
                };
                let dynamic = self.indexed_dynamic_functions.get(&qualified)?.clone();
                if dynamic.kind != kind {
                    return None;
                }
                let previous = self.indexed_program.replace(Arc::clone(&dynamic.program));
                let result =
                    self.call_indexed_direct(dynamic.function, dynamic.kind, args, call_span);
                self.indexed_program = previous;
                return result;
            }
            Err(error) => return Some(Err(indexed_error(error, call_span))),
        };
        let header = match view.header() {
            Ok(header) => header,
            Err(error) => return Some(Err(indexed_error(error, call_span))),
        };
        let slots = self.try_bind_lowered_runtime_args(&header, args)?;
        let frame_support = match self.indexed_frames_supported(view, call_span) {
            Ok(supported) => supported,
            Err(error) => return Some(Err(error)),
        };
        if frame_support && !super::indexed_recursive_fast_path_allowed(header.return_kind) {
            return Some(
                super::with_indexed_explicit_frames(|| {
                    self.eval_indexed_with_frame_slots(
                        program.as_ref(),
                        function,
                        kind,
                        slots,
                        call_span,
                    )
                })
                .map(LoweredValue::into_value),
            );
        }
        let mut slots = slots;
        let result = self
            .eval_indexed_call_frame(function, kind, view, &header, &mut slots, call_span)
            .and_then(|value| lowered_return_value(header.return_kind, value, call_span))
            .map(LoweredValue::into_value);
        self.recycle_lowered_slots(slots);
        Some(result)
    }

    fn eval_indexed_call_frame(
        &mut self,
        function: LoweredFunctionKey,
        kind: LoweredFunctionKind,
        view: FullFunctionView<'_>,
        header: &FunctionHeader,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        let (frame_kind, enter_kind, exit_kind) = match kind {
            LoweredFunctionKind::Pure => (
                TracebackFrameKind::Pure,
                TraceKind::PureEnter,
                TraceKind::PureExit,
            ),
            LoweredFunctionKind::Proc => (
                TracebackFrameKind::Proc,
                TraceKind::ProcEnter,
                TraceKind::ProcExit,
            ),
        };
        let name = function.display_name();
        self.trace_enter(enter_kind, Some(call_span), Some(&name), TracePayload::None);
        self.call_stack.push(TracebackFrame {
            kind: frame_kind,
            name: name.clone(),
            definition_span: None,
            call_span: Some(call_span),
        });
        let result = with_indexed_eval_depth(call_span, || {
            self.eval_indexed_function(view, header, slots, call_span)
        });
        self.call_stack.pop();
        self.trace_exit(exit_kind, Some(call_span), Some(&name), TracePayload::None);
        result
    }

    fn eval_indexed_named_call(
        &mut self,
        function: LoweredFunctionKey,
        values: &[LoweredValue],
        call_span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        let program = Arc::clone(
            self.indexed_program
                .as_ref()
                .expect("indexed caller retains its indexed program"),
        );
        let (kind, index) = if let Some(index) = self
            .indexed_function_index(&program, function, LoweredFunctionKind::Pure)
            .map_err(|error| indexed_error(error, call_span))?
        {
            (LoweredFunctionKind::Pure, index)
        } else if let Some(index) = self
            .indexed_function_index(&program, function, LoweredFunctionKind::Proc)
            .map_err(|error| indexed_error(error, call_span))?
        {
            (LoweredFunctionKind::Proc, index)
        } else {
            return Err(
                RuntimeError::new("unresolved-lowered-call", function.display_name())
                    .with_span(call_span),
            );
        };
        let view = program
            .function_view_at(index)
            .expect("cached lowered function index is valid");
        let header = view
            .header()
            .map_err(|error| indexed_error(error, call_span))?;
        if self.indexed_frames_supported(view, call_span)?
            && !super::indexed_recursive_fast_path_allowed(header.return_kind)
        {
            return super::with_indexed_explicit_frames(|| {
                self.eval_indexed_with_frames(program.as_ref(), function, kind, values, call_span)
            });
        }
        let mut next_slots = self.bind_lowered_values(&header, values, call_span)?;
        let result = self
            .eval_indexed_call_frame(function, kind, view, &header, &mut next_slots, call_span)
            .and_then(|value| lowered_return_value(header.return_kind, value, call_span));
        self.recycle_lowered_slots(next_slots);
        result
    }

    fn eval_indexed_direct_pure_call(
        &mut self,
        function: LoweredFunctionKey,
        values: &[LoweredValue],
        call_span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        let program = Arc::clone(
            self.indexed_program
                .as_ref()
                .expect("indexed caller retains its indexed program"),
        );
        let index = self
            .indexed_function_index(&program, function, LoweredFunctionKind::Pure)
            .map_err(|error| indexed_error(error, call_span))?
            .ok_or_else(|| {
                RuntimeError::new("unresolved-lowered-call", function.display_name())
                    .with_span(call_span)
            })?;
        let view = program
            .function_view_at(index)
            .expect("cached lowered function index is valid");
        let header = view
            .header()
            .map_err(|error| indexed_error(error, call_span))?;
        if self.indexed_frames_supported(view, call_span)?
            && !super::indexed_recursive_fast_path_allowed(header.return_kind)
        {
            return super::with_indexed_explicit_frames(|| {
                self.eval_indexed_with_frames(
                    program.as_ref(),
                    function,
                    LoweredFunctionKind::Pure,
                    values,
                    call_span,
                )
            });
        }
        let mut next_slots = self.bind_lowered_values(&header, values, call_span)?;
        let name = function.display_name();
        self.call_stack.push(TracebackFrame {
            kind: TracebackFrameKind::Pure,
            name,
            definition_span: None,
            call_span: Some(call_span),
        });
        let result = with_indexed_eval_depth(call_span, || {
            self.eval_indexed_function(view, &header, &mut next_slots, call_span)
        });
        self.call_stack.pop();
        let result =
            result.and_then(|value| lowered_return_value(header.return_kind, value, call_span));
        self.recycle_lowered_slots(next_slots);
        result
    }

    fn eval_indexed_self_call(
        &mut self,
        function: LoweredFunctionKey,
        values: &[LoweredValue],
        call_span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        let program = Arc::clone(
            self.indexed_program
                .as_ref()
                .expect("indexed caller retains its indexed program"),
        );
        let (kind, index) = if let Some(index) = self
            .indexed_function_index(&program, function, LoweredFunctionKind::Pure)
            .map_err(|error| indexed_error(error, call_span))?
        {
            (LoweredFunctionKind::Pure, index)
        } else if let Some(index) = self
            .indexed_function_index(&program, function, LoweredFunctionKind::Proc)
            .map_err(|error| indexed_error(error, call_span))?
        {
            (LoweredFunctionKind::Proc, index)
        } else {
            return Err(
                RuntimeError::new("unresolved-lowered-call", function.display_name())
                    .with_span(call_span),
            );
        };
        let view = program
            .function_view_at(index)
            .expect("cached lowered function index is valid");
        let header = view
            .header()
            .map_err(|error| indexed_error(error, call_span))?;
        if self.indexed_frames_supported(view, call_span)?
            && !super::indexed_recursive_fast_path_allowed(header.return_kind)
        {
            return super::with_indexed_explicit_frames(|| {
                self.eval_indexed_with_frames(program.as_ref(), function, kind, values, call_span)
            });
        }
        let mut next_slots = self.bind_lowered_values(&header, values, call_span)?;
        let result = with_indexed_eval_depth(call_span, || {
            self.eval_indexed_function(view, &header, &mut next_slots, call_span)
        })
        .and_then(|value| lowered_return_value(header.return_kind, value, call_span));
        self.recycle_lowered_slots(next_slots);
        result
    }

    fn eval_indexed_function(
        &mut self,
        view: FullFunctionView<'_>,
        header: &FunctionHeader,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        self.hydrate_lowered_captures(header, slots, call_span)?;
        let execution = view
            .execution()
            .map_err(|error| indexed_error(error, call_span))?;
        let (_, body) = view
            .body(&execution)
            .map_err(|error| indexed_error(error, call_span))?;
        if matches!(
            header.return_kind,
            LoweredReturnKind::Plain(LoweredType::Stream)
        ) {
            let previous_items = std::mem::take(&mut self.stream_items);
            let result = self.eval_indexed_stmts(&execution, body, header, slots, call_span);
            let write_back = self.write_back_lowered_captures(header, slots, call_span);
            let items = std::mem::take(&mut self.stream_items);
            self.stream_items = previous_items;
            let flow = result?;
            write_back?;
            return match flow {
                StmtFlow::None => Ok(LoweredValue::Stream(Box::new(StreamValue::from_values(
                    items,
                )))),
                StmtFlow::Return(value) if matches!(value, LoweredValue::Stream(_)) => Ok(value),
                StmtFlow::Return(LoweredValue::Unit) => Ok(LoweredValue::Stream(Box::new(
                    StreamValue::from_values(items),
                ))),
                StmtFlow::Return(value) => Err(RuntimeError::new(
                    "type-error",
                    format!("stream producer returned {}", value.type_name()),
                )
                .with_span(call_span)),
                StmtFlow::Propagate(value) => Ok(value),
                StmtFlow::Break(_) => {
                    Err(RuntimeError::new("control-flow", "break outside loop")
                        .with_span(call_span))
                }
                StmtFlow::Continue => {
                    Err(RuntimeError::new("control-flow", "continue outside loop")
                        .with_span(call_span))
                }
            };
        }
        let result = self.eval_indexed_stmts(&execution, body, header, slots, call_span);
        let write_back = self.write_back_lowered_captures(header, slots, call_span);
        let flow = result?;
        write_back?;
        match flow {
            StmtFlow::Return(value) | StmtFlow::Propagate(value) => Ok(value),
            StmtFlow::None => Err(
                RuntimeError::new("return", "lowered function did not return").with_span(call_span),
            ),
            StmtFlow::Continue => {
                Err(RuntimeError::new("control-flow", "continue outside loop").with_span(call_span))
            }
            StmtFlow::Break(_) => {
                Err(RuntimeError::new("control-flow", "break outside loop").with_span(call_span))
            }
        }
    }

    fn indexed_stage_name(tag: FullStageTag) -> &'static str {
        match tag {
            FullStageTag::TextLines => "text.lines",
            FullStageTag::JsonLines => "json.lines",
            FullStageTag::Where => "where",
            FullStageTag::Map | FullStageTag::MapBlock => "map",
            FullStageTag::FlatMap | FullStageTag::FlatMapBlock => "flat-map",
            FullStageTag::BytesChunks => "bytes.chunks",
            FullStageTag::BatchCount | FullStageTag::BatchMaxArgv | FullStageTag::BatchMaxBytes => {
                "batch"
            }
            FullStageTag::Shuffle => "shuffle",
            FullStageTag::Fold => "fold",
            FullStageTag::ReduceBy => "reduce-by",
            FullStageTag::ParMap | FullStageTag::ParMapBlock => "par-map",
            FullStageTag::ParMapFlatMapReduceBy => "par-map",
            FullStageTag::Tee => "tee",
            FullStageTag::Each => "each",
            FullStageTag::TablePrint => "table.print",
            FullStageTag::Enumerate => "enumerate",
            FullStageTag::Zip => "zip",
            FullStageTag::Sort => "sort",
            FullStageTag::SortBy => "sort-by",
            FullStageTag::GroupBy => "group-by",
            FullStageTag::CountBy | FullStageTag::Count => "count",
            FullStageTag::Any => "any",
            FullStageTag::All => "all",
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

    fn eval_indexed_pipeline_descending(
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
                format!("--desc expected Bool, found {}", value.type_name()),
            )
            .with_span(span)),
            ControlFlow::Break(value) => Err(runtime_error_from_value(value.into_value(), span)),
        }
    }

    fn indexed_field_projection<'program>(
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

    fn indexed_field_chain_ref<'slots>(
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

    fn indexed_string_literal<'program>(
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

    fn indexed_item_predicate<'program>(
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

    fn eval_indexed_item_predicate(
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
                let equal = matches!(field, LoweredValue::Str(text) if text == *value);
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

    fn indexed_record_fields(
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

    fn indexed_reduce_projection<'program>(
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

    pub(super) fn eval_indexed_expr(
        &mut self,
        execution: &FullExecution<'_>,
        instruction: u32,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let (tag, mut payload) = indexed_value(execution.instruction_id(instruction), call_span)?;
        let result = match tag {
            FullTag::ExprNull => {
                indexed_finish(payload, call_span)?;
                ControlFlow::Continue(LoweredValue::Null)
            }
            FullTag::ExprUnit => {
                indexed_finish(payload, call_span)?;
                ControlFlow::Continue(LoweredValue::Unit)
            }
            FullTag::ExprInt => {
                let value = indexed_decode::<i64>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                ControlFlow::Continue(LoweredValue::Int(value))
            }
            FullTag::ExprFloat => {
                let value = indexed_decode::<crate::runtime::value::FloatValue>(
                    &mut payload,
                    execution,
                    call_span,
                )?;
                indexed_finish(payload, call_span)?;
                ControlFlow::Continue(LoweredValue::Float(value))
            }
            FullTag::ExprDuration => {
                let value = indexed_decode::<DurationValue>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                ControlFlow::Continue(LoweredValue::Duration(value))
            }
            FullTag::ExprBool => {
                let value = indexed_decode::<bool>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                ControlFlow::Continue(LoweredValue::Bool(value))
            }
            FullTag::ExprStr => {
                let value = indexed_decode::<Arc<str>>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                ControlFlow::Continue(LoweredValue::Str(value))
            }
            FullTag::ExprBytes => {
                let value = indexed_decode::<Arc<[u8]>>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                ControlFlow::Continue(LoweredValue::Bytes(value))
            }
            FullTag::ExprPath => {
                let value = indexed_decode::<PathValue>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                ControlFlow::Continue(LoweredValue::Path(value))
            }
            FullTag::ExprFunctionRef => {
                let function = indexed_decode::<FunctionName>(&mut payload, execution, call_span)?;
                let pure = indexed_decode::<bool>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                ControlFlow::Continue(if pure {
                    LoweredValue::Pure(function)
                } else {
                    LoweredValue::Proc(function)
                })
            }
            FullTag::ExprPathFrom => {
                let value = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let value = match self.eval_indexed_expr(execution, value, slots, span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                ControlFlow::Continue(LoweredValue::Path(lowered_path_from_value(
                    value, "Path", span,
                )?))
            }
            FullTag::ExprParam => {
                let slot = indexed_decode::<usize>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                lowered_freeze_large_slot_list(&mut slots[slot]);
                ControlFlow::Continue(slots[slot].clone())
            }
            FullTag::ExprBinary => {
                let op = indexed_decode::<BinaryOp>(&mut payload, execution, call_span)?;
                let left = indexed_raw(&mut payload, call_span)?;
                let right = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                if op == BinaryOp::And {
                    let left = match self.eval_indexed_bool(execution, left, slots, span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    };
                    if !left {
                        return Ok(ControlFlow::Continue(LoweredValue::Bool(false)));
                    }
                    return self
                        .eval_indexed_bool(execution, right, slots, span)
                        .map(|flow| flow.map_continue(LoweredValue::Bool));
                }
                if op == BinaryOp::Or {
                    let left = match self.eval_indexed_bool(execution, left, slots, span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    };
                    if left {
                        return Ok(ControlFlow::Continue(LoweredValue::Bool(true)));
                    }
                    return self
                        .eval_indexed_bool(execution, right, slots, span)
                        .map(|flow| flow.map_continue(LoweredValue::Bool));
                }
                return self
                    .eval_indexed_binary_stack(execution, slots, call_span, op, left, right, span);
            }
            FullTag::ExprIf => {
                let (_, mut branches) = execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, call_span))?;
                let branch_count = indexed_raw(&mut branches, call_span)? as usize;
                let else_value = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                for _ in 0..branch_count {
                    let condition = indexed_raw(&mut branches, span)?;
                    let value = indexed_raw(&mut branches, span)?;
                    let condition =
                        match self.eval_indexed_bool(execution, condition, slots, span)? {
                            ControlFlow::Continue(value) => value,
                            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                        };
                    if condition {
                        return self.eval_indexed_expr(execution, value, slots, call_span);
                    }
                }
                indexed_finish(branches, span)?;
                return self.eval_indexed_expr(execution, else_value, slots, call_span);
            }
            FullTag::ExprMatch => {
                let value = indexed_raw(&mut payload, call_span)?;
                let (_, mut arms) = execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, call_span))?;
                let arm_count = indexed_raw(&mut arms, call_span)? as usize;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let mut decoded_arms = Vec::with_capacity(arm_count);
                for _ in 0..arm_count {
                    decoded_arms.push((
                        indexed_raw(&mut arms, span)?,
                        indexed_optional_raw(&mut arms, span)?,
                        indexed_raw(&mut arms, span)?,
                    ));
                }
                indexed_finish(arms, span)?;
                let value = match self.eval_indexed_expr(execution, value, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                for (pattern, guard, arm_value) in decoded_arms {
                    if Self::indexed_pattern_matches(execution, pattern, &value, slots, span)? {
                        if let Some(guard) = guard {
                            match self.eval_indexed_expr(execution, guard, slots, call_span)? {
                                ControlFlow::Continue(LoweredValue::Bool(true)) => {}
                                ControlFlow::Continue(_) => continue,
                                ControlFlow::Break(value) => {
                                    return Ok(ControlFlow::Break(value));
                                }
                            }
                        }
                        return self.eval_indexed_expr(execution, arm_value, slots, call_span);
                    }
                }
                return Err(lowered_match_no_arm(span));
            }
            FullTag::ExprStrMatch | FullTag::ExprTagMatch => {
                let value = indexed_raw(&mut payload, call_span)?;
                let arm_count = indexed_raw(&mut payload, call_span)? as usize;
                let mut arms = Vec::with_capacity(arm_count);
                for _ in 0..arm_count {
                    arms.push((
                        indexed_decode::<Arc<str>>(&mut payload, execution, call_span)?,
                        indexed_raw(&mut payload, call_span)?,
                    ));
                }
                let fallback = indexed_optional_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let value = match self.eval_indexed_expr(execution, value, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let key = if tag == FullTag::ExprStrMatch {
                    lowered_str_key(&value)
                } else {
                    lowered_tag_key(&value)
                };
                if let Some(key) = key
                    && let Some((_, arm)) =
                        arms.iter().find(|(candidate, _)| candidate.as_ref() == key)
                {
                    return self.eval_indexed_expr(execution, *arm, slots, call_span);
                }
                if let Some(fallback) = fallback {
                    return self.eval_indexed_expr(execution, fallback, slots, call_span);
                }
                return Err(lowered_match_no_arm(span));
            }
            FullTag::ExprResultFallback => {
                let left = indexed_raw(&mut payload, call_span)?;
                let right = indexed_raw(&mut payload, call_span)?;
                indexed_finish(payload, call_span)?;
                return match self.eval_indexed_expr(execution, left, slots, call_span)? {
                    ControlFlow::Continue(LoweredValue::ResultOk(value)) => {
                        Ok(ControlFlow::Continue(*value))
                    }
                    ControlFlow::Continue(LoweredValue::ResultErr(_) | LoweredValue::Null) => {
                        self.eval_indexed_expr(execution, right, slots, call_span)
                    }
                    ControlFlow::Continue(value) => Ok(ControlFlow::Continue(value)),
                    ControlFlow::Break(value) => Ok(ControlFlow::Break(value)),
                };
            }
            FullTag::ExprFmtString | FullTag::ExprPathFmtString => {
                let path = tag == FullTag::ExprPathFmtString;
                let (_, mut parts) = execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, call_span))?;
                let len = indexed_raw(&mut parts, call_span)? as usize;
                let path_span = if path {
                    Some(indexed_decode::<Span>(&mut payload, execution, call_span)?)
                } else {
                    None
                };
                indexed_finish(payload, call_span)?;
                let mut text = String::new();
                for _ in 0..len {
                    match indexed_raw(&mut parts, call_span)? {
                        0 => {
                            let part =
                                indexed_decode::<Arc<str>>(&mut parts, execution, call_span)?;
                            text.push_str(&part);
                        }
                        1 => {
                            let expr = indexed_raw(&mut parts, call_span)?;
                            let span = indexed_decode::<Span>(&mut parts, execution, call_span)?;
                            let spec = indexed_decode::<Option<FormatSpec>>(
                                &mut parts, execution, call_span,
                            )?;
                            let value =
                                match self.eval_indexed_expr(execution, expr, slots, call_span)? {
                                    ControlFlow::Continue(value) => value,
                                    ControlFlow::Break(value) => {
                                        return Ok(ControlFlow::Break(value));
                                    }
                                };
                            push_lowered_fmt_value(&mut text, &value, span, spec.as_ref())?;
                        }
                        _ => {
                            return Err(RuntimeError::new(
                                "indexed-ir",
                                "invalid indexed format part",
                            )
                            .with_span(call_span));
                        }
                    }
                }
                indexed_finish(parts, call_span)?;
                if let Some(span) = path_span {
                    ControlFlow::Continue(LoweredValue::Path(
                        PathValue::from_text(text).map_err(|error| error.with_span(span))?,
                    ))
                } else {
                    ControlFlow::Continue(LoweredValue::Str(text.into()))
                }
            }
            FullTag::ExprGlob => {
                let pattern = indexed_decode::<Arc<str>>(&mut payload, execution, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let matches = crate::runtime::eval::expand_glob_pattern(&self.cwd, &pattern, span)?;
                let mut values = Vec::with_capacity(matches.len());
                for bytes in matches {
                    values.push(LoweredValue::Path(
                        PathValue::new(bytes).map_err(|error| error.with_span(span))?,
                    ));
                }
                ControlFlow::Continue(LoweredValue::List(values))
            }
            FullTag::ExprLastStatus => {
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let status = self.last_status.clone().ok_or_else(|| {
                    RuntimeError::new("last-status", "`$?` is not set").with_span(span)
                })?;
                ControlFlow::Continue(LoweredValue::Status(Box::new(status)))
            }
            FullTag::ExprRecord => {
                let (_, mut entries) = execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, call_span))?;
                let len = indexed_raw(&mut entries, call_span)? as usize;
                indexed_finish(payload, call_span)?;
                let mut record = Vec::with_capacity(len);
                for _ in 0..len {
                    match indexed_raw(&mut entries, call_span)? {
                        0 => {
                            let name = indexed_decode::<Name>(&mut entries, execution, call_span)?;
                            let expr = indexed_raw(&mut entries, call_span)?;
                            let value =
                                match self.eval_indexed_expr(execution, expr, slots, call_span)? {
                                    ControlFlow::Continue(value) => value,
                                    ControlFlow::Break(value) => {
                                        return Ok(ControlFlow::Break(value));
                                    }
                                };
                            lowered_record_vec_append_or_replace_unsorted(&mut record, name, value);
                        }
                        1 => {
                            let expr = indexed_raw(&mut entries, call_span)?;
                            let value =
                                match self.eval_indexed_expr(execution, expr, slots, call_span)? {
                                    ControlFlow::Continue(value) => value,
                                    ControlFlow::Break(value) => {
                                        return Ok(ControlFlow::Break(value));
                                    }
                                };
                            match value {
                                LoweredValue::Record(fields) | LoweredValue::Module(fields) => {
                                    for (key, value) in fields {
                                        lowered_record_vec_append_or_replace_unsorted(
                                            &mut record,
                                            Name::intern(key.as_ref()),
                                            value,
                                        );
                                    }
                                }
                                LoweredValue::RecordVec(fields) => {
                                    for (key, value) in fields {
                                        lowered_record_vec_append_or_replace_unsorted(
                                            &mut record,
                                            key,
                                            value,
                                        );
                                    }
                                }
                                LoweredValue::Stats {
                                    blanks,
                                    code,
                                    comments,
                                } => {
                                    for (key, value) in
                                        lowered_inline_stats_to_record_vec(blanks, code, comments)
                                    {
                                        lowered_record_vec_append_or_replace_unsorted(
                                            &mut record,
                                            key,
                                            value,
                                        );
                                    }
                                }
                                LoweredValue::StatsBlob(stats) => {
                                    for (key, value) in stats.to_record_vec() {
                                        lowered_record_vec_append_or_replace_unsorted(
                                            &mut record,
                                            key,
                                            value,
                                        );
                                    }
                                }
                                value => {
                                    return Err(RuntimeError::new(
                                        "type-error",
                                        format!(
                                            "record spread expected Record, found {}",
                                            value.type_name()
                                        ),
                                    )
                                    .with_span(call_span));
                                }
                            }
                        }
                        _ => {
                            return Err(RuntimeError::new(
                                "indexed-ir",
                                "invalid indexed record entry",
                            )
                            .with_span(call_span));
                        }
                    }
                }
                indexed_finish(entries, call_span)?;
                record.sort_unstable_by_key(|left| left.0);
                ControlFlow::Continue(lowered_record_vec_or_stats(record))
            }
            FullTag::ExprList => {
                let (_, mut values) = execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, call_span))?;
                let len = indexed_raw(&mut values, call_span)? as usize;
                indexed_finish(payload, call_span)?;
                let mut result = Vec::with_capacity(len);
                for _ in 0..len {
                    let value = indexed_raw(&mut values, call_span)?;
                    match self.eval_indexed_expr(execution, value, slots, call_span)? {
                        ControlFlow::Continue(value) => result.push(value),
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    }
                }
                indexed_finish(values, call_span)?;
                ControlFlow::Continue(LoweredValue::List(result))
            }
            FullTag::ExprEmptyMap => {
                indexed_finish(payload, call_span)?;
                ControlFlow::Continue(LoweredValue::Map(BTreeMap::new()))
            }
            FullTag::ExprBytesConcat => {
                let arg = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let value = match self.eval_indexed_expr(execution, arg, slots, span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let items = match value {
                    LoweredValue::List(items) => items,
                    LoweredValue::SharedList(items) => items.iter().cloned().collect(),
                    _ => {
                        return Err(RuntimeError::new(
                            "type-error",
                            "bytes.concat expected List[Bytes]",
                        )
                        .with_span(span));
                    }
                };
                let len = items
                    .iter()
                    .map(|item| lowered_bytes_value(item).map_or(0, <[u8]>::len))
                    .sum();
                let mut out = Vec::with_capacity(len);
                for item in &items {
                    let Some(bytes) = lowered_bytes_value(item) else {
                        return Err(RuntimeError::new(
                            "type-error",
                            "bytes.concat expected List[Bytes]",
                        )
                        .with_span(span));
                    };
                    out.extend_from_slice(bytes);
                }
                ControlFlow::Continue(LoweredValue::Bytes(Arc::from(out)))
            }
            FullTag::ExprRange => {
                let start = indexed_raw(&mut payload, call_span)?;
                let end = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let start = match self.eval_indexed_expr(execution, start, slots, span)? {
                    ControlFlow::Continue(LoweredValue::Int(value)) => value,
                    ControlFlow::Continue(value) => {
                        return Err(RuntimeError::new(
                            "type-error",
                            format!("range start expected Int, found {}", value.type_name()),
                        )
                        .with_span(span));
                    }
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let end = match self.eval_indexed_expr(execution, end, slots, span)? {
                    ControlFlow::Continue(LoweredValue::Int(value)) => value,
                    ControlFlow::Continue(value) => {
                        return Err(RuntimeError::new(
                            "type-error",
                            format!("range end expected Int, found {}", value.type_name()),
                        )
                        .with_span(span));
                    }
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let values = if start <= end {
                    (start..end).map(LoweredValue::Int).collect()
                } else {
                    (end + 1..=start).rev().map(LoweredValue::Int).collect()
                };
                ControlFlow::Continue(LoweredValue::List(values))
            }
            FullTag::ExprTag => {
                let name = indexed_decode::<Arc<str>>(&mut payload, execution, call_span)?;
                let (_, mut fields) = execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, call_span))?;
                let len = indexed_raw(&mut fields, call_span)? as usize;
                indexed_finish(payload, call_span)?;
                let mut values = Vec::with_capacity(len);
                for _ in 0..len {
                    let field = indexed_raw(&mut fields, call_span)?;
                    match self.eval_indexed_expr(execution, field, slots, call_span)? {
                        ControlFlow::Continue(value) => values.push(value),
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    }
                }
                indexed_finish(fields, call_span)?;
                ControlFlow::Continue(LoweredValue::Tag(Box::new(LoweredTagValue {
                    name,
                    fields: values,
                })))
            }
            FullTag::ExprListComp | FullTag::ExprMapComp => {
                let map = tag == FullTag::ExprMapComp;
                let key = map
                    .then(|| indexed_raw(&mut payload, call_span))
                    .transpose()?;
                let value = indexed_raw(&mut payload, call_span)?;
                let target =
                    indexed_decode::<LoweredCompTarget>(&mut payload, execution, call_span)?;
                let iter = indexed_raw(&mut payload, call_span)?;
                let condition = indexed_optional_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let iter = match self.eval_indexed_expr(execution, iter, slots, span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let items = self.lowered_list_items(
                    iter,
                    span,
                    if map {
                        "map comprehension expected List"
                    } else {
                        "list comprehension expected List"
                    },
                )?;
                if map {
                    let mut values = BTreeMap::new();
                    for item in items {
                        bind_lowered_comp_target(&target, item, slots, span)?;
                        if let Some(condition) = condition {
                            match self.eval_indexed_bool(execution, condition, slots, span)? {
                                ControlFlow::Continue(true) => {}
                                ControlFlow::Continue(false) => continue,
                                ControlFlow::Break(value) => {
                                    return Ok(ControlFlow::Break(value));
                                }
                            }
                        }
                        let key = match self.eval_indexed_expr(
                            execution,
                            key.expect("map comprehension key"),
                            slots,
                            span,
                        )? {
                            ControlFlow::Continue(value) => value,
                            ControlFlow::Break(value) => {
                                return Ok(ControlFlow::Break(value));
                            }
                        };
                        let Some(key) = lowered_str_value(&key) else {
                            return Err(RuntimeError::new(
                                "type-error",
                                "map comprehension key expected Str",
                            )
                            .with_span(span));
                        };
                        let value = match self.eval_indexed_expr(execution, value, slots, span)? {
                            ControlFlow::Continue(value) => value,
                            ControlFlow::Break(value) => {
                                return Ok(ControlFlow::Break(value));
                            }
                        };
                        values.insert(key.to_string(), value);
                    }
                    ControlFlow::Continue(LoweredValue::Map(values))
                } else {
                    let mut values = Vec::new();
                    for item in items {
                        bind_lowered_comp_target(&target, item, slots, span)?;
                        if let Some(condition) = condition {
                            match self.eval_indexed_bool(execution, condition, slots, span)? {
                                ControlFlow::Continue(true) => {}
                                ControlFlow::Continue(false) => continue,
                                ControlFlow::Break(value) => {
                                    return Ok(ControlFlow::Break(value));
                                }
                            }
                        }
                        match self.eval_indexed_expr(execution, value, slots, span)? {
                            ControlFlow::Continue(value) => values.push(value),
                            ControlFlow::Break(value) => {
                                return Ok(ControlFlow::Break(value));
                            }
                        }
                    }
                    ControlFlow::Continue(LoweredValue::List(values))
                }
            }
            FullTag::ExprPipeline => {
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
                for _ in 0..stage_count {
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
                    current = match tag {
                        FullStageTag::TextLines => {
                            indexed_finish(stage_payload, span)?;
                            let Some((text, start, end)) = lowered_str_parts(&current) else {
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
                                let view_end = if line_end > cursor && bytes[line_end - 1] == b'\r'
                                {
                                    line_end - 1
                                } else {
                                    line_end
                                };
                                lines.push(lowered_str_view_value(text.clone(), cursor, view_end));
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
                            let values = crate::modules::json::parse_json_lines(text, span)?;
                            let mut lowered = Vec::with_capacity(values.len());
                            for value in values {
                                let Some(value) = lowered_value_from_runtime_any(&value) else {
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
                                        LoweredValue::Record(btree_map(vec![
                                            (Arc::from("index"), LoweredValue::Int(index as i64)),
                                            (Arc::from("value"), value),
                                        ]))
                                    })
                                    .collect(),
                            )
                        }
                        FullStageTag::Zip => {
                            let other = indexed_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let left = self.lowered_pipeline_input_items(current, span)?;
                            let other =
                                match self.eval_indexed_expr(execution, other, slots, span)? {
                                    ControlFlow::Continue(value) => value,
                                    ControlFlow::Break(value) => {
                                        return Ok(ControlFlow::Break(value));
                                    }
                                };
                            let right =
                                self.lowered_list_items(other, span, "zip expected List")?;
                            LoweredValue::List(
                                left.into_iter()
                                    .zip(right)
                                    .map(|(left, right)| {
                                        LoweredValue::Record(btree_map(vec![
                                            (Arc::from("left"), left),
                                            (Arc::from("right"), right),
                                        ]))
                                    })
                                    .collect(),
                            )
                        }
                        FullStageTag::Sort => {
                            let descending = indexed_optional_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let mut items = self.lowered_pipeline_input_items(current, span)?;
                            items.sort_unstable_by(compare_lowered_sort_keys);
                            if self.eval_indexed_pipeline_descending(
                                execution, descending, slots, span,
                            )? {
                                items.reverse();
                            }
                            LoweredValue::List(items)
                        }
                        FullStageTag::SortBy => {
                            let slot =
                                indexed_decode::<usize>(&mut stage_payload, execution, span)?;
                            let key = indexed_raw(&mut stage_payload, span)?;
                            let descending = indexed_optional_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let items = self.lowered_pipeline_input_items(current, span)?;
                            let projection =
                                Self::indexed_field_projection(execution, key, slot, span)?;
                            let mut keyed = Vec::with_capacity(items.len());
                            for item in items {
                                if let Some(field) = projection
                                    && let Some(key) =
                                        self.indexed_borrowed_field_value(&item, field, span)?
                                {
                                    keyed.push((key, item));
                                    continue;
                                }
                                slots[slot] = item;
                                let key =
                                    match self.eval_indexed_expr(execution, key, slots, span)? {
                                        ControlFlow::Continue(value) => value,
                                        ControlFlow::Break(value) => {
                                            return Ok(ControlFlow::Break(value));
                                        }
                                    };
                                let item = std::mem::replace(&mut slots[slot], LoweredValue::Unit);
                                keyed.push((key, item));
                            }
                            keyed.sort_unstable_by(|(left, _), (right, _)| {
                                compare_lowered_sort_keys(left, right)
                            });
                            if self.eval_indexed_pipeline_descending(
                                execution, descending, slots, span,
                            )? {
                                keyed.reverse();
                            }
                            LoweredValue::List(keyed.into_iter().map(|(_, item)| item).collect())
                        }
                        FullStageTag::GroupBy => {
                            let slot =
                                indexed_decode::<usize>(&mut stage_payload, execution, span)?;
                            let key = indexed_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let items = self.lowered_pipeline_input_items(current, span)?;
                            let projection =
                                Self::indexed_field_projection(execution, key, slot, span)?;
                            let mut groups: Vec<(LoweredValue, Vec<LoweredValue>)> = Vec::new();
                            for item in items {
                                let mut item = Some(item);
                                let key = if let Some(field) = projection
                                    && let Some(key) = self.indexed_borrowed_field_value(
                                        item.as_ref().expect("group item is present"),
                                        field,
                                        span,
                                    )? {
                                    key
                                } else {
                                    slots[slot] = item.take().expect("group item is present");
                                    match self.eval_indexed_expr(execution, key, slots, span)? {
                                        ControlFlow::Continue(value) => value,
                                        ControlFlow::Break(value) => {
                                            return Ok(ControlFlow::Break(value));
                                        }
                                    }
                                };
                                let item = item.unwrap_or_else(|| {
                                    std::mem::replace(&mut slots[slot], LoweredValue::Unit)
                                });
                                if let Some((_, group_items)) =
                                    groups.iter_mut().find(|(existing, _)| existing == &key)
                                {
                                    group_items.push(item);
                                } else {
                                    groups.push((key, vec![item]));
                                }
                            }
                            LoweredValue::List(
                                groups
                                    .into_iter()
                                    .map(|(key, items)| {
                                        LoweredValue::Record(btree_map(vec![
                                            (Arc::from("items"), LoweredValue::List(items)),
                                            (Arc::from("key"), key),
                                        ]))
                                    })
                                    .collect(),
                            )
                        }
                        FullStageTag::CountBy => {
                            let slot =
                                indexed_decode::<usize>(&mut stage_payload, execution, span)?;
                            let key = indexed_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let items = self.lowered_pipeline_input_items(current, span)?;
                            let mut counts = BTreeMap::new();
                            for item in items {
                                slots[slot] = item;
                                let key =
                                    match self.eval_indexed_expr(execution, key, slots, span)? {
                                        ControlFlow::Continue(value) => {
                                            lowered_count_key(&value, span)?
                                        }
                                        ControlFlow::Break(value) => {
                                            return Ok(ControlFlow::Break(value));
                                        }
                                    };
                                let entry = counts.entry(key).or_insert(LoweredValue::Int(0));
                                let LoweredValue::Int(count) = entry else {
                                    unreachable!("count accumulator only stores ints");
                                };
                                *count += 1;
                            }
                            LoweredValue::Map(counts)
                        }
                        FullStageTag::UniqueBy => {
                            let slot =
                                indexed_decode::<usize>(&mut stage_payload, execution, span)?;
                            let key = indexed_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let items = self.lowered_pipeline_input_items(current, span)?;
                            let mut seen = Vec::new();
                            let mut unique = Vec::with_capacity(items.len());
                            for item in items {
                                slots[slot] = item;
                                let key =
                                    match self.eval_indexed_expr(execution, key, slots, span)? {
                                        ControlFlow::Continue(value) => value,
                                        ControlFlow::Break(value) => {
                                            return Ok(ControlFlow::Break(value));
                                        }
                                    };
                                let item = std::mem::replace(&mut slots[slot], LoweredValue::Unit);
                                if !seen.iter().any(|existing| existing == &key) {
                                    seen.push(key);
                                    unique.push(item);
                                }
                            }
                            LoweredValue::List(unique)
                        }
                        FullStageTag::Where => {
                            let slot =
                                indexed_decode::<usize>(&mut stage_payload, execution, span)?;
                            let predicate = indexed_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let items = self.lowered_pipeline_input_items(current, span)?;
                            let item_predicate =
                                Self::indexed_item_predicate(execution, predicate, slot, span)?;
                            let mut filtered = Vec::new();
                            for item in items {
                                if let Some(predicate) = &item_predicate {
                                    if self.eval_indexed_item_predicate(predicate, &item, span)? {
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
                                let item = std::mem::replace(&mut slots[slot], LoweredValue::Unit);
                                if keep {
                                    filtered.push(item);
                                }
                            }
                            LoweredValue::List(filtered)
                        }
                        FullStageTag::Any | FullStageTag::All => {
                            let slot =
                                indexed_decode::<usize>(&mut stage_payload, execution, span)?;
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
                        FullStageTag::Map => {
                            let slot =
                                indexed_decode::<usize>(&mut stage_payload, execution, span)?;
                            let value = indexed_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let items = self.lowered_pipeline_input_items(current, span)?;
                            let projection =
                                Self::indexed_field_projection(execution, value, slot, span)?;
                            let mut mapped = Vec::with_capacity(items.len());
                            for (index, item) in items.into_iter().enumerate() {
                                if let Some(field) = projection
                                    && let Some(value) =
                                        self.indexed_borrowed_field_value(&item, field, span)?
                                {
                                    mapped.push(value);
                                    continue;
                                }
                                slots[slot] = item;
                                let value =
                                    match self.eval_indexed_expr(execution, value, slots, span) {
                                        Ok(ControlFlow::Continue(value)) => value,
                                        Ok(ControlFlow::Break(value)) => {
                                            return Ok(ControlFlow::Break(value));
                                        }
                                        Err(error) => {
                                            return Err(
                                                self.stream_item_runtime_error("map", index, error)
                                            );
                                        }
                                    };
                                mapped.push(value);
                            }
                            LoweredValue::List(mapped)
                        }
                        FullStageTag::MapBlock | FullStageTag::FlatMapBlock => {
                            let slot =
                                indexed_decode::<usize>(&mut stage_payload, execution, span)?;
                            let body = indexed_raw(&mut stage_payload, span)?;
                            let value = indexed_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let flat = tag == FullStageTag::FlatMapBlock;
                            let items = self.lowered_pipeline_input_items(current, span)?;
                            let mut mapped = Vec::with_capacity(items.len());
                            let block_header = Self::indexed_block_header(slots.len());
                            for item in items {
                                slots[slot] = item;
                                match self.eval_indexed_statement_block(
                                    execution,
                                    body,
                                    &block_header,
                                    slots,
                                    call_span,
                                )? {
                                    StmtFlow::None => {}
                                    StmtFlow::Return(value) | StmtFlow::Propagate(value) => {
                                        return Ok(ControlFlow::Break(value));
                                    }
                                    StmtFlow::Break(_) => {
                                        return Err(RuntimeError::new(
                                            "break-outside-loop",
                                            "break used outside loop",
                                        )
                                        .with_span(span));
                                    }
                                    StmtFlow::Continue => {
                                        return Err(RuntimeError::new(
                                            "continue-outside-loop",
                                            "continue used outside loop",
                                        )
                                        .with_span(span));
                                    }
                                }
                                let value =
                                    match self.eval_indexed_expr(execution, value, slots, span)? {
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
                            let slot =
                                indexed_decode::<usize>(&mut stage_payload, execution, span)?;
                            let value = indexed_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let items = self.lowered_pipeline_input_items(current, span)?;
                            let mut mapped = Vec::new();
                            for item in items {
                                slots[slot] = item;
                                let value =
                                    match self.eval_indexed_expr(execution, value, slots, span)? {
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
                            let size = match self.eval_indexed_expr(execution, size, slots, span)? {
                                ControlFlow::Continue(LoweredValue::Int(value)) if value > 0 => {
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
                                let Some(chunk) = lowered_value_from_runtime_any(&chunk) else {
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
                        FullStageTag::BatchCount => {
                            let count = indexed_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let count = match self
                                .eval_indexed_expr(execution, count, slots, span)?
                            {
                                ControlFlow::Continue(LoweredValue::Int(value)) if value > 0 => {
                                    value as usize
                                }
                                ControlFlow::Continue(LoweredValue::Int(_)) => {
                                    return Err(RuntimeError::new(
                                        "stream-stage-option",
                                        "--count must be positive",
                                    )
                                    .with_span(span));
                                }
                                ControlFlow::Continue(value) => {
                                    return Err(RuntimeError::new(
                                        "type-error",
                                        format!(
                                            "--count expected Int, found {}",
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
                                    batches.push(LoweredValue::List(std::mem::take(&mut batch)));
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
                                let max_argv = indexed_optional_raw(&mut stage_payload, span)?;
                                match max_argv {
                                    Some(expr) => {
                                        match self
                                            .eval_indexed_expr(execution, expr, slots, span)?
                                        {
                                            ControlFlow::Continue(value) => {
                                                lowered_nonnegative_count(value, span)?
                                            }
                                            ControlFlow::Break(value) => {
                                                return Ok(ControlFlow::Break(value));
                                            }
                                        }
                                    }
                                    None => super::super::stream::platform_arg_max()
                                        .saturating_sub(4096)
                                        .clamp(1, 128 * 1024),
                                }
                            } else {
                                let max_bytes = indexed_raw(&mut stage_payload, span)?;
                                match self.eval_indexed_expr(execution, max_bytes, slots, span)? {
                                    ControlFlow::Continue(value) => {
                                        lowered_nonnegative_count(value, span)?
                                    }
                                    ControlFlow::Break(value) => {
                                        return Ok(ControlFlow::Break(value));
                                    }
                                }
                            };
                            indexed_finish(stage_payload, span)?;
                            let items = self.lowered_pipeline_input_items(current, span)?;
                            let mut batches = Vec::new();
                            let mut batch = Vec::new();
                            let mut batch_len = 0usize;
                            for item in items {
                                let item_len = lowered_value_argv_len(&item);
                                if tag == FullStageTag::BatchMaxBytes && item_len > limit {
                                    return Err(RuntimeError::new(
                                        "argv-limit",
                                        "batch item exceeds byte budget",
                                    )
                                    .with_span(span));
                                }
                                let separator = usize::from(
                                    tag == FullStageTag::BatchMaxArgv && !batch.is_empty(),
                                );
                                if !batch.is_empty() && batch_len + separator + item_len > limit {
                                    batches.push(LoweredValue::List(std::mem::take(&mut batch)));
                                    batch_len = 0;
                                }
                                let separator = usize::from(
                                    tag == FullStageTag::BatchMaxArgv && !batch.is_empty(),
                                );
                                batch_len += separator + item_len;
                                batch.push(item);
                            }
                            if !batch.is_empty() {
                                batches.push(LoweredValue::List(batch));
                            }
                            LoweredValue::List(batches)
                        }
                        FullStageTag::Shuffle => {
                            let seed = indexed_optional_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let seed = match seed {
                                Some(seed) => {
                                    match self.eval_indexed_expr(execution, seed, slots, span)? {
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
                            let mut items = self.lowered_pipeline_input_items(current, span)?;
                            let mut state =
                                seed ^ (items.len() as u64).wrapping_mul(0x9e3779b97f4a7c15);
                            for index in (1..items.len()).rev() {
                                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                                let swap = (state as usize) % (index + 1);
                                items.swap(index, swap);
                            }
                            LoweredValue::List(items)
                        }
                        FullStageTag::Fold => {
                            let acc_slot =
                                indexed_decode::<usize>(&mut stage_payload, execution, span)?;
                            let item_slot =
                                indexed_decode::<usize>(&mut stage_payload, execution, span)?;
                            let initial = indexed_raw(&mut stage_payload, span)?;
                            let body = indexed_raw(&mut stage_payload, span)?;
                            let value = indexed_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let mut acc =
                                match self.eval_indexed_expr(execution, initial, slots, span)? {
                                    ControlFlow::Continue(value) => value,
                                    ControlFlow::Break(value) => {
                                        return Ok(ControlFlow::Break(value));
                                    }
                                };
                            let items = self.lowered_pipeline_input_items(current, span)?;
                            let block_header = Self::indexed_block_header(slots.len());
                            for item in items {
                                slots[acc_slot] = acc;
                                slots[item_slot] = item;
                                match self.eval_indexed_statement_block(
                                    execution,
                                    body,
                                    &block_header,
                                    slots,
                                    span,
                                )? {
                                    StmtFlow::None | StmtFlow::Continue => {}
                                    StmtFlow::Propagate(value) | StmtFlow::Return(value) => {
                                        return Ok(ControlFlow::Break(value));
                                    }
                                    StmtFlow::Break(value) => {
                                        return Ok(ControlFlow::Break(
                                            value.unwrap_or(LoweredValue::Unit),
                                        ));
                                    }
                                }
                                acc = match self.eval_indexed_expr(execution, value, slots, span)? {
                                    ControlFlow::Continue(value) => value,
                                    ControlFlow::Break(value) => {
                                        return Ok(ControlFlow::Break(value));
                                    }
                                };
                            }
                            slots[acc_slot] = LoweredValue::Unit;
                            slots[item_slot] = LoweredValue::Unit;
                            acc
                        }
                        FullStageTag::ReduceBy => {
                            let item_slot =
                                indexed_decode::<usize>(&mut stage_payload, execution, span)?;
                            let body = indexed_raw(&mut stage_payload, span)?;
                            let value = indexed_raw(&mut stage_payload, span)?;
                            let op =
                                indexed_decode::<ReduceByOp>(&mut stage_payload, execution, span)?;
                            indexed_finish(stage_payload, span)?;
                            let items = self.lowered_pipeline_input_items(current, span)?;
                            let block_header = Self::indexed_block_header(slots.len());
                            let mut projection = Self::indexed_reduce_projection(
                                execution, item_slot, body, value, op, span,
                            )?
                            .map(LoweredProjectedReduceState::new);
                            let mut groups = BTreeMap::new();
                            for item in items {
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
                                    &block_header,
                                    slots,
                                    span,
                                )? {
                                    StmtFlow::None | StmtFlow::Continue => {}
                                    StmtFlow::Propagate(value) | StmtFlow::Return(value) => {
                                        return Ok(ControlFlow::Break(value));
                                    }
                                    StmtFlow::Break(value) => {
                                        return Ok(ControlFlow::Break(
                                            value.unwrap_or(LoweredValue::Unit),
                                        ));
                                    }
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
                            LoweredValue::Map(groups.into_iter().collect())
                        }
                        FullStageTag::ParMapFlatMapReduceBy => {
                            let slot =
                                indexed_decode::<usize>(&mut stage_payload, execution, span)?;
                            let body = indexed_optional_raw(&mut stage_payload, span)?;
                            let jobs = indexed_optional_raw(&mut stage_payload, span)?;
                            let value = indexed_raw(&mut stage_payload, span)?;
                            let flatten =
                                indexed_decode::<bool>(&mut stage_payload, execution, span)?;
                            let reduce_item_slot =
                                indexed_decode::<usize>(&mut stage_payload, execution, span)?;
                            let reduce_body = indexed_raw(&mut stage_payload, span)?;
                            let reduce_value = indexed_raw(&mut stage_payload, span)?;
                            let op =
                                indexed_decode::<ReduceByOp>(&mut stage_payload, execution, span)?;
                            indexed_finish(stage_payload, span)?;
                            let jobs = match jobs {
                                Some(jobs) => {
                                    match self.eval_indexed_expr(execution, jobs, slots, span)? {
                                        ControlFlow::Continue(value) => {
                                            lowered_nonnegative_count(value, span)?.max(1)
                                        }
                                        ControlFlow::Break(value) => {
                                            return Ok(ControlFlow::Break(value));
                                        }
                                    }
                                }
                                None => std::thread::available_parallelism()
                                    .map_or(1, |count| count.get().min(DEFAULT_PAR_MAP_WORKERS)),
                            };
                            let items = self.lowered_pipeline_input_items(current, span)?;
                            if self.trace_enabled || jobs <= 1 || items.len() <= 1 {
                                let map_header = Self::indexed_block_header(slots.len());
                                let mut groups = BTreeMap::new();
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
                                    let mapped = self.eval_indexed_par_map_item(
                                        execution,
                                        body,
                                        value,
                                        &map_header,
                                        slots,
                                        slot,
                                        item,
                                        span,
                                    );
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
                                        slots,
                                        &mut groups,
                                        span,
                                    )?;
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
                                LoweredValue::Map(groups)
                            } else {
                                self.eval_indexed_par_map_flat_map_reduce_by(
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
                                )?
                            }
                        }
                        FullStageTag::ParMap | FullStageTag::ParMapBlock => {
                            let slot =
                                indexed_decode::<usize>(&mut stage_payload, execution, span)?;
                            let body = if tag == FullStageTag::ParMapBlock {
                                Some(indexed_raw(&mut stage_payload, span)?)
                            } else {
                                None
                            };
                            let jobs = indexed_optional_raw(&mut stage_payload, span)?;
                            let value = indexed_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let jobs = match jobs {
                                Some(jobs) => {
                                    match self.eval_indexed_expr(execution, jobs, slots, span)? {
                                        ControlFlow::Continue(value) => {
                                            lowered_nonnegative_count(value, span)?.max(1)
                                        }
                                        ControlFlow::Break(value) => {
                                            return Ok(ControlFlow::Break(value));
                                        }
                                    }
                                }
                                None => std::thread::available_parallelism()
                                    .map_or(1, |count| count.get().min(DEFAULT_PAR_MAP_WORKERS)),
                            };
                            let items = self.lowered_pipeline_input_items(current, span)?;
                            let block_header = Self::indexed_block_header(slots.len());
                            let results = if self.trace_enabled || jobs <= 1 || items.len() <= 1 {
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
                                        execution,
                                        body,
                                        value,
                                        &block_header,
                                        slots,
                                        slot,
                                        item,
                                        span,
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
                                    results.push(result);
                                }
                                results
                            } else {
                                self.eval_indexed_par_map_parallel(
                                    execution,
                                    body,
                                    value,
                                    &block_header,
                                    slots,
                                    slot,
                                    items,
                                    jobs,
                                    span,
                                )?
                            };
                            slots[slot] = LoweredValue::Unit;
                            LoweredValue::List(results)
                        }
                        FullStageTag::Tee | FullStageTag::Each => {
                            let slot =
                                indexed_decode::<usize>(&mut stage_payload, execution, span)?;
                            let body = indexed_raw(&mut stage_payload, span)?;
                            let parallel = if tag == FullStageTag::Each {
                                indexed_decode::<bool>(&mut stage_payload, execution, span)?
                            } else {
                                false
                            };
                            indexed_finish(stage_payload, span)?;
                            let items = self.lowered_pipeline_input_items(current, span)?;
                            let tee = tag == FullStageTag::Tee;
                            let output = if tee { items.clone() } else { Vec::new() };
                            let block_header = Self::indexed_block_header(slots.len());
                            for (item_index, item) in items.into_iter().enumerate() {
                                if parallel {
                                    self.trace_lowered_parallel_job(
                                        TraceKind::ParallelJobStart,
                                        "each",
                                        item_index,
                                        None,
                                        span,
                                    );
                                }
                                slots[slot] = item;
                                let flow = match self.eval_indexed_statement_block(
                                    execution,
                                    body,
                                    &block_header,
                                    slots,
                                    span,
                                ) {
                                    Ok(flow) => flow,
                                    Err(error) => {
                                        if parallel {
                                            let trace_error =
                                                TraceError::new(&error.kind, &error.message);
                                            self.trace_lowered_parallel_job(
                                                TraceKind::ParallelCancel,
                                                "each",
                                                item_index,
                                                Some(trace_error.clone()),
                                                span,
                                            );
                                            self.trace_lowered_parallel_job(
                                                TraceKind::ParallelJobEnd,
                                                "each",
                                                item_index,
                                                Some(trace_error),
                                                span,
                                            );
                                        }
                                        return Err(error);
                                    }
                                };
                                match flow {
                                    StmtFlow::None | StmtFlow::Continue => {}
                                    StmtFlow::Return(value) | StmtFlow::Propagate(value) => {
                                        if parallel {
                                            let runtime_value = value.clone().into_value();
                                            let trace_error =
                                                lowered_trace_error_from_value(&runtime_value);
                                            self.trace_lowered_parallel_job(
                                                TraceKind::ParallelCancel,
                                                "each",
                                                item_index,
                                                Some(trace_error.clone()),
                                                span,
                                            );
                                            self.trace_lowered_parallel_job(
                                                TraceKind::ParallelJobEnd,
                                                "each",
                                                item_index,
                                                Some(trace_error),
                                                span,
                                            );
                                        }
                                        return Ok(ControlFlow::Break(value));
                                    }
                                    StmtFlow::Break(value) => {
                                        return Ok(ControlFlow::Break(
                                            value.unwrap_or(LoweredValue::Unit),
                                        ));
                                    }
                                }
                                if parallel {
                                    self.trace_lowered_parallel_job(
                                        TraceKind::ParallelJobEnd,
                                        "each",
                                        item_index,
                                        None,
                                        span,
                                    );
                                }
                            }
                            slots[slot] = LoweredValue::Unit;
                            LoweredValue::List(output)
                        }
                        FullStageTag::TablePrint => {
                            let columns = indexed_decode::<Option<Vec<String>>>(
                                &mut stage_payload,
                                execution,
                                span,
                            )?;
                            indexed_finish(stage_payload, span)?;
                            let records = lowered_pipeline_record_list(&current, span)?;
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
                                        .unwrap_or(crate::terminal::table::TableAlign::Left);
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
                                crate::terminal::table::terminal_table_width_for_stdout(20, 120);
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
                            if let LoweredValue::Stream(stream) = current {
                                let mut count = stream.items.len() as i64;
                                while stream.next_live(span)?.is_some() {
                                    count += 1;
                                }
                                LoweredValue::Int(count)
                            } else {
                                let items = self.lowered_pipeline_input_items(current, span)?;
                                LoweredValue::Int(items.len() as i64)
                            }
                        }
                        FullStageTag::Sum => {
                            indexed_finish(stage_payload, span)?;
                            let items = self.lowered_pipeline_input_items(current, span)?;
                            let mut sum = 0i64;
                            for item in items {
                                let LoweredValue::Int(value) = item else {
                                    return Err(RuntimeError::new(
                                        "type-error",
                                        "sum expected Int stream",
                                    )
                                    .with_span(span));
                                };
                                sum += value;
                            }
                            LoweredValue::Int(sum)
                        }
                        FullStageTag::First
                        | FullStageTag::Last
                        | FullStageTag::Min
                        | FullStageTag::Max => {
                            indexed_finish(stage_payload, span)?;
                            let items = self.lowered_pipeline_input_items(current, span)?;
                            let item = match tag {
                                FullStageTag::First => items.into_iter().next(),
                                FullStageTag::Last => items.into_iter().last(),
                                FullStageTag::Min => {
                                    items.into_iter().min_by(compare_lowered_sort_keys)
                                }
                                FullStageTag::Max => {
                                    items.into_iter().max_by(compare_lowered_sort_keys)
                                }
                                _ => unreachable!(),
                            };
                            match item {
                                Some(item) => lowered_result_ok(item),
                                None => lowered_result_err_value(
                                    RuntimeError::new("empty-stream", "stream was empty")
                                        .with_span(span),
                                ),
                            }
                        }
                        FullStageTag::Collect => {
                            indexed_finish(stage_payload, span)?;
                            if let LoweredValue::Stream(stream) = current {
                                let values = self.collect_stream_values(*stream, span)?;
                                let mut lowered = Vec::with_capacity(values.len());
                                for value in values {
                                    let Some(value) = lowered_value_from_runtime_any(&value) else {
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
                            let count =
                                match self.eval_indexed_expr(execution, count, slots, span)? {
                                    ControlFlow::Continue(value) => {
                                        lowered_nonnegative_count(value, span)?
                                    }
                                    ControlFlow::Break(value) => {
                                        return Ok(ControlFlow::Break(value));
                                    }
                                };
                            let items = self.lowered_pipeline_input_items(current, span)?;
                            if tag == FullStageTag::Take {
                                LoweredValue::List(items.into_iter().take(count).collect())
                            } else {
                                LoweredValue::List(items.into_iter().skip(count).collect())
                            }
                        }
                        FullStageTag::Repeat => {
                            let count = indexed_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let count =
                                match self.eval_indexed_expr(execution, count, slots, span)? {
                                    ControlFlow::Continue(value) => {
                                        lowered_nonnegative_count(value, span)?
                                    }
                                    ControlFlow::Break(value) => {
                                        return Ok(ControlFlow::Break(value));
                                    }
                                };
                            let items = self.lowered_pipeline_input_items(current, span)?;
                            let mut repeated = Vec::with_capacity(items.len() * count);
                            for _ in 0..count {
                                repeated.extend(items.iter().cloned());
                            }
                            LoweredValue::List(repeated)
                        }
                        FullStageTag::Range => {
                            let start = indexed_raw(&mut stage_payload, span)?;
                            let end = indexed_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let start =
                                match self.eval_indexed_expr(execution, start, slots, span)? {
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
                            let end = match self.eval_indexed_expr(execution, end, slots, span)? {
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
                    self.trace_exit(
                        TraceKind::StreamStageExit,
                        Some(span),
                        Some(stage_name),
                        TracePayload::StreamStage {
                            stage: stage_name.to_string(),
                            item_count: None,
                            error: None,
                        },
                    );
                }
                indexed_finish(stages, span)?;
                ControlFlow::Continue(current)
            }
            FullTag::ExprField => {
                let base = indexed_raw(&mut payload, call_span)?;
                let name = indexed_string(&mut payload, execution, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                if let Some(base) = Self::indexed_field_chain_ref(execution, base, slots, span)?
                    && let Some(value) = lowered_record_field_value(base, name)
                {
                    return Ok(ControlFlow::Continue(value));
                }
                let base = match self.eval_indexed_expr(execution, base, slots, span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                ControlFlow::Continue(self.indexed_field_value(base, name, span)?)
            }
            FullTag::ExprIndex => {
                let base = indexed_raw(&mut payload, call_span)?;
                let index = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let base = match self.eval_indexed_expr(execution, base, slots, span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let index = match self.eval_indexed_expr(execution, index, slots, span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                ControlFlow::Continue(lowered_index_value(base, index, span)?)
            }
            FullTag::ExprSlice => {
                let base = indexed_raw(&mut payload, call_span)?;
                let start = indexed_optional_raw(&mut payload, call_span)?;
                let end = indexed_optional_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let base = match self.eval_indexed_expr(execution, base, slots, span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let start = match start {
                    Some(value) => match self.eval_indexed_expr(execution, value, slots, span)? {
                        ControlFlow::Continue(value) => Some(value),
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    },
                    None => None,
                };
                let end = match end {
                    Some(value) => match self.eval_indexed_expr(execution, value, slots, span)? {
                        ControlFlow::Continue(value) => Some(value),
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    },
                    None => None,
                };
                ControlFlow::Continue(lowered_slice_value(base, start, end, span)?)
            }
            FullTag::ExprMethod => {
                let receiver = indexed_raw(&mut payload, call_span)?;
                let name = indexed_string(&mut payload, execution, call_span)?;
                let (_, mut args) = execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, call_span))?;
                let len = indexed_raw(&mut args, call_span)? as usize;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let receiver = match self.eval_indexed_expr(execution, receiver, slots, span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let mut values = Vec::with_capacity(len);
                for _ in 0..len {
                    let arg = indexed_raw(&mut args, span)?;
                    match self.eval_indexed_expr(execution, arg, slots, span)? {
                        ControlFlow::Continue(value) => values.push(value),
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    }
                }
                indexed_finish(args, span)?;
                if !self.trace_enabled {
                    return self.eval_lowered_method_dispatch(receiver, name, values, &span);
                }
                let trace_name = format!("{}.{}", receiver.type_name(), name);
                self.trace_enter(
                    TraceKind::MethodCall,
                    Some(span),
                    Some(&trace_name),
                    TracePayload::None,
                );
                let result = self.eval_lowered_method_dispatch(receiver, name, values, &span);
                self.trace_exit(
                    TraceKind::MethodResult,
                    Some(span),
                    Some(&trace_name),
                    TracePayload::None,
                );
                return result;
            }
            FullTag::ExprStrByteLen => {
                let receiver = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let receiver = match self.eval_indexed_expr(execution, receiver, slots, span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                ControlFlow::Continue(LoweredValue::Int(lowered_str_byte_len_value(
                    &receiver, span,
                )?))
            }
            FullTag::ExprStrByteAt => {
                let receiver = indexed_raw(&mut payload, call_span)?;
                let index = indexed_raw(&mut payload, call_span)?;
                let default = indexed_optional_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let receiver = match self.eval_indexed_expr(execution, receiver, slots, span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let index = match self.eval_indexed_expr(execution, index, slots, span)? {
                    ControlFlow::Continue(LoweredValue::Int(value)) => value,
                    ControlFlow::Continue(_) => {
                        return Err(
                            RuntimeError::new("type-error", "byte_at expected Int").with_span(span)
                        );
                    }
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let default = match default {
                    Some(value) => match self.eval_indexed_expr(execution, value, slots, span)? {
                        ControlFlow::Continue(LoweredValue::Int(value)) => value,
                        ControlFlow::Continue(_) => {
                            return Err(RuntimeError::new(
                                "type-error",
                                "byte_at default expected Int",
                            )
                            .with_span(span));
                        }
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    },
                    None => -1,
                };
                ControlFlow::Continue(LoweredValue::Int(lowered_str_byte_at_value(
                    &receiver, index, default, span,
                )?))
            }
            FullTag::ExprStrPredicate => {
                let receiver = indexed_raw(&mut payload, call_span)?;
                let predicate =
                    indexed_decode::<LoweredStrPredicate>(&mut payload, execution, call_span)?;
                let needle = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let receiver = match self.eval_indexed_expr(execution, receiver, slots, span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let needle = match self.eval_indexed_expr(execution, needle, slots, span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                ControlFlow::Continue(LoweredValue::Bool(lowered_str_predicate_value(
                    &receiver, predicate, &needle, span,
                )?))
            }
            FullTag::ExprContains => {
                let receiver = indexed_raw(&mut payload, call_span)?;
                let needle = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let receiver = match self.eval_indexed_expr(execution, receiver, slots, span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let needle = match self.eval_indexed_expr(execution, needle, slots, span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                ControlFlow::Continue(LoweredValue::Bool(lowered_contains_value(
                    &receiver, &needle, span,
                )?))
            }
            FullTag::ExprRegexCompile => {
                let pattern = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let pattern = match self.eval_indexed_expr(execution, pattern, slots, span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let Some(pattern) = lowered_str_value(&pattern) else {
                    return Err(
                        RuntimeError::new("type-error", "regex.compile expected Str")
                            .with_span(span),
                    );
                };
                ControlFlow::Continue(match crate::modules::regex::compile(pattern, span) {
                    Ok(regex) => LoweredValue::ResultOk(Box::new(LoweredValue::Regex(Box::new(
                        RegexValue {
                            pattern: pattern.to_string(),
                            regex: Arc::new(regex),
                        },
                    )))),
                    Err(error) => LoweredValue::ResultErr(Box::new(Value::Error(Box::new(error)))),
                })
            }
            FullTag::ExprRequire => {
                let value = indexed_raw(&mut payload, call_span)?;
                let check = indexed_decode::<LoweredTypeCheck>(&mut payload, execution, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let value = match self.eval_indexed_expr(execution, value, slots, span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                ControlFlow::Continue(
                    if lowered_value_satisfies_require(self, &value, &check.ty) {
                        lowered_result_ok(value)
                    } else {
                        lowered_result_err_value(
                            RuntimeError::new(
                                "schema",
                                format!(
                                    "schema check failed: expected {}, found {}",
                                    check.name,
                                    value.type_name()
                                ),
                            )
                            .with_span(span),
                        )
                    },
                )
            }
            FullTag::ExprLoop => {
                let body = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let header = Self::indexed_block_header(slots.len());
                loop {
                    self.service_pending_signal(span)?;
                    if self.signal_state.shutdown_complete {
                        break ControlFlow::Continue(LoweredValue::Unit);
                    }
                    match self
                        .eval_indexed_statement_block(execution, body, &header, slots, span)?
                    {
                        StmtFlow::None | StmtFlow::Continue => {}
                        StmtFlow::Break(value) => {
                            break ControlFlow::Continue(value.unwrap_or(LoweredValue::Unit));
                        }
                        StmtFlow::Return(value) => {
                            break ControlFlow::Continue(value);
                        }
                        StmtFlow::Propagate(value) => {
                            break ControlFlow::Break(value);
                        }
                    }
                }
            }
            FullTag::ExprRetry => {
                let (_, mut delays) = execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, call_span))?;
                let delay_count = indexed_raw(&mut delays, call_span)? as usize;
                let body = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let mut delay_values = Vec::with_capacity(delay_count);
                for _ in 0..delay_count {
                    let delay = indexed_raw(&mut delays, span)?;
                    match self.eval_indexed_expr(execution, delay, slots, span)? {
                        ControlFlow::Continue(LoweredValue::Duration(value)) => {
                            delay_values.push(value);
                        }
                        ControlFlow::Continue(value) => {
                            return Err(RuntimeError::new(
                                "type-error",
                                format!(
                                    "retry delay expected Duration, found {}",
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
                indexed_finish(delays, span)?;
                let header = Self::indexed_block_header(slots.len());
                let max_attempts = delay_values.len() + 1;
                let mut final_error = None;
                let mut final_traceback = None;
                for attempt_index in 0..max_attempts {
                    if attempt_index > 0 {
                        self.sleep_lowered_retry_delay(&delay_values[attempt_index - 1], span)?;
                        if self.signal_state.shutdown_complete {
                            break;
                        }
                    }
                    let attempt_flow =
                        self.eval_indexed_statement_block(execution, body, &header, slots, span)?;
                    match self.lowered_retry_attempt_value(attempt_flow) {
                        LoweredRetryAttemptValue::Success(value) => {
                            self.trace_lowered_retry_attempt(
                                span,
                                attempt_index + 1,
                                max_attempts,
                                None,
                                None,
                            );
                            return Ok(ControlFlow::Continue(LoweredValue::ResultOk(Box::new(
                                value,
                            ))));
                        }
                        LoweredRetryAttemptValue::Failed { error, traceback } => {
                            let next_delay =
                                delay_values.get(attempt_index).map(|delay| delay.millis);
                            self.trace_lowered_retry_attempt(
                                span,
                                attempt_index + 1,
                                max_attempts,
                                next_delay,
                                Some(lowered_trace_error_from_value(&error)),
                            );
                            final_error = Some(error);
                            final_traceback = traceback;
                            self.pending_traceback = None;
                        }
                        LoweredRetryAttemptValue::ControlBreak => {
                            return Ok(ControlFlow::Continue(LoweredValue::Unit));
                        }
                        LoweredRetryAttemptValue::Escape(value) => {
                            return Ok(ControlFlow::Break(value));
                        }
                    }
                }
                let error = final_error.unwrap_or_else(|| {
                    Value::Error(Box::new(RuntimeError::new(
                        "retry",
                        "retry block did not produce a value",
                    )))
                });
                self.pending_traceback = final_traceback;
                ControlFlow::Continue(LoweredValue::ResultErr(Box::new(error)))
            }
            FullTag::ExprFsFiles | FullTag::ExprFsWalk => {
                let root = indexed_raw(&mut payload, call_span)?;
                let gitignore = indexed_decode::<bool>(&mut payload, execution, call_span)?;
                let stat = indexed_decode::<bool>(&mut payload, execution, call_span)?;
                let hidden = indexed_decode::<bool>(&mut payload, execution, call_span)?;
                let exts = indexed_optional_raw(&mut payload, call_span)?;
                let result_wrapped = indexed_decode::<bool>(&mut payload, execution, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let root = match self.eval_indexed_expr(execution, root, slots, span)? {
                    ControlFlow::Continue(LoweredValue::Path(path)) => path,
                    ControlFlow::Continue(_) => {
                        let operation = if tag == FullTag::ExprFsFiles {
                            "fs.files"
                        } else {
                            "fs.walk"
                        };
                        return Err(RuntimeError::new(
                            "type-error",
                            format!("{operation} expected Path"),
                        )
                        .with_span(span));
                    }
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let exts = match self.eval_indexed_optional_expr(execution, exts, slots, span)? {
                    ControlFlow::Continue(Some(value)) => lowered_str_list_arg(
                        Some(value),
                        if tag == FullTag::ExprFsFiles {
                            "fs.files exts"
                        } else {
                            "fs.walk exts"
                        },
                        span,
                    )?,
                    ControlFlow::Continue(None) => Vec::new(),
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let emit = if tag == FullTag::ExprFsFiles {
                    crate::modules::fs::WalkEmit::Files
                } else {
                    crate::modules::fs::WalkEmit::All
                };
                let stream = match crate::modules::fs::walk_filesystem(
                    self.host_path(&root),
                    gitignore,
                    stat,
                    hidden,
                    emit,
                    exts,
                    span,
                ) {
                    Ok(stream) => stream,
                    Err(error) if result_wrapped => {
                        return Ok(ControlFlow::Continue(lowered_result_err_value(error)));
                    }
                    Err(error) => return Err(error),
                };
                let value = LoweredValue::Stream(Box::new(stream));
                ControlFlow::Continue(if result_wrapped {
                    lowered_result_ok(value)
                } else {
                    value
                })
            }
            FullTag::ExprFsList => {
                let op = indexed_decode::<RuntimeOp>(&mut payload, execution, call_span)?;
                let path = indexed_raw(&mut payload, call_span)?;
                let stat = indexed_optional_raw(&mut payload, call_span)?;
                let ordered = indexed_optional_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let operation = if op == RuntimeOp::FsLs {
                    "fs.ls"
                } else {
                    "fs.children"
                };
                let path = match self.eval_indexed_expr(execution, path, slots, span)? {
                    ControlFlow::Continue(value) => lowered_path_arg(value, operation, span)?,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let stat = match self.eval_indexed_optional_expr(execution, stat, slots, span)? {
                    ControlFlow::Continue(value) => {
                        lowered_bool_arg_or(value, true, operation, span)?
                    }
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let ordered =
                    match self.eval_indexed_optional_expr(execution, ordered, slots, span)? {
                        ControlFlow::Continue(value) => {
                            lowered_bool_arg_or(value, true, operation, span)?
                        }
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    };
                ControlFlow::Continue(self.lowered_stream_list_result(
                    fs_module::list_filesystem(self.host_path(&path), stat, ordered, span),
                    span,
                )?)
            }
            FullTag::ExprFsTempDir => {
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                ControlFlow::Continue(
                    match cap_tempfile::TempDir::new(cap_tempfile::ambient_authority()) {
                        Ok(dir) => {
                            let id = self.fs_roots.len() as i64 + 1;
                            self.fs_roots.push(Some(FsRootHandle::TempDir(dir)));
                            lowered_result_ok(fs_root_record(id))
                        }
                        Err(error) => lowered_result_err_value(
                            RuntimeError::new("fs-temp-dir", error.to_string()).with_span(span),
                        ),
                    },
                )
            }
            FullTag::ExprFsWrite | FullTag::ExprPathWrite => {
                let path = indexed_raw(&mut payload, call_span)?;
                let data = indexed_raw(&mut payload, call_span)?;
                let atomic = if tag == FullTag::ExprPathWrite {
                    indexed_decode::<bool>(&mut payload, execution, call_span)?
                } else {
                    false
                };
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let operation = if tag == FullTag::ExprFsWrite {
                    "fs.write"
                } else {
                    "write"
                };
                let path = match self.eval_indexed_expr(execution, path, slots, span)? {
                    ControlFlow::Continue(value) => lowered_path_arg(value, operation, span)?,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let data = match self.eval_indexed_expr(execution, data, slots, span)? {
                    ControlFlow::Continue(value) => {
                        lowered_bytes_or_str_owned(value, "write", span)?
                    }
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let result = if atomic {
                    crate::modules::fs::write_atomic(self.host_path(&path), &data, span)
                } else {
                    crate::modules::fs::write_path(self.host_path(&path), &data, span)
                };
                ControlFlow::Continue(lowered_unit_result(result))
            }
            FullTag::ExprFsMkdir | FullTag::ExprPathMkdir => {
                let path = indexed_raw(&mut payload, call_span)?;
                let parents = indexed_optional_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let operation = if tag == FullTag::ExprFsMkdir {
                    "fs.mkdir"
                } else {
                    "mkdir"
                };
                let path = match self.eval_indexed_expr(execution, path, slots, span)? {
                    ControlFlow::Continue(value) => lowered_path_arg(value, operation, span)?,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let parents =
                    match self.eval_indexed_optional_expr(execution, parents, slots, span)? {
                        ControlFlow::Continue(value) => {
                            lowered_bool_arg_or(value, true, operation, span)?
                        }
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    };
                ControlFlow::Continue(lowered_unit_result(crate::modules::fs::mkdir_path(
                    self.host_path(&path),
                    parents,
                    None,
                    span,
                )))
            }
            FullTag::ExprFsRemove | FullTag::ExprPathRemove => {
                let path = indexed_raw(&mut payload, call_span)?;
                let missing_ok = indexed_optional_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let operation = if tag == FullTag::ExprFsRemove {
                    "fs.remove"
                } else {
                    "remove"
                };
                let path = match self.eval_indexed_expr(execution, path, slots, span)? {
                    ControlFlow::Continue(value) => lowered_path_arg(value, operation, span)?,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let missing_ok =
                    match self.eval_indexed_optional_expr(execution, missing_ok, slots, span)? {
                        ControlFlow::Continue(value) => {
                            lowered_bool_arg_or(value, false, operation, span)?
                        }
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    };
                ControlFlow::Continue(lowered_unit_result(crate::modules::fs::remove_path(
                    self.host_path(&path),
                    missing_ok,
                    span,
                )))
            }
            FullTag::ExprFsCloseRoot | FullTag::ExprFsRootPath => {
                let root = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let root = match self.eval_indexed_expr(execution, root, slots, span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let value = if tag == FullTag::ExprFsCloseRoot {
                    match lowered_root_id(&root, span)
                        .ok()
                        .and_then(|id| {
                            id.checked_sub(1)
                                .and_then(|index| usize::try_from(index).ok())
                        })
                        .and_then(|index| self.fs_roots.get_mut(index))
                    {
                        Some(slot) => {
                            if slot.take().is_some() {
                                lowered_result_ok(LoweredValue::Unit)
                            } else {
                                lowered_result_err_value(
                                    RuntimeError::new("fs-root", "root handle is not active")
                                        .with_span(span),
                                )
                            }
                        }
                        None => lowered_result_err_value(
                            RuntimeError::new("fs-root", "root handle is not active")
                                .with_span(span),
                        ),
                    }
                } else {
                    match lowered_fs_root_dir(&self.fs_roots, &root, span)
                        .and_then(|dir| root_path_from_dir(dir, span))
                    {
                        Ok(path) => lowered_result_ok(LoweredValue::Path(path)),
                        Err(error) => lowered_result_err_value(error),
                    }
                };
                ControlFlow::Continue(value)
            }
            FullTag::ExprPathReadText | FullTag::ExprPathReadBytes => {
                let path = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let operation = if tag == FullTag::ExprPathReadText {
                    "read_text"
                } else {
                    "read_bytes"
                };
                let path = match self.eval_indexed_expr(execution, path, slots, span)? {
                    ControlFlow::Continue(LoweredValue::Path(path)) => path,
                    ControlFlow::Continue(_) => {
                        return Err(RuntimeError::new(
                            "type-error",
                            format!("{operation} expected Path"),
                        )
                        .with_span(span));
                    }
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let value = if tag == FullTag::ExprPathReadText {
                    match read_host_path_bytes_vec(&self.host_path(&path), span) {
                        Ok(bytes) => match String::from_utf8(bytes) {
                            Ok(text) => {
                                LoweredValue::ResultOk(Box::new(LoweredValue::Str(text.into())))
                            }
                            Err(error) => {
                                LoweredValue::ResultErr(Box::new(Value::Error(Box::new(
                                    RuntimeError::new(
                                        "invalid-utf8",
                                        format!(
                                            "file is not valid UTF-8 at byte {}",
                                            error.utf8_error().valid_up_to()
                                        ),
                                    )
                                    .with_span(span),
                                ))))
                            }
                        },
                        Err(error) => {
                            LoweredValue::ResultErr(Box::new(Value::Error(Box::new(error))))
                        }
                    }
                } else {
                    match read_host_path_bytes(&self.host_path(&path), span) {
                        Ok(bytes) => LoweredValue::ResultOk(Box::new(LoweredValue::Bytes(bytes))),
                        Err(error) => {
                            LoweredValue::ResultErr(Box::new(Value::Error(Box::new(error))))
                        }
                    }
                };
                ControlFlow::Continue(value)
            }
            FullTag::ExprPathExists
            | FullTag::ExprPathExecutable
            | FullTag::ExprPathDu
            | FullTag::ExprPathMetadata
            | FullTag::ExprPathReadlink
            | FullTag::ExprPathResolve => {
                let path = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let operation = match tag {
                    FullTag::ExprPathExists => "exists",
                    FullTag::ExprPathExecutable => "executable",
                    FullTag::ExprPathDu => "du",
                    FullTag::ExprPathMetadata => "metadata",
                    FullTag::ExprPathReadlink => "readlink",
                    FullTag::ExprPathResolve => "resolve",
                    _ => unreachable!(),
                };
                let path = match self.eval_indexed_expr(execution, path, slots, span)? {
                    ControlFlow::Continue(value) => lowered_path_arg(value, operation, span)?,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let host_path = self.host_path(&path);
                let value = match tag {
                    FullTag::ExprPathExists => match crate::modules::fs::exists(host_path, span) {
                        Ok(value) => lowered_result_ok(LoweredValue::Bool(value)),
                        Err(error) => lowered_result_err_value(error),
                    },
                    FullTag::ExprPathExecutable => {
                        match crate::modules::fs::executable(host_path, span) {
                            Ok(value) => lowered_result_ok(LoweredValue::Bool(value)),
                            Err(error) => lowered_result_err_value(error),
                        }
                    }
                    FullTag::ExprPathDu => match crate::modules::fs::disk_usage(host_path, span) {
                        Ok(value) => lowered_result_ok(LoweredValue::Int(value)),
                        Err(error) => lowered_result_err_value(error),
                    },
                    FullTag::ExprPathMetadata => {
                        match crate::modules::fs::metadata(host_path, span) {
                            Ok(value) => lowered_value_from_runtime_any(&value)
                                .map(lowered_result_ok)
                                .ok_or_else(|| {
                                    RuntimeError::new(
                                        "type-error",
                                        format!(
                                            "metadata produced unsupported {}",
                                            value.type_name()
                                        ),
                                    )
                                    .with_span(span)
                                })?,
                            Err(error) => lowered_result_err_value(error),
                        }
                    }
                    FullTag::ExprPathReadlink => {
                        match crate::modules::fs::readlink(host_path, span) {
                            Ok(value) => lowered_value_from_runtime_any(&value)
                                .map(lowered_result_ok)
                                .ok_or_else(|| {
                                    RuntimeError::new(
                                        "type-error",
                                        format!(
                                            "readlink produced unsupported {}",
                                            value.type_name()
                                        ),
                                    )
                                    .with_span(span)
                                })?,
                            Err(error) => lowered_result_err_value(error),
                        }
                    }
                    FullTag::ExprPathResolve => {
                        match crate::modules::fs::resolve_path(host_path, span) {
                            Ok(value) => lowered_result_ok(LoweredValue::Path(value)),
                            Err(error) => lowered_result_err_value(error),
                        }
                    }
                    _ => unreachable!(),
                };
                ControlFlow::Continue(value)
            }
            FullTag::ExprJsonEncode => {
                let value = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let value = match self.eval_indexed_expr(execution, value, slots, span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                ControlFlow::Continue(match lowered_encode_json(&value, false, span) {
                    Ok(text) => LoweredValue::ResultOk(Box::new(LoweredValue::Str(text.into()))),
                    Err(error) => LoweredValue::ResultErr(Box::new(Value::Error(Box::new(error)))),
                })
            }
            FullTag::ExprArchiveTarCreate => {
                let path = indexed_raw(&mut payload, call_span)?;
                let root = indexed_raw(&mut payload, call_span)?;
                let entries = indexed_raw(&mut payload, call_span)?;
                let compression = indexed_optional_raw(&mut payload, call_span)?;
                let overwrite = indexed_optional_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let path = match self.eval_indexed_expr(execution, path, slots, span)? {
                    ControlFlow::Continue(value) => {
                        lowered_path_arg(value, "archive.tar_create", span)?
                    }
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let root = match self.eval_indexed_expr(execution, root, slots, span)? {
                    ControlFlow::Continue(value) => {
                        lowered_path_arg(value, "archive.tar_create", span)?
                    }
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let entries = match self.eval_indexed_expr(execution, entries, slots, span)? {
                    ControlFlow::Continue(value) => {
                        lowered_path_list_arg(value, "archive.tar_create", span)?
                    }
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let compression =
                    match self.eval_indexed_optional_expr(execution, compression, slots, span)? {
                        ControlFlow::Continue(value) => {
                            lowered_str_arg_owned(value, "auto", "archive.tar_create", span)?
                        }
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    };
                let overwrite =
                    match self.eval_indexed_optional_expr(execution, overwrite, slots, span)? {
                        ControlFlow::Continue(value) => {
                            lowered_bool_arg_or(value, false, "archive.tar_create", span)?
                        }
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    };
                ControlFlow::Continue(lowered_unit_result(crate::modules::archive::tar_create(
                    self.host_path(&path),
                    self.host_path(&root),
                    entries,
                    &compression,
                    overwrite,
                    span,
                )))
            }
            FullTag::ExprArchiveTarList => {
                let path = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let path = match self.eval_indexed_expr(execution, path, slots, span)? {
                    ControlFlow::Continue(value) => {
                        lowered_path_arg(value, "archive.tar_list", span)?
                    }
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                ControlFlow::Continue(
                    match crate::modules::archive::tar_list(
                        self.host_path(&path),
                        "auto",
                        Vec::new(),
                        span,
                    ) {
                        Ok(stream) => lowered_result_ok(LoweredValue::Stream(Box::new(stream))),
                        Err(error) => lowered_result_err_value(error),
                    },
                )
            }
            FullTag::ExprArchiveTarExtract => {
                let path = indexed_raw(&mut payload, call_span)?;
                let dest = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let path = match self.eval_indexed_expr(execution, path, slots, span)? {
                    ControlFlow::Continue(value) => {
                        lowered_path_arg(value, "archive.tar_extract", span)?
                    }
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let dest = match self.eval_indexed_expr(execution, dest, slots, span)? {
                    ControlFlow::Continue(value) => {
                        lowered_path_arg(value, "archive.tar_extract", span)?
                    }
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                ControlFlow::Continue(lowered_unit_result(crate::modules::archive::tar_extract(
                    self.host_path(&path),
                    self.host_path(&dest),
                    0,
                    "auto",
                    false,
                    Vec::new(),
                    span,
                )))
            }
            FullTag::ExprHashVerifyFile => {
                let path = indexed_raw(&mut payload, call_span)?;
                let algorithm =
                    indexed_decode::<HashAlgorithm>(&mut payload, execution, call_span)?;
                let expected = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let path = match self.eval_indexed_expr(execution, path, slots, span)? {
                    ControlFlow::Continue(value) => {
                        lowered_path_arg(value, "hash.verify_file", span)?
                    }
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let expected = match self.eval_indexed_expr(execution, expected, slots, span)? {
                    ControlFlow::Continue(value) => {
                        lowered_str_arg_owned(Some(value), "", "hash.verify_file", span)?
                    }
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                ControlFlow::Continue(
                    match hash_module::digest_file(algorithm, &self.host_path(&path), span)
                        .and_then(|digest| hash_module::verify_hex(&digest, &expected, span))
                    {
                        Ok(()) => lowered_result_ok(LoweredValue::Unit),
                        Err(error) => lowered_result_err_value(error),
                    },
                )
            }
            FullTag::ExprModuleCall => {
                let op = indexed_decode::<RuntimeOp>(&mut payload, execution, call_span)?;
                let (_, mut args) = execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, call_span))?;
                let len = indexed_raw(&mut args, call_span)? as usize;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let mut values = Vec::with_capacity(len);
                for _ in 0..len {
                    let arg = indexed_raw(&mut args, span)?;
                    match self.eval_indexed_expr(execution, arg, slots, span)? {
                        ControlFlow::Continue(value) => values.push(value),
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    }
                }
                indexed_finish(args, span)?;
                if !self.trace_enabled {
                    return self.eval_lowered_module_call_values(op, values, span);
                }
                let trace_name = crate::modules::signature::api_spec()
                    .op_trace_name(op)
                    .map(str::to_string);
                self.trace_enter(
                    TraceKind::ModuleCall,
                    Some(span),
                    trace_name.as_deref(),
                    TracePayload::None,
                );
                let result = self.eval_lowered_module_call_values(op, values, span);
                self.trace_exit(
                    TraceKind::ModuleResult,
                    Some(span),
                    trace_name.as_deref(),
                    TracePayload::None,
                );
                return result;
            }
            FullTag::ExprProcessCommandArgv => {
                let target = indexed_raw(&mut payload, call_span)?;
                let argv = indexed_raw(&mut payload, call_span)?;
                let mut optional = [None; 12];
                for value in &mut optional {
                    *value = indexed_optional_raw(&mut payload, call_span)?;
                }
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let target = match self.eval_indexed_expr(execution, target, slots, span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let argv = match self.eval_indexed_expr(execution, argv, slots, span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let mut evaluated = Vec::with_capacity(optional.len());
                for value in optional {
                    match self.eval_indexed_optional_expr(execution, value, slots, span)? {
                        ControlFlow::Continue(value) => evaluated.push(value),
                        ControlFlow::Break(value) => {
                            return Ok(ControlFlow::Break(value));
                        }
                    }
                }
                let [
                    cwd,
                    env,
                    stdin,
                    stdout,
                    stderr,
                    stdout_append,
                    stderr_append,
                    timeout,
                    detach,
                    new_session,
                    ignore_hup,
                    cpu_max,
                ]: [Option<LoweredValue>; 12] = evaluated
                    .try_into()
                    .expect("indexed command optional field count");
                ControlFlow::Continue(lowered_command_plan_value(
                    target,
                    argv,
                    cwd,
                    env,
                    stdin,
                    stdout,
                    stderr,
                    stdout_append,
                    stderr_append,
                    timeout,
                    detach,
                    new_session,
                    ignore_hup,
                    cpu_max,
                    span,
                )?)
            }
            FullTag::ExprProcessCommandBuilder => {
                let entries = Self::decode_indexed_process_command_entries(
                    &mut payload,
                    execution,
                    call_span,
                )?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                self.trace_enter(
                    TraceKind::ModuleCall,
                    Some(span),
                    Some("process.command"),
                    TracePayload::None,
                );
                let mut plan = None;
                let mut cwd = None;
                let mut env = BTreeMap::new();
                let mut stdin = None;
                let mut stdout = None;
                let mut stderr = None;
                let mut stdout_append = false;
                let mut stderr_append = false;
                let mut timeout = None;
                let mut cpu_max = None;
                let mut detach = None;
                let mut new_session = None;
                let mut ignore_hup = None;
                for entry in entries {
                    match entry {
                        ProcessCommandEntry::Field { name, value, span } => {
                            let value =
                                match self.eval_indexed_expr(execution, value, slots, span)? {
                                    ControlFlow::Continue(value) => value,
                                    ControlFlow::Break(value) => {
                                        return Ok(ControlFlow::Break(value));
                                    }
                                };
                            match name.as_str().as_str() {
                                "cwd" => {
                                    cwd =
                                        Some(lowered_path_like_arg(value, "process.command", span)?)
                                }
                                "env" => env.extend(lowered_env_record_arg(
                                    value,
                                    "process.command",
                                    span,
                                )?),
                                "stdin" => stdin = Some(value),
                                "stdout" => stdout = Some(value),
                                "stderr" => stderr = Some(value),
                                "stdout_append" => {
                                    stdout_append =
                                        lowered_bool_builder_field(value, "stdout_append", span)?
                                }
                                "stderr_append" => {
                                    stderr_append =
                                        lowered_bool_builder_field(value, "stderr_append", span)?
                                }
                                "timeout" => {
                                    timeout = Some(lowered_duration_arg(
                                        Some(value),
                                        "process.command",
                                        span,
                                    )?)
                                }
                                "cpu_max" => {
                                    let value =
                                        lowered_int_arg(Some(value), "process.command", span)?;
                                    if value <= 0 {
                                        return Err(RuntimeError::new(
                                            "cpu-max",
                                            "cpu_max must be positive",
                                        )
                                        .with_span(span));
                                    }
                                    cpu_max = Some(value);
                                }
                                "detach" => {
                                    detach =
                                        Some(lowered_bool_builder_field(value, "detach", span)?)
                                }
                                "new_session" => {
                                    new_session = Some(lowered_bool_builder_field(
                                        value,
                                        "new_session",
                                        span,
                                    )?)
                                }
                                "ignore_hup" => {
                                    ignore_hup =
                                        Some(lowered_bool_builder_field(value, "ignore_hup", span)?)
                                }
                                _ => {
                                    return Err(RuntimeError::new(
                                        "builder-field",
                                        format!("unknown process.command field `{name}`"),
                                    )
                                    .with_span(span));
                                }
                            }
                        }
                        ProcessCommandEntry::Run {
                            target,
                            args,
                            env: run_env,
                            timeout: run_timeout,
                            cpu_max: run_cpu_max,
                            span,
                        } => {
                            if plan.is_some() {
                                return Err(RuntimeError::new(
                                    "builder-entry",
                                    "process.command accepts one run entry",
                                )
                                .with_span(span));
                            }
                            let target_items =
                                match self.eval_indexed_run_arg(execution, &target, slots, span)? {
                                    ControlFlow::Continue(value) => value,
                                    ControlFlow::Break(value) => {
                                        return Ok(ControlFlow::Break(value));
                                    }
                                };
                            let [target_value]: [Vec<u8>; 1] =
                                target_items.try_into().map_err(|_| {
                                    RuntimeError::new(
                                        "argv-conversion",
                                        "run target must produce one argv item",
                                    )
                                    .with_span(target.span)
                                })?;
                            let mut argv = Vec::new();
                            for arg in &args {
                                match self.eval_indexed_run_arg(execution, arg, slots, span)? {
                                    ControlFlow::Continue(items) => argv.extend(items),
                                    ControlFlow::Break(value) => {
                                        return Ok(ControlFlow::Break(value));
                                    }
                                }
                            }
                            let env_overlay = match self
                                .eval_indexed_run_env(execution, &run_env, slots, span)?
                            {
                                ControlFlow::Continue(value) => value,
                                ControlFlow::Break(value) => {
                                    return Ok(ControlFlow::Break(value));
                                }
                            };
                            let mut run_env = BTreeMap::new();
                            for (name, value) in env_overlay {
                                run_env.insert(
                                    String::from_utf8_lossy(&name).into_owned(),
                                    String::from_utf8_lossy(&value).into_owned(),
                                );
                            }
                            let run_timeout = match self.eval_indexed_optional_expr(
                                execution,
                                run_timeout,
                                slots,
                                span,
                            )? {
                                ControlFlow::Continue(value) => value
                                    .map(|value| {
                                        lowered_duration_arg(Some(value), "process.command", span)
                                    })
                                    .transpose()?,
                                ControlFlow::Break(value) => {
                                    return Ok(ControlFlow::Break(value));
                                }
                            };
                            let run_cpu_max = match self.eval_indexed_optional_expr(
                                execution,
                                run_cpu_max,
                                slots,
                                span,
                            )? {
                                ControlFlow::Continue(value) => value
                                    .map(|value| {
                                        lowered_int_arg(Some(value), "process.command", span)
                                    })
                                    .transpose()?,
                                ControlFlow::Break(value) => {
                                    return Ok(ControlFlow::Break(value));
                                }
                            };
                            if run_cpu_max.is_some_and(|value| value <= 0) {
                                return Err(RuntimeError::new(
                                    "cpu-max",
                                    "cpu_max must be positive",
                                )
                                .with_span(span));
                            }
                            plan = Some(CommandPlan {
                                target: target_value,
                                argv,
                                cwd: None,
                                env: run_env,
                                redirections: Vec::new(),
                                timeout: run_timeout,
                                cpu_max: run_cpu_max,
                                detach: false,
                                new_session: false,
                                ignore_hup: false,
                            });
                        }
                    }
                }
                let mut plan = plan.ok_or_else(|| {
                    RuntimeError::new("builder-check", "process.command requires a run entry")
                        .with_span(span)
                })?;
                if cwd.is_some() {
                    plan.cwd = cwd;
                }
                plan.env.extend(env);
                plan.redirections.extend(lowered_command_redirections(
                    stdin,
                    stdout,
                    stderr,
                    stdout_append,
                    stderr_append,
                    "process.command",
                    span,
                )?);
                if timeout.is_some() {
                    plan.timeout = timeout;
                }
                if cpu_max.is_some() {
                    plan.cpu_max = cpu_max;
                }
                if let Some(value) = detach {
                    plan.detach = value;
                }
                if let Some(value) = new_session {
                    plan.new_session = value;
                }
                if let Some(value) = ignore_hup {
                    plan.ignore_hup = value;
                }
                self.trace_exit(
                    TraceKind::ModuleResult,
                    Some(span),
                    Some("process.command"),
                    TracePayload::None,
                );
                ControlFlow::Continue(LoweredValue::Command(Box::new(plan)))
            }
            FullTag::ExprRunPipeline => {
                let segments =
                    Self::decode_indexed_run_segments(&mut payload, execution, call_span)?;
                let propagate = indexed_decode::<bool>(&mut payload, execution, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let mut invocations = Vec::with_capacity(segments.len());
                for segment in &segments {
                    match self.indexed_process_invocation(
                        execution,
                        &segment.target,
                        &segment.args,
                        &segment.env,
                        &segment.redirections,
                        segment.timeout,
                        segment.cpu_max,
                        slots,
                        span,
                    )? {
                        ControlFlow::Continue(value) => invocations.push(value),
                        ControlFlow::Break(value) => {
                            return Ok(ControlFlow::Break(value));
                        }
                    }
                }
                self.trace_lowered_pipeline_enter(span);
                let end = match run_pipeline_inherit_with_policy(&invocations, self) {
                    Ok(end) => end,
                    Err(error) => {
                        self.trace_lowered_pipeline_end(
                            span,
                            &ProcessEnd {
                                pid: Some(0),
                                status: error.status.as_deref().cloned(),
                                error: Some(error.clone()),
                            },
                        );
                        return Ok(ControlFlow::Continue(lowered_process_run_error(error)));
                    }
                };
                if let Some(status) = &end.status {
                    self.last_status = Some(status.clone());
                }
                self.trace_lowered_pipeline_end(span, &end);
                if self.signal_state.shutdown_complete
                    && self.signal_state.shutdown_status.is_some()
                {
                    return Ok(ControlFlow::Continue(LoweredValue::ResultOk(Box::new(
                        LoweredValue::Status(Box::new(
                            end.status
                                .clone()
                                .unwrap_or_else(|| ProcessStatus::signaled(libc::SIGTERM)),
                        )),
                    ))));
                }
                let status = end
                    .status
                    .clone()
                    .unwrap_or_else(|| ProcessStatus::exited(1));
                if !status.success && propagate {
                    ControlFlow::Continue(lowered_process_run_error(
                        RunError::from_status(status).with_span(span),
                    ))
                } else if propagate {
                    ControlFlow::Continue(LoweredValue::ResultOk(Box::new(LoweredValue::Status(
                        Box::new(status),
                    ))))
                } else {
                    ControlFlow::Continue(LoweredValue::Status(Box::new(status)))
                }
            }
            FullTag::ExprRunCapture | FullTag::ExprSpawnRun => {
                let spawn = tag == FullTag::ExprSpawnRun;
                let kind = if spawn {
                    RunKind::Plain
                } else {
                    indexed_decode::<RunKind>(&mut payload, execution, call_span)?
                };
                let target = Self::decode_indexed_run_arg(&mut payload, execution, call_span)?;
                let args = Self::decode_indexed_run_args(&mut payload, execution, call_span)?;
                let env = Self::decode_indexed_run_env(&mut payload, execution, call_span)?;
                let redirections =
                    Self::decode_indexed_run_redirections(&mut payload, execution, call_span)?;
                let timeout = indexed_optional_raw(&mut payload, call_span)?;
                let cpu_max = indexed_optional_raw(&mut payload, call_span)?;
                let (propagate, assert_success) = if spawn {
                    (false, false)
                } else {
                    (
                        indexed_decode::<bool>(&mut payload, execution, call_span)?,
                        indexed_decode::<bool>(&mut payload, execution, call_span)?,
                    )
                };
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let invocation = match self.indexed_process_invocation(
                    execution,
                    &target,
                    &args,
                    &env,
                    &redirections,
                    timeout,
                    cpu_max,
                    slots,
                    span,
                )? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                if spawn {
                    return self.eval_lowered_spawn_invocation(
                        invocation,
                        SpawnOptions::default(),
                        span,
                    );
                }
                self.trace_process_run_start(span, &invocation);
                let execution_result = execute_run_with_policy(
                    kind,
                    std::slice::from_ref(&invocation),
                    span,
                    assert_success,
                    self,
                );
                if let Some(status) = execution_result.end.status.clone() {
                    self.last_status = Some(status);
                }
                self.trace_process_run_end(span, &execution_result.end);
                if self.signal_state.shutdown_complete
                    && self.signal_state.shutdown_status.is_some()
                {
                    return Ok(ControlFlow::Continue(LoweredValue::ResultOk(Box::new(
                        LoweredValue::Status(Box::new(
                            execution_result
                                .end
                                .status
                                .clone()
                                .unwrap_or_else(|| ProcessStatus::signaled(libc::SIGTERM)),
                        )),
                    ))));
                }
                let value = execution_result.value?;
                let mut value = lowered_value_from_runtime_any(&value).ok_or_else(|| {
                    RuntimeError::new(
                        "type-error",
                        format!("lowered run produced unsupported {}", value.type_name()),
                    )
                    .with_span(span)
                })?;
                if matches!(kind, RunKind::Status)
                    && let LoweredValue::ResultOk(inner) = value
                {
                    value = *inner;
                }
                if propagate && matches!(value, LoweredValue::ResultErr(_)) {
                    return Ok(ControlFlow::Break(
                        self.lowered_question_propagation_value(value, span)?,
                    ));
                }
                ControlFlow::Continue(value)
            }
            FullTag::ExprSpawnCommand => {
                let command = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let command = match self.eval_indexed_expr(execution, command, slots, span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let LoweredValue::Command(plan) = command else {
                    return Err(RuntimeError::new(
                        "type-error",
                        format!("spawn expected Command, found {}", command.type_name()),
                    )
                    .with_span(span));
                };
                let options = SpawnOptions {
                    detach: plan.detach,
                    new_session: plan.new_session,
                    ignore_hup: plan.ignore_hup,
                };
                let invocation = self.invocation_from_command_plan(&plan, span)?;
                return self.eval_lowered_spawn_invocation(invocation, options, span);
            }
            FullTag::ExprWait => {
                let target = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let target = match self.eval_indexed_expr(execution, target, slots, span)? {
                    ControlFlow::Continue(value) => value.into_value(),
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let value = match target {
                    Value::ProcessHandle(handle) => self.wait_one_process_handle(*handle, span)?,
                    Value::List(items) => self.wait_process_handle_list(items, span)?,
                    value => process_handle::process_handle_error(
                        RunError::new(
                            "unknown",
                            format!(
                                "wait expected ProcessHandle or List[ProcessHandle], found {}",
                                value.type_name()
                            ),
                        )
                        .with_span(span),
                    ),
                };
                ControlFlow::Continue(lowered_value_from_runtime_any(&value).ok_or_else(|| {
                    RuntimeError::new("type-error", "lowered wait produced unsupported value")
                        .with_span(span)
                })?)
            }
            FullTag::ExprAbort => {
                let status = indexed_raw(&mut payload, call_span)?;
                let force = indexed_optional_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let status = match self.eval_indexed_expr(execution, status, slots, span)? {
                    ControlFlow::Continue(LoweredValue::Int(value)) => exit_status(value, span)?,
                    ControlFlow::Continue(value) => {
                        return Err(RuntimeError::new(
                            "type-error",
                            format!("abort status expected Int, found {}", value.type_name()),
                        )
                        .with_span(span));
                    }
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let force = match force {
                    Some(force) => match self.eval_indexed_expr(execution, force, slots, span)? {
                        ControlFlow::Continue(LoweredValue::Bool(value)) => value,
                        ControlFlow::Continue(value) => {
                            return Err(RuntimeError::new(
                                "type-error",
                                format!("abort force expected Bool, found {}", value.type_name()),
                            )
                            .with_span(span));
                        }
                        ControlFlow::Break(value) => {
                            return Ok(ControlFlow::Break(value));
                        }
                    },
                    None => false,
                };
                return Err(RuntimeError::abort(status, force).with_span(span));
            }
            FullTag::ExprOk => {
                let value = indexed_raw(&mut payload, call_span)?;
                indexed_finish(payload, call_span)?;
                let value = match self.eval_indexed_expr(execution, value, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                ControlFlow::Continue(LoweredValue::ResultOk(Box::new(value)))
            }
            FullTag::ExprErr => {
                let value = indexed_raw(&mut payload, call_span)?;
                indexed_finish(payload, call_span)?;
                let value = match self.eval_indexed_expr(execution, value, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                ControlFlow::Continue(LoweredValue::ResultErr(Box::new(value.into_value())))
            }
            FullTag::ExprError => {
                let error = match indexed_raw(&mut payload, call_span)? {
                    0 => {
                        let kind = indexed_decode::<String>(&mut payload, execution, call_span)?;
                        let message = indexed_decode::<String>(&mut payload, execution, call_span)?;
                        LoweredValue::Error(Box::new(error_constructor(kind, message)))
                    }
                    1 => {
                        let family = indexed_decode::<String>(&mut payload, execution, call_span)
                            .map_err(|error| {
                            RuntimeError::new(
                                error.kind,
                                format!("structured error family: {}", error.message),
                            )
                            .with_span(call_span)
                        })?;
                        let variant = indexed_decode::<String>(&mut payload, execution, call_span)
                            .map_err(|error| {
                                RuntimeError::new(
                                    error.kind,
                                    format!("structured error variant: {}", error.message),
                                )
                                .with_span(call_span)
                            })?;
                        let (_, mut fields) = execution
                            .block(&mut payload, BLOCK_LIST)
                            .map_err(|error| indexed_error(error, call_span))?;
                        let field_count = indexed_raw(&mut fields, call_span)? as usize;
                        let (_, mut facets) = execution
                            .block(&mut payload, BLOCK_LIST)
                            .map_err(|error| indexed_error(error, call_span))?;
                        let facet_count = indexed_raw(&mut facets, call_span)? as usize;
                        let mut record = RecordMap::new();
                        for _ in 0..field_count {
                            let name =
                                indexed_decode::<Arc<str>>(&mut fields, execution, call_span)
                                    .map_err(|error| {
                                        RuntimeError::new(
                                            error.kind,
                                            format!("structured error field: {}", error.message),
                                        )
                                        .with_span(call_span)
                                    })?;
                            let value = indexed_raw(&mut fields, call_span)?;
                            let value =
                                match self.eval_indexed_expr(execution, value, slots, call_span)? {
                                    ControlFlow::Continue(value) => value.into_value(),
                                    ControlFlow::Break(value) => {
                                        return Ok(ControlFlow::Break(value));
                                    }
                                };
                            record.insert(name, value);
                        }
                        indexed_finish(fields, call_span)?;
                        let mut facet_names = Vec::with_capacity(facet_count);
                        for _ in 0..facet_count {
                            facet_names.push(
                                indexed_decode::<Name>(&mut facets, execution, call_span)
                                    .map_err(|error| {
                                        RuntimeError::new(
                                            error.kind,
                                            format!("structured error facet: {}", error.message),
                                        )
                                        .with_span(call_span)
                                    })?
                                    .as_str()
                                    .to_string(),
                            );
                        }
                        indexed_finish(facets, call_span)?;
                        let message = match record.get("message") {
                            Some(Value::Str(message)) => message.to_string(),
                            _ => format!("{family}.{variant}"),
                        };
                        LoweredValue::Error(Box::new(structured_error_constructor(
                            family,
                            variant,
                            record,
                            facet_names,
                            message,
                        )))
                    }
                    _ => {
                        return Err(RuntimeError::new(
                            "indexed-ir",
                            "invalid indexed error expression tag",
                        )
                        .with_span(call_span));
                    }
                };
                indexed_finish(payload, call_span)?;
                ControlFlow::Continue(error)
            }
            FullTag::ExprTry => {
                let value = indexed_raw(&mut payload, call_span)?;
                indexed_finish(payload, call_span)?;
                return match self.eval_indexed_expr(execution, value, slots, call_span)? {
                    ControlFlow::Break(value) => Ok(ControlFlow::Break(value)),
                    ControlFlow::Continue(LoweredValue::ResultOk(value)) => {
                        Ok(ControlFlow::Continue(*value))
                    }
                    ControlFlow::Continue(LoweredValue::ResultErr(error)) => {
                        let value = self.lowered_question_propagation_value(
                            LoweredValue::ResultErr(error),
                            call_span,
                        )?;
                        Ok(ControlFlow::Break(value))
                    }
                    ControlFlow::Continue(_) => Err(RuntimeError::new(
                        "type-error",
                        "lowered `?` expected Result",
                    )
                    .with_span(call_span)),
                };
            }
            FullTag::ExprCall => {
                let function =
                    indexed_decode::<LoweredFunctionKey>(&mut payload, execution, call_span)?;
                let (_, mut args) = execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, call_span))?;
                let len = indexed_raw(&mut args, call_span)? as usize;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let mut values = Vec::with_capacity(len);
                for _ in 0..len {
                    let kind = indexed_raw(&mut args, span)?;
                    let arg = indexed_raw(&mut args, span)?;
                    let value = match self.eval_indexed_expr(execution, arg, slots, span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    };
                    match kind {
                        0 => values.push(value),
                        1 => values.extend(lowered_splice_arg_items(value, span)?),
                        _ => {
                            return Err(RuntimeError::new(
                                "indexed-ir",
                                "invalid indexed call argument kind",
                            )
                            .with_span(span));
                        }
                    }
                }
                indexed_finish(args, span)?;
                return self
                    .eval_indexed_named_call(function, &values, span)
                    .map(ControlFlow::Continue);
            }
            FullTag::ExprDirectPureCall => {
                let function =
                    indexed_decode::<LoweredFunctionKey>(&mut payload, execution, call_span)?;
                let (_, mut args) = execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, call_span))?;
                let len = indexed_raw(&mut args, call_span)? as usize;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let mut values = Vec::with_capacity(len);
                for _ in 0..len {
                    let kind = indexed_raw(&mut args, span)?;
                    let arg = indexed_raw(&mut args, span)?;
                    let value = match self.eval_indexed_expr(execution, arg, slots, span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    };
                    match kind {
                        0 => values.push(value),
                        1 => values.extend(lowered_splice_arg_items(value, span)?),
                        _ => {
                            return Err(RuntimeError::new(
                                "indexed-ir",
                                "invalid indexed call argument kind",
                            )
                            .with_span(span));
                        }
                    }
                }
                indexed_finish(args, span)?;
                let result = if self.trace_enabled {
                    self.eval_indexed_named_call(function, &values, span)?
                } else {
                    self.eval_indexed_direct_pure_call(function, &values, span)?
                };
                return Ok(ControlFlow::Continue(result));
            }
            FullTag::ExprDynamicCall => {
                let callee = indexed_raw(&mut payload, call_span)?;
                let (_, mut args) = execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, call_span))?;
                let arg_count = indexed_raw(&mut args, call_span)? as usize;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let callee = match self.eval_indexed_expr(execution, callee, slots, span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let mut values = Vec::with_capacity(arg_count);
                for _ in 0..arg_count {
                    let splice = match indexed_raw(&mut args, span)? {
                        0 => false,
                        1 => true,
                        _ => {
                            return Err(RuntimeError::new(
                                "indexed-ir",
                                "invalid indexed call argument tag",
                            )
                            .with_span(span));
                        }
                    };
                    let arg = indexed_raw(&mut args, span)?;
                    let value = match self.eval_indexed_expr(execution, arg, slots, span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => {
                            return Ok(ControlFlow::Break(value));
                        }
                    };
                    if splice {
                        values.extend(
                            lowered_splice_arg_items(value, span)?
                                .into_iter()
                                .map(LoweredValue::into_value),
                        );
                    } else {
                        values.push(value.into_value());
                    }
                }
                indexed_finish(args, span)?;
                let result = match callee {
                    LoweredValue::Pure(function) => self
                        .call_indexed_direct(
                            function
                                .as_name()
                                .map(LoweredFunctionKey::Name)
                                .or_else(|| {
                                    function.as_qualified().map(LoweredFunctionKey::Qualified)
                                })
                                .expect("function identity is interned"),
                            LoweredFunctionKind::Pure,
                            &values,
                            span,
                        )
                        .ok_or_else(|| {
                            RuntimeError::new(
                                "unresolved-call",
                                format!(
                                    "dynamic call to {} could not be lowered",
                                    function.display_name()
                                ),
                            )
                            .with_span(span)
                        })??,
                    LoweredValue::Proc(function) => self
                        .call_indexed_direct(
                            function
                                .as_name()
                                .map(LoweredFunctionKey::Name)
                                .or_else(|| {
                                    function.as_qualified().map(LoweredFunctionKey::Qualified)
                                })
                                .expect("function identity is interned"),
                            LoweredFunctionKind::Proc,
                            &values,
                            span,
                        )
                        .ok_or_else(|| {
                            RuntimeError::new(
                                "unresolved-call",
                                format!(
                                    "dynamic call to {} could not be lowered",
                                    function.display_name()
                                ),
                            )
                            .with_span(span)
                        })??,
                    other => {
                        return Err(RuntimeError::new(
                            "type-error",
                            format!(
                                "dynamic call expected Pure or Proc, found {}",
                                other.type_name()
                            ),
                        )
                        .with_span(span));
                    }
                };
                ControlFlow::Continue(lowered_value_from_runtime_any(&result).ok_or_else(|| {
                    RuntimeError::new(
                        "type-error",
                        format!("dynamic call returned unsupported {}", result.type_name()),
                    )
                    .with_span(span)
                })?)
            }
            FullTag::ExprSelfCall => {
                let (_, mut args) = execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, call_span))?;
                let len = indexed_raw(&mut args, call_span)? as usize;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let mut values = Vec::with_capacity(len);
                for _ in 0..len {
                    let kind = indexed_raw(&mut args, span)?;
                    let arg = indexed_raw(&mut args, span)?;
                    let value = match self.eval_indexed_expr(execution, arg, slots, span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    };
                    match kind {
                        0 => values.push(value),
                        1 => values.extend(lowered_splice_arg_items(value, span)?),
                        _ => {
                            return Err(RuntimeError::new(
                                "indexed-ir",
                                "invalid indexed call argument kind",
                            )
                            .with_span(span));
                        }
                    }
                }
                indexed_finish(args, span)?;
                let (function, _) = execution
                    .function_identity()
                    .map_err(|error| indexed_error(error, span))?;
                return self
                    .eval_indexed_self_call(function, &values, span)
                    .map(ControlFlow::Continue);
            }
            _ => {
                return Err(RuntimeError::new(
                    "indexed-ir",
                    format!("direct indexed evaluator does not support {tag:?}"),
                )
                .with_span(call_span));
            }
        };
        Ok(result)
    }

    fn eval_indexed_binary_stack(
        &mut self,
        execution: &FullExecution<'_>,
        slots: &mut [LoweredValue],
        call_span: Span,
        op: BinaryOp,
        left: u32,
        right: u32,
        span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let mut work = vec![
            BinaryWork::Apply { op, span },
            BinaryWork::Expr(right),
            BinaryWork::Expr(left),
        ];
        let mut values = Vec::new();
        while let Some(item) = work.pop() {
            match item {
                BinaryWork::Apply { op, span } => {
                    let right = values.pop().ok_or_else(|| {
                        RuntimeError::new(
                            "indexed-ir",
                            "binary expression is missing a right value",
                        )
                        .with_span(span)
                    })?;
                    let left = values.pop().ok_or_else(|| {
                        RuntimeError::new("indexed-ir", "binary expression is missing a left value")
                            .with_span(span)
                    })?;
                    values.push(lowered_binary_value(op, left, right, span)?);
                }
                BinaryWork::Expr(instruction) => {
                    let (tag, mut payload) =
                        indexed_value(execution.instruction_id(instruction), call_span)?;
                    if tag != FullTag::ExprBinary {
                        match self.eval_indexed_expr(execution, instruction, slots, call_span)? {
                            ControlFlow::Continue(value) => values.push(value),
                            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                        }
                        continue;
                    }
                    let op = indexed_decode::<BinaryOp>(&mut payload, execution, call_span)?;
                    let left = indexed_raw(&mut payload, call_span)?;
                    let right = indexed_raw(&mut payload, call_span)?;
                    let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                    indexed_finish(payload, call_span)?;
                    if op == BinaryOp::And || op == BinaryOp::Or {
                        match self.eval_indexed_expr(execution, instruction, slots, call_span)? {
                            ControlFlow::Continue(value) => values.push(value),
                            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                        }
                    } else {
                        work.push(BinaryWork::Apply { op, span });
                        work.push(BinaryWork::Expr(right));
                        work.push(BinaryWork::Expr(left));
                    }
                }
            }
        }
        let value = values.pop().ok_or_else(|| {
            RuntimeError::new("indexed-ir", "binary expression produced no value").with_span(span)
        })?;
        if !values.is_empty() {
            return Err(RuntimeError::new(
                "indexed-ir",
                "binary expression left extra values on its work stack",
            )
            .with_span(span));
        }
        Ok(ControlFlow::Continue(value))
    }

    fn eval_indexed_stmts(
        &mut self,
        execution: &FullExecution<'_>,
        mut statements: FullPayload<'_>,
        header: &FunctionHeader,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<StmtFlow, RuntimeError> {
        let len = indexed_raw(&mut statements, call_span)? as usize;
        let mut defers = Vec::new();
        for _ in 0..len {
            let statement = indexed_raw(&mut statements, call_span)?;
            let (tag, mut payload) = indexed_value(execution.instruction_id(statement), call_span)?;
            if tag == FullTag::StmtDefer {
                let value = indexed_raw(&mut payload, call_span)?;
                indexed_finish(payload, call_span)?;
                defers.push(value);
                continue;
            }
            let flow = match self.eval_indexed_stmt(execution, statement, header, slots, call_span)
            {
                Ok(flow) => flow,
                Err(error) => {
                    if !error.abort.as_ref().is_some_and(|signal| signal.force) {
                        let _ = self.run_indexed_defers(execution, &defers, slots, call_span);
                    }
                    return Err(error);
                }
            };
            match flow {
                StmtFlow::None => {}
                flow @ (StmtFlow::Return(_)
                | StmtFlow::Propagate(_)
                | StmtFlow::Break(_)
                | StmtFlow::Continue) => {
                    self.run_indexed_defers(execution, &defers, slots, call_span)?;
                    return Ok(flow);
                }
            }
        }
        indexed_finish(statements, call_span)?;
        self.run_indexed_defers(execution, &defers, slots, call_span)?;
        Ok(StmtFlow::None)
    }

    pub(in crate::runtime::eval) fn eval_indexed_body_as_signal_hook(
        &mut self,
        view: crate::runtime::eval::indexed::full::FullDriverStepView<'_>,
        body: u32,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<Flow, RuntimeError> {
        let execution = view
            .execution()
            .map_err(|error| indexed_error(error, call_span))?;
        let header = Self::indexed_block_header(slots.len());
        let flow =
            self.eval_indexed_statement_block(&execution, body, &header, slots, call_span)?;
        match flow {
            StmtFlow::None => Ok(Flow::Continue(Value::Unit)),
            StmtFlow::Return(value) => Ok(Flow::Continue(value.into_value())),
            StmtFlow::Propagate(value) => {
                let error = match value {
                    LoweredValue::Error(error) => *error,
                    LoweredValue::ResultErr(error) => *error,
                    other => Value::Error(Box::new(
                        RuntimeError::new(
                            "signal-hook",
                            format!("propagated {}", other.type_name()),
                        )
                        .with_span(call_span),
                    )),
                };
                let kind = error.error_kind().unwrap_or("error").to_string();
                let message = error
                    .error_message()
                    .unwrap_or("signal hook error")
                    .to_string();
                let traceback = self.pending_traceback.take().unwrap_or_else(|| Traceback {
                    failing_span: Some(call_span),
                    exe_path: self.exe_path_for_traceback(),
                    operation_kind: "signal.hook".to_string(),
                    error: TraceError { kind, message },
                    frames: self.call_stack.clone(),
                });
                Ok(Flow::Propagate(Propagation { error, traceback }))
            }
            StmtFlow::Break(_) | StmtFlow::Continue => Ok(Flow::Continue(Value::Unit)),
        }
    }

    fn run_indexed_defers(
        &mut self,
        execution: &FullExecution<'_>,
        defers: &[u32],
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<(), RuntimeError> {
        for value in defers.iter().rev().copied() {
            if matches!(
                self.eval_indexed_expr(execution, value, slots, call_span)?,
                ControlFlow::Break(_)
            ) {
                return Err(RuntimeError::new(
                    "defer-control-flow",
                    "deferred expression produced invalid control flow",
                )
                .with_span(call_span));
            }
        }
        Ok(())
    }

    fn eval_indexed_statement_block(
        &mut self,
        execution: &FullExecution<'_>,
        block: u32,
        header: &FunctionHeader,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<StmtFlow, RuntimeError> {
        let (_, statements) = execution
            .block_id(block, BLOCK_STATEMENTS)
            .map_err(|error| indexed_error(error, call_span))?;
        self.eval_indexed_stmts(execution, statements, header, slots, call_span)
    }

    fn eval_indexed_optional_statement_block(
        &mut self,
        execution: &FullExecution<'_>,
        payload: &mut FullPayload<'_>,
        header: &FunctionHeader,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<Option<StmtFlow>, RuntimeError> {
        let Some(block) = indexed_optional_raw(payload, call_span)? else {
            return Ok(None);
        };
        self.eval_indexed_statement_block(execution, block, header, slots, call_span)
            .map(Some)
    }

    fn eval_indexed_stmt(
        &mut self,
        execution: &FullExecution<'_>,
        instruction: u32,
        header: &FunctionHeader,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<StmtFlow, RuntimeError> {
        let (tag, mut payload) = indexed_value(execution.instruction_id(instruction), call_span)?;
        match tag {
            FullTag::StmtLet => {
                let slot = indexed_decode::<usize>(&mut payload, execution, call_span)?;
                let value = indexed_raw(&mut payload, call_span)?;
                indexed_finish(payload, call_span)?;
                match self.eval_indexed_expr(execution, value, slots, call_span)? {
                    ControlFlow::Continue(value) => slots[slot] = value,
                    ControlFlow::Break(value) => return Ok(StmtFlow::Return(value)),
                }
                Ok(StmtFlow::None)
            }
            FullTag::StmtGuard => {
                let slot = indexed_decode::<usize>(&mut payload, execution, call_span)?;
                let value = indexed_raw(&mut payload, call_span)?;
                let else_param_slot =
                    indexed_decode::<Option<usize>>(&mut payload, execution, call_span)?;
                let else_body = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let value = match self.eval_indexed_expr(execution, value, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(StmtFlow::Return(value)),
                };
                match value {
                    LoweredValue::ResultOk(value) => {
                        slots[slot] = *value;
                        Ok(StmtFlow::None)
                    }
                    LoweredValue::ResultErr(error) => {
                        if let Some(slot) = else_param_slot {
                            slots[slot] = LoweredValue::Error(error);
                        }
                        match self.eval_indexed_statement_block(
                            execution, else_body, header, slots, span,
                        )? {
                            StmtFlow::None => {
                                Err(RuntimeError::new("guard", "guard else block must diverge")
                                    .with_span(span))
                            }
                            flow => Ok(flow),
                        }
                    }
                    other => Err(RuntimeError::new(
                        "type-error",
                        format!("guard expected Result, found {}", other.type_name()),
                    )
                    .with_span(span)),
                }
            }
            FullTag::StmtLetRecord => {
                let source = indexed_raw(&mut payload, call_span)?;
                let (_, mut fields) = execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, call_span))?;
                let field_count = indexed_raw(&mut fields, call_span)? as usize;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let source = match self.eval_indexed_expr(execution, source, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => {
                        return Ok(StmtFlow::Return(value));
                    }
                };
                for _ in 0..field_count {
                    let name = indexed_decode::<Name>(&mut fields, execution, span)?;
                    let slot = indexed_decode::<usize>(&mut fields, execution, span)?;
                    let Some(value) = lowered_record_field_value(&source, &name.as_str()) else {
                        return Err(RuntimeError::new(
                            "field-access",
                            format!("record has no field `{}`", name.as_str()),
                        )
                        .with_span(span));
                    };
                    slots[slot] = value;
                }
                indexed_finish(fields, span)?;
                Ok(StmtFlow::None)
            }
            FullTag::StmtLetInt => {
                let slot = indexed_decode::<usize>(&mut payload, execution, call_span)?;
                let value = indexed_raw(&mut payload, call_span)?;
                indexed_finish(payload, call_span)?;
                match self.eval_indexed_typed_int(execution, value, slots, call_span)? {
                    ControlFlow::Continue(value) => slots[slot] = LoweredValue::Int(value),
                    ControlFlow::Break(value) => return Ok(StmtFlow::Return(value)),
                }
                Ok(StmtFlow::None)
            }
            FullTag::StmtLetBool => {
                let slot = indexed_decode::<usize>(&mut payload, execution, call_span)?;
                let value = indexed_raw(&mut payload, call_span)?;
                indexed_finish(payload, call_span)?;
                match self.eval_indexed_typed_bool(execution, value, slots, call_span)? {
                    ControlFlow::Continue(value) => slots[slot] = LoweredValue::Bool(value),
                    ControlFlow::Break(value) => return Ok(StmtFlow::Return(value)),
                }
                Ok(StmtFlow::None)
            }
            FullTag::StmtAssign => {
                let slot = indexed_decode::<usize>(&mut payload, execution, call_span)?;
                let op = indexed_decode::<AssignOp>(&mut payload, execution, call_span)?;
                let value = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let value = match self.eval_indexed_expr(execution, value, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(StmtFlow::Return(value)),
                };
                slots[slot] = lowered_assign_value(op, slots[slot].clone(), value, span)?;
                Ok(StmtFlow::None)
            }
            FullTag::StmtAssignField | FullTag::StmtAssignFieldInt => {
                let typed = tag == FullTag::StmtAssignFieldInt;
                let slot = indexed_decode::<usize>(&mut payload, execution, call_span)?;
                let field = indexed_decode::<Arc<str>>(&mut payload, execution, call_span)?;
                let op = indexed_decode::<AssignOp>(&mut payload, execution, call_span)?;
                let value = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let value = if typed {
                    match self.eval_indexed_typed_int(execution, value, slots, call_span)? {
                        ControlFlow::Continue(value) => LoweredValue::Int(value),
                        ControlFlow::Break(value) => {
                            return Ok(StmtFlow::Return(value));
                        }
                    }
                } else {
                    match self.eval_indexed_expr(execution, value, slots, call_span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => {
                            return Ok(StmtFlow::Return(value));
                        }
                    }
                };
                if matches!(
                    slots[slot],
                    LoweredValue::Stats { .. } | LoweredValue::StatsBlob(_)
                ) {
                    let stats = std::mem::replace(&mut slots[slot], LoweredValue::Unit);
                    slots[slot] = LoweredValue::RecordVec(match stats {
                        LoweredValue::Stats {
                            blanks,
                            code,
                            comments,
                        } => lowered_inline_stats_to_record_vec(blanks, code, comments),
                        LoweredValue::StatsBlob(stats) => stats.to_record_vec(),
                        _ => unreachable!("checked indexed stats assignment target"),
                    });
                }
                let current = match &mut slots[slot] {
                    LoweredValue::Record(record) => record.get(field.as_ref()).cloned(),
                    LoweredValue::RecordVec(record) => {
                        lowered_record_vec_get(record, field.as_ref()).cloned()
                    }
                    _ => {
                        return Err(RuntimeError::new(
                            "type-error",
                            "lowered expression expected Record",
                        )
                        .with_span(span));
                    }
                }
                .ok_or_else(|| {
                    RuntimeError::new("missing-field", field.to_string()).with_span(span)
                })?;
                let value = lowered_assign_value(op, current, value, span)?;
                match &mut slots[slot] {
                    LoweredValue::Record(record) => {
                        record.insert(field.clone(), value);
                    }
                    LoweredValue::RecordVec(record) => {
                        lowered_record_vec_insert(record, Name::intern(field.as_ref()), value);
                    }
                    _ => unreachable!("checked indexed record assignment target"),
                }
                Ok(StmtFlow::None)
            }
            FullTag::StmtAssignIndex => {
                let slot = indexed_decode::<usize>(&mut payload, execution, call_span)?;
                let index = indexed_raw(&mut payload, call_span)?;
                let op = indexed_decode::<AssignOp>(&mut payload, execution, call_span)?;
                let value = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let key = match self.eval_indexed_expr(execution, index, slots, call_span)? {
                    ControlFlow::Continue(value) => {
                        lowered_str_arg(&value, "indexed assignment", span)?.to_string()
                    }
                    ControlFlow::Break(value) => {
                        return Ok(StmtFlow::Return(value));
                    }
                };
                let value = match self.eval_indexed_expr(execution, value, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(StmtFlow::Return(value)),
                };
                let LoweredValue::Map(map) = &mut slots[slot] else {
                    return Err(RuntimeError::new(
                        "type-error",
                        "indexed assignment requires a map value",
                    )
                    .with_span(span));
                };
                if op == AssignOp::Set {
                    map.insert(key, value);
                    return Ok(StmtFlow::None);
                }
                let current = map.get(key.as_str()).cloned().ok_or_else(|| {
                    RuntimeError::new("missing-field", key.clone()).with_span(span)
                })?;
                map.insert(key, lowered_assign_value(op, current, value, span)?);
                Ok(StmtFlow::None)
            }
            FullTag::StmtAssignInt => {
                let slot = indexed_decode::<usize>(&mut payload, execution, call_span)?;
                let op = indexed_decode::<AssignOp>(&mut payload, execution, call_span)?;
                let value = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let value = match self.eval_indexed_typed_int(execution, value, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(StmtFlow::Return(value)),
                };
                if op == AssignOp::Set {
                    slots[slot] = LoweredValue::Int(value);
                    return Ok(StmtFlow::None);
                }
                let LoweredValue::Int(current) = slots[slot] else {
                    return Err(
                        RuntimeError::new("type-error", "lowered expression expected Int")
                            .with_span(span),
                    );
                };
                slots[slot] = LoweredValue::Int(match op {
                    AssignOp::Add => current + value,
                    AssignOp::Sub => current - value,
                    AssignOp::Mul => current * value,
                    AssignOp::Div => current / value,
                    AssignOp::Rem => current % value,
                    AssignOp::Set => unreachable!(),
                });
                Ok(StmtFlow::None)
            }
            FullTag::StmtAssignBool => {
                let slot = indexed_decode::<usize>(&mut payload, execution, call_span)?;
                let value = indexed_raw(&mut payload, call_span)?;
                indexed_finish(payload, call_span)?;
                match self.eval_indexed_typed_bool(execution, value, slots, call_span)? {
                    ControlFlow::Continue(value) => slots[slot] = LoweredValue::Bool(value),
                    ControlFlow::Break(value) => return Ok(StmtFlow::Return(value)),
                }
                Ok(StmtFlow::None)
            }
            FullTag::StmtExpr => {
                let value = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                match self.eval_indexed_expr(execution, value, slots, span)? {
                    ControlFlow::Continue(value @ LoweredValue::ResultErr(_)) => {
                        let value = self.lowered_question_propagation_value(value, span)?;
                        Ok(StmtFlow::Propagate(value))
                    }
                    ControlFlow::Continue(_) => Ok(StmtFlow::None),
                    ControlFlow::Break(value) => Ok(StmtFlow::Propagate(value)),
                }
            }
            FullTag::StmtIf | FullTag::StmtIfBool => {
                let typed = tag == FullTag::StmtIfBool;
                let (_, mut branches) = execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, call_span))?;
                let len = indexed_raw(&mut branches, call_span)? as usize;
                for _ in 0..len {
                    let condition = indexed_raw(&mut branches, call_span)?;
                    let body = indexed_raw(&mut branches, call_span)?;
                    let condition = if typed {
                        match self
                            .eval_indexed_typed_bool(execution, condition, slots, call_span)?
                        {
                            ControlFlow::Continue(value) => value,
                            ControlFlow::Break(value) => {
                                return Ok(StmtFlow::Return(value));
                            }
                        }
                    } else {
                        match self.eval_indexed_bool(execution, condition, slots, call_span)? {
                            ControlFlow::Continue(value) => value,
                            ControlFlow::Break(value) => {
                                return Ok(StmtFlow::Return(value));
                            }
                        }
                    };
                    if condition {
                        let _ = indexed_optional_raw(&mut payload, call_span)?;
                        indexed_finish(payload, call_span)?;
                        return self.eval_indexed_statement_block(
                            execution, body, header, slots, call_span,
                        );
                    }
                }
                indexed_finish(branches, call_span)?;
                let flow = self.eval_indexed_optional_statement_block(
                    execution,
                    &mut payload,
                    header,
                    slots,
                    call_span,
                )?;
                indexed_finish(payload, call_span)?;
                Ok(flow.unwrap_or(StmtFlow::None))
            }
            FullTag::StmtWhile | FullTag::StmtWhileBool => {
                let typed = tag == FullTag::StmtWhileBool;
                let condition = indexed_raw(&mut payload, call_span)?;
                let body = indexed_raw(&mut payload, call_span)?;
                indexed_finish(payload, call_span)?;
                loop {
                    self.service_pending_signal(call_span)?;
                    if self.signal_state.shutdown_complete {
                        return Ok(StmtFlow::None);
                    }
                    let condition = if typed {
                        match self
                            .eval_indexed_typed_bool(execution, condition, slots, call_span)?
                        {
                            ControlFlow::Continue(value) => value,
                            ControlFlow::Break(value) => {
                                return Ok(StmtFlow::Return(value));
                            }
                        }
                    } else {
                        match self.eval_indexed_bool(execution, condition, slots, call_span)? {
                            ControlFlow::Continue(value) => value,
                            ControlFlow::Break(value) => {
                                return Ok(StmtFlow::Return(value));
                            }
                        }
                    };
                    if !condition {
                        break;
                    }
                    match self
                        .eval_indexed_statement_block(execution, body, header, slots, call_span)?
                    {
                        StmtFlow::None | StmtFlow::Continue => {}
                        StmtFlow::Break(_) => break,
                        StmtFlow::Return(value) => {
                            return Ok(StmtFlow::Return(value));
                        }
                        StmtFlow::Propagate(value) => {
                            return Ok(StmtFlow::Propagate(value));
                        }
                    }
                }
                Ok(StmtFlow::None)
            }
            FullTag::StmtMatch => {
                let value = indexed_raw(&mut payload, call_span)?;
                let (_, mut arms) = execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, call_span))?;
                let arm_count = indexed_raw(&mut arms, call_span)? as usize;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let mut decoded_arms = Vec::with_capacity(arm_count);
                for _ in 0..arm_count {
                    decoded_arms.push((
                        indexed_raw(&mut arms, span)?,
                        indexed_optional_raw(&mut arms, span)?,
                        indexed_raw(&mut arms, span)?,
                    ));
                }
                indexed_finish(arms, span)?;
                let value = match self.eval_indexed_expr(execution, value, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(StmtFlow::Return(value)),
                };
                for (pattern, guard, body) in decoded_arms {
                    if Self::indexed_pattern_matches(execution, pattern, &value, slots, span)? {
                        if let Some(guard) = guard {
                            match self.eval_indexed_bool(execution, guard, slots, call_span)? {
                                ControlFlow::Continue(true) => {}
                                ControlFlow::Continue(false) => continue,
                                ControlFlow::Break(value) => {
                                    return Ok(StmtFlow::Return(value));
                                }
                            }
                        }
                        return self.eval_indexed_statement_block(
                            execution, body, header, slots, call_span,
                        );
                    }
                }
                Err(lowered_match_no_arm(span))
            }
            FullTag::StmtStrMatch | FullTag::StmtTagMatch => {
                let value = indexed_raw(&mut payload, call_span)?;
                let arm_count = indexed_raw(&mut payload, call_span)? as usize;
                let mut arms = Vec::with_capacity(arm_count);
                for _ in 0..arm_count {
                    arms.push((
                        indexed_decode::<Arc<str>>(&mut payload, execution, call_span)?,
                        indexed_raw(&mut payload, call_span)?,
                    ));
                }
                let fallback = indexed_optional_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let value = match self.eval_indexed_expr(execution, value, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(StmtFlow::Return(value)),
                };
                let key = if tag == FullTag::StmtStrMatch {
                    lowered_str_key(&value)
                } else {
                    lowered_tag_key(&value)
                };
                if let Some(key) = key
                    && let Some((_, body)) =
                        arms.iter().find(|(candidate, _)| candidate.as_ref() == key)
                {
                    return self
                        .eval_indexed_statement_block(execution, *body, header, slots, call_span);
                }
                if let Some(body) = fallback {
                    return self
                        .eval_indexed_statement_block(execution, body, header, slots, call_span);
                }
                Err(lowered_match_no_arm(span))
            }
            FullTag::StmtFor => {
                let slot = indexed_decode::<usize>(&mut payload, execution, call_span)?;
                let iter = indexed_raw(&mut payload, call_span)?;
                let body = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let iter = match self.eval_indexed_expr(execution, iter, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(StmtFlow::Return(value)),
                };
                let items = self.lowered_list_items(iter, span, "lowered for expected List")?;
                for item in items {
                    self.service_pending_signal(span)?;
                    if self.signal_state.shutdown_complete {
                        return Ok(StmtFlow::None);
                    }
                    slots[slot] = item;
                    match self
                        .eval_indexed_statement_block(execution, body, header, slots, call_span)?
                    {
                        StmtFlow::None | StmtFlow::Continue => {}
                        StmtFlow::Break(_) => break,
                        StmtFlow::Return(value) => {
                            return Ok(StmtFlow::Return(value));
                        }
                        StmtFlow::Propagate(value) => {
                            return Ok(StmtFlow::Propagate(value));
                        }
                    }
                }
                Ok(StmtFlow::None)
            }
            FullTag::StmtForRecord => {
                let (_, mut fields) = execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, call_span))?;
                let field_count = indexed_raw(&mut fields, call_span)? as usize;
                let mut bindings = Vec::with_capacity(field_count);
                for _ in 0..field_count {
                    bindings.push((
                        indexed_decode::<Name>(&mut fields, execution, call_span)?,
                        indexed_decode::<usize>(&mut fields, execution, call_span)?,
                    ));
                }
                indexed_finish(fields, call_span)?;
                let iter = indexed_raw(&mut payload, call_span)?;
                let body = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let iter = match self.eval_indexed_expr(execution, iter, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(StmtFlow::Return(value)),
                };
                let items = self.lowered_list_items(iter, span, "lowered for expected List")?;
                for item in items {
                    self.service_pending_signal(span)?;
                    if self.signal_state.shutdown_complete {
                        return Ok(StmtFlow::None);
                    }
                    for (name, slot) in &bindings {
                        let Some(value) = lowered_record_field_value(&item, &name.as_str()) else {
                            return Err(RuntimeError::new(
                                "field-access",
                                format!("record has no field `{}`", name.as_str()),
                            )
                            .with_span(span));
                        };
                        slots[*slot] = value;
                    }
                    match self
                        .eval_indexed_statement_block(execution, body, header, slots, call_span)?
                    {
                        StmtFlow::None | StmtFlow::Continue => {}
                        StmtFlow::Break(_) => break,
                        flow @ (StmtFlow::Return(_) | StmtFlow::Propagate(_)) => return Ok(flow),
                    }
                }
                Ok(StmtFlow::None)
            }
            FullTag::StmtForStrLines => {
                let slot = indexed_decode::<usize>(&mut payload, execution, call_span)?;
                let text = indexed_raw(&mut payload, call_span)?;
                let body = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let text = match self.eval_indexed_expr(execution, text, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(StmtFlow::Return(value)),
                };
                if let Some((bytes, start, end)) = lowered_bytes_parts(&text) {
                    let mut cursor = start;
                    let mut line_count = 0u32;
                    while cursor < end {
                        let newline = memchr::memchr(b'\n', &bytes[cursor..end])
                            .map(|offset| cursor + offset);
                        let line_end = newline.unwrap_or(end);
                        let view_end = if line_end > cursor && bytes[line_end - 1] == b'\r' {
                            line_end - 1
                        } else {
                            line_end
                        };
                        line_count = line_count.wrapping_add(1);
                        if line_count & 63 == 0 {
                            self.service_pending_signal(span)?;
                            if self.signal_state.shutdown_complete {
                                return Ok(StmtFlow::None);
                            }
                        }
                        assign_lowered_bytes_view(&mut slots[slot], &bytes, cursor, view_end);
                        match self.eval_indexed_statement_block(
                            execution, body, header, slots, call_span,
                        )? {
                            StmtFlow::None | StmtFlow::Continue => {}
                            StmtFlow::Break(_) => break,
                            flow @ (StmtFlow::Return(_) | StmtFlow::Propagate(_)) => {
                                return Ok(flow);
                            }
                        }
                        let Some(newline) = newline else {
                            break;
                        };
                        cursor = newline + 1;
                    }
                    return Ok(StmtFlow::None);
                }
                let Some((text, start, end)) = lowered_str_parts(&text) else {
                    return Err(RuntimeError::new(
                        "type-error",
                        "lowered for lines expected Str or Bytes",
                    )
                    .with_span(span));
                };
                let bytes = text.as_bytes();
                let mut cursor = start;
                let mut line_count = 0u32;
                while cursor < end {
                    let newline =
                        memchr::memchr(b'\n', &bytes[cursor..end]).map(|offset| cursor + offset);
                    let line_end = newline.unwrap_or(end);
                    let view_end = if line_end > cursor && bytes[line_end - 1] == b'\r' {
                        line_end - 1
                    } else {
                        line_end
                    };
                    line_count = line_count.wrapping_add(1);
                    if line_count & 63 == 0 {
                        self.service_pending_signal(span)?;
                        if self.signal_state.shutdown_complete {
                            return Ok(StmtFlow::None);
                        }
                    }
                    assign_lowered_str_view(&mut slots[slot], &text, cursor, view_end);
                    match self
                        .eval_indexed_statement_block(execution, body, header, slots, call_span)?
                    {
                        StmtFlow::None | StmtFlow::Continue => {}
                        StmtFlow::Break(_) => break,
                        flow @ (StmtFlow::Return(_) | StmtFlow::Propagate(_)) => return Ok(flow),
                    }
                    let Some(newline) = newline else {
                        break;
                    };
                    cursor = newline + 1;
                }
                Ok(StmtFlow::None)
            }
            FullTag::StmtScanLines => {
                let text_slot = indexed_decode::<usize>(&mut payload, execution, call_span)?;
                let line_slot = indexed_decode::<usize>(&mut payload, execution, call_span)?;
                let checks = indexed_decode::<Vec<ScanCheck>>(&mut payload, execution, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let (text, start, end, bytes_mode) = if let Some((text, start, end)) =
                    lowered_str_parts(&slots[text_slot])
                {
                    (Arc::<[u8]>::from(text.as_bytes()), start, end, false)
                } else if let Some((bytes, start, end)) = lowered_bytes_parts(&slots[text_slot]) {
                    (bytes, start, end, true)
                } else {
                    return Err(
                        RuntimeError::new("type-error", "ScanLines expected Str or Bytes")
                            .with_span(span),
                    );
                };
                let mut cursor = start;
                let mut line_count = 0u32;
                while cursor < end {
                    let newline =
                        memchr::memchr(b'\n', &text[cursor..end]).map(|offset| cursor + offset);
                    let line_end = newline.unwrap_or(end);
                    let view_end = if line_end > cursor && text[line_end - 1] == b'\r' {
                        line_end - 1
                    } else {
                        line_end
                    };
                    line_count = line_count.wrapping_add(1);
                    if line_count & 63 == 0 {
                        self.service_pending_signal(span)?;
                        if self.signal_state.shutdown_complete {
                            return Ok(StmtFlow::None);
                        }
                    }
                    if bytes_mode {
                        assign_lowered_bytes_view(&mut slots[line_slot], &text, cursor, view_end);
                    } else {
                        let line = std::str::from_utf8(&text[cursor..view_end])
                            .expect("source string slice remains UTF-8");
                        slots[line_slot] = LoweredValue::Str(Arc::from(line));
                    }
                    for check in &checks {
                        let matches = match &check.condition {
                            ScanCondition::TrimEmpty => {
                                lowered_trim_is_empty_value(&slots[line_slot], span)?
                            }
                            ScanCondition::TrimStartsWith(needle) => {
                                lowered_trim_str_predicate_value(
                                    &slots[line_slot],
                                    LoweredStrPredicate::StartsWith,
                                    needle.as_slice(),
                                    span,
                                )?
                            }
                            ScanCondition::StartsWith(needle) => lowered_str_predicate_text(
                                &slots[line_slot],
                                LoweredStrPredicate::StartsWith,
                                needle.as_slice(),
                                span,
                            )?,
                        };
                        if matches {
                            if let LoweredValue::Int(ref mut value) = slots[check.counter_slot] {
                                *value += 1;
                            }
                            break;
                        }
                    }
                    let Some(newline) = newline else {
                        break;
                    };
                    cursor = newline + 1;
                }
                slots[line_slot] = LoweredValue::Unit;
                Ok(StmtFlow::None)
            }
            FullTag::StmtScanBytes => {
                let config = indexed_decode::<ScanBytes>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let Some((line, start, end)) = lowered_bytes_parts(&slots[config.line_slot]) else {
                    return Err(RuntimeError::new("type-error", "ScanBytes expected Bytes")
                        .with_span(config.span));
                };
                let bytes = &line[start..end];
                let mut block_depth = match slots[config.block_depth_slot] {
                    LoweredValue::Int(value) => value,
                    _ => {
                        return Err(RuntimeError::new(
                            "type-error",
                            "ScanBytes block depth expected Int",
                        )
                        .with_span(config.span));
                    }
                };
                let mut index = 0usize;
                let mut code_seen = false;
                let mut comment_seen = false;
                let mut in_string = false;
                let mut string_delim = -1i64;
                let mut escaped = false;
                while index < bytes.len() {
                    if index & 4095 == 0 {
                        self.service_pending_signal(config.span)?;
                        if self.signal_state.shutdown_complete {
                            return Ok(StmtFlow::None);
                        }
                    }
                    let byte = i64::from(bytes[index]);
                    let next_byte = bytes.get(index + 1).copied().map(i64::from).unwrap_or(-1);
                    if block_depth > 0 {
                        comment_seen = true;
                        if config.nested && byte == 47 && next_byte == 42 {
                            block_depth += 1;
                            index += 2;
                        } else if byte == 42 && next_byte == 47 {
                            block_depth -= 1;
                            index += 2;
                        } else {
                            index += 1;
                        }
                    } else if in_string {
                        code_seen = true;
                        if escaped {
                            escaped = false;
                        } else if byte == 92 {
                            escaped = true;
                        } else if byte == string_delim {
                            in_string = false;
                        }
                        index += 1;
                    } else if byte == 34 || byte == 39 || byte == 96 {
                        code_seen = true;
                        in_string = true;
                        string_delim = byte;
                        index += 1;
                    } else if byte == 47 && next_byte == 47 {
                        comment_seen = true;
                        index = bytes.len();
                    } else if byte == 47 && next_byte == 42 {
                        comment_seen = true;
                        block_depth = 1;
                        index += 2;
                    } else {
                        if byte != 32 && byte != 9 {
                            code_seen = true;
                        }
                        index += 1;
                    }
                }
                slots[config.block_depth_slot] = LoweredValue::Int(block_depth);
                slots[config.code_seen_slot] = LoweredValue::Bool(code_seen);
                slots[config.comment_seen_slot] = LoweredValue::Bool(comment_seen);
                slots[config.in_string_slot] = LoweredValue::Bool(in_string);
                slots[config.string_delim_slot] = LoweredValue::Int(string_delim);
                slots[config.escaped_slot] = LoweredValue::Bool(escaped);
                Ok(StmtFlow::None)
            }
            FullTag::StmtCd => {
                let target = indexed_raw(&mut payload, call_span)?;
                let body = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let target = match self.eval_indexed_expr(execution, target, slots, call_span)? {
                    ControlFlow::Continue(value) => lowered_path_like_arg(value, "cd", span)?,
                    ControlFlow::Break(value) => {
                        return Ok(StmtFlow::Propagate(value));
                    }
                };
                let previous = self.cwd.clone();
                let next = self.host_path(&target);
                match cap_std::fs::Dir::open_ambient_dir(&next, cap_std::ambient_authority()) {
                    Ok(_) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotADirectory => {
                        return Ok(StmtFlow::Propagate(LoweredValue::ResultErr(Box::new(
                            Value::Error(Box::new(
                                RuntimeError::new(
                                    "cwd-not-directory",
                                    "cwd target is not a directory",
                                )
                                .with_span(span),
                            )),
                        ))));
                    }
                    Err(error) => {
                        return Ok(StmtFlow::Propagate(LoweredValue::ResultErr(Box::new(
                            Value::Error(Box::new(
                                RuntimeError::new("cwd", error.to_string()).with_span(span),
                            )),
                        ))));
                    }
                }
                self.trace_enter(
                    TraceKind::CwdEnter,
                    Some(span),
                    Some("cd"),
                    TracePayload::Cwd {
                        previous: TraceArg::bytes(path_bytes(&previous)),
                        current: TraceArg::bytes(path_bytes(&next)),
                    },
                );
                self.cwd = next;
                let result =
                    self.eval_indexed_statement_block(execution, body, header, slots, call_span);
                let current = self.cwd.clone();
                self.cwd = previous.clone();
                self.trace_exit(
                    TraceKind::CwdExit,
                    Some(span),
                    Some("cd"),
                    TracePayload::Cwd {
                        previous: TraceArg::bytes(path_bytes(&current)),
                        current: TraceArg::bytes(path_bytes(&previous)),
                    },
                );
                result
            }
            FullTag::StmtEnv => {
                let env = Self::decode_indexed_run_env(&mut payload, execution, call_span)?;
                let body = indexed_raw(&mut payload, call_span)?;
                indexed_finish(payload, call_span)?;
                for assignment in &env {
                    check_env_name(&assignment.name.as_str(), assignment.value.span)?;
                }
                let overlay = match self.eval_indexed_run_env(execution, &env, slots, call_span)? {
                    ControlFlow::Continue(overlay) => overlay,
                    ControlFlow::Break(value) => {
                        return Ok(StmtFlow::Propagate(value));
                    }
                };
                let previous = self.env.clone();
                self.env.extend(overlay);
                let result =
                    self.eval_indexed_statement_block(execution, body, header, slots, call_span);
                self.env = previous;
                result
            }
            FullTag::StmtProc => {
                let op = indexed_decode::<RuntimeOp>(&mut payload, execution, call_span)?;
                let (_, mut args) = execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, call_span))?;
                let len = indexed_raw(&mut args, call_span)? as usize;
                let propagate_result = indexed_decode::<bool>(&mut payload, execution, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let mut values = Vec::with_capacity(len);
                for _ in 0..len {
                    let arg = indexed_raw(&mut args, span)?;
                    let value = match self.eval_indexed_expr(execution, arg, slots, call_span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => {
                            return Ok(StmtFlow::Return(value));
                        }
                    };
                    values.push(value);
                }
                indexed_finish(args, span)?;
                let (positionals, flags) = lowered_parse_command_values(values, span)?;
                let result = match op {
                    RuntimeOp::FsWrite => {
                        if positionals.len() != 2 {
                            return Err(RuntimeError::new(
                                "arity",
                                "fs.write expected path and data",
                            )
                            .with_span(span));
                        }
                        let data = lowered_bytes_or_str_owned(
                            positionals.last().cloned().expect("checked length"),
                            "fs.write",
                            span,
                        )?;
                        let path = lowered_path_arg(
                            positionals.first().cloned().expect("checked length"),
                            "fs.write",
                            span,
                        )?;
                        lowered_unit_result(fs_module::write_path(
                            self.host_path(&path),
                            &data,
                            span,
                        ))
                    }
                    RuntimeOp::FsMkdir => {
                        let parents = flags.get("parents").copied().unwrap_or(true);
                        let path = lowered_path_arg(
                            positionals.first().cloned().ok_or_else(|| {
                                RuntimeError::new("arity", "fs.mkdir expected path").with_span(span)
                            })?,
                            "fs.mkdir",
                            span,
                        )?;
                        lowered_unit_result(fs_module::mkdir_path(
                            self.host_path(&path),
                            parents,
                            None,
                            span,
                        ))
                    }
                    RuntimeOp::FsRemove => {
                        let missing_ok = flags.get("missing_ok").copied().unwrap_or(false);
                        let path = lowered_path_arg(
                            positionals.first().cloned().ok_or_else(|| {
                                RuntimeError::new("arity", "fs.remove expected path")
                                    .with_span(span)
                            })?,
                            "fs.remove",
                            span,
                        )?;
                        lowered_unit_result(fs_module::remove_path(
                            self.host_path(&path),
                            missing_ok,
                            span,
                        ))
                    }
                    RuntimeOp::JsonWrite => {
                        if positionals.len() != 2 {
                            return Err(RuntimeError::new(
                                "arity",
                                "json.write expected path and value",
                            )
                            .with_span(span));
                        }
                        let pretty = flags.get("pretty").copied().unwrap_or(false);
                        let value = positionals
                            .last()
                            .cloned()
                            .expect("checked length")
                            .into_value();
                        let path = lowered_path_arg(
                            positionals.first().cloned().expect("checked length"),
                            "json.write",
                            span,
                        )?;
                        match json_module::encode_json(&value, pretty, span) {
                            Ok(text) => lowered_unit_result(fs_module::write_path(
                                self.host_path(&path),
                                text.as_bytes(),
                                span,
                            )),
                            Err(error) => lowered_result_err_value(error),
                        }
                    }
                    _ => {
                        let name = api_spec().op_trace_name(op).unwrap_or("unknown");
                        return Err(RuntimeError::new(
                            "unsupported-proc-command",
                            format!(
                                "proc command syntax for {name} is not yet supported in compact lowering"
                            ),
                        )
                        .with_span(span));
                    }
                };
                if propagate_result {
                    match result {
                        LoweredValue::ResultOk(_) => Ok(StmtFlow::None),
                        LoweredValue::ResultErr(error) => {
                            let kind = error.error_kind().unwrap_or("error").to_string();
                            let message = error
                                .error_message()
                                .unwrap_or("propagated error")
                                .to_string();
                            self.trace_leaf(
                                TraceKind::ResultPropagate,
                                Some(span),
                                None,
                                TracePayload::ResultPropagate {
                                    error_kind: kind.clone(),
                                },
                            );
                            let _traceback =
                                self.pending_traceback.take().unwrap_or_else(|| Traceback {
                                    failing_span: Some(span),
                                    exe_path: self.exe_path.clone(),
                                    operation_kind: "result.propagate".to_string(),
                                    error: TraceError { kind, message },
                                    frames: self.call_stack.clone(),
                                });
                            Ok(StmtFlow::Propagate(LoweredValue::ResultErr(error)))
                        }
                        other => Err(RuntimeError::new(
                            "type-error",
                            format!("`?` expected Result, found {}", other.type_name()),
                        )
                        .with_span(span)),
                    }
                } else {
                    Ok(StmtFlow::None)
                }
            }
            FullTag::StmtPrint => {
                let (_, mut args) = execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, call_span))?;
                let len = indexed_raw(&mut args, call_span)? as usize;
                let stderr = indexed_decode::<bool>(&mut payload, execution, call_span)?;
                let flush = indexed_decode::<bool>(&mut payload, execution, call_span)?;
                let propagate_result = indexed_decode::<bool>(&mut payload, execution, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let mut line = String::new();
                let mut argv = Vec::with_capacity(len);
                for index in 0..len {
                    let arg = indexed_raw(&mut args, span)?;
                    let value = match self.eval_indexed_expr(execution, arg, slots, call_span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => {
                            return Ok(StmtFlow::Return(value));
                        }
                    };
                    if index > 0 {
                        line.push(' ');
                    }
                    let start = line.len();
                    push_lowered_display(&mut line, &value, span)?;
                    if self.trace_enabled {
                        argv.push(TraceArg::text(&line[start..]));
                    }
                }
                indexed_finish(args, span)?;
                let trace_name = if stderr { "eprint" } else { "print" };
                self.trace_enter(
                    TraceKind::CoreCall,
                    Some(span),
                    Some(trace_name),
                    TracePayload::Core { argv },
                );
                if stderr && flush {
                    self.flush_stderr_line(&line);
                } else if stderr {
                    self.write_stderr_line(&line);
                } else if flush {
                    self.flush_stdout_line(&line);
                } else {
                    self.write_stdout_line(&line);
                }
                self.trace_exit(
                    TraceKind::CoreResult,
                    Some(span),
                    Some(trace_name),
                    TracePayload::None,
                );
                if propagate_result {
                    match self.last_status.as_ref().and_then(|status| status.code) {
                        Some(0) | None => Ok(StmtFlow::None),
                        Some(code) => Ok(StmtFlow::Propagate(LoweredValue::Int(i64::from(code)))),
                    }
                } else {
                    Ok(StmtFlow::None)
                }
            }
            FullTag::StmtRun => {
                let value = indexed_raw(&mut payload, call_span)?;
                let propagate_result = indexed_decode::<bool>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                match self.eval_indexed_expr(execution, value, slots, call_span)? {
                    ControlFlow::Continue(value) => {
                        if propagate_result {
                            match value {
                                LoweredValue::ResultOk(_) => Ok(StmtFlow::None),
                                value @ LoweredValue::ResultErr(_) => {
                                    let value =
                                        self.lowered_question_propagation_value(value, call_span)?;
                                    Ok(StmtFlow::Propagate(value))
                                }
                                other => Err(RuntimeError::new(
                                    "type-error",
                                    format!("`?` expected Result, found {}", other.type_name()),
                                )
                                .with_span(call_span)),
                            }
                        } else {
                            Ok(StmtFlow::None)
                        }
                    }
                    ControlFlow::Break(value) => Ok(StmtFlow::Propagate(value)),
                }
            }
            FullTag::StmtLoop => {
                let body = indexed_raw(&mut payload, call_span)?;
                indexed_finish(payload, call_span)?;
                loop {
                    match self
                        .eval_indexed_statement_block(execution, body, header, slots, call_span)?
                    {
                        StmtFlow::None | StmtFlow::Continue => {}
                        StmtFlow::Break(_) => break,
                        StmtFlow::Return(value) => {
                            return Ok(StmtFlow::Return(value));
                        }
                        StmtFlow::Propagate(value) => {
                            return Ok(StmtFlow::Propagate(value));
                        }
                    }
                }
                Ok(StmtFlow::None)
            }
            FullTag::StmtReturn => {
                let value = indexed_raw(&mut payload, call_span)?;
                indexed_finish(payload, call_span)?;
                let value = match self.eval_indexed_expr(execution, value, slots, call_span)? {
                    ControlFlow::Continue(value) | ControlFlow::Break(value) => value,
                };
                Ok(StmtFlow::Return(value))
            }
            FullTag::StmtYield => {
                let value = indexed_raw(&mut payload, call_span)?;
                indexed_finish(payload, call_span)?;
                if !matches!(
                    header.return_kind,
                    LoweredReturnKind::Plain(LoweredType::Stream)
                ) {
                    return Err(
                        RuntimeError::new("control-flow", "yield outside stream producer")
                            .with_span(call_span),
                    );
                }
                let value = match self.eval_indexed_expr(execution, value, slots, call_span)? {
                    ControlFlow::Continue(value) | ControlFlow::Break(value) => value,
                };
                self.stream_items.push(value.into_value());
                Ok(StmtFlow::None)
            }
            FullTag::StmtBreak => {
                indexed_finish(payload, call_span)?;
                Ok(StmtFlow::Break(None))
            }
            FullTag::StmtBreakValue => {
                let value = indexed_raw(&mut payload, call_span)?;
                indexed_finish(payload, call_span)?;
                match self.eval_indexed_expr(execution, value, slots, call_span)? {
                    ControlFlow::Continue(value) => Ok(StmtFlow::Break(Some(value))),
                    ControlFlow::Break(value) => Ok(StmtFlow::Propagate(value)),
                }
            }
            FullTag::StmtContinue => {
                indexed_finish(payload, call_span)?;
                Ok(StmtFlow::Continue)
            }
            _ => Err(RuntimeError::new(
                "indexed-ir",
                format!("direct indexed evaluator does not support {tag:?}"),
            )
            .with_span(call_span)),
        }
    }

    fn indexed_field_value(
        &mut self,
        base: LoweredValue,
        name: &str,
        span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        if let Some(value) = self.indexed_borrowed_field_value(&base, name, span)? {
            return Ok(value);
        }
        match base {
            LoweredValue::Error(value) => {
                let (kind, message) = match value.as_ref() {
                    Value::Error(error) => (error.kind.clone(), error.message.clone()),
                    Value::RunError(error) => (error.kind.clone(), error.message.clone()),
                    _ => {
                        return Err(
                            RuntimeError::new("type-error", "field access expected Error")
                                .with_span(span),
                        );
                    }
                };
                match name {
                    "kind" => Ok(LoweredValue::Str(kind.into())),
                    "message" => Ok(LoweredValue::Str(message.into())),
                    _ => Err(RuntimeError::new("missing-field", name).with_span(span)),
                }
            }
            LoweredValue::Regex(regex) => match name {
                "pattern" => Ok(LoweredValue::Str(regex.pattern.clone().into())),
                _ => Err(RuntimeError::new("missing-field", name).with_span(span)),
            },
            LoweredValue::Status(status) => match name {
                "ok" | "success" => Ok(LoweredValue::Bool(status.success)),
                "kind" => Ok(LoweredValue::Str(
                    format!("{:?}", status.kind).to_lowercase().into(),
                )),
                "segments" => Ok(LoweredValue::List(
                    status
                        .segments
                        .iter()
                        .map(lowered_status_segment_record)
                        .collect(),
                )),
                _ => Err(RuntimeError::new("missing-field", name).with_span(span)),
            },
            LoweredValue::ProcessHandle(handle) => match name {
                "pid" => Ok(LoweredValue::Int(handle.pid)),
                "command" => Ok(LoweredValue::Str(handle.command.clone())),
                "argv" => Ok(LoweredValue::List(
                    handle.argv.iter().cloned().map(LoweredValue::Str).collect(),
                )),
                "detached" => Ok(LoweredValue::Bool(handle.detached)),
                _ => Err(RuntimeError::new("missing-field", name).with_span(span)),
            },
            LoweredValue::Path(path) => lowered_path_method_value(path, name, Vec::new(), span),
            _ => Err(RuntimeError::new("missing-field", name).with_span(span)),
        }
    }

    fn indexed_borrowed_field_value(
        &mut self,
        base: &LoweredValue,
        name: &str,
        span: Span,
    ) -> Result<Option<LoweredValue>, RuntimeError> {
        match base {
            LoweredValue::Record(record) | LoweredValue::Module(record) => record
                .get(name)
                .cloned()
                .map(Some)
                .ok_or_else(|| RuntimeError::new("missing-field", name).with_span(span)),
            LoweredValue::RecordVec(record) => lowered_record_vec_get(record.as_slice(), name)
                .cloned()
                .map(Some)
                .ok_or_else(|| RuntimeError::new("missing-field", name).with_span(span)),
            LoweredValue::Stats {
                blanks,
                code,
                comments,
            } => lowered_inline_stats_field_value(*blanks, *code, *comments, name)
                .map(Some)
                .ok_or_else(|| RuntimeError::new("missing-field", name).with_span(span)),
            LoweredValue::StatsBlob(stats) => lowered_stats_field_value(stats, name)
                .map(Some)
                .ok_or_else(|| RuntimeError::new("missing-field", name).with_span(span)),
            LoweredValue::FsEntry(entry) => {
                let value = entry
                    .field_value(name)
                    .ok_or_else(|| RuntimeError::new("missing-field", name).with_span(span))?
                    .map_err(|error| error.with_span(span))?;
                lowered_value_from_runtime_any(&value)
                    .map(Some)
                    .ok_or_else(|| {
                        RuntimeError::new(
                            "type-error",
                            format!("fs entry field produced unsupported {}", value.type_name()),
                        )
                        .with_span(span)
                    })
            }
            _ => Ok(None),
        }
    }

    fn eval_indexed_typed_int(
        &mut self,
        execution: &FullExecution<'_>,
        instruction: u32,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<ControlFlow<LoweredValue, i64>, RuntimeError> {
        let (tag, mut payload) = indexed_value(execution.instruction_id(instruction), call_span)?;
        let value = match tag {
            FullTag::IntInt => {
                let value = indexed_decode::<i64>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                value
            }
            FullTag::IntSlot => {
                let slot = indexed_decode::<usize>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let LoweredValue::Int(value) = slots[slot] else {
                    return Err(
                        RuntimeError::new("type-error", "lowered expression expected Int")
                            .with_span(call_span),
                    );
                };
                value
            }
            FullTag::IntBinary => {
                let op = indexed_decode::<BinaryOp>(&mut payload, execution, call_span)?;
                let left = indexed_raw(&mut payload, call_span)?;
                let right = indexed_raw(&mut payload, call_span)?;
                indexed_finish(payload, call_span)?;
                let left = match self.eval_indexed_typed_int(execution, left, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let right = match self.eval_indexed_typed_int(execution, right, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                match op {
                    BinaryOp::Add => left + right,
                    BinaryOp::Sub => left - right,
                    BinaryOp::Mul => left * right,
                    BinaryOp::Div if right != 0 => left / right,
                    BinaryOp::Rem if right != 0 => left % right,
                    BinaryOp::Div | BinaryOp::Rem => {
                        return Err(RuntimeError::new("division-by-zero", "division by zero")
                            .with_span(call_span));
                    }
                    _ => unreachable!("verified typed int operation"),
                }
            }
            FullTag::IntStrByteLenSlot => {
                let slot = indexed_decode::<usize>(&mut payload, execution, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                lowered_str_byte_len_value(&slots[slot], span)?
            }
            FullTag::IntStrCountLinesSlot => {
                let slot = indexed_decode::<usize>(&mut payload, execution, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                lowered_str_count_lines_value(&slots[slot], span)?
            }
            FullTag::IntStrByteAtSlot => {
                let slot = indexed_decode::<usize>(&mut payload, execution, call_span)?;
                let index = indexed_raw(&mut payload, call_span)?;
                let default = indexed_optional_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let index = match self.eval_indexed_typed_int(execution, index, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let default = match default {
                    Some(default) => {
                        match self.eval_indexed_typed_int(execution, default, slots, call_span)? {
                            ControlFlow::Continue(value) => value,
                            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                        }
                    }
                    None => -1,
                };
                lowered_str_byte_at_value(&slots[slot], index, default, span)?
            }
            _ => {
                return Err(RuntimeError::new(
                    "indexed-ir",
                    format!("direct indexed int evaluator does not support {tag:?}"),
                )
                .with_span(call_span));
            }
        };
        Ok(ControlFlow::Continue(value))
    }

    fn eval_indexed_typed_bool(
        &mut self,
        execution: &FullExecution<'_>,
        instruction: u32,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<ControlFlow<LoweredValue, bool>, RuntimeError> {
        let (tag, mut payload) = indexed_value(execution.instruction_id(instruction), call_span)?;
        let value = match tag {
            FullTag::BoolBool => {
                let value = indexed_decode::<bool>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                value
            }
            FullTag::BoolSlot => {
                let slot = indexed_decode::<usize>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                match &slots[slot] {
                    LoweredValue::Bool(value) => *value,
                    LoweredValue::Status(status) => status.success,
                    _ => {
                        return Err(RuntimeError::new(
                            "type-error",
                            "lowered expression expected Bool",
                        )
                        .with_span(call_span));
                    }
                }
            }
            FullTag::BoolNot => {
                let value = indexed_raw(&mut payload, call_span)?;
                indexed_finish(payload, call_span)?;
                match self.eval_indexed_typed_bool(execution, value, slots, call_span)? {
                    ControlFlow::Continue(value) => !value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                }
            }
            FullTag::BoolAnd | FullTag::BoolOr => {
                let left = indexed_raw(&mut payload, call_span)?;
                let right = indexed_raw(&mut payload, call_span)?;
                indexed_finish(payload, call_span)?;
                let left = match self.eval_indexed_typed_bool(execution, left, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                if tag == FullTag::BoolAnd && !left {
                    return Ok(ControlFlow::Continue(false));
                }
                if tag == FullTag::BoolOr && left {
                    return Ok(ControlFlow::Continue(true));
                }
                return self.eval_indexed_typed_bool(execution, right, slots, call_span);
            }
            FullTag::BoolIntCompare => {
                let op = indexed_decode::<BinaryOp>(&mut payload, execution, call_span)?;
                let left = indexed_raw(&mut payload, call_span)?;
                let right = indexed_raw(&mut payload, call_span)?;
                indexed_finish(payload, call_span)?;
                let left = match self.eval_indexed_typed_int(execution, left, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let right = match self.eval_indexed_typed_int(execution, right, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                match op {
                    BinaryOp::Eq => left == right,
                    BinaryOp::Ne => left != right,
                    BinaryOp::Lt => left < right,
                    BinaryOp::Le => left <= right,
                    BinaryOp::Gt => left > right,
                    BinaryOp::Ge => left >= right,
                    _ => unreachable!("verified typed comparison"),
                }
            }
            FullTag::BoolStrPredicateSlot => {
                let slot = indexed_decode::<usize>(&mut payload, execution, call_span)?;
                let predicate =
                    indexed_decode::<LoweredStrPredicate>(&mut payload, execution, call_span)?;
                let needle = indexed_decode::<Arc<[u8]>>(&mut payload, execution, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                lowered_str_predicate_text(&slots[slot], predicate, &needle, span)?
            }
            FullTag::BoolContainsSlot => {
                let slot = indexed_decode::<usize>(&mut payload, execution, call_span)?;
                let needle = indexed_decode::<LoweredValue>(&mut payload, execution, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                lowered_contains_value(&slots[slot], &needle, span)?
            }
            FullTag::BoolStrContainsSlot => {
                let slot = indexed_decode::<usize>(&mut payload, execution, call_span)?;
                let needle = indexed_decode::<Arc<str>>(&mut payload, execution, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                if let Some(text) = lowered_str_value(&slots[slot]) {
                    bytes_contains(text.as_bytes(), needle.as_bytes())
                } else {
                    lowered_contains_value(&slots[slot], &LoweredValue::Str(needle), span)?
                }
            }
            FullTag::BoolTrimEmptySlot => {
                let slot = indexed_decode::<usize>(&mut payload, execution, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                lowered_trim_is_empty_value(&slots[slot], span)?
            }
            FullTag::BoolTrimStrPredicateSlot => {
                let slot = indexed_decode::<usize>(&mut payload, execution, call_span)?;
                let predicate =
                    indexed_decode::<LoweredStrPredicate>(&mut payload, execution, call_span)?;
                let needle = indexed_decode::<Arc<[u8]>>(&mut payload, execution, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                lowered_trim_str_predicate_value(&slots[slot], predicate, &needle, span)?
            }
            FullTag::BoolLiteralCompareSlot => {
                let op = indexed_decode::<BinaryOp>(&mut payload, execution, call_span)?;
                let slot = indexed_decode::<usize>(&mut payload, execution, call_span)?;
                let value = indexed_decode::<LoweredValue>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let equal = slots[slot] == value;
                match op {
                    BinaryOp::Eq => equal,
                    BinaryOp::Ne => !equal,
                    _ => unreachable!("verified literal comparison"),
                }
            }
            _ => {
                return Err(RuntimeError::new(
                    "indexed-ir",
                    format!("direct indexed bool evaluator does not support {tag:?}"),
                )
                .with_span(call_span));
            }
        };
        Ok(ControlFlow::Continue(value))
    }

    fn eval_indexed_bool(
        &mut self,
        execution: &FullExecution<'_>,
        instruction: u32,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<ControlFlow<LoweredValue, bool>, RuntimeError> {
        match self.eval_indexed_expr(execution, instruction, slots, call_span)? {
            ControlFlow::Break(value) => Ok(ControlFlow::Break(value)),
            ControlFlow::Continue(LoweredValue::Bool(value)) => Ok(ControlFlow::Continue(value)),
            ControlFlow::Continue(LoweredValue::Status(status)) => {
                Ok(ControlFlow::Continue(status.success))
            }
            ControlFlow::Continue(_) => Err(RuntimeError::new(
                "type-error",
                "lowered expression expected Bool",
            )
            .with_span(call_span)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceMap;
    use crate::syntax::parser::Parser;

    #[test]
    fn direct_indexed_function_executes_without_decoding_its_body() {
        crate::runtime::eval::run_eval(
            direct_indexed_function_executes_without_decoding_its_body_inner,
        );
    }

    fn direct_indexed_function_executes_without_decoding_its_body_inner() {
        let source = r#"
pure double(value: Int) -> Int {
  return value * 2
}

pure countdown(n: Int) -> Int {
  if n <= 0 {
    return 0
  }
  return 1 + countdown(n - 1)
}

pure direct_limit(n: Int) -> Int {
  let base: Int = 2
  if n > base {
    return double(n)
  }
  return base
}

pure pipeline(values: List[Int]) -> List[Int] {
  return values
    |> where . > 1
    |> map . * 2
    |> sort
}
"#;
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("direct-indexed.xsh", source);
        let parsed = Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
        assert!(
            evaluator
                .prepare_compact_indexed_only(&parsed.arena, source_id)
                .is_some()
        );
        let (direct_limit, countdown, pipeline) = parsed.arena.symbol_owner().with_current(|| {
            (
                Name::intern("direct_limit"),
                Name::intern("countdown"),
                Name::intern("pipeline"),
            )
        });

        let result = evaluator
            .call_indexed_direct(
                LoweredFunctionKey::Name(direct_limit),
                LoweredFunctionKind::Pure,
                &[Value::Int(4)],
                Span::new(source_id, 0, 0),
            )
            .expect("function uses only direct indexed opcodes")
            .unwrap();

        assert_eq!(result, Value::Int(8));
        let recursive = evaluator
            .call_indexed_direct(
                LoweredFunctionKey::Name(countdown),
                LoweredFunctionKind::Pure,
                &[Value::Int(4)],
                Span::new(source_id, 0, 0),
            )
            .expect("self-recursive function uses only direct indexed opcodes")
            .unwrap();
        assert_eq!(recursive, Value::Int(4));
        let piped = evaluator
            .call_indexed_direct(
                LoweredFunctionKey::Name(pipeline),
                LoweredFunctionKind::Pure,
                &[Value::List(vec![
                    Value::Int(3),
                    Value::Int(1),
                    Value::Int(2),
                ])],
                Span::new(source_id, 0, 0),
            )
            .expect("collection pipeline uses only direct indexed opcodes")
            .unwrap();
        assert_eq!(piped, Value::List(vec![Value::Int(4), Value::Int(6)]));
    }
}
