use super::{
    Arc, AssertionFailure, AssertionWork, BLOCK_LIST, BinaryOp, BinaryWork, ControlFlow, Evaluator,
    FullExecution, FullTag, LoweredStrPredicate, LoweredValue, Name, RuntimeError, Span, StmtFlow,
    TraceKind, TracePayload, Value, bytes_contains, checked_int_binary, indexed_decode,
    indexed_error, indexed_finish, indexed_optional_raw, indexed_raw, indexed_value,
    lowered_binary_value, lowered_contains_value, lowered_inline_stats_field_value,
    lowered_path_method_value, lowered_record_vec_get, lowered_record_vec_or_stats,
    lowered_result_err_value, lowered_stats_field_value, lowered_status_segment_record,
    lowered_str_byte_at_value, lowered_str_byte_len_value, lowered_str_count_lines_value,
    lowered_str_predicate_text, lowered_str_value, lowered_trim_is_empty_value,
    lowered_trim_str_predicate_value, lowered_value_from_runtime_any,
};

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
        result
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

    pub(super) fn eval_indexed_assertion(
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
                            let failure = result
                                .1
                                .get_or_insert_with(|| "boolean assertion failed".into());
                            *failure = bounded_assertion_text(
                                &format!("{failure} (right operand skipped)"),
                                1024,
                            );
                        }
                    } else {
                        work.push(AssertionWork::Right {
                            op,
                            left_failure: result.1.take(),
                        });
                        work.push(AssertionWork::Expr(right));
                    }
                }
                AssertionWork::Right { op, left_failure } => {
                    if op == BinaryOp::Or
                        && !result.0
                        && let Some(left_failure) = left_failure
                    {
                        let right = result
                            .1
                            .take()
                            .unwrap_or_else(|| "boolean assertion failed".into());
                        result.1 = Some(bounded_assertion_text(
                            &format!("{left_failure}; {right}"),
                            1024,
                        ));
                    }
                }
                AssertionWork::Expr(instruction) => {
                    let (tag, mut payload) =
                        indexed_value(execution.instruction_id(instruction), span)?;
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
                            let left = match self.eval_indexed_expr(
                                execution,
                                left,
                                slots,
                                operand_span,
                            )? {
                                ControlFlow::Continue(value) => value,
                                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                            };
                            let right = match self.eval_indexed_expr(
                                execution,
                                right,
                                slots,
                                operand_span,
                            )? {
                                ControlFlow::Continue(value) => value,
                                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                            };
                            let passed =
                                crate::runtime::eval::lowered_ops::lowered_assertion_comparison(
                                    op,
                                    &left,
                                    &right,
                                    operand_span,
                                )?;
                            if !passed && instruction == condition {
                                return Ok(ControlFlow::Continue(Some(
                                    AssertionFailure::Operands(left, right),
                                )));
                            }
                            result = (
                                passed,
                                if passed {
                                    None
                                } else {
                                    Some(comparison_failure_text(op, &left, &right, operand_span))
                                },
                            );
                            continue;
                        }
                    }
                    if tag == FullTag::ExprComparisonChain {
                        let (_, mut pairs) = execution
                            .block(&mut payload, BLOCK_LIST)
                            .map_err(|error| indexed_error(error, span))?;
                        let len = indexed_raw(&mut pairs, span)? as usize;
                        indexed_decode::<bool>(&mut payload, execution, span)?;
                        indexed_finish(payload, span)?;
                        let mut previous = None;
                        result = (true, None);
                        for index in 0..len {
                            let pair = indexed_raw(&mut pairs, span)?;
                            let (pair_tag, mut pair_payload) =
                                indexed_value(execution.instruction_id(pair), span)?;
                            if pair_tag != FullTag::ExprBinary {
                                return Err(RuntimeError::new(
                                    "indexed-ir",
                                    "comparison chain requires binary pairs",
                                )
                                .with_span(span));
                            }
                            let op =
                                indexed_decode::<BinaryOp>(&mut pair_payload, execution, span)?;
                            let left = indexed_raw(&mut pair_payload, span)?;
                            let right = indexed_raw(&mut pair_payload, span)?;
                            let operand_span =
                                indexed_decode::<Span>(&mut pair_payload, execution, span)?;
                            indexed_finish(pair_payload, span)?;
                            let left = match previous.take() {
                                Some(value) => value,
                                None => match self.eval_indexed_expr(
                                    execution,
                                    left,
                                    slots,
                                    operand_span,
                                )? {
                                    ControlFlow::Continue(value) => value,
                                    ControlFlow::Break(value) => {
                                        return Ok(ControlFlow::Break(value));
                                    }
                                },
                            };
                            let right = match self.eval_indexed_expr(
                                execution,
                                right,
                                slots,
                                operand_span,
                            )? {
                                ControlFlow::Continue(value) => value,
                                ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                            };
                            if lowered_binary_value(op, left.clone(), right.clone(), operand_span)?
                                == LoweredValue::Bool(false)
                            {
                                let mut failure =
                                    comparison_failure_text(op, &left, &right, operand_span);
                                if index + 1 < len {
                                    failure.push_str(" (later operands skipped)");
                                }
                                result = (false, Some(failure));
                                break;
                            }
                            previous = Some(right);
                        }
                        continue;
                    }
                    match self.eval_indexed_expr(execution, instruction, slots, span) {
                        Ok(ControlFlow::Continue(LoweredValue::Bool(passed))) => {
                            result = (passed, None)
                        }
                        Ok(ControlFlow::Continue(_)) => {
                            return Err(RuntimeError::new(
                                "type-error",
                                "assert condition requires Bool",
                            )
                            .with_span(span));
                        }
                        Ok(ControlFlow::Break(value)) => return Ok(ControlFlow::Break(value)),
                        Err(error) => return Err(error),
                    }
                }
            }
        }
        Ok(ControlFlow::Continue(match result {
            (true, _) => None,
            (false, Some(failure)) => Some(AssertionFailure::Reached(failure)),
            (false, None) => Some(AssertionFailure::False),
        }))
    }

    pub(super) fn eval_indexed_binary_stack(
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

    pub(super) fn eval_indexed_typed_int(
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

    pub(super) fn eval_indexed_typed_bool(
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

    pub(super) fn eval_indexed_bool(
        &mut self,
        execution: &FullExecution<'_>,
        instruction: u32,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<ControlFlow<LoweredValue, bool>, RuntimeError> {
        match self.eval_indexed_expr(execution, instruction, slots, call_span)? {
            ControlFlow::Break(value) => Ok(ControlFlow::Break(value)),
            ControlFlow::Continue(value) => {
                lowered_condition_bool(value, call_span).map(ControlFlow::Continue)
            }
        }
    }
}
