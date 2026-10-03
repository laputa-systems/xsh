use crate::runtime::eval::lowered_run::validate_parameter_default;
use crate::runtime::eval::lowered_ops::lowered_record_update_batch;
use crate::map_key::MapKey;
use super::{
    Arc, AssignOp, BTreeMap, BinaryOp, Binding, CommandPlan, ControlFlow, Duration, DurationValue,
    Evaluator, FileRedirectionMode, Flow, FormatSpec, FunctionHeader, FunctionName,
    LoweredScalarCursor, LoweredMapCursor, LoweredCompTarget, LoweredFunctionKey, LoweredFunctionKind, LoweredModuleExportKind,
    LoweredProjectedReduceState, LoweredReduceProjection, LoweredRetryAttemptValue,
    LoweredReturnKind, LoweredStrPredicate, LoweredTagValue, LoweredType, LoweredValue, Name,
    PathValue, ProcessEnd, ProcessInvocation, ProcessRedirection, ProcessStatus, QualifiedName,
    RecordMap, RedirectionKind, RedirectionStream, ReduceByOp, RegexValue, RunError, RunKind,
    RuntimeError, RuntimeOp, ScanCondition, Span, SpawnOptions, StmtFlow, StreamValue, TraceArg,
    TraceError, TraceKind, TracePayload, Traceback, TracebackFrame, TracebackFrameKind, Type,
    Value, api_spec, assign_lowered_bytes_view, assign_lowered_str_view, bind_lowered_comp_target,
    btree_map, bytes_contains, bytes_module, check_env_name, checked_int_binary,
    compare_lowered_sort_keys, compound_assignment_value, error_constructor,
    execute_run_with_policy, exit_status, fs_module, json_module,
    append_lowered_list_element, append_lowered_map_literal, lowered_map_literal_key, lowered_assign_value, lowered_binary_value, lowered_bool_arg_or, lowered_bool_builder_field,
    lowered_bytes_or_str_owned, lowered_bytes_parts, lowered_bytes_value,
    lowered_command_plan_value, lowered_command_redirections, lowered_contains_value,
    lowered_count_key, lowered_duration_arg, lowered_encode_json, lowered_env_record_arg,
    lowered_error_message, lowered_freeze_large_slot_list,
    lowered_index_value, lowered_inline_stats_field_value, lowered_inline_stats_to_record_vec,
    lowered_int_arg, lowered_match_no_arm, lowered_nonnegative_count, lowered_parse_command_values,
    lowered_path_arg, lowered_path_from_value, lowered_path_like_arg, lowered_path_list_arg,
    lowered_path_method_value, lowered_pipeline_input, lowered_pipeline_item_count,
    lowered_pipeline_record_list, lowered_process_run_error, lowered_record_field_value,
    lowered_record_vec_append_or_replace_unsorted, lowered_record_vec_get,
    lowered_record_vec_or_stats, lowered_reduce_fields_owned,
    lowered_reduce_group_insert, lowered_reduce_key_value_owned, lowered_result_err_value,
    lowered_result_ok, lowered_slice_value,
    lowered_sort_key_orderable, lowered_splice_arg_items, lowered_stats_field_value,
    lowered_status_segment_record, lowered_stmt_flow_to_flow,
    lowered_str_arg_owned, lowered_str_byte_at_value, lowered_str_byte_len_value,
    lowered_str_count_lines_value, lowered_str_key, lowered_str_list_arg, lowered_str_parts,
    lowered_str_predicate_text, lowered_str_predicate_value, lowered_str_value,
    lowered_str_view_value, lowered_table_print_value, lowered_tag_key,
    lowered_trace_error_from_value, lowered_trim_is_empty_value, lowered_trim_str_predicate_value,
    lowered_type_name, lowered_unit_result, lowered_value_argv_len, lowered_value_from_runtime,
    lowered_value_from_runtime_any, lowered_value_matches_static_type,
    new_temp_fs_root, path_bytes, push_lowered_display,
    push_lowered_fmt_value, push_lowered_native_fmt_value, read_host_path_bytes, read_host_path_bytes_vec,
    run_pipeline_inherit_with_policy, runtime_error_from_value, splice_to_argv,
    structured_error_constructor, value_matches_static_type, value_to_argv_bytes,
    with_indexed_eval_depth,
};
use crate::runtime::eval::indexed::IrVerifyError;
use crate::runtime::eval::indexed::full::{
    BLOCK_LIST, BLOCK_STATEMENTS, FullDriverTag, FullExecution, FullFunctionView, FullPatternTag,
    FullPayload, FullProgram, FullStageTag, FullTag,
};
use crate::runtime::eval::lower::{lowered_error_value_has_facet, lowered_error_variant_matches, lowered_record_field};
use crate::runtime::eval::{
    LoweredModuleExport, LoweredTopLevelSlot, LoweredTypeCheck, Propagation, ScanBytes, ScanCheck,
    process_handle,
};
use smallvec::SmallVec;

pub(in crate::runtime::eval) mod explicit_run;
mod producer;
mod serial_pipeline;

use serial_pipeline::IndexedPipelineItems;

use xsh_registry::stream_parameters::DEFAULT_PAR_MAP_WORKERS;

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
    accept: Option<u32>,
}

// Diagnostics retain only bounded scalar text; reporting never traverses or
// materializes containers and never reevaluates an operand.
fn assertion_operand_text(value: &LoweredValue, span: Span) -> String {
    match value {
        LoweredValue::Str(value) => bounded_assertion_text(value, 160),
        LoweredValue::StrView(value) => bounded_assertion_text(value.as_str(), 160),
        LoweredValue::Path(value) => {
            let bytes = &value.bytes[..value.bytes.len().min(640)];
            let mut text = bounded_assertion_text(&String::from_utf8_lossy(bytes), 160);
            if bytes.len() < value.bytes.len() && !text.ends_with('…') { text.push('…'); }
            text
        }
        LoweredValue::Error(value) => match value.as_ref() {
            Value::Error(error) => bounded_assertion_text(&error.message, 160),
            _ => "<Error>".into(),
        },
        LoweredValue::Int(_) | LoweredValue::Float(_) | LoweredValue::Duration(_)
        | LoweredValue::Bool(_) | LoweredValue::Status(_) => {
            let mut text = String::new();
            if super::push_lowered_display(&mut text, value, span).is_err() {
                return format!("<{}>", value.type_name());
            }
            bounded_assertion_text(&text, 160)
        }
        _ => format!("<{}>", value.type_name()),
    }
}

fn bounded_assertion_text(text: &str, limit: usize) -> String {
    let mut chars = text.chars();
    let mut output: String = chars.by_ref().take(limit).collect();
    if chars.next().is_some() { output.push('…'); }
    output
}

// Assertion failures are checked language errors, so retry and local capture
// handle their nominal payload through the ordinary propagation boundary.
fn checked_assertion_failure(message: impl Into<String>, span: Span) -> RuntimeError {
    let mut error = crate::runtime::eval::modules::assertion_error(message, Some(span));
    error.propagated = true;
    error
}

fn comparison_chain_assertion_failure(op: BinaryOp, left: &LoweredValue, right: &LoweredValue, span: Span) -> Result<RuntimeError, RuntimeError> {
    Ok(checked_assertion_failure(comparison_failure_text(op, left, right, span), span))
}

fn comparison_failure_text(op: BinaryOp, left: &LoweredValue, right: &LoweredValue, span: Span) -> String {
    let (left_text, right_text) = if matches!(op, BinaryOp::In | BinaryOp::NotIn) {
        use crate::runtime::eval::lowered_ops::lowered_assertion_value_detail;
        (
            bounded_assertion_text(&lowered_assertion_value_detail(left), 160),
            bounded_assertion_text(&lowered_assertion_value_detail(right), 160),
        )
    } else {
        (assertion_operand_text(left, span), assertion_operand_text(right, span))
    };
    let operator = match op { BinaryOp::Eq => "==", BinaryOp::Ne => "!=", BinaryOp::Lt => "<", BinaryOp::Le => "<=", BinaryOp::Gt => ">", BinaryOp::Ge => ">=", BinaryOp::In => "in", BinaryOp::NotIn => "not in", _ => unreachable!() };
    let label = if matches!(op, BinaryOp::Eq | BinaryOp::Ne) { "comparison" } else if matches!(op, BinaryOp::In | BinaryOp::NotIn) { "membership comparison" } else { "ordering comparison" };
    format!("{label} failed: {left_text} {operator} {right_text}")
}

// The one formatter for bare Bool statements and `assert`: the expression,
// then the optional message, then the reached operands of the failure.
fn assertion_failure_message(
    expression: Option<&str>,
    operands: Option<(&LoweredValue, &LoweredValue)>,
    reached: Option<&str>,
    context: Option<&str>,
) -> String {
    let mut message = "assertion failed".to_string();
    if let Some(expression) = expression {
        message.push_str(": ");
        message.extend(expression.chars().take(512));
    }
    if let Some(context) = context {
        message.push_str(": ");
        message.push_str(context);
    } else if operands.is_none() && reached.is_none() {
        message.push_str(": evaluated to false");
    }
    if let Some(reached) = reached {
        message.push('\n');
        message.push_str(reached);
    }
    if let Some((left, right)) = operands {
        use crate::runtime::eval::lowered_ops::lowered_assertion_value_detail;
        message.push_str(&format!("\nleft: {}\nright: {}", lowered_assertion_value_detail(left), lowered_assertion_value_detail(right)));
        if let (Some(left), Some(right)) = (lowered_str_value(left), lowered_str_value(right)) {
            if left != right && left.len() <= 4096 && right.len() <= 4096 && (left.contains('\n') || right.contains('\n')) {
                message.push_str("\ndiff:\n");
                message.push_str(&diffy::create_patch(left, right).to_string());
            }
        }
    }
    message
}

enum AssertionFailure {
    Operands(LoweredValue, LoweredValue),
    Reached(String),
    False,
}

enum AssertionWork {
    Expr(u32),
    Left { op: BinaryOp, right: u32 },
    Right { op: BinaryOp, left_failure: Option<RuntimeError> },
}

fn assertion_comparison_op(op: BinaryOp) -> bool {
    matches!(op, BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge | BinaryOp::In | BinaryOp::NotIn)
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

// Only checked language failures cross a local Result boundary. Runtime faults
// and abort signals retain their original escape behavior.
fn capture_checked_error(mut error: RuntimeError) -> Result<LoweredValue, RuntimeError> {
    if error.abort.is_some() || !error.propagated {
        return Err(error);
    }
    error.propagated = false;
    let value = if let Some(mut original) = error.propagated_run_error.take() {
        original.contexts = error.contexts;
        original.cause = error.cause;
        Value::RunError(original)
    } else { Value::Error(Box::new(error)) };
    Ok(LoweredValue::ResultErr(Box::new(value)))
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

fn decode_record_updates<'a>(execution: &FullExecution<'a>, payload: &mut FullPayload<'a>, span: Span) -> Result<Vec<(Vec<Name>, u32, Span)>, RuntimeError> {
    let (_, mut entries) = execution.block(payload, BLOCK_LIST).map_err(|error| indexed_error(error, span))?;
    let count = indexed_raw(&mut entries, span)? as usize;
    let mut updates = Vec::with_capacity(count);
    for _ in 0..count {
        let path = indexed_decode::<Vec<Name>>(&mut entries, execution, span)?;
        let value = indexed_raw(&mut entries, span)?;
        let field_span = indexed_decode::<Span>(&mut entries, execution, span)?;
        updates.push((path, value, field_span));
    }
    indexed_finish(entries, span)?;
    Ok(updates)
}

/// One decoded entry of an aggregate literal's operand list.
trait IndexedOperand: Sized {
    fn decode<'a>(execution: &FullExecution<'a>, input: &mut FullPayload<'a>, build: bool, span: Span) -> Result<Self, RuntimeError>;
}

/// A list element: its instruction, whether it splices, and its span.
impl IndexedOperand for (u32, bool, Span) {
    #[inline]
    fn decode<'a>(execution: &FullExecution<'a>, input: &mut FullPayload<'a>, build: bool, span: Span) -> Result<Self, RuntimeError> {
        if !build {
            return Ok((indexed_raw(input, span)?, false, span));
        }
        let splice = indexed_decode::<bool>(input, execution, span)?;
        Ok((indexed_raw(input, span)?, splice, indexed_decode::<Span>(input, execution, span)?))
    }
}

/// A map literal entry: its optional computed key, value, and span.
impl IndexedOperand for (Option<u32>, u32, Span) {
    #[inline]
    fn decode<'a>(execution: &FullExecution<'a>, input: &mut FullPayload<'a>, _: bool, span: Span) -> Result<Self, RuntimeError> {
        Ok((indexed_optional_raw(input, span)?, indexed_raw(input, span)?, indexed_decode::<Span>(input, execution, span)?))
    }
}

impl IndexedOperand for IndexedRecordEntry {
    #[inline]
    fn decode<'a>(execution: &FullExecution<'a>, input: &mut FullPayload<'a>, _: bool, span: Span) -> Result<Self, RuntimeError> {
        Ok(match indexed_raw(input, span)? {
            0 => Self::Field { name: indexed_decode(input, execution, span)?, instruction: indexed_raw(input, span)? },
            1 => Self::Spread(indexed_raw(input, span)?),
            _ => return Err(RuntimeError::new("indexed-ir", "invalid indexed record entry").with_span(span)),
        })
    }
}

impl IndexedOperand for IndexedFmtPart {
    #[inline]
    fn decode<'a>(execution: &FullExecution<'a>, input: &mut FullPayload<'a>, _: bool, span: Span) -> Result<Self, RuntimeError> {
        Ok(match indexed_raw(input, span)? {
            0 => Self::Text(indexed_decode(input, execution, span)?),
            1 => Self::Expr(indexed_raw(input, span)?, indexed_decode(input, execution, span)?, indexed_decode(input, execution, span)?),
            _ => return Err(RuntimeError::new("indexed-ir", "invalid indexed format part").with_span(span)),
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
    fn new(execution: &'e FullExecution<'a>, payload: &mut FullPayload<'a>, build: bool, span: Span) -> Result<Self, RuntimeError> {
        let (_, mut input) = execution.block(payload, BLOCK_LIST).map_err(|error| indexed_error(error, span))?;
        let remaining = indexed_raw(&mut input, span)? as usize;
        Ok(Self { execution, input, remaining, build, span, entry: std::marker::PhantomData })
    }

    /// Decodes a list, map, or record literal, whose payload holds only the operand list.
    fn literal(execution: &'e FullExecution<'a>, mut payload: FullPayload<'a>, build: bool, span: Span) -> Result<Self, RuntimeError> {
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
        Ok(Some(T::decode(self.execution, &mut self.input, self.build, self.span)?))
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

/// Format parts and the accumulator, whose path span follows the parts.
fn fmt_operands<'e, 'a>(execution: &'e FullExecution<'a>, mut payload: FullPayload<'a>, path: bool, span: Span) -> Result<(IndexedOperands<'e, 'a, IndexedFmtPart>, IndexedFmt), RuntimeError> {
    let operands = IndexedOperands::new(execution, &mut payload, false, span)?;
    let path_span = if path { Some(indexed_decode::<Span>(&mut payload, execution, span)?) } else { None };
    indexed_finish(payload, span)?;
    Ok((operands, IndexedFmt { text: String::new(), native: Vec::new(), path_span }))
}

enum IndexedRecordEntry {
    Field { name: Name, instruction: u32 },
    Spread(u32),
}

impl IndexedRecordEntry {
    fn instruction(&self) -> u32 {
        match self { Self::Field { instruction, .. } | Self::Spread(instruction) => *instruction }
    }

    fn append(&self, fields: &mut Vec<(Name, LoweredValue)>, value: LoweredValue, span: Span) -> Result<(), RuntimeError> {
        if let Self::Field { name, .. } = self {
            lowered_record_vec_append_or_replace_unsorted(fields, *name, value);
            return Ok(());
        }
        match value {
            LoweredValue::Record(record) | LoweredValue::Module(record) => {
                for (key, value) in record.iter() {
                    lowered_record_vec_append_or_replace_unsorted(fields, Name::intern(key.as_ref()), value.clone());
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
}

fn finish_record_entries(mut fields: Vec<(Name, LoweredValue)>) -> LoweredValue {
    fields.sort_unstable_by_key(|(name, _)| *name);
    lowered_record_vec_or_stats(fields)
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
        if self.path_span.is_some() { self.native.extend_from_slice(text.as_bytes()) } else { self.text.push_str(text) }
    }

    fn push_value(&mut self, value: &LoweredValue, span: Span, spec: Option<&FormatSpec>) -> Result<(), RuntimeError> {
        if self.path_span.is_some() { push_lowered_native_fmt_value(&mut self.native, value, span, spec) }
        else { push_lowered_fmt_value(&mut self.text, value, span, spec) }
    }

    fn finish(self) -> Result<LoweredValue, RuntimeError> {
        match self.path_span {
            Some(span) => Ok(LoweredValue::Path(PathValue::new(self.native).map_err(|error| error.with_span(span))?)),
            None => Ok(LoweredValue::Str(self.text.into())),
        }
    }
}

#[derive(Clone)]
enum IndexedAssignStep {
    Field(Name),
    Index(u32),
}

fn decode_assign_path<'a>(execution: &FullExecution<'a>, payload: &mut FullPayload<'a>, span: Span) -> Result<Vec<IndexedAssignStep>, RuntimeError> {
    let (_, mut entries) = execution.block(payload, BLOCK_LIST).map_err(|error| indexed_error(error, span))?;
    let count = indexed_raw(&mut entries, span)?;
    let mut path = Vec::with_capacity(count as usize);
    for _ in 0..count {
        path.push(match indexed_raw(&mut entries, span)? {
            0 => IndexedAssignStep::Field(indexed_decode(&mut entries, execution, span)?),
            1 => IndexedAssignStep::Index(indexed_raw(&mut entries, span)?),
            _ => return Err(RuntimeError::new("indexed-ir", "invalid assignment path step").with_span(span)),
        });
    }
    indexed_finish(entries, span)?;
    Ok(path)
}

#[derive(Clone)]
enum ContextScopeRestore {
    Cwd { previous: std::path::PathBuf, span: Span },
    Env(super::super::RuntimeEnv),
}

#[derive(Clone)]
enum IndexedCompQualifier {
    For { target: LoweredCompTarget, iter: u32, span: Span },
    If { condition: u32, span: Span },
}

impl IndexedCompQualifier {
    fn span(&self) -> Span { match self { Self::For { span, .. } | Self::If { span, .. } => *span } }
}

fn decode_comp_qualifiers<'a>(execution: &FullExecution<'a>, payload: &mut FullPayload<'a>, span: Span) -> Result<Vec<IndexedCompQualifier>, RuntimeError> {
    let (_, mut entries) = execution.block(payload, BLOCK_LIST).map_err(|error| indexed_error(error, span))?;
    let count = indexed_raw(&mut entries, span)?;
    let mut qualifiers = Vec::new();
    for _ in 0..count {
        qualifiers.push(match indexed_raw(&mut entries, span)? {
            0 => IndexedCompQualifier::For { target: indexed_decode(&mut entries, execution, span)?, iter: indexed_raw(&mut entries, span)?, span: indexed_decode(&mut entries, execution, span)? },
            1 => IndexedCompQualifier::If { condition: indexed_raw(&mut entries, span)?, span: indexed_decode(&mut entries, execution, span)? },
            _ => return Err(RuntimeError::new("indexed-ir", "invalid comprehension qualifier").with_span(span)),
        });
    }
    indexed_finish(entries, span)?;
    if !matches!(qualifiers.first(), Some(IndexedCompQualifier::For { .. })) {
        return Err(RuntimeError::new("indexed-ir", "comprehension qualifiers must start with for").with_span(span));
    }
    Ok(qualifiers)
}

fn lowered_comp_iterable(value: LoweredValue, span: Span) -> Result<LoweredValue, RuntimeError> {
    match value {
        LoweredValue::ResultOk(value) => lowered_comp_iterable(*value, span),
        LoweredValue::ResultErr(error) => Err(super::runtime_error_from_value(*error, span)),
        value => Ok(value),
    }
}

impl Evaluator {
    fn enter_indexed_context_scope(&mut self, kind: crate::syntax::arena::ContextScopeKind, value: LoweredValue, span: Span) -> Result<ContextScopeRestore, RuntimeError> {
        match kind {
            crate::syntax::arena::ContextScopeKind::Cwd => {
                let target = lowered_path_like_arg(value, "cd", span)?;
                let previous = self.cwd.clone();
                let next = self.host_path(&target);
                match fs_module::cd_target_is_dir(&next) {
                    Ok(true) => {},
                    Ok(false) => return Err(RuntimeError::new("cwd-not-directory", "cwd target is not a directory").with_span(span)),
                    Err(error) => return Err(RuntimeError::new("cwd", error.to_string()).with_span(span)),
                }
                self.trace_enter(TraceKind::CwdEnter, Some(span), Some("cd"), TracePayload::Cwd {
                    previous: TraceArg::bytes(path_bytes(&previous)), current: TraceArg::bytes(path_bytes(&next)),
                });
                self.cwd = next;
                Ok(ContextScopeRestore::Cwd { previous, span })
            }
            crate::syntax::arena::ContextScopeKind::Env => {
                let fields = match value {
                    LoweredValue::Record(fields) => fields.iter().map(|(name, value)| (name.to_string(), value.clone())).collect::<Vec<_>>(),
                    LoweredValue::RecordVec(fields) => fields.iter().map(|(name, value)| (name.to_string(), value.clone())).collect(),
                    LoweredValue::Map(fields) => fields.iter().map(|(name, value)| {
                        let name = name.as_str().ok_or_else(|| RuntimeError::new("env-name", "environment overlay keys must be Str").with_span(span))?;
                        Ok((name.to_string(), value.clone()))
                    }).collect::<Result<Vec<_>, RuntimeError>>()?,
                    _ => return Err(RuntimeError::new("type-error", "environment overlay requires Record or string-keyed Map").with_span(span)),
                };
                let mut overlay = BTreeMap::new();
                for (name, value) in fields {
                    check_env_name(&name, span)?;
                    let value = super::super::value_to_argv_bytes(value.into_value(), span)?;
                    overlay.insert(name.into_bytes(), value);
                }
                let previous = self.env.clone();
                self.env.extend(overlay);
                Ok(ContextScopeRestore::Env(previous))
            }
        }
    }

    pub(in crate::runtime::eval) fn context_scope_runtime_value_escapes(value: &Value) -> bool {
        value.resource_reachable_values().any(|value|
            matches!(value, Value::Stream(_) | Value::ProcessHandle(_) | Value::NetJob(_)))
    }

    fn context_scope_runtime_error_escapes(error: &RuntimeError) -> bool {
        error.abort.is_none() && error.propagated
            && error.resource_reachable_values().any(|value| matches!(value,
                Value::Stream(_) | Value::ProcessHandle(_) | Value::NetJob(_)))
    }

    pub(in crate::runtime::eval) fn context_scope_value_escapes(value: &LoweredValue) -> bool {
        match value {
            LoweredValue::Stream(_) | LoweredValue::ProcessHandle(_) | LoweredValue::NetJob(_) => true,
            LoweredValue::List(items) => items.iter().any(Self::context_scope_value_escapes),
            LoweredValue::SharedList(items) => items.iter().any(Self::context_scope_value_escapes),
            LoweredValue::Map(fields) => fields.values().any(Self::context_scope_value_escapes),
            LoweredValue::Record(fields) | LoweredValue::Module(fields) => fields.values().any(Self::context_scope_value_escapes),
            LoweredValue::RecordVec(fields) => fields.iter().any(|(_, value)| Self::context_scope_value_escapes(value)),
            LoweredValue::Tag(tag) => tag.fields.iter().any(Self::context_scope_value_escapes),
            LoweredValue::ResultOk(value) => Self::context_scope_value_escapes(value),
            LoweredValue::Error(value) | LoweredValue::ResultErr(value) => Self::context_scope_runtime_value_escapes(value),
            _ => false,
        }
    }

    fn declare_recursive_context_slot(&mut self, slots: &[LoweredValue], slot: usize) {
        let identity = slots.as_ptr() as usize;
        for (owner, locals) in &mut self.recursive_context_slots {
            if *owner == identity { locals.insert(slot); }
        }
    }

    fn declare_recursive_context_target(&mut self, slots: &[LoweredValue], target: &LoweredCompTarget) {
        match target {
            LoweredCompTarget::Slot(slot) => self.declare_recursive_context_slot(slots, *slot),
            LoweredCompTarget::Record { fields } => {
                for (_, target, _) in fields { self.declare_recursive_context_target(slots, target); }
            }
            LoweredCompTarget::Discard => {}
        }
    }

    fn check_recursive_context_assignment(&self, slots: &[LoweredValue], slot: usize, value: &LoweredValue, span: Span) -> Result<(), RuntimeError> {
        if Self::context_scope_value_escapes(value) && self.recursive_context_slots.iter().any(|(owner, locals)|
            *owner == slots.as_ptr() as usize && !locals.contains(&slot)) {
            return Err(RuntimeError::new("context-scope-escape", "a live producer or host handle cannot escape through an outer assignment").with_span(span));
        }
        Ok(())
    }

    fn restore_indexed_context_scope(&mut self, restore: ContextScopeRestore) {
        match restore {
            ContextScopeRestore::Cwd { previous, span } => {
                let current = self.cwd.clone();
                self.cwd = previous.clone();
                self.trace_exit(TraceKind::CwdExit, Some(span), Some("cd"), TracePayload::Cwd {
                    previous: TraceArg::bytes(path_bytes(&current)), current: TraceArg::bytes(path_bytes(&previous)),
                });
            }
            ContextScopeRestore::Env(previous) => { self.env = previous; }
        }
    }

    /// The index `function`/`kind` resolves to inside `program`.
    ///
    /// The program is part of the cache key because one evaluator resolves the
    /// same qualified key against more than one program: a dynamically loaded
    /// module links its standard calls to the loading program's prepared
    /// implementations, so `<xsh-stdlib:hash> verify_file` names a function in
    /// both. The entry keeps the program alive, so its identity cannot be
    /// reused by a later allocation while the entry is cached.
    fn indexed_function_index(
        &mut self,
        program: &Arc<FullProgram>,
        function: LoweredFunctionKey,
        kind: LoweredFunctionKind,
    ) -> Result<Option<usize>, IrVerifyError> {
        let cache_key = (function, kind);
        if let Some((cached_program, index)) = self.indexed_function_cache.get(&cache_key)
            && Arc::ptr_eq(cached_program, program)
        {
            return Ok(Some(*index));
        }
        let view = program.function_view(function, kind)?;
        if let Some(view) = view {
            let index = view.index();
            self.indexed_function_cache
                .insert(cache_key, (Arc::clone(program), index));
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
            return_check: None,
            slot_count,
        }
    }

    // Expression transport retains the statement target separately from its payload.
    // In particular, callback returns and loop controls must cross retry and cleanup.
    fn indexed_driver_expression_escape(&mut self, value: LoweredValue, span: Span) -> Flow {
        match self.pending_value_block_flow.take().unwrap_or(StmtFlow::Propagate(value)) {
            StmtFlow::Propagate(value) => self.question_flow(value.into_value(), span),
            flow => lowered_stmt_flow_to_flow(flow),
        }
    }

    fn preserve_lexical_expression_flow<T>(&mut self, flow: StmtFlow) -> ControlFlow<LoweredValue, T> {
        let value = match &flow {
            StmtFlow::Value(value) | StmtFlow::Return(value) | StmtFlow::Propagate(value) => value.clone(),
            StmtFlow::Break(value) => value.clone().unwrap_or(LoweredValue::Unit),
            StmtFlow::Continue | StmtFlow::None => LoweredValue::Unit,
        };
        self.pending_value_block_flow = Some(flow);
        ControlFlow::Break(value)
    }

    /// Returned Result values stay in-band; only explicit propagation escapes.
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
    ) -> Result<LoweredValue, RuntimeError> {
        slots[slot] = item;
        let item_result = if let Some(body) = body {
            match self.eval_indexed_statement_block(execution, body, block_header, slots, span) {
                Ok(StmtFlow::None) => {
                    self.eval_indexed_expr(execution, value, slots, span)
                }
                Ok(flow) => Ok(self.preserve_lexical_expression_flow(flow)),
                Err(error) => Err(error),
            }
        } else {
            self.eval_indexed_expr(execution, value, slots, span)
        };
        let item_result = match item_result {
            Ok(ControlFlow::Continue(value)) => value,
            Ok(ControlFlow::Break(value)) => value,
            Err(error) => return Err(error),
        };
        if let Some(flow) = self.pending_value_block_flow.take() {
            if let StmtFlow::Propagate(LoweredValue::ResultErr(error)) = flow {
                let mut error = runtime_error_from_value(*error, span);
                error.propagated = true;
                return Err(error);
            }
            self.pending_value_block_flow = Some(flow);
            return Ok(item_result);
        }
        Ok(item_result)
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
                        let allocation_stage = crate::mem_track::begin_worker_stage();
                        let _symbols = symbols.enter();
                        let (mut worker, mut worker_slots) = {
                            let _setup = allocation_stage
                                .scope(crate::mem_track::WorkerAllocationScope::Setup);
                            (Evaluator::new_lowered_worker(shared), base_slots)
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
                                let result = worker.eval_indexed_par_map_item(
                                        &execution,
                                        body,
                                        value,
                                        &block_header,
                                        &mut worker_slots,
                                        slot,
                                        item,
                                        span,
                                    );
                                results.push((item_index, (result, worker.pending_value_block_flow.take())));
                            }
                        }
                        sender
                            .send((chunk_index, results, std::mem::take(&mut worker.stderr)))
                            .expect("lowered par-map receiver dropped");
                    })
                    .expect("failed to spawn lowered par-map worker");
                workers.push((chunk_index, worker));
            }
            drop(sender);
            let mut completed: Vec<
                Option<(Vec<(usize, (Result<LoweredValue, RuntimeError>, Option<StmtFlow>))>, Vec<u8>)>,
            > = (0..workers.len()).map(|_| None).collect();
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
            let mut ordered: Vec<Option<(Result<LoweredValue, RuntimeError>, Option<StmtFlow>)>> =
                (0..item_count).map(|_| None).collect();
            let mut stderr = Vec::new();
            for completed in completed {
                let (mut results, worker_stderr) = completed.expect("par-map worker missing");
                for (item_index, result) in results.drain(..) {
                    ordered[item_index] = Some(result);
                }
                stderr.extend(worker_stderr);
            }
            let mut results = Vec::with_capacity(item_count);
            for (item_index, result) in ordered.into_iter().enumerate() {
                let (result, flow) = result.expect("par-map result missing");
                let value = result.map_err(|error| self.stream_item_runtime_error("par-map", item_index, error))?;
                if let Some(flow) = flow {
                    self.pending_value_block_flow = Some(flow);
                    break;
                }
                results.push(value);
            }
            Ok((results, stderr))
        })?;
        self.stderr.extend(stderr);
        Ok(chunks)
    }

    // Fused workers use the same projection as the ordinary reduce-by handler:
    // simple record sums update accumulators without rebuilding each output
    // record.
    fn eval_indexed_reduce_rows(
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
        let block_header = Self::indexed_block_header(slots.len());
        for row in rows {
            if let Some(projection) = projection.as_mut() {
                self.eval_lowered_projected_reduce_by_item(projection, row, groups, span)?;
                continue;
            }
            slots[reduce_item_slot] = row;
            match self.eval_indexed_statement_block(
                execution,
                reduce_body,
                &block_header,
                slots,
                span,
            )? {
                StmtFlow::None => {}
                flow => {
                    self.pending_value_block_flow = Some(flow);
                    return Ok(());
                }
            }
            let output = match self.eval_indexed_expr(execution, reduce_value, slots, span)? {
                ControlFlow::Continue(value) => value,
                ControlFlow::Break(value) => {
                    if self.pending_value_block_flow.is_some() { return Ok(()); }
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
                        let allocation_stage = crate::mem_track::begin_worker_stage();
                        let _symbols = symbols.enter();
                        let (mut worker, mut worker_slots, mut groups) = {
                            let _setup = allocation_stage
                                .scope(crate::mem_track::WorkerAllocationScope::Setup);
                            (
                                Evaluator::new_lowered_worker(shared),
                                base_slots,
                                BTreeMap::new(),
                            )
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
                                let mapped = {
                                    let _item = allocation_stage
                                        .scope(crate::mem_track::WorkerAllocationScope::ParMapItem);
                                    worker.eval_indexed_par_map_item(
                                        &execution,
                                        body,
                                        value,
                                        &map_header,
                                        &mut worker_slots,
                                        slot,
                                        item,
                                        span,
                                    )?
                                };
                                if worker.pending_value_block_flow.is_some() { break; }
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
                                if worker.pending_value_block_flow.is_some() { break; }
                            }
                            Ok::<_, RuntimeError>(groups)
                        })();
                        (chunk_index, result, std::mem::take(&mut worker.stderr), worker.pending_value_block_flow.take())
                    })
                    .expect("failed to spawn fused par-map worker");
                workers.push(worker);
            }
            let mut completed: Vec<
                Option<(
                    Result<BTreeMap<String, LoweredValue>, RuntimeError>,
                    Vec<u8>,
                    Option<StmtFlow>,
                )>,
            > = (0..workers.len()).map(|_| None).collect();
            while !workers.is_empty() {
                let mut index = 0;
                let mut progress = false;
                while index < workers.len() {
                    if workers[index].is_finished() {
                        let worker = workers.swap_remove(index);
                        let (chunk_index, result, worker_stderr, flow) =
                            worker.join().expect("fused par-map worker thread panicked");
                        completed[chunk_index] = Some((result, worker_stderr, flow));
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
                .flat_map(|(_, stderr, _)| stderr.iter().copied()),
        );
        let mut groups = BTreeMap::new();
        for completed in completed {
            let (result, _, flow) = completed.expect("fused par-map worker missing");
            let result = result?;
            if let Some(flow) = flow {
                self.pending_value_block_flow = Some(flow);
                break;
            }
            for (key, value) in result {
                lowered_reduce_group_insert(&mut groups, key, value, op, span)?;
            }
        }
        Ok(LoweredValue::Map(Arc::new(groups.into_iter().map(|(key, value)| (MapKey::from(key), value)).collect())))
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
                accept: indexed_optional_raw(&mut values, span)?,
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
                    accept: indexed_optional_raw(&mut values, span)?,
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
            let value = match self.eval_indexed_expr(execution, redirection.target.value, slots, redirection.span)? {
                ControlFlow::Continue(value) => value,
                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
            };
            if let LoweredValue::Bytes(bytes) = value {
                if redirection.kind != RedirectionKind::StdinRead || redirection.target.mode == 2 {
                    return Err(RuntimeError::new("redirection-target", "Bytes are only valid as a single stdin input").with_span(redirection.span));
                }
                out.push(ProcessRedirection::Input { bytes });
                continue;
            }
            let value = value.into_value();
            let target = match redirection.target.mode {
                2 => splice_to_argv(value, redirection.target.span)?,
                1 if matches!(value, Value::List(_)) => splice_to_argv(value, redirection.target.span)?,
                _ => vec![value_to_argv_bytes(value, redirection.target.span)?],
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
        accept: Option<u32>,
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
        let accepted_exit_codes = match self.eval_indexed_optional_expr(execution, accept, slots, span)? {
            ControlFlow::Continue(value) => value.map(|value| super::lowered_accepted_exit_codes(value, span)).transpose()?,
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
            accepted_exit_codes,
        }))
    }

    fn decode_indexed_pattern_fields<'a>(payload: &mut FullPayload<'a>, execution: &FullExecution<'a>, span: Span) -> Result<Vec<(Name, u32)>, RuntimeError> {
        let (_, mut fields) = execution.block(payload, BLOCK_LIST).map_err(|error| indexed_error(error, span))?;
        let count = indexed_raw(&mut fields, span)? as usize;
        let mut decoded = Vec::with_capacity(count);
        for _ in 0..count {
            decoded.push((indexed_decode::<Name>(&mut fields, execution, span)?, indexed_raw(&mut fields, span)?));
        }
        indexed_finish(fields, span)?;
        Ok(decoded)
    }

    // Structural validation precedes capture publication throughout the pattern tree.
    // Failed nested patterns leave every capture slot untouched and allocate no list rest.
    pub(in crate::runtime::eval) fn indexed_pattern_matches(
        execution: &FullExecution<'_>, pattern: u32, value: &LoweredValue,
        slots: &mut [LoweredValue], span: Span,
    ) -> Result<bool, RuntimeError> {
        if !Self::indexed_pattern_match_pass(execution, pattern, value, slots, span, false)? {
            return Ok(false);
        }
        Self::indexed_pattern_match_pass(execution, pattern, value, slots, span, true)
    }

    fn indexed_pattern_match_pass(
        execution: &FullExecution<'_>,
        pattern: u32,
        value: &LoweredValue,
        slots: &mut [LoweredValue],
        span: Span,
        bind: bool,
    ) -> Result<bool, RuntimeError> {
        let (tag, mut payload) = execution
            .pattern(pattern)
            .map_err(|error| indexed_error(error, span))?;
        let matched = match tag {
            FullPatternTag::TagType => {
                let type_name = indexed_decode::<Name>(&mut payload, execution, span)?;
                let variants = indexed_decode::<Vec<Name>>(&mut payload, execution, span)?;
                matches!(value, LoweredValue::Tag(value) if value.type_name == type_name && variants.iter().any(|name| value.name.as_ref() == name.as_str()))
            }
            FullPatternTag::RecordTest => {
                let fields = Self::decode_indexed_pattern_fields(&mut payload, execution, span)?;
                let mut matched = matches!(value, LoweredValue::Record(_) | LoweredValue::RecordVec(_));
                for (name, pattern) in fields.iter() {
                    let Some(field) = lowered_record_field(value, &name.as_str()) else { matched = false; break; };
                    if !Self::indexed_pattern_match_pass(execution, *pattern, field, slots, span, bind)? { matched = false; break; }
                }
                matched
            }
            FullPatternTag::ResultTest => {
                let ok = indexed_decode::<bool>(&mut payload, execution, span)?;
                let inner = indexed_raw(&mut payload, span)?;
                match value {
                    LoweredValue::ResultOk(value) if ok => Self::indexed_pattern_match_pass(execution, inner, value, slots, span, bind)?,
                    LoweredValue::ResultErr(value) if !ok => {
                        if let Some(value) = lowered_value_from_runtime_any(value) {
                            Self::indexed_pattern_match_pass(execution, inner, &value, slots, span, bind)?
                        } else { false }
                    }
                    _ => false,
                }
            }
            FullPatternTag::TagTest => {
                let type_name = indexed_decode::<Name>(&mut payload, execution, span)?;
                let name = indexed_decode::<Name>(&mut payload, execution, span)?;
                let (_, mut patterns) = execution.block(&mut payload, BLOCK_LIST).map_err(|error| indexed_error(error, span))?;
                let count = indexed_raw(&mut patterns, span)? as usize;
                let mut fields = Vec::with_capacity(count);
                for _ in 0..count { fields.push(indexed_raw(&mut patterns, span)?); }
                indexed_finish(patterns, span)?;
                if let LoweredValue::Tag(value) = value {
                    let mut matched = value.type_name == type_name && value.name.as_ref() == name.as_str() && value.fields.len() == fields.len();
                    if matched {
                        for (pattern, value) in fields.iter().zip(&value.fields) {
                            if !Self::indexed_pattern_match_pass(execution, *pattern, value, slots, span, bind)? { matched = false; break; }
                        }
                    }
                    matched
                } else { false }
            }
            FullPatternTag::ErrorTest => {
                let family = indexed_decode::<Name>(&mut payload, execution, span)?;
                let variant = indexed_decode::<Name>(&mut payload, execution, span)?;
                let fields = Self::decode_indexed_pattern_fields(&mut payload, execution, span)?;
                if let LoweredValue::Error(value) = value {
                    let error_fields = match value.as_ref() {
                        Value::Error(error) if error.family_name() == family && error.variant_name() == variant => Some(error.payload.clone()),
                        Value::RunError(error) if family == Name::PROCESS_ERROR && error.variant_name() == variant.as_str() => Some(error.payload()),
                        _ => None,
                    };
                    if let Some(values) = error_fields {
                        let mut matched = true;
                        for (name, pattern) in fields.iter() {
                            let Some(value) = values.get(&name.as_str()).and_then(lowered_value_from_runtime_any) else { matched = false; break; };
                            if !Self::indexed_pattern_match_pass(execution, *pattern, &value, slots, span, bind)? { matched = false; break; }
                        }
                        matched
                    } else { false }
                } else { false }
            }
            FullPatternTag::List => {
                let (_, mut patterns) = execution.block(&mut payload, BLOCK_LIST).map_err(|error| indexed_error(error, span))?;
                let count = indexed_raw(&mut patterns, span)? as usize;
                let mut elements = Vec::with_capacity(count);
                for _ in 0..count { elements.push(indexed_raw(&mut patterns, span)?); }
                indexed_finish(patterns, span)?;
                let rest = if indexed_decode::<bool>(&mut payload, execution, span)? { Some(indexed_raw(&mut payload, span)?) } else { None };
                let items = match value {
                    LoweredValue::List(items) => Some(items.as_slice()),
                    LoweredValue::SharedList(items) => Some(items.as_slice()),
                    _ => None,
                };
                if let Some(items) = items {
                    let mut matched = items.len() >= count && (rest.is_some() || items.len() == count);
                    if matched {
                        for (pattern, item) in elements.iter().zip(items) {
                            if !Self::indexed_pattern_match_pass(execution, *pattern, item, slots, span, bind)? { matched = false; break; }
                        }
                    }
                    if matched && bind && let Some(rest) = rest {
                        let (tag, mut rest_payload) = execution.pattern(rest).map_err(|error| indexed_error(error, span))?;
                        if tag == FullPatternTag::Bind {
                            let slot = indexed_decode::<usize>(&mut rest_payload, execution, span)?;
                            slots[slot] = LoweredValue::List(items[count..].to_vec());
                        }
                        indexed_finish(rest_payload, span)?;
                    }
                    matched
                } else { false }
            }
            FullPatternTag::Alias => {
                let pattern = indexed_raw(&mut payload, span)?;
                let slot = indexed_decode::<usize>(&mut payload, execution, span)?;
                let matched = Self::indexed_pattern_match_pass(execution, pattern, value, slots, span, bind)?;
                if matched && bind { slots[slot] = value.clone(); }
                matched
            }
            FullPatternTag::Alternation => {
                let (_, mut children) = execution.block(&mut payload, BLOCK_LIST).map_err(|error| indexed_error(error, span))?;
                let count = indexed_raw(&mut children, span)? as usize;
                let mut selected = None;
                for _ in 0..count {
                    let child = indexed_raw(&mut children, span)?;
                    if selected.is_none() && Self::indexed_pattern_match_pass(execution, child, value, slots, span, false)? { selected = Some(child); }
                }
                indexed_finish(children, span)?;
                if let Some(selected) = selected {
                    if bind { Self::indexed_pattern_match_pass(execution, selected, value, slots, span, true)? } else { true }
                } else { false }
            }
            FullPatternTag::Wildcard => true,
            FullPatternTag::Bind => {
                let slot = indexed_decode::<usize>(&mut payload, execution, span)?;
                if bind { slots[slot] = value.clone(); }
                true
            }
            FullPatternTag::Type => {
                let ty = indexed_decode::<Type>(&mut payload, execution, span)?;
                let slot = indexed_decode::<Option<usize>>(&mut payload, execution, span)?;
                if !lowered_value_matches_static_type(value, &ty) {
                    false
                } else {
                    if let Some(slot) = slot {
                        if bind { slots[slot] = value.clone(); }
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
                            if bind { slots[slot] = inner.as_ref().clone(); }
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
                        if bind { slots[slot] = inner; }
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
                lowered_error_variant_matches(&family, &variant, &fields, error, slots, bind)
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
                let type_name = indexed_decode::<Name>(&mut payload, execution, span)?;
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
                if value.type_name != type_name || value.name.as_ref() != name.as_str() || value.fields.len() != field_slots.len() {
                    false
                } else {
                    for (slot, field) in field_slots.iter().zip(&value.fields) {
                        if let Some(slot) = slot {
                            if bind { slots[*slot] = field.clone(); }
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

    /// Preserve an option expression's effects even when the stage currently
    /// runs serially; worker stages use the same positive-count boundary.
    fn eval_indexed_jobs_option(
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
        let outcome = self.eval_indexed_driver_step_inner(view, call_span);
        // A top-level statement is a boundary at which nothing can still reach a
        // producer it built and dropped.
        let swept = self.sweep_script_producers(call_span);
        Some(match (outcome, swept) {
            (Ok(flow), Ok(())) => Ok(flow),
            (Err(error), _) | (Ok(_), Err(error)) => Err(error),
        })
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
            slots[slot.slot] = Self::share_indexed_root_value(value);
        }
        let header = Self::indexed_block_header(view.slot_count());
        let previous_root_slots = self.indexed_root_slots.replace(super::super::IndexedRootSlots {
            address: slots.as_ptr() as usize,
            scope_revision: self.scope_write_revision,
            bindings: top_level_slots.iter().filter(|slot| slot.mutable)
                .map(|slot| (slot.clone(), slots[slot.slot].clone())).collect(),
        });
        let result = (|| {
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
                    record_fields.push((export.name, value));
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
                            return Ok(Some(self.indexed_driver_expression_escape(value, call_span)));
                        }
                    };
                if let Some(check) = &validation {
                    if matches!(&check.ty, Type::Map(_, _))
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
                            return Ok(Some(self.indexed_driver_expression_escape(value, call_span)));
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
                let fields = indexed_decode::<Vec<(Name, usize)>>(&mut payload, &execution, call_span)?;
                let target = indexed_decode::<LoweredCompTarget>(&mut payload, &execution, call_span)?;
                let mutable = indexed_decode::<bool>(&mut payload, &execution, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, &execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let source =
                    match self.eval_indexed_expr(&execution, source, &mut slots, call_span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => {
                            return Ok(Some(self.indexed_driver_expression_escape(value, call_span)));
                        }
                    };
                bind_lowered_comp_target(&target, source, &mut slots, span)?;
                for (name, slot) in fields {
                    self.define(name, Binding { value: slots[slot].clone().into_value(), mutable });
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
                        return Ok(Some(self.indexed_driver_expression_escape(value, span)));
                    }
                }
            }
            FullDriverTag::Stmt => {
                let statement = indexed_raw(&mut payload, call_span)?;
                indexed_finish(payload, call_span)?;
                let flow =
                    self.eval_indexed_stmt(&execution, statement, &header, &mut slots, call_span)?;
                match flow {
                    StmtFlow::Propagate(value) => self.indexed_driver_expression_escape(value, call_span),
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
                            return Ok(Some(self.indexed_driver_expression_escape(value, call_span)));
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
                self.eval_indexed_deferred_expr(&execution, value, &mut slots, span)?;
                Flow::Continue(Value::Unit)
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
        })();
        let publication = self.sync_indexed_root_slots(&mut slots, call_span);
        self.indexed_root_slots = previous_root_slots;
        match (result, publication) {
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(error),
            (Ok(flow), Ok(())) => Ok(flow),
        }
    }

    fn share_indexed_root_value(value: LoweredValue) -> LoweredValue {
        match value {
            LoweredValue::List(values) => LoweredValue::SharedList(Arc::new(values)),
            value => value,
        }
    }

    fn indexed_root_value_unchanged(value: &LoweredValue, previous: &LoweredValue) -> bool {
        match (value, previous) {
            (LoweredValue::SharedList(value), LoweredValue::SharedList(previous)) => Arc::ptr_eq(value, previous),
            (LoweredValue::Map(value), LoweredValue::Map(previous)) => Arc::ptr_eq(value, previous),
            (LoweredValue::Record(value), LoweredValue::Record(previous)) => Arc::ptr_eq(value, previous),
            (LoweredValue::RecordVec(value), LoweredValue::RecordVec(previous)) => Arc::ptr_eq(value, previous),
            _ => value == previous,
        }
    }

    fn sync_indexed_root_slots(&mut self, slots: &mut [LoweredValue], span: Span) -> Result<(), RuntimeError> {
        let Some(mut root) = self.indexed_root_slots.take() else { return Ok(()); };
        if root.address != slots.as_ptr() as usize {
            self.indexed_root_slots = Some(root);
            return Ok(());
        }
        let result = (|| {
            let scope_changed = root.scope_revision != self.scope_write_revision;
            for (binding, previous) in &mut root.bindings {
                let slot = &mut slots[binding.slot];
                if !Self::indexed_root_value_unchanged(slot, previous) {
                    *slot = Self::share_indexed_root_value(std::mem::replace(slot, LoweredValue::Unit));
                    self.assign(&binding.name.as_str(), slot.clone().into_value(), span)?;
                } else if scope_changed && let Some(value) = self.lookup(binding.name).and_then(|binding| lowered_value_from_runtime_any(&binding.value)) {
                    *slot = Self::share_indexed_root_value(value);
                }
                *previous = slot.clone();
            }
            root.scope_revision = self.scope_write_revision;
            Ok(())
        })();
        self.indexed_root_slots = Some(root);
        result
    }

    fn indexed_argument_default(&self, callee: &LoweredValue, slot: usize, span: Span) -> Result<LoweredValue, RuntimeError> {
        let (function, kind) = match callee {
            LoweredValue::Pure(function) => (function, LoweredFunctionKind::Pure),
            LoweredValue::Proc(function) => (function, LoweredFunctionKind::Proc),
            _ => return Err(RuntimeError::new("type-error", "argument default requires a prepared callable").with_span(span)),
        };
        let key = function.as_name().map(LoweredFunctionKey::Name)
            .or_else(|| function.as_qualified().map(LoweredFunctionKey::Qualified)).expect("callable identity is interned");
        self.indexed_argument_default_for(key, kind, slot, span)
    }

    fn indexed_argument_default_for(&self, key: LoweredFunctionKey, kind: LoweredFunctionKind, slot: usize, span: Span) -> Result<LoweredValue, RuntimeError> {
        let program = self.indexed_program.as_ref().expect("indexed call retains its program");
        let view = if let Some(view) = program.function_view(key, kind).map_err(|error| indexed_error(error, span))? { view }
            else {
                let LoweredFunctionKey::Qualified(qualified) = key else { return Err(RuntimeError::new("unresolved-call", "argument default callable is not prepared").with_span(span)); };
                let dynamic = self.indexed_dynamic_functions.get(&qualified).ok_or_else(|| RuntimeError::new("unresolved-call", "argument default callable is not prepared").with_span(span))?;
                dynamic.program.function_view(dynamic.function, dynamic.kind).map_err(|error| indexed_error(error, span))?
                    .ok_or_else(|| RuntimeError::new("unresolved-call", "argument default callable is not prepared").with_span(span))?
            };
        view.header().map_err(|error| indexed_error(error, span))?.param_defaults.get(slot).and_then(Clone::clone)
            .ok_or_else(|| RuntimeError::new("indexed-ir", "checked omitted argument has no prepared default").with_span(span))
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
        if let Err(error) = super::validate_unsigned_runtime_args(&header, args, call_span) { return Some(Err(error)); }
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
            .and_then(|value| super::checked_lowered_return_value(&header, value, call_span))
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
        let definition_span = view
            .definition_span()
            .map_err(|error| indexed_error(error, call_span))?;
        // Rendering a display name allocates, so it happens only when a trace
        // event will use it; the traceback keeps the symbol handles instead.
        if self.trace_enabled {
            let name = function.display_name();
            self.trace_enter_with_definition(
                enter_kind,
                Some(call_span),
                Some(definition_span),
                Some(&name),
                TracePayload::None,
            );
        }
        self.call_stack.push(TracebackFrame {
            kind: frame_kind,
            name: function.traceback_name(),
            definition_span: Some(definition_span),
            call_span: Some(call_span),
        });
        let result = with_indexed_eval_depth(call_span, || {
            self.eval_indexed_function(view, header, slots, call_span)
        });
        self.call_stack.pop();
        if self.trace_enabled {
            let name = function.display_name();
            self.trace_exit_with_definition(
                exit_kind,
                Some(call_span),
                Some(definition_span),
                Some(&name),
                TracePayload::None,
            );
        }
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
            .and_then(|value| super::checked_lowered_return_value(&header, value, call_span));
        self.recycle_lowered_slots(next_slots);
        result
    }

    /// Call an implementation a loading program published.
    ///
    /// A dynamically loaded module can name a standard-library implementation
    /// its loader already prepared. That function has no identity in the loaded
    /// module's own store, so the call resolves through the dynamic function
    /// table and executes inside the program that prepared it.
    fn eval_indexed_external_call(
        &mut self,
        qualified: QualifiedName,
        values: &[LoweredValue],
        call_span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        let key = LoweredFunctionKey::Qualified(qualified);
        let program = Arc::clone(
            self.indexed_program
                .as_ref()
                .expect("indexed caller retains its indexed program"),
        );
        for kind in [LoweredFunctionKind::Pure, LoweredFunctionKind::Proc] {
            if self
                .indexed_function_index(&program, key, kind)
                .map_err(|error| indexed_error(error, call_span))?
                .is_some()
            {
                return self.eval_indexed_named_call(key, values, call_span);
            }
        }
        let dynamic = self
            .indexed_dynamic_functions
            .get(&qualified)
            .cloned()
            .ok_or_else(|| {
                RuntimeError::new("unresolved-lowered-call", qualified.to_string())
                    .with_span(call_span)
            })?;
        let previous = self.indexed_program.replace(Arc::clone(&dynamic.program));
        let result = self.eval_indexed_named_call(dynamic.function, values, call_span);
        self.indexed_program = previous;
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
        self.call_stack.push(TracebackFrame {
            kind: TracebackFrameKind::Pure,
            name: function.traceback_name(),
            definition_span: None,
            call_span: Some(call_span),
        });
        let result = with_indexed_eval_depth(call_span, || {
            self.eval_indexed_function(view, &header, &mut next_slots, call_span)
        });
        self.call_stack.pop();
        let result =
            result.and_then(|value| super::checked_lowered_return_value(&header, value, call_span));
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
        .and_then(|value| super::checked_lowered_return_value(&header, value, call_span));
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
            // A producer call does not run the body: the bound slots become a
            // suspended continuation that consuming the stream resumes, one
            // `yield` at a time.
            let (function, kind) = execution
                .function_identity()
                .map_err(|error| indexed_error(error, call_span))?;
            let state =
                self.start_script_producer(function, kind, view, slots.to_vec(), call_span)?;
            return Ok(LoweredValue::Stream(Box::new(StreamValue::from_script(
                state,
            ))));
        }
        let result = self.eval_indexed_stmts(&execution, body, header, slots, call_span);
        let write_back = self.write_back_lowered_captures(header, slots, call_span);
        let flow = result?;
        write_back?;
        match flow {
            StmtFlow::Value(value) | StmtFlow::Return(value) | StmtFlow::Propagate(value) => Ok(value),
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
            FullStageTag::Where | FullStageTag::WhereBlock => "where",
            FullStageTag::Map | FullStageTag::MapBlock => "map",
            FullStageTag::FlatMap | FullStageTag::FlatMapBlock => "flat-map",
            FullStageTag::BytesChunks => "bytes.chunks",
            FullStageTag::BatchCount | FullStageTag::BatchMaxArgv | FullStageTag::BatchMaxBytes | FullStageTag::BatchLimits => {
                "batch"
            }
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

    fn indexed_assertion_outcome(
        &mut self,
        passed: bool,
        values: Option<(&LoweredValue, &LoweredValue)>,
        span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        if passed { return Ok(ControlFlow::Continue(LoweredValue::Unit)); }
        let message = assertion_failure_message(self.sources.span_text(span).as_deref(), values, None, None);
        let error = crate::runtime::eval::modules::assertion_error(message, Some(span));
        let propagated = self.lowered_question_propagation_value(lowered_result_err_value(error), span)?;
        Ok(ControlFlow::Break(propagated))
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
                format!("desc expected Bool, found {}", value.type_name()),
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

    /// The slot a receiver instruction reads, when it reads exactly one.
    ///
    /// A consuming call needs the receiver to be the plain slot read itself: any
    /// other expression (a field access, a call) has already derived a value of
    /// its own and owns whatever it produced.
    fn indexed_slot_read(
        execution: &FullExecution<'_>,
        instruction: u32,
        span: Span,
    ) -> Result<Option<usize>, RuntimeError> {
        let (tag, mut payload) = indexed_value(execution.instruction_id(instruction), span)?;
        match tag {
            FullTag::ExprParam => Ok(Some(indexed_decode::<usize>(
                &mut payload,
                execution,
                span,
            )?)),
            _ => Ok(None),
        }
    }

    fn eval_indexed_comp_qualifiers(&mut self, execution: &FullExecution<'_>, qualifiers: &[IndexedCompQualifier], position: usize, key: Option<u32>, value: u32, slots: &mut [LoweredValue], values: &mut Vec<LoweredValue>, map_values: &mut BTreeMap<MapKey, LoweredValue>, span: Span) -> Result<ControlFlow<LoweredValue, ()>, RuntimeError> {
        if let Some(qualifier) = qualifiers.get(position) {
            match qualifier {
                IndexedCompQualifier::If { condition, span } => {
                    match self.eval_indexed_bool(execution, *condition, slots, *span)? {
                        ControlFlow::Continue(false) => return Ok(ControlFlow::Continue(())),
                        ControlFlow::Continue(true) => {},
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    }
                    return self.eval_indexed_comp_qualifiers(execution, qualifiers, position + 1, key, value, slots, values, map_values, span.to_owned());
                }
                IndexedCompQualifier::For { target, iter, span } => {
                    let iterable = match self.eval_indexed_expr(execution, *iter, slots, *span)? {
                        ControlFlow::Continue(value) => lowered_comp_iterable(value, *span)?,
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    };
                    if let LoweredValue::Stream(mut stream) = iterable {
                        let result = (|| {
                            while let Some(item) = self.stream_next(&mut stream, *span)? {
                                let item = lowered_value_from_runtime_any(&item).ok_or_else(|| RuntimeError::new("type-error", "stream produced unsupported comprehension item").with_span(*span))?;
                                bind_lowered_comp_target(target, item, slots, *span)?;
                                if let ControlFlow::Break(value) = self.eval_indexed_comp_qualifiers(execution, qualifiers, position + 1, key, value, slots, values, map_values, *span)? { return Ok(ControlFlow::Break(value)); }
                            }
                            Ok(ControlFlow::Continue(()))
                        })();
                        let cleanup = self.stream_cancel(&mut stream, *span);
                        return match result { Ok(value) => cleanup.map(|()| value), Err(error) => Err(error) };
                    }
                    let iterable = match LoweredScalarCursor::try_new(iterable) {
                        Ok(mut cursor) => {
                            while let Some(item) = cursor.next() {
                                self.service_pending_signal(*span)?;
                                if self.signal_state.shutdown_complete { return Ok(ControlFlow::Continue(())); }
                                bind_lowered_comp_target(target, item, slots, *span)?;
                                if let ControlFlow::Break(value) = self.eval_indexed_comp_qualifiers(execution, qualifiers, position + 1, key, value, slots, values, map_values, *span)? { return Ok(ControlFlow::Break(value)); }
                            }
                            return Ok(ControlFlow::Continue(()));
                        }
                        Err(iterable) => iterable,
                    };
                    if let LoweredValue::Map(entries) = iterable {
                        let mut cursor = LoweredMapCursor::new(entries);
                        while let Some(item) = cursor.next() {
                            bind_lowered_comp_target(target, item, slots, *span)?;
                            if let ControlFlow::Break(value) = self.eval_indexed_comp_qualifiers(execution, qualifiers, position + 1, key, value, slots, values, map_values, *span)? { return Ok(ControlFlow::Break(value)); }
                        }
                        return Ok(ControlFlow::Continue(()));
                    }
                    for item in self.lowered_list_items(iterable, *span, "comprehension expected List or Stream")? {
                        bind_lowered_comp_target(target, item, slots, *span)?;
                        if let ControlFlow::Break(value) = self.eval_indexed_comp_qualifiers(execution, qualifiers, position + 1, key, value, slots, values, map_values, *span)? { return Ok(ControlFlow::Break(value)); }
                    }
                    return Ok(ControlFlow::Continue(()));
                }
            }
        }
        let key = if let Some(key) = key {
            let key = match self.eval_indexed_expr(execution, key, slots, span)? { ControlFlow::Continue(value) => value, ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)) };
            Some(lowered_map_literal_key(&key, span)?)
        } else { None };
        let value = match self.eval_indexed_expr(execution, value, slots, span)? { ControlFlow::Continue(value) => value, ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)) };
        if let Some(key) = key { map_values.insert(key, value); } else { values.push(value); }
        Ok(ControlFlow::Continue(()))
    }

    // Named argument preparation binds each value with a nested match. Follow
    // selected match arms iteratively so constructor width does not become
    // native call depth, while subjects and guards still evaluate in order.
    fn eval_indexed_match_expr(
        &mut self,
        execution: &FullExecution<'_>,
        mut instruction: u32,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        loop {
            let (tag, mut payload) = indexed_value(execution.instruction_id(instruction), call_span)?;
            if tag != FullTag::ExprMatch {
                return self.eval_indexed_expr(execution, instruction, slots, call_span);
            }
            self.sync_indexed_root_slots(slots, call_span)?;
            let value = indexed_raw(&mut payload, call_span)?;
            let (_, mut arms) = execution.block(&mut payload, BLOCK_LIST)
                .map_err(|error| indexed_error(error, call_span))?;
            let arm_count = indexed_raw(&mut arms, call_span)? as usize;
            let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
            indexed_finish(payload, call_span)?;
            let mut decoded_arms = Vec::with_capacity(arm_count);
            for _ in 0..arm_count {
                decoded_arms.push((indexed_raw(&mut arms, span)?, indexed_optional_raw(&mut arms, span)?, indexed_raw(&mut arms, span)?));
            }
            indexed_finish(arms, span)?;
            let value = match self.eval_indexed_expr(execution, value, slots, call_span)? {
                ControlFlow::Continue(value) => value,
                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
            };
            let mut selected = None;
            for (pattern, guard, arm_value) in decoded_arms {
                if !Self::indexed_pattern_matches(execution, pattern, &value, slots, span)? { continue; }
                if let Some(guard) = guard {
                    match self.eval_indexed_expr(execution, guard, slots, call_span)? {
                        ControlFlow::Continue(LoweredValue::Bool(true)) => {}
                        ControlFlow::Continue(_) => continue,
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    }
                }
                selected = Some(arm_value);
                break;
            }
            instruction = selected.ok_or_else(|| lowered_match_no_arm(span))?;
        }
    }

    pub(super) fn eval_indexed_expr(
        &mut self,
        execution: &FullExecution<'_>,
        instruction: u32,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        // A statement that is about to overwrite a slot offers that slot to its
        // outermost expression; nested expressions (operands, arguments) must
        // see the slot as it is, because they may read it before the store.
        self.sync_indexed_root_slots(slots, call_span)?;
        let saved_consuming = self.consuming_receiver.take();
        // Root bindings are also visible through scopes; taking their slot
        // before assignment would publish a transient Unit to a called proc.
        let consuming = saved_consuming.filter(|slot| !self.indexed_root_slots.as_ref().is_some_and(|root|
            root.address == slots.as_ptr() as usize && root.bindings.iter().any(|(binding, _)| binding.slot == *slot)));

        let result =
            self.eval_indexed_expr_inner(execution, instruction, slots, call_span, consuming);
        self.consuming_receiver = saved_consuming;
        let publication = self.sync_indexed_root_slots(slots, call_span);
        match (result, publication) {
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(error),
            (Ok(flow), Ok(())) => Ok(flow),
        }
    }

    fn eval_indexed_module_call_values(
        &mut self, op: RuntimeOp, values: super::NativeArgumentValues,
        span: Span, cli_plan: Option<&crate::modules::cli::CliDescriptorPlan>,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        if !self.trace_enabled {
            return self.eval_lowered_module_call_values(op, values, span, cli_plan);
        }
        let trace_name = crate::modules::signature::api_spec()
            .op_trace_name(op)
            .map(str::to_string);
        let rooted = trace_name.as_deref().is_some_and(|name| name.starts_with("FsRoot."));
        self.trace_enter(
            if rooted { TraceKind::MethodCall } else { TraceKind::ModuleCall },
            Some(span),
            trace_name.as_deref(),
            TracePayload::None,
        );
        let result = self.eval_lowered_module_call_values(op, values, span, cli_plan);
        self.trace_exit(
            if rooted { TraceKind::MethodResult } else { TraceKind::ModuleResult },
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
        consuming: Option<usize>,
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
                let value = indexed_decode::<crate::runtime::eval::PreparedConstantValue>(&mut payload, execution, call_span)?;
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
            FullTag::ExprAssert => {
                let value = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let (tag, mut predicate) = indexed_value(execution.instruction_id(value), span)?;
                if tag == FullTag::ExprBinary {
                    let op = indexed_decode::<BinaryOp>(&mut predicate, execution, span)?;
                    let left = indexed_raw(&mut predicate, span)?;
                    let right = indexed_raw(&mut predicate, span)?;
                    let _ = indexed_decode::<Span>(&mut predicate, execution, span)?;
                    indexed_finish(predicate, span)?;
                    if assertion_comparison_op(op) {
                        let left = match self.eval_indexed_expr(execution, left, slots, span)? {
                            ControlFlow::Continue(value) => value,
                            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                        };
                        let right = match self.eval_indexed_expr(execution, right, slots, span)? {
                            ControlFlow::Continue(value) => value,
                            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                        };
                        let passed = crate::runtime::eval::lowered_ops::lowered_assertion_comparison(op, &left, &right, span)?;
                        return self.indexed_assertion_outcome(passed, Some((&left, &right)), span);
                    }
                }
                let value = match self.eval_indexed_expr(execution, value, slots, span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let LoweredValue::Bool(passed) = value else {
                    return Err(RuntimeError::new("indexed-ir", "assertion requires checked Bool").with_span(span));
                };
                return self.indexed_assertion_outcome(passed, None, span);
            }
            FullTag::ExprComparisonChain => {
                let (_, mut pairs) = execution.block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, call_span))?;
                let len = indexed_raw(&mut pairs, call_span)? as usize;
                let assertion = indexed_decode::<bool>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let mut previous = None;
                for _ in 0..len {
                    let pair = indexed_raw(&mut pairs, call_span)?;
                    let (tag, mut pair_payload) = indexed_value(execution.instruction_id(pair), call_span)?;
                    if tag != FullTag::ExprBinary { return Err(RuntimeError::new("indexed-ir", "comparison chain requires binary pairs").with_span(call_span)); }
                    let op = indexed_decode::<BinaryOp>(&mut pair_payload, execution, call_span)?;
                    let left = indexed_raw(&mut pair_payload, call_span)?;
                    let right = indexed_raw(&mut pair_payload, call_span)?;
                    let span = indexed_decode::<Span>(&mut pair_payload, execution, call_span)?;
                    indexed_finish(pair_payload, span)?;
                    let left = match previous.take() {
                        Some(value) => value,
                        None => match self.eval_indexed_expr(execution, left, slots, span)? {
                            ControlFlow::Continue(value) => value,
                            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                        },
                    };
                    let right = match self.eval_indexed_expr(execution, right, slots, span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    };
                    let result = lowered_binary_value(op, left.clone(), right.clone(), span)?;
                    if result == LoweredValue::Bool(false) {
                        if assertion { return Err(comparison_chain_assertion_failure(op, &left, &right, span)?); }
                        return Ok(ControlFlow::Continue(LoweredValue::Bool(false)));
                    }
                    previous = Some(right);
                }
                indexed_finish(pairs, call_span)?;
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
                let (_, mut branches) = execution.block(&mut payload, BLOCK_LIST).map_err(|error| indexed_error(error, call_span))?;
                let count = indexed_raw(&mut branches, call_span)? as usize;
                let mut decoded = Vec::with_capacity(count);
                for _ in 0..count {
                    let condition = indexed_raw(&mut branches, call_span)?;
                    let value = indexed_raw(&mut branches, call_span)?;
                    let captures = indexed_decode::<Vec<usize>>(&mut branches, execution, call_span)?;
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
                        StmtFlow::Propagate(value) | StmtFlow::Return(value) => return Ok(ControlFlow::Break(value)),
                        _ => unreachable!("expression branch produced statement control flow"),
                    }
                }
                self.eval_indexed_expr(execution, else_value, slots, span)?
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
                return self.eval_indexed_match_expr(execution, instruction, slots, call_span);
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
                let (mut parts, mut fmt) = fmt_operands(execution, payload, tag == FullTag::ExprPathFmtString, call_span)?;
                while let Some(part) = parts.next_operand()? {
                    match part {
                        IndexedFmtPart::Text(text) => fmt.push_text(&text),
                        IndexedFmtPart::Expr(expr, span, spec) => match self.eval_indexed_expr(execution, expr, slots, call_span)? {
                            ControlFlow::Continue(value) => fmt.push_value(&value, span, spec.as_ref())?,
                            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                        },
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
                let mut entries = IndexedOperands::<(Option<u32>, u32, Span)>::literal(execution, payload, false, call_span)?;
                while let Some((key, value, span)) = entries.next_operand()? {
                    let key = if let Some(key) = key {
                        let key = match self.eval_indexed_expr(execution, key, slots, span)? {
                            ControlFlow::Continue(value) => value,
                            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                        };
                        Some(lowered_map_literal_key(&key, span)?)
                    } else { None };
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
                let mut entries = IndexedOperands::<IndexedRecordEntry>::literal(execution, payload, false, call_span)?;
                let mut record = Vec::with_capacity(entries.len());
                while let Some(entry) = entries.next_operand()? {
                    match self.eval_indexed_expr(execution, entry.instruction(), slots, call_span)? {
                        ControlFlow::Continue(value) => entry.append(&mut record, value, call_span)?,
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    }
                }
                entries.finish()?;
                ControlFlow::Continue(finish_record_entries(record))
            }
            FullTag::ExprList | FullTag::ExprListBuild => {
                let mut items = IndexedOperands::<(u32, bool, Span)>::literal(execution, payload, tag == FullTag::ExprListBuild, call_span)?;
                let mut result = Vec::with_capacity(items.len());
                while let Some((expr, splice, span)) = items.next_operand()? {
                    match self.eval_indexed_expr(execution, expr, slots, span)? {
                        ControlFlow::Continue(value) => append_lowered_list_element(&mut result, value, splice, span)?,
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
                let wire = indexed_decode::<Option<Arc<crate::sema::wire_enums::WireEnumMapping>>>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                ControlFlow::Continue(LoweredValue::Tag(Box::new(LoweredTagValue {
                    type_name,
                    wire,
                    name,
                    fields: values,
                })))
            }
            FullTag::ExprListComp | FullTag::ExprMapComp => {
                let map = tag == FullTag::ExprMapComp;
                let key = map.then(|| indexed_raw(&mut payload, call_span)).transpose()?;
                let value = indexed_raw(&mut payload, call_span)?;
                let qualifiers = decode_comp_qualifiers(execution, &mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let mut values = Vec::new();
                let mut map_values = BTreeMap::new();
                let flow = self.eval_indexed_comp_qualifiers(execution, &qualifiers, 0, key, value, slots, &mut values, &mut map_values, span)?;
                match flow {
                    ControlFlow::Break(value) => ControlFlow::Break(value),
                    ControlFlow::Continue(()) => ControlFlow::Continue(if map { LoweredValue::Map(Arc::new(map_values)) } else { LoweredValue::List(values) }),
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
                                    LoweredValue::Map(Arc::new(counts.into_iter().map(|(key, value)| (MapKey::from(key), value)).collect()))
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
                                    let block_header = Self::indexed_block_header(slots.len());
                                    let mut filtered = Vec::new();
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
                                            flow => return Ok(self.preserve_lexical_expression_flow(flow)),
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
                                    let block_header = Self::indexed_block_header(slots.len());
                                    let mut matched = all;
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
                                            flow => return Ok(self.preserve_lexical_expression_flow(flow)),
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
                                            flow => return Ok(self.preserve_lexical_expression_flow(flow)),
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
                                    let configuration = match self.eval_indexed_expr(execution, configuration, slots, span)? {
                                        ControlFlow::Continue(value) => value,
                                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                                    };
                                    let fields = match configuration {
                                            LoweredValue::Record(fields) => fields,
                                            LoweredValue::RecordVec(fields) => Arc::new(fields.iter().map(|(name, value)| (Arc::<str>::from(name.as_str().as_str()), value.clone())).collect()),
                                            _ => return Err(RuntimeError::new("indexed-ir", "stage configuration must be a record").with_span(span)),
                                        };
                                    let positive_limit = |name: &str| -> Result<Option<usize>, RuntimeError> {
                                        match fields.get(name) {
                                            None => Ok(None),
                                            Some(LoweredValue::Int(value)) if *value > 0 => Ok(Some(*value as usize)),
                                            _ => Err(RuntimeError::new("stream-batch", format!("batch {name} must be a positive Int")).with_span(span)),
                                        }
                                    };
                                    let count = positive_limit("count")?;
                                    let max_bytes = positive_limit("max_bytes")?;
                                    let max_argv = match fields.get("max_argv") {
                                        None | Some(LoweredValue::Bool(false)) => None,
                                        Some(LoweredValue::Bool(true)) => Some(super::super::stream::platform_arg_max().saturating_sub(4096).clamp(1, 128 * 1024)),
                                        _ => return Err(RuntimeError::new("type-error", "batch max_argv must be Bool").with_span(span)),
                                    };
                                    if count.is_none() && max_bytes.is_none() && max_argv.is_none() {
                                        return Err(RuntimeError::new("stream-batch", "batch requires an enabled limit").with_span(span));
                                    }
                                    let mut items = IndexedPipelineItems::new(self, current, span)?;
                                    let driven = (|| -> Result<Vec<LoweredValue>, RuntimeError> {
                                        let mut batches = Vec::new();
                                        let mut batch = Vec::new();
                                        let mut bytes = 0usize;
                                        let mut argv_bytes = 0usize;
                                        while let Some(item) = items.next(self, span)? {
                                            let item_bytes = if max_bytes.is_some() || max_argv.is_some() { lowered_value_argv_len(&item) } else { 0 };
                                            if max_bytes.is_some_and(|limit| item_bytes > limit) {
                                                return Err(RuntimeError::new("argv-limit", "batch item exceeds byte budget").with_span(span));
                                            }
                                            let argv_cost = item_bytes.saturating_add(usize::from(!batch.is_empty()));
                                            let full = count.is_some_and(|limit| batch.len() >= limit)
                                                || max_bytes.is_some_and(|limit| bytes.saturating_add(item_bytes) > limit)
                                                || max_argv.is_some_and(|limit| argv_bytes.saturating_add(argv_cost) > limit);
                                            if !batch.is_empty() && full {
                                                batches.push(LoweredValue::List(std::mem::take(&mut batch)));
                                                bytes = 0;
                                                argv_bytes = 0;
                                            }
                                            bytes = bytes.saturating_add(item_bytes);
                                            argv_bytes = argv_bytes.saturating_add(item_bytes).saturating_add(usize::from(!batch.is_empty()));
                                            batch.push(item);
                                        }
                                        if !batch.is_empty() { batches.push(LoweredValue::List(batch)); }
                                        Ok(batches)
                                    })();
                                    let close = items.cancel(self, span);
                                    match driven {
                                        Ok(batches) => { close?; LoweredValue::List(batches) }
                                        Err(error) => { let _ = close; return Err(error); }
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
                                            None => super::super::stream::platform_arg_max()
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
                                    let block_header = Self::indexed_block_header(slots.len());
                                    let driven = (|| -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
                                while let Some(item) = items.next(self, span)? {
                                    slots[acc_slot] = acc;
                                    slots[item_slot] = item;
                                    match self.eval_indexed_statement_block(
                                        execution,
                                        body,
                                        &block_header,
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
                                        let configuration = match self.eval_indexed_expr(execution, configuration, slots, span)? {
                                            ControlFlow::Continue(value) => value,
                                            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                                        };
                                        let fields = match configuration {
                                            LoweredValue::Record(fields) => fields,
                                            LoweredValue::RecordVec(fields) => Arc::new(fields.iter().map(|(name, value)| (Arc::<str>::from(name.as_str().as_str()), value.clone())).collect()),
                                            _ => return Err(RuntimeError::new("indexed-ir", "stage configuration must be a record").with_span(span)),
                                        };
                                        let mut selected = None;
                                        for (name, mode) in [("sum", ReduceByOp::Sum), ("min", ReduceByOp::Min), ("max", ReduceByOp::Max)] {
                                            match fields.get(name) {
                                                Some(LoweredValue::Bool(true)) => {
                                                    if selected.replace(mode).is_some() { return Err(RuntimeError::new("stream-reduce-mode", "reduce-by requires exactly one enabled reduction mode").with_span(span)); }
                                                }
                                                None | Some(LoweredValue::Bool(false)) => {}
                                                _ => return Err(RuntimeError::new("type-error", "reduction modes must be Bool").with_span(span)),
                                            }
                                        }
                                        if let Some(jobs) = fields.get("jobs") {
                                            if !matches!(jobs, LoweredValue::Int(value) if *value > 0) {
                                                return Err(RuntimeError::new("stream-jobs", "stream worker count must be a positive Int").with_span(span));
                                            }
                                        }
                                        selected.ok_or_else(|| RuntimeError::new("stream-reduce-mode", "reduce-by requires exactly one enabled reduction mode").with_span(span))?
                                    } else {
                                        let op = indexed_decode::<ReduceByOp>(&mut stage_payload, execution, span)?;
                                        let jobs = indexed_optional_raw(&mut stage_payload, span)?;
                                        indexed_finish(stage_payload, span)?;
                                        if let ControlFlow::Break(value) = self.eval_indexed_jobs_option(execution, jobs, slots, span)? {
                                            return Ok(ControlFlow::Break(value));
                                        }
                                        op
                                    };
                                    let mut items = IndexedPipelineItems::new(self, current, span)?;
                                    let block_header = Self::indexed_block_header(slots.len());
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
                                        &block_header,
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
                                    LoweredValue::Map(Arc::new(groups.into_iter().map(|(key, value)| (MapKey::from(key), value)).collect()))
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
                                        let map_header = Self::indexed_block_header(slots.len());
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
                                                execution,
                                                body,
                                                value,
                                                &map_header,
                                                slots,
                                                slot,
                                                item,
                                                span,
                                            ) {
                                                Ok(value) => value,
                                                Err(error) => {
                                                    return Err(self.stream_item_runtime_error(
                                                        "par-map", item_index, error,
                                                    ));
                                                }
                                            };
                                            if let Some(flow) = self.pending_value_block_flow.take() {
                                                return Ok(self.preserve_lexical_expression_flow(flow));
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
                                            if let Some(flow) = self.pending_value_block_flow.take() {
                                                return Ok(self.preserve_lexical_expression_flow(flow));
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
                                        LoweredValue::Map(Arc::new(groups.into_iter().map(|(key, value)| (MapKey::from(key), value)).collect()))
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
                                    let block_header = Self::indexed_block_header(slots.len());
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
                                            if let Some(flow) = self.pending_value_block_flow.take() {
                                                return Ok(self.preserve_lexical_expression_flow(flow));
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
                                    let block_header = Self::indexed_block_header(slots.len());
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
                                            &block_header,
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
                                        let values = match self.eval_indexed_expr(execution, expression, slots, span)? {
                                            ControlFlow::Continue(value) => self.lowered_pipeline_input_items(value, span)?,
                                            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                                        };
                                        Some(values.into_iter().map(|value| match value {
                                            LoweredValue::Str(text) => Ok(text.to_string()),
                                            _ => Err(RuntimeError::new("type-error", "table columns must be Str").with_span(span)),
                                        }).collect::<Result<Vec<_>, _>>()?)
                                    } else {
                                        let columns = indexed_decode::<Option<Vec<String>>>(&mut stage_payload, execution, span)?;
                                        indexed_finish(stage_payload, span)?;
                                        columns
                                    };
                                    let mut items = IndexedPipelineItems::new(self, current, span)?;
                                    let collected = (|| -> Result<Vec<LoweredValue>, RuntimeError> {
                                        let mut records = Vec::new();
                                        while let Some(item) = items.next(self, span)? { records.push(item); }
                                        Ok(records)
                                    })();
                                    let close = items.cancel(self, span);
                                    let collected = match collected {
                                        Ok(records) => { close?; records }
                                        Err(error) => { let _ = close; return Err(error); }
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
                // A statement that overwrites a slot may hand this call the
                // value that slot holds, but only when the receiver is exactly a
                // read of that slot and the arguments have not replaced it.
                let consumes = match consuming {
                    Some(target)
                        if Self::indexed_slot_read(execution, receiver, call_span)?
                            == Some(target) =>
                    {
                        Some(target)
                    }
                    _ => None,
                };
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
                let receiver = match consumes {
                    Some(target) if lowered_shares_backing(&slots[target], &receiver) => {
                        std::mem::replace(&mut slots[target], LoweredValue::Unit)
                    }
                    _ => receiver,
                };
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
                ControlFlow::Continue(if byte < 0 { LoweredValue::Null } else { LoweredValue::Int(byte) })
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
                ControlFlow::Continue(super::super::require::require_value(self, value, &check, span))
            }
            FullTag::ExprContextScope => {
                let kind = indexed_decode::<crate::syntax::arena::ContextScopeKind>(&mut payload, execution, call_span)?;
                let input = indexed_raw(&mut payload, call_span)?;
                let body = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let value = match self.eval_indexed_expr(execution, input, slots, span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let restore = match self.enter_indexed_context_scope(kind, value, span) {
                    Ok(restore) => restore,
                    Err(error) => return Ok(ControlFlow::Continue(lowered_result_err_value(error))),
                };
                let header = Self::indexed_block_header(slots.len());
                // Keep resources owned by the context until the escaping value
                // has been checked, so rejected handles close before restoration.
                let context_owner = self.enter_owned_host_scope();
                self.recursive_context_slots.push((slots.as_ptr() as usize, Default::default()));
                let result = self.eval_indexed_statement_block(execution, body, &header, slots, span);
                self.recursive_context_slots.pop();
                let result = match result {
                    Ok(StmtFlow::Value(value) | StmtFlow::Return(value) | StmtFlow::Propagate(value) | StmtFlow::Break(Some(value))) if Self::context_scope_value_escapes(&value) => {
                        self.pending_traceback = None;
                        Err(RuntimeError::new("context-scope-escape", "a live producer or host handle cannot escape a restored context").with_span(span))
                    }
                    Err(error) if Self::context_scope_runtime_error_escapes(&error) => {
                        self.pending_traceback = None;
                        Err(RuntimeError::new("context-scope-escape", "a live producer or host handle cannot escape a restored context").with_span(span))
                    }
                    result => result,
                };
                let cleanup = self.exit_owned_host_scope(context_owner);
                self.restore_indexed_context_scope(restore);
                let result = match (result, cleanup) {
                    (Err(primary), Err(secondary)) => { self.report_cleanup_error(&secondary, span); Err(primary) },
                    (Err(error), _) | (Ok(_), Err(error)) => Err(error),
                    (Ok(flow), Ok(())) => Ok(flow),
                };
                match result? {
                    StmtFlow::Value(value) => ControlFlow::Continue(lowered_result_ok(value)),
                    StmtFlow::None => ControlFlow::Continue(lowered_result_ok(LoweredValue::Unit)),
                    flow => self.preserve_lexical_expression_flow(flow),
                }
            }
            FullTag::ExprCapture => {
                let body = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let header = Self::indexed_block_header(slots.len());
                match self.eval_indexed_error_boundary_block(execution, body, &header, slots, span)? {
                    StmtFlow::Value(value) => ControlFlow::Continue(LoweredValue::ResultOk(Box::new(value))),
                    StmtFlow::None => ControlFlow::Continue(LoweredValue::ResultOk(Box::new(LoweredValue::Unit))),
                    StmtFlow::Propagate(value) => {
                        self.pending_traceback = None;
                        ControlFlow::Continue(value)
                    }
                    flow => self.preserve_lexical_expression_flow(flow),
                }
            }
            FullTag::ExprErrorContext => {
                let message = indexed_raw(&mut payload, call_span)?;
                let body = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let message = match self.eval_indexed_expr(execution, message, slots, span)? {
                    ControlFlow::Continue(value) => lowered_str_arg_owned(Some(value), "", "ctx description", span)?,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let context = crate::runtime::value::ErrorContext { kind: "ctx".to_string(), message: Some(message), span: Some(span) };
                let header = Self::indexed_block_header(slots.len());
                self.cleanup_error_contexts.push(context.clone());
                let result = self.eval_indexed_statement_block(execution, body, &header, slots, span);
                self.cleanup_error_contexts.pop();
                match result {
                    Ok(StmtFlow::Propagate(value)) => {
                        let contextual = match value {
                            LoweredValue::ResultErr(error) => LoweredValue::ResultErr(Box::new(super::super::add_error_context(*error, context))),
                            other => LoweredValue::Error(Box::new(super::super::add_error_context(other.into_value(), context))),
                        };
                        if let Some(traceback) = &mut self.pending_traceback {
                            let error = match &contextual { LoweredValue::ResultErr(error) => error.as_ref(), other => &other.clone().into_value() };
                            traceback.error = TraceError::from_value(error);
                        }
                        self.preserve_lexical_expression_flow(StmtFlow::Propagate(contextual))
                    }
                    Ok(StmtFlow::Value(value)) => ControlFlow::Continue(value),
                    Ok(StmtFlow::None) => ControlFlow::Continue(LoweredValue::Unit),
                    Ok(flow) => self.preserve_lexical_expression_flow(flow),
                    Err(error) if error.abort.is_some() => return Err(error),
                    Err(error) => {
                        let Value::Error(error) = super::super::add_error_context(Value::Error(Box::new(error)), context) else { unreachable!() };
                        if let Some(traceback) = &mut self.pending_traceback { traceback.error = TraceError::from_runtime_error(&error); }
                        return Err(*error);
                    }
                }
            }
            FullTag::ExprValueBlock => {
                let body = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let header = Self::indexed_block_header(slots.len());
                match self.eval_indexed_statement_block(execution, body, &header, slots, span)? {
                    StmtFlow::Value(value) => ControlFlow::Continue(value),
                    StmtFlow::None => ControlFlow::Continue(LoweredValue::Unit),
                    flow => self.preserve_lexical_expression_flow(flow),
                }
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
                        flow @ (StmtFlow::Value(_) | StmtFlow::Return(_) | StmtFlow::Propagate(_)) => {
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
                        self.eval_indexed_error_boundary_block(execution, body, &header, slots, span)?;
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
                                Some(pattern) => Some(Self::indexed_pattern_match_pass(execution, pattern, &LoweredValue::Error(Box::new(error.clone())), slots, span, false)?),
                                None => None,
                            };
                            let next_delay = if selected == Some(false) { None } else {
                                delay_values.get(attempt_index).map(|delay| delay.millis)
                            };
                            let stop_reason = if selected == Some(false) {
                                Some(crate::trace::RetryStopReason::Nonmatching)
                            } else if next_delay.is_none() {
                                Some(crate::trace::RetryStopReason::Exhausted)
                            } else { None };
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
                            if stop_reason.is_some() { break; }
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
                ControlFlow::Continue(self.lowered_stream_list_result(
                    fs_module::list_filesystem(self.host_path(&path), stat, ordered, span),
                    span,
                )?)
            }
            FullTag::ExprFsTempDir => {
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                ControlFlow::Continue(match new_temp_fs_root("fs-temp-dir", span) {
                    Ok(root) => {
                        let id = self.fs_roots.len() as i64 + 1;
                        self.fs_roots.push(Some(root));
                        lowered_result_ok(LoweredValue::FsRoot(super::super::FsRootValue { id, owner: self.fs_root_owner.clone() }))
                    }
                    Err(error) => lowered_result_err_value(error),
                })
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
            FullTag::ExprModuleCall => {
                let op = indexed_decode::<RuntimeOp>(&mut payload, execution, call_span)?;
                let cli_plan = indexed_decode::<Option<Arc<crate::modules::cli::CliDescriptorPlan>>>(&mut payload, execution, call_span)?;
                let (_, mut args) = execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, call_span))?;
                let len = indexed_raw(&mut args, call_span)? as usize;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let mut values = Vec::with_capacity(len);
                for _ in 0..len {
                    let arg = indexed_optional_raw(&mut args, span)?;
                    if let Some(arg) = arg {
                        match self.eval_indexed_expr(execution, arg, slots, span)? {
                            ControlFlow::Continue(value) => values.push(Some(value)),
                            ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                        }
                    } else { values.push(None); }
                }
                indexed_finish(args, span)?;
                let values = super::NativeArgumentValues::new(values);
                return self.eval_indexed_module_call_values(op, values, span, cli_plan.as_deref());
            }
            FullTag::ExprProcessCommandArgv => {
                let target = indexed_raw(&mut payload, call_span)?;
                let argv = indexed_raw(&mut payload, call_span)?;
                let mut optional = [None; 13];
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
                ]: [Option<LoweredValue>; 13] = evaluated
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
                                    accepted_exit_codes = Some(super::lowered_accepted_exit_codes(value, span)?);
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
                            let run_accept = match self.eval_indexed_optional_expr(execution, run_accept, slots, span)? {
                                ControlFlow::Continue(value) => value.map(|value| super::lowered_accepted_exit_codes(value, span)).transpose()?,
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
                        return Err(RuntimeError::new("accept-policy", "accept cannot be supplied both as a field and a run option").with_span(span));
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
                let validation_error = end.status.as_ref().and_then(|status| crate::runtime::run::run_completion_error(status, &invocations, propagate));
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
                    if invocations.iter().any(|invocation| invocation.accepted_exit_codes.is_some()) {
                        let value = self.lowered_question_propagation_value(value, span)?;
                        return Ok(self.preserve_lexical_expression_flow(StmtFlow::Propagate(value)));
                    }
                    ControlFlow::Continue(value)                } else if propagate {
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
                if invocation.accepted_exit_codes.is_some() && matches!(kind, RunKind::StreamText | RunKind::StreamBytes) {
                    let value = self.start_policy_process_stream(&invocation, kind == RunKind::StreamText, span)?;
                    if propagate && matches!(value, LoweredValue::ResultErr(_)) {
                        let value = self.lowered_question_propagation_value(value, span)?;
                        return Ok(self.preserve_lexical_expression_flow(StmtFlow::Propagate(value)));
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
                if (propagate || (matches!(kind, RunKind::Status | RunKind::Plain) && invocation.accepted_exit_codes.is_some())) && matches!(value, LoweredValue::ResultErr(_)) {
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
            FullTag::ExprErr => {
                let value = indexed_raw(&mut payload, call_span)?;
                let cause = indexed_optional_raw(&mut payload, call_span)?;
                indexed_finish(payload, call_span)?;
                let value = match self.eval_indexed_expr(execution, value, slots, call_span)? {
                    ControlFlow::Continue(value) => value.into_value(),
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let value = if let Some(cause) = cause {
                    let cause = match self.eval_indexed_expr(execution, cause, slots, call_span)? {
                        ControlFlow::Continue(value) => value.into_value(),
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    };
                    value.with_error_cause(cause).map_err(|error| error.with_span(call_span))?
                } else { value };
                ControlFlow::Continue(LoweredValue::ResultErr(Box::new(value)))
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
                        // Statement consumers must distinguish propagation from lexical return.
                        Ok(self.preserve_lexical_expression_flow(StmtFlow::Propagate(value)))
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
                    if kind == 2 {
                        let default = self.indexed_argument_default_for(function, LoweredFunctionKind::Pure, arg as usize, span)
                            .or_else(|_| self.indexed_argument_default_for(function, LoweredFunctionKind::Proc, arg as usize, span))?;
                        values.push(default);
                        continue;
                    }
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
            FullTag::ExprExternalCall => {
                let qualified =
                    indexed_decode::<QualifiedName>(&mut payload, execution, call_span)?;
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
                    if kind == 2 {
                        let key = LoweredFunctionKey::Qualified(qualified);
                        let default = self.indexed_argument_default_for(key, LoweredFunctionKind::Pure, arg as usize, span)
                            .or_else(|_| self.indexed_argument_default_for(key, LoweredFunctionKind::Proc, arg as usize, span))?;
                        values.push(default); continue;
                    }
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
                    .eval_indexed_external_call(qualified, &values, span)
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
                    let argument_kind = indexed_raw(&mut args, span)?;
                    if argument_kind == 2 {
                        let slot = indexed_raw(&mut args, span)? as usize;
                        values.push(self.indexed_argument_default(&callee, slot, span)?);
                        continue;
                    }
                    let splice = match argument_kind {
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
                        values.extend(lowered_splice_arg_items(value, span)?);
                    } else {
                        values.push(value);
                    }
                }
                indexed_finish(args, span)?;
                let (function, _) = indexed_callable_identity(&callee, span)?;
                let result = match function {
                    LoweredFunctionKey::Name(_) => self.eval_indexed_named_call(function, &values, span)?,
                    LoweredFunctionKey::Qualified(qualified) => self.eval_indexed_external_call(qualified, &values, span)?,
                };
                ControlFlow::Continue(result)
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

    fn eval_indexed_assertion(
        &mut self,
        execution: &FullExecution<'_>,
        condition: u32,
        slots: &mut [LoweredValue],
        span: Span,
    ) -> Result<ControlFlow<LoweredValue, Option<AssertionFailure>>, RuntimeError> {
        let mut work = vec![AssertionWork::Expr(condition)];
        let mut result = (true, None);
        while let Some(item) = work.pop() {
            match item {
                AssertionWork::Left { op, right } => {
                    if (op == BinaryOp::And && !result.0) || (op == BinaryOp::Or && result.0) {
                        if !result.0 {
                            let failure = result.1.get_or_insert_with(|| checked_assertion_failure("boolean assertion failed", span));
                            failure.message = bounded_assertion_text(&format!("{} (right operand skipped)", failure.message), 1024);
                        }
                    } else {
                        work.push(AssertionWork::Right { op, left_failure: result.1.take() });
                        work.push(AssertionWork::Expr(right));
                    }
                }
                AssertionWork::Right { op, left_failure } => {
                    if op == BinaryOp::Or && !result.0 && let Some(left_failure) = left_failure {
                        let right = result.1.take().map(|error| error.message).unwrap_or_else(|| "boolean assertion failed".into());
                        result.1 = Some(checked_assertion_failure(bounded_assertion_text(&format!("{}; {}", left_failure.message, right), 1024), span));
                    }
                }
                AssertionWork::Expr(instruction) => {
                    let (tag, mut payload) = indexed_value(execution.instruction_id(instruction), span)?;
                    if tag == FullTag::ExprBinary {
                        let op = indexed_decode::<BinaryOp>(&mut payload, execution, span)?;
                        let left = indexed_raw(&mut payload, span)?;
                        let right = indexed_raw(&mut payload, span)?;
                        let operand_span = indexed_decode::<Span>(&mut payload, execution, span)?;
                        indexed_finish(payload, span)?;
                        if matches!(op, BinaryOp::And | BinaryOp::Or) {
                            work.push(AssertionWork::Left { op, right });
                            work.push(AssertionWork::Expr(left));
                            continue;
                        }
                        if assertion_comparison_op(op) {
                            let left = match self.eval_indexed_expr(execution, left, slots, operand_span)? {
                                ControlFlow::Continue(value) => value,
                                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                            };
                            let right = match self.eval_indexed_expr(execution, right, slots, operand_span)? {
                                ControlFlow::Continue(value) => value,
                                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                            };
                            let passed = crate::runtime::eval::lowered_ops::lowered_assertion_comparison(op, &left, &right, operand_span)?;
                            if !passed && instruction == condition {
                                return Ok(ControlFlow::Continue(Some(AssertionFailure::Operands(left, right))));
                            }
                            result = (passed, if passed { None } else { Some(comparison_chain_assertion_failure(op, &left, &right, operand_span)?) });
                            continue;
                        }
                    }
                    if tag == FullTag::ExprComparisonChain {
                        let (_, mut pairs) = execution.block(&mut payload, BLOCK_LIST).map_err(|error| indexed_error(error, span))?;
                        let len = indexed_raw(&mut pairs, span)? as usize;
                        indexed_decode::<bool>(&mut payload, execution, span)?;
                        indexed_finish(payload, span)?;
                        let mut previous = None;
                        result = (true, None);
                        for index in 0..len {
                            let pair = indexed_raw(&mut pairs, span)?;
                            let (pair_tag, mut pair_payload) = indexed_value(execution.instruction_id(pair), span)?;
                            if pair_tag != FullTag::ExprBinary { return Err(RuntimeError::new("indexed-ir", "comparison chain requires binary pairs").with_span(span)); }
                            let op = indexed_decode::<BinaryOp>(&mut pair_payload, execution, span)?;
                            let left = indexed_raw(&mut pair_payload, span)?;
                            let right = indexed_raw(&mut pair_payload, span)?;
                            let operand_span = indexed_decode::<Span>(&mut pair_payload, execution, span)?;
                            indexed_finish(pair_payload, span)?;
                            let left = match previous.take() {
                                Some(value) => value,
                                None => match self.eval_indexed_expr(execution, left, slots, operand_span)? {
                                    ControlFlow::Continue(value) => value,
                                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                                },
                            };
                            let right = match self.eval_indexed_expr(execution, right, slots, operand_span)? {
                                ControlFlow::Continue(value) => value,
                                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                            };
                            if lowered_binary_value(op, left.clone(), right.clone(), operand_span)? == LoweredValue::Bool(false) {
                                let mut failure = comparison_chain_assertion_failure(op, &left, &right, operand_span)?;
                                if index + 1 < len { failure.message.push_str(" (later operands skipped)"); }
                                result = (false, Some(failure));
                                break;
                            }
                            previous = Some(right);
                        }
                        continue;
                    }
                    match self.eval_indexed_expr(execution, instruction, slots, span) {
                        Ok(ControlFlow::Continue(LoweredValue::Bool(passed))) => result = (passed, None),
                        Ok(ControlFlow::Continue(_)) => return Err(RuntimeError::new("type-error", "assert condition requires Bool").with_span(span)),
                        Ok(ControlFlow::Break(value)) => return Ok(ControlFlow::Break(value)),
                        Err(error) => return Err(error),
                    }
                }
            }
        }
        Ok(ControlFlow::Continue(match result {
            (true, _) => None,
            (false, Some(failure)) => Some(AssertionFailure::Reached(failure.message)),
            (false, None) => Some(AssertionFailure::False),
        }))
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
                        if let Err(cleanup) = self.run_indexed_defers(execution, &defers, slots, call_span) {
                            if cleanup.abort.as_ref().is_some_and(|signal| signal.force) { return Err(cleanup); }
                            self.report_cleanup_error(&cleanup, call_span);
                        }
                    }
                    return Err(error);
                }
            };
            match flow {
                StmtFlow::None => {}
                flow @ (StmtFlow::Value(_) | StmtFlow::Return(_)
                | StmtFlow::Propagate(_)
                | StmtFlow::Break(_)
                | StmtFlow::Continue) => {
                    let cleanup = self.run_indexed_defers(execution, &defers, slots, call_span);
                    if cleanup.as_ref().err().is_some_and(|error| error.abort.as_ref().is_some_and(|signal| signal.force)) { return Err(cleanup.expect_err("forced cleanup abort")); }
                    if matches!(flow, StmtFlow::Propagate(_) | StmtFlow::Return(LoweredValue::ResultErr(_))) {
                        if let Err(error) = cleanup { self.report_cleanup_error(&error, call_span); }
                    } else {
                        cleanup?;
                    }
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
            StmtFlow::Value(value) | StmtFlow::Return(value) => Ok(Flow::Continue(value.into_value())),
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
                let traceback = self.pending_traceback.take().unwrap_or_else(|| Traceback {
                    failing_span: Some(call_span),
                    exe_path: self.exe_path_for_traceback(),
                    operation_kind: "signal.hook".to_string(),
                    error: TraceError::from_value(&error),
                    frames: self.call_stack.clone(),
                });
                Ok(Flow::Propagate(Propagation { error, traceback }))
            }
            StmtFlow::Break(_) | StmtFlow::Continue => Ok(Flow::Continue(Value::Unit)),
        }
    }

    pub(super) fn eval_indexed_deferred_expr(
        &mut self,
        execution: &FullExecution<'_>,
        value: u32,
        slots: &mut [LoweredValue],
        span: Span,
    ) -> Result<(), RuntimeError> {
        let result = self.eval_indexed_expr(execution, value, slots, span);
        let pending = self.pending_value_block_flow.take();
        let value = match (result?, pending) {
            (_, Some(StmtFlow::Propagate(value) | StmtFlow::Return(value))) => value,
            (_, Some(_)) => return Err(RuntimeError::new("defer-control-flow", "deferred cleanup produced invalid control flow").with_span(span)),
            (ControlFlow::Continue(value) | ControlFlow::Break(value), None) => value,
        };
        match value {
            LoweredValue::ResultErr(error) => {
                let mut error = runtime_error_from_value(*error, span);
                error.propagated = true;
                Err(error)
            },
            LoweredValue::ResultOk(_) | LoweredValue::Unit | LoweredValue::Status(_) => Ok(()),
            _ => Err(RuntimeError::new("defer-type", "deferred cleanup must produce Unit").with_span(span)),
        }
    }

    pub(super) fn run_indexed_defers(
        &mut self,
        execution: &FullExecution<'_>,
        defers: &[u32],
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<(), RuntimeError> {
        let primary_traceback = self.pending_traceback.take();
        let mut first_error = None;
        let mut first_traceback = None;
        for value in defers.iter().rev().copied() {
            if let Err(error) = self.eval_indexed_deferred_expr(execution, value, slots, call_span) {
                if error.abort.as_ref().is_some_and(|signal| signal.force) {
                    self.pending_traceback = primary_traceback;
                    return Err(error);
                }
                if first_error.is_none() {
                    first_error = Some(error);
                    first_traceback = self.pending_traceback.take();
                } else {
                    self.report_cleanup_error(&error, call_span);
                    self.pending_traceback = None;
                }
            }
        }
        self.pending_traceback = primary_traceback.or(first_traceback);
        first_error.map_or(Ok(()), Err)
    }

    fn finish_indexed_pattern_scope(
        &mut self,
        scope_id: u64,
        captures: &[usize],
        slots: &mut [LoweredValue],
        result: Result<StmtFlow, RuntimeError>,
    ) -> Result<StmtFlow, RuntimeError> {
        let parent_scope = self.parent_owned_host_scope();
        if let Ok(StmtFlow::Value(value) | StmtFlow::Return(value) | StmtFlow::Propagate(value) | StmtFlow::Break(Some(value))) = &result {
            self.transfer_owned_host_resources_in_value(&value.clone().into_value(), scope_id, parent_scope);
        }
        if let Err(error) = &result
            && error.abort.is_none() && error.propagated {
            self.transfer_owned_host_resources_in_runtime_error(error, scope_id, parent_scope);
        }
        // Captures are iteration/branch locals. Retain escaping values before
        // releasing these references and the condition's temporary resources.
        for slot in captures { slots[*slot] = LoweredValue::Unit; }
        let cleanup = self.exit_owned_host_scope(scope_id);
        match (result, cleanup) {
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(error),
            (Ok(flow), Ok(())) => Ok(flow),
        }
    }

    fn eval_indexed_error_boundary_block(
        &mut self, execution: &FullExecution<'_>, block: u32, header: &FunctionHeader,
        slots: &mut [LoweredValue], span: Span,
    ) -> Result<StmtFlow, RuntimeError> {
        match self.eval_indexed_statement_block(execution, block, header, slots, span) {
            Err(error) => capture_checked_error(error).map(StmtFlow::Propagate),
            result => result,
        }
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
        let scope_id = self.enter_owned_host_scope();
        let parent_scope = self.parent_owned_host_scope();
        let result = self.eval_indexed_stmts(execution, statements, header, slots, call_span);

        // Outgoing values and checked failures retain their opaque resources
        // in the parent before this lexical block closes.
        if let Ok(flow) = &result {
            match flow {
                StmtFlow::Value(value) | StmtFlow::Return(value) | StmtFlow::Propagate(value) => self.transfer_owned_host_resources_in_value(
                    &value.clone().into_value(),
                    scope_id,
                    parent_scope,
                ),
                StmtFlow::Break(Some(value)) => self.transfer_owned_host_resources_in_value(
                    &value.clone().into_value(),
                    scope_id,
                    parent_scope,
                ),
                StmtFlow::None
                | StmtFlow::Break(None)
                | StmtFlow::Continue => {}
            }
        }

        if let Err(error) = &result
            && error.abort.is_none() && error.propagated {
            self.transfer_owned_host_resources_in_runtime_error(error, scope_id, parent_scope);
        }
        let cleanup = self.exit_owned_host_scope(scope_id);
        match (result, cleanup) {
            (_, Err(error)) if error.abort.as_ref().is_some_and(|signal| signal.force) => Err(error),
            (Err(primary), Err(secondary)) => {
                self.report_cleanup_error(&secondary, call_span);
                Err(primary)
            }
            (Ok(flow @ (StmtFlow::Propagate(_) | StmtFlow::Return(LoweredValue::ResultErr(_)))), Err(secondary)) => {
                self.report_cleanup_error(&secondary, call_span);
                Ok(flow)
            }
            (Err(error), _) | (Ok(_), Err(error)) => Err(error),
            (Ok(flow), Ok(())) => Ok(flow),
        }
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
        self.sync_indexed_root_slots(slots, call_span)?;
        let result = self.eval_indexed_stmt_inner(execution, instruction, header, slots, call_span);
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
        header: &FunctionHeader,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<StmtFlow, RuntimeError> {
        let (tag, mut payload) = indexed_value(execution.instruction_id(instruction), call_span)?;
        match tag {
            FullTag::StmtDefaultParameter => {
                let slot = indexed_decode::<usize>(&mut payload, execution, call_span)?;
                let value = indexed_raw(&mut payload, call_span)?;
                let kind = indexed_decode::<LoweredType>(&mut payload, execution, call_span)?;
                let check = indexed_decode::<Option<LoweredTypeCheck>>(&mut payload, execution, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                if matches!(slots[slot], LoweredValue::OmittedArgument) {
                    let value = match self.eval_indexed_expr(execution, value, slots, span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(self.pending_value_block_flow.take().unwrap_or(StmtFlow::Propagate(value))),
                    };
                    validate_parameter_default(&value, kind, check.as_ref(), span)?;
                    slots[slot] = value;
                }
                Ok(StmtFlow::None)
            }
            FullTag::StmtLet => {
                let slot = indexed_decode::<usize>(&mut payload, execution, call_span)?;
                self.declare_recursive_context_slot(slots, slot);
                let value = indexed_raw(&mut payload, call_span)?;
                indexed_finish(payload, call_span)?;
                match self.eval_indexed_expr(execution, value, slots, call_span)? {
                    ControlFlow::Continue(value) => slots[slot] = value,
                    ControlFlow::Break(value) => return Ok(StmtFlow::Return(value)),
                }
                Ok(StmtFlow::None)
            }
            FullTag::StmtWith => {
                let (_, mut binding_words) = execution.block(&mut payload, BLOCK_LIST).map_err(|error| indexed_error(error, call_span))?;
                let count = indexed_raw(&mut binding_words, call_span)? as usize;
                let mut bindings = Vec::with_capacity(count);
                for _ in 0..count {
                    let slot = indexed_decode::<usize>(&mut binding_words, execution, call_span)?;
                    let value = indexed_raw(&mut binding_words, call_span)?;
                    self.declare_recursive_context_slot(slots, slot);
                    bindings.push((slot, value));
                }
                indexed_finish(binding_words, call_span)?;
                let body = indexed_raw(&mut payload, call_span)?;
                let else_param_slot = indexed_decode::<Option<usize>>(&mut payload, execution, call_span)?;
                let else_body = indexed_raw(&mut payload, call_span)?;
                let captures = indexed_decode::<Vec<usize>>(&mut payload, execution, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let scope_id = self.enter_owned_host_scope();
                let result = (|| {
                    for (slot, value) in bindings {
                        let value = self.eval_indexed_expr(execution, value, slots, span)?;
                        let value = match value {
                            ControlFlow::Continue(value) => value,
                            ControlFlow::Break(value) => {
                                match self.pending_value_block_flow.take() {
                                    Some(StmtFlow::Propagate(value)) => value,
                                    Some(flow) => return Ok(flow),
                                    None => value,
                                }
                            }
                        };
                        match value {
                            LoweredValue::ResultErr(error) => {
                                self.pending_traceback = None;
                                if let Some(slot) = else_param_slot { slots[slot] = LoweredValue::Error(error); }
                                return self.eval_indexed_statement_block(execution, else_body, header, slots, span);
                            }
                            LoweredValue::ResultOk(value) => slots[slot] = *value,
                            value => slots[slot] = value,
                        }
                    }
                    self.eval_indexed_statement_block(execution, body, header, slots, span)
                })();
                self.finish_indexed_pattern_scope(scope_id, &captures, slots, result)
            }
            FullTag::StmtGuard => {
                let target = indexed_decode::<LoweredCompTarget>(&mut payload, execution, call_span)?;
                self.declare_recursive_context_target(slots, &target);
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
                        bind_lowered_comp_target(&target, *value, slots, span)?;
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
                let target = indexed_decode::<LoweredCompTarget>(&mut payload, execution, call_span)?;
                self.declare_recursive_context_target(slots, &target);
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let source = match self.eval_indexed_expr(execution, source, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(StmtFlow::Return(value)),
                };
                bind_lowered_comp_target(&target, source, slots, span)?;
                Ok(StmtFlow::None)
            }
            FullTag::StmtLetInt => {
                let slot = indexed_decode::<usize>(&mut payload, execution, call_span)?;
                self.declare_recursive_context_slot(slots, slot);
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
                self.declare_recursive_context_slot(slots, slot);
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
                let check = indexed_decode::<Option<LoweredTypeCheck>>(&mut payload, execution, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let (value, singleton) = indexed_assignment_operand(execution, value, op, call_span)?;
                // A plain overwrite offers the slot to the value expression, so
                // an accumulating call like `m = m.set(k, v)` can update the map
                // in place instead of copying it into a second map.
                let saved = self.consuming_receiver;
                self.consuming_receiver = (op == AssignOp::Set && check.is_none()).then_some(slot);
                let evaluated = self.eval_indexed_expr(execution, value, slots, call_span);
                self.consuming_receiver = saved;
                let value = match evaluated? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(StmtFlow::Return(value)),
                };
                self.check_recursive_context_assignment(slots, slot, &value, span)?;
                slots[slot] = if let Some(check) = check.as_ref() {
                    checked_indexed_assignment(&slots[slot], op, value, singleton, check, span)?
                } else { match op {

                    AssignOp::Set => value,
                    _ => apply_indexed_assignment(&mut slots[slot], op, value, singleton, span)?,
                }};
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
                let (value, singleton) = indexed_assignment_operand(execution, value, op, call_span)?;
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
                self.check_recursive_context_assignment(slots, slot, &value, span)?;
                let current = super::super::lowered_ops::lowered_record_field_mut(
                    &mut slots[slot], Name::intern(field.as_ref()), span,
                )?;
                *current = apply_indexed_assignment(current, op, value, singleton, span)?;
                Ok(StmtFlow::None)
            }
            FullTag::StmtAssignPath => {
                let slot = indexed_decode::<usize>(&mut payload, execution, call_span)?;
                let path = decode_assign_path(execution, &mut payload, call_span)?;
                let op = indexed_decode::<AssignOp>(&mut payload, execution, call_span)?;
                let value = indexed_raw(&mut payload, call_span)?;
                let check = indexed_decode::<Option<LoweredTypeCheck>>(&mut payload, execution, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let mut selectors = Vec::with_capacity(path.len());
                for step in path {
                    selectors.push(match step {
                        IndexedAssignStep::Field(name) => ResolvedAssignStep::Field(name),
                        IndexedAssignStep::Index(expr) => {
                            let selector = match self.eval_indexed_expr(execution, expr, slots, call_span)? {
                                ControlFlow::Continue(value) => value,
                                ControlFlow::Break(value) => return Ok(StmtFlow::Return(value)),
                            };
                            resolve_assign_index(selector, span)?
                        }
                    });
                }
                let (value, singleton) = indexed_assignment_operand(execution, value, op, call_span)?;
                let value = match self.eval_indexed_expr(execution, value, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(StmtFlow::Return(value)),
                };
                self.check_recursive_context_assignment(slots, slot, &value, span)?;
                apply_indexed_path_assignment(&mut slots[slot], &selectors, op, value, singleton, check.as_ref(), span)?;

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
            FullTag::StmtValue => {
                let value = indexed_raw(&mut payload, call_span)?;
                indexed_finish(payload, call_span)?;
                match self.eval_indexed_expr(execution, value, slots, call_span)? {
                    ControlFlow::Continue(value) => Ok(StmtFlow::Value(value)),
                    ControlFlow::Break(value) => Ok(StmtFlow::Propagate(value)),
                }
            }
            FullTag::StmtAssert => {
                let value = indexed_raw(&mut payload, call_span)?;
                let message = match indexed_raw(&mut payload, call_span)? {
                    0 => None,
                    1 => Some(indexed_raw(&mut payload, call_span)?),
                    _ => return Err(RuntimeError::new("indexed-ir", "invalid assertion message option").with_span(call_span)),
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
                        _ => return Err(RuntimeError::new("type-error", "assert message requires Str").with_span(span)),
                    });
                }
                let expression = self.sources.span_text(span);
                let (operands, reached) = match &failure {
                    AssertionFailure::Operands(left, right) => (Some((left, right)), None),
                    AssertionFailure::Reached(reached) => (None, Some(reached.as_str())),
                    AssertionFailure::False => (None, None),
                };
                let message = assertion_failure_message(expression.as_deref(), operands, reached, context.as_deref());
                Err(checked_assertion_failure(message, span))
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
            FullTag::StmtPatternIf => {
                let (_, mut branches) = execution.block(&mut payload, BLOCK_LIST).map_err(|error| indexed_error(error, call_span))?;
                let count = indexed_raw(&mut branches, call_span)? as usize;
                let mut decoded = Vec::with_capacity(count);
                for _ in 0..count {
                    let condition = indexed_raw(&mut branches, call_span)?;
                    let body = indexed_raw(&mut branches, call_span)?;
                    let captures = indexed_decode::<Vec<usize>>(&mut branches, execution, call_span)?;
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
                            ControlFlow::Continue(true) => { selected = true; }
                        }
                        self.eval_indexed_statement_block(execution, body, header, slots, span)
                    })();
                    let flow = self.finish_indexed_pattern_scope(scope_id, &captures, slots, result)?;
                    if selected || !matches!(flow, StmtFlow::None) { return Ok(flow); }
                }
                match else_body {
                    Some(body) => self.eval_indexed_statement_block(execution, body, header, slots, span),
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
                    if self.signal_state.shutdown_complete { return Ok(StmtFlow::None); }
                    let scope_id = self.enter_owned_host_scope();
                    let result = (|| {
                        match self.eval_indexed_bool(execution, condition, slots, span)? {
                            ControlFlow::Continue(false) => return Ok(StmtFlow::Break(None)),
                            ControlFlow::Break(value) => return Ok(StmtFlow::Propagate(value)),
                            ControlFlow::Continue(true) => {}
                        }
                        self.eval_indexed_statement_block(execution, body, header, slots, span)
                    })();
                    match self.finish_indexed_pattern_scope(scope_id, &captures, slots, result)? {
                        StmtFlow::None | StmtFlow::Continue => {}
                        StmtFlow::Break(_) => break,
                        flow => return Ok(flow),
                    }
                }
                Ok(StmtFlow::None)
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
                        StmtFlow::Value(value) | StmtFlow::Return(value) => {
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
            FullTag::StmtFor | FullTag::StmtForRecord => {
                let target = if tag == FullTag::StmtForRecord {
                    indexed_decode::<LoweredCompTarget>(&mut payload, execution, call_span)?
                } else {
                    LoweredCompTarget::Slot(indexed_decode::<usize>(&mut payload, execution, call_span)?)
                };
                self.declare_recursive_context_target(slots, &target);
                let iter = indexed_raw(&mut payload, call_span)?;
                let body = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let iter = match self.eval_indexed_expr(execution, iter, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(self.pending_value_block_flow.take().unwrap_or(StmtFlow::Propagate(value))),
                };
                // A producer's items arrive one pull at a time: the loop never
                // holds the whole stream, and stopping it early runs the
                // producer's defers instead of the rest of its body.
                let script_stream =
                    matches!(&iter, LoweredValue::Stream(stream) if stream.script().is_some());
                if script_stream {
                    let LoweredValue::Stream(mut stream) = iter else {
                        unreachable!("checked above")
                    };
                    loop {
                        self.service_pending_signal(span)?;
                        if self.signal_state.shutdown_complete {
                            self.stream_cancel(&mut stream, span)?;
                            return Ok(StmtFlow::None);
                        }
                        let Some(value) = self.stream_next(&mut stream, span)? else {
                            return Ok(StmtFlow::None);
                        };
                        let Some(item) = lowered_value_from_runtime_any(&value) else {
                            return Err(RuntimeError::new(
                                "type-error",
                                format!("stream produced unsupported {}", value.type_name()),
                            )
                            .with_span(span));
                        };
                        if let Err(error) = bind_lowered_comp_target(&target, item, slots, span) {
                            self.stream_cancel(&mut stream, span)?;
                            return Err(error);
                        }
                        match self.eval_indexed_statement_block(
                            execution, body, header, slots, call_span,
                        )? {
                            StmtFlow::None | StmtFlow::Continue => {}
                            flow => {
                                self.stream_cancel(&mut stream, span)?;
                                return Ok(match flow {
                                    StmtFlow::Break(_) => StmtFlow::None,
                                    other => other,
                                });
                            }
                        }
                    }
                }
                let iter = match LoweredScalarCursor::try_new(iter) {
                    Ok(mut cursor) => {
                        while let Some(item) = cursor.next() {
                            self.service_pending_signal(span)?;
                            if self.signal_state.shutdown_complete { return Ok(StmtFlow::None); }
                            bind_lowered_comp_target(&target, item, slots, span)?;
                            match self.eval_indexed_statement_block(execution, body, header, slots, call_span)? {
                                StmtFlow::None | StmtFlow::Continue => {},
                                StmtFlow::Break(_) => break,
                                flow => return Ok(flow),
                            }
                        }
                        return Ok(StmtFlow::None);
                    }
                    Err(iter) => iter,
                };
                if let LoweredValue::Map(entries) = iter {
                    let mut cursor = LoweredMapCursor::new(entries);
                    while let Some(item) = cursor.next() {
                        self.service_pending_signal(span)?;
                        if self.signal_state.shutdown_complete { return Ok(StmtFlow::None); }
                        bind_lowered_comp_target(&target, item, slots, span)?;
                        match self.eval_indexed_statement_block(execution, body, header, slots, call_span)? {
                            StmtFlow::None | StmtFlow::Continue => {},
                            StmtFlow::Break(_) => break,
                            flow => return Ok(flow),
                        }
                    }
                    return Ok(StmtFlow::None);
                }
                let items = self.lowered_list_items(iter, span, "lowered for expected List")?;
                for item in items {
                    self.service_pending_signal(span)?;
                    if self.signal_state.shutdown_complete {
                        return Ok(StmtFlow::None);
                    }
                    bind_lowered_comp_target(&target, item, slots, span)?;
                    match self
                        .eval_indexed_statement_block(execution, body, header, slots, call_span)?
                    {
                        StmtFlow::None | StmtFlow::Continue => {}
                        StmtFlow::Break(_) => break,
                        StmtFlow::Value(value) | StmtFlow::Return(value) => {
                            return Ok(StmtFlow::Return(value));
                        }
                        StmtFlow::Propagate(value) => {
                            return Ok(StmtFlow::Propagate(value));
                        }
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
                            flow @ (StmtFlow::Value(_) | StmtFlow::Return(_) | StmtFlow::Propagate(_)) => {
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
                        flow @ (StmtFlow::Value(_) | StmtFlow::Return(_) | StmtFlow::Propagate(_)) => return Ok(flow),
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
                        StmtFlow::Value(value) | StmtFlow::Return(value) => {
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
                // Producers run on the frame engine, which suspends at a
                // `yield`; the recursive statement evaluator never runs a
                // producer body, so reaching this means a producer ran outside
                // the machine that can stop it.
                indexed_finish(payload, call_span)?;
                Err(RuntimeError::new(
                    "control-flow",
                    "yield reached the recursive evaluator, which cannot suspend",
                )
                .with_span(call_span))
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
                checked_int_binary(op, left, right, call_span)?
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

// A singleton RHS carries its item directly through assignment execution,
// avoiding a temporary list while retaining ordinary RHS-before-update order.
fn indexed_assignment_operand(
    execution: &FullExecution<'_>,
    value: u32,
    op: AssignOp,
    span: Span,
) -> Result<(u32, bool), RuntimeError> {
    if op == AssignOp::Add {
        let (tag, mut payload) = indexed_value(execution.instruction_id(value), span)?;
        if tag == FullTag::ExprList {
            let (_, mut items) = execution.block(&mut payload, BLOCK_LIST)
                .map_err(|error| indexed_error(error, span))?;
            if indexed_raw(&mut items, span)? == 1 {
                let item = indexed_raw(&mut items, span)?;
                indexed_finish(items, span)?;
                indexed_finish(payload, span)?;
                return Ok((item, true));
            }
        }
    }
    Ok((value, false))
}

// Taking a container is safe only after both operands prove a list update.
// Other operations may fail, and defers must still see the original target.
fn apply_indexed_assignment(
    current: &mut LoweredValue,
    op: AssignOp,
    value: LoweredValue,
    singleton: bool,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    if op == AssignOp::Add && matches!(current, LoweredValue::List(_) | LoweredValue::SharedList(_)) {
        if singleton {
            let owned = std::mem::replace(current, LoweredValue::Unit);
            return super::lowered_method_value(owned, "push", vec![value], span);
        }
        if matches!(value, LoweredValue::List(_) | LoweredValue::SharedList(_)) {
            let owned = std::mem::replace(current, LoweredValue::Unit);
            return lowered_assign_value(op, owned, value, span);
        }
    }
    let value = if singleton { LoweredValue::List(vec![value]) } else { value };
    lowered_assign_value(op, current.clone(), value, span)
}

enum ResolvedAssignStep {
    Field(Name),
    Map(MapKey),
    List(i64),
}

fn resolve_assign_index(value: LoweredValue, span: Span) -> Result<ResolvedAssignStep, RuntimeError> {
    Ok(match value {
        LoweredValue::Int(index) => ResolvedAssignStep::List(index),
        value => ResolvedAssignStep::Map(lowered_map_literal_key(&value, span)?),
    })
}

fn validate_indexed_assignment(value: &LoweredValue, check: &LoweredTypeCheck, span: Span) -> Result<(), RuntimeError> {
    if !lowered_value_matches_static_type(value, &check.ty) {
        return Err(RuntimeError::new("type-error", format!("assignment violates UInt constraint in {}", check.name)).with_span(span));
    }
    Ok(())
}

fn checked_indexed_assignment(
    current: &LoweredValue,
    op: AssignOp,
    value: LoweredValue,
    singleton: bool,
    check: &LoweredTypeCheck,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    let rhs = if singleton { LoweredValue::List(vec![value]) } else { value };
    let replacement = if op == AssignOp::Set { rhs } else { lowered_assign_value(op, current.clone(), rhs, span)? };
    validate_indexed_assignment(&replacement, check, span)?;
    Ok(replacement)
}

// Validate the complete path before copying or rebuilding an ancestor. The root
// is observed after the RHS so unrelated changes made by either operand survive.
fn apply_indexed_path_assignment(
    root: &mut LoweredValue,
    path: &[ResolvedAssignStep],
    op: AssignOp,
    value: LoweredValue,
    singleton: bool,
    check: Option<&LoweredTypeCheck>,
    span: Span,
) -> Result<(), RuntimeError> {
    let mut selected = &*root;
    let mut inline_field = None;
    for (position, step) in path.iter().enumerate() {
        if let ResolvedAssignStep::Field(name) = step
            && matches!(selected, LoweredValue::Stats { .. } | LoweredValue::StatsBlob(_))
            && position + 1 == path.len()
        {
            inline_field = Some(lowered_record_field_value(selected, name.as_str().as_str())
                .ok_or_else(|| RuntimeError::new("missing-field", name.to_string()).with_span(span))?);
            break;
        }
        selected = match (step, selected) {
            (ResolvedAssignStep::Field(name), record) => lowered_record_field(record, name.as_str().as_str())
                .ok_or_else(|| RuntimeError::new("missing-field", name.to_string()).with_span(span))?,
            (ResolvedAssignStep::Map(key), LoweredValue::Map(map)) => {
                super::super::lowered_ops::require_lowered_map_key_domain(map, key.as_ref(), span)?;
                if position + 1 == path.len() && op == AssignOp::Set { break; }
                map.get(key).ok_or_else(|| RuntimeError::new("missing-field", format!("{key:?}")).with_span(span))?
            }
            (ResolvedAssignStep::List(index), LoweredValue::Map(map)) => {
                super::super::lowered_ops::require_lowered_map_key_domain(map, crate::map_key::MapKeyRef::Int(*index), span)?;
                if position + 1 == path.len() && op == AssignOp::Set { break; }
                map.get(&MapKey::Int(*index)).ok_or_else(|| RuntimeError::new("missing-field", index.to_string()).with_span(span))?
            }
            (ResolvedAssignStep::List(index), LoweredValue::List(list)) => list.get(*index as usize)
                .ok_or_else(|| RuntimeError::new("index-out-of-range", "list index").with_span(span))?,
            (ResolvedAssignStep::List(index), LoweredValue::SharedList(list)) => list.get(*index as usize)
                .ok_or_else(|| RuntimeError::new("index-out-of-range", "list index").with_span(span))?,
            _ => return Err(RuntimeError::new("type-error", "assignment path requires a compatible collection").with_span(span)),
        };
    }
    let selected = inline_field.as_ref().unwrap_or(selected);
    // Fallible arithmetic finishes before mutable descent. List concatenation is
    // safe to consume in place once both operand types have been established.
    let consume_list = check.is_none() && op == AssignOp::Add && matches!(selected, LoweredValue::List(_) | LoweredValue::SharedList(_))
        && (singleton || matches!(value, LoweredValue::List(_) | LoweredValue::SharedList(_)));
    let mut operand = Some(value);
    let replacement = if consume_list { None } else {
        let value = operand.take().expect("assignment operand");
        let rhs = if singleton { LoweredValue::List(vec![value]) } else { value };
        Some(if op == AssignOp::Set { rhs } else { lowered_assign_value(op, selected.clone(), rhs, span)? })
    };
    if let (Some(check), Some(replacement)) = (check, replacement.as_ref()) {
        validate_indexed_assignment(replacement, check, span)?;
    }
    let mut selected = root;
    for (position, step) in path.iter().enumerate() {
        selected = match step {
            ResolvedAssignStep::Field(name) => super::super::lowered_ops::lowered_record_field_mut(selected, *name, span)?,
            ResolvedAssignStep::Map(key) => {
                let LoweredValue::Map(map) = selected else { unreachable!("validated map path") };
                let map = Arc::make_mut(map);
                if position + 1 == path.len() && op == AssignOp::Set {
                    map.insert(key.clone(), replacement.expect("set replacement"));
                    return Ok(());
                }
                map.get_mut(key).expect("validated map key")
            }
            ResolvedAssignStep::List(index) => match selected {
                LoweredValue::Map(map) => {
                    let map = Arc::make_mut(map);
                    if position + 1 == path.len() && op == AssignOp::Set {
                        map.insert(MapKey::Int(*index), replacement.expect("set replacement"));
                        return Ok(());
                    }
                    map.get_mut(&MapKey::Int(*index)).expect("validated map key")
                }
                LoweredValue::List(list) => &mut list[*index as usize],
                LoweredValue::SharedList(list) => &mut Arc::make_mut(list)[*index as usize],
                _ => unreachable!("validated list path"),
            },
        };
    }
    *selected = match replacement {
        Some(value) => value,
        None => apply_indexed_assignment(selected, op, operand.expect("consuming list operand"), singleton, span)?,
    };
    Ok(())
}

/// Whether two values are the same container through shared backing.
///
/// A consuming call only takes the value out of its slot when the slot still
/// holds the very container the receiver was read from: an argument may have
/// replaced it, and that replacement must be what the slot ends up with.
pub(super) fn lowered_shares_backing(left: &LoweredValue, right: &LoweredValue) -> bool {
    match (left, right) {
        (LoweredValue::Record(left), LoweredValue::Record(right))
        | (LoweredValue::Module(left), LoweredValue::Module(right)) => Arc::ptr_eq(left, right),
        (LoweredValue::RecordVec(left), LoweredValue::RecordVec(right)) => Arc::ptr_eq(left, right),
        (LoweredValue::Map(left), LoweredValue::Map(right)) => Arc::ptr_eq(left, right),
        (LoweredValue::SharedList(left), LoweredValue::SharedList(right)) => {
            Arc::ptr_eq(left, right)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::check::Checker;
    use crate::source::SourceMap;
    use crate::syntax::parser::Parser;

    #[test]
    fn parametric_records_keep_concrete_schemas_in_both_call_routes() {
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
            let frames = run_program_through_route(source, false);
            let recursive = run_program_through_route(source, true);
            assert_eq!(frames, recursive);
            assert_eq!(frames.0, 0);
            assert_eq!(frames.1, b"7\n7\n10\n");
            assert!(frames.2.is_empty());
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
            let frames = run_program_through_route(source, false);
            let recursive = run_program_through_route(source, true);
            assert_eq!(frames, recursive);
            assert_eq!(frames.0, 0);
            assert_eq!(frames.1, b"7\n21\ncontext\n");
            assert!(frames.2.is_empty());
        });
    }

    #[test]
    fn assignment_path_copies_only_shared_ancestors() {
        let span = Span::new(crate::source::SourceId::new(0), 0, 0);
        let list = || LoweredValue::SharedList(Arc::new(vec![LoweredValue::Int(1), LoweredValue::Int(2)]));
        let mut root = LoweredValue::Map(Arc::new(BTreeMap::from([
            (MapKey::from("selected"), list()), (MapKey::from("untouched"), list()),
        ])));
        let backing = |root: &LoweredValue| {
            let LoweredValue::Map(map) = root else { unreachable!() };
            let LoweredValue::SharedList(selected) = &map[&MapKey::from("selected")] else { unreachable!() };
            let LoweredValue::SharedList(untouched) = &map[&MapKey::from("untouched")] else { unreachable!() };
            (Arc::as_ptr(map), Arc::as_ptr(selected), Arc::as_ptr(untouched))
        };
        let path = [ResolvedAssignStep::Map("selected".into()), ResolvedAssignStep::List(1)];
        let original = backing(&root);
        apply_indexed_path_assignment(&mut root, &path, AssignOp::Set, LoweredValue::Int(9), false, None, span).unwrap();
        assert_eq!(backing(&root), original);
        let alias = root.clone();
        apply_indexed_path_assignment(&mut root, &path, AssignOp::Set, LoweredValue::Int(10), false, None, span).unwrap();
        let changed = backing(&root);
        assert_ne!(changed.0, original.0);
        assert_ne!(changed.1, original.1);
        assert_eq!(changed.2, original.2);
        assert_eq!(backing(&alias), original);
        let invalid = [ResolvedAssignStep::Map("selected".into()), ResolvedAssignStep::List(99)];
        assert!(apply_indexed_path_assignment(&mut root, &invalid, AssignOp::Set, LoweredValue::Int(0), false, None, span).is_err());
        assert_eq!(backing(&root), changed);
    }

    #[test]
    fn assignment_path_reuses_unique_storage_and_preserves_aliases() {
        let span = Span::new(crate::source::SourceId::new(0), 0, 0);
        let path = [ResolvedAssignStep::List(1)];
        let mut owned = LoweredValue::List(vec![LoweredValue::Int(1), LoweredValue::Int(2)]);
        let LoweredValue::List(list) = &owned else { unreachable!() };
        let backing = list.as_ptr();
        apply_indexed_path_assignment(&mut owned, &path, AssignOp::Set, LoweredValue::Int(9), false, None, span).unwrap();
        let LoweredValue::List(list) = &owned else { unreachable!() };
        assert_eq!(list.as_ptr(), backing);
        let mut shared = LoweredValue::SharedList(Arc::new(vec![LoweredValue::Int(1), LoweredValue::Int(2)]));
        let LoweredValue::SharedList(list) = &shared else { unreachable!() };
        let backing = Arc::as_ptr(list);
        apply_indexed_path_assignment(&mut shared, &path, AssignOp::Set, LoweredValue::Int(9), false, None, span).unwrap();
        let LoweredValue::SharedList(list) = &shared else { unreachable!() };
        assert_eq!(Arc::as_ptr(list), backing);
        let alias = shared.clone();
        apply_indexed_path_assignment(&mut shared, &path, AssignOp::Add, LoweredValue::Int(1), false, None, span).unwrap();
        assert_eq!(alias, LoweredValue::List(vec![LoweredValue::Int(1), LoweredValue::Int(9)]));
        assert_eq!(shared, LoweredValue::List(vec![LoweredValue::Int(1), LoweredValue::Int(10)]));
        let LoweredValue::SharedList(list) = &shared else { unreachable!() };
        let backing = Arc::as_ptr(list);
        assert!(apply_indexed_path_assignment(&mut shared, &path, AssignOp::Div, LoweredValue::Int(0), false, None, span).is_err());
        let LoweredValue::SharedList(list) = &shared else { unreachable!() };
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
    /// Compare the ordinary release call route with explicit frames on one
    /// program covering calls, captures, Results, recursion, and streams.
    #[test]
    fn both_call_routes_agree_on_the_same_public_program() {
        crate::runtime::eval::run_eval(both_call_routes_agree_on_the_same_public_program_inner);
    }

    fn both_call_routes_agree_on_the_same_public_program_inner() {
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
      print f"row ${row}"
    }
    print f"${nested(-5, 2)} ${with_defaults(1)} ${with_defaults(1, 7, 4, 5)}"
    print f"${is_even(7)} ${is_odd(7)} ${is_even(8)}"
    print f"${scaled("41")?}"
    print f"${add_to_total(5)} ${add_to_total(6)} ${total}"
    match scaled("nope") {
      Ok(value) => print f"ok ${value}"
      Err(error) => print f"rejected ${error.message}"
    }
    let values: List[Int] = [1, 2, 3, 4]
    print f"${values |> where . > 1 |> map . * factor |> sum}"
    "#;
        let frames = run_program_through_route(source, false);
        let recursive = run_program_through_route(source, true);
        assert_eq!(frames, recursive, "the two call routes disagree");
        assert_eq!(frames.0, 0);
        assert_eq!(
            frames.1.as_slice(),
            concat!(
                "row 0\nrow 3\nrow 6\n",
                "24 8 17\n",
                "false true true\n",
                "123\n",
                "5 11 11\n",
                "rejected invalid integer `nope`\n",
                "27\n",
            )
            .as_bytes()
        );
        assert!(frames.2.is_empty());
    }

    fn run_program_through_route(source: &str, force_recursive: bool) -> (u8, Vec<u8>, Vec<u8>) {
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("call-routes.xsh", source);
        let parsed = Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        Checker::check_compact_declarations(&parsed.arena);
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
        let plan = evaluator
            .prepare_compact_indexed_only(&parsed.arena, source_id)
            .expect("the call-route program prepares");
        let output = if force_recursive {
            crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(|| {
                evaluator.eval_installed_compact_indexed_only(plan)
            })
        } else {
            evaluator.eval_installed_compact_indexed_only(plan)
        };
        let output = match output {
            Ok(output) => output,
            // The error arm hands the evaluator back, which has no `Debug`.
            Err(_) => panic!("the call-route program installs and runs"),
        };
        (output.status, output.stdout, output.stderr)
    }
}

// Defaults retain their private omission marker until the actual callee binds
// its slots. Callable aliases therefore keep lowered values across dispatch.
fn indexed_callable_identity(callee: &LoweredValue, span: Span) -> Result<(LoweredFunctionKey, LoweredFunctionKind), RuntimeError> {
    let (function, kind) = match callee {
        LoweredValue::Pure(function) => (function, LoweredFunctionKind::Pure),
        LoweredValue::Proc(function) => (function, LoweredFunctionKind::Proc),
        other => return Err(RuntimeError::new("type-error", format!("dynamic call expected Pure or Proc, found {}", other.type_name())).with_span(span)),
    };
    let key = function.as_name().map(LoweredFunctionKey::Name)
        .or_else(|| function.as_qualified().map(LoweredFunctionKey::Qualified)).expect("callable identity is interned");
    Ok((key, kind))
}
