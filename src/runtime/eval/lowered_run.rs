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
    resolve_executable, run_inherit_with_policy, run_pipeline_inherit_with_policy,
    run_quiet_with_policy, spawn_command, spawn_managed,
};
use crate::runtime::run::execute_run_with_policy;
use crate::runtime::value::{
    CommandPlan, DurationValue, FunctionName, LiveStream, PathValue, ProcessHandleValue, RecordMap,
    RegexValue, RunError, RuntimeError, StreamValue, Value, error_constructor,
    structured_error_constructor,
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
use rustc_hash::FxHashMap;
use std::collections::BTreeMap;
#[cfg(target_os = "macos")]
use std::ffi::CStr;
use std::ffi::OsString;
use std::io::{Read, Write};
use std::ops::ControlFlow;
use std::os::fd::AsRawFd;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::lower::{
    lowered_empty_string_literal, lowered_literal_value, lowered_match_no_arm,
    lowered_needle_bytes, lowered_pattern_matches, lowered_record_field, lowered_stmt_flow_to_flow,
    lowered_str_key, lowered_sum_records, lowered_tag_key, lowered_trim_slot,
};
use super::lowered_ops::{
    compare_lowered_sort_keys, lowered_assign_value, lowered_binary_value, lowered_bool_value,
    lowered_bytes_arg, lowered_bytes_parts, lowered_bytes_value, lowered_contains_value,
    lowered_index_ref, lowered_index_value, lowered_method_ref, lowered_method_value,
    lowered_nonnegative_count, lowered_path_method_value, lowered_return_value,
    lowered_slice_value, lowered_str_arg, lowered_str_byte_at_value, lowered_str_byte_len_value,
    lowered_str_count_lines_value, lowered_str_parts, lowered_str_predicate_text,
    lowered_str_predicate_value, lowered_str_value, lowered_trim_is_empty_value,
    lowered_trim_str_predicate_value, lowered_type_name, lowered_value_from_runtime,
    lowered_value_from_runtime_any, lowered_value_matches, push_lowered_display,
};
use super::modules::{
    auth as auth_module, display_spawn_argv, intercept_test_host_call, record_int_field,
    record_path, record_str, run_error_to_runtime, test_contains_value, test_error_kind,
    test_failure, test_mock_expected_return_type, test_temp_path, test_value_matches_type,
    utils_cache_key, validate_module_contract,
};
use super::{
    Binding, Evaluator, Flow, FsRootHandle, LowerableFunctions, LoweredBoolExpr, LoweredCallArg,
    LoweredCompTarget, LoweredErrorExpr, LoweredExpr, LoweredFmtPart, LoweredFunctionKey,
    LoweredIntExpr, LoweredModuleExportKind, LoweredPipelineStage,
    LoweredProcessCommandBuilderEntry, LoweredPureFunction, LoweredRecordEntry, LoweredReturnKind,
    LoweredRunArg, LoweredRunArgKind, LoweredRunEnv, LoweredRunPipelineSegment,
    LoweredRunRedirection, LoweredStmt, LoweredStmtFlow, LoweredStrView, LoweredTopLevelKind,
    LoweredTopLevelStmt, LoweredType, LoweredValue, Name, ReduceByOp, TestMock,
    assign_lowered_bytes_view, assign_lowered_str_view, bytes_contains, check_env_name,
    compound_assignment_value, display_value, exit_status, lowered_value_matches_static_type,
    module_error, module_io_error, path_absolute_value, path_value_from_pathbuf,
    pathbuf_from_path_value, runtime_error_from_value, splice_to_argv, trace_env_overlay,
    trace_status, value_matches_static_type, value_to_argv_bytes,
};
use cap_directories::{ProjectDirs, UserDirs, ambient_authority as directories_authority};
use cap_tempfile::{TempDir, TempFile, ambient_authority as tempfile_authority};

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
        LoweredValue::Map(entries) => Some(entries.len()),
        _ => None,
    }
}

/// Stringify a `reduce-by` block's `key` field (Str/Int/Bool), matching the
/// keys produced by `count { key }` and the old recursive evaluator.
fn lowered_reduce_by_key(output: &LoweredValue, span: Span) -> Result<String, RuntimeError> {
    let key = lowered_record_field(output, "key").ok_or_else(|| {
        RuntimeError::new("reduce-by-key", "reduce-by record is missing field `key`")
            .with_span(span)
    })?;
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
        (acc @ LoweredValue::Record(_), value @ LoweredValue::Record(_)) => {
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
    tv.tv_sec
        .saturating_mul(1_000_000_000)
        .saturating_add(i64::from(tv.tv_usec).saturating_mul(1_000))
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
        (Arc::from("status"), LoweredValue::Status(status)),
        (
            Arc::from("duration_ms"),
            LoweredValue::Int(wall_ns / 1_000_000),
        ),
        (Arc::from("wall_ns"), LoweredValue::Int(wall_ns)),
        (Arc::from("user_ns"), LoweredValue::Int(user_ns)),
        (Arc::from("system_ns"), LoweredValue::Int(system_ns)),
    ]))
}

fn read_host_path_bytes(path: &Path, span: Span) -> Result<Vec<u8>, RuntimeError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .ok_or_else(|| RuntimeError::new("fs-read", "path has no file name").with_span(span))?;
    let dir = cap_std::fs::Dir::open_ambient_dir(parent, cap_std::ambient_authority())
        .map_err(|error| RuntimeError::new("fs-read", error.to_string()).with_span(span))?;
    let mut file = dir
        .open(Path::new(name))
        .map_err(|error| RuntimeError::new("fs-read", error.to_string()).with_span(span))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|error| RuntimeError::new("fs-read", error.to_string()).with_span(span))?;
    Ok(bytes)
}

fn read_host_path_string(path: &Path, operation: &str, span: Span) -> Result<String, RuntimeError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .ok_or_else(|| RuntimeError::new(operation, "path has no file name").with_span(span))?;
    let dir = cap_std::fs::Dir::open_ambient_dir(parent, cap_std::ambient_authority())
        .map_err(|error| RuntimeError::new(operation, error.to_string()).with_span(span))?;
    let mut file = dir
        .open(Path::new(name))
        .map_err(|error| RuntimeError::new(operation, error.to_string()).with_span(span))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|error| RuntimeError::new(operation, error.to_string()).with_span(span))?;
    String::from_utf8(bytes)
        .map_err(|error| RuntimeError::new(operation, error.to_string()).with_span(span))
}

fn create_host_dir_all(path: &Path, operation: &str, span: Span) -> Result<(), RuntimeError> {
    let (root, rel) = if path.is_absolute() {
        (Path::new("/"), path.strip_prefix("/").unwrap_or(path))
    } else {
        (Path::new("."), path)
    };
    let dir = cap_std::fs::Dir::open_ambient_dir(root, cap_std::ambient_authority())
        .map_err(|error| RuntimeError::new(operation, error.to_string()).with_span(span))?;
    dir.create_dir_all(rel)
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
        let Some(value) = module.get(name.as_str()) else {
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
    let Some(LoweredValue::List(items)) = value else {
        return Err(
            RuntimeError::new("type-error", format!("{operation} expected List[Str]"))
                .with_span(span),
        );
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
    let Some(LoweredValue::List(items)) = value else {
        return Err(
            RuntimeError::new("type-error", format!("{operation} expected List[Path]"))
                .with_span(span),
        );
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
    let Some(LoweredValue::List(items)) = value else {
        return Err(
            RuntimeError::new("type-error", format!("{operation} expected List[Int]"))
                .with_span(span),
        );
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
    let Some(LoweredValue::List(items)) = value else {
        return Err(
            RuntimeError::new("type-error", format!("{operation} expected List[Bytes]"))
                .with_span(span),
        );
    };
    let mut chunks = Vec::with_capacity(items.len());
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
        Some(LoweredValue::Record(fields) | LoweredValue::Module(fields)) => {
            let mut record = RecordMap::new();
            for (key, value) in fields {
                record.insert(key, value.into_value());
            }
            Ok(record)
        }
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
    let LoweredValue::Record(fields) = value else {
        return Err(
            RuntimeError::new("type-error", format!("{operation} expected Record")).with_span(span),
        );
    };
    let mut env = BTreeMap::new();
    for (name, value) in fields {
        let mut text = String::new();
        push_lowered_display(&mut text, &value, span)?;
        env.insert(name.to_string(), text);
    }
    Ok(env)
}

fn lowered_command_arg(
    value: LoweredValue,
    operation: &str,
    span: Span,
) -> Result<CommandPlan, RuntimeError> {
    match value {
        LoweredValue::Command(plan) => Ok(plan),
        other => Err(RuntimeError::new(
            "type-error",
            format!("{operation} expected Command, found {}", other.type_name()),
        )
        .with_span(span)),
    }
}

fn lowered_command_plan_value(
    target: LoweredValue,
    argv: LoweredValue,
    cwd: Option<LoweredValue>,
    env: Option<LoweredValue>,
    timeout: Option<LoweredValue>,
    detach: Option<LoweredValue>,
    new_session: Option<LoweredValue>,
    ignore_hup: Option<LoweredValue>,
    cpu_max: Option<LoweredValue>,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    let target = lowered_command_target_bytes(target, span)?;
    let argv_words = lowered_str_list_arg(Some(argv), "process.command_argv", span)?;
    if argv_words.is_empty() {
        return Err(RuntimeError::new("argv-empty", "argv must contain argv[0]").with_span(span));
    }
    for word in &argv_words {
        if word.contains('\0') {
            return Err(
                RuntimeError::new("nul-argv", "argv items cannot contain NUL bytes")
                    .with_span(span),
            );
        }
    }
    let mut argv = Vec::with_capacity(argv_words.len().saturating_sub(1));
    for word in argv_words.into_iter().skip(1) {
        argv.push(word.into_bytes());
    }
    let cwd = cwd
        .map(|value| lowered_path_like_arg(value, "process.command_argv", span))
        .transpose()?;
    let env = env
        .map(|value| lowered_env_record_arg(value, "process.command_argv", span))
        .transpose()?
        .unwrap_or_default();
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

    Ok(LoweredValue::Command(CommandPlan {
        target,
        argv,
        cwd,
        env,
        timeout,
        cpu_max,
        detach,
        new_session,
        ignore_hup,
    }))
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
    lowered_runtime_value(
        match result {
            Ok(values) => Value::ok(Value::List(values)),
            Err(error) => Value::err(Value::Error(Box::new(error))),
        },
        span,
    )
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
        (Arc::from("status"), LoweredValue::Status(status)),
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
    let LoweredValue::List(items) = value else {
        return Err(
            RuntimeError::new("type-error", format!("{operation} expected List[Path]"))
                .with_span(span),
        );
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
) -> Result<&'a cap_std::fs::Dir, RuntimeError> {
    let id = lowered_root_id(root, span)?;
    let Some(slot) = id
        .checked_sub(1)
        .and_then(|index| usize::try_from(index).ok())
        .and_then(|index| roots.get(index))
    else {
        return Err(RuntimeError::new("fs-root", "root handle is not active").with_span(span));
    };
    slot.as_ref()
        .map(FsRootHandle::dir)
        .ok_or_else(|| RuntimeError::new("fs-root", "root handle is not active").with_span(span))
}

fn read_link_path(path: &Path) -> std::io::Result<PathBuf> {
    rustix::fs::readlink(path, Vec::new())
        .map(|target| PathBuf::from(std::ffi::OsString::from_vec(target.as_bytes().to_vec())))
        .map_err(std::io::Error::from)
}

fn root_path_from_dir(dir: &cap_std::fs::Dir, span: Span) -> Result<PathValue, RuntimeError> {
    let fd = dir.as_raw_fd();
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

fn lowered_rest_index(lowered: &LoweredPureFunction) -> Option<usize> {
    lowered.param_rest.iter().position(|rest| *rest)
}

fn lowered_required_arg_count(lowered: &LoweredPureFunction) -> usize {
    let limit = lowered_rest_index(lowered).unwrap_or(lowered.params.len());
    lowered
        .param_defaults
        .iter()
        .take(limit)
        .filter(|default| default.is_none())
        .count()
}

fn lowered_call_arity_message(lowered: &LoweredPureFunction, actual: usize) -> String {
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
            let LoweredValue::Record(record) = value else {
                return Err(RuntimeError::new(
                    "type-error",
                    "record destructuring requires a record value",
                )
                .with_span(span));
            };
            for (name, slot, field_span) in fields {
                let value = record.get(name).cloned().ok_or_else(|| {
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

fn lowered_param_check(
    lowered: &LoweredPureFunction,
    index: usize,
) -> Option<&super::LoweredTypeCheck> {
    lowered.param_checks.get(index).and_then(Option::as_ref)
}

fn lowered_runtime_arg_matches_param(
    lowered: &LoweredPureFunction,
    index: usize,
    value: &Value,
) -> bool {
    lowered_param_check(lowered, index)
        .is_none_or(|check| value_matches_static_type(value, &check.ty))
}

fn lowered_value_matches_param(
    lowered: &LoweredPureFunction,
    index: usize,
    kind: LoweredType,
    value: &LoweredValue,
) -> bool {
    lowered_value_matches(kind, value)
        && lowered_param_check(lowered, index)
            .is_none_or(|check| lowered_value_matches_static_type(value, &check.ty))
}

fn lowered_param_type_name(lowered: &LoweredPureFunction, index: usize, kind: LoweredType) -> &str {
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
        stream: StreamValue,
        span: Span,
    ) -> Result<Vec<LoweredValue>, RuntimeError> {
        let values = self.collect_stream_values(stream, span)?;
        let mut lowered = Vec::with_capacity(values.len());
        for value in values {
            let Some(value) = lowered_value_from_runtime_any(&value) else {
                return Err(RuntimeError::new(
                    "type-error",
                    format!("stream produced unsupported {}", value.type_name()),
                )
                .with_span(span));
            };
            lowered.push(value);
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
            LoweredValue::Stream(stream) => self.collect_lowered_stream_values(stream, span),
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

    fn push_lowered_fs_root(&mut self, root: FsRootHandle) -> LoweredValue {
        let id = self.fs_roots.len() as i64 + 1;
        self.fs_roots.push(Some(root));
        fs_root_record(id)
    }

    fn lowered_create_temp_file_root(&mut self, span: Span) -> Result<LoweredValue, RuntimeError> {
        let dir = TempDir::new(tempfile_authority()).map_err(|error| {
            RuntimeError::new("fs-temp-file", error.to_string()).with_span(span)
        })?;
        let mut file = TempFile::new(&dir).map_err(|error| {
            RuntimeError::new("fs-temp-file", error.to_string()).with_span(span)
        })?;
        file.flush().map_err(|error| {
            RuntimeError::new("fs-temp-file", error.to_string()).with_span(span)
        })?;
        file.replace("file").map_err(|error| {
            RuntimeError::new("fs-temp-file", error.to_string()).with_span(span)
        })?;
        let root = self.push_lowered_fs_root(FsRootHandle::TempDir(dir));
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
        let dirs = ProjectDirs::from(
            qualifier,
            organization,
            application,
            directories_authority(),
        )
        .ok_or_else(|| {
            RuntimeError::new("fs-dir", "project directories are unavailable").with_span(span)
        })?;
        let dir = match kind {
            "cache" => dirs.cache_dir(),
            "config" => dirs.config_dir(),
            "data" => dirs.data_dir(),
            "data_local" => dirs.data_local_dir(),
            "runtime" => dirs.runtime_dir(),
            "state" => dirs.state_dir().and_then(|dir| {
                dir.ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "state directory is unavailable",
                    )
                })
            }),
            _ => {
                return Err(RuntimeError::new(
                    "fs-dir",
                    format!("unknown project directory kind `{kind}`"),
                )
                .with_span(span));
            }
        }
        .map_err(|error| RuntimeError::new("fs-dir", error.to_string()).with_span(span))?;
        Ok(self.push_lowered_fs_root(FsRootHandle::Dir(dir)))
    }

    fn lowered_user_root(&mut self, kind: &str, span: Span) -> Result<LoweredValue, RuntimeError> {
        let dirs = UserDirs::new().ok_or_else(|| {
            RuntimeError::new("fs-dir", "user directories are unavailable").with_span(span)
        })?;
        let authority = directories_authority();
        let dir = match kind {
            "home" => dirs.home_dir(authority),
            "audio" => dirs.audio_dir(authority),
            "desktop" => dirs.desktop_dir(authority),
            "documents" => dirs.document_dir(authority),
            "downloads" => dirs.download_dir(authority),
            "fonts" => dirs.font_dir(authority),
            "pictures" => dirs.picture_dir(authority),
            "public" => dirs.public_dir(authority),
            "templates" => dirs.template_dir(authority),
            "videos" => dirs.video_dir(authority),
            _ => {
                return Err(RuntimeError::new(
                    "fs-dir",
                    format!("unknown user directory kind `{kind}`"),
                )
                .with_span(span));
            }
        }
        .map_err(|error| RuntimeError::new("fs-dir", error.to_string()).with_span(span))?;
        Ok(self.push_lowered_fs_root(FsRootHandle::Dir(dir)))
    }

    fn eval_lowered_module_call(
        &mut self,
        lowered: &LoweredPureFunction,
        op: RuntimeOp,
        args: &[LoweredExpr],
        slots: &mut [LoweredValue],
        span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let mut values = Vec::with_capacity(args.len());
        for arg in args {
            values.push(match self.eval_lowered_expr(lowered, arg, slots, span)? {
                ControlFlow::Continue(value) => value,
                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
            });
        }
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
                lowered_runtime_list_result(
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
                lowered_runtime_list_result(
                    archive_module::tar_list(self.host_path(&path), &compression, members, span),
                    span,
                )?
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
                lowered_runtime_list_result(
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
            RuntimeOp::FsMounts if values.is_empty() => match fs_module::mounts(span) {
                Ok(mounts) => {
                    let mut items = Vec::with_capacity(mounts.len());
                    for mount in mounts {
                        items.push(lowered_fs_mount_record(mount)?);
                    }
                    lowered_result_ok(LoweredValue::List(items))
                }
                Err(error) => lowered_result_err_value(error),
            },
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
                match read_host_path_bytes(&self.host_path(&path), span) {
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
                let overwrite =
                    lowered_bool_arg_or(values.get(4).cloned(), false, "fs.root_symlink", span)?;
                let parents =
                    lowered_bool_arg_or(values.get(3).cloned(), true, "fs.root_symlink", span)?;
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
            RuntimeOp::FsTempDir if values.is_empty() => match TempDir::new(tempfile_authority()) {
                Ok(dir) => lowered_result_ok(self.push_lowered_fs_root(FsRootHandle::TempDir(dir))),
                Err(error) => lowered_result_err_value(
                    RuntimeError::new("fs-temp-dir", error.to_string()).with_span(span),
                ),
            },
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
                        LoweredValue::Digest(hash_module::digest_bytes(algorithm, &bytes))
                    }
                    LoweredValue::BytesView(bytes) => {
                        LoweredValue::Digest(hash_module::digest_bytes(algorithm, bytes.as_slice()))
                    }
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
            RuntimeOp::TimeFormat if values.len() == 2 || values.len() == 3 => {
                let utc = lowered_bool_arg_or(values.get(2).cloned(), false, "time.format", span)?;
                let format =
                    lowered_str_arg_owned(values.get(1).cloned(), "", "time.format", span)?;
                let epoch_ms = lowered_int_arg(values.first().cloned(), "time.format", span)?;
                match time_module::format_epoch_ms(epoch_ms, &format, utc, span) {
                    Ok(formatted) => lowered_result_ok(LoweredValue::Str(formatted.into())),
                    Err(error) => lowered_result_err_value(error),
                }
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
                match json_module::encode_json(&values[0].clone().into_value(), pretty, span) {
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
                    lowered_result_ok(LoweredValue::List(vec![LoweredValue::Record(
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
                    lowered_result_ok(LoweredValue::List(vec![LoweredValue::Record(
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
                lowered_runtime_list_result(process_module::list_processes(span), span)?
            }
            RuntimeOp::ProcessThreads if values.is_empty() || values.len() == 1 => {
                let pid = match values.pop() {
                    Some(value) => Some(lowered_int_arg(Some(value), "process.threads", span)?),
                    None => None,
                };
                lowered_runtime_list_result(process_module::list_threads(pid, span), span)?
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
                lowered_runtime_list_result(process_module::port_processes(port, span), span)?
            }
            RuntimeOp::ProcessPorts if values.is_empty() => {
                lowered_runtime_list_result(process_module::listening_port_processes(span), span)?
            }
            RuntimeOp::ProcessPortsForPid if values.len() == 1 => {
                let pid = lowered_int_arg(values.pop(), "process.ports", span)?;
                lowered_runtime_list_result(process_module::pid_port_processes(pid, span), span)?
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
                match run_inherit_with_policy(&invocation, self) {
                    Ok(end) => {
                        let status = end.status.clone().expect("completed process has status");
                        self.last_status = Some(status.clone());
                        self.trace_process_run_end(span, &end);
                        lowered_result_ok(LoweredValue::Status(status))
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
                            lowered_result_ok(LoweredValue::Status(status))
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
                    Ok(regex) => lowered_result_ok(LoweredValue::Regex(RegexValue {
                        pattern: text,
                        regex: Arc::new(regex),
                    })),
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
            RuntimeOp::TestFail if values.is_empty() || values.len() == 1 => {
                let message =
                    lowered_str_arg_owned(values.pop(), "test failed", "test.fail", span)?;
                lowered_runtime_value(test_failure(message), span)?
            }
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
            RuntimeOp::TestTempPath if values.len() == 1 || values.len() == 2 => {
                let ctx = lowered_record_arg(values.first().cloned(), "test.temp_path", span)?;
                let name =
                    lowered_str_arg_owned(values.get(1).cloned(), "", "test.temp_path", span)?;
                LoweredValue::Path(test_temp_path(self, &ctx, &name, span)?)
            }
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
                    let result = self
                        .call_lowered_function_value_with_values(function, pure, &call_args, span)
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
                    lowered_result_err_value(
                        RuntimeError::new(
                            "linux-unimplemented",
                            "linux.write_device is not implemented for real mode",
                        )
                        .with_span(span),
                    )
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
                    lowered_result_err_value(
                        RuntimeError::new(
                            "linux-unimplemented",
                            "linux.read_device is not implemented for real mode",
                        )
                        .with_span(span),
                    )
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
        // Real-mode linux boot primitives are not ported to the lowered runtime
        // (they require a live Linux host); preserve the prior stub behavior.
        if self.linux_real() && !self.linux_dry_run() {
            return Ok(Value::ok(Value::Unit));
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
                Ok(Value::ok(Value::List(vec![Value::Record(
                    RecordMap::from([
                        (Arc::from("name"), Value::Str("xsh_demo".into())),
                        (Arc::from("size"), Value::Int(4096)),
                        (
                            Arc::from("used_by"),
                            Value::List(vec![Value::Str("xsh_dep".into())]),
                        ),
                    ]),
                )])))
            }
            RuntimeOp::LinuxDmesg => {
                self.linux_dry_run_log("dmesg", &[], span)?;
                Ok(Value::ok(Value::List(vec![Value::Str(
                    "xsh dry-run kernel message".into(),
                )])))
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
                Ok(Value::ok(Value::List(vec![Value::Record(
                    RecordMap::from([
                        (Arc::from("device"), Value::Str("rootfs".into())),
                        (Arc::from("mount"), Value::Str(mount.into())),
                        (Arc::from("fstype"), Value::Str("tmpfs".into())),
                        (Arc::from("total"), Value::Int(1024 * 1024 * 1024)),
                        (Arc::from("used"), Value::Int(256 * 1024 * 1024)),
                        (Arc::from("available"), Value::Int(768 * 1024 * 1024)),
                    ]),
                )])))
            }
            RuntimeOp::LinuxBlockDevices => {
                self.linux_dry_run_log("block_devices", &[], span)?;
                Ok(Value::ok(Value::List(vec![
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
                ])))
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
                Ok(Value::ok(Value::List(vec![Value::Record(
                    RecordMap::from([
                        (Arc::from("id"), Value::Int(0)),
                        (Arc::from("name"), Value::Str("phy0".into())),
                        (Arc::from("type"), Value::Str("wlan".into())),
                        (Arc::from("soft_blocked"), Value::Bool(false)),
                        (Arc::from("hard_blocked"), Value::Bool(false)),
                    ]),
                )])))
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
                Ok(Value::ok(Value::List(vec![Value::Record(
                    RecordMap::from([
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
                    ]),
                )])))
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
                Ok(Value::ok(Value::List(vec![Value::Record(
                    RecordMap::from([
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
                    ]),
                )])))
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
        lowered: &LoweredPureFunction,
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

    fn try_bind_lowered_values(
        &mut self,
        lowered: &LoweredPureFunction,
        args: &[LoweredValue],
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
                    let value = LoweredValue::List(args.get(index..).unwrap_or(&[]).to_vec());
                    if !lowered_value_matches_param(lowered, index, LoweredType::List, &value) {
                        return None;
                    }
                    values.push(value);
                    break;
                }
                match args.get(index) {
                    Some(value) => {
                        if !lowered_value_matches_param(lowered, index, kind, value) {
                            return None;
                        }
                        values.push(value.clone());
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
                        if !lowered_value_matches_param(lowered, index, kind, value) {
                            return None;
                        }
                        values.push(value.clone());
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
        lowered: &LoweredPureFunction,
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
        lowered: &LoweredPureFunction,
        values: Vec<LoweredValue>,
    ) -> Vec<LoweredValue> {
        let mut slots = self.take_lowered_slots(lowered.slot_count);
        for (slot, value) in values.into_iter().enumerate() {
            slots[slot] = value;
        }
        slots
    }

    pub(super) fn call_lowered_pure(
        &mut self,
        function: Name,
        args: &[Value],
        call_span: Span,
    ) -> Option<Result<Value, RuntimeError>> {
        if self.function_modules.contains_key(&function) {
            return None;
        }
        let lowered = self.lowered_pures.get(&function)?.clone();
        let mut slots = self.try_bind_lowered_runtime_args(&lowered, args)?;
        let result = self
            .eval_lowered_call_frame(
                TracebackFrameKind::Pure,
                function.to_string(),
                &lowered,
                &mut slots,
                call_span,
            )
            .and_then(|value| lowered_return_value(lowered.return_kind, value, call_span))
            .map(LoweredValue::into_value);
        self.recycle_lowered_slots(slots);
        Some(result)
    }

    pub(super) fn call_lowered_qualified_pure(
        &mut self,
        function: QualifiedName,
        args: &[Value],
        call_span: Span,
    ) -> Option<Result<Value, RuntimeError>> {
        if self.qualified_function_modules.contains_key(&function) {
            return None;
        }
        let lowered = self.lowered_qualified_pures.get(&function)?.clone();
        let mut slots = self.try_bind_lowered_runtime_args(&lowered, args)?;
        let result = self
            .eval_lowered_call_frame(
                TracebackFrameKind::Pure,
                function.to_string(),
                &lowered,
                &mut slots,
                call_span,
            )
            .and_then(|value| lowered_return_value(lowered.return_kind, value, call_span))
            .map(LoweredValue::into_value);
        self.recycle_lowered_slots(slots);
        Some(result)
    }

    pub(super) fn call_lowered_proc(
        &mut self,
        function: Name,
        args: &[Value],
        call_span: Span,
    ) -> Option<Result<Value, RuntimeError>> {
        if self.function_modules.contains_key(&function) {
            return None;
        }
        let lowered = self.lowered_procs.get(&function)?.clone();
        let mut slots = self.try_bind_lowered_runtime_args(&lowered, args)?;
        let result = self
            .eval_lowered_call_frame(
                TracebackFrameKind::Proc,
                function.to_string(),
                &lowered,
                &mut slots,
                call_span,
            )
            .and_then(|value| lowered_return_value(lowered.return_kind, value, call_span))
            .map(LoweredValue::into_value);
        self.recycle_lowered_slots(slots);
        Some(result)
    }

    fn call_lowered_function_value_with_values(
        &mut self,
        function: FunctionName,
        pure: bool,
        args: &[Value],
        call_span: Span,
    ) -> Option<Result<Value, RuntimeError>> {
        if let Some(function) = function.as_name() {
            return if pure {
                self.call_lowered_pure(function, args, call_span)
            } else {
                self.call_lowered_proc(function, args, call_span)
            };
        }
        let function = function.as_qualified()?;
        if pure {
            return self.call_lowered_qualified_pure(function, args, call_span);
        }
        if self.qualified_function_modules.contains_key(&function) {
            return None;
        }
        let lowered = self.lowered_qualified_procs.get(&function)?.clone();
        let mut slots = self.try_bind_lowered_runtime_args(&lowered, args)?;
        let result = self
            .eval_lowered_call_frame(
                TracebackFrameKind::Proc,
                function.to_string(),
                &lowered,
                &mut slots,
                call_span,
            )
            .and_then(|value| lowered_return_value(lowered.return_kind, value, call_span))
            .map(LoweredValue::into_value);
        self.recycle_lowered_slots(slots);
        Some(result)
    }

    pub(super) fn eval_lowered_function(
        &mut self,
        lowered: &LoweredPureFunction,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        self.hydrate_lowered_captures(lowered, slots, call_span)?;
        if matches!(
            lowered.return_kind,
            LoweredReturnKind::Plain(LoweredType::Stream)
        ) {
            let previous_items = std::mem::take(&mut self.stream_items);
            let result = self.eval_lowered_stmts(lowered, &lowered.body, slots, call_span);
            let write_back = self.write_back_lowered_captures(lowered, slots, call_span);
            let items = std::mem::take(&mut self.stream_items);
            self.stream_items = previous_items;
            let flow = result?;
            write_back?;
            return match flow {
                LoweredStmtFlow::None => Ok(LoweredValue::Stream(StreamValue::from_values(items))),
                LoweredStmtFlow::Return(value) if matches!(value, LoweredValue::Stream(_)) => {
                    Ok(value)
                }
                LoweredStmtFlow::Return(value) => Err(RuntimeError::new(
                    "type-error",
                    format!("stream producer returned {}", value.type_name()),
                )
                .with_span(call_span)),
                LoweredStmtFlow::Propagate(value) => Ok(value),
                LoweredStmtFlow::Break(_) => {
                    Err(RuntimeError::new("control-flow", "break outside loop")
                        .with_span(call_span))
                }
                LoweredStmtFlow::Continue => {
                    Err(RuntimeError::new("control-flow", "continue outside loop")
                        .with_span(call_span))
                }
            };
        }
        if let LoweredReturnKind::Plain(LoweredType::Int | LoweredType::Bool) = lowered.return_kind
            && let Some(value) = self.eval_lowered_fast_plain_return(lowered, slots)?
        {
            return Ok(value);
        }
        if let Some(value) = self.eval_lowered_fast_return(lowered, slots, call_span)? {
            return Ok(value);
        }
        let result = self.eval_lowered_stmts(lowered, &lowered.body, slots, call_span);
        // Persist mutations to captured mutable top-level (global) bindings on
        // every exit path, including errors — the side effects happened before
        // the proc returned/propagated/failed and must be visible to the caller.
        let write_back = self.write_back_lowered_captures(lowered, slots, call_span);
        let flow = result?;
        write_back?;
        match flow {
            LoweredStmtFlow::Return(value) | LoweredStmtFlow::Propagate(value) => Ok(value),
            LoweredStmtFlow::None => Err(RuntimeError::new(
                "return",
                "lowered function did not return",
            )
            .with_span(call_span)),
            LoweredStmtFlow::Continue => {
                Err(RuntimeError::new("control-flow", "continue outside loop").with_span(call_span))
            }
            LoweredStmtFlow::Break(_) => {
                Err(RuntimeError::new("control-flow", "break outside loop").with_span(call_span))
            }
        }
    }

    fn hydrate_lowered_captures(
        &mut self,
        lowered: &LoweredPureFunction,
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
            let Some(value) = lowered_value_from_runtime(&binding.value, capture.kind) else {
                return Err(RuntimeError::new(
                    "type-error",
                    format!(
                        "captured name `{}` no longer matches lowered type",
                        capture.name
                    ),
                )
                .with_span(call_span));
            };
            slots[capture.slot] = value;
        }
        Ok(())
    }

    fn write_back_lowered_captures(
        &mut self,
        lowered: &LoweredPureFunction,
        slots: &[LoweredValue],
        call_span: Span,
    ) -> Result<(), RuntimeError> {
        for capture in &lowered.captures {
            if capture.mutable {
                self.assign(
                    &capture.name,
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

    fn lowered_traceback_frame_kind(&self, function: LoweredFunctionKey) -> TracebackFrameKind {
        match function {
            LoweredFunctionKey::Name(name) if self.lowered_procs.contains_key(&name) => {
                TracebackFrameKind::Proc
            }
            _ => TracebackFrameKind::Pure,
        }
    }

    fn eval_lowered_call_with_frame(
        &mut self,
        function: LoweredFunctionKey,
        callee: &LoweredPureFunction,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        self.eval_lowered_call_frame(
            self.lowered_traceback_frame_kind(function),
            function.display_name(),
            callee,
            slots,
            call_span,
        )
    }

    fn eval_lowered_call_frame(
        &mut self,
        kind: TracebackFrameKind,
        name: String,
        callee: &LoweredPureFunction,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        let (enter_kind, exit_kind) = match kind {
            TracebackFrameKind::Pure => (TraceKind::PureEnter, TraceKind::PureExit),
            TracebackFrameKind::Proc => (TraceKind::ProcEnter, TraceKind::ProcExit),
        };
        self.trace_enter(enter_kind, Some(call_span), Some(&name), TracePayload::None);
        self.call_stack.push(TracebackFrame {
            kind,
            name: name.clone(),
            definition_span: None,
            call_span: Some(call_span),
        });
        let result = self.eval_lowered_function(callee, slots, call_span);
        self.call_stack.pop();
        self.trace_exit(exit_kind, Some(call_span), Some(&name), TracePayload::None);
        result
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
            Flow::Return(_)
            | Flow::Break(_)
            | Flow::ContinueLoop => Err(RuntimeError::new(
                "control-flow",
                "lowered propagation produced unsupported control flow",
            )
            .with_span(span)),
        }
    }

    fn lowered_retry_attempt_value(&mut self, flow: LoweredStmtFlow) -> LoweredRetryAttemptValue {
        match flow {
            LoweredStmtFlow::None | LoweredStmtFlow::Continue => {
                LoweredRetryAttemptValue::Success(LoweredValue::Unit)
            }
            // The retry body's trailing expression is lowered as `BreakValue`, so
            // a successful attempt's value arrives as `Break(Some(..))`.
            LoweredStmtFlow::Break(Some(value)) => LoweredRetryAttemptValue::Success(value),
            LoweredStmtFlow::Break(None) => LoweredRetryAttemptValue::ControlBreak,
            // `?` failures inside the body propagate; the retry catches them and
            // retries (or surfaces the final error once attempts are exhausted).
            // The propagation value is a `ResultErr` wrapping the real error;
            // unwrap it so the attempt trace and final error carry the actual
            // error value rather than a `Result` wrapper.
            LoweredStmtFlow::Propagate(value) => {
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
            LoweredStmtFlow::Return(value) => LoweredRetryAttemptValue::Escape(value),
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

    pub(super) fn eval_lowered_fast_plain_return(
        &mut self,
        lowered: &LoweredPureFunction,
        slots: &mut [LoweredValue],
    ) -> Result<Option<LoweredValue>, RuntimeError> {
        match lowered.body.as_slice() {
            [LoweredStmt::Return { value }] => self.eval_lowered_plain_expr(lowered, value, slots),
            [
                LoweredStmt::If {
                    branches,
                    else_body: None,
                },
                LoweredStmt::Return { value: fallback },
            ] if branches.len() == 1 => {
                let (condition, body) = &branches[0];
                let [LoweredStmt::Return { value }] = body.as_slice() else {
                    return Ok(None);
                };
                let Some(LoweredValue::Bool(condition)) =
                    self.eval_lowered_plain_expr(lowered, condition, slots)?
                else {
                    return Ok(None);
                };
                if condition {
                    self.eval_lowered_plain_expr(lowered, value, slots)
                } else {
                    self.eval_lowered_plain_expr(lowered, fallback, slots)
                }
            }
            _ => Ok(None),
        }
    }

    pub(super) fn eval_lowered_plain_expr(
        &mut self,
        lowered: &LoweredPureFunction,
        expr: &LoweredExpr,
        slots: &mut [LoweredValue],
    ) -> Result<Option<LoweredValue>, RuntimeError> {
        match expr {
            LoweredExpr::Null => Ok(Some(LoweredValue::Null)),
            LoweredExpr::Unit => Ok(Some(LoweredValue::Unit)),
            LoweredExpr::Int(value) => Ok(Some(LoweredValue::Int(*value))),
            LoweredExpr::Float(value) => Ok(Some(LoweredValue::Float(*value))),
            LoweredExpr::Duration(value) => Ok(Some(LoweredValue::Duration(value.clone()))),
            LoweredExpr::Bool(value) => Ok(Some(LoweredValue::Bool(*value))),
            LoweredExpr::Str(value) => Ok(Some(LoweredValue::Str(value.clone()))),
            LoweredExpr::Bytes(value) => Ok(Some(LoweredValue::Bytes(value.clone()))),
            LoweredExpr::Path(value) => Ok(Some(LoweredValue::Path(value.clone()))),
            LoweredExpr::FunctionRef { function, pure } => {
                if *pure {
                    Ok(Some(LoweredValue::Pure(*function)))
                } else {
                    Ok(Some(LoweredValue::Proc(*function)))
                }
            }
            LoweredExpr::PathFrom { value, span } => {
                let Some(value) = self.eval_lowered_plain_expr(lowered, value, slots)? else {
                    return Ok(None);
                };
                lowered_path_from_value(value, "Path", *span)
                    .map(LoweredValue::Path)
                    .map(Some)
            }
            LoweredExpr::Param(index) => Ok(Some(slots[*index].clone())),
            LoweredExpr::StrByteLen { receiver, span } => {
                if let LoweredExpr::Param(slot) = receiver.as_ref() {
                    return lowered_str_byte_len_value(&slots[*slot], *span)
                        .map(LoweredValue::Int)
                        .map(Some);
                }
                let Some(receiver) = self.eval_lowered_plain_expr(lowered, receiver, slots)? else {
                    return Ok(None);
                };
                lowered_str_byte_len_value(&receiver, *span)
                    .map(LoweredValue::Int)
                    .map(Some)
            }
            LoweredExpr::StrByteAt {
                receiver,
                index,
                default,
                span,
            } => {
                let Some(index) = self.eval_lowered_plain_expr(lowered, index, slots)? else {
                    return Ok(None);
                };
                let LoweredValue::Int(index) = index else {
                    return Err(
                        RuntimeError::new("type-error", "byte_at expected Int").with_span(*span)
                    );
                };
                let default = match default {
                    Some(default) => {
                        let Some(default) =
                            self.eval_lowered_plain_expr(lowered, default, slots)?
                        else {
                            return Ok(None);
                        };
                        let LoweredValue::Int(default) = default else {
                            return Err(RuntimeError::new(
                                "type-error",
                                "byte_at default expected Int",
                            )
                            .with_span(*span));
                        };
                        default
                    }
                    None => -1,
                };
                if let LoweredExpr::Param(slot) = receiver.as_ref() {
                    return lowered_str_byte_at_value(&slots[*slot], index, default, *span)
                        .map(LoweredValue::Int)
                        .map(Some);
                }
                let Some(receiver) = self.eval_lowered_plain_expr(lowered, receiver, slots)? else {
                    return Ok(None);
                };
                lowered_str_byte_at_value(&receiver, index, default, *span)
                    .map(LoweredValue::Int)
                    .map(Some)
            }
            LoweredExpr::StrPredicate {
                receiver,
                predicate,
                needle,
                span,
            } => {
                let Some(needle) = self.eval_lowered_plain_expr(lowered, needle, slots)? else {
                    return Ok(None);
                };
                if let LoweredExpr::Param(slot) = receiver.as_ref() {
                    return lowered_str_predicate_value(&slots[*slot], *predicate, &needle, *span)
                        .map(LoweredValue::Bool)
                        .map(Some);
                }
                let Some(receiver) = self.eval_lowered_plain_expr(lowered, receiver, slots)? else {
                    return Ok(None);
                };
                lowered_str_predicate_value(&receiver, *predicate, &needle, *span)
                    .map(LoweredValue::Bool)
                    .map(Some)
            }
            LoweredExpr::Contains {
                receiver,
                needle,
                span,
            } => {
                let Some(needle) = self.eval_lowered_plain_expr(lowered, needle, slots)? else {
                    return Ok(None);
                };
                if let LoweredExpr::Param(slot) = receiver.as_ref() {
                    return lowered_contains_value(&slots[*slot], &needle, *span)
                        .map(LoweredValue::Bool)
                        .map(Some);
                }
                let Some(receiver) = self.eval_lowered_plain_expr(lowered, receiver, slots)? else {
                    return Ok(None);
                };
                lowered_contains_value(&receiver, &needle, *span)
                    .map(LoweredValue::Bool)
                    .map(Some)
            }
            LoweredExpr::Tag { name, fields } => {
                let mut values = Vec::with_capacity(fields.len());
                for field in fields {
                    let Some(value) = self.eval_lowered_plain_expr(lowered, field, slots)? else {
                        return Ok(None);
                    };
                    values.push(value);
                }
                Ok(Some(LoweredValue::Tag {
                    name: name.clone(),
                    fields: values,
                }))
            }
            LoweredExpr::Binary {
                op,
                left,
                right,
                span,
            } => {
                if *op == BinaryOp::And {
                    let Some(left) = self.eval_lowered_plain_expr(lowered, left, slots)? else {
                        return Ok(None);
                    };
                    if !lowered_bool_value(left, *span)? {
                        return Ok(Some(LoweredValue::Bool(false)));
                    }
                    let Some(right) = self.eval_lowered_plain_expr(lowered, right, slots)? else {
                        return Ok(None);
                    };
                    return lowered_bool_value(right, *span)
                        .map(LoweredValue::Bool)
                        .map(Some);
                }
                if *op == BinaryOp::Or {
                    let Some(left) = self.eval_lowered_plain_expr(lowered, left, slots)? else {
                        return Ok(None);
                    };
                    if lowered_bool_value(left, *span)? {
                        return Ok(Some(LoweredValue::Bool(true)));
                    }
                    let Some(right) = self.eval_lowered_plain_expr(lowered, right, slots)? else {
                        return Ok(None);
                    };
                    return lowered_bool_value(right, *span)
                        .map(LoweredValue::Bool)
                        .map(Some);
                }
                let Some(left) = self.eval_lowered_plain_expr(lowered, left, slots)? else {
                    return Ok(None);
                };
                let Some(right) = self.eval_lowered_plain_expr(lowered, right, slots)? else {
                    return Ok(None);
                };
                lowered_binary_value(*op, left, right, *span).map(Some)
            }
            LoweredExpr::IfExpr {
                branches,
                else_value,
                span,
            } => {
                for (condition, value) in branches {
                    let Some(condition) =
                        self.eval_lowered_plain_expr(lowered, condition, slots)?
                    else {
                        return Ok(None);
                    };
                    if lowered_bool_value(condition, *span)? {
                        return self.eval_lowered_plain_expr(lowered, value, slots);
                    }
                }
                self.eval_lowered_plain_expr(lowered, else_value, slots)
            }
            LoweredExpr::MatchExpr { value, arms, span } => {
                let Some(value) = self.eval_lowered_plain_expr(lowered, value, slots)? else {
                    return Ok(None);
                };
                for (pattern, guard, arm_value) in arms {
                    if lowered_pattern_matches(pattern, &value, slots) {
                        if let Some(guard_expr) = guard {
                            let Some(guard_val) =
                                self.eval_lowered_plain_expr(lowered, guard_expr, slots)?
                            else {
                                continue;
                            };
                            if guard_val != LoweredValue::Bool(true) {
                                continue;
                            }
                        }
                        return self.eval_lowered_plain_expr(lowered, arm_value, slots);
                    }
                }
                Err(lowered_match_no_arm(*span))
            }
            LoweredExpr::StrMatchExpr {
                value,
                arms,
                fallback,
                span,
            } => {
                let Some(value) = self.eval_lowered_plain_expr(lowered, value, slots)? else {
                    return Ok(None);
                };
                if let Some(key) = lowered_str_key(&value)
                    && let Some(arm_value) = arms.get(key)
                {
                    return self.eval_lowered_plain_expr(lowered, arm_value, slots);
                }
                if let Some(fallback) = fallback {
                    return self.eval_lowered_plain_expr(lowered, fallback, slots);
                }
                Err(lowered_match_no_arm(*span))
            }
            LoweredExpr::TagMatchExpr {
                value,
                arms,
                fallback,
                span,
            } => {
                let Some(value) = self.eval_lowered_plain_expr(lowered, value, slots)? else {
                    return Ok(None);
                };
                if let Some(key) = lowered_tag_key(&value)
                    && let Some(arm_value) = arms.get(key)
                {
                    return self.eval_lowered_plain_expr(lowered, arm_value, slots);
                }
                if let Some(fallback) = fallback {
                    return self.eval_lowered_plain_expr(lowered, fallback, slots);
                }
                Err(lowered_match_no_arm(*span))
            }
            LoweredExpr::ResultFallback { .. } => Ok(None),
            LoweredExpr::HashVerifyFile { .. } => Ok(None),
            LoweredExpr::ModuleCall { .. } => Ok(None),
            LoweredExpr::DynamicCall { .. } => Ok(None),
            LoweredExpr::Glob { .. } => Ok(None),
            LoweredExpr::LastStatus { .. } => Ok(None),
            LoweredExpr::ProcessCommandArgv {
                target,
                argv,
                cwd,
                env,
                timeout,
                detach,
                new_session,
                ignore_hup,
                cpu_max,
                span,
            } => {
                let Some(target) = self.eval_lowered_plain_expr(lowered, target, slots)? else {
                    return Ok(None);
                };
                let Some(argv) = self.eval_lowered_plain_expr(lowered, argv, slots)? else {
                    return Ok(None);
                };
                let cwd = match cwd {
                    Some(value) => match self.eval_lowered_plain_expr(lowered, value, slots)? {
                        Some(value) => Some(value),
                        None => return Ok(None),
                    },
                    None => None,
                };
                let env = match env {
                    Some(value) => match self.eval_lowered_plain_expr(lowered, value, slots)? {
                        Some(value) => Some(value),
                        None => return Ok(None),
                    },
                    None => None,
                };
                let timeout = match timeout {
                    Some(value) => match self.eval_lowered_plain_expr(lowered, value, slots)? {
                        Some(value) => Some(value),
                        None => return Ok(None),
                    },
                    None => None,
                };
                let detach = match detach {
                    Some(value) => match self.eval_lowered_plain_expr(lowered, value, slots)? {
                        Some(value) => Some(value),
                        None => return Ok(None),
                    },
                    None => None,
                };
                let new_session = match new_session {
                    Some(value) => match self.eval_lowered_plain_expr(lowered, value, slots)? {
                        Some(value) => Some(value),
                        None => return Ok(None),
                    },
                    None => None,
                };
                let ignore_hup = match ignore_hup {
                    Some(value) => match self.eval_lowered_plain_expr(lowered, value, slots)? {
                        Some(value) => Some(value),
                        None => return Ok(None),
                    },
                    None => None,
                };
                let cpu_max = match cpu_max {
                    Some(value) => match self.eval_lowered_plain_expr(lowered, value, slots)? {
                        Some(value) => Some(value),
                        None => return Ok(None),
                    },
                    None => None,
                };
                lowered_command_plan_value(
                    target,
                    argv,
                    cwd,
                    env,
                    timeout,
                    detach,
                    new_session,
                    ignore_hup,
                    cpu_max,
                    *span,
                )
                .map(Some)
            }
            LoweredExpr::Call {
                function,
                args,
                span,
            } => {
                let Some(callee) = self.lowered_function(*function) else {
                    return Err(RuntimeError::new(
                        "unresolved-lowered-call",
                        function.display_name(),
                    )
                    .with_span(*span));
                };
                if !matches!(
                    callee.return_kind,
                    LoweredReturnKind::Plain(LoweredType::Int | LoweredType::Bool)
                ) {
                    return Ok(None);
                }
                let mut values = Vec::with_capacity(args.len());
                for arg in args {
                    match arg {
                        LoweredCallArg::Single(arg) => {
                            let Some(value) = self.eval_lowered_plain_expr(lowered, arg, slots)?
                            else {
                                return Ok(None);
                            };
                            values.push(value);
                        }
                        LoweredCallArg::Splice(arg) => {
                            let Some(value) = self.eval_lowered_plain_expr(lowered, arg, slots)?
                            else {
                                return Ok(None);
                            };
                            values.extend(lowered_splice_arg_items(value, *span)?);
                        }
                    }
                }
                let Some(mut next_slots) = self.try_bind_lowered_values(&callee, &values) else {
                    return Ok(None);
                };
                let result = self
                    .eval_lowered_call_with_frame(*function, &callee, &mut next_slots, *span)
                    .and_then(|value| lowered_return_value(callee.return_kind, value, *span))
                    .map(Some);
                self.recycle_lowered_slots(next_slots);
                result
            }
            LoweredExpr::SelfCall { args, span } => {
                let mut values = Vec::with_capacity(args.len());
                for arg in args {
                    match arg {
                        LoweredCallArg::Single(arg) => {
                            let Some(value) = self.eval_lowered_plain_expr(lowered, arg, slots)?
                            else {
                                return Ok(None);
                            };
                            values.push(value);
                        }
                        LoweredCallArg::Splice(arg) => {
                            let Some(value) = self.eval_lowered_plain_expr(lowered, arg, slots)?
                            else {
                                return Ok(None);
                            };
                            values.extend(lowered_splice_arg_items(value, *span)?);
                        }
                    }
                }
                let Some(mut next_slots) = self.try_bind_lowered_values(lowered, &values) else {
                    return Ok(None);
                };
                let result = self
                    .eval_lowered_function(lowered, &mut next_slots, *span)
                    .and_then(|value| lowered_return_value(lowered.return_kind, value, *span))
                    .map(Some);
                self.recycle_lowered_slots(next_slots);
                result
            }
            LoweredExpr::FmtString(_)
            | LoweredExpr::PathFmtString { .. }
            | LoweredExpr::Record(_)
            | LoweredExpr::List(_)
            | LoweredExpr::EmptyMap
            | LoweredExpr::BytesConcat { .. }
            | LoweredExpr::Range { .. }
            | LoweredExpr::ListComp { .. }
            | LoweredExpr::MapComp { .. }
            | LoweredExpr::ListPipeline { .. }
            | LoweredExpr::Field { .. }
            | LoweredExpr::Index { .. }
            | LoweredExpr::Method { .. }
            | LoweredExpr::RegexCompile { .. }
            | LoweredExpr::Require { .. }
            | LoweredExpr::RunCapture { .. }
            | LoweredExpr::RunPipeline { .. }
            | LoweredExpr::SpawnRun { .. }
            | LoweredExpr::SpawnCommand { .. }
            | LoweredExpr::Wait { .. }
            | LoweredExpr::Loop { .. }
            | LoweredExpr::Retry { .. }
            | LoweredExpr::FsList { .. }
            | LoweredExpr::FsFiles { .. }
            | LoweredExpr::FsWalk { .. }
            | LoweredExpr::FsTempDir { .. }
            | LoweredExpr::FsWrite { .. }
            | LoweredExpr::FsMkdir { .. }
            | LoweredExpr::FsRemove { .. }
            | LoweredExpr::FsCloseRoot { .. }
            | LoweredExpr::FsRootPath { .. }
            | LoweredExpr::PathReadText { .. }
            | LoweredExpr::PathReadBytes { .. }
            | LoweredExpr::PathExists { .. }
            | LoweredExpr::PathExecutable { .. }
            | LoweredExpr::PathDu { .. }
            | LoweredExpr::PathMetadata { .. }
            | LoweredExpr::PathReadlink { .. }
            | LoweredExpr::PathResolve { .. }
            | LoweredExpr::PathWrite { .. }
            | LoweredExpr::PathMkdir { .. }
            | LoweredExpr::PathRemove { .. }
            | LoweredExpr::JsonEncode { .. }
            | LoweredExpr::ArchiveTarCreate { .. }
            | LoweredExpr::ArchiveTarList { .. }
            | LoweredExpr::ArchiveTarExtract { .. }
            | LoweredExpr::Slice { .. }
            | LoweredExpr::ProcessCommandBuilder { .. }
            | LoweredExpr::Abort { .. }
            | LoweredExpr::Ok(_)
            | LoweredExpr::Err(_)
            | LoweredExpr::Error(_)
            | LoweredExpr::Try(_) => Ok(None),
        }
    }

    pub(super) fn eval_lowered_fast_return(
        &mut self,
        lowered: &LoweredPureFunction,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<Option<LoweredValue>, RuntimeError> {
        match lowered.body.as_slice() {
            [LoweredStmt::Return { value }] => {
                let value = match self.eval_lowered_expr(lowered, value, slots, call_span)? {
                    ControlFlow::Continue(value) | ControlFlow::Break(value) => value,
                };
                Ok(Some(value))
            }
            [
                LoweredStmt::If {
                    branches,
                    else_body: None,
                },
                LoweredStmt::Return { value: fallback },
            ] if branches.len() == 1 => {
                let (condition, body) = &branches[0];
                let [LoweredStmt::Return { value }] = body.as_slice() else {
                    return Ok(None);
                };
                let condition =
                    match self.eval_lowered_bool(lowered, condition, slots, call_span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(Some(value)),
                    };
                if condition {
                    let value = match self.eval_lowered_expr(lowered, value, slots, call_span)? {
                        ControlFlow::Continue(value) | ControlFlow::Break(value) => value,
                    };
                    Ok(Some(value))
                } else {
                    let value = match self.eval_lowered_expr(lowered, fallback, slots, call_span)? {
                        ControlFlow::Continue(value) | ControlFlow::Break(value) => value,
                    };
                    Ok(Some(value))
                }
            }
            _ => Ok(None),
        }
    }

    pub(super) fn eval_lowered_stmts(
        &mut self,
        lowered: &LoweredPureFunction,
        statements: &[LoweredStmt],
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<LoweredStmtFlow, RuntimeError> {
        let mut defers: Vec<LoweredExpr> = Vec::new();
        for stmt in statements {
            if let LoweredStmt::Defer { value, .. } = stmt {
                defers.push(value.clone());
                continue;
            }
            match self.eval_lowered_stmt(lowered, stmt, slots, call_span) {
                Ok(LoweredStmtFlow::None) => {}
                Ok(
                    flow @ (LoweredStmtFlow::Return(_)
                    | LoweredStmtFlow::Propagate(_)
                    | LoweredStmtFlow::Break(_)
                    | LoweredStmtFlow::Continue),
                ) => {
                    self.run_lowered_defers(lowered, &defers, slots, call_span)?;
                    return Ok(flow);
                }
                Err(error) => {
                    // Run registered defers before propagating the error (scope
                    // exit), unless this is a FORCED abort which skips them. The
                    // original error takes precedence over any defer failure.
                    let forced = error.abort.as_ref().is_some_and(|signal| signal.force);
                    if !forced {
                        let _ = self.run_lowered_defers(lowered, &defers, slots, call_span);
                    }
                    return Err(error);
                }
            }
        }
        self.run_lowered_defers(lowered, &defers, slots, call_span)?;
        Ok(LoweredStmtFlow::None)
    }

    fn run_lowered_defers(
        &mut self,
        lowered: &LoweredPureFunction,
        defers: &[LoweredExpr],
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<(), RuntimeError> {
        for value in defers.iter().rev() {
            match self.eval_lowered_expr(lowered, value, slots, call_span)? {
                ControlFlow::Continue(_) => {}
                ControlFlow::Break(_value) => {
                    return Err(RuntimeError::new(
                        "defer-control-flow",
                        "deferred expression produced invalid control flow",
                    )
                    .with_span(call_span));
                }
            }
        }
        Ok(())
    }

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

        // Pass 1: install the module file's functions (and any submodules it
        // imports) into this evaluator's lowered-function tables. The module's
        // top-level functions install unqualified; their private sibling calls,
        // submodule (`use`) imports, and captures of module-scope `let` bindings
        // all resolve through the standard compact installation. Capture each
        // exported function's signature so `module.require` can validate
        // `export proc`/`export pure` contract fields.
        self.install_compact_lowered_functions_with_source(
            &parsed.arena,
            module_source_id,
            &module_text,
        );
        let declarations = crate::sema::check::Checker::check_compact_declarations(&parsed.arena);
        if declarations.diagnostics.is_empty() {
            for (name, pure) in self.module_namespace_export_functions(&parsed.arena) {
                let function = crate::runtime::value::FunctionName::name(name);
                let sig = if pure {
                    declarations.pures.get(&name)
                } else {
                    declarations.procs.get(&name)
                };
                if let Some(sig) = sig {
                    self.record_module_export_signature(function, pure, sig);
                }
            }
        }

        // Pass 2: run the module file (with top-level `export` stripped) as its
        // own compact program in a child evaluator to materialize exported `let`
        // values. The stripped source keeps byte offsets, so `use` imports and
        // the module's own functions still resolve, and the `let` initializers
        // execute as ordinary top-level statements.
        let harvest_text = Self::module_harvest_source(&parsed.arena, &module_text);
        let harvest_entry = crate::loader::entry_source_from_text(&display_path, harvest_text);
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
            let exported_let_names = self.module_namespace_export_let_names(&parsed.arena);
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
        // plus handles for exported functions (unqualified, matching how they
        // were installed in Pass 1).
        let mut record = child_exports;
        for (name, pure) in self.module_namespace_export_functions(&parsed.arena) {
            let function = crate::runtime::value::FunctionName::name(name);
            let value = if pure {
                Value::Pure(function)
            } else {
                Value::Proc(function)
            };
            record.insert(Arc::from(name.as_str()), value);
        }

        self.module_value_cache.insert(key, record.clone());
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
        self.install_compact_lowered_program(program, source_id);
        let source = self
            .sources
            .get(source_id)
            .map(|source| source.text().to_string())
            .unwrap_or_default();
        let declarations = crate::sema::check::Checker::check_compact_declarations(program);
        let bodies = crate::sema::check::Checker::probe_compact_bodies(program, &declarations);
        let module_programs = if declarations.diagnostics.is_empty() {
            let functions = LowerableFunctions::all(
                &self.lowered_pures,
                &self.lowered_procs,
                &self.lowered_qualified_pures,
                &self.lowered_qualified_procs,
            );
            program
                .modules
                .iter()
                .map(|module| {
                    let statements = program.module_statements(module).collect::<Vec<_>>();
                    let lowered = super::lower::lower_compact_module_program(
                        program,
                        &declarations,
                        &bodies,
                        &source,
                        module.name,
                        &functions,
                    );
                    (statements, lowered.statements)
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        for (statements, lowered_statements) in module_programs {
            for (index, stmt) in statements.iter().copied().enumerate() {
                let span = program.arena.stmt(stmt).span;
                let Some(lowered) = lowered_statements.get(index).cloned().flatten() else {
                    continue;
                };
                if matches!(lowered.kind, LoweredTopLevelKind::Defer { .. }) {
                    continue;
                }
                match self.eval_lowered_top_level_stmt(&lowered, span)? {
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
        }
        let root: Vec<_> = program.statement_ids().collect();
        let lowered_statements = self.lowered_program.statements.clone();
        for (index, stmt) in root.iter().copied().enumerate() {
            let span = program.arena.stmt(stmt).span;
            let Some(lowered) = lowered_statements.get(index).cloned().flatten() else {
                continue;
            };
            if matches!(lowered.kind, LoweredTopLevelKind::Defer { .. }) {
                continue;
            }
            match self.eval_lowered_top_level_stmt(&lowered, span)? {
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
        // Collect exported `let` values for the export record, and the child
        // top-level bindings so the caller can make the module's functions
        // resolve captured module-scope values and imported namespaces.
        let mut record = RecordMap::new();
        for name in exported_let_names {
            if let Some(binding) = self.lookup(*name) {
                record.insert(Arc::from(name.as_str()), binding.value.clone());
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
        Ok((record, bindings))
    }

    pub(super) fn eval_lowered_top_level_stmt(
        &mut self,
        lowered: &LoweredTopLevelStmt,
        call_span: Span,
    ) -> Result<Option<Flow>, RuntimeError> {
        let mut slots = vec![LoweredValue::Unit; lowered.slot_count];
        for slot in &lowered.slots {
            let Some(binding) = self.lookup(slot.name) else {
                return Ok(None);
            };
            // Re-hydrate the captured binding into its slot. `slot.kind` is the
            // statically-inferred type; if it disagrees with the actual runtime
            // value (e.g. a run-result binding inferred as Status but holding
            // Bytes), fall back to a kind-agnostic conversion so a correct
            // program still runs instead of silently dropping to the fallback
            // interpreter.
            let Some(value) = lowered_value_from_runtime(&binding.value, slot.kind)
                .or_else(|| lowered_value_from_runtime_any(&binding.value))
            else {
                return Ok(None);
            };
            slots[slot.slot] = value;
        }
        let lowered_function = LoweredPureFunction {
            params: Default::default(),
            param_kinds: Default::default(),
            param_checks: Default::default(),
            param_rest: Default::default(),
            param_defaults: Default::default(),
            captures: Default::default(),
            return_kind: LoweredReturnKind::Plain(LoweredType::Unit),
            slot_count: lowered.slot_count,
            body: Vec::new(),
        };
        let flow = match &lowered.kind {
            LoweredTopLevelKind::Use {
                key,
                alias,
                path,
                namespace,
                exports,
                module_statements,
                span,
            } => {
                if path.is_empty() {
                    return Err(
                        RuntimeError::new("unknown-module", "empty module path").with_span(*span)
                    );
                }
                let import_name = alias.unwrap_or(*namespace);
                for (module_span, module_stmt) in module_statements {
                    if matches!(module_stmt.kind, LoweredTopLevelKind::Defer { .. }) {
                        continue;
                    }
                    match self.eval_lowered_top_level_stmt(module_stmt, *module_span)? {
                        Some(Flow::Continue(_)) | None => {}
                        Some(Flow::Propagate(propagation)) => {
                            return Err(runtime_error_from_value(propagation.error, *module_span));
                        }
                        Some(_) => {
                            return Err(RuntimeError::new(
                                "module-load",
                                format!("invalid control flow while importing {key}"),
                            )
                            .with_span(*module_span));
                        }
                    }
                }
                let mut record = RecordMap::new();
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
                                .with_span(*span)
                            })?,
                        LoweredModuleExportKind::Pure => {
                            let namespace = export.function_namespace.unwrap_or(*namespace);
                            Value::Pure(QualifiedName::new(namespace, export.name).into())
                        }
                        LoweredModuleExportKind::Proc => {
                            let namespace = export.function_namespace.unwrap_or(*namespace);
                            Value::Proc(QualifiedName::new(namespace, export.name).into())
                        }
                    };
                    record.insert(Arc::from(export.name.as_str()), value.clone());
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
                self.define(
                    import_name,
                    Binding {
                        value: Value::Module(record),
                        mutable: false,
                    },
                );
                Flow::Continue(Value::Unit)
            }
            LoweredTopLevelKind::Let {
                target,
                ty,
                validation,
                mutable,
                value,
                value_span,
            } => {
                let mut value = match self.eval_lowered_expr(
                    &lowered_function,
                    value,
                    &mut slots,
                    call_span,
                )? {
                    ControlFlow::Continue(value) => value.into_value(),
                    ControlFlow::Break(value) => {
                        return Ok(Some(self.question_flow(value.into_value(), call_span)));
                    }
                };
                if let Some(check) = validation {
                    if matches!(&check.ty, crate::sema::types::Type::Map(_))
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
                        .with_span(*value_span));
                    }
                } else if let Some(ty) = ty
                    && lowered_value_from_runtime(&value, *ty).is_none()
                {
                    return Err(RuntimeError::new(
                        "type-error",
                        format!("expected {}", lowered_type_name(*ty)),
                    )
                    .with_span(*value_span));
                }
                if validation.is_none()
                    && *ty == Some(LoweredType::Map)
                    && let Value::Record(record) = &value
                    && record.is_empty()
                {
                    self.define(
                        *target,
                        Binding {
                            value: Value::Map(Default::default()),
                            mutable: *mutable,
                        },
                    );
                } else {
                    self.define(
                        *target,
                        Binding {
                            value,
                            mutable: *mutable,
                        },
                    );
                }
                Flow::Continue(Value::Unit)
            }
            LoweredTopLevelKind::LetRecord {
                source,
                fields,
                mutable,
                span,
            } => {
                let source = match self.eval_lowered_expr(
                    &lowered_function,
                    source,
                    &mut slots,
                    call_span,
                )? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => {
                        return Ok(Some(self.question_flow(value.into_value(), call_span)));
                    }
                };
                for name in fields {
                    let Some(value) = lowered_record_field(&source, name.as_str()) else {
                        return Err(RuntimeError::new(
                            "field-access",
                            format!("record has no field `{}`", name.as_str()),
                        )
                        .with_span(*span));
                    };
                    self.define(
                        *name,
                        Binding {
                            value: value.clone().into_value(),
                            mutable: *mutable,
                        },
                    );
                }
                Flow::Continue(Value::Unit)
            }
            LoweredTopLevelKind::Assign {
                target,
                op,
                value,
                span,
            } => {
                let value = match self.eval_lowered_expr(
                    &lowered_function,
                    value,
                    &mut slots,
                    call_span,
                )? {
                    ControlFlow::Continue(value) => value.into_value(),
                    ControlFlow::Break(value) => {
                        return Ok(Some(self.question_flow(value.into_value(), call_span)));
                    }
                };
                let value = if *op == AssignOp::Set {
                    value
                } else {
                    let current = self
                        .lookup(*target)
                        .map(|binding| binding.value.clone())
                        .ok_or_else(|| {
                            RuntimeError::new("unresolved-name", target).with_span(*span)
                        })?;
                    compound_assignment_value(*op, current, value, *span)?
                };
                self.assign(target, value, *span)?;
                Flow::Continue(Value::Unit)
            }
            LoweredTopLevelKind::Discard { value, span } => {
                match self.eval_lowered_expr(&lowered_function, value, &mut slots, *span)? {
                    ControlFlow::Continue(_) => Flow::Continue(Value::Unit),
                    ControlFlow::Break(value) => {
                        return Ok(Some(self.question_flow(value.into_value(), *span)));
                    }
                }
            }
            LoweredTopLevelKind::Stmt(stmt) => {
                let flow = self.eval_lowered_stmt(&lowered_function, stmt, &mut slots, call_span);
                self.write_back_lowered_top_level_slots(lowered, &slots, call_span)?;
                match flow? {
                    LoweredStmtFlow::Propagate(value) => {
                        self.question_flow(value.into_value(), call_span)
                    }
                    flow => lowered_stmt_flow_to_flow(flow),
                }
            }
            LoweredTopLevelKind::Expr(value) => {
                let value = match self.eval_lowered_expr(
                    &lowered_function,
                    value,
                    &mut slots,
                    call_span,
                )? {
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
            LoweredTopLevelKind::Defer { value, span } => {
                match self.eval_lowered_expr(&lowered_function, value, &mut slots, *span)? {
                    ControlFlow::Continue(_) => Flow::Continue(Value::Unit),
                    ControlFlow::Break(value) => {
                        return Ok(Some(self.question_flow(value.into_value(), *span)));
                    }
                }
            }
            LoweredTopLevelKind::SignalHook {
                signal,
                pre_cancel,
                body,
                slots: hook_slots,
                slot_count,
                span,
            } => {
                self.register_compact_signal_hook(
                    signal.as_str(),
                    pre_cancel.as_deref(),
                    body.clone(),
                    hook_slots.clone(),
                    *slot_count,
                    *span,
                )?;
                Flow::Continue(Value::Unit)
            }
        };
        Ok(Some(flow))
    }

    pub(super) fn write_back_lowered_top_level_slots(
        &mut self,
        lowered: &LoweredTopLevelStmt,
        slots: &[LoweredValue],
        span: Span,
    ) -> Result<(), RuntimeError> {
        for slot in &lowered.slots {
            if slot.mutable {
                self.assign(&slot.name, slots[slot.slot].clone().into_value(), span)?;
            }
        }
        Ok(())
    }

    pub(super) fn eval_lowered_typed_int(
        &mut self,
        expr: &LoweredIntExpr,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<ControlFlow<LoweredValue, i64>, RuntimeError> {
        let _ = call_span;
        match expr {
            LoweredIntExpr::Int(value) => Ok(ControlFlow::Continue(*value)),
            LoweredIntExpr::Slot(slot) => match slots[*slot] {
                LoweredValue::Int(value) => Ok(ControlFlow::Continue(value)),
                _ => Err(
                    RuntimeError::new("type-error", "lowered expression expected Int")
                        .with_span(call_span),
                ),
            },
            LoweredIntExpr::Binary { op, left, right } => {
                let left = match self.eval_lowered_typed_int(left, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let right = match self.eval_lowered_typed_int(right, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let value = match op {
                    BinaryOp::Add => left + right,
                    BinaryOp::Sub => left - right,
                    BinaryOp::Mul => left * right,
                    BinaryOp::Div => left / right,
                    BinaryOp::Rem => left % right,
                    _ => unreachable!(),
                };
                Ok(ControlFlow::Continue(value))
            }
            LoweredIntExpr::StrByteLenSlot { slot, span } => {
                lowered_str_byte_len_value(&slots[*slot], *span).map(ControlFlow::Continue)
            }
            LoweredIntExpr::StrCountLinesSlot { slot, span } => {
                lowered_str_count_lines_value(&slots[*slot], *span).map(ControlFlow::Continue)
            }
            LoweredIntExpr::StrByteAtSlot {
                slot,
                index,
                default,
                span,
            } => {
                let index = match self.eval_lowered_typed_int(index, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let default = match default {
                    Some(default) => {
                        match self.eval_lowered_typed_int(default, slots, call_span)? {
                            ControlFlow::Continue(value) => value,
                            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                        }
                    }
                    None => -1,
                };
                lowered_str_byte_at_value(&slots[*slot], index, default, *span)
                    .map(ControlFlow::Continue)
            }
        }
    }

    pub(super) fn eval_lowered_typed_bool(
        &mut self,
        expr: &LoweredBoolExpr,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<ControlFlow<LoweredValue, bool>, RuntimeError> {
        match expr {
            LoweredBoolExpr::Bool(value) => Ok(ControlFlow::Continue(*value)),
            LoweredBoolExpr::Slot(slot) => match slots[*slot] {
                LoweredValue::Bool(value) => Ok(ControlFlow::Continue(value)),
                LoweredValue::Status(ref status) => Ok(ControlFlow::Continue(status.success)),
                _ => Err(
                    RuntimeError::new("type-error", "lowered expression expected Bool")
                        .with_span(call_span),
                ),
            },
            LoweredBoolExpr::Not(inner) => {
                let value = match self.eval_lowered_typed_bool(inner, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                Ok(ControlFlow::Continue(!value))
            }
            LoweredBoolExpr::And(left, right) => {
                let left = match self.eval_lowered_typed_bool(left, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                if !left {
                    return Ok(ControlFlow::Continue(false));
                }
                self.eval_lowered_typed_bool(right, slots, call_span)
            }
            LoweredBoolExpr::Or(left, right) => {
                let left = match self.eval_lowered_typed_bool(left, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                if left {
                    return Ok(ControlFlow::Continue(true));
                }
                self.eval_lowered_typed_bool(right, slots, call_span)
            }
            LoweredBoolExpr::IntCompare { op, left, right } => {
                let left = match self.eval_lowered_typed_int(left, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let right = match self.eval_lowered_typed_int(right, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let value = match op {
                    BinaryOp::Eq => left == right,
                    BinaryOp::Ne => left != right,
                    BinaryOp::Lt => left < right,
                    BinaryOp::Le => left <= right,
                    BinaryOp::Gt => left > right,
                    BinaryOp::Ge => left >= right,
                    _ => unreachable!(),
                };
                Ok(ControlFlow::Continue(value))
            }
            LoweredBoolExpr::StrPredicateSlot {
                slot,
                predicate,
                needle,
                span,
            } => lowered_str_predicate_text(&slots[*slot], *predicate, needle, *span)
                .map(ControlFlow::Continue),
            LoweredBoolExpr::ContainsSlot { slot, needle, span } => {
                lowered_contains_value(&slots[*slot], needle, *span).map(ControlFlow::Continue)
            }
            LoweredBoolExpr::StrContainsSlot { slot, needle, span } => {
                if let Some(text) = lowered_str_value(&slots[*slot]) {
                    Ok(ControlFlow::Continue(bytes_contains(
                        text.as_bytes(),
                        needle.as_bytes(),
                    )))
                } else {
                    let needle = LoweredValue::Str(needle.clone());
                    lowered_contains_value(&slots[*slot], &needle, *span).map(ControlFlow::Continue)
                }
            }
            LoweredBoolExpr::TrimEmptySlot { slot, span } => {
                lowered_trim_is_empty_value(&slots[*slot], *span).map(ControlFlow::Continue)
            }
            LoweredBoolExpr::TrimStrPredicateSlot {
                slot,
                predicate,
                needle,
                span,
            } => lowered_trim_str_predicate_value(&slots[*slot], *predicate, needle, *span)
                .map(ControlFlow::Continue),
            LoweredBoolExpr::LiteralCompareSlot { op, slot, value } => {
                let equal = slots[*slot] == *value;
                Ok(ControlFlow::Continue(match op {
                    BinaryOp::Eq => equal,
                    BinaryOp::Ne => !equal,
                    _ => unreachable!(),
                }))
            }
        }
    }

    pub(super) fn eval_lowered_stmt(
        &mut self,
        lowered: &LoweredPureFunction,
        stmt: &LoweredStmt,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<LoweredStmtFlow, RuntimeError> {
        match stmt {
            LoweredStmt::Let { slot, value } => {
                match self.eval_lowered_expr(lowered, value, slots, call_span)? {
                    ControlFlow::Continue(value) => slots[*slot] = value,
                    ControlFlow::Break(value) => return Ok(LoweredStmtFlow::Return(value)),
                }
                Ok(LoweredStmtFlow::None)
            }
            LoweredStmt::Guard {
                slot,
                value,
                else_param_slot,
                else_body,
                span,
            } => {
                let value = match self.eval_lowered_expr(lowered, value, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(LoweredStmtFlow::Return(value)),
                };
                match value {
                    LoweredValue::ResultOk(inner) => {
                        slots[*slot] = *inner;
                        Ok(LoweredStmtFlow::None)
                    }
                    LoweredValue::ResultErr(err) => {
                        if let Some(else_slot) = else_param_slot {
                            slots[*else_slot] = LoweredValue::Error(err);
                        }
                        let flow = self.eval_lowered_stmts(lowered, else_body, slots, *span)?;
                        match flow {
                            // The else block must diverge; if it falls through
                            // the success binding is unset, which the checker
                            // forbids — surface it rather than continue.
                            LoweredStmtFlow::None => {
                                Err(RuntimeError::new("guard", "guard else block must diverge")
                                    .with_span(*span))
                            }
                            other => Ok(other),
                        }
                    }
                    other => Err(RuntimeError::new(
                        "type-error",
                        format!("guard expected Result, found {}", other.type_name()),
                    )
                    .with_span(*span)),
                }
            }
            LoweredStmt::LetRecord {
                source,
                fields,
                span,
            } => {
                let source = match self.eval_lowered_expr(lowered, source, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(LoweredStmtFlow::Return(value)),
                };
                for (name, slot) in fields {
                    let Some(value) = lowered_record_field(&source, name.as_str()) else {
                        return Err(RuntimeError::new(
                            "field-access",
                            format!("record has no field `{}`", name.as_str()),
                        )
                        .with_span(*span));
                    };
                    slots[*slot] = value.clone();
                }
                Ok(LoweredStmtFlow::None)
            }
            LoweredStmt::LetInt { slot, value } => {
                match self.eval_lowered_typed_int(value, slots, call_span)? {
                    ControlFlow::Continue(value) => slots[*slot] = LoweredValue::Int(value),
                    ControlFlow::Break(value) => return Ok(LoweredStmtFlow::Return(value)),
                }
                Ok(LoweredStmtFlow::None)
            }
            LoweredStmt::LetBool { slot, value } => {
                match self.eval_lowered_typed_bool(value, slots, call_span)? {
                    ControlFlow::Continue(value) => slots[*slot] = LoweredValue::Bool(value),
                    ControlFlow::Break(value) => return Ok(LoweredStmtFlow::Return(value)),
                }
                Ok(LoweredStmtFlow::None)
            }
            LoweredStmt::Assign {
                slot,
                op,
                value,
                span,
            } => {
                if *op == AssignOp::Set
                    && let LoweredExpr::Method {
                        receiver,
                        name,
                        args,
                        span: method_span,
                    } = value
                    && let LoweredExpr::Param(receiver_slot) = receiver.as_ref()
                    && receiver_slot == slot
                {
                    if name == "push" && args.len() == 1 {
                        let item = match self
                            .eval_lowered_expr(lowered, &args[0], slots, call_span)?
                        {
                            ControlFlow::Continue(value) => value,
                            ControlFlow::Break(value) => return Ok(LoweredStmtFlow::Return(value)),
                        };
                        if let LoweredValue::List(items) = &mut slots[*slot] {
                            items.push(item);
                            return Ok(LoweredStmtFlow::None);
                        }
                    } else if name == "set" && args.len() == 2 {
                        let key =
                            match self.eval_lowered_expr(lowered, &args[0], slots, call_span)? {
                                ControlFlow::Continue(value) => value,
                                ControlFlow::Break(value) => {
                                    return Ok(LoweredStmtFlow::Return(value));
                                }
                            };
                        let value =
                            match self.eval_lowered_expr(lowered, &args[1], slots, call_span)? {
                                ControlFlow::Continue(value) => value,
                                ControlFlow::Break(value) => {
                                    return Ok(LoweredStmtFlow::Return(value));
                                }
                            };
                        if let LoweredValue::Map(map) = &mut slots[*slot] {
                            let key = lowered_str_arg(&key, "set", *method_span)?.to_string();
                            map.insert(key, value);
                            return Ok(LoweredStmtFlow::None);
                        }
                    }
                }
                if matches!(
                    op,
                    AssignOp::Add | AssignOp::Sub | AssignOp::Mul | AssignOp::Div | AssignOp::Rem
                ) && let LoweredValue::Int(current) = slots[*slot]
                    && let Some(value) = self.eval_lowered_int_candidate(lowered, value, slots)?
                {
                    let value = match value {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(LoweredStmtFlow::Return(value)),
                    };
                    slots[*slot] = LoweredValue::Int(match op {
                        AssignOp::Add => current + value,
                        AssignOp::Sub => current - value,
                        AssignOp::Mul => current * value,
                        AssignOp::Div => current / value,
                        AssignOp::Rem => current % value,
                        AssignOp::Set => unreachable!(),
                    });
                    return Ok(LoweredStmtFlow::None);
                }
                let value = match self.eval_lowered_expr(lowered, value, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(LoweredStmtFlow::Return(value)),
                };
                slots[*slot] = lowered_assign_value(*op, slots[*slot].clone(), value, *span)?;
                Ok(LoweredStmtFlow::None)
            }
            LoweredStmt::AssignInt {
                slot,
                op,
                value,
                span,
            } => {
                let value = match self.eval_lowered_typed_int(value, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(LoweredStmtFlow::Return(value)),
                };
                if *op == AssignOp::Set {
                    slots[*slot] = LoweredValue::Int(value);
                    return Ok(LoweredStmtFlow::None);
                }
                let LoweredValue::Int(current) = slots[*slot] else {
                    return Err(
                        RuntimeError::new("type-error", "lowered expression expected Int")
                            .with_span(*span),
                    );
                };
                slots[*slot] = LoweredValue::Int(match op {
                    AssignOp::Add => current + value,
                    AssignOp::Sub => current - value,
                    AssignOp::Mul => current * value,
                    AssignOp::Div => current / value,
                    AssignOp::Rem => current % value,
                    AssignOp::Set => unreachable!(),
                });
                Ok(LoweredStmtFlow::None)
            }
            LoweredStmt::AssignField {
                slot,
                field,
                op,
                value,
                span,
            } => {
                let value = match self.eval_lowered_expr(lowered, value, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(LoweredStmtFlow::Return(value)),
                };
                let LoweredValue::Record(record) = &mut slots[*slot] else {
                    return Err(RuntimeError::new(
                        "type-error",
                        "lowered expression expected Record",
                    )
                    .with_span(*span));
                };
                let current = record.get(field.as_ref()).cloned().ok_or_else(|| {
                    RuntimeError::new("missing-field", field.to_string()).with_span(*span)
                })?;
                let value = lowered_assign_value(*op, current, value, *span)?;
                record.insert(field.clone(), value);
                Ok(LoweredStmtFlow::None)
            }
            LoweredStmt::AssignFieldInt {
                slot,
                field,
                op,
                value,
                span,
            } => {
                let value = match self.eval_lowered_typed_int(value, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(LoweredStmtFlow::Return(value)),
                };
                let LoweredValue::Record(record) = &mut slots[*slot] else {
                    return Err(RuntimeError::new(
                        "type-error",
                        "lowered expression expected Record",
                    )
                    .with_span(*span));
                };
                let current = record.get_mut(field.as_ref()).ok_or_else(|| {
                    RuntimeError::new("missing-field", field.to_string()).with_span(*span)
                })?;
                if *op == AssignOp::Set {
                    *current = LoweredValue::Int(value);
                    return Ok(LoweredStmtFlow::None);
                }
                let LoweredValue::Int(current_value) = current else {
                    return Err(
                        RuntimeError::new("type-error", "lowered expression expected Int")
                            .with_span(*span),
                    );
                };
                *current_value = match op {
                    AssignOp::Add => *current_value + value,
                    AssignOp::Sub => *current_value - value,
                    AssignOp::Mul => *current_value * value,
                    AssignOp::Div => *current_value / value,
                    AssignOp::Rem => *current_value % value,
                    AssignOp::Set => unreachable!(),
                };
                Ok(LoweredStmtFlow::None)
            }
            LoweredStmt::AssignIndex {
                slot,
                index,
                op,
                value,
                span,
            } => {
                let key = match self.eval_lowered_expr(lowered, index, slots, call_span)? {
                    ControlFlow::Continue(value) => {
                        lowered_str_arg(&value, "indexed assignment", *span)?.to_string()
                    }
                    ControlFlow::Break(value) => return Ok(LoweredStmtFlow::Return(value)),
                };
                let value = match self.eval_lowered_expr(lowered, value, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(LoweredStmtFlow::Return(value)),
                };
                let LoweredValue::Map(map) = &mut slots[*slot] else {
                    return Err(RuntimeError::new(
                        "type-error",
                        "indexed assignment requires a map value",
                    )
                    .with_span(*span));
                };
                if *op == AssignOp::Set {
                    map.insert(key, value);
                    return Ok(LoweredStmtFlow::None);
                }
                let current = map.get(key.as_str()).cloned().ok_or_else(|| {
                    RuntimeError::new("missing-field", key.clone()).with_span(*span)
                })?;
                let value = lowered_assign_value(*op, current, value, *span)?;
                map.insert(key, value);
                Ok(LoweredStmtFlow::None)
            }
            LoweredStmt::AssignBool { slot, value } => {
                match self.eval_lowered_typed_bool(value, slots, call_span)? {
                    ControlFlow::Continue(value) => slots[*slot] = LoweredValue::Bool(value),
                    ControlFlow::Break(value) => return Ok(LoweredStmtFlow::Return(value)),
                }
                Ok(LoweredStmtFlow::None)
            }
            LoweredStmt::Expr { value, span } => {
                match self.eval_lowered_expr(lowered, value, slots, *span)? {
                    ControlFlow::Continue(value @ LoweredValue::ResultErr(_)) => {
                        let value = self.lowered_question_propagation_value(value, *span)?;
                        Ok(LoweredStmtFlow::Propagate(value))
                    }
                    ControlFlow::Continue(_) => Ok(LoweredStmtFlow::None),
                    ControlFlow::Break(value) => Ok(LoweredStmtFlow::Propagate(value)),
                }
            }
            LoweredStmt::If {
                branches,
                else_body,
            } => {
                for (condition, body) in branches {
                    let condition =
                        match self.eval_lowered_bool(lowered, condition, slots, call_span)? {
                            ControlFlow::Continue(value) => value,
                            ControlFlow::Break(value) => return Ok(LoweredStmtFlow::Return(value)),
                        };
                    if condition {
                        return self.eval_lowered_stmts(lowered, body, slots, call_span);
                    }
                }
                if let Some(body) = else_body {
                    self.eval_lowered_stmts(lowered, body, slots, call_span)
                } else {
                    Ok(LoweredStmtFlow::None)
                }
            }
            LoweredStmt::IfBool {
                branches,
                else_body,
            } => {
                for (condition, body) in branches {
                    let condition =
                        match self.eval_lowered_typed_bool(condition, slots, call_span)? {
                            ControlFlow::Continue(value) => value,
                            ControlFlow::Break(value) => return Ok(LoweredStmtFlow::Return(value)),
                        };
                    if condition {
                        return self.eval_lowered_stmts(lowered, body, slots, call_span);
                    }
                }
                if let Some(body) = else_body {
                    self.eval_lowered_stmts(lowered, body, slots, call_span)
                } else {
                    Ok(LoweredStmtFlow::None)
                }
            }
            LoweredStmt::While { condition, body } => {
                loop {
                    self.service_pending_signal(call_span)?;
                    if self.signal_state.shutdown_complete {
                        return Ok(LoweredStmtFlow::None);
                    }
                    let condition =
                        match self.eval_lowered_bool(lowered, condition, slots, call_span)? {
                            ControlFlow::Continue(value) => value,
                            ControlFlow::Break(value) => return Ok(LoweredStmtFlow::Return(value)),
                        };
                    if !condition {
                        break;
                    }
                    match self.eval_lowered_stmts(lowered, body, slots, call_span)? {
                        LoweredStmtFlow::None | LoweredStmtFlow::Continue => {}
                        LoweredStmtFlow::Break(_) => break,
                        LoweredStmtFlow::Return(value) => {
                            return Ok(LoweredStmtFlow::Return(value));
                        }
                        LoweredStmtFlow::Propagate(value) => {
                            return Ok(LoweredStmtFlow::Propagate(value));
                        }
                    }
                }
                Ok(LoweredStmtFlow::None)
            }
            LoweredStmt::WhileBool { condition, body } => {
                loop {
                    self.service_pending_signal(call_span)?;
                    if self.signal_state.shutdown_complete {
                        return Ok(LoweredStmtFlow::None);
                    }
                    let condition =
                        match self.eval_lowered_typed_bool(condition, slots, call_span)? {
                            ControlFlow::Continue(value) => value,
                            ControlFlow::Break(value) => return Ok(LoweredStmtFlow::Return(value)),
                        };
                    if !condition {
                        break;
                    }
                    match self.eval_lowered_stmts(lowered, body, slots, call_span)? {
                        LoweredStmtFlow::None | LoweredStmtFlow::Continue => {}
                        LoweredStmtFlow::Break(_) => break,
                        LoweredStmtFlow::Return(value) => {
                            return Ok(LoweredStmtFlow::Return(value));
                        }
                        LoweredStmtFlow::Propagate(value) => {
                            return Ok(LoweredStmtFlow::Propagate(value));
                        }
                    }
                }
                Ok(LoweredStmtFlow::None)
            }
            LoweredStmt::Match { value, arms, span } => {
                let value = match self.eval_lowered_expr(lowered, value, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(LoweredStmtFlow::Return(value)),
                };
                for (pattern, guard, body) in arms {
                    if lowered_pattern_matches(pattern, &value, slots) {
                        if let Some(guard_expr) = guard {
                            let guard_value = match self
                                .eval_lowered_bool(lowered, guard_expr, slots, call_span)?
                            {
                                ControlFlow::Continue(v) => v,
                                ControlFlow::Break(v) => {
                                    return Ok(LoweredStmtFlow::Return(v));
                                }
                            };
                            if !guard_value {
                                continue;
                            }
                        }
                        return self.eval_lowered_stmts(lowered, body, slots, call_span);
                    }
                }
                Err(lowered_match_no_arm(*span))
            }
            LoweredStmt::StrMatch {
                value,
                arms,
                fallback,
                span,
            } => {
                let value = match self.eval_lowered_expr(lowered, value, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(LoweredStmtFlow::Return(value)),
                };
                if let Some(key) = lowered_str_key(&value)
                    && let Some(body) = arms.get(key)
                {
                    return self.eval_lowered_stmts(lowered, body, slots, call_span);
                }
                if let Some(body) = fallback {
                    return self.eval_lowered_stmts(lowered, body, slots, call_span);
                }
                Err(lowered_match_no_arm(*span))
            }
            LoweredStmt::TagMatch {
                value,
                arms,
                fallback,
                span,
            } => {
                let value = match self.eval_lowered_expr(lowered, value, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(LoweredStmtFlow::Return(value)),
                };
                if let Some(key) = lowered_tag_key(&value)
                    && let Some(body) = arms.get(key)
                {
                    return self.eval_lowered_stmts(lowered, body, slots, call_span);
                }
                if let Some(body) = fallback {
                    return self.eval_lowered_stmts(lowered, body, slots, call_span);
                }
                Err(lowered_match_no_arm(*span))
            }
            LoweredStmt::For {
                slot,
                iter,
                body,
                span,
            } => {
                let iter = match self.eval_lowered_expr(lowered, iter, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(LoweredStmtFlow::Return(value)),
                };
                let items = self.lowered_list_items(iter, *span, "lowered for expected List")?;
                for item in items {
                    self.service_pending_signal(*span)?;
                    if self.signal_state.shutdown_complete {
                        return Ok(LoweredStmtFlow::None);
                    }
                    slots[*slot] = item;
                    match self.eval_lowered_stmts(lowered, body, slots, call_span)? {
                        LoweredStmtFlow::None | LoweredStmtFlow::Continue => {}
                        LoweredStmtFlow::Break(_) => break,
                        LoweredStmtFlow::Return(value) => {
                            return Ok(LoweredStmtFlow::Return(value));
                        }
                        LoweredStmtFlow::Propagate(value) => {
                            return Ok(LoweredStmtFlow::Propagate(value));
                        }
                    }
                }
                Ok(LoweredStmtFlow::None)
            }
            LoweredStmt::ForRecord {
                fields,
                iter,
                body,
                span,
            } => {
                let iter = match self.eval_lowered_expr(lowered, iter, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(LoweredStmtFlow::Return(value)),
                };
                let items = self.lowered_list_items(iter, *span, "lowered for expected List")?;
                for item in items {
                    self.service_pending_signal(*span)?;
                    if self.signal_state.shutdown_complete {
                        return Ok(LoweredStmtFlow::None);
                    }
                    for (name, slot) in fields {
                        let Some(value) = lowered_record_field(&item, name.as_str()) else {
                            return Err(RuntimeError::new(
                                "field-access",
                                format!("record has no field `{}`", name.as_str()),
                            )
                            .with_span(*span));
                        };
                        slots[*slot] = value.clone();
                    }
                    match self.eval_lowered_stmts(lowered, body, slots, call_span)? {
                        LoweredStmtFlow::None | LoweredStmtFlow::Continue => {}
                        LoweredStmtFlow::Break(_) => break,
                        LoweredStmtFlow::Return(value) => {
                            return Ok(LoweredStmtFlow::Return(value));
                        }
                        LoweredStmtFlow::Propagate(value) => {
                            return Ok(LoweredStmtFlow::Propagate(value));
                        }
                    }
                }
                Ok(LoweredStmtFlow::None)
            }
            LoweredStmt::ForStrLines {
                slot,
                text,
                body,
                span,
            } => {
                let text = match self.eval_lowered_expr(lowered, text, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(LoweredStmtFlow::Return(value)),
                };
                if let Some((bytes, start, end)) = lowered_bytes_parts(&text) {
                    let mut cursor = start;
                    while cursor < end {
                        let newline = memchr::memchr(b'\n', &bytes[cursor..end])
                            .map(|offset| cursor + offset);
                        let line_end = newline.unwrap_or(end);
                        let view_end = if line_end > cursor && bytes[line_end - 1] == b'\r' {
                            line_end - 1
                        } else {
                            line_end
                        };
                        self.service_pending_signal(*span)?;
                        if self.signal_state.shutdown_complete {
                            return Ok(LoweredStmtFlow::None);
                        }
                        assign_lowered_bytes_view(&mut slots[*slot], &bytes, cursor, view_end);
                        match self.eval_lowered_stmts(lowered, body, slots, call_span)? {
                            LoweredStmtFlow::None | LoweredStmtFlow::Continue => {}
                            LoweredStmtFlow::Break(_) => break,
                            LoweredStmtFlow::Return(value) => {
                                return Ok(LoweredStmtFlow::Return(value));
                            }
                            LoweredStmtFlow::Propagate(value) => {
                                return Ok(LoweredStmtFlow::Propagate(value));
                            }
                        }
                        let Some(newline) = newline else {
                            break;
                        };
                        cursor = newline + 1;
                    }
                    return Ok(LoweredStmtFlow::None);
                }
                let Some((text, start, end)) = lowered_str_parts(&text) else {
                    return Err(RuntimeError::new(
                        "type-error",
                        "lowered for lines expected Str or Bytes",
                    )
                    .with_span(*span));
                };
                let bytes = text.as_bytes();
                let mut cursor = start;
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
                    self.service_pending_signal(*span)?;
                    if self.signal_state.shutdown_complete {
                        return Ok(LoweredStmtFlow::None);
                    }
                    assign_lowered_str_view(&mut slots[*slot], &text, cursor, view_end);
                    match self.eval_lowered_stmts(lowered, body, slots, call_span)? {
                        LoweredStmtFlow::None | LoweredStmtFlow::Continue => {}
                        LoweredStmtFlow::Break(_) => break,
                        LoweredStmtFlow::Return(value) => {
                            return Ok(LoweredStmtFlow::Return(value));
                        }
                        LoweredStmtFlow::Propagate(value) => {
                            return Ok(LoweredStmtFlow::Propagate(value));
                        }
                    }
                    let Some(newline) = newline else {
                        break;
                    };
                    cursor = newline + 1;
                }
                Ok(LoweredStmtFlow::None)
            }
            LoweredStmt::Print {
                args,
                stderr,
                propagate_result,
                span,
            } => {
                let mut line = String::new();
                let mut argv = Vec::with_capacity(args.len());
                for (index, arg) in args.iter().enumerate() {
                    let value = match self.eval_lowered_expr(lowered, arg, slots, call_span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(LoweredStmtFlow::Return(value)),
                    };
                    if index > 0 {
                        line.push(' ');
                    }
                    let start = line.len();
                    push_lowered_display(&mut line, &value, *span)?;
                    if self.trace_enabled {
                        argv.push(TraceArg::text(&line[start..]));
                    }
                }
                let trace_name = if *stderr { "eprint" } else { "print" };
                self.trace_enter(
                    TraceKind::CoreCall,
                    Some(*span),
                    Some(trace_name),
                    TracePayload::Core { argv },
                );
                if *stderr {
                    self.stderr.extend_from_slice(line.as_bytes());
                    self.stderr.push(b'\n');
                } else {
                    self.stdout.extend_from_slice(line.as_bytes());
                    self.stdout.push(b'\n');
                }
                self.trace_exit(
                    TraceKind::CoreResult,
                    Some(*span),
                    Some(trace_name),
                    TracePayload::None,
                );
                if *propagate_result {
                    match self.last_status.as_ref().and_then(|status| status.code) {
                        Some(0) | None => Ok(LoweredStmtFlow::None),
                        Some(code) => Ok(LoweredStmtFlow::Propagate(LoweredValue::Int(i64::from(
                            code,
                        )))),
                    }
                } else {
                    Ok(LoweredStmtFlow::None)
                }
            }
            LoweredStmt::Cd {
                target,
                body,
                propagate_result,
                span,
            } => {
                let target = match self.eval_lowered_expr(lowered, target, slots, call_span)? {
                    ControlFlow::Continue(value) => lowered_path_like_arg(value, "cd", *span)?,
                    ControlFlow::Break(value) => return Ok(LoweredStmtFlow::Propagate(value)),
                };
                let previous = self.cwd.clone();
                let next = self.host_path(&target);
                match cap_std::fs::Dir::open_ambient_dir(&next, cap_std::ambient_authority()) {
                    Ok(_) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotADirectory => {
                        return Ok(LoweredStmtFlow::Propagate(LoweredValue::ResultErr(
                            Box::new(Value::Error(Box::new(
                                RuntimeError::new(
                                    "cwd-not-directory",
                                    "cwd target is not a directory",
                                )
                                .with_span(*span),
                            ))),
                        )));
                    }
                    Err(error) => {
                        return Ok(LoweredStmtFlow::Propagate(LoweredValue::ResultErr(
                            Box::new(Value::Error(Box::new(
                                RuntimeError::new("cwd", error.to_string()).with_span(*span),
                            ))),
                        )));
                    }
                }
                self.trace_enter(
                    TraceKind::CwdEnter,
                    Some(*span),
                    Some("cd"),
                    TracePayload::Cwd {
                        previous: TraceArg::bytes(path_bytes(&previous)),
                        current: TraceArg::bytes(path_bytes(&next)),
                    },
                );
                self.cwd = next;
                let result = self.eval_lowered_stmts(lowered, body, slots, call_span);
                let current = self.cwd.clone();
                self.cwd = previous.clone();
                self.trace_exit(
                    TraceKind::CwdExit,
                    Some(*span),
                    Some("cd"),
                    TracePayload::Cwd {
                        previous: TraceArg::bytes(path_bytes(&current)),
                        current: TraceArg::bytes(path_bytes(&previous)),
                    },
                );
                match result? {
                    LoweredStmtFlow::None => {
                        if *propagate_result {
                            Ok(LoweredStmtFlow::None)
                        } else {
                            Ok(LoweredStmtFlow::None)
                        }
                    }
                    flow @ (LoweredStmtFlow::Return(_)
                    | LoweredStmtFlow::Propagate(_)
                    | LoweredStmtFlow::Break(_)
                    | LoweredStmtFlow::Continue) => Ok(flow),
                }
            }
            LoweredStmt::Proc {
                module,
                name,
                args,
                propagate_result,
                span,
            } => {
                let Some(op) = api_spec().module_op(module, name) else {
                    return Err(
                        RuntimeError::new("unknown-method", name.to_string()).with_span(*span)
                    );
                };
                let mut values = Vec::with_capacity(args.len());
                for arg in args {
                    let value = match self.eval_lowered_expr(lowered, arg, slots, call_span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(LoweredStmtFlow::Return(value)),
                    };
                    values.push(value);
                }
                let (positionals, flags) = lowered_parse_command_values(values, *span)?;
                let result = match op {
                    RuntimeOp::FsWrite => {
                        if positionals.len() != 2 {
                            return Err(RuntimeError::new(
                                "arity",
                                "fs.write expected path and data",
                            )
                            .with_span(*span));
                        }
                        let data = lowered_bytes_or_str_owned(
                            positionals.last().cloned().expect("checked length"),
                            "fs.write",
                            *span,
                        )?;
                        let path = lowered_path_arg(
                            positionals.first().cloned().expect("checked length"),
                            "fs.write",
                            *span,
                        )?;
                        lowered_unit_result(fs_module::write_path(
                            self.host_path(&path),
                            &data,
                            *span,
                        ))
                    }
                    RuntimeOp::FsMkdir => {
                        let parents = flags.get("parents").copied().unwrap_or(true);
                        let path = lowered_path_arg(
                            positionals.first().cloned().ok_or_else(|| {
                                RuntimeError::new("arity", "fs.mkdir expected path")
                                    .with_span(*span)
                            })?,
                            "fs.mkdir",
                            *span,
                        )?;
                        lowered_unit_result(fs_module::mkdir_path(
                            self.host_path(&path),
                            parents,
                            None,
                            *span,
                        ))
                    }
                    RuntimeOp::FsRemove => {
                        let missing_ok = flags.get("missing_ok").copied().unwrap_or(false);
                        let path = lowered_path_arg(
                            positionals.first().cloned().ok_or_else(|| {
                                RuntimeError::new("arity", "fs.remove expected path")
                                    .with_span(*span)
                            })?,
                            "fs.remove",
                            *span,
                        )?;
                        lowered_unit_result(fs_module::remove_path(
                            self.host_path(&path),
                            missing_ok,
                            *span,
                        ))
                    }
                    RuntimeOp::JsonWrite => {
                        if positionals.len() != 2 {
                            return Err(RuntimeError::new(
                                "arity",
                                "json.write expected path and value",
                            )
                            .with_span(*span));
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
                            *span,
                        )?;
                        match json_module::encode_json(&value, pretty, *span) {
                            Ok(text) => lowered_unit_result(fs_module::write_path(
                                self.host_path(&path),
                                text.as_bytes(),
                                *span,
                            )),
                            Err(error) => lowered_result_err_value(error),
                        }
                    }
                    _ => {
                        return Err(RuntimeError::new(
                            "unsupported-proc-command",
                            format!("proc command syntax for {module}.{name} is not yet supported in compact lowering"),
                        ).with_span(*span));
                    }
                };
                if *propagate_result {
                    match result {
                        LoweredValue::ResultOk(_) => Ok(LoweredStmtFlow::None),
                        LoweredValue::ResultErr(error) => {
                            let kind = error.error_kind().unwrap_or("error").to_string();
                            let message = error
                                .error_message()
                                .unwrap_or("propagated error")
                                .to_string();
                            self.trace_leaf(
                                TraceKind::ResultPropagate,
                                Some(*span),
                                None,
                                TracePayload::ResultPropagate {
                                    error_kind: kind.clone(),
                                },
                            );
                            let _traceback =
                                self.pending_traceback.take().unwrap_or_else(|| Traceback {
                                    failing_span: Some(*span),
                                    operation_kind: "result.propagate".to_string(),
                                    error: TraceError { kind, message },
                                    frames: self.call_stack.clone(),
                                });
                            Ok(LoweredStmtFlow::Propagate(LoweredValue::ResultErr(error)))
                        }
                        other => Err(RuntimeError::new(
                            "type-error",
                            format!("`?` expected Result, found {}", other.type_name()),
                        )
                        .with_span(*span)),
                    }
                } else {
                    Ok(LoweredStmtFlow::None)
                }
            }
            LoweredStmt::Env {
                env,
                body,
            } => {
                for assignment in env {
                    check_env_name(assignment.name.as_str(), assignment.value.span)?;
                }
                let overlay = match self.eval_lowered_run_env(lowered, env, slots, call_span)? {
                    ControlFlow::Continue(overlay) => overlay,
                    ControlFlow::Break(value) => return Ok(LoweredStmtFlow::Propagate(value)),
                };
                let previous = self.env.clone();
                self.env.extend(overlay);
                let result = self.eval_lowered_stmts(lowered, body, slots, call_span);
                self.env = previous;
                match result? {
                    LoweredStmtFlow::None => Ok(LoweredStmtFlow::None),
                    flow @ (LoweredStmtFlow::Return(_)
                    | LoweredStmtFlow::Propagate(_)
                    | LoweredStmtFlow::Break(_)
                    | LoweredStmtFlow::Continue) => Ok(flow),
                }
            }
            LoweredStmt::Run {
                value,
                propagate_result,
            } => match self.eval_lowered_expr(lowered, value, slots, call_span)? {
                ControlFlow::Continue(value) => {
                    if *propagate_result {
                        match value {
                            LoweredValue::ResultOk(_) => Ok(LoweredStmtFlow::None),
                            value @ LoweredValue::ResultErr(_) => {
                                let value =
                                    self.lowered_question_propagation_value(value, call_span)?;
                                Ok(LoweredStmtFlow::Propagate(value))
                            }
                            other => Err(RuntimeError::new(
                                "type-error",
                                format!("`?` expected Result, found {}", other.type_name()),
                            )
                            .with_span(call_span)),
                        }
                    } else {
                        Ok(LoweredStmtFlow::None)
                    }
                }
                ControlFlow::Break(value) => Ok(LoweredStmtFlow::Propagate(value)),
            },
            LoweredStmt::Loop { body } => {
                loop {
                    let flow = self.eval_lowered_stmts(lowered, body, slots, call_span)?;
                    match flow {
                        LoweredStmtFlow::None | LoweredStmtFlow::Continue => {}
                        LoweredStmtFlow::Break(_) => break,
                        LoweredStmtFlow::Return(v) => return Ok(LoweredStmtFlow::Return(v)),
                        LoweredStmtFlow::Propagate(v) => return Ok(LoweredStmtFlow::Propagate(v)),
                    }
                }
                Ok(LoweredStmtFlow::None)
            }
            LoweredStmt::Return { value } => {
                let value = match self.eval_lowered_expr(lowered, value, slots, call_span)? {
                    ControlFlow::Continue(value) | ControlFlow::Break(value) => value,
                };
                Ok(LoweredStmtFlow::Return(value))
            }
            LoweredStmt::Yield { value } => {
                if !matches!(
                    lowered.return_kind,
                    LoweredReturnKind::Plain(LoweredType::Stream)
                ) {
                    return Err(
                        RuntimeError::new("control-flow", "yield outside stream producer")
                            .with_span(call_span),
                    );
                }
                let value = match self.eval_lowered_expr(lowered, value, slots, call_span)? {
                    ControlFlow::Continue(value) | ControlFlow::Break(value) => value,
                };
                self.stream_items.push(value.into_value());
                Ok(LoweredStmtFlow::None)
            }
            LoweredStmt::Break => Ok(LoweredStmtFlow::Break(None)),
            LoweredStmt::BreakValue { value } => {
                match self.eval_lowered_expr(lowered, value, slots, call_span)? {
                    ControlFlow::Continue(value) => Ok(LoweredStmtFlow::Break(Some(value))),
                    ControlFlow::Break(value) => Ok(LoweredStmtFlow::Propagate(value)),
                }
            }
            LoweredStmt::Continue => Ok(LoweredStmtFlow::Continue),
            LoweredStmt::Defer { .. } => Ok(LoweredStmtFlow::None),
        }
    }

    pub(super) fn eval_lowered_bool(
        &mut self,
        lowered: &LoweredPureFunction,
        expr: &LoweredExpr,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<ControlFlow<LoweredValue, bool>, RuntimeError> {
        if let Some(value) = self.eval_lowered_bool_candidate(lowered, expr, slots)? {
            return Ok(value);
        }
        match self.eval_lowered_expr(lowered, expr, slots, call_span)? {
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

    pub(super) fn eval_lowered_bool_candidate(
        &mut self,
        lowered: &LoweredPureFunction,
        expr: &LoweredExpr,
        slots: &mut [LoweredValue],
    ) -> Result<Option<ControlFlow<LoweredValue, bool>>, RuntimeError> {
        match expr {
            LoweredExpr::Bool(value) => Ok(Some(ControlFlow::Continue(*value))),
            LoweredExpr::Param(index) => match slots[*index] {
                LoweredValue::Bool(value) => Ok(Some(ControlFlow::Continue(value))),
                _ => Ok(None),
            },
            LoweredExpr::Binary {
                op,
                left,
                right,
                span: _,
            } => match op {
                BinaryOp::And => {
                    let Some(left) = self.eval_lowered_bool_candidate(lowered, left, slots)? else {
                        return Ok(None);
                    };
                    let left = match left {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(Some(ControlFlow::Break(value))),
                    };
                    if !left {
                        return Ok(Some(ControlFlow::Continue(false)));
                    }
                    self.eval_lowered_bool_candidate(lowered, right, slots)
                }
                BinaryOp::Or => {
                    let Some(left) = self.eval_lowered_bool_candidate(lowered, left, slots)? else {
                        return Ok(None);
                    };
                    let left = match left {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(Some(ControlFlow::Break(value))),
                    };
                    if left {
                        return Ok(Some(ControlFlow::Continue(true)));
                    }
                    self.eval_lowered_bool_candidate(lowered, right, slots)
                }
                BinaryOp::Eq
                | BinaryOp::Ne
                | BinaryOp::Lt
                | BinaryOp::Le
                | BinaryOp::Gt
                | BinaryOp::Ge => {
                    if matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
                        if lowered_empty_string_literal(right)
                            && let Some((slot, span)) = lowered_trim_slot(left)
                        {
                            let empty = lowered_trim_is_empty_value(&slots[slot], span)?;
                            return Ok(Some(ControlFlow::Continue(match op {
                                BinaryOp::Eq => empty,
                                BinaryOp::Ne => !empty,
                                _ => unreachable!(),
                            })));
                        }
                        if lowered_empty_string_literal(left)
                            && let Some((slot, span)) = lowered_trim_slot(right)
                        {
                            let empty = lowered_trim_is_empty_value(&slots[slot], span)?;
                            return Ok(Some(ControlFlow::Continue(match op {
                                BinaryOp::Eq => empty,
                                BinaryOp::Ne => !empty,
                                _ => unreachable!(),
                            })));
                        }
                        if let LoweredExpr::Param(slot) = left.as_ref()
                            && let Some(value) = lowered_literal_value(right)
                        {
                            let equal = slots[*slot] == value;
                            return Ok(Some(ControlFlow::Continue(match op {
                                BinaryOp::Eq => equal,
                                BinaryOp::Ne => !equal,
                                _ => unreachable!(),
                            })));
                        }
                        if let LoweredExpr::Param(slot) = right.as_ref()
                            && let Some(value) = lowered_literal_value(left)
                        {
                            let equal = slots[*slot] == value;
                            return Ok(Some(ControlFlow::Continue(match op {
                                BinaryOp::Eq => equal,
                                BinaryOp::Ne => !equal,
                                _ => unreachable!(),
                            })));
                        }
                    }
                    let Some(left) = self.eval_lowered_int_candidate(lowered, left, slots)? else {
                        return Ok(None);
                    };
                    let left = match left {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(Some(ControlFlow::Break(value))),
                    };
                    let Some(right) = self.eval_lowered_int_candidate(lowered, right, slots)?
                    else {
                        return Ok(None);
                    };
                    let right = match right {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(Some(ControlFlow::Break(value))),
                    };
                    let value = match op {
                        BinaryOp::Eq => left == right,
                        BinaryOp::Ne => left != right,
                        BinaryOp::Lt => left < right,
                        BinaryOp::Le => left <= right,
                        BinaryOp::Gt => left > right,
                        BinaryOp::Ge => left >= right,
                        _ => unreachable!(),
                    };
                    Ok(Some(ControlFlow::Continue(value)))
                }
                _ => Ok(None),
            },
            LoweredExpr::StrPredicate {
                receiver,
                predicate,
                needle,
                span,
            } => {
                let Some(needle) = lowered_needle_bytes(needle) else {
                    return Ok(None);
                };
                if let LoweredExpr::Param(slot) = receiver.as_ref() {
                    return lowered_str_predicate_text(&slots[*slot], *predicate, &needle, *span)
                        .map(ControlFlow::Continue)
                        .map(Some);
                }
                if let Some((slot, trim_span)) = lowered_trim_slot(receiver) {
                    return lowered_trim_str_predicate_value(
                        &slots[slot],
                        *predicate,
                        &needle,
                        trim_span,
                    )
                    .map(ControlFlow::Continue)
                    .map(Some);
                }
                Ok(None)
            }
            LoweredExpr::Contains {
                receiver,
                needle,
                span,
            } => {
                let LoweredExpr::Param(slot) = receiver.as_ref() else {
                    return Ok(None);
                };
                let Some(needle) = lowered_literal_value(needle) else {
                    return Ok(None);
                };
                lowered_contains_value(&slots[*slot], &needle, *span)
                    .map(ControlFlow::Continue)
                    .map(Some)
            }
            _ => Ok(None),
        }
    }

    pub(super) fn eval_lowered_int_candidate(
        &mut self,
        lowered: &LoweredPureFunction,
        expr: &LoweredExpr,
        slots: &mut [LoweredValue],
    ) -> Result<Option<ControlFlow<LoweredValue, i64>>, RuntimeError> {
        match expr {
            LoweredExpr::Int(value) => Ok(Some(ControlFlow::Continue(*value))),
            LoweredExpr::Param(index) => match slots[*index] {
                LoweredValue::Int(value) => Ok(Some(ControlFlow::Continue(value))),
                _ => Ok(None),
            },
            LoweredExpr::Binary {
                op,
                left,
                right,
                span: _,
            } if matches!(
                op,
                BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem
            ) =>
            {
                let Some(left) = self.eval_lowered_int_candidate(lowered, left, slots)? else {
                    return Ok(None);
                };
                let left = match left {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(Some(ControlFlow::Break(value))),
                };
                let Some(right) = self.eval_lowered_int_candidate(lowered, right, slots)? else {
                    return Ok(None);
                };
                let right = match right {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(Some(ControlFlow::Break(value))),
                };
                let value = match op {
                    BinaryOp::Add => left + right,
                    BinaryOp::Sub => left - right,
                    BinaryOp::Mul => left * right,
                    BinaryOp::Div => left / right,
                    BinaryOp::Rem => left % right,
                    _ => unreachable!(),
                };
                Ok(Some(ControlFlow::Continue(value)))
            }
            LoweredExpr::StrByteLen { receiver, span } => {
                if let LoweredExpr::Param(slot) = receiver.as_ref() {
                    return lowered_str_byte_len_value(&slots[*slot], *span)
                        .map(ControlFlow::Continue)
                        .map(Some);
                }
                let receiver = match self.eval_lowered_expr(lowered, receiver, slots, *span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(Some(ControlFlow::Break(value))),
                };
                lowered_str_byte_len_value(&receiver, *span)
                    .map(ControlFlow::Continue)
                    .map(Some)
            }
            LoweredExpr::StrByteAt {
                receiver,
                index,
                default,
                span,
            } => {
                let Some(index) = self.eval_lowered_int_candidate(lowered, index, slots)? else {
                    return Ok(None);
                };
                let index = match index {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(Some(ControlFlow::Break(value))),
                };
                let default = match default {
                    Some(default) => {
                        let Some(default) =
                            self.eval_lowered_int_candidate(lowered, default, slots)?
                        else {
                            return Ok(None);
                        };
                        match default {
                            ControlFlow::Continue(value) => value,
                            ControlFlow::Break(value) => {
                                return Ok(Some(ControlFlow::Break(value)));
                            }
                        }
                    }
                    None => -1,
                };
                if let LoweredExpr::Param(slot) = receiver.as_ref() {
                    return lowered_str_byte_at_value(&slots[*slot], index, default, *span)
                        .map(ControlFlow::Continue)
                        .map(Some);
                }
                let receiver = match self.eval_lowered_expr(lowered, receiver, slots, *span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(Some(ControlFlow::Break(value))),
                };
                lowered_str_byte_at_value(&receiver, index, default, *span)
                    .map(ControlFlow::Continue)
                    .map(Some)
            }
            LoweredExpr::Method {
                receiver,
                name,
                args,
                span,
            } if name == "count_lines" && args.is_empty() => {
                if let LoweredExpr::Param(slot) = receiver.as_ref() {
                    return lowered_str_count_lines_value(&slots[*slot], *span)
                        .map(ControlFlow::Continue)
                        .map(Some);
                }
                let receiver = match self.eval_lowered_expr(lowered, receiver, slots, *span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(Some(ControlFlow::Break(value))),
                };
                lowered_str_count_lines_value(&receiver, *span)
                    .map(ControlFlow::Continue)
                    .map(Some)
            }
            _ => Ok(None),
        }
    }

    pub(super) fn eval_lowered_error_expr(
        &mut self,
        lowered: &LoweredPureFunction,
        expr: &LoweredErrorExpr,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        match expr {
            LoweredErrorExpr::Simple { kind, message } => Ok(LoweredValue::Error(Box::new(
                error_constructor(kind.clone(), message.clone()),
            ))),
            LoweredErrorExpr::Structured {
                family,
                variant,
                fields,
                facets,
            } => {
                let mut payload = RecordMap::new();
                for (name, value) in fields {
                    let value = match self.eval_lowered_expr(lowered, value, slots, call_span)? {
                        ControlFlow::Continue(value) => value.into_value(),
                        ControlFlow::Break(value) => return Ok(value),
                    };
                    payload.insert(name.clone(), value);
                }
                let message = match payload.get("message") {
                    Some(Value::Str(message)) => message.to_string(),
                    _ => format!("{family}.{variant}"),
                };
                Ok(LoweredValue::Error(Box::new(structured_error_constructor(
                    family.clone(),
                    variant.clone(),
                    payload,
                    facets.iter().map(|facet| facet.to_string()).collect(),
                    message,
                ))))
            }
        }
    }

    fn eval_lowered_run_redirections(
        &mut self,
        lowered: &LoweredPureFunction,
        redirections: &[LoweredRunRedirection],
        slots: &mut [LoweredValue],
    ) -> Result<ControlFlow<LoweredValue, Vec<ProcessRedirection>>, RuntimeError> {
        let mut out = Vec::with_capacity(redirections.len());
        for redirection in redirections {
            let target = match self.eval_lowered_run_arg(
                lowered,
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
            // fd-duplication forms (`>& 2`, `<& 0`): the target is an fd number,
            // not a path.
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
                let stream = match redirection.kind {
                    RedirectionKind::StdinDup => RedirectionStream::Stdin,
                    _ => RedirectionStream::Stdout,
                };
                out.push(ProcessRedirection::Dup { stream, fd });
                continue;
            }
            let path = PathValue::new(target).map_err(|error| error.with_span(redirection.span))?;
            let stream = match redirection.kind {
                RedirectionKind::StdinRead => RedirectionStream::Stdin,
                RedirectionKind::StderrWrite | RedirectionKind::StderrAppend => {
                    RedirectionStream::Stderr
                }
                RedirectionKind::StdoutWrite | RedirectionKind::StdoutAppend => {
                    RedirectionStream::Stdout
                }
                RedirectionKind::StdoutDup | RedirectionKind::StdinDup => {
                    unreachable!("fd redirection handled above")
                }
            };
            let mode = match redirection.kind {
                RedirectionKind::StdinRead => FileRedirectionMode::Read,
                RedirectionKind::StdoutAppend | RedirectionKind::StderrAppend => {
                    FileRedirectionMode::Append
                }
                RedirectionKind::StdoutWrite | RedirectionKind::StderrWrite => {
                    FileRedirectionMode::Write
                }
                RedirectionKind::StdoutDup | RedirectionKind::StdinDup => {
                    unreachable!("fd redirection handled above")
                }
            };
            out.push(ProcessRedirection::File {
                stream,
                mode,
                path: self.host_path(&path),
            });
        }
        Ok(ControlFlow::Continue(out))
    }

    fn eval_lowered_fold_step(
        &mut self,
        lowered: &LoweredPureFunction,
        acc_slot: usize,
        item_slot: usize,
        acc: LoweredValue,
        item: LoweredValue,
        body: &[LoweredStmt],
        value: &LoweredExpr,
        slots: &mut [LoweredValue],
        span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        slots[acc_slot] = acc;
        slots[item_slot] = item;
        match self.eval_lowered_stmts(lowered, body, slots, span)? {
            LoweredStmtFlow::None => {}
            LoweredStmtFlow::Return(value) | LoweredStmtFlow::Propagate(value) => {
                return Ok(ControlFlow::Break(value));
            }
            LoweredStmtFlow::Break(_) => {
                return Err(
                    RuntimeError::new("break-outside-loop", "break used outside loop")
                        .with_span(span),
                );
            }
            LoweredStmtFlow::Continue => {
                return Err(RuntimeError::new(
                    "continue-outside-loop",
                    "continue used outside loop",
                )
                .with_span(span));
            }
        }
        self.eval_lowered_expr(lowered, value, slots, span)
    }

    fn eval_lowered_pipeline_descending(
        &mut self,
        lowered: &LoweredPureFunction,
        descending: Option<&LoweredExpr>,
        slots: &mut [LoweredValue],
        span: Span,
    ) -> Result<bool, RuntimeError> {
        let Some(descending) = descending else {
            return Ok(false);
        };
        match self.eval_lowered_expr(lowered, descending, slots, span)? {
            ControlFlow::Continue(LoweredValue::Bool(value)) => Ok(value),
            ControlFlow::Continue(value) => Err(RuntimeError::new(
                "type-error",
                format!("--desc expected Bool, found {}", value.type_name()),
            )
            .with_span(span)),
            ControlFlow::Break(value) => Err(runtime_error_from_value(value.into_value(), span)),
        }
    }

    fn eval_lowered_run_capture(
        &mut self,
        lowered: &LoweredPureFunction,
        kind: RunKind,
        target: &LoweredRunArg,
        args: &[LoweredRunArg],
        env: &[LoweredRunEnv],
        redirections: &[LoweredRunRedirection],
        timeout: Option<&LoweredExpr>,
        propagate: bool,
        assert_success: bool,
        span: Span,
        slots: &mut [LoweredValue],
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let target_items = match self.eval_lowered_run_arg(lowered, target, slots, span)? {
            ControlFlow::Continue(items) => items,
            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
        };
        let [target]: [Vec<u8>; 1] = target_items.try_into().map_err(|_| {
            RuntimeError::new("argv-conversion", "run target must produce one argv item")
                .with_span(target.span)
        })?;
        let mut argv = Vec::new();
        for arg in args {
            match self.eval_lowered_run_arg(lowered, arg, slots, span)? {
                ControlFlow::Continue(items) => argv.extend(items),
                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
            }
        }
        let env_overlay = match self.eval_lowered_run_env(lowered, env, slots, span)? {
            ControlFlow::Continue(env) => env,
            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
        };
        let redirections = match self.eval_lowered_run_redirections(lowered, redirections, slots)? {
            ControlFlow::Continue(redirections) => redirections,
            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
        };
        let timeout = match timeout {
            Some(expr) => match self.eval_lowered_expr(lowered, expr, slots, span)? {
                ControlFlow::Continue(LoweredValue::Duration(duration)) => {
                    Some(Duration::from_millis(duration.millis))
                }
                ControlFlow::Continue(other) => {
                    return Err(RuntimeError::new(
                        "type-error",
                        format!("run timeout expected Duration, found {}", other.type_name()),
                    )
                    .with_span(span));
                }
                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
            },
            None => None,
        };
        let mut full_env = self.env.snapshot_clone();
        full_env.extend(env_overlay.clone());
        let invocation = ProcessInvocation {
            target,
            argv,
            cwd: self.cwd.clone(),
            env: full_env,
            env_overlay,
            redirections,
            timeout,
            cpu_max: None,
        };
        self.trace_process_run_start(span, &invocation);
        let execution = execute_run_with_policy(
            kind,
            std::slice::from_ref(&invocation),
            span,
            assert_success,
            self,
        );
        if let Some(status) = execution.end.status.clone() {
            self.last_status = Some(status);
        }
        self.trace_process_run_end(span, &execution.end);
        if self.signal_state.shutdown_complete && self.signal_state.shutdown_status.is_some() {
            return Ok(ControlFlow::Continue(LoweredValue::ResultOk(Box::new(
                LoweredValue::Status(execution.end.status.clone().unwrap_or_else(|| {
                    crate::runtime::process::ProcessStatus::signaled(libc::SIGTERM)
                })),
            ))));
        }
        let value = execution.value?;
        let Some(mut value) = lowered_value_from_runtime_any(&value) else {
            return Err(RuntimeError::new(
                "type-error",
                format!("lowered run produced unsupported {}", value.type_name()),
            )
            .with_span(span));
        };
        if matches!(kind, RunKind::Status)
            && let LoweredValue::ResultOk(inner) = value
        {
            value = *inner;
        }
        // Plain/Status run values with `?`: a run-level failure (timeout, spawn
        // error) surfaces as a Result::Err; propagate it. On success the value
        // is a bare Status, which passes through unchanged.
        if propagate && let LoweredValue::ResultErr(_) = value {
            let propagated = self.lowered_question_propagation_value(value, span)?;
            return Ok(ControlFlow::Break(propagated));
        }
        Ok(ControlFlow::Continue(value))
    }

    fn eval_lowered_run_pipeline(
        &mut self,
        lowered: &LoweredPureFunction,
        segments: &[LoweredRunPipelineSegment],
        propagate: bool,
        span: Span,
        slots: &mut [LoweredValue],
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let mut invocations = Vec::with_capacity(segments.len());
        for segment in segments {
            let target_items =
                match self.eval_lowered_run_arg(lowered, &segment.target, slots, span)? {
                    ControlFlow::Continue(items) => items,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
            let [target]: [Vec<u8>; 1] = target_items.try_into().map_err(|_| {
                RuntimeError::new("argv-conversion", "run target must produce one argv item")
                    .with_span(segment.target.span)
            })?;
            let mut argv = Vec::new();
            for arg in &segment.args {
                match self.eval_lowered_run_arg(lowered, arg, slots, span)? {
                    ControlFlow::Continue(items) => argv.extend(items),
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                }
            }
            let env_overlay = match self.eval_lowered_run_env(lowered, &segment.env, slots, span)? {
                ControlFlow::Continue(env) => env,
                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
            };
            let redirections =
                match self.eval_lowered_run_redirections(lowered, &segment.redirections, slots)? {
                    ControlFlow::Continue(redirections) => redirections,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
            let mut full_env = self.env.snapshot_clone();
            full_env.extend(env_overlay.clone());
            invocations.push(ProcessInvocation {
                target,
                argv,
                cwd: self.cwd.clone(),
                env: full_env,
                env_overlay,
                redirections,
                timeout: None,
                cpu_max: None,
            });
        }
        self.trace_lowered_pipeline_enter(span);
        let end = match run_pipeline_inherit_with_policy(&invocations, self) {
            Ok(end) => end,
            Err(err) => {
                self.trace_lowered_pipeline_end(
                    span,
                    &ProcessEnd {
                        pid: Some(0),
                        status: err.status.as_deref().cloned(),
                        error: Some(err.clone()),
                    },
                );
                return Ok(ControlFlow::Continue(lowered_process_run_error(err)));
            }
        };
        if let Some(ref status) = end.status {
            self.last_status = Some(status.clone());
        }
        self.trace_lowered_pipeline_end(span, &end);
        if self.signal_state.shutdown_complete && self.signal_state.shutdown_status.is_some() {
            return Ok(ControlFlow::Continue(LoweredValue::ResultOk(Box::new(
                LoweredValue::Status(end.status.clone().unwrap_or_else(|| {
                    crate::runtime::process::ProcessStatus::signaled(libc::SIGTERM)
                })),
            ))));
        }
        let status = end
            .status
            .clone()
            .unwrap_or_else(|| crate::runtime::process::ProcessStatus::exited(1));
        let success = status.success;
        if !success && propagate {
            return Ok(ControlFlow::Continue(lowered_process_run_error(
                RunError::from_status(status).with_span(span),
            )));
        }
        if propagate {
            Ok(ControlFlow::Continue(LoweredValue::ResultOk(Box::new(
                LoweredValue::Status(status),
            ))))
        } else {
            Ok(ControlFlow::Continue(LoweredValue::Status(status)))
        }
    }

    fn eval_lowered_process_command_builder(
        &mut self,
        lowered: &LoweredPureFunction,
        entries: &[LoweredProcessCommandBuilderEntry],
        span: Span,
        slots: &mut [LoweredValue],
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        self.trace_enter(
            TraceKind::ModuleCall,
            Some(span),
            Some("process.command"),
            TracePayload::None,
        );
        let mut plan = None;
        let mut cwd = None;
        let mut env = BTreeMap::new();
        let mut timeout = None;
        let mut cpu_max = None;
        let mut detach = None;
        let mut new_session = None;
        let mut ignore_hup = None;
        for entry in entries {
            match entry {
                LoweredProcessCommandBuilderEntry::Field { name, value, span } => {
                    let value = match self.eval_lowered_expr(lowered, value, slots, *span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    };
                    match name.as_str() {
                        "cwd" => {
                            cwd = Some(lowered_path_like_arg(value, "process.command", *span)?)
                        }
                        "env" => {
                            env.extend(lowered_env_record_arg(value, "process.command", *span)?)
                        }
                        "timeout" => {
                            timeout =
                                Some(lowered_duration_arg(Some(value), "process.command", *span)?)
                        }
                        "cpu_max" => {
                            let value = lowered_int_arg(Some(value), "process.command", *span)?;
                            if value <= 0 {
                                return Err(RuntimeError::new(
                                    "cpu-max",
                                    "cpu_max must be positive",
                                )
                                .with_span(*span));
                            }
                            cpu_max = Some(value);
                        }
                        "detach" => {
                            detach = Some(lowered_bool_builder_field(value, "detach", *span)?)
                        }
                        "new_session" => {
                            new_session =
                                Some(lowered_bool_builder_field(value, "new_session", *span)?)
                        }
                        "ignore_hup" => {
                            ignore_hup =
                                Some(lowered_bool_builder_field(value, "ignore_hup", *span)?)
                        }
                        _ => {
                            return Err(RuntimeError::new(
                                "builder-field",
                                format!("unknown process.command field `{name}`"),
                            )
                            .with_span(*span));
                        }
                    }
                }
                LoweredProcessCommandBuilderEntry::Run {
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
                        .with_span(*span));
                    }
                    let target_items =
                        match self.eval_lowered_run_arg(lowered, target, slots, *span)? {
                            ControlFlow::Continue(items) => items,
                            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                        };
                    let [target]: [Vec<u8>; 1] = target_items.try_into().map_err(|_| {
                        RuntimeError::new(
                            "argv-conversion",
                            "run target must produce one argv item",
                        )
                        .with_span(target.span)
                    })?;
                    let mut argv = Vec::new();
                    for arg in args {
                        match self.eval_lowered_run_arg(lowered, arg, slots, *span)? {
                            ControlFlow::Continue(items) => argv.extend(items),
                            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                        }
                    }
                    let env_overlay =
                        match self.eval_lowered_run_env(lowered, run_env, slots, *span)? {
                            ControlFlow::Continue(env) => env,
                            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                        };
                    let mut run_env = BTreeMap::new();
                    for (name, value) in env_overlay {
                        run_env.insert(
                            String::from_utf8_lossy(&name).into_owned(),
                            String::from_utf8_lossy(&value).into_owned(),
                        );
                    }
                    let run_timeout = match run_timeout {
                        Some(value) => {
                            match self.eval_lowered_expr(lowered, value, slots, *span)? {
                                ControlFlow::Continue(value) => Some(lowered_duration_arg(
                                    Some(value),
                                    "process.command",
                                    *span,
                                )?),
                                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                            }
                        }
                        None => None,
                    };
                    let run_cpu_max = match run_cpu_max {
                        Some(value) => {
                            match self.eval_lowered_expr(lowered, value, slots, *span)? {
                                ControlFlow::Continue(value) => {
                                    let value =
                                        lowered_int_arg(Some(value), "process.command", *span)?;
                                    if value <= 0 {
                                        return Err(RuntimeError::new(
                                            "cpu-max",
                                            "cpu_max must be positive",
                                        )
                                        .with_span(*span));
                                    }
                                    Some(value)
                                }
                                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                            }
                        }
                        None => None,
                    };
                    plan = Some(CommandPlan {
                        target,
                        argv,
                        cwd: None,
                        env: run_env,
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
        if timeout.is_some() {
            plan.timeout = timeout;
        }
        if cpu_max.is_some() {
            plan.cpu_max = cpu_max;
        }
        if let Some(detach) = detach {
            plan.detach = detach;
        }
        if let Some(new_session) = new_session {
            plan.new_session = new_session;
        }
        if let Some(ignore_hup) = ignore_hup {
            plan.ignore_hup = ignore_hup;
        }
        self.trace_exit(
            TraceKind::ModuleResult,
            Some(span),
            Some("process.command"),
            TracePayload::None,
        );
        Ok(ControlFlow::Continue(LoweredValue::Command(plan)))
    }

    fn eval_lowered_spawn_run(
        &mut self,
        lowered: &LoweredPureFunction,
        target: &LoweredRunArg,
        args: &[LoweredRunArg],
        env: &[LoweredRunEnv],
        redirections: &[LoweredRunRedirection],
        span: Span,
        slots: &mut [LoweredValue],
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let target_items = match self.eval_lowered_run_arg(lowered, target, slots, span)? {
            ControlFlow::Continue(items) => items,
            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
        };
        let [target]: [Vec<u8>; 1] = target_items.try_into().map_err(|_| {
            RuntimeError::new("argv-conversion", "spawn target must produce one argv item")
                .with_span(target.span)
        })?;
        let mut argv = Vec::new();
        for arg in args {
            match self.eval_lowered_run_arg(lowered, arg, slots, span)? {
                ControlFlow::Continue(items) => argv.extend(items),
                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
            }
        }
        let env_overlay = match self.eval_lowered_run_env(lowered, env, slots, span)? {
            ControlFlow::Continue(env) => env,
            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
        };
        let redirections = match self.eval_lowered_run_redirections(lowered, redirections, slots)? {
            ControlFlow::Continue(redirections) => redirections,
            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
        };
        let mut full_env = self.env.snapshot_clone();
        full_env.extend(env_overlay.clone());
        let invocation = ProcessInvocation {
            target,
            argv,
            cwd: self.cwd.clone(),
            env: full_env,
            env_overlay,
            redirections,
            timeout: None,
            cpu_max: None,
        };
        self.eval_lowered_spawn_invocation(invocation, SpawnOptions::default(), span)
    }

    fn eval_lowered_spawn_command(
        &mut self,
        lowered: &LoweredPureFunction,
        command: &LoweredExpr,
        span: Span,
        slots: &mut [LoweredValue],
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let command = match self.eval_lowered_expr(lowered, command, slots, span)? {
            ControlFlow::Continue(command) => command,
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
        self.eval_lowered_spawn_invocation(invocation, options, span)
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

    fn eval_lowered_wait(
        &mut self,
        lowered: &LoweredPureFunction,
        target: &LoweredExpr,
        span: Span,
        slots: &mut [LoweredValue],
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let target = match self.eval_lowered_expr(lowered, target, slots, span)? {
            ControlFlow::Continue(value) => value.into_value(),
            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
        };
        let value = match target {
            Value::ProcessHandle(handle) => self.wait_one_process_handle(*handle, span)?,
            Value::List(items) => self.wait_process_handle_list(items, span)?,
            value => super::process_handle::process_handle_error(
                crate::runtime::value::RunError::new(
                    "unknown",
                    format!(
                        "wait expected ProcessHandle or List[ProcessHandle], found {}",
                        value.type_name()
                    ),
                )
                .with_span(span),
            ),
        };
        let Some(value) = lowered_value_from_runtime_any(&value) else {
            return Err(
                RuntimeError::new("type-error", "lowered wait produced unsupported value")
                    .with_span(span),
            );
        };
        Ok(ControlFlow::Continue(value))
    }

    fn eval_lowered_run_env(
        &mut self,
        lowered: &LoweredPureFunction,
        env: &[LoweredRunEnv],
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<ControlFlow<LoweredValue, BTreeMap<Vec<u8>, Vec<u8>>>, RuntimeError> {
        let mut overlay = BTreeMap::new();
        for assignment in env {
            let items =
                match self.eval_lowered_run_arg(lowered, &assignment.value, slots, call_span)? {
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

    fn eval_lowered_run_arg(
        &mut self,
        lowered: &LoweredPureFunction,
        arg: &LoweredRunArg,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<ControlFlow<LoweredValue, Vec<Vec<u8>>>, RuntimeError> {
        match &arg.kind {
            LoweredRunArgKind::Single(expr) => {
                let value = match self.eval_lowered_expr(lowered, expr, slots, call_span)? {
                    ControlFlow::Continue(value) => value.into_value(),
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                Ok(ControlFlow::Continue(vec![value_to_argv_bytes(
                    value, arg.span,
                )?]))
            }
            LoweredRunArgKind::SingleOrSplice(expr) => {
                let value = match self.eval_lowered_expr(lowered, expr, slots, call_span)? {
                    ControlFlow::Continue(value) => value.into_value(),
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                match value {
                    Value::List(_) => splice_to_argv(value, arg.span).map(ControlFlow::Continue),
                    value => Ok(ControlFlow::Continue(vec![value_to_argv_bytes(
                        value, arg.span,
                    )?])),
                }
            }
            LoweredRunArgKind::Splice(expr) => {
                let value = match self.eval_lowered_expr(lowered, expr, slots, call_span)? {
                    ControlFlow::Continue(value) => value.into_value(),
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                splice_to_argv(value, arg.span).map(ControlFlow::Continue)
            }
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
        match receiver {
            LoweredValue::Stream(stream) if name == "collect" && values.is_empty() => {
                let values = self.collect_stream_values(stream, *span)?;
                let value = lowered_runtime_value(Value::List(values), *span)?;
                Ok(ControlFlow::Continue(value))
            }
            receiver => {
                lowered_method_value(receiver, name, values, *span).map(ControlFlow::Continue)
            }
        }
    }

    pub(super) fn eval_lowered_expr(
        &mut self,
        lowered: &LoweredPureFunction,
        expr: &LoweredExpr,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        match expr {
            LoweredExpr::Null => Ok(ControlFlow::Continue(LoweredValue::Null)),
            LoweredExpr::Unit => Ok(ControlFlow::Continue(LoweredValue::Unit)),
            LoweredExpr::Int(value) => Ok(ControlFlow::Continue(LoweredValue::Int(*value))),
            LoweredExpr::Float(value) => Ok(ControlFlow::Continue(LoweredValue::Float(*value))),
            LoweredExpr::Duration(value) => {
                Ok(ControlFlow::Continue(LoweredValue::Duration(value.clone())))
            }
            LoweredExpr::Bool(value) => Ok(ControlFlow::Continue(LoweredValue::Bool(*value))),
            LoweredExpr::Str(value) => Ok(ControlFlow::Continue(LoweredValue::Str(value.clone()))),
            LoweredExpr::Bytes(value) => {
                Ok(ControlFlow::Continue(LoweredValue::Bytes(value.clone())))
            }
            LoweredExpr::Path(value) => {
                Ok(ControlFlow::Continue(LoweredValue::Path(value.clone())))
            }
            LoweredExpr::FunctionRef { function, pure } => {
                if *pure {
                    Ok(ControlFlow::Continue(LoweredValue::Pure(*function)))
                } else {
                    Ok(ControlFlow::Continue(LoweredValue::Proc(*function)))
                }
            }
            LoweredExpr::PathFrom { value, span } => {
                let value = match self.eval_lowered_expr(lowered, value, slots, *span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                lowered_path_from_value(value, "Path", *span)
                    .map(LoweredValue::Path)
                    .map(ControlFlow::Continue)
            }
            LoweredExpr::Param(index) => Ok(ControlFlow::Continue(slots[*index].clone())),
            LoweredExpr::Binary {
                op,
                left,
                right,
                span,
            } => {
                if *op == BinaryOp::And {
                    let left = match self.eval_lowered_bool(lowered, left, slots, *span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    };
                    if !left {
                        return Ok(ControlFlow::Continue(LoweredValue::Bool(false)));
                    }
                    return match self.eval_lowered_bool(lowered, right, slots, *span)? {
                        ControlFlow::Continue(value) => {
                            Ok(ControlFlow::Continue(LoweredValue::Bool(value)))
                        }
                        ControlFlow::Break(value) => Ok(ControlFlow::Break(value)),
                    };
                }
                if *op == BinaryOp::Or {
                    let left = match self.eval_lowered_bool(lowered, left, slots, *span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    };
                    if left {
                        return Ok(ControlFlow::Continue(LoweredValue::Bool(true)));
                    }
                    return match self.eval_lowered_bool(lowered, right, slots, *span)? {
                        ControlFlow::Continue(value) => {
                            Ok(ControlFlow::Continue(LoweredValue::Bool(value)))
                        }
                        ControlFlow::Break(value) => Ok(ControlFlow::Break(value)),
                    };
                }
                let left = match self.eval_lowered_expr(lowered, left, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let right = match self.eval_lowered_expr(lowered, right, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                lowered_binary_value(*op, left, right, *span).map(ControlFlow::Continue)
            }
            LoweredExpr::IfExpr {
                branches,
                else_value,
                span,
            } => {
                for (condition, value) in branches {
                    let condition =
                        match self.eval_lowered_bool(lowered, condition, slots, *span)? {
                            ControlFlow::Continue(value) => value,
                            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                        };
                    if condition {
                        return self.eval_lowered_expr(lowered, value, slots, call_span);
                    }
                }
                self.eval_lowered_expr(lowered, else_value, slots, call_span)
            }
            LoweredExpr::MatchExpr { value, arms, span } => {
                let value = match self.eval_lowered_expr(lowered, value, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                for (pattern, guard, arm_value) in arms {
                    if lowered_pattern_matches(pattern, &value, slots) {
                        if let Some(guard_expr) = guard {
                            match self.eval_lowered_expr(lowered, guard_expr, slots, call_span)? {
                                ControlFlow::Continue(guard_val) => {
                                    if guard_val != LoweredValue::Bool(true) {
                                        continue;
                                    }
                                }
                                ControlFlow::Break(v) => return Ok(ControlFlow::Break(v)),
                            }
                        }
                        return self.eval_lowered_expr(lowered, arm_value, slots, call_span);
                    }
                }
                Err(lowered_match_no_arm(*span))
            }
            LoweredExpr::StrMatchExpr {
                value,
                arms,
                fallback,
                span,
            } => {
                let value = match self.eval_lowered_expr(lowered, value, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                if let Some(key) = lowered_str_key(&value)
                    && let Some(arm_value) = arms.get(key)
                {
                    return self.eval_lowered_expr(lowered, arm_value, slots, call_span);
                }
                if let Some(fallback) = fallback {
                    return self.eval_lowered_expr(lowered, fallback, slots, call_span);
                }
                Err(lowered_match_no_arm(*span))
            }
            LoweredExpr::TagMatchExpr {
                value,
                arms,
                fallback,
                span,
            } => {
                let value = match self.eval_lowered_expr(lowered, value, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                if let Some(key) = lowered_tag_key(&value)
                    && let Some(arm_value) = arms.get(key)
                {
                    return self.eval_lowered_expr(lowered, arm_value, slots, call_span);
                }
                if let Some(fallback) = fallback {
                    return self.eval_lowered_expr(lowered, fallback, slots, call_span);
                }
                Err(lowered_match_no_arm(*span))
            }
            LoweredExpr::ResultFallback { left, right } => {
                match self.eval_lowered_expr(lowered, left, slots, call_span)? {
                    ControlFlow::Continue(LoweredValue::ResultOk(value)) => {
                        Ok(ControlFlow::Continue(*value))
                    }
                    ControlFlow::Continue(LoweredValue::ResultErr(_)) => {
                        self.eval_lowered_expr(lowered, right, slots, call_span)
                    }
                    ControlFlow::Continue(LoweredValue::Null) => {
                        self.eval_lowered_expr(lowered, right, slots, call_span)
                    }
                    ControlFlow::Continue(value) => Ok(ControlFlow::Continue(value)),
                    ControlFlow::Break(value) => Ok(ControlFlow::Break(value)),
                }
            }
            LoweredExpr::FmtString(parts) => {
                let mut text = String::new();
                for part in parts {
                    match part {
                        LoweredFmtPart::Text(part) => text.push_str(part),
                        LoweredFmtPart::Expr(expr, span, spec) => {
                            let value = match self
                                .eval_lowered_expr(lowered, expr, slots, call_span)?
                            {
                                ControlFlow::Continue(value) => value,
                                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                            };
                            push_lowered_fmt_value(&mut text, &value, *span, spec.as_ref())?;
                        }
                    }
                }
                Ok(ControlFlow::Continue(LoweredValue::Str(text.into())))
            }
            LoweredExpr::PathFmtString { parts, span } => {
                let mut text = String::new();
                for part in parts {
                    match part {
                        LoweredFmtPart::Text(part) => text.push_str(part),
                        LoweredFmtPart::Expr(expr, span, spec) => {
                            let value = match self
                                .eval_lowered_expr(lowered, expr, slots, call_span)?
                            {
                                ControlFlow::Continue(value) => value,
                                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                            };
                            push_lowered_fmt_value(&mut text, &value, *span, spec.as_ref())?;
                        }
                    }
                }
                PathValue::from_text(text)
                    .map(LoweredValue::Path)
                    .map(ControlFlow::Continue)
                    .map_err(|error| error.with_span(*span))
            }
            LoweredExpr::Glob { pattern, span } => {
                let matches = super::expand_glob_pattern(&self.cwd, pattern, *span)?;
                let mut values = Vec::with_capacity(matches.len());
                for bytes in matches {
                    values.push(LoweredValue::Path(
                        PathValue::new(bytes).map_err(|error| error.with_span(*span))?,
                    ));
                }
                Ok(ControlFlow::Continue(LoweredValue::List(values)))
            }
            LoweredExpr::LastStatus { span } => match self.last_status.clone() {
                Some(status) => Ok(ControlFlow::Continue(LoweredValue::Status(status))),
                None => Err(RuntimeError::new("last-status", "`$?` is not set").with_span(*span)),
            },
            LoweredExpr::Record(fields) => {
                let mut record = BTreeMap::new();
                for entry in fields {
                    match entry {
                        LoweredRecordEntry::Field(name, expr) => {
                            let value = match self
                                .eval_lowered_expr(lowered, expr, slots, call_span)?
                            {
                                ControlFlow::Continue(value) => value,
                                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                            };
                            record.insert(name.clone(), value);
                        }
                        LoweredRecordEntry::Spread(expr) => {
                            let value = match self
                                .eval_lowered_expr(lowered, expr, slots, call_span)?
                            {
                                ControlFlow::Continue(value) => value,
                                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                            };
                            let LoweredValue::Record(fields) = value else {
                                return Err(RuntimeError::new(
                                    "type-error",
                                    format!(
                                        "record spread expected Record, found {}",
                                        value.type_name()
                                    ),
                                )
                                .with_span(call_span));
                            };
                            record.extend(fields);
                        }
                    }
                }
                Ok(ControlFlow::Continue(LoweredValue::Record(record)))
            }
            LoweredExpr::List(items) => {
                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    let value = match self.eval_lowered_expr(lowered, item, slots, call_span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    };
                    values.push(value);
                }
                Ok(ControlFlow::Continue(LoweredValue::List(values)))
            }
            LoweredExpr::EmptyMap => Ok(ControlFlow::Continue(LoweredValue::Map(BTreeMap::new()))),
            LoweredExpr::BytesConcat { arg, span } => {
                let value = match self.eval_lowered_expr(lowered, arg, slots, *span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let LoweredValue::List(items) = value else {
                    return Err(RuntimeError::new(
                        "type-error",
                        "bytes.concat expected List[Bytes]",
                    )
                    .with_span(*span));
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
                        .with_span(*span));
                    };
                    out.extend_from_slice(bytes);
                }
                Ok(ControlFlow::Continue(LoweredValue::Bytes(Arc::from(out))))
            }
            LoweredExpr::Range { start, end, span } => {
                let start = match self.eval_lowered_expr(lowered, start, slots, *span)? {
                    ControlFlow::Continue(LoweredValue::Int(value)) => value,
                    ControlFlow::Continue(value) => {
                        return Err(RuntimeError::new(
                            "type-error",
                            format!("range start expected Int, found {}", value.type_name()),
                        )
                        .with_span(*span));
                    }
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let end = match self.eval_lowered_expr(lowered, end, slots, *span)? {
                    ControlFlow::Continue(LoweredValue::Int(value)) => value,
                    ControlFlow::Continue(value) => {
                        return Err(RuntimeError::new(
                            "type-error",
                            format!("range end expected Int, found {}", value.type_name()),
                        )
                        .with_span(*span));
                    }
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let values = if start <= end {
                    (start..end).map(LoweredValue::Int).collect()
                } else {
                    (end + 1..=start).rev().map(LoweredValue::Int).collect()
                };
                Ok(ControlFlow::Continue(LoweredValue::List(values)))
            }
            LoweredExpr::Tag { name, fields } => {
                let mut values = Vec::with_capacity(fields.len());
                for field in fields {
                    let value = match self.eval_lowered_expr(lowered, field, slots, call_span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    };
                    values.push(value);
                }
                Ok(ControlFlow::Continue(LoweredValue::Tag {
                    name: name.clone(),
                    fields: values,
                }))
            }
            LoweredExpr::ListComp {
                value,
                target,
                iter,
                condition,
                span,
            } => {
                let iter = match self.eval_lowered_expr(lowered, iter, slots, *span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let items =
                    self.lowered_list_items(iter, *span, "list comprehension expected List")?;
                let mut values = Vec::new();
                for item in items {
                    bind_lowered_comp_target(target, item, slots, *span)?;
                    let include = match condition {
                        Some(condition) => {
                            match self.eval_lowered_bool(lowered, condition, slots, *span)? {
                                ControlFlow::Continue(value) => value,
                                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                            }
                        }
                        None => true,
                    };
                    if include {
                        let value = match self.eval_lowered_expr(lowered, value, slots, *span)? {
                            ControlFlow::Continue(value) => value,
                            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                        };
                        values.push(value);
                    }
                }
                Ok(ControlFlow::Continue(LoweredValue::List(values)))
            }
            LoweredExpr::MapComp {
                key,
                value,
                target,
                iter,
                condition,
                span,
            } => {
                let iter = match self.eval_lowered_expr(lowered, iter, slots, *span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let items =
                    self.lowered_list_items(iter, *span, "map comprehension expected List")?;
                let mut values = BTreeMap::new();
                for item in items {
                    bind_lowered_comp_target(target, item, slots, *span)?;
                    let include = match condition {
                        Some(condition) => {
                            match self.eval_lowered_bool(lowered, condition, slots, *span)? {
                                ControlFlow::Continue(value) => value,
                                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                            }
                        }
                        None => true,
                    };
                    if include {
                        let key = match self.eval_lowered_expr(lowered, key, slots, *span)? {
                            ControlFlow::Continue(value) => value,
                            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                        };
                        let Some(key) = lowered_str_value(&key) else {
                            return Err(RuntimeError::new(
                                "type-error",
                                "map comprehension key expected Str",
                            )
                            .with_span(*span));
                        };
                        let value = match self.eval_lowered_expr(lowered, value, slots, *span)? {
                            ControlFlow::Continue(value) => value,
                            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                        };
                        values.insert(key.to_string(), value);
                    }
                }
                Ok(ControlFlow::Continue(LoweredValue::Map(values)))
            }
            LoweredExpr::ListPipeline {
                input,
                stages,
                span,
            } => {
                let current = match self.eval_lowered_expr(lowered, input, slots, *span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let mut current = lowered_pipeline_input(current, *span)?;
                for stage in stages {
                    let stage_name = stage.trace_name();
                    self.trace_enter(
                        TraceKind::StreamStageEnter,
                        Some(*span),
                        Some(stage_name),
                        TracePayload::StreamStage {
                            stage: stage_name.to_string(),
                            item_count: lowered_pipeline_item_count(&current),
                            error: None,
                        },
                    );
                    match stage {
                        LoweredPipelineStage::TextLines => {
                            let Some((text, start, end)) = lowered_str_parts(&current) else {
                                return Err(RuntimeError::new(
                                    "type-error",
                                    format!(
                                        "text.lines expected Str, found {}",
                                        current.type_name()
                                    ),
                                )
                                .with_span(*span));
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
                                lines.push(LoweredValue::StrView(LoweredStrView::new(
                                    text.clone(),
                                    cursor,
                                    view_end,
                                )));
                                let Some(newline) = newline else {
                                    break;
                                };
                                cursor = newline + 1;
                            }
                            current = LoweredValue::List(lines);
                        }
                        LoweredPipelineStage::JsonLines => {
                            let Some(text) = lowered_str_value(&current) else {
                                return Err(RuntimeError::new(
                                    "type-error",
                                    format!(
                                        "json.lines expected Str, found {}",
                                        current.type_name()
                                    ),
                                )
                                .with_span(*span));
                            };
                            let values = crate::modules::json::parse_json_lines(text, *span)?;
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
                                    .with_span(*span));
                                };
                                lowered.push(value);
                            }
                            current = LoweredValue::List(lowered);
                        }
                        LoweredPipelineStage::Enumerate => {
                            let mut items = self.lowered_pipeline_input_items(current, *span)?;
                            items = items
                                .into_iter()
                                .enumerate()
                                .map(|(index, value)| {
                                    LoweredValue::Record(btree_map(vec![
                                        (Arc::from("index"), LoweredValue::Int(index as i64)),
                                        (Arc::from("value"), value),
                                    ]))
                                })
                                .collect();
                            current = LoweredValue::List(items);
                        }
                        LoweredPipelineStage::Zip { other } => {
                            let left = self.lowered_pipeline_input_items(current, *span)?;
                            let other = match self
                                .eval_lowered_expr(lowered, other, slots, *span)?
                            {
                                ControlFlow::Continue(value) => value,
                                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                            };
                            let right =
                                self.lowered_list_items(other, *span, "zip expected List")?;
                            current = LoweredValue::List(
                                left.into_iter()
                                    .zip(right)
                                    .map(|(left, right)| {
                                        LoweredValue::Record(btree_map(vec![
                                            (Arc::from("left"), left),
                                            (Arc::from("right"), right),
                                        ]))
                                    })
                                    .collect(),
                            );
                        }
                        LoweredPipelineStage::Sort { descending } => {
                            let mut items = self.lowered_pipeline_input_items(current, *span)?;
                            items.sort_unstable_by(compare_lowered_sort_keys);
                            if self.eval_lowered_pipeline_descending(
                                lowered,
                                descending.as_ref(),
                                slots,
                                *span,
                            )? {
                                items.reverse();
                            }
                            current = LoweredValue::List(items);
                        }
                        LoweredPipelineStage::SortBy {
                            slot,
                            key,
                            descending,
                        } => {
                            let items = self.lowered_pipeline_input_items(current, *span)?;
                            let mut keyed = Vec::with_capacity(items.len());
                            for item in items {
                                slots[*slot] = item;
                                let key =
                                    match self.eval_lowered_expr(lowered, key, slots, *span)? {
                                        ControlFlow::Continue(value) => value,
                                        ControlFlow::Break(value) => {
                                            return Ok(ControlFlow::Break(value));
                                        }
                                    };
                                let item = std::mem::replace(&mut slots[*slot], LoweredValue::Unit);
                                keyed.push((key, item));
                            }
                            keyed.sort_unstable_by(|(left, _), (right, _)| {
                                compare_lowered_sort_keys(left, right)
                            });
                            if self.eval_lowered_pipeline_descending(
                                lowered,
                                descending.as_ref(),
                                slots,
                                *span,
                            )? {
                                keyed.reverse();
                            }
                            current = LoweredValue::List(
                                keyed.into_iter().map(|(_, item)| item).collect(),
                            );
                        }
                        LoweredPipelineStage::GroupBy { slot, key } => {
                            let items = self.lowered_pipeline_input_items(current, *span)?;
                            let mut groups: Vec<(LoweredValue, Vec<LoweredValue>)> = Vec::new();
                            for item in items {
                                slots[*slot] = item;
                                let key =
                                    match self.eval_lowered_expr(lowered, key, slots, *span)? {
                                        ControlFlow::Continue(value) => value,
                                        ControlFlow::Break(value) => {
                                            return Ok(ControlFlow::Break(value));
                                        }
                                    };
                                let item = std::mem::replace(&mut slots[*slot], LoweredValue::Unit);
                                if let Some((_, group_items)) =
                                    groups.iter_mut().find(|(existing, _)| existing == &key)
                                {
                                    group_items.push(item);
                                } else {
                                    groups.push((key, vec![item]));
                                }
                            }
                            current = LoweredValue::List(
                                groups
                                    .into_iter()
                                    .map(|(key, items)| {
                                        LoweredValue::Record(btree_map(vec![
                                            (Arc::from("items"), LoweredValue::List(items)),
                                            (Arc::from("key"), key),
                                        ]))
                                    })
                                    .collect(),
                            );
                        }
                        LoweredPipelineStage::CountBy { slot, key } => {
                            let items = self.lowered_pipeline_input_items(current, *span)?;
                            let mut counts = BTreeMap::new();
                            for item in items {
                                slots[*slot] = item;
                                let key =
                                    match self.eval_lowered_expr(lowered, key, slots, *span)? {
                                        ControlFlow::Continue(value) => {
                                            lowered_count_key(&value, *span)?
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
                            current = LoweredValue::Map(counts);
                        }
                        LoweredPipelineStage::UniqueBy { slot, key } => {
                            let items = self.lowered_pipeline_input_items(current, *span)?;
                            let mut seen = Vec::new();
                            let mut unique = Vec::with_capacity(items.len());
                            for item in items {
                                slots[*slot] = item;
                                let key =
                                    match self.eval_lowered_expr(lowered, key, slots, *span)? {
                                        ControlFlow::Continue(value) => value,
                                        ControlFlow::Break(value) => {
                                            return Ok(ControlFlow::Break(value));
                                        }
                                    };
                                let item = std::mem::replace(&mut slots[*slot], LoweredValue::Unit);
                                if !seen.iter().any(|existing| existing == &key) {
                                    seen.push(key);
                                    unique.push(item);
                                }
                            }
                            current = LoweredValue::List(unique);
                        }
                        LoweredPipelineStage::Where { slot, predicate } => {
                            let items = self.lowered_pipeline_input_items(current, *span)?;
                            let mut filtered = Vec::new();
                            for item in items {
                                slots[*slot] = item;
                                let keep = match self
                                    .eval_lowered_bool(lowered, predicate, slots, *span)?
                                {
                                    ControlFlow::Continue(value) => value,
                                    ControlFlow::Break(value) => {
                                        return Ok(ControlFlow::Break(value));
                                    }
                                };
                                let item = std::mem::replace(&mut slots[*slot], LoweredValue::Unit);
                                if keep {
                                    filtered.push(item);
                                }
                            }
                            current = LoweredValue::List(filtered);
                        }
                        LoweredPipelineStage::Any { slot, predicate } => {
                            let items = self.lowered_pipeline_input_items(current, *span)?;
                            let mut matched = false;
                            for item in items {
                                slots[*slot] = item;
                                let keep = match self
                                    .eval_lowered_bool(lowered, predicate, slots, *span)?
                                {
                                    ControlFlow::Continue(value) => value,
                                    ControlFlow::Break(value) => {
                                        return Ok(ControlFlow::Break(value));
                                    }
                                };
                                slots[*slot] = LoweredValue::Unit;
                                if keep {
                                    matched = true;
                                    break;
                                }
                            }
                            current = LoweredValue::Bool(matched);
                        }
                        LoweredPipelineStage::All { slot, predicate } => {
                            let items = self.lowered_pipeline_input_items(current, *span)?;
                            let mut matched = true;
                            for item in items {
                                slots[*slot] = item;
                                let keep = match self
                                    .eval_lowered_bool(lowered, predicate, slots, *span)?
                                {
                                    ControlFlow::Continue(value) => value,
                                    ControlFlow::Break(value) => {
                                        return Ok(ControlFlow::Break(value));
                                    }
                                };
                                slots[*slot] = LoweredValue::Unit;
                                if !keep {
                                    matched = false;
                                    break;
                                }
                            }
                            current = LoweredValue::Bool(matched);
                        }
                        LoweredPipelineStage::Map { slot, value } => {
                            let items = self.lowered_pipeline_input_items(current, *span)?;
                            let mut mapped = Vec::with_capacity(items.len());
                            for (index, item) in items.into_iter().enumerate() {
                                slots[*slot] = item;
                                let value =
                                    match self.eval_lowered_expr(lowered, value, slots, *span) {
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
                            current = LoweredValue::List(mapped);
                        }
                        LoweredPipelineStage::MapBlock { slot, body, value } => {
                            let items = self.lowered_pipeline_input_items(current, *span)?;
                            let mut mapped = Vec::with_capacity(items.len());
                            for item in items {
                                slots[*slot] = item;
                                match self.eval_lowered_stmts(lowered, body, slots, call_span)? {
                                    LoweredStmtFlow::None => {}
                                    LoweredStmtFlow::Return(value)
                                    | LoweredStmtFlow::Propagate(value) => {
                                        return Ok(ControlFlow::Break(value));
                                    }
                                    LoweredStmtFlow::Break(_) => {
                                        return Err(RuntimeError::new(
                                            "break-outside-loop",
                                            "break used outside loop",
                                        )
                                        .with_span(*span));
                                    }
                                    LoweredStmtFlow::Continue => {
                                        return Err(RuntimeError::new(
                                            "continue-outside-loop",
                                            "continue used outside loop",
                                        )
                                        .with_span(*span));
                                    }
                                }
                                let value =
                                    match self.eval_lowered_expr(lowered, value, slots, *span)? {
                                        ControlFlow::Continue(value) => value,
                                        ControlFlow::Break(value) => {
                                            return Ok(ControlFlow::Break(value));
                                        }
                                    };
                                mapped.push(value);
                            }
                            current = LoweredValue::List(mapped);
                        }
                        LoweredPipelineStage::FlatMap { slot, value } => {
                            let items = self.lowered_pipeline_input_items(current, *span)?;
                            let mut mapped = Vec::new();
                            for item in items {
                                slots[*slot] = item;
                                let value =
                                    match self.eval_lowered_expr(lowered, value, slots, *span)? {
                                        ControlFlow::Continue(value) => value,
                                        ControlFlow::Break(value) => {
                                            return Ok(ControlFlow::Break(value));
                                        }
                                    };
                                mapped.extend(self.lowered_list_items(
                                    value,
                                    *span,
                                    "flat-map expected List",
                                )?);
                            }
                            current = LoweredValue::List(mapped);
                        }
                        LoweredPipelineStage::FlatMapBlock { slot, body, value } => {
                            let items = self.lowered_pipeline_input_items(current, *span)?;
                            let mut mapped = Vec::new();
                            for item in items {
                                slots[*slot] = item;
                                match self.eval_lowered_stmts(lowered, body, slots, call_span)? {
                                    LoweredStmtFlow::None => {}
                                    LoweredStmtFlow::Return(value)
                                    | LoweredStmtFlow::Propagate(value) => {
                                        return Ok(ControlFlow::Break(value));
                                    }
                                    LoweredStmtFlow::Break(_) => {
                                        return Err(RuntimeError::new(
                                            "break-outside-loop",
                                            "break used outside loop",
                                        )
                                        .with_span(*span));
                                    }
                                    LoweredStmtFlow::Continue => {
                                        return Err(RuntimeError::new(
                                            "continue-outside-loop",
                                            "continue used outside loop",
                                        )
                                        .with_span(*span));
                                    }
                                }
                                let value =
                                    match self.eval_lowered_expr(lowered, value, slots, *span)? {
                                        ControlFlow::Continue(value) => value,
                                        ControlFlow::Break(value) => {
                                            return Ok(ControlFlow::Break(value));
                                        }
                                    };
                                mapped.extend(self.lowered_list_items(
                                    value,
                                    *span,
                                    "flat-map expected List",
                                )?);
                            }
                            current = LoweredValue::List(mapped);
                        }
                        LoweredPipelineStage::BytesChunks { size } => {
                            let bytes = lowered_bytes_value(&current)
                                .ok_or_else(|| {
                                    RuntimeError::new(
                                        "type-error",
                                        format!(
                                            "bytes.chunks expected Bytes, found {}",
                                            current.type_name()
                                        ),
                                    )
                                    .with_span(*span)
                                })?
                                .to_vec();
                            let size = match self.eval_lowered_expr(lowered, size, slots, *span)? {
                                ControlFlow::Continue(LoweredValue::Int(value)) if value > 0 => {
                                    value
                                }
                                ControlFlow::Continue(LoweredValue::Int(_)) => {
                                    return Err(RuntimeError::new(
                                        "bytes-chunks",
                                        "chunk size must be positive",
                                    )
                                    .with_span(*span));
                                }
                                ControlFlow::Continue(value) => {
                                    return Err(RuntimeError::new(
                                        "type-error",
                                        format!(
                                            "bytes.chunks size expected Int, found {}",
                                            value.type_name()
                                        ),
                                    )
                                    .with_span(*span));
                                }
                                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                            };
                            let chunks = bytes_module::chunks(bytes, size, *span)?;
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
                                    .with_span(*span));
                                };
                                lowered.push(chunk);
                            }
                            current = LoweredValue::List(lowered);
                        }
                        LoweredPipelineStage::BatchCount { count } => {
                            let count = match self
                                .eval_lowered_expr(lowered, count, slots, *span)?
                            {
                                ControlFlow::Continue(LoweredValue::Int(value)) if value > 0 => {
                                    value as usize
                                }
                                ControlFlow::Continue(LoweredValue::Int(_)) => {
                                    return Err(RuntimeError::new(
                                        "stream-stage-option",
                                        "--count must be positive",
                                    )
                                    .with_span(*span));
                                }
                                ControlFlow::Continue(value) => {
                                    return Err(RuntimeError::new(
                                        "type-error",
                                        format!(
                                            "--count expected Int, found {}",
                                            value.type_name()
                                        ),
                                    )
                                    .with_span(*span));
                                }
                                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                            };
                            let items = self.lowered_pipeline_input_items(current, *span)?;
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
                            current = LoweredValue::List(batches);
                        }
                        LoweredPipelineStage::Shuffle { seed } => {
                            let seed = match seed {
                                Some(seed) => {
                                    match self.eval_lowered_expr(lowered, seed, slots, *span)? {
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
                                            .with_span(*span));
                                        }
                                        ControlFlow::Break(value) => {
                                            return Ok(ControlFlow::Break(value));
                                        }
                                    }
                                }
                                None => 0,
                            };
                            let mut items = self.lowered_pipeline_input_items(current, *span)?;
                            let mut state =
                                seed ^ (items.len() as u64).wrapping_mul(0x9e3779b97f4a7c15);
                            for index in (1..items.len()).rev() {
                                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                                let swap = (state as usize) % (index + 1);
                                items.swap(index, swap);
                            }
                            current = LoweredValue::List(items);
                        }
                        LoweredPipelineStage::Fold {
                            acc_slot,
                            item_slot,
                            initial,
                            body,
                            value,
                        } => {
                            let mut acc = match self
                                .eval_lowered_expr(lowered, initial, slots, *span)?
                            {
                                ControlFlow::Continue(value) => value,
                                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                            };
                            match current {
                                LoweredValue::List(items) => {
                                    for item in items {
                                        acc = match self.eval_lowered_fold_step(
                                            lowered, *acc_slot, *item_slot, acc, item, body, value,
                                            slots, *span,
                                        )? {
                                            ControlFlow::Continue(value) => value,
                                            ControlFlow::Break(value) => {
                                                return Ok(ControlFlow::Break(value));
                                            }
                                        };
                                    }
                                }
                                LoweredValue::Stream(stream) => {
                                    while let Some(item) = stream.next_live(*span)? {
                                        let Some(item) = lowered_value_from_runtime_any(&item)
                                        else {
                                            return Err(RuntimeError::new(
                                                "type-error",
                                                format!(
                                                    "stream produced unsupported {}",
                                                    item.type_name()
                                                ),
                                            )
                                            .with_span(*span));
                                        };
                                        acc = match self.eval_lowered_fold_step(
                                            lowered, *acc_slot, *item_slot, acc, item, body, value,
                                            slots, *span,
                                        )? {
                                            ControlFlow::Continue(value) => value,
                                            ControlFlow::Break(value) => {
                                                return Ok(ControlFlow::Break(value));
                                            }
                                        };
                                    }
                                }
                                _ => {
                                    return Err(RuntimeError::new(
                                        "type-error",
                                        "pipeline input expected List",
                                    )
                                    .with_span(*span));
                                }
                            }
                            slots[*acc_slot] = LoweredValue::Unit;
                            slots[*item_slot] = LoweredValue::Unit;
                            current = acc;
                        }
                        LoweredPipelineStage::Count => {
                            let items = self.lowered_pipeline_input_items(current, *span)?;
                            current = LoweredValue::Int(items.len() as i64);
                        }
                        LoweredPipelineStage::Sum => {
                            let items = self.lowered_pipeline_input_items(current, *span)?;
                            let mut sum = 0i64;
                            for item in items {
                                let LoweredValue::Int(value) = item else {
                                    return Err(RuntimeError::new(
                                        "type-error",
                                        "sum expected Int stream",
                                    )
                                    .with_span(*span));
                                };
                                sum += value;
                            }
                            current = LoweredValue::Int(sum);
                        }
                        LoweredPipelineStage::First => {
                            let items = self.lowered_pipeline_input_items(current, *span)?;
                            current = match items.into_iter().next() {
                                Some(item) => lowered_result_ok(item),
                                None => lowered_result_err_value(
                                    RuntimeError::new("empty-stream", "stream was empty")
                                        .with_span(*span),
                                ),
                            };
                        }
                        LoweredPipelineStage::Last => {
                            let items = self.lowered_pipeline_input_items(current, *span)?;
                            current = match items.into_iter().last() {
                                Some(item) => lowered_result_ok(item),
                                None => lowered_result_err_value(
                                    RuntimeError::new("empty-stream", "stream was empty")
                                        .with_span(*span),
                                ),
                            };
                        }
                        LoweredPipelineStage::Min => {
                            let items = self.lowered_pipeline_input_items(current, *span)?;
                            current = match items.into_iter().min_by(compare_lowered_sort_keys) {
                                Some(item) => lowered_result_ok(item),
                                None => lowered_result_err_value(
                                    RuntimeError::new("empty-stream", "stream was empty")
                                        .with_span(*span),
                                ),
                            };
                        }
                        LoweredPipelineStage::Max => {
                            let items = self.lowered_pipeline_input_items(current, *span)?;
                            current = match items.into_iter().max_by(compare_lowered_sort_keys) {
                                Some(item) => lowered_result_ok(item),
                                None => lowered_result_err_value(
                                    RuntimeError::new("empty-stream", "stream was empty")
                                        .with_span(*span),
                                ),
                            };
                        }
                        LoweredPipelineStage::Collect => {
                            if let LoweredValue::Stream(stream) = current {
                                let values = self.collect_stream_values(stream, *span)?;
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
                                        .with_span(*span));
                                    };
                                    lowered.push(value);
                                }
                                current = LoweredValue::List(lowered);
                            } else if !matches!(current, LoweredValue::List(_)) {
                                return Err(RuntimeError::new(
                                    "type-error",
                                    "pipeline input expected List",
                                )
                                .with_span(*span));
                            }
                        }
                        LoweredPipelineStage::Take(count) => {
                            let count = match self
                                .eval_lowered_expr(lowered, count, slots, *span)?
                            {
                                ControlFlow::Continue(value) => {
                                    lowered_nonnegative_count(value, *span)?
                                }
                                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                            };
                            match current {
                                LoweredValue::List(mut items) => {
                                    items.truncate(count.min(items.len()));
                                    current = LoweredValue::List(items);
                                }
                                LoweredValue::Stream(stream) => {
                                    let mut items =
                                        self.collect_lowered_stream_values(stream, *span)?;
                                    items.truncate(count.min(items.len()));
                                    current = LoweredValue::List(items);
                                }
                                _ => {
                                    return Err(RuntimeError::new(
                                        "type-error",
                                        "pipeline input expected List",
                                    )
                                    .with_span(*span));
                                }
                            }
                        }
                        LoweredPipelineStage::Drop(count) => {
                            let count = match self
                                .eval_lowered_expr(lowered, count, slots, *span)?
                            {
                                ControlFlow::Continue(value) => {
                                    lowered_nonnegative_count(value, *span)?
                                }
                                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                            };
                            match current {
                                LoweredValue::List(items) => {
                                    current =
                                        LoweredValue::List(items.into_iter().skip(count).collect());
                                }
                                LoweredValue::Stream(stream) => {
                                    current = LoweredValue::List(
                                        self.collect_lowered_stream_values(stream, *span)?
                                            .into_iter()
                                            .skip(count)
                                            .collect(),
                                    );
                                }
                                _ => {
                                    return Err(RuntimeError::new(
                                        "type-error",
                                        "pipeline input expected List",
                                    )
                                    .with_span(*span));
                                }
                            }
                        }
                        LoweredPipelineStage::Repeat { count } => {
                            let n = match self.eval_lowered_expr(lowered, count, slots, *span)? {
                                ControlFlow::Continue(value) => {
                                    lowered_nonnegative_count(value, *span)?
                                }
                                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                            };
                            current = match current {
                                LoweredValue::List(items) => {
                                    let mut repeated = Vec::with_capacity(items.len() * n);
                                    for _ in 0..n {
                                        repeated.extend(items.iter().cloned());
                                    }
                                    LoweredValue::List(repeated)
                                }
                                LoweredValue::Stream(stream) => {
                                    let items =
                                        self.collect_lowered_stream_values(stream, *span)?;
                                    let mut repeated = Vec::with_capacity(items.len() * n);
                                    for _ in 0..n {
                                        repeated.extend(items.iter().cloned());
                                    }
                                    LoweredValue::List(repeated)
                                }
                                _ => {
                                    return Err(RuntimeError::new(
                                        "type-error",
                                        "pipeline input expected List",
                                    )
                                    .with_span(*span));
                                }
                            };
                        }
                        LoweredPipelineStage::Range { start, end } => {
                            let s = match self.eval_lowered_expr(lowered, start, slots, *span)? {
                                ControlFlow::Continue(LoweredValue::Int(v)) => v,
                                ControlFlow::Continue(v) => {
                                    return Err(RuntimeError::new(
                                        "type-error",
                                        format!(
                                            "range start expected Int, found {}",
                                            v.type_name()
                                        ),
                                    )
                                    .with_span(*span));
                                }
                                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                            };
                            let e = match self.eval_lowered_expr(lowered, end, slots, *span)? {
                                ControlFlow::Continue(LoweredValue::Int(v)) => v,
                                ControlFlow::Continue(v) => {
                                    return Err(RuntimeError::new(
                                        "type-error",
                                        format!("range end expected Int, found {}", v.type_name()),
                                    )
                                    .with_span(*span));
                                }
                                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                            };
                            let values: Vec<LoweredValue> = if s <= e {
                                (s..e).map(LoweredValue::Int).collect()
                            } else {
                                (e + 1..=s).rev().map(LoweredValue::Int).collect()
                            };
                            current = LoweredValue::List(values);
                        }
                        LoweredPipelineStage::BatchMaxArgv { max_argv } => {
                            let s = *span;
                            let max = match max_argv {
                                Some(expr) => {
                                    match self.eval_lowered_expr(lowered, expr, slots, s)? {
                                        ControlFlow::Continue(value) => {
                                            lowered_nonnegative_count(value, s)?
                                        }
                                        ControlFlow::Break(value) => {
                                            return Ok(ControlFlow::Break(value));
                                        }
                                    }
                                }
                                None => super::stream::platform_arg_max()
                                    .saturating_sub(4096)
                                    .clamp(1, 128 * 1024),
                            };
                            let items = self.lowered_pipeline_input_items(current, s)?;
                            let mut batches = Vec::new();
                            let mut batch = Vec::new();
                            let mut batch_len = 0usize;
                            for item in items {
                                let item_len = lowered_value_argv_len(&item);
                                if !batch.is_empty() && batch_len + 1 + item_len > max {
                                    batches.push(LoweredValue::List(batch));
                                    batch = Vec::new();
                                    batch_len = 0;
                                }
                                batch.push(item);
                                batch_len += if batch.len() == 1 {
                                    item_len
                                } else {
                                    1 + item_len
                                };
                            }
                            if !batch.is_empty() {
                                batches.push(LoweredValue::List(batch));
                            }
                            current = LoweredValue::List(batches);
                        }
                        LoweredPipelineStage::BatchMaxBytes { max_bytes } => {
                            let s = *span;
                            let budget = match self
                                .eval_lowered_expr(lowered, max_bytes, slots, s)?
                            {
                                ControlFlow::Continue(value) => {
                                    lowered_nonnegative_count(value, s)?
                                }
                                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                            };
                            let items = self.lowered_pipeline_input_items(current, s)?;
                            let mut batches = Vec::new();
                            let mut batch = Vec::new();
                            let mut batch_len = 0usize;
                            for item in items {
                                let item_len = lowered_value_argv_len(&item);
                                if item_len > budget {
                                    return Err(RuntimeError::new(
                                        "argv-limit",
                                        "batch item exceeds byte budget",
                                    )
                                    .with_span(s));
                                }
                                if !batch.is_empty() && batch_len + item_len > budget {
                                    batches.push(LoweredValue::List(std::mem::take(&mut batch)));
                                    batch_len = 0;
                                }
                                batch_len += item_len;
                                batch.push(item);
                            }
                            if !batch.is_empty() {
                                batches.push(LoweredValue::List(batch));
                            }
                            current = LoweredValue::List(batches);
                        }
                        LoweredPipelineStage::ParMap { slot, value } => {
                            let s = *span;
                            let items = self.lowered_pipeline_input_items(current, s)?;
                            let mut results = Vec::with_capacity(items.len());
                            for (item_index, item) in items.into_iter().enumerate() {
                                self.trace_leaf(
                                    TraceKind::ParallelJobStart,
                                    Some(s),
                                    Some("par-map"),
                                    TracePayload::ParallelJob {
                                        stage: "par-map".to_string(),
                                        item_index,
                                        error: None,
                                    },
                                );
                                slots[*slot] = item;
                                let mut trace_error = None;
                                let item_result = match self
                                    .eval_lowered_expr(lowered, value, slots, s)
                                {
                                    Ok(ControlFlow::Continue(v)) => v,
                                    Ok(ControlFlow::Break(v)) => {
                                        let runtime_value = v.clone().into_value();
                                        trace_error =
                                            Some(lowered_trace_error_from_value(&runtime_value));
                                        self.stderr.extend_from_slice(
                                            format!(
                                                "par-map error: {}\n",
                                                lowered_error_message(&v)
                                            )
                                            .as_bytes(),
                                        );
                                        LoweredValue::List(Vec::new())
                                    }
                                    Err(error) => {
                                        trace_error =
                                            Some(TraceError::new(&error.kind, &error.message));
                                        self.stderr.extend_from_slice(
                                            format!("par-map error: {error:?}\n").as_bytes(),
                                        );
                                        LoweredValue::List(Vec::new())
                                    }
                                };
                                if let LoweredValue::ResultErr(_) = item_result {
                                    let runtime_value = item_result.clone().into_value();
                                    trace_error =
                                        Some(lowered_trace_error_from_value(&runtime_value));
                                    self.stderr.extend_from_slice(
                                        format!(
                                            "par-map error: {}\n",
                                            lowered_error_message(&item_result)
                                        )
                                        .as_bytes(),
                                    );
                                    results.push(LoweredValue::List(Vec::new()));
                                } else if let LoweredValue::ResultOk(value) = item_result {
                                    results.push(*value);
                                } else {
                                    results.push(item_result);
                                }
                                self.trace_leaf(
                                    TraceKind::ParallelJobEnd,
                                    Some(s),
                                    Some("par-map"),
                                    TracePayload::ParallelJob {
                                        stage: "par-map".to_string(),
                                        item_index,
                                        error: trace_error,
                                    },
                                );
                            }
                            current = LoweredValue::List(results);
                        }
                        LoweredPipelineStage::ParMapBlock { slot, body, value } => {
                            let s = *span;
                            let items = self.lowered_pipeline_input_items(current, s)?;
                            let mut results = Vec::with_capacity(items.len());
                            for item in items {
                                slots[*slot] = item;
                                let item_result = match self
                                    .eval_lowered_stmts(lowered, body, slots, s)
                                {
                                    Err(error) => {
                                        self.stderr.extend_from_slice(
                                            format!("par-map error: {error:?}\n").as_bytes(),
                                        );
                                        LoweredValue::List(Vec::new())
                                    }
                                    Ok(flow) => match flow {
                                        LoweredStmtFlow::None | LoweredStmtFlow::Continue => {
                                            match self.eval_lowered_expr(lowered, value, slots, s) {
                                                Ok(ControlFlow::Continue(v)) => v,
                                                Ok(ControlFlow::Break(v)) => {
                                                    self.stderr.extend_from_slice(
                                                        format!(
                                                            "par-map error: {}\n",
                                                            lowered_error_message(&v)
                                                        )
                                                        .as_bytes(),
                                                    );
                                                    LoweredValue::List(Vec::new())
                                                }
                                                Err(error) => {
                                                    self.stderr.extend_from_slice(
                                                        format!("par-map error: {error:?}\n")
                                                            .as_bytes(),
                                                    );
                                                    LoweredValue::List(Vec::new())
                                                }
                                            }
                                        }
                                        LoweredStmtFlow::Propagate(v) => {
                                            self.stderr.extend_from_slice(
                                                format!(
                                                    "par-map error: {}\n",
                                                    lowered_error_message(&v)
                                                )
                                                .as_bytes(),
                                            );
                                            LoweredValue::List(Vec::new())
                                        }
                                        LoweredStmtFlow::Return(v) => {
                                            return Ok(ControlFlow::Break(v));
                                        }
                                        LoweredStmtFlow::Break(v) => {
                                            return Ok(ControlFlow::Break(
                                                v.unwrap_or(LoweredValue::Unit),
                                            ));
                                        }
                                    },
                                };
                                if let LoweredValue::ResultErr(_) = item_result {
                                    self.stderr.extend_from_slice(
                                        format!(
                                            "par-map error: {}\n",
                                            lowered_error_message(&item_result)
                                        )
                                        .as_bytes(),
                                    );
                                    results.push(LoweredValue::List(Vec::new()));
                                } else if let LoweredValue::ResultOk(value) = item_result {
                                    results.push(*value);
                                } else {
                                    results.push(item_result);
                                }
                            }
                            current = LoweredValue::List(results);
                        }
                        LoweredPipelineStage::Tee { slot, body } => {
                            let s = *span;
                            let items = self.lowered_pipeline_input_items(current, s)?;
                            for item in &items {
                                slots[*slot] = item.clone();
                                let flow = self.eval_lowered_stmts(lowered, body, slots, s)?;
                                match flow {
                                    LoweredStmtFlow::None | LoweredStmtFlow::Continue => {}
                                    LoweredStmtFlow::Propagate(v) => {
                                        return Ok(ControlFlow::Break(v));
                                    }
                                    LoweredStmtFlow::Return(v) => return Ok(ControlFlow::Break(v)),
                                    LoweredStmtFlow::Break(v) => {
                                        return Ok(ControlFlow::Break(
                                            v.unwrap_or(LoweredValue::Unit),
                                        ));
                                    }
                                }
                            }
                            current = LoweredValue::List(items);
                        }
                        LoweredPipelineStage::Each {
                            slot,
                            body,
                            parallel,
                        } => {
                            let s = *span;
                            let items = self.lowered_pipeline_input_items(current, s)?;
                            for (item_index, item) in items.into_iter().enumerate() {
                                if *parallel {
                                    self.trace_leaf(
                                        TraceKind::ParallelJobStart,
                                        Some(s),
                                        Some("each"),
                                        TracePayload::ParallelJob {
                                            stage: "each".to_string(),
                                            item_index,
                                            error: None,
                                        },
                                    );
                                }
                                slots[*slot] = item;
                                let flow = match self.eval_lowered_stmts(lowered, body, slots, s) {
                                    Ok(flow) => flow,
                                    Err(error) => {
                                        if *parallel {
                                            let trace_error =
                                                TraceError::new(&error.kind, &error.message);
                                            self.trace_leaf(
                                                TraceKind::ParallelCancel,
                                                Some(s),
                                                Some("each"),
                                                TracePayload::ParallelJob {
                                                    stage: "each".to_string(),
                                                    item_index,
                                                    error: Some(trace_error.clone()),
                                                },
                                            );
                                            self.trace_leaf(
                                                TraceKind::ParallelJobEnd,
                                                Some(s),
                                                Some("each"),
                                                TracePayload::ParallelJob {
                                                    stage: "each".to_string(),
                                                    item_index,
                                                    error: Some(trace_error),
                                                },
                                            );
                                        }
                                        return Err(error);
                                    }
                                };
                                match flow {
                                    LoweredStmtFlow::None | LoweredStmtFlow::Continue => {
                                        if *parallel {
                                            self.trace_leaf(
                                                TraceKind::ParallelJobEnd,
                                                Some(s),
                                                Some("each"),
                                                TracePayload::ParallelJob {
                                                    stage: "each".to_string(),
                                                    item_index,
                                                    error: None,
                                                },
                                            );
                                        }
                                    }
                                    LoweredStmtFlow::Propagate(v) => {
                                        if *parallel {
                                            let runtime_value = v.clone().into_value();
                                            let trace_error =
                                                lowered_trace_error_from_value(&runtime_value);
                                            self.trace_leaf(
                                                TraceKind::ParallelCancel,
                                                Some(s),
                                                Some("each"),
                                                TracePayload::ParallelJob {
                                                    stage: "each".to_string(),
                                                    item_index,
                                                    error: Some(trace_error.clone()),
                                                },
                                            );
                                            self.trace_leaf(
                                                TraceKind::ParallelJobEnd,
                                                Some(s),
                                                Some("each"),
                                                TracePayload::ParallelJob {
                                                    stage: "each".to_string(),
                                                    item_index,
                                                    error: Some(trace_error),
                                                },
                                            );
                                        }
                                        return Ok(ControlFlow::Break(v));
                                    }
                                    LoweredStmtFlow::Return(v) => {
                                        if *parallel {
                                            let runtime_value = v.clone().into_value();
                                            let trace_error =
                                                lowered_trace_error_from_value(&runtime_value);
                                            self.trace_leaf(
                                                TraceKind::ParallelCancel,
                                                Some(s),
                                                Some("each"),
                                                TracePayload::ParallelJob {
                                                    stage: "each".to_string(),
                                                    item_index,
                                                    error: Some(trace_error.clone()),
                                                },
                                            );
                                            self.trace_leaf(
                                                TraceKind::ParallelJobEnd,
                                                Some(s),
                                                Some("each"),
                                                TracePayload::ParallelJob {
                                                    stage: "each".to_string(),
                                                    item_index,
                                                    error: Some(trace_error),
                                                },
                                            );
                                        }
                                        return Ok(ControlFlow::Break(v));
                                    }
                                    LoweredStmtFlow::Break(v) => {
                                        if *parallel {
                                            self.trace_leaf(
                                                TraceKind::ParallelJobEnd,
                                                Some(s),
                                                Some("each"),
                                                TracePayload::ParallelJob {
                                                    stage: "each".to_string(),
                                                    item_index,
                                                    error: None,
                                                },
                                            );
                                        }
                                        return Ok(ControlFlow::Break(
                                            v.unwrap_or(LoweredValue::Unit),
                                        ));
                                    }
                                }
                            }
                            current = LoweredValue::List(Vec::new());
                        }
                        LoweredPipelineStage::TablePrint { columns } => {
                            let s = *span;
                            let records = lowered_pipeline_record_list(&current, s)?;
                            let cols: Vec<String> = columns.clone().unwrap_or_else(|| {
                                let mut seen = std::collections::BTreeSet::new();
                                let mut cols = Vec::new();
                                for record in &records {
                                    for key in record.keys() {
                                        if seen.insert(key.clone()) {
                                            cols.push(key.to_string());
                                        }
                                    }
                                }
                                cols
                            });
                            let table_cols: Vec<crate::terminal::table::TextTableColumn> = cols
                                .iter()
                                .map(|name| {
                                    let align = records
                                        .first()
                                        .and_then(|r| r.get(name.as_str()))
                                        .map(|v| match v {
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
                                .collect();
                            let rows: Vec<Vec<String>> = records
                                .iter()
                                .map(|record| {
                                    cols.iter()
                                        .map(|col| {
                                            let value = record
                                                .get(col.as_str())
                                                .cloned()
                                                .unwrap_or(LoweredValue::Null);
                                            crate::terminal::table::sanitize_table_text(
                                                &lowered_table_print_value(&value),
                                            )
                                        })
                                        .collect()
                                })
                                .collect();
                            let mut output = String::new();
                            let tw =
                                crate::terminal::table::terminal_table_width_for_stdout(20, 120);
                            crate::terminal::table::render_text_table(
                                &table_cols,
                                &rows,
                                tw,
                                &mut output,
                            );
                            self.stdout.extend_from_slice(output.as_bytes());
                            current = LoweredValue::Unit;
                        }
                        LoweredPipelineStage::ReduceBy {
                            item_slot,
                            body,
                            value,
                            op,
                        } => {
                            let s = *span;
                            let items = self.lowered_pipeline_input_items(current, s)?;
                            // Each item's block yields a `{key, value}` record; values are
                            // combined per (stringified) key by the chosen reducer, keeping
                            // one accumulator per key. Result is a Map keyed by the key.
                            let mut groups: BTreeMap<String, LoweredValue> = BTreeMap::new();
                            for item in items {
                                slots[*item_slot] = item;
                                let flow = self.eval_lowered_stmts(lowered, body, slots, s)?;
                                match flow {
                                    LoweredStmtFlow::None | LoweredStmtFlow::Continue => {}
                                    LoweredStmtFlow::Propagate(v) => {
                                        return Ok(ControlFlow::Break(v));
                                    }
                                    LoweredStmtFlow::Return(v) => return Ok(ControlFlow::Break(v)),
                                    LoweredStmtFlow::Break(v) => {
                                        return Ok(ControlFlow::Break(
                                            v.unwrap_or(LoweredValue::Unit),
                                        ));
                                    }
                                }
                                let output =
                                    match self.eval_lowered_expr(lowered, value, slots, s)? {
                                        ControlFlow::Continue(v) => v,
                                        ControlFlow::Break(v) => return Ok(ControlFlow::Break(v)),
                                    };
                                let key = lowered_reduce_by_key(&output, s)?;
                                let val = lowered_record_field(&output, "value")
                                    .cloned()
                                    .ok_or_else(|| {
                                        RuntimeError::new(
                                            "reduce-by-value",
                                            "reduce-by record is missing field `value`",
                                        )
                                        .with_span(s)
                                    })?;
                                match groups.entry(key) {
                                    std::collections::btree_map::Entry::Vacant(slot) => {
                                        slot.insert(val);
                                    }
                                    std::collections::btree_map::Entry::Occupied(mut slot) => {
                                        let prev =
                                            std::mem::replace(slot.get_mut(), LoweredValue::Unit);
                                        *slot.get_mut() =
                                            lowered_reduce_combine(*op, prev, val, s)?;
                                    }
                                }
                            }
                            current = LoweredValue::Map(groups.into_iter().collect());
                        }
                    }
                    self.trace_exit(
                        TraceKind::StreamStageExit,
                        Some(*span),
                        Some(stage_name),
                        TracePayload::StreamStage {
                            stage: stage_name.to_string(),
                            item_count: None,
                            error: None,
                        },
                    );
                }
                Ok(ControlFlow::Continue(current))
            }
            LoweredExpr::Field { base, name, span } => {
                let base = match self.eval_lowered_expr(lowered, base, slots, *span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let value = match base {
                    LoweredValue::Record(record) | LoweredValue::Module(record) => record
                        .get(name.as_str())
                        .cloned()
                        .ok_or_else(|| RuntimeError::new("missing-field", name).with_span(*span))?,
                    LoweredValue::Error(value) => {
                        let (kind, message) = match value.as_ref() {
                            Value::Error(error) => (error.kind.clone(), error.message.clone()),
                            Value::RunError(error) => (error.kind.clone(), error.message.clone()),
                            _ => {
                                return Err(RuntimeError::new(
                                    "type-error",
                                    "field access expected Error",
                                )
                                .with_span(*span));
                            }
                        };
                        match name.as_str() {
                            "kind" => LoweredValue::Str(kind.into()),
                            "message" => LoweredValue::Str(message.into()),
                            _ => {
                                return Err(
                                    RuntimeError::new("missing-field", name).with_span(*span)
                                );
                            }
                        }
                    }
                    LoweredValue::Regex(regex) => match name.as_str() {
                        "pattern" => LoweredValue::Str(regex.pattern.clone().into()),
                        _ => {
                            return Err(RuntimeError::new("missing-field", name).with_span(*span));
                        }
                    },
                    LoweredValue::Status(status) => match name.as_str() {
                        "ok" | "success" => LoweredValue::Bool(status.success),
                        "kind" => {
                            LoweredValue::Str(format!("{:?}", status.kind).to_lowercase().into())
                        }
                        "segments" => LoweredValue::List(
                            status
                                .segments
                                .iter()
                                .map(lowered_status_segment_record)
                                .collect(),
                        ),
                        _ => {
                            return Err(RuntimeError::new("missing-field", name).with_span(*span));
                        }
                    },
                    LoweredValue::ProcessHandle(handle) => match name.as_str() {
                        "pid" => LoweredValue::Int(handle.pid),
                        "command" => LoweredValue::Str(handle.command.clone()),
                        "argv" => LoweredValue::List(
                            handle.argv.iter().cloned().map(LoweredValue::Str).collect(),
                        ),
                        "detached" => LoweredValue::Bool(handle.detached),
                        _ => {
                            return Err(RuntimeError::new("missing-field", name).with_span(*span));
                        }
                    },
                    LoweredValue::Path(path) => {
                        lowered_path_method_value(path, name, Vec::new(), *span)?
                    }
                    _ => {
                        return Err(RuntimeError::new(
                            "type-error",
                            "field access expected Record",
                        )
                        .with_span(*span));
                    }
                };
                Ok(ControlFlow::Continue(value))
            }
            LoweredExpr::Index { base, index, span } => {
                if let LoweredExpr::Param(slot) = base.as_ref() {
                    let index = match self.eval_lowered_expr(lowered, index, slots, *span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    };
                    return lowered_index_ref(&slots[*slot], index, *span)
                        .map(ControlFlow::Continue);
                }
                let base = match self.eval_lowered_expr(lowered, base, slots, *span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let index = match self.eval_lowered_expr(lowered, index, slots, *span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                lowered_index_value(base, index, *span).map(ControlFlow::Continue)
            }
            LoweredExpr::Slice {
                base,
                start,
                end,
                span,
            } => {
                let base = match self.eval_lowered_expr(lowered, base, slots, *span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let start = match start {
                    Some(start) => match self.eval_lowered_expr(lowered, start, slots, *span)? {
                        ControlFlow::Continue(value) => Some(value),
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    },
                    None => None,
                };
                let end = match end {
                    Some(end) => match self.eval_lowered_expr(lowered, end, slots, *span)? {
                        ControlFlow::Continue(value) => Some(value),
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    },
                    None => None,
                };
                lowered_slice_value(base, start, end, *span).map(ControlFlow::Continue)
            }
            LoweredExpr::StrByteLen { receiver, span } => {
                if let LoweredExpr::Param(slot) = receiver.as_ref() {
                    return lowered_str_byte_len_value(&slots[*slot], *span)
                        .map(LoweredValue::Int)
                        .map(ControlFlow::Continue);
                }
                let receiver = match self.eval_lowered_expr(lowered, receiver, slots, *span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                lowered_str_byte_len_value(&receiver, *span)
                    .map(LoweredValue::Int)
                    .map(ControlFlow::Continue)
            }
            LoweredExpr::StrByteAt {
                receiver,
                index,
                default,
                span,
            } => {
                let index = match self.eval_lowered_expr(lowered, index, slots, *span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let LoweredValue::Int(index) = index else {
                    return Err(
                        RuntimeError::new("type-error", "byte_at expected Int").with_span(*span)
                    );
                };
                let default = match default {
                    Some(default) => {
                        let value = match self.eval_lowered_expr(lowered, default, slots, *span)? {
                            ControlFlow::Continue(value) => value,
                            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                        };
                        let LoweredValue::Int(value) = value else {
                            return Err(RuntimeError::new(
                                "type-error",
                                "byte_at default expected Int",
                            )
                            .with_span(*span));
                        };
                        value
                    }
                    None => -1,
                };
                if let LoweredExpr::Param(slot) = receiver.as_ref() {
                    return lowered_str_byte_at_value(&slots[*slot], index, default, *span)
                        .map(LoweredValue::Int)
                        .map(ControlFlow::Continue);
                }
                let receiver = match self.eval_lowered_expr(lowered, receiver, slots, *span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                lowered_str_byte_at_value(&receiver, index, default, *span)
                    .map(LoweredValue::Int)
                    .map(ControlFlow::Continue)
            }
            LoweredExpr::Method {
                receiver,
                name,
                args,
                span,
            } => {
                if !self.trace_enabled
                    && let LoweredExpr::Param(slot) = receiver.as_ref()
                {
                    let mut values = Vec::with_capacity(args.len());
                    for arg in args {
                        values.push(match self.eval_lowered_expr(lowered, arg, slots, *span)? {
                            ControlFlow::Continue(value) => value,
                            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                        });
                    }
                    if let Some(value) = lowered_method_ref(&slots[*slot], name, values, *span)? {
                        return Ok(ControlFlow::Continue(value));
                    }
                }
                let receiver = match self.eval_lowered_expr(lowered, receiver, slots, *span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let mut values = Vec::with_capacity(args.len());
                for arg in args {
                    values.push(match self.eval_lowered_expr(lowered, arg, slots, *span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    });
                }
                if !self.trace_enabled {
                    return self.eval_lowered_method_dispatch(receiver, name, values, span);
                }
                let trace_name = format!("{}.{}", receiver.type_name(), name);
                self.trace_enter(
                    TraceKind::MethodCall,
                    Some(*span),
                    Some(&trace_name),
                    TracePayload::None,
                );
                let result = self.eval_lowered_method_dispatch(receiver, name, values, span);
                self.trace_exit(
                    TraceKind::MethodResult,
                    Some(*span),
                    Some(&trace_name),
                    TracePayload::None,
                );
                result
            }
            LoweredExpr::StrPredicate {
                receiver,
                predicate,
                needle,
                span,
            } => {
                if let LoweredExpr::Param(slot) = receiver.as_ref() {
                    let needle = match self.eval_lowered_expr(lowered, needle, slots, *span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    };
                    return lowered_str_predicate_value(&slots[*slot], *predicate, &needle, *span)
                        .map(LoweredValue::Bool)
                        .map(ControlFlow::Continue);
                }
                let receiver = match self.eval_lowered_expr(lowered, receiver, slots, *span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let needle = match self.eval_lowered_expr(lowered, needle, slots, *span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                lowered_str_predicate_value(&receiver, *predicate, &needle, *span)
                    .map(LoweredValue::Bool)
                    .map(ControlFlow::Continue)
            }
            LoweredExpr::Contains {
                receiver,
                needle,
                span,
            } => {
                let needle = match self.eval_lowered_expr(lowered, needle, slots, *span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                if let LoweredExpr::Param(slot) = receiver.as_ref() {
                    return lowered_contains_value(&slots[*slot], &needle, *span)
                        .map(LoweredValue::Bool)
                        .map(ControlFlow::Continue);
                }
                let receiver = match self.eval_lowered_expr(lowered, receiver, slots, *span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                lowered_contains_value(&receiver, &needle, *span)
                    .map(LoweredValue::Bool)
                    .map(ControlFlow::Continue)
            }
            LoweredExpr::RegexCompile { pattern, span } => {
                let pattern = match self.eval_lowered_expr(lowered, pattern, slots, *span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let Some(pattern) = lowered_str_value(&pattern) else {
                    return Err(
                        RuntimeError::new("type-error", "regex.compile expected Str")
                            .with_span(*span),
                    );
                };
                let regex = match crate::modules::regex::compile(pattern, *span) {
                    Ok(regex) => LoweredValue::Regex(RegexValue {
                        pattern: pattern.to_string(),
                        regex: Arc::new(regex),
                    }),
                    Err(error) => LoweredValue::ResultErr(Box::new(Value::Error(Box::new(error)))),
                };
                Ok(ControlFlow::Continue(match regex {
                    LoweredValue::Regex(regex) => {
                        LoweredValue::ResultOk(Box::new(LoweredValue::Regex(regex)))
                    }
                    error => error,
                }))
            }
            LoweredExpr::Require { value, check, span } => {
                let value = match self.eval_lowered_expr(lowered, value, slots, *span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                if lowered_value_satisfies_require(self, &value, &check.ty) {
                    Ok(ControlFlow::Continue(lowered_result_ok(value)))
                } else {
                    let message = format!(
                        "schema check failed: expected {}, found {}",
                        check.name,
                        value.type_name(),
                    );
                    Ok(ControlFlow::Continue(lowered_result_err_value(
                        RuntimeError::new("schema", message).with_span(*span),
                    )))
                }
            }
            LoweredExpr::RunCapture {
                kind,
                target,
                args,
                env,
                redirections,
                timeout,
                propagate,
                assert_success,
                span,
            } => self.eval_lowered_run_capture(
                lowered,
                *kind,
                target,
                args,
                env,
                redirections,
                timeout.as_deref(),
                *propagate,
                *assert_success,
                *span,
                slots,
            ),
            LoweredExpr::RunPipeline {
                segments,
                propagate,
                span,
            } => self.eval_lowered_run_pipeline(lowered, segments, *propagate, *span, slots),
            LoweredExpr::SpawnRun {
                target,
                args,
                env,
                redirections,
                span,
            } => {
                self.eval_lowered_spawn_run(lowered, target, args, env, redirections, *span, slots)
            }
            LoweredExpr::SpawnCommand { command, span } => {
                self.eval_lowered_spawn_command(lowered, command, *span, slots)
            }
            LoweredExpr::Wait { target, span } => {
                self.eval_lowered_wait(lowered, target, *span, slots)
            }
            LoweredExpr::Loop { body, span } => loop {
                self.service_pending_signal(*span)?;
                if self.signal_state.shutdown_complete {
                    return Ok(ControlFlow::Continue(LoweredValue::Unit));
                }
                match self.eval_lowered_stmts(lowered, body, slots, *span)? {
                    LoweredStmtFlow::None | LoweredStmtFlow::Continue => {}
                    LoweredStmtFlow::Break(value) => {
                        return Ok(ControlFlow::Continue(value.unwrap_or(LoweredValue::Unit)));
                    }
                    LoweredStmtFlow::Return(value) => return Ok(ControlFlow::Continue(value)),
                    LoweredStmtFlow::Propagate(value) => return Ok(ControlFlow::Break(value)),
                }
            },
            LoweredExpr::Retry { delays, body, span } => {
                let mut delay_values = Vec::with_capacity(delays.len());
                for delay in delays {
                    match self.eval_lowered_expr(lowered, delay, slots, *span)? {
                        ControlFlow::Continue(LoweredValue::Duration(duration)) => {
                            delay_values.push(duration)
                        }
                        ControlFlow::Continue(value) => {
                            return Err(RuntimeError::new(
                                "type-error",
                                format!(
                                    "retry delay expected Duration, found {}",
                                    value.type_name()
                                ),
                            )
                            .with_span(*span));
                        }
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    }
                }

                let max_attempts = delay_values.len() + 1;
                let mut final_error = None;
                let mut final_traceback = None;
                for attempt_index in 0..max_attempts {
                    if attempt_index > 0 {
                        self.sleep_lowered_retry_delay(&delay_values[attempt_index - 1], *span)?;
                        if self.signal_state.shutdown_complete {
                            break;
                        }
                    }
                    let attempt_flow = self.eval_lowered_stmts(lowered, body, slots, *span)?;
                    match self.lowered_retry_attempt_value(attempt_flow) {
                        LoweredRetryAttemptValue::Success(value) => {
                            self.trace_lowered_retry_attempt(
                                *span,
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
                                *span,
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
                            // Propagate the proc-level return out of the retry: at
                            // the enclosing statement this becomes a Propagate flow,
                            // which unwinds the function with `value`.
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
                Ok(ControlFlow::Continue(LoweredValue::ResultErr(Box::new(
                    error,
                ))))
            }
            LoweredExpr::FsList {
                op,
                path,
                stat,
                ordered,
                span,
            } => {
                let operation = if *op == RuntimeOp::FsLs {
                    "fs.ls"
                } else {
                    "fs.children"
                };
                let path = match self.eval_lowered_expr(lowered, path, slots, *span)? {
                    ControlFlow::Continue(value) => lowered_path_arg(value, operation, *span)?,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let stat = match stat {
                    Some(stat) => {
                        let value = match self.eval_lowered_expr(lowered, stat, slots, *span)? {
                            ControlFlow::Continue(value) => value,
                            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                        };
                        lowered_bool_arg_or(Some(value), true, operation, *span)?
                    }
                    None => true,
                };
                let ordered = match ordered {
                    Some(ordered) => {
                        let value = match self.eval_lowered_expr(lowered, ordered, slots, *span)? {
                            ControlFlow::Continue(value) => value,
                            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                        };
                        lowered_bool_arg_or(Some(value), true, operation, *span)?
                    }
                    None => true,
                };
                let value = self.lowered_stream_list_result(
                    fs_module::list_filesystem(self.host_path(&path), stat, ordered, *span),
                    *span,
                )?;
                Ok(ControlFlow::Continue(value))
            }
            LoweredExpr::FsFiles {
                root,
                gitignore,
                stat,
                hidden,
                exts,
                result_wrapped,
                span,
            } => {
                let root = match self.eval_lowered_expr(lowered, root, slots, *span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let LoweredValue::Path(root) = root else {
                    return Err(
                        RuntimeError::new("type-error", "fs.files expected Path").with_span(*span)
                    );
                };
                let exts = match exts {
                    Some(exts) => {
                        let value = match self.eval_lowered_expr(lowered, exts, slots, *span)? {
                            ControlFlow::Continue(value) => value,
                            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                        };
                        lowered_str_list_arg(Some(value), "fs.files exts", *span)?
                    }
                    None => Vec::new(),
                };
                let stream = match crate::modules::fs::walk_filesystem(
                    self.host_path(&root),
                    *gitignore,
                    *stat,
                    *hidden,
                    crate::modules::fs::WalkEmit::Files,
                    exts,
                    *span,
                ) {
                    Ok(stream) => stream,
                    Err(error) if *result_wrapped => {
                        return Ok(ControlFlow::Continue(lowered_result_err_value(error)));
                    }
                    Err(error) => return Err(error),
                };
                let values = match self.collect_stream_values(stream, *span) {
                    Ok(values) => values,
                    Err(error) if *result_wrapped => {
                        return Ok(ControlFlow::Continue(lowered_result_err_value(error)));
                    }
                    Err(error) => return Err(error),
                };
                let mut lowered = Vec::with_capacity(values.len());
                for value in values {
                    let Some(value) = lowered_value_from_runtime_any(&value) else {
                        return Err(RuntimeError::new(
                            "type-error",
                            format!("fs.files produced unsupported {}", value.type_name()),
                        )
                        .with_span(*span));
                    };
                    lowered.push(value);
                }
                let value = LoweredValue::List(lowered);
                Ok(ControlFlow::Continue(if *result_wrapped {
                    lowered_result_ok(value)
                } else {
                    value
                }))
            }
            LoweredExpr::FsWalk {
                root,
                gitignore,
                stat,
                hidden,
                exts,
                result_wrapped,
                span,
            } => {
                let root = match self.eval_lowered_expr(lowered, root, slots, *span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let LoweredValue::Path(root) = root else {
                    return Err(
                        RuntimeError::new("type-error", "fs.walk expected Path").with_span(*span)
                    );
                };
                let exts = match exts {
                    Some(exts) => {
                        let value = match self.eval_lowered_expr(lowered, exts, slots, *span)? {
                            ControlFlow::Continue(value) => value,
                            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                        };
                        lowered_str_list_arg(Some(value), "fs.walk exts", *span)?
                    }
                    None => Vec::new(),
                };
                let stream = match crate::modules::fs::walk_filesystem(
                    self.host_path(&root),
                    *gitignore,
                    *stat,
                    *hidden,
                    crate::modules::fs::WalkEmit::All,
                    exts,
                    *span,
                ) {
                    Ok(stream) => stream,
                    Err(error) if *result_wrapped => {
                        return Ok(ControlFlow::Continue(lowered_result_err_value(error)));
                    }
                    Err(error) => return Err(error),
                };
                let values = match self.collect_stream_values(stream, *span) {
                    Ok(values) => values,
                    Err(error) if *result_wrapped => {
                        return Ok(ControlFlow::Continue(lowered_result_err_value(error)));
                    }
                    Err(error) => return Err(error),
                };
                let mut lowered = Vec::with_capacity(values.len());
                for value in values {
                    let Some(value) = lowered_value_from_runtime_any(&value) else {
                        return Err(RuntimeError::new(
                            "type-error",
                            format!("fs.walk produced unsupported {}", value.type_name()),
                        )
                        .with_span(*span));
                    };
                    lowered.push(value);
                }
                let value = LoweredValue::List(lowered);
                Ok(ControlFlow::Continue(if *result_wrapped {
                    lowered_result_ok(value)
                } else {
                    value
                }))
            }
            LoweredExpr::FsTempDir { span } => {
                let value = match cap_tempfile::TempDir::new(cap_tempfile::ambient_authority()) {
                    Ok(dir) => {
                        let id = self.fs_roots.len() as i64 + 1;
                        self.fs_roots.push(Some(FsRootHandle::TempDir(dir)));
                        lowered_result_ok(fs_root_record(id))
                    }
                    Err(error) => lowered_result_err_value(
                        RuntimeError::new("fs-temp-dir", error.to_string()).with_span(*span),
                    ),
                };
                Ok(ControlFlow::Continue(value))
            }
            LoweredExpr::FsWrite { path, data, span } => {
                let path = match self.eval_lowered_expr(lowered, path, slots, *span)? {
                    ControlFlow::Continue(value) => lowered_path_arg(value, "fs.write", *span)?,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let data = match self.eval_lowered_expr(lowered, data, slots, *span)? {
                    ControlFlow::Continue(value) => {
                        lowered_bytes_or_str_owned(value, "write", *span)?
                    }
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let value = lowered_unit_result(crate::modules::fs::write_path(
                    self.host_path(&path),
                    &data,
                    *span,
                ));
                Ok(ControlFlow::Continue(value))
            }
            LoweredExpr::FsMkdir {
                path,
                parents,
                span,
            } => {
                let path = match self.eval_lowered_expr(lowered, path, slots, *span)? {
                    ControlFlow::Continue(value) => lowered_path_arg(value, "fs.mkdir", *span)?,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let parents = match parents {
                    Some(value) => match self.eval_lowered_expr(lowered, value, slots, *span)? {
                        ControlFlow::Continue(value) => Some(value),
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    },
                    None => None,
                };
                let parents = lowered_bool_arg_or(parents, true, "fs.mkdir", *span)?;
                let value = lowered_unit_result(crate::modules::fs::mkdir_path(
                    self.host_path(&path),
                    parents,
                    None,
                    *span,
                ));
                Ok(ControlFlow::Continue(value))
            }
            LoweredExpr::FsRemove {
                path,
                missing_ok,
                span,
            } => {
                let path = match self.eval_lowered_expr(lowered, path, slots, *span)? {
                    ControlFlow::Continue(value) => lowered_path_arg(value, "fs.remove", *span)?,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let missing_ok = match missing_ok {
                    Some(value) => match self.eval_lowered_expr(lowered, value, slots, *span)? {
                        ControlFlow::Continue(value) => Some(value),
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    },
                    None => None,
                };
                let missing_ok = lowered_bool_arg_or(missing_ok, false, "fs.remove", *span)?;
                let value = lowered_unit_result(crate::modules::fs::remove_path(
                    self.host_path(&path),
                    missing_ok,
                    *span,
                ));
                Ok(ControlFlow::Continue(value))
            }
            LoweredExpr::FsCloseRoot { root, span } => {
                let root = match self.eval_lowered_expr(lowered, root, slots, *span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let value = match lowered_root_id(&root, *span)
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
                                    .with_span(*span),
                            )
                        }
                    }
                    _ => lowered_result_err_value(
                        RuntimeError::new("fs-root", "root handle is not active").with_span(*span),
                    ),
                };
                Ok(ControlFlow::Continue(value))
            }
            LoweredExpr::FsRootPath { root, span } => {
                let root = match self.eval_lowered_expr(lowered, root, slots, *span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let value = match lowered_fs_root_dir(&self.fs_roots, &root, *span)
                    .and_then(|dir| root_path_from_dir(dir, *span))
                {
                    Ok(path) => lowered_result_ok(LoweredValue::Path(path)),
                    Err(error) => lowered_result_err_value(error),
                };
                Ok(ControlFlow::Continue(value))
            }
            LoweredExpr::PathReadText { path, span } => {
                let path = match self.eval_lowered_expr(lowered, path, slots, *span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let LoweredValue::Path(path) = path else {
                    return Err(
                        RuntimeError::new("type-error", "read_text expected Path").with_span(*span)
                    );
                };
                let value = match read_host_path_bytes(&self.host_path(&path), *span) {
                    Ok(bytes) => match String::from_utf8(bytes) {
                        Ok(text) => {
                            LoweredValue::ResultOk(Box::new(LoweredValue::Str(text.into())))
                        }
                        Err(error) => LoweredValue::ResultErr(Box::new(Value::Error(Box::new(
                            RuntimeError::new(
                                "invalid-utf8",
                                format!(
                                    "file is not valid UTF-8 at byte {}",
                                    error.utf8_error().valid_up_to()
                                ),
                            )
                            .with_span(*span),
                        )))),
                    },
                    Err(error) => LoweredValue::ResultErr(Box::new(Value::Error(Box::new(error)))),
                };
                Ok(ControlFlow::Continue(value))
            }
            LoweredExpr::PathReadBytes { path, span } => {
                let path = match self.eval_lowered_expr(lowered, path, slots, *span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let LoweredValue::Path(path) = path else {
                    return Err(RuntimeError::new("type-error", "read_bytes expected Path")
                        .with_span(*span));
                };
                let value = match read_host_path_bytes(&self.host_path(&path), *span) {
                    Ok(bytes) => {
                        LoweredValue::ResultOk(Box::new(LoweredValue::Bytes(bytes.into())))
                    }
                    Err(error) => LoweredValue::ResultErr(Box::new(Value::Error(Box::new(error)))),
                };
                Ok(ControlFlow::Continue(value))
            }
            LoweredExpr::PathExists { path, span } => {
                let path = match self.eval_lowered_expr(lowered, path, slots, *span)? {
                    ControlFlow::Continue(value) => lowered_path_arg(value, "exists", *span)?,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let value = match crate::modules::fs::exists(self.host_path(&path), *span) {
                    Ok(exists) => lowered_result_ok(LoweredValue::Bool(exists)),
                    Err(error) => lowered_result_err_value(error),
                };
                Ok(ControlFlow::Continue(value))
            }
            LoweredExpr::PathExecutable { path, span } => {
                let path = match self.eval_lowered_expr(lowered, path, slots, *span)? {
                    ControlFlow::Continue(value) => lowered_path_arg(value, "executable", *span)?,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let value = match crate::modules::fs::executable(self.host_path(&path), *span) {
                    Ok(executable) => lowered_result_ok(LoweredValue::Bool(executable)),
                    Err(error) => lowered_result_err_value(error),
                };
                Ok(ControlFlow::Continue(value))
            }
            LoweredExpr::PathDu { path, span } => {
                let path = match self.eval_lowered_expr(lowered, path, slots, *span)? {
                    ControlFlow::Continue(value) => lowered_path_arg(value, "du", *span)?,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let value = match crate::modules::fs::disk_usage(self.host_path(&path), *span) {
                    Ok(size) => lowered_result_ok(LoweredValue::Int(size)),
                    Err(error) => lowered_result_err_value(error),
                };
                Ok(ControlFlow::Continue(value))
            }
            LoweredExpr::PathMetadata { path, span } => {
                let path = match self.eval_lowered_expr(lowered, path, slots, *span)? {
                    ControlFlow::Continue(value) => lowered_path_arg(value, "metadata", *span)?,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let value = match crate::modules::fs::metadata(self.host_path(&path), *span) {
                    Ok(record) => match lowered_value_from_runtime_any(&record) {
                        Some(value) => lowered_result_ok(value),
                        None => {
                            return Err(RuntimeError::new(
                                "type-error",
                                format!("metadata produced unsupported {}", record.type_name()),
                            )
                            .with_span(*span));
                        }
                    },
                    Err(error) => lowered_result_err_value(error),
                };
                Ok(ControlFlow::Continue(value))
            }
            LoweredExpr::PathReadlink { path, span } => {
                let path = match self.eval_lowered_expr(lowered, path, slots, *span)? {
                    ControlFlow::Continue(value) => lowered_path_arg(value, "readlink", *span)?,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let value = match crate::modules::fs::readlink(self.host_path(&path), *span) {
                    Ok(value) => match lowered_value_from_runtime_any(&value) {
                        Some(value) => lowered_result_ok(value),
                        None => {
                            return Err(RuntimeError::new(
                                "type-error",
                                format!("readlink produced unsupported {}", value.type_name()),
                            )
                            .with_span(*span));
                        }
                    },
                    Err(error) => lowered_result_err_value(error),
                };
                Ok(ControlFlow::Continue(value))
            }
            LoweredExpr::PathResolve { path, span } => {
                let path = match self.eval_lowered_expr(lowered, path, slots, *span)? {
                    ControlFlow::Continue(value) => lowered_path_arg(value, "resolve", *span)?,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let value = match crate::modules::fs::resolve_path(self.host_path(&path), *span) {
                    Ok(path) => lowered_result_ok(LoweredValue::Path(path)),
                    Err(error) => lowered_result_err_value(error),
                };
                Ok(ControlFlow::Continue(value))
            }
            LoweredExpr::PathWrite {
                path,
                data,
                atomic,
                span,
            } => {
                let path = match self.eval_lowered_expr(lowered, path, slots, *span)? {
                    ControlFlow::Continue(value) => lowered_path_arg(value, "write", *span)?,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let data = match self.eval_lowered_expr(lowered, data, slots, *span)? {
                    ControlFlow::Continue(value) => {
                        lowered_bytes_or_str_owned(value, "write", *span)?
                    }
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let result = if *atomic {
                    crate::modules::fs::write_atomic(self.host_path(&path), &data, *span)
                } else {
                    crate::modules::fs::write_path(self.host_path(&path), &data, *span)
                };
                Ok(ControlFlow::Continue(lowered_unit_result(result)))
            }
            LoweredExpr::PathMkdir {
                path,
                parents,
                span,
            } => {
                let path = match self.eval_lowered_expr(lowered, path, slots, *span)? {
                    ControlFlow::Continue(value) => lowered_path_arg(value, "mkdir", *span)?,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let parents = match parents {
                    Some(value) => match self.eval_lowered_expr(lowered, value, slots, *span)? {
                        ControlFlow::Continue(value) => Some(value),
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    },
                    None => None,
                };
                let parents = lowered_bool_arg_or(parents, true, "mkdir", *span)?;
                let value = lowered_unit_result(crate::modules::fs::mkdir_path(
                    self.host_path(&path),
                    parents,
                    None,
                    *span,
                ));
                Ok(ControlFlow::Continue(value))
            }
            LoweredExpr::PathRemove {
                path,
                missing_ok,
                span,
            } => {
                let path = match self.eval_lowered_expr(lowered, path, slots, *span)? {
                    ControlFlow::Continue(value) => lowered_path_arg(value, "remove", *span)?,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let missing_ok = match missing_ok {
                    Some(value) => match self.eval_lowered_expr(lowered, value, slots, *span)? {
                        ControlFlow::Continue(value) => Some(value),
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    },
                    None => None,
                };
                let missing_ok = lowered_bool_arg_or(missing_ok, false, "remove", *span)?;
                let value = lowered_unit_result(crate::modules::fs::remove_path(
                    self.host_path(&path),
                    missing_ok,
                    *span,
                ));
                Ok(ControlFlow::Continue(value))
            }
            LoweredExpr::ArchiveTarCreate {
                path,
                root,
                entries,
                compression,
                overwrite,
                span,
            } => {
                let path = match self.eval_lowered_expr(lowered, path, slots, *span)? {
                    ControlFlow::Continue(value) => {
                        lowered_path_arg(value, "archive.tar_create", *span)?
                    }
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let root = match self.eval_lowered_expr(lowered, root, slots, *span)? {
                    ControlFlow::Continue(value) => {
                        lowered_path_arg(value, "archive.tar_create", *span)?
                    }
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let entries = match self.eval_lowered_expr(lowered, entries, slots, *span)? {
                    ControlFlow::Continue(value) => {
                        lowered_path_list_arg(value, "archive.tar_create", *span)?
                    }
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let compression = match compression {
                    Some(value) => match self.eval_lowered_expr(lowered, value, slots, *span)? {
                        ControlFlow::Continue(value) => Some(value),
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    },
                    None => None,
                };
                let overwrite = match overwrite {
                    Some(value) => match self.eval_lowered_expr(lowered, value, slots, *span)? {
                        ControlFlow::Continue(value) => Some(value),
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    },
                    None => None,
                };
                let compression =
                    lowered_str_arg_owned(compression, "auto", "archive.tar_create", *span)?;
                let overwrite = lowered_bool_arg_or(overwrite, false, "archive.tar_create", *span)?;
                let value = lowered_unit_result(crate::modules::archive::tar_create(
                    self.host_path(&path),
                    self.host_path(&root),
                    entries,
                    &compression,
                    overwrite,
                    *span,
                ));
                Ok(ControlFlow::Continue(value))
            }
            LoweredExpr::ArchiveTarList { path, span } => {
                let path = match self.eval_lowered_expr(lowered, path, slots, *span)? {
                    ControlFlow::Continue(value) => {
                        lowered_path_arg(value, "archive.tar_list", *span)?
                    }
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let value = match crate::modules::archive::tar_list(
                    self.host_path(&path),
                    "auto",
                    Vec::new(),
                    *span,
                ) {
                    Ok(values) => {
                        let mut lowered = Vec::with_capacity(values.len());
                        for value in values {
                            let Some(value) = lowered_value_from_runtime_any(&value) else {
                                return Err(RuntimeError::new(
                                    "type-error",
                                    format!(
                                        "archive.tar_list produced unsupported {}",
                                        value.type_name()
                                    ),
                                )
                                .with_span(*span));
                            };
                            lowered.push(value);
                        }
                        lowered_result_ok(LoweredValue::List(lowered))
                    }
                    Err(error) => lowered_result_err_value(error),
                };
                Ok(ControlFlow::Continue(value))
            }
            LoweredExpr::ArchiveTarExtract { path, dest, span } => {
                let path = match self.eval_lowered_expr(lowered, path, slots, *span)? {
                    ControlFlow::Continue(value) => {
                        lowered_path_arg(value, "archive.tar_extract", *span)?
                    }
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let dest = match self.eval_lowered_expr(lowered, dest, slots, *span)? {
                    ControlFlow::Continue(value) => {
                        lowered_path_arg(value, "archive.tar_extract", *span)?
                    }
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let value = lowered_unit_result(crate::modules::archive::tar_extract(
                    self.host_path(&path),
                    self.host_path(&dest),
                    0,
                    "auto",
                    false,
                    Vec::new(),
                    *span,
                ));
                Ok(ControlFlow::Continue(value))
            }
            LoweredExpr::HashVerifyFile {
                path,
                algorithm,
                expected,
                span,
            } => {
                let path = match self.eval_lowered_expr(lowered, path, slots, *span)? {
                    ControlFlow::Continue(value) => {
                        lowered_path_arg(value, "hash.verify_file", *span)?
                    }
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let expected = match self.eval_lowered_expr(lowered, expected, slots, *span)? {
                    ControlFlow::Continue(value) => {
                        lowered_str_arg_owned(Some(value), "", "hash.verify_file", *span)?
                    }
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let value =
                    match hash_module::digest_file(*algorithm, &self.host_path(&path), *span)
                        .and_then(|digest| hash_module::verify_hex(&digest, &expected, *span))
                    {
                        Ok(()) => lowered_result_ok(LoweredValue::Unit),
                        Err(error) => lowered_result_err_value(error),
                    };
                Ok(ControlFlow::Continue(value))
            }
            LoweredExpr::JsonEncode { value, span } => {
                let value = match self.eval_lowered_expr(lowered, value, slots, *span)? {
                    ControlFlow::Continue(value) => value.into_value(),
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let value = match crate::modules::json::encode_json(&value, false, *span) {
                    Ok(text) => LoweredValue::ResultOk(Box::new(LoweredValue::Str(text.into()))),
                    Err(error) => LoweredValue::ResultErr(Box::new(Value::Error(Box::new(error)))),
                };
                Ok(ControlFlow::Continue(value))
            }
            LoweredExpr::ProcessCommandArgv {
                target,
                argv,
                cwd,
                env,
                timeout,
                detach,
                new_session,
                ignore_hup,
                cpu_max,
                span,
            } => {
                let target = match self.eval_lowered_expr(lowered, target, slots, *span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let argv = match self.eval_lowered_expr(lowered, argv, slots, *span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let cwd = match cwd {
                    Some(value) => match self.eval_lowered_expr(lowered, value, slots, *span)? {
                        ControlFlow::Continue(value) => Some(value),
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    },
                    None => None,
                };
                let env = match env {
                    Some(value) => match self.eval_lowered_expr(lowered, value, slots, *span)? {
                        ControlFlow::Continue(value) => Some(value),
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    },
                    None => None,
                };
                let timeout = match timeout {
                    Some(value) => match self.eval_lowered_expr(lowered, value, slots, *span)? {
                        ControlFlow::Continue(value) => Some(value),
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    },
                    None => None,
                };
                let detach = match detach {
                    Some(value) => match self.eval_lowered_expr(lowered, value, slots, *span)? {
                        ControlFlow::Continue(value) => Some(value),
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    },
                    None => None,
                };
                let new_session = match new_session {
                    Some(value) => match self.eval_lowered_expr(lowered, value, slots, *span)? {
                        ControlFlow::Continue(value) => Some(value),
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    },
                    None => None,
                };
                let ignore_hup = match ignore_hup {
                    Some(value) => match self.eval_lowered_expr(lowered, value, slots, *span)? {
                        ControlFlow::Continue(value) => Some(value),
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    },
                    None => None,
                };
                let cpu_max = match cpu_max {
                    Some(value) => match self.eval_lowered_expr(lowered, value, slots, *span)? {
                        ControlFlow::Continue(value) => Some(value),
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    },
                    None => None,
                };
                lowered_command_plan_value(
                    target,
                    argv,
                    cwd,
                    env,
                    timeout,
                    detach,
                    new_session,
                    ignore_hup,
                    cpu_max,
                    *span,
                )
                .map(ControlFlow::Continue)
            }
            LoweredExpr::ProcessCommandBuilder { entries, span } => {
                self.eval_lowered_process_command_builder(lowered, entries, *span, slots)
            }
            LoweredExpr::ModuleCall { op, args, span } => {
                if !self.trace_enabled {
                    return self.eval_lowered_module_call(lowered, *op, args, slots, *span);
                }
                let trace_name = crate::modules::signature::api_spec()
                    .op_trace_name(*op)
                    .map(str::to_string);
                self.trace_enter(
                    TraceKind::ModuleCall,
                    Some(*span),
                    trace_name.as_deref(),
                    TracePayload::None,
                );
                let result = self.eval_lowered_module_call(lowered, *op, args, slots, *span);
                self.trace_exit(
                    TraceKind::ModuleResult,
                    Some(*span),
                    trace_name.as_deref(),
                    TracePayload::None,
                );
                result
            }
            LoweredExpr::DynamicCall { callee, args, span } => {
                let callee = match self.eval_lowered_expr(lowered, callee, slots, *span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let mut values = Vec::with_capacity(args.len());
                for arg in args {
                    match arg {
                        LoweredCallArg::Single(arg) => {
                            let value = match self.eval_lowered_expr(lowered, arg, slots, *span)? {
                                ControlFlow::Continue(value) => value,
                                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                            };
                            values.push(value.into_value());
                        }
                        LoweredCallArg::Splice(arg) => {
                            let value = match self.eval_lowered_expr(lowered, arg, slots, *span)? {
                                ControlFlow::Continue(value) => value,
                                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                            };
                            values.extend(
                                lowered_splice_arg_items(value, *span)?
                                    .into_iter()
                                    .map(LoweredValue::into_value),
                            );
                        }
                    }
                }
                let result = match callee {
                    LoweredValue::Pure(function) => self
                        .call_lowered_function_value_with_values(function, true, &values, *span)
                        .ok_or_else(|| {
                            RuntimeError::new(
                                "unresolved-call",
                                format!(
                                    "dynamic call to {} could not be lowered",
                                    function.display_name()
                                ),
                            )
                            .with_span(*span)
                        })??,
                    LoweredValue::Proc(function) => self
                        .call_lowered_function_value_with_values(function, false, &values, *span)
                        .ok_or_else(|| {
                            RuntimeError::new(
                                "unresolved-call",
                                format!(
                                    "dynamic call to {} could not be lowered",
                                    function.display_name()
                                ),
                            )
                            .with_span(*span)
                        })??,
                    other => {
                        return Err(RuntimeError::new(
                            "type-error",
                            format!(
                                "dynamic call expected Pure or Proc, found {}",
                                other.type_name()
                            ),
                        )
                        .with_span(*span));
                    }
                };
                let lowered = lowered_value_from_runtime_any(&result).ok_or_else(|| {
                    RuntimeError::new(
                        "type-error",
                        format!("dynamic call returned unsupported {}", result.type_name()),
                    )
                    .with_span(*span)
                })?;
                Ok(ControlFlow::Continue(lowered))
            }
            LoweredExpr::Abort {
                status,
                force,
                span,
            } => {
                let status = match self.eval_lowered_expr(lowered, status, slots, *span)? {
                    ControlFlow::Continue(LoweredValue::Int(value)) => exit_status(value, *span)?,
                    ControlFlow::Continue(value) => {
                        return Err(RuntimeError::new(
                            "type-error",
                            format!("abort status expected Int, found {}", value.type_name()),
                        )
                        .with_span(*span));
                    }
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let force = match force {
                    Some(force) => match self.eval_lowered_expr(lowered, force, slots, *span)? {
                        ControlFlow::Continue(LoweredValue::Bool(value)) => value,
                        ControlFlow::Continue(value) => {
                            return Err(RuntimeError::new(
                                "type-error",
                                format!("abort force expected Bool, found {}", value.type_name()),
                            )
                            .with_span(*span));
                        }
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    },
                    None => false,
                };
                Err(RuntimeError::abort(status, force).with_span(*span))
            }
            LoweredExpr::Ok(value) => {
                let value = match self.eval_lowered_expr(lowered, value, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                Ok(ControlFlow::Continue(LoweredValue::ResultOk(Box::new(
                    value,
                ))))
            }
            LoweredExpr::Err(value) => {
                let value = match self.eval_lowered_expr(lowered, value, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                Ok(ControlFlow::Continue(LoweredValue::ResultErr(Box::new(
                    value.into_value(),
                ))))
            }
            LoweredExpr::Error(error) => self
                .eval_lowered_error_expr(lowered, error, slots, call_span)
                .map(ControlFlow::Continue),
            LoweredExpr::Try(value) => {
                match self.eval_lowered_expr(lowered, value, slots, call_span)? {
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
                }
            }
            LoweredExpr::Call {
                function,
                args,
                span,
            } => {
                let Some(callee) = self.lowered_function(*function) else {
                    return Err(RuntimeError::new(
                        "unresolved-lowered-call",
                        function.display_name(),
                    )
                    .with_span(*span));
                };
                let mut values = Vec::with_capacity(args.len());
                for arg in args {
                    match arg {
                        LoweredCallArg::Single(arg) => {
                            let value = match self.eval_lowered_expr(lowered, arg, slots, *span)? {
                                ControlFlow::Continue(value) => value,
                                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                            };
                            values.push(value);
                        }
                        LoweredCallArg::Splice(arg) => {
                            let value = match self.eval_lowered_expr(lowered, arg, slots, *span)? {
                                ControlFlow::Continue(value) => value,
                                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                            };
                            values.extend(lowered_splice_arg_items(value, *span)?);
                        }
                    }
                }
                let mut next_slots = self.bind_lowered_values(&callee, &values, *span)?;
                let result = self
                    .eval_lowered_call_with_frame(*function, &callee, &mut next_slots, *span)
                    .and_then(|value| lowered_return_value(callee.return_kind, value, *span))
                    .map(ControlFlow::Continue);
                self.recycle_lowered_slots(next_slots);
                result
            }
            LoweredExpr::SelfCall { args, span } => {
                let mut values = Vec::with_capacity(args.len());
                for arg in args {
                    match arg {
                        LoweredCallArg::Single(arg) => {
                            let value = match self.eval_lowered_expr(lowered, arg, slots, *span)? {
                                ControlFlow::Continue(value) => value,
                                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                            };
                            values.push(value);
                        }
                        LoweredCallArg::Splice(arg) => {
                            let value = match self.eval_lowered_expr(lowered, arg, slots, *span)? {
                                ControlFlow::Continue(value) => value,
                                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                            };
                            values.extend(lowered_splice_arg_items(value, *span)?);
                        }
                    }
                }
                let mut next_slots = self.bind_lowered_values(lowered, &values, *span)?;
                let result = self
                    .eval_lowered_function(lowered, &mut next_slots, *span)
                    .and_then(|value| lowered_return_value(lowered.return_kind, value, *span))
                    .map(ControlFlow::Continue);
                self.recycle_lowered_slots(next_slots);
                result
            }
        }
    }
}
