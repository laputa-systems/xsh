//! Shared value/type adapters and native module-operation dispatch for the
//! indexed runtime. Domain implementations live in private child modules;
//! `indexed_run` owns expression and statement scheduling.

use super::LoweredTypeCheck;
use crate::diagnostic::DiagnosticCode;
use crate::map_key::MapKey;

use crate::modules::{
    RuntimeOp, api_spec, archive as archive_module, bytes as bytes_module, cli as cli_module,
    diff as diff_module, dns as dns_module, elf as elf_module, fs as fs_module,
    group as group_module, hash as hash_module, ini as ini_module, json as json_module,
    linux as linux_module, mime as mime_module, net as net_module, patch as patch_module,
    process as process_module, regex as regex_module, shlex, system, tui, unix as unix_module,
    user as user_module,
};
use crate::runtime::process::{
    CancellationPolicy, ChildWaitOutcome, FileRedirectionMode, ManagedStdio, ProcessEnd,
    ProcessInvocation, ProcessRedirection, ProcessSegmentStatus, ProcessStatus, RedirectionStream,
    ProcessGroupConfig, SpawnManagedOptions, SpawnOptions, WAIT_POLL, cancel_managed, path_bytes, poll_managed,
    resolve_executable, run_capture_with_stderr_policy, run_inherit_with_policy,
    run_pipeline_inherit_with_policy, run_quiet_with_policy, spawn_command, spawn_managed,
};
use crate::runtime::run::execute_run_with_policy;
use crate::runtime::value::{
    CommandPlan, CommandRedirection, CommandRedirectionMode, CommandRedirectionStream,
    DurationValue, FunctionName, LiveStream, PathValue, ProcessHandleValue, RecordMap, RegexValue,
    RunError, RuntimeError, StreamItemErrorAttribution, StreamValue, Value, error_constructor,
    identified_structured_error_constructor,
};
use crate::sema::types::Type;
use crate::source::Span;
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
use rustix::event as revent;
use std::collections::BTreeMap;
#[cfg(target_os = "macos")]
use std::ffi::CStr;
use std::ffi::OsString;
use std::io::{ErrorKind, Read, Write};
use std::ops::ControlFlow;
use std::os::fd::{AsFd, AsRawFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use xsh_root::Root;

pub(in crate::runtime::eval) mod indexed_run;
mod namespaces;
mod module_accounts;
mod module_archive;
mod module_cli;
mod module_codecs;
mod module_dns;
mod module_env;
mod module_fs;
mod module_host;
mod module_linux;
mod module_loading;
mod module_net;
mod module_process;
#[cfg(feature = "native-tests")]
mod module_test;
mod module_text;
mod module_unix;
mod module_utils;
use module_linux::with_linux_call_rows;
use module_unix::with_unix_call_rows;

#[cfg(feature = "native-tests")]
use super::display_value;
use super::lower::{
    lowered_match_no_arm, lowered_record_field, lowered_stmt_flow_to_flow, lowered_str_key,
    lowered_sum_records, lowered_sum_values, lowered_tag_key, take_shared,
};
use super::lowered_ops::{
    append_lowered_list_element, append_lowered_map_literal, checked_int_binary,
    compare_lowered_sort_keys, lowered_assign_value, lowered_binary_value, lowered_bytes_arg,
    lowered_bytes_parts, lowered_bytes_value, lowered_contains_value, lowered_index_from_end_value,
    lowered_index_value, lowered_map_key_value, lowered_map_literal_key, lowered_method_value,
    lowered_nonnegative_count, lowered_path_method_value, lowered_return_value,
    lowered_slice_value, lowered_sort_key_orderable, lowered_str_byte_at_value,
    lowered_str_byte_len_value, lowered_str_count_lines_value, lowered_str_parts,
    lowered_str_predicate_text, lowered_str_predicate_value, lowered_str_value,
    lowered_trim_is_empty_value, lowered_trim_str_predicate_value, lowered_type_name,
    lowered_value_from_runtime, lowered_value_from_runtime_any, lowered_value_matches,
    push_lowered_display,
};
use super::modules::{
    auth as auth_module, display_spawn_argv, intercept_test_host_call, record_int_field,
    record_path, record_str, run_error_to_runtime, utils_cache_key,
};
#[cfg(feature = "native-tests")]
use super::modules::{
    test_error_kind, test_failure, test_mock_expected_return_type, test_temp_path,
};
use super::net_job::NetJobTask;
use crate::runtime::eval::indexed::IrVerifyError;
use crate::runtime::eval::indexed::full::decode::{CompTargetView, InlineList};
use super::{
    Binding, DynamicFunction, Evaluator, Flow, FsRootHandle, FunctionHeader, LoweredCompTarget,
    LoweredFunctionKey, LoweredFunctionKind, LoweredModuleExportKind, LoweredReturnKind,
    LoweredStrPredicate, LoweredTagValue, LoweredType, LoweredValue, Name, ReduceByOp,
    StmtFlow, assign_lowered_bytes_view, assign_lowered_str_view, bytes_contains,
    check_env_name, compound_assignment_value, exit_status, lowered_inline_stats_field_value,
    lowered_inline_stats_to_record_vec, lowered_record_vec_get, lowered_record_vec_get_mut,
    lowered_record_vec_insert, lowered_record_vec_or_stats, lowered_stats_field_value,
    lowered_str_view_value, module_error, module_io_error,
    path_absolute_value, path_value_from_pathbuf, pathbuf_from_path_value,
    runtime_error_from_value, splice_to_argv, trace_env_overlay, trace_status,
    ValueView, value_matches_static_type, value_to_argv_bytes, value_to_env_bytes,
};
#[cfg(feature = "native-tests")]
use super::{NativeTestRunKind, NativeTestRunRequest, TestMock};
const LOWERED_SHARED_LIST_THRESHOLD: usize = 16;

macro_rules! lowered_module_op_support_guard {
    (callable; $op:expr) => {
        true
    };
    (internal; $op:expr) => {
        false
    };
    (fallback; $op:expr) => {
        false
    };
}

macro_rules! lowered_unix_route_pattern {
    (
        unix_rows {
            first_callable {
                $( callable; $first_pattern:pat $(if $first_guard:expr)? => $first_body:expr $(,)? )*
            }
            internal { $($internal_rows:tt)* }
            callable {
                $( callable; $callable_pattern:pat $(if $callable_guard:expr)? => $callable_body:expr $(,)? )*
            }
            fallback { $($fallback_rows:tt)* }
        }
    ) => {
        $($first_pattern)|* | $($callable_pattern)|*
    };
}

macro_rules! lowered_linux_route_pattern {
    (
        linux_rows {
            host_rows {
                $( callable; $host_pattern:pat $(if $host_guard:expr)? => $host_body:expr $(,)? )*
            }
            host_fallback { $($fallback_rows:tt)* }
            fake_rows { $($fake_rows:tt)* }
        }
    ) => {
        $($host_pattern)|*
    };
}

// The same rows drive evaluation and lowering capability. Capability matching
// ignores argument guards so malformed calls retain their evaluator errors.
macro_rules! define_lowered_module_op_authority {
    (
        fn $eval_fn:ident(
            &mut $this:ident,
            $op:ident: RuntimeOp,
            mut $values:ident: NativeArgumentValues,
            $span:ident: Span,
            $cli_plan:ident: Option<&crate::modules::cli::CliDescriptorPlan>,
        ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError>;
        prelude { $($prelude:tt)* }
        pre_unix_rows {
            $(
                $(#[$pre_attribute:meta])*
                $pre_class:ident; $pre_pattern:pat $(if $pre_guard:expr)? => $pre_body:expr $(,)?
            )*
        }
        post_unix_rows {
            $(
                $(#[$post_attribute:meta])*
                $post_class:ident; $post_pattern:pat $(if $post_guard:expr)? => $post_body:expr $(,)?
            )*
        }
        tail_rows {
            $(
                $(#[$tail_attribute:meta])*
                $tail_class:ident; $tail_pattern:pat $(if $tail_guard:expr)? => $tail_body:expr $(,)?
            )*
        }
    ) => {
        fn $eval_fn(
            &mut $this,
            $op: RuntimeOp,
            mut $values: NativeArgumentValues,
            $span: Span,
            $cli_plan: Option<&crate::modules::cli::CliDescriptorPlan>,
        ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
            $($prelude)*
            let value = match $op {
                $(
                    $(#[$pre_attribute])*
                    $pre_pattern $(if $pre_guard)? => $pre_body,
                )*
                with_unix_call_rows! {
                    lowered_unix_route_pattern! {}
                    using $this, $op, $values, $span
                } => {
                    $this.eval_lowered_unix_call($op, $values, $span)?
                }
                $(
                    $(#[$post_attribute])*
                    $post_pattern $(if $post_guard)? => $post_body,
                )*
                with_linux_call_rows! {
                    lowered_linux_route_pattern! {}
                    using $this, $op, $values, $span
                } => {
                    $this.eval_lowered_linux_call($op, $values, $span)?
                }
                $(
                    $(#[$tail_attribute])*
                    $tail_pattern $(if $tail_guard)? => $tail_body,
                )*
            };
            Ok(ControlFlow::Continue(value))
        }

        pub(super) fn lowered_module_op_supported($op: RuntimeOp) -> bool {
            if process_module::is_prim($op)
                || unix_module::is_prim($op)
                || linux_module::is_prim($op)
                || linux_module::is_net_prim($op)
                || linux_module::storage::handles($op)
            {
                return true;
            }
            match $op {
                $(
                    $(#[$pre_attribute])*
                    $pre_pattern if lowered_module_op_support_guard!($pre_class; $op) => true,
                )*
                with_unix_call_rows! {
                    lowered_unix_route_pattern! {}
                    using $this, $op, $values, $span
                } => true,
                $(
                    $(#[$post_attribute])*
                    $post_pattern if lowered_module_op_support_guard!($post_class; $op) => true,
                )*
                with_linux_call_rows! {
                    lowered_linux_route_pattern! {}
                    using $this, $op, $values, $span
                } => true,
                $(
                    $(#[$tail_attribute])*
                    $tail_pattern if lowered_module_op_support_guard!($tail_class; $op) => true,
                )*
                _ => false,
            }
        }
    };
}

#[cfg(feature = "native-tests")]
impl Evaluator {
    fn lowered_test_run_script(
        &mut self,
        ctx: &RecordMap,
        source: &str,
        args: &[std::ffi::OsString],
        env: &BTreeMap<String, Vec<u8>>,
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
        script_args: &[std::ffi::OsString],
        env: &BTreeMap<String, Vec<u8>>,
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
        script_args: &[std::ffi::OsString],
        env: &BTreeMap<String, Vec<u8>>,
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
        script_args: &[std::ffi::OsString],
        env: &BTreeMap<String, Vec<u8>>,
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
        let route_witness_output = if let Some(invocation) = &self.route_witness_invocation {
            if kind != NativeTestRunKind::Xsh
                || self.linux_fake.is_some()
                || self.unix_fake.is_some()
            {
                return Err(RuntimeError::new(
                    error_kind,
                    "route-witness capture requires a direct xsh child without test fakes",
                )
                .with_span(span));
            }
            Some(
                tempfile::TempDir::new_in(&invocation.output_dir).map_err(|error| {
                    RuntimeError::new(
                        error_kind,
                        format!("failed to create child route-witness directory: {error}"),
                    )
                    .with_span(span)
                })?,
            )
        } else {
            None
        };
        let route_witness_transport = match (
            &self.route_witness_invocation,
            route_witness_output.as_ref(),
        ) {
            (Some(invocation), Some(output_dir)) => Some(
                super::route_witness::NativeTestRouteWitnessTransport::new(
                    &invocation.test_id,
                    output_dir.path(),
                    source.as_bytes(),
                )
                .map_err(|error| {
                    RuntimeError::new(
                        error_kind,
                        format!("failed to prepare route-witness descriptor: {error}"),
                    )
                    .with_span(span)
                })?,
            ),
            (None, None) => None,
            _ => {
                return Err(RuntimeError::new(
                    error_kind,
                    "route-witness context and child directory are inconsistent",
                )
                .with_span(span));
            }
        };
        let value = super::route_witness::with_transport(route_witness_transport, || {
            host(NativeTestRunRequest {
                kind,
                script_path,
                source: source.to_string(),
                tool_args: tool_args.to_vec(),
                script_args: script_args.to_vec(),
                env: env.clone(),
                force_frames: self.force_frames,
                stdin: stdin.to_vec(),
                linux_fake: self.linux_fake.as_deref().cloned(),
                unix_fake: self.unix_fake.as_deref().cloned(),
                cancellation: self.test_cancellation.clone(),
                span,
            })
        })?;
        if let Some(output_dir) = route_witness_output {
            let child_succeeded = matches!(
                &value,
                Value::Record(record)
                    if record.get("success") == Some(&Value::Bool(true))
            );
            if child_succeeded
                || super::route_witness::artifact_exists(output_dir.path())
                    .map_err(|message| RuntimeError::new(error_kind, message).with_span(span))?
            {
                super::route_witness::validate_artifact(
                    output_dir.path(),
                    &self
                        .route_witness_invocation
                        .as_ref()
                        .expect("route-witness output has its invocation context")
                        .test_id,
                    source.as_bytes(),
                    self.force_frames,
                )
                .map_err(|message| RuntimeError::new(error_kind, message).with_span(span))?;
                let _ = output_dir.keep();
            }
        }
        match lowered_runtime_value(value, span)? {
            LoweredValue::Record(record) => Ok(take_shared(record)),
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

/// Why a finished script run is not the one a test expected: every mismatch
/// of the exit status and of the required output fragments, followed by the
/// whole captured output, so one failed run shows everything a rerun would.
/// `None` when the run matches.
#[cfg(feature = "native-tests")]
fn script_expectation_failure(
    output: &BTreeMap<Arc<str>, LoweredValue>,
    status: i64,
    stderr: &[String],
    stdout: &[String],
) -> Option<String> {
    let text = |field: &str| output.get(field).and_then(lowered_str_value).unwrap_or("");
    let actual_status = match output.get("status") {
        Some(LoweredValue::Int(status)) => Some(*status),
        _ => None,
    };
    let mut mismatches = Vec::new();
    if actual_status != Some(status) {
        mismatches.push(format!("  expected status {status}"));
    }
    for (stream, fragments) in [("stderr", stderr), ("stdout", stdout)] {
        let captured = text(stream);
        for fragment in fragments {
            if !captured.contains(fragment.as_str()) {
                mismatches.push(format!("  {stream} does not contain {fragment:?}"));
            }
        }
    }
    if mismatches.is_empty() {
        return None;
    }
    let actual_status =
        actual_status.map_or_else(|| "unknown".to_string(), |status| status.to_string());
    Some(format!(
        "test.expect: the script did not behave as expected\n{}\nstatus: {actual_status}\nstdout:\n{}\nstderr:\n{}",
        mismatches.join("\n"),
        text("stdout"),
        text("stderr"),
    ))
}

#[cfg(all(test, feature = "native-tests"))]
mod route_witness_capture_tests {
    use super::*;
    use crate::runtime::eval::route_witness::InvocationContext;

    fn scratch() -> tempfile::TempDir {
        let scratch = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".work/tmp");
        std::fs::create_dir_all(&scratch).expect("create repository scratch directory");
        tempfile::tempdir_in(scratch).expect("create native test fixture")
    }

    fn child_result(success: bool, status: i64) -> Value {
        Value::Record(RecordMap::from([
            (Arc::from("success"), Value::Bool(success)),
            (Arc::from("status"), Value::Int(status)),
            (Arc::from("stdout"), Value::Str(Arc::from("child stdout\n"))),
            (Arc::from("stderr"), Value::Str(Arc::from("child stderr\n"))),
        ]))
    }

    fn run_child(
        write_artifact: bool,
        success: bool,
        status: i64,
        forced: bool,
    ) -> (
        tempfile::TempDir,
        PathBuf,
        Result<BTreeMap<Arc<str>, LoweredValue>, RuntimeError>,
    ) {
        let fixture = scratch();
        let output_dir = fixture.path().join("route-witnesses");
        std::fs::create_dir(&output_dir).expect("create route-witness parent");
        let source = "let empty: Map[Str, Int] = {}\n";
        let test_id = "tests/xsh/expression-route-oracles.xsh::test_expr_empty_map";
        let host = Arc::new(move |request: NativeTestRunRequest| {
            if write_artifact {
                crate::runtime::eval::route_witness::write_active_artifact_for_test(
                    request.source.as_bytes(),
                    request.force_frames,
                )
                .expect("write authentic child artifact through active transport");
            }
            Ok(child_result(success, status))
        });
        let mut evaluator = Evaluator::new(Vec::new()).with_native_test_host(host);
        evaluator.force_frames = forced;
        evaluator.route_witness_invocation = Some(InvocationContext {
            test_id: test_id.to_string(),
            output_dir: output_dir.clone(),
        });
        let ctx = RecordMap::from([(
            Arc::from("temp_root"),
            Value::Path(PathValue::new(path_bytes(fixture.path())).expect("valid fixture path")),
        )]);
        let span = Span::new(crate::source::SourceId::new(0), 0, 1);
        let output = evaluator.lowered_native_test_run(
            NativeTestRunKind::Xsh,
            &ctx,
            source,
            &[],
            &[],
            &BTreeMap::new(),
            &[],
            "child.xsh",
            span,
            "test-run-xsh",
        );
        (fixture, output_dir, output)
    }

    #[test]
    fn nonzero_executed_child_retains_its_authenticated_route_witness() {
        for forced in [false, true] {
            let (_fixture, output_dir, output) = run_child(true, false, 23, forced);
            let output = output.expect("native test host result is preserved");

            assert_eq!(output.get("success"), Some(&LoweredValue::Bool(false)));
            assert_eq!(output.get("status"), Some(&LoweredValue::Int(23)));
            assert_eq!(
                output.get("stdout"),
                Some(&LoweredValue::Str(Arc::from("child stdout\n")))
            );
            assert_eq!(
                output.get("stderr"),
                Some(&LoweredValue::Str(Arc::from("child stderr\n")))
            );

            let retained = std::fs::read_dir(&output_dir)
                .expect("inspect route-witness output")
                .map(|entry| entry.expect("read route-witness output entry").path())
                .collect::<Vec<_>>();
            assert_eq!(retained.len(), 1);
            assert!(crate::runtime::eval::route_witness::artifact_path(&retained[0]).is_file());
        }
    }

    #[test]
    fn nonzero_child_without_an_artifact_preserves_its_result_without_coverage() {
        let (_fixture, output_dir, output) = run_child(false, false, 2, false);
        let output = output.expect("native test host result is preserved");

        assert_eq!(output.get("success"), Some(&LoweredValue::Bool(false)));
        assert_eq!(output.get("status"), Some(&LoweredValue::Int(2)));
        assert_eq!(
            output.get("stdout"),
            Some(&LoweredValue::Str(Arc::from("child stdout\n")))
        );
        assert_eq!(
            output.get("stderr"),
            Some(&LoweredValue::Str(Arc::from("child stderr\n")))
        );
        assert_eq!(
            std::fs::read_dir(output_dir)
                .expect("inspect route-witness output")
                .count(),
            0,
        );
    }

    #[test]
    fn successful_child_without_an_artifact_still_fails_capture() {
        let (_fixture, _output_dir, error) = run_child(false, true, 0, false);

        assert!(error
            .expect_err("successful child must write route-witness artifact")
            .message
            .contains("failed to read route-witness artifact"));
    }
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

/// Native path fragments retain their bytes; other fragments use ordinary
/// display conversion. Width affects padding, never the native fragment data.
fn push_lowered_native_fmt_value(
    bytes: &mut Vec<u8>,
    value: &LoweredValue,
    span: Span,
    spec: Option<&FormatSpec>,
) -> Result<(), RuntimeError> {
    if let LoweredValue::Path(path) = value {
        if let Some(spec) = spec {
            let padding = spec.width.saturating_sub(path.display().chars().count());
            match spec.kind {
                FormatSpecKind::RightAlign => {
                    bytes.extend(std::iter::repeat_n(b' ', padding));
                    bytes.extend_from_slice(&path.bytes);
                }
                FormatSpecKind::LeftAlign => {
                    bytes.extend_from_slice(&path.bytes);
                    bytes.extend(std::iter::repeat_n(b' ', padding));
                }
                FormatSpecKind::ZeroPad => {
                    if let Some(rest) = path.bytes.strip_prefix(b"-") {
                        bytes.push(b'-');
                        bytes.extend(std::iter::repeat_n(b'0', padding));
                        bytes.extend_from_slice(rest);
                    } else {
                        bytes.extend(std::iter::repeat_n(b'0', padding));
                        bytes.extend_from_slice(&path.bytes);
                    }
                }
            }
        } else {
            bytes.extend_from_slice(&path.bytes);
        }
    } else {
        let mut text = String::new();
        push_lowered_fmt_value(&mut text, value, span, spec)?;
        bytes.extend_from_slice(text.as_bytes());
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

// Synthesized fields on compact records have the same visible values as stored
// fields. Validation and method reads share this owned view.
pub(super) fn lowered_record_field_value(
    value: &LoweredValue,
    field: &str,
) -> Option<LoweredValue> {
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
        LoweredValue::Record(entries) => {
            let mut entries = take_shared(entries);
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
        LoweredValue::RecordVec(entries) => {
            let mut entries = take_shared(entries);
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

#[allow(clippy::large_enum_variant)]
/// The delays a `retry` takes between its attempts.
enum LoweredRetryDelays {
    /// The written delays, one after each failed attempt but the last.
    Listed(Vec<DurationValue>),
    /// Delays that double from the first interval to the cap. Another delay
    /// is taken only while it would end within the limit, measured from the
    /// start of the first attempt.
    Backoff {
        next: u64,
        cap: u64,
        limit: Duration,
        started: Instant,
    },
}

impl LoweredRetryDelays {
    fn backoff(first: u64, cap: u64, limit: u64) -> Self {
        Self::Backoff {
            next: first,
            cap,
            limit: Duration::from_millis(limit),
            started: Instant::now(),
        }
    }

    /// The delay after failed attempt `attempt_index` (from zero), or `None`
    /// when that attempt was the last.
    fn next_delay(&mut self, attempt_index: usize) -> Option<u64> {
        match self {
            Self::Listed(delays) => delays.get(attempt_index).map(|delay| delay.millis),
            Self::Backoff {
                next,
                cap,
                limit,
                started,
            } => {
                let delay = *next;
                let ends = started
                    .elapsed()
                    .checked_add(Duration::from_millis(delay))?;
                if ends > *limit {
                    return None;
                }
                *next = backoff_step(delay, *cap);
                Some(delay)
            }
        }
    }

    /// The most attempts the schedule allows: for a backoff, the count if
    /// every attempt took no time.
    fn max_attempts(&self) -> usize {
        match self {
            Self::Listed(delays) => delays.len() + 1,
            Self::Backoff {
                next, cap, limit, ..
            } => {
                let limit = u64::try_from(limit.as_millis()).unwrap_or(u64::MAX);
                let mut delay = *next;
                let mut waited = 0u64;
                let mut attempts = 1usize;
                loop {
                    let Some(total) = waited.checked_add(delay).filter(|total| *total <= limit)
                    else {
                        return attempts;
                    };
                    let following = backoff_step(delay, *cap);
                    if following == delay {
                        // The delay no longer changes: the rest of the limit
                        // divides into equal steps.
                        let more = (limit - total).checked_div(delay).unwrap_or(u64::MAX);
                        return attempts
                            .saturating_add(1)
                            .saturating_add(usize::try_from(more).unwrap_or(usize::MAX));
                    }
                    waited = total;
                    attempts += 1;
                    delay = following;
                }
            }
        }
    }
}

/// The delay after `delay` in a backoff: twice as long, and never above the
/// cap. A first interval above the cap is taken as written, once.
fn backoff_step(delay: u64, cap: u64) -> u64 {
    if delay > cap / 2 { cap } else { delay * 2 }
}

fn lowered_trace_error_from_value(value: &Value) -> TraceError {
    TraceError::from_value(value)
}

fn lowered_elf_info_value(path: PathValue, info: elf_module::ElfInfo) -> LoweredValue {
    LoweredValue::Record(Arc::new(BTreeMap::from([
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
                        LoweredValue::Record(Arc::new(BTreeMap::from([
                            (Arc::from("tag"), LoweredValue::Str(entry.tag.into())),
                            (Arc::from("value"), LoweredValue::Int(entry.value)),
                        ])))
                    })
                    .collect(),
            ),
        ),
    ])))
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
    LoweredValue::Record(Arc::new(BTreeMap::from([
        (Arc::from("status"), LoweredValue::Status(Box::new(status))),
        (
            Arc::from("duration_ms"),
            LoweredValue::Int(wall_ns / 1_000_000),
        ),
        (Arc::from("wall_ns"), LoweredValue::Int(wall_ns)),
        (Arc::from("user_ns"), LoweredValue::Int(user_ns)),
        (Arc::from("system_ns"), LoweredValue::Int(system_ns)),
    ])))
}

fn read_host_path_bytes_vec(path: &Path, span: Span) -> Result<Vec<u8>, RuntimeError> {
    std::fs::read(path).map_err(|error| {
        RuntimeError::new("fs-read", format!("{}: {error}", path.display()))
            .with_host_facet(&error)
            .with_span(span)
    })
}

/// The UTF-8 text of a file: the one read behind `read_text` and `read_lines`,
/// so the two cannot disagree on which files they accept or how they fail.
fn read_host_path_text(path: &Path, span: Span) -> Result<String, RuntimeError> {
    String::from_utf8(read_host_path_bytes_vec(path, span)?).map_err(|error| {
        RuntimeError::new(
            "invalid-utf8",
            format!(
                "file is not valid UTF-8 at byte {}",
                error.utf8_error().valid_up_to()
            ),
        )
        .with_span(span)
    })
}

fn read_host_path_bytes(path: &Path, span: Span) -> Result<Arc<[u8]>, RuntimeError> {
    read_host_path_bytes_unnamed(path, span).map_err(|mut error| {
        error.message = format!("{}: {}", path.display(), error.message);
        error
    })
}

fn read_host_path_bytes_unnamed(path: &Path, span: Span) -> Result<Arc<[u8]>, RuntimeError> {
    let mut file = std::fs::File::open(path)
        .map_err(|error| RuntimeError::host("fs-read", &error).with_span(span))?;
    let len = file
        .metadata()
        .map_err(|error| RuntimeError::host("fs-read", &error).with_span(span))?
        .len()
        .try_into()
        .map_err(|_| RuntimeError::new("fs-read", "file is too large").with_span(span))?;
    if len == 0 {
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|error| RuntimeError::host("fs-read", &error).with_span(span))?;
        return Ok(bytes.into());
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
                return Err(RuntimeError::host("fs-read", &error).with_span(span));
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
                file.read_to_end(&mut value)
                    .map_err(|error| RuntimeError::host("fs-read", &error).with_span(span))?;
                return Ok(value.into());
            }
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) => {
                return Err(RuntimeError::host("fs-read", &error).with_span(span));
            }
        }
    }
}

fn initialized_prefix_to_arc(bytes: &[std::mem::MaybeUninit<u8>], initialized: usize) -> Arc<[u8]> {
    let initialized =
        unsafe { std::slice::from_raw_parts(bytes.as_ptr().cast::<u8>(), initialized) };
    initialized.into()
}

pub(super) fn lowered_write_json_str(value: &str, output: &mut String) {
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

fn read_host_path_string(path: &Path, operation: &str, span: Span) -> Result<String, RuntimeError> {
    let bytes = std::fs::read(path)
        .map_err(|error| RuntimeError::host(operation, &error).with_span(span))?;
    String::from_utf8(bytes)
        .map_err(|error| RuntimeError::new(operation, error.to_string()).with_span(span))
}

#[cfg(feature = "native-tests")]
fn create_host_dir_all(path: &Path, operation: &str, span: Span) -> Result<(), RuntimeError> {
    std::fs::create_dir_all(path)
        .map_err(|error| RuntimeError::host(operation, &error).with_span(span))
}

/// Builds a `module-load` error naming the module and its first diagnostics,
/// so a failed `module.load` points at the broken file instead of only the
/// loading call site.
fn module_load_failure(
    stage: &str,
    display_path: &str,
    diagnostics: &[crate::diagnostic::Diagnostic],
    sources: &crate::source::SourceMap,
    span: Span,
) -> RuntimeError {
    const SHOWN: usize = 3;
    let mut causes = diagnostics
        .iter()
        .take(SHOWN)
        .map(|diagnostic| {
            let primary = diagnostic
                .labels
                .iter()
                .find(|label| label.style == crate::diagnostic::LabelStyle::Primary)
                .or_else(|| diagnostic.labels.first());
            let location = primary
                .map(|label| label.span)
                .or(diagnostic.span)
                .and_then(|span| sources.location(span.source_id, span.start()))
                .map(|location| {
                    format!("{}:{}:{}: ", location.file, location.line, location.column)
                })
                .unwrap_or_default();
            let code = diagnostic
                .code
                .map(|code| format!("{code}: "))
                .unwrap_or_default();
            let detail = primary
                .and_then(|label| label.message.as_deref())
                .map(|message| format!(" ({message})"))
                .unwrap_or_default();
            format!("{location}{code}{}{detail}", diagnostic.message)
        })
        .collect::<Vec<_>>();
    if diagnostics.len() > SHOWN {
        causes.push(format!("and {} more", diagnostics.len() - SHOWN));
    }
    RuntimeError::new(
        "module-load",
        format!("module `{display_path}` {stage}: {}", causes.join("; ")),
    )
    .with_span(span)
}

/// Process-wide prepared form of a dynamically loaded module.
///
/// Parsing, checking, indexed encoding, and preparing the export harvest depend
/// only on the module's source files and module roots, so evaluators that load
/// the same unchanged files share one preparation. Each load still executes the
/// harvest in a fresh child evaluator, so top-level initializers keep per-load
/// effects. Names inside the cached programs are interned under the cached
/// arenas' own symbol owners, which the cache keeps alive.
struct PreparedDynamicModule {
    module_roots: Arc<[PathBuf]>,
    entry_bytes: Vec<u8>,
    imported_files: Vec<(PathBuf, Vec<u8>)>,
    arena: ArenaProgram,
    declarations: crate::sema::check::CompactDeclOutput,
    module_program: Arc<super::indexed::full::FullProgram>,
    exported_functions: Vec<(Name, bool)>,
    exported_let_names: Vec<Name>,
    harvest: Option<PreparedModuleHarvest>,
}

/// A loaded module's exports, valid while its prepared program still matches
/// the files on disk.
#[derive(Clone)]
pub(super) struct CachedModule {
    prepared: Arc<PreparedDynamicModule>,
    record: RecordMap,
}

struct PreparedModuleHarvest {
    plan: super::CompactIndexedRunPlan,
    shared: Arc<super::LoweredSharedState>,
    _arena: ArenaProgram,
}

type PreparedDynamicModules = std::sync::Mutex<FxHashMap<String, Arc<PreparedDynamicModule>>>;

fn prepared_dynamic_modules() -> &'static PreparedDynamicModules {
    static MODULES: std::sync::OnceLock<PreparedDynamicModules> = std::sync::OnceLock::new();
    MODULES.get_or_init(Default::default)
}

fn lookup_prepared_dynamic_module(
    key: &str,
    module_roots: &Arc<[PathBuf]>,
    entry_bytes: &[u8],
) -> Option<Arc<PreparedDynamicModule>> {
    let prepared = prepared_dynamic_modules()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(key)
        .cloned()?;
    let unchanged = prepared.module_roots == *module_roots
        && prepared.entry_bytes == entry_bytes
        && prepared
            .imported_files
            .iter()
            .all(|(path, bytes)| std::fs::read(path).is_ok_and(|current| current == *bytes));
    unchanged.then_some(prepared)
}

fn store_prepared_dynamic_module(key: &str, prepared: &Arc<PreparedDynamicModule>) {
    prepared_dynamic_modules()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(key.to_string(), Arc::clone(prepared));
}

fn prepare_dynamic_module(
    key: &str,
    display_path: &str,
    bytes: Vec<u8>,
    module_roots: &Arc<[PathBuf]>,
    span: Span,
) -> Result<PreparedDynamicModule, RuntimeError> {
    use crate::loader::{
        StdlibLinkage, entry_source_from_bytes, parse_load_entry_source_arena_only_with_linkage,
    };

    let entry_bytes = bytes.clone();
    let entry_source = entry_source_from_bytes(display_path, bytes);
    if !entry_source.diagnostics.is_empty() {
        return Err(module_load_failure(
            "failed to load",
            display_path,
            &entry_source.diagnostics,
            &entry_source.sources,
            span,
        ));
    }
    // A dynamically loaded module never prepares standard-library source:
    // the loading program prepared every applicable implementation before
    // execution started, and this module links its standard calls to those
    // already verified functions.
    let (module_sources, mut parsed) = parse_load_entry_source_arena_only_with_linkage(
        display_path,
        entry_source,
        module_roots.to_vec(),
        StdlibLinkage::LinkToPrepared,
    );
    if !parsed.diagnostics.is_empty() {
        return Err(module_load_failure(
            "failed to parse",
            display_path,
            &parsed.diagnostics,
            &module_sources,
            span,
        ));
    }
    let owner = parsed.arena.symbol_owner().clone();
    owner.with_current(|| {
        parsed.arena.root_nominal_namespace = Some(Name::intern(key));
        validate_dynamic_module_top_level(&parsed.arena, display_path, span)?;
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
            .expect("loaded module source exists");

        let doc_diagnostics =
            crate::sema::check::Checker::check_public_module_docs(&parsed.arena, &module_text);
        if let Some(diagnostic) = doc_diagnostics.first() {
            return Err(RuntimeError::new(
                "module-load",
                format!(
                    "loaded module has undocumented exports: {}",
                    diagnostic.message
                ),
            )
            .with_span(span));
        }

        let declarations = crate::sema::check::Checker::check_compact_declarations(
            &parsed.arena,
            &module_sources,
            module_source_id,
        );
        if !declarations.diagnostics.is_empty() {
            return Err(module_load_failure(
                "failed to check",
                display_path,
                &declarations.diagnostics,
                &module_sources,
                span,
            ));
        }
        let module_program = Arc::new(
            super::indexed::full::FullBuilder::build_compact_external_stdlib(
                &parsed.arena,
                &declarations,
                &module_text,
                Arc::new(module_sources.clone()),
                module_source_id,
            )
            .map_err(|error| {
                let message = match error.detail {
                    Some(detail) => format!(
                        "loaded module could not encode `{}`: {detail}",
                        error.construct
                    ),
                    None => format!("loaded module could not encode `{}`", error.construct),
                };
                RuntimeError::new(
                    "module-load",
                    message,
                )
                .with_span(span)
            })?,
        );
        let exported_functions = Evaluator::module_namespace_export_functions(&parsed.arena);
        let exported_let_names = Evaluator::module_namespace_export_let_names(&parsed.arena);

        let harvest_text = Evaluator::module_harvest_source(&parsed.arena, &module_text);
        let harvest_entry = crate::loader::entry_source_from_text(display_path, harvest_text);
        let (harvest_sources, mut harvest_parsed) =
            crate::loader::parse_load_entry_source_arena_only(
                display_path,
                harvest_entry,
                module_roots.to_vec(),
            );
        let harvest = if harvest_parsed.diagnostics.is_empty() {
            let harvest_source_id = harvest_sources
                .files()
                .first()
                .map(crate::source::SourceFile::id)
                .unwrap_or(module_source_id);
            harvest_parsed.arena.root_nominal_namespace = Some(
                harvest_parsed
                    .arena
                    .symbol_owner()
                    .with_current(|| Name::intern(key)),
            );
            let mut child = Evaluator::new_with_sources(Vec::new(), harvest_sources);
            let plan = child
                .prepare_compact_indexed_only(&harvest_parsed.arena, harvest_source_id)
                .ok_or_else(|| {
                    RuntimeError::new(
                        "module-load",
                        "loaded module could not encode as indexed IR",
                    )
                    .with_span(span)
                })?;
            Some(PreparedModuleHarvest {
                plan,
                shared: child.lowered_shared_state(),
                _arena: harvest_parsed.arena,
            })
        } else {
            None
        };

        let imported_files = module_sources
            .files()
            .iter()
            .skip(1)
            .filter_map(|file| {
                let path = PathBuf::from(file.name());
                path.is_file()
                    .then(|| (path, file.text().as_bytes().to_vec()))
            })
            .collect();
        Ok(PreparedDynamicModule {
            module_roots: Arc::clone(module_roots),
            entry_bytes,
            imported_files,
            arena: parsed.arena,
            declarations,
            module_program,
            exported_functions,
            exported_let_names,
            harvest,
        })
    })
}

fn validate_dynamic_module_top_level(
    program: &ArenaProgram,
    display_path: &str,
    span: Span,
) -> Result<(), RuntimeError> {
    for stmt in program.statement_ids() {
        let code = match program.arena.stmt(program.arena.core_stmt_id(stmt)).kind {
            crate::syntax::arena::ArenaStmtKind::SignalHook(_) => {
                Some(DiagnosticCode::CheckSignalHookModule)
            }
            crate::syntax::arena::ArenaStmtKind::Var { .. }
            | crate::syntax::arena::ArenaStmtKind::Command(_) => {
                Some(DiagnosticCode::CheckModuleTopLevel)
            }
            crate::syntax::arena::ArenaStmtKind::Expr(expr)
                if matches!(
                    program.arena.expr(expr).kind,
                    crate::syntax::arena::ArenaExprKind::ValueBlock(_)
                ) =>
            {
                Some(DiagnosticCode::CheckModuleTopLevel)
            }
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

fn lowered_optional_int_arg(
    value: Option<&LoweredValue>,
    operation: &str,
    span: Span,
) -> Result<Option<i64>, RuntimeError> {
    match value {
        None | Some(LoweredValue::Null) => Ok(None),
        Some(LoweredValue::Int(value)) => Ok(Some(*value)),
        Some(other) => Err(RuntimeError::new(
            "type-error",
            format!(
                "{operation} expected Int or null, found {}",
                other.type_name()
            ),
        )
        .with_span(span)),
    }
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

fn lowered_json_path_arg(
    value: &LoweredValue,
    operation: &str,
    span: Span,
) -> Result<Value, RuntimeError> {
    let path = value.clone().into_value();
    match path {
        Value::List(_) => Ok(path),
        other => Err(RuntimeError::new(
            "type-error",
            format!(
                "{operation} path expected List[Any], found {}",
                other.type_name()
            ),
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

/// An optional list of argv words, each as the bytes a child receives: text
/// as UTF-8 and a Path as its native bytes.
#[cfg(feature = "native-tests")]
fn lowered_optional_argv_words(
    value: Option<LoweredValue>,
    operation: &str,
    span: Span,
) -> Result<Vec<std::ffi::OsString>, RuntimeError> {
    match value {
        Some(value) => Ok(lowered_argv_list(Some(value), operation, span)?
            .into_iter()
            .map(std::ffi::OsString::from_vec)
            .collect()),
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
                take_shared(fields)
                    .into_iter()
                    .map(|(key, value)| (Name::intern(key.as_ref()), value.into_value()))
                    .collect(),
            ))
        }
        Some(LoweredValue::RecordVec(fields)) => Ok(RecordMap::from_name_values(
            take_shared(fields)
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
fn lowered_optional_env_record(
    value: Option<LoweredValue>,
    operation: &str,
    span: Span,
) -> Result<BTreeMap<String, Vec<u8>>, RuntimeError> {
    let Some(value) = value else {
        return Ok(BTreeMap::new());
    };
    let fields = match value {
        LoweredValue::Record(fields) | LoweredValue::Module(fields) => fields,
        LoweredValue::RecordVec(fields) => Arc::new(
            take_shared(fields)
                .into_iter()
                .map(|(name, value)| (Arc::<str>::from(name.as_str().as_str()), value))
                .collect::<BTreeMap<_, _>>(),
        ),
        other => {
            return Err(RuntimeError::new(
                "type-error",
                format!("{operation} expected Record, found {}", other.type_name()),
            )
            .with_span(span));
        }
    };

    let mut env = BTreeMap::new();
    for (key, value) in fields.iter() {
        // Text as UTF-8, a Path as its native bytes, and a list of paths as
        // a search path; nothing else is an environment value here.
        let bytes = match value {
            LoweredValue::Path(path) => path.bytes.clone(),
            LoweredValue::List(_) | LoweredValue::SharedList(_) => {
                value_to_env_bytes(value.clone().into_value(), span)?
            }
            value => match lowered_str_value(value) {
                Some(text) => text.as_bytes().to_vec(),
                None => {
                    return Err(RuntimeError::new(
                        "type-error",
                        format!(
                            "{operation} env field `{key}` expected Str or Path, found {}",
                            value.type_name()
                        ),
                    )
                    .with_span(span));
                }
            },
        };
        env.insert(key.to_string(), bytes);
    }
    Ok(env)
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

/// The bytes one environment value hands to a child: a Path's native bytes,
/// and for every other value its display text, which is what an argv word
/// carries. Converting a Path through its display text would replace every
/// byte sequence that is not UTF-8. A list is a search path: its entries
/// must be paths, joined by the separator.
fn lowered_env_value_bytes(value: &LoweredValue, span: Span) -> Result<Vec<u8>, RuntimeError> {
    if let LoweredValue::Path(path) = value {
        return Ok(path.bytes.clone());
    }
    if matches!(value, LoweredValue::List(_) | LoweredValue::SharedList(_)) {
        return value_to_env_bytes(value.clone().into_value(), span);
    }
    let mut text = String::new();
    push_lowered_display(&mut text, value, span)?;
    Ok(text.into_bytes())
}

fn lowered_env_record_arg(
    value: LoweredValue,
    operation: &str,
    span: Span,
) -> Result<BTreeMap<String, Vec<u8>>, RuntimeError> {
    let mut env = BTreeMap::new();
    match value {
        LoweredValue::Record(fields) => {
            for (name, value) in fields.iter() {
                env.insert(name.to_string(), lowered_env_value_bytes(value, span)?);
            }
        }
        LoweredValue::RecordVec(fields) => {
            for (name, value) in fields.iter() {
                env.insert(name.to_string(), lowered_env_value_bytes(value, span)?);
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
        match value {
            LoweredValue::Bytes(bytes) => redirections.push(CommandRedirection::Input { bytes }),
            // A child owns its stdin bytes after the evaluator frame ends.
            LoweredValue::BytesView(view) => redirections.push(CommandRedirection::Input {
                bytes: Arc::from(view.as_slice()),
            }),
            value => redirections.push(CommandRedirection::File {
                stream: CommandRedirectionStream::Stdin,
                mode: CommandRedirectionMode::Read,
                path: lowered_path_like_arg(value, operation, span)?,
            }),
        }
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

fn lowered_accepted_exit_codes(
    value: LoweredValue,
    span: Span,
) -> Result<crate::runtime::process::AcceptedExitCodes, RuntimeError> {
    let items = match &value {
        LoweredValue::List(items) => items.as_slice(),
        LoweredValue::SharedList(items) => items.as_slice(),
        _ => {
            return Err(
                RuntimeError::new("accept-policy", "accept must be a nonempty List[Int]")
                    .with_span(span),
            );
        }
    };
    let codes = items
        .iter()
        .map(|item| match item {
            LoweredValue::Int(code) => Ok(*code),
            _ => {
                Err(RuntimeError::new("accept-policy", "accept items must be Int").with_span(span))
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    crate::runtime::process::AcceptedExitCodes::new(&codes)
        .map_err(|error| RuntimeError::new(error.kind, error.message).with_span(span))
}

fn empty_command_plan() -> CommandPlan {
    CommandPlan {
        target: Vec::new(),
        argv: Vec::new(),
        cwd: None,
        env: BTreeMap::new(),
        redirections: Vec::new(),
        timeout: None,
        cpu_max: None,
        accepted_exit_codes: None,
        detach: false,
        new_session: false,
        ignore_hup: false,
        same_group: false,
    }
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
    accept: Option<LoweredValue>,
    same_group: Option<LoweredValue>,
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
    let same_group = lowered_bool_arg_or(same_group, false, "process.command_argv", span)?;
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

    let mut plan = empty_command_plan();
    plan.target = target;
    plan.argv = argv;
    plan.cwd = cwd;
    plan.env = env;
    plan.redirections = redirections;
    plan.timeout = timeout;
    plan.cpu_max = cpu_max;
    plan.accepted_exit_codes = accept
        .map(|value| lowered_accepted_exit_codes(value, span))
        .transpose()?;
    plan.detach = detach;
    plan.new_session = new_session;
    plan.ignore_hup = ignore_hup;
    plan.same_group = same_group;
    Ok(LoweredValue::Command(Box::new(plan)))
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

fn unix_fake_tty_attrs() -> Value {
    Value::Record(RecordMap::from([
        (Arc::from("iflag"), Value::Int(0)),
        (Arc::from("oflag"), Value::Int(0)),
        (Arc::from("cflag"), Value::Int(0)),
        (Arc::from("lflag"), Value::Int(0)),
        (Arc::from("line"), Value::Int(0)),
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
    owner: &Arc<()>,
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
        if !Arc::ptr_eq(&handle.owner, owner) {
            return Ok(Err(RunError::new("unknown", "process handle is no longer live").with_span(span)));
        }
        handles.push(*handle);
    }
    Ok(Ok(handles))
}

fn lowered_process_wait_any_record(index: usize, pid: u32, status: ProcessStatus) -> LoweredValue {
    LoweredValue::Record(Arc::new(BTreeMap::from([
        (Arc::from("index"), LoweredValue::Int(index as i64)),
        (Arc::from("pid"), LoweredValue::Int(pid as i64)),
        (Arc::from("status"), LoweredValue::Status(Box::new(status))),
    ])))
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
        LoweredValue::Error(error) | LoweredValue::ResultErr(error) => error
            .error_message()
            .filter(|_| matches!(error.as_ref(), Value::Error(_)))
            .unwrap_or("error")
            .to_string(),
        _ => value.type_name().to_string(),
    }
}

fn lowered_pipeline_record_list(
    value: &LoweredValue,
    span: Span,
) -> Result<Vec<Arc<BTreeMap<Arc<str>, LoweredValue>>>, RuntimeError> {
    match value {
        LoweredValue::List(items) => items
            .iter()
            .map(|item| match item {
                LoweredValue::Record(record) => Ok(Arc::clone(record)),
                LoweredValue::RecordVec(record) => Ok(Arc::new(
                    record
                        .iter()
                        .map(|(key, value)| {
                            (Arc::<str>::from(key.as_str().as_str()), value.clone())
                        })
                        .collect::<BTreeMap<_, _>>(),
                )),
                LoweredValue::Map(map) => Ok(Arc::new(
                    map.iter()
                        .map(|(k, v)| {
                            k.as_str()
                                .map(|key| (Arc::from(key), v.clone()))
                                .ok_or_else(|| {
                                    RuntimeError::new(
                                        "type-error",
                                        "table rows require Str map keys",
                                    )
                                    .with_span(span)
                                })
                        })
                        .collect::<Result<BTreeMap<_, _>, _>>()?,
                )),
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

fn lowered_root_id(root: &LoweredValue, owner: &Arc<()>, span: Span) -> Result<i64, RuntimeError> {
    match root {
        LoweredValue::FsRoot(handle) if Arc::ptr_eq(&handle.owner, owner) => Ok(handle.id),
        LoweredValue::FsRoot(_) => {
            Err(RuntimeError::new("fs-root", "root handle is not active").with_span(span))
        }
        _ => Err(RuntimeError::new("type-error", "expected FsRoot capability").with_span(span)),
    }
}

fn lowered_fs_root_children_result(
    result: fs_module::RootChildrenResult,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    let children = result
        .children
        .into_iter()
        .map(|path| {
            path_value_from_pathbuf(path)
                .map(LoweredValue::Path)
                .map_err(|error| error.with_span(span))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(LoweredValue::Record(Arc::new(BTreeMap::from([
        (Arc::from("state"), LoweredValue::Str(result.state.into())),
        (
            Arc::from("enumeration_succeeded"),
            LoweredValue::Bool(result.enumeration_succeeded),
        ),
        (Arc::from("children"), LoweredValue::List(children)),
        (
            Arc::from("errno"),
            result.errno.map_or(LoweredValue::Null, LoweredValue::Int),
        ),
        (
            Arc::from("error_kind"),
            result
                .error_kind
                .map_or(LoweredValue::Null, |kind| LoweredValue::Str(kind.into())),
        ),
    ]))))
}

fn lowered_fs_root_read_result(result: fs_module::RootReadResult) -> LoweredValue {
    LoweredValue::Record(Arc::new(BTreeMap::from([
        (Arc::from("state"), LoweredValue::Str(result.state.into())),
        (
            Arc::from("data"),
            result
                .data
                .map_or(LoweredValue::Null, |data| LoweredValue::Bytes(data.into())),
        ),
        (
            Arc::from("errno"),
            result.errno.map_or(LoweredValue::Null, LoweredValue::Int),
        ),
        (
            Arc::from("error_kind"),
            result
                .error_kind
                .map_or(LoweredValue::Null, |kind| LoweredValue::Str(kind.into())),
        ),
        (Arc::from("truncated"), LoweredValue::Bool(result.truncated)),
    ])))
}

fn lowered_fs_root_readlink_result(
    result: fs_module::RootReadlinkResult,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    let target = match result.target {
        Some(path) => LoweredValue::Path(
            path_value_from_pathbuf(path).map_err(|error| error.with_span(span))?,
        ),
        None => LoweredValue::Null,
    };
    Ok(LoweredValue::Record(Arc::new(BTreeMap::from([
        (Arc::from("state"), LoweredValue::Str(result.state.into())),
        (Arc::from("target"), target),
        (
            Arc::from("errno"),
            result.errno.map_or(LoweredValue::Null, LoweredValue::Int),
        ),
        (
            Arc::from("error_kind"),
            result
                .error_kind
                .map_or(LoweredValue::Null, |kind| LoweredValue::Str(kind.into())),
        ),
    ]))))
}

fn lowered_fs_root_filesystem_stats(result: fs_module::RootFilesystemStats) -> LoweredValue {
    LoweredValue::Record(Arc::new(BTreeMap::from([
        (Arc::from("state"), LoweredValue::Str(result.state.into())),
        (
            Arc::from("total_bytes"),
            result
                .total_bytes
                .map_or(LoweredValue::Null, LoweredValue::Int),
        ),
        (
            Arc::from("used_bytes"),
            result
                .used_bytes
                .map_or(LoweredValue::Null, LoweredValue::Int),
        ),
        (
            Arc::from("available_bytes"),
            result
                .available_bytes
                .map_or(LoweredValue::Null, LoweredValue::Int),
        ),
        (
            Arc::from("block_size_bytes"),
            result
                .block_size_bytes
                .map_or(LoweredValue::Null, LoweredValue::Int),
        ),
        (
            Arc::from("errno"),
            result.errno.map_or(LoweredValue::Null, LoweredValue::Int),
        ),
        (
            Arc::from("error_kind"),
            result
                .error_kind
                .map_or(LoweredValue::Null, |kind| LoweredValue::Str(kind.into())),
        ),
    ])))
}

fn lowered_filesystem_stats_record(stats: fs_module::FilesystemStats) -> LoweredValue {
    LoweredValue::Record(Arc::new(BTreeMap::from([
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
    ])))
}

fn lowered_fs_mount_record(mount: fs_module::FsMount) -> Result<LoweredValue, RuntimeError> {
    let mounted_on = path_value_from_pathbuf(mount.mounted_on)?;
    Ok(LoweredValue::Record(Arc::new(BTreeMap::from([
        (
            Arc::from("filesystem"),
            LoweredValue::Str(mount.filesystem.into()),
        ),
        (Arc::from("mounted_on"), LoweredValue::Path(mounted_on)),
        (Arc::from("fstype"), LoweredValue::Str(mount.fstype.into())),
        (Arc::from("device"), LoweredValue::Int(mount.device as i64)),
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
    ]))))
}

fn lowered_status_segment_record(segment: &ProcessSegmentStatus) -> LoweredValue {
    LoweredValue::Record(Arc::new(BTreeMap::from([
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
    ])))
}

fn lowered_fs_root_dir<'a>(
    roots: &'a [Option<super::OwnedFsCapability>],
    owner: &Arc<()>,
    root: &LoweredValue,
    span: Span,
) -> Result<&'a Root, RuntimeError> {
    let id = lowered_root_id(root, owner, span)?;
    let Some(slot) = id
        .checked_sub(1)
        .and_then(|index| usize::try_from(index).ok())
        .and_then(|index| roots.get(index))
    else {
        return Err(RuntimeError::new("fs-root", "root handle is not active").with_span(span));
    };
    slot.as_ref()
        .and_then(|owned| match &owned.value {
            super::FsCapability::Root(root) => Some(root.root()),
            super::FsCapability::Lock(_) => None,
        })
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
    let temp =
        TempDir::new().map_err(|error| RuntimeError::host(operation, &error).with_span(span))?;
    fs_root_from_temp_dir(temp, operation, span)
}

/// Create scratch storage that stays owner-only even under a permissive umask.
pub(super) fn new_temp_fs_root_in(
    directory: PathValue,
    operation: &'static str,
    span: Span,
) -> Result<FsRootHandle, RuntimeError> {
    let directory = PathBuf::from(OsString::from_vec(directory.bytes));
    let temp = tempfile::Builder::new()
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir_in(directory)
        .map_err(|error| RuntimeError::host(operation, &error).with_span(span))?;
    fs_root_from_temp_dir(temp, operation, span)
}

fn fs_root_from_temp_dir(
    temp: TempDir,
    operation: &'static str,
    span: Span,
) -> Result<FsRootHandle, RuntimeError> {
    let root = Root::open(temp.path())
        .map_err(|error| RuntimeError::host(operation, &error).with_span(span))?;
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
        LoweredValue::SharedList(items) => match Arc::try_unwrap(items) {
            Ok(items) => Ok(items),
            Err(items) => Ok(items.as_ref().clone()),
        },
        other => Err(RuntimeError::new(
            "type-error",
            format!("`@` expected List, found {}", other.type_name()),
        )
        .with_span(span)),
    }
}

// Direct scalar iteration retains its evaluated source, including view bounds.
// Advancing produces one scalar view or byte value without an intermediate List.
enum LoweredScalarCursor {
    Str {
        text: Arc<str>,
        position: usize,
        end: usize,
    },
    Bytes {
        bytes: Arc<[u8]>,
        position: usize,
        end: usize,
    },
}

impl LoweredScalarCursor {
    fn try_new(value: LoweredValue) -> Result<Self, LoweredValue> {
        Ok(match value {
            LoweredValue::Str(text) => {
                let end = text.len();
                Self::Str {
                    text,
                    position: 0,
                    end,
                }
            }
            LoweredValue::StrView(view) => Self::Str {
                position: view.start(),
                end: view.end(),
                text: view.text,
            },
            LoweredValue::Bytes(bytes) => {
                let end = bytes.len();
                Self::Bytes {
                    bytes,
                    position: 0,
                    end,
                }
            }
            LoweredValue::BytesView(view) => Self::Bytes {
                position: view.start(),
                end: view.end(),
                bytes: view.bytes,
            },
            other => return Err(other),
        })
    }

    fn next(&mut self) -> Option<LoweredValue> {
        match self {
            Self::Str {
                text,
                position,
                end,
            } => {
                let scalar = text[*position..*end].chars().next()?;
                let start = *position;
                *position += scalar.len_utf8();
                Some(lowered_str_view_value(text.clone(), start, *position))
            }
            Self::Bytes {
                bytes,
                position,
                end,
            } => {
                if *position == *end {
                    return None;
                }
                let value = bytes[*position];
                *position += 1;
                Some(LoweredValue::Int(i64::from(value)))
            }
        }
    }
}

// Retaining the source storage keeps keys and values stable across body updates.
// A range cursor holds only the last key and constructs one structural entry.
struct LoweredMapCursor {
    entries: Arc<BTreeMap<MapKey, LoweredValue>>,
    previous: Option<MapKey>,
    key_field: Name,
    value_field: Name,
}

impl LoweredMapCursor {
    fn new(entries: Arc<BTreeMap<MapKey, LoweredValue>>) -> Self {
        Self {
            entries,
            previous: None,
            key_field: Name::intern("key"),
            value_field: Name::intern("value"),
        }
    }

    fn next(&mut self) -> Option<LoweredValue> {
        use std::ops::Bound::{Excluded, Unbounded};
        let selected = match self.previous.as_ref() {
            Some(previous) => self
                .entries
                .range::<MapKey, _>((Excluded(previous), Unbounded))
                .next(),
            None => self.entries.iter().next(),
        };
        let (key, value) = selected?;
        let entry = LoweredValue::RecordVec(Arc::new(vec![
            (self.key_field, lowered_map_key_value(key)),
            (self.value_field, value.clone()),
        ]));
        self.previous = Some(key.clone());
        Some(entry)
    }

    fn visit_retained_values(
        &self,
        visitor: &mut dyn for<'a> FnMut(crate::runtime::eval::RetainedValueRef<'a>),
    ) {
        crate::runtime::eval::visit_retained_lowered_values(self.entries.values(), visitor);
    }
}

enum CompTargetNode<'a> {
    Lowered(&'a LoweredCompTarget),
    Indexed(CompTargetView<'a>),
}

enum CompTargetKind<'a> {
    Discard,
    Slot(usize),
    Record(CompTargetFields<'a>),
}

enum CompTargetFields<'a> {
    Lowered(std::slice::Iter<'a, (Name, Box<LoweredCompTarget>, Span)>),
    Indexed(InlineList<'a, (Name, Box<LoweredCompTarget>, Span)>),
}

impl<'a> CompTargetNode<'a> {
    fn kind(self) -> CompTargetKind<'a> {
        match self {
            Self::Lowered(LoweredCompTarget::Discard) | Self::Indexed(CompTargetView::Discard) => {
                CompTargetKind::Discard
            }
            Self::Lowered(LoweredCompTarget::Slot(slot)) => CompTargetKind::Slot(*slot),
            Self::Indexed(CompTargetView::Slot(slot)) => CompTargetKind::Slot(slot),
            Self::Lowered(LoweredCompTarget::Record { fields }) => {
                CompTargetKind::Record(CompTargetFields::Lowered(fields.iter()))
            }
            Self::Indexed(CompTargetView::Record { fields }) => {
                CompTargetKind::Record(CompTargetFields::Indexed(fields))
            }
        }
    }
}

impl<'a> CompTargetFields<'a> {
    fn next(
        &mut self,
        span: Span,
        indexed_error: Option<fn(IrVerifyError, Span) -> RuntimeError>,
    ) -> Result<Option<(Name, CompTargetNode<'a>, Span)>, RuntimeError> {
        match self {
            Self::Lowered(fields) => Ok(fields.next().map(|(name, target, field_span)| {
                (
                    *name,
                    CompTargetNode::Lowered(target.as_ref()),
                    *field_span,
                )
            })),
            Self::Indexed(fields) => fields
                .next()
                .transpose()
                .map(|field| {
                    field.map(|(name, target, field_span)| {
                        (name, CompTargetNode::Indexed(target), field_span)
                    })
                })
                .map_err(|error| {
                    indexed_error.expect("indexed target fields retain their error mapper")(
                        error, span,
                    )
                }),
        }
    }
}

fn bind_comp_target(
    target: CompTargetNode<'_>,
    value: LoweredValue,
    slots: &mut [LoweredValue],
    span: Span,
    indexed_error: Option<fn(IrVerifyError, Span) -> RuntimeError>,
) -> Result<(), RuntimeError> {
    fn collect(
        target: CompTargetKind<'_>,
        value: LoweredValue,
        span: Span,
        pending: &mut Vec<(usize, LoweredValue)>,
        indexed_error: Option<fn(IrVerifyError, Span) -> RuntimeError>,
    ) -> Result<(), RuntimeError> {
        match target {
            CompTargetKind::Discard => Ok(()),
            CompTargetKind::Slot(slot) => {
                pending.push((slot, value));
                Ok(())
            }
            CompTargetKind::Record(mut fields) => {
                if !matches!(
                    value,
                    LoweredValue::Record(_)
                        | LoweredValue::RecordVec(_)
                        | LoweredValue::Stats { .. }
                        | LoweredValue::StatsBlob(_)
                ) {
                    return Err(RuntimeError::new(
                        "type-error",
                        "record destructuring requires a record value",
                    )
                    .with_span(span));
                }
                while let Some((name, target, field_span)) = fields.next(span, indexed_error)? {
                    let selected = lowered_record_field_value(&value, &name.as_str()).ok_or_else(|| {
                        RuntimeError::new(
                            "missing-field",
                            format!("missing destructured field `{name}`"),
                        )
                        .with_span(field_span)
                    })?;
                    collect(target.kind(), selected, field_span, pending, indexed_error)?;
                }
                Ok(())
            }
        }
    }

    let kind = target.kind();
    match &kind {
        CompTargetKind::Discard => return Ok(()),
        CompTargetKind::Slot(slot) => {
            slots[*slot] = value;
            return Ok(());
        }
        CompTargetKind::Record(_) => {}
    }
    let mut pending = Vec::new();
    collect(kind, value, span, &mut pending, indexed_error)?;
    for (slot, value) in pending {
        slots[slot] = value;
    }
    Ok(())
}

fn bind_lowered_comp_target(
    target: &LoweredCompTarget,
    value: LoweredValue,
    slots: &mut [LoweredValue],
    span: Span,
) -> Result<(), RuntimeError> {
    bind_comp_target(
        CompTargetNode::Lowered(target),
        value,
        slots,
        span,
        None,
    )
}

pub(in crate::runtime::eval) fn bind_indexed_comp_target(
    target: CompTargetView<'_>,
    value: LoweredValue,
    slots: &mut [LoweredValue],
    span: Span,
    indexed_error: fn(IrVerifyError, Span) -> RuntimeError,
) -> Result<(), RuntimeError> {
    bind_comp_target(
        CompTargetNode::Indexed(target),
        value,
        slots,
        span,
        Some(indexed_error),
    )
}

fn checked_unsigned_value(
    value: &LoweredValue,
    check: &super::LoweredTypeCheck,
    span: Span,
) -> Result<(), RuntimeError> {
    if !value_matches_static_type(ValueView::Lowered(value), &check.ty) {
        return Err(RuntimeError::new(
            "type-error",
            format!("value violates UInt constraint in {}", check.name),
        )
        .with_span(span));
    }
    Ok(())
}

fn checked_lowered_return_value(
    header: &FunctionHeader,
    value: LoweredValue,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    let value = lowered_return_value(header.return_kind, value, span)?;
    if let Some(check) = &header.return_check
        && !value_matches_static_type(ValueView::Lowered(&value), &check.ty)
    {
        return Err(RuntimeError::new(
            "type-error",
            format!("return violates UInt constraint in {}", check.name),
        )
        .with_span(span));
    }
    Ok(value)
}

fn lowered_param_check(lowered: &FunctionHeader, index: usize) -> Option<&super::LoweredTypeCheck> {
    lowered.param_checks.get(index).and_then(Option::as_ref)
}

fn validate_unsigned_runtime_args(
    header: &FunctionHeader,
    args: &[Value],
    omitted: &[usize],
    span: Span,
) -> Result<(), RuntimeError> {
    for (index, check) in header.param_checks.iter().enumerate() {
        let Some(check) = check
            .as_ref()
            .filter(|check| check.ty.has_unsigned_constraint())
        else {
            continue;
        };
        let valid = if header.param_rest[index] {
            let Type::List(item) = &check.ty else {
                continue;
            };
            args.get(index..)
                .unwrap_or(&[])
                .iter()
                .all(|value| value_matches_static_type(ValueView::Runtime(value), item))
        } else if let Some(value) = args.get(index).filter(|_| !omitted.contains(&index)) {
            value_matches_static_type(ValueView::Runtime(value), &check.ty)
        } else {
            header
                .param_defaults
                .get(index)
                .and_then(Option::as_ref)
                .is_none_or(|value| value_matches_static_type(ValueView::Lowered(value), &check.ty))
        };
        if !valid {
            return Err(RuntimeError::new(
                "type-error",
                format!("call violates UInt constraint in {}", check.name),
            )
            .with_span(span));
        }
    }
    Ok(())
}

fn lowered_runtime_arg_matches_param(
    lowered: &FunctionHeader,
    index: usize,
    value: &Value,
) -> bool {
    lowered_param_check(lowered, index)
        .is_none_or(|check| value_matches_static_type(ValueView::Runtime(value), &check.ty))
}

fn validate_parameter_default(
    value: &LoweredValue,
    kind: LoweredType,
    check: Option<&LoweredTypeCheck>,
    span: Span,
) -> Result<(), RuntimeError> {
    if lowered_value_matches(kind, value)
        && check.is_none_or(|check| value_matches_static_type(ValueView::Lowered(value), &check.ty))
    {
        Ok(())
    } else {
        Err(RuntimeError::new(
            "type-error",
            format!(
                "parameter default expected {}, found {}",
                check.map_or_else(|| lowered_type_name(kind), |check| check.name.as_ref()),
                value.type_name()
            ),
        )
        .with_span(span))
    }
}

fn lowered_value_matches_param(
    lowered: &FunctionHeader,
    index: usize,
    kind: LoweredType,
    value: &LoweredValue,
) -> bool {
    if matches!(value, LoweredValue::OmittedArgument) {
        return lowered
            .param_defaults
            .get(index)
            .is_some_and(|default| matches!(default, Some(LoweredValue::OmittedArgument)));
    }
    lowered_value_matches(kind, value)
        && lowered_param_check(lowered, index)
            .is_none_or(|check| value_matches_static_type(ValueView::Lowered(value), &check.ty))
}

fn lowered_param_type_name(lowered: &FunctionHeader, index: usize, kind: LoweredType) -> &str {
    lowered_param_check(lowered, index).map_or_else(|| lowered_type_name(kind), |check| &check.name)
}

/// Native parameter slots retain omission independently of a supplied null.
/// Optional accessors see an omitted slot as absent; required accesses are
/// reached only after static signature binding has supplied those slots.
struct NativeArgumentValues(Vec<Option<LoweredValue>>);

impl NativeArgumentValues {
    fn new(values: Vec<Option<LoweredValue>>) -> Self {
        Self(values)
    }
    fn len(&self) -> usize {
        self.0.len()
    }
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    fn get(&self, index: usize) -> Option<&LoweredValue> {
        self.0.get(index).and_then(Option::as_ref)
    }
    fn first(&self) -> Option<&LoweredValue> {
        self.get(0)
    }
    fn pop(&mut self) -> Option<LoweredValue> {
        self.0.pop().flatten()
    }
    fn remove(&mut self, index: usize) -> LoweredValue {
        self.0
            .remove(index)
            .expect("required native argument was not supplied")
    }
}

impl std::ops::Index<usize> for NativeArgumentValues {
    type Output = LoweredValue;
    fn index(&self, index: usize) -> &Self::Output {
        self.0[index]
            .as_ref()
            .expect("required native argument was not supplied")
    }
}

impl NativeArgumentValues {
    /// The arguments as runtime values for the typed host primitives; a path
    /// is resolved against the script's working directory first, and an
    /// omitted parameter stays `None`.
    fn into_host_values(
        self,
        evaluator: &Evaluator,
        span: Span,
    ) -> Result<Vec<Option<Value>>, RuntimeError> {
        self.0
            .into_iter()
            .map(|value| match value {
                Some(LoweredValue::Path(path)) => {
                    path_value_from_pathbuf(evaluator.host_path(&path))
                        .map(|path| Some(Value::Path(path)))
                        .map_err(|error| error.with_span(span))
                }
                other => Ok(other.map(LoweredValue::into_value)),
            })
            .collect()
    }
}

impl Evaluator {
    // Reading the descriptor directly keeps every stdin operation on one cursor.
    // Polling gives the evaluator a checkpoint even when a signal is delivered
    // to another thread and therefore cannot interrupt this thread's read.
    fn stdin_shutdown_abort(&self, span: Span) -> RuntimeError {
        match self.signal_state.shutdown_status {
            Some(status) => RuntimeError::abort(status, self.signal_state.shutdown_force)
                .with_span(span),
            None => RuntimeError::new("canceled", "stdin read was interrupted during shutdown")
                .with_span(span),
        }
    }

    fn service_stdin_signal(&mut self, span: Span) -> Result<(), RuntimeError> {
        self.service_pending_signal(span)?;
        if self.signal_state.shutdown_complete {
            return Err(self.stdin_shutdown_abort(span));
        }
        Ok(())
    }

    fn read_stdin_chunk(
        &mut self,
        data: &mut [u8],
        operation: &str,
        span: Span,
    ) -> Result<usize, RuntimeError> {
        crate::startup_stdio::check(0)
            .map_err(|error| RuntimeError::host(operation, &error).with_span(span))?;
        let stdin = rustix::stdio::stdin();
        let timeout = revent::Timespec::try_from(WAIT_POLL).expect("WAIT_POLL fits Timespec");
        loop {
            let mut pollfds = [revent::PollFd::from_borrowed_fd(
                stdin.as_fd(),
                revent::PollFlags::IN,
            )];
            match revent::poll(&mut pollfds, Some(&timeout)) {
                Ok(0) | Err(rustix::io::Errno::INTR) => {
                    self.service_stdin_signal(span)?;
                    continue;
                }
                Ok(_) => {}
                Err(error) => {
                    return Err(RuntimeError::host(operation, &std::io::Error::from(error))
                        .with_span(span));
                }
            }
            match rustix::io::read(rustix::stdio::stdin(), &mut *data) {
                Ok(count) => return Ok(count),
                Err(rustix::io::Errno::INTR) => {
                    self.service_stdin_signal(span)?;
                }
                Err(error) => {
                    return Err(RuntimeError::host(operation, &std::io::Error::from(error))
                        .with_span(span));
                }
            }
        }
    }

    fn read_stdin_all(
        &mut self,
        operation: &str,
        span: Span,
    ) -> Result<Vec<u8>, RuntimeError> {
        let mut data = Vec::new();
        let mut chunk = [0; 8192];
        loop {
            let count = self.read_stdin_chunk(&mut chunk, operation, span)?;
            if count == 0 {
                return Ok(data);
            }
            data.extend_from_slice(&chunk[..count]);
        }
    }

    fn read_stdin_line_bytes(
        &mut self,
        operation: &str,
        span: Span,
    ) -> Result<Vec<u8>, RuntimeError> {
        let mut data = Vec::new();
        let mut byte = [0];
        loop {
            if self.read_stdin_chunk(&mut byte, operation, span)? == 0 {
                return Ok(data);
            }
            if byte[0] == b'\n' {
                if data.last() == Some(&b'\r') {
                    data.pop();
                }
                return Ok(data);
            }
            data.push(byte[0]);
        }
    }

    /// Dispatches the typed process, terminal, session, and socket primitives
    /// of `modules::process::prims`, `modules::unix::tty`,
    /// `modules::unix::sessions`, and `modules::linux::net_prims`.
    fn eval_host_primitive(
        &mut self,
        op: RuntimeOp,
        values: NativeArgumentValues,
        span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        if matches!(op, RuntimeOp::UnixRedirectFd | RuntimeOp::UnixDupFd) {
            let index = if op == RuntimeOp::UnixDupFd { 1 } else { 0 };
            let fd = lowered_int_arg(values.get(index).cloned(), "unix descriptor target", span)?;
            let flushed = match fd {
                1 => self.flush_stdout_checked(span),
                2 => self.flush_stderr_checked(span),
                _ => Ok(()),
            };
            if let Err(error) = flushed {
                return Ok(lowered_result_err_value(error));
            }
        }
        let values = values.into_host_values(self, span)?;
        if linux_module::storage::handles(op) {
            return lowered_module_result_value(self.linux_storage_call(op, &values, span), span);
        }
        if linux_module::net_prim_fakeable(op)
            && let Some(result) = self.linux_netlink_fake_call(op, &values, span)
        {
            return lowered_module_result_value(result, span);
        }
        let args = process_module::Args::new(op, &values, span);
        let result = if linux_module::is_net_prim(op) {
            linux_module::net_prim_call(op, &values, span)
        } else if process_module::is_prim(op) {
            process_module::prim_call(op, &args)
        } else if linux_module::is_prim(op) {
            linux_module::prim_call(op, &args)
        } else {
            unix_module::prim_call(op, &args)
        };
        lowered_module_result_value(result, span)
    }

    /// Writes the buffered stdout to the host descriptor and reports the first
    /// failure with its errno. Nothing is written when output is captured or
    /// owned by an embedding host.
    fn flush_stdout_checked(&mut self, span: Span) -> Result<(), RuntimeError> {
        if !self.shared_stdio || self.capture_process_output || self.stdout.is_empty() {
            return Ok(());
        }
        let bytes = std::mem::take(&mut self.stdout);
        crate::startup_stdio::check(1)
            .map_err(|error| RuntimeError::host("io-flush-stdout", &error).with_span(span))?;
        let mut offset = 0;
        while offset < bytes.len() {
            // A raw write, not `std::io::stdout`, which reports a closed
            // descriptor as success.
            match rustix::io::write(rustix::stdio::stdout(), &bytes[offset..]) {
                Ok(0) => {
                    return Err(RuntimeError::host(
                        "io-flush-stdout",
                        &std::io::Error::from(std::io::ErrorKind::WriteZero),
                    )
                    .with_span(span));
                }
                Ok(written) => offset += written,
                Err(rustix::io::Errno::INTR) => {}
                Err(rustix::io::Errno::AGAIN) => std::thread::sleep(Duration::from_millis(1)),
                Err(error) => {
                    return Err(RuntimeError::host("io-flush-stdout", &error).with_span(span));
                }
            }
        }
        Ok(())
    }

    /// Captured stderr belongs to its output sink; only shared host output is
    /// drained here, so a prompt can become visible before reading stdin.
    fn flush_stderr_checked(&mut self, span: Span) -> Result<(), RuntimeError> {
        if !self.shared_stdio || self.capture_process_output || self.stderr.is_empty() {
            return Ok(());
        }
        let bytes = std::mem::take(&mut self.stderr);
        crate::startup_stdio::check(2)
            .map_err(|error| RuntimeError::host("io-flush-stderr", &error).with_span(span))?;
        let mut offset = 0;
        while offset < bytes.len() {
            match rustix::io::write(rustix::stdio::stderr(), &bytes[offset..]) {
                Ok(0) => {
                    return Err(RuntimeError::host(
                        "io-flush-stderr",
                        &std::io::Error::from(std::io::ErrorKind::WriteZero),
                    ).with_span(span));
                }
                Ok(written) => offset += written,
                Err(rustix::io::Errno::INTR) => {}
                Err(rustix::io::Errno::AGAIN) => std::thread::sleep(Duration::from_millis(1)),
                Err(error) => {
                    return Err(RuntimeError::host("io-flush-stderr", &error).with_span(span));
                }
            }
        }
        Ok(())
    }

    fn lowered_stream_list_result(
        &mut self,
        result: Result<StreamValue, RuntimeError>,
        span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        let mut stream = match result {
            Ok(stream) => stream,
            Err(error) => return Ok(lowered_result_err_value(error)),
        };
        // fs.dirs materializes this intermediate stream immediately, so stamp
        // its evaluator owner before the generic consumer validates and drains it.
        self.register_returned_stream(&mut stream, span)?;
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
        if stream.source.is_some() || stream.script().is_some() {
            while let Some(value) = self.stream_next(&mut stream, span)? {
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
            // A set is iterated as its elements in key order.
            LoweredValue::Set(elements) => Ok(super::set::lowered_set_items(&elements)),
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
        let id = self.push_fs_capability(super::FsCapability::Root(Box::new(root)));
        LoweredValue::FsRoot(super::FsRootValue {
            id,
            owner: self.resource_owner.clone(),
        })
    }

    fn lowered_create_temp_file_root(&mut self, span: Span) -> Result<LoweredValue, RuntimeError> {
        let root = new_temp_fs_root("fs-temp-file", span)?;
        root.root()
            .create("file")
            .and_then(|mut file| file.flush())
            .map_err(|error| RuntimeError::host("fs-temp-file", &error).with_span(span))?;
        let root = self.push_lowered_fs_root(root);
        let path = PathValue::new(b"file".to_vec()).map_err(|error| error.with_span(span))?;
        Ok(LoweredValue::Record(Arc::new(BTreeMap::from([
            (Arc::from("root"), root),
            (Arc::from("path"), LoweredValue::Path(path)),
        ]))))
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
            .map_err(|error| RuntimeError::host("fs-dir", &error).with_span(span))?;
        let root = Root::open(path)
            .map_err(|error| RuntimeError::host("fs-dir", &error).with_span(span))?;
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
            .map_err(|error| RuntimeError::host("fs-dir", &error).with_span(span))?;
        Ok(self.push_lowered_fs_root(FsRootHandle::Dir(root)))
    }

    define_lowered_module_op_authority! {
        fn eval_lowered_module_call_values(
            &mut self,
            op: RuntimeOp,
            mut values: NativeArgumentValues,
            span: Span,
            cli_plan: Option<&crate::modules::cli::CliDescriptorPlan>,
        ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError>;
        prelude {
            if process_module::is_prim(op)
                || unix_module::is_prim(op)
                || linux_module::is_prim(op)
                || linux_module::is_net_prim(op)
                || linux_module::storage::handles(op)
            {
                return Ok(ControlFlow::Continue(
                    self.eval_host_primitive(op, values, span)?,
                ));
            }
        }
        pre_unix_rows {
            callable; RuntimeOp::CpuCount if values.is_empty() => {
                LoweredValue::Int(crate::modules::cpu::count())
            }
            callable; RuntimeOp::AppletHashPassword if values.len() == 2 => {
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
            callable; RuntimeOp::AppletVerifyPassword if values.len() == 2 => {
                let hash = lowered_str_arg_owned(values.pop(), "", "applet.verify_password", span)?;
                let password =
                    lowered_str_arg_owned(values.pop(), "", "applet.verify_password", span)?;
                LoweredValue::Bool(auth_module::verify_password(&password, &hash))
            }
            callable; RuntimeOp::AppletCurrentEuid if values.is_empty() => {
                LoweredValue::Int(rustix::process::geteuid().as_raw() as i64)
            }
            callable; RuntimeOp::AppletCurrentExe if values.is_empty() => match std::env::current_exe() {
                Ok(path) => match path_value_from_pathbuf(path) {
                    Ok(path) => lowered_result_ok(LoweredValue::Path(path)),
                    Err(error) => lowered_result_err_value(error.with_span(span)),
                },
                Err(error) => lowered_result_err_value(
                    RuntimeError::host("applet.current_exe", &error).with_span(span),
                ),
            },
            callable; RuntimeOp::AppletLoginSession if values.len() == 3 => {
                let host = lowered_str_arg_owned(values.pop(), "", "applet.login_session", span)?;
                let preserve_env =
                    lowered_bool_arg_or(values.pop(), false, "applet.login_session", span)?;
                let record = lowered_record_arg(values.pop(), "applet.login_session", span)?;
                let home = self.host_path(&record_path(&record, "home", span)?);
                let user = lowered_session_user_record(record, home, span)?;
                match auth_module::login_session(&user, preserve_env, &host) {
                    Ok(code) => lowered_result_ok(LoweredValue::Int(i64::from(code))),
                    Err(error) => lowered_result_err_value(
                        RuntimeError::host("applet.login_session", &error).with_span(span),
                    ),
                }
            }
            callable; RuntimeOp::AppletSuSession if values.len() == 6 => return self.eval_lowered_applet_su_session_values(values, span),
            callable; RuntimeOp::AppletSuloginSession if values.len() == 1 => {
                let record = lowered_record_arg(values.pop(), "applet.sulogin_session", span)?;
                let home = self.host_path(&record_path(&record, "home", span)?);
                let user = lowered_session_user_record(record, home, span)?;
                match auth_module::sulogin_session(&user) {
                    Ok(code) => lowered_result_ok(LoweredValue::Int(i64::from(code))),
                    Err(error) => lowered_result_err_value(
                        RuntimeError::host("applet.sulogin_session", &error).with_span(span),
                    ),
                }
            }
            callable; RuntimeOp::AppletMdev if values.len() == 1 => {
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
            callable; RuntimeOp::ArchiveCompress if (2..=5).contains(&values.len()) => return self.eval_lowered_archive_compress_values(values, span),
            callable; RuntimeOp::ArchiveDecompress if (2..=4).contains(&values.len()) => {
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
            callable; RuntimeOp::ArchiveCpioCreate if values.len() == 3 || values.len() == 4 => {
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
            callable; RuntimeOp::ArchiveCpioExtract if values.len() == 2 || values.len() == 3 => {
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
            callable; RuntimeOp::ArchiveCpioList if values.len() == 1 => {
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
            callable; RuntimeOp::ArchiveDecompressBytes if values.len() == 1 || values.len() == 2 => {
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
            callable; RuntimeOp::ArchiveTarCreate if (3..=5).contains(&values.len()) => return self.eval_lowered_archive_tar_create_values(values, span),
            callable; RuntimeOp::ArchiveTarExtract if (2..=6).contains(&values.len()) => return self.eval_lowered_archive_tar_extract_values(values, span),
            callable; RuntimeOp::ArchiveTarList if (1..=3).contains(&values.len()) => {
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
            callable; RuntimeOp::ArchiveZipExtract if values.len() == 2 || values.len() == 3 => {
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
            callable; RuntimeOp::ArchiveZipList if values.len() == 1 => {
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
            callable; RuntimeOp::CliParse if values.len() == 2 || values.len() == 3 => {
                let command = lowered_str_arg_owned(
                    values.get(2).cloned(),
                    &self.command_name,
                    "cli.parse",
                    span,
                )?;
                let schema = lowered_record_runtime_arg(values.remove(1), "cli.parse", span)?;
                let argv = lowered_str_list_runtime_arg(values.remove(0), "cli.parse", span)?;
                lowered_module_result_value(
                    cli_module::parse_cli(argv, schema, &command, span, cli_plan),
                    span,
                )?
            }
            callable; RuntimeOp::CliApplet if values.len() == 2 || values.len() == 3 => {
                let command = lowered_str_arg_owned(
                    values.get(2).cloned(),
                    &self.command_name,
                    "cli.applet",
                    span,
                )?;
                let schema = lowered_record_runtime_arg(values.remove(1), "cli.applet", span)?;
                let argv = lowered_str_list_runtime_arg(values.remove(0), "cli.applet", span)?;
                lowered_module_result_value(
                    cli_module::parse_cli_applet(argv, schema, &command, span, cli_plan),
                    span,
                )?
            }
            callable; RuntimeOp::CliParseFull if (2..=4).contains(&values.len()) => {
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
                    cli_module::parse_cli_full(argv, schema, env, &command, span, cli_plan),
                    span,
                )?
            }
            callable; RuntimeOp::CliCommands
                if values.len() == 2 || values.len() == 3 || values.len() == 4 =>
            return self.eval_lowered_cli_commands_values(values, span, cli_plan),
            callable; RuntimeOp::CliTokens if values.len() == 1 || values.len() == 2 => {
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
            callable; RuntimeOp::CliUsage if values.len() == 1 || values.len() == 2 => {
                let command =
                    lowered_str_arg_owned(values.get(1).cloned(), "command", "cli.usage", span)?;
                let schema = lowered_record_runtime_arg(values.remove(0), "cli.usage", span)?;
                lowered_runtime_value(cli_module::render_usage(schema, command, span)?, span)?
            }
            callable; RuntimeOp::DiffUnified if values.len() == 2 || values.len() == 3 => {
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
            callable; RuntimeOp::ElfInspect if values.len() == 1 => {
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
            callable; RuntimeOp::DnsLookup if (1..=4).contains(&values.len()) => return self.eval_lowered_dns_lookup_values(values, span),
            callable; RuntimeOp::DnsResolveHost if values.len() == 1 || values.len() == 2 => return self.eval_lowered_dns_resolve_host_values(values, span),
            callable; RuntimeOp::DnsReverse if values.len() == 1 => {
                let addr = lowered_str_arg_owned(values.pop(), "", "dns.reverse", span)?;
                let args = RecordMap::from([(Arc::from("addr"), Value::Str(addr.as_str().into()))]);
                if let Some(value) = intercept_test_host_call(self, "dns.reverse", args, span) {
                    lowered_runtime_value(value, span)?
                } else {
                    lowered_runtime_list_result(dns_module::reverse(&addr, span), span)?
                }
            }
            callable; RuntimeOp::DnsNameservers if values.is_empty() => {
                let args = RecordMap::default();
                if let Some(value) = intercept_test_host_call(self, "dns.nameservers", args, span) {
                    lowered_runtime_value(value, span)?
                } else {
                    lowered_runtime_list_result(dns_module::nameservers(span), span)?
                }
            }
            callable; RuntimeOp::FsCwd if values.is_empty() => {
                match path_value_from_pathbuf(self.cwd.clone()) {
                    Ok(path) => lowered_result_ok(LoweredValue::Path(path)),
                    Err(error) => lowered_result_err_value(error.with_span(span)),
                }
            }
            callable; RuntimeOp::FsGitroot if values.is_empty() => {
                match crate::modules::fs::gitroot(self.cwd.clone(), span).and_then(|path| {
                    path_value_from_pathbuf(path).map_err(|error| error.with_span(span))
                }) {
                    Ok(path) => lowered_result_ok(LoweredValue::Path(path)),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::FsTempSibling if values.len() == 1 => {
                let path = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "fs.temp_sibling",
                    span,
                )?;
                // The name is found against the host path, and given back on
                // the path as the program spelled it.
                let host = self.host_path(&path);
                match crate::modules::fs::temp_sibling_name(&host, span)
                    .map(|name| {
                        PathBuf::from(OsString::from_vec(path.bytes.clone())).with_file_name(name)
                    })
                    .and_then(|sibling| {
                        path_value_from_pathbuf(sibling).map_err(|error| error.with_span(span))
                    }) {
                    Ok(sibling) => lowered_result_ok(LoweredValue::Path(sibling)),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::FsDirs if (1..=4).contains(&values.len()) => {
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
            callable; RuntimeOp::FsChildren if (1..=3).contains(&values.len()) => {
                let operation = "fs.children";
                let ordered = lowered_bool_arg_or(values.get(2).cloned(), true, operation, span)?;
                let stat = lowered_bool_arg_or(values.get(1).cloned(), true, operation, span)?;
                let path = lowered_path_arg(values.remove(0), operation, span)?;
                lowered_runtime_stream_result(
                    fs_module::list_filesystem(self.host_path(&path), stat, ordered, span),
                    span,
                )?
            }
            callable; RuntimeOp::FsMetadata if values.len() == 1 => {
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
            callable; RuntimeOp::FsFilesystemStats if values.len() == 1 => {
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
            callable; RuntimeOp::FsMounts if values.is_empty() => {
                lowered_runtime_stream_result(fs_module::mounts(span), span)?
            }
            callable; RuntimeOp::FsMountFor if values.len() == 1 => {
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
            callable; RuntimeOp::FsReadText if values.len() == 1 => {
                let path = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "fs.read_text",
                    span,
                )?;
                match read_host_path_text(&self.host_path(&path), span) {
                    Ok(text) => lowered_result_ok(LoweredValue::Str(text.into())),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::FsWrite if values.len() == 2 => {
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
            callable; RuntimeOp::FsWrite if values.len() == 3 => {
                let mode = lowered_int_arg(values.pop(), "fs.write", span)?;
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
                lowered_unit_result(fs_module::write_path_with_mode(
                    self.host_path(&path),
                    &data,
                    mode,
                    span,
                ))
            }
            callable; RuntimeOp::FsWriteAtomic if values.len() == 2 => {
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
            callable; RuntimeOp::FsMkdir if values.len() == 1 || values.len() == 2 => {
                let parents = lowered_bool_arg_or(values.get(1).cloned(), true, "fs.mkdir", span)?;
                let path = lowered_path_arg(values.remove(0), "fs.mkdir", span)?;
                lowered_unit_result(fs_module::mkdir_path(
                    self.host_path(&path),
                    parents,
                    None,
                    span,
                ))
            }
            callable; RuntimeOp::FsRemove if values.len() == 1 || values.len() == 2 => {
                let missing_ok = lowered_bool_arg_or(
                    values.get(1).cloned(),
                    xsh_registry::signature::REMOVE_MISSING_OK_DEFAULT,
                    "fs.remove",
                    span,
                )?;
                let path = lowered_path_arg(values.remove(0), "fs.remove", span)?;
                lowered_unit_result(fs_module::remove_path(
                    self.host_path(&path),
                    missing_ok,
                    span,
                ))
            }
            callable; RuntimeOp::FsExists if values.len() == 1 => {
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
            callable; RuntimeOp::FsIsDir | RuntimeOp::FsIsFile | RuntimeOp::FsIsSymlink
                if values.len() == 1 =>
            {
                let (operation, kind) = match op {
                    RuntimeOp::FsIsDir => ("Path.is_dir", fs_module::PathKind::Dir),
                    RuntimeOp::FsIsFile => ("Path.is_file", fs_module::PathKind::File),
                    _ => ("Path.is_symlink", fs_module::PathKind::Symlink),
                };
                let path =
                    lowered_path_arg(values.pop().expect("checked value length"), operation, span)?;
                match fs_module::path_is_kind(self.host_path(&path), kind, span) {
                    Ok(found) => lowered_result_ok(LoweredValue::Bool(found)),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::FsExecutable if values.len() == 1 => return self.eval_lowered_fs_executable_values(values, span),
            callable; RuntimeOp::FsWorldWritable if values.len() == 1 => {
                let mode = lowered_int_arg(values.pop(), "fs.world_writable", span)?;
                LoweredValue::Bool(fs_module::mode_world_writable(mode))
            }
            callable; RuntimeOp::FsSticky if values.len() == 1 => {
                let mode = lowered_int_arg(values.pop(), "fs.sticky", span)?;
                LoweredValue::Bool(fs_module::mode_sticky(mode))
            }
            callable; RuntimeOp::FsSetuid if values.len() == 1 => {
                let mode = lowered_int_arg(values.pop(), "fs.setuid", span)?;
                LoweredValue::Bool(fs_module::mode_setuid(mode))
            }
            callable; RuntimeOp::FsSetgid if values.len() == 1 => {
                let mode = lowered_int_arg(values.pop(), "fs.setgid", span)?;
                LoweredValue::Bool(fs_module::mode_setgid(mode))
            }
            callable; RuntimeOp::FsOwnerExecutable if values.len() == 1 => {
                let mode = lowered_int_arg(values.pop(), "fs.owner_executable", span)?;
                LoweredValue::Bool(fs_module::mode_owner_executable(mode))
            }
            callable; RuntimeOp::FsGroupExecutable if values.len() == 1 => {
                let mode = lowered_int_arg(values.pop(), "fs.group_executable", span)?;
                LoweredValue::Bool(fs_module::mode_group_executable(mode))
            }
            callable; RuntimeOp::FsOtherExecutable if values.len() == 1 => {
                let mode = lowered_int_arg(values.pop(), "fs.other_executable", span)?;
                LoweredValue::Bool(fs_module::mode_other_executable(mode))
            }
            callable; RuntimeOp::FsOpenRoot if values.len() == 1 => {
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
            // The scope's release differs from the method only on a root that
            // is already closed, which it accepts: the body may have closed
            // it. A value that never was this evaluator's root still fails.
            callable; RuntimeOp::FsCloseRoot | RuntimeOp::FsCloseRootIfOpen if values.len() == 1 => return self.eval_lowered_fs_close_root_values(op, values, span),
            callable; RuntimeOp::FsRootPath if values.len() == 1 => {
                let root = values.pop().expect("checked value length");
                match lowered_fs_root_dir(&self.fs_capabilities, &self.resource_owner, &root, span)
                    .and_then(|dir| root_path_from_dir(dir, span))
                {
                    Ok(path) => lowered_result_ok(LoweredValue::Path(path)),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::FsRootOpenRoot if values.len() == 2 => {
                let path =
                    lowered_path_arg(values.pop().expect("checked value length"), "fs.root", span)?;
                let root = values.pop().expect("checked value length");
                let rel = pathbuf_from_path_value(&path);
                match lowered_fs_root_dir(&self.fs_capabilities, &self.resource_owner, &root, span)
                    .and_then(|dir| fs_module::rooted_open_root(dir, &rel, span))
                {
                    Ok(dir) => lowered_result_ok(self.push_lowered_fs_root(FsRootHandle::Dir(dir))),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::FsRootChildren if (2..=4).contains(&values.len()) => {
                let max_entries =
                    lowered_int_arg_or(values.get(2).cloned(), 65_536, "fs.root_children", span)?;
                let ordered =
                    lowered_bool_arg_or(values.get(3).cloned(), true, "fs.root_children", span)?;
                let path = lowered_path_arg(
                    values.get(1).cloned().expect("checked value length"),
                    "fs.root_children",
                    span,
                )?;
                let root = values.first().cloned().expect("checked value length");
                let rel = pathbuf_from_path_value(&path);
                match lowered_fs_root_dir(&self.fs_capabilities, &self.resource_owner, &root, span)
                    .and_then(|dir| fs_module::rooted_children(dir, &rel, max_entries, ordered, span))
                {
                    Ok(result) => lowered_result_ok(lowered_fs_root_children_result(result, span)?),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::FsRootRead if values.len() == 2 => {
                let path = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "fs.root_read",
                    span,
                )?;
                let root = values.pop().expect("checked value length");
                let rel = pathbuf_from_path_value(&path);
                match lowered_fs_root_dir(&self.fs_capabilities, &self.resource_owner, &root, span)
                    .and_then(|dir| fs_module::rooted_read(dir, &rel, span))
                {
                    Ok(bytes) => lowered_result_ok(LoweredValue::Bytes(bytes.into())),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::FsRootReadResult if values.len() == 2 || values.len() == 3 => return self.eval_lowered_fs_root_read_result_values(values, span),
            callable; RuntimeOp::FsRootFilesystemStats if values.len() == 2 => {
                let path = lowered_path_arg(
                    values.get(1).cloned().expect("checked value length"),
                    "fs.root_filesystem_stats",
                    span,
                )?;
                let root = values.first().cloned().expect("checked value length");
                let rel = pathbuf_from_path_value(&path);
                match lowered_fs_root_dir(&self.fs_capabilities, &self.resource_owner, &root, span)
                    .and_then(|dir| fs_module::rooted_filesystem_stats(dir, &rel, span))
                {
                    Ok(result) => lowered_result_ok(lowered_fs_root_filesystem_stats(result)),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::FsRootReadText if values.len() == 2 => return self.eval_lowered_fs_root_read_text_values(values, span),
            callable; RuntimeOp::FsRootWrite | RuntimeOp::FsRootWriteAtomic if values.len() == 3 => return self.eval_lowered_fs_root_write_values(op, values, span),
            callable; RuntimeOp::FsRootMetadata if values.len() == 2 => return self.eval_lowered_fs_root_metadata_values(values, span),
            callable; RuntimeOp::FsRootStat if (2..=3).contains(&values.len()) => return self.eval_lowered_fs_root_stat_values(values, span),
            callable; RuntimeOp::FsRootExists if values.len() == 2 => {
                let path = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "fs.root_exists",
                    span,
                )?;
                let root = values.pop().expect("checked value length");
                let rel = pathbuf_from_path_value(&path);
                match lowered_fs_root_dir(&self.fs_capabilities, &self.resource_owner, &root, span)
                    .and_then(|dir| fs_module::rooted_exists(dir, &rel, span))
                {
                    Ok(exists) => lowered_result_ok(LoweredValue::Bool(exists)),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::FsRootMkdir if (2..=4).contains(&values.len()) => {
                let mode =
                    lowered_int_arg_or(values.get(2).cloned(), 0o777, "fs.root_mkdir", span)?;
                let parents =
                    lowered_bool_arg_or(values.get(3).cloned(), false, "fs.root_mkdir", span)?;
                let path = lowered_path_arg(
                    values.get(1).cloned().expect("checked value length"),
                    "fs.root_mkdir",
                    span,
                )?;
                let root = values.first().cloned().expect("checked value length");
                let rel = pathbuf_from_path_value(&path);
                lowered_unit_result(
                    lowered_fs_root_dir(&self.fs_capabilities, &self.resource_owner, &root, span)
                        .and_then(|dir| fs_module::rooted_mkdir(dir, &rel, mode, parents, span)),
                )
            }
            callable; RuntimeOp::FsRootRemove if values.len() == 2 || values.len() == 3 => {
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
                    lowered_fs_root_dir(&self.fs_capabilities, &self.resource_owner, &root, span)
                        .and_then(|root| fs_module::rooted_remove(root, &rel, dir, span)),
                )
            }
            callable; RuntimeOp::FsRootReadlink if values.len() == 2 => {
                let path = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "fs.root_readlink",
                    span,
                )?;
                let root = values.pop().expect("checked value length");
                let rel = pathbuf_from_path_value(&path);
                match lowered_fs_root_dir(&self.fs_capabilities, &self.resource_owner, &root, span)
                    .and_then(|dir| fs_module::rooted_readlink(dir, &rel, span))
                    .and_then(|path| {
                        path_value_from_pathbuf(path).map_err(|error| error.with_span(span))
                    }) {
                    Ok(path) => lowered_result_ok(LoweredValue::Path(path)),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::FsRootReadlinkResult if values.len() == 2 => {
                let path = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "fs.root_readlink_result",
                    span,
                )?;
                let root = values.pop().expect("checked value length");
                let rel = pathbuf_from_path_value(&path);
                match lowered_fs_root_dir(&self.fs_capabilities, &self.resource_owner, &root, span)
                    .and_then(|dir| fs_module::rooted_readlink_result(dir, &rel, span))
                {
                    Ok(result) => lowered_result_ok(lowered_fs_root_readlink_result(result, span)?),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::FsRootSymlink if (3..=5).contains(&values.len()) => return self.eval_lowered_fs_root_symlink_values(values, span),
            callable; RuntimeOp::FsRootChmod if values.len() == 3 => {
                let mode = lowered_int_arg(values.pop(), "fs.root_chmod", span)?;
                let path = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "fs.root_chmod",
                    span,
                )?;
                let root = values.pop().expect("checked value length");
                let rel = pathbuf_from_path_value(&path);
                lowered_unit_result(
                    lowered_fs_root_dir(&self.fs_capabilities, &self.resource_owner, &root, span)
                        .and_then(|dir| fs_module::rooted_chmod(dir, &rel, mode, span)),
                )
            }
            callable; RuntimeOp::FsRootInstallFile if (5..=7).contains(&values.len()) => return self.eval_lowered_fs_root_install_file_values(values, span),
            callable; RuntimeOp::FsCopy if values.len() == 2 || values.len() == 3 => {
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
            callable; RuntimeOp::FsCopyTree if (2..=5).contains(&values.len()) => return self.eval_lowered_fs_copy_tree_values(values, span),
            callable; RuntimeOp::FsRename if values.len() == 2 || values.len() == 3 => {
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
            callable; RuntimeOp::FsRemoveManifest if (2..=4).contains(&values.len()) => return self.eval_lowered_fs_remove_manifest_values(values, span),
            callable; RuntimeOp::FsInstall if (3..=5).contains(&values.len()) => return self.eval_lowered_fs_install_values(values, span),
            callable; RuntimeOp::FsInstallAs if (5..=7).contains(&values.len()) => return self.eval_lowered_fs_install_as_values(values, span),
            callable; RuntimeOp::FsTruncate if values.len() == 2 => {
                let size = lowered_int_arg(values.pop(), "fs.truncate", span)?;
                let path = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "fs.truncate",
                    span,
                )?;
                lowered_unit_result(fs_module::truncate_path(self.host_path(&path), size, span))
            }
            callable; RuntimeOp::FsChmod if values.len() == 2 || values.len() == 3 => {
                let follow_symlinks =
                    lowered_bool_arg_or(values.get(2).cloned(), true, "Path.chmod", span)?;
                let mode = lowered_int_arg(values.get(1).cloned(), "Path.chmod", span)?;
                let path = lowered_path_arg(
                    values.first().cloned().expect("checked value length"),
                    "Path.chmod",
                    span,
                )?;
                lowered_unit_result(fs_module::chmod_path(
                    self.host_path(&path),
                    mode,
                    follow_symlinks,
                    span,
                ))
            }
            callable; RuntimeOp::FsHardlink if values.len() == 2 => {
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
            callable; RuntimeOp::FsChown if values.len() == 2 || values.len() == 3 => {
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
            callable; RuntimeOp::FsChgrp if values.len() == 2 || values.len() == 3 => {
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
            callable; RuntimeOp::FsMkfifo if values.len() == 2 => {
                let mode = lowered_int_arg(values.pop(), "fs.mkfifo", span)?;
                let path = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "fs.mkfifo",
                    span,
                )?;
                lowered_unit_result(fs_module::mkfifo_path(self.host_path(&path), mode, span))
            }
            callable; RuntimeOp::FsStat if (1..=2).contains(&values.len()) => {
                let follow = lowered_bool_arg_or(values.get(1).cloned(), false, "fs.stat", span)?;
                let path = lowered_path_arg(values.remove(0), "fs.stat", span)?;
                lowered_runtime_result(fs_module::stat(self.kernel_path(&path), follow, span), span)?
            }
            callable; RuntimeOp::FsSetOwner if (1..=4).contains(&values.len()) => {
                let follow =
                    lowered_bool_arg_or(values.get(3).cloned(), false, "fs.set_owner", span)?;
                let gid = lowered_optional_int_arg(values.get(2), "fs.set_owner", span)?;
                let uid = lowered_optional_int_arg(values.get(1), "fs.set_owner", span)?;
                let path = lowered_path_arg(values.remove(0), "fs.set_owner", span)?;
                lowered_unit_result(fs_module::set_owner(
                    self.host_path(&path),
                    uid,
                    gid,
                    follow,
                    span,
                ))
            }
            callable; RuntimeOp::FsSetTimes if (1..=10).contains(&values.len()) => return self.eval_lowered_fs_set_times_values(values, span),
            callable; RuntimeOp::FsMknod if (3..=5).contains(&values.len()) => {
                let operation = "fs.mknod";
                let minor = lowered_int_arg_or(values.get(4).cloned(), 0, operation, span)?;
                let major = lowered_int_arg_or(values.get(3).cloned(), 0, operation, span)?;
                let mode = lowered_int_arg(values.get(2).cloned(), operation, span)?;
                let kind = lowered_str_arg_owned(values.get(1).cloned(), "", operation, span)?;
                let path = lowered_path_arg(values.remove(0), operation, span)?;
                lowered_unit_result(fs_module::mknod(
                    self.host_path(&path),
                    &kind,
                    mode,
                    major,
                    minor,
                    span,
                ))
            }
            callable; RuntimeOp::FsMakedev if values.len() == 2 => {
                let minor = lowered_int_arg(values.pop(), "fs.makedev", span)?;
                let major = lowered_int_arg(values.pop(), "fs.makedev", span)?;
                LoweredValue::Int(fs_module::makedev(major, minor))
            }
            callable; RuntimeOp::FsDevMajor if values.len() == 1 => {
                let dev = lowered_int_arg(values.pop(), "fs.dev_major", span)?;
                LoweredValue::Int(fs_module::dev_major(dev))
            }
            callable; RuntimeOp::FsDevMinor if values.len() == 1 => {
                let dev = lowered_int_arg(values.pop(), "fs.dev_minor", span)?;
                LoweredValue::Int(fs_module::dev_minor(dev))
            }
            callable; RuntimeOp::FsLink if (2..=3).contains(&values.len()) => {
                let follow = lowered_bool_arg_or(values.get(2).cloned(), false, "fs.link", span)?;
                let dest = lowered_path_arg(values.remove(1), "fs.link", span)?;
                let source = lowered_path_arg(values.remove(0), "fs.link", span)?;
                lowered_unit_result(fs_module::link(
                    self.host_path(&source),
                    self.host_path(&dest),
                    follow,
                    span,
                ))
            }
            callable; RuntimeOp::FsUmask if values.is_empty() => {
                lowered_result_ok(LoweredValue::Int(fs_module::umask()))
            }
            callable; RuntimeOp::FsStatvfs if values.len() == 1 => {
                let path = lowered_path_arg(values.remove(0), "fs.statvfs", span)?;
                lowered_runtime_result(
                    fs_module::statvfs_record(self.host_path(&path), span),
                    span,
                )?
            }
            callable; RuntimeOp::FsRenameNoreplace if values.len() == 2 => {
                let dest = lowered_path_arg(values.remove(1), "fs.rename_noreplace", span)?;
                let source = lowered_path_arg(values.remove(0), "fs.rename_noreplace", span)?;
                lowered_unit_result(fs_module::rename_noreplace(
                    self.host_path(&source),
                    self.host_path(&dest),
                    span,
                ))
            }
            callable; RuntimeOp::FsDataRanges if values.len() == 1 => {
                let path = lowered_path_arg(values.remove(0), "fs.data_ranges", span)?;
                lowered_runtime_result(fs_module::data_ranges(self.host_path(&path), span), span)?
            }
            callable; RuntimeOp::FsCopyFile if (2..=7).contains(&values.len()) => return self.eval_lowered_fs_copy_file_values(values, span),
            callable; RuntimeOp::FsFsync if values.len() == 1 => {
                let path = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "fs.fsync",
                    span,
                )?;
                lowered_unit_result(fs_module::fsync_path(self.host_path(&path), span))
            }
            callable; RuntimeOp::FsAccess if (1..=5).contains(&values.len()) => {
                let operation = "fs.access";
                let read = lowered_bool_arg_or(values.get(1).cloned(), false, operation, span)?;
                let write = lowered_bool_arg_or(values.get(2).cloned(), false, operation, span)?;
                let execute = lowered_bool_arg_or(values.get(3).cloned(), false, operation, span)?;
                let follow = lowered_bool_arg_or(values.get(4).cloned(), true, operation, span)?;
                let path = lowered_path_arg(values.remove(0), operation, span)?;
                lowered_runtime_result(fs_module::access(self.host_path(&path), read, write, execute, follow, span).map(Value::Bool), span)?
            }
            callable; RuntimeOp::FsXattrList if (1..=2).contains(&values.len()) => {
                let follow = lowered_bool_arg_or(values.get(1).cloned(), true, "fs.xattr_list", span)?;
                let path = lowered_path_arg(values.remove(0), "fs.xattr_list", span)?;
                lowered_runtime_result(fs_module::xattr_list(self.host_path(&path), follow, span), span)?
            }
            callable; RuntimeOp::FsXattrGet if (2..=3).contains(&values.len()) => {
                let follow = lowered_bool_arg_or(values.get(2).cloned(), true, "fs.xattr_get", span)?;
                let name = lowered_str_arg_owned(values.get(1).cloned(), "", "fs.xattr_get", span)?;
                let path = lowered_path_arg(values.remove(0), "fs.xattr_get", span)?;
                lowered_runtime_result(fs_module::xattr_get(self.host_path(&path), &name, follow, span), span)?
            }
            callable; RuntimeOp::FsXattrSet if (3..=5).contains(&values.len()) => {
                let operation = "fs.xattr_set";
                let follow = lowered_bool_arg_or(values.get(4).cloned(), true, operation, span)?;
                let mode = lowered_str_arg_owned(values.get(3).cloned(), "upsert", operation, span)?;
                let name = lowered_str_arg_owned(values.get(1).cloned(), "", operation, span)?;
                let data_value = values.remove(2);
                let data = lowered_bytes_arg(&data_value, operation, span)?;
                let path = lowered_path_arg(values.remove(0), operation, span)?;
                lowered_unit_result(fs_module::xattr_set(self.host_path(&path), &name, data, &mode, follow, span))
            }
            callable; RuntimeOp::FsXattrRemove if (2..=3).contains(&values.len()) => {
                let follow = lowered_bool_arg_or(values.get(2).cloned(), true, "fs.xattr_remove", span)?;
                let name = lowered_str_arg_owned(values.get(1).cloned(), "", "fs.xattr_remove", span)?;
                let path = lowered_path_arg(values.remove(0), "fs.xattr_remove", span)?;
                lowered_unit_result(fs_module::xattr_remove(self.host_path(&path), &name, follow, span))
            }
            callable; RuntimeOp::FsSyncPath if (1..=2).contains(&values.len()) => {
                let mode = lowered_str_arg_owned(values.get(1).cloned(), "all", "fs.sync_path", span)?;
                let path = lowered_path_arg(values.remove(0), "fs.sync_path", span)?;
                lowered_unit_result(fs_module::sync_path(self.host_path(&path), &mode, span))
            }
            callable; RuntimeOp::FsRenameExchange if values.len() == 2 => {
                let dest = lowered_path_arg(values.remove(1), "fs.rename_exchange", span)?;
                let source = lowered_path_arg(values.remove(0), "fs.rename_exchange", span)?;
                lowered_unit_result(fs_module::rename_exchange(self.host_path(&source), self.host_path(&dest), span))
            }
            callable; RuntimeOp::FsPathLimits if values.len() == 1 => {
                let path = lowered_path_arg(values.remove(0), "fs.path_limits", span)?;
                lowered_runtime_result(fs_module::path_limits(self.host_path(&path), span), span)?
            }
            callable; RuntimeOp::FsSync if values.is_empty() => {
                fs_module::sync_filesystems();
                lowered_result_ok(LoweredValue::Unit)
            }
            // The method's receiver is the link, so its operands arrive in
            // the reverse of `fs.symlink(target, path)`.
            callable; RuntimeOp::FsSymlinkAt if values.len() == 2 => {
                let target = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "Path.symlink",
                    span,
                )?;
                let path = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "Path.symlink",
                    span,
                )?;
                lowered_unit_result(fs_module::symlink_path(
                    pathbuf_from_path_value(&target),
                    self.host_path(&path),
                    span,
                ))
            }
            callable; RuntimeOp::FsSymlink if values.len() == 2 => {
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
            callable; RuntimeOp::FsTempFile if values.is_empty() => {
                match self.lowered_create_temp_file_root(span) {
                    Ok(record) => lowered_result_ok(record),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            internal; RuntimeOp::FsTempDir if values.is_empty() => {
                match new_temp_fs_root("fs-temp-dir", span) {
                    Ok(root) => lowered_result_ok(self.push_lowered_fs_root(root)),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::FsTempDirIn if values.len() == 1 => {
                let directory = lowered_path_arg(
                    values.remove(0),
                    "fs.tempdir_in",
                    span,
                )?;
                match new_temp_fs_root_in(directory, "fs-temp-dir", span) {
                    Ok(root) => lowered_result_ok(self.push_lowered_fs_root(root)),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::FsProjectRoot if values.len() == 4 => {
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
            callable; RuntimeOp::FsUserRoot if values.len() == 1 => {
                let kind = lowered_str_arg_owned(values.pop(), "", "fs.user_root", span)?;
                match self.lowered_user_root(&kind, span) {
                    Ok(root) => lowered_result_ok(root),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::FsLock if (1..=3).contains(&values.len()) => {
                let nonblocking =
                    lowered_bool_arg_or(values.get(2).cloned(), false, "fs.lock", span)?;
                let shared = lowered_bool_arg_or(values.get(1).cloned(), false, "fs.lock", span)?;
                let path = lowered_path_arg(values.remove(0), "fs.lock", span)?;
                match fs_module::lock_path(self.host_path(&path), shared, nonblocking, span) {
                    Ok(file) => {
                        let id = self.push_fs_capability(super::FsCapability::Lock(file));
                        lowered_result_ok(LoweredValue::FsLock(Box::new(super::FsLockValue {
                            id,
                            owner: self.resource_owner.clone(),
                            path,
                            shared,
                        })))
                    }
                    Err(error) => lowered_result_err_value(error),
                }
            }
            // As for a root: the scope's release accepts a lock the body
            // already released, and still rejects a foreign evaluator's lock.
            callable; RuntimeOp::FsUnlock | RuntimeOp::FsUnlockIfHeld if values.len() == 1 => return self.eval_lowered_fs_unlock_values(op, values, span),
            callable; RuntimeOp::GroupCurrent if values.is_empty() => {
                lowered_runtime_result(group_module::current(span), span)?
            }
            callable; RuntimeOp::GroupLookup if values.len() == 1 => {
                let name = lowered_str_arg_owned(values.pop(), "", "group.lookup", span)?;
                lowered_runtime_result(group_module::lookup(&name, span), span)?
            }
            callable; RuntimeOp::GroupByGid if values.len() == 1 => {
                let gid = lowered_int_arg(values.pop(), "group.by_gid", span)?;
                lowered_runtime_result(
                    group_module::gid_from_i64(gid, span)
                        .and_then(|gid| group_module::by_gid(gid, span)),
                    span,
                )?
            }
            callable; RuntimeOp::GroupAdd if values.len() == 1 || values.len() == 2 => {
                let gid = match values.get(1).cloned() {
                    Some(value) => Some(lowered_int_arg(Some(value), "group.add", span)?),
                    None => None,
                };
                let name = lowered_str_arg_owned(values.first().cloned(), "", "group.add", span)?;
                lowered_runtime_result(group_module::add(&name, gid, span), span)?
            }
            callable; RuntimeOp::GroupRemove if values.len() == 1 => {
                let name = lowered_str_arg_owned(values.pop(), "", "group.remove", span)?;
                lowered_runtime_result(group_module::remove(&name, span), span)?
            }
            callable; RuntimeOp::HashDigestFile if (2..=3).contains(&values.len()) => {
                let algorithm = lowered_str_arg_owned(values.get(1).cloned(), "", "hash.digest_file", span)?;
                let length = lowered_int_arg_or(values.get(2).cloned(), 0, "hash.digest_file", span)?;
                let path = lowered_path_arg(values.remove(0), "hash.digest_file", span)?;
                lowered_runtime_result(hash_module::named_digest_file(&algorithm, length, &self.host_path(&path), span).map(Value::digest), span)?
            }
            callable; RuntimeOp::HashDigestStdin if (1..=2).contains(&values.len()) => {
                if let Err(error) = crate::startup_stdio::check(0) {
                    return Ok(ControlFlow::Continue(lowered_result_err_value(RuntimeError::host("hash-digest-stdin", &error).with_span(span))));
                }
                let algorithm = lowered_str_arg_owned(values.get(0).cloned(), "", "hash.digest_stdin", span)?;
                let length = lowered_int_arg_or(values.get(1).cloned(), 0, "hash.digest_stdin", span)?;
                lowered_runtime_result(hash_module::named_digest_reader(&algorithm, length, &mut std::io::stdin().lock(), span).map(Value::digest), span)?
            }
            callable; RuntimeOp::HashChecksum if values.len() == 2 => {
                let algorithm = lowered_str_arg_owned(values.get(1).cloned(), "", "hash.checksum", span)?;
                let path = lowered_path_arg(values.remove(0), "hash.checksum", span)?;
                lowered_runtime_result(hash_module::checksum_file(&algorithm, &self.host_path(&path), span), span)?
            }
            callable; RuntimeOp::HashChecksumStdin if values.len() == 1 => {
                if let Err(error) = crate::startup_stdio::check(0) {
                    return Ok(ControlFlow::Continue(lowered_result_err_value(RuntimeError::host("hash-checksum-stdin", &error).with_span(span))));
                }
                let algorithm = lowered_str_arg_owned(values.get(0).cloned(), "", "hash.checksum_stdin", span)?;
                lowered_runtime_result(hash_module::checksum_reader(&algorithm, &mut std::io::stdin().lock(), span), span)?
            }
            callable; RuntimeOp::HashMd5
            | RuntimeOp::HashSha1
            | RuntimeOp::HashSha256
            | RuntimeOp::HashSha512
                if values.len() == 1 =>
            return self.eval_lowered_hash_md5_values(op, values, span),
            callable; RuntimeOp::HashCrc32 if values.len() == 1 => {
                let value = values.pop().expect("checked value length");
                let bytes = lowered_bytes_arg(&value, "hash.crc32", span)?;
                LoweredValue::Int(hash_module::crc32(bytes))
            }
            callable; RuntimeOp::HashCrc32c if values.len() == 1 => {
                let value = values.pop().expect("checked value length");
                let bytes = lowered_bytes_arg(&value, "hash.crc32c", span)?;
                LoweredValue::Int(hash_module::crc32c(bytes))
            }
            // Private bridges are reachable only from their declaring module.
            internal; RuntimeOp::BridgeTypeName if values.len() == 1 => {
                let name = values[0].type_name().to_string();
                LoweredValue::Str(name.into())
            }
            callable; RuntimeOp::HashParseCheckLine if values.len() == 1 => {
                let line = lowered_str_arg_owned(values.pop(), "", "hash.parse_check_line", span)?;
                match hash_module::parse_check_line(&line, span) {
                    Ok(line) => {
                        lowered_result_ok(LoweredValue::Record(Arc::new(BTreeMap::from([
                            (Arc::from("hex"), LoweredValue::Str(line.hex.into())),
                            (Arc::from("path"), LoweredValue::Str(line.path.into())),
                            (Arc::from("binary"), LoweredValue::Bool(line.binary)),
                        ]))))
                    }
                    Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::IniDecode if values.len() == 1 => {
                let text = lowered_str_arg_owned(values.pop(), "", "ini.decode", span)?;
                lowered_runtime_result(ini_module::decode(&text, span), span)?
            }
            callable; RuntimeOp::IniRead if values.len() == 1 => {
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
            callable; RuntimeOp::IniEncode if values.len() == 1 => {
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
            callable; RuntimeOp::IniWrite if values.len() == 2 || values.len() == 3 => return self.eval_lowered_ini_write_values(values, span),
            callable; RuntimeOp::IoStdinRead if values.len() == 1 => {
                let count = lowered_int_arg(values.pop(), "io.stdin_read", span)?;
                let result: Result<LoweredValue, RuntimeError> = (|| {
                    let count = usize::try_from(count).ok().filter(|count| *count > 0)
                        .ok_or_else(|| RuntimeError::new("io-stdin-read", "max_bytes must be positive").with_span(span))?;
                    let mut data = Vec::new();
                    data.try_reserve_exact(count).map_err(|error| {
                        RuntimeError::new("io-stdin-read", error.to_string()).with_span(span)
                    })?;
                    data.resize(count, 0);
                    let read = self.read_stdin_chunk(&mut data, "io.stdin_read", span)?;
                    data.truncate(read);
                    Ok(LoweredValue::Bytes(data.into()))
                })();
                match result {
                    Ok(data) => lowered_result_ok(data),
                    Err(error) if error.abort.is_some() => return Err(error),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::IoStdinBytes if values.is_empty() => {
                match self.read_stdin_all("io.stdin_bytes", span) {
                    Ok(data) => lowered_result_ok(LoweredValue::Bytes(data.into())),
                    Err(error) if error.abort.is_some() => return Err(error),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::IoStdinText if values.is_empty() => {
                let result = self.read_stdin_all("io.stdin_text", span).and_then(|data| {
                    String::from_utf8(data).map_err(|error| {
                        RuntimeError::host(
                            "io.stdin_text",
                            &std::io::Error::new(std::io::ErrorKind::InvalidData, error),
                        )
                        .with_span(span)
                    })
                });
                match result {
                    Ok(data) => lowered_result_ok(LoweredValue::Str(data.into())),
                    Err(error) if error.abort.is_some() => return Err(error),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::IoStdinLine if values.is_empty() => {
                let result = self.read_stdin_line_bytes("io.stdin_line", span).and_then(|data| {
                    String::from_utf8(data).map_err(|error| {
                        RuntimeError::host(
                            "io.stdin_line",
                            &std::io::Error::new(std::io::ErrorKind::InvalidData, error),
                        )
                        .with_span(span)
                    })
                });
                match result {
                    Ok(line) => lowered_result_ok(LoweredValue::Str(line.into())),
                    Err(error) if error.abort.is_some() => return Err(error),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::IoWriteStderr if values.len() == 1 => {
                let text = lowered_str_arg_owned(values.pop(), "", "io.write_stderr", span)?;
                self.stderr.extend_from_slice(text.as_bytes());
                lowered_result_ok(LoweredValue::Unit)
            }
            callable; RuntimeOp::IoWriteStderrBytes if values.len() == 1 => {
                let value = values.pop().expect("checked value length");
                let data = lowered_bytes_arg(&value, "io.write_stderr_bytes", span)?;
                self.stderr.extend_from_slice(data);
                lowered_result_ok(LoweredValue::Unit)
            }
            callable; RuntimeOp::IoFlushStderr if values.is_empty() => {
                lowered_unit_result(self.flush_stderr_checked(span))
            }
            callable; RuntimeOp::IoWriteStdout if values.len() == 1 => {
                let text = lowered_str_arg_owned(values.pop(), "", "io.write_stdout", span)?;
                self.stdout.extend_from_slice(text.as_bytes());
                lowered_result_ok(LoweredValue::Unit)
            }
            callable; RuntimeOp::IoFlushStdout if values.is_empty() => {
                lowered_unit_result(self.flush_stdout_checked(span))
            }
            callable; RuntimeOp::IoWriteStdoutBytes if values.len() == 1 => {
                let value = values.pop().expect("checked value length");
                let data = lowered_bytes_arg(&value, "io.write_stdout_bytes", span)?;
                self.stdout.extend_from_slice(data);
                lowered_result_ok(LoweredValue::Unit)
            }
            callable; RuntimeOp::MapEmpty if values.is_empty() => {
                LoweredValue::Map(Arc::new(BTreeMap::new()))
            }
            callable; RuntimeOp::TimeToCalendar => {
                let epoch = lowered_int_arg(values.first().cloned(), "time.to_calendar", span)?;
                let utc = lowered_bool_arg_or(values.get(1).cloned(), false, "time.to_calendar", span)?;
                match crate::modules::time::to_calendar(epoch, utc) {
                    Ok(calendar) => lowered_result_ok(LoweredValue::Record(Arc::new(BTreeMap::from([
                        (Arc::from("year"), LoweredValue::Int(calendar.year)),
                        (Arc::from("month"), LoweredValue::Int(calendar.month)),
                        (Arc::from("day"), LoweredValue::Int(calendar.day)),
                        (Arc::from("hour"), LoweredValue::Int(calendar.hour)),
                        (Arc::from("minute"), LoweredValue::Int(calendar.minute)),
                        (Arc::from("second"), LoweredValue::Int(calendar.second)),
                        (Arc::from("weekday"), LoweredValue::Int(calendar.weekday)),
                        (Arc::from("offset_seconds"), LoweredValue::Int(calendar.offset_seconds)),
                        (Arc::from("nanosecond"), LoweredValue::Int(calendar.nanosecond)),
                    ])))),
                    Err(message) => lowered_result_err_value(RuntimeError::new("time-calendar", message).with_span(span)),
                }
            }
            callable; RuntimeOp::TimeFormat if (2..=3).contains(&values.len()) => {
                let epoch = lowered_int_arg(values.get(0).cloned(), "time.format", span)?;
                let format = lowered_str_arg_owned(values.get(1).cloned(), "", "time.format", span)?;
                let utc = lowered_bool_arg_or(values.get(2).cloned(), false, "time.format", span)?;
                lowered_runtime_result(crate::modules::time::format(epoch, &format, utc).map(|text| Value::Str(text.into())).map_err(|message| RuntimeError::new("time-format", message).with_span(span)), span)?
            }
            callable; RuntimeOp::TimeFromCalendar if (3..=8).contains(&values.len()) => {
                let operation = "time.from_calendar";
                let year = lowered_int_arg(values.get(0).cloned(), operation, span)?;
                let month = lowered_int_arg(values.get(1).cloned(), operation, span)?;
                let day = lowered_int_arg(values.get(2).cloned(), operation, span)?;
                let hour = lowered_int_arg_or(values.get(3).cloned(), 0, operation, span)?;
                let minute = lowered_int_arg_or(values.get(4).cloned(), 0, operation, span)?;
                let second = lowered_int_arg_or(values.get(5).cloned(), 0, operation, span)?;
                let utc = lowered_bool_arg_or(values.get(6).cloned(), false, operation, span)?;
                let normalize = lowered_bool_arg_or(values.get(7).cloned(), false, operation, span)?;
                lowered_runtime_result(crate::modules::time::from_calendar(year, month, day, hour, minute, second, utc, normalize).map(Value::Int).map_err(|message| RuntimeError::new("time-calendar", message).with_span(span)), span)?
            }
            callable; RuntimeOp::TimeClockResolution if values.is_empty() => {
                lowered_runtime_result(crate::modules::time::clock_resolution().map(Value::Int).map_err(|message| RuntimeError::new("time-resolution", message).with_span(span)), span)?
            }
            callable; RuntimeOp::TimeMonotonic if values.is_empty() => {
                lowered_runtime_result(crate::modules::time::monotonic_ns().map(Value::Int).map_err(|message| RuntimeError::new("time-monotonic", message).with_span(span)), span)?
            }
            callable; RuntimeOp::TimeNow if values.is_empty() => {
                LoweredValue::Int(crate::modules::time::now_epoch_ms())
            }
            callable; RuntimeOp::TimeSleep if values.len() == 1 => {
                let duration = lowered_duration_arg(values.pop(), "time.sleep", span)?;
                let deadline = std::time::Instant::now() + Duration::from_millis(duration.millis);
                while std::time::Instant::now() < deadline {
                    self.service_pending_signal(span)?;
                    if self.shutting_down() {
                        break;
                    }
                    let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                    std::thread::sleep(std::cmp::min(WAIT_POLL, remaining));
                }
                lowered_result_ok(LoweredValue::Unit)
            }
            callable; RuntimeOp::TimeMillis if values.len() == 1 => {
                let ms = lowered_int_arg(values.pop(), "time.millis", span)?;
                let millis = if ms < 0 { 0 } else { ms as u64 };
                LoweredValue::Duration(DurationValue { millis })
            }
            callable; RuntimeOp::TimeSeconds if values.len() == 1 => {
                let seconds = lowered_int_arg(values.pop(), "time.seconds", span)?;
                let millis = if seconds < 0 {
                    0
                } else {
                    (seconds as u64).saturating_mul(1000)
                };
                LoweredValue::Duration(DurationValue { millis })
            }
            callable; RuntimeOp::TimeMeasure if values.len() == 1 || values.len() == 2 => return self.eval_lowered_time_measure_values(values, span),
            callable; RuntimeOp::TimeDurationCompact if values.len() == 1 => {
                let seconds = lowered_int_arg(values.pop(), "time.duration_compact", span)?;
                LoweredValue::Str(crate::modules::time::duration_compact(seconds).into())
            }
            callable; RuntimeOp::BytesHuman if values.len() == 1 => {
                let size = lowered_int_arg(values.pop(), "bytes.human", span)?;
                LoweredValue::Str(bytes_module::human(size).into())
            }
            callable; RuntimeOp::BytesCopy if (2..=7).contains(&values.len()) => return self.eval_lowered_bytes_copy_values(values, span),
            callable; RuntimeOp::BytesCopyFile if (2..=7).contains(&values.len()) => return self.eval_lowered_bytes_copy_file_values(values, span),
            callable; RuntimeOp::BytesZero if values.len() == 1 => {
                let length = lowered_int_arg(values.pop(), "bytes.zero", span)?;
                match bytes_module::zero(length, span) {
                    Ok(bytes) => lowered_result_ok(LoweredValue::Bytes(bytes.into())),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::BytesFromText if values.len() == 1 => {
                let text = lowered_str_arg_owned(values.pop(), "", "bytes.from_text", span)?;
                LoweredValue::Bytes(bytes_module::from_text(&text).into())
            }
            callable; RuntimeOp::BytesFromInts if values.len() == 1 => {
                let ints = lowered_int_list_arg(values.pop(), "bytes.from_ints", span)?;
                match bytes_module::from_ints(ints, span) {
                    Ok(bytes) => lowered_result_ok(LoweredValue::Bytes(bytes.into())),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::BytesSqueeze if values.len() == 2 => {
                let byte = lowered_int_arg(values.get(1).cloned(), "bytes.squeeze", span)?;
                let data = lowered_bytes_arg(
                    values.first().expect("checked value length"),
                    "bytes.squeeze",
                    span,
                )?;
                match bytes_module::squeeze(data, byte, span) {
                    Ok(bytes) => lowered_result_ok(LoweredValue::Bytes(bytes.into())),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::BytesRepeatPrefixCount if values.len() == 2 => {
                let pattern = lowered_bytes_arg(
                    values.get(1).expect("checked value length"),
                    "bytes.repeat_prefix_count",
                    span,
                )?;
                let data = lowered_bytes_arg(
                    values.first().expect("checked value length"),
                    "bytes.repeat_prefix_count",
                    span,
                )?;
                match bytes_module::repeat_prefix_count(data, pattern, span) {
                    Ok(count) => lowered_result_ok(LoweredValue::Int(count)),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::BytesConcat if values.len() == 1 => {
                let chunks = lowered_bytes_list_arg(values.pop(), "bytes.concat", span)?;
                LoweredValue::Bytes(bytes_module::concat(chunks).into())
            }
            callable; RuntimeOp::BytesPackLe if values.len() == 2 => {
                let width = lowered_int_arg(values.pop(), "bytes.pack_le", span)?;
                let value = lowered_int_arg(values.pop(), "bytes.pack_le", span)?;
                match bytes_module::pack_int_le(value, width, span) {
                    Ok(bytes) => lowered_result_ok(LoweredValue::Bytes(bytes.into())),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::BytesPackBe if values.len() == 2 => {
                let width = lowered_int_arg(values.pop(), "bytes.pack_be", span)?;
                let value = lowered_int_arg(values.pop(), "bytes.pack_be", span)?;
                match bytes_module::pack_int_be(value, width, span) {
                    Ok(bytes) => lowered_result_ok(LoweredValue::Bytes(bytes.into())),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::BytesUnpackLe if values.len() == 2 || values.len() == 3 => {
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
            callable; RuntimeOp::BytesUnpackBe if values.len() == 2 || values.len() == 3 => {
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
            callable; RuntimeOp::BytesUnpackFloat if values.len() == 4 => {
                let data = lowered_bytes_arg(values.first().expect("checked value length"), "bytes.unpack_float", span)?;
                let offset = lowered_int_arg(values.get(1).cloned(), "bytes.unpack_float", span)?;
                let format = lowered_str_arg_owned(values.get(2).cloned(), "", "bytes.unpack_float", span)?;
                let endian = lowered_str_arg_owned(values.get(3).cloned(), "", "bytes.unpack_float", span)?;
                match bytes_module::unpack_float(data, offset, &format, &endian, span) {
                    Ok(value) => lowered_result_ok(LoweredValue::Float(crate::runtime::value::FloatValue::new(value))),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::BytesResize => {
                let path = lowered_path_arg(unix_require_arg(values.first().cloned(), "bytes.resize", span)?, "bytes.resize", span)?;
                let size = lowered_int_arg(values.get(1).cloned(), "bytes.resize", span)?;
                let create = lowered_bool_arg_or(values.get(2).cloned(), false, "bytes.resize", span)?;
                let exclusive = lowered_bool_arg_or(values.get(3).cloned(), false, "bytes.resize", span)?;
                let regular = lowered_bool_arg_or(values.get(4).cloned(), false, "bytes.resize", span)?;
                match bytes_module::resize(self.host_path(&path), size, create, exclusive, regular, span) {
                    Ok(()) => lowered_result_ok(LoweredValue::Unit), Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::BytesReadAt if (3..=4).contains(&values.len()) => {
                let regular = if values.len() == 4 { lowered_bool_arg_or(values.pop(), false, "bytes.read_at", span)? } else { false };
                let length = lowered_int_arg(values.pop(), "bytes.read_at", span)?;
                let offset = lowered_int_arg(values.pop(), "bytes.read_at", span)?;
                let path = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "bytes.read_at",
                    span,
                )?;
                match bytes_module::read_at(self.host_path(&path), offset, length, regular, span) {
                    Ok(bytes) => lowered_result_ok(LoweredValue::Bytes(bytes.into())),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::BytesWriteAt if (3..=5).contains(&values.len()) => {
                let regular = lowered_bool_arg_or(values.get(4).cloned(), false, "bytes.write_at", span)?;
                let create =
                    lowered_bool_arg_or(values.get(3).cloned(), false, "bytes.write_at", span)?;
                let data_value = values.remove(2);
                let data = lowered_bytes_arg(&data_value, "bytes.write_at", span)?;
                let offset = lowered_int_arg(Some(values.remove(1)), "bytes.write_at", span)?;
                let path = lowered_path_arg(values.remove(0), "bytes.write_at", span)?;
                match bytes_module::write_at(self.host_path(&path), offset, data, create, regular, span) {
                    Ok(written) => lowered_result_ok(LoweredValue::Int(written)),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::BytesZeroAt if (3..=5).contains(&values.len()) => {
                let regular = lowered_bool_arg_or(values.get(4).cloned(), false, "bytes.zero_at", span)?;
                let create =
                    lowered_bool_arg_or(values.get(3).cloned(), false, "bytes.zero_at", span)?;
                let length = lowered_int_arg(Some(values.remove(2)), "bytes.zero_at", span)?;
                let offset = lowered_int_arg(Some(values.remove(1)), "bytes.zero_at", span)?;
                let path = lowered_path_arg(values.remove(0), "bytes.zero_at", span)?;
                match bytes_module::zero_at(self.host_path(&path), offset, length, create, regular, span) {
                    Ok(written) => lowered_result_ok(LoweredValue::Int(written)),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::EnvGet if values.len() == 1 => return self.eval_lowered_env_get_values(values, span),
            // `e"NAME" = value`: the parser admits only identifier names, and
            // the value converts like an `env (...)` overlay value. The change
            // lives in the evaluator environment, so the innermost enclosing
            // context scope undoes it and child processes inherit it.
            callable; RuntimeOp::EnvSet if values.len() == 2 => {
                let value = values.pop().expect("checked value length");
                let name = lowered_str_arg_owned(values.pop(), "", "env", span)?;
                let value = value_to_env_bytes(value.into_value(), span)?;
                self.env.insert(name.into_bytes(), value);
                LoweredValue::Unit
            }
            // Read the scoped environment overlay at invocation time. Only an
            // unset name selects a fallback; invalid names and bytes keep
            // their original error kinds.
            callable; RuntimeOp::EnvGetOr if values.len() == 1 || values.len() == 2 => {
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
            callable; RuntimeOp::EnvBool if values.len() == 1 || values.len() == 2 => return self.eval_lowered_env_bool_values(values, span),
            callable; RuntimeOp::EnvPath if values.len() == 1 || values.len() == 2 => return self.eval_lowered_env_path_values(values, span),
            callable; RuntimeOp::EnvInt if values.len() == 1 || values.len() == 2 => return self.eval_lowered_env_int_values(values, span),
            callable; RuntimeOp::EnvEntries if values.is_empty() => {
                let items = self
                    .env
                    .snapshot()
                    .iter()
                    .map(|(name, value)| {
                        LoweredValue::Record(Arc::new(BTreeMap::from([
                            (Arc::from("name"), LoweredValue::Bytes(name.clone().into())),
                            (Arc::from("value"), LoweredValue::Bytes(value.clone().into())),
                        ])))
                    })
                    .collect();
                lowered_result_ok(LoweredValue::List(items))
            }
            callable; RuntimeOp::EnvList if values.is_empty() => return self.eval_lowered_env_list_values(values, span),
            callable; RuntimeOp::EnvPathList if values.is_empty() => {
                let paths = self
                    .lowered_env_path_entries(span)?
                    .into_iter()
                    .map(LoweredValue::Path)
                    .collect();
                LoweredValue::List(paths)
            }
            callable; RuntimeOp::EnvPathList if values.len() == 1 => return self.eval_lowered_env_path_list_values(values, span),
            internal; RuntimeOp::EnvPathPrepend if values.len() == 1 => {
                let path = lowered_path_arg(values.remove(0), "env.PATH.prepend", span)?;
                let mut entries = self.lowered_env_path_entries(span)?;
                entries.insert(0, path);
                self.lowered_set_env_path_entries(&entries, span)?;
                lowered_result_ok(LoweredValue::Unit)
            }
            internal; RuntimeOp::EnvPathAppend if values.len() == 1 => {
                let path = lowered_path_arg(values.remove(0), "env.PATH.append", span)?;
                let mut entries = self.lowered_env_path_entries(span)?;
                entries.push(path);
                self.lowered_set_env_path_entries(&entries, span)?;
                lowered_result_ok(LoweredValue::Unit)
            }
            internal; RuntimeOp::EnvPathPop if values.is_empty() => {
                let mut entries = self.lowered_env_path_entries(span)?;
                let Some(path) = entries.pop() else {
                    return Ok(ControlFlow::Continue(lowered_result_err_value(
                        RuntimeError::new("env-path-empty", "PATH is empty").with_span(span),
                    )));
                };
                self.lowered_set_env_path_entries(&entries, span)?;
                lowered_result_ok(LoweredValue::Path(path))
            }
            callable; RuntimeOp::EnvPathEntries if values.len() == 1 => return self.eval_lowered_env_path_entries_values(values, span),
            callable; RuntimeOp::JsonDecode if values.len() == 1 => {
                let Some(text) = lowered_str_value(&values[0]) else {
                    return Err(
                        RuntimeError::new("type-error", "json.decode expected Str").with_span(span)
                    );
                };
                lowered_runtime_result(json_module::parse_json(text, span), span)?
            }
            callable; RuntimeOp::JsonEncode if values.len() == 1 || values.len() == 2 => {
                let pretty =
                    lowered_bool_arg_or(values.get(1).cloned(), false, "json.encode", span)?;
                match super::encode_lowered_json(&values[0], pretty, span) {
                    Ok(text) => lowered_result_ok(LoweredValue::Str(text.into())),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::JsonWrite if values.len() == 2 || values.len() == 3 => {
                let pretty =
                    lowered_bool_arg_or(values.get(2).cloned(), false, "json.write", span)?;
                let path = lowered_path_arg(values.remove(0), "json.write", span)?;
                match super::encode_lowered_json_for_write(&values[0], pretty, span) {
                    Ok(text) => lowered_unit_result(fs_module::write_path(
                        self.host_path(&path),
                        text.as_bytes(),
                        span,
                    )),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::JsonWriteLines if values.len() == 2 => {
                let path = lowered_path_arg(values.remove(0), "json.write_lines", span)?;
                match super::encode_lowered_json_lines_for_write(&values[0], span) {
                    Ok(text) => lowered_unit_result(fs_module::write_path(
                        self.host_path(&path),
                        text.as_bytes(),
                        span,
                    )),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::JsonGet if values.len() == 2 || values.len() == 3 => {
                let value = values[0].clone().into_value();
                let path = lowered_json_path_arg(&values[1], "json.get", span)?;
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
            callable; RuntimeOp::JsonRead if values.len() == 1 => {
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
            callable; RuntimeOp::JsonRemove if values.len() == 2 => {
                let value = values[0].clone().into_value();
                let path = lowered_json_path_arg(&values[1], "json.remove", span)?;
                lowered_runtime_result(json_module::json_path_remove(&value, &path, span), span)?
            }
            callable; RuntimeOp::JsonSet if values.len() == 3 => {
                let value = values[0].clone().into_value();
                let path = lowered_json_path_arg(&values[1], "json.set", span)?;
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
            callable; RuntimeOp::LinuxInterfaces if values.is_empty() => return self.eval_lowered_linux_interfaces_values(values, span),
            callable; RuntimeOp::LinuxRoutes if values.is_empty() => return self.eval_lowered_linux_routes_values(values, span),
            callable; RuntimeOp::LinuxNetworkDump if values.is_empty() => return self.eval_lowered_linux_network_dump_values(values, span),
            callable; RuntimeOp::LinuxLinkUp if values.len() == 1 => {
                let interface = lowered_str_arg_owned(values.pop(), "", "linux.link_up", span)?;
                if !self.linux_fake_active() {
                    lowered_runtime_result(linux_module::link_up(&interface, span), span)?
                } else {
                    self.linux_fake_log("link_up", &[("interface", interface)], span)?;
                    lowered_result_ok(LoweredValue::Unit)
                }
            }
            callable; RuntimeOp::LinuxLinkDown if values.len() == 1 => {
                let interface = lowered_str_arg_owned(values.pop(), "", "linux.link_down", span)?;
                if !self.linux_fake_active() {
                    lowered_runtime_result(linux_module::link_down(&interface, span), span)?
                } else {
                    self.linux_fake_log("link_down", &[("iface", interface)], span)?;
                    lowered_result_ok(LoweredValue::Unit)
                }
            }
            callable; RuntimeOp::LinuxSetIpv4Address if values.len() == 3 => return self.eval_lowered_linux_set_ipv4_address_values(values, span),
            callable; RuntimeOp::LinuxFlushIpv4Addresses if values.len() == 1 => {
                let interface =
                    lowered_str_arg_owned(values.pop(), "", "linux.flush_ipv4_addresses", span)?;
                if !self.linux_fake_active() {
                    lowered_runtime_result(
                        linux_module::flush_ipv4_addresses(&interface, span),
                        span,
                    )?
                } else {
                    self.linux_fake_log("flush_ipv4_addresses", &[("iface", interface)], span)?;
                    lowered_result_ok(LoweredValue::Unit)
                }
            }
            callable; RuntimeOp::LinuxAddDefaultIpv4Route if values.len() == 1 || values.len() == 2 => return self.eval_lowered_linux_add_default_ipv4_route_values(values, span),
            callable; RuntimeOp::LinuxDelDefaultIpv4Route if values.len() == 2 => return self.eval_lowered_linux_del_default_ipv4_route_values(values, span),
            callable; RuntimeOp::LinuxDhcpSocket if values.len() == 1 => {
                let interface = lowered_str_arg_owned(values.pop(), "", "linux.dhcp_socket", span)?;
                if !self.linux_fake_active() {
                    lowered_runtime_result(linux_module::dhcp_socket(&interface, span), span)?
                } else {
                    self.linux_fake_log("dhcp_socket", &[("interface", interface)], span)?;
                    lowered_result_ok(LoweredValue::Int(-1))
                }
            }
            callable; RuntimeOp::LinuxDhcpSend if values.len() == 2 => {
                let fd = lowered_int_arg(values.first().cloned(), "linux.dhcp_send", span)?;
                let payload = lowered_bytes_arg(&values[1], "linux.dhcp_send", span)?;
                if !self.linux_fake_active() {
                    lowered_runtime_result(linux_module::dhcp_send(fd, payload, span), span)?
                } else {
                    self.linux_fake_log(
                        "dhcp_send",
                        &[("bytes", payload.len().to_string())],
                        span,
                    )?;
                    lowered_result_ok(LoweredValue::Unit)
                }
            }
            callable; RuntimeOp::LinuxDhcpRecv if values.len() == 2 => {
                let fd = lowered_int_arg(values.first().cloned(), "linux.dhcp_recv", span)?;
                let timeout = lowered_int_arg(values.get(1).cloned(), "linux.dhcp_recv", span)?;
                if !self.linux_fake_active() {
                    lowered_runtime_result(linux_module::dhcp_recv(fd, timeout, span), span)?
                } else {
                    self.linux_fake_log("dhcp_recv", &[("timeout_ms", timeout.to_string())], span)?;
                    lowered_result_ok(LoweredValue::Bytes(Vec::new().into()))
                }
            }
            callable; RuntimeOp::LinuxDhcpClose if values.len() == 1 => {
                let fd = lowered_int_arg(values.pop(), "linux.dhcp_close", span)?;
                if !self.linux_fake_active() {
                    lowered_runtime_result(linux_module::dhcp_close(fd, span), span)?
                } else {
                    self.linux_fake_log("dhcp_close", &[("fd", fd.to_string())], span)?;
                    lowered_result_ok(LoweredValue::Unit)
                }
            }
            callable; RuntimeOp::LinuxDhcpSendRelease if values.len() == 3 => return self.eval_lowered_linux_dhcp_send_release_values(values, span),
            callable; RuntimeOp::MimeParse if values.len() == 1 => {
                let value = lowered_str_arg_owned(values.pop(), "", "mime.parse", span)?;
                match mime_module::parse(&value, span) {
                    Ok(value) => lowered_runtime_value(Value::ok(value), span)?,
                    Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::MimeLookupExt if values.len() == 1 => {
                let ext = lowered_str_arg_owned(values.pop(), "", "mime.lookup_ext", span)?;
                lowered_runtime_value(mime_module::lookup_ext(&ext).unwrap_or(Value::Null), span)?
            }
            callable; RuntimeOp::MimeLookupPath if values.len() == 1 => {
                let path = lowered_path_arg(
                    values.pop().expect("checked value length"),
                    "mime.lookup_path",
                    span,
                )?;
                lowered_runtime_value(
                    mime_module::lookup_path(&path.display()).unwrap_or(Value::Null),
                    span,
                )?
            }
            callable; RuntimeOp::ModuleLoad if values.len() == 1 => return self.eval_lowered_module_load_values(values, span),
            callable; RuntimeOp::NetPool if values.len() <= 3 => return self.eval_lowered_net_pool_values(values, span),
            callable; RuntimeOp::NetClosePool if values.len() <= 1 => {
                let name = lowered_str_arg_owned(values.pop(), "default", "net.close_pool", span)?;
                self.net_agents.retain(|key, _| key.pool != name);
                lowered_result_ok(LoweredValue::Unit)
            }
            callable; RuntimeOp::NetCloseAllPools if values.is_empty() => {
                self.net_agents.clear();
                lowered_result_ok(LoweredValue::Unit)
            }
            callable; RuntimeOp::NetStart if values.len() == 1 => return self.eval_lowered_net_start_values(values, span),
            callable; RuntimeOp::NetRequest if values.len() == 1 => return self.eval_lowered_net_request_values(values, span),
            callable; RuntimeOp::NetRequestMany if values.len() == 1 => return self.eval_lowered_net_request_many_values(values, span),
            callable; RuntimeOp::NetDownloadMany if values.len() == 1 => return self.eval_lowered_net_download_many_values(values, span),
            callable; RuntimeOp::NetDownload if values.len() == 1 => return self.eval_lowered_net_download_values(values, span),
            callable; RuntimeOp::NetUpload if values.len() == 1 => return self.eval_lowered_net_upload_values(values, span),
            callable; RuntimeOp::PathAbsolute if values.len() == 1 => {
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
            callable; RuntimeOp::PathParseBytes if values.len() == 1 => {
                let value = values.pop().expect("checked value length");
                let bytes = lowered_bytes_arg(&value, "Path.parse_bytes", span)?;
                match PathValue::new(bytes.to_vec()) {
                    Ok(path) => lowered_result_ok(LoweredValue::Path(path)),
                    Err(error) => lowered_result_err_value(error.with_span(span)),
                }
            }
            callable; RuntimeOp::PatchApply
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
            callable; RuntimeOp::ProcessList if values.is_empty() => {
                lowered_runtime_stream_result(process_module::list_processes(span), span)?
            }
            callable; RuntimeOp::ProcessThreads if values.is_empty() || values.len() == 1 => {
                let pid = match values.pop() {
                    Some(value) => Some(lowered_int_arg(Some(value), "process.threads", span)?),
                    None => None,
                };
                lowered_runtime_stream_result(process_module::list_threads(pid, span), span)?
            }
            callable; RuntimeOp::ProcessCurrentPid if values.is_empty() => {
                lowered_result_ok(LoweredValue::Int(std::process::id() as i64))
            }
            callable; RuntimeOp::ProcessScriptPath if values.is_empty() => {
                match process_module::script_path(span) {
                    Ok(path) => lowered_result_ok(LoweredValue::Path(path)),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::ProcessStats if values.len() == 1 => {
                let pid = lowered_int_arg(values.pop(), "process.stats", span)?;
                lowered_runtime_result(process_module::process_stats(pid, span), span)?
            }
            callable; RuntimeOp::ProcessWhich if values.len() == 1 => return self.eval_lowered_process_which_values(values, span),
            callable; RuntimeOp::ProcessPort if values.len() == 1 => {
                let port = lowered_int_arg(values.pop(), "process.port", span)?;
                lowered_runtime_stream_result(process_module::port_processes(port, span), span)?
            }
            callable; RuntimeOp::ProcessPorts if values.is_empty() => {
                lowered_runtime_stream_result(process_module::listening_port_processes(span), span)?
            }
            callable; RuntimeOp::ProcessPortsForPid if values.len() == 1 => {
                let pid = lowered_int_arg(values.pop(), "process.ports", span)?;
                lowered_runtime_stream_result(process_module::pid_port_processes(pid, span), span)?
            }
            callable; RuntimeOp::ProcessSignal if values.len() == 1 => {
                let signal = lowered_str_arg_owned(values.pop(), "", "process.signal", span)?;
                lowered_runtime_result(
                    process_module::signal_info(&signal, span).map(process_module::signal_record),
                    span,
                )?
            }
            callable; RuntimeOp::ProcessKill if values.len() == 1 || values.len() == 2 => {
                let signal = lowered_str_arg_owned(values.pop(), "TERM", "process.kill", span)?;
                let pid = lowered_int_arg(values.pop(), "process.kill", span)?;
                lowered_module_result_value(self.process_kill(pid, &signal, span), span)?
            }
            callable; RuntimeOp::ProcessRun if values.len() == 1 => {
                let plan = lowered_command_arg(
                    values.pop().expect("checked value length"),
                    "process.run",
                    span,
                )?;
                let invocation = self.invocation_from_command_plan(&plan, span)?;
                self.run_process_invocation(invocation, span)
            }
            callable; RuntimeOp::LinuxRunInNamespaces | RuntimeOp::LinuxNamespaces
                if self.linux_fake_active() =>
            {
                lowered_result_err_value(
                    RuntimeError::new(
                        "linux-fake-unsupported",
                        "namespace operations have no Linux fake implementation",
                    )
                    .with_span(span),
                )
            }
            callable; RuntimeOp::LinuxRunInNamespaces => self.eval_run_in_namespaces(values, span)?,
            callable; RuntimeOp::LinuxNamespaces => {
                let pid = lowered_optional_int_arg(values.get(0), "linux.namespaces", span)?;
                lowered_runtime_result(linux_module::namespaces(pid, span), span)?
            }
            callable; RuntimeOp::ProcessSpawn if values.len() == 1 => return self.eval_lowered_process_spawn_values(values, span),
            callable; RuntimeOp::ProcessWaitAny | RuntimeOp::ProcessWaitTimeout
                if values.len() == 1 || values.len() == 2 =>
            return self.eval_lowered_process_wait_any_values(op, values, span),
            callable; RuntimeOp::ProcessWaitReady if values.len() == 1 => return self.eval_lowered_process_wait_ready_values(values, span),
            callable; RuntimeOp::RegexCompile if values.len() == 1 => {
                let text = lowered_str_arg_owned(values.pop(), "", "regex.compile", span)?;
                match regex_module::compile(&text, span) {
                    Ok(regex) => lowered_result_ok(LoweredValue::Regex(Box::new(RegexValue {
                        pattern: text,
                        regex: Arc::new(regex),
                    }))),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::RegexCapturesBytes => {
                let pattern = lowered_str_arg_owned(values.first().cloned(), "", "regex.captures_bytes", span)?;
                let input = lowered_bytes_arg_or_empty(values.get(1).cloned(), "regex.captures_bytes", span)?;
                let offset = lowered_int_arg_or(values.get(2).cloned(), 0, "regex.captures_bytes", span)?;
                let extended = lowered_bool_arg_or(values.get(3).cloned(), false, "regex.captures_bytes", span)?;
                let ignore_case = lowered_bool_arg_or(values.get(4).cloned(), false, "regex.captures_bytes", span)?;
                match regex_module::captures_bytes(&pattern, &input, offset, extended, ignore_case, span) {
                    Ok(value) => lowered_result_ok(lowered_runtime_value(value, span)?), Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::UnixReadFd => {
                let fd = lowered_int_arg(values.first().cloned(), "unix.read_fd", span)?;
                let max_bytes = lowered_int_arg(values.get(1).cloned(), "unix.read_fd", span)?;
                lowered_runtime_result(unix_module::read_fd(fd, max_bytes, span), span)?
            }
            callable; RuntimeOp::UnixWriteFd => {
                let fd = lowered_int_arg(values.first().cloned(), "unix.write_fd", span)?;
                let data = lowered_bytes_arg(values.get(1).ok_or_else(|| RuntimeError::new("type-error", "unix.write_fd expected Bytes").with_span(span))?, "unix.write_fd", span)?;
                lowered_runtime_result(unix_module::write_fd(fd, data, span), span)?
            }
            callable; RuntimeOp::UnixSeekFd => {
                let fd = lowered_int_arg(values.first().cloned(), "unix.seek_fd", span)?;
                let offset = lowered_int_arg(values.get(1).cloned(), "unix.seek_fd", span)?;
                let whence = lowered_str_arg_owned(values.get(2).cloned(), "start", "unix.seek_fd", span)?;
                lowered_runtime_result(unix_module::seek_fd(fd, offset, &whence, span), span)?
            }
            callable; RuntimeOp::UnixFadvise => {
                let fd = lowered_int_arg(values.first().cloned(), "unix.fadvise", span)?;
                let offset = lowered_int_arg(values.get(1).cloned(), "unix.fadvise", span)?;
                let length = lowered_int_arg(values.get(2).cloned(), "unix.fadvise", span)?;
                let advice = lowered_str_arg_owned(values.get(3).cloned(), "", "unix.fadvise", span)?;
                lowered_runtime_result(unix_module::fadvise(fd, offset, length, &advice, span), span)?
            }
            callable; RuntimeOp::UnixCpuFeatures if values.is_empty() => {
                lowered_runtime_value(unix_module::cpu_features(), span)?
            }
            callable; RuntimeOp::RegexFindBytes if (2..=4).contains(&values.len()) => {
                let pattern = lowered_str_arg_owned(values.first().cloned(), "", "regex.find_bytes", span)?;
                let input = lowered_bytes_arg_or_empty(values.get(1).cloned(), "regex.find_bytes", span)?;
                let extended = lowered_bool_arg_or(values.get(2).cloned(), false, "regex.find_bytes", span)?;
                let ignore_case = lowered_bool_arg_or(values.get(3).cloned(), false, "regex.find_bytes", span)?;
                match regex_module::find_bytes(&pattern, &input, extended, ignore_case, span) {
                    Ok(found) => lowered_result_ok(lowered_runtime_value(found, span)?),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::LocaleNumericInfo | RuntimeOp::LocaleTimeInfo => {
                let api = if op == RuntimeOp::LocaleNumericInfo { "system.locale_numeric_info" } else { "system.locale_time_info" };
                let name = lowered_str_arg_owned(values.first().cloned(), "", api, span)?;
                let result = if op == RuntimeOp::LocaleNumericInfo {
                    crate::modules::locale::numeric_info(&name, span)
                } else {
                    crate::modules::locale::time_info(&name, span)
                };
                lowered_runtime_result(result, span)?
            }
            callable; RuntimeOp::NumericLongDoublePrecision => LoweredValue::Int(crate::modules::numeric::precision()),
            callable; RuntimeOp::NumericParseLongDouble => {
                let text = lowered_str_arg_owned(values.first().cloned(), "", "system.parse_long_double", span)?;
                lowered_runtime_result(crate::modules::numeric::parse(&text, span), span)?
            }
            callable; RuntimeOp::NumericFormatLongDouble => {
                let text = lowered_str_arg_owned(values.first().cloned(), "", "system.format_long_double", span)?;
                let conversion = lowered_str_arg_owned(values.get(1).cloned(), "", "system.format_long_double", span)?;
                let precision = match values.get(2) {
                    None | Some(LoweredValue::Null) => None,
                    _ => Some(lowered_int_arg(values.get(2).cloned(), "system.format_long_double", span)?),
                };
                let alternate = lowered_bool_arg_or(values.get(3).cloned(), false, "system.format_long_double", span)?;
                lowered_runtime_result(crate::modules::numeric::format(&text, &conversion, precision, alternate, span), span)?
            }
            callable; RuntimeOp::CompressionTransform => {
                let source = match values.first().cloned() { None | Some(LoweredValue::Null) => None, Some(value) => Some(lowered_path_arg(value, "compression.transform", span)?) };
                let destination = match values.get(1).cloned() { None | Some(LoweredValue::Null) => None, Some(value) => Some(lowered_path_arg(value, "compression.transform", span)?) };
                let source = source.as_ref().map(|path| self.host_path(path));
                let destination = destination.as_ref().map(|path| self.host_path(path));
                let format = lowered_str_arg_owned(values.get(2).cloned(), "gzip", "compression.transform", span)?;
                let request = crate::modules::compression::TransformRequest {
                    source: source.as_deref(), destination: destination.as_deref(), format: &format,
                    decode: lowered_bool_arg_or(values.get(3).cloned(), false, "compression.transform", span)?,
                    level: lowered_int_arg_or(values.get(4).cloned(), 6, "compression.transform", span)?,
                    test: lowered_bool_arg_or(values.get(5).cloned(), false, "compression.transform", span)?,
                    metadata: lowered_bool_arg_or(values.get(6).cloned(), true, "compression.transform", span)?,
                    overwrite: lowered_bool_arg_or(values.get(7).cloned(), false, "compression.transform", span)?,
                    pass_through: lowered_bool_arg_or(values.get(8).cloned(), false, "compression.transform", span)?,
                };
                self.flush_stdout_checked(span)?;
                match crate::modules::compression::transform(request, span) {
                    Ok(()) => lowered_result_ok(LoweredValue::Unit), Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::CompressionGzipName => {
                let source = lowered_path_arg(unix_require_arg(values.first().cloned(), "compression.gzip_name", span)?, "compression.gzip_name", span)?;
                match crate::modules::compression::gzip_name(&self.host_path(&source), span) {
                    Ok(path) => lowered_result_ok(match path { Some(path) => lowered_runtime_value(Value::Path(path_value_from_pathbuf(path)?), span)?, None => LoweredValue::Null }),
                    Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::LinuxSample => {
                let root = match values.first().cloned() { None | Some(LoweredValue::Null) => None, Some(value) => Some(lowered_path_arg(value, "linux.sample", span)?) };
                let root = root.as_ref().map(|path| self.host_path(path));
                match linux_module::sampling::sample(root.as_deref(), span) {
                    Ok(value) => lowered_result_ok(lowered_runtime_value(value, span)?), Err(error) => lowered_result_err_value(error),
                }
            }
            callable; RuntimeOp::UnixPollFd if (2..=3).contains(&values.len()) => {
                let fd = lowered_int_arg(values.first().cloned(), "unix.poll_fd", span)?;
                let events = lowered_str_list_arg(values.get(1).cloned(), "unix.poll_fd", span)?;
                let timeout = lowered_int_arg_or(values.get(2).cloned(), 0, "unix.poll_fd", span)?;
                lowered_runtime_result(unix_module::poll_fd(fd, &events, timeout, span), span)?
            }
            // Lowering builds both from the checked call; these are what a
            // call that reaches the module operation itself evaluates to.
            callable; RuntimeOp::SetEmpty if values.is_empty() => {
                LoweredValue::Set(Arc::new(std::collections::BTreeSet::new()))
            }
            callable; RuntimeOp::SetFrom if values.len() == 1 => match values.pop() {
                Some(LoweredValue::List(items)) => {
                    super::set::lowered_set_from_items(&items, span)?
                }
                Some(LoweredValue::SharedList(items)) => {
                    super::set::lowered_set_from_items(&items, span)?
                }
                _ => {
                    return Err(
                        RuntimeError::new("type-error", "set.from expected a list").with_span(span)
                    );
                }
            },
            callable; RuntimeOp::ShlexQuote if values.len() == 1 => {
                let text = lowered_str_arg_owned(values.pop(), "", "shlex.quote", span)?;
                LoweredValue::Str(shlex::quote(&text).into())
            }
            callable; RuntimeOp::ShlexJoin if values.len() == 1 => {
                let argv = lowered_str_list_arg(values.pop(), "shlex.join", span)?;
                LoweredValue::Str(shlex::join(&argv).into())
            }
            callable; RuntimeOp::SystemHostname if values.is_empty() => lowered_runtime_result(
                system::hostname(span).map(|hostname| Value::Str(hostname.into())),
                span,
            )?,
            callable; RuntimeOp::SystemUname if values.is_empty() => {
                lowered_runtime_result(system::uname(span), span)?
            }
            callable; RuntimeOp::SystemMemory if values.is_empty() => {
                lowered_runtime_result(system::memory(span), span)?
            }
            callable; RuntimeOp::SystemExecutionUnits if values.is_empty() => {
                lowered_runtime_result(system::execution_units(span), span)?
            }
            callable; RuntimeOp::SystemOsRelease if values.is_empty() => {
                lowered_runtime_result(system::os_release(span), span)?
            }
            #[cfg(feature = "native-tests")]
            callable; RuntimeOp::TestOk if values.len() == 1 || values.len() == 2 => {
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
            callable; RuntimeOp::TestEq if values.len() == 2 || values.len() == 3 => {
                let equal = crate::runtime::eval::lowered_values_semantically_equal(
                    &values[0], &values[1],
                );
                let left = values[0].clone().into_value();
                let right = values[1].clone().into_value();
                let message = lowered_str_arg_owned(values.get(2).cloned(), "", "test.eq", span)?;
                if equal {
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
            callable; RuntimeOp::TestNe if values.len() == 2 || values.len() == 3 => {
                let equal = crate::runtime::eval::lowered_values_semantically_equal(
                    &values[0], &values[1],
                );
                let left = values[0].clone().into_value();
                let message = lowered_str_arg_owned(values.get(2).cloned(), "", "test.ne", span)?;
                if !equal {
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
            callable; RuntimeOp::TestErrorKind if values.len() == 2 || values.len() == 3 => return self.eval_lowered_test_error_kind_values(values, span),
            #[cfg(feature = "native-tests")]
            callable; RuntimeOp::TestFail if values.is_empty() || values.len() == 1 => {
                let message =
                    lowered_str_arg_owned(values.pop(), "test failed", "test.fail", span)?;
                lowered_runtime_value(test_failure(message), span)?
            }
            #[cfg(feature = "native-tests")]
            callable; RuntimeOp::TestSkip if values.is_empty() || values.len() == 1 => {
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
            callable; RuntimeOp::TestTimeout if values.len() == 2 => {
                let limit = lowered_duration_arg(values.pop(), "test.timeout", span)?;
                lowered_record_arg(values.pop(), "test.timeout", span)?;
                if let Some(cancellation) = &self.test_cancellation {
                    cancellation.request_timeout(Duration::from_millis(limit.millis));
                }
                LoweredValue::Unit
            }
            #[cfg(feature = "native-tests")]
            callable; RuntimeOp::TestTempPath if values.len() == 1 || values.len() == 2 => {
                let ctx = lowered_record_arg(values.first().cloned(), "test.temp_path", span)?;
                let name =
                    lowered_str_arg_owned(values.get(1).cloned(), "", "test.temp_path", span)?;
                LoweredValue::Path(test_temp_path(self, &ctx, &name, span)?)
            }
            #[cfg(feature = "native-tests")]
            callable; RuntimeOp::TestTempDir if values.len() == 1 || values.len() == 2 => {
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
            callable; RuntimeOp::TestTempFile if (1..=3).contains(&values.len()) => {
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
            callable; RuntimeOp::TestMock if (4..=5).contains(&values.len()) => return self.eval_lowered_test_mock_values(op, values, span),
            #[cfg(feature = "native-tests")]
            callable; RuntimeOp::TestLinuxFake | RuntimeOp::TestUnixFake
                if values.len() == 1 || values.len() == 2 =>
            return self.eval_lowered_test_linux_fake_values(op, values, span),
            #[cfg(feature = "native-tests")]
            callable; RuntimeOp::TestCalls if values.len() == 1 || values.len() == 2 => {
                let _ctx = lowered_record_arg(values.first().cloned(), "test.calls", span)?;
                let op = lowered_str_arg_owned(values.get(1).cloned(), "", "test.calls", span)?;
                let mut calls = Vec::new();
                for call in self
                    .test_calls
                    .iter()
                    .filter(|call| op.is_empty() || call.op == op)
                {
                    let args = lowered_runtime_value(Value::Record(call.args.clone()), span)?;
                    calls.push(LoweredValue::Record(Arc::new(BTreeMap::from([
                        (Arc::from("op"), LoweredValue::Str(call.op.as_str().into())),
                        (Arc::from("args"), args),
                    ]))));
                }
                LoweredValue::List(calls)
            }
            #[cfg(feature = "native-tests")]
            callable; RuntimeOp::TestRunScript if (2..=6).contains(&values.len()) => return self.eval_lowered_test_run_script_values(values, span),
            #[cfg(feature = "native-tests")]
            callable; RuntimeOp::TestRunXsh if (2..=7).contains(&values.len()) => return self.eval_lowered_test_run_xsh_values(values, span),
            #[cfg(feature = "native-tests")]
            callable; RuntimeOp::TestRunXshtTrace if (2..=7).contains(&values.len()) => return self.eval_lowered_test_run_xsht_trace_values(values, span),
            #[cfg(feature = "native-tests")]
            callable; RuntimeOp::TestExpect if (3..=9).contains(&values.len()) => return self.eval_lowered_test_expect_values(values, span),
            callable; RuntimeOp::TuiLeftPad if values.len() == 2 => {
                let width = lowered_int_arg(values.pop(), "tui.left_pad", span)?;
                let text = lowered_str_arg_owned(values.pop(), "", "tui.left_pad", span)?;
                LoweredValue::Str(tui::left_pad(&text, width).into())
            }
            callable; RuntimeOp::TuiRightPad if values.len() == 2 => {
                let width = lowered_int_arg(values.pop(), "tui.right_pad", span)?;
                let text = lowered_str_arg_owned(values.pop(), "", "tui.right_pad", span)?;
                LoweredValue::Str(tui::right_pad(&text, width).into())
            }
            callable; RuntimeOp::TuiCellWidth if values.len() == 1 => {
                let text = lowered_str_arg_owned(values.pop(), "", "tui.cell_width", span)?;
                LoweredValue::Int(tui::cell_width(&text))
            }
            callable; RuntimeOp::TuiCellClip if values.len() == 2 => {
                let width = lowered_int_arg(values.pop(), "tui.cell_clip", span)?;
                let text = lowered_str_arg_owned(values.pop(), "", "tui.cell_clip", span)?;
                LoweredValue::Str(tui::cell_clip(&text, width).into())
            }
            callable; RuntimeOp::TuiCellSlice if values.len() == 3 => {
                let width = lowered_int_arg(values.pop(), "tui.cell_slice", span)?;
                let start = lowered_int_arg(values.pop(), "tui.cell_slice", span)?;
                let text = lowered_str_arg_owned(values.pop(), "", "tui.cell_slice", span)?;
                LoweredValue::Str(tui::cell_slice(&text, start, width).into())
            }
            callable; RuntimeOp::TuiCellLeftPad if values.len() == 2 => {
                let width = lowered_int_arg(values.pop(), "tui.cell_left_pad", span)?;
                let text = lowered_str_arg_owned(values.pop(), "", "tui.cell_left_pad", span)?;
                LoweredValue::Str(tui::cell_left_pad(&text, width).into())
            }
            callable; RuntimeOp::TuiCellRightPad if values.len() == 2 => {
                let width = lowered_int_arg(values.pop(), "tui.cell_right_pad", span)?;
                let text = lowered_str_arg_owned(values.pop(), "", "tui.cell_right_pad", span)?;
                LoweredValue::Str(tui::cell_right_pad(&text, width).into())
            }
            callable; RuntimeOp::TuiSafeSingleLine if values.len() == 1 => {
                let text = lowered_str_arg_owned(values.pop(), "", "tui.safe_single_line", span)?;
                LoweredValue::Str(tui::safe_single_line(&text).into())
            }
            callable; RuntimeOp::TuiReadSecret if values.len() == 1 => {
                let prompt = lowered_str_arg_owned(values.pop(), "", "tui.read_secret", span)?;
                match tui::read_secret(&prompt) {
                    Ok(secret) => lowered_result_ok(LoweredValue::Str(secret.into())),
                    Err(error) => lowered_result_err_value(
                        RuntimeError::host("tui.read_secret", &error).with_span(span),
                    ),
                }
            }
        }
        post_unix_rows {
            callable; RuntimeOp::UserCurrent if values.is_empty() => {
                lowered_runtime_result(user_module::current(span), span)?
            }
            callable; RuntimeOp::UserGroups if (1..=2).contains(&values.len()) => {
                let name = lowered_str_arg_owned(values.get(0).cloned(), "", "user.groups", span)?;
                let gid = lowered_optional_int_arg(values.get(1), "user.groups", span)?;
                lowered_runtime_result(user_module::groups(&name, gid, span), span)?
            }
            callable; RuntimeOp::UserLookup if values.len() == 1 => {
                let name = lowered_str_arg_owned(values.pop(), "", "user.lookup", span)?;
                lowered_runtime_result(user_module::lookup(&name, span), span)?
            }
            callable; RuntimeOp::UserByUid if values.len() == 1 => {
                let uid = lowered_int_arg(values.pop(), "user.by_uid", span)?;
                lowered_runtime_result(
                    user_module::uid_from_i64(uid, span)
                        .and_then(|uid| user_module::by_uid(uid, span)),
                    span,
                )?
            }
            callable; RuntimeOp::UserAdd if (1..=6).contains(&values.len()) => return self.eval_lowered_user_add_values(values, span),
            callable; RuntimeOp::UserRemove if values.len() == 1 || values.len() == 2 => {
                let remove_home =
                    lowered_bool_arg_or(values.get(1).cloned(), false, "user.remove", span)?;
                let name = lowered_str_arg_owned(values.first().cloned(), "", "user.remove", span)?;
                lowered_runtime_result(user_module::remove(&name, remove_home, span), span)?
            }
            callable; RuntimeOp::ErrorFailure if values.len() == 1 => {
                let message = lowered_str_arg_owned(values.pop(), "", "error.failure", span)?;
                lowered_runtime_value(error_constructor("validation", message), span)?
            }
            callable; RuntimeOp::UtilsCache if values.len() == 1 || values.len() == 2 => return self.eval_lowered_utils_cache_values(values, span),
            callable; RuntimeOp::LinuxWriteDevice if values.len() == 2 => return self.eval_lowered_linux_write_device_values(values, span),
            callable; RuntimeOp::LinuxReadDevice if values.len() == 3 => return self.eval_lowered_linux_read_device_values(values, span),
        }
        tail_rows {
            fallback; _ => {
                return Err(RuntimeError::new(
                    "unsupported-call",
                    "unsupported lowered module call",
                )
                .with_span(span));
            }        }
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

    /// `omitted` lists the positions whose argument was not supplied even
    /// though a later one was; each takes the parameter's default, as a
    /// position past the end of `args` does.
    fn try_bind_lowered_runtime_args(
        &mut self,
        lowered: &FunctionHeader,
        args: &[Value],
        omitted: &[usize],
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
                match args.get(index).filter(|_| !omitted.contains(&index)) {
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
                match args.get(index).filter(|_| !omitted.contains(&index)) {
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
        if values.iter().enumerate().any(|(index, value)| {
            lowered_param_check(lowered, index)
                .is_some_and(|check| check.ty.has_unsigned_constraint())
                && !lowered_value_matches_param(lowered, index, lowered.param_kinds[index], value)
        }) {
            return None;
        }
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
        for (index, value) in values.iter().enumerate() {
            if lowered_param_check(lowered, index)
                .is_some_and(|check| check.ty.has_unsigned_constraint())
                && !lowered_value_matches_param(lowered, index, lowered.param_kinds[index], value)
            {
                return Err(RuntimeError::new(
                    "type-error",
                    format!(
                        "lowered call expected {}, found {}",
                        lowered_param_type_name(lowered, index, lowered.param_kinds[index]),
                        value.type_name()
                    ),
                )
                .with_span(span));
            }
        }
        Ok(self.lowered_call_slots(lowered, values))
    }

    fn bind_lowered_values_owned(
        &mut self,
        lowered: &FunctionHeader,
        mut args: Vec<LoweredValue>,
        span: Span,
    ) -> Result<Vec<LoweredValue>, RuntimeError> {
        if lowered_rest_index(lowered).is_none()
            && args.len() == lowered.params.len()
            && lowered.slot_count <= args.capacity()
            && lowered
                .param_kinds
                .iter()
                .copied()
                .enumerate()
                .all(|(index, kind)| {
                    lowered_value_matches_param(lowered, index, kind, &args[index])
                })
        {
            // A fully bound call owns its evaluated arguments. Use that vector
            // as the frame slots so the binding step need not copy the values.
            args.resize(lowered.slot_count, LoweredValue::Int(0));
            return Ok(args);
        }
        self.bind_lowered_values(lowered, &args, span)
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
        for (slot, value) in self.lowered_capture_values(lowered, call_span)? {
            slots[slot] = value;
        }
        Ok(())
    }

    fn lowered_capture_values(
        &self,
        lowered: &FunctionHeader,
        call_span: Span,
    ) -> Result<smallvec::SmallVec<[(usize, LoweredValue); 4]>, RuntimeError> {
        let mut values = smallvec::SmallVec::new();
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
            values.push((capture.slot, value));
        }
        Ok(values)
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

    fn sleep_lowered_retry_delay(
        &mut self,
        delay: &DurationValue,
        span: Span,
    ) -> Result<(), RuntimeError> {
        let deadline = Instant::now() + Duration::from_millis(delay.millis);
        while Instant::now() < deadline {
            self.service_pending_signal(span)?;
            if self.shutting_down() {
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
        selected: Option<bool>,
        stop_reason: Option<crate::trace::RetryStopReason>,
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
                selected,
                stop_reason,
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
        let host_path = self.host_path(&path);
        match std::fs::File::open(&host_path) {
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
            Err(error) => Value::err(Value::Error(Box::new(
                RuntimeError::new("fs-read", format!("{}: {error}", host_path.display()))
                    .with_host_facet(&error)
                    .with_span(span),
            ))),
        }
    }

    /// Wrap a per-item stream-stage error: emit a `stream.item.error` trace
    /// leaf and reword the message as `stream stage `<stage>` item <i> failed:
    /// …`, preserving the original kind/span/abort (mirrors the deleted
    /// recursive evaluator). Abort errors pass through unchanged.
    fn stream_item_runtime_error(
        &mut self,
        stage: &'static str,
        item_index: usize,
        mut error: RuntimeError,
    ) -> RuntimeError {
        if error.abort.is_some() {
            return error;
        }
        if let Some(mut attribution) = error.stream_item_attribution.take() {
            if self.trace_enabled && !attribution.trace_emitted {
                self.trace_leaf(
                    TraceKind::StreamItemError,
                    error.span,
                    Some(attribution.stage),
                    TracePayload::StreamItem {
                        stage: attribution.stage.to_string(),
                        item_index: attribution.item_index,
                        error: Some(attribution.trace_error.clone()),
                    },
                );
                attribution.trace_emitted = true;
            }
            error.stream_item_attribution = Some(attribution);
            return error;
        }
        let trace_error = TraceError::from_runtime_error(&error);
        let mut attribution = StreamItemErrorAttribution {
            stage,
            item_index,
            trace_error: trace_error.clone(),
            trace_emitted: self.trace_enabled,
        };
        if self.trace_enabled {
            self.trace_leaf(
                TraceKind::StreamItemError,
                error.span,
                Some(stage),
                TracePayload::StreamItem {
                    stage: stage.to_string(),
                    item_index,
                    error: Some(trace_error),
                },
            );
            attribution.trace_emitted = true;
        }
        error.message = format!(
            "stream stage `{stage}` item {item_index} failed: {}",
            error.message
        );
        error.stream_item_attribution = Some(attribution);
        error
    }

    /// Wrap a stage error that is leaving a nested evaluator operation. The
    /// marker lives on this evaluator until explicit unwinding begins, where it
    /// prevents an outer pending map callback from attributing the same error.
    fn stream_item_runtime_error_before_unwind(
        &mut self,
        stage: &'static str,
        item_index: usize,
        error: RuntimeError,
    ) -> RuntimeError {
        if self.pending_stream_item_error_unwind {
            return error;
        }
        let error = self.stream_item_runtime_error(stage, item_index, error);
        self.pending_stream_item_error_unwind = error.abort.is_none();
        error
    }

    pub(super) fn take_pending_stream_item_error_unwind(&mut self) -> bool {
        std::mem::take(&mut self.pending_stream_item_error_unwind)
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
                    error: Some(TraceError::from_run_error(error)),
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
                error: end.error.as_ref().map(TraceError::from_run_error),
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
                error: end.error.as_ref().map(TraceError::from_run_error),
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
    /// Make this program's prepared standard-library implementations callable
    /// from a dynamically loaded module.
    ///
    /// The loading program prepared every applicable embedded module before
    /// execution, so a loaded module links its standard calls to these
    /// already-verified functions instead of reparsing embedded source.
    fn expose_prepared_stdlib_implementations(&mut self) {
        let Some(program) = self.indexed_program.clone() else {
            return;
        };
        let mut additions = Vec::new();
        program.symbol_owner().with_current(|| {
            for (_owner, _entry, script) in api_spec().script_impls() {
                let namespace = Name::intern(crate::stdlib::namespace_text(script.module));
                let function = Name::intern(script.function);
                let qualified = QualifiedName::new(namespace, function);
                let key = LoweredFunctionKey::Qualified(qualified);
                for kind in [LoweredFunctionKind::Pure, LoweredFunctionKind::Proc] {
                    if program.contains_function(key, kind) {
                        additions.push((
                            qualified,
                            DynamicFunction {
                                program: Arc::clone(&program),
                                function: key,
                                kind,
                            },
                        ));
                        break;
                    }
                }
            }
        });
        if additions.is_empty() {
            return;
        }
        let table = Arc::make_mut(&mut self.indexed_dynamic_functions);
        for (qualified, function) in additions {
            table.entry(qualified).or_insert(function);
        }
    }

    pub(super) fn load_dynamic_module(
        &mut self,
        path: PathValue,
        span: Span,
    ) -> Result<RecordMap, RuntimeError> {
        let module_path = self.host_path(&path);
        let key = crate::loader::module_key(&module_path);
        let display_path = module_path.to_string_lossy().into_owned();
        let bytes = std::fs::read(&module_path).map_err(|error| {
            RuntimeError::new("module-load", format!("failed to read module: {error}"))
                .with_span(span)
        })?;
        let current = lookup_prepared_dynamic_module(&key, &self.module_roots, &bytes);
        // Loading an unchanged module again returns the same exports without
        // rerunning its initializers; a module whose files changed reloads.
        if let (Some(cached), Some(current)) = (self.module_value_cache.get(&key), &current)
            && Arc::ptr_eq(&cached.prepared, current)
        {
            return Ok(cached.record.clone());
        }
        self.expose_prepared_stdlib_implementations();
        if self.active_modules.iter().any(|active| active == &key) {
            return Err(RuntimeError::new("module-cycle", "cyclic module import").with_span(span));
        }

        let prepared = match current {
            Some(prepared) => prepared,
            None => {
                let prepared = Arc::new(prepare_dynamic_module(
                    &key,
                    &display_path,
                    bytes,
                    &self.module_roots,
                    span,
                )?);
                store_prepared_dynamic_module(&key, &prepared);
                prepared
            }
        };
        let PreparedDynamicModule {
            arena,
            declarations,
            module_program,
            exported_functions,
            exported_let_names,
            harvest,
            ..
        } = &*prepared;
        let exported_functions = exported_functions.clone();
        let exported_let_names = exported_let_names.clone();
        let dynamic_namespace = Name::intern(format!("dynamic:{key}"));
        for (name, pure) in &exported_functions {
            let qualified = QualifiedName::new(dynamic_namespace, *name);
            Arc::make_mut(&mut self.indexed_dynamic_functions).insert(
                qualified,
                DynamicFunction {
                    program: Arc::clone(module_program),
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
        let child_exports = if let Some(harvest) = harvest {
            self.active_modules.push(key.clone());
            let mut child = Evaluator::new_lowered_worker(&harvest.shared);
            child.cwd = self.cwd.clone();
            child.env = self.env.clone();
            let child_output =
                child.run_prepared_module_top_level(&harvest.plan, &exported_let_names);
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
                let alias = arena.statement_ids().find_map(|statement| {
                    let crate::syntax::arena::ArenaStmtKind::Export(inner) = arena.arena.stmt(statement).kind else { return None; };
                    let crate::syntax::arena::ArenaStmtKind::Let { target, ty: None, initializer: crate::syntax::arena::ArenaExprOrRun::Expr(expression) } = arena.arena.stmt(inner).kind else { return None; };
                    if !matches!(arena.arena.binding_target(target).kind, crate::syntax::arena::ArenaBindingTargetKind::Name(binding) if binding == name) { return None; }
                    declarations.static_callable_aliases.get(&arena.arena.expr(expression).span)
                });
                let value = if let Some(alias) = alias {
                    let original = match value {
                        Value::Pure(function) | Value::Proc(function) => *function,
                        _ => {
                            return Err(RuntimeError::new(
                                "module-load",
                                "checked callable alias did not produce a callable",
                            )
                            .with_span(span));
                        }
                    };
                    let target = original
                        .as_name()
                        .map(LoweredFunctionKey::Name)
                        .or_else(|| original.as_qualified().map(LoweredFunctionKey::Qualified))
                        .expect("callable identity is interned");
                    let qualified = QualifiedName::new(dynamic_namespace, name);
                    Arc::make_mut(&mut self.indexed_dynamic_functions).insert(
                        qualified,
                        DynamicFunction {
                            program: Arc::clone(module_program),
                            function: target,
                            kind: if alias.pure {
                                LoweredFunctionKind::Pure
                            } else {
                                LoweredFunctionKind::Proc
                            },
                        },
                    );
                    let function = crate::runtime::value::FunctionName::qualified(qualified);
                    Arc::make_mut(&mut self.module_export_signatures).insert(
                        function,
                        super::ModuleExportSignature {
                            pure: alias.pure,
                            sig: alias.signature.clone(),
                        },
                    );
                    if alias.pure {
                        Value::Pure(function)
                    } else {
                        Value::Proc(function)
                    }
                } else {
                    value.clone()
                };
                record_fields.push((name, value));
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

        Arc::make_mut(&mut self.module_value_cache).insert(
            key,
            CachedModule {
                prepared: Arc::clone(&prepared),
                record: record.clone(),
            },
        );
        Ok(record)
    }

    /// Enumerate the module file's top-level `export let` binding names.
    fn module_namespace_export_let_names(arena: &ArenaProgram) -> Vec<Name> {
        let mut names = Vec::new();
        for stmt in arena.statement_ids() {
            let crate::syntax::arena::ArenaStmtKind::Export(inner) = arena.arena.stmt(stmt).kind
            else {
                continue;
            };
            if let crate::syntax::arena::ArenaStmtKind::Let { target, .. }
            | crate::syntax::arena::ArenaStmtKind::Const { target, .. } =
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
    fn module_namespace_export_functions(arena: &ArenaProgram) -> Vec<(Name, bool)> {
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
    fn run_prepared_module_top_level(
        &mut self,
        plan: &super::CompactIndexedRunPlan,
        exported_let_names: &[Name],
    ) -> Result<(RecordMap, Vec<(Name, Value)>), RuntimeError> {
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
            // Every statement succeeded, so the module's top level is not
            // leaving with an error and its `errdefer` actions do not run.
            let on_error = self
                .indexed_program
                .as_ref()
                .expect("indexed module program remains installed")
                .driver_step_defers_on_error(index)
                .map_err(|error| RuntimeError::new("module-load", error.message).with_span(span))?;
            if on_error {
                continue;
            }
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

    /// Runs a byte pipeline whose head is `run.text`, `run.bytes`, or
    /// `run.capture`, capturing the last segment's stdout. Pairs with the
    /// caller's `trace_lowered_pipeline_enter`.
    fn eval_capture_pipeline(
        &mut self,
        kind: RunKind,
        invocations: &[ProcessInvocation],
        span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let execution = execute_run_with_policy(kind, invocations, span, false, self);
        if let Some(status) = execution.end.status.clone() {
            self.last_status = Some(status);
        }
        self.trace_lowered_pipeline_end(span, &execution.end);
        if self.signal_state.shutdown_complete && self.signal_state.shutdown_status.is_some() {
            return Ok(ControlFlow::Continue(LoweredValue::ResultOk(Box::new(
                LoweredValue::Status(Box::new(
                    execution
                        .end
                        .status
                        .unwrap_or_else(|| ProcessStatus::signaled(libc::SIGTERM)),
                )),
            ))));
        }
        let value = execution.value?;
        let value = lowered_value_from_runtime_any(&value).ok_or_else(|| {
            RuntimeError::new(
                "type-error",
                format!(
                    "lowered pipeline produced unsupported {}",
                    value.type_name()
                ),
            )
            .with_span(span)
        })?;
        Ok(ControlFlow::Continue(value))
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
        if options.same_group {
            managed_options.group = ProcessGroupConfig::Inherit;
        }
        self.trace_spawn_start(span, &invocation, options.detach || options.new_session);
        self.flush_shared_stdio();
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
        if let LoweredValue::Path(root) = &receiver
            && (name == "glob" || name == "rglob")
            && values.len() == 1
        {
            let Some(pattern) = lowered_str_value(&values[0]) else {
                return Err(RuntimeError::new(
                    "type-error",
                    format!("{name} expected Str, found {}", values[0].type_name()),
                )
                .with_span(*span));
            };
            let found = super::expand_glob_below(
                &self.host_path(root),
                root,
                pattern,
                name == "rglob",
                *span,
            )
            .and_then(|matches| {
                matches
                    .into_iter()
                    .map(|bytes| PathValue::new(bytes).map(LoweredValue::Path))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|error| error.with_span(*span))
            });
            let value = match found {
                Ok(paths) => lowered_result_ok(LoweredValue::List(paths)),
                Err(error) => lowered_result_err_value(error),
            };
            return Ok(ControlFlow::Continue(value));
        }
        if let LoweredValue::Path(path) = &receiver
            && name == "read_lines"
            && values.is_empty()
        {
            // `str::lines` is the split behind `Str.lines`, so this is
            // `read_text()?.lines()` in one call.
            let value = match read_host_path_text(&self.host_path(path), *span) {
                Ok(text) => lowered_result_ok(LoweredValue::List(
                    text.lines()
                        .map(|line| LoweredValue::Str(line.into()))
                        .collect(),
                )),
                Err(error) => lowered_result_err_value(error),
            };
            return Ok(ControlFlow::Continue(value));
        }
        if let LoweredValue::Path(path) = &receiver
            && name == "write_lines"
            && values.len() == 1
        {
            let lines = lowered_str_list_arg(values.pop(), "Path.write_lines", *span)?;
            let mut data = Vec::with_capacity(lines.iter().map(|line| line.len() + 1).sum());
            for line in &lines {
                data.extend_from_slice(line.as_bytes());
                data.push(b'\n');
            }
            // The same write as `Path.write`, so creation, replacement, mode,
            // and failures cannot drift from it.
            let value =
                lowered_unit_result(fs_module::write_path(self.host_path(path), &data, *span));
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
            if !Arc::ptr_eq(&handle.owner, &self.resource_owner) {
                return Ok(ControlFlow::Continue(lowered_result_err_value(
                    run_error_to_runtime(RunError::new("unknown", "process handle is no longer live"), *span),
                )));
            }
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
            // Cancelling is idempotent: a handle whose child was already
            // waited for, cancelled, or released by its owning scope has
            // nothing left to stop, which is what the caller asked for. A
            // cleanup action can therefore cancel a handle without knowing
            // whether the scope got to it first.
            let Some(live) = self.take_process_handle(&handle.id) else {
                self.trace_spawn_cancel(*span, handle.id, None, &signal.name, kill_after, None);
                return Ok(ControlFlow::Continue(lowered_result_ok(LoweredValue::Unit)));
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
        if let LoweredValue::NetJob(handle) = &receiver
            && values.is_empty()
        {
            let value = match name {
                "wait" => match self.wait_net_job((**handle).clone(), *span) {
                    Ok(response) => lowered_runtime_value(Value::ok(response), *span)?,
                    Err(error) => lowered_result_err_value(error),
                },
                "cancel" => match self.cancel_net_job((**handle).clone(), *span) {
                    Ok(()) => lowered_result_ok(LoweredValue::Unit),
                    Err(error) => lowered_result_err_value(error),
                },
                _ => {
                    return Ok(ControlFlow::Continue(lowered_result_err_value(
                        RuntimeError::new(
                            "unknown-method",
                            format!("NetJob has no method `{name}`"),
                        )
                        .with_span(*span),
                    )));
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
                slot.insert(LoweredValue::RecordVec(Arc::new(
                    lowered_projected_record_value(
                        &state.projection,
                        &item,
                        indices.as_ref(),
                        span,
                    )?,
                )));
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
                            let acc_value = &mut Arc::make_mut(acc)[index].1;
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
                            if let Some(acc_value) = lowered_record_vec_get_mut(
                                Arc::make_mut(acc).as_mut_slice(),
                                &name.as_str(),
                            ) {
                                *acc_value = lowered_sum_values(
                                    std::mem::replace(acc_value, LoweredValue::Unit),
                                    field_value,
                                );
                            } else {
                                lowered_record_vec_insert(Arc::make_mut(acc), *name, field_value);
                            }
                        }
                    }
                } else {
                    let prev = std::mem::replace(slot.get_mut(), LoweredValue::Unit);
                    *slot.get_mut() = lowered_reduce_combine(
                        ReduceByOp::Sum,
                        prev,
                        LoweredValue::RecordVec(Arc::new(lowered_projected_record_value(
                            &state.projection,
                            &item,
                            indices.as_ref(),
                            span,
                        )?)),
                        span,
                    )?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod record_binding_tests {
    use super::*;
    use crate::runtime::eval::LoweredCompFields;

    #[test]
    fn record_binding_selects_all_fields_before_writing_any_slot() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let span = Span::new(crate::source::SourceId::new(0), 0, 1);
            let mut nested = LoweredCompFields::new();
            nested.push((
                Name::intern("missing"),
                Box::new(LoweredCompTarget::Slot(1)),
                span,
            ));
            let mut fields = LoweredCompFields::new();
            fields.push((
                Name::intern("first"),
                Box::new(LoweredCompTarget::Slot(0)),
                span,
            ));
            fields.push((
                Name::intern("nested"),
                Box::new(LoweredCompTarget::Record { fields: nested }),
                span,
            ));
            let source = LoweredValue::Record(Arc::new(std::collections::BTreeMap::from([
                (Arc::from("first"), LoweredValue::Int(9)),
                (
                    Arc::from("nested"),
                    LoweredValue::Record(Arc::new(std::collections::BTreeMap::new())),
                ),
            ])));
            let mut slots = [LoweredValue::Int(1), LoweredValue::Int(2)];
            let error = bind_lowered_comp_target(
                &LoweredCompTarget::Record { fields },
                source,
                &mut slots,
                span,
            )
            .unwrap_err();
            assert_eq!(error.kind, "missing-field");
            assert_eq!(slots, [LoweredValue::Int(1), LoweredValue::Int(2)]);
        });
    }
}

#[cfg(test)]
mod fs_root_identity_tests {
    use super::*;

    #[test]
    fn opaque_fs_root_identity_rejects_records_and_foreign_evaluator_owners() {
        let span = Span::new(crate::source::SourceId::new(0), 0, 1);
        let owner = Arc::new(());
        let capability = LoweredValue::FsRoot(super::super::FsRootValue {
            id: 1,
            owner: owner.clone(),
        });
        assert_eq!(lowered_root_id(&capability, &owner, span).unwrap(), 1);
        assert_eq!(
            lowered_root_id(&capability.clone(), &owner, span).unwrap(),
            1
        );
        assert_eq!(
            lowered_root_id(&capability, &Arc::new(()), span)
                .unwrap_err()
                .kind,
            "fs-root"
        );
        let forged = LoweredValue::Record(Arc::new(BTreeMap::from([(
            Arc::from("id"),
            LoweredValue::Int(1),
        )])));
        assert_eq!(
            lowered_root_id(&forged, &owner, span).unwrap_err().kind,
            "type-error"
        );
    }
}

#[cfg(test)]
mod scalar_cursor_tests {
    use super::*;

    #[test]
    fn scalar_cursor_retains_one_source_and_builds_only_reached_views() {
        let source: Arc<str> = Arc::from("é".repeat(65_536));
        let mut cursor = LoweredScalarCursor::try_new(LoweredValue::Str(source.clone()))
            .ok()
            .unwrap();
        assert_eq!(Arc::strong_count(&source), 2);
        for offset in 0..3 {
            let Some(LoweredValue::StrView(view)) = cursor.next() else {
                panic!("scalar view")
            };
            assert!(Arc::ptr_eq(&source, &view.text));
            assert_eq!((view.start(), view.end()), (offset * 2, offset * 2 + 2));
            assert_eq!(view.as_str(), "é");
            assert_eq!(Arc::strong_count(&source), 3);
        }
        let LoweredScalarCursor::Str { position, end, .. } = cursor else {
            panic!("Str cursor")
        };
        assert_eq!((position, end), (6, source.len()));
        assert_eq!(Arc::strong_count(&source), 2);
    }

    #[test]
    fn scalar_cursor_preserves_string_and_byte_view_bounds() {
        let text: Arc<str> = Arc::from("aé🙂z");
        let view = lowered_str_view_value(text.clone(), 1, 7);
        let mut cursor = LoweredScalarCursor::try_new(view).ok().unwrap();
        assert_eq!(
            cursor.next().unwrap().into_value(),
            Value::Str(Arc::from("é"))
        );
        assert_eq!(
            cursor.next().unwrap().into_value(),
            Value::Str(Arc::from("🙂"))
        );
        assert!(cursor.next().is_none());
        let source: Arc<[u8]> = Arc::from([7, 0, 128, 255, 8]);
        let view = crate::runtime::eval::lowered_bytes_view_value(source.clone(), 1, 4);
        let mut cursor = LoweredScalarCursor::try_new(view).ok().unwrap();
        assert_eq!(Arc::strong_count(&source), 2);
        assert!(matches!(cursor.next(), Some(LoweredValue::Int(0))));
        assert!(matches!(cursor.next(), Some(LoweredValue::Int(128))));
        assert!(matches!(cursor.next(), Some(LoweredValue::Int(255))));
        assert!(cursor.next().is_none());
    }
}

#[cfg(test)]
mod lowered_module_support_authority_tests {
    use super::*;

    #[test]
    fn lowered_module_support_tracks_public_body_rows_and_excludes_internal_rows() {
        for op in [
            RuntimeOp::CpuCount,
            RuntimeOp::ProcessWaitTimeout,
            RuntimeOp::IoFlushStdout,
            RuntimeOp::IoFlushStderr,
            RuntimeOp::UnixKillAll,
            RuntimeOp::UnixExecEnv,
            RuntimeOp::LinuxRootDevice,
            RuntimeOp::TimeMonotonic,
            RuntimeOp::TuiCellWidth,
            RuntimeOp::TuiCellClip,
            RuntimeOp::TuiCellSlice,
            RuntimeOp::TuiCellLeftPad,
            RuntimeOp::TuiCellRightPad,
            RuntimeOp::TuiSafeSingleLine,
            RuntimeOp::TuiReadSecret,
        ] {
            assert!(
                Evaluator::lowered_module_op_supported(op),
                "expected lowered module support for {op:?}"
            );
        }

        for op in [
            RuntimeOp::FsTempDir,
            RuntimeOp::BridgeTypeName,
            RuntimeOp::EnvPathPrepend,
            RuntimeOp::EnvPathAppend,
            RuntimeOp::EnvPathPop,
            RuntimeOp::JsonEncodeLines,
        ] {
            assert!(
                !Evaluator::lowered_module_op_supported(op),
                "unexpected lowered module support for {op:?}"
            );
        }

        #[cfg(feature = "native-tests")]
        assert!(Evaluator::lowered_module_op_supported(RuntimeOp::TestOk));
    }
}

#[cfg(test)]
mod stream_item_attribution_tests {
    use super::*;

    #[test]
    fn receiving_evaluator_emits_carried_trace_without_wrapping_again() {
        let mut evaluator = Evaluator::new(Vec::new()).with_tracing();
        let mut error = RuntimeError::new(
            "division-by-zero",
            "stream stage `map` item 1 failed: division by zero",
        );
        error.stream_item_attribution = Some(StreamItemErrorAttribution {
            stage: "map",
            item_index: 1,
            trace_error: TraceError::new("division-by-zero", "division by zero"),
            trace_emitted: false,
        });

        let error = evaluator.stream_item_runtime_error("par-map", 0, error);
        assert_eq!(
            error.message,
            "stream stage `map` item 1 failed: division by zero"
        );
        let error = evaluator.stream_item_runtime_error("par-map", 0, error);
        assert_eq!(
            error.message,
            "stream stage `map` item 1 failed: division by zero"
        );

        let leaves = evaluator
            .trace_events
            .iter()
            .filter(|event| event.kind == TraceKind::StreamItemError)
            .collect::<Vec<_>>();
        assert_eq!(leaves.len(), 1);
        assert!(matches!(
            &leaves[0].payload,
            TracePayload::StreamItem {
                stage,
                item_index: 1,
                error: Some(error),
            } if stage == "map"
                && error.kind == "division-by-zero"
                && error.message == "division by zero"
        ));
    }
}
