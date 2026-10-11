use super::{
    Arc, AssignOp, BTreeMap, BinaryOp, Binding, CommandPlan, ControlFlow, Duration, DurationValue,
    Evaluator, FileRedirectionMode, Flow, FormatSpec, FunctionHeader, FunctionName,
    LoweredCompTarget, LoweredFunctionKey, LoweredFunctionKind, LoweredMapCursor,
    LoweredModuleExportKind, LoweredProjectedReduceState, LoweredReduceProjection,
    LoweredRetryAttemptValue, LoweredReturnKind, LoweredScalarCursor, LoweredStrPredicate,
    LoweredTagValue, LoweredType, LoweredValue, Name, PathValue, ProcessEnd, ProcessInvocation,
    ProcessRedirection, ProcessStatus, QualifiedName, RecordMap, RedirectionKind,
    RedirectionStream, ReduceByOp, RegexValue, RunError, RunKind, RuntimeError, RuntimeOp,
    ScanCondition, Span, SpawnOptions, StmtFlow, StreamValue, TraceArg, TraceError, TraceKind,
    TracePayload, Traceback, TracebackFrame, TracebackFrameKind, Type, Value, api_spec,
    append_lowered_list_element, append_lowered_map_literal, assign_lowered_bytes_view,
    assign_lowered_str_view, bind_lowered_comp_target, btree_map, bytes_contains, bytes_module,
    check_env_name, checked_int_binary, compare_lowered_sort_keys, compound_assignment_value,
    error_constructor, execute_run_with_policy, exit_status, fs_module, json_module,
    lowered_assign_value, lowered_binary_value, lowered_bool_arg_or, lowered_bool_builder_field,
    lowered_bytes_or_str_owned, lowered_bytes_parts, lowered_bytes_value,
    lowered_command_plan_value, lowered_command_redirections, lowered_contains_value,
    lowered_count_key, lowered_duration_arg, lowered_encode_json, lowered_env_record_arg,
    lowered_error_message, lowered_freeze_large_slot_list, lowered_index_from_end_value,
    lowered_index_value, lowered_inline_stats_field_value, lowered_inline_stats_to_record_vec,
    lowered_int_arg, lowered_map_literal_key, lowered_match_no_arm, lowered_nonnegative_count,
    lowered_parse_command_values, lowered_path_arg, lowered_path_from_value, lowered_path_like_arg,
    lowered_path_list_arg, lowered_path_method_value, lowered_pipeline_input,
    lowered_pipeline_item_count, lowered_pipeline_record_list, lowered_process_run_error,
    lowered_record_field_value, lowered_record_vec_append_or_replace_unsorted,
    lowered_record_vec_get, lowered_record_vec_or_stats, lowered_reduce_fields_owned,
    lowered_reduce_group_insert, lowered_reduce_key_value_owned, lowered_result_err_value,
    lowered_result_ok, lowered_slice_value, lowered_sort_key_orderable, lowered_splice_arg_items,
    lowered_stats_field_value, lowered_status_segment_record, lowered_stmt_flow_to_flow,
    lowered_str_arg_owned, lowered_str_byte_at_value, lowered_str_byte_len_value,
    lowered_str_count_lines_value, lowered_str_key, lowered_str_list_arg, lowered_str_parts,
    lowered_str_predicate_text, lowered_str_predicate_value, lowered_str_value,
    lowered_str_view_value, lowered_table_print_value, lowered_tag_key,
    lowered_trace_error_from_value, lowered_trim_is_empty_value, lowered_trim_str_predicate_value,
    lowered_type_name, lowered_unit_result, lowered_value_argv_len, lowered_value_from_runtime,
    lowered_value_from_runtime_any, new_temp_fs_root,
    path_bytes, push_lowered_display, push_lowered_fmt_value, push_lowered_native_fmt_value,
    read_host_path_bytes, read_host_path_text, run_pipeline_inherit_with_policy,
    runtime_error_from_value, splice_to_argv, structured_error_constructor,
    ValueView, value_matches_static_type, value_to_argv_bytes,
};
use crate::map_key::MapKey;
use crate::runtime::eval::indexed::IrVerifyError;
use crate::runtime::eval::indexed::full::{
    BLOCK_LIST, BLOCK_STATEMENTS, FullDriverTag, FullExecution, FullFunctionView, FullPatternTag,
    FullPayload, FullProgram, FullStageTag, FullTag,
};
use crate::runtime::eval::lower::{
    lowered_error_value_has_facet, lowered_error_variant_matches, lowered_record_field,
};
use crate::runtime::eval::lowered_ops::lowered_record_update_batch;
use crate::runtime::eval::{
    LoweredModuleExport, LoweredTopLevelSlot, LoweredTypeCheck, Propagation, ScanBytes, ScanCheck,
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
    indexed_assignment_operand, apply_indexed_assignment, resolve_assign_index,
    checked_indexed_assignment, apply_indexed_path_assignment,
};
use calls::{
    append_call_argument, indexed_typed_callee_kind, indexed_callable_identity,
};
use operands::{
    indexed_error, indexed_value, indexed_decode, indexed_raw, indexed_string, indexed_finish,
    indexed_optional_raw, decode_record_updates, decode_comparison_chain, decode_match_expr,
    decode_module_call, decode_assign_path, decode_comp_qualifiers, fmt_operands,
};

use process::run_target_and_leading_argv;
use values::{
    bounded_assertion_text, capture_checked_error, lowered_condition_bool, lowered_fallback_value,
    lowered_err_with_cause, comparison_link_holds, finish_record_entries, lowered_comp_iterable,
    proc_call_result,
};

use serial_pipeline::IndexedPipelineItems;

use xsh_registry::stream_parameters::DEFAULT_PAR_MAP_WORKERS;

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
    kind: RunKind,
    target: RunArg,
    args: Vec<RunArg>,
    env: Vec<RunEnv>,
    redirections: Vec<RunRedirection>,
    timeout: Option<u32>,
    cpu_max: Option<u32>,
    accept: Option<u32>,
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
        accept: Option<u32>,
        span: Span,
    },
}

type IndexedModuleCall = (
    RuntimeOp,
    Option<Arc<crate::modules::cli::CliDescriptorPlan>>,
    Vec<Option<u32>>,
    Span,
);

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

/// What a call's arguments are evaluated for: a function the call names, or a
/// callable value.
enum IndexedCallee {
    Named(LoweredFunctionKey),
    Value(LoweredValue),
}

impl IndexedCallee {
    /// The prepared default for an argument the call omits.
    fn argument_default(
        &self,
        evaluator: &Evaluator,
        slot: usize,
        span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        match self {
            Self::Named(function) => evaluator
                .indexed_argument_default_for(*function, LoweredFunctionKind::Pure, slot, span)
                .or_else(|_| {
                    evaluator.indexed_argument_default_for(
                        *function,
                        LoweredFunctionKind::Proc,
                        slot,
                        span,
                    )
                }),
            Self::Value(callee) => evaluator.indexed_argument_default(callee, slot, span),
        }
    }
}

/// One decoded entry of an aggregate literal's operand list.
trait IndexedOperand: Sized {
    fn decode<'a>(
        execution: &FullExecution<'a>,
        input: &mut FullPayload<'a>,
        build: bool,
        span: Span,
    ) -> Result<Self, RuntimeError>;
}

/// A list element: its instruction, whether it splices, and its span.
impl IndexedOperand for (u32, bool, Span) {
    #[inline]
    fn decode<'a>(
        execution: &FullExecution<'a>,
        input: &mut FullPayload<'a>,
        build: bool,
        span: Span,
    ) -> Result<Self, RuntimeError> {
        if !build {
            return Ok((indexed_raw(input, span)?, false, span));
        }
        let splice = indexed_decode::<bool>(input, execution, span)?;
        Ok((
            indexed_raw(input, span)?,
            splice,
            indexed_decode::<Span>(input, execution, span)?,
        ))
    }
}

/// A map literal entry: its optional computed key, value, and span.
impl IndexedOperand for (Option<u32>, u32, Span) {
    #[inline]
    fn decode<'a>(
        execution: &FullExecution<'a>,
        input: &mut FullPayload<'a>,
        _: bool,
        span: Span,
    ) -> Result<Self, RuntimeError> {
        Ok((
            indexed_optional_raw(input, span)?,
            indexed_raw(input, span)?,
            indexed_decode::<Span>(input, execution, span)?,
        ))
    }
}

impl IndexedOperand for IndexedRecordEntry {
    #[inline]
    fn decode<'a>(
        execution: &FullExecution<'a>,
        input: &mut FullPayload<'a>,
        _: bool,
        span: Span,
    ) -> Result<Self, RuntimeError> {
        Ok(match indexed_raw(input, span)? {
            0 => Self::Field {
                name: indexed_decode(input, execution, span)?,
                instruction: indexed_raw(input, span)?,
            },
            1 => Self::Spread(indexed_raw(input, span)?),
            _ => {
                return Err(
                    RuntimeError::new("indexed-ir", "invalid indexed record entry").with_span(span),
                );
            }
        })
    }
}

impl IndexedOperand for IndexedFmtPart {
    #[inline]
    fn decode<'a>(
        execution: &FullExecution<'a>,
        input: &mut FullPayload<'a>,
        _: bool,
        span: Span,
    ) -> Result<Self, RuntimeError> {
        Ok(match indexed_raw(input, span)? {
            0 => Self::Text(indexed_decode(input, execution, span)?),
            1 => Self::Expr(
                indexed_raw(input, span)?,
                indexed_decode(input, execution, span)?,
                indexed_decode(input, execution, span)?,
            ),
            _ => {
                return Err(
                    RuntimeError::new("indexed-ir", "invalid indexed format part").with_span(span),
                );
            }
        })
    }
}

/// An aggregate literal's operand list, decoded one entry at a time so the
/// recursive path needs no intermediate list; the frame path collects it. Both
/// dispatch paths evaluate these operands in source order.
struct IndexedOperands<'e, 'a, T> {
    execution: &'e FullExecution<'a>,
    input: FullPayload<'a>,
    remaining: usize,
    build: bool,
    span: Span,
    entry: std::marker::PhantomData<T>,
}

impl<'e, 'a, T: IndexedOperand> IndexedOperands<'e, 'a, T> {
    fn new(
        execution: &'e FullExecution<'a>,
        payload: &mut FullPayload<'a>,
        build: bool,
        span: Span,
    ) -> Result<Self, RuntimeError> {
        let (_, mut input) = execution
            .block(payload, BLOCK_LIST)
            .map_err(|error| indexed_error(error, span))?;
        let remaining = indexed_raw(&mut input, span)? as usize;
        Ok(Self {
            execution,
            input,
            remaining,
            build,
            span,
            entry: std::marker::PhantomData,
        })
    }

    /// Decodes a list, map, or record literal, whose payload holds only the operand list.
    fn literal(
        execution: &'e FullExecution<'a>,
        mut payload: FullPayload<'a>,
        build: bool,
        span: Span,
    ) -> Result<Self, RuntimeError> {
        let operands = Self::new(execution, &mut payload, build, span)?;
        indexed_finish(payload, span)?;
        Ok(operands)
    }

    fn len(&self) -> usize {
        self.remaining
    }

    #[inline]
    fn next_operand(&mut self) -> Result<Option<T>, RuntimeError> {
        if self.remaining == 0 {
            return Ok(None);
        }
        self.remaining -= 1;
        Ok(Some(T::decode(
            self.execution,
            &mut self.input,
            self.build,
            self.span,
        )?))
    }

    fn finish(self) -> Result<(), RuntimeError> {
        indexed_finish(self.input, self.span)
    }

    fn into_vec(mut self) -> Result<Vec<T>, RuntimeError> {
        let mut items = Vec::with_capacity(self.remaining);
        while let Some(item) = self.next_operand()? {
            items.push(item);
        }
        self.finish()?;
        Ok(items)
    }
}

enum IndexedRecordEntry {
    Field { name: Name, instruction: u32 },
    Spread(u32),
}

impl IndexedRecordEntry {
    fn instruction(&self) -> u32 {
        match self {
            Self::Field { instruction, .. } | Self::Spread(instruction) => *instruction,
        }
    }

    fn append(
        &self,
        fields: &mut Vec<(Name, LoweredValue)>,
        value: LoweredValue,
        span: Span,
    ) -> Result<(), RuntimeError> {
        if let Self::Field { name, .. } = self {
            lowered_record_vec_append_or_replace_unsorted(fields, *name, value);
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
            LoweredValue::Stats {
                blanks,
                code,
                comments,
            } => {
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
}

#[derive(Clone)]
enum IndexedFmtPart {
    Text(Arc<str>),
    Expr(u32, Span, Option<FormatSpec>),
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
enum IndexedAssignStep {
    Field(Name),
    Index(u32),
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

#[derive(Clone)]
enum IndexedCompQualifier {
    For {
        target: LoweredCompTarget,
        iter: u32,
        span: Span,
    },
    If {
        condition: u32,
        span: Span,
    },
}

impl IndexedCompQualifier {
    fn span(&self) -> Span {
        match self {
            Self::For { span, .. } | Self::If { span, .. } => *span,
        }
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
        let result = self.eval_indexed_expr_inner(execution, instruction, slots, call_span);
        let publication = self.sync_indexed_root_slots(slots, call_span);
        match (result, publication) {
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(error),
            (Ok(flow), Ok(())) => Ok(flow),
        }
    }

    fn eval_indexed_module_call_values(
        &mut self,
        op: RuntimeOp,
        values: super::NativeArgumentValues,
        span: Span,
        cli_plan: Option<&crate::modules::cli::CliDescriptorPlan>,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        if !self.trace_enabled {
            return self.eval_lowered_module_call_values(op, values, span, cli_plan);
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
        let result = self.eval_lowered_module_call_values(op, values, span, cli_plan);
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
        result
    }

    fn eval_indexed_expr_inner(
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
            FullTag::ExprPreparedConstant => {
                let value = indexed_decode::<crate::runtime::eval::PreparedConstantValue>(
                    &mut payload,
                    execution,
                    call_span,
                )?;
                indexed_finish(payload, call_span)?;
                ControlFlow::Continue(value.0)
            }
            FullTag::ExprPreparedRegex => {
                let value = indexed_decode::<RegexValue>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                ControlFlow::Continue(LoweredValue::Regex(Box::new(value)))
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
            FullTag::ExprComparisonChain => {
                let (first, pairs) = decode_comparison_chain(execution, &mut payload, call_span)?;
                indexed_finish(payload, call_span)?;
                let mut left = match self.eval_indexed_expr(execution, first, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                for (op, right, span) in pairs {
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
            FullTag::ExprPatternIf => {
                let (_, mut branches) = execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, call_span))?;
                let count = indexed_raw(&mut branches, call_span)? as usize;
                let mut decoded = Vec::with_capacity(count);
                for _ in 0..count {
                    let condition = indexed_raw(&mut branches, call_span)?;
                    let value = indexed_raw(&mut branches, call_span)?;
                    let captures =
                        indexed_decode::<Vec<usize>>(&mut branches, execution, call_span)?;
                    decoded.push((condition, value, captures));
                }
                indexed_finish(branches, call_span)?;
                let else_value = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                for (condition, value, captures) in decoded {
                    let scope_id = self.enter_owned_host_scope();
                    let result = (|| {
                        match self.eval_indexed_bool(execution, condition, slots, span)? {
                            ControlFlow::Continue(false) => return Ok(StmtFlow::None),
                            ControlFlow::Break(value) => return Ok(StmtFlow::Propagate(value)),
                            ControlFlow::Continue(true) => {}
                        }
                        match self.eval_indexed_expr(execution, value, slots, span)? {
                            ControlFlow::Continue(value) => Ok(StmtFlow::Value(value)),
                            ControlFlow::Break(value) => Ok(StmtFlow::Return(value)),
                        }
                    })();
                    match self.finish_indexed_pattern_scope(scope_id, &captures, slots, result)? {
                        StmtFlow::None => {}
                        StmtFlow::Value(value) => return Ok(ControlFlow::Continue(value)),
                        StmtFlow::Propagate(value) | StmtFlow::Return(value) => {
                            return Ok(ControlFlow::Break(value));
                        }
                        _ => unreachable!("expression branch produced statement control flow"),
                    }
                }
                self.eval_indexed_expr(execution, else_value, slots, span)?
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
                    ControlFlow::Continue(value) => match lowered_fallback_value(value) {
                        Some(value) => Ok(ControlFlow::Continue(value)),
                        None => {
                            // Taking the fallback handles the left side's
                            // `Err`: its traceback describes nothing the
                            // fallback produces, even an equal error.
                            self.pending_traceback = None;
                            self.eval_indexed_expr(execution, right, slots, call_span)
                        }
                    },
                    ControlFlow::Break(value) => Ok(ControlFlow::Break(value)),
                };
            }
            FullTag::ExprFmtString | FullTag::ExprPathFmtString => {
                let (mut parts, mut fmt) = fmt_operands(
                    execution,
                    payload,
                    tag == FullTag::ExprPathFmtString,
                    call_span,
                )?;
                while let Some(part) = parts.next_operand()? {
                    match part {
                        IndexedFmtPart::Text(text) => fmt.push_text(&text),
                        IndexedFmtPart::Expr(expr, span, spec) => {
                            match self.eval_indexed_expr(execution, expr, slots, call_span)? {
                                ControlFlow::Continue(value) => {
                                    fmt.push_value(&value, span, spec.as_ref())?
                                }
                                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                            }
                        }
                    }
                }
                parts.finish()?;
                ControlFlow::Continue(fmt.finish()?)
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
            FullTag::ExprMapLiteral => {
                let mut map = BTreeMap::new();
                let mut entries = IndexedOperands::<(Option<u32>, u32, Span)>::literal(
                    execution, payload, false, call_span,
                )?;
                while let Some((key, value, span)) = entries.next_operand()? {
                    let key = if let Some(key) = key {
                        let key = match self.eval_indexed_expr(execution, key, slots, span)? {
                            ControlFlow::Continue(value) => value,
                            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                        };
                        Some(lowered_map_literal_key(&key, span)?)
                    } else {
                        None
                    };
                    let value = match self.eval_indexed_expr(execution, value, slots, span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    };
                    append_lowered_map_literal(&mut map, key, value, span)?;
                }
                entries.finish()?;
                ControlFlow::Continue(LoweredValue::Map(Arc::new(map)))
            }
            FullTag::ExprRecordUpdate => {
                let base = indexed_raw(&mut payload, call_span)?;
                let updates = decode_record_updates(execution, &mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let base = match self.eval_indexed_expr(execution, base, slots, span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let mut replacements = Vec::with_capacity(updates.len());
                for (path, value, field_span) in updates {
                    let value = match self.eval_indexed_expr(execution, value, slots, field_span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    };
                    replacements.push((path, value, field_span));
                }
                ControlFlow::Continue(lowered_record_update_batch(base, replacements, span)?)
            }
            FullTag::ExprRecord => {
                let mut entries = IndexedOperands::<IndexedRecordEntry>::literal(
                    execution, payload, false, call_span,
                )?;
                let mut record = Vec::with_capacity(entries.len());
                while let Some(entry) = entries.next_operand()? {
                    match self.eval_indexed_expr(
                        execution,
                        entry.instruction(),
                        slots,
                        call_span,
                    )? {
                        ControlFlow::Continue(value) => {
                            entry.append(&mut record, value, call_span)?
                        }
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    }
                }
                entries.finish()?;
                ControlFlow::Continue(finish_record_entries(record))
            }
            FullTag::ExprList | FullTag::ExprListBuild => {
                let mut items = IndexedOperands::<(u32, bool, Span)>::literal(
                    execution,
                    payload,
                    tag == FullTag::ExprListBuild,
                    call_span,
                )?;
                let mut result = Vec::with_capacity(items.len());
                while let Some((expr, splice, span)) = items.next_operand()? {
                    match self.eval_indexed_expr(execution, expr, slots, span)? {
                        ControlFlow::Continue(value) => {
                            append_lowered_list_element(&mut result, value, splice, span)?
                        }
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    }
                }
                items.finish()?;
                ControlFlow::Continue(LoweredValue::List(result))
            }
            FullTag::ExprEmptyMap => {
                indexed_finish(payload, call_span)?;
                ControlFlow::Continue(LoweredValue::Map(Arc::new(BTreeMap::new())))
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
                let type_name = indexed_decode::<Name>(&mut payload, execution, call_span)?;
                let name = indexed_decode::<Arc<str>>(&mut payload, execution, call_span)?;
                let (_, mut fields) = execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, call_span))?;
                let len = indexed_raw(&mut fields, call_span)? as usize;
                let mut values = Vec::with_capacity(len);
                for _ in 0..len {
                    let field = indexed_raw(&mut fields, call_span)?;
                    match self.eval_indexed_expr(execution, field, slots, call_span)? {
                        ControlFlow::Continue(value) => values.push(value),
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    }
                }
                indexed_finish(fields, call_span)?;
                let wire = indexed_decode::<Option<Arc<crate::sema::wire_enums::WireEnumMapping>>>(
                    &mut payload,
                    execution,
                    call_span,
                )?;
                indexed_finish(payload, call_span)?;
                ControlFlow::Continue(LoweredValue::Tag(Box::new(LoweredTagValue {
                    type_name,
                    wire,
                    name,
                    fields: values,
                })))
            }
            // These run on the frames, which own branch selection, comprehension
            // iteration and stream cancellation, and the boundaries that
            // captures, error contexts and context scopes put around their bodies.
            FullTag::ExprIf
            | FullTag::ExprMatch
            | FullTag::ExprListComp
            | FullTag::ExprMapComp
            | FullTag::ExprCapture
            | FullTag::ExprErrorContext
            | FullTag::ExprContextScope => {
                self.eval_indexed_expr_with_frames(execution, instruction, slots, call_span)?
            }
            FullTag::ExprPipeline => {
                self.eval_indexed_pipeline(execution, payload, slots, call_span)?
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
            FullTag::ExprIndexFromEnd => {
                let base = indexed_raw(&mut payload, call_span)?;
                let distance = indexed_decode::<crate::runtime::eval::EndDistance>(
                    &mut payload,
                    execution,
                    call_span,
                )?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let base = match self.eval_indexed_expr(execution, base, slots, span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                ControlFlow::Continue(lowered_index_from_end_value(base, distance.get(), span)?)
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
                return self.eval_indexed_method_dispatch(receiver, name, values, span);
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
                let byte = lowered_str_byte_at_value(&receiver, index, -1, span)?;
                ControlFlow::Continue(if byte < 0 {
                    LoweredValue::Null
                } else {
                    LoweredValue::Int(byte)
                })
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
            FullTag::ExprCheckedValue => {
                let value = indexed_raw(&mut payload, call_span)?;
                let check = indexed_decode::<LoweredTypeCheck>(&mut payload, execution, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                match self.eval_indexed_expr(execution, value, slots, span)? {
                    ControlFlow::Continue(value) => {
                        super::checked_unsigned_value(&value, &check, span)?;
                        ControlFlow::Continue(value)
                    }
                    ControlFlow::Break(value) => ControlFlow::Break(value),
                }
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
                ControlFlow::Continue(super::super::require::require_value(
                    self, value, &check, span,
                ))
            }
            // A value block enters a block frame directly, without the expression
            // boundary an expression frame adds; stage bodies run one per item.
            FullTag::ExprValueBlock => {
                let body = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                match self.eval_indexed_statement_block(execution, body, slots, span)? {
                    StmtFlow::Value(value) => ControlFlow::Continue(value),
                    StmtFlow::None => ControlFlow::Continue(LoweredValue::Unit),
                    flow => self.preserve_lexical_expression_flow(flow),
                }
            }
            FullTag::ExprLoop => {
                let body = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                loop {
                    self.service_pending_signal(span)?;
                    if self.shutting_down() {
                        break ControlFlow::Continue(LoweredValue::Unit);
                    }
                    match self.eval_indexed_statement_block(execution, body, slots, span)? {
                        StmtFlow::None | StmtFlow::Continue => {}
                        StmtFlow::Break(value) => {
                            break ControlFlow::Continue(value.unwrap_or(LoweredValue::Unit));
                        }
                        flow @ (StmtFlow::Value(_)
                        | StmtFlow::Return(_)
                        | StmtFlow::Propagate(_)) => {
                            self.pending_value_block_flow = Some(flow);
                            break ControlFlow::Break(LoweredValue::Unit);
                        }
                    }
                }
            }
            FullTag::ExprRetry => {
                let (_, mut delays) = execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, call_span))?;
                let delay_count = indexed_raw(&mut delays, call_span)? as usize;
                let pattern = indexed_optional_raw(&mut payload, call_span)?;
                let body = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                let schedule = indexed_decode::<crate::syntax::arena::RetrySchedule>(
                    &mut payload,
                    execution,
                    call_span,
                )?;
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
                let mut schedule = match (schedule, delay_values.as_slice()) {
                    (crate::syntax::arena::RetrySchedule::Delays, _) => {
                        super::LoweredRetryDelays::Listed(delay_values)
                    }
                    (crate::syntax::arena::RetrySchedule::Backoff, [first, cap, limit]) => {
                        super::LoweredRetryDelays::backoff(first.millis, cap.millis, limit.millis)
                    }
                    (crate::syntax::arena::RetrySchedule::Backoff, _) => {
                        return Err(RuntimeError::new(
                            "type-error",
                            "retry backoff expected a first interval, a cap, and a limit",
                        )
                        .with_span(span));
                    }
                };
                let max_attempts = schedule.max_attempts();
                let mut final_error = None;
                let mut final_traceback = None;
                let mut pending_delay: Option<u64> = None;
                for attempt_index in 0.. {
                    if let Some(millis) = pending_delay.take() {
                        self.sleep_lowered_retry_delay(&DurationValue { millis }, span)?;
                        if self.shutting_down() {
                            break;
                        }
                    }
                    let attempt_flow =
                        self.eval_indexed_error_boundary_block(execution, body, slots, span)?;
                    if matches!(attempt_flow, StmtFlow::Continue | StmtFlow::Break(_)) {
                        self.pending_value_block_flow = Some(attempt_flow);
                        return Ok(ControlFlow::Break(LoweredValue::Unit));
                    }
                    match self.lowered_retry_attempt_value(attempt_flow) {
                        LoweredRetryAttemptValue::Success(value) => {
                            self.trace_lowered_retry_attempt(
                                span,
                                attempt_index + 1,
                                max_attempts,
                                None,
                                None,
                                None,
                                Some(crate::trace::RetryStopReason::Success),
                            );
                            return Ok(ControlFlow::Continue(LoweredValue::ResultOk(Box::new(
                                value,
                            ))));
                        }
                        LoweredRetryAttemptValue::Failed { error, traceback } => {
                            let selected = match pattern {
                                Some(pattern) => Some(Self::indexed_pattern_match_pass(
                                    execution,
                                    pattern,
                                    &LoweredValue::Error(Box::new(error.clone())),
                                    slots,
                                    span,
                                    false,
                                )?),
                                None => None,
                            };
                            let next_delay = if selected == Some(false) {
                                None
                            } else {
                                schedule.next_delay(attempt_index)
                            };
                            pending_delay = next_delay;
                            let stop_reason = if selected == Some(false) {
                                Some(crate::trace::RetryStopReason::Nonmatching)
                            } else if next_delay.is_none() {
                                Some(crate::trace::RetryStopReason::Exhausted)
                            } else {
                                None
                            };
                            self.trace_lowered_retry_attempt(
                                span,
                                attempt_index + 1,
                                max_attempts,
                                next_delay,
                                Some(lowered_trace_error_from_value(&error)),
                                selected,
                                stop_reason,
                            );
                            final_error = Some(error);
                            final_traceback = traceback;
                            self.pending_traceback = None;
                            if stop_reason.is_some() {
                                break;
                            }
                        }
                        LoweredRetryAttemptValue::ControlBreak => {
                            return Ok(ControlFlow::Continue(LoweredValue::Unit));
                        }
                        LoweredRetryAttemptValue::Escape(value) => {
                            self.pending_value_block_flow = Some(StmtFlow::Return(value));
                            return Ok(ControlFlow::Break(LoweredValue::Unit));
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
                let gitignore = indexed_optional_raw(&mut payload, call_span)?;
                let stat = indexed_optional_raw(&mut payload, call_span)?;
                let hidden = indexed_optional_raw(&mut payload, call_span)?;
                let exts = indexed_optional_raw(&mut payload, call_span)?;
                let result_wrapped = indexed_decode::<bool>(&mut payload, execution, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let operation = if tag == FullTag::ExprFsFiles {
                    "fs.files"
                } else {
                    "fs.walk"
                };
                let root = match self.eval_indexed_expr(execution, root, slots, span)? {
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
                let gitignore =
                    match self.eval_indexed_optional_expr(execution, gitignore, slots, span)? {
                        ControlFlow::Continue(value) => {
                            lowered_bool_arg_or(value, true, operation, span)?
                        }
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    };
                let stat = match self.eval_indexed_optional_expr(execution, stat, slots, span)? {
                    ControlFlow::Continue(value) => {
                        lowered_bool_arg_or(value, true, operation, span)?
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
                let hidden =
                    match self.eval_indexed_optional_expr(execution, hidden, slots, span)? {
                        ControlFlow::Continue(value) => {
                            lowered_bool_arg_or(value, false, operation, span)?
                        }
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
                let _op = indexed_decode::<RuntimeOp>(&mut payload, execution, call_span)?;
                let path = indexed_raw(&mut payload, call_span)?;
                let stat = indexed_optional_raw(&mut payload, call_span)?;
                let ordered = indexed_optional_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let operation = "fs.children";
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
                ControlFlow::Continue(super::lowered_runtime_stream_result(
                    fs_module::list_filesystem(self.host_path(&path), stat, ordered, span),
                    span,
                )?)
            }
            FullTag::ExprFsTempDir => {
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                ControlFlow::Continue(match new_temp_fs_root("fs-temp-dir", span) {
                    Ok(root) => {
                        lowered_result_ok(self.push_lowered_fs_root(root))
                    }
                    Err(error) => lowered_result_err_value(error),
                })
            }
            FullTag::ExprPathWrite => {
                let path = indexed_raw(&mut payload, call_span)?;
                let data = indexed_raw(&mut payload, call_span)?;
                let atomic = indexed_decode::<bool>(&mut payload, execution, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let path = match self.eval_indexed_expr(execution, path, slots, span)? {
                    ControlFlow::Continue(value) => lowered_path_arg(value, "write", span)?,
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
                        ControlFlow::Continue(value) => lowered_bool_arg_or(
                            value,
                            xsh_registry::signature::REMOVE_MISSING_OK_DEFAULT,
                            operation,
                            span,
                        )?,
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    };
                ControlFlow::Continue(lowered_unit_result(crate::modules::fs::remove_path(
                    self.host_path(&path),
                    missing_ok,
                    span,
                )))
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
                    match read_host_path_text(&self.host_path(&path), span) {
                        Ok(text) => {
                            LoweredValue::ResultOk(Box::new(LoweredValue::Str(text.into())))
                        }
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
            FullTag::ExprModuleCall => {
                let (op, cli_plan, args, span) =
                    decode_module_call(execution, &mut payload, call_span)?;
                indexed_finish(payload, call_span)?;
                let mut values = Vec::with_capacity(args.len());
                for arg in args {
                    let Some(arg) = arg else {
                        values.push(None);
                        continue;
                    };
                    match self.eval_indexed_expr(execution, arg, slots, span)? {
                        ControlFlow::Continue(value) => values.push(Some(value)),
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    }
                }
                let values = super::NativeArgumentValues::new(values);
                return self.eval_indexed_module_call_values(op, values, span, cli_plan.as_deref());
            }
            FullTag::ExprProcessCommandArgv => {
                let target = indexed_raw(&mut payload, call_span)?;
                let argv = indexed_raw(&mut payload, call_span)?;
                let mut optional = [None; 14];
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
                    accept,
                    same_group,
                ]: [Option<LoweredValue>; 14] = evaluated
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
                    accept,
                    same_group,
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
                let mut accepted_exit_codes = None;
                let mut detach = None;
                let mut new_session = None;
                let mut ignore_hup = None;
                let mut same_group = None;
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
                                "accept" => {
                                    accepted_exit_codes =
                                        Some(super::lowered_accepted_exit_codes(value, span)?);
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
                                "same_group" => {
                                    same_group =
                                        Some(lowered_bool_builder_field(value, "same_group", span)?)
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
                            accept: run_accept,
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
                            let (target_value, mut argv) =
                                run_target_and_leading_argv(&target, target_items)?;
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
                                run_env.insert(String::from_utf8_lossy(&name).into_owned(), value);
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
                            let run_accept = match self
                                .eval_indexed_optional_expr(execution, run_accept, slots, span)?
                            {
                                ControlFlow::Continue(value) => value
                                    .map(|value| super::lowered_accepted_exit_codes(value, span))
                                    .transpose()?,
                                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                            };
                            plan = Some(CommandPlan {
                                target: target_value,
                                argv,
                                cwd: None,
                                env: run_env,
                                redirections: Vec::new(),
                                timeout: run_timeout,
                                cpu_max: run_cpu_max,
                                accepted_exit_codes: run_accept,
                                detach: false,
                                new_session: false,
                                ignore_hup: false,
                                same_group: false,
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
                if accepted_exit_codes.is_some() {
                    if plan.accepted_exit_codes.is_some() {
                        return Err(RuntimeError::new(
                            "accept-policy",
                            "accept cannot be supplied both as a field and a run option",
                        )
                        .with_span(span));
                    }
                    plan.accepted_exit_codes = accepted_exit_codes;
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
                if let Some(value) = same_group {
                    plan.same_group = value;
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
                        segment.accept,
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
                let head = segments
                    .first()
                    .map_or(RunKind::Plain, |segment| segment.kind);
                if !matches!(head, RunKind::Plain | RunKind::Status) {
                    return self.eval_capture_pipeline(head, &invocations, span);
                }
                let mut end = match run_pipeline_inherit_with_policy(&invocations, self) {
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
                let validation_error = end.status.as_ref().and_then(|status| {
                    crate::runtime::run::run_completion_error(status, &invocations, propagate)
                });
                end.error = validation_error.clone();
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
                if let Some(error) = validation_error {
                    let value = lowered_process_run_error(error.with_span(span));
                    if invocations
                        .iter()
                        .any(|invocation| invocation.accepted_exit_codes.is_some())
                    {
                        let value = self.lowered_question_propagation_value(value, span)?;
                        return Ok(
                            self.preserve_lexical_expression_flow(StmtFlow::Propagate(value))
                        );
                    }
                    ControlFlow::Continue(value)
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
                let accept = indexed_optional_raw(&mut payload, call_span)?;
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
                    accept,
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
                if invocation.accepted_exit_codes.is_some()
                    && matches!(kind, RunKind::StreamText | RunKind::StreamBytes)
                {
                    let value = self.start_policy_process_stream(
                        &invocation,
                        kind == RunKind::StreamText,
                        span,
                    )?;
                    if propagate && matches!(value, LoweredValue::ResultErr(_)) {
                        let value = self.lowered_question_propagation_value(value, span)?;
                        return Ok(
                            self.preserve_lexical_expression_flow(StmtFlow::Propagate(value))
                        );
                    }
                    return Ok(ControlFlow::Continue(value));
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
                if (propagate
                    || (matches!(kind, RunKind::Status | RunKind::Plain)
                        && invocation.accepted_exit_codes.is_some()))
                    && matches!(value, LoweredValue::ResultErr(_))
                {
                    let value = self.lowered_question_propagation_value(value, span)?;
                    return Ok(self.preserve_lexical_expression_flow(StmtFlow::Propagate(value)));
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
                    same_group: plan.same_group,
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
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let status = match self.eval_indexed_expr(execution, status, slots, span)? {
                    ControlFlow::Continue(LoweredValue::Int(value)) => exit_status(value, span)?,
                    ControlFlow::Continue(value) => {
                        return Err(RuntimeError::new(
                            "type-error",
                            format!("exit status expected Int, found {}", value.type_name()),
                        )
                        .with_span(span));
                    }
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                // A script's own exit always unwinds; only a forced harness
                // cancellation skips cleanup.
                return Err(RuntimeError::abort(status, false).with_span(span));
            }
            FullTag::ExprFail => {
                let message = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let message = match self.eval_indexed_expr(execution, message, slots, span)? {
                    ControlFlow::Continue(value) => match lowered_str_value(&value) {
                        Some(message) => message.to_string(),
                        None => {
                            return Err(RuntimeError::new(
                                "type-error",
                                format!("error.fail expected Str, found {}", value.type_name()),
                            )
                            .with_span(span));
                        }
                    },
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                return Ok(ControlFlow::Continue(LoweredValue::ResultErr(Box::new(
                    error_constructor("validation", message),
                ))));
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
            FullTag::ExprProcCallResult => {
                let value = indexed_raw(&mut payload, call_span)?;
                indexed_finish(payload, call_span)?;
                let value = match self.eval_indexed_expr(execution, value, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                ControlFlow::Continue(proc_call_result(value))
            }
            FullTag::ExprErr => {
                let value = indexed_raw(&mut payload, call_span)?;
                let cause = indexed_optional_raw(&mut payload, call_span)?;
                indexed_finish(payload, call_span)?;
                let value = match self.eval_indexed_expr(execution, value, slots, call_span)? {
                    ControlFlow::Continue(value) => value.into_value(),
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let cause = match cause {
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
                // The propagation's own place: a traceback that starts here
                // names it, not the call that is running.
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                // The `?` reuses only a traceback that its operand records.
                self.pending_traceback = None;
                return match self.eval_indexed_expr(execution, value, slots, call_span)? {
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
            // Operands evaluate here, in order; the callee runs on the frames.
            FullTag::ExprCall
            | FullTag::ExprSelfCall
            | FullTag::ExprDirectPureCall
            | FullTag::ExprExternalCall
            | FullTag::ExprDynamicCall
            | FullTag::ExprTypedCall => {
                let named = match tag {
                    FullTag::ExprCall | FullTag::ExprDirectPureCall => {
                        Some(indexed_decode::<LoweredFunctionKey>(
                            &mut payload,
                            execution,
                            call_span,
                        )?)
                    }
                    FullTag::ExprExternalCall => {
                        Some(LoweredFunctionKey::Qualified(indexed_decode::<
                            QualifiedName,
                        >(
                            &mut payload,
                            execution,
                            call_span,
                        )?))
                    }
                    FullTag::ExprSelfCall => Some(
                        execution
                            .function_identity()
                            .map_err(|error| indexed_error(error, call_span))?
                            .0,
                    ),
                    _ => None,
                };
                let dynamic = named
                    .is_none()
                    .then(|| indexed_raw(&mut payload, call_span))
                    .transpose()?;
                let typed_pure = if tag == FullTag::ExprTypedCall {
                    let pure = indexed_decode::<bool>(&mut payload, execution, call_span)?;
                    // The signature is for the verifier; execution binds the
                    // verified arguments by position.
                    indexed_raw(&mut payload, call_span)?;
                    Some(pure)
                } else {
                    None
                };
                let mut args = self.frame_scratch.take_call_args();
                explicit_run::decode_call_args_into(execution, &mut payload, call_span, &mut args)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let callee = match (named, dynamic) {
                    (Some(function), _) => IndexedCallee::Named(function),
                    (None, Some(instruction)) => {
                        match self.eval_indexed_expr(execution, instruction, slots, span)? {
                            ControlFlow::Continue(value) => {
                                if let Some(pure) = typed_pure {
                                    indexed_typed_callee_kind(&value, pure, span)?;
                                }
                                IndexedCallee::Value(value)
                            }
                            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                        }
                    }
                    (None, None) => unreachable!("a dynamic call decodes its callee"),
                };
                let mut values = Vec::with_capacity(args.len());
                for &(kind, arg) in &args {
                    if kind == 2 {
                        values.push(callee.argument_default(self, arg as usize, span)?);
                        continue;
                    }
                    match self.eval_indexed_expr(execution, arg, slots, span)? {
                        ControlFlow::Continue(value) => {
                            append_call_argument(&mut values, kind, value, span)?
                        }
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    }
                }
                self.frame_scratch.recycle_call_args(args);
                let function = match callee {
                    IndexedCallee::Value(value) => indexed_callable_identity(&value, span)?.0,
                    IndexedCallee::Named(function) => function,
                };
                return match function {
                    LoweredFunctionKey::Qualified(qualified)
                        if matches!(
                            tag,
                            FullTag::ExprExternalCall
                                | FullTag::ExprDynamicCall
                                | FullTag::ExprTypedCall
                        ) =>
                    {
                        self.eval_indexed_external_call(qualified, &values, span)
                    }
                    function => self.eval_indexed_named_call(function, &values, span),
                }
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

    /// Runs a statement the heap-backed frames delegate here, publishing root
    /// slots around it like an expression.
    fn eval_indexed_stmt(
        &mut self,
        execution: &FullExecution<'_>,
        instruction: u32,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<StmtFlow, RuntimeError> {
        self.sync_indexed_root_slots(slots, call_span)?;
        let result = self.eval_indexed_stmt_inner(execution, instruction, slots, call_span);
        let publication = self.sync_indexed_root_slots(slots, call_span);
        let result = match (result, publication) {
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(error),
            (Ok(flow), Ok(())) => Ok(flow),
        };
        match (result, self.pending_value_block_flow.take()) {
            (Ok(_), Some(flow)) => Ok(flow),
            (result, _) => result,
        }
    }

    fn eval_indexed_stmt_inner(
        &mut self,
        execution: &FullExecution<'_>,
        instruction: u32,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<StmtFlow, RuntimeError> {
        let (tag, mut payload) = indexed_value(execution.instruction_id(instruction), call_span)?;
        match tag {
            FullTag::StmtAssignField | FullTag::StmtAssignFieldInt => {
                let typed = tag == FullTag::StmtAssignFieldInt;
                let slot = indexed_decode::<usize>(&mut payload, execution, call_span)?;
                let field = indexed_decode::<Arc<str>>(&mut payload, execution, call_span)?;
                let op = indexed_decode::<AssignOp>(&mut payload, execution, call_span)?;
                let value = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let (value, singleton) =
                    indexed_assignment_operand(execution, value, op, call_span)?;
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
                self.check_lent_context_assignment(slots, slot, &value, span)?;
                let current = super::super::lowered_ops::lowered_record_field_mut(
                    &mut slots[slot],
                    Name::intern(field.as_ref()),
                    span,
                )?;
                *current = apply_indexed_assignment(current, op, value, singleton, span)?;
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
                slots[slot] = LoweredValue::Int(checked_int_binary(
                    match op {
                        AssignOp::Add => BinaryOp::Add,
                        AssignOp::Sub => BinaryOp::Sub,
                        AssignOp::Mul => BinaryOp::Mul,
                        AssignOp::Div => BinaryOp::Div,
                        AssignOp::Rem => BinaryOp::Rem,
                        AssignOp::Set => unreachable!(),
                    },
                    current,
                    value,
                    span,
                )?);
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
            FullTag::StmtAssert => {
                let value = indexed_raw(&mut payload, call_span)?;
                let message = match indexed_raw(&mut payload, call_span)? {
                    0 => None,
                    1 => Some(indexed_raw(&mut payload, call_span)?),
                    _ => {
                        return Err(RuntimeError::new(
                            "indexed-ir",
                            "invalid assertion message option",
                        )
                        .with_span(call_span));
                    }
                };
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let failure = match self.eval_indexed_assertion(execution, value, slots, span)? {
                    ControlFlow::Continue(None) => return Ok(StmtFlow::None),
                    ControlFlow::Continue(Some(failure)) => failure,
                    ControlFlow::Break(value) => return Ok(StmtFlow::Propagate(value)),
                };
                let mut context = None;
                if let Some(message) = message {
                    let value = match self.eval_indexed_expr(execution, message, slots, span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(StmtFlow::Propagate(value)),
                    };
                    context = Some(match value {
                        LoweredValue::Str(text) => bounded_assertion_text(&text, 1024),
                        LoweredValue::StrView(text) => bounded_assertion_text(text.as_str(), 1024),
                        _ => {
                            return Err(RuntimeError::new(
                                "type-error",
                                "assert message requires Str",
                            )
                            .with_span(span));
                        }
                    });
                }
                Ok(StmtFlow::Propagate(self.indexed_assertion_failed(
                    failure,
                    context.as_deref(),
                    span,
                )?))
            }
            FullTag::StmtPatternIf => {
                let (_, mut branches) = execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, call_span))?;
                let count = indexed_raw(&mut branches, call_span)? as usize;
                let mut decoded = Vec::with_capacity(count);
                for _ in 0..count {
                    let condition = indexed_raw(&mut branches, call_span)?;
                    let body = indexed_raw(&mut branches, call_span)?;
                    let captures =
                        indexed_decode::<Vec<usize>>(&mut branches, execution, call_span)?;
                    decoded.push((condition, body, captures));
                }
                indexed_finish(branches, call_span)?;
                let else_body = indexed_optional_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                for (condition, body, captures) in decoded {
                    let scope_id = self.enter_owned_host_scope();
                    let mut selected = false;
                    let result = (|| {
                        match self.eval_indexed_bool(execution, condition, slots, span)? {
                            ControlFlow::Continue(false) => return Ok(StmtFlow::None),
                            ControlFlow::Break(value) => return Ok(StmtFlow::Propagate(value)),
                            ControlFlow::Continue(true) => {
                                selected = true;
                            }
                        }
                        self.eval_indexed_statement_block(execution, body, slots, span)
                    })();
                    let flow =
                        self.finish_indexed_pattern_scope(scope_id, &captures, slots, result)?;
                    if selected || !matches!(flow, StmtFlow::None) {
                        return Ok(flow);
                    }
                }
                match else_body {
                    Some(body) => self.eval_indexed_statement_block(execution, body, slots, span),
                    None => Ok(StmtFlow::None),
                }
            }
            FullTag::StmtPatternWhile => {
                let condition = indexed_raw(&mut payload, call_span)?;
                let body = indexed_raw(&mut payload, call_span)?;
                let captures = indexed_decode::<Vec<usize>>(&mut payload, execution, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                loop {
                    self.service_pending_signal(span)?;
                    if self.shutting_down() {
                        return Ok(StmtFlow::None);
                    }
                    let scope_id = self.enter_owned_host_scope();
                    let result = (|| {
                        match self.eval_indexed_bool(execution, condition, slots, span)? {
                            ControlFlow::Continue(false) => return Ok(StmtFlow::Break(None)),
                            ControlFlow::Break(value) => return Ok(StmtFlow::Propagate(value)),
                            ControlFlow::Continue(true) => {}
                        }
                        self.eval_indexed_statement_block(execution, body, slots, span)
                    })();
                    match self.finish_indexed_pattern_scope(scope_id, &captures, slots, result)? {
                        StmtFlow::None | StmtFlow::Continue => {}
                        StmtFlow::Break(_) => break,
                        flow => return Ok(flow),
                    }
                }
                Ok(StmtFlow::None)
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
                    return self.eval_indexed_statement_block(execution, *body, slots, call_span);
                }
                if let Some(body) = fallback {
                    return self.eval_indexed_statement_block(execution, body, slots, call_span);
                }
                Err(lowered_match_no_arm(span))
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
                        if self.shutting_down() {
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
                        if self.shutting_down() {
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
                match fs_module::cd_target_is_dir(&next) {
                    Ok(true) => {}
                    Ok(false) => {
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
                                RuntimeError::host("cwd", &error).with_span(span),
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
                let result = self.eval_indexed_statement_block(execution, body, slots, call_span);
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
                let result = self.eval_indexed_statement_block(execution, body, slots, call_span);
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
                            self.trace_leaf(
                                TraceKind::ResultPropagate,
                                Some(span),
                                None,
                                TracePayload::ResultPropagate {
                                    error: TraceError::caused_from_value(&error),
                                    error_kind: kind.clone(),
                                },
                            );
                            let _traceback =
                                self.pending_traceback.take().unwrap_or_else(|| Traceback {
                                    failing_span: Some(span),
                                    exe_path: self.exe_path.clone(),
                                    operation_kind: "result.propagate".to_string(),
                                    error: TraceError::from_propagated_value(&error),
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
                                    // The statement carries no span of its own; the failing
                                    // command's span locates the traceback, not the call
                                    // site of the enclosing function.
                                    let span = match &value {
                                        LoweredValue::ResultErr(error) => match error.as_ref() {
                                            Value::RunError(error) => error.span,
                                            Value::Error(error) => error.span,
                                            _ => None,
                                        },
                                        _ => None,
                                    }
                                    .unwrap_or(call_span);
                                    let value =
                                        self.lowered_question_propagation_value(value, span)?;
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
            _ => Err(RuntimeError::new(
                "indexed-ir",
                format!("direct indexed evaluator does not support {tag:?}"),
            )
            .with_span(call_span)),
        }
    }

}

enum ResolvedAssignStep {
    Field(Name),
    Map(MapKey),
    List(i64),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::check::Checker;
    use crate::source::SourceMap;
    use crate::syntax::parser::Parser;

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
        Checker::check_compact_declarations(&parsed.arena);
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
