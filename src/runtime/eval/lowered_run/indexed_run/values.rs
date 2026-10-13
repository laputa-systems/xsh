use super::{
    Arc, AssertionFailure, BinaryOp, ControlFlow, Evaluator,
    FullExecution, FullTag, LoweredValue, Name, RuntimeError, Span, StmtFlow,
    TraceKind, TracePayload, Value, bytes_contains, checked_int_binary, indexed_error,
    indexed_value,
    lowered_binary_value, lowered_contains_value,
    lowered_path_method_value, lowered_record_field_value, lowered_record_vec_or_stats,
    lowered_result_err_value, lowered_status_segment_record,
    lowered_str_byte_at_value, lowered_str_byte_len_value, lowered_str_count_lines_value,
    lowered_str_predicate_text, lowered_str_value, lowered_trim_is_empty_value,
    lowered_trim_str_predicate_value, lowered_value_from_runtime_any,
};
use crate::runtime::eval::indexed::full::schema::{BoolView, IntView};

// Diagnostics retain only bounded scalar text; reporting never traverses or
// materializes containers and never reevaluates an operand.
pub(super) fn assertion_operand_text(value: &LoweredValue, span: Span) -> String {
    match value {
        LoweredValue::Str(value) => bounded_assertion_text(value, 160),
        LoweredValue::StrView(value) => bounded_assertion_text(value.as_str(), 160),
        LoweredValue::Path(value) => {
            let bytes = &value.bytes[..value.bytes.len().min(640)];
            let mut text = bounded_assertion_text(&String::from_utf8_lossy(bytes), 160);
            if bytes.len() < value.bytes.len() && !text.ends_with('…') {
                text.push('…');
            }
            text
        }
        LoweredValue::Error(value) => match value.as_ref() {
            Value::Error(error) => bounded_assertion_text(&error.message, 160),
            _ => "<Error>".into(),
        },
        LoweredValue::Int(_)
        | LoweredValue::Float(_)
        | LoweredValue::Duration(_)
        | LoweredValue::Bool(_)
        | LoweredValue::Status(_) => {
            let mut text = String::new();
            if super::super::push_lowered_display(&mut text, value, span).is_err() {
                return format!("<{}>", value.type_name());
            }
            bounded_assertion_text(&text, 160)
        }
        _ => format!("<{}>", value.type_name()),
    }
}

pub(super) fn bounded_assertion_text(text: &str, limit: usize) -> String {
    let mut chars = text.chars();
    let mut output: String = chars.by_ref().take(limit).collect();
    if chars.next().is_some() {
        output.push('…');
    }
    output
}

pub(super) fn comparison_failure_text(
    op: BinaryOp,
    left: &LoweredValue,
    right: &LoweredValue,
    span: Span,
) -> String {
    let (left_text, right_text) = if matches!(op, BinaryOp::In | BinaryOp::NotIn) {
        use crate::runtime::eval::lowered_ops::lowered_assertion_value_detail;
        (
            bounded_assertion_text(&lowered_assertion_value_detail(left), 160),
            bounded_assertion_text(&lowered_assertion_value_detail(right), 160),
        )
    } else {
        (
            assertion_operand_text(left, span),
            assertion_operand_text(right, span),
        )
    };
    let operator = match op {
        BinaryOp::Eq => "==",
        BinaryOp::Ne => "!=",
        BinaryOp::Lt => "<",
        BinaryOp::Le => "<=",
        BinaryOp::Gt => ">",
        BinaryOp::Ge => ">=",
        BinaryOp::In => "in",
        BinaryOp::NotIn => "not in",
        _ => unreachable!(),
    };
    let label = if matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
        "comparison"
    } else if matches!(op, BinaryOp::In | BinaryOp::NotIn) {
        "membership comparison"
    } else {
        "ordering comparison"
    };
    format!("{label} failed: {left_text} {operator} {right_text}")
}

// The one formatter for bare Bool statements and `assert`: the expression,
// then the optional message, then the reached operands of the failure.
pub(super) fn assertion_failure_message(
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
        message.push_str(&format!(
            "\nleft: {}\nright: {}",
            lowered_assertion_value_detail(left),
            lowered_assertion_value_detail(right)
        ));
        if let (Some(left), Some(right)) = (lowered_str_value(left), lowered_str_value(right))
            && left != right
            && left.len() <= 4096
            && right.len() <= 4096
            && (left.contains('\n') || right.contains('\n'))
        {
            message.push_str("\ndiff:\n");
            message.push_str(&diffy::create_patch(left, right).to_string());
        }
    }
    message
}

pub(super) fn assertion_comparison_op(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::Eq
            | BinaryOp::Ne
            | BinaryOp::Lt
            | BinaryOp::Le
            | BinaryOp::Gt
            | BinaryOp::Ge
            | BinaryOp::In
            | BinaryOp::NotIn
    )
}

// Only checked language failures cross a local Result boundary. Runtime faults
// and abort signals retain their original escape behavior.
pub(super) fn capture_checked_error(mut error: RuntimeError) -> Result<LoweredValue, RuntimeError> {
    if error.abort.is_some() || !error.propagated {
        return Err(error);
    }
    error.propagated = false;
    error.scope_cleanup = false;
    let value = if let Some(mut original) = error.propagated_run_error.take() {
        original.contexts = error.contexts;
        original.cause = error.cause;
        Value::RunError(original)
    } else {
        Value::Error(Box::new(error))
    };
    Ok(LoweredValue::ResultErr(Box::new(value)))
}

/// A condition's truth: a Bool, or whether a Status succeeded.
pub(super) fn lowered_condition_bool(value: LoweredValue, span: Span) -> Result<bool, RuntimeError> {
    match value {
        LoweredValue::Bool(value) => Ok(value),
        LoweredValue::Status(status) => Ok(status.success),
        _ => {
            Err(RuntimeError::new("type-error", "lowered expression expected Bool").with_span(span))
        }
    }
}

/// The operand of `??` when it supplies the value, or `None` when the fallback
/// must be evaluated.
pub(super) fn lowered_fallback_value(value: LoweredValue) -> Option<LoweredValue> {
    match value {
        LoweredValue::ResultOk(value) => Some(*value),
        LoweredValue::ResultErr(_) | LoweredValue::Null => None,
        value => Some(value),
    }
}

/// `Err(error, cause: ..)` once its operands are evaluated.
pub(super) fn lowered_err_with_cause(
    error: Value,
    cause: Option<LoweredValue>,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    let error = match cause {
        Some(cause) => error
            .with_error_cause(cause.into_value())
            .map_err(|error| error.with_span(span))?,
        None => error,
    };
    Ok(LoweredValue::ResultErr(Box::new(error)))
}

/// Whether one link of a comparison chain holds.
pub(super) fn comparison_link_holds(
    op: BinaryOp,
    left: &LoweredValue,
    right: &LoweredValue,
    span: Span,
) -> Result<bool, RuntimeError> {
    Ok(lowered_binary_value(op, left.clone(), right.clone(), span)? != LoweredValue::Bool(false))
}

pub(super) fn finish_record_entries(mut fields: Vec<(Name, LoweredValue)>) -> LoweredValue {
    fields.sort_unstable_by_key(|(name, _)| *name);
    lowered_record_vec_or_stats(fields)
}

pub(super) fn lowered_comp_iterable(value: LoweredValue, span: Span) -> Result<LoweredValue, RuntimeError> {
    match value {
        LoweredValue::ResultOk(value) => lowered_comp_iterable(*value, span),
        LoweredValue::ResultErr(error) => Err(super::super::runtime_error_from_value(*error, span)),
        value => Ok(value),
    }
}

// Defaults retain their private omission marker until the actual callee binds
// its slots. Callable aliases therefore keep lowered values across dispatch.
/// The value of `Proc.call`: the `Result` the proc returned, or `Ok` of
/// anything else it returned. The test reads the value because a dynamic
/// handle's proc is not known where the call is checked.
pub(super) fn proc_call_result(value: LoweredValue) -> LoweredValue {
    match value {
        LoweredValue::ResultOk(_) | LoweredValue::ResultErr(_) => value,
        value => LoweredValue::ResultOk(Box::new(value)),
    }
}

impl Evaluator {
    /// `?` on an evaluated operand: its success value, or the propagation its
    /// error becomes.
    pub(super) fn indexed_question_value(
        &mut self,
        value: LoweredValue,
        span: Span,
    ) -> Result<Result<LoweredValue, LoweredValue>, RuntimeError> {
        match value {
            LoweredValue::ResultOk(value) => Ok(Ok(*value)),
            error @ LoweredValue::ResultErr(_) => {
                Ok(Err(self.lowered_question_propagation_value(error, span)?))
            }
            _ => {
                Err(RuntimeError::new("type-error", "lowered `?` expected Result").with_span(span))
            }
        }
    }

    /// Dispatches a method call, tracing it when a tool asked for events.
    pub(super) fn eval_indexed_method_dispatch(
        &mut self,
        receiver: LoweredValue,
        name: &str,
        values: Vec<LoweredValue>,
        span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        if !self.trace_enabled {
            let mut result = self.eval_lowered_method_dispatch(receiver, name, values, &span)?;
            self.register_returned_lowered_streams(&mut result, span)?;
            return Ok(result);
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
        let mut result = result?;
        self.register_returned_lowered_streams(&mut result, span)?;
        Ok(result)
    }

    pub(super) fn preserve_lexical_expression_flow<T>(
        &mut self,
        flow: StmtFlow,
    ) -> ControlFlow<LoweredValue, T> {
        let value = match &flow {
            StmtFlow::Value(value) | StmtFlow::Return(value) | StmtFlow::Propagate(value) => {
                value.clone()
            }
            StmtFlow::Break(value) => value.clone().unwrap_or(LoweredValue::Unit),
            StmtFlow::Continue | StmtFlow::None => LoweredValue::Unit,
        };
        self.pending_value_block_flow = Some(flow);
        ControlFlow::Break(value)
    }

    // Bare Bool statements and `assert` share this failure route: one message
    // formatter and ordinary checked-error propagation, so retry, try, and
    // the top-level traceback see identical failures from both forms.
    pub(super) fn indexed_assertion_failed(
        &mut self,
        failure: AssertionFailure,
        context: Option<&str>,
        span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        let (operands, reached) = match &failure {
            AssertionFailure::Operands(left, right) => (Some((left, right)), None),
            AssertionFailure::Reached(reached) => (None, Some(reached.as_str())),
            AssertionFailure::False => (None, None),
        };
        let message =
            assertion_failure_message(self.sources.span_text(span), operands, reached, context);
        let error = crate::runtime::eval::modules::assertion_error(message, Some(span));
        self.lowered_question_propagation_value(lowered_result_err_value(error), span)
    }

    pub(super) fn indexed_field_value(
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
                let errno = match value.as_ref() {
                    Value::Error(error) => error.errno(),
                    _ => None,
                };
                let (kind, message) = value
                    .error_kind()
                    .zip(value.error_message())
                    .map(|(kind, message)| (kind.to_owned(), message.to_owned()))
                    .ok_or_else(|| {
                        RuntimeError::new("type-error", "field access expected Error")
                            .with_span(span)
                    })?;
                match name {
                    "kind" => Ok(LoweredValue::Str(kind.into())),
                    "message" => Ok(LoweredValue::Str(message.into())),
                    "errno" => Ok(errno.map_or(LoweredValue::Null, LoweredValue::Int)),
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
            LoweredValue::FsLock(lock) => match name {
                "path" => Ok(LoweredValue::Path(lock.path.clone())),
                "shared" => Ok(LoweredValue::Bool(lock.shared)),
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

    pub(super) fn indexed_borrowed_field_value(
        &mut self,
        base: &LoweredValue,
        name: &str,
        span: Span,
    ) -> Result<Option<LoweredValue>, RuntimeError> {
        match base {
            LoweredValue::Record(_)
            | LoweredValue::Module(_)
            | LoweredValue::RecordVec(_)
            | LoweredValue::Stats { .. }
            | LoweredValue::StatsBlob(_) => lowered_record_field_value(base, name)
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

    pub(super) fn eval_indexed_typed_int(
        &mut self,
        execution: &FullExecution<'_>,
        instruction: u32,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<ControlFlow<LoweredValue, i64>, RuntimeError> {
        match eval_indexed_typed_scalar(execution, instruction, slots, call_span, true)? {
            TypedScalarValue::Int(value) => Ok(ControlFlow::Continue(value)),
            TypedScalarValue::Bool(_) => Err(RuntimeError::new(
                "indexed-ir",
                "typed integer evaluator produced Bool",
            )
            .with_span(call_span)),
        }
    }

    pub(super) fn eval_indexed_typed_bool(
        &mut self,
        execution: &FullExecution<'_>,
        instruction: u32,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<ControlFlow<LoweredValue, bool>, RuntimeError> {
        match eval_indexed_typed_scalar(execution, instruction, slots, call_span, false)? {
            TypedScalarValue::Bool(value) => Ok(ControlFlow::Continue(value)),
            TypedScalarValue::Int(_) => Err(RuntimeError::new(
                "indexed-ir",
                "typed boolean evaluator produced Int",
            )
            .with_span(call_span)),
        }
    }

    pub(super) fn eval_indexed_bool(
        &mut self,
        execution: &FullExecution<'_>,
        instruction: u32,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<ControlFlow<LoweredValue, bool>, RuntimeError> {
        let (tag, _) = indexed_value(execution.instruction_id(instruction), call_span)?;
        if matches!(
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
        ) {
            return self.eval_indexed_typed_bool(execution, instruction, slots, call_span);
        }
        match self.eval_indexed_expr(execution, instruction, slots, call_span)? {
            ControlFlow::Break(value) => Ok(ControlFlow::Break(value)),
            ControlFlow::Continue(value) => {
                lowered_condition_bool(value, call_span).map(ControlFlow::Continue)
            }
        }
    }
}

enum TypedScalarValue {
    Int(i64),
    Bool(bool),
}

enum TypedScalarWork {
    Int(u32),
    Bool(u32),
    ApplyIntBinary { op: BinaryOp, span: Span },
    ApplyBoolNot,
    ApplyBoolBranch { op: FullTag, right: u32 },
    ApplyBoolIntCompare { op: BinaryOp },
    ApplyIntByteAt {
        slot: usize,
        default: Option<u32>,
        span: Span,
    },
}

fn eval_indexed_typed_scalar(
    execution: &FullExecution<'_>,
    instruction: u32,
    slots: &mut [LoweredValue],
    call_span: Span,
    integer: bool,
) -> Result<TypedScalarValue, RuntimeError> {
    let mut work = vec![if integer {
        TypedScalarWork::Int(instruction)
    } else {
        TypedScalarWork::Bool(instruction)
    }];
    let mut values = Vec::new();
    while let Some(task) = work.pop() {
        match task {
            TypedScalarWork::Int(instruction) => {
                let (tag, payload) = indexed_value(execution.instruction_id(instruction), call_span)?;
                let view = IntView::decode(execution, tag, payload)
                    .map_err(|error| indexed_error(error, call_span))?;
                match view {
                    IntView::Int(value) => values.push(TypedScalarValue::Int(value)),
                    IntView::Slot(slot) => {
                        let LoweredValue::Int(value) = slots[slot] else {
                            return Err(RuntimeError::new(
                                "type-error",
                                "lowered expression expected Int",
                            )
                            .with_span(call_span));
                        };
                        values.push(TypedScalarValue::Int(value));
                    }
                    IntView::Binary { op, left, right } => {
                        work.push(TypedScalarWork::ApplyIntBinary {
                            op,
                            span: call_span,
                        });
                        work.push(TypedScalarWork::Int(right.raw()));
                        work.push(TypedScalarWork::Int(left.raw()));
                    }
                    IntView::StrByteLenSlot { slot, span } => {
                        values.push(TypedScalarValue::Int(lowered_str_byte_len_value(
                            &slots[slot], span,
                        )?));
                    }
                    IntView::StrCountLinesSlot { slot, span } => {
                        values.push(TypedScalarValue::Int(lowered_str_count_lines_value(
                            &slots[slot], span,
                        )?));
                    }
                    IntView::StrByteAtSlot {
                        slot,
                        index,
                        default,
                        span,
                    } => {
                        work.push(TypedScalarWork::ApplyIntByteAt {
                            slot,
                            default: default.map(|id| id.raw()),
                            span,
                        });
                        work.push(TypedScalarWork::Int(index.raw()));
                    }
                }
            }
            TypedScalarWork::Bool(instruction) => {
                let (tag, payload) = indexed_value(execution.instruction_id(instruction), call_span)?;
                let view = BoolView::decode(execution, tag, payload)
                    .map_err(|error| indexed_error(error, call_span))?;
                match view {
                    BoolView::Bool(value) => values.push(TypedScalarValue::Bool(value)),
                    BoolView::Slot(slot) => {
                        let value = match &slots[slot] {
                            LoweredValue::Bool(value) => *value,
                            LoweredValue::Status(status) => status.success,
                            _ => {
                                return Err(RuntimeError::new(
                                    "type-error",
                                    "lowered expression expected Bool",
                                )
                                .with_span(call_span));
                            }
                        };
                        values.push(TypedScalarValue::Bool(value));
                    }
                    BoolView::Not(value) => {
                        work.push(TypedScalarWork::ApplyBoolNot);
                        work.push(TypedScalarWork::Bool(value.raw()));
                    }
                    BoolView::And(left, right) => {
                        work.push(TypedScalarWork::ApplyBoolBranch {
                            op: FullTag::BoolAnd,
                            right: right.raw(),
                        });
                        work.push(TypedScalarWork::Bool(left.raw()));
                    }
                    BoolView::Or(left, right) => {
                        work.push(TypedScalarWork::ApplyBoolBranch {
                            op: FullTag::BoolOr,
                            right: right.raw(),
                        });
                        work.push(TypedScalarWork::Bool(left.raw()));
                    }
                    BoolView::IntCompare { op, left, right } => {
                        work.push(TypedScalarWork::ApplyBoolIntCompare { op });
                        work.push(TypedScalarWork::Int(right.raw()));
                        work.push(TypedScalarWork::Int(left.raw()));
                    }
                    BoolView::StrPredicateSlot {
                        slot,
                        predicate,
                        needle,
                        span,
                    } => values.push(TypedScalarValue::Bool(lowered_str_predicate_text(
                        &slots[slot], predicate, needle, span,
                    )?)),
                    BoolView::ContainsSlot { slot, needle, span } => {
                        let needle = needle
                            .materialize()
                            .map_err(|error| indexed_error(error, span))?;
                        values.push(TypedScalarValue::Bool(lowered_contains_value(
                            &slots[slot], &needle, span,
                        )?));
                    }
                    BoolView::StrContainsSlot { slot, needle, span } => {
                        let result = if let Some(text) = lowered_str_value(&slots[slot]) {
                            bytes_contains(text.as_bytes(), needle.as_bytes())
                        } else {
                            lowered_contains_value(
                                &slots[slot],
                                &LoweredValue::Str(Arc::from(needle)),
                                span,
                            )?
                        };
                        values.push(TypedScalarValue::Bool(result));
                    }
                    BoolView::TrimEmptySlot { slot, span } => {
                        values.push(TypedScalarValue::Bool(lowered_trim_is_empty_value(
                            &slots[slot], span,
                        )?));
                    }
                    BoolView::TrimStrPredicateSlot {
                        slot,
                        predicate,
                        needle,
                        span,
                    } => values.push(TypedScalarValue::Bool(lowered_trim_str_predicate_value(
                        &slots[slot], predicate, needle, span,
                    )?)),
                    BoolView::LiteralCompareSlot { op, slot, value } => {
                        let value = value
                            .materialize()
                            .map_err(|error| indexed_error(error, call_span))?;
                        let equal = crate::runtime::eval::lowered_values_semantically_equal(
                            &slots[slot], &value,
                        );
                        let result = match op {
                            BinaryOp::Eq => equal,
                            BinaryOp::Ne => !equal,
                            _ => unreachable!("verified literal comparison"),
                        };
                        values.push(TypedScalarValue::Bool(result));
                    }
                }
            }
            TypedScalarWork::ApplyIntBinary { op, span } => {
                let Some(TypedScalarValue::Int(right)) = values.pop() else {
                    return Err(typed_scalar_stack_error("integer right operand", call_span));
                };
                let Some(TypedScalarValue::Int(left)) = values.pop() else {
                    return Err(typed_scalar_stack_error("integer left operand", call_span));
                };
                values.push(TypedScalarValue::Int(checked_int_binary(op, left, right, span)?));
            }
            TypedScalarWork::ApplyBoolNot => {
                let Some(TypedScalarValue::Bool(value)) = values.pop() else {
                    return Err(typed_scalar_stack_error("boolean operand", call_span));
                };
                values.push(TypedScalarValue::Bool(!value));
            }
            TypedScalarWork::ApplyBoolBranch { op, right } => {
                let Some(TypedScalarValue::Bool(left)) = values.pop() else {
                    return Err(typed_scalar_stack_error("boolean left operand", call_span));
                };
                if (op == FullTag::BoolAnd && !left) || (op == FullTag::BoolOr && left) {
                    values.push(TypedScalarValue::Bool(left));
                } else {
                    work.push(TypedScalarWork::Bool(right));
                }
            }
            TypedScalarWork::ApplyBoolIntCompare { op } => {
                let Some(TypedScalarValue::Int(right)) = values.pop() else {
                    return Err(typed_scalar_stack_error("integer right comparison operand", call_span));
                };
                let Some(TypedScalarValue::Int(left)) = values.pop() else {
                    return Err(typed_scalar_stack_error("integer left comparison operand", call_span));
                };
                let result = match op {
                    BinaryOp::Eq => left == right,
                    BinaryOp::Ne => left != right,
                    BinaryOp::Lt => left < right,
                    BinaryOp::Le => left <= right,
                    BinaryOp::Gt => left > right,
                    BinaryOp::Ge => left >= right,
                    _ => unreachable!("verified typed comparison"),
                };
                values.push(TypedScalarValue::Bool(result));
            }
            TypedScalarWork::ApplyIntByteAt {
                slot,
                default,
                span,
            } => {
                let Some(TypedScalarValue::Int(index)) = values.pop() else {
                    return Err(typed_scalar_stack_error("byte index", call_span));
                };
                let value = lowered_str_byte_at_value(&slots[slot], index, -1, span)?;
                if value == -1 {
                    if let Some(default) = default {
                        work.push(TypedScalarWork::Int(default));
                    } else {
                        values.push(TypedScalarValue::Int(-1));
                    }
                } else {
                    values.push(TypedScalarValue::Int(value));
                }
            }
        }
    }
    if values.len() != 1 {
        return Err(typed_scalar_stack_error("one final result", call_span));
    }
    values
        .pop()
        .ok_or_else(|| typed_scalar_stack_error("final result", call_span))
}

fn typed_scalar_stack_error(expected: &str, span: Span) -> RuntimeError {
    RuntimeError::new(
        "indexed-ir",
        format!("typed scalar evaluator expected {expected}"),
    )
    .with_span(span)
}
