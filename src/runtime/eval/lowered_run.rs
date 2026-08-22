//! The lowered-IR evaluator: the `eval_lowered_*` methods, split out of
//! `eval.rs` as a separate `impl Evaluator` block. Registry/bridge methods
//! (`refresh_lowered_pures`, `call_lowered_pure`) stay in the parent.

use crate::modules::{
    RuntimeOp, api_spec, archive as archive_module, bytes as bytes_module, cli as cli_module,
    diff as diff_module, dns as dns_module, elf as elf_module, fs as fs_module,
    group as group_module, hash as hash_module, ini as ini_module, json as json_module,
    linux as linux_module, mime as mime_module, net as net_module, patch as patch_module,
    process as process_module, regex as regex_module, shlex, system, time as time_module,
    tui::{self, Sequence},
    unix as unix_module, user as user_module,
};
use crate::runtime::process::{
    CancellationPolicy, ChildWaitOutcome, FileRedirectionMode, ManagedStdio, ProcessEnd,
    ProcessInvocation, ProcessRedirection, ProcessSegmentStatus, ProcessStatus, RedirectionStream,
    SpawnManagedOptions, SpawnOptions, WAIT_POLL, cancel_managed, path_bytes, poll_managed,
    resolve_executable, run_capture_with_stderr_policy, run_inherit_with_policy,
    run_pipeline_inherit_with_policy, run_quiet_with_policy, spawn_command, spawn_managed,
};
use crate::runtime::run::execute_run_with_policy;
use crate::runtime::value::{
    CommandPlan, CommandRedirection, CommandRedirectionMode, CommandRedirectionStream,
    DurationValue, FunctionName, LiveStream, PathValue, ProcessHandleValue, RecordMap, RegexValue,
    RunError, RuntimeError, StreamValue, Value, error_constructor, structured_error_constructor,
};
use crate::sema::types::{CallableType, ModuleExportType, Type};
use crate::source::{SourceId, Span};
use crate::symbol::QualifiedName;
use crate::syntax::arena::ArenaProgram;
use crate::syntax::node::{
    AssignOp, BinaryOp, FormatSpec, FormatSpecKind, RedirectionKind, RunKind,
};
use crate::trace::{
    TraceArg, TraceError, TraceKind, TracePayload, Traceback, TracebackFrame, TracebackFrameKind,
};
use directories::{ProjectDirs, UserDirs};
use rustc_hash::FxHashMap;
use std::cell::Cell;
use std::collections::BTreeMap;
#[cfg(target_os = "macos")]
use std::ffi::CStr;
use std::ffi::OsString;
use std::fmt::Write as _;
use std::io::{ErrorKind, Read, Write};
use std::ops::ControlFlow;
use std::os::fd::{AsFd, AsRawFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use xsh_root::Root;

mod indexed_run;

#[cfg(feature = "native-tests")]
use super::display_value;
use super::lower::{
    lowered_match_no_arm, lowered_record_field, lowered_stmt_flow_to_flow, lowered_str_key,
    lowered_sum_records, lowered_sum_values, lowered_tag_key,
};
use super::lowered_ops::{
    checked_int_binary, compare_lowered_sort_keys, lowered_assign_value, lowered_binary_value,
    lowered_bytes_arg, lowered_bytes_parts, lowered_bytes_value, lowered_contains_value,
    lowered_index_value, lowered_method_value, lowered_nonnegative_count,
    lowered_path_method_value, lowered_return_value, lowered_slice_value,
    lowered_sort_key_orderable, lowered_str_arg, lowered_str_byte_at_value,
    lowered_str_byte_len_value, lowered_str_count_lines_value, lowered_str_parts,
    lowered_str_predicate_text, lowered_str_predicate_value, lowered_str_value,
    lowered_trim_is_empty_value, lowered_trim_str_predicate_value, lowered_type_name,
    lowered_value_from_runtime, lowered_value_from_runtime_any, lowered_value_matches,
    push_lowered_display,
};
use super::modules::{
    auth as auth_module, display_spawn_argv, intercept_test_host_call, record_int_field,
    record_path, record_str, run_error_to_runtime, utils_cache_key, validate_module_contract,
};
#[cfg(feature = "native-tests")]
use super::modules::{
    test_contains_value, test_error_kind, test_failure, test_mock_expected_return_type,
    test_temp_path, test_value_matches_type,
};
use super::{
    Binding, DynamicFunction, Evaluator, Flow, FsRootHandle, FunctionHeader, LoweredCompTarget,
    LoweredFunctionKey, LoweredFunctionKind, LoweredModuleExportKind, LoweredReturnKind,
    LoweredStrPredicate, LoweredTagValue, LoweredType, LoweredValue, Name, ReduceByOp,
    ScanCondition, StmtFlow, assign_lowered_bytes_view, assign_lowered_str_view, bytes_contains,
    check_env_name, compound_assignment_value, exit_status, lowered_inline_stats_field_value,
    lowered_inline_stats_to_record_vec, lowered_record_vec_get, lowered_record_vec_get_mut,
    lowered_record_vec_insert, lowered_record_vec_or_stats, lowered_stats_field_value,
    lowered_str_view_value, lowered_value_matches_static_type, module_error, module_io_error,
    path_absolute_value, path_value_from_pathbuf, pathbuf_from_path_value,
    runtime_error_from_value, splice_to_argv, trace_env_overlay, trace_status,
    value_matches_static_type, value_to_argv_bytes,
};
#[cfg(feature = "native-tests")]
use super::{NativeTestRunKind, NativeTestRunRequest, TestMock};
const LOWERED_SHARED_LIST_THRESHOLD: usize = 16;
const INDEXED_EVAL_DEPTH_LIMIT: usize = 2048;
const INDEXED_SMALL_STACK_EVAL_DEPTH_LIMIT: usize = 128;

fn indexed_eval_depth_limit() -> usize {
    if cfg!(debug_assertions) && std::env::var_os("XSH_TEST_SMALL_EVAL_STACK").is_some() {
        INDEXED_SMALL_STACK_EVAL_DEPTH_LIMIT
    } else {
        INDEXED_EVAL_DEPTH_LIMIT
    }
}

#[cfg(feature = "native-tests")]
impl Evaluator {
    fn lowered_test_run_script(
        &mut self,
        ctx: &RecordMap,
        source: &str,
        args: &[String],
        env: &BTreeMap<String, String>,
        stdin: &[u8],
        name: &str,
        span: Span,
    ) -> Result<BTreeMap<Arc<str>, LoweredValue>, RuntimeError> {
        self.lowered_test_run_xsh(ctx, source, &[], args, env, stdin, name, span)
    }

    fn lowered_test_run_xsh(
        &mut self,
        ctx: &RecordMap,
        source: &str,
        xsh_args: &[String],
        script_args: &[String],
        env: &BTreeMap<String, String>,
        stdin: &[u8],
        name: &str,
        span: Span,
    ) -> Result<BTreeMap<Arc<str>, LoweredValue>, RuntimeError> {
        self.lowered_native_test_run(
            NativeTestRunKind::Xsh,
            ctx,
            source,
            xsh_args,
            script_args,
            env,
            stdin,
            name,
            span,
            "test-run-xsh",
        )
    }

    fn lowered_test_run_xsht_trace(
        &mut self,
        ctx: &RecordMap,
        source: &str,
        trace_args: &[String],
        script_args: &[String],
        env: &BTreeMap<String, String>,
        stdin: &[u8],
        name: &str,
        span: Span,
    ) -> Result<BTreeMap<Arc<str>, LoweredValue>, RuntimeError> {
        self.lowered_native_test_run(
            NativeTestRunKind::XshtTrace,
            ctx,
            source,
            trace_args,
            script_args,
            env,
            stdin,
            name,
            span,
            "test-run-xsht-trace",
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn lowered_native_test_run(
        &mut self,
        kind: NativeTestRunKind,
        ctx: &RecordMap,
        source: &str,
        tool_args: &[String],
        script_args: &[String],
        env: &BTreeMap<String, String>,
        stdin: &[u8],
        name: &str,
        span: Span,
        error_kind: &str,
    ) -> Result<BTreeMap<Arc<str>, LoweredValue>, RuntimeError> {
        let script_name = if name.is_empty() { "script.xsh" } else { name };
        let script_path = test_temp_path(self, ctx, script_name, span)?;
        let Some(host) = self.native_test_host.clone() else {
            return Err(
                RuntimeError::new(error_kind, "native test host is not installed").with_span(span),
            );
        };
        let value = host(NativeTestRunRequest {
            kind,
            script_path,
            source: source.to_string(),
            tool_args: tool_args.to_vec(),
            script_args: script_args.to_vec(),
            env: env.clone(),
            stdin: stdin.to_vec(),
            span,
        })?;
        match lowered_runtime_value(value, span)? {
            LoweredValue::Record(record) => Ok(record),
            other => Err(RuntimeError::new(
                error_kind,
                format!(
                    "native test host returned {}, expected Record",
                    other.type_name()
                ),
            )
            .with_span(span)),
        }
    }
}

thread_local! {
    static INDEXED_EVAL_DEPTH: Cell<usize> = const { Cell::new(0) };
    static INDEXED_EXPLICIT_FRAMES: Cell<bool> = const { Cell::new(false) };
}

pub(super) fn indexed_explicit_frames_active() -> bool {
    INDEXED_EXPLICIT_FRAMES.with(Cell::get)
}

pub(super) fn indexed_recursive_fast_path_allowed(return_kind: LoweredReturnKind) -> bool {
    // Result propagation adds wrapper and unwind work around a call. Keep it
    // on the heap-backed frame path even when shallow plain calls may recurse.
    if matches!(return_kind, LoweredReturnKind::Result(_)) {
        return false;
    }
    if cfg!(debug_assertions) {
        return false;
    }
    !indexed_explicit_frames_active()
        && INDEXED_EVAL_DEPTH.with(|depth| depth.get() < (indexed_eval_depth_limit() / 16).max(1))
}

pub(super) fn with_indexed_explicit_frames<R>(f: impl FnOnce() -> R) -> R {
    INDEXED_EXPLICIT_FRAMES.with(|active| {
        let previous = active.replace(true);
        let result = f();
        active.set(previous);
        result
    })
}

struct EvalDepthReset<'a> {
    depth: &'a Cell<usize>,
    previous: usize,
}

impl Drop for EvalDepthReset<'_> {
    fn drop(&mut self) {
        self.depth.set(self.previous);
    }
}

fn with_indexed_eval_depth<R>(
    span: Span,
    f: impl FnOnce() -> Result<R, RuntimeError>,
) -> Result<R, RuntimeError> {
    INDEXED_EVAL_DEPTH.with(|depth| {
        let current = depth.get();
        if current >= indexed_eval_depth_limit() {
            return Err(RuntimeError::new(
                "compact.stack-depth",
                "indexed evaluation exceeded the stack-depth limit",
            )
            .with_span(span));
        }
        depth.set(current + 1);
        let _reset = EvalDepthReset {
            depth,
            previous: current,
        };
        f()
    })
}

fn btree_map<K: Ord, V>(entries: Vec<(K, V)>) -> BTreeMap<K, V> {
    let mut map = BTreeMap::new();
    map.extend(entries);
    map
}

/// Display a fmt-string interpolation, applying an optional `:>N`/`:<N`/`:0N`
/// width/alignment format spec (matches the deleted evaluator).
fn push_lowered_fmt_value(
    text: &mut String,
    value: &LoweredValue,
    span: Span,
    spec: Option<&FormatSpec>,
) -> Result<(), RuntimeError> {
    let Some(spec) = spec else {
        return push_lowered_display(text, value, span);
    };
    let mut rendered = String::new();
    push_lowered_display(&mut rendered, value, span)?;
    let width = spec.width;
    let len = rendered.chars().count();
    if len >= width {
        text.push_str(&rendered);
        return Ok(());
    }
    let pad = width - len;
    match spec.kind {
        FormatSpecKind::RightAlign => {
            text.extend(std::iter::repeat_n(' ', pad));
            text.push_str(&rendered);
        }
        FormatSpecKind::LeftAlign => {
            text.push_str(&rendered);
            text.extend(std::iter::repeat_n(' ', pad));
        }
        FormatSpecKind::ZeroPad => {
            // Zero-pad after a leading sign so `-7:04` renders `-007`.
            if let Some(rest) = rendered.strip_prefix('-') {
                text.push('-');
                text.extend(std::iter::repeat_n('0', pad));
                text.push_str(rest);
            } else {
                text.extend(std::iter::repeat_n('0', pad));
                text.push_str(&rendered);
            }
        }
    }
    Ok(())
}

/// The number of items entering a pipeline stage, for the `item_count` field of
/// a `stream.stage` trace event. `None` for non-collection inputs (adapters,
/// scalars), matching the old evaluator which only reported it for streams.
fn lowered_pipeline_item_count(value: &LoweredValue) -> Option<usize> {
    match value {
        LoweredValue::List(items) => Some(items.len()),
        LoweredValue::SharedList(items) => Some(items.len()),
        LoweredValue::Map(entries) => Some(entries.len()),
        _ => None,
    }
}

fn lowered_record_field_value(value: &LoweredValue, field: &str) -> Option<LoweredValue> {
    match value {
        LoweredValue::Stats {
            blanks,
            code,
            comments,
        } => lowered_inline_stats_field_value(*blanks, *code, *comments, field),
        LoweredValue::StatsBlob(stats) => lowered_stats_field_value(stats, field),
        _ => lowered_record_field(value, field).cloned(),
    }
}

fn lowered_freeze_large_slot_list(slot: &mut LoweredValue) {
    let LoweredValue::List(items) = slot else {
        return;
    };
    if items.len() < LOWERED_SHARED_LIST_THRESHOLD {
        return;
    }
    let items = std::mem::take(items);
    *slot = LoweredValue::SharedList(Arc::new(items));
}

fn lowered_reduce_fields_owned(
    output: LoweredValue,
    key_field: &str,
    value_field: &str,
    span: Span,
) -> Result<(LoweredValue, LoweredValue), RuntimeError> {
    match output {
        LoweredValue::Record(mut entries) => {
            let key = entries.remove(key_field).ok_or_else(|| {
                RuntimeError::new("reduce-by-key", "reduce-by record is missing field `key`")
                    .with_span(span)
            })?;
            let value = entries.remove(value_field).ok_or_else(|| {
                RuntimeError::new(
                    "reduce-by-value",
                    "reduce-by record is missing field `value`",
                )
                .with_span(span)
            })?;
            Ok((key, value))
        }
        LoweredValue::RecordVec(mut entries) => {
            let key_index =
                lowered_record_vec_field_index(&entries, key_field).ok_or_else(|| {
                    RuntimeError::new("reduce-by-key", "reduce-by record is missing field `key`")
                        .with_span(span)
                })?;
            let value_index =
                lowered_record_vec_field_index(&entries, value_field).ok_or_else(|| {
                    RuntimeError::new(
                        "reduce-by-value",
                        "reduce-by record is missing field `value`",
                    )
                    .with_span(span)
                })?;
            if key_index == value_index {
                return Err(RuntimeError::new(
                    "reduce-by-value",
                    "reduce-by record key and value fields overlap",
                )
                .with_span(span));
            }
            let value = entries.swap_remove(value_index).1;
            let key_index = if value_index < key_index {
                key_index - 1
            } else {
                key_index
            };
            let key = entries.swap_remove(key_index).1;
            Ok((key, value))
        }
        _ => Err(
            RuntimeError::new("reduce-by-value", "reduce-by block must return a record")
                .with_span(span),
        ),
    }
}

fn lowered_reduce_key_value_owned(key: LoweredValue, span: Span) -> Result<String, RuntimeError> {
    match key {
        LoweredValue::Str(value) => Ok(value.to_string()),
        LoweredValue::StrView(value) => Ok(value.as_str().to_string()),
        LoweredValue::Int(value) => Ok(value.to_string()),
        LoweredValue::Bool(value) => Ok(value.to_string()),
        other => Err(RuntimeError::new(
            "type-error",
            format!(
                "reduce-by key must be Str, Int, or Bool, found {}",
                other.type_name()
            ),
        )
        .with_span(span)),
    }
}

fn lowered_reduce_key_value(key: &LoweredValue, span: Span) -> Result<String, RuntimeError> {
    match key {
        LoweredValue::Str(value) => Ok(value.to_string()),
        LoweredValue::StrView(value) => Ok(value.as_str().to_string()),
        LoweredValue::Int(value) => Ok(value.to_string()),
        LoweredValue::Bool(value) => Ok(value.to_string()),
        other => Err(RuntimeError::new(
            "type-error",
            format!(
                "reduce-by key must be Str, Int, or Bool, found {}",
                other.type_name()
            ),
        )
        .with_span(span)),
    }
}

/// Combine the per-key accumulator with the next item's `value` using the
/// `reduce-by` reducer (`--sum`/`--min`/`--max`).
fn lowered_reduce_combine(
    op: ReduceByOp,
    acc: LoweredValue,
    value: LoweredValue,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    match op {
        ReduceByOp::Sum => lowered_reduce_sum(acc, value, span),
        ReduceByOp::Min => Ok(
            if compare_lowered_sort_keys(&value, &acc) == std::cmp::Ordering::Less {
                value
            } else {
                acc
            },
        ),
        ReduceByOp::Max => Ok(
            if compare_lowered_sort_keys(&value, &acc) == std::cmp::Ordering::Greater {
                value
            } else {
                acc
            },
        ),
    }
}

fn lowered_reduce_sum(
    acc: LoweredValue,
    value: LoweredValue,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    match (acc, value) {
        (LoweredValue::Int(a), LoweredValue::Int(b)) => Ok(LoweredValue::Int(a + b)),
        (LoweredValue::Float(a), LoweredValue::Float(b)) => Ok(LoweredValue::Float(
            crate::runtime::value::FloatValue::new(a.0 + b.0),
        )),
        (acc @ LoweredValue::Record(_), value @ LoweredValue::Record(_))
        | (acc @ LoweredValue::RecordVec(_), value @ LoweredValue::RecordVec(_))
        | (acc @ LoweredValue::Record(_), value @ LoweredValue::RecordVec(_))
        | (acc @ LoweredValue::RecordVec(_), value @ LoweredValue::Record(_)) => {
            Ok(lowered_sum_records(acc, value))
        }
        (acc, value) => Err(RuntimeError::new(
            "type-error",
            format!(
                "reduce-by --sum cannot add {} and {}",
                acc.type_name(),
                value.type_name()
            ),
        )
        .with_span(span)),
    }
}

fn lowered_reduce_group_insert(
    groups: &mut BTreeMap<String, LoweredValue>,
    key: String,
    value: LoweredValue,
    op: ReduceByOp,
    span: Span,
) -> Result<(), RuntimeError> {
    match groups.entry(key) {
        std::collections::btree_map::Entry::Vacant(slot) => {
            slot.insert(value);
        }
        std::collections::btree_map::Entry::Occupied(mut slot) => {
            let prev = std::mem::replace(slot.get_mut(), LoweredValue::Unit);
            *slot.get_mut() = lowered_reduce_combine(op, prev, value, span)?;
        }
    }
    Ok(())
}

struct LoweredReduceProjection<'a> {
    key_field: &'a str,
    value_fields: Vec<(Name, &'a str)>,
}

struct LoweredProjectedReduceState<'a> {
    projection: LoweredReduceProjection<'a>,
    record_indices: Option<LoweredProjectedRecordIndices>,
    output_fields_unique: bool,
}

#[derive(Clone)]
struct LoweredProjectedRecordIndices {
    key: usize,
    values: Vec<usize>,
}

impl<'a> LoweredProjectedReduceState<'a> {
    fn new(projection: LoweredReduceProjection<'a>) -> Self {
        let output_fields_unique = lowered_projected_output_fields_unique(&projection);
        Self {
            projection,
            record_indices: None,
            output_fields_unique,
        }
    }

    fn record_indices_for(&mut self, item: &LoweredValue) -> Option<LoweredProjectedRecordIndices> {
        let LoweredValue::RecordVec(record) = item else {
            return None;
        };
        if let Some(indices) = &self.record_indices
            && lowered_projected_record_indices_match(&self.projection, record, indices)
        {
            return Some(indices.clone());
        }
        self.record_indices = lowered_projected_record_indices(&self.projection, record);
        self.record_indices.clone()
    }
}

fn lowered_projected_output_fields_unique(projection: &LoweredReduceProjection<'_>) -> bool {
    for index in 0..projection.value_fields.len() {
        let name = projection.value_fields[index].0;
        if projection.value_fields[..index]
            .iter()
            .any(|(previous, _)| *previous == name)
        {
            return false;
        }
    }
    true
}

fn lowered_record_vec_field_index(record: &[(Name, LoweredValue)], field: &str) -> Option<usize> {
    record.iter().position(|(name, _)| name.as_str() == field)
}

fn lowered_projected_record_indices(
    projection: &LoweredReduceProjection<'_>,
    record: &[(Name, LoweredValue)],
) -> Option<LoweredProjectedRecordIndices> {
    let key = lowered_record_vec_field_index(record, projection.key_field)?;
    let mut values = Vec::with_capacity(projection.value_fields.len());
    for (_, source_field) in &projection.value_fields {
        values.push(lowered_record_vec_field_index(record, source_field)?);
    }
    Some(LoweredProjectedRecordIndices { key, values })
}

fn lowered_projected_record_indices_match(
    projection: &LoweredReduceProjection<'_>,
    record: &[(Name, LoweredValue)],
    indices: &LoweredProjectedRecordIndices,
) -> bool {
    record
        .get(indices.key)
        .is_some_and(|(name, _)| name.as_str() == projection.key_field)
        && indices.values.len() == projection.value_fields.len()
        && indices
            .values
            .iter()
            .zip(&projection.value_fields)
            .all(|(index, (_, source_field))| {
                record
                    .get(*index)
                    .is_some_and(|(name, _)| name.as_str() == *source_field)
            })
}

fn lowered_projected_key_value(
    projection: &LoweredReduceProjection<'_>,
    item: &LoweredValue,
    indices: Option<&LoweredProjectedRecordIndices>,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    if let (LoweredValue::RecordVec(record), Some(indices)) = (item, indices) {
        return Ok(record[indices.key].1.clone());
    }
    lowered_record_field_value(item, projection.key_field)
        .ok_or_else(|| RuntimeError::new("missing-field", projection.key_field).with_span(span))
}

fn lowered_projected_record_value(
    projection: &LoweredReduceProjection<'_>,
    item: &LoweredValue,
    indices: Option<&LoweredProjectedRecordIndices>,
    span: Span,
) -> Result<Vec<(Name, LoweredValue)>, RuntimeError> {
    let mut value = Vec::with_capacity(projection.value_fields.len());
    for (index, (name, source_field)) in projection.value_fields.iter().enumerate() {
        let field_value = if let (LoweredValue::RecordVec(record), Some(indices)) = (item, indices)
        {
            record[indices.values[index]].1.clone()
        } else {
            lowered_record_field_value(item, source_field)
                .ok_or_else(|| RuntimeError::new("missing-field", *source_field).with_span(span))?
        };
        value.push((*name, field_value));
    }
    Ok(value)
}

fn lowered_projected_acc_layout_matches(
    projection: &LoweredReduceProjection<'_>,
    acc: &[(Name, LoweredValue)],
) -> bool {
    acc.len() == projection.value_fields.len()
        && acc
            .iter()
            .zip(&projection.value_fields)
            .all(|((acc_name, _), (name, _))| acc_name == name)
}

fn lowered_record_vec_append_or_replace_unsorted(
    record: &mut Vec<(Name, LoweredValue)>,
    field: Name,
    value: LoweredValue,
) {
    if let Some((_, slot)) = record.iter_mut().find(|(key, _)| *key == field) {
        *slot = value;
    } else {
        record.push((field, value));
    }
}

enum LoweredRetryAttemptValue {
    Success(LoweredValue),
    Failed {
        error: Value,
        traceback: Option<Traceback>,
    },
    ControlBreak,
    /// An explicit `return` inside the retry body: escape the retry entirely and
    /// return from the enclosing proc with this value.
    Escape(LoweredValue),
}

fn lowered_trace_error_from_value(value: &Value) -> TraceError {
    TraceError::new(
        value.error_kind().unwrap_or("runtime-error"),
        value.error_message().unwrap_or("runtime error"),
    )
}

fn lowered_elf_info_value(path: PathValue, info: elf_module::ElfInfo) -> LoweredValue {
    LoweredValue::Record(BTreeMap::from([
        (Arc::from("path"), LoweredValue::Path(path)),
        (Arc::from("class"), LoweredValue::Str(info.class.into())),
        (Arc::from("endian"), LoweredValue::Str(info.endian.into())),
        (Arc::from("machine"), LoweredValue::Str(info.machine.into())),
        (Arc::from("os_abi"), LoweredValue::Str(info.os_abi.into())),
        (Arc::from("type"), LoweredValue::Str(info.elf_type.into())),
        (
            Arc::from("interpreter"),
            LoweredValue::Str(info.interpreter.into()),
        ),
        (Arc::from("soname"), LoweredValue::Str(info.soname.into())),
        (
            Arc::from("needed"),
            LoweredValue::List(
                info.needed
                    .into_iter()
                    .map(|item| LoweredValue::Str(item.into()))
                    .collect(),
            ),
        ),
        (Arc::from("rpath"), LoweredValue::Str(info.rpath.into())),
        (Arc::from("runpath"), LoweredValue::Str(info.runpath.into())),
        (
            Arc::from("flags"),
            LoweredValue::List(
                info.flags
                    .into_iter()
                    .map(|item| LoweredValue::Str(item.into()))
                    .collect(),
            ),
        ),
        (
            Arc::from("dynamic_tags"),
            LoweredValue::List(
                info.dynamic_tags
                    .into_iter()
                    .map(|entry| {
                        LoweredValue::Record(BTreeMap::from([
                            (Arc::from("tag"), LoweredValue::Str(entry.tag.into())),
                            (Arc::from("value"), LoweredValue::Int(entry.value)),
                        ]))
                    })
                    .collect(),
            ),
        ),
    ]))
}

fn child_cpu_ns() -> (i64, i64) {
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    if unsafe { libc::getrusage(libc::RUSAGE_CHILDREN, &mut usage) } != 0 {
        return (0, 0);
    }
    (timeval_ns(usage.ru_utime), timeval_ns(usage.ru_stime))
}

fn timeval_ns(tv: libc::timeval) -> i64 {
    #[cfg(target_os = "linux")]
    let usec = tv.tv_usec;
    #[cfg(not(target_os = "linux"))]
    let usec = i64::from(tv.tv_usec);
    tv.tv_sec
        .saturating_mul(1_000_000_000)
        .saturating_add(usec.saturating_mul(1_000))
}

fn cpu_ns_delta(before: (i64, i64)) -> (i64, i64) {
    let (user, system) = child_cpu_ns();
    ((user - before.0).max(0), (system - before.1).max(0))
}

fn lowered_measured_command_record(
    status: ProcessStatus,
    wall_ns: i64,
    user_ns: i64,
    system_ns: i64,
) -> LoweredValue {
    LoweredValue::Record(BTreeMap::from([
        (Arc::from("status"), LoweredValue::Status(Box::new(status))),
        (
            Arc::from("duration_ms"),
            LoweredValue::Int(wall_ns / 1_000_000),
        ),
        (Arc::from("wall_ns"), LoweredValue::Int(wall_ns)),
        (Arc::from("user_ns"), LoweredValue::Int(user_ns)),
        (Arc::from("system_ns"), LoweredValue::Int(system_ns)),
    ]))
}

fn read_host_path_bytes_vec(path: &Path, span: Span) -> Result<Vec<u8>, RuntimeError> {
    std::fs::read(path)
        .map_err(|error| RuntimeError::new("fs-read", error.to_string()).with_span(span))
}

fn read_host_path_bytes(path: &Path, span: Span) -> Result<Arc<[u8]>, RuntimeError> {
    let mut file = std::fs::File::open(path)
        .map_err(|error| RuntimeError::new("fs-read", error.to_string()).with_span(span))?;
    let len = file
        .metadata()
        .map_err(|error| RuntimeError::new("fs-read", error.to_string()).with_span(span))?
        .len()
        .try_into()
        .map_err(|_| RuntimeError::new("fs-read", "file is too large").with_span(span))?;
    if len == 0 {
        return Ok(Arc::from([]));
    }

    let mut bytes = Arc::<[u8]>::new_uninit_slice(len);
    let uninit = Arc::get_mut(&mut bytes).expect("new Arc has no aliases");
    let mut initialized = 0usize;
    while initialized < len {
        let chunk = unsafe {
            std::slice::from_raw_parts_mut(
                uninit[initialized..].as_mut_ptr().cast::<u8>(),
                len - initialized,
            )
        };
        match file.read(chunk) {
            Ok(0) => return Ok(initialized_prefix_to_arc(uninit, initialized)),
            Ok(read) => initialized += read,
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) => {
                return Err(RuntimeError::new("fs-read", error.to_string()).with_span(span));
            }
        }
    }

    let mut extra = [0u8; 1];
    loop {
        match file.read(&mut extra) {
            Ok(0) => return Ok(unsafe { bytes.assume_init() }),
            Ok(read) => {
                let mut value = unsafe { bytes.assume_init() }.to_vec();
                value.extend_from_slice(&extra[..read]);
                file.read_to_end(&mut value).map_err(|error| {
                    RuntimeError::new("fs-read", error.to_string()).with_span(span)
                })?;
                return Ok(value.into());
            }
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) => {
                return Err(RuntimeError::new("fs-read", error.to_string()).with_span(span));
            }
        }
    }
}

fn initialized_prefix_to_arc(bytes: &[std::mem::MaybeUninit<u8>], initialized: usize) -> Arc<[u8]> {
    let initialized =
        unsafe { std::slice::from_raw_parts(bytes.as_ptr().cast::<u8>(), initialized) };
    initialized.into()
}

fn lowered_encode_json(
    value: &LoweredValue,
    pretty: bool,
    span: Span,
) -> Result<String, RuntimeError> {
    if !pretty {
        let mut output = String::with_capacity(lowered_compact_json_capacity(value));
        lowered_write_compact_json(value, &mut output, span)?;
        return Ok(output);
    }

    let value = lowered_to_json(value, span)?;
    Ok(json_module::pretty_raw_json(&value))
}

fn lowered_compact_json_capacity(value: &LoweredValue) -> usize {
    match value {
        LoweredValue::Null => 4,
        LoweredValue::Bool(true) => 4,
        LoweredValue::Bool(false) => 5,
        LoweredValue::Int(value) => lowered_i64_json_len(*value),
        LoweredValue::Float(value) => value.0.to_string().len(),
        LoweredValue::Str(value) => value.len() + 2,
        LoweredValue::StrView(value) => value.as_str().len() + 2,
        LoweredValue::List(items) => lowered_compact_json_seq_capacity(items.iter()),
        LoweredValue::SharedList(items) => lowered_compact_json_seq_capacity(items.iter()),
        LoweredValue::Map(fields) => {
            let fields = fields.iter().map(|(key, value)| (key.as_str(), value));
            lowered_compact_json_map_capacity(fields)
        }
        LoweredValue::Record(fields) => {
            let fields = fields.iter().map(|(key, value)| (key.as_ref(), value));
            lowered_compact_json_map_capacity(fields)
        }
        LoweredValue::RecordVec(fields) => {
            let fields = fields
                .iter()
                .map(|(key, value)| (key.as_str().to_string(), value))
                .collect::<Vec<_>>();
            lowered_compact_json_map_capacity(
                fields.iter().map(|(key, value)| (key.as_str(), *value)),
            )
        }
        LoweredValue::Stats {
            blanks,
            code,
            comments,
        } => lowered_compact_json_stats_capacity(*blanks, None, *code, *comments),
        LoweredValue::StatsBlob(stats) => lowered_compact_json_stats_capacity(
            stats.blanks,
            Some(&stats.blobs),
            stats.code,
            stats.comments,
        ),
        LoweredValue::FsEntry(entry) => entry
            .to_record_map()
            .ok()
            .map(|record| {
                let mut capacity = 2;
                let mut first = true;
                for (key, value) in record.iter() {
                    let Some(value) = lowered_value_from_runtime_any(value) else {
                        continue;
                    };
                    if !first {
                        capacity += 1;
                    }
                    capacity += key.len() + 3 + lowered_compact_json_capacity(&value);
                    first = false;
                }
                capacity
            })
            .unwrap_or(4),
        _ => 4,
    }
}

fn lowered_compact_json_seq_capacity<'a>(items: impl Iterator<Item = &'a LoweredValue>) -> usize {
    let mut capacity = 2;
    let mut first = true;
    for item in items {
        if !first {
            capacity += 1;
        }
        capacity += lowered_compact_json_capacity(item);
        first = false;
    }
    capacity
}

fn lowered_compact_json_map_capacity<'a>(
    fields: impl Iterator<Item = (&'a str, &'a LoweredValue)>,
) -> usize {
    let mut capacity = 2;
    let mut first = true;
    for (key, value) in fields {
        if !first {
            capacity += 1;
        }
        capacity += key.len() + 3 + lowered_compact_json_capacity(value);
        first = false;
    }
    capacity
}

fn lowered_compact_json_stats_capacity(
    blanks: i64,
    blobs: Option<&BTreeMap<String, LoweredValue>>,
    code: i64,
    comments: i64,
) -> usize {
    let blobs = blobs
        .map(|blobs| {
            let fields = blobs.iter().map(|(key, value)| (key.as_str(), value));
            lowered_compact_json_map_capacity(fields)
        })
        .unwrap_or(2);
    35 + lowered_i64_json_len(blanks)
        + blobs
        + lowered_i64_json_len(code)
        + lowered_i64_json_len(comments)
}

fn lowered_i64_json_len(value: i64) -> usize {
    if value == 0 {
        return 1;
    }
    let negative = value < 0;
    let mut digits = 0usize;
    let mut value = value.unsigned_abs();
    while value > 0 {
        digits += 1;
        value /= 10;
    }
    digits + usize::from(negative)
}

fn lowered_write_compact_json(
    value: &LoweredValue,
    output: &mut String,
    span: Span,
) -> Result<(), RuntimeError> {
    match value {
        LoweredValue::Null => output.push_str("null"),
        LoweredValue::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
        LoweredValue::Int(value) => {
            let _ = write!(output, "{value}");
        }
        LoweredValue::Float(value) if value.0.is_finite() => {
            output.push_str(&miniserde::json::to_string(&value.0));
        }
        LoweredValue::Float(_) => {
            return Err(RuntimeError::new(
                "json-compatible",
                "non-finite Float values are not JSON-compatible",
            )
            .with_span(span));
        }
        LoweredValue::Str(value) => lowered_write_json_str(value.as_ref(), output),
        LoweredValue::StrView(value) => lowered_write_json_str(value.as_str(), output),
        LoweredValue::List(items) => lowered_write_json_seq(items.iter(), output, span)?,
        LoweredValue::SharedList(items) => lowered_write_json_seq(items.iter(), output, span)?,
        LoweredValue::Map(fields) => {
            lowered_write_json_map(
                fields.iter().map(|(key, value)| (key.as_str(), value)),
                output,
                span,
            )?;
        }
        LoweredValue::Record(fields) => {
            lowered_write_json_map(
                fields.iter().map(|(key, value)| (key.as_ref(), value)),
                output,
                span,
            )?;
        }
        LoweredValue::RecordVec(fields) => {
            lowered_write_json_map(
                fields
                    .iter()
                    .map(|(key, value)| (key.as_str().to_string(), value))
                    .collect::<Vec<_>>()
                    .iter()
                    .map(|(key, value)| (key.as_str(), *value)),
                output,
                span,
            )?;
        }
        LoweredValue::Stats {
            blanks,
            code,
            comments,
        } => {
            lowered_write_json_stats(*blanks, None, *code, *comments, output, span)?;
        }
        LoweredValue::StatsBlob(stats) => {
            lowered_write_json_stats(
                stats.blanks,
                Some(&stats.blobs),
                stats.code,
                stats.comments,
                output,
                span,
            )?;
        }
        LoweredValue::FsEntry(entry) => {
            let record = entry
                .to_record_map()
                .map_err(|error| error.with_span(span))?;
            output.push('{');
            let mut first = true;
            for (key, item) in record {
                let Some(item) = lowered_value_from_runtime_any(&item) else {
                    return Err(RuntimeError::new(
                        "type-error",
                        "fs entry field produced unsupported value",
                    )
                    .with_span(span));
                };
                if !first {
                    output.push(',');
                }
                lowered_write_json_str(key.as_ref(), output);
                output.push(':');
                lowered_write_compact_json(&item, output, span)?;
                first = false;
            }
            output.push('}');
        }
        value => return Err(lowered_json_compatible_error(value, span)),
    }
    Ok(())
}

fn lowered_write_json_seq<'a>(
    items: impl Iterator<Item = &'a LoweredValue>,
    output: &mut String,
    span: Span,
) -> Result<(), RuntimeError> {
    output.push('[');
    let mut first = true;
    for item in items {
        if !first {
            output.push(',');
        }
        lowered_write_compact_json(item, output, span)?;
        first = false;
    }
    output.push(']');
    Ok(())
}

fn lowered_write_json_map<'a>(
    fields: impl Iterator<Item = (&'a str, &'a LoweredValue)>,
    output: &mut String,
    span: Span,
) -> Result<(), RuntimeError> {
    output.push('{');
    let mut first = true;
    for (key, value) in fields {
        if !first {
            output.push(',');
        }
        lowered_write_json_str(key, output);
        output.push(':');
        lowered_write_compact_json(value, output, span)?;
        first = false;
    }
    output.push('}');
    Ok(())
}

fn lowered_write_json_stats(
    blanks: i64,
    blobs: Option<&BTreeMap<String, LoweredValue>>,
    code: i64,
    comments: i64,
    output: &mut String,
    span: Span,
) -> Result<(), RuntimeError> {
    output.push_str("{\"blanks\":");
    let _ = write!(output, "{blanks}");
    output.push_str(",\"blobs\":");
    if let Some(blobs) = blobs {
        lowered_write_json_map(
            blobs.iter().map(|(key, value)| (key.as_str(), value)),
            output,
            span,
        )?;
    } else {
        output.push_str("{}");
    }
    output.push_str(",\"code\":");
    let _ = write!(output, "{code}");
    output.push_str(",\"comments\":");
    let _ = write!(output, "{comments}");
    output.push('}');
    Ok(())
}

fn lowered_write_json_str(value: &str, output: &mut String) {
    output.push('"');
    let bytes = value.as_bytes();
    let mut start = 0usize;
    for (index, &byte) in bytes.iter().enumerate() {
        let escaped = match byte {
            b'\x08' => Some("\\b"),
            b'\t' => Some("\\t"),
            b'\n' => Some("\\n"),
            b'\x0c' => Some("\\f"),
            b'\r' => Some("\\r"),
            b'"' => Some("\\\""),
            b'\\' => Some("\\\\"),
            0x00..=0x1f => {
                if start < index {
                    output.push_str(&value[start..index]);
                }
                const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";
                output.push_str("\\u00");
                output.push(HEX_DIGITS[(byte >> 4) as usize] as char);
                output.push(HEX_DIGITS[(byte & 0x0f) as usize] as char);
                start = index + 1;
                continue;
            }
            _ => None,
        };
        if let Some(escaped) = escaped {
            if start < index {
                output.push_str(&value[start..index]);
            }
            output.push_str(escaped);
            start = index + 1;
        }
    }
    if start != bytes.len() {
        output.push_str(&value[start..]);
    }
    output.push('"');
}

fn lowered_json_compatible_error(value: &LoweredValue, span: Span) -> RuntimeError {
    RuntimeError::new(
        "json-compatible",
        format!(
            "{} is not JSON-compatible; convert Path, Bytes, Status, Result, and errors explicitly",
            value.type_name()
        ),
    )
    .with_span(span)
}

fn lowered_to_json(
    value: &LoweredValue,
    span: Span,
) -> Result<miniserde::json::Value, RuntimeError> {
    match value {
        LoweredValue::Null => Ok(miniserde::json::Value::Null),
        LoweredValue::Bool(value) => Ok(json_module::raw_json_bool(*value)),
        LoweredValue::Int(value) => Ok(json_module::raw_json_i64(*value)),
        LoweredValue::Float(value) if value.0.is_finite() => Ok(json_module::raw_json_f64(value.0)),
        LoweredValue::Float(_) => Err(RuntimeError::new(
            "json-compatible",
            "non-finite Float values are not JSON-compatible",
        )
        .with_span(span)),
        LoweredValue::Str(value) => Ok(json_module::raw_json_string(value.as_ref())),
        LoweredValue::StrView(value) => Ok(json_module::raw_json_string(value.as_str())),
        LoweredValue::List(items) => {
            let mut values = Vec::with_capacity(items.len());
            for item in items {
                values.push(lowered_to_json(item, span)?);
            }
            Ok(json_module::raw_json_array(values))
        }
        LoweredValue::SharedList(items) => {
            let mut values = Vec::with_capacity(items.len());
            for item in items.iter() {
                values.push(lowered_to_json(item, span)?);
            }
            Ok(json_module::raw_json_array(values))
        }
        LoweredValue::Map(fields) => {
            let mut values = Vec::with_capacity(fields.len());
            for (key, item) in fields {
                values.push((key.clone(), lowered_to_json(item, span)?));
            }
            Ok(json_module::raw_json_object(values))
        }
        LoweredValue::Record(fields) => {
            let mut values = Vec::with_capacity(fields.len());
            for (key, item) in fields {
                values.push((key.to_string(), lowered_to_json(item, span)?));
            }
            Ok(json_module::raw_json_object(values))
        }
        LoweredValue::RecordVec(fields) => {
            let mut values = Vec::with_capacity(fields.len());
            for (key, item) in fields.iter() {
                values.push((key.to_string(), lowered_to_json(item, span)?));
            }
            Ok(json_module::raw_json_object(values))
        }
        LoweredValue::Stats {
            blanks,
            code,
            comments,
        } => Ok(json_module::raw_json_object(vec![
            (
                "blanks".to_string(),
                lowered_to_json(&LoweredValue::Int(*blanks), span)?,
            ),
            (
                "blobs".to_string(),
                lowered_to_json(&LoweredValue::Map(BTreeMap::new()), span)?,
            ),
            (
                "code".to_string(),
                lowered_to_json(&LoweredValue::Int(*code), span)?,
            ),
            (
                "comments".to_string(),
                lowered_to_json(&LoweredValue::Int(*comments), span)?,
            ),
        ])),
        LoweredValue::StatsBlob(stats) => Ok(json_module::raw_json_object(vec![
            (
                "blanks".to_string(),
                lowered_to_json(&LoweredValue::Int(stats.blanks), span)?,
            ),
            (
                "blobs".to_string(),
                lowered_to_json(&LoweredValue::Map(stats.blobs.clone()), span)?,
            ),
            (
                "code".to_string(),
                lowered_to_json(&LoweredValue::Int(stats.code), span)?,
            ),
            (
                "comments".to_string(),
                lowered_to_json(&LoweredValue::Int(stats.comments), span)?,
            ),
        ])),
        LoweredValue::FsEntry(entry) => {
            let record = entry.to_record_map().map_err(|error| error.with_span(span))?;
            let mut values = Vec::with_capacity(record.len());
            for (key, item) in record {
                let Some(item) = lowered_value_from_runtime_any(&item) else {
                    return Err(RuntimeError::new(
                        "type-error",
                        "fs entry field produced unsupported value",
                    )
                    .with_span(span));
                };
                values.push((key.to_string(), lowered_to_json(&item, span)?));
            }
            Ok(json_module::raw_json_object(values))
        }
        value => Err(RuntimeError::new(
            "json-compatible",
            format!(
                "{} is not JSON-compatible; convert Path, Bytes, Status, Result, and errors explicitly",
                value.type_name()
            ),
        )
        .with_span(span)),
    }
}

fn read_host_path_string(path: &Path, operation: &str, span: Span) -> Result<String, RuntimeError> {
    let bytes = std::fs::read(path)
        .map_err(|error| RuntimeError::new(operation, error.to_string()).with_span(span))?;
    String::from_utf8(bytes)
        .map_err(|error| RuntimeError::new(operation, error.to_string()).with_span(span))
}

#[cfg(feature = "native-tests")]
fn create_host_dir_all(path: &Path, operation: &str, span: Span) -> Result<(), RuntimeError> {
    std::fs::create_dir_all(path)
        .map_err(|error| RuntimeError::new(operation, error.to_string()).with_span(span))
}

/// Whether a captured module-export signature satisfies the contract `expected`:
/// same arity, matching (bidirectional) param types and rest flags, a defaulted
/// flag that the contract permits, compatible return type, and the contract's
/// effects are a subset of the function's effects.
fn lowered_signature_matches_contract(sig: &CallableType, expected: &CallableType) -> bool {
    sig.params.len() == expected.params.len()
        && sig
            .params
            .iter()
            .zip(&expected.params)
            .all(|(actual, expected)| {
                actual.rest == expected.rest && actual.ty.matches_expected(&expected.ty)
            })
        && match (&sig.effects, &expected.effects) {
            (None, None) => true,
            (Some(actual), Some(expected)) => expected.iter().all(|effect| actual.contains(effect)),
            _ => false,
        }
        && sig.return_ty.matches_expected(&expected.return_ty)
}

fn lowered_module_matches_contract(
    evaluator: &Evaluator,
    module: &BTreeMap<Arc<str>, LoweredValue>,
    exports: &BTreeMap<Name, ModuleExportType>,
) -> bool {
    exports.iter().all(|(name, export)| {
        let name_text = name.as_str();
        let Some(value) = module.get::<str>(name_text.as_str()) else {
            return export.optional();
        };
        match export {
            ModuleExportType::Value { ty, .. } => lowered_value_matches_static_type(value, ty),
            ModuleExportType::Proc { sig, .. } => match value {
                LoweredValue::Proc(function) => evaluator
                    .lookup_module_export_signature(*function)
                    .is_some_and(|captured| {
                        !captured.pure && lowered_signature_matches_contract(&captured.sig, sig)
                    }),
                _ => false,
            },
            ModuleExportType::Pure { sig, .. } => match value {
                LoweredValue::Pure(function) => evaluator
                    .lookup_module_export_signature(*function)
                    .is_some_and(|captured| {
                        captured.pure && lowered_signature_matches_contract(&captured.sig, sig)
                    }),
                _ => false,
            },
        }
    })
}

fn validate_dynamic_module_top_level(
    program: &ArenaProgram,
    display_path: &str,
    span: Span,
) -> Result<(), RuntimeError> {
    for stmt in program.statement_ids() {
        let code = match program.arena.stmt(stmt).kind {
            crate::syntax::arena::ArenaStmtKind::SignalHook(_) => Some("check.signal-hook-module"),
            crate::syntax::arena::ArenaStmtKind::Var { .. }
            | crate::syntax::arena::ArenaStmtKind::Command(_) => Some("check.module-top-level"),
            _ => None,
        };
        if let Some(code) = code {
            return Err(RuntimeError::new(
                "module-check",
                format!("{display_path}: {code}: invalid dynamic module top-level statement"),
            )
            .with_span(span));
        }
    }
    Ok(())
}

fn lowered_value_satisfies_require(evaluator: &Evaluator, value: &LoweredValue, ty: &Type) -> bool {
    match (value, ty) {
        (LoweredValue::Module(module), Type::Module(exports)) => {
            lowered_module_matches_contract(evaluator, module, exports)
        }
        _ => lowered_value_matches_static_type(value, ty),
    }
}

fn lowered_result_ok(value: LoweredValue) -> LoweredValue {
    LoweredValue::ResultOk(Box::new(value))
}

fn lowered_result_err_value(error: RuntimeError) -> LoweredValue {
    LoweredValue::ResultErr(Box::new(Value::Error(Box::new(error))))
}

fn lowered_unit_result(result: Result<(), RuntimeError>) -> LoweredValue {
    match result {
        Ok(()) => lowered_result_ok(LoweredValue::Unit),
        Err(error) => lowered_result_err_value(error),
    }
}

fn lowered_path_arg(
    value: LoweredValue,
    operation: &str,
    span: Span,
) -> Result<PathValue, RuntimeError> {
    match value {
        LoweredValue::Path(path) => Ok(path),
        other => Err(RuntimeError::new(
            "type-error",
            format!("{operation} expected Path, found {}", other.type_name()),
        )
        .with_span(span)),
    }
}

fn lowered_path_from_value(
    value: LoweredValue,
    operation: &str,
    span: Span,
) -> Result<PathValue, RuntimeError> {
    let Some(text) = lowered_str_value(&value) else {
        return Err(RuntimeError::new(
            "type-error",
            format!("{operation} expected Str, found {}", value.type_name()),
        )
        .with_span(span));
    };
    PathValue::from_text(text).map_err(|error| error.with_span(span))
}

fn lowered_str_arg_owned(
    value: Option<LoweredValue>,
    default: &str,
    operation: &str,
    span: Span,
) -> Result<String, RuntimeError> {
    match value {
        Some(LoweredValue::Str(value)) => Ok(value.to_string()),
        Some(LoweredValue::StrView(value)) => Ok(value.as_str().to_string()),
        Some(other) => Err(RuntimeError::new(
            "type-error",
            format!("{operation} expected Str, found {}", other.type_name()),
        )
        .with_span(span)),
        None => Ok(default.to_string()),
    }
}

fn lowered_bool_arg_or(
    value: Option<LoweredValue>,
    default: bool,
    operation: &str,
    span: Span,
) -> Result<bool, RuntimeError> {
    match value {
        Some(LoweredValue::Bool(value)) => Ok(value),
        Some(other) => Err(RuntimeError::new(
            "type-error",
            format!("{operation} expected Bool, found {}", other.type_name()),
        )
        .with_span(span)),
        None => Ok(default),
    }
}

fn lowered_int_arg(
    value: Option<LoweredValue>,
    operation: &str,
    span: Span,
) -> Result<i64, RuntimeError> {
    match value {
        Some(LoweredValue::Int(value)) => Ok(value),
        Some(other) => Err(RuntimeError::new(
            "type-error",
            format!("{operation} expected Int, found {}", other.type_name()),
        )
        .with_span(span)),
        None => Err(
            RuntimeError::new("arity", format!("{operation} expected an argument")).with_span(span),
        ),
    }
}

fn lowered_parse_command_values(
    values: Vec<LoweredValue>,
    _span: Span,
) -> Result<(Vec<LoweredValue>, FxHashMap<String, bool>), RuntimeError> {
    let mut positionals = Vec::new();
    let mut flags: FxHashMap<String, bool> = FxHashMap::default();
    for value in values {
        if let Some(flag) = lowered_flag_name(&value) {
            flags.insert(flag, true);
        } else {
            positionals.push(value);
        }
    }
    Ok((positionals, flags))
}

fn lowered_flag_name(value: &LoweredValue) -> Option<String> {
    let text = lowered_str_value(value)?;
    let flag = text.strip_prefix("--")?;
    if flag.is_empty() || flag.contains('=') {
        return None;
    }
    Some(flag.replace('-', "_"))
}

fn lowered_int_arg_or(
    value: Option<LoweredValue>,
    default: i64,
    operation: &str,
    span: Span,
) -> Result<i64, RuntimeError> {
    match value {
        Some(value) => lowered_int_arg(Some(value), operation, span),
        None => Ok(default),
    }
}

fn lowered_session_user_record(
    record: RecordMap,
    home: PathBuf,
    span: Span,
) -> Result<auth_module::SessionUser, RuntimeError> {
    let uid = lowered_record_u32(&record, "uid", span)?;
    let gid = lowered_record_u32(&record, "gid", span)?;
    Ok(auth_module::SessionUser {
        name: record_str(&record, "name", None, span)?,
        uid,
        gid,
        home,
        shell: record_str(&record, "shell", Some(""), span)?,
    })
}

fn lowered_record_u32(record: &RecordMap, name: &str, span: Span) -> Result<u32, RuntimeError> {
    let value = record_int_field(record, name, "applet-session", span)?;
    if !(0..=u32::MAX as i64).contains(&value) {
        return Err(
            RuntimeError::new("uid-range", format!("{name} is out of range")).with_span(span),
        );
    }
    Ok(value as u32)
}

fn lowered_duration_arg(
    value: Option<LoweredValue>,
    operation: &str,
    span: Span,
) -> Result<DurationValue, RuntimeError> {
    match value {
        Some(LoweredValue::Duration(value)) => Ok(value),
        Some(value) => Err(RuntimeError::new(
            "type-error",
            format!("{operation} expected Duration, found {}", value.type_name()),
        )
        .with_span(span)),
        None => Err(RuntimeError::new(
            "arity-error",
            format!("{operation} expected Duration argument"),
        )
        .with_span(span)),
    }
}

fn lowered_str_list_arg(
    value: Option<LoweredValue>,
    operation: &str,
    span: Span,
) -> Result<Vec<String>, RuntimeError> {
    let Some(value) = value else {
        return Err(
            RuntimeError::new("type-error", format!("{operation} expected List[Str]"))
                .with_span(span),
        );
    };
    let items = match value {
        LoweredValue::List(items) => items,
        LoweredValue::SharedList(items) => items.iter().cloned().collect(),
        _ => {
            return Err(
                RuntimeError::new("type-error", format!("{operation} expected List[Str]"))
                    .with_span(span),
            );
        }
    };
    let mut strings = Vec::with_capacity(items.len());
    for item in items {
        if let Some(text) = lowered_str_value(&item) {
            strings.push(text.to_string());
        } else {
            return Err(RuntimeError::new(
                "type-error",
                format!(
                    "{operation} expected List[Str], found List containing {}",
                    item.type_name()
                ),
            )
            .with_span(span));
        }
    }
    Ok(strings)
}

fn lowered_argv_list(
    value: Option<LoweredValue>,
    operation: &str,
    span: Span,
) -> Result<Vec<Vec<u8>>, RuntimeError> {
    let Some(value) = value else {
        return Err(RuntimeError::new(
            "type-error",
            format!("{operation} expected List[Str] or List[Path]"),
        )
        .with_span(span));
    };
    let items = match value {
        LoweredValue::List(items) => items,
        LoweredValue::SharedList(items) => items.iter().cloned().collect(),
        _ => {
            return Err(RuntimeError::new(
                "type-error",
                format!("{operation} expected List[Str] or List[Path]"),
            )
            .with_span(span));
        }
    };
    let mut words = Vec::with_capacity(items.len());
    for item in items {
        match item {
            LoweredValue::Str(text) => words.push(text.as_bytes().to_vec()),
            LoweredValue::StrView(view) => words.push(view.as_str().as_bytes().to_vec()),
            LoweredValue::Path(path) => words.push(path.bytes),
            other => {
                return Err(RuntimeError::new(
                    "type-error",
                    format!(
                        "{operation} expected List[Str] or List[Path], found List containing {}",
                        other.type_name()
                    ),
                )
                .with_span(span));
            }
        }
    }
    Ok(words)
}

fn lowered_optional_str_list(
    value: Option<LoweredValue>,
    operation: &str,
    span: Span,
) -> Result<Vec<String>, RuntimeError> {
    match value {
        Some(value) => lowered_str_list_arg(Some(value), operation, span),
        None => Ok(Vec::new()),
    }
}

fn lowered_path_list(
    value: Option<LoweredValue>,
    operation: &str,
    span: Span,
) -> Result<Vec<PathValue>, RuntimeError> {
    let Some(value) = value else {
        return Err(
            RuntimeError::new("type-error", format!("{operation} expected List[Path]"))
                .with_span(span),
        );
    };
    let items = match value {
        LoweredValue::List(items) => items,
        LoweredValue::SharedList(items) => items.iter().cloned().collect(),
        _ => {
            return Err(RuntimeError::new(
                "type-error",
                format!("{operation} expected List[Path]"),
            )
            .with_span(span));
        }
    };
    items
        .into_iter()
        .map(|item| lowered_path_arg(item, operation, span))
        .collect()
}

fn lowered_int_list_arg(
    value: Option<LoweredValue>,
    operation: &str,
    span: Span,
) -> Result<Vec<i64>, RuntimeError> {
    let Some(value) = value else {
        return Err(
            RuntimeError::new("type-error", format!("{operation} expected List[Int]"))
                .with_span(span),
        );
    };
    let items = match value {
        LoweredValue::List(items) => items,
        LoweredValue::SharedList(items) => items.iter().cloned().collect(),
        _ => {
            return Err(
                RuntimeError::new("type-error", format!("{operation} expected List[Int]"))
                    .with_span(span),
            );
        }
    };
    let mut ints = Vec::with_capacity(items.len());
    for item in items {
        let LoweredValue::Int(value) = item else {
            return Err(RuntimeError::new(
                "type-error",
                format!(
                    "{operation} expected List[Int], found List containing {}",
                    item.type_name()
                ),
            )
            .with_span(span));
        };
        ints.push(value);
    }
    Ok(ints)
}

fn lowered_bytes_list_arg(
    value: Option<LoweredValue>,
    operation: &str,
    span: Span,
) -> Result<Vec<Vec<u8>>, RuntimeError> {
    let Some(value) = value else {
        return Err(
            RuntimeError::new("type-error", format!("{operation} expected List[Bytes]"))
                .with_span(span),
        );
    };
    let items = match value {
        LoweredValue::List(items) => items,
        LoweredValue::SharedList(items) => items.iter().cloned().collect(),
        _ => {
            return Err(RuntimeError::new(
                "type-error",
                format!("{operation} expected List[Bytes]"),
            )
            .with_span(span));
        }
    };
    let mut chunks = Vec::new();
    for item in items {
        if let Some(bytes) = lowered_bytes_value(&item) {
            chunks.push(bytes.to_vec());
        } else {
            return Err(RuntimeError::new(
                "type-error",
                format!(
                    "{operation} expected List[Bytes], found List containing {}",
                    item.type_name()
                ),
            )
            .with_span(span));
        }
    }
    Ok(chunks)
}

fn lowered_bool_map_arg(
    value: Option<LoweredValue>,
    operation: &str,
    span: Span,
) -> Result<BTreeMap<String, LoweredValue>, RuntimeError> {
    let Some(LoweredValue::Map(items)) = value else {
        return Err(
            RuntimeError::new("type-error", format!("{operation} expected Map[Bool]"))
                .with_span(span),
        );
    };
    for value in items.values() {
        if !matches!(value, LoweredValue::Bool(_)) {
            return Err(RuntimeError::new(
                "type-error",
                format!(
                    "{operation} expected Map[Bool], found Map containing {}",
                    value.type_name()
                ),
            )
            .with_span(span));
        }
    }
    Ok(items)
}

fn lowered_record_arg(
    value: Option<LoweredValue>,
    operation: &str,
    span: Span,
) -> Result<RecordMap, RuntimeError> {
    match value {
        Some(LoweredValue::FsEntry(entry)) => {
            entry.to_record_map().map_err(|error| error.with_span(span))
        }
        Some(LoweredValue::Record(fields) | LoweredValue::Module(fields)) => {
            Ok(RecordMap::from_name_values(
                fields
                    .into_iter()
                    .map(|(key, value)| (Name::intern(key.as_ref()), value.into_value()))
                    .collect(),
            ))
        }
        Some(LoweredValue::RecordVec(fields)) => Ok(RecordMap::from_name_values(
            fields
                .into_iter()
                .map(|(key, value)| (key, value.into_value()))
                .collect(),
        )),
        Some(LoweredValue::Stats {
            blanks,
            code,
            comments,
        }) => Ok(super::lowered_inline_stats_to_record_map(
            blanks, code, comments,
        )),
        Some(LoweredValue::StatsBlob(stats)) => Ok(stats.to_record_map()),
        Some(other) => Err(RuntimeError::new(
            "type-error",
            format!("{operation} expected Record, found {}", other.type_name()),
        )
        .with_span(span)),
        None => Err(
            RuntimeError::new("arity", format!("{operation} expected an argument")).with_span(span),
        ),
    }
}

#[cfg(feature = "native-tests")]
fn lowered_optional_str_record(
    value: Option<LoweredValue>,
    operation: &str,
    span: Span,
) -> Result<BTreeMap<String, String>, RuntimeError> {
    let Some(value) = value else {
        return Ok(BTreeMap::new());
    };
    let fields = match value {
        LoweredValue::Record(fields) | LoweredValue::Module(fields) => fields,
        LoweredValue::RecordVec(fields) => fields
            .into_iter()
            .map(|(name, value)| (Arc::<str>::from(name.as_str().as_str()), value))
            .collect(),
        other => {
            return Err(RuntimeError::new(
                "type-error",
                format!("{operation} expected Record, found {}", other.type_name()),
            )
            .with_span(span));
        }
    };

    let mut env = BTreeMap::new();
    for (key, value) in fields {
        let Some(text) = lowered_str_value(&value) else {
            return Err(RuntimeError::new(
                "type-error",
                format!(
                    "{operation} env field `{key}` expected Str, found {}",
                    value.type_name()
                ),
            )
            .with_span(span));
        };
        env.insert(key.to_string(), text.to_string());
    }
    Ok(env)
}

#[cfg(feature = "native-tests")]
fn lowered_bytes_arg_or_empty(
    value: Option<LoweredValue>,
    operation: &str,
    span: Span,
) -> Result<Vec<u8>, RuntimeError> {
    match value {
        Some(LoweredValue::Bytes(bytes)) => Ok(bytes.to_vec()),
        Some(LoweredValue::BytesView(bytes)) => Ok(bytes.as_slice().to_vec()),
        Some(other) => Err(RuntimeError::new(
            "type-error",
            format!("{operation} expected Bytes, found {}", other.type_name()),
        )
        .with_span(span)),
        None => Ok(Vec::new()),
    }
}

fn lowered_command_target_bytes(value: LoweredValue, span: Span) -> Result<Vec<u8>, RuntimeError> {
    let target = match value {
        LoweredValue::Str(value) => value.as_bytes().to_vec(),
        LoweredValue::StrView(value) => value.as_str().as_bytes().to_vec(),
        LoweredValue::Path(value) => value.bytes,
        other => {
            return Err(RuntimeError::new(
                "type-error",
                format!("expected Str or Path, found {}", other.type_name()),
            )
            .with_span(span));
        }
    };
    if target.contains(&0) {
        return Err(
            RuntimeError::new("nul-target", "run target cannot contain NUL").with_span(span),
        );
    }
    Ok(target)
}

fn lowered_path_like_arg(
    value: LoweredValue,
    operation: &str,
    span: Span,
) -> Result<PathValue, RuntimeError> {
    match value {
        LoweredValue::Path(path) => Ok(path),
        LoweredValue::Str(text) => {
            PathValue::from_text(text).map_err(|error| error.with_span(span))
        }
        LoweredValue::StrView(text) => {
            PathValue::from_text(text.as_str()).map_err(|error| error.with_span(span))
        }
        other => Err(RuntimeError::new(
            "type-error",
            format!(
                "{operation} expected Path or Str, found {}",
                other.type_name()
            ),
        )
        .with_span(span)),
    }
}

fn lowered_env_record_arg(
    value: LoweredValue,
    operation: &str,
    span: Span,
) -> Result<BTreeMap<String, String>, RuntimeError> {
    let mut env = BTreeMap::new();
    match value {
        LoweredValue::Record(fields) => {
            for (name, value) in fields {
                let mut text = String::new();
                push_lowered_display(&mut text, &value, span)?;
                env.insert(name.to_string(), text);
            }
        }
        LoweredValue::RecordVec(fields) => {
            for (name, value) in fields.iter() {
                let mut text = String::new();
                push_lowered_display(&mut text, value, span)?;
                env.insert(name.to_string(), text);
            }
        }
        _ => {
            return Err(
                RuntimeError::new("type-error", format!("{operation} expected Record"))
                    .with_span(span),
            );
        }
    }
    Ok(env)
}

fn lowered_command_arg(
    value: LoweredValue,
    operation: &str,
    span: Span,
) -> Result<CommandPlan, RuntimeError> {
    match value {
        LoweredValue::Command(plan) => Ok(*plan),
        other => Err(RuntimeError::new(
            "type-error",
            format!("{operation} expected Command, found {}", other.type_name()),
        )
        .with_span(span)),
    }
}

fn lowered_command_redirections(
    stdin: Option<LoweredValue>,
    stdout: Option<LoweredValue>,
    stderr: Option<LoweredValue>,
    stdout_append: bool,
    stderr_append: bool,
    operation: &str,
    span: Span,
) -> Result<Vec<CommandRedirection>, RuntimeError> {
    let mut redirections = Vec::new();

    if let Some(value) = stdin {
        redirections.push(CommandRedirection::File {
            stream: CommandRedirectionStream::Stdin,
            mode: CommandRedirectionMode::Read,
            path: lowered_path_like_arg(value, operation, span)?,
        });
    }

    if let Some(value) = stdout {
        redirections.push(CommandRedirection::File {
            stream: CommandRedirectionStream::Stdout,
            mode: if stdout_append {
                CommandRedirectionMode::Append
            } else {
                CommandRedirectionMode::Write
            },
            path: lowered_path_like_arg(value, operation, span)?,
        });
    }

    if let Some(value) = stderr {
        redirections.push(CommandRedirection::File {
            stream: CommandRedirectionStream::Stderr,
            mode: if stderr_append {
                CommandRedirectionMode::Append
            } else {
                CommandRedirectionMode::Write
            },
            path: lowered_path_like_arg(value, operation, span)?,
        });
    }

    Ok(redirections)
}

fn lowered_command_plan_value(
    target: LoweredValue,
    argv: LoweredValue,
    cwd: Option<LoweredValue>,
    env: Option<LoweredValue>,
    stdin: Option<LoweredValue>,
    stdout: Option<LoweredValue>,
    stderr: Option<LoweredValue>,
    stdout_append: Option<LoweredValue>,
    stderr_append: Option<LoweredValue>,
    timeout: Option<LoweredValue>,
    detach: Option<LoweredValue>,
    new_session: Option<LoweredValue>,
    ignore_hup: Option<LoweredValue>,
    cpu_max: Option<LoweredValue>,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    let target = lowered_command_target_bytes(target, span)?;
    let argv_words = lowered_argv_list(Some(argv), "process.command_argv", span)?;
    if argv_words.is_empty() {
        return Err(RuntimeError::new(
            "argv-empty",
            "argv must include argv[0], the child program name",
        )
        .with_span(span));
    }
    for word in &argv_words {
        if word.contains(&0) {
            return Err(
                RuntimeError::new("nul-argv", "argv items cannot contain NUL bytes")
                    .with_span(span),
            );
        }
    }
    let mut argv = Vec::with_capacity(argv_words.len().saturating_sub(1));
    for word in argv_words.into_iter().skip(1) {
        argv.push(word);
    }
    let cwd = cwd
        .map(|value| lowered_path_like_arg(value, "process.command_argv", span))
        .transpose()?;
    let env = env
        .map(|value| lowered_env_record_arg(value, "process.command_argv", span))
        .transpose()?
        .unwrap_or_default();
    let stdout_append = lowered_bool_arg_or(stdout_append, false, "process.command_argv", span)?;
    let stderr_append = lowered_bool_arg_or(stderr_append, false, "process.command_argv", span)?;
    let redirections = lowered_command_redirections(
        stdin,
        stdout,
        stderr,
        stdout_append,
        stderr_append,
        "process.command_argv",
        span,
    )?;
    let timeout = timeout
        .map(|value| lowered_duration_arg(Some(value), "process.command_argv", span))
        .transpose()?;
    let detach = lowered_bool_arg_or(detach, false, "process.command_argv", span)?;
    let new_session = lowered_bool_arg_or(new_session, false, "process.command_argv", span)?;
    let ignore_hup = lowered_bool_arg_or(ignore_hup, false, "process.command_argv", span)?;
    let cpu_max = match cpu_max {
        Some(value) => {
            let value = lowered_int_arg(Some(value), "process.command_argv", span)?;
            if value <= 0 {
                return Err(
                    RuntimeError::new("cpu-max", "cpu_max must be positive").with_span(span)
                );
            }
            Some(value)
        }
        None => None,
    };

    Ok(LoweredValue::Command(Box::new(CommandPlan {
        target,
        argv,
        cwd,
        env,
        redirections,
        timeout,
        cpu_max,
        detach,
        new_session,
        ignore_hup,
    })))
}

fn lowered_bool_builder_field(
    value: LoweredValue,
    field: &str,
    span: Span,
) -> Result<bool, RuntimeError> {
    let LoweredValue::Bool(value) = value else {
        return Err(
            RuntimeError::new("type-error", format!("{field} expected Bool")).with_span(span),
        );
    };
    Ok(value)
}

fn lowered_runtime_value(value: Value, span: Span) -> Result<LoweredValue, RuntimeError> {
    lowered_value_from_runtime_any(&value).ok_or_else(|| {
        RuntimeError::new("type-error", "unsupported lowered module value").with_span(span)
    })
}

fn lowered_runtime_result(
    result: Result<Value, RuntimeError>,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    lowered_runtime_value(
        match result {
            // Module functions (e.g. linux real-mode) may already return a
            // wrapped Result value — don't double-wrap.
            Ok(value @ Value::Result(_)) => value,
            Ok(value) => Value::ok(value),
            Err(error) => Value::err(Value::Error(Box::new(error))),
        },
        span,
    )
}

fn lowered_module_result_value(
    result: Result<Value, RuntimeError>,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    lowered_runtime_value(
        match result {
            Ok(value) => value,
            Err(error) => Value::err(Value::Error(Box::new(error))),
        },
        span,
    )
}

fn unix_require_arg(
    value: Option<LoweredValue>,
    operation: &str,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    value.ok_or_else(|| {
        RuntimeError::new("arity", format!("{operation} expected an argument")).with_span(span)
    })
}

fn pid1_event_record(event: unix_module::Pid1Event) -> Value {
    let kind = match event.kind {
        unix_module::Pid1EventKind::Signal => "signal",
        unix_module::Pid1EventKind::Children => "children",
        unix_module::Pid1EventKind::Poll => "poll",
        unix_module::Pid1EventKind::Timeout => "timeout",
    };
    Value::Record(RecordMap::from([
        (Arc::from("kind"), Value::Str(kind.into())),
        (Arc::from("signal"), Value::Str(event.signal.into())),
        (
            Arc::from("children"),
            Value::List(
                event
                    .children
                    .into_iter()
                    .map(|child| {
                        Value::Record(RecordMap::from([
                            (Arc::from("pid"), Value::Int(child.pid)),
                            (Arc::from("status"), Value::Status(child.status)),
                        ]))
                    })
                    .collect(),
            ),
        ),
    ]))
}

fn pid1_shutdown_record(shutdown: unix_module::Pid1Shutdown) -> Value {
    Value::Record(RecordMap::from([
        (Arc::from("term_sent"), Value::Int(shutdown.term_sent)),
        (Arc::from("kill_sent"), Value::Int(shutdown.kill_sent)),
        (
            Arc::from("reaped"),
            Value::List(
                shutdown
                    .reaped
                    .into_iter()
                    .map(|child| {
                        Value::Record(RecordMap::from([
                            (Arc::from("pid"), Value::Int(child.pid)),
                            (Arc::from("status"), Value::Status(child.status)),
                        ]))
                    })
                    .collect(),
            ),
        ),
        (
            Arc::from("remaining"),
            Value::List(shutdown.remaining.into_iter().map(Value::Int).collect()),
        ),
    ]))
}

fn spawned_child_record(child: unix_module::SpawnedChild) -> Value {
    Value::ok(Value::Record(RecordMap::from([
        (Arc::from("pid"), Value::Int(child.pid)),
        (Arc::from("command"), Value::Str(child.command.into())),
        (
            Arc::from("argv"),
            Value::List(
                child
                    .argv
                    .into_iter()
                    .map(|arg| Value::Str(arg.into()))
                    .collect(),
            ),
        ),
        (Arc::from("detach"), Value::Bool(child.detach)),
        (Arc::from("new_session"), Value::Bool(child.new_session)),
        (Arc::from("ignore_hup"), Value::Bool(child.ignore_hup)),
        (Arc::from("notify_fd"), Value::Int(child.notify_fd)),
    ])))
}

fn unix_dry_run_tty_attrs() -> Value {
    Value::Record(RecordMap::from([
        (Arc::from("iflag"), Value::Int(0)),
        (Arc::from("oflag"), Value::Int(0)),
        (Arc::from("cflag"), Value::Int(0)),
        (Arc::from("lflag"), Value::Int(0)),
        (Arc::from("ispeed"), Value::Int(0)),
        (Arc::from("ospeed"), Value::Int(0)),
        (Arc::from("echo"), Value::Bool(false)),
        (Arc::from("raw"), Value::Bool(true)),
        (Arc::from("crnl"), Value::Bool(false)),
        (Arc::from("control_chars"), Value::List(Vec::new())),
    ]))
}

fn validate_mknod_args(kind: &str, major: i64, minor: i64, span: Span) -> Result<(), RuntimeError> {
    if !matches!(kind, "block" | "char" | "fifo") {
        return Err(
            RuntimeError::new("linux-mknod", "kind must be `block`, `char`, or `fifo`")
                .with_span(span),
        );
    }
    if !(0..=u32::MAX as i64).contains(&major) || !(0..=u32::MAX as i64).contains(&minor) {
        return Err(RuntimeError::new(
            "linux-mknod",
            "major and minor must be between 0 and 4294967295",
        )
        .with_span(span));
    }
    Ok(())
}

fn validate_linux_sysctl_key(key: &str, span: Span) -> Result<(), RuntimeError> {
    if key.is_empty()
        || key.contains('\0')
        || key.contains("..")
        || key
            .split('.')
            .any(|part| part.is_empty() || part.contains('/'))
    {
        Err(RuntimeError::new("linux-sysctl", "invalid sysctl key").with_span(span))
    } else {
        Ok(())
    }
}

fn validate_linux_file_attrs_flags(flags: i64, span: Span) -> Result<(), RuntimeError> {
    if (0..=u32::MAX as i64).contains(&flags) {
        Ok(())
    } else {
        Err(
            RuntimeError::new("linux-file-attrs", "flags must be between 0 and 4294967295")
                .with_span(span),
        )
    }
}

fn validate_linux_file_version(version: i64, span: Span) -> Result<(), RuntimeError> {
    if (0..=u32::MAX as i64).contains(&version) {
        Ok(())
    } else {
        Err(RuntimeError::new(
            "linux-file-version",
            "version must be between 0 and 4294967295",
        )
        .with_span(span))
    }
}

fn linux_file_attrs_record(flags: i64) -> Value {
    const FS_SECRM_FL: i64 = 0x0000_0001;
    const FS_UNRM_FL: i64 = 0x0000_0002;
    const FS_COMPR_FL: i64 = 0x0000_0004;
    const FS_SYNC_FL: i64 = 0x0000_0008;
    const FS_IMMUTABLE_FL: i64 = 0x0000_0010;
    const FS_APPEND_FL: i64 = 0x0000_0020;
    const FS_NODUMP_FL: i64 = 0x0000_0040;
    const FS_NOATIME_FL: i64 = 0x0000_0080;
    const FS_INDEX_FL: i64 = 0x0000_1000;
    const FS_JOURNAL_DATA_FL: i64 = 0x0000_4000;
    const FS_NOTAIL_FL: i64 = 0x0000_8000;
    const FS_DIRSYNC_FL: i64 = 0x0001_0000;
    const FS_TOPDIR_FL: i64 = 0x0002_0000;

    Value::Record(RecordMap::from([
        (Arc::from("flags"), Value::Int(flags)),
        (
            Arc::from("indexed_directory"),
            Value::Bool(flags & FS_INDEX_FL != 0),
        ),
        (
            Arc::from("secure_deletion"),
            Value::Bool(flags & FS_SECRM_FL != 0),
        ),
        (Arc::from("undelete"), Value::Bool(flags & FS_UNRM_FL != 0)),
        (Arc::from("sync"), Value::Bool(flags & FS_SYNC_FL != 0)),
        (
            Arc::from("dirsync"),
            Value::Bool(flags & FS_DIRSYNC_FL != 0),
        ),
        (
            Arc::from("immutable"),
            Value::Bool(flags & FS_IMMUTABLE_FL != 0),
        ),
        (
            Arc::from("append_only"),
            Value::Bool(flags & FS_APPEND_FL != 0),
        ),
        (Arc::from("no_dump"), Value::Bool(flags & FS_NODUMP_FL != 0)),
        (
            Arc::from("no_atime"),
            Value::Bool(flags & FS_NOATIME_FL != 0),
        ),
        (
            Arc::from("compression_requested"),
            Value::Bool(flags & FS_COMPR_FL != 0),
        ),
        (
            Arc::from("journaled_data"),
            Value::Bool(flags & FS_JOURNAL_DATA_FL != 0),
        ),
        (
            Arc::from("no_tailmerging"),
            Value::Bool(flags & FS_NOTAIL_FL != 0),
        ),
        (
            Arc::from("top_of_directory_hierarchies"),
            Value::Bool(flags & FS_TOPDIR_FL != 0),
        ),
    ]))
}

fn linux_dry_run_partition_table() -> Value {
    Value::Record(RecordMap::from([
        (Arc::from("label"), Value::Str("gpt".into())),
        (
            Arc::from("id"),
            Value::Str("00000000-0000-0000-0000-000000000003".into()),
        ),
        (Arc::from("sector_size"), Value::Int(512)),
        (
            Arc::from("partitions"),
            Value::List(vec![Value::Record(RecordMap::from([
                (Arc::from("index"), Value::Int(1)),
                (Arc::from("start"), Value::Int(2048)),
                (Arc::from("end"), Value::Int(4095)),
                (Arc::from("size"), Value::Int(2048)),
                (Arc::from("type"), Value::Str("linux".into())),
                (
                    Arc::from("uuid"),
                    Value::Str("00000000-0000-0000-0000-000000000004".into()),
                ),
                (Arc::from("name"), Value::Str("root".into())),
            ]))]),
        ),
    ]))
}

fn linux_dry_run_uevent() -> Value {
    Value::Record(RecordMap::from([
        (Arc::from("action"), Value::Str("add".into())),
        (Arc::from("subsystem"), Value::Str("block".into())),
        (Arc::from("devname"), Value::Str("sda".into())),
        (
            Arc::from("devpath"),
            Value::Str("/devices/virtual/block/sda".into()),
        ),
        (
            Arc::from("env"),
            Value::List(vec![
                Value::Record(RecordMap::from([
                    (Arc::from("name"), Value::Str("ACTION".into())),
                    (Arc::from("value"), Value::Str("add".into())),
                ])),
                Value::Record(RecordMap::from([
                    (Arc::from("name"), Value::Str("SUBSYSTEM".into())),
                    (Arc::from("value"), Value::Str("block".into())),
                ])),
                Value::Record(RecordMap::from([
                    (Arc::from("name"), Value::Str("DEVNAME".into())),
                    (Arc::from("value"), Value::Str("sda".into())),
                ])),
                Value::Record(RecordMap::from([
                    (Arc::from("name"), Value::Str("DEVPATH".into())),
                    (
                        Arc::from("value"),
                        Value::Str("/devices/virtual/block/sda".into()),
                    ),
                ])),
            ]),
        ),
    ]))
}

#[derive(Default)]
struct DryRunUeventStream {
    emitted: bool,
}

impl LiveStream for DryRunUeventStream {
    fn next(&mut self, _span: Span) -> Result<Option<Value>, RuntimeError> {
        if self.emitted {
            return Ok(None);
        }
        self.emitted = true;
        Ok(Some(linux_dry_run_uevent()))
    }
}

fn lowered_runtime_list_result(
    result: Result<Vec<Value>, RuntimeError>,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    // Only callers whose declared result is List may cross this boundary. A
    // declared Stream must stay a Value::Stream until its consumer pulls it.
    lowered_runtime_value(
        match result {
            Ok(values) => Value::ok(Value::List(values)),
            Err(error) => Value::err(Value::Error(Box::new(error))),
        },
        span,
    )
}

fn lowered_runtime_stream_result(
    result: Result<crate::runtime::value::StreamValue, RuntimeError>,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    lowered_runtime_value(
        match result {
            Ok(stream) => Value::ok(Value::stream(stream)),
            Err(error) => Value::err(Value::Error(Box::new(error))),
        },
        span,
    )
}

fn lowered_stream_from_values(values: Vec<LoweredValue>) -> LoweredValue {
    LoweredValue::Stream(Box::new(StreamValue::from_values_live(
        "linux.dry_run",
        values.into_iter().map(LoweredValue::into_value).collect(),
    )))
}

fn lowered_process_handle_list_arg(
    value: LoweredValue,
    operation: &str,
    span: Span,
) -> Result<Result<Vec<ProcessHandleValue>, RunError>, RuntimeError> {
    let LoweredValue::List(items) = value else {
        return Err(RuntimeError::new(
            "type-error",
            format!("{operation} expects List[ProcessHandle]"),
        )
        .with_span(span));
    };
    if items.is_empty() {
        return Ok(Err(RunError::new(
            "unknown",
            format!("{operation} requires at least one handle"),
        )
        .with_span(span)));
    }

    let mut handles = Vec::with_capacity(items.len());
    for item in items {
        let LoweredValue::ProcessHandle(handle) = item else {
            return Ok(Err(RunError::new(
                "unknown",
                format!("{operation} list items must be ProcessHandle"),
            )
            .with_span(span)));
        };
        handles.push(*handle);
    }
    Ok(Ok(handles))
}

fn lowered_process_wait_any_record(index: usize, pid: u32, status: ProcessStatus) -> LoweredValue {
    LoweredValue::Record(BTreeMap::from([
        (Arc::from("index"), LoweredValue::Int(index as i64)),
        (Arc::from("pid"), LoweredValue::Int(pid as i64)),
        (Arc::from("status"), LoweredValue::Status(Box::new(status))),
    ]))
}

fn lowered_process_run_error(error: RunError) -> LoweredValue {
    LoweredValue::ResultErr(Box::new(Value::RunError(Box::new(error))))
}

fn lowered_timeout_elapsed(deadline: Option<Instant>) -> bool {
    deadline.is_some_and(|deadline| Instant::now() >= deadline)
}

fn lowered_str_list_runtime_arg(
    value: LoweredValue,
    operation: &str,
    span: Span,
) -> Result<Vec<String>, RuntimeError> {
    let Value::List(items) = value.into_value() else {
        return Err(
            RuntimeError::new("type-error", format!("{operation} expected List[Str]"))
                .with_span(span),
        );
    };
    let mut strings = Vec::with_capacity(items.len());
    for item in items {
        match item {
            Value::Str(value) => strings.push(value.to_string()),
            other => {
                return Err(RuntimeError::new(
                    "type-error",
                    format!(
                        "{operation} expected List[Str], found {}",
                        other.type_name()
                    ),
                )
                .with_span(span));
            }
        }
    }
    Ok(strings)
}

fn lowered_record_runtime_arg(
    value: LoweredValue,
    operation: &str,
    span: Span,
) -> Result<RecordMap, RuntimeError> {
    match value.into_value() {
        Value::Record(record) => Ok(record),
        Value::FsEntry(entry) => entry.to_record_map().map_err(|error| error.with_span(span)),
        other => Err(RuntimeError::new(
            "type-error",
            format!("{operation} expected Record, found {}", other.type_name()),
        )
        .with_span(span)),
    }
}

fn lowered_pipeline_input(value: LoweredValue, span: Span) -> Result<LoweredValue, RuntimeError> {
    match value {
        LoweredValue::ResultOk(value) => Ok(*value),
        LoweredValue::ResultErr(value) => Err(runtime_error_from_value(*value, span)),
        value => Ok(value),
    }
}

fn lowered_error_message(value: &LoweredValue) -> String {
    match value {
        LoweredValue::Error(e) => match e.as_ref() {
            Value::Error(err) => err.message.clone(),
            _ => "error".to_string(),
        },
        LoweredValue::ResultErr(e) => match e.as_ref() {
            Value::Error(err) => err.message.clone(),
            _ => "error".to_string(),
        },
        _ => value.type_name().to_string(),
    }
}

fn lowered_pipeline_record_list(
    value: &LoweredValue,
    span: Span,
) -> Result<Vec<BTreeMap<Arc<str>, LoweredValue>>, RuntimeError> {
    match value {
        LoweredValue::List(items) => items
            .iter()
            .map(|item| match item {
                LoweredValue::Record(record) => Ok(record.clone()),
                LoweredValue::RecordVec(record) => Ok(record
                    .iter()
                    .map(|(key, value)| (Arc::<str>::from(key.as_str().as_str()), value.clone()))
                    .collect()),
                LoweredValue::Map(map) => Ok(map
                    .iter()
                    .map(|(k, v)| (Arc::from(k.as_str()), v.clone()))
                    .collect()),
                other => Err(RuntimeError::new(
                    "type-error",
                    format!("table.print expected Record, found {}", other.type_name()),
                )
                .with_span(span)),
            })
            .collect(),
        other => Err(RuntimeError::new(
            "type-error",
            format!("table.print expected List, found {}", other.type_name()),
        )
        .with_span(span)),
    }
}

fn lowered_table_print_value(value: &LoweredValue) -> String {
    match value {
        LoweredValue::Null => String::new(),
        LoweredValue::Unit => String::new(),
        LoweredValue::Bool(b) => {
            if *b {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        LoweredValue::Int(i) => i.to_string(),
        LoweredValue::Float(f) => f.format(),
        LoweredValue::Str(s) => s.to_string(),
        LoweredValue::StrView(v) => v.as_str().to_string(),
        LoweredValue::Path(p) => p.display(),
        LoweredValue::Duration(d) => format!("{}ms", d.millis),
        other => format!("<{}>", other.type_name()),
    }
}

fn lowered_value_argv_len(value: &LoweredValue) -> usize {
    match value {
        LoweredValue::Str(s) => s.len(),
        LoweredValue::StrView(v) => v.as_str().len(),
        LoweredValue::Path(p) => p.display().len(),
        LoweredValue::Int(i) => i.to_string().len(),
        LoweredValue::Float(f) => f.format().len(),
        other => other.type_name().to_string().len(),
    }
}

fn lowered_bytes_or_str_owned(
    value: LoweredValue,
    operation: &str,
    span: Span,
) -> Result<Vec<u8>, RuntimeError> {
    if let Some(bytes) = lowered_bytes_value(&value) {
        return Ok(bytes.to_vec());
    }
    if let Some(text) = lowered_str_value(&value) {
        return Ok(text.as_bytes().to_vec());
    }
    Err(RuntimeError::new(
        "type-error",
        format!(
            "{operation} expected Bytes or Str, found {}",
            value.type_name()
        ),
    )
    .with_span(span))
}

fn lowered_path_list_arg(
    value: LoweredValue,
    operation: &str,
    span: Span,
) -> Result<Vec<PathValue>, RuntimeError> {
    let items = match value {
        LoweredValue::List(items) => items,
        LoweredValue::SharedList(items) => items.iter().cloned().collect(),
        _ => {
            return Err(RuntimeError::new(
                "type-error",
                format!("{operation} expected List[Path]"),
            )
            .with_span(span));
        }
    };
    let mut paths = Vec::with_capacity(items.len());
    for item in items {
        let LoweredValue::Path(path) = item else {
            return Err(RuntimeError::new(
                "type-error",
                format!("{operation} expected List[Path]"),
            )
            .with_span(span));
        };
        paths.push(path);
    }
    Ok(paths)
}

fn lowered_env_key_arg(
    value: Option<LoweredValue>,
    span: Span,
) -> Result<Result<String, LoweredValue>, RuntimeError> {
    let key = lowered_str_arg_owned(value, "", "env", span)?;
    if key.is_empty() || key.contains('\0') || key.contains('=') {
        Ok(Err(lowered_result_err_value(
            RuntimeError::new(
                "env-name",
                "environment names cannot be empty or contain NUL or `=`",
            )
            .with_span(span),
        )))
    } else {
        Ok(Ok(key))
    }
}

fn lowered_root_id(root: &LoweredValue, span: Span) -> Result<i64, RuntimeError> {
    let LoweredValue::Record(record) = root else {
        return Err(RuntimeError::new("type-error", "fs root expected Record").with_span(span));
    };
    match record.get("id") {
        Some(LoweredValue::Int(id)) => Ok(*id),
        Some(value) => Err(RuntimeError::new(
            "type-error",
            format!("expected `id` to be Int, found {}", value.type_name()),
        )
        .with_span(span)),
        None => Err(RuntimeError::new("fs-root", "missing `id` field").with_span(span)),
    }
}

fn fs_root_record(id: i64) -> LoweredValue {
    LoweredValue::Record(BTreeMap::from([(Arc::from("id"), LoweredValue::Int(id))]))
}

fn lowered_filesystem_stats_record(stats: fs_module::FilesystemStats) -> LoweredValue {
    LoweredValue::Record(BTreeMap::from([
        (
            Arc::from("blocks_1k"),
            LoweredValue::Int(stats.blocks_1k as i64),
        ),
        (
            Arc::from("used_1k"),
            LoweredValue::Int(stats.used_1k as i64),
        ),
        (
            Arc::from("available_1k"),
            LoweredValue::Int(stats.available_1k as i64),
        ),
        (
            Arc::from("capacity_percent"),
            LoweredValue::Int(stats.capacity_percent as i64),
        ),
    ]))
}

fn lowered_fs_mount_record(mount: fs_module::FsMount) -> Result<LoweredValue, RuntimeError> {
    let mounted_on = path_value_from_pathbuf(mount.mounted_on)?;
    Ok(LoweredValue::Record(BTreeMap::from([
        (
            Arc::from("filesystem"),
            LoweredValue::Str(mount.filesystem.into()),
        ),
        (Arc::from("mounted_on"), LoweredValue::Path(mounted_on)),
        (Arc::from("fstype"), LoweredValue::Str(mount.fstype.into())),
        (
            Arc::from("blocks_1k"),
            LoweredValue::Int(mount.blocks_1k as i64),
        ),
        (
            Arc::from("used_1k"),
            LoweredValue::Int(mount.used_1k as i64),
        ),
        (
            Arc::from("available_1k"),
            LoweredValue::Int(mount.available_1k as i64),
        ),
        (
            Arc::from("capacity_percent"),
            LoweredValue::Int(mount.capacity_percent as i64),
        ),
        (Arc::from("files"), LoweredValue::Int(mount.files as i64)),
        (
            Arc::from("files_used"),
            LoweredValue::Int(mount.files_used as i64),
        ),
        (
            Arc::from("files_free"),
            LoweredValue::Int(mount.files_free as i64),
        ),
        (
            Arc::from("files_capacity_percent"),
            LoweredValue::Int(mount.files_capacity_percent as i64),
        ),
        (Arc::from("readonly"), LoweredValue::Bool(mount.readonly)),
    ])))
}

fn lowered_status_segment_record(segment: &ProcessSegmentStatus) -> LoweredValue {
    LoweredValue::Record(BTreeMap::from([
        (Arc::from("index"), LoweredValue::Int(segment.index as i64)),
        (
            Arc::from("target"),
            LoweredValue::Str(String::from_utf8_lossy(&segment.target).as_ref().into()),
        ),
        (Arc::from("success"), LoweredValue::Bool(segment.success)),
        (Arc::from("ok"), LoweredValue::Bool(segment.success)),
        (
            Arc::from("kind"),
            LoweredValue::Str(format!("{:?}", segment.kind).to_lowercase().into()),
        ),
        (
            Arc::from("code"),
            segment
                .code
                .map_or(LoweredValue::Null, |code| LoweredValue::Int(code as i64)),
        ),
        (
            Arc::from("error_kind"),
            segment
                .error_kind
                .as_ref()
                .map_or(LoweredValue::Null, |kind| {
                    LoweredValue::Str(kind.as_str().into())
                }),
        ),
        (
            Arc::from("error_message"),
            segment
                .error_message
                .as_ref()
                .map_or(LoweredValue::Null, |message| {
                    LoweredValue::Str(message.as_str().into())
                }),
        ),
    ]))
}

fn lowered_fs_root_dir<'a>(
    roots: &'a [Option<FsRootHandle>],
    root: &LoweredValue,
    span: Span,
) -> Result<&'a Root, RuntimeError> {
    let id = lowered_root_id(root, span)?;
    let Some(slot) = id
        .checked_sub(1)
        .and_then(|index| usize::try_from(index).ok())
        .and_then(|index| roots.get(index))
    else {
        return Err(RuntimeError::new("fs-root", "root handle is not active").with_span(span));
    };
    slot.as_ref()
        .map(FsRootHandle::root)
        .ok_or_else(|| RuntimeError::new("fs-root", "root handle is not active").with_span(span))
}

fn read_link_path(path: &Path) -> std::io::Result<PathBuf> {
    rustix::fs::readlink(path, Vec::new())
        .map(|target| PathBuf::from(std::ffi::OsString::from_vec(target.as_bytes().to_vec())))
        .map_err(std::io::Error::from)
}

fn root_path_from_dir(dir: &Root, span: Span) -> Result<PathValue, RuntimeError> {
    let fd = dir.as_fd().as_raw_fd();
    #[cfg(target_os = "macos")]
    {
        let mut buffer = [0 as libc::c_char; libc::PATH_MAX as usize];
        let result = unsafe { libc::fcntl(fd, libc::F_GETPATH, buffer.as_mut_ptr()) };
        if result == 0 {
            let path = unsafe { CStr::from_ptr(buffer.as_ptr()) };
            let path = PathBuf::from(std::ffi::OsStr::from_bytes(path.to_bytes()));
            if path.is_dir() {
                return path_value_from_pathbuf(path).map_err(|error| error.with_span(span));
            }
        }
    }
    let candidates = [
        PathBuf::from(format!("/proc/self/fd/{fd}")),
        PathBuf::from(format!("/dev/fd/{fd}")),
    ];
    let mut last_error = None;
    for candidate in candidates {
        match read_link_path(&candidate) {
            Ok(path) if path.is_dir() => {
                return path_value_from_pathbuf(path).map_err(|error| error.with_span(span));
            }
            Ok(_) => {
                last_error = Some(format!("{} is not a directory", candidate.display()));
            }
            Err(error) => {
                last_error = Some(error.to_string());
            }
        }
    }
    Err(RuntimeError::new(
        "fs-root-path",
        last_error.unwrap_or_else(|| "root path is unavailable on this platform".to_string()),
    )
    .with_span(span))
}

pub(super) fn new_temp_fs_root(
    operation: &'static str,
    span: Span,
) -> Result<FsRootHandle, RuntimeError> {
    let temp = TempDir::new()
        .map_err(|error| RuntimeError::new(operation, error.to_string()).with_span(span))?;
    let root = Root::open(temp.path())
        .map_err(|error| RuntimeError::new(operation, error.to_string()).with_span(span))?;
    Ok(FsRootHandle::TempDir { root, _temp: temp })
}

fn lowered_count_key(value: &LoweredValue, span: Span) -> Result<String, RuntimeError> {
    if let Some(text) = lowered_str_value(value) {
        return Ok(text.to_string());
    }
    match value {
        LoweredValue::Int(value) => Ok(value.to_string()),
        LoweredValue::Bool(value) => Ok(value.to_string()),
        other => Err(RuntimeError::new(
            "type-error",
            format!(
                "count key must be Str, Int, or Bool, found {}",
                other.type_name()
            ),
        )
        .with_span(span)),
    }
}

fn lowered_rest_index(lowered: &FunctionHeader) -> Option<usize> {
    lowered.param_rest.iter().position(|rest| *rest)
}

fn lowered_required_arg_count(lowered: &FunctionHeader) -> usize {
    let limit = lowered_rest_index(lowered).unwrap_or(lowered.params.len());
    lowered
        .param_defaults
        .iter()
        .take(limit)
        .filter(|default| default.is_none())
        .count()
}

fn lowered_call_arity_message(lowered: &FunctionHeader, actual: usize) -> String {
    let required = lowered_required_arg_count(lowered);
    if let Some(rest_index) = lowered_rest_index(lowered) {
        format!(
            "expected at least {required} and up to rest parameter {rest_index} arguments, found {actual}"
        )
    } else if required == lowered.params.len() {
        format!(
            "expected {} arguments, found {actual}",
            lowered.params.len()
        )
    } else {
        format!(
            "expected {required} to {} arguments, found {actual}",
            lowered.params.len()
        )
    }
}

fn lowered_splice_arg_items(
    value: LoweredValue,
    span: Span,
) -> Result<Vec<LoweredValue>, RuntimeError> {
    match value {
        LoweredValue::List(items) => Ok(items),
        other => Err(RuntimeError::new(
            "type-error",
            format!("`@` expected List, found {}", other.type_name()),
        )
        .with_span(span)),
    }
}

fn bind_lowered_comp_target(
    target: &LoweredCompTarget,
    value: LoweredValue,
    slots: &mut [LoweredValue],
    span: Span,
) -> Result<(), RuntimeError> {
    match target {
        LoweredCompTarget::Slot(slot) => {
            slots[*slot] = value;
            Ok(())
        }
        LoweredCompTarget::Record { fields } => {
            for (name, slot, field_span) in fields {
                let value = match &value {
                    LoweredValue::Record(record) => {
                        let name_text = name.as_str();
                        record.get::<str>(name_text.as_str()).cloned()
                    }
                    LoweredValue::RecordVec(record) => {
                        let name_text = name.as_str();
                        lowered_record_vec_get(record, name_text.as_str()).cloned()
                    }
                    _ => {
                        return Err(RuntimeError::new(
                            "type-error",
                            "record destructuring requires a record value",
                        )
                        .with_span(span));
                    }
                }
                .ok_or_else(|| {
                    RuntimeError::new(
                        "missing-field",
                        format!("missing destructured field `{name}`"),
                    )
                    .with_span(*field_span)
                })?;
                slots[*slot] = value;
            }
            Ok(())
        }
    }
}

fn lowered_param_check(lowered: &FunctionHeader, index: usize) -> Option<&super::LoweredTypeCheck> {
    lowered.param_checks.get(index).and_then(Option::as_ref)
}

fn lowered_runtime_arg_matches_param(
    lowered: &FunctionHeader,
    index: usize,
    value: &Value,
) -> bool {
    lowered_param_check(lowered, index)
        .is_none_or(|check| value_matches_static_type(value, &check.ty))
}

fn lowered_value_matches_param(
    lowered: &FunctionHeader,
    index: usize,
    kind: LoweredType,
    value: &LoweredValue,
) -> bool {
    lowered_value_matches(kind, value)
        && lowered_param_check(lowered, index)
            .is_none_or(|check| lowered_value_matches_static_type(value, &check.ty))
}

fn lowered_param_type_name(lowered: &FunctionHeader, index: usize, kind: LoweredType) -> &str {
    lowered_param_check(lowered, index).map_or_else(|| lowered_type_name(kind), |check| &check.name)
}

impl Evaluator {
    fn lowered_stream_list_result(
        &mut self,
        result: Result<StreamValue, RuntimeError>,
        span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        let stream = match result {
            Ok(stream) => stream,
            Err(error) => return Ok(lowered_result_err_value(error)),
        };
        let values = self.collect_stream_values(stream, span)?;
        lowered_runtime_value(Value::ok(Value::List(values)), span)
    }

    fn collect_lowered_stream_values(
        &mut self,
        mut stream: StreamValue,
        span: Span,
    ) -> Result<Vec<LoweredValue>, RuntimeError> {
        // Materialize pre-collected items first, then drain live items.
        // This avoids a separate intermediate Vec<Value> by converting each
        // item to LoweredValue as it arrives.
        let mut lowered: Vec<LoweredValue> = std::mem::take(&mut stream.items)
            .into_iter()
            .map(|item| item.value)
            .filter_map(|v| lowered_value_from_runtime_any(&v))
            .collect();
        if stream.source.is_some() {
            while let Some(value) = stream.next_live(span)? {
                let Some(item) = lowered_value_from_runtime_any(&value) else {
                    return Err(RuntimeError::new(
                        "type-error",
                        format!("stream produced unsupported {}", value.type_name()),
                    )
                    .with_span(span));
                };
                lowered.push(item);
            }
        }
        Ok(lowered)
    }

    fn lowered_list_items(
        &mut self,
        value: LoweredValue,
        span: Span,
        message: &'static str,
    ) -> Result<Vec<LoweredValue>, RuntimeError> {
        match value {
            LoweredValue::List(items) => Ok(items),
            LoweredValue::SharedList(items) => match Arc::try_unwrap(items) {
                Ok(items) => Ok(items),
                Err(items) => Ok(items.as_ref().clone()),
            },
            LoweredValue::Stream(stream) => self.collect_lowered_stream_values(*stream, span),
            LoweredValue::ResultOk(value) => self.lowered_list_items(*value, span, message),
            LoweredValue::ResultErr(value) => Err(runtime_error_from_value(*value, span)),
            _ => Err(RuntimeError::new("type-error", message).with_span(span)),
        }
    }

    fn lowered_pipeline_input_items(
        &mut self,
        value: LoweredValue,
        span: Span,
    ) -> Result<Vec<LoweredValue>, RuntimeError> {
        let value = lowered_pipeline_input(value, span)?;
        self.lowered_list_items(value, span, "pipeline input expected List")
    }

    /// Process stream-compatible pipeline stages lazily from a Stream input.
    /// Map, Where, and FlatMap stages are applied per-item as they arrive from
    /// the stream, avoiding intermediate Vecs. ParMapBlock feeds directly from
    /// the stream into parallel workers. When a stage that can't stream is
    /// reached, remaining items are collected into a Vec.
    ///
    /// Returns (new_current, stage_count_consumed).
    fn push_lowered_fs_root(&mut self, root: FsRootHandle) -> LoweredValue {
        let id = self.fs_roots.len() as i64 + 1;
        self.fs_roots.push(Some(root));
        fs_root_record(id)
    }

    fn lowered_create_temp_file_root(&mut self, span: Span) -> Result<LoweredValue, RuntimeError> {
        let root = new_temp_fs_root("fs-temp-file", span)?;
        root.root()
            .create("file")
            .and_then(|mut file| file.flush())
            .map_err(|error| {
                RuntimeError::new("fs-temp-file", error.to_string()).with_span(span)
            })?;
        let root = self.push_lowered_fs_root(root);
        let path = PathValue::new(b"file".to_vec()).map_err(|error| error.with_span(span))?;
        Ok(LoweredValue::Record(BTreeMap::from([
            (Arc::from("root"), root),
            (Arc::from("path"), LoweredValue::Path(path)),
        ])))
    }

    fn lowered_project_root(
        &mut self,
        kind: &str,
        qualifier: &str,
        organization: &str,
        application: &str,
        span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        let dirs = ProjectDirs::from(qualifier, organization, application).ok_or_else(|| {
            RuntimeError::new("fs-dir", "project directories are unavailable").with_span(span)
        })?;
        let path = match kind {
            "cache" => dirs.cache_dir(),
            "config" => dirs.config_dir(),
            "data" => dirs.data_dir(),
            "data_local" => dirs.data_local_dir(),
            "runtime" => dirs.runtime_dir().ok_or_else(|| {
                RuntimeError::new("fs-dir", "runtime directory is unavailable").with_span(span)
            })?,
            "state" => dirs.state_dir().ok_or_else(|| {
                RuntimeError::new("fs-dir", "state directory is unavailable").with_span(span)
            })?,
            _ => {
                return Err(RuntimeError::new(
                    "fs-dir",
                    format!("unknown project directory kind `{kind}`"),
                )
                .with_span(span));
            }
        };
        std::fs::create_dir_all(path)
            .map_err(|error| RuntimeError::new("fs-dir", error.to_string()).with_span(span))?;
        let root = Root::open(path)
            .map_err(|error| RuntimeError::new("fs-dir", error.to_string()).with_span(span))?;
        Ok(self.push_lowered_fs_root(FsRootHandle::Dir(root)))
    }

    fn lowered_user_root(&mut self, kind: &str, span: Span) -> Result<LoweredValue, RuntimeError> {
        let dirs = UserDirs::new().ok_or_else(|| {
            RuntimeError::new("fs-dir", "user directories are unavailable").with_span(span)
        })?;
        let path = match kind {
            "home" => Some(dirs.home_dir()),
            "audio" => dirs.audio_dir(),
            "desktop" => dirs.desktop_dir(),
            "documents" => dirs.document_dir(),
            "downloads" => dirs.download_dir(),
            "fonts" => dirs.font_dir(),
            "pictures" => dirs.picture_dir(),
            "public" => dirs.public_dir(),
            "templates" => dirs.template_dir(),
            "videos" => dirs.video_dir(),
            _ => {
                return Err(RuntimeError::new(
                    "fs-dir",
                    format!("unknown user directory kind `{kind}`"),
                )
                .with_span(span));
            }
        }
        .ok_or_else(|| {
            RuntimeError::new("fs-dir", "user directory is unavailable").with_span(span)
        })?;
        let root = Root::open(path)
            .map_err(|error| RuntimeError::new("fs-dir", error.to_string()).with_span(span))?;
        Ok(self.push_lowered_fs_root(FsRootHandle::Dir(root)))
    }

    fn eval_lowered_module_call_values(
        &mut self,
        op: RuntimeOp,
        mut values: Vec<LoweredValue>,
        span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = match op {
            RuntimeOp::CpuCount if values.is_empty() => {
                LoweredValue::Int(crate::modules::cpu::count())
            }
            RuntimeOp::AppletHashPassword if values.len() == 2 => {
                let algorithm =
                    lowered_str_arg_owned(values.pop(), "", "applet.hash_password", span)?;
                let password =
                    lowered_str_arg_owned(values.pop(), "", "applet.hash_password", span)?;
                match auth_module::hash_password(&password, &algorithm) {
                    Ok(hash) => lowered_result_ok(LoweredValue::Str(hash.into())),
                    Err(message) => lowered_result_err_value(
                        RuntimeError::new("applet-hash-password", message).with_span(span),
                    ),
                }
            }
            RuntimeOp::AppletVerifyPassword if values.len() == 2 => {
                let hash = lowered_str_arg_owned(values.pop(), "", "applet.verify_password", span)?;
                let password =
                    lowered_str_arg_owned(values.pop(), "", "applet.verify_password", span)?;
                LoweredValue::Bool(auth_module::verify_password(&password, &hash))
            }
            RuntimeOp::AppletCurrentEuid if values.is_empty() => {
                LoweredValue::Int(rustix::process::geteuid().as_raw() as i64)
            }
            RuntimeOp::AppletCurrentExe if values.is_empty() => match std::env::current_exe() {
                Ok(path) => match path_value_from_pathbuf(path) {
                    Ok(path) => lowered_result_ok(LoweredValue::Path(path)),
                    Err(error) => lowered_result_err_value(error.with_span(span)),
                },
                Err(error) => lowered_result_err_value(
                    RuntimeError::new("applet.current_exe", error.to_string()).with_span(span),
                ),
            },
            RuntimeOp::AppletLoginSession if values.len() == 3 => {
                let host = lowered_str_arg_owned(values.pop(), "", "applet.login_session", span)?;
                let preserve_env =
                    lowered_bool_arg_or(values.pop(), false, "applet.login_session", span)?;
                let record = lowered_record_arg(values.pop(), "applet.login_session", span)?;
                let home = self.host_path(&record_path(&record, "home", span)?);
                let user = lowered_session_user_record(record, home, span)?;
                match auth_module::login_session(&user, preserve_env, &host) {
                    Ok(code) => lowered_result_ok(LoweredValue::Int(i64::from(code))),
                    Err(error) => lowered_result_err_value(
                        RuntimeError::new("applet.login_session", error.to_string())
                            .with_span(span),
                    ),
                }
            }
            RuntimeOp::AppletSuSession if values.len() == 6 => {
                let extra_args = lowered_str_list_arg(values.pop(), "applet.su_session", span)?;
                let command = lowered_str_arg_owned(values.pop(), "", "applet.su_session", span)?;
                let shell = lowered_str_arg_owned(values.pop(), "", "applet.su_session", span)?;
                let preserve_env =
                    lowered_bool_arg_or(values.pop(), false, "applet.su_session", span)?;
                let login = lowered_bool_arg_or(values.pop(), false, "applet.su_session", span)?;
                let record = lowered_record_arg(values.pop(), "applet.su_session", span)?;
                let home = self.host_path(&record_path(&record, "home", span)?);
                let user = lowered_session_user_record(record, home, span)?;
                match auth_module::su_session(
                    &user,
                    login,
                    preserve_env,
                    &shell,
                    &command,
                    &extra_args,
                ) {
                    Ok(code) => lowered_result_ok(LoweredValue::Int(i64::from(code))),
                    Err(error) => lowered_result_err_value(
                        RuntimeError::new("applet.su_session", error.to_string()).with_span(span),
                    ),
                }
            }
            RuntimeOp::AppletSuloginSession if values.len() == 1 => {
                let record = lowered_record_arg(values.pop(), "applet.sulogin_session", span)?;
                let home = self.host_path(&record_path(&record, "home", span)?);
                let user = lowered_session_user_record(record, home, span)?;
                match auth_module::sulogin_session(&user) {
                    Ok(code) => lowered_result_ok(LoweredValue::Int(i64::from(code))),
                    Err(error) => lowered_result_err_value(
                        RuntimeError::new("applet.sulogin_session", error.to_string())
                            .with_span(span),
                    ),
                }
            }
            RuntimeOp::AppletMdev if values.len() == 1 => {
                let argv = lowered_str_list_arg(values.pop(), "applet.mdev", span)?
                    .into_iter()
                    .map(OsString::from)
                    .collect::<Vec<_>>();
                #[cfg(target_os = "linux")]
                {
                    lowered_result_ok(LoweredValue::Int(i64::from(xsh_applets::mdev::main_args(
                        &argv,
                    ))))
                }
                #[cfg(not(target_os = "linux"))]
                {
                    let _ = argv;
                    lowered_result_err_value(
                        RuntimeError::new("applet-platform", "mdev is only available on Linux")
                            .with_span(span),
                    )
                }
            }
            RuntimeOp::ArchiveCompress if (2..=5).contains(&values.len()) => {
                let overwrite =
                    lowered_bool_arg_or(values.get(4).cloned(), false, "archive.compress", span)?;
                let level =
                    lowered_int_arg_or(values.get(3).cloned(), 6, "archive.compress", span)?;
                let format = lowered_str_arg_owned(
                    values.get(2).cloned(),
                    "auto",
                    "archive.compress",
                    span,
                )?;
                let dest = lowered_path_arg(values.remove(1), "archive.compress", span)?;
                let source = lowered_path_arg(values.remove(0), "archive.compress", span)?;
                lowered_unit_result(archive_module::compress_file(
                    self.host_path(&source),
                    self.host_path(&dest),
                    &format,
                    level,
                    overwrite,
                    span,
                ))
            }
            RuntimeOp::ArchiveDecompress if (2..=4).contains(&values.len()) => {
                let overwrite =
                    lowered_bool_arg_or(values.get(3).cloned(), false, "archive.decompress", span)?;
                let format = lowered_str_arg_owned(
                    values.get(2).cloned(),
                    "auto",
                    "archive.decompress",
                    span,
                )?;
                let dest = lowered_path_arg(values.remove(1), "archive.decompress", span)?;
                let source = lowered_path_arg(values.remove(0), "archive.decompress", span)?;
                lowered_unit_result(archive_module::decompress_file(
                    self.host_path(&source),
                    self.host_path(&dest),
                    &format,
                    overwrite,
                    span,
                ))
            }
            RuntimeOp::ArchiveCpioCreate if values.len() == 3 || values.len() == 4 => {
                let overwrite = lowered_bool_arg_or(
                    values.get(3).cloned(),
                    false,
                    "archive.cpio_create",
                    span,
                )?;
                let entries = lowered_path_list_arg(values.remove(2), "archive.cpio_create", span)?;
                let root = lowered_path_arg(values.remove(1), "archive.cpio_create", span)?;
                let path = lowered_path_arg(values.remove(0), "archive.cpio_create", span)?;
                lowered_unit_result(archive_module::cpio_create(
                    self.host_path(&path),
                    self.host_path(&root),
                    entries,
                    overwrite,
                    span,
                ))
            }
            RuntimeOp::ArchiveCpioExtract if values.len() == 2 || values.len() == 3 => {
                let overwrite = lowered_bool_arg_or(
                    values.get(2).cloned(),
                    false,
                    "archive.cpio_extract",
                    span,
                )?;
                let dest = lowered_path_arg(values.remove(1), "archive.cpio_extract", span)?;
                let path = lowered_path_arg(values.remove(0), "archive.cpio_extract", span)?;
                lowered_unit_result(archive_module::cpio_extract(
                    self.host_path(&path),
                    self.host_path(&dest),
                    overwrite,
                    span,
                ))
            }
            RuntimeOp::ArchiveCpioList if values.len() == 1 => {
                let path = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "archive.cpio_list",
                    span,
                )?;
                lowered_runtime_stream_result(
                    archive_module::cpio_list(self.host_path(&path), span),
                    span,
                )?
            }
            RuntimeOp::ArchiveDecompressBytes if values.len() == 1 || values.len() == 2 => {
                let format = lowered_str_arg_owned(
                    values.get(1).cloned(),
                    "auto",
                    "archive.decompress_bytes",
                    span,
                )?;
                let path = lowered_path_arg(values.remove(0), "archive.decompress_bytes", span)?;
                lowered_runtime_result(
                    archive_module::decompress_bytes(self.host_path(&path), &format, span)
                        .map(Value::Bytes),
                    span,
                )?
            }
            RuntimeOp::ArchiveTarCreate if (3..=5).contains(&values.len()) => {
                let overwrite =
                    lowered_bool_arg_or(values.get(4).cloned(), false, "archive.tar_create", span)?;
                let compression = lowered_str_arg_owned(
                    values.get(3).cloned(),
                    "auto",
                    "archive.tar_create",
                    span,
                )?;
                let entries = lowered_path_list_arg(values.remove(2), "archive.tar_create", span)?;
                let root = lowered_path_arg(values.remove(1), "archive.tar_create", span)?;
                let path = lowered_path_arg(values.remove(0), "archive.tar_create", span)?;
                lowered_unit_result(archive_module::tar_create(
                    self.host_path(&path),
                    self.host_path(&root),
                    entries,
                    &compression,
                    overwrite,
                    span,
                ))
            }
            RuntimeOp::ArchiveTarExtract if (2..=6).contains(&values.len()) => {
                let members = if values.get(5).is_some() {
                    lowered_path_list_arg(values.remove(5), "archive.tar_extract", span)?
                } else {
                    Vec::new()
                };
                let overwrite = lowered_bool_arg_or(
                    values.get(4).cloned(),
                    false,
                    "archive.tar_extract",
                    span,
                )?;
                let compression = lowered_str_arg_owned(
                    values.get(3).cloned(),
                    "auto",
                    "archive.tar_extract",
                    span,
                )?;
                let strip_components =
                    lowered_int_arg_or(values.get(2).cloned(), 0, "archive.tar_extract", span)?;
                let dest = lowered_path_arg(values.remove(1), "archive.tar_extract", span)?;
                let path = lowered_path_arg(values.remove(0), "archive.tar_extract", span)?;
                lowered_unit_result(archive_module::tar_extract(
                    self.host_path(&path),
                    self.host_path(&dest),
                    strip_components,
                    &compression,
                    overwrite,
                    members,
                    span,
                ))
            }
            RuntimeOp::ArchiveTarList if (1..=3).contains(&values.len()) => {
                let members = if values.get(2).is_some() {
                    lowered_path_list_arg(values.remove(2), "archive.tar_list", span)?
                } else {
                    Vec::new()
                };
                let compression = lowered_str_arg_owned(
                    values.get(1).cloned(),
                    "auto",
                    "archive.tar_list",
                    span,
                )?;
                let path = lowered_path_arg(values.remove(0), "archive.tar_list", span)?;
                match archive_module::tar_list(self.host_path(&path), &compression, members, span) {
                    Ok(stream) => lowered_result_ok(LoweredValue::Stream(Box::new(stream))),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::ArchiveZipExtract if values.len() == 2 || values.len() == 3 => {
                let overwrite = lowered_bool_arg_or(
                    values.get(2).cloned(),
                    false,
                    "archive.zip_extract",
                    span,
                )?;
                let dest = lowered_path_arg(values.remove(1), "archive.zip_extract", span)?;
                let path = lowered_path_arg(values.remove(0), "archive.zip_extract", span)?;
                lowered_unit_result(archive_module::zip_extract(
                    self.host_path(&path),
                    self.host_path(&dest),
                    overwrite,
                    span,
                ))
            }
            RuntimeOp::ArchiveZipList if values.len() == 1 => {
                let path = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "archive.zip_list",
                    span,
                )?;
                lowered_runtime_stream_result(
                    archive_module::zip_list(self.host_path(&path), span),
                    span,
                )?
            }
            RuntimeOp::CliParse if values.len() == 2 || values.len() == 3 => {
                let command = lowered_str_arg_owned(
                    values.get(2).cloned(),
                    &self.command_name,
                    "cli.parse",
                    span,
                )?;
                let schema = lowered_record_runtime_arg(values.remove(1), "cli.parse", span)?;
                let argv = lowered_str_list_runtime_arg(values.remove(0), "cli.parse", span)?;
                lowered_module_result_value(
                    cli_module::parse_cli(argv, schema, &command, span),
                    span,
                )?
            }
            RuntimeOp::CliApplet if values.len() == 2 || values.len() == 3 => {
                let command = lowered_str_arg_owned(
                    values.get(2).cloned(),
                    &self.command_name,
                    "cli.applet",
                    span,
                )?;
                let schema = lowered_record_runtime_arg(values.remove(1), "cli.applet", span)?;
                let argv = lowered_str_list_runtime_arg(values.remove(0), "cli.applet", span)?;
                lowered_module_result_value(
                    cli_module::parse_cli_applet(argv, schema, &command, span),
                    span,
                )?
            }
            RuntimeOp::CliParseFull if (2..=4).contains(&values.len()) => {
                let command = lowered_str_arg_owned(
                    values.get(3).cloned(),
                    &self.command_name,
                    "cli.parse_full",
                    span,
                )?;
                let env = match values.get(2).cloned() {
                    Some(value) => lowered_record_runtime_arg(value, "cli.parse_full", span)?,
                    None => RecordMap::new(),
                };
                let schema = lowered_record_runtime_arg(values.remove(1), "cli.parse_full", span)?;
                let argv = lowered_str_list_runtime_arg(values.remove(0), "cli.parse_full", span)?;
                lowered_module_result_value(
                    cli_module::parse_cli_full(argv, schema, env, &command, span),
                    span,
                )?
            }
            RuntimeOp::CliCommands
                if values.len() == 2 || values.len() == 3 || values.len() == 4 =>
            {
                let argv = lowered_str_list_runtime_arg(values.remove(0), "cli.commands", span)?;
                let (rootless_default, commands, fallback_command) = if values.len() == 1 {
                    (
                        String::new(),
                        lowered_record_runtime_arg(values.remove(0), "cli.commands", span)?,
                        None,
                    )
                } else {
                    let rootless_default =
                        lowered_str_arg_owned(Some(values.remove(0)), "", "cli.commands", span)?;
                    let commands =
                        lowered_record_runtime_arg(values.remove(0), "cli.commands", span)?;
                    let fallback_command = match values.pop() {
                        Some(value) => {
                            Some(lowered_record_runtime_arg(value, "cli.commands", span)?)
                        }
                        None => None,
                    };
                    (rootless_default, commands, fallback_command)
                };
                lowered_module_result_value(
                    cli_module::parse_commands(
                        argv,
                        rootless_default,
                        commands,
                        fallback_command,
                        span,
                    ),
                    span,
                )?
            }
            RuntimeOp::CliTokens if values.len() == 1 || values.len() == 2 => {
                let value_flags = match values.get(1).cloned() {
                    Some(value) => lowered_str_list_runtime_arg(value, "cli.tokens", span)?,
                    None => Vec::new(),
                };
                let argv = lowered_str_list_runtime_arg(values.remove(0), "cli.tokens", span)?;
                lowered_module_result_value(
                    cli_module::tokenize_flags(argv, value_flags, span),
                    span,
                )?
            }
            RuntimeOp::CliUsage if values.len() == 1 || values.len() == 2 => {
                let command =
                    lowered_str_arg_owned(values.get(1).cloned(), "command", "cli.usage", span)?;
                let schema = lowered_record_runtime_arg(values.remove(0), "cli.usage", span)?;
                lowered_runtime_value(cli_module::render_usage(schema, command, span)?, span)?
            }
            RuntimeOp::DiffUnified if values.len() == 2 || values.len() == 3 => {
                let context = match values.get(2).cloned() {
                    Some(value) => lowered_int_arg(Some(value), "diff.unified", span)?,
                    None => 3,
                };
                let modified = lowered_path_arg(values.remove(1), "diff.unified", span)?;
                let original = lowered_path_arg(values.remove(0), "diff.unified", span)?;
                match diff_module::unified(
                    self.host_path(&original),
                    self.host_path(&modified),
                    context,
                    span,
                ) {
                    Ok(value) => lowered_runtime_value(value, span)?,
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::ElfInspect if values.len() == 1 => {
                let path = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "elf.inspect",
                    span,
                )?;
                match elf_module::inspect_path(&self.host_path(&path), span) {
                    Ok(info) => lowered_result_ok(lowered_elf_info_value(path, info)),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::DnsLookup if (1..=4).contains(&values.len()) => {
                let timeout = match values.get(3).cloned() {
                    Some(value) => Duration::from_millis(
                        lowered_duration_arg(Some(value), "dns.lookup", span)?.millis,
                    ),
                    None => Duration::from_millis(5_000),
                };
                let server = lowered_str_arg_owned(values.get(2).cloned(), "", "dns.lookup", span)?;
                let record =
                    lowered_str_arg_owned(values.get(1).cloned(), "A", "dns.lookup", span)?;
                let name = lowered_str_arg_owned(values.first().cloned(), "", "dns.lookup", span)?;
                let args = RecordMap::from([
                    (Arc::from("name"), Value::Str(name.as_str().into())),
                    (Arc::from("record"), Value::Str(record.as_str().into())),
                    (Arc::from("server"), Value::Str(server.as_str().into())),
                    (
                        Arc::from("timeout_ms"),
                        Value::Int(timeout.as_millis() as i64),
                    ),
                ]);
                if let Some(value) = intercept_test_host_call(self, "dns.lookup", args, span) {
                    lowered_runtime_value(value, span)?
                } else {
                    lowered_runtime_list_result(
                        dns_module::lookup(&name, &record, &server, timeout, span),
                        span,
                    )?
                }
            }
            RuntimeOp::DnsResolveHost if values.len() == 1 || values.len() == 2 => {
                let family =
                    lowered_str_arg_owned(values.get(1).cloned(), "any", "dns.resolve_host", span)?;
                let name =
                    lowered_str_arg_owned(values.first().cloned(), "", "dns.resolve_host", span)?;
                let args = RecordMap::from([
                    (Arc::from("name"), Value::Str(name.as_str().into())),
                    (Arc::from("family"), Value::Str(family.as_str().into())),
                ]);
                if let Some(value) = intercept_test_host_call(self, "dns.resolve_host", args, span)
                {
                    lowered_runtime_value(value, span)?
                } else {
                    match dns_module::AddressFamily::from_name(&family)
                        .map_err(|error| {
                            RuntimeError::new(error.kind, error.message).with_span(span)
                        })
                        .and_then(|family| dns_module::resolve_host(&name, family, span))
                    {
                        Ok(records) => {
                            lowered_runtime_value(Value::ok(Value::List(records)), span)?
                        }
                        Err(error) => lowered_result_err_value(error),
                    }
                }
            }
            RuntimeOp::DnsReverse if values.len() == 1 => {
                let addr = lowered_str_arg_owned(values.pop(), "", "dns.reverse", span)?;
                let args = RecordMap::from([(Arc::from("addr"), Value::Str(addr.as_str().into()))]);
                if let Some(value) = intercept_test_host_call(self, "dns.reverse", args, span) {
                    lowered_runtime_value(value, span)?
                } else {
                    lowered_runtime_list_result(dns_module::reverse(&addr, span), span)?
                }
            }
            RuntimeOp::DnsNameservers if values.is_empty() => {
                let args = RecordMap::default();
                if let Some(value) = intercept_test_host_call(self, "dns.nameservers", args, span) {
                    lowered_runtime_value(value, span)?
                } else {
                    lowered_runtime_list_result(dns_module::nameservers(span), span)?
                }
            }
            RuntimeOp::FsCwd if values.is_empty() => {
                match path_value_from_pathbuf(self.cwd.clone()) {
                    Ok(path) => lowered_result_ok(LoweredValue::Path(path)),
                    Err(error) => lowered_result_err_value(error.with_span(span)),
                }
            }
            RuntimeOp::FsGitroot if values.is_empty() => {
                match crate::modules::fs::gitroot(self.cwd.clone(), span).and_then(|path| {
                    path_value_from_pathbuf(path).map_err(|error| error.with_span(span))
                }) {
                    Ok(path) => lowered_result_ok(LoweredValue::Path(path)),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::FsDirs if (1..=4).contains(&values.len()) => {
                let hidden = lowered_bool_arg_or(values.get(3).cloned(), false, "fs.dirs", span)?;
                let stat = lowered_bool_arg_or(values.get(2).cloned(), true, "fs.dirs", span)?;
                let gitignore = lowered_bool_arg_or(values.get(1).cloned(), true, "fs.dirs", span)?;
                let path = lowered_path_arg(values.remove(0), "fs.dirs", span)?;
                self.lowered_stream_list_result(
                    fs_module::walk_filesystem(
                        self.host_path(&path),
                        gitignore,
                        stat,
                        hidden,
                        fs_module::WalkEmit::Dirs,
                        Vec::new(),
                        span,
                    ),
                    span,
                )?
            }
            RuntimeOp::FsLs | RuntimeOp::FsChildren if (1..=3).contains(&values.len()) => {
                let operation = if op == RuntimeOp::FsLs {
                    "fs.ls"
                } else {
                    "fs.children"
                };
                let ordered = lowered_bool_arg_or(values.get(2).cloned(), true, operation, span)?;
                let stat = lowered_bool_arg_or(values.get(1).cloned(), true, operation, span)?;
                let path = lowered_path_arg(values.remove(0), operation, span)?;
                self.lowered_stream_list_result(
                    fs_module::list_filesystem(self.host_path(&path), stat, ordered, span),
                    span,
                )?
            }
            RuntimeOp::FsMetadata if values.len() == 1 => {
                let path = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "fs.metadata",
                    span,
                )?;
                match fs_module::metadata(self.host_path(&path), span) {
                    Ok(record) => match lowered_value_from_runtime_any(&record) {
                        Some(value) => lowered_result_ok(value),
                        None => {
                            return Err(RuntimeError::new(
                                "type-error",
                                format!("fs.metadata produced unsupported {}", record.type_name()),
                            )
                            .with_span(span));
                        }
                    },
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::FsFilesystemStats if values.len() == 1 => {
                let path = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "fs.filesystem_stats",
                    span,
                )?;
                match fs_module::filesystem_stats(&self.host_path(&path), span) {
                    Ok(stats) => lowered_result_ok(lowered_filesystem_stats_record(stats)),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::FsMounts if values.is_empty() => {
                lowered_runtime_stream_result(fs_module::mounts(span), span)?
            }
            RuntimeOp::FsMountFor if values.len() == 1 => {
                let path = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "fs.mount_for",
                    span,
                )?;
                match fs_module::mount_for(&self.host_path(&path), span) {
                    Ok(mount) => lowered_result_ok(lowered_fs_mount_record(mount)?),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::FsReadText if values.len() == 1 => {
                let path = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "fs.read_text",
                    span,
                )?;
                match read_host_path_bytes_vec(&self.host_path(&path), span) {
                    Ok(bytes) => match String::from_utf8(bytes) {
                        Ok(text) => lowered_result_ok(LoweredValue::Str(text.into())),
                        Err(error) => lowered_result_err_value(
                            RuntimeError::new(
                                "invalid-utf8",
                                format!(
                                    "file is not valid UTF-8 at byte {}",
                                    error.utf8_error().valid_up_to()
                                ),
                            )
                            .with_span(span),
                        ),
                    },
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::FsWrite if values.len() == 2 => {
                let data = lowered_bytes_or_str_owned(
                    values.pop().expect("checked value length"),
                    "fs.write",
                    span,
                )?;
                let path = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "fs.write",
                    span,
                )?;
                lowered_unit_result(fs_module::write_path(self.host_path(&path), &data, span))
            }
            RuntimeOp::FsWriteAtomic if values.len() == 2 => {
                let data = lowered_bytes_or_str_owned(
                    values.pop().expect("checked value length"),
                    "fs.write_atomic",
                    span,
                )?;
                let path = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "fs.write_atomic",
                    span,
                )?;
                lowered_unit_result(fs_module::write_atomic(self.host_path(&path), &data, span))
            }
            RuntimeOp::FsMkdir if values.len() == 1 || values.len() == 2 => {
                let parents = lowered_bool_arg_or(values.get(1).cloned(), true, "fs.mkdir", span)?;
                let path = lowered_path_arg(values.remove(0), "fs.mkdir", span)?;
                lowered_unit_result(fs_module::mkdir_path(
                    self.host_path(&path),
                    parents,
                    None,
                    span,
                ))
            }
            RuntimeOp::FsRemove if values.len() == 1 || values.len() == 2 => {
                let missing_ok =
                    lowered_bool_arg_or(values.get(1).cloned(), false, "fs.remove", span)?;
                let path = lowered_path_arg(values.remove(0), "fs.remove", span)?;
                lowered_unit_result(fs_module::remove_path(
                    self.host_path(&path),
                    missing_ok,
                    span,
                ))
            }
            RuntimeOp::FsExists if values.len() == 1 => {
                let path = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "fs.exists",
                    span,
                )?;
                match fs_module::exists(self.host_path(&path), span) {
                    Ok(exists) => lowered_result_ok(LoweredValue::Bool(exists)),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::FsExecutable if values.len() == 1 => {
                match values.pop().expect("checked value length") {
                    LoweredValue::Path(path) => {
                        match fs_module::executable(self.host_path(&path), span) {
                            Ok(executable) => lowered_result_ok(LoweredValue::Bool(executable)),
                            Err(error) => lowered_result_err_value(error),
                        }
                    }
                    LoweredValue::Int(mode) => LoweredValue::Bool(fs_module::mode_executable(mode)),
                    other => {
                        return Err(RuntimeError::new(
                            "type-error",
                            format!(
                                "fs.executable expected Path or Int, found {}",
                                other.type_name()
                            ),
                        )
                        .with_span(span));
                    }
                }
            }
            RuntimeOp::FsWorldWritable if values.len() == 1 => {
                let mode = lowered_int_arg(values.pop(), "fs.world_writable", span)?;
                LoweredValue::Bool(fs_module::mode_world_writable(mode))
            }
            RuntimeOp::FsSticky if values.len() == 1 => {
                let mode = lowered_int_arg(values.pop(), "fs.sticky", span)?;
                LoweredValue::Bool(fs_module::mode_sticky(mode))
            }
            RuntimeOp::FsSetuid if values.len() == 1 => {
                let mode = lowered_int_arg(values.pop(), "fs.setuid", span)?;
                LoweredValue::Bool(fs_module::mode_setuid(mode))
            }
            RuntimeOp::FsSetgid if values.len() == 1 => {
                let mode = lowered_int_arg(values.pop(), "fs.setgid", span)?;
                LoweredValue::Bool(fs_module::mode_setgid(mode))
            }
            RuntimeOp::FsOwnerExecutable if values.len() == 1 => {
                let mode = lowered_int_arg(values.pop(), "fs.owner_executable", span)?;
                LoweredValue::Bool(fs_module::mode_owner_executable(mode))
            }
            RuntimeOp::FsGroupExecutable if values.len() == 1 => {
                let mode = lowered_int_arg(values.pop(), "fs.group_executable", span)?;
                LoweredValue::Bool(fs_module::mode_group_executable(mode))
            }
            RuntimeOp::FsOtherExecutable if values.len() == 1 => {
                let mode = lowered_int_arg(values.pop(), "fs.other_executable", span)?;
                LoweredValue::Bool(fs_module::mode_other_executable(mode))
            }
            RuntimeOp::FsOpenRoot if values.len() == 1 => {
                let path = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "fs.open_root",
                    span,
                )?;
                match fs_module::open_root(self.host_path(&path), span) {
                    Ok(dir) => lowered_result_ok(self.push_lowered_fs_root(FsRootHandle::Dir(dir))),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::FsCloseRoot if values.len() == 1 => {
                let root = values.pop().expect("checked value length");

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
                        RuntimeError::new("fs-root", "root handle is not active").with_span(span),
                    ),
                }
            }
            RuntimeOp::FsRootPath if values.len() == 1 => {
                let root = values.pop().expect("checked value length");
                match lowered_fs_root_dir(&self.fs_roots, &root, span)
                    .and_then(|dir| root_path_from_dir(dir, span))
                {
                    Ok(path) => lowered_result_ok(LoweredValue::Path(path)),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::FsRootOpenRoot if values.len() == 2 => {
                let path =
                    lowered_path_arg(values.pop().expect("checked value length"), "fs.root", span)?;
                let root = values.pop().expect("checked value length");
                let rel = pathbuf_from_path_value(&path);
                match lowered_fs_root_dir(&self.fs_roots, &root, span)
                    .and_then(|dir| fs_module::rooted_open_root(dir, &rel, span))
                {
                    Ok(dir) => lowered_result_ok(self.push_lowered_fs_root(FsRootHandle::Dir(dir))),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::FsRootRead if values.len() == 2 => {
                let path = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "fs.root_read",
                    span,
                )?;
                let root = values.pop().expect("checked value length");
                let rel = pathbuf_from_path_value(&path);
                match lowered_fs_root_dir(&self.fs_roots, &root, span)
                    .and_then(|dir| fs_module::rooted_read(dir, &rel, span))
                {
                    Ok(bytes) => lowered_result_ok(LoweredValue::Bytes(bytes.into())),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::FsRootReadText if values.len() == 2 => {
                let path = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "fs.root_read_text",
                    span,
                )?;
                let root = values.pop().expect("checked value length");
                let rel = pathbuf_from_path_value(&path);
                match lowered_fs_root_dir(&self.fs_roots, &root, span)
                    .and_then(|dir| fs_module::rooted_read(dir, &rel, span))
                {
                    Ok(bytes) => match String::from_utf8(bytes) {
                        Ok(text) => lowered_result_ok(LoweredValue::Str(text.into())),
                        Err(error) => lowered_result_err_value(
                            RuntimeError::new(
                                "invalid-utf8",
                                format!(
                                    "file is not valid UTF-8 at byte {}",
                                    error.utf8_error().valid_up_to()
                                ),
                            )
                            .with_span(span),
                        ),
                    },
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::FsRootWrite | RuntimeOp::FsRootWriteAtomic if values.len() == 3 => {
                let operation = if op == RuntimeOp::FsRootWriteAtomic {
                    "fs.root_write_atomic"
                } else {
                    "fs.root_write"
                };
                let data = lowered_bytes_or_str_owned(
                    values.pop().expect("checked value length"),
                    operation,
                    span,
                )?;
                let path =
                    lowered_path_arg(values.pop().expect("checked value length"), operation, span)?;
                let root = values.pop().expect("checked value length");
                let rel = pathbuf_from_path_value(&path);
                let result = lowered_fs_root_dir(&self.fs_roots, &root, span).and_then(|dir| {
                    if op == RuntimeOp::FsRootWriteAtomic {
                        fs_module::rooted_write_atomic(dir, &rel, &data, span)
                    } else {
                        fs_module::rooted_write(dir, &rel, &data, span)
                    }
                });
                lowered_unit_result(result)
            }
            RuntimeOp::FsRootMetadata if values.len() == 2 => {
                let path = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "fs.root_metadata",
                    span,
                )?;
                let root = values.pop().expect("checked value length");
                let rel = pathbuf_from_path_value(&path);
                match lowered_fs_root_dir(&self.fs_roots, &root, span)
                    .and_then(|dir| fs_module::rooted_metadata(dir, &rel, span))
                {
                    Ok(record) => match lowered_value_from_runtime_any(&record) {
                        Some(value) => lowered_result_ok(value),
                        None => {
                            return Err(RuntimeError::new(
                                "type-error",
                                format!(
                                    "fs.root_metadata produced unsupported {}",
                                    record.type_name()
                                ),
                            )
                            .with_span(span));
                        }
                    },
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::FsRootExists if values.len() == 2 => {
                let path = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "fs.root_exists",
                    span,
                )?;
                let root = values.pop().expect("checked value length");
                let rel = pathbuf_from_path_value(&path);
                match lowered_fs_root_dir(&self.fs_roots, &root, span)
                    .and_then(|dir| fs_module::rooted_exists(dir, &rel, span))
                {
                    Ok(exists) => lowered_result_ok(LoweredValue::Bool(exists)),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::FsRootMkdir if (2..=4).contains(&values.len()) => {
                let (mode, parents) = match (values.get(2).cloned(), values.get(3).cloned()) {
                    (Some(LoweredValue::Bool(parents)), None) => (0o777, parents),
                    (mode, parents) => (
                        match mode {
                            Some(value) => lowered_int_arg(Some(value), "fs.root_mkdir", span)?,
                            None => 0o777,
                        },
                        lowered_bool_arg_or(parents, false, "fs.root_mkdir", span)?,
                    ),
                };
                let path = lowered_path_arg(
                    values.get(1).cloned().expect("checked value length"),
                    "fs.root_mkdir",
                    span,
                )?;
                let root = values.first().cloned().expect("checked value length");
                let rel = pathbuf_from_path_value(&path);
                lowered_unit_result(
                    lowered_fs_root_dir(&self.fs_roots, &root, span)
                        .and_then(|dir| fs_module::rooted_mkdir(dir, &rel, mode, parents, span)),
                )
            }
            RuntimeOp::FsRootRemove if values.len() == 2 || values.len() == 3 => {
                let dir =
                    lowered_bool_arg_or(values.get(2).cloned(), false, "fs.root_remove", span)?;
                let path = lowered_path_arg(
                    values.get(1).cloned().expect("checked value length"),
                    "fs.root_remove",
                    span,
                )?;
                let root = values.first().cloned().expect("checked value length");
                let rel = pathbuf_from_path_value(&path);
                lowered_unit_result(
                    lowered_fs_root_dir(&self.fs_roots, &root, span)
                        .and_then(|root| fs_module::rooted_remove(root, &rel, dir, span)),
                )
            }
            RuntimeOp::FsRootReadlink if values.len() == 2 => {
                let path = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "fs.root_readlink",
                    span,
                )?;
                let root = values.pop().expect("checked value length");
                let rel = pathbuf_from_path_value(&path);
                match lowered_fs_root_dir(&self.fs_roots, &root, span)
                    .and_then(|dir| fs_module::rooted_readlink(dir, &rel, span))
                    .and_then(|path| {
                        path_value_from_pathbuf(path).map_err(|error| error.with_span(span))
                    }) {
                    Ok(path) => lowered_result_ok(LoweredValue::Path(path)),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::FsRootSymlink if (3..=5).contains(&values.len()) => {
                let (parents, overwrite) = match (values.get(3).cloned(), values.get(4).cloned()) {
                    (Some(value), Some(overwrite)) => (
                        lowered_bool_arg_or(Some(value), true, "fs.root_symlink", span)?,
                        lowered_bool_arg_or(Some(overwrite), false, "fs.root_symlink", span)?,
                    ),
                    (Some(value), None) => (
                        true,
                        lowered_bool_arg_or(Some(value), false, "fs.root_symlink", span)?,
                    ),
                    (None, None) => (true, false),
                    (None, Some(_)) => unreachable!("checked value length"),
                };
                let path = lowered_path_arg(
                    values.get(2).cloned().expect("checked value length"),
                    "fs.root_symlink",
                    span,
                )?;
                let target = lowered_path_arg(
                    values.get(1).cloned().expect("checked value length"),
                    "fs.root_symlink",
                    span,
                )?;
                let root = values.first().cloned().expect("checked value length");
                let rel = pathbuf_from_path_value(&path);
                let target_rel = pathbuf_from_path_value(&target);
                lowered_unit_result(lowered_fs_root_dir(&self.fs_roots, &root, span).and_then(
                    |dir| {
                        fs_module::rooted_symlink(dir, &target_rel, &rel, parents, overwrite, span)
                    },
                ))
            }
            RuntimeOp::FsRootChmod if values.len() == 3 => {
                let mode = lowered_int_arg(values.pop(), "fs.root_chmod", span)?;
                let path = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "fs.root_chmod",
                    span,
                )?;
                let root = values.pop().expect("checked value length");
                let rel = pathbuf_from_path_value(&path);
                lowered_unit_result(
                    lowered_fs_root_dir(&self.fs_roots, &root, span)
                        .and_then(|dir| fs_module::rooted_chmod(dir, &rel, mode, span)),
                )
            }
            RuntimeOp::FsRootInstallFile if (5..=7).contains(&values.len()) => {
                let (parents, overwrite) = match (values.get(5).cloned(), values.get(6).cloned()) {
                    (Some(value), Some(overwrite)) => (
                        lowered_bool_arg_or(Some(value), true, "fs.root_install_file", span)?,
                        lowered_bool_arg_or(Some(overwrite), false, "fs.root_install_file", span)?,
                    ),
                    (Some(value), None) => (
                        true,
                        lowered_bool_arg_or(Some(value), false, "fs.root_install_file", span)?,
                    ),
                    (None, None) => (true, false),
                    (None, Some(_)) => unreachable!("checked value length"),
                };
                let mode = lowered_int_arg(values.get(4).cloned(), "fs.root_install_file", span)?;
                let dest = lowered_path_arg(
                    values.get(3).cloned().expect("checked value length"),
                    "fs.root_install_file",
                    span,
                )?;
                let dest_root = values.get(2).cloned().expect("checked value length");
                let source = lowered_path_arg(
                    values.get(1).cloned().expect("checked value length"),
                    "fs.root_install_file",
                    span,
                )?;
                let source_root = values.first().cloned().expect("checked value length");
                let dest_rel = pathbuf_from_path_value(&dest);
                let source_rel = pathbuf_from_path_value(&source);
                let result = match (
                    lowered_fs_root_dir(&self.fs_roots, &source_root, span),
                    lowered_fs_root_dir(&self.fs_roots, &dest_root, span),
                ) {
                    (Ok(source_dir), Ok(dest_dir)) => fs_module::rooted_install_file(
                        source_dir,
                        &source_rel,
                        dest_dir,
                        &dest_rel,
                        fs_module::RootedInstallOptions {
                            mode,
                            parents,
                            overwrite,
                            span,
                        },
                    ),
                    (Err(error), _) | (_, Err(error)) => Err(error),
                };
                lowered_unit_result(result)
            }
            RuntimeOp::FsCopy if values.len() == 2 || values.len() == 3 => {
                let overwrite =
                    lowered_bool_arg_or(values.get(2).cloned(), false, "fs.copy", span)?;
                let dest = lowered_path_arg(
                    values.get(1).cloned().expect("checked value length"),
                    "fs.copy",
                    span,
                )?;
                let source = lowered_path_arg(
                    values.first().cloned().expect("checked value length"),
                    "fs.copy",
                    span,
                )?;
                lowered_unit_result(fs_module::copy_file(
                    self.host_path(&source),
                    self.host_path(&dest),
                    overwrite,
                    span,
                ))
            }
            RuntimeOp::FsCopyTree if (2..=5).contains(&values.len()) => {
                let follow_symlinks =
                    lowered_bool_arg_or(values.get(4).cloned(), false, "fs.copy_tree", span)?;
                let overwrite =
                    lowered_bool_arg_or(values.get(3).cloned(), false, "fs.copy_tree", span)?;
                let parents =
                    lowered_bool_arg_or(values.get(2).cloned(), true, "fs.copy_tree", span)?;
                let dest = lowered_path_arg(
                    values.get(1).cloned().expect("checked value length"),
                    "fs.copy_tree",
                    span,
                )?;
                let source = lowered_path_arg(
                    values.first().cloned().expect("checked value length"),
                    "fs.copy_tree",
                    span,
                )?;
                lowered_runtime_result(
                    fs_module::copy_tree(
                        self.host_path(&source),
                        self.host_path(&dest),
                        overwrite,
                        parents,
                        follow_symlinks,
                        span,
                    ),
                    span,
                )?
            }
            RuntimeOp::FsRename if values.len() == 2 || values.len() == 3 => {
                let overwrite =
                    lowered_bool_arg_or(values.get(2).cloned(), false, "fs.rename", span)?;
                let dest = lowered_path_arg(
                    values.get(1).cloned().expect("checked value length"),
                    "fs.rename",
                    span,
                )?;
                let source = lowered_path_arg(
                    values.first().cloned().expect("checked value length"),
                    "fs.rename",
                    span,
                )?;
                lowered_unit_result(fs_module::rename_path(
                    self.host_path(&source),
                    self.host_path(&dest),
                    overwrite,
                    span,
                ))
            }
            RuntimeOp::FsRemoveManifest if (2..=4).contains(&values.len()) => {
                let prune_dirs =
                    lowered_bool_arg_or(values.get(3).cloned(), true, "fs.remove_manifest", span)?;
                let missing_ok =
                    lowered_bool_arg_or(values.get(2).cloned(), false, "fs.remove_manifest", span)?;
                let manifest = lowered_path_list_arg(
                    values.get(1).cloned().expect("checked value length"),
                    "fs.remove_manifest",
                    span,
                )?;
                let root = lowered_path_arg(
                    values.first().cloned().expect("checked value length"),
                    "fs.remove_manifest",
                    span,
                )?;
                lowered_runtime_result(
                    fs_module::remove_manifest(
                        self.host_path(&root),
                        manifest,
                        missing_ok,
                        prune_dirs,
                        span,
                    ),
                    span,
                )?
            }
            RuntimeOp::FsInstall if (3..=5).contains(&values.len()) => {
                let overwrite =
                    lowered_bool_arg_or(values.get(4).cloned(), false, "fs.install", span)?;
                let parents =
                    lowered_bool_arg_or(values.get(3).cloned(), true, "fs.install", span)?;
                let mode = lowered_int_arg(values.get(2).cloned(), "fs.install", span)?;
                let dest = lowered_path_arg(
                    values.get(1).cloned().expect("checked value length"),
                    "fs.install",
                    span,
                )?;
                let source = lowered_path_arg(
                    values.first().cloned().expect("checked value length"),
                    "fs.install",
                    span,
                )?;
                lowered_unit_result(fs_module::install_file(
                    self.host_path(&source),
                    self.host_path(&dest),
                    mode,
                    parents,
                    overwrite,
                    None,
                    None,
                    span,
                ))
            }
            RuntimeOp::FsInstallAs if (5..=7).contains(&values.len()) => {
                let overwrite =
                    lowered_bool_arg_or(values.get(6).cloned(), false, "fs.install_as", span)?;
                let parents =
                    lowered_bool_arg_or(values.get(5).cloned(), true, "fs.install_as", span)?;
                let group = lowered_record_arg(values.get(4).cloned(), "fs.install_as", span)?;
                let owner = lowered_record_arg(values.get(3).cloned(), "fs.install_as", span)?;
                let mode = lowered_int_arg(values.get(2).cloned(), "fs.install_as", span)?;
                let dest = lowered_path_arg(
                    values.get(1).cloned().expect("checked value length"),
                    "fs.install_as",
                    span,
                )?;
                let source = lowered_path_arg(
                    values.first().cloned().expect("checked value length"),
                    "fs.install_as",
                    span,
                )?;
                let owner_uid = record_int_field(&owner, "uid", "fs-install", span)?;
                let group_gid = record_int_field(&group, "gid", "fs-install", span)?;
                lowered_unit_result(fs_module::install_file(
                    self.host_path(&source),
                    self.host_path(&dest),
                    mode,
                    parents,
                    overwrite,
                    Some(owner_uid),
                    Some(group_gid),
                    span,
                ))
            }
            RuntimeOp::FsTruncate if values.len() == 2 => {
                let size = lowered_int_arg(values.pop(), "fs.truncate", span)?;
                let path = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "fs.truncate",
                    span,
                )?;
                lowered_unit_result(fs_module::truncate_path(self.host_path(&path), size, span))
            }
            RuntimeOp::FsChmod if values.len() == 2 => {
                let mode = lowered_int_arg(values.pop(), "fs.chmod", span)?;
                let path = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "fs.chmod",
                    span,
                )?;
                lowered_unit_result(fs_module::chmod_path(self.host_path(&path), mode, span))
            }
            RuntimeOp::FsHardlink if values.len() == 2 => {
                let path = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "fs.hardlink",
                    span,
                )?;
                let source = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "fs.hardlink",
                    span,
                )?;
                lowered_unit_result(fs_module::hardlink(
                    self.host_path(&source),
                    self.host_path(&path),
                    span,
                ))
            }
            RuntimeOp::FsChown if values.len() == 2 || values.len() == 3 => {
                let follow_symlinks =
                    lowered_bool_arg_or(values.get(2).cloned(), false, "fs.chown", span)?;
                let owner = lowered_record_arg(values.get(1).cloned(), "fs.chown", span)?;
                let path = lowered_path_arg(
                    values.first().cloned().expect("checked value length"),
                    "fs.chown",
                    span,
                )?;
                let uid = record_int_field(&owner, "uid", "fs-chown", span)?;
                lowered_unit_result(fs_module::chown_path(
                    self.host_path(&path),
                    uid,
                    follow_symlinks,
                    span,
                ))
            }
            RuntimeOp::FsChgrp if values.len() == 2 || values.len() == 3 => {
                let follow_symlinks =
                    lowered_bool_arg_or(values.get(2).cloned(), false, "fs.chgrp", span)?;
                let group = lowered_record_arg(values.get(1).cloned(), "fs.chgrp", span)?;
                let path = lowered_path_arg(
                    values.first().cloned().expect("checked value length"),
                    "fs.chgrp",
                    span,
                )?;
                let gid = record_int_field(&group, "gid", "fs-chgrp", span)?;
                lowered_unit_result(fs_module::chgrp_path(
                    self.host_path(&path),
                    gid,
                    follow_symlinks,
                    span,
                ))
            }
            RuntimeOp::FsMkfifo if values.len() == 2 => {
                let mode = lowered_int_arg(values.pop(), "fs.mkfifo", span)?;
                let path = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "fs.mkfifo",
                    span,
                )?;
                lowered_unit_result(fs_module::mkfifo_path(self.host_path(&path), mode, span))
            }
            RuntimeOp::FsFsync if values.len() == 1 => {
                let path = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "fs.fsync",
                    span,
                )?;
                lowered_unit_result(fs_module::fsync_path(self.host_path(&path), span))
            }
            RuntimeOp::FsSync if values.is_empty() => {
                fs_module::sync_filesystems();
                lowered_result_ok(LoweredValue::Unit)
            }
            RuntimeOp::FsSymlink if values.len() == 2 => {
                let path = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "fs.symlink",
                    span,
                )?;
                let target = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "fs.symlink",
                    span,
                )?;
                lowered_unit_result(fs_module::symlink_path(
                    pathbuf_from_path_value(&target),
                    self.host_path(&path),
                    span,
                ))
            }
            RuntimeOp::FsTempFile if values.is_empty() => {
                match self.lowered_create_temp_file_root(span) {
                    Ok(record) => lowered_result_ok(record),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::FsTempDir if values.is_empty() => {
                match new_temp_fs_root("fs-temp-dir", span) {
                    Ok(root) => lowered_result_ok(self.push_lowered_fs_root(root)),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::FsProjectRoot if values.len() == 4 => {
                let application =
                    lowered_str_arg_owned(values.get(3).cloned(), "", "fs.project_root", span)?;
                let organization =
                    lowered_str_arg_owned(values.get(2).cloned(), "", "fs.project_root", span)?;
                let qualifier =
                    lowered_str_arg_owned(values.get(1).cloned(), "", "fs.project_root", span)?;
                let kind =
                    lowered_str_arg_owned(values.first().cloned(), "", "fs.project_root", span)?;
                match self.lowered_project_root(
                    &kind,
                    &qualifier,
                    &organization,
                    &application,
                    span,
                ) {
                    Ok(root) => lowered_result_ok(root),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::FsUserRoot if values.len() == 1 => {
                let kind = lowered_str_arg_owned(values.pop(), "", "fs.user_root", span)?;
                match self.lowered_user_root(&kind, span) {
                    Ok(root) => lowered_result_ok(root),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::FsLock if (1..=3).contains(&values.len()) => {
                let nonblocking =
                    lowered_bool_arg_or(values.get(2).cloned(), false, "fs.lock", span)?;
                let shared = lowered_bool_arg_or(values.get(1).cloned(), false, "fs.lock", span)?;
                let path = lowered_path_arg(values.remove(0), "fs.lock", span)?;
                match fs_module::lock_path(self.host_path(&path), shared, nonblocking, span) {
                    Ok(file) => {
                        let id = self.fs_locks.len() as i64 + 1;
                        self.fs_locks.push(Some(file));
                        lowered_result_ok(LoweredValue::Record(BTreeMap::from([
                            (Arc::from("id"), LoweredValue::Int(id)),
                            (Arc::from("path"), LoweredValue::Path(path)),
                            (Arc::from("shared"), LoweredValue::Bool(shared)),
                        ])))
                    }
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::FsUnlock if values.len() == 1 => {
                let lock = lowered_record_arg(values.pop(), "fs.unlock", span)?;
                let id = record_int_field(&lock, "id", "fs-lock", span)?;
                let Some(slot) = id
                    .checked_sub(1)
                    .and_then(|index| usize::try_from(index).ok())
                    .and_then(|index| self.fs_locks.get_mut(index))
                else {
                    return Ok(ControlFlow::Continue(lowered_result_err_value(
                        RuntimeError::new("fs-lock", "lock handle is not active").with_span(span),
                    )));
                };
                let Some(file) = slot.take() else {
                    return Ok(ControlFlow::Continue(lowered_result_err_value(
                        RuntimeError::new("fs-lock", "lock handle is not active").with_span(span),
                    )));
                };
                lowered_unit_result(fs_module::unlock_file(&file, span))
            }
            RuntimeOp::GroupCurrent if values.is_empty() => {
                lowered_runtime_result(group_module::current(span), span)?
            }
            RuntimeOp::GroupLookup if values.len() == 1 => {
                let name = lowered_str_arg_owned(values.pop(), "", "group.lookup", span)?;
                lowered_runtime_result(group_module::lookup(&name, span), span)?
            }
            RuntimeOp::GroupByGid if values.len() == 1 => {
                let gid = lowered_int_arg(values.pop(), "group.by_gid", span)?;
                lowered_runtime_result(
                    group_module::gid_from_i64(gid, span)
                        .and_then(|gid| group_module::by_gid(gid, span)),
                    span,
                )?
            }
            RuntimeOp::GroupAdd if values.len() == 1 || values.len() == 2 => {
                let gid = match values.get(1).cloned() {
                    Some(value) => Some(lowered_int_arg(Some(value), "group.add", span)?),
                    None => None,
                };
                let name = lowered_str_arg_owned(values.first().cloned(), "", "group.add", span)?;
                lowered_runtime_result(group_module::add(&name, gid, span), span)?
            }
            RuntimeOp::GroupRemove if values.len() == 1 => {
                let name = lowered_str_arg_owned(values.pop(), "", "group.remove", span)?;
                lowered_runtime_result(group_module::remove(&name, span), span)?
            }
            RuntimeOp::HashMd5
            | RuntimeOp::HashSha1
            | RuntimeOp::HashSha256
            | RuntimeOp::HashSha512
                if values.len() == 1 =>
            {
                let algorithm = match op {
                    RuntimeOp::HashMd5 => hash_module::HashAlgorithm::Md5,
                    RuntimeOp::HashSha1 => hash_module::HashAlgorithm::Sha1,
                    RuntimeOp::HashSha256 => hash_module::HashAlgorithm::Sha256,
                    RuntimeOp::HashSha512 => hash_module::HashAlgorithm::Sha512,
                    _ => unreachable!("checked hash digest op"),
                };
                match values.pop().expect("checked value length") {
                    LoweredValue::Bytes(bytes) => {
                        LoweredValue::Digest(Box::new(hash_module::digest_bytes(algorithm, &bytes)))
                    }
                    LoweredValue::BytesView(bytes) => LoweredValue::Digest(Box::new(
                        hash_module::digest_bytes(algorithm, bytes.as_slice()),
                    )),
                    LoweredValue::Path(path) => lowered_runtime_result(
                        hash_module::digest_file(algorithm, &self.host_path(&path), span)
                            .map(Value::digest),
                        span,
                    )?,
                    LoweredValue::Str(text) => {
                        let path =
                            PathValue::from_text(text).map_err(|error| error.with_span(span))?;
                        lowered_runtime_result(
                            hash_module::digest_file(algorithm, &self.host_path(&path), span)
                                .map(Value::digest),
                            span,
                        )?
                    }
                    LoweredValue::StrView(text) => {
                        let path = PathValue::from_text(text.as_str())
                            .map_err(|error| error.with_span(span))?;
                        lowered_runtime_result(
                            hash_module::digest_file(algorithm, &self.host_path(&path), span)
                                .map(Value::digest),
                            span,
                        )?
                    }
                    other => {
                        return Err(RuntimeError::new(
                            "type-error",
                            format!(
                                "hash digest expected Bytes or Path, found {}",
                                other.type_name()
                            ),
                        )
                        .with_span(span));
                    }
                }
            }
            RuntimeOp::HashCrc32 if values.len() == 1 => {
                let value = values.pop().expect("checked value length");
                let bytes = lowered_bytes_arg(&value, "hash.crc32", span)?;
                LoweredValue::Int(hash_module::crc32(bytes))
            }
            RuntimeOp::HashCrc32c if values.len() == 1 => {
                let value = values.pop().expect("checked value length");
                let bytes = lowered_bytes_arg(&value, "hash.crc32c", span)?;
                LoweredValue::Int(hash_module::crc32c(bytes))
            }
            RuntimeOp::HashParseCheckLine if values.len() == 1 => {
                let line = lowered_str_arg_owned(values.pop(), "", "hash.parse_check_line", span)?;
                match hash_module::parse_check_line(&line, span) {
                    Ok(line) => lowered_result_ok(LoweredValue::Record(BTreeMap::from([
                        (Arc::from("hex"), LoweredValue::Str(line.hex.into())),
                        (Arc::from("path"), LoweredValue::Str(line.path.into())),
                        (Arc::from("binary"), LoweredValue::Bool(line.binary)),
                    ]))),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::IniDecode if values.len() == 1 => {
                let text = lowered_str_arg_owned(values.pop(), "", "ini.decode", span)?;
                lowered_runtime_result(ini_module::decode(&text, span), span)?
            }
            RuntimeOp::IniRead if values.len() == 1 => {
                let path = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "ini.read",
                    span,
                )?;
                match read_host_path_string(&self.host_path(&path), "ini-read", span) {
                    Ok(text) => lowered_runtime_result(ini_module::decode(&text, span), span)?,
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::IniEncode if values.len() == 1 => {
                let value = values.pop().expect("checked value length").into_value();
                let Value::Record(record) = value else {
                    return Err(
                        RuntimeError::new("type-error", "ini.encode expected Record")
                            .with_span(span),
                    );
                };
                lowered_runtime_result(
                    ini_module::encode(&record, span).map(|text| Value::Str(text.into())),
                    span,
                )?
            }
            RuntimeOp::IniWrite if values.len() == 2 || values.len() == 3 => {
                let overwrite =
                    lowered_bool_arg_or(values.get(2).cloned(), true, "ini.write", span)?;
                let value = values.remove(1).into_value();
                let Value::Record(record) = value else {
                    return Err(RuntimeError::new("type-error", "ini.write expected Record")
                        .with_span(span));
                };
                let path = lowered_path_arg(values.remove(0), "ini.write", span)?;
                let text = match ini_module::encode(&record, span) {
                    Ok(text) => text,
                    Err(error) => {
                        return Ok(ControlFlow::Continue(lowered_result_err_value(error)));
                    }
                };
                let host_path = self.host_path(&path);
                if !overwrite {
                    match fs_module::exists(host_path.clone(), span) {
                        Ok(true) => {
                            return Ok(ControlFlow::Continue(lowered_result_err_value(
                                RuntimeError::new("ini-write", "destination exists")
                                    .with_span(span),
                            )));
                        }
                        Ok(false) => {}
                        Err(error) => {
                            return Ok(ControlFlow::Continue(lowered_result_err_value(error)));
                        }
                    }
                }
                lowered_unit_result(fs_module::write_path(host_path, text.as_bytes(), span))
            }
            RuntimeOp::HashVerifyFile if values.len() == 1 || values.len() == 2 => {
                let expected = if values.len() == 2 {
                    Some(lowered_str_arg_owned(
                        values.pop(),
                        "",
                        "hash.verify_file",
                        span,
                    )?)
                } else {
                    None
                };
                let path = lowered_path_arg(values.remove(0), "hash.verify_file", span)?;
                let Some(expected) = expected else {
                    let error =
                        RuntimeError::new("checksum-format", "verify_file requires a checksum")
                            .with_span(span);
                    return Ok(ControlFlow::Continue(lowered_result_err_value(error)));
                };
                match hash_module::digest_file(
                    hash_module::HashAlgorithm::Sha256,
                    &self.host_path(&path),
                    span,
                )
                .and_then(|digest| hash_module::verify_hex(&digest, &expected, span))
                {
                    Ok(()) => lowered_result_ok(LoweredValue::Unit),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::IoStdinBytes if values.is_empty() => {
                let mut data = Vec::new();
                match std::io::stdin().read_to_end(&mut data) {
                    Ok(_) => lowered_result_ok(LoweredValue::Bytes(data.into())),
                    Err(error) => lowered_result_err_value(
                        RuntimeError::new("io.stdin_bytes", error.to_string()).with_span(span),
                    ),
                }
            }
            RuntimeOp::IoStdinText if values.is_empty() => {
                let mut data = String::new();
                match std::io::stdin().read_to_string(&mut data) {
                    Ok(_) => lowered_result_ok(LoweredValue::Str(data.into())),
                    Err(error) => lowered_result_err_value(
                        RuntimeError::new("io.stdin_text", error.to_string()).with_span(span),
                    ),
                }
            }
            RuntimeOp::IoStdinLine if values.is_empty() => {
                let mut line = String::new();
                match std::io::BufRead::read_line(
                    &mut std::io::BufReader::new(std::io::stdin().lock()),
                    &mut line,
                ) {
                    Ok(_) => {
                        if line.ends_with('\n') {
                            line.pop();
                            if line.ends_with('\r') {
                                line.pop();
                            }
                        }
                        lowered_result_ok(LoweredValue::Str(line.into()))
                    }
                    Err(error) => lowered_result_err_value(
                        RuntimeError::new("io.stdin_line", error.to_string()).with_span(span),
                    ),
                }
            }
            RuntimeOp::IoWriteStdout if values.len() == 1 => {
                let text = lowered_str_arg_owned(values.pop(), "", "io.write_stdout", span)?;
                self.stdout.extend_from_slice(text.as_bytes());
                lowered_result_ok(LoweredValue::Unit)
            }
            RuntimeOp::IoWriteStdoutBytes if values.len() == 1 => {
                let value = values.pop().expect("checked value length");
                let data = lowered_bytes_arg(&value, "io.write_stdout_bytes", span)?;
                self.stdout.extend_from_slice(data);
                lowered_result_ok(LoweredValue::Unit)
            }
            RuntimeOp::MapEmpty if values.is_empty() => LoweredValue::Map(BTreeMap::new()),
            RuntimeOp::TimeNow if values.is_empty() => {
                LoweredValue::Int(crate::modules::time::now_epoch_ms())
            }
            RuntimeOp::TimeSleep if values.len() == 1 => {
                let duration = lowered_duration_arg(values.pop(), "time.sleep", span)?;
                let deadline = std::time::Instant::now() + Duration::from_millis(duration.millis);
                while std::time::Instant::now() < deadline {
                    self.service_pending_signal(span)?;
                    if self.signal_state.shutdown_complete {
                        break;
                    }
                    let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                    std::thread::sleep(std::cmp::min(WAIT_POLL, remaining));
                }
                lowered_result_ok(LoweredValue::Unit)
            }
            RuntimeOp::TimeMillis if values.len() == 1 => {
                let ms = lowered_int_arg(values.pop(), "time.millis", span)?;
                let millis = if ms < 0 { 0 } else { ms as u64 };
                LoweredValue::Duration(DurationValue { millis })
            }
            RuntimeOp::TimeSeconds if values.len() == 1 => {
                let seconds = lowered_int_arg(values.pop(), "time.seconds", span)?;
                let millis = if seconds < 0 {
                    0
                } else {
                    (seconds as u64).saturating_mul(1000)
                };
                LoweredValue::Duration(DurationValue { millis })
            }
            RuntimeOp::TimeMeasure if values.len() == 1 || values.len() == 2 => {
                let quiet =
                    lowered_bool_arg_or(values.get(1).cloned(), false, "time.measure", span)?;
                let plan = lowered_command_arg(values.remove(0), "time.measure", span)?;
                let invocation = self.invocation_from_command_plan(&plan, span)?;
                let cpu_before = child_cpu_ns();
                let started = Instant::now();
                let outcome = if quiet {
                    run_quiet_with_policy(&invocation, self)
                } else {
                    run_inherit_with_policy(&invocation, self)
                };
                match outcome {
                    Ok(end) => {
                        let wall_ns = started.elapsed().as_nanos().min(i64::MAX as u128) as i64;
                        let (user_ns, system_ns) = cpu_ns_delta(cpu_before);
                        let status = end.status.expect("measured command has status");
                        lowered_result_ok(lowered_measured_command_record(
                            status, wall_ns, user_ns, system_ns,
                        ))
                    }
                    Err(error) => {
                        if self.signal_state.shutdown_complete
                            && self.signal_state.shutdown_status.is_some()
                        {
                            let wall_ns = started.elapsed().as_nanos().min(i64::MAX as u128) as i64;
                            let (user_ns, system_ns) = cpu_ns_delta(cpu_before);
                            let status = error
                                .status
                                .as_deref()
                                .cloned()
                                .unwrap_or_else(|| ProcessStatus::signaled(libc::SIGTERM));
                            lowered_result_ok(lowered_measured_command_record(
                                status, wall_ns, user_ns, system_ns,
                            ))
                        } else {
                            lowered_result_err_value(run_error_to_runtime(error, span))
                        }
                    }
                }
            }
            RuntimeOp::TimeDurationCompact if values.len() == 1 => {
                let seconds = lowered_int_arg(values.pop(), "time.duration_compact", span)?;
                LoweredValue::Str(time_module::duration_compact(seconds).into())
            }
            RuntimeOp::BytesCopy if (2..=7).contains(&values.len()) => {
                let overwrite =
                    lowered_bool_arg_or(values.get(6).cloned(), false, "bytes.copy", span)?;
                let seek = lowered_int_arg_or(values.get(5).cloned(), 0, "bytes.copy", span)?;
                let skip = lowered_int_arg_or(values.get(4).cloned(), 0, "bytes.copy", span)?;
                let count = match values.get(3).cloned() {
                    Some(value) => Some(lowered_int_arg(Some(value), "bytes.copy", span)?),
                    None => None,
                };
                let block_size =
                    lowered_int_arg_or(values.get(2).cloned(), 512, "bytes.copy", span)?;
                let dest = lowered_path_arg(values.remove(1), "bytes.copy", span)?;
                let source = lowered_path_arg(values.remove(0), "bytes.copy", span)?;
                lowered_runtime_result(
                    bytes_module::copy_blocks(
                        self.host_path(&source),
                        self.host_path(&dest),
                        block_size,
                        count,
                        skip,
                        seek,
                        overwrite,
                        span,
                    ),
                    span,
                )?
            }
            RuntimeOp::BytesCopyFile if (2..=7).contains(&values.len()) => {
                let truncate =
                    lowered_bool_arg_or(values.get(6).cloned(), false, "bytes.copy_file", span)?;
                let create =
                    lowered_bool_arg_or(values.get(5).cloned(), true, "bytes.copy_file", span)?;
                let length = match values.get(4).cloned() {
                    Some(value) => Some(lowered_int_arg(Some(value), "bytes.copy_file", span)?),
                    None => None,
                };
                let dest_offset =
                    lowered_int_arg_or(values.get(3).cloned(), 0, "bytes.copy_file", span)?;
                let source_offset =
                    lowered_int_arg_or(values.get(2).cloned(), 0, "bytes.copy_file", span)?;
                let dest = lowered_path_arg(values.remove(1), "bytes.copy_file", span)?;
                let source = lowered_path_arg(values.remove(0), "bytes.copy_file", span)?;
                lowered_runtime_result(
                    bytes_module::copy_file(
                        self.host_path(&source),
                        self.host_path(&dest),
                        source_offset,
                        dest_offset,
                        length,
                        create,
                        truncate,
                        span,
                    ),
                    span,
                )?
            }
            RuntimeOp::BytesZero if values.len() == 1 => {
                let length = lowered_int_arg(values.pop(), "bytes.zero", span)?;
                match bytes_module::zero(length, span) {
                    Ok(bytes) => lowered_result_ok(LoweredValue::Bytes(bytes.into())),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::BytesFromText if values.len() == 1 => {
                let text = lowered_str_arg_owned(values.pop(), "", "bytes.from_text", span)?;
                LoweredValue::Bytes(bytes_module::from_text(&text).into())
            }
            RuntimeOp::BytesFromInts if values.len() == 1 => {
                let ints = lowered_int_list_arg(values.pop(), "bytes.from_ints", span)?;
                match bytes_module::from_ints(ints, span) {
                    Ok(bytes) => lowered_result_ok(LoweredValue::Bytes(bytes.into())),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::BytesConcat if values.len() == 1 => {
                let chunks = lowered_bytes_list_arg(values.pop(), "bytes.concat", span)?;
                LoweredValue::Bytes(bytes_module::concat(chunks).into())
            }
            RuntimeOp::BytesHuman if values.len() == 1 => {
                let size = lowered_int_arg(values.pop(), "bytes.human", span)?;
                LoweredValue::Str(bytes_module::human(size).into())
            }
            RuntimeOp::BytesPackLe if values.len() == 2 => {
                let width = lowered_int_arg(values.pop(), "bytes.pack_le", span)?;
                let value = lowered_int_arg(values.pop(), "bytes.pack_le", span)?;
                match bytes_module::pack_int_le(value, width, span) {
                    Ok(bytes) => lowered_result_ok(LoweredValue::Bytes(bytes.into())),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::BytesPackBe if values.len() == 2 => {
                let width = lowered_int_arg(values.pop(), "bytes.pack_be", span)?;
                let value = lowered_int_arg(values.pop(), "bytes.pack_be", span)?;
                match bytes_module::pack_int_be(value, width, span) {
                    Ok(bytes) => lowered_result_ok(LoweredValue::Bytes(bytes.into())),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::BytesUnpackLe if values.len() == 2 || values.len() == 3 => {
                let offset = if values.len() == 3 {
                    lowered_int_arg(values.pop(), "bytes.unpack_le", span)?
                } else {
                    0
                };
                let width = lowered_int_arg(values.pop(), "bytes.unpack_le", span)?;
                let data = values.pop().expect("checked value length");
                let bytes = lowered_bytes_value(&data).ok_or_else(|| {
                    RuntimeError::new("type-error", "bytes.unpack_le expected Bytes")
                        .with_span(span)
                })?;
                match bytes_module::unpack_int_le(bytes, offset, width, span) {
                    Ok(value) => lowered_result_ok(LoweredValue::Int(value)),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::BytesUnpackBe if values.len() == 2 || values.len() == 3 => {
                let offset = if values.len() == 3 {
                    lowered_int_arg(values.pop(), "bytes.unpack_be", span)?
                } else {
                    0
                };
                let width = lowered_int_arg(values.pop(), "bytes.unpack_be", span)?;
                let data = values.pop().expect("checked value length");
                let bytes = lowered_bytes_value(&data).ok_or_else(|| {
                    RuntimeError::new("type-error", "bytes.unpack_be expected Bytes")
                        .with_span(span)
                })?;
                match bytes_module::unpack_int_be(bytes, offset, width, span) {
                    Ok(value) => lowered_result_ok(LoweredValue::Int(value)),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::BytesReadAt if values.len() == 3 => {
                let length = lowered_int_arg(values.pop(), "bytes.read_at", span)?;
                let offset = lowered_int_arg(values.pop(), "bytes.read_at", span)?;
                let path = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "bytes.read_at",
                    span,
                )?;
                match bytes_module::read_at(self.host_path(&path), offset, length, span) {
                    Ok(bytes) => lowered_result_ok(LoweredValue::Bytes(bytes.into())),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::BytesWriteAt if values.len() == 3 || values.len() == 4 => {
                let create =
                    lowered_bool_arg_or(values.get(3).cloned(), false, "bytes.write_at", span)?;
                let data_value = values.remove(2);
                let data = lowered_bytes_arg(&data_value, "bytes.write_at", span)?;
                let offset = lowered_int_arg(Some(values.remove(1)), "bytes.write_at", span)?;
                let path = lowered_path_arg(values.remove(0), "bytes.write_at", span)?;
                match bytes_module::write_at(self.host_path(&path), offset, data, create, span) {
                    Ok(written) => lowered_result_ok(LoweredValue::Int(written)),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::BytesZeroAt if values.len() == 3 || values.len() == 4 => {
                let create =
                    lowered_bool_arg_or(values.get(3).cloned(), false, "bytes.zero_at", span)?;
                let length = lowered_int_arg(Some(values.remove(2)), "bytes.zero_at", span)?;
                let offset = lowered_int_arg(Some(values.remove(1)), "bytes.zero_at", span)?;
                let path = lowered_path_arg(values.remove(0), "bytes.zero_at", span)?;
                match bytes_module::zero_at(self.host_path(&path), offset, length, create, span) {
                    Ok(written) => lowered_result_ok(LoweredValue::Int(written)),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::EnvGet if values.len() == 1 => {
                let key = match lowered_env_key_arg(values.pop(), span)? {
                    Ok(key) => key,
                    Err(error) => return Ok(ControlFlow::Continue(error)),
                };
                let value = match self.env.get_owned(key.as_bytes()) {
                    Some(value) => value,
                    None => {
                        return Ok(ControlFlow::Continue(lowered_result_err_value(
                            RuntimeError::new("env-missing", "environment value is unset")
                                .with_span(span),
                        )));
                    }
                };
                match String::from_utf8(value) {
                    Ok(text) => lowered_result_ok(LoweredValue::Str(text.into())),
                    Err(_) => lowered_result_err_value(
                        RuntimeError::new("invalid-utf8", "environment value is not valid UTF-8")
                            .with_span(span),
                    ),
                }
            }
            RuntimeOp::EnvGetOr if values.len() == 1 || values.len() == 2 => {
                let fallback =
                    lowered_str_arg_owned(values.get(1).cloned(), "", "env.get_or", span)?;
                let key = match lowered_env_key_arg(values.first().cloned(), span)? {
                    Ok(key) => key,
                    Err(error) => return Ok(ControlFlow::Continue(error)),
                };
                let Some(value) = self.env.get_owned(key.as_bytes()) else {
                    return Ok(ControlFlow::Continue(lowered_result_ok(LoweredValue::Str(
                        fallback.into(),
                    ))));
                };
                match String::from_utf8(value) {
                    Ok(text) => lowered_result_ok(LoweredValue::Str(text.into())),
                    Err(_) => lowered_result_err_value(
                        RuntimeError::new("invalid-utf8", "environment value is not valid UTF-8")
                            .with_span(span),
                    ),
                }
            }
            RuntimeOp::EnvBool if values.len() == 1 || values.len() == 2 => {
                let fallback =
                    lowered_bool_arg_or(values.get(1).cloned(), false, "env.bool", span)?;
                let key = match lowered_env_key_arg(values.first().cloned(), span)? {
                    Ok(key) => key,
                    Err(error) => return Ok(ControlFlow::Continue(error)),
                };
                let Some(value) = self.env.get_owned(key.as_bytes()) else {
                    return Ok(ControlFlow::Continue(lowered_result_ok(
                        LoweredValue::Bool(fallback),
                    )));
                };
                let text = match String::from_utf8(value) {
                    Ok(text) => text.trim().to_ascii_lowercase(),
                    Err(_) => {
                        return Ok(ControlFlow::Continue(lowered_result_err_value(
                            RuntimeError::new(
                                "invalid-utf8",
                                "environment value is not valid UTF-8",
                            )
                            .with_span(span),
                        )));
                    }
                };
                lowered_result_ok(LoweredValue::Bool(matches!(
                    text.as_str(),
                    "1" | "true" | "yes" | "on"
                )))
            }
            RuntimeOp::EnvPath if values.len() == 1 || values.len() == 2 => {
                let fallback = match values.get(1).cloned() {
                    Some(LoweredValue::Path(path)) => path,
                    Some(other) => {
                        return Err(RuntimeError::new(
                            "type-error",
                            format!(
                                "env.path expected Path fallback, found {}",
                                other.type_name()
                            ),
                        )
                        .with_span(span));
                    }
                    None => PathValue::from_text("").map_err(|error| error.with_span(span))?,
                };
                let key = match lowered_env_key_arg(values.first().cloned(), span)? {
                    Ok(key) => key,
                    Err(error) => return Ok(ControlFlow::Continue(error)),
                };
                let Some(value) = self.env.get_owned(key.as_bytes()) else {
                    return Ok(ControlFlow::Continue(lowered_result_ok(
                        LoweredValue::Path(fallback),
                    )));
                };
                match PathValue::new(value).map_err(|error| error.with_span(span)) {
                    Ok(path) => lowered_result_ok(LoweredValue::Path(path)),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::EnvInt if values.len() == 1 || values.len() == 2 => {
                let fallback = match values.get(1).cloned() {
                    Some(value) => lowered_int_arg(Some(value), "env.int", span)?,
                    None => 0,
                };
                let key = match lowered_env_key_arg(values.first().cloned(), span)? {
                    Ok(key) => key,
                    Err(error) => return Ok(ControlFlow::Continue(error)),
                };
                let Some(value) = self.env.get_owned(key.as_bytes()) else {
                    return Ok(ControlFlow::Continue(lowered_result_ok(LoweredValue::Int(
                        fallback,
                    ))));
                };
                let text = match String::from_utf8(value) {
                    Ok(text) => text,
                    Err(_) => {
                        return Ok(ControlFlow::Continue(lowered_result_err_value(
                            RuntimeError::new(
                                "invalid-utf8",
                                "environment value is not valid UTF-8",
                            )
                            .with_span(span),
                        )));
                    }
                };
                match text.trim().parse::<i64>() {
                    Ok(value) => lowered_result_ok(LoweredValue::Int(value)),
                    Err(_) => lowered_result_err_value(
                        RuntimeError::new("env-int", "environment value is not an integer")
                            .with_span(span),
                    ),
                }
            }
            RuntimeOp::EnvList if values.is_empty() => {
                let mut items = Vec::new();
                for (name, value) in self.env.snapshot() {
                    let name = match String::from_utf8(name.clone()) {
                        Ok(text) => text,
                        Err(_) => {
                            return Ok(ControlFlow::Continue(lowered_result_err_value(
                                RuntimeError::new(
                                    "invalid-utf8",
                                    "environment name is not valid UTF-8",
                                )
                                .with_span(span),
                            )));
                        }
                    };
                    let value = match String::from_utf8(value.clone()) {
                        Ok(text) => text,
                        Err(_) => {
                            return Ok(ControlFlow::Continue(lowered_result_err_value(
                                RuntimeError::new(
                                    "invalid-utf8",
                                    "environment value is not valid UTF-8",
                                )
                                .with_span(span),
                            )));
                        }
                    };
                    items.push(LoweredValue::Record(BTreeMap::from([
                        (Arc::from("name"), LoweredValue::Str(name.into())),
                        (Arc::from("value"), LoweredValue::Str(value.into())),
                    ])));
                }
                lowered_result_ok(LoweredValue::List(items))
            }
            RuntimeOp::EnvPathList if values.is_empty() => {
                let paths = self
                    .lowered_env_path_entries(span)?
                    .into_iter()
                    .map(LoweredValue::Path)
                    .collect();
                LoweredValue::List(paths)
            }
            RuntimeOp::EnvPathList if values.len() == 1 => {
                let key = match lowered_env_key_arg(values.pop(), span)? {
                    Ok(key) => key,
                    Err(error) => return Ok(ControlFlow::Continue(error)),
                };
                let value = match self.env.get_owned(key.as_bytes()) {
                    Some(value) => value,
                    None => {
                        return Ok(ControlFlow::Continue(lowered_result_err_value(
                            RuntimeError::new("env-missing", "environment value is unset")
                                .with_span(span),
                        )));
                    }
                };
                let mut paths = Vec::new();
                let value = OsString::from_vec(value);
                for path in std::env::split_paths(&value) {
                    paths.push(LoweredValue::Path(path_value_from_pathbuf(path)?));
                }
                lowered_result_ok(LoweredValue::List(paths))
            }
            RuntimeOp::EnvPathPrepend if values.len() == 1 => {
                let path = lowered_path_arg(values.remove(0), "env.PATH.prepend", span)?;
                let mut entries = self.lowered_env_path_entries(span)?;
                entries.insert(0, path);
                self.lowered_set_env_path_entries(&entries, span)?;
                lowered_result_ok(LoweredValue::Unit)
            }
            RuntimeOp::EnvPathAppend if values.len() == 1 => {
                let path = lowered_path_arg(values.remove(0), "env.PATH.append", span)?;
                let mut entries = self.lowered_env_path_entries(span)?;
                entries.push(path);
                self.lowered_set_env_path_entries(&entries, span)?;
                lowered_result_ok(LoweredValue::Unit)
            }
            RuntimeOp::EnvPathPop if values.is_empty() => {
                let mut entries = self.lowered_env_path_entries(span)?;
                let Some(path) = entries.pop() else {
                    return Ok(ControlFlow::Continue(lowered_result_err_value(
                        RuntimeError::new("env-path-empty", "PATH is empty").with_span(span),
                    )));
                };
                self.lowered_set_env_path_entries(&entries, span)?;
                lowered_result_ok(LoweredValue::Path(path))
            }
            RuntimeOp::EnvPathEntries if values.len() == 1 => {
                let key = match lowered_env_key_arg(values.pop(), span)? {
                    Ok(key) => key,
                    Err(error) => return Ok(ControlFlow::Continue(error)),
                };
                let value = match self.env.get_owned(key.as_bytes()) {
                    Some(value) => value,
                    None => {
                        return Ok(ControlFlow::Continue(lowered_result_err_value(
                            RuntimeError::new("env-missing", "environment value is unset")
                                .with_span(span),
                        )));
                    }
                };
                let text = match String::from_utf8(value) {
                    Ok(text) => text,
                    Err(_) => {
                        return Ok(ControlFlow::Continue(lowered_result_err_value(
                            RuntimeError::new(
                                "invalid-utf8",
                                "environment value is not valid UTF-8",
                            )
                            .with_span(span),
                        )));
                    }
                };
                let mut entries = Vec::new();
                for (index, raw) in text.split(':').enumerate() {
                    let path = if raw.is_empty() {
                        PathValue::from_text(".").map_err(|error| error.with_span(span))?
                    } else {
                        PathValue::from_text(raw).map_err(|error| error.with_span(span))?
                    };
                    entries.push(LoweredValue::Record(BTreeMap::from([
                        (Arc::from("index"), LoweredValue::Int(index as i64)),
                        (Arc::from("raw"), LoweredValue::Str(raw.into())),
                        (Arc::from("path"), LoweredValue::Path(path)),
                        (Arc::from("empty"), LoweredValue::Bool(raw.is_empty())),
                    ])));
                }
                lowered_result_ok(LoweredValue::List(entries))
            }
            RuntimeOp::JsonDecode if values.len() == 1 => {
                let Some(text) = lowered_str_value(&values[0]) else {
                    return Err(
                        RuntimeError::new("type-error", "json.decode expected Str").with_span(span)
                    );
                };
                lowered_runtime_result(json_module::parse_json(text, span), span)?
            }
            RuntimeOp::JsonEncode if values.len() == 1 || values.len() == 2 => {
                let pretty =
                    lowered_bool_arg_or(values.get(1).cloned(), false, "json.encode", span)?;
                match lowered_encode_json(&values[0], pretty, span) {
                    Ok(text) => lowered_result_ok(LoweredValue::Str(text.into())),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::JsonEncodeLines if values.len() == 1 => {
                match json_module::encode_json_lines(&values[0].clone().into_value(), span) {
                    Ok(text) => lowered_result_ok(LoweredValue::Str(text.into())),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::JsonWrite if values.len() == 2 || values.len() == 3 => {
                let pretty =
                    lowered_bool_arg_or(values.get(2).cloned(), false, "json.write", span)?;
                let value = values[1].clone().into_value();
                let path = lowered_path_arg(values.remove(0), "json.write", span)?;
                match json_module::encode_json(&value, pretty, span) {
                    Ok(text) => lowered_unit_result(fs_module::write_path(
                        self.host_path(&path),
                        text.as_bytes(),
                        span,
                    )),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::JsonWriteLines if values.len() == 2 => {
                let value = values[1].clone().into_value();
                let path = lowered_path_arg(values.remove(0), "json.write_lines", span)?;
                match json_module::encode_json_lines(&value, span) {
                    Ok(text) => lowered_unit_result(fs_module::write_path(
                        self.host_path(&path),
                        text.as_bytes(),
                        span,
                    )),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::JsonGet if values.len() == 2 || values.len() == 3 => {
                let value = values[0].clone().into_value();
                let path = values[1].clone().into_value();
                match values.get(2) {
                    Some(fallback) => match json_module::json_path_get(&value, &path, span) {
                        Ok(found) => lowered_runtime_value(found, span)?,
                        Err(error) if error.kind == "json-path" => fallback.clone(),
                        Err(error) => return Err(error),
                    },
                    None => match json_module::json_path_get(&value, &path, span) {
                        Ok(found) => lowered_runtime_value(Value::ok(found), span)?,
                        Err(error) => lowered_result_err_value(error),
                    },
                }
            }
            RuntimeOp::JsonRead if values.len() == 1 => {
                let LoweredValue::Path(path) = &values[0] else {
                    return Err(
                        RuntimeError::new("type-error", "json.read expected Path").with_span(span)
                    );
                };
                match read_host_path_string(&self.host_path(path), "json-read", span) {
                    Ok(text) => match json_module::parse_json(&text, span) {
                        Ok(value) => lowered_runtime_value(Value::ok(value), span)?,
                        Err(error) => lowered_result_err_value(error),
                    },
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::JsonRemove if values.len() == 2 => {
                let value = values[0].clone().into_value();
                let path = values[1].clone().into_value();
                lowered_runtime_result(json_module::json_path_remove(&value, &path, span), span)?
            }
            RuntimeOp::JsonSet if values.len() == 3 => {
                let value = values[0].clone().into_value();
                let path = values[1].clone().into_value();
                let replacement = values[2].clone().into_value();
                if let Err(error) = json_module::encode_json(&value, false, span) {
                    lowered_result_err_value(error)
                } else if let Err(error) = json_module::encode_json(&replacement, false, span) {
                    lowered_result_err_value(error)
                } else {
                    lowered_runtime_result(
                        json_module::json_path_set(&value, &path, replacement, span),
                        span,
                    )?
                }
            }
            RuntimeOp::LinuxInterfaces if values.is_empty() => {
                if !self.linux_dry_run() && !self.linux_real() {
                    lowered_result_err_value(RuntimeError::new(
                        "linux-unimplemented",
                        "linux.* boot primitives require XSH_LINUX_DRY_RUN=1 or XSH_LINUX_REAL=1",
                    ).with_span(span))
                } else if self.linux_real() && !self.linux_dry_run() {
                    lowered_runtime_result(linux_module::interfaces(span), span)?
                } else {
                    self.linux_dry_run_log("interfaces", &[], span)?;
                    lowered_result_ok(lowered_stream_from_values(vec![LoweredValue::Record(
                        BTreeMap::from([
                            (Arc::from("name"), LoweredValue::Str("eth0".into())),
                            (
                                Arc::from("flags"),
                                LoweredValue::List(vec![
                                    LoweredValue::Str("UP".into()),
                                    LoweredValue::Str("BROADCAST".into()),
                                    LoweredValue::Str("RUNNING".into()),
                                ]),
                            ),
                            (Arc::from("mtu"), LoweredValue::Int(1500)),
                            (
                                Arc::from("mac"),
                                LoweredValue::Str("02:00:00:00:00:01".into()),
                            ),
                            (
                                Arc::from("addresses"),
                                LoweredValue::List(vec![LoweredValue::Record(BTreeMap::from([
                                    (Arc::from("family"), LoweredValue::Str("inet".into())),
                                    (Arc::from("addr"), LoweredValue::Str("192.0.2.10".into())),
                                    (Arc::from("prefix_len"), LoweredValue::Int(24)),
                                ]))]),
                            ),
                        ]),
                    )]))
                }
            }
            RuntimeOp::LinuxRoutes if values.is_empty() => {
                if !self.linux_dry_run() && !self.linux_real() {
                    lowered_result_err_value(RuntimeError::new(
                        "linux-unimplemented",
                        "linux.* boot primitives require XSH_LINUX_DRY_RUN=1 or XSH_LINUX_REAL=1",
                    ).with_span(span))
                } else if self.linux_real() && !self.linux_dry_run() {
                    lowered_runtime_result(linux_module::routes(span), span)?
                } else {
                    self.linux_dry_run_log("routes", &[], span)?;
                    lowered_result_ok(lowered_stream_from_values(vec![LoweredValue::Record(
                        BTreeMap::from([
                            (Arc::from("family"), LoweredValue::Str("inet".into())),
                            (Arc::from("dst"), LoweredValue::Str("default".into())),
                            (Arc::from("prefix_len"), LoweredValue::Int(0)),
                            (Arc::from("gateway"), LoweredValue::Str("192.0.2.1".into())),
                            (Arc::from("dev"), LoweredValue::Str("eth0".into())),
                            (Arc::from("metric"), LoweredValue::Int(100)),
                            (
                                Arc::from("flags"),
                                LoweredValue::List(vec![
                                    LoweredValue::Str("UP".into()),
                                    LoweredValue::Str("GATEWAY".into()),
                                ]),
                            ),
                        ]),
                    )]))
                }
            }
            RuntimeOp::LinuxLinkUp if values.len() == 1 => {
                let interface = lowered_str_arg_owned(values.pop(), "", "linux.link_up", span)?;
                if !self.linux_dry_run() && !self.linux_real() {
                    lowered_result_err_value(RuntimeError::new(
                        "linux-unimplemented",
                        "linux.* boot primitives require XSH_LINUX_DRY_RUN=1 or XSH_LINUX_REAL=1",
                    ).with_span(span))
                } else if self.linux_real() && !self.linux_dry_run() {
                    lowered_runtime_result(linux_module::link_up(&interface, span), span)?
                } else {
                    self.linux_dry_run_log("link_up", &[("interface", interface)], span)?;
                    lowered_result_ok(LoweredValue::Unit)
                }
            }
            RuntimeOp::LinuxLinkDown if values.len() == 1 => {
                let interface = lowered_str_arg_owned(values.pop(), "", "linux.link_down", span)?;
                if !self.linux_dry_run() && !self.linux_real() {
                    lowered_result_err_value(RuntimeError::new(
                        "linux-unimplemented",
                        "linux.* boot primitives require XSH_LINUX_DRY_RUN=1 or XSH_LINUX_REAL=1",
                    ).with_span(span))
                } else if self.linux_real() && !self.linux_dry_run() {
                    lowered_runtime_result(linux_module::link_down(&interface, span), span)?
                } else {
                    self.linux_dry_run_log("link_down", &[("iface", interface)], span)?;
                    lowered_result_ok(LoweredValue::Unit)
                }
            }
            RuntimeOp::LinuxSetIpv4Address if values.len() == 3 => {
                let netmask = lowered_str_arg_owned(
                    values.get(2).cloned(),
                    "",
                    "linux.set_ipv4_address",
                    span,
                )?;
                let address = lowered_str_arg_owned(
                    values.get(1).cloned(),
                    "",
                    "linux.set_ipv4_address",
                    span,
                )?;
                let interface = lowered_str_arg_owned(
                    values.first().cloned(),
                    "",
                    "linux.set_ipv4_address",
                    span,
                )?;
                if !self.linux_dry_run() && !self.linux_real() {
                    lowered_result_err_value(RuntimeError::new(
                        "linux-unimplemented",
                        "linux.* boot primitives require XSH_LINUX_DRY_RUN=1 or XSH_LINUX_REAL=1",
                    ).with_span(span))
                } else if self.linux_real() && !self.linux_dry_run() {
                    lowered_runtime_result(
                        linux_module::set_ipv4_address(&interface, &address, &netmask, span),
                        span,
                    )?
                } else {
                    self.linux_dry_run_log(
                        "set_ipv4_address",
                        &[
                            ("interface", interface),
                            ("address", address),
                            ("netmask", netmask),
                        ],
                        span,
                    )?;
                    lowered_result_ok(LoweredValue::Unit)
                }
            }
            RuntimeOp::LinuxFlushIpv4Addresses if values.len() == 1 => {
                let interface =
                    lowered_str_arg_owned(values.pop(), "", "linux.flush_ipv4_addresses", span)?;
                if !self.linux_dry_run() && !self.linux_real() {
                    lowered_result_err_value(RuntimeError::new(
                        "linux-unimplemented",
                        "linux.* boot primitives require XSH_LINUX_DRY_RUN=1 or XSH_LINUX_REAL=1",
                    ).with_span(span))
                } else if self.linux_real() && !self.linux_dry_run() {
                    lowered_runtime_result(
                        linux_module::flush_ipv4_addresses(&interface, span),
                        span,
                    )?
                } else {
                    self.linux_dry_run_log("flush_ipv4_addresses", &[("iface", interface)], span)?;
                    lowered_result_ok(LoweredValue::Unit)
                }
            }
            RuntimeOp::LinuxAddDefaultIpv4Route if values.len() == 1 || values.len() == 2 => {
                let interface = lowered_str_arg_owned(
                    values.get(1).cloned(),
                    "",
                    "linux.add_default_ipv4_route",
                    span,
                )?;
                let gateway = lowered_str_arg_owned(
                    values.first().cloned(),
                    "",
                    "linux.add_default_ipv4_route",
                    span,
                )?;
                if !self.linux_dry_run() && !self.linux_real() {
                    lowered_result_err_value(RuntimeError::new(
                        "linux-unimplemented",
                        "linux.* boot primitives require XSH_LINUX_DRY_RUN=1 or XSH_LINUX_REAL=1",
                    ).with_span(span))
                } else if self.linux_real() && !self.linux_dry_run() {
                    lowered_runtime_result(
                        linux_module::add_default_ipv4_route(&gateway, &interface, span),
                        span,
                    )?
                } else {
                    self.linux_dry_run_log(
                        "add_default_ipv4_route",
                        &[("gateway", gateway), ("interface", interface)],
                        span,
                    )?;
                    lowered_result_ok(LoweredValue::Unit)
                }
            }
            RuntimeOp::LinuxDelDefaultIpv4Route if values.len() == 2 => {
                let interface = lowered_str_arg_owned(
                    values.get(1).cloned(),
                    "",
                    "linux.del_default_ipv4_route",
                    span,
                )?;
                let gateway = lowered_str_arg_owned(
                    values.first().cloned(),
                    "",
                    "linux.del_default_ipv4_route",
                    span,
                )?;
                if !self.linux_dry_run() && !self.linux_real() {
                    lowered_result_err_value(RuntimeError::new(
                        "linux-unimplemented",
                        "linux.* boot primitives require XSH_LINUX_DRY_RUN=1 or XSH_LINUX_REAL=1",
                    ).with_span(span))
                } else if self.linux_real() && !self.linux_dry_run() {
                    lowered_runtime_result(
                        linux_module::del_default_ipv4_route(&gateway, &interface, span),
                        span,
                    )?
                } else {
                    self.linux_dry_run_log(
                        "del_default_ipv4_route",
                        &[("gateway", gateway), ("interface", interface)],
                        span,
                    )?;
                    lowered_result_ok(LoweredValue::Unit)
                }
            }
            RuntimeOp::LinuxDhcpSocket if values.len() == 1 => {
                let interface = lowered_str_arg_owned(values.pop(), "", "linux.dhcp_socket", span)?;
                if !self.linux_dry_run() && !self.linux_real() {
                    lowered_result_err_value(RuntimeError::new(
                        "linux-unimplemented",
                        "linux.* boot primitives require XSH_LINUX_DRY_RUN=1 or XSH_LINUX_REAL=1",
                    ).with_span(span))
                } else if self.linux_real() && !self.linux_dry_run() {
                    lowered_runtime_result(linux_module::dhcp_socket(&interface, span), span)?
                } else {
                    self.linux_dry_run_log("dhcp_socket", &[("interface", interface)], span)?;
                    lowered_result_ok(LoweredValue::Int(-1))
                }
            }
            RuntimeOp::LinuxDhcpSend if values.len() == 2 => {
                let fd = lowered_int_arg(values.first().cloned(), "linux.dhcp_send", span)?;
                let payload = lowered_bytes_arg(&values[1], "linux.dhcp_send", span)?;
                if !self.linux_dry_run() && !self.linux_real() {
                    lowered_result_err_value(RuntimeError::new(
                        "linux-unimplemented",
                        "linux.* boot primitives require XSH_LINUX_DRY_RUN=1 or XSH_LINUX_REAL=1",
                    ).with_span(span))
                } else if self.linux_real() && !self.linux_dry_run() {
                    lowered_runtime_result(linux_module::dhcp_send(fd, payload, span), span)?
                } else {
                    self.linux_dry_run_log(
                        "dhcp_send",
                        &[("bytes", payload.len().to_string())],
                        span,
                    )?;
                    lowered_result_ok(LoweredValue::Unit)
                }
            }
            RuntimeOp::LinuxDhcpRecv if values.len() == 2 => {
                let fd = lowered_int_arg(values.first().cloned(), "linux.dhcp_recv", span)?;
                let timeout = lowered_int_arg(values.get(1).cloned(), "linux.dhcp_recv", span)?;
                if !self.linux_dry_run() && !self.linux_real() {
                    lowered_result_err_value(RuntimeError::new(
                        "linux-unimplemented",
                        "linux.* boot primitives require XSH_LINUX_DRY_RUN=1 or XSH_LINUX_REAL=1",
                    ).with_span(span))
                } else if self.linux_real() && !self.linux_dry_run() {
                    lowered_runtime_result(linux_module::dhcp_recv(fd, timeout, span), span)?
                } else {
                    self.linux_dry_run_log(
                        "dhcp_recv",
                        &[("timeout_ms", timeout.to_string())],
                        span,
                    )?;
                    lowered_result_ok(LoweredValue::Bytes(Vec::new().into()))
                }
            }
            RuntimeOp::LinuxDhcpClose if values.len() == 1 => {
                let fd = lowered_int_arg(values.pop(), "linux.dhcp_close", span)?;
                if !self.linux_dry_run() && !self.linux_real() {
                    lowered_result_err_value(RuntimeError::new(
                        "linux-unimplemented",
                        "linux.* boot primitives require XSH_LINUX_DRY_RUN=1 or XSH_LINUX_REAL=1",
                    ).with_span(span))
                } else if self.linux_real() && !self.linux_dry_run() {
                    lowered_runtime_result(linux_module::dhcp_close(fd, span), span)?
                } else {
                    self.linux_dry_run_log("dhcp_close", &[("fd", fd.to_string())], span)?;
                    lowered_result_ok(LoweredValue::Unit)
                }
            }
            RuntimeOp::LinuxDhcpSendRelease if values.len() == 3 => {
                let server_id = lowered_str_arg_owned(
                    values.get(2).cloned(),
                    "",
                    "linux.dhcp_send_release",
                    span,
                )?;
                let address = lowered_str_arg_owned(
                    values.get(1).cloned(),
                    "",
                    "linux.dhcp_send_release",
                    span,
                )?;
                let interface = lowered_str_arg_owned(
                    values.first().cloned(),
                    "",
                    "linux.dhcp_send_release",
                    span,
                )?;
                if !self.linux_dry_run() && !self.linux_real() {
                    lowered_result_err_value(RuntimeError::new(
                        "linux-unimplemented",
                        "linux.* boot primitives require XSH_LINUX_DRY_RUN=1 or XSH_LINUX_REAL=1",
                    ).with_span(span))
                } else if self.linux_real() && !self.linux_dry_run() {
                    lowered_runtime_result(
                        linux_module::dhcp_send_release(&interface, &address, &server_id, span),
                        span,
                    )?
                } else {
                    self.linux_dry_run_log(
                        "dhcp_send_release",
                        &[
                            ("interface", interface),
                            ("address", address),
                            ("server_id", server_id),
                        ],
                        span,
                    )?;
                    lowered_result_ok(LoweredValue::Unit)
                }
            }
            RuntimeOp::MimeParse if values.len() == 1 => {
                let value = lowered_str_arg_owned(values.pop(), "", "mime.parse", span)?;
                match mime_module::parse(&value, span) {
                    Ok(value) => lowered_runtime_value(Value::ok(value), span)?,
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::MimeLookupExt if values.len() == 1 => {
                let ext = lowered_str_arg_owned(values.pop(), "", "mime.lookup_ext", span)?;
                lowered_runtime_value(mime_module::lookup_ext(&ext).unwrap_or(Value::Null), span)?
            }
            RuntimeOp::MimeLookupPath if values.len() == 1 => {
                let path = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "mime.lookup_path",
                    span,
                )?;
                match mime_module::lookup_path(&path.display()) {
                    Some(value) => lowered_runtime_value(Value::ok(value), span)?,
                    None => lowered_result_err_value(
                        RuntimeError::new("mime-lookup", "no MIME entry for path").with_span(span),
                    ),
                }
            }
            RuntimeOp::ModuleLoad if values.len() == 1 => {
                let path = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "module.load",
                    span,
                )?;
                match self.load_dynamic_module(path, span) {
                    Ok(record) => {
                        let Some(module) = lowered_value_from_runtime_any(&Value::Module(record))
                        else {
                            return Err(RuntimeError::new(
                                "type-error",
                                "module.load returned unsupported Module",
                            )
                            .with_span(span));
                        };
                        lowered_result_ok(module)
                    }
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::NetPool if values.len() <= 3 => {
                let name =
                    lowered_str_arg_owned(values.first().cloned(), "default", "net.pool", span)?;
                let max_idle_per_host =
                    lowered_int_arg_or(values.get(1).cloned(), 8, "net.pool", span)?;
                if max_idle_per_host < 0 {
                    lowered_result_err_value(
                        RuntimeError::new("net-pool", "max_idle_per_host cannot be negative")
                            .with_span(span),
                    )
                } else {
                    let idle_timeout = match values.get(2).cloned() {
                        Some(value) => lowered_duration_arg(Some(value), "net.pool", span)?,
                        None => DurationValue { millis: 90_000 },
                    };
                    self.net_pool_options.insert(
                        name.to_string(),
                        net_module::NetPoolOptions {
                            max_idle_per_host: max_idle_per_host as usize,
                            idle_timeout: Duration::from_millis(idle_timeout.millis),
                        },
                    );
                    self.net_agents.retain(|key, _| key.pool != name);
                    lowered_result_ok(LoweredValue::Record(BTreeMap::from([
                        (Arc::from("name"), LoweredValue::Str(name.into())),
                        (
                            Arc::from("max_idle_per_host"),
                            LoweredValue::Int(max_idle_per_host),
                        ),
                        (
                            Arc::from("idle_timeout_ms"),
                            LoweredValue::Int(idle_timeout.millis as i64),
                        ),
                    ])))
                }
            }
            RuntimeOp::NetClosePool if values.len() <= 1 => {
                let name = lowered_str_arg_owned(values.pop(), "default", "net.close_pool", span)?;
                self.net_agents.retain(|key, _| key.pool != name);
                lowered_result_ok(LoweredValue::Unit)
            }
            RuntimeOp::NetCloseAllPools if values.is_empty() => {
                self.net_agents.clear();
                lowered_result_ok(LoweredValue::Unit)
            }
            RuntimeOp::NetRequest if values.len() == 1 => {
                let record = lowered_record_arg(values.pop(), "net.request", span)?;
                if let Some(value) =
                    intercept_test_host_call(self, "net.request", record.clone(), span)
                {
                    lowered_runtime_value(value, span)?
                } else {
                    let options = self.net_call_options(&record, span)?;
                    let request = self.net_request_from_record(record, span)?;
                    match self.net_agent(&options, span) {
                        Ok(agent) => match net_module::request(&agent, request, span) {
                            Ok(value) => lowered_runtime_value(value, span)?,
                            Err(error) => lowered_result_err_value(error),
                        },
                        Err(error) => lowered_result_err_value(error),
                    }
                }
            }
            RuntimeOp::NetRequestMany if values.len() == 1 => {
                let batch = lowered_record_arg(values.pop(), "net.request_many", span)?;
                if let Some(value) =
                    intercept_test_host_call(self, "net.request_many", batch.clone(), span)
                {
                    lowered_runtime_value(value, span)?
                } else {
                    let Some(values) = batch.get("requests") else {
                        return Ok(ControlFlow::Continue(lowered_result_err_value(
                            RuntimeError::new("net-request-many", "requests is required")
                                .with_span(span),
                        )));
                    };
                    let Value::List(records) = values else {
                        return Ok(ControlFlow::Continue(lowered_result_err_value(
                            RuntimeError::new("type-error", "requests must be List[Record]")
                                .with_span(span),
                        )));
                    };
                    let requests = records
                        .iter()
                        .map(|value| match value {
                            Value::Record(record) => {
                                self.net_request_from_record(record.clone(), span)
                            }
                            _ => Err(RuntimeError::new(
                                "type-error",
                                "requests must be List[Record]",
                            )
                            .with_span(span)),
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    let concurrency = match batch.get("concurrency") {
                        Some(Value::Int(value)) if *value >= 0 => *value as usize,
                        Some(Value::Int(_)) => {
                            return Ok(ControlFlow::Continue(lowered_result_err_value(
                                RuntimeError::new(
                                    "net-concurrency",
                                    "concurrency must be at least one",
                                )
                                .with_span(span),
                            )));
                        }
                        Some(_) => {
                            return Ok(ControlFlow::Continue(lowered_result_err_value(
                                RuntimeError::new("type-error", "concurrency must be Int")
                                    .with_span(span),
                            )));
                        }
                        None => 16,
                    };
                    if concurrency == 0 {
                        return Ok(ControlFlow::Continue(lowered_result_err_value(
                            RuntimeError::new(
                                "net-concurrency",
                                "concurrency must be at least one",
                            )
                            .with_span(span),
                        )));
                    }
                    let options = self.net_call_options(&batch, span)?;
                    match self.net_agent(&options, span) {
                        Ok(agent) => {
                            match net_module::request_many(&agent, requests, concurrency, span) {
                                Ok(value) => lowered_runtime_value(value, span)?,
                                Err(error) => lowered_result_err_value(error),
                            }
                        }
                        Err(error) => lowered_result_err_value(error),
                    }
                }
            }
            RuntimeOp::NetDownloadMany if values.len() == 1 => {
                let batch = lowered_record_arg(values.pop(), "net.download_many", span)?;
                if let Some(value) =
                    intercept_test_host_call(self, "net.download_many", batch.clone(), span)
                {
                    lowered_runtime_value(value, span)?
                } else {
                    let Some(values) = batch.get("downloads") else {
                        return Ok(ControlFlow::Continue(lowered_result_err_value(
                            RuntimeError::new("net-download-many", "downloads is required")
                                .with_span(span),
                        )));
                    };
                    let Value::List(records) = values else {
                        return Ok(ControlFlow::Continue(lowered_result_err_value(
                            RuntimeError::new("type-error", "downloads must be List[Record]")
                                .with_span(span),
                        )));
                    };
                    let downloads = records
                        .iter()
                        .map(|value| match value {
                            Value::Record(record) => {
                                self.net_download_from_record(record.clone(), span)
                            }
                            _ => Err(RuntimeError::new(
                                "type-error",
                                "downloads must be List[Record]",
                            )
                            .with_span(span)),
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    let concurrency = match batch.get("concurrency") {
                        Some(Value::Int(value)) if *value >= 0 => *value as usize,
                        Some(Value::Int(_)) => {
                            return Ok(ControlFlow::Continue(lowered_result_err_value(
                                RuntimeError::new(
                                    "net-concurrency",
                                    "concurrency must be at least one",
                                )
                                .with_span(span),
                            )));
                        }
                        Some(_) => {
                            return Ok(ControlFlow::Continue(lowered_result_err_value(
                                RuntimeError::new("type-error", "concurrency must be Int")
                                    .with_span(span),
                            )));
                        }
                        None => 16,
                    };
                    if concurrency == 0 {
                        return Ok(ControlFlow::Continue(lowered_result_err_value(
                            RuntimeError::new(
                                "net-concurrency",
                                "concurrency must be at least one",
                            )
                            .with_span(span),
                        )));
                    }
                    let options = self.net_call_options(&batch, span)?;
                    match self.net_agent(&options, span) {
                        Ok(agent) => {
                            match net_module::download_many(&agent, downloads, concurrency, span) {
                                Ok(value) => lowered_runtime_value(value, span)?,
                                Err(error) => lowered_result_err_value(error),
                            }
                        }
                        Err(error) => lowered_result_err_value(error),
                    }
                }
            }
            RuntimeOp::NetDownload if values.len() == 1 => {
                let record = lowered_record_arg(values.pop(), "net.download", span)?;
                if let Some(value) =
                    intercept_test_host_call(self, "net.download", record.clone(), span)
                {
                    lowered_runtime_value(value, span)?
                } else {
                    let options = self.net_call_options(&record, span)?;
                    let download = self.net_download_from_record(record, span)?;
                    match self.net_agent(&options, span) {
                        Ok(agent) => match net_module::download(&agent, download, span) {
                            Ok(value) => lowered_runtime_value(value, span)?,
                            Err(error) => lowered_result_err_value(error),
                        },
                        Err(error) => lowered_result_err_value(error),
                    }
                }
            }
            RuntimeOp::NetUpload if values.len() == 1 => {
                let record = lowered_record_arg(values.pop(), "net.upload", span)?;
                if let Some(value) =
                    intercept_test_host_call(self, "net.upload", record.clone(), span)
                {
                    lowered_runtime_value(value, span)?
                } else {
                    let options = self.net_call_options(&record, span)?;
                    let upload = self.net_upload_from_record(record, span)?;
                    match self.net_agent(&options, span) {
                        Ok(agent) => match net_module::upload(&agent, upload, span) {
                            Ok(value) => lowered_runtime_value(value, span)?,
                            Err(error) => lowered_result_err_value(error),
                        },
                        Err(error) => lowered_result_err_value(error),
                    }
                }
            }
            RuntimeOp::PathAbsolute if values.len() == 1 => {
                let LoweredValue::Path(path) = values.pop().expect("checked value length") else {
                    return Err(
                        RuntimeError::new("type-error", "Path.absolute expected Path")
                            .with_span(span),
                    );
                };
                match path_absolute_value(&self.cwd, &path) {
                    Ok(path) => lowered_result_ok(LoweredValue::Path(path)),
                    Err(error) => lowered_result_err_value(error.with_span(span)),
                }
            }
            RuntimeOp::PathParseBytes if values.len() == 1 => {
                let value = values.pop().expect("checked value length");
                let bytes = lowered_bytes_arg(&value, "Path.parse_bytes", span)?;
                match PathValue::new(bytes.to_vec()) {
                    Ok(path) => lowered_result_ok(LoweredValue::Path(path)),
                    Err(error) => lowered_result_err_value(error.with_span(span)),
                }
            }
            RuntimeOp::PatchApply
                if values.len() == 2 || values.len() == 3 || values.len() == 4 =>
            {
                let overwrite =
                    lowered_bool_arg_or(values.get(3).cloned(), false, "patch.apply", span)?;
                let strip_components = match values.get(2).cloned() {
                    Some(value) => lowered_int_arg(Some(value), "patch.apply", span)?,
                    None => 0,
                };
                let text = lowered_str_arg_owned(values.get(1).cloned(), "", "patch.apply", span)?;
                let root = lowered_path_arg(values.remove(0), "patch.apply", span)?;
                match patch_module::apply(
                    self.host_path(&root),
                    &text,
                    strip_components,
                    overwrite,
                    span,
                ) {
                    Ok(value) => lowered_runtime_value(value, span)?,
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::RecordRequire if (2..=4).contains(&values.len()) => {
                let record = lowered_record_arg(values.first().cloned(), "record.require", span)?;
                let required = lowered_record_arg(values.get(1).cloned(), "record.require", span)?;
                let mut optional = RecordMap::new();
                let mut source = None;
                if let Some(value) = values.get(2).cloned() {
                    match value {
                        LoweredValue::Path(path) => source = Some(path.display()),
                        other => {
                            optional = lowered_record_arg(Some(other), "record.require", span)?
                        }
                    }
                }
                if let Some(value) = values.get(3).cloned() {
                    let path = lowered_path_arg(value, "record.require", span)?;
                    source = Some(path.display());
                }
                match validate_module_contract(
                    &self.module_export_signatures,
                    &record,
                    &required,
                    &optional,
                    source.as_deref(),
                ) {
                    Ok(()) => lowered_runtime_value(Value::ok(Value::Record(record)), span)?,
                    Err(message) => lowered_result_err_value(
                        RuntimeError::new("record-contract", message).with_span(span),
                    ),
                }
            }
            RuntimeOp::ProcessList if values.is_empty() => {
                lowered_runtime_stream_result(process_module::list_processes(span), span)?
            }
            RuntimeOp::ProcessThreads if values.is_empty() || values.len() == 1 => {
                let pid = match values.pop() {
                    Some(value) => Some(lowered_int_arg(Some(value), "process.threads", span)?),
                    None => None,
                };
                lowered_runtime_stream_result(process_module::list_threads(pid, span), span)?
            }
            RuntimeOp::ProcessCurrentPid if values.is_empty() => {
                lowered_result_ok(LoweredValue::Int(std::process::id() as i64))
            }
            RuntimeOp::ProcessStats if values.len() == 1 => {
                let pid = lowered_int_arg(values.pop(), "process.stats", span)?;
                lowered_runtime_result(process_module::process_stats(pid, span), span)?
            }
            RuntimeOp::ProcessWhich if values.len() == 1 => {
                let name = lowered_str_arg_owned(values.pop(), "", "process.which", span)?;
                if name.is_empty() || name.contains('\0') {
                    lowered_result_err_value(
                        RuntimeError::new(
                            "process-which",
                            "command name cannot be empty or contain NUL",
                        )
                        .with_span(span),
                    )
                } else {
                    let invocation = ProcessInvocation {
                        target: name.into_bytes(),
                        argv: Vec::new(),
                        cwd: self.cwd.clone(),
                        env: self.env.snapshot_clone(),
                        env_overlay: BTreeMap::new(),
                        redirections: Vec::new(),
                        timeout: None,
                        cpu_max: None,
                    };
                    match resolve_executable(&invocation)
                        .map_err(|error| run_error_to_runtime(error, span))
                        .and_then(|path| {
                            path_value_from_pathbuf(path).map_err(|error| error.with_span(span))
                        }) {
                        Ok(path) => lowered_result_ok(LoweredValue::Path(path)),
                        Err(error) => lowered_result_err_value(error),
                    }
                }
            }
            RuntimeOp::ProcessPort if values.len() == 1 => {
                let port = lowered_int_arg(values.pop(), "process.port", span)?;
                lowered_runtime_stream_result(process_module::port_processes(port, span), span)?
            }
            RuntimeOp::ProcessPorts if values.is_empty() => {
                lowered_runtime_stream_result(process_module::listening_port_processes(span), span)?
            }
            RuntimeOp::ProcessPortsForPid if values.len() == 1 => {
                let pid = lowered_int_arg(values.pop(), "process.ports", span)?;
                lowered_runtime_stream_result(process_module::pid_port_processes(pid, span), span)?
            }
            RuntimeOp::ProcessSignal if values.len() == 1 => {
                let signal = lowered_str_arg_owned(values.pop(), "", "process.signal", span)?;
                lowered_runtime_result(
                    process_module::signal_info(&signal, span).map(process_module::signal_record),
                    span,
                )?
            }
            RuntimeOp::ProcessKill if values.len() == 1 || values.len() == 2 => {
                let signal = lowered_str_arg_owned(values.pop(), "TERM", "process.kill", span)?;
                let pid = lowered_int_arg(values.pop(), "process.kill", span)?;
                lowered_module_result_value(self.process_kill(pid, &signal, span), span)?
            }
            RuntimeOp::ProcessArgvWords if values.len() == 1 => {
                let text = lowered_str_arg_owned(values.pop(), "", "process.argv_words", span)?;
                lowered_runtime_result(
                    process_module::argv_words(&text, span).map(|words| {
                        Value::List(
                            words
                                .into_iter()
                                .map(|word| Value::Str(word.into()))
                                .collect(),
                        )
                    }),
                    span,
                )?
            }
            RuntimeOp::ProcessRun if values.len() == 1 => {
                let plan = lowered_command_arg(
                    values.pop().expect("checked value length"),
                    "process.run",
                    span,
                )?;
                let invocation = self.invocation_from_command_plan(&plan, span)?;
                self.trace_process_run_start(span, &invocation);
                let outcome = if self.capture_process_output {
                    run_capture_with_stderr_policy(&invocation, self).map(|output| {
                        self.stdout.extend_from_slice(&output.stdout);
                        self.stderr.extend_from_slice(&output.stderr);
                        output.end
                    })
                } else {
                    run_inherit_with_policy(&invocation, self)
                };
                match outcome {
                    Ok(end) => {
                        let status = end.status.clone().expect("completed process has status");
                        self.last_status = Some(status.clone());
                        self.trace_process_run_end(span, &end);
                        lowered_result_ok(LoweredValue::Status(Box::new(status)))
                    }
                    Err(error) => {
                        let error = error.with_span(span);
                        let end = ProcessEnd {
                            pid: None,
                            status: error.status.as_deref().cloned(),
                            error: Some(error.clone()),
                        };
                        if let Some(status) = &end.status {
                            self.last_status = Some(status.clone());
                        }
                        self.trace_process_run_end(span, &end);
                        if self.signal_state.shutdown_complete
                            && self.signal_state.shutdown_status.is_some()
                        {
                            let status = end
                                .status
                                .clone()
                                .unwrap_or_else(|| ProcessStatus::signaled(libc::SIGTERM));
                            lowered_result_ok(LoweredValue::Status(Box::new(status)))
                        } else {
                            LoweredValue::ResultErr(Box::new(Value::RunError(Box::new(error))))
                        }
                    }
                }
            }
            RuntimeOp::ProcessSpawn if values.len() == 1 => {
                let plan = lowered_command_arg(
                    values.pop().expect("checked value length"),
                    "process.spawn",
                    span,
                )?;
                let invocation = self.invocation_from_command_plan(&plan, span)?;
                let options = SpawnOptions {
                    detach: plan.detach,
                    new_session: plan.new_session,
                    ignore_hup: plan.ignore_hup,
                };
                match spawn_command(&invocation, options) {
                    Ok(started) => lowered_result_ok(LoweredValue::Record(BTreeMap::from([
                        (Arc::from("pid"), LoweredValue::Int(started.pid as i64)),
                        (
                            Arc::from("command"),
                            LoweredValue::Str(
                                String::from_utf8_lossy(&started.target).into_owned().into(),
                            ),
                        ),
                        (
                            Arc::from("argv"),
                            LoweredValue::Str(
                                display_spawn_argv(&started.target, &started.argv).into(),
                            ),
                        ),
                        (
                            Arc::from("detach"),
                            LoweredValue::Bool(started.options.detach),
                        ),
                        (
                            Arc::from("new_session"),
                            LoweredValue::Bool(started.options.new_session),
                        ),
                        (
                            Arc::from("ignore_hup"),
                            LoweredValue::Bool(started.options.ignore_hup),
                        ),
                    ]))),
                    Err(error) => lowered_result_err_value(run_error_to_runtime(error, span)),
                }
            }
            RuntimeOp::ProcessWaitAny if values.len() == 1 => {
                let handles = match lowered_process_handle_list_arg(
                    values.pop().expect("checked value length"),
                    "process.wait_any",
                    span,
                )? {
                    Ok(handles) => handles,
                    Err(error) => {
                        return Ok(ControlFlow::Continue(lowered_process_run_error(error)));
                    }
                };

                loop {
                    for (index, handle) in handles.iter().enumerate() {
                        let Some(live) = self.process_handles.get_mut(&handle.id) else {
                            return Ok(ControlFlow::Continue(lowered_process_run_error(
                                RunError::new("unknown", "process handle is no longer live")
                                    .with_span(span),
                            )));
                        };
                        match poll_managed(&mut live.child) {
                            Ok(
                                ChildWaitOutcome::Exited(status)
                                | ChildWaitOutcome::Signaled(status),
                            ) => {
                                let pid = live.child.pid;
                                let group = live.child.process_group();
                                let _ = live;
                                self.process_handles.remove(&handle.id);
                                <Self as CancellationPolicy>::process_group_finished(self, group);
                                self.last_status = Some(status.clone());
                                return Ok(ControlFlow::Continue(lowered_result_ok(
                                    lowered_process_wait_any_record(index, pid, status),
                                )));
                            }
                            Ok(
                                ChildWaitOutcome::Stopped { .. } | ChildWaitOutcome::StillRunning,
                            ) => {
                                if lowered_timeout_elapsed(live.child.deadline) {
                                    live.child.process_group().kill();
                                }
                            }
                            Err(error) => {
                                return Ok(ControlFlow::Continue(lowered_process_run_error(
                                    error.with_span(span),
                                )));
                            }
                        }
                    }

                    std::thread::sleep(Duration::from_millis(1));
                }
            }
            RuntimeOp::ProcessWaitReady if values.len() == 1 => {
                let handles = match lowered_process_handle_list_arg(
                    values.pop().expect("checked value length"),
                    "process.wait_ready",
                    span,
                )? {
                    Ok(handles) => handles,
                    Err(error) => {
                        return Ok(ControlFlow::Continue(lowered_process_run_error(error)));
                    }
                };

                loop {
                    let mut completed = Vec::new();
                    let mut completed_ids = rustc_hash::FxHashSet::default();

                    for (index, handle) in handles.iter().enumerate() {
                        let Some(live) = self.process_handles.get_mut(&handle.id) else {
                            return Ok(ControlFlow::Continue(lowered_process_run_error(
                                RunError::new("unknown", "process handle is no longer live")
                                    .with_span(span),
                            )));
                        };
                        match poll_managed(&mut live.child) {
                            Ok(
                                ChildWaitOutcome::Exited(status)
                                | ChildWaitOutcome::Signaled(status),
                            ) => {
                                completed_ids.insert(handle.id);
                                completed.push((
                                    index,
                                    handle.id,
                                    live.child.pid,
                                    live.child.process_group(),
                                    status,
                                ));
                            }
                            Ok(
                                ChildWaitOutcome::Stopped { .. } | ChildWaitOutcome::StillRunning,
                            ) => {
                                if lowered_timeout_elapsed(live.child.deadline) {
                                    live.child.process_group().kill();
                                }
                            }
                            Err(error) => {
                                return Ok(ControlFlow::Continue(lowered_process_run_error(
                                    error.with_span(span),
                                )));
                            }
                        }
                    }

                    if !completed.is_empty() {
                        let drain_until = Instant::now() + Duration::from_millis(1);

                        while completed.len() < handles.len() && Instant::now() < drain_until {
                            let mut drained = false;

                            for (index, handle) in handles.iter().enumerate() {
                                if completed_ids.contains(&handle.id) {
                                    continue;
                                }

                                let Some(live) = self.process_handles.get_mut(&handle.id) else {
                                    return Ok(ControlFlow::Continue(lowered_process_run_error(
                                        RunError::new(
                                            "unknown",
                                            "process handle is no longer live",
                                        )
                                        .with_span(span),
                                    )));
                                };
                                match poll_managed(&mut live.child) {
                                    Ok(
                                        ChildWaitOutcome::Exited(status)
                                        | ChildWaitOutcome::Signaled(status),
                                    ) => {
                                        completed_ids.insert(handle.id);
                                        completed.push((
                                            index,
                                            handle.id,
                                            live.child.pid,
                                            live.child.process_group(),
                                            status,
                                        ));
                                        drained = true;
                                    }
                                    Ok(
                                        ChildWaitOutcome::Stopped { .. }
                                        | ChildWaitOutcome::StillRunning,
                                    ) => {
                                        if lowered_timeout_elapsed(live.child.deadline) {
                                            live.child.process_group().kill();
                                        }
                                    }
                                    Err(error) => {
                                        return Ok(ControlFlow::Continue(
                                            lowered_process_run_error(error.with_span(span)),
                                        ));
                                    }
                                }
                            }

                            if !drained {
                                std::thread::sleep(Duration::from_micros(250));
                            }
                        }

                        let mut values = Vec::with_capacity(completed.len());
                        for (index, id, pid, group, status) in completed {
                            self.process_handles.remove(&id);
                            <Self as CancellationPolicy>::process_group_finished(self, group);
                            self.last_status = Some(status.clone());
                            values.push(lowered_process_wait_any_record(index, pid, status));
                        }

                        return Ok(ControlFlow::Continue(lowered_result_ok(
                            LoweredValue::List(values),
                        )));
                    }

                    std::thread::sleep(Duration::from_millis(1));
                }
            }
            RuntimeOp::RegexCompile if values.len() == 1 => {
                let text = lowered_str_arg_owned(values.pop(), "", "regex.compile", span)?;
                match regex_module::compile(&text, span) {
                    Ok(regex) => lowered_result_ok(LoweredValue::Regex(Box::new(RegexValue {
                        pattern: text,
                        regex: Arc::new(regex),
                    }))),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::SetEmpty if values.is_empty() => LoweredValue::Map(BTreeMap::new()),
            RuntimeOp::SetFrom if values.len() == 1 => {
                let items = lowered_str_list_arg(values.pop(), "set.from", span)?;
                let mut set = BTreeMap::new();
                for item in items {
                    set.insert(item, LoweredValue::Bool(true));
                }
                LoweredValue::Map(set)
            }
            RuntimeOp::SetHas if values.len() == 2 => {
                let item = lowered_str_arg_owned(values.pop(), "", "set.has", span)?;
                let set = lowered_bool_map_arg(values.pop(), "set.has", span)?;
                LoweredValue::Bool(set.contains_key(&item))
            }
            RuntimeOp::SetAdd if values.len() == 2 => {
                let item = lowered_str_arg_owned(values.pop(), "", "set.add", span)?;
                let mut set = lowered_bool_map_arg(values.pop(), "set.add", span)?;
                set.insert(item, LoweredValue::Bool(true));
                LoweredValue::Map(set)
            }
            RuntimeOp::SetRemove if values.len() == 2 => {
                let item = lowered_str_arg_owned(values.pop(), "", "set.remove", span)?;
                let mut set = lowered_bool_map_arg(values.pop(), "set.remove", span)?;
                set.remove(&item);
                LoweredValue::Map(set)
            }
            RuntimeOp::ShlexQuote if values.len() == 1 => {
                let text = lowered_str_arg_owned(values.pop(), "", "shlex.quote", span)?;
                LoweredValue::Str(shlex::quote(&text).into())
            }
            RuntimeOp::ShlexJoin if values.len() == 1 => {
                let argv = lowered_str_list_arg(values.pop(), "shlex.join", span)?;
                LoweredValue::Str(shlex::join(&argv).into())
            }
            RuntimeOp::SystemHostname if values.is_empty() => lowered_runtime_result(
                system::hostname(span).map(|hostname| Value::Str(hostname.into())),
                span,
            )?,
            RuntimeOp::SystemUname if values.is_empty() => {
                lowered_runtime_result(system::uname(span), span)?
            }
            RuntimeOp::SystemMemory if values.is_empty() => {
                lowered_runtime_result(system::memory(span), span)?
            }
            RuntimeOp::SystemOsRelease if values.is_empty() => {
                lowered_runtime_result(system::os_release(span), span)?
            }
            #[cfg(feature = "native-tests")]
            RuntimeOp::TestOk if values.len() == 1 || values.len() == 2 => {
                let message = lowered_str_arg_owned(
                    values.get(1).cloned(),
                    "condition was false",
                    "test.ok",
                    span,
                )?;
                let Some(LoweredValue::Bool(condition)) = values.first() else {
                    return Err(
                        RuntimeError::new("type-error", "test.ok expected Bool").with_span(span)
                    );
                };
                if *condition {
                    lowered_result_ok(LoweredValue::Unit)
                } else {
                    lowered_runtime_value(test_failure(message), span)?
                }
            }
            #[cfg(feature = "native-tests")]
            RuntimeOp::TestEq if values.len() == 2 || values.len() == 3 => {
                let left = values[0].clone().into_value();
                let right = values[1].clone().into_value();
                let message = lowered_str_arg_owned(values.get(2).cloned(), "", "test.eq", span)?;
                if left == right {
                    lowered_result_ok(LoweredValue::Unit)
                } else {
                    let detail = if message.is_empty() {
                        format!(
                            "expected equality, left={}, right={}",
                            display_value(&left, span)?,
                            display_value(&right, span)?
                        )
                    } else {
                        message
                    };
                    lowered_runtime_value(test_failure(detail), span)?
                }
            }
            #[cfg(feature = "native-tests")]
            RuntimeOp::TestNe if values.len() == 2 || values.len() == 3 => {
                let left = values[0].clone().into_value();
                let right = values[1].clone().into_value();
                let message = lowered_str_arg_owned(values.get(2).cloned(), "", "test.ne", span)?;
                if left != right {
                    lowered_result_ok(LoweredValue::Unit)
                } else {
                    let detail = if message.is_empty() {
                        format!("expected inequality, both={}", display_value(&left, span)?)
                    } else {
                        message
                    };
                    lowered_runtime_value(test_failure(detail), span)?
                }
            }
            #[cfg(feature = "native-tests")]
            RuntimeOp::TestContains if values.len() == 2 || values.len() == 3 => {
                let haystack = values[0].clone().into_value();
                let needle = values[1].clone().into_value();
                let message =
                    lowered_str_arg_owned(values.get(2).cloned(), "", "test.contains", span)?;
                if test_contains_value(&haystack, &needle) {
                    lowered_result_ok(LoweredValue::Unit)
                } else {
                    let detail = if message.is_empty() {
                        format!(
                            "expected {} to contain {}",
                            display_value(&haystack, span)?,
                            display_value(&needle, span)?
                        )
                    } else {
                        message
                    };
                    lowered_runtime_value(test_failure(detail), span)?
                }
            }
            #[cfg(feature = "native-tests")]
            RuntimeOp::TestNotContains if values.len() == 2 || values.len() == 3 => {
                let haystack = values[0].clone().into_value();
                let needle = values[1].clone().into_value();
                let message =
                    lowered_str_arg_owned(values.get(2).cloned(), "", "test.not_contains", span)?;
                if !test_contains_value(&haystack, &needle) {
                    lowered_result_ok(LoweredValue::Unit)
                } else {
                    let detail = if message.is_empty() {
                        format!(
                            "expected {} to not contain {}",
                            display_value(&haystack, span)?,
                            display_value(&needle, span)?
                        )
                    } else {
                        message
                    };
                    lowered_runtime_value(test_failure(detail), span)?
                }
            }
            #[cfg(feature = "native-tests")]
            RuntimeOp::TestErrorKind if values.len() == 2 || values.len() == 3 => {
                let value = values[0].clone().into_value();
                let expected =
                    lowered_str_arg_owned(values.get(1).cloned(), "", "test.error_kind", span)?;
                let message =
                    lowered_str_arg_owned(values.get(2).cloned(), "", "test.error_kind", span)?;
                let actual = test_error_kind(&value);
                if actual.as_deref() == Some(expected.as_str()) {
                    lowered_result_ok(LoweredValue::Unit)
                } else {
                    let detail = if message.is_empty() {
                        format!(
                            "expected error kind `{expected}`, found `{}`",
                            actual.unwrap_or_else(|| "none".to_string())
                        )
                    } else {
                        message
                    };
                    lowered_runtime_value(test_failure(detail), span)?
                }
            }
            #[cfg(feature = "native-tests")]
            RuntimeOp::TestFail if values.is_empty() || values.len() == 1 => {
                let message =
                    lowered_str_arg_owned(values.pop(), "test failed", "test.fail", span)?;
                lowered_runtime_value(test_failure(message), span)?
            }
            #[cfg(feature = "native-tests")]
            RuntimeOp::TestSkip if values.is_empty() || values.len() == 1 => {
                let message =
                    lowered_str_arg_owned(values.pop(), "test skipped", "test.skip", span)?;
                lowered_runtime_value(
                    Value::err(Value::Error(Box::new(RuntimeError::new(
                        "test-skip",
                        message,
                    )))),
                    span,
                )?
            }
            #[cfg(feature = "native-tests")]
            RuntimeOp::TestTempPath if values.len() == 1 || values.len() == 2 => {
                let ctx = lowered_record_arg(values.first().cloned(), "test.temp_path", span)?;
                let name =
                    lowered_str_arg_owned(values.get(1).cloned(), "", "test.temp_path", span)?;
                LoweredValue::Path(test_temp_path(self, &ctx, &name, span)?)
            }
            #[cfg(feature = "native-tests")]
            RuntimeOp::TestTempDir if values.len() == 1 || values.len() == 2 => {
                let ctx = lowered_record_arg(values.first().cloned(), "test.temp_dir", span)?;
                let name =
                    lowered_str_arg_owned(values.get(1).cloned(), "", "test.temp_dir", span)?;
                let path = test_temp_path(self, &ctx, &name, span)?;
                match create_host_dir_all(&self.host_path(&path), "test-temp", span) {
                    Ok(()) => lowered_result_ok(LoweredValue::Path(path)),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            #[cfg(feature = "native-tests")]
            RuntimeOp::TestTempFile if (1..=3).contains(&values.len()) => {
                let ctx = lowered_record_arg(values.first().cloned(), "test.temp_file", span)?;
                let name =
                    lowered_str_arg_owned(values.get(1).cloned(), "", "test.temp_file", span)?;
                let contents =
                    lowered_bytes_arg_or_empty(values.get(2).cloned(), "test.temp_file", span)?;
                let path = test_temp_path(self, &ctx, &name, span)?;
                let host_path = self.host_path(&path);
                if let Some(parent) = host_path.parent()
                    && let Err(error) = create_host_dir_all(parent, "test-temp", span)
                {
                    return Ok(ControlFlow::Continue(lowered_result_err_value(error)));
                }
                match crate::modules::fs::write_path(host_path, &contents, span) {
                    Ok(()) => lowered_result_ok(LoweredValue::Path(path)),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            #[cfg(feature = "native-tests")]
            RuntimeOp::TestMock if (4..=5).contains(&values.len()) => {
                let _ctx = lowered_record_arg(values.first().cloned(), "test.mock", span)?;
                let op = lowered_str_arg_owned(values.get(1).cloned(), "", "test.mock", span)?;
                let matcher = lowered_record_arg(values.get(2).cloned(), "test.mock", span)?;
                let result = values[3].clone().into_value();
                let times = lowered_int_arg_or(values.get(4).cloned(), 1, "test.mock", span)?;
                if times < 1 {
                    lowered_result_err_value(
                        RuntimeError::new("test-mock", "times must be at least 1").with_span(span),
                    )
                } else if let Some(expected) = test_mock_expected_return_type(&op)
                    && !test_value_matches_type(&result, &expected)
                {
                    lowered_result_err_value(
                        RuntimeError::new(
                            "test-mock",
                            format!("mock result for `{op}` must match {expected}"),
                        )
                        .with_span(span),
                    )
                } else {
                    self.test_mocks
                        .entry(op.to_string())
                        .or_default()
                        .push(TestMock {
                            matcher,
                            result,
                            remaining: times,
                        });
                    lowered_result_ok(LoweredValue::Unit)
                }
            }
            #[cfg(feature = "native-tests")]
            RuntimeOp::TestCalls if values.len() == 1 || values.len() == 2 => {
                let _ctx = lowered_record_arg(values.first().cloned(), "test.calls", span)?;
                let op = lowered_str_arg_owned(values.get(1).cloned(), "", "test.calls", span)?;
                let mut calls = Vec::new();
                for call in self
                    .test_calls
                    .iter()
                    .filter(|call| op.is_empty() || call.op == op)
                {
                    let args = lowered_runtime_value(Value::Record(call.args.clone()), span)?;
                    calls.push(LoweredValue::Record(BTreeMap::from([
                        (Arc::from("op"), LoweredValue::Str(call.op.as_str().into())),
                        (Arc::from("args"), args),
                    ])));
                }
                LoweredValue::List(calls)
            }
            #[cfg(feature = "native-tests")]
            RuntimeOp::TestRunScript if (2..=6).contains(&values.len()) => {
                let ctx = lowered_record_arg(values.first().cloned(), "test.run_script", span)?;
                let source =
                    lowered_str_arg_owned(values.get(1).cloned(), "", "test.run_script", span)?;
                let args =
                    lowered_optional_str_list(values.get(2).cloned(), "test.run_script", span)?;
                let env =
                    lowered_optional_str_record(values.get(3).cloned(), "test.run_script", span)?;
                let stdin =
                    lowered_bytes_arg_or_empty(values.get(4).cloned(), "test.run_script", span)?;
                let name = lowered_str_arg_owned(
                    values.get(5).cloned(),
                    "script.xsh",
                    "test.run_script",
                    span,
                )?;
                match self.lowered_test_run_script(&ctx, &source, &args, &env, &stdin, &name, span)
                {
                    Ok(record) => lowered_result_ok(LoweredValue::Record(record)),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            #[cfg(feature = "native-tests")]
            RuntimeOp::TestRunXsh if (2..=7).contains(&values.len()) => {
                let ctx = lowered_record_arg(values.first().cloned(), "test.run_xsh", span)?;
                let source =
                    lowered_str_arg_owned(values.get(1).cloned(), "", "test.run_xsh", span)?;
                let xsh_args =
                    lowered_optional_str_list(values.get(2).cloned(), "test.run_xsh", span)?;
                let script_args =
                    lowered_optional_str_list(values.get(3).cloned(), "test.run_xsh", span)?;
                let env =
                    lowered_optional_str_record(values.get(4).cloned(), "test.run_xsh", span)?;
                let stdin =
                    lowered_bytes_arg_or_empty(values.get(5).cloned(), "test.run_xsh", span)?;
                let name = lowered_str_arg_owned(
                    values.get(6).cloned(),
                    "script.xsh",
                    "test.run_xsh",
                    span,
                )?;
                match self.lowered_test_run_xsh(
                    &ctx,
                    &source,
                    &xsh_args,
                    &script_args,
                    &env,
                    &stdin,
                    &name,
                    span,
                ) {
                    Ok(record) => lowered_result_ok(LoweredValue::Record(record)),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            #[cfg(feature = "native-tests")]
            RuntimeOp::TestRunXshtTrace if (2..=7).contains(&values.len()) => {
                let ctx = lowered_record_arg(values.first().cloned(), "test.run_xsht_trace", span)?;
                let source =
                    lowered_str_arg_owned(values.get(1).cloned(), "", "test.run_xsht_trace", span)?;
                let trace_args =
                    lowered_optional_str_list(values.get(2).cloned(), "test.run_xsht_trace", span)?;
                let script_args =
                    lowered_optional_str_list(values.get(3).cloned(), "test.run_xsht_trace", span)?;
                let env = lowered_optional_str_record(
                    values.get(4).cloned(),
                    "test.run_xsht_trace",
                    span,
                )?;
                let stdin = lowered_bytes_arg_or_empty(
                    values.get(5).cloned(),
                    "test.run_xsht_trace",
                    span,
                )?;
                let name = lowered_str_arg_owned(
                    values.get(6).cloned(),
                    "script.xsh",
                    "test.run_xsht_trace",
                    span,
                )?;
                match self.lowered_test_run_xsht_trace(
                    &ctx,
                    &source,
                    &trace_args,
                    &script_args,
                    &env,
                    &stdin,
                    &name,
                    span,
                ) {
                    Ok(record) => lowered_result_ok(LoweredValue::Record(record)),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            RuntimeOp::TuiReset if values.is_empty() => {
                LoweredValue::Str(tui::sequence(Sequence::Reset).into())
            }
            RuntimeOp::TuiBold if values.is_empty() => {
                LoweredValue::Str(tui::sequence(Sequence::Bold).into())
            }
            RuntimeOp::TuiDim if values.is_empty() => {
                LoweredValue::Str(tui::sequence(Sequence::Dim).into())
            }
            RuntimeOp::TuiRed if values.is_empty() => {
                LoweredValue::Str(tui::sequence(Sequence::Red).into())
            }
            RuntimeOp::TuiGreen if values.is_empty() => {
                LoweredValue::Str(tui::sequence(Sequence::Green).into())
            }
            RuntimeOp::TuiYellow if values.is_empty() => {
                LoweredValue::Str(tui::sequence(Sequence::Yellow).into())
            }
            RuntimeOp::TuiBlue if values.is_empty() => {
                LoweredValue::Str(tui::sequence(Sequence::Blue).into())
            }
            RuntimeOp::TuiMagenta if values.is_empty() => {
                LoweredValue::Str(tui::sequence(Sequence::Magenta).into())
            }
            RuntimeOp::TuiCyan if values.is_empty() => {
                LoweredValue::Str(tui::sequence(Sequence::Cyan).into())
            }
            RuntimeOp::TuiWhite if values.is_empty() => {
                LoweredValue::Str(tui::sequence(Sequence::White).into())
            }
            RuntimeOp::TuiGray if values.is_empty() => {
                LoweredValue::Str(tui::sequence(Sequence::Gray).into())
            }
            RuntimeOp::TuiClear if values.is_empty() => {
                LoweredValue::Str(tui::sequence(Sequence::Clear).into())
            }
            RuntimeOp::TuiHome if values.is_empty() => {
                LoweredValue::Str(tui::sequence(Sequence::Home).into())
            }
            RuntimeOp::TuiEraseLine if values.is_empty() => {
                LoweredValue::Str(tui::sequence(Sequence::EraseLine).into())
            }
            RuntimeOp::TuiHideCursor if values.is_empty() => {
                LoweredValue::Str(tui::sequence(Sequence::HideCursor).into())
            }
            RuntimeOp::TuiShowCursor if values.is_empty() => {
                LoweredValue::Str(tui::sequence(Sequence::ShowCursor).into())
            }
            RuntimeOp::TuiLeftPad if values.len() == 2 => {
                let width = lowered_int_arg(values.pop(), "tui.left_pad", span)?;
                let text = lowered_str_arg_owned(values.pop(), "", "tui.left_pad", span)?;
                LoweredValue::Str(tui::left_pad(&text, width).into())
            }
            RuntimeOp::TuiRightPad if values.len() == 2 => {
                let width = lowered_int_arg(values.pop(), "tui.right_pad", span)?;
                let text = lowered_str_arg_owned(values.pop(), "", "tui.right_pad", span)?;
                LoweredValue::Str(tui::right_pad(&text, width).into())
            }
            RuntimeOp::TuiReadSecret if values.len() == 1 => {
                let prompt = lowered_str_arg_owned(values.pop(), "", "tui.read_secret", span)?;
                match tui::read_secret(&prompt) {
                    Ok(secret) => lowered_result_ok(LoweredValue::Str(secret.into())),
                    Err(error) => lowered_result_err_value(
                        RuntimeError::new("tui.read_secret", error.to_string()).with_span(span),
                    ),
                }
            }
            RuntimeOp::UnixKillAll
            | RuntimeOp::UnixUptimeSeconds
            | RuntimeOp::UnixTty
            | RuntimeOp::UnixId
            | RuntimeOp::UnixTtyAttrs
            | RuntimeOp::UnixSetTtyAttrs
            | RuntimeOp::UnixSetHostname
            | RuntimeOp::UnixReapChildEvents
            | RuntimeOp::UnixPid1Setup
            | RuntimeOp::UnixWaitPid1Event
            | RuntimeOp::UnixShutdownProcessGroups
            | RuntimeOp::UnixSpawnProcessGroup
            | RuntimeOp::UnixSpawnProcessGroupLog
            | RuntimeOp::UnixSpawnLoggedProcessGroup
            | RuntimeOp::UnixSpawnWithTty
            | RuntimeOp::UnixNotifyReady
            | RuntimeOp::UnixNotifyClose
            | RuntimeOp::UnixKillProcessGroup
            | RuntimeOp::UnixExec => self.eval_lowered_unix_call(op, values, span)?,
            RuntimeOp::UserCurrent if values.is_empty() => {
                lowered_runtime_result(user_module::current(span), span)?
            }
            RuntimeOp::UserLookup if values.len() == 1 => {
                let name = lowered_str_arg_owned(values.pop(), "", "user.lookup", span)?;
                lowered_runtime_result(user_module::lookup(&name, span), span)?
            }
            RuntimeOp::UserByUid if values.len() == 1 => {
                let uid = lowered_int_arg(values.pop(), "user.by_uid", span)?;
                lowered_runtime_result(
                    user_module::uid_from_i64(uid, span)
                        .and_then(|uid| user_module::by_uid(uid, span)),
                    span,
                )?
            }
            RuntimeOp::UserAdd if (1..=6).contains(&values.len()) => {
                let gecos = lowered_str_arg_owned(values.get(5).cloned(), "", "user.add", span)?;
                let shell = match values.get(4).cloned() {
                    Some(value) => Some(lowered_path_arg(value, "user.add", span)?),
                    None => None,
                };
                let home = match values.get(3).cloned() {
                    Some(value) => Some(lowered_path_arg(value, "user.add", span)?),
                    None => None,
                };
                let gid = match values.get(2).cloned() {
                    Some(value) => Some(lowered_int_arg(Some(value), "user.add", span)?),
                    None => None,
                };
                let uid = match values.get(1).cloned() {
                    Some(value) => Some(lowered_int_arg(Some(value), "user.add", span)?),
                    None => None,
                };
                let name = lowered_str_arg_owned(values.first().cloned(), "", "user.add", span)?;
                lowered_runtime_result(
                    user_module::add(&name, uid, gid, home, shell, &gecos, span),
                    span,
                )?
            }
            RuntimeOp::UserRemove if values.len() == 1 || values.len() == 2 => {
                let remove_home =
                    lowered_bool_arg_or(values.get(1).cloned(), false, "user.remove", span)?;
                let name = lowered_str_arg_owned(values.first().cloned(), "", "user.remove", span)?;
                lowered_runtime_result(user_module::remove(&name, remove_home, span), span)?
            }
            RuntimeOp::UtilsCache if values.len() == 1 || values.len() == 2 => {
                let callee = values.remove(0);
                let call_args = if values.is_empty() {
                    Vec::new()
                } else {
                    let args = values.remove(0);
                    let LoweredValue::List(args) = args else {
                        return Err(RuntimeError::new(
                            "type-error",
                            "utils.cache expected List args",
                        )
                        .with_span(span));
                    };
                    args.into_iter()
                        .map(LoweredValue::into_value)
                        .collect::<Vec<_>>()
                };
                let (function, pure) = match callee {
                    LoweredValue::Pure(function) => (function, true),
                    LoweredValue::Proc(function) => (function, false),
                    other => {
                        return Err(RuntimeError::new(
                            "type-error",
                            format!(
                                "utils.cache expected Pure or Proc, found {}",
                                other.type_name()
                            ),
                        )
                        .with_span(span));
                    }
                };
                let display_name = function.display_name();
                let key = utils_cache_key(&display_name, &call_args).map_err(|bad_type| {
                    RuntimeError::new(
                        "cache-key-error",
                        format!("args contains a {bad_type}, which cannot be used as a cache key"),
                    )
                    .with_span(span)
                })?;
                let result = if let Some(cached) = self.utils_cache.get(&key).cloned() {
                    cached
                } else {
                    let function_key = function
                        .as_name()
                        .map(LoweredFunctionKey::Name)
                        .or_else(|| function.as_qualified().map(LoweredFunctionKey::Qualified))
                        .expect("function identity is interned");
                    let result = self
                        .call_indexed_direct(
                            function_key,
                            if pure {
                                LoweredFunctionKind::Pure
                            } else {
                                LoweredFunctionKind::Proc
                            },
                            &call_args,
                            span,
                        )
                        .ok_or_else(|| {
                            RuntimeError::new(
                                "unresolved-call",
                                format!(
                                    "utils.cache target {} could not be lowered",
                                    function.display_name()
                                ),
                            )
                            .with_span(span)
                        })??;
                    self.utils_cache.insert(key, result.clone());
                    result
                };
                lowered_value_from_runtime_any(&result).ok_or_else(|| {
                    RuntimeError::new(
                        "type-error",
                        format!("utils.cache returned unsupported {}", result.type_name()),
                    )
                    .with_span(span)
                })?
            }
            RuntimeOp::LinuxWriteDevice if values.len() == 2 => {
                if !self.linux_dry_run() && !self.linux_real() {
                    lowered_result_err_value(RuntimeError::new(
                        "linux-unimplemented",
                        "linux.* boot primitives require XSH_LINUX_DRY_RUN=1 or XSH_LINUX_REAL=1",
                    ).with_span(span))
                } else if self.linux_real() && !self.linux_dry_run() {
                    let device = lowered_path_arg(values.remove(0), "linux.write_device", span)?;
                    let source = lowered_path_arg(values.remove(0), "linux.write_device", span)?;
                    let host_device = self.host_path(&device);
                    let host_source = self.host_path(&source);
                    lowered_runtime_result(
                        linux_module::write_device(&host_device, &host_source, span),
                        span,
                    )?
                } else {
                    let device = lowered_path_arg(values.remove(0), "linux.write_device", span)?;
                    let source = lowered_path_arg(values.remove(0), "linux.write_device", span)?;
                    self.linux_dry_run_log(
                        "write_device",
                        &[("device", device.display()), ("source", source.display())],
                        span,
                    )?;
                    lowered_result_ok(LoweredValue::Unit)
                }
            }
            RuntimeOp::LinuxReadDevice if values.len() == 3 => {
                if !self.linux_dry_run() && !self.linux_real() {
                    lowered_result_err_value(RuntimeError::new(
                        "linux-unimplemented",
                        "linux.* boot primitives require XSH_LINUX_DRY_RUN=1 or XSH_LINUX_REAL=1",
                    ).with_span(span))
                } else if self.linux_real() && !self.linux_dry_run() {
                    let device = lowered_path_arg(values.remove(0), "linux.read_device", span)?;
                    let dest = lowered_path_arg(values.remove(0), "linux.read_device", span)?;
                    let host_device = self.host_path(&device);
                    let host_dest = self.host_path(&dest);
                    let bytes = lowered_int_arg(Some(values.remove(0)), "linux.read_device", span)?;
                    lowered_runtime_result(
                        linux_module::read_device(&host_device, &host_dest, bytes, span),
                        span,
                    )?
                } else {
                    let device = lowered_path_arg(values.remove(0), "linux.read_device", span)?;
                    let dest = lowered_path_arg(values.remove(0), "linux.read_device", span)?;
                    let bytes = lowered_int_arg(Some(values.remove(0)), "linux.read_device", span)?;
                    if !(0..=1024 * 1024).contains(&bytes) {
                        return Ok(ControlFlow::Continue(lowered_runtime_value(
                            module_error(
                                "linux-read-device",
                                "bytes must be between 0 and 1048576 in dry-run mode",
                                span,
                            ),
                            span,
                        )?));
                    }
                    let host_dest = self.host_path(&dest);
                    match std::fs::write(host_dest, vec![0_u8; bytes as usize]) {
                        Ok(()) => {
                            self.linux_dry_run_log(
                                "read_device",
                                &[
                                    ("device", device.display()),
                                    ("dest", dest.display()),
                                    ("bytes", bytes.to_string()),
                                ],
                                span,
                            )?;
                            lowered_result_ok(LoweredValue::Unit)
                        }
                        Err(error) => lowered_runtime_value(
                            module_io_error("linux-read-device", error, span),
                            span,
                        )?,
                    }
                }
            }
            RuntimeOp::LinuxBlkid
            | RuntimeOp::LinuxBlockDevices
            | RuntimeOp::LinuxChroot
            | RuntimeOp::LinuxDepmod
            | RuntimeOp::LinuxDmesg
            | RuntimeOp::LinuxDiskUsage
            | RuntimeOp::LinuxFileAttrs
            | RuntimeOp::LinuxFileVersion
            | RuntimeOp::LinuxFsck
            | RuntimeOp::LinuxHalt
            | RuntimeOp::LinuxHwclock
            | RuntimeOp::LinuxInsmod
            | RuntimeOp::LinuxIsMountpoint
            | RuntimeOp::LinuxKillAll
            | RuntimeOp::LinuxLoopAttach
            | RuntimeOp::LinuxLoopDetach
            | RuntimeOp::LinuxLoopList
            | RuntimeOp::LinuxMemInfo
            | RuntimeOp::LinuxMknod
            | RuntimeOp::LinuxMkswap
            | RuntimeOp::LinuxModinfo
            | RuntimeOp::LinuxModprobe
            | RuntimeOp::LinuxModules
            | RuntimeOp::LinuxMount
            | RuntimeOp::LinuxMountAll
            | RuntimeOp::LinuxOpenFiles
            | RuntimeOp::LinuxPartitionTable
            | RuntimeOp::LinuxPivotRoot
            | RuntimeOp::LinuxPoweroff
            | RuntimeOp::LinuxReboot
            | RuntimeOp::LinuxRfkillBlock
            | RuntimeOp::LinuxRfkillList
            | RuntimeOp::LinuxRfkillUnblock
            | RuntimeOp::LinuxRmmod
            | RuntimeOp::LinuxRootDevice
            | RuntimeOp::LinuxSetFileAttrs
            | RuntimeOp::LinuxSetFileVersion
            | RuntimeOp::LinuxSetHwclock
            | RuntimeOp::LinuxSetSystemClock
            | RuntimeOp::LinuxSwapon
            | RuntimeOp::LinuxSwaponAll
            | RuntimeOp::LinuxSwapoff
            | RuntimeOp::LinuxSwapoffAll
            | RuntimeOp::LinuxSwitchRoot
            | RuntimeOp::LinuxSysctlGet
            | RuntimeOp::LinuxSysctlLoadDirs
            | RuntimeOp::LinuxSysctlSet
            | RuntimeOp::LinuxUeventStream
            | RuntimeOp::LinuxUmountAll
            | RuntimeOp::LinuxWritePartitionTable => {
                self.eval_lowered_linux_call(op, values, span)?
            }
            _ => {
                return Err(RuntimeError::new(
                    "unsupported-call",
                    "unsupported lowered module call",
                )
                .with_span(span));
            }
        };
        Ok(ControlFlow::Continue(value))
    }

    fn eval_lowered_unix_call(
        &mut self,
        op: RuntimeOp,
        values: Vec<LoweredValue>,
        span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        let out = self.eval_unix_call_value(op, values, span)?;
        lowered_value_from_runtime_any(&out).ok_or_else(|| {
            RuntimeError::new(
                "type-error",
                format!("cannot lower unix result {}", out.type_name()),
            )
            .with_span(span)
        })
    }

    fn eval_unix_call_value(
        &mut self,
        op: RuntimeOp,
        values: Vec<LoweredValue>,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        match op {
            RuntimeOp::UnixKillAll => {
                let name =
                    lowered_str_arg_owned(values.first().cloned(), "", "unix.kill_all", span)?;
                let signal =
                    lowered_str_arg_owned(values.get(1).cloned(), "TERM", "unix.kill_all", span)?;
                unix_module::kill_all(&name, &signal, span)
            }
            RuntimeOp::UnixUptimeSeconds if self.unix_dry_run() => {
                let uptime = self
                    .unix_dry_run_env("XSH_UNIX_UPTIME_SECONDS", "0")
                    .parse::<i64>()
                    .unwrap_or(0);
                self.unix_dry_run_log("uptime_seconds", &[("seconds", uptime.to_string())], span)?;
                Ok(Value::ok(Value::Int(uptime)))
            }
            RuntimeOp::UnixTty if self.unix_dry_run() => {
                let tty = self.unix_dry_run_env("XSH_UNIX_TTY", "/dev/tty");
                self.unix_dry_run_log("tty", &[("tty", tty.clone())], span)?;
                Ok(Value::ok(Value::Str(tty.into())))
            }
            RuntimeOp::UnixId if self.unix_dry_run() => {
                self.unix_dry_run_log("id", &[], span)?;
                Ok(Value::ok(Value::Record(RecordMap::from([
                    (Arc::from("uid"), Value::Int(0)),
                    (Arc::from("euid"), Value::Int(0)),
                    (Arc::from("gid"), Value::Int(0)),
                    (Arc::from("egid"), Value::Int(0)),
                    (
                        Arc::from("groups"),
                        Value::List(vec![Value::Record(RecordMap::from([
                            (Arc::from("gid"), Value::Int(0)),
                            (Arc::from("name"), Value::Str("root".into())),
                        ]))]),
                    ),
                ]))))
            }
            RuntimeOp::UnixTtyAttrs if self.unix_dry_run() => {
                let fd = lowered_int_arg_or(values.first().cloned(), 0, "unix.tty_attrs", span)?;
                self.unix_dry_run_log("tty_attrs", &[("fd", fd.to_string())], span)?;
                Ok(Value::ok(unix_dry_run_tty_attrs()))
            }
            RuntimeOp::UnixSetTtyAttrs if self.unix_dry_run() => {
                let _attrs =
                    lowered_record_arg(values.first().cloned(), "unix.set_tty_attrs", span)?;
                let fd = lowered_int_arg_or(values.get(1).cloned(), 0, "unix.set_tty_attrs", span)?;
                self.unix_dry_run_log("set_tty_attrs", &[("fd", fd.to_string())], span)?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::UnixSetHostname if self.unix_dry_run() => {
                let hostname =
                    lowered_str_arg_owned(values.first().cloned(), "", "unix.set_hostname", span)?;
                self.unix_dry_run_log("set_hostname", &[("hostname", hostname.clone())], span)?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::UnixReapChildEvents
            | RuntimeOp::UnixPid1Setup
            | RuntimeOp::UnixWaitPid1Event
            | RuntimeOp::UnixShutdownProcessGroups
            | RuntimeOp::UnixSpawnProcessGroup
            | RuntimeOp::UnixSpawnProcessGroupLog
            | RuntimeOp::UnixSpawnLoggedProcessGroup
            | RuntimeOp::UnixSpawnWithTty
            | RuntimeOp::UnixNotifyReady
            | RuntimeOp::UnixNotifyClose
            | RuntimeOp::UnixKillProcessGroup
            | RuntimeOp::UnixExec
                if self.unix_dry_run() =>
            {
                self.eval_unix_dry_run_call(op, values, span)
            }
            RuntimeOp::UnixReapChildEvents => unix_module::reap_child_events(span),
            RuntimeOp::UnixPid1Setup => {
                let signals =
                    lowered_str_list_arg(values.first().cloned(), "unix.pid1_setup", span)?;
                let subreaper =
                    lowered_bool_arg_or(values.get(1).cloned(), true, "unix.pid1_setup", span)?;
                let allow_non_pid1 =
                    lowered_bool_arg_or(values.get(2).cloned(), false, "unix.pid1_setup", span)?;
                unix_module::pid1_setup_native(&signals, subreaper, allow_non_pid1, span)?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::UnixWaitPid1Event => {
                let deadline = match values.first().cloned() {
                    Some(value) => {
                        let timeout =
                            lowered_duration_arg(Some(value), "unix.wait_pid1_event", span)?;
                        Some(Instant::now() + Duration::from_millis(timeout.millis))
                    }
                    None => None,
                };
                let event = unix_module::wait_pid1_event_native(deadline, span)?;
                Ok(Value::ok(pid1_event_record(event)))
            }
            RuntimeOp::UnixShutdownProcessGroups => {
                let groups = lowered_int_list_arg(
                    values.first().cloned(),
                    "unix.shutdown_process_groups",
                    span,
                )?;
                let term_timeout = lowered_duration_arg(
                    values.get(1).cloned(),
                    "unix.shutdown_process_groups",
                    span,
                )?;
                let kill_timeout = match values.get(2).cloned() {
                    Some(value) => {
                        lowered_duration_arg(Some(value), "unix.shutdown_process_groups", span)?
                            .millis
                    }
                    None => 0,
                };
                let shutdown = unix_module::shutdown_process_groups_native(
                    &groups,
                    Duration::from_millis(term_timeout.millis),
                    Duration::from_millis(kill_timeout),
                    span,
                )?;
                Ok(Value::ok(pid1_shutdown_record(shutdown)))
            }
            RuntimeOp::UnixSpawnProcessGroup => {
                let plan = lowered_command_arg(
                    unix_require_arg(values.first().cloned(), "unix.spawn_process_group", span)?,
                    "unix.spawn_process_group",
                    span,
                )?;
                let notify = lowered_bool_arg_or(
                    values.get(1).cloned(),
                    false,
                    "unix.spawn_process_group",
                    span,
                )?;
                let invocation = self.invocation_from_command_plan(&plan, span)?;
                unix_module::spawn_process_group(&invocation, notify, span)
            }
            RuntimeOp::UnixNotifyReady => {
                let fd = lowered_int_arg(values.first().cloned(), "unix.notify_ready", span)?;
                match unix_module::notify_ready_native(fd, span) {
                    Ok(ready) => Ok(Value::ok(Value::Bool(ready))),
                    Err(error) => Ok(Value::err(Value::Error(Box::new(error)))),
                }
            }
            RuntimeOp::UnixNotifyClose => {
                let fd = lowered_int_arg(values.first().cloned(), "unix.notify_close", span)?;
                match unix_module::notify_close_native(fd, span) {
                    Ok(()) => Ok(Value::ok(Value::Unit)),
                    Err(error) => Ok(Value::err(Value::Error(Box::new(error)))),
                }
            }
            RuntimeOp::UnixSpawnProcessGroupLog => {
                let plan = lowered_command_arg(
                    unix_require_arg(
                        values.first().cloned(),
                        "unix.spawn_process_group_log",
                        span,
                    )?,
                    "unix.spawn_process_group_log",
                    span,
                )?;
                let log = lowered_path_arg(
                    unix_require_arg(values.get(1).cloned(), "unix.spawn_process_group_log", span)?,
                    "unix.spawn_process_group_log",
                    span,
                )?;
                let host_log = self.host_path(&log);
                if let Some(parent) = host_log.parent()
                    && !parent.as_os_str().is_empty()
                {
                    std::fs::create_dir_all(parent).map_err(|error| {
                        RuntimeError::new("unix-spawn-log", error.to_string()).with_span(span)
                    })?;
                }
                let stdout = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .mode(0o600)
                    .open(&host_log)
                    .map_err(|error| {
                        RuntimeError::new("unix-spawn-log", error.to_string()).with_span(span)
                    })?;
                let stderr = stdout.try_clone().map_err(|error| {
                    RuntimeError::new("unix-spawn-log", error.to_string()).with_span(span)
                })?;
                let notify = lowered_bool_arg_or(
                    values.get(2).cloned(),
                    false,
                    "unix.spawn_process_group_log",
                    span,
                )?;
                let invocation = self.invocation_from_command_plan(&plan, span)?;
                match unix_module::spawn_process_group_with_stdio_native(
                    &invocation,
                    stdout,
                    stderr,
                    notify,
                    span,
                ) {
                    Ok(child) => Ok(spawned_child_record(child)),
                    Err(error) => Ok(Value::err(Value::Error(Box::new(error)))),
                }
            }
            RuntimeOp::UnixSpawnLoggedProcessGroup => {
                let plan = lowered_command_arg(
                    unix_require_arg(
                        values.first().cloned(),
                        "unix.spawn_logged_process_group",
                        span,
                    )?,
                    "unix.spawn_logged_process_group",
                    span,
                )?;
                let logger_plan = lowered_command_arg(
                    unix_require_arg(
                        values.get(1).cloned(),
                        "unix.spawn_logged_process_group",
                        span,
                    )?,
                    "unix.spawn_logged_process_group",
                    span,
                )?;
                let invocation = self.invocation_from_command_plan(&plan, span)?;
                let logger_invocation = self.invocation_from_command_plan(&logger_plan, span)?;
                unix_module::spawn_logged_process_group(&invocation, &logger_invocation, span)
            }
            RuntimeOp::UnixSpawnWithTty => {
                let plan = lowered_command_arg(
                    unix_require_arg(values.first().cloned(), "unix.spawn_with_tty", span)?,
                    "unix.spawn_with_tty",
                    span,
                )?;
                let tty =
                    lowered_str_arg_owned(values.get(1).cloned(), "", "unix.spawn_with_tty", span)?;
                let invocation = self.invocation_from_command_plan(&plan, span)?;
                unix_module::spawn_with_tty(&invocation, &tty, span)
            }
            RuntimeOp::UnixKillProcessGroup => {
                let pid =
                    lowered_int_arg(values.first().cloned(), "unix.kill_process_group", span)?;
                let signal = lowered_str_arg_owned(
                    values.get(1).cloned(),
                    "",
                    "unix.kill_process_group",
                    span,
                )?;
                let signal = match process_module::signal_info(&signal, span) {
                    Ok(signal) => signal,
                    Err(error) => return Ok(Value::err(Value::Error(Box::new(error)))),
                };
                unix_module::kill_process_group(pid, signal.number, span)
            }
            RuntimeOp::UnixExec => {
                let plan = lowered_command_arg(
                    unix_require_arg(values.first().cloned(), "unix.exec", span)?,
                    "unix.exec",
                    span,
                )?;
                let invocation = self.invocation_from_command_plan(&plan, span)?;
                unix_module::exec(&invocation, span)
            }
            RuntimeOp::UnixSetHostname => {
                if !self.unix_real() {
                    return Ok(module_error(
                        "unix-real-required",
                        "unix.set_hostname requires XSH_UNIX_DRY_RUN=1 or XSH_UNIX_REAL=1",
                        span,
                    ));
                }
                let hostname =
                    lowered_str_arg_owned(values.first().cloned(), "", "unix.set_hostname", span)?;
                unix_module::set_hostname(&hostname, span)
            }
            RuntimeOp::UnixUptimeSeconds => unix_module::uptime_seconds(span),
            RuntimeOp::UnixTty => unix_module::tty(span),
            RuntimeOp::UnixId => unix_module::id(span),
            RuntimeOp::UnixTtyAttrs => {
                let fd = lowered_int_arg_or(values.first().cloned(), 0, "unix.tty_attrs", span)?;
                unix_module::tty_attrs(fd, span)
            }
            RuntimeOp::UnixSetTtyAttrs => {
                if !self.unix_real() {
                    return Ok(module_error(
                        "unix-real-required",
                        "unix.set_tty_attrs requires XSH_UNIX_DRY_RUN=1 or XSH_UNIX_REAL=1",
                        span,
                    ));
                }
                let attrs =
                    lowered_record_arg(values.first().cloned(), "unix.set_tty_attrs", span)?;
                let fd = lowered_int_arg_or(values.get(1).cloned(), 0, "unix.set_tty_attrs", span)?;
                unix_module::set_tty_attrs(&attrs, fd, span)
            }
            _ => unreachable!("unix operation expected"),
        }
    }

    fn eval_unix_dry_run_call(
        &mut self,
        op: RuntimeOp,
        values: Vec<LoweredValue>,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        match op {
            RuntimeOp::UnixReapChildEvents => {
                self.unix_dry_run_log("reap_child_events", &[], span)?;
                Ok(Value::ok(Value::List(self.unix_dry_run_child_events())))
            }
            RuntimeOp::UnixPid1Setup => {
                let signals =
                    lowered_str_list_arg(values.first().cloned(), "unix.pid1_setup", span)?;
                let subreaper =
                    lowered_bool_arg_or(values.get(1).cloned(), true, "unix.pid1_setup", span)?;
                let allow_non_pid1 =
                    lowered_bool_arg_or(values.get(2).cloned(), false, "unix.pid1_setup", span)?;
                self.unix_dry_run_log(
                    "pid1_setup",
                    &[
                        ("signals", signals.join(",")),
                        ("subreaper", subreaper.to_string()),
                        ("allow_non_pid1", allow_non_pid1.to_string()),
                    ],
                    span,
                )?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::UnixWaitPid1Event => {
                let kind = self.unix_dry_run_env("XSH_UNIX_DRY_RUN_EVENT_KIND", "signal");
                let signal = self.unix_dry_run_env("XSH_UNIX_DRY_RUN_SIGNAL", "TERM");
                let pid = self.unix_dry_run_wait_pid();
                self.unix_dry_run_log(
                    "wait_pid1_event",
                    &[
                        ("kind", kind.clone()),
                        ("signal", signal.clone()),
                        ("pid", pid.to_string()),
                    ],
                    span,
                )?;
                let children = self.unix_dry_run_wait_children(pid);
                let kind = if kind == "children" || kind == "child" {
                    "children"
                } else if kind == "poll" {
                    "poll"
                } else if kind == "timeout" {
                    "timeout"
                } else {
                    "signal"
                };
                Ok(Value::ok(Value::Record(RecordMap::from([
                    (Arc::from("kind"), Value::Str(kind.into())),
                    (Arc::from("signal"), Value::Str(signal.into())),
                    (Arc::from("children"), Value::List(children)),
                ]))))
            }
            RuntimeOp::UnixShutdownProcessGroups => {
                let groups = lowered_int_list_arg(
                    values.first().cloned(),
                    "unix.shutdown_process_groups",
                    span,
                )?;
                let term_timeout = lowered_duration_arg(
                    values.get(1).cloned(),
                    "unix.shutdown_process_groups",
                    span,
                )?;
                let kill_timeout = match values.get(2).cloned() {
                    Some(value) => {
                        lowered_duration_arg(Some(value), "unix.shutdown_process_groups", span)?
                            .millis
                    }
                    None => 0,
                };
                self.unix_dry_run_log(
                    "pid1_shutdown",
                    &[
                        (
                            "groups",
                            groups
                                .iter()
                                .map(i64::to_string)
                                .collect::<Vec<_>>()
                                .join(","),
                        ),
                        ("term_timeout_ms", term_timeout.millis.to_string()),
                        ("kill_timeout_ms", kill_timeout.to_string()),
                    ],
                    span,
                )?;
                Ok(Value::ok(Value::Record(RecordMap::from([
                    (Arc::from("term_sent"), Value::Int(groups.len() as i64)),
                    (Arc::from("kill_sent"), Value::Int(0)),
                    (Arc::from("reaped"), Value::List(Vec::new())),
                    (Arc::from("remaining"), Value::List(Vec::new())),
                ]))))
            }
            RuntimeOp::UnixSpawnProcessGroup => {
                let plan = lowered_command_arg(
                    unix_require_arg(values.first().cloned(), "unix.spawn_process_group", span)?,
                    "unix.spawn_process_group",
                    span,
                )?;
                let notify = lowered_bool_arg_or(
                    values.get(1).cloned(),
                    false,
                    "unix.spawn_process_group",
                    span,
                )?;
                self.unix_dry_run_spawn(plan, None, notify, span)
            }
            RuntimeOp::UnixSpawnProcessGroupLog => {
                let plan = lowered_command_arg(
                    unix_require_arg(
                        values.first().cloned(),
                        "unix.spawn_process_group_log",
                        span,
                    )?,
                    "unix.spawn_process_group_log",
                    span,
                )?;
                let log = lowered_path_arg(
                    unix_require_arg(values.get(1).cloned(), "unix.spawn_process_group_log", span)?,
                    "unix.spawn_process_group_log",
                    span,
                )?;
                let notify = lowered_bool_arg_or(
                    values.get(2).cloned(),
                    false,
                    "unix.spawn_process_group_log",
                    span,
                )?;
                self.unix_dry_run_spawn_log(plan, log.display(), notify, span)
            }
            RuntimeOp::UnixSpawnLoggedProcessGroup => {
                let plan = lowered_command_arg(
                    unix_require_arg(
                        values.first().cloned(),
                        "unix.spawn_logged_process_group",
                        span,
                    )?,
                    "unix.spawn_logged_process_group",
                    span,
                )?;
                let logger_plan = lowered_command_arg(
                    unix_require_arg(
                        values.get(1).cloned(),
                        "unix.spawn_logged_process_group",
                        span,
                    )?,
                    "unix.spawn_logged_process_group",
                    span,
                )?;
                self.unix_dry_run_logged_spawn(plan, logger_plan, span)
            }
            RuntimeOp::UnixSpawnWithTty => {
                let plan = lowered_command_arg(
                    unix_require_arg(values.first().cloned(), "unix.spawn_with_tty", span)?,
                    "unix.spawn_with_tty",
                    span,
                )?;
                let tty =
                    lowered_str_arg_owned(values.get(1).cloned(), "", "unix.spawn_with_tty", span)?;
                self.unix_dry_run_spawn(plan, Some(tty), false, span)
            }
            RuntimeOp::UnixNotifyReady => {
                let fd = lowered_int_arg(values.first().cloned(), "unix.notify_ready", span)?;
                let ready = if fd < 0 {
                    false
                } else {
                    let value = self.unix_dry_run_env("XSH_UNIX_DRY_RUN_READY", "1");
                    value == "1" || value == "true" || value == "yes"
                };
                self.unix_dry_run_log("notify_ready", &[("ready", ready.to_string())], span)?;
                Ok(Value::ok(Value::Bool(ready)))
            }
            RuntimeOp::UnixNotifyClose => {
                let fd = lowered_int_arg(values.first().cloned(), "unix.notify_close", span)?;
                self.unix_dry_run_log("notify_close", &[("fd", fd.to_string())], span)?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::UnixKillProcessGroup => {
                let pid =
                    lowered_int_arg(values.first().cloned(), "unix.kill_process_group", span)?;
                if pid <= 0 {
                    return Ok(module_error(
                        "pid-range",
                        "pid must be a positive process id",
                        span,
                    ));
                }
                let signal = lowered_str_arg_owned(
                    values.get(1).cloned(),
                    "",
                    "unix.kill_process_group",
                    span,
                )?;
                if let Err(error) = process_module::signal_info(&signal, span) {
                    return Ok(Value::err(Value::Error(Box::new(error))));
                }
                self.unix_dry_run_log(
                    "kill_process_group",
                    &[("pid", pid.to_string()), ("signal", signal)],
                    span,
                )?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::UnixExec => {
                let plan = lowered_command_arg(
                    unix_require_arg(values.first().cloned(), "unix.exec", span)?,
                    "unix.exec",
                    span,
                )?;
                self.unix_dry_run_log(
                    "exec",
                    &[
                        (
                            "command",
                            String::from_utf8_lossy(&plan.target).into_owned(),
                        ),
                        ("argv", display_spawn_argv(&plan.target, &plan.argv)),
                    ],
                    span,
                )?;
                Ok(Value::ok(Value::Unit))
            }
            _ => unreachable!("unix dry-run operation expected"),
        }
    }

    fn unix_real(&self) -> bool {
        self.env
            .get_owned(b"XSH_UNIX_REAL".as_slice())
            .and_then(|value| String::from_utf8(value).ok())
            .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
    }

    fn unix_dry_run_env(&self, name: &str, default: &str) -> String {
        self.env
            .get_owned(name.as_bytes())
            .and_then(|value| String::from_utf8(value).ok())
            .unwrap_or_else(|| default.to_string())
    }

    fn unix_dry_run_status_kind(&self) -> String {
        self.unix_dry_run_env("XSH_UNIX_DRY_RUN_STATUS_KIND", "exit")
    }

    fn unix_dry_run_status_code(&self) -> i32 {
        self.unix_dry_run_env("XSH_UNIX_DRY_RUN_STATUS_CODE", "0")
            .parse::<i32>()
            .unwrap_or(0)
    }

    fn unix_dry_run_status(&self) -> ProcessStatus {
        let kind = self.unix_dry_run_status_kind();
        let code = self.unix_dry_run_status_code();
        if kind == "signal" {
            ProcessStatus::signaled(if code > 0 { code } else { libc::SIGTERM })
        } else {
            ProcessStatus::exited(code)
        }
    }

    fn unix_dry_run_child_events(&self) -> Vec<Value> {
        let pid = self
            .unix_dry_run_env("XSH_UNIX_DRY_RUN_CHILD_PID", "0")
            .parse::<i64>()
            .unwrap_or(0);
        if pid <= 0 {
            return Vec::new();
        }
        self.unix_dry_run_wait_children(pid)
    }

    fn unix_dry_run_wait_pid(&self) -> i64 {
        let child_pid = self
            .unix_dry_run_env("XSH_UNIX_DRY_RUN_CHILD_PID", "0")
            .parse::<i64>()
            .unwrap_or(0);
        if child_pid > 0 {
            return child_pid;
        }
        self.unix_dry_run_env("XSH_UNIX_DRY_RUN_PID", "0")
            .parse::<i64>()
            .unwrap_or(0)
    }

    fn unix_dry_run_wait_children(&self, pid: i64) -> Vec<Value> {
        if pid <= 0 {
            return Vec::new();
        }
        vec![Value::Record(RecordMap::from([
            (Arc::from("pid"), Value::Int(pid)),
            (
                Arc::from("status"),
                Value::Status(self.unix_dry_run_status()),
            ),
        ]))]
    }

    fn unix_dry_run_spawn(
        &mut self,
        plan: CommandPlan,
        tty: Option<String>,
        notify: bool,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let pid = self.unix_next_pid;
        self.unix_next_pid += 1;
        let command = String::from_utf8_lossy(&plan.target).into_owned();
        let argv = std::iter::once(plan.target.as_slice())
            .chain(plan.argv.iter().map(Vec::as_slice))
            .map(|item| Value::Str(String::from_utf8_lossy(item).into_owned().into()))
            .collect::<Vec<_>>();
        let new_session = tty.is_some();
        // No real pipe in dry-run; report the fake pid as the notify fd so a
        // supervisor treats the unit as notify-capable and polls it.
        let notify_fd = if notify { pid } else { -1 };
        let op = if new_session {
            "spawn_with_tty"
        } else {
            "spawn_process_group"
        };
        let mut fields = vec![
            ("pid", pid.to_string()),
            ("command", command.clone()),
            ("argv", display_spawn_argv(&plan.target, &plan.argv)),
            ("detach", "true".to_string()),
            ("new_session", new_session.to_string()),
            ("ignore_hup", "true".to_string()),
            ("notify_fd", notify_fd.to_string()),
        ];
        if let Some(tty) = tty {
            fields.push(("tty", tty));
        }
        self.unix_dry_run_log(op, &fields, span)?;
        Ok(Value::ok(Value::Record(RecordMap::from([
            (Arc::from("pid"), Value::Int(pid)),
            (Arc::from("command"), Value::Str(command.into())),
            (Arc::from("argv"), Value::List(argv)),
            (Arc::from("detach"), Value::Bool(true)),
            (Arc::from("new_session"), Value::Bool(new_session)),
            (Arc::from("ignore_hup"), Value::Bool(true)),
            (Arc::from("notify_fd"), Value::Int(notify_fd)),
        ]))))
    }

    fn unix_dry_run_spawn_log(
        &mut self,
        plan: CommandPlan,
        log_path: String,
        notify: bool,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let pid = self.unix_next_pid;
        self.unix_next_pid += 1;
        let command = String::from_utf8_lossy(&plan.target).into_owned();
        let argv = std::iter::once(plan.target.as_slice())
            .chain(plan.argv.iter().map(Vec::as_slice))
            .map(|item| Value::Str(String::from_utf8_lossy(item).into_owned().into()))
            .collect::<Vec<_>>();
        let notify_fd = if notify { pid } else { -1 };
        self.unix_dry_run_log(
            "spawn_process_group",
            &[
                ("pid", pid.to_string()),
                ("command", command.clone()),
                ("argv", display_spawn_argv(&plan.target, &plan.argv)),
                ("log", "append".to_string()),
                ("log_path", log_path),
                ("detach", "true".to_string()),
                ("new_session", "false".to_string()),
                ("ignore_hup", "true".to_string()),
                ("notify_fd", notify_fd.to_string()),
            ],
            span,
        )?;
        Ok(Value::ok(Value::Record(RecordMap::from([
            (Arc::from("pid"), Value::Int(pid)),
            (Arc::from("command"), Value::Str(command.into())),
            (Arc::from("argv"), Value::List(argv)),
            (Arc::from("detach"), Value::Bool(true)),
            (Arc::from("new_session"), Value::Bool(false)),
            (Arc::from("ignore_hup"), Value::Bool(true)),
            (Arc::from("notify_fd"), Value::Int(notify_fd)),
        ]))))
    }

    fn unix_dry_run_logged_spawn(
        &mut self,
        plan: CommandPlan,
        logger_plan: CommandPlan,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let pid = self.unix_next_pid;
        self.unix_next_pid += 1;
        let log_pid = self.unix_next_pid;
        self.unix_next_pid += 1;
        let command = String::from_utf8_lossy(&plan.target).into_owned();
        let logger = String::from_utf8_lossy(&logger_plan.target).into_owned();
        let argv = std::iter::once(plan.target.as_slice())
            .chain(plan.argv.iter().map(Vec::as_slice))
            .map(|item| Value::Str(String::from_utf8_lossy(item).into_owned().into()))
            .collect::<Vec<_>>();
        self.unix_dry_run_log(
            "spawn_logged_process_group",
            &[
                ("pid", pid.to_string()),
                ("log_pid", log_pid.to_string()),
                ("command", command.clone()),
                ("argv", display_spawn_argv(&plan.target, &plan.argv)),
                ("logger", logger),
                (
                    "logger_argv",
                    display_spawn_argv(&logger_plan.target, &logger_plan.argv),
                ),
                ("detach", "true".to_string()),
                ("new_session", "false".to_string()),
                ("ignore_hup", "true".to_string()),
            ],
            span,
        )?;
        Ok(Value::ok(Value::Record(RecordMap::from([
            (Arc::from("pid"), Value::Int(pid)),
            (Arc::from("log_pid"), Value::Int(log_pid)),
            (Arc::from("command"), Value::Str(command.into())),
            (Arc::from("argv"), Value::List(argv)),
            (Arc::from("detach"), Value::Bool(true)),
            (Arc::from("new_session"), Value::Bool(false)),
            (Arc::from("ignore_hup"), Value::Bool(true)),
        ]))))
    }

    fn unix_dry_run_log(
        &self,
        op: &str,
        fields: &[(&str, String)],
        span: Span,
    ) -> Result<(), RuntimeError> {
        let Some(path) = self.env.get_owned(b"XSH_UNIX_DRY_RUN_LOG".as_slice()) else {
            return Ok(());
        };
        let path = std::path::PathBuf::from(OsString::from_vec(path));
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|error| {
                RuntimeError::new("unix-dry-run-log", error.to_string()).with_span(span)
            })?;
        }
        let mut json_fields = Vec::with_capacity(fields.len() + 1);
        json_fields.push(("op".to_string(), json_module::raw_json_string(op)));
        for (name, value) in fields {
            json_fields.push((
                (*name).to_string(),
                json_module::raw_json_string(value.clone()),
            ));
        }
        let line = json_module::compact_raw_json(&json_module::raw_json_object(json_fields));
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|error| {
                RuntimeError::new("unix-dry-run-log", error.to_string()).with_span(span)
            })?;
        writeln!(file, "{line}").map_err(|error| {
            RuntimeError::new("unix-dry-run-log", error.to_string()).with_span(span)
        })
    }

    fn linux_dry_run_env(&self, name: &str, default: &str) -> String {
        self.env
            .get_owned(name.as_bytes())
            .and_then(|value| String::from_utf8(value).ok())
            .unwrap_or_else(|| default.to_string())
    }

    fn linux_dry_run_file_attrs_flags(&self, span: Span) -> Result<i64, RuntimeError> {
        let value = self.linux_dry_run_env("XSH_LINUX_FILE_ATTRS_FLAGS", "48");
        let flags = value.parse::<i64>().map_err(|_| {
            RuntimeError::new("linux-file-attrs", "invalid XSH_LINUX_FILE_ATTRS_FLAGS")
                .with_span(span)
        })?;
        validate_linux_file_attrs_flags(flags, span)?;
        Ok(flags)
    }

    fn linux_dry_run_file_version(&self, span: Span) -> Result<i64, RuntimeError> {
        let value = self.linux_dry_run_env("XSH_LINUX_FILE_VERSION", "0");
        let version = value.parse::<i64>().map_err(|_| {
            RuntimeError::new("linux-file-version", "invalid XSH_LINUX_FILE_VERSION")
                .with_span(span)
        })?;
        validate_linux_file_version(version, span)?;
        Ok(version)
    }

    fn eval_lowered_linux_call(
        &mut self,
        op: RuntimeOp,
        values: Vec<LoweredValue>,
        span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        let out = self.eval_linux_call_value(op, values, span)?;
        lowered_value_from_runtime_any(&out).ok_or_else(|| {
            RuntimeError::new(
                "type-error",
                format!("cannot lower linux result {}", out.type_name()),
            )
            .with_span(span)
        })
    }

    fn eval_linux_call_value(
        &mut self,
        op: RuntimeOp,
        values: Vec<LoweredValue>,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        if !self.linux_dry_run() && !self.linux_real() {
            return Ok(module_error(
                "linux-unimplemented",
                "linux.* boot primitives require XSH_LINUX_DRY_RUN=1 or XSH_LINUX_REAL=1",
                span,
            ));
        }
        if self.linux_real() && !self.linux_dry_run() {
            return match op {
                RuntimeOp::LinuxRootDevice => linux_module::root_device(span),
                RuntimeOp::LinuxMemInfo => linux_module::meminfo(span),
                RuntimeOp::LinuxModules => linux_module::modules(span),
                RuntimeOp::LinuxDmesg => linux_module::dmesg(span),
                RuntimeOp::LinuxIsMountpoint => {
                    let path = lowered_path_arg(
                        unix_require_arg(values.first().cloned(), "linux.is_mountpoint", span)?,
                        "linux.is_mountpoint",
                        span,
                    )?;
                    let host_path = self.host_path(&path);
                    linux_module::is_mountpoint(&host_path, span)
                }
                RuntimeOp::LinuxDiskUsage => {
                    let path = values
                        .first()
                        .cloned()
                        .map(|value| lowered_path_arg(value, "linux.disk_usage", span))
                        .transpose()?;
                    let host_path = path.as_ref().map(|pv| self.host_path(pv));
                    linux_module::disk_usage(host_path.as_deref(), span)
                }
                RuntimeOp::LinuxSysctlGet => {
                    let key = lowered_str_arg_owned(
                        values.first().cloned(),
                        "",
                        "linux.sysctl_get",
                        span,
                    )?;
                    linux_module::sysctl_get(&key, span)
                }
                RuntimeOp::LinuxFileAttrs => {
                    let path = lowered_path_arg(
                        unix_require_arg(values.first().cloned(), "linux.file_attrs", span)?,
                        "linux.file_attrs",
                        span,
                    )?;
                    let host_path = self.host_path(&path);
                    linux_module::file_attrs(&host_path, span)
                }
                RuntimeOp::LinuxFileVersion => {
                    let path = lowered_path_arg(
                        unix_require_arg(values.first().cloned(), "linux.file_version", span)?,
                        "linux.file_version",
                        span,
                    )?;
                    let host_path = self.host_path(&path);
                    linux_module::file_version(&host_path, span)
                }
                RuntimeOp::LinuxLoopList => linux_module::loop_list(span),
                RuntimeOp::LinuxOpenFiles => {
                    let pid = values
                        .first()
                        .cloned()
                        .map(|value| lowered_int_arg(Some(value), "linux.open_files", span))
                        .transpose()?;
                    linux_module::open_files(pid, span)
                }
                RuntimeOp::LinuxBlockDevices => linux_module::block_devices(span),
                RuntimeOp::LinuxBlkid => {
                    let device = lowered_path_arg(
                        unix_require_arg(values.first().cloned(), "linux.blkid", span)?,
                        "linux.blkid",
                        span,
                    )?;
                    let host_path = self.host_path(&device);
                    linux_module::blkid(&host_path, span)
                }
                RuntimeOp::LinuxModinfo => {
                    let name =
                        lowered_str_arg_owned(values.first().cloned(), "", "linux.modinfo", span)?;
                    linux_module::modinfo(&name, span)
                }
                RuntimeOp::LinuxPartitionTable => {
                    let device = lowered_path_arg(
                        unix_require_arg(values.first().cloned(), "linux.partition_table", span)?,
                        "linux.partition_table",
                        span,
                    )?;
                    let host_path = self.host_path(&device);
                    linux_module::partition_table(&host_path, span)
                }
                RuntimeOp::LinuxUeventStream => linux_module::uevent_stream(span),
                RuntimeOp::LinuxChroot => {
                    let path = lowered_path_arg(
                        unix_require_arg(values.first().cloned(), "linux.chroot", span)?,
                        "linux.chroot",
                        span,
                    )?;
                    let host_path = self.host_path(&path);
                    linux_module::chroot(&host_path, span)
                }
                // Boot / privileged operations are not safe in a non-privileged
                // container; return Ok(()) so scripts can feature-gate on errors.
                RuntimeOp::LinuxDepmod => {
                    let version =
                        lowered_str_arg_owned(values.first().cloned(), "", "linux.depmod", span)?;
                    linux_module::depmod(&version, span)
                }
                RuntimeOp::LinuxFsck => {
                    let device = lowered_path_arg(
                        unix_require_arg(values.first().cloned(), "linux.fsck", span)?,
                        "linux.fsck",
                        span,
                    )?;
                    let fstype =
                        lowered_str_arg_owned(values.get(1).cloned(), "", "linux.fsck", span)?;
                    let repair =
                        lowered_bool_arg_or(values.get(2).cloned(), false, "linux.fsck", span)?;
                    let host_device = self.host_path(&device);
                    linux_module::fsck(&host_device, &fstype, repair, span)
                }
                RuntimeOp::LinuxHalt => linux_module::halt(span),
                RuntimeOp::LinuxHwclock => linux_module::hwclock(span),
                RuntimeOp::LinuxInsmod => {
                    let path = lowered_path_arg(
                        unix_require_arg(values.first().cloned(), "linux.insmod", span)?,
                        "linux.insmod",
                        span,
                    )?;
                    let params =
                        lowered_str_arg_owned(values.get(1).cloned(), "", "linux.insmod", span)?;
                    let host_path = self.host_path(&path);
                    linux_module::insmod(&host_path, &params, span)
                }
                RuntimeOp::LinuxKillAll => {
                    let signal_str = lowered_str_arg_owned(
                        values.first().cloned(),
                        "TERM",
                        "linux.kill_all",
                        span,
                    )?;
                    let signal = process_module::signal_info(&signal_str, span)?.number;
                    let except_pid1 =
                        lowered_bool_arg_or(values.get(1).cloned(), false, "linux.kill_all", span)?;
                    linux_module::kill_all(signal, except_pid1, span)
                }
                RuntimeOp::LinuxLoopAttach => {
                    let file = lowered_path_arg(
                        unix_require_arg(values.first().cloned(), "linux.loop_attach", span)?,
                        "linux.loop_attach",
                        span,
                    )?;
                    let device = values
                        .get(1)
                        .cloned()
                        .map(|value| lowered_path_arg(value, "linux.loop_attach", span))
                        .transpose()?;
                    let host_file = self.host_path(&file);
                    let host_device = device.as_ref().map(|pv| self.host_path(pv));
                    linux_module::loop_attach(&host_file, host_device.as_deref(), span)
                }
                RuntimeOp::LinuxLoopDetach => {
                    let device = lowered_path_arg(
                        unix_require_arg(values.first().cloned(), "linux.loop_detach", span)?,
                        "linux.loop_detach",
                        span,
                    )?;
                    let host_device = self.host_path(&device);
                    linux_module::loop_detach(&host_device, span)
                }
                RuntimeOp::LinuxMknod => {
                    let path = lowered_path_arg(
                        unix_require_arg(values.first().cloned(), "linux.mknod", span)?,
                        "linux.mknod",
                        span,
                    )?;
                    let kind =
                        lowered_str_arg_owned(values.get(1).cloned(), "", "linux.mknod", span)?;
                    let major = lowered_int_arg(values.get(2).cloned(), "linux.mknod", span)?;
                    let minor = lowered_int_arg(values.get(3).cloned(), "linux.mknod", span)?;
                    let host_path = self.host_path(&path);
                    linux_module::mknod(&host_path, &kind, major, minor, span)
                }
                RuntimeOp::LinuxMkswap => {
                    let device = lowered_path_arg(
                        unix_require_arg(values.first().cloned(), "linux.mkswap", span)?,
                        "linux.mkswap",
                        span,
                    )?;
                    let host_device = self.host_path(&device);
                    linux_module::mkswap(&host_device, span)
                }
                RuntimeOp::LinuxModprobe => {
                    let name =
                        lowered_str_arg_owned(values.first().cloned(), "", "linux.modprobe", span)?;
                    let params =
                        lowered_str_arg_owned(values.get(1).cloned(), "", "linux.modprobe", span)?;
                    linux_module::modprobe(&name, &params, span)
                }
                RuntimeOp::LinuxMount => {
                    let source =
                        lowered_str_arg_owned(values.first().cloned(), "", "linux.mount", span)?;
                    let target = lowered_path_arg(
                        unix_require_arg(values.get(1).cloned(), "linux.mount", span)?,
                        "linux.mount",
                        span,
                    )?;
                    let fstype =
                        lowered_str_arg_owned(values.get(2).cloned(), "", "linux.mount", span)?;
                    let options =
                        lowered_optional_str_list(values.get(3).cloned(), "linux.mount", span)?;
                    let host_target = self.host_path(&target);
                    linux_module::mount(&source, &host_target, &fstype, &options, span)
                }
                RuntimeOp::LinuxMountAll => linux_module::mount_all(span),
                RuntimeOp::LinuxPivotRoot => {
                    let new_root = lowered_path_arg(
                        unix_require_arg(values.first().cloned(), "linux.pivot_root", span)?,
                        "linux.pivot_root",
                        span,
                    )?;
                    let put_old = lowered_path_arg(
                        unix_require_arg(values.get(1).cloned(), "linux.pivot_root", span)?,
                        "linux.pivot_root",
                        span,
                    )?;
                    let host_new_root = self.host_path(&new_root);
                    let host_put_old = self.host_path(&put_old);
                    linux_module::pivot_root(&host_new_root, &host_put_old, span)
                }
                RuntimeOp::LinuxPoweroff => linux_module::poweroff(span),
                RuntimeOp::LinuxReboot => linux_module::reboot_system(span),
                RuntimeOp::LinuxRfkillBlock => {
                    let id = lowered_int_arg(values.first().cloned(), "linux.rfkill_block", span)?;
                    linux_module::rfkill_set(id, true, span)
                }
                RuntimeOp::LinuxRfkillList => linux_module::rfkill_list(span),
                RuntimeOp::LinuxRfkillUnblock => {
                    let id =
                        lowered_int_arg(values.first().cloned(), "linux.rfkill_unblock", span)?;
                    linux_module::rfkill_set(id, false, span)
                }
                RuntimeOp::LinuxRmmod => {
                    let name =
                        lowered_str_arg_owned(values.first().cloned(), "", "linux.rmmod", span)?;
                    let force =
                        lowered_bool_arg_or(values.get(1).cloned(), false, "linux.rmmod", span)?;
                    linux_module::rmmod(&name, force, span)
                }
                RuntimeOp::LinuxSetFileAttrs => {
                    let path = lowered_path_arg(
                        unix_require_arg(values.first().cloned(), "linux.set_file_attrs", span)?,
                        "linux.set_file_attrs",
                        span,
                    )?;
                    let flags =
                        lowered_int_arg(values.get(1).cloned(), "linux.set_file_attrs", span)?;
                    let host_path = self.host_path(&path);
                    linux_module::set_file_attrs(&host_path, flags, span)
                }
                RuntimeOp::LinuxSetFileVersion => {
                    let path = lowered_path_arg(
                        unix_require_arg(values.first().cloned(), "linux.set_file_version", span)?,
                        "linux.set_file_version",
                        span,
                    )?;
                    let version =
                        lowered_int_arg(values.get(1).cloned(), "linux.set_file_version", span)?;
                    let host_path = self.host_path(&path);
                    linux_module::set_file_version(&host_path, version, span)
                }
                RuntimeOp::LinuxSetHwclock => {
                    let epoch_ms =
                        lowered_int_arg(values.first().cloned(), "linux.set_hwclock", span)?;
                    linux_module::set_hwclock(epoch_ms, span)
                }
                RuntimeOp::LinuxSetSystemClock => {
                    let epoch_ms =
                        lowered_int_arg(values.first().cloned(), "linux.set_system_clock", span)?;
                    linux_module::set_system_clock(epoch_ms, span)
                }
                RuntimeOp::LinuxSwapon => {
                    let device = lowered_path_arg(
                        unix_require_arg(values.first().cloned(), "linux.swapon", span)?,
                        "linux.swapon",
                        span,
                    )?;
                    let priority =
                        lowered_int_arg_or(values.get(1).cloned(), -1, "linux.swapon", span)?;
                    let host_device = self.host_path(&device);
                    linux_module::swapon(&host_device, priority, span)
                }
                RuntimeOp::LinuxSwaponAll => linux_module::swapon_all(span),
                RuntimeOp::LinuxSwapoff => {
                    let device = lowered_path_arg(
                        unix_require_arg(values.first().cloned(), "linux.swapoff", span)?,
                        "linux.swapoff",
                        span,
                    )?;
                    let host_device = self.host_path(&device);
                    linux_module::swapoff(&host_device, span)
                }
                RuntimeOp::LinuxSwapoffAll => linux_module::swapoff_all(span),
                RuntimeOp::LinuxSwitchRoot => {
                    let new_root = lowered_path_arg(
                        unix_require_arg(values.first().cloned(), "linux.switch_root", span)?,
                        "linux.switch_root",
                        span,
                    )?;
                    let init = lowered_path_arg(
                        unix_require_arg(values.get(1).cloned(), "linux.switch_root", span)?,
                        "linux.switch_root",
                        span,
                    )?;
                    let host_new_root = self.host_path(&new_root);
                    let host_init = self.host_path(&init);
                    linux_module::switch_root(&host_new_root, &host_init, span)
                }
                RuntimeOp::LinuxSysctlLoadDirs => {
                    let dirs =
                        lowered_path_list(values.first().cloned(), "linux.sysctl_load_dirs", span)?;
                    let fallback = values
                        .get(1)
                        .cloned()
                        .map(|value| lowered_path_arg(value, "linux.sysctl_load_dirs", span))
                        .transpose()?;
                    let host_dirs: Vec<PathBuf> =
                        dirs.iter().map(|pv| self.host_path(pv)).collect();
                    let host_fallback = fallback.as_ref().map(|pv| self.host_path(pv));
                    linux_module::sysctl_load_dirs(&host_dirs, host_fallback.as_deref(), span)
                }
                RuntimeOp::LinuxSysctlSet => {
                    let key = lowered_str_arg_owned(
                        values.first().cloned(),
                        "",
                        "linux.sysctl_set",
                        span,
                    )?;
                    let value = lowered_str_arg_owned(
                        values.get(1).cloned(),
                        "",
                        "linux.sysctl_set",
                        span,
                    )?;
                    linux_module::sysctl_set(&key, &value, span)
                }
                RuntimeOp::LinuxUmountAll => {
                    let types = lowered_optional_str_list(
                        values.first().cloned(),
                        "linux.umount_all",
                        span,
                    )?;
                    linux_module::umount_all(&types, span)
                }
                RuntimeOp::LinuxWritePartitionTable => {
                    let device = lowered_path_arg(
                        unix_require_arg(
                            values.first().cloned(),
                            "linux.write_partition_table",
                            span,
                        )?,
                        "linux.write_partition_table",
                        span,
                    )?;
                    let table = lowered_record_arg(
                        values.get(1).cloned(),
                        "linux.write_partition_table",
                        span,
                    )?;
                    let host_device = self.host_path(&device);
                    linux_module::write_partition_table(&host_device, &table, span)
                }
                _ => unreachable!(
                    "new linux RuntimeOp routed to eval_linux_call_value must be handled"
                ),
            };
        }
        match op {
            RuntimeOp::LinuxUeventStream => {
                self.linux_dry_run_log("uevent_stream", &[], span)?;
                Ok(Value::ok(Value::stream(StreamValue::from_live(
                    "linux.uevent_stream.dry_run",
                    DryRunUeventStream::default(),
                ))))
            }
            RuntimeOp::LinuxMount => {
                let source =
                    lowered_str_arg_owned(values.first().cloned(), "", "linux.mount", span)?;
                let target = lowered_path_arg(
                    unix_require_arg(values.get(1).cloned(), "linux.mount", span)?,
                    "linux.mount",
                    span,
                )?;
                let fstype =
                    lowered_str_arg_owned(values.get(2).cloned(), "", "linux.mount", span)?;
                let options =
                    lowered_optional_str_list(values.get(3).cloned(), "linux.mount", span)?;
                self.linux_dry_run_log(
                    "mount",
                    &[
                        ("source", source),
                        ("target", target.display()),
                        ("fstype", fstype),
                        ("options", options.join(",")),
                    ],
                    span,
                )?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxMountAll => {
                self.linux_dry_run_log("mount_all", &[], span)?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxUmountAll => {
                let types =
                    lowered_optional_str_list(values.first().cloned(), "linux.umount_all", span)?;
                self.linux_dry_run_log("umount_all", &[("types", types.join(","))], span)?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxSwaponAll => {
                self.linux_dry_run_log("swapon_all", &[], span)?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxSwapoffAll => {
                self.linux_dry_run_log("swapoff_all", &[], span)?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxRootDevice => {
                let root = self.linux_dry_run_env("XSH_LINUX_ROOT_DEVICE", "rootfs");
                self.linux_dry_run_log("root_device", &[("device", root.clone())], span)?;
                Ok(Value::ok(Value::Str(root.into())))
            }
            RuntimeOp::LinuxMemInfo => {
                self.linux_dry_run_log("meminfo", &[], span)?;
                Ok(Value::ok(Value::Record(RecordMap::from([
                    (Arc::from("total"), Value::Int(1024 * 1024 * 1024)),
                    (Arc::from("free"), Value::Int(256 * 1024 * 1024)),
                    (Arc::from("available"), Value::Int(512 * 1024 * 1024)),
                    (Arc::from("buffers"), Value::Int(64 * 1024 * 1024)),
                    (Arc::from("cached"), Value::Int(128 * 1024 * 1024)),
                    (Arc::from("swap_total"), Value::Int(512 * 1024 * 1024)),
                    (Arc::from("swap_free"), Value::Int(384 * 1024 * 1024)),
                ]))))
            }
            RuntimeOp::LinuxModules => {
                self.linux_dry_run_log("modules", &[], span)?;
                Ok(Value::ok(Value::stream(StreamValue::from_values_live(
                    "linux.modules",
                    vec![Value::Record(RecordMap::from([
                        (Arc::from("name"), Value::Str("xsh_demo".into())),
                        (Arc::from("size"), Value::Int(4096)),
                        (
                            Arc::from("used_by"),
                            Value::List(vec![Value::Str("xsh_dep".into())]),
                        ),
                    ]))],
                ))))
            }
            RuntimeOp::LinuxDmesg => {
                self.linux_dry_run_log("dmesg", &[], span)?;
                Ok(Value::ok(Value::stream(StreamValue::from_values_live(
                    "linux.dmesg",
                    vec![Value::Str("xsh dry-run kernel message".into())],
                ))))
            }
            RuntimeOp::LinuxIsMountpoint => {
                let path = lowered_path_arg(
                    unix_require_arg(values.first().cloned(), "linux.is_mountpoint", span)?,
                    "linux.is_mountpoint",
                    span,
                )?;
                self.linux_dry_run_log("is_mountpoint", &[("path", path.display())], span)?;
                Ok(Value::ok(Value::Bool(
                    path.display() == "/" || path.display() == "/proc",
                )))
            }
            RuntimeOp::LinuxDiskUsage => {
                let path = match values.first().cloned() {
                    Some(value) => {
                        Some(lowered_path_arg(value, "linux.disk_usage", span)?.display())
                    }
                    None => None,
                };
                let mount = path.unwrap_or_else(|| "/".to_string());
                self.linux_dry_run_log("disk_usage", &[("path", mount.clone())], span)?;
                Ok(Value::ok(Value::stream(StreamValue::from_values_live(
                    "linux.disk_usage",
                    vec![Value::Record(RecordMap::from([
                        (Arc::from("device"), Value::Str("rootfs".into())),
                        (Arc::from("mount"), Value::Str(mount.into())),
                        (Arc::from("fstype"), Value::Str("tmpfs".into())),
                        (Arc::from("total"), Value::Int(1024 * 1024 * 1024)),
                        (Arc::from("used"), Value::Int(256 * 1024 * 1024)),
                        (Arc::from("available"), Value::Int(768 * 1024 * 1024)),
                    ]))],
                ))))
            }
            RuntimeOp::LinuxBlockDevices => {
                self.linux_dry_run_log("block_devices", &[], span)?;
                Ok(Value::ok(Value::stream(StreamValue::from_values_live(
                    "linux.block_devices",
                    vec![
                        Value::Record(RecordMap::from([
                            (Arc::from("name"), Value::Str("vda".into())),
                            (
                                Arc::from("path"),
                                Value::Path(path_value_from_pathbuf(PathBuf::from("/dev/vda"))?),
                            ),
                            (Arc::from("size"), Value::Int(128 * 1024 * 1024)),
                            (Arc::from("sectors"), Value::Int(262144)),
                            (Arc::from("sector_size"), Value::Int(512)),
                            (Arc::from("removable"), Value::Bool(false)),
                            (Arc::from("rotational"), Value::Bool(false)),
                            (Arc::from("partitioned"), Value::Bool(false)),
                            (Arc::from("partitions"), Value::List(Vec::new())),
                        ])),
                        Value::Record(RecordMap::from([
                            (Arc::from("name"), Value::Str("vdb".into())),
                            (
                                Arc::from("path"),
                                Value::Path(path_value_from_pathbuf(PathBuf::from("/dev/vdb"))?),
                            ),
                            (Arc::from("size"), Value::Int(128 * 1024 * 1024)),
                            (Arc::from("sectors"), Value::Int(262144)),
                            (Arc::from("sector_size"), Value::Int(512)),
                            (Arc::from("removable"), Value::Bool(false)),
                            (Arc::from("rotational"), Value::Bool(false)),
                            (Arc::from("partitioned"), Value::Bool(true)),
                            (
                                Arc::from("partitions"),
                                Value::List(vec![Value::Path(path_value_from_pathbuf(
                                    PathBuf::from("/dev/vdb1"),
                                )?)]),
                            ),
                        ])),
                    ],
                ))))
            }
            RuntimeOp::LinuxSysctlGet => {
                let key =
                    lowered_str_arg_owned(values.first().cloned(), "", "linux.sysctl_get", span)?;
                if let Err(error) = validate_linux_sysctl_key(&key, span) {
                    return Ok(Value::err(Value::Error(Box::new(error))));
                }
                self.linux_dry_run_log("sysctl_get", &[("key", key)], span)?;
                Ok(Value::ok(Value::Str(
                    self.linux_dry_run_env("XSH_LINUX_SYSCTL_VALUE", "1").into(),
                )))
            }
            RuntimeOp::LinuxSysctlSet => {
                let key =
                    lowered_str_arg_owned(values.first().cloned(), "", "linux.sysctl_set", span)?;
                let value =
                    lowered_str_arg_owned(values.get(1).cloned(), "", "linux.sysctl_set", span)?;
                if let Err(error) = validate_linux_sysctl_key(&key, span) {
                    return Ok(Value::err(Value::Error(Box::new(error))));
                }
                self.linux_dry_run_log("sysctl_set", &[("key", key), ("value", value)], span)?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxFileAttrs => {
                let path = lowered_path_arg(
                    unix_require_arg(values.first().cloned(), "linux.file_attrs", span)?,
                    "linux.file_attrs",
                    span,
                )?;
                let flags = match self.linux_dry_run_file_attrs_flags(span) {
                    Ok(flags) => flags,
                    Err(error) => return Ok(Value::err(Value::Error(Box::new(error)))),
                };
                self.linux_dry_run_log("file_attrs", &[("path", path.display())], span)?;
                Ok(Value::ok(linux_file_attrs_record(flags)))
            }
            RuntimeOp::LinuxSetFileAttrs => {
                let path = lowered_path_arg(
                    unix_require_arg(values.first().cloned(), "linux.set_file_attrs", span)?,
                    "linux.set_file_attrs",
                    span,
                )?;
                let flags = lowered_int_arg(values.get(1).cloned(), "linux.set_file_attrs", span)?;
                if let Err(error) = validate_linux_file_attrs_flags(flags, span) {
                    return Ok(Value::err(Value::Error(Box::new(error))));
                }
                self.linux_dry_run_log(
                    "set_file_attrs",
                    &[("path", path.display()), ("flags", flags.to_string())],
                    span,
                )?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxFileVersion => {
                let path = lowered_path_arg(
                    unix_require_arg(values.first().cloned(), "linux.file_version", span)?,
                    "linux.file_version",
                    span,
                )?;
                let version = self.linux_dry_run_file_version(span)?;
                self.linux_dry_run_log("file_version", &[("path", path.display())], span)?;
                Ok(Value::ok(Value::Int(version)))
            }
            RuntimeOp::LinuxSetFileVersion => {
                let path = lowered_path_arg(
                    unix_require_arg(values.first().cloned(), "linux.set_file_version", span)?,
                    "linux.set_file_version",
                    span,
                )?;
                let version =
                    lowered_int_arg(values.get(1).cloned(), "linux.set_file_version", span)?;
                if let Err(error) = validate_linux_file_version(version, span) {
                    return Ok(Value::err(Value::Error(Box::new(error))));
                }
                self.linux_dry_run_log(
                    "set_file_version",
                    &[("path", path.display()), ("version", version.to_string())],
                    span,
                )?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxSysctlLoadDirs => {
                let dirs =
                    lowered_path_list(values.first().cloned(), "linux.sysctl_load_dirs", span)?;
                let fallback = match values.get(1).cloned() {
                    Some(value) => {
                        lowered_path_arg(value, "linux.sysctl_load_dirs", span)?.display()
                    }
                    None => String::new(),
                };
                let dir_text = dirs
                    .iter()
                    .map(PathValue::display)
                    .collect::<Vec<_>>()
                    .join(",");
                self.linux_dry_run_log(
                    "sysctl_load_dirs",
                    &[("dirs", dir_text), ("fallback", fallback)],
                    span,
                )?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxKillAll => {
                let signal =
                    lowered_str_arg_owned(values.first().cloned(), "TERM", "linux.kill_all", span)?;
                if let Err(error) = process_module::signal_info(&signal, span) {
                    return Ok(Value::err(Value::Error(Box::new(error))));
                }
                let except_pid1 =
                    lowered_bool_arg_or(values.get(1).cloned(), false, "linux.kill_all", span)?;
                self.linux_dry_run_log(
                    "kill_all",
                    &[("signal", signal), ("except_pid1", except_pid1.to_string())],
                    span,
                )?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxChroot => {
                let path = lowered_path_arg(
                    unix_require_arg(values.first().cloned(), "linux.chroot", span)?,
                    "linux.chroot",
                    span,
                )?;
                self.linux_dry_run_log("chroot", &[("path", path.display())], span)?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxMknod => {
                let path = lowered_path_arg(
                    unix_require_arg(values.first().cloned(), "linux.mknod", span)?,
                    "linux.mknod",
                    span,
                )?;
                let kind = lowered_str_arg_owned(values.get(1).cloned(), "", "linux.mknod", span)?;
                let major = lowered_int_arg(values.get(2).cloned(), "linux.mknod", span)?;
                let minor = lowered_int_arg(values.get(3).cloned(), "linux.mknod", span)?;
                if let Err(error) = validate_mknod_args(&kind, major, minor, span) {
                    return Ok(Value::err(Value::Error(Box::new(error))));
                }
                self.linux_dry_run_log(
                    "mknod",
                    &[
                        ("path", path.display()),
                        ("kind", kind),
                        ("major", major.to_string()),
                        ("minor", minor.to_string()),
                    ],
                    span,
                )?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxInsmod => {
                let path = lowered_path_arg(
                    unix_require_arg(values.first().cloned(), "linux.insmod", span)?,
                    "linux.insmod",
                    span,
                )?;
                let params =
                    lowered_str_arg_owned(values.get(1).cloned(), "", "linux.insmod", span)?;
                self.linux_dry_run_log(
                    "insmod",
                    &[("path", path.display()), ("params", params)],
                    span,
                )?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxRmmod => {
                let name = lowered_str_arg_owned(values.first().cloned(), "", "linux.rmmod", span)?;
                let force =
                    lowered_bool_arg_or(values.get(1).cloned(), false, "linux.rmmod", span)?;
                self.linux_dry_run_log(
                    "rmmod",
                    &[("name", name), ("force", force.to_string())],
                    span,
                )?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxPivotRoot => {
                let new_root = lowered_path_arg(
                    unix_require_arg(values.first().cloned(), "linux.pivot_root", span)?,
                    "linux.pivot_root",
                    span,
                )?;
                let put_old = lowered_path_arg(
                    unix_require_arg(values.get(1).cloned(), "linux.pivot_root", span)?,
                    "linux.pivot_root",
                    span,
                )?;
                self.linux_dry_run_log(
                    "pivot_root",
                    &[
                        ("new_root", new_root.display()),
                        ("put_old", put_old.display()),
                    ],
                    span,
                )?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxSwitchRoot => {
                let new_root = lowered_path_arg(
                    unix_require_arg(values.first().cloned(), "linux.switch_root", span)?,
                    "linux.switch_root",
                    span,
                )?;
                let init = lowered_path_arg(
                    unix_require_arg(values.get(1).cloned(), "linux.switch_root", span)?,
                    "linux.switch_root",
                    span,
                )?;
                self.linux_dry_run_log(
                    "switch_root",
                    &[("new_root", new_root.display()), ("init", init.display())],
                    span,
                )?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxHwclock => {
                self.linux_dry_run_log("hwclock", &[], span)?;
                let epoch_ms = self
                    .linux_dry_run_env("XSH_LINUX_HWCLOCK_EPOCH_MS", "0")
                    .parse::<i64>()
                    .unwrap_or(0);
                Ok(Value::ok(Value::Int(epoch_ms)))
            }
            RuntimeOp::LinuxSetHwclock => {
                let epoch_ms = lowered_int_arg(values.first().cloned(), "linux.set_hwclock", span)?;
                self.linux_dry_run_log("set_hwclock", &[("epoch_ms", epoch_ms.to_string())], span)?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxSetSystemClock => {
                let epoch_ms =
                    lowered_int_arg(values.first().cloned(), "linux.set_system_clock", span)?;
                self.linux_dry_run_log(
                    "set_system_clock",
                    &[("epoch_ms", epoch_ms.to_string())],
                    span,
                )?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxRfkillList => {
                self.linux_dry_run_log("rfkill_list", &[], span)?;
                Ok(Value::ok(Value::stream(StreamValue::from_values_live(
                    "linux.rfkill_list",
                    vec![Value::Record(RecordMap::from([
                        (Arc::from("id"), Value::Int(0)),
                        (Arc::from("name"), Value::Str("phy0".into())),
                        (Arc::from("type"), Value::Str("wlan".into())),
                        (Arc::from("soft_blocked"), Value::Bool(false)),
                        (Arc::from("hard_blocked"), Value::Bool(false)),
                    ]))],
                ))))
            }
            RuntimeOp::LinuxRfkillBlock | RuntimeOp::LinuxRfkillUnblock => {
                let id = lowered_int_arg(values.first().cloned(), "linux.rfkill", span)?;
                if id < 0 {
                    return Ok(module_error("linux-rfkill", "id cannot be negative", span));
                }
                let op_name = if op == RuntimeOp::LinuxRfkillBlock {
                    "rfkill_block"
                } else {
                    "rfkill_unblock"
                };
                self.linux_dry_run_log(op_name, &[("id", id.to_string())], span)?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxLoopAttach => {
                let file = lowered_path_arg(
                    unix_require_arg(values.first().cloned(), "linux.loop_attach", span)?,
                    "linux.loop_attach",
                    span,
                )?;
                let device = match values.get(1).cloned() {
                    Some(value) => lowered_path_arg(value, "linux.loop_attach", span)?.display(),
                    None => "/dev/loop0".to_string(),
                };
                self.linux_dry_run_log(
                    "loop_attach",
                    &[("file", file.display()), ("device", device.clone())],
                    span,
                )?;
                path_value_from_pathbuf(PathBuf::from(device))
                    .map(Value::Path)
                    .map(Value::ok)
                    .map_err(|error| error.with_span(span))
            }
            RuntimeOp::LinuxLoopDetach => {
                let device = lowered_path_arg(
                    unix_require_arg(values.first().cloned(), "linux.loop_detach", span)?,
                    "linux.loop_detach",
                    span,
                )?;
                self.linux_dry_run_log("loop_detach", &[("device", device.display())], span)?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxLoopList => {
                self.linux_dry_run_log("loop_list", &[], span)?;
                Ok(Value::ok(Value::stream(StreamValue::from_values_live(
                    "linux.loop_list",
                    vec![Value::Record(RecordMap::from([
                        (
                            Arc::from("device"),
                            Value::Path(path_value_from_pathbuf(PathBuf::from("/dev/loop0"))?),
                        ),
                        (
                            Arc::from("file"),
                            Value::Path(path_value_from_pathbuf(PathBuf::from("/tmp/disk.img"))?),
                        ),
                        (Arc::from("offset"), Value::Int(0)),
                        (Arc::from("size"), Value::Int(0)),
                    ]))],
                ))))
            }
            RuntimeOp::LinuxMkswap => {
                let device = lowered_path_arg(
                    unix_require_arg(values.first().cloned(), "linux.mkswap", span)?,
                    "linux.mkswap",
                    span,
                )?;
                self.linux_dry_run_log("mkswap", &[("device", device.display())], span)?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxSwapon => {
                let device = lowered_path_arg(
                    unix_require_arg(values.first().cloned(), "linux.swapon", span)?,
                    "linux.swapon",
                    span,
                )?;
                let priority =
                    lowered_int_arg_or(values.get(1).cloned(), -1, "linux.swapon", span)?;
                self.linux_dry_run_log(
                    "swapon",
                    &[
                        ("device", device.display()),
                        ("priority", priority.to_string()),
                    ],
                    span,
                )?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxSwapoff => {
                let device = lowered_path_arg(
                    unix_require_arg(values.first().cloned(), "linux.swapoff", span)?,
                    "linux.swapoff",
                    span,
                )?;
                self.linux_dry_run_log("swapoff", &[("device", device.display())], span)?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxBlkid => {
                let device = lowered_path_arg(
                    unix_require_arg(values.first().cloned(), "linux.blkid", span)?,
                    "linux.blkid",
                    span,
                )?;
                self.linux_dry_run_log("blkid", &[("device", device.display())], span)?;
                Ok(Value::ok(Value::Record(RecordMap::from([
                    (Arc::from("type"), Value::Str("ext4".into())),
                    (
                        Arc::from("uuid"),
                        Value::Str("00000000-0000-0000-0000-000000000001".into()),
                    ),
                    (Arc::from("label"), Value::Str("rootfs".into())),
                    (Arc::from("part_table_type"), Value::Str("gpt".into())),
                    (
                        Arc::from("part_entry_uuid"),
                        Value::Str("00000000-0000-0000-0000-000000000002".into()),
                    ),
                ]))))
            }
            RuntimeOp::LinuxModinfo => {
                let name =
                    lowered_str_arg_owned(values.first().cloned(), "", "linux.modinfo", span)?;
                self.linux_dry_run_log("modinfo", &[("name", name.clone())], span)?;
                Ok(Value::ok(Value::Record(RecordMap::from([
                    (Arc::from("name"), Value::Str(name.into())),
                    (
                        Arc::from("filename"),
                        Value::Path(path_value_from_pathbuf(PathBuf::from(
                            "/lib/modules/dry-run/demo.ko",
                        ))?),
                    ),
                    (
                        Arc::from("description"),
                        Value::Str("dry-run module".into()),
                    ),
                    (Arc::from("license"), Value::Str("GPL".into())),
                    (Arc::from("version"), Value::Str("1".into())),
                    (
                        Arc::from("params"),
                        Value::List(vec![Value::Record(RecordMap::from([
                            (Arc::from("name"), Value::Str("debug".into())),
                            (Arc::from("type"), Value::Str("bool".into())),
                            (Arc::from("description"), Value::Str("enable debug".into())),
                        ]))]),
                    ),
                ]))))
            }
            RuntimeOp::LinuxModprobe => {
                let name =
                    lowered_str_arg_owned(values.first().cloned(), "", "linux.modprobe", span)?;
                let params =
                    lowered_str_arg_owned(values.get(1).cloned(), "", "linux.modprobe", span)?;
                self.linux_dry_run_log("modprobe", &[("name", name), ("params", params)], span)?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxDepmod => {
                let version =
                    lowered_str_arg_owned(values.first().cloned(), "", "linux.depmod", span)?;
                self.linux_dry_run_log("depmod", &[("version", version)], span)?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxOpenFiles => {
                let pid = match values.first().cloned() {
                    Some(value) => Some(lowered_int_arg(Some(value), "linux.open_files", span)?),
                    None => None,
                };
                let pid_value = pid.unwrap_or(123);
                self.linux_dry_run_log("open_files", &[("pid", pid_value.to_string())], span)?;
                Ok(Value::ok(Value::stream(StreamValue::from_values_live(
                    "linux.open_files",
                    vec![Value::Record(RecordMap::from([
                        (Arc::from("pid"), Value::Int(pid_value)),
                        (Arc::from("command"), Value::Str("xsh".into())),
                        (Arc::from("fd"), Value::Int(1)),
                        (Arc::from("type"), Value::Str("file".into())),
                        (
                            Arc::from("path"),
                            Value::Path(path_value_from_pathbuf(PathBuf::from("/tmp/xsh.log"))?),
                        ),
                        (Arc::from("inode"), Value::Int(0)),
                        (Arc::from("protocol"), Value::Str("".into())),
                        (Arc::from("local"), Value::Str("".into())),
                        (Arc::from("remote"), Value::Str("".into())),
                    ]))],
                ))))
            }
            RuntimeOp::LinuxPartitionTable => {
                let device = lowered_path_arg(
                    unix_require_arg(values.first().cloned(), "linux.partition_table", span)?,
                    "linux.partition_table",
                    span,
                )?;
                self.linux_dry_run_log("partition_table", &[("device", device.display())], span)?;
                Ok(Value::ok(linux_dry_run_partition_table()))
            }
            RuntimeOp::LinuxWritePartitionTable => {
                let device = lowered_path_arg(
                    unix_require_arg(values.first().cloned(), "linux.write_partition_table", span)?,
                    "linux.write_partition_table",
                    span,
                )?;
                let _table = lowered_record_arg(
                    values.get(1).cloned(),
                    "linux.write_partition_table",
                    span,
                )?;
                self.linux_dry_run_log(
                    "write_partition_table",
                    &[("device", device.display())],
                    span,
                )?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxFsck => {
                let device = lowered_path_arg(
                    unix_require_arg(values.first().cloned(), "linux.fsck", span)?,
                    "linux.fsck",
                    span,
                )?;
                let fstype = lowered_str_arg_owned(values.get(1).cloned(), "", "linux.fsck", span)?;
                let repair =
                    lowered_bool_arg_or(values.get(2).cloned(), false, "linux.fsck", span)?;
                self.linux_dry_run_log(
                    "fsck",
                    &[
                        ("device", device.display()),
                        ("fstype", fstype),
                        ("repair", repair.to_string()),
                    ],
                    span,
                )?;
                Ok(Value::ok(Value::Record(RecordMap::from([
                    (Arc::from("status"), Value::Int(0)),
                    (Arc::from("errors"), Value::List(Vec::new())),
                ]))))
            }
            RuntimeOp::LinuxHalt => {
                self.linux_dry_run_log("halt", &[], span)?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxPoweroff => {
                self.linux_dry_run_log("poweroff", &[], span)?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxReboot => {
                self.linux_dry_run_log("reboot", &[], span)?;
                Ok(Value::ok(Value::Unit))
            }
            _ => unreachable!("linux dry-run operation expected"),
        }
    }

    fn lowered_env_path_entries(&self, span: Span) -> Result<Vec<PathValue>, RuntimeError> {
        let value = self.env.get_owned(b"PATH").unwrap_or_default();
        let os = std::ffi::OsString::from_vec(value);
        std::env::split_paths(&os)
            .map(|pathbuf| path_value_from_pathbuf(pathbuf).map_err(|error| error.with_span(span)))
            .collect::<Result<Vec<_>, _>>()
    }

    fn lowered_set_env_path_entries(
        &mut self,
        entries: &[PathValue],
        _span: Span,
    ) -> Result<(), RuntimeError> {
        let pathbufs: Vec<PathBuf> = entries.iter().map(pathbuf_from_path_value).collect();
        let value = std::env::join_paths(&pathbufs)
            .map_err(|_| RuntimeError::new("invalid-path", "PATH entry contains invalid bytes"))?
            .as_os_str()
            .as_bytes()
            .to_vec();
        self.env.insert(b"PATH".to_vec(), value);
        Ok(())
    }

    fn try_bind_lowered_runtime_args(
        &mut self,
        lowered: &FunctionHeader,
        args: &[Value],
    ) -> Option<Vec<LoweredValue>> {
        let values = if let Some(rest_index) = lowered_rest_index(lowered) {
            if args.len() < lowered_required_arg_count(lowered)
                || lowered.param_kinds[rest_index] != LoweredType::List
            {
                return None;
            }
            let mut values = Vec::with_capacity(lowered.params.len());
            for (index, kind) in lowered.param_kinds.iter().copied().enumerate() {
                if index == rest_index {
                    let value = LoweredValue::List(
                        args.get(index..)
                            .unwrap_or(&[])
                            .iter()
                            .map(lowered_value_from_runtime_any)
                            .collect::<Option<Vec<_>>>()?,
                    );
                    if !lowered_value_matches_param(lowered, index, LoweredType::List, &value) {
                        return None;
                    }
                    values.push(value);
                    break;
                }
                match args.get(index) {
                    Some(value) => {
                        if !lowered_runtime_arg_matches_param(lowered, index, value) {
                            return None;
                        }
                        values.push(lowered_value_from_runtime(value, kind)?);
                    }
                    None => values.push(lowered.param_defaults.get(index)?.clone()?),
                }
            }
            values
        } else {
            if args.len() < lowered_required_arg_count(lowered) || args.len() > lowered.params.len()
            {
                return None;
            }
            let mut values = Vec::with_capacity(lowered.params.len());
            for (index, kind) in lowered.param_kinds.iter().copied().enumerate() {
                match args.get(index) {
                    Some(value) => {
                        if !lowered_runtime_arg_matches_param(lowered, index, value) {
                            return None;
                        }
                        values.push(lowered_value_from_runtime(value, kind)?);
                    }
                    None => values.push(lowered.param_defaults.get(index)?.clone()?),
                }
            }
            values
        };
        Some(self.lowered_call_slots(lowered, values))
    }

    fn bind_lowered_values(
        &mut self,
        lowered: &FunctionHeader,
        args: &[LoweredValue],
        span: Span,
    ) -> Result<Vec<LoweredValue>, RuntimeError> {
        let values = if let Some(rest_index) = lowered_rest_index(lowered) {
            if args.len() < lowered_required_arg_count(lowered)
                || lowered.param_kinds[rest_index] != LoweredType::List
            {
                return Err(RuntimeError::new(
                    "arity",
                    lowered_call_arity_message(lowered, args.len()),
                )
                .with_span(span));
            }
            let mut values = Vec::with_capacity(lowered.params.len());
            for (index, kind) in lowered.param_kinds.iter().copied().enumerate() {
                if index == rest_index {
                    let value = LoweredValue::List(args.get(index..).unwrap_or(&[]).to_vec());
                    if !lowered_value_matches_param(lowered, index, LoweredType::List, &value) {
                        return Err(RuntimeError::new(
                            "type-error",
                            format!(
                                "lowered call expected {}, found List",
                                lowered_param_type_name(lowered, index, LoweredType::List)
                            ),
                        )
                        .with_span(span));
                    }
                    values.push(value);
                    break;
                }
                match args.get(index) {
                    Some(value) => {
                        if !lowered_value_matches_param(lowered, index, kind, value) {
                            return Err(RuntimeError::new(
                                "type-error",
                                format!(
                                    "lowered call expected {}, found {}",
                                    lowered_param_type_name(lowered, index, kind),
                                    value.type_name()
                                ),
                            )
                            .with_span(span));
                        }
                        values.push(value.clone());
                    }
                    None => {
                        let Some(Some(default)) = lowered.param_defaults.get(index) else {
                            return Err(RuntimeError::new(
                                "arity",
                                lowered_call_arity_message(lowered, args.len()),
                            )
                            .with_span(span));
                        };
                        values.push(default.clone());
                    }
                }
            }
            values
        } else {
            if args.len() < lowered_required_arg_count(lowered) || args.len() > lowered.params.len()
            {
                return Err(RuntimeError::new(
                    "arity",
                    lowered_call_arity_message(lowered, args.len()),
                )
                .with_span(span));
            }
            let mut values = Vec::with_capacity(lowered.params.len());
            for (index, kind) in lowered.param_kinds.iter().copied().enumerate() {
                match args.get(index) {
                    Some(value) => {
                        if !lowered_value_matches_param(lowered, index, kind, value) {
                            return Err(RuntimeError::new(
                                "type-error",
                                format!(
                                    "lowered call expected {}, found {}",
                                    lowered_param_type_name(lowered, index, kind),
                                    value.type_name()
                                ),
                            )
                            .with_span(span));
                        }
                        values.push(value.clone());
                    }
                    None => {
                        let Some(Some(default)) = lowered.param_defaults.get(index) else {
                            return Err(RuntimeError::new(
                                "arity",
                                lowered_call_arity_message(lowered, args.len()),
                            )
                            .with_span(span));
                        };
                        values.push(default.clone());
                    }
                }
            }
            values
        };
        Ok(self.lowered_call_slots(lowered, values))
    }

    fn lowered_call_slots(
        &mut self,
        lowered: &FunctionHeader,
        values: Vec<LoweredValue>,
    ) -> Vec<LoweredValue> {
        let mut slots = self.take_lowered_slots(lowered.slot_count);
        for (slot, value) in values.into_iter().enumerate() {
            slots[slot] = value;
        }
        slots
    }

    fn call_lowered_function_value_with_values(
        &mut self,
        function: FunctionName,
        pure: bool,
        args: &[Value],
        call_span: Span,
    ) -> Option<Result<Value, RuntimeError>> {
        let function = function
            .as_name()
            .map(LoweredFunctionKey::Name)
            .or_else(|| function.as_qualified().map(LoweredFunctionKey::Qualified))?;
        self.call_indexed_direct(
            function,
            if pure {
                LoweredFunctionKind::Pure
            } else {
                LoweredFunctionKind::Proc
            },
            args,
            call_span,
        )
    }

    fn hydrate_lowered_captures(
        &mut self,
        lowered: &FunctionHeader,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<(), RuntimeError> {
        for capture in &lowered.captures {
            let Some(binding) = self.lookup(capture.name) else {
                return Err(RuntimeError::new(
                    "unknown-name",
                    format!("unknown captured name `{}`", capture.name),
                )
                .with_span(call_span));
            };
            let Some(value) = self
                .lookup_lowered_capture(capture.name, capture.kind)
                .or_else(|| lowered_value_from_runtime_any(&binding.value))
            else {
                let detail = if capture.kind == LoweredType::Module {
                    format!(
                        " (found runtime value of type `{}`; the module namespace \
                         may have been overwritten by a same-named export or \
                         top-level binding)",
                        binding.value.type_name()
                    )
                } else {
                    format!(
                        " (found runtime value of type `{}`)",
                        binding.value.type_name()
                    )
                };
                return Err(RuntimeError::new(
                    "type-error",
                    format!(
                        "captured name `{}` no longer matches lowered type {:?}{}",
                        capture.name, capture.kind, detail
                    ),
                )
                .with_span(call_span));
            };
            slots[capture.slot] = value;
        }
        Ok(())
    }

    fn lookup_lowered_capture(&self, name: Name, kind: LoweredType) -> Option<LoweredValue> {
        self.scopes.iter().rev().find_map(|scope| {
            scope
                .get(&name)
                .and_then(|binding| lowered_value_from_runtime(&binding.value, kind))
        })
    }

    fn write_back_lowered_captures(
        &mut self,
        lowered: &FunctionHeader,
        slots: &[LoweredValue],
        call_span: Span,
    ) -> Result<(), RuntimeError> {
        for capture in &lowered.captures {
            if capture.mutable {
                self.assign(
                    &capture.name.as_str(),
                    slots[capture.slot].clone().into_value(),
                    call_span,
                )?;
            }
        }
        Ok(())
    }

    pub(super) fn take_lowered_slots(&mut self, slot_count: usize) -> Vec<LoweredValue> {
        let mut slots = self.lowered_slot_pool.pop().unwrap_or_default();
        slots.clear();
        slots.resize(slot_count, LoweredValue::Int(0));
        slots
    }

    pub(super) fn recycle_lowered_slots(&mut self, mut slots: Vec<LoweredValue>) {
        const LOWERED_SLOT_POOL_CAP: usize = 64;
        if self.lowered_slot_pool.len() < LOWERED_SLOT_POOL_CAP {
            slots.clear();
            self.lowered_slot_pool.push(slots);
        }
    }

    fn lowered_question_propagation_value(
        &mut self,
        value: LoweredValue,
        span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        match self.question_flow(value.into_value(), span) {
            Flow::Propagate(propagation) => {
                self.pending_traceback = Some(propagation.traceback);
                Ok(LoweredValue::ResultErr(Box::new(propagation.error)))
            }
            Flow::Continue(_) => Err(RuntimeError::new(
                "control-flow",
                "lowered propagation unexpectedly continued",
            )
            .with_span(span)),
            Flow::Return(_) | Flow::Break(_) | Flow::ContinueLoop => Err(RuntimeError::new(
                "control-flow",
                "lowered propagation produced unsupported control flow",
            )
            .with_span(span)),
        }
    }

    fn lowered_retry_attempt_value(&mut self, flow: StmtFlow) -> LoweredRetryAttemptValue {
        match flow {
            StmtFlow::None | StmtFlow::Continue => {
                LoweredRetryAttemptValue::Success(LoweredValue::Unit)
            }
            // The retry body's trailing expression is lowered as `BreakValue`, so
            // a successful attempt's value arrives as `Break(Some(..))`.
            StmtFlow::Break(Some(value)) => LoweredRetryAttemptValue::Success(value),
            StmtFlow::Break(None) => LoweredRetryAttemptValue::ControlBreak,
            // `?` failures inside the body propagate; the retry catches them and
            // retries (or surfaces the final error once attempts are exhausted).
            // The propagation value is a `ResultErr` wrapping the real error;
            // unwrap it so the attempt trace and final error carry the actual
            // error value rather than a `Result` wrapper.
            StmtFlow::Propagate(value) => {
                let error = match value {
                    LoweredValue::ResultErr(error) => *error,
                    other => other.into_value(),
                };
                LoweredRetryAttemptValue::Failed {
                    error,
                    traceback: self.pending_traceback.take(),
                }
            }
            // An explicit `return` escapes the retry and returns from the proc.
            StmtFlow::Return(value) => LoweredRetryAttemptValue::Escape(value),
        }
    }

    fn sleep_lowered_retry_delay(
        &mut self,
        delay: &DurationValue,
        span: Span,
    ) -> Result<(), RuntimeError> {
        let deadline = Instant::now() + Duration::from_millis(delay.millis);
        while Instant::now() < deadline {
            self.service_pending_signal(span)?;
            if self.signal_state.shutdown_complete {
                break;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            std::thread::sleep(std::cmp::min(Duration::from_millis(10), remaining));
        }
        Ok(())
    }

    fn trace_lowered_retry_attempt(
        &mut self,
        span: Span,
        attempt: usize,
        max_attempts: usize,
        next_delay_ms: Option<u64>,
        error: Option<TraceError>,
    ) {
        self.trace_leaf(
            TraceKind::RetryAttempt,
            Some(span),
            Some("retry"),
            TracePayload::RetryAttempt {
                attempt,
                max_attempts,
                next_delay_ms,
                error,
            },
        );
    }

    #[inline(never)]
    /// Open a file for `Path.lines()` / `Path.bytes_lines()` and wrap it in a
    /// live line stream. Returns a `Result` value (Ok stream / Err on open
    /// failure), matching the runtime contract of the lowered method dispatch.
    fn path_lines_stream(
        &mut self,
        path: crate::runtime::value::PathValue,
        bytes: bool,
        span: Span,
    ) -> Value {
        match std::fs::File::open(self.host_path(&path)) {
            Ok(file) => {
                let stream = if bytes {
                    StreamValue::from_live(
                        "Path.bytes_lines",
                        super::stream::FileBytesLineStream {
                            reader: std::io::BufReader::new(file),
                            buffer: Vec::new(),
                        },
                    )
                } else {
                    StreamValue::from_live(
                        "Path.lines",
                        super::stream::FileLineStream {
                            reader: std::io::BufReader::new(file),
                            buffer: String::new(),
                        },
                    )
                };
                Value::ok(Value::stream(stream))
            }
            Err(error) => super::module_io_error("fs-read", error, span),
        }
    }

    /// Wrap a per-item stream-stage error: emit a `stream.item.error` trace
    /// leaf and reword the message as `stream stage `<stage>` item <i> failed:
    /// …`, preserving the original kind/span/abort (mirrors the deleted
    /// recursive evaluator). Abort errors pass through unchanged.
    fn stream_item_runtime_error(
        &mut self,
        stage: &str,
        item_index: usize,
        mut error: RuntimeError,
    ) -> RuntimeError {
        if error.abort.is_some() {
            return error;
        }
        self.trace_leaf(
            TraceKind::StreamItemError,
            error.span,
            Some(stage),
            TracePayload::StreamItem {
                stage: stage.to_string(),
                item_index,
                error: Some(TraceError::new(&error.kind, &error.message)),
            },
        );
        error.message = format!(
            "stream stage `{stage}` item {item_index} failed: {}",
            error.message
        );
        error
    }

    pub(super) fn trace_process_run_start(&mut self, span: Span, invocation: &ProcessInvocation) {
        self.trace_enter(
            TraceKind::RunStart,
            Some(span),
            Some(&String::from_utf8_lossy(&invocation.target)),
            TracePayload::RunStart {
                target: TraceArg::bytes(invocation.target.clone()),
                argv: invocation
                    .argv
                    .iter()
                    .cloned()
                    .map(TraceArg::bytes)
                    .collect(),
                cwd: TraceArg::bytes(path_bytes(&invocation.cwd)),
                env: trace_env_overlay(&invocation.env_overlay),
            },
        );
    }

    pub(super) fn trace_process_run_end(&mut self, span: Span, end: &ProcessEnd) {
        // A redirection-setup failure surfaces as a run error of kind
        // "redirection"; emit a dedicated redirection.setup leaf (mirrors the
        // deleted evaluator's trace_redirection_failure).
        if let Some(error) = end
            .error
            .as_ref()
            .filter(|error| error.kind == "redirection")
        {
            self.trace_leaf(
                TraceKind::RedirectionSetup,
                Some(span),
                None,
                TracePayload::Redirection {
                    op: "failure".to_string(),
                    target: None,
                    fd: None,
                    error: Some(TraceError::new(&error.kind, &error.message)),
                },
            );
        }
        self.trace_exit(
            TraceKind::RunEnd,
            Some(span),
            None,
            TracePayload::RunEnd {
                pid: end.pid,
                status: end.status.as_ref().map(trace_status),
                error: end
                    .error
                    .as_ref()
                    .map(|error| TraceError::new(&error.kind, &error.message)),
            },
        );
    }

    fn trace_lowered_pipeline_enter(&mut self, span: Span) {
        self.trace_enter(
            TraceKind::PipelineEnter,
            Some(span),
            Some("pipeline"),
            TracePayload::None,
        );
    }

    /// Pair with `trace_lowered_pipeline_enter`: emit a `pipeline.segment.end`
    /// leaf per segment (index/pid/status/error) then the `pipeline.exit`.
    fn trace_lowered_pipeline_end(&mut self, span: Span, end: &ProcessEnd) {
        if let Some(status) = &end.status {
            for segment in &status.segments {
                self.trace_leaf(
                    TraceKind::PipelineSegmentEnd,
                    Some(span),
                    None,
                    TracePayload::PipelineSegmentEnd {
                        index: segment.index,
                        pid: segment.pid,
                        status: Some(super::trace_segment_status(segment)),
                        error: segment.error_kind.as_ref().map(|kind| {
                            TraceError::new(
                                kind,
                                segment.error_message.as_deref().unwrap_or("exec failure"),
                            )
                        }),
                    },
                );
            }
        }
        self.trace_exit(
            TraceKind::PipelineExit,
            Some(span),
            Some("pipeline"),
            TracePayload::PipelineEnd {
                status: end.status.as_ref().map(trace_status),
                error: end
                    .error
                    .as_ref()
                    .map(|error| TraceError::new(&error.kind, &error.message)),
            },
        );
    }

    fn trace_lowered_parallel_job(
        &mut self,
        kind: TraceKind,
        stage: &'static str,
        item_index: usize,
        error: Option<TraceError>,
        span: Span,
    ) {
        self.trace_leaf(
            kind,
            Some(span),
            Some(stage),
            TracePayload::ParallelJob {
                stage: stage.to_string(),
                item_index,
                error,
            },
        );
    }

    /// Runtime `module.load(path)`: load a user module file through the compact
    /// pipeline and return its export record. Exported `pure`/`proc` definitions
    /// are installed into the qualified-function tables under the module's
    /// path-derived namespace so the returned `Value::Pure`/`Value::Proc`
    /// handles dispatch through the lowered runtime; exported `let` values are
    /// produced by running the module file as its own compact program in an
    /// isolated child evaluator. Results are cached by module key.
    pub(super) fn load_dynamic_module(
        &mut self,
        path: PathValue,
        span: Span,
    ) -> Result<RecordMap, RuntimeError> {
        use crate::loader::{
            entry_source_from_bytes, module_key, parse_load_entry_source_arena_only,
        };

        let module_path = self.host_path(&path);
        let key = module_key(&module_path);
        if let Some(cached) = self.module_value_cache.get(&key) {
            return Ok(cached.clone());
        }
        if self.active_modules.iter().any(|active| active == &key) {
            return Err(RuntimeError::new("module-cycle", "cyclic module import").with_span(span));
        }

        let display_path = module_path.to_string_lossy().into_owned();
        let bytes = std::fs::read(&module_path).map_err(|error| {
            RuntimeError::new("module-load", format!("failed to read module: {error}"))
                .with_span(span)
        })?;
        let entry_source = entry_source_from_bytes(&display_path, bytes);
        if !entry_source.diagnostics.is_empty() {
            return Err(RuntimeError::new("module-load", "failed to load module").with_span(span));
        }
        let (module_sources, parsed) =
            parse_load_entry_source_arena_only(&display_path, entry_source, Vec::new());
        if !parsed.diagnostics.is_empty() {
            return Err(
                RuntimeError::new("module-load", "loaded module failed to parse").with_span(span),
            );
        }
        validate_dynamic_module_top_level(&parsed.arena, &display_path, span)?;
        let module_source_id = module_sources
            .files()
            .first()
            .map(crate::source::SourceFile::id)
            .ok_or_else(|| {
                RuntimeError::new("module-load", "loaded module has no source").with_span(span)
            })?;
        let module_text = module_sources
            .get(module_source_id)
            .map(|source| source.text().to_string())
            .unwrap_or_default();

        let doc_diagnostics =
            crate::sema::check::Checker::check_public_module_docs(&parsed.arena, &module_text);
        if !doc_diagnostics.is_empty() {
            return Err(
                RuntimeError::new("module-load", "loaded module has undocumented exports")
                    .with_span(span),
            );
        }

        let declarations = crate::sema::check::Checker::check_compact_declarations(&parsed.arena);
        if !declarations.diagnostics.is_empty() {
            return Err(
                RuntimeError::new("module-load", "loaded module failed to check").with_span(span),
            );
        }
        let bodies =
            crate::sema::check::Checker::probe_compact_bodies(&parsed.arena, &declarations);
        if !bodies.diagnostics.is_empty() {
            return Err(
                RuntimeError::new("module-load", "loaded module body failed to check")
                    .with_span(span),
            );
        }
        let module_program = Arc::new(
            super::indexed::full::FullBuilder::build_compact(
                &parsed.arena,
                &declarations,
                &bodies,
                &module_text,
                Arc::new(module_sources.clone()),
                module_source_id,
            )
            .map_err(|error| {
                RuntimeError::new(
                    "module-load",
                    format!("loaded module could not encode `{}`", error.construct),
                )
                .with_span(span)
            })?,
        );
        let dynamic_namespace = Name::intern(format!("dynamic:{key}"));
        let exported_functions = self.module_namespace_export_functions(&parsed.arena);
        for (name, pure) in &exported_functions {
            let qualified = QualifiedName::new(dynamic_namespace, *name);
            Arc::make_mut(&mut self.indexed_dynamic_functions).insert(
                qualified,
                DynamicFunction {
                    program: Arc::clone(&module_program),
                    function: LoweredFunctionKey::Name(*name),
                    kind: if *pure {
                        LoweredFunctionKind::Pure
                    } else {
                        LoweredFunctionKind::Proc
                    },
                },
            );
        }
        for (name, pure) in &exported_functions {
            let function = crate::runtime::value::FunctionName::qualified(QualifiedName::new(
                dynamic_namespace,
                *name,
            ));
            let sig = if *pure {
                declarations.pures.get(name)
            } else {
                declarations.procs.get(name)
            };
            if let Some(sig) = sig {
                self.record_module_export_signature(function, *pure, sig);
            }
        }

        // Pass 2: run the module file (with top-level `export` stripped) as its
        // own compact program in a child evaluator to materialize exported `let`
        // values. The stripped source keeps byte offsets, so `use` imports and
        // the module's own functions still resolve, and the `let` initializers
        // execute as ordinary top-level statements.
        let harvest_text = Self::module_harvest_source(&parsed.arena, &module_text);
        let harvest_entry = crate::loader::entry_source_from_text(&display_path, harvest_text);
        let exported_let_names = self.module_namespace_export_let_names(&parsed.arena);
        let (harvest_sources, harvest_parsed) = crate::loader::parse_load_entry_source_arena_only(
            &display_path,
            harvest_entry,
            Vec::new(),
        );
        let child_exports = if harvest_parsed.diagnostics.is_empty() {
            let harvest_source_id = harvest_sources
                .files()
                .first()
                .map(crate::source::SourceFile::id)
                .unwrap_or(module_source_id);
            self.active_modules.push(key.clone());
            let mut child = Evaluator::new_with_sources(Vec::new(), harvest_sources);
            child.cwd = self.cwd.clone();
            child.env = self.env.clone();
            let child_output = child.run_module_top_level(
                &harvest_parsed.arena,
                harvest_source_id,
                &exported_let_names,
            );
            self.active_modules.retain(|active| active != &key);
            let (record, bindings) = child_output.map_err(|error| error.with_span(span))?;
            // Make the module's top-level bindings resolvable so its functions
            // (installed under the module namespace) can read the module-scope
            // values they capture when invoked.
            for (name, value) in bindings {
                if self
                    .lookup(name)
                    .is_some_and(|binding| matches!(binding.value, Value::Module(_)))
                {
                    continue;
                }
                self.define(
                    name,
                    super::Binding {
                        value,
                        mutable: false,
                    },
                );
            }
            record
        } else {
            RecordMap::new()
        };

        // Build the export record: exported `let` values from the child run,
        // plus handles for exported functions under this dynamic module's
        // private namespace so same-named exports from other loaded files do
        // not overwrite them.
        let mut record_fields = Vec::with_capacity(
            exported_let_names
                .len()
                .saturating_add(exported_functions.len()),
        );
        for name in exported_let_names {
            if let Some(value) = child_exports.get_name(name) {
                record_fields.push((name, value.clone()));
            }
        }
        for (name, pure) in exported_functions {
            let function = crate::runtime::value::FunctionName::qualified(QualifiedName::new(
                dynamic_namespace,
                name,
            ));
            let value = if pure {
                Value::Pure(function)
            } else {
                Value::Proc(function)
            };
            record_fields.push((name, value));
        }
        let record = RecordMap::from_name_values(record_fields);

        Arc::make_mut(&mut self.module_value_cache).insert(key, record.clone());
        Ok(record)
    }

    /// Enumerate the module file's top-level `export let` binding names.
    fn module_namespace_export_let_names(&self, arena: &ArenaProgram) -> Vec<Name> {
        let mut names = Vec::new();
        for stmt in arena.statement_ids() {
            let crate::syntax::arena::ArenaStmtKind::Export(inner) = arena.arena.stmt(stmt).kind
            else {
                continue;
            };
            if let crate::syntax::arena::ArenaStmtKind::Let { target, .. } =
                arena.arena.stmt(inner).kind
                && let crate::syntax::arena::ArenaBindingTargetKind::Name(name) =
                    arena.arena.binding_target(target).kind
            {
                names.push(name);
            }
        }
        names
    }

    /// Enumerate the module file's top-level `export pure`/`export proc`
    /// declarations as `(name, is_pure)` pairs.
    fn module_namespace_export_functions(&self, arena: &ArenaProgram) -> Vec<(Name, bool)> {
        let mut functions = Vec::new();
        for stmt in arena.statement_ids() {
            let crate::syntax::arena::ArenaStmtKind::Export(inner) = arena.arena.stmt(stmt).kind
            else {
                continue;
            };
            match arena.arena.stmt(inner).kind {
                crate::syntax::arena::ArenaStmtKind::PureDef(def) => {
                    functions.push((arena.arena.function_def(def).name, true));
                }
                crate::syntax::arena::ArenaStmtKind::ProcDef(def) => {
                    functions.push((arena.arena.function_def(def).name, false));
                }
                _ => {}
            }
        }
        functions
    }

    /// Build a copy of the module source where each top-level `export ` keyword
    /// is blanked to whitespace (preserving every byte offset, so the arena and
    /// spans stay valid). Top-level `export let`/`export proc`/`export pure`
    /// statements are not lowerable as script top level, so for the export-value
    /// harvest run we strip the `export` wrapper, leaving plain top-level
    /// statements the compact runner can execute.
    fn module_harvest_source(arena: &ArenaProgram, text: &str) -> String {
        const EXPORT_KEYWORD: &[u8] = b"export";
        let mut bytes = text.as_bytes().to_vec();
        for stmt in arena.statement_ids() {
            let crate::syntax::arena::ArenaStmtKind::Export(_inner) = arena.arena.stmt(stmt).kind
            else {
                continue;
            };
            // The arena's `Export` node shares the inner statement's span, which
            // starts at the `export` keyword. Blank that keyword (replacing it
            // with spaces preserves every byte offset, so the re-parsed arena's
            // spans stay valid) so the harvest run sees a plain top-level
            // statement the compact runner can execute.
            let start = arena.arena.stmt(stmt).span.range().start.min(bytes.len());
            let end = (start + EXPORT_KEYWORD.len()).min(bytes.len());
            if &text.as_bytes()[start..end] == EXPORT_KEYWORD {
                for byte in &mut bytes[start..end] {
                    *byte = b' ';
                }
            }
        }
        String::from_utf8(bytes).unwrap_or_else(|_| text.to_string())
    }

    /// Run a module file as a compact top-level program (installing + executing
    /// its top-level statements) and return the record of its exported `let`
    /// values. Used by `module.load`.
    fn run_module_top_level(
        &mut self,
        program: &ArenaProgram,
        source_id: SourceId,
        exported_let_names: &[Name],
    ) -> Result<(RecordMap, Vec<(Name, Value)>), RuntimeError> {
        let plan = self
            .prepare_compact_indexed_only(program, source_id)
            .ok_or_else(|| {
                RuntimeError::new(
                    "module-load",
                    "loaded module could not encode as indexed IR",
                )
            })?;
        let mut defers = Vec::new();
        for (index, statement) in plan.statements.iter().enumerate() {
            let span = statement.span;
            let indexed = self
                .indexed_program
                .as_ref()
                .expect("indexed module program remains installed");
            if indexed.driver_step_is_skip(index).unwrap_or(false) {
                continue;
            }
            if indexed.driver_step_is_defer(index).unwrap_or(false) {
                defers.push((index, span));
                continue;
            }
            match self.eval_indexed_driver_step(index, span).ok_or_else(|| {
                RuntimeError::new("module-load", "indexed module driver step is unavailable")
                    .with_span(span)
            })?? {
                Some(Flow::Continue(_)) | None => {}
                Some(Flow::Propagate(propagation)) => {
                    return Err(runtime_error_from_value(propagation.error, span));
                }
                Some(_) => {
                    return Err(RuntimeError::new(
                        "module-load",
                        "invalid control flow at module top level",
                    )
                    .with_span(span));
                }
            }
        }
        for (index, span) in defers.into_iter().rev() {
            let _ = self.eval_indexed_driver_step(index, span).ok_or_else(|| {
                RuntimeError::new("module-load", "indexed module defer is unavailable")
                    .with_span(span)
            })??;
        }
        // Collect exported `let` values for the export record, and the child
        // top-level bindings so the caller can make the module's functions
        // resolve captured module-scope values and imported namespaces.
        let mut record_fields = Vec::with_capacity(exported_let_names.len());
        for name in exported_let_names {
            if let Some(binding) = self.lookup(*name) {
                record_fields.push((*name, binding.value.clone()));
            }
        }
        let bindings = self
            .scopes
            .last()
            .map(|scope| {
                scope
                    .iter()
                    .map(|(name, binding)| (*name, binding.value.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Ok((RecordMap::from_name_values(record_fields), bindings))
    }

    fn eval_lowered_spawn_invocation(
        &mut self,
        invocation: ProcessInvocation,
        options: SpawnOptions,
        span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let mut managed_options = SpawnManagedOptions::inherited_process_group();
        managed_options.stdin = ManagedStdio::Inherit;
        managed_options.stdout = ManagedStdio::Inherit;
        managed_options.stderr = ManagedStdio::Inherit;
        managed_options.apply_redirections = true;
        managed_options.spawn = options;
        self.trace_spawn_start(span, &invocation, options.detach || options.new_session);
        match spawn_managed(&invocation, managed_options) {
            Ok(child) => {
                let handle = self.process_handle_value(child, span);
                self.trace_leaf(
                    TraceKind::SpawnReady,
                    Some(span),
                    None,
                    TracePayload::SpawnReady {
                        handle_id: handle.id,
                        pid: Some(handle.pid as u32),
                    },
                );
                Ok(ControlFlow::Continue(lowered_result_ok(
                    LoweredValue::ProcessHandle(Box::new(handle)),
                )))
            }
            Err(error) => Ok(ControlFlow::Continue(lowered_process_run_error(
                error.with_span(span),
            ))),
        }
    }

    fn eval_lowered_method_dispatch(
        &mut self,
        receiver: LoweredValue,
        name: &str,
        mut values: Vec<LoweredValue>,
        span: &Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        if let LoweredValue::Path(path) = &receiver
            && (name == "lines" || name == "bytes_lines")
            && values.is_empty()
        {
            let result = self.path_lines_stream(path.clone(), name == "bytes_lines", *span);
            let value = lowered_runtime_value(result, *span)?;
            return Ok(ControlFlow::Continue(value));
        }
        if let LoweredValue::Path(source) = &receiver
            && name == "copy"
            && (values.len() == 1 || values.len() == 2)
        {
            let overwrite = lowered_bool_arg_or(values.get(1).cloned(), false, "Path.copy", *span)?;
            let dest = lowered_path_arg(
                values.first().cloned().expect("checked value length"),
                "Path.copy",
                *span,
            )?;
            let value = lowered_unit_result(fs_module::copy_file(
                self.host_path(source),
                self.host_path(&dest),
                overwrite,
                *span,
            ));
            return Ok(ControlFlow::Continue(value));
        }
        if let LoweredValue::Path(source) = &receiver
            && name == "rename"
            && (values.len() == 1 || values.len() == 2)
        {
            let overwrite =
                lowered_bool_arg_or(values.get(1).cloned(), false, "Path.rename", *span)?;
            let dest = lowered_path_arg(
                values.first().cloned().expect("checked value length"),
                "Path.rename",
                *span,
            )?;
            let value = lowered_unit_result(fs_module::rename_path(
                self.host_path(source),
                self.host_path(&dest),
                overwrite,
                *span,
            ));
            return Ok(ControlFlow::Continue(value));
        }
        if let LoweredValue::Path(path) = &receiver
            && name == "remove_dir"
            && values.is_empty()
        {
            let value = lowered_unit_result(fs_module::remove_dir(self.host_path(path), *span));
            return Ok(ControlFlow::Continue(value));
        }
        if let LoweredValue::Path(path) = &receiver
            && name == "touch"
            && values.len() <= 1
        {
            let create = lowered_bool_arg_or(values.first().cloned(), true, "Path.touch", *span)?;
            let value =
                lowered_unit_result(fs_module::touch_path(self.host_path(path), create, *span));
            return Ok(ControlFlow::Continue(value));
        }
        if let LoweredValue::Path(path) = &receiver
            && name == "touch_from"
            && values.len() == 1
        {
            let reference = lowered_path_arg(
                values.first().cloned().expect("checked value length"),
                "Path.touch_from",
                *span,
            )?;
            let value = lowered_unit_result(fs_module::touch_path_from(
                self.host_path(path),
                &self.host_path(&reference),
                *span,
            ));
            return Ok(ControlFlow::Continue(value));
        }
        if let LoweredValue::Path(path) = &receiver
            && name == "truncate"
            && values.len() == 1
        {
            let size = lowered_int_arg(values.pop(), "Path.truncate", *span)?;
            let value =
                lowered_unit_result(fs_module::truncate_path(self.host_path(path), size, *span));
            return Ok(ControlFlow::Continue(value));
        }
        if let LoweredValue::Path(path) = &receiver
            && name == "chmod"
            && values.len() == 1
        {
            let mode = lowered_int_arg(values.pop(), "Path.chmod", *span)?;
            let value =
                lowered_unit_result(fs_module::chmod_path(self.host_path(path), mode, *span));
            return Ok(ControlFlow::Continue(value));
        }
        if let LoweredValue::Path(source) = &receiver
            && name == "hardlink"
            && values.len() == 1
        {
            let path = lowered_path_arg(
                values.pop().expect("checked value length"),
                "Path.hardlink",
                *span,
            )?;
            let value = lowered_unit_result(fs_module::hardlink(
                self.host_path(source),
                self.host_path(&path),
                *span,
            ));
            return Ok(ControlFlow::Continue(value));
        }
        if let LoweredValue::Path(path) = &receiver
            && name == "unlink"
            && values.is_empty()
        {
            let value = lowered_unit_result(fs_module::unlink(self.host_path(path), *span));
            return Ok(ControlFlow::Continue(value));
        }
        if let LoweredValue::ProcessHandle(handle) = &receiver
            && name == "cancel"
            && values.len() <= 2
        {
            // `cancel` has two optional args (signal: Str, kill_after:
            // Duration). A named call may skip the leading `signal`, so
            // the lowering compacts bound args and we dispatch by type
            // rather than position.
            let mut signal_name: Option<String> = None;
            let mut kill_after: Option<Duration> = None;
            for value in &values {
                match value {
                    LoweredValue::Duration(duration) => {
                        kill_after = Some(Duration::from_millis(duration.millis));
                    }
                    _ => {
                        signal_name = Some(lowered_str_arg_owned(
                            Some(value.clone()),
                            "TERM",
                            "cancel",
                            *span,
                        )?);
                    }
                }
            }
            let signal_name = signal_name.unwrap_or_else(|| "TERM".to_string());
            let signal = match process_module::signal_info(&signal_name, *span) {
                Ok(signal) => signal,
                Err(error) => {
                    return Ok(ControlFlow::Continue(lowered_result_err_value(error)));
                }
            };
            let kill_after = kill_after.unwrap_or_else(|| Duration::from_secs(2));
            let Some(live) = self.process_handles.remove(&handle.id) else {
                let error = super::process_handle::invalid_process_handle_error(handle.id, *span);
                self.trace_spawn_cancel(
                    *span,
                    handle.id,
                    None,
                    &signal.name,
                    kill_after,
                    Some(&error),
                );
                return Ok(ControlFlow::Continue(lowered_result_err_value(
                    run_error_to_runtime(error, *span),
                )));
            };
            let pid = Some(live.child.pid);
            let value = match cancel_managed(live.child, signal.number, kill_after) {
                Ok(_) => {
                    self.trace_spawn_cancel(*span, handle.id, pid, &signal.name, kill_after, None);
                    lowered_result_ok(LoweredValue::Unit)
                }
                Err(error) => {
                    let error = error.with_span(*span);
                    self.trace_spawn_cancel(
                        *span,
                        handle.id,
                        pid,
                        &signal.name,
                        kill_after,
                        Some(&error),
                    );
                    lowered_result_err_value(run_error_to_runtime(error, *span))
                }
            };
            return Ok(ControlFlow::Continue(value));
        }
        if name == "call"
            && let Some((function, pure)) = match &receiver {
                LoweredValue::Pure(function) => Some((*function, true)),
                LoweredValue::Proc(function) => Some((*function, false)),
                _ => None,
            }
        {
            let args = values
                .into_iter()
                .map(LoweredValue::into_value)
                .collect::<Vec<_>>();
            let result = self
                .call_lowered_function_value_with_values(function, pure, &args, *span)
                .ok_or_else(|| {
                    RuntimeError::new(
                        "unresolved-call",
                        format!(
                            "method call target {} could not be lowered",
                            function.display_name()
                        ),
                    )
                    .with_span(*span)
                })??;
            let value = lowered_value_from_runtime_any(&result).ok_or_else(|| {
                RuntimeError::new(
                    "type-error",
                    format!("method call returned unsupported {}", result.type_name()),
                )
                .with_span(*span)
            })?;
            return Ok(ControlFlow::Continue(value));
        }
        match receiver {
            LoweredValue::Stream(stream) if name == "collect" && values.is_empty() => {
                let values = self.collect_stream_values(*stream, *span)?;
                let value = lowered_runtime_value(Value::List(values), *span)?;
                Ok(ControlFlow::Continue(value))
            }
            receiver => {
                lowered_method_value(receiver, name, values, *span).map(ControlFlow::Continue)
            }
        }
    }

    fn eval_lowered_projected_reduce_by_item(
        &mut self,
        state: &mut LoweredProjectedReduceState<'_>,
        item: LoweredValue,
        groups: &mut BTreeMap<String, LoweredValue>,
        span: Span,
    ) -> Result<(), RuntimeError> {
        let indices = state.record_indices_for(&item);
        let key = lowered_projected_key_value(&state.projection, &item, indices.as_ref(), span)?;
        let key = lowered_reduce_key_value(&key, span)?;
        match groups.entry(key) {
            std::collections::btree_map::Entry::Vacant(slot) => {
                slot.insert(LoweredValue::RecordVec(lowered_projected_record_value(
                    &state.projection,
                    &item,
                    indices.as_ref(),
                    span,
                )?));
            }
            std::collections::btree_map::Entry::Occupied(mut slot) => {
                if let LoweredValue::RecordVec(acc) = slot.get_mut() {
                    if state.output_fields_unique
                        && lowered_projected_acc_layout_matches(&state.projection, acc)
                    {
                        for (index, (_, source_field)) in
                            state.projection.value_fields.iter().enumerate()
                        {
                            let field_value =
                                if let (LoweredValue::RecordVec(record), Some(indices)) =
                                    (&item, indices.as_ref())
                                {
                                    record[indices.values[index]].1.clone()
                                } else {
                                    lowered_record_field_value(&item, source_field).ok_or_else(
                                        || {
                                            RuntimeError::new("missing-field", *source_field)
                                                .with_span(span)
                                        },
                                    )?
                                };
                            let acc_value = &mut acc[index].1;
                            *acc_value = lowered_sum_values(
                                std::mem::replace(acc_value, LoweredValue::Unit),
                                field_value,
                            );
                        }
                    } else {
                        for (name, source_field) in &state.projection.value_fields {
                            let field_value = lowered_record_field_value(&item, source_field)
                                .ok_or_else(|| {
                                    RuntimeError::new("missing-field", *source_field)
                                        .with_span(span)
                                })?;
                            if let Some(acc_value) = lowered_record_vec_get_mut(acc, &name.as_str())
                            {
                                *acc_value = lowered_sum_values(
                                    std::mem::replace(acc_value, LoweredValue::Unit),
                                    field_value,
                                );
                            } else {
                                lowered_record_vec_insert(acc, *name, field_value);
                            }
                        }
                    }
                } else {
                    let prev = std::mem::replace(slot.get_mut(), LoweredValue::Unit);
                    *slot.get_mut() = lowered_reduce_combine(
                        ReduceByOp::Sum,
                        prev,
                        LoweredValue::RecordVec(lowered_projected_record_value(
                            &state.projection,
                            &item,
                            indices.as_ref(),
                            span,
                        )?),
                        span,
                    )?;
                }
            }
        }
        Ok(())
    }
}
