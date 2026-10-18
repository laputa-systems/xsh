use super::{
    Arc, AssignOp, BTreeMap, BinaryOp, Binding, CommandPlan, ControlFlow, Duration, DurationValue,
    Evaluator, FileRedirectionMode, Flow, FormatSpec, FunctionHeader, FunctionName,
    LoweredFunctionKey, LoweredFunctionKind, LoweredMapCursor,
    LoweredProjectedReduceState, LoweredReduceProjection,
    LoweredReturnKind, LoweredScalarCursor, LoweredStrPredicate,
    LoweredTagValue, LoweredType, LoweredValue, Name, PathValue, ProcessEnd, ProcessInvocation,
    ProcessRedirection, ProcessStatus, RecordMap, RedirectionKind,
    RedirectionStream, ReduceByOp, RunError, RunKind, RuntimeError, RuntimeOp,
    Span, SpawnOptions, StmtFlow, StreamValue, TraceArg, TraceError, TraceKind,
    TracePayload, Traceback, TracebackFrame, TracebackFrameKind, Type, Value,
    append_lowered_list_element, append_lowered_map_literal, assign_lowered_bytes_view,
    assign_lowered_str_view, bind_indexed_comp_target, bind_lowered_comp_target, btree_map,
    bytes_contains, bytes_module,
    check_env_name, checked_int_binary, compare_lowered_sort_keys, compound_assignment_value,
    error_constructor, execute_run_with_policy, exit_status, fs_module,
    lowered_assign_value, lowered_binary_value, lowered_bool_arg_or, lowered_bool_builder_field,
    lowered_bytes_parts, lowered_bytes_value,
    empty_command_plan, lowered_command_plan_value, lowered_command_redirections,
    lowered_contains_value,
    lowered_count_key, lowered_duration_arg, lowered_env_record_arg,
    lowered_error_message, lowered_freeze_large_slot_list, lowered_index_from_end_value,
    lowered_index_value, lowered_inline_stats_field_value, lowered_inline_stats_to_record_vec,
    lowered_int_arg, lowered_map_literal_key, lowered_match_no_arm, lowered_nonnegative_count,
    lowered_path_arg, lowered_path_from_value, lowered_path_like_arg,
    lowered_path_method_value, lowered_pipeline_input,
    lowered_pipeline_item_count, lowered_pipeline_record_list, lowered_process_run_error,
    lowered_record_field_value, lowered_record_vec_append_or_replace_unsorted,
    lowered_record_vec_get, lowered_record_vec_or_stats, lowered_reduce_fields_owned,
    lowered_reduce_group_insert, lowered_reduce_key_value_owned, lowered_result_err_value,
    lowered_result_ok, lowered_slice_value, lowered_sort_key_orderable, lowered_splice_arg_items,
    lowered_stats_field_value, lowered_status_segment_record, lowered_stmt_flow_to_flow,
    lowered_path_list_arg, lowered_str_arg_owned, lowered_str_byte_at_value,
    lowered_str_byte_len_value,
    lowered_str_count_lines_value, lowered_str_key, lowered_str_list_arg, lowered_str_parts,
    lowered_str_predicate_text, lowered_str_predicate_value, lowered_str_value,
    lowered_str_view_value, lowered_table_print_value, lowered_tag_key,
    lowered_trim_is_empty_value, lowered_trim_str_predicate_value,
    lowered_type_name, lowered_value_argv_len, lowered_value_from_runtime,
    lowered_value_from_runtime_any,
    path_bytes, push_lowered_display, push_lowered_fmt_value, push_lowered_native_fmt_value,
    read_host_path_bytes, run_pipeline_inherit_with_policy,
    identified_structured_error_constructor, runtime_error_from_value, splice_to_argv,
    ValueView, value_matches_static_type, value_to_argv_bytes,
};
use crate::map_key::MapKey;
use crate::runtime::eval::BuildExprId;
use crate::runtime::eval::BindingNamespaceLease;
use crate::runtime::eval::route_witness::{
    RouteWitnessMode, recursive_route,
};
use crate::runtime::eval::indexed::IrVerifyError;
use crate::runtime::eval::indexed::full::decode::{
    ErrorExprView, FmtPartView, ListPosition, ListView, RecordEntryView,
};
use crate::runtime::eval::indexed::full::{
    FrameStrategy, FullDriverTag, FullExecution, FullFunctionView,
    FullPayload, FullProgram, FullStageTag, FullTag,
};
use crate::runtime::eval::indexed::full::schema::{ExprView, ExprViewKind};
use crate::runtime::eval::lower::{lowered_error_value_has_facet, lowered_record_field};
use crate::runtime::eval::lowered_ops::lowered_record_update_batch;
use crate::runtime::eval::{
    LoweredTopLevelSlot, LoweredTypeCheck, ModuleCallPlan, Propagation,
    process_handle,
};
use smallvec::SmallVec;

pub(in crate::runtime::eval) mod explicit_run;
mod producer;
mod serial_pipeline;
mod assignments;
mod calls;
mod context;
mod driver;
mod operands;
mod patterns;
mod pipeline;
mod process;
mod values;

pub(super) use assignments::lowered_shares_backing;
use assignments::{
    apply_indexed_assignment, checked_indexed_assignment, indexed_assignment_operand,
    resolve_assign_index, apply_indexed_path_assignment,
};
use calls::{append_call_argument, indexed_callable_identity, indexed_typed_callee_kind};
use operands::{
    indexed_error, indexed_value, decode_record_updates, decode_comparison_chain,
    decode_comparison_pair, comparison_pair_at, decode_match_expr, match_expr_arm_at,
    record_update_at, ComparisonChainPosition, MatchExprPosition, RecordUpdatePosition,
    decode_fmt,
};

use process::run_target_and_leading_argv;
use values::{
    capture_checked_error, lowered_condition_bool, lowered_fallback_value,
    lowered_err_with_cause, comparison_link_holds, finish_record_entries, lowered_comp_iterable,
    proc_call_result,
};

use xsh_registry::stream_parameters::DEFAULT_PAR_MAP_WORKERS;

/// Eager sequence materialization reports host capacity failures through XSH errors.
fn indexed_allocation_error(kind: &str, span: Span) -> RuntimeError {
    let mut error = RuntimeError::host(
        kind,
        &std::io::Error::from_raw_os_error(libc::ENOMEM),
    ).with_span(span);
    // Materialization failure belongs to the checked expression, so local
    // Result boundaries must capture it just like explicit propagation.
    error.propagated = true;
    error
}

fn indexed_range_values(start: i64, end: i64, span: Span) -> Result<Vec<LoweredValue>, RuntimeError> {
    let count = usize::try_from(start.abs_diff(end))
        .map_err(|_| indexed_allocation_error("range", span))?;
    let mut values = Vec::new();
    values.try_reserve_exact(count).map_err(|_| indexed_allocation_error("range", span))?;
    if start <= end {
        values.extend((start..end).map(LoweredValue::Int));
    } else {
        values.extend((end + 1..=start).rev().map(LoweredValue::Int));
    }
    Ok(values)
}

/// The `RunArg::mode` of an explicit `@` splice.
const RUN_ARG_SPLICE: u32 = 2;

/// A deferred action a scope registered: the expression to run, and whether
/// it is an `errdefer`.
///
/// Both are packed into the one word a frame's defer list already stored per
/// action, so registering a `defer` costs what it did.
#[derive(Clone, Copy)]
pub(super) struct RegisteredDefer(u32);

impl RegisteredDefer {
    const ON_ERROR: u32 = 1 << 31;

    fn new(value: u32, on_error: bool) -> Self {
        // An expression is an index into a program's instructions, which a
        // verified program keeps far below this bit.
        assert!(
            value & Self::ON_ERROR == 0,
            "deferred expression index overflows its word"
        );
        Self(if on_error {
            value | Self::ON_ERROR
        } else {
            value
        })
    }

    fn value(self) -> u32 {
        self.0 & !Self::ON_ERROR
    }

    fn on_error(self) -> bool {
        self.0 & Self::ON_ERROR != 0
    }
}

#[derive(Clone)]
struct RunArg {
    mode: u32,
    value: u32,
    span: Span,
}

enum AssertionFailure {
    Operands(LoweredValue, LoweredValue),
    Reached(String),
    False,
}

enum AssertionWork {
    Expr(u32),
    Left {
        op: BinaryOp,
        right: u32,
    },
    Right {
        op: BinaryOp,
        left_failure: Option<String>,
    },
}

/// One `par-map` item as a worker left it.
struct ParMapItemOutcome {
    /// Its value or runtime error.
    result: Result<LoweredValue, RuntimeError>,
    /// The control flow its callback left pending.
    flow: Option<StmtFlow>,
    /// The traceback of a `?` failure.
    traceback: Option<Traceback>,
    /// Whether the item ended after its worker acted on the stage being
    /// stopped. Such an item did not end on its own, so it never decides
    /// the stage.
    interrupted: bool,
}

fn append_indexed_record_entry(
    entry: RecordEntryView<'_>,
    fields: &mut Vec<(Name, LoweredValue)>,
    value: LoweredValue,
    span: Span,
) -> Result<(), RuntimeError> {
    if let RecordEntryView::Field(name, _) = entry {
        lowered_record_vec_append_or_replace_unsorted(fields, name, value);
        return Ok(());
    }
    match value {
        LoweredValue::Record(record) | LoweredValue::Module(record) => {
            for (key, value) in record.iter() {
                lowered_record_vec_append_or_replace_unsorted(
                    fields,
                    Name::intern(key.as_ref()),
                    value.clone(),
                );
            }
        }
        LoweredValue::RecordVec(record) => {
            for (key, value) in record.iter() {
                lowered_record_vec_append_or_replace_unsorted(fields, *key, value.clone());
            }
        }
        LoweredValue::Stats { blanks, code, comments } => {
            for (key, value) in lowered_inline_stats_to_record_vec(blanks, code, comments) {
                lowered_record_vec_append_or_replace_unsorted(fields, key, value);
            }
        }
        LoweredValue::StatsBlob(stats) => {
            for (key, value) in stats.to_record_vec() {
                lowered_record_vec_append_or_replace_unsorted(fields, key, value);
            }
        }
        value => {
            return Err(RuntimeError::new(
                "type-error",
                format!("record spread expected Record, found {}", value.type_name()),
            )
            .with_span(span));
        }
    }
    Ok(())
}

/// A formatted string or path being assembled from its parts.
struct IndexedFmt {
    text: String,
    native: Vec<u8>,
    path_span: Option<Span>,
}

impl IndexedFmt {
    fn push_text(&mut self, text: &str) {
        if self.path_span.is_some() {
            self.native.extend_from_slice(text.as_bytes())
        } else {
            self.text.push_str(text)
        }
    }

    fn push_value(
        &mut self,
        value: &LoweredValue,
        span: Span,
        spec: Option<&FormatSpec>,
    ) -> Result<(), RuntimeError> {
        if self.path_span.is_some() {
            push_lowered_native_fmt_value(&mut self.native, value, span, spec)
        } else {
            push_lowered_fmt_value(&mut self.text, value, span, spec)
        }
    }

    fn finish(self) -> Result<LoweredValue, RuntimeError> {
        match self.path_span {
            Some(span) => Ok(LoweredValue::Path(
                PathValue::new(self.native).map_err(|error| error.with_span(span))?,
            )),
            None => Ok(LoweredValue::Str(self.text.into())),
        }
    }
}

#[derive(Clone)]
enum ContextScopeRestore {
    Cwd {
        previous: std::path::PathBuf,
        span: Span,
    },
    Env(super::super::RuntimeEnv),
    /// The identity of the deadline the scope opened.
    Within {
        id: u64,
    },
}

fn indexed_eager_atom_value<'e>(
    execution: &'e FullExecution<'_>,
    tag: FullTag,
    payload: FullPayload<'e>,
    slots: &mut [LoweredValue],
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    let view = ExprView::decode(execution, tag, payload)
        .map_err(|error| indexed_error(error, span))?;
    let value = match view {
        ExprView::Null => LoweredValue::Null,
        ExprView::Unit => LoweredValue::Unit,
        ExprView::Int(value) => LoweredValue::Int(value),
        ExprView::UInt(value) => LoweredValue::Int(value),
        ExprView::Float(value) => LoweredValue::Float(value),
        ExprView::Duration(value) => LoweredValue::Duration(value),
        ExprView::Bool(value) => LoweredValue::Bool(value),
        ExprView::Str(value) => LoweredValue::Str(Arc::from(value)),
        ExprView::Bytes(value) => LoweredValue::Bytes(Arc::from(value)),
        ExprView::PreparedRegex(value) => LoweredValue::Regex(Box::new(value.clone())),
        ExprView::PreparedConstant(value) => value.0.clone(),
        ExprView::Path(value) => LoweredValue::Path(
            PathValue::new(value.to_vec())
                .map_err(|error| indexed_error(IrVerifyError { message: error.message }, span))?,
        ),
        ExprView::Param(slot) => {
            lowered_freeze_large_slot_list(&mut slots[slot]);
            slots[slot].clone()
        }
        _ => unreachable!("only eager atom tags reach this value helper"),
    };
    Ok(value)
}

pub(super) fn indexed_frame_apply_tag(tag: FullTag) -> bool {
    matches!(
        tag,
        FullTag::ExprFunctionRef
            | FullTag::ExprPathFrom
            | FullTag::ExprEmptyMap
            | FullTag::ExprRange
            | FullTag::ExprTag
            | FullTag::ExprSlice
            | FullTag::ExprStrByteLen
            | FullTag::ExprStrByteAt
            | FullTag::ExprStrPredicate
            | FullTag::ExprGlob
            | FullTag::ExprLastStatus
            | FullTag::ExprAbort
            | FullTag::ExprFail
            | FullTag::ExprError
            | FullTag::ExprProcessCommandArgv
            | FullTag::ExprFsFiles
            | FullTag::ExprFsWalk
            | FullTag::ExprFsList
            | FullTag::ExprFsTempDir
            | FullTag::ExprFsMkdir
            | FullTag::ExprFsRemove
            | FullTag::ExprPathReadText
            | FullTag::ExprPathReadBytes
            | FullTag::ExprPathExists
            | FullTag::ExprPathExecutable
            | FullTag::ExprPathDu
            | FullTag::ExprPathMetadata
            | FullTag::ExprPathReadlink
            | FullTag::ExprPathResolve
            | FullTag::ExprPathWrite
            | FullTag::ExprPathMkdir
            | FullTag::ExprPathRemove
            | FullTag::ExprArchiveTarCreate
    )
}

pub(super) fn indexed_type_check(
    check: crate::runtime::eval::indexed::full::decode::TypeCheckView<'_>,
    span: Span,
) -> Result<LoweredTypeCheck, RuntimeError> {
    Ok(LoweredTypeCheck {
        ty: check
            .ty
            .materialize()
            .map_err(|error| indexed_error(error, span))?,
        name: Arc::from(check.name),
        schema: check.schema.cloned(),
    })
}

#[derive(Clone, Copy)]
pub(super) enum FrameApplyOperand {
    Raw,
    Path(&'static str),
    PathLike(&'static str),
    Bool(&'static str),
    BuilderBool(&'static str),
    String(&'static str),
    StringList(&'static str),
    PathList(&'static str),
    Environment(&'static str),
    Duration(&'static str),
    PositiveInt(&'static str),
    AcceptedExitCodes,
    ErrorFailMessage,
    ExitStatus,
}

pub(super) enum FrameApplyValue {
    Value(LoweredValue),
    Path(PathValue),
    Bool(bool),
    String(String),
    StringList(Vec<String>),
    PathList(Vec<PathValue>),
    Environment(BTreeMap<String, Vec<u8>>),
    Duration(DurationValue),
    Int(i64),
    AcceptedExitCodes(crate::runtime::process::AcceptedExitCodes),
    ExitStatus(u8),
}

pub(super) struct FrameApplyChild {
    pub(super) id: u32,
    pub(super) operand: Option<FrameApplyOperand>,
    pub(super) tag_position: Option<ListPosition<BuildExprId>>,
    pub(super) next_tag_position: Option<ListPosition<BuildExprId>>,
    pub(super) error_field_position: Option<ListPosition<(Arc<str>, BuildExprId)>>,
    pub(super) next_error_field_position: Option<ListPosition<(Arc<str>, BuildExprId)>>,
    pub(super) span: Span,
}

type FrameApplyField = Option<(u32, Option<FrameApplyOperand>)>;

fn indexed_frame_apply_child_from_fields<const N: usize>(
    fields: [FrameApplyField; N],
    position: usize,
    span: Span,
) -> Option<FrameApplyChild> {
    fields
        .into_iter()
        .flatten()
        .nth(position)
        .map(|(id, operand)| FrameApplyChild {
            id,
            operand,
            tag_position: None,
            next_tag_position: None,
            error_field_position: None,
            next_error_field_position: None,
            span,
        })
}

pub(super) fn indexed_frame_apply_child(
    tag: FullTag,
    view: &mut ExprView<'_>,
    position: usize,
    tag_position: Option<ListPosition<BuildExprId>>,
    error_field_position: Option<ListPosition<(Arc<str>, BuildExprId)>>,
    span: Span,
) -> Result<Option<FrameApplyChild>, RuntimeError> {
    let child = match view {
        ExprView::ProcessCommandArgv(command) => indexed_frame_apply_child_from_fields(
            [
                Some((command.target.raw(), None)),
                Some((command.argv.raw(), None)),
                command.cwd.map(|id| (id.raw(), None)),
                command.env.map(|id| (id.raw(), None)),
                command.stdin.map(|id| (id.raw(), None)),
                command.stdout.map(|id| (id.raw(), None)),
                command.stderr.map(|id| (id.raw(), None)),
                command.stdout_append.map(|id| (id.raw(), None)),
                command.stderr_append.map(|id| (id.raw(), None)),
                command.timeout.map(|id| (id.raw(), None)),
                command.detach.map(|id| (id.raw(), None)),
                command.new_session.map(|id| (id.raw(), None)),
                command.ignore_hup.map(|id| (id.raw(), None)),
                command.cpu_max.map(|id| (id.raw(), None)),
                command.accept.map(|id| (id.raw(), None)),
                command.same_group.map(|id| (id.raw(), None)),
            ],
            position,
            command.span,
        ),
        ExprView::FunctionRef { .. } | ExprView::EmptyMap
        | ExprView::Glob { .. } | ExprView::LastStatus { .. } => None,
        ExprView::PathFrom { value, span } => {
            indexed_frame_apply_child_from_fields([(Some((value.raw(), None)))], position, *span)
        }
        ExprView::Range { start, end, span } => indexed_frame_apply_child_from_fields(
            [(Some((start.raw(), None))), (Some((end.raw(), None)))],
            position,
            *span,
        ),
        ExprView::Tag { fields, .. } => {
            if let Some(tag_position) = tag_position {
                fields
                    .resume(tag_position)
                    .map_err(|error| indexed_error(error, span))?;
            }
            let tag_position = ListView::position(fields);
            let Some(id) = fields
                .next()
                .transpose()
                .map_err(|error| indexed_error(error, span))?
            else {
                return Ok(None);
            };
            let next_tag_position = ListView::position(fields);
            return Ok(Some(FrameApplyChild {
                id: id.raw(),
                operand: None,
                tag_position: Some(tag_position),
                next_tag_position: Some(next_tag_position),
                error_field_position: None,
                next_error_field_position: None,
                span,
            }));
        }
        ExprView::Abort { status, span } => indexed_frame_apply_child_from_fields(
            [Some((status.raw(), Some(FrameApplyOperand::ExitStatus)))],
            position,
            *span,
        ),
        ExprView::Fail { message, span } => indexed_frame_apply_child_from_fields(
            [Some((message.raw(), Some(FrameApplyOperand::ErrorFailMessage)))],
            position,
            *span,
        ),
        ExprView::Error(ErrorExprView::Structured { fields, .. }) => {
            if let Some(error_field_position) = error_field_position {
                fields.resume(error_field_position).map_err(|error| {
                    RuntimeError::new(
                        "indexed-ir",
                        format!(
                            "structured error field: indexed IR verification failed: {}",
                            error.message
                        ),
                    )
                    .with_span(span)
                })?;
            }
            let error_field_position = ListView::position(fields);
            let Some((_, id)) = fields.next().transpose().map_err(|error| {
                RuntimeError::new(
                    "indexed-ir",
                    format!(
                        "structured error field: indexed IR verification failed: {}",
                        error.message
                    ),
                )
                .with_span(span)
            })? else {
                return Ok(None);
            };
            let next_error_field_position = ListView::position(fields);
            return Ok(Some(FrameApplyChild {
                id: id.raw(),
                operand: None,
                tag_position: None,
                next_tag_position: None,
                error_field_position: Some(error_field_position),
                next_error_field_position: Some(next_error_field_position),
                span,
            }));
        }
        ExprView::Slice { base, start, end, span } => indexed_frame_apply_child_from_fields(
            [Some((base.raw(), None)), start.map(|id| (id.raw(), None)), end.map(|id| (id.raw(), None))],
            position,
            *span,
        ),
        ExprView::StrByteLen { receiver, span } => {
            indexed_frame_apply_child_from_fields([Some((receiver.raw(), None))], position, *span)
        }
        ExprView::StrByteAt { receiver, index, span } => indexed_frame_apply_child_from_fields(
            [Some((receiver.raw(), None)), Some((index.raw(), None))],
            position,
            *span,
        ),
        ExprView::StrPredicate { receiver, needle, span, .. } => {
            indexed_frame_apply_child_from_fields(
                [Some((receiver.raw(), None)), Some((needle.raw(), None))],
                position,
                *span,
            )
        }
        ExprView::FsFiles { root, gitignore, stat, hidden, exts, span, .. }
        | ExprView::FsWalk { root, gitignore, stat, hidden, exts, span, .. } => {
            let operation = if tag == FullTag::ExprFsFiles { "fs.files" } else { "fs.walk" };
            let ext_operation = if tag == FullTag::ExprFsFiles { "fs.files exts" } else { "fs.walk exts" };
            indexed_frame_apply_child_from_fields(
                [
                    Some((root.raw(), Some(FrameApplyOperand::Path(operation)))),
                    gitignore.map(|id| (id.raw(), Some(FrameApplyOperand::Bool(operation)))),
                    stat.map(|id| (id.raw(), Some(FrameApplyOperand::Bool(operation)))),
                    exts.map(|id| (id.raw(), Some(FrameApplyOperand::StringList(ext_operation)))),
                    hidden.map(|id| (id.raw(), Some(FrameApplyOperand::Bool(operation)))),
                ],
                position,
                *span,
            )
        }
        ExprView::FsList { path, stat, ordered, span, .. } => {
            indexed_frame_apply_child_from_fields(
                [
                    Some((path.raw(), Some(FrameApplyOperand::Path("fs.children")))),
                    stat.map(|id| (id.raw(), Some(FrameApplyOperand::Bool("fs.children")))),
                    ordered.map(|id| (id.raw(), Some(FrameApplyOperand::Bool("fs.children")))),
                ],
                position,
                *span,
            )
        }
        ExprView::FsTempDir { .. } => None,
        ExprView::PathWrite { path, data, span, .. } => indexed_frame_apply_child_from_fields(
            [
                Some((path.raw(), Some(FrameApplyOperand::Path("write")))),
                Some((data.raw(), Some(FrameApplyOperand::Raw))),
            ],
            position,
            *span,
        ),
        ExprView::FsMkdir { path, parents, span }
        | ExprView::PathMkdir { path, parents, span } => {
            let operation = if tag == FullTag::ExprFsMkdir { "fs.mkdir" } else { "mkdir" };
            indexed_frame_apply_child_from_fields(
                [
                    Some((path.raw(), Some(FrameApplyOperand::Path(operation)))),
                    parents.map(|id| (id.raw(), Some(FrameApplyOperand::Bool(operation)))),
                ],
                position,
                *span,
            )
        }
        ExprView::FsRemove { path, missing_ok, span }
        | ExprView::PathRemove { path, missing_ok, span } => {
            let operation = if tag == FullTag::ExprFsRemove { "fs.remove" } else { "remove" };
            indexed_frame_apply_child_from_fields(
                [
                    Some((path.raw(), Some(FrameApplyOperand::Path(operation)))),
                    missing_ok.map(|id| (id.raw(), Some(FrameApplyOperand::Bool(operation)))),
                ],
                position,
                *span,
            )
        }
        ExprView::PathReadText { path, span }
        | ExprView::PathReadBytes { path, span }
        | ExprView::PathExists { path, span }
        | ExprView::PathExecutable { path, span }
        | ExprView::PathDu { path, span }
        | ExprView::PathMetadata { path, span }
        | ExprView::PathReadlink { path, span }
        | ExprView::PathResolve { path, span } => {
            indexed_frame_apply_child_from_fields([Some((path.raw(), None))], position, *span)
        }
        ExprView::ArchiveTarCreate { path, root, entries, compression, overwrite, span } => {
            indexed_frame_apply_child_from_fields(
                [
                    Some((path.raw(), Some(FrameApplyOperand::Path("archive.tar_create")))),
                    Some((root.raw(), Some(FrameApplyOperand::Path("archive.tar_create")))),
                    Some((entries.raw(), Some(FrameApplyOperand::PathList("archive.tar_create")))),
                    compression.map(|id| (id.raw(), Some(FrameApplyOperand::String("archive.tar_create")))),
                    overwrite.map(|id| (id.raw(), Some(FrameApplyOperand::Bool("archive.tar_create")))),
                ],
                position,
                *span,
            )
        }
        _ => unreachable!("frame apply tag has a matching typed view"),
    };
    Ok(child)
}

impl FrameApplyValue {
    fn path(self) -> PathValue {
        let Self::Path(path) = self else {
            unreachable!("frame-apply path operand was converted before apply")
        };
        path
    }

    fn bool(self) -> bool {
        let Self::Bool(value) = self else {
            unreachable!("frame-apply bool operand was converted before apply")
        };
        value
    }

    fn string(self) -> String {
        let Self::String(value) = self else {
            unreachable!("frame-apply string operand was converted before apply")
        };
        value
    }

    fn string_list(self) -> Vec<String> {
        let Self::StringList(values) = self else {
            unreachable!("frame-apply string-list operand was converted before apply")
        };
        values
    }

    fn path_list(self) -> Vec<PathValue> {
        let Self::PathList(values) = self else {
            unreachable!("frame-apply path-list operand was converted before apply")
        };
        values
    }

    fn into_module_argument(self) -> LoweredValue {
        match self {
            Self::Value(value) => value,
            Self::Path(value) => LoweredValue::Path(value),
            Self::Bool(value) => LoweredValue::Bool(value),
            _ => unreachable!("typed frame-apply operand stays with its host operation"),
        }
    }
}

fn convert_frame_apply_operand(
    value: LoweredValue,
    operand: FrameApplyOperand,
    span: Span,
) -> Result<FrameApplyValue, RuntimeError> {
    match operand {
        FrameApplyOperand::Raw => Ok(FrameApplyValue::Value(value)),
        FrameApplyOperand::Path(operation) => Ok(FrameApplyValue::Path(lowered_path_arg(value, operation, span)?)),
        FrameApplyOperand::PathLike(operation) => Ok(FrameApplyValue::Path(lowered_path_like_arg(value, operation, span)?)),
        FrameApplyOperand::Bool(operation) => Ok(FrameApplyValue::Bool(lowered_bool_arg_or(Some(value), false, operation, span)?)),
        FrameApplyOperand::BuilderBool(field) => Ok(FrameApplyValue::Bool(lowered_bool_builder_field(value, field, span)?)),
        FrameApplyOperand::String(operation) => Ok(FrameApplyValue::String(lowered_str_arg_owned(Some(value), "", operation, span)?)),
        FrameApplyOperand::StringList(operation) => Ok(FrameApplyValue::StringList(lowered_str_list_arg(Some(value), operation, span)?)),
        FrameApplyOperand::PathList(operation) => Ok(FrameApplyValue::PathList(lowered_path_list_arg(value, operation, span)?)),
        FrameApplyOperand::Environment(operation) => Ok(FrameApplyValue::Environment(lowered_env_record_arg(value, operation, span)?)),
        FrameApplyOperand::Duration(operation) => Ok(FrameApplyValue::Duration(lowered_duration_arg(Some(value), operation, span)?)),
        FrameApplyOperand::PositiveInt(operation) => {
            let value = lowered_int_arg(Some(value), operation, span)?;
            if value <= 0 {
                return Err(RuntimeError::new("cpu-max", "cpu_max must be positive").with_span(span));
            }
            Ok(FrameApplyValue::Int(value))
        }
        FrameApplyOperand::AcceptedExitCodes => Ok(FrameApplyValue::AcceptedExitCodes(
            super::lowered_accepted_exit_codes(value, span)?,
        )),
        FrameApplyOperand::ErrorFailMessage => match lowered_str_value(&value) {
            Some(message) => Ok(FrameApplyValue::String(message.to_string())),
            None => Err(RuntimeError::new(
                "type-error",
                format!("error.fail expected Str, found {}", value.type_name()),
            )
            .with_span(span)),
        },
        FrameApplyOperand::ExitStatus => match value {
            LoweredValue::Int(status) => {
                Ok(FrameApplyValue::ExitStatus(exit_status(status, span)?))
            }
            value => Err(RuntimeError::new(
                "type-error",
                format!("exit status expected Int, found {}", value.type_name()),
            )
            .with_span(span)),
        },
    }
}

pub(super) fn indexed_frame_apply_operand(
    value: LoweredValue,
    operand: Option<FrameApplyOperand>,
    span: Span,
) -> Result<FrameApplyValue, RuntimeError> {
    match operand {
        Some(operand) => convert_frame_apply_operand(value, operand, span),
        None => Ok(FrameApplyValue::Value(value)),
    }
}

fn indexed_frame_apply_prepares_operands(tag: FullTag) -> bool {
    matches!(
        tag,
        FullTag::ExprFsFiles
            | FullTag::ExprFsWalk
            | FullTag::ExprFsList
            | FullTag::ExprFsMkdir
            | FullTag::ExprFsRemove
            | FullTag::ExprPathWrite
            | FullTag::ExprPathMkdir
            | FullTag::ExprPathRemove
            | FullTag::ExprArchiveTarCreate
    )
}

pub(super) fn indexed_function_ref_value(
    function: FunctionName,
    pure: bool,
) -> LoweredValue {
    if pure {
        LoweredValue::Pure(function)
    } else {
        LoweredValue::Proc(function)
    }
}

pub(super) fn indexed_path_from_value(
    value: LoweredValue,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    Ok(LoweredValue::Path(lowered_path_from_value(value, "Path", span)?))
}

pub(super) fn indexed_empty_map_value() -> LoweredValue {
    LoweredValue::Map(Arc::new(BTreeMap::new()))
}

pub(super) fn indexed_range_bound(
    value: &LoweredValue,
    bound: &str,
    span: Span,
) -> Result<i64, RuntimeError> {
    match value {
        LoweredValue::Int(value) => Ok(*value),
        value => Err(RuntimeError::new(
            "type-error",
            format!("range {bound} expected Int, found {}", value.type_name()),
        )
        .with_span(span)),
    }
}

pub(super) fn indexed_range_value(
    start: LoweredValue,
    end: LoweredValue,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    let start = indexed_range_bound(&start, "start", span)?;
    let end = indexed_range_bound(&end, "end", span)?;
    Ok(LoweredValue::List(indexed_range_values(start, end, span)?))
}

pub(super) fn indexed_tag_value(
    type_name: Name,
    wire: Option<Arc<crate::sema::wire_enums::WireEnumMapping>>,
    name: Arc<str>,
    fields: Vec<LoweredValue>,
) -> LoweredValue {
    LoweredValue::Tag(Box::new(LoweredTagValue {
        type_name,
        wire,
        name,
        fields,
    }))
}

pub(super) fn indexed_byte_at_index(
    value: &LoweredValue,
    span: Span,
) -> Result<i64, RuntimeError> {
    match value {
        LoweredValue::Int(value) => Ok(*value),
        _ => Err(RuntimeError::new("type-error", "byte_at expected Int").with_span(span)),
    }
}

pub(super) fn indexed_str_byte_at_value(
    receiver: LoweredValue,
    index: LoweredValue,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    let index = indexed_byte_at_index(&index, span)?;
    let byte = lowered_str_byte_at_value(&receiver, index, -1, span)?;
    Ok(if byte < 0 {
        LoweredValue::Null
    } else {
        LoweredValue::Int(byte)
    })
}

fn indexed_frame_apply_module_call(
    evaluator: &mut Evaluator,
    op: RuntimeOp,
    values: Vec<Option<LoweredValue>>,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    match evaluator.eval_indexed_module_call_values(
        op,
        super::NativeArgumentValues::new(values),
        span,
        None,
    )? {
        ControlFlow::Continue(value) => Ok(value),
        ControlFlow::Break(_) => unreachable!("filesystem module calls do not break control flow"),
    }
}

fn indexed_frame_apply_host_value(
    evaluator: &mut Evaluator,
    tag: FullTag,
    view: ExprView<'_>,
    values: Vec<FrameApplyValue>,
) -> Result<LoweredValue, RuntimeError> {
    let mut values = values.into_iter();
    match view {
        ExprView::FsFiles { gitignore, stat, hidden, exts, result_wrapped, span, .. }
        | ExprView::FsWalk { gitignore, stat, hidden, exts, result_wrapped, span, .. } => {
            let root = values.next().expect("filesystem walk root operand").path();
            let gitignore = gitignore.is_none_or(|_| values.next().expect("gitignore operand").bool());
            let stat = stat.is_none_or(|_| values.next().expect("stat operand").bool());
            let exts = exts.map_or_else(Vec::new, |_| values.next().expect("extensions operand").string_list());
            let hidden = hidden.is_some_and(|_| values.next().expect("hidden operand").bool());
            assert!(values.next().is_none(), "filesystem walk operand count mismatch");
            let emit = if tag == FullTag::ExprFsFiles { fs_module::WalkEmit::Files } else { fs_module::WalkEmit::All };
            let mut stream = match fs_module::walk_filesystem(
                evaluator.host_path(&root), gitignore, stat, hidden, emit, exts, span,
            ) {
                Ok(stream) => stream,
                Err(error) if result_wrapped => return Ok(lowered_result_err_value(error)),
                Err(error) => return Err(error),
            };
            evaluator.register_returned_stream(&mut stream, span)?;
            let value = LoweredValue::Stream(Box::new(stream));
            Ok(if result_wrapped { lowered_result_ok(value) } else { value })
        }
        ExprView::FsList { op, stat, ordered, span, .. } => {
            let path = Some(values.next().expect("filesystem list path operand").into_module_argument());
            let stat = stat.map(|_| values.next().expect("filesystem list stat operand").into_module_argument());
            let ordered = ordered.map(|_| values.next().expect("filesystem list ordered operand").into_module_argument());
            assert!(values.next().is_none(), "filesystem list operand count mismatch");
            indexed_frame_apply_module_call(evaluator, op, vec![path, stat, ordered], span)
        }
        ExprView::FsMkdir { parents, span, .. } | ExprView::PathMkdir { parents, span, .. } => {
            let path = Some(values.next().expect("mkdir path operand").into_module_argument());
            let parents = parents.map(|_| values.next().expect("mkdir parents operand").into_module_argument());
            assert!(values.next().is_none(), "mkdir operand count mismatch");
            indexed_frame_apply_module_call(evaluator, RuntimeOp::FsMkdir, vec![path, parents], span)
        }
        ExprView::FsRemove { missing_ok, span, .. } | ExprView::PathRemove { missing_ok, span, .. } => {
            let path = Some(values.next().expect("remove path operand").into_module_argument());
            let missing_ok = missing_ok.map(|_| values.next().expect("remove missing_ok operand").into_module_argument());
            assert!(values.next().is_none(), "remove operand count mismatch");
            indexed_frame_apply_module_call(evaluator, RuntimeOp::FsRemove, vec![path, missing_ok], span)
        }
        ExprView::PathWrite { atomic, span, .. } => {
            let path = Some(values.next().expect("path write path operand").into_module_argument());
            let data = Some(values.next().expect("path write data operand").into_module_argument());
            assert!(values.next().is_none(), "path write operand count mismatch");
            let op = if atomic { RuntimeOp::FsWriteAtomic } else { RuntimeOp::FsWrite };
            indexed_frame_apply_module_call(evaluator, op, vec![path, data], span)
        }
        ExprView::ArchiveTarCreate { compression, overwrite, span, .. } => {
            let path = values.next().expect("archive output path operand").path();
            let root = values.next().expect("archive root operand").path();
            let entries = values.next().expect("archive entries operand").path_list();
            let compression = compression.map(|_| {
                values.next().expect("archive compression operand").string()
            });
            let overwrite = overwrite.map(|_| {
                values.next().expect("archive overwrite operand").bool()
            });
            assert!(values.next().is_none(), "archive create operand count mismatch");
            evaluator.eval_archive_tar_create_apply(span, || {
                Ok((path, root, entries, compression, overwrite))
            })
        }
        _ => unreachable!("frame-apply tag has a matching typed view"),
    }
}

pub(super) fn indexed_frame_apply_value(
    evaluator: &mut Evaluator,
    execution: &FullExecution<'_>,
    instruction: u32,
    values: Vec<FrameApplyValue>,
    call_span: Span,
) -> Result<LoweredValue, RuntimeError> {
    let (tag, payload) = indexed_value(execution.instruction_id(instruction), call_span)?;
    let view = ExprView::decode(execution, tag, payload)
        .map_err(|error| indexed_error(error, call_span))?;
    if indexed_frame_apply_prepares_operands(tag) {
        return indexed_frame_apply_host_value(evaluator, tag, view, values);
    }
    let view = match view {
        ExprView::Abort { span, .. } => {
            let mut values = values.into_iter();
            let Some(FrameApplyValue::ExitStatus(status)) = values.next() else {
                unreachable!("exit status operand was converted before apply")
            };
            assert!(values.next().is_none(), "exit has one operand");
            // A script's own exit always unwinds; only a forced harness
            // cancellation skips cleanup.
            return Err(RuntimeError::abort(status, false).with_span(span));
        }
        ExprView::Fail { .. } => {
            let mut values = values.into_iter();
            let Some(FrameApplyValue::String(message)) = values.next() else {
                unreachable!("error.fail message was converted before apply")
            };
            assert!(values.next().is_none(), "error.fail has one operand");
            return Ok(LoweredValue::ResultErr(Box::new(error_constructor(
                "validation",
                message,
            ))));
        }
        ExprView::Error(ErrorExprView::Structured {
            family,
            family_identity,
            variant,
            mut fields,
            facets,
        }) => {
            assert_eq!(fields.len(), values.len(), "structured error field count mismatch");
            let mut record = RecordMap::new();
            for value in values {
                let FrameApplyValue::Value(value) = value else {
                    unreachable!("structured error field remains a raw operand")
                };
                let (name, _) = fields
                    .next()
                    .transpose()
                    .map_err(|error| {
                        RuntimeError::new(
                            "indexed-ir",
                            format!(
                                "structured error field: indexed IR verification failed: {}",
                                error.message
                            ),
                        )
                        .with_span(call_span)
                    })?
                    .expect("structured error field remains in its encoded list");
                record.insert(Arc::from(name), value.into_value());
            }
            let mut facet_names = Vec::with_capacity(facets.len());
            for facet in facets {
                facet_names.push(
                    facet
                        .map_err(|error| {
                            RuntimeError::new(
                                "indexed-ir",
                                format!(
                                    "structured error facet: indexed IR verification failed: {}",
                                    error.message
                                ),
                            )
                            .with_span(call_span)
                        })?
                        .as_str()
                        .to_string(),
                );
            }
            let message = match record.get("message") {
                Some(Value::Str(message)) => message.to_string(),
                _ => format!("{family}.{variant}"),
            };
            return Ok(LoweredValue::Error(Box::new(identified_structured_error_constructor(
                family.to_string(),
                family_identity.to_string(),
                variant.to_string(),
                record,
                facet_names,
                message,
            ))));
        }
        view => view,
    };
    let values = values
        .into_iter()
        .map(|value| match value {
            FrameApplyValue::Value(value) => value,
            _ => unreachable!("non-filesystem frame-apply operand was not raw"),
        })
        .collect::<Vec<_>>();
    let mut values = values.into_iter();
    match view {
        ExprView::ProcessCommandArgv(command) => {
            let target = values.next().expect("process command target operand");
            let argv = values.next().expect("process command argv operand");
            let cwd = command.cwd.map(|_| values.next().expect("process command cwd operand"));
            let env = command.env.map(|_| values.next().expect("process command env operand"));
            let stdin = command.stdin.map(|_| values.next().expect("process command stdin operand"));
            let stdout = command.stdout.map(|_| values.next().expect("process command stdout operand"));
            let stderr = command.stderr.map(|_| values.next().expect("process command stderr operand"));
            let stdout_append = command.stdout_append.map(|_| values.next().expect("process command stdout_append operand"));
            let stderr_append = command.stderr_append.map(|_| values.next().expect("process command stderr_append operand"));
            let timeout = command.timeout.map(|_| values.next().expect("process command timeout operand"));
            let detach = command.detach.map(|_| values.next().expect("process command detach operand"));
            let new_session = command.new_session.map(|_| values.next().expect("process command new_session operand"));
            let ignore_hup = command.ignore_hup.map(|_| values.next().expect("process command ignore_hup operand"));
            let cpu_max = command.cpu_max.map(|_| values.next().expect("process command cpu_max operand"));
            let accept = command.accept.map(|_| values.next().expect("process command accept operand"));
            let same_group = command.same_group.map(|_| values.next().expect("process command same_group operand"));
            assert!(values.next().is_none(), "process command operand count mismatch");
            lowered_command_plan_value(
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
                accept,
                same_group,
                command.span,
            )
        }
        ExprView::FunctionRef { function, pure } => {
            assert!(values.next().is_none(), "function reference has no operands");
            let function = match function {
                crate::runtime::eval::indexed::full::decode::FunctionNameView::Name(name) => {
                    FunctionName::name(name)
                }
                crate::runtime::eval::indexed::full::decode::FunctionNameView::Qualified(name) => {
                    FunctionName::qualified(name)
                }
            };
            Ok(indexed_function_ref_value(function, pure))
        }
        ExprView::PathFrom { span, .. } => {
            let value = values.next().expect("path constructor operand");
            assert!(values.next().is_none(), "path constructor has one operand");
            indexed_path_from_value(value, span)
        }
        ExprView::EmptyMap => {
            assert!(values.next().is_none(), "empty map has no operands");
            Ok(indexed_empty_map_value())
        }
        ExprView::Range { span, .. } => {
            let start = values.next().expect("range start operand");
            let end = values.next().expect("range end operand");
            assert!(values.next().is_none(), "range has two operands");
            indexed_range_value(start, end, span)
        }
        ExprView::Tag {
            type_name, name, wire, fields,
        } => {
            assert_eq!(fields.len(), values.len(), "tag field count mismatch");
            Ok(indexed_tag_value(type_name, wire.cloned(), Arc::from(name), values.collect()))
        }
        ExprView::Slice {
            start, end, span, ..
        } => {
            let base = values.next().expect("slice base operand");
            let start = start.map(|_| values.next().expect("slice start operand"));
            let end = end.map(|_| values.next().expect("slice end operand"));
            assert!(values.next().is_none(), "slice operand count mismatch");
            lowered_slice_value(base, start, end, span)
        }
        ExprView::StrByteLen { span, .. } => {
            let receiver = values.next().expect("byte length receiver");
            assert!(values.next().is_none(), "byte length has one operand");
            Ok(LoweredValue::Int(lowered_str_byte_len_value(&receiver, span)?))
        }
        ExprView::StrByteAt { span, .. } => {
            let receiver = values.next().expect("byte_at receiver");
            let index = values.next().expect("byte_at index");
            assert!(values.next().is_none(), "byte_at has two operands");
            indexed_str_byte_at_value(receiver, index, span)
        }
        ExprView::StrPredicate {
            predicate, span, ..
        } => {
            let receiver = values.next().expect("string predicate receiver");
            let needle = values.next().expect("string predicate needle");
            assert!(values.next().is_none(), "string predicate has two operands");
            Ok(LoweredValue::Bool(lowered_str_predicate_value(
                &receiver, predicate, &needle, span,
            )?))
        }
        ExprView::Glob { pattern, span } => {
            assert!(values.next().is_none(), "glob has no operands");
            let matches = crate::runtime::eval::expand_glob_pattern(&evaluator.cwd, pattern, span)?;
            let values = matches
                .into_iter()
                .map(|bytes| {
                    PathValue::new(bytes)
                        .map(LoweredValue::Path)
                        .map_err(|error| error.with_span(span))
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(LoweredValue::List(values))
        }
        ExprView::LastStatus { span } => {
            assert!(values.next().is_none(), "last status has no operands");
            evaluator
                .last_status
                .clone()
                .map(|status| LoweredValue::Status(Box::new(status)))
                .ok_or_else(|| {
                    RuntimeError::new("last-status", "`$?` is not set").with_span(span)
                })
        }
        ExprView::FsTempDir { span } => {
            assert!(values.next().is_none(), "temporary directory has no operands");
            indexed_frame_apply_module_call(evaluator, RuntimeOp::FsTempDir, Vec::new(), span)
        }
        ExprView::PathReadText { span, .. } => {
            let path = match values.next().expect("path read operand") {
                LoweredValue::Path(path) => path,
                _ => return Err(RuntimeError::new("type-error", "read_text expected Path").with_span(span)),
            };
            assert!(values.next().is_none(), "path read operand count mismatch");
            indexed_frame_apply_module_call(
                evaluator,
                RuntimeOp::FsReadText,
                vec![Some(LoweredValue::Path(path))],
                span,
            )
        }
        ExprView::PathReadBytes { span, .. } => {
            let path = match values.next().expect("path read operand") {
                LoweredValue::Path(path) => path,
                _ => {
                    return Err(RuntimeError::new("type-error", "read_bytes expected Path").with_span(span));
                }
            };
            assert!(values.next().is_none(), "path read operand count mismatch");
            Ok(match read_host_path_bytes(&evaluator.host_path(&path), span) {
                Ok(bytes) => lowered_result_ok(LoweredValue::Bytes(bytes)),
                Err(error) => lowered_result_err_value(error),
            })
        }
        ExprView::PathExists { span, .. }
        | ExprView::PathExecutable { span, .. }
        | ExprView::PathDu { span, .. }
        | ExprView::PathMetadata { span, .. }
        | ExprView::PathReadlink { span, .. }
        | ExprView::PathResolve { span, .. } => {
            let path = values.next().expect("path query operand");
            assert!(values.next().is_none(), "path query operand count mismatch");
            let operation = match tag {
                FullTag::ExprPathExists => "exists",
                FullTag::ExprPathExecutable => "executable",
                FullTag::ExprPathDu => "du",
                FullTag::ExprPathMetadata => "metadata",
                FullTag::ExprPathReadlink => "readlink",
                FullTag::ExprPathResolve => "resolve",
                _ => unreachable!(),
            };
            let path = lowered_path_arg(path, operation, span)?;
            if matches!(tag, FullTag::ExprPathExists | FullTag::ExprPathExecutable | FullTag::ExprPathMetadata) {
                let op = match tag {
                    FullTag::ExprPathExists => RuntimeOp::FsExists,
                    FullTag::ExprPathExecutable => RuntimeOp::FsExecutable,
                    FullTag::ExprPathMetadata => RuntimeOp::FsMetadata,
                    _ => unreachable!(),
                };
                return indexed_frame_apply_module_call(
                    evaluator,
                    op,
                    vec![Some(LoweredValue::Path(path))],
                    span,
                );
            }
            let host_path = evaluator.host_path(&path);
            Ok(match tag {
                FullTag::ExprPathDu => match fs_module::disk_usage(host_path, span) {
                    Ok(value) => lowered_result_ok(LoweredValue::Int(value)),
                    Err(error) => lowered_result_err_value(error),
                },
                FullTag::ExprPathReadlink => match fs_module::readlink(host_path, span) {
                    Ok(value) => lowered_value_from_runtime_any(&value).map(lowered_result_ok).ok_or_else(|| {
                        RuntimeError::new("type-error", format!("readlink produced unsupported {}", value.type_name())).with_span(span)
                    })?,
                    Err(error) => lowered_result_err_value(error),
                },
                FullTag::ExprPathResolve => match fs_module::resolve_path(host_path, span) {
                    Ok(value) => lowered_result_ok(LoweredValue::Path(value)),
                    Err(error) => lowered_result_err_value(error),
                },
                _ => unreachable!(),
            })
        }
        ExprView::Abort { .. } | ExprView::Fail { .. } | ExprView::Error(_) => {
            unreachable!("error and abort values are applied before generic operands")
        }
        _ => unreachable!("frame apply tag has a matching typed view"),
    }
}

impl Evaluator {

    pub(super) fn eval_indexed_expr(
        &mut self,
        execution: &FullExecution<'_>,
        instruction: u32,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        self.sync_indexed_root_slots(slots, call_span)?;
        let result = if self.force_frames || !execution.is_in_place(instruction) {
            self.eval_indexed_expr_with_frames(execution, instruction, slots, call_span)
        } else {
            self.eval_indexed_expr_inner(execution, instruction, slots, call_span)
        };
        let publication = self.sync_indexed_root_slots(slots, call_span);
        let mut flow = match (result, publication) {
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(error),
            (Ok(flow), Ok(())) => Ok(flow),
        }?;
        self.register_returned_lowered_streams(&mut flow, call_span)?;
        Ok(flow)
    }

    fn eval_indexed_module_call_values(
        &mut self,
        op: RuntimeOp,
        mut values: super::NativeArgumentValues,
        span: Span,
        call_plan: Option<&crate::runtime::eval::call_plan::ModuleCallPlan>,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        if let Some(plan) = call_plan {
            plan.convert_command_words(&mut values.0)?;
        }
        let cli_descriptor = call_plan.and_then(ModuleCallPlan::cli_descriptor);
        if !self.trace_enabled || op == RuntimeOp::ArchiveTarCreate {
            let mut result = self.eval_lowered_module_call_values(op, values, span, cli_descriptor)?;
            self.register_returned_lowered_streams(&mut result, span)?;
            return Ok(result);
        }
        let trace_name = crate::modules::signature::api_spec()
            .op_trace_name(op)
            .map(str::to_string);
        let rooted = trace_name
            .as_deref()
            .is_some_and(|name| name.starts_with("FsRoot."));
        self.trace_enter(
            if rooted {
                TraceKind::MethodCall
            } else {
                TraceKind::ModuleCall
            },
            Some(span),
            trace_name.as_deref(),
            TracePayload::None,
        );
        let result = self.eval_lowered_module_call_values(op, values, span, cli_descriptor);
        self.trace_exit(
            if rooted {
                TraceKind::MethodResult
            } else {
                TraceKind::ModuleResult
            },
            Some(span),
            trace_name.as_deref(),
            TracePayload::None,
        );
        let mut result = result?;
        self.register_returned_lowered_streams(&mut result, span)?;
        Ok(result)
    }

    fn eval_indexed_expr_inner(
        &mut self,
        execution: &FullExecution<'_>,
        instruction: u32,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        if !execution.is_in_place(instruction) {
            return Err(RuntimeError::new(
                "indexed-ir",
                "unmarked instruction entered the in-place evaluator",
            )
            .with_span(call_span));
        }
        let (tag, payload) = indexed_value(execution.instruction_id(instruction), call_span)?;
        let in_place = execution.is_in_place(instruction);
        if !indexed_frame_apply_tag(tag)
            && (in_place || !matches!(tag, FullTag::ExprIf | FullTag::ExprMatch))
            && let Some(sink) = &self.route_witness_sink
            && let Some(route) = recursive_route(tag, in_place)
        {
            sink.record(tag, route, RouteWitnessMode::Normal);
        }
        if is_typed_int_tag(tag) {
            return self
                .eval_indexed_typed_int(execution, instruction, slots, call_span)
                .map(|flow| flow.map_continue(LoweredValue::Int));
        }
        if is_typed_bool_tag(tag) {
            return self
                .eval_indexed_typed_bool(execution, instruction, slots, call_span)
                .map(|flow| flow.map_continue(LoweredValue::Bool));
        }
        if matches!(tag, FullTag::ExprIf | FullTag::ExprMatch) {
            let view = ExprView::decode(execution, tag, payload)
                .map_err(|error| indexed_error(error, call_span))?;
            return match view {
                ExprView::IfExpr {
                    branches,
                    else_value,
                    span,
                } => {
                    for branch in branches {
                        let (condition, value) =
                            branch.map_err(|error| indexed_error(error, span))?;
                        match self.eval_indexed_bool(execution, condition.raw(), slots, span)? {
                            ControlFlow::Continue(true) => {
                                return self.eval_indexed_expr(
                                    execution,
                                    value.raw(),
                                    slots,
                                    span,
                                );
                            }
                            ControlFlow::Continue(false) => {}
                            ControlFlow::Break(value) => {
                                return Ok(ControlFlow::Break(value));
                            }
                        }
                    }
                    self.eval_indexed_expr(execution, else_value.raw(), slots, span)
                }
                ExprView::MatchExpr { value, arms, span } => {
                    let value = match self.eval_indexed_expr(execution, value.raw(), slots, span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    };
                    if matches!(value, LoweredValue::ResultErr(_)) {
                        self.pending_traceback = None;
                    }
                    for arm in arms {
                        let (pattern, guard, body) =
                            arm.map_err(|error| indexed_error(error, span))?;
                        if !Self::indexed_pattern_matches(
                            execution,
                            pattern.raw(),
                            &value,
                            slots,
                            span,
                        )? {
                            continue;
                        }
                        if let Some(guard) = guard {
                            match self.eval_indexed_expr(
                                execution,
                                guard.raw(),
                                slots,
                                span,
                            )? {
                                ControlFlow::Continue(LoweredValue::Bool(true)) => {}
                                ControlFlow::Continue(_) => continue,
                                ControlFlow::Break(value) => {
                                    return Ok(ControlFlow::Break(value));
                                }
                            }
                        }
                        return self.eval_indexed_expr(execution, body.raw(), slots, span);
                    }
                    Err(lowered_match_no_arm(span))
                }
                _ => unreachable!("selection tag has a matching typed view"),
            };
        }
        if tag.is_eager_atom() {
            return Ok(ControlFlow::Continue(indexed_eager_atom_value(
                execution,
                tag,
                payload,
                slots,
                call_span,
            )?));
        }
        let unsupported = || {
            RuntimeError::new(
                "indexed-ir",
                format!("direct indexed evaluator does not support {tag:?}"),
            )
            .with_span(call_span)
        };
        let Some(view_kind) = tag.expr_view_kind() else {
            return Err(unsupported());
        };
        let result = match view_kind {
            ExprViewKind::ComparisonChain => {
                let (first, mut pairs) =
                    decode_comparison_chain(execution, tag, payload, call_span)?;
                let mut left = match self.eval_indexed_expr(execution, first, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                for pair in pairs.by_ref() {
                    let pair = pair.map_err(|error| indexed_error(error, call_span))?;
                    let (op, _, right, span) =
                        decode_comparison_pair(execution, pair, call_span)?;
                    let right = match self.eval_indexed_expr(execution, right, slots, span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    };
                    if !comparison_link_holds(op, &left, &right, span)? {
                        return Ok(ControlFlow::Continue(LoweredValue::Bool(false)));
                    }
                    left = right;
                }
                ControlFlow::Continue(LoweredValue::Bool(true))
            }
            ExprViewKind::ResultFallback => {
                let ExprView::ResultFallback { left, right } =
                    ExprView::decode(execution, tag, payload)
                        .map_err(|error| indexed_error(error, call_span))?
                else {
                    unreachable!("result-fallback tag has a result-fallback view")
                };
                return match self.eval_indexed_expr(execution, left.raw(), slots, call_span)? {
                    ControlFlow::Continue(value) => match lowered_fallback_value(value) {
                        Some(value) => Ok(ControlFlow::Continue(value)),
                        None => {
                            // Taking the fallback handles the left side's
                            // `Err`: its traceback describes nothing the
                            // fallback produces, even an equal error.
                            self.pending_traceback = None;
                            self.eval_indexed_expr(execution, right.raw(), slots, call_span)
                        }
                    },
                    ControlFlow::Break(value) => Ok(ControlFlow::Break(value)),
                };
            }
            ExprViewKind::FmtString | ExprViewKind::PathFmtString => {
                let (parts, mut fmt) =
                    decode_fmt(execution, tag, payload, call_span)?;
                for part in parts {
                    match part.map_err(|error| indexed_error(error, call_span))? {
                        FmtPartView::Text(text) => fmt.push_text(text.as_ref()),
                        FmtPartView::Expr(expr, span, spec) => {
                            match self.eval_indexed_expr(execution, expr.raw(), slots, span)? {
                                ControlFlow::Continue(value) => {
                                    fmt.push_value(&value, span, spec.as_ref())?
                                }
                                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                            }
                        }
                    }
                }
                ControlFlow::Continue(fmt.finish()?)
            }
            ExprViewKind::MapLiteral => {
                let mut map = BTreeMap::new();
                let view = ExprView::decode(execution, tag, payload)
                    .map_err(|error| indexed_error(error, call_span))?;
                let ExprView::MapLiteral(entries) = view else {
                    unreachable!("map literal tag has a typed map view")
                };
                for entry in entries {
                    let (key, value, span) =
                        entry.map_err(|error| indexed_error(error, call_span))?;
                    let key = if let Some(key) = key {
                        let key = match self.eval_indexed_expr(execution, key.raw(), slots, span)? {
                            ControlFlow::Continue(value) => value,
                            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                        };
                        Some(lowered_map_literal_key(&key, span)?)
                    } else {
                        None
                    };
                    let value = match self.eval_indexed_expr(execution, value.raw(), slots, span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    };
                    append_lowered_map_literal(&mut map, key, value, span)?;
                }
                ControlFlow::Continue(LoweredValue::Map(Arc::new(map)))
            }
            ExprViewKind::RecordUpdate => {
                let (base, mut updates, span) =
                    decode_record_updates(execution, tag, payload, call_span)?;
                let base = match self.eval_indexed_expr(execution, base, slots, span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let mut replacements = Vec::with_capacity(updates.len());
                for update in updates.by_ref() {
                    let (path, value, field_span) =
                        update.map_err(|error| indexed_error(error, span))?;
                    let path = path
                        .map(|name| name.map_err(|error| indexed_error(error, field_span)))
                        .collect::<Result<Vec<_>, _>>()?;
                    let value = match self.eval_indexed_expr(execution, value.raw(), slots, field_span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    };
                    replacements.push((path, value, field_span));
                }
                ControlFlow::Continue(lowered_record_update_batch(base, replacements, span)?)
            }
            ExprViewKind::Record => {
                let view = ExprView::decode(execution, tag, payload)
                    .map_err(|error| indexed_error(error, call_span))?;
                let ExprView::Record(entries) = view else {
                    unreachable!("record literal tag has a typed record view")
                };
                let mut record = Vec::with_capacity(entries.len());
                for entry in entries {
                    let entry = entry.map_err(|error| indexed_error(error, call_span))?;
                    let instruction = match &entry {
                        RecordEntryView::Field(_, value) | RecordEntryView::Spread(value) => {
                            value.raw()
                        }
                    };
                    match self.eval_indexed_expr(
                        execution,
                        instruction,
                        slots,
                        call_span,
                    )? {
                        ControlFlow::Continue(value) => {
                            append_indexed_record_entry(entry, &mut record, value, call_span)?
                        }
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    }
                }
                ControlFlow::Continue(finish_record_entries(record))
            }
            ExprViewKind::List => {
                let view = ExprView::decode(execution, tag, payload)
                    .map_err(|error| indexed_error(error, call_span))?;
                let ExprView::List(items) = view else {
                    unreachable!("list literal tag has a typed list view")
                };
                let mut result = Vec::with_capacity(items.len());
                for item in items {
                    let expr = item.map_err(|error| indexed_error(error, call_span))?;
                    let span = call_span;
                    match self.eval_indexed_expr(execution, expr.raw(), slots, span)? {
                        ControlFlow::Continue(value) => {
                            append_lowered_list_element(&mut result, value, false, span)?
                        }
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    }
                }
                ControlFlow::Continue(LoweredValue::List(result))
            }
            ExprViewKind::ListBuild => {
                let view = ExprView::decode(execution, tag, payload)
                    .map_err(|error| indexed_error(error, call_span))?;
                let ExprView::ListBuild(items) = view else {
                    unreachable!("list build tag has a typed list build view")
                };
                let mut result = Vec::with_capacity(items.len());
                for item in items {
                    let (splice, expr, span) =
                        item.map_err(|error| indexed_error(error, call_span))?;
                    match self.eval_indexed_expr(execution, expr.raw(), slots, span)? {
                        ControlFlow::Continue(value) => {
                            append_lowered_list_element(&mut result, value, splice, span)?
                        }
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    }
                }
                ControlFlow::Continue(LoweredValue::List(result))
            }
            ExprViewKind::Field => {
                let ExprView::Field { base, name, span } =
                    ExprView::decode(execution, tag, payload)
                        .map_err(|error| indexed_error(error, call_span))?
                else {
                    unreachable!("field tag has a field view")
                };
                let base = base.raw();
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
            ExprViewKind::Index => {
                let ExprView::Index { base, index, span } =
                    ExprView::decode(execution, tag, payload)
                        .map_err(|error| indexed_error(error, call_span))?
                else {
                    unreachable!("index tag has an index view")
                };
                let base = base.raw();
                let index = index.raw();
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
            ExprViewKind::IndexFromEnd => {
                let ExprView::IndexFromEnd {
                    base,
                    distance,
                    span,
                } = ExprView::decode(execution, tag, payload)
                    .map_err(|error| indexed_error(error, call_span))?
                else {
                    unreachable!("index-from-end tag has an index-from-end view")
                };
                let base = base.raw();
                let base = match self.eval_indexed_expr(execution, base, slots, span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                ControlFlow::Continue(lowered_index_from_end_value(base, distance.get(), span)?)
            }
            ExprViewKind::CheckedValue => {
                let ExprView::CheckedValue { value, check, span } =
                    ExprView::decode(execution, tag, payload)
                        .map_err(|error| indexed_error(error, call_span))?
                else {
                    unreachable!("checked-value tag has a checked-value view")
                };
                let check = indexed_type_check(check, call_span)?;
                let value = value.raw();
                match self.eval_indexed_expr(execution, value, slots, span)? {
                    ControlFlow::Continue(value) => {
                        super::checked_unsigned_value(&value, &check, span)?;
                        ControlFlow::Continue(value)
                    }
                    ControlFlow::Break(value) => ControlFlow::Break(value),
                }
            }
            ExprViewKind::Require => {
                let ExprView::Require { value, check, span } =
                    ExprView::decode(execution, tag, payload)
                        .map_err(|error| indexed_error(error, call_span))?
                else {
                    unreachable!("require tag has a require view")
                };
                let check = indexed_type_check(check, call_span)?;
                let value = value.raw();
                let value = match self.eval_indexed_expr(execution, value, slots, span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                ControlFlow::Continue(super::super::require::require_value(
                    self, value, &check, span,
                ))
            }
            ExprViewKind::Ok => {
                let ExprView::Ok(value) = ExprView::decode(execution, tag, payload)
                    .map_err(|error| indexed_error(error, call_span))?
                else {
                    unreachable!("ok tag has an ok view")
                };
                let value = match self.eval_indexed_expr(
                    execution,
                    value.raw(),
                    slots,
                    call_span,
                )? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                ControlFlow::Continue(lowered_result_ok(value))
            }
            ExprViewKind::ProcCallResult => {
                let ExprView::ProcCallResult(value) = ExprView::decode(execution, tag, payload)
                    .map_err(|error| indexed_error(error, call_span))?
                else {
                    unreachable!("proc-call-result tag has a proc-call-result view")
                };
                let value = match self.eval_indexed_expr(
                    execution,
                    value.raw(),
                    slots,
                    call_span,
                )? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                ControlFlow::Continue(proc_call_result(value))
            }
            ExprViewKind::Err => {
                let ExprView::Err { value, cause } = ExprView::decode(execution, tag, payload)
                    .map_err(|error| indexed_error(error, call_span))?
                else {
                    unreachable!("err tag has an err view")
                };
                let value = match self.eval_indexed_expr(
                    execution,
                    value.raw(),
                    slots,
                    call_span,
                )? {
                    ControlFlow::Continue(value) => value.into_value(),
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let cause = match cause.map(|cause| cause.raw()) {
                    Some(cause) => {
                        match self.eval_indexed_expr(execution, cause, slots, call_span)? {
                            ControlFlow::Continue(value) => Some(value),
                            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                        }
                    }
                    None => None,
                };
                ControlFlow::Continue(lowered_err_with_cause(value, cause, call_span)?)
            }
            ExprViewKind::Try => {
                let ExprView::Try { value, span } = ExprView::decode(execution, tag, payload)
                    .map_err(|error| indexed_error(error, call_span))?
                else {
                    unreachable!("try tag has a try view")
                };
                // The propagation's own place: a traceback that starts here
                // names it, not the call that is running.
                // The `?` reuses only a traceback that its operand records.
                self.pending_traceback = None;
                return match self.eval_indexed_expr(execution, value.raw(), slots, call_span)? {
                    ControlFlow::Break(value) => Ok(ControlFlow::Break(value)),
                    ControlFlow::Continue(value) => match self
                        .indexed_question_value(value, span)?
                    {
                        Ok(value) => Ok(ControlFlow::Continue(value)),
                        // Statement consumers must distinguish propagation from lexical return.
                        Err(value) => {
                            Ok(self.preserve_lexical_expression_flow(StmtFlow::Propagate(value)))
                        }
                    },
                };
            }
            ExprViewKind::Null |
            ExprViewKind::Unit |
            ExprViewKind::Int |
            ExprViewKind::UInt |
            ExprViewKind::Float |
            ExprViewKind::Duration |
            ExprViewKind::Bool |
            ExprViewKind::Str |
            ExprViewKind::Bytes |
            ExprViewKind::PreparedRegex |
            ExprViewKind::PreparedConstant |
            ExprViewKind::Path |
            ExprViewKind::FunctionRef |
            ExprViewKind::PathFrom |
            ExprViewKind::Param |
            ExprViewKind::Binary |
            ExprViewKind::IfExpr |
            ExprViewKind::PatternIf |
            ExprViewKind::MatchExpr |
            ExprViewKind::StrMatchExpr |
            ExprViewKind::TagMatchExpr |
            ExprViewKind::Glob |
            ExprViewKind::LastStatus |
            ExprViewKind::EmptyMap |
            ExprViewKind::Range |
            ExprViewKind::Tag |
            ExprViewKind::ListComp |
            ExprViewKind::MapComp |
            ExprViewKind::ListPipeline |
            ExprViewKind::Slice |
            ExprViewKind::Method |
            ExprViewKind::StrByteLen |
            ExprViewKind::StrByteAt |
            ExprViewKind::StrPredicate |
            ExprViewKind::RunCapture |
            ExprViewKind::RunPipeline |
            ExprViewKind::SpawnRun |
            ExprViewKind::SpawnCommand |
            ExprViewKind::Wait |
            ExprViewKind::Capture |
            ExprViewKind::ValueBlock |
            ExprViewKind::ErrorContext |
            ExprViewKind::ContextScope |
            ExprViewKind::Loop |
            ExprViewKind::Retry |
            ExprViewKind::FsFiles |
            ExprViewKind::FsWalk |
            ExprViewKind::FsList |
            ExprViewKind::FsTempDir |
            ExprViewKind::FsMkdir |
            ExprViewKind::FsRemove |
            ExprViewKind::PathReadText |
            ExprViewKind::PathReadBytes |
            ExprViewKind::PathExists |
            ExprViewKind::PathExecutable |
            ExprViewKind::PathDu |
            ExprViewKind::PathMetadata |
            ExprViewKind::PathReadlink |
            ExprViewKind::PathResolve |
            ExprViewKind::PathWrite |
            ExprViewKind::PathMkdir |
            ExprViewKind::PathRemove |
            ExprViewKind::ArchiveTarCreate |
            ExprViewKind::ModuleCall |
            ExprViewKind::ProcessCommandArgv |
            ExprViewKind::ProcessCommandBuilder |
            ExprViewKind::Abort |
            ExprViewKind::Fail |
            ExprViewKind::Error |
            ExprViewKind::Call |
            ExprViewKind::DirectPureCall |
            ExprViewKind::ExternalCall |
            ExprViewKind::DynamicCall |
            ExprViewKind::SelfCall |
            ExprViewKind::TypedCall => return Err(unsupported()),
        };
        Ok(result)
    }

}

fn is_typed_int_tag(tag: FullTag) -> bool {
    matches!(
        tag,
        FullTag::IntInt
            | FullTag::IntSlot
            | FullTag::IntBinary
            | FullTag::IntStrByteLenSlot
            | FullTag::IntStrCountLinesSlot
            | FullTag::IntStrByteAtSlot
    )
}

fn is_typed_bool_tag(tag: FullTag) -> bool {
    matches!(
        tag,
        FullTag::BoolBool
            | FullTag::BoolSlot
            | FullTag::BoolNot
            | FullTag::BoolAnd
            | FullTag::BoolOr
            | FullTag::BoolIntCompare
            | FullTag::BoolStrPredicateSlot
            | FullTag::BoolContainsSlot
            | FullTag::BoolStrContainsSlot
            | FullTag::BoolTrimEmptySlot
            | FullTag::BoolTrimStrPredicateSlot
            | FullTag::BoolLiteralCompareSlot
    )
}

enum ResolvedAssignStep {
    Field(Name),
    Map(MapKey),
    List(i64),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::eval::route_witness::{RouteWitnessRoute, frame_route};
    use crate::sema::check::Checker;
    use crate::source::SourceMap;
    use crate::syntax::parser::Parser;

    #[test]
    fn frame_apply_dispatch_tags_have_matching_schema_routes() {
        use crate::runtime::eval::indexed::full::schema::{INSTRUCTION_TAGS, InstructionKind};

        for tag in INSTRUCTION_TAGS.iter().copied().filter(|tag| {
            tag.kind() == InstructionKind::Expr && !tag.is_eager_atom()
        }) {
            if indexed_frame_apply_tag(tag) {
                assert_eq!(
                    tag.frame_strategy(),
                    FrameStrategy::FrameApply,
                    "{}",
                    tag.name(),
                );
                assert_eq!(
                    frame_route(tag, false),
                    Some(RouteWitnessRoute::ExplicitFrameApply),
                    "{}",
                    tag.name(),
                );
                assert_eq!(
                    frame_route(tag, true),
                    Some(RouteWitnessRoute::GenericOperandApply),
                    "{}",
                    tag.name(),
                );
            }
        }
    }

    #[test]
    fn parametric_records_keep_concrete_schemas_across_calls() {
        crate::runtime::eval::run_eval(|| {
            let source = r#"type Box[T] = {value: T, items: List[T] = []}
type Count = Box[Int]
type Values[T] = List[T]
pure retain(value: Box[Int]) -> Box[Int] { value }
pure sum(values: Values[Int]) -> Int { values[0] + values[1] }
let value = retain(Count(value: 7))
print ${value.value + value.items.len()}
print ${sum([3, 4])}
let raw: Record = {value: 9, items: [1]}
let checked = raw.require(Count)?
print ${checked.value + checked.items[0]}
"#;
            let output = run_program(source);
            assert_eq!(output.0, 0);
            assert_eq!(output.1, b"7\n7\n10\n");
            assert!(output.2.is_empty());
        });
    }

    #[test]
    fn inferred_record_constructors_keep_concrete_facts_after_frontend_drop() {
        crate::runtime::eval::run_eval(|| {
            let source = r#"type Inner[T] = {value: T?}
type Outer[T] = {inner: Inner[T], anchor: T, items: List[T] = []}
type Marker[T] = {name: Str}
const prepared = Outer(inner: Inner(value: null), anchor: 7)
var value = Outer(inner: Inner(value: null), anchor: 9)
let retained = value
value.anchor = 12
let marker: Marker[Int] = Marker(name: "context")
print ${prepared.anchor + prepared.items.len()}
print ${retained.anchor + value.anchor}
print $marker.name
"#;
            let output = run_program(source);
            assert_eq!(output.0, 0);
            assert_eq!(output.1, b"7\n21\ncontext\n");
            assert!(output.2.is_empty());
        });
    }

    #[test]
    fn assignment_path_copies_only_shared_ancestors() {
        let span = Span::new(crate::source::SourceId::new(0), 0, 0);
        let list =
            || LoweredValue::SharedList(Arc::new(vec![LoweredValue::Int(1), LoweredValue::Int(2)]));
        let mut root = LoweredValue::Map(Arc::new(BTreeMap::from([
            (MapKey::from("selected"), list()),
            (MapKey::from("untouched"), list()),
        ])));
        let backing = |root: &LoweredValue| {
            let LoweredValue::Map(map) = root else {
                unreachable!()
            };
            let LoweredValue::SharedList(selected) = &map[&MapKey::from("selected")] else {
                unreachable!()
            };
            let LoweredValue::SharedList(untouched) = &map[&MapKey::from("untouched")] else {
                unreachable!()
            };
            (
                Arc::as_ptr(map),
                Arc::as_ptr(selected),
                Arc::as_ptr(untouched),
            )
        };
        let path = [
            ResolvedAssignStep::Map("selected".into()),
            ResolvedAssignStep::List(1),
        ];
        let original = backing(&root);
        apply_indexed_path_assignment(
            &mut root,
            &path,
            AssignOp::Set,
            LoweredValue::Int(9),
            false,
            None,
            span,
        )
        .unwrap();
        assert_eq!(backing(&root), original);
        let alias = root.clone();
        apply_indexed_path_assignment(
            &mut root,
            &path,
            AssignOp::Set,
            LoweredValue::Int(10),
            false,
            None,
            span,
        )
        .unwrap();
        let changed = backing(&root);
        assert_ne!(changed.0, original.0);
        assert_ne!(changed.1, original.1);
        assert_eq!(changed.2, original.2);
        assert_eq!(backing(&alias), original);
        let invalid = [
            ResolvedAssignStep::Map("selected".into()),
            ResolvedAssignStep::List(99),
        ];
        assert!(
            apply_indexed_path_assignment(
                &mut root,
                &invalid,
                AssignOp::Set,
                LoweredValue::Int(0),
                false,
                None,
                span
            )
            .is_err()
        );
        assert_eq!(backing(&root), changed);
    }

    #[test]
    fn assignment_path_reuses_unique_storage_and_preserves_aliases() {
        let span = Span::new(crate::source::SourceId::new(0), 0, 0);
        let path = [ResolvedAssignStep::List(1)];
        let mut owned = LoweredValue::List(vec![LoweredValue::Int(1), LoweredValue::Int(2)]);
        let LoweredValue::List(list) = &owned else {
            unreachable!()
        };
        let backing = list.as_ptr();
        apply_indexed_path_assignment(
            &mut owned,
            &path,
            AssignOp::Set,
            LoweredValue::Int(9),
            false,
            None,
            span,
        )
        .unwrap();
        let LoweredValue::List(list) = &owned else {
            unreachable!()
        };
        assert_eq!(list.as_ptr(), backing);
        let mut shared =
            LoweredValue::SharedList(Arc::new(vec![LoweredValue::Int(1), LoweredValue::Int(2)]));
        let LoweredValue::SharedList(list) = &shared else {
            unreachable!()
        };
        let backing = Arc::as_ptr(list);
        apply_indexed_path_assignment(
            &mut shared,
            &path,
            AssignOp::Set,
            LoweredValue::Int(9),
            false,
            None,
            span,
        )
        .unwrap();
        let LoweredValue::SharedList(list) = &shared else {
            unreachable!()
        };
        assert_eq!(Arc::as_ptr(list), backing);
        let alias = shared.clone();
        apply_indexed_path_assignment(
            &mut shared,
            &path,
            AssignOp::Add,
            LoweredValue::Int(1),
            false,
            None,
            span,
        )
        .unwrap();
        assert_eq!(
            alias,
            LoweredValue::List(vec![LoweredValue::Int(1), LoweredValue::Int(9)])
        );
        assert_eq!(
            shared,
            LoweredValue::List(vec![LoweredValue::Int(1), LoweredValue::Int(10)])
        );
        let LoweredValue::SharedList(list) = &shared else {
            unreachable!()
        };
        let backing = Arc::as_ptr(list);
        assert!(
            apply_indexed_path_assignment(
                &mut shared,
                &path,
                AssignOp::Div,
                LoweredValue::Int(0),
                false,
                None,
                span
            )
            .is_err()
        );
        let LoweredValue::SharedList(list) = &shared else {
            unreachable!()
        };
        assert_eq!(Arc::as_ptr(list), backing);
        assert_eq!(list[1], LoweredValue::Int(10));
    }

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
    /// One program covering calls, captures, Results, recursion, and streams,
    /// with top-level statements that read script bindings a proc writes back.
    #[test]
    fn calls_and_top_level_statements_share_script_bindings() {
        crate::runtime::eval::run_eval(calls_and_top_level_statements_share_script_bindings_inner);
    }

    fn calls_and_top_level_statements_share_script_bindings_inner() {
        let source = r#"
    let factor: Int = 3
    var total: Int = 0

    pure nested(a: Int, b: Int) -> Int {
      return (a + b) * (a - b) + factor
    }

    pure with_defaults(a: Int, b: Int = 7, ...rest: List[Int]) -> Int {
      var sum = a + b
      for value in rest {
        sum = sum + value
      }
      return sum
    }

    pure is_even(n: Int) -> Bool {
      if n == 0 {
        return true
      }
      return is_odd(n - 1)
    }

    pure is_odd(n: Int) -> Bool {
      if n == 0 {
        return false
      }
      return is_even(n - 1)
    }

    pure scaled(text: Str) -> Result[Int] {
      let value = text.parse_int()?
      return Ok(value * factor)
    }

    proc add_to_total(value: Int) -> Int {
      total = total + value
      return total
    }

    stream rows(limit: Int) -> Stream[Int] {
      var index = 0
      while index < limit {
        yield index * factor
        index = index + 1
      }
    }

    for row in rows(3) {
      print f"row {row}"
    }
    print f"{nested(-5, 2)} {with_defaults(1)} {with_defaults(1, 7, 4, 5)}"
    print f"{is_even(7)} {is_odd(7)} {is_even(8)}"
    print f"{scaled("41")?}"
    print f"{add_to_total(5)} {add_to_total(6)} {total}"
    match scaled("nope") {
      Ok(value) => print f"ok {value}"
      Err(error) => print f"rejected {error.message}"
    }
    let values: List[Int] = [1, 2, 3, 4]
    print f"{values |> where . > 1 |> map . * factor |> sum}"
    if total > 0 {
      let seen = add_to_total(1) + total
      print f"{seen} {total}"
    }
    "#;
        let output = run_program(source);
        assert_eq!(output.0, 0);
        assert_eq!(
            output.1.as_slice(),
            concat!(
                "row 0\nrow 3\nrow 6\n",
                "24 8 17\n",
                "false true true\n",
                "123\n",
                "5 11 11\n",
                "rejected invalid integer `nope`\n",
                "27\n",
                "24 12\n",
            )
            .as_bytes()
        );
        assert!(output.2.is_empty());
    }

    fn run_program(source: &str) -> (u8, Vec<u8>, Vec<u8>) {
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("call-routes.xsh", source);
        let parsed = Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
        let plan = evaluator
            .prepare_compact_indexed_only(&parsed.arena, source_id)
            .expect("the call-route program prepares");
        let output = match evaluator.eval_installed_compact_indexed_only(plan) {
            Ok(output) => output,
            // The error arm hands the evaluator back, which has no `Debug`.
            Err(_) => panic!("the call-route program installs and runs"),
        };
        (output.status, output.stdout, output.stderr)
    }
}
