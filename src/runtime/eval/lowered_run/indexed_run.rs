use super::*;
use crate::runtime::eval::indexed::full::{
    BLOCK_LIST, BLOCK_STATEMENTS, FullDriverTag, FullExecution, FullFunctionView, FullPayload,
    FullStageTag, FullTag,
};
use crate::runtime::eval::indexed::IrVerifyError;

fn indexed_error(error: IrVerifyError, span: Span) -> RuntimeError {
    RuntimeError::new(
        "indexed-ir",
        format!("indexed IR verification failed: {}", error.message),
    )
    .with_span(span)
}

fn indexed_value(
    value: Result<(FullTag, FullPayload<'_>), IrVerifyError>,
    span: Span,
) -> Result<(FullTag, FullPayload<'_>), RuntimeError> {
    value.map_err(|error| indexed_error(error, span))
}

fn indexed_decode<'a, T: crate::runtime::eval::indexed::full::FullCodec>(
    payload: &mut FullPayload<'a>,
    execution: &FullExecution<'a>,
    span: Span,
) -> Result<T, RuntimeError> {
    payload
        .decode(execution)
        .map_err(|error| indexed_error(error, span))
}

fn indexed_raw(payload: &mut FullPayload<'_>, span: Span) -> Result<u32, RuntimeError> {
    payload.raw().map_err(|error| indexed_error(error, span))
}

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
    pub(in crate::runtime::eval) fn eval_indexed_driver_step(
        &mut self,
        index: usize,
        call_span: Span,
    ) -> Option<Result<Option<Flow>, RuntimeError>> {
        let program = Arc::clone(self.indexed_program.as_ref()?);
        let view = match program.driver_step_view(index) {
            Ok(view) => view,
            Err(error) => return Some(Err(indexed_error(error, call_span))),
        };
        if !matches!(
            view.tag(),
            FullDriverTag::Let
                | FullDriverTag::Assign
                | FullDriverTag::Discard
                | FullDriverTag::Stmt
                | FullDriverTag::Expr
        ) {
            return None;
        }
        let tags = match view.instruction_tags() {
            Ok(tags) => tags,
            Err(error) => return Some(Err(indexed_error(error, call_span))),
        };
        if !tags.iter().copied().all(Self::indexed_direct_tag) {
            return None;
        }
        let stage_tags = match view.pipeline_stage_tags() {
            Ok(tags) => tags,
            Err(error) => return Some(Err(indexed_error(error, call_span))),
        };
        if !stage_tags
            .iter()
            .copied()
            .all(Self::indexed_direct_stage_tag)
        {
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
        let header = LoweredPureFunction {
            params: Default::default(),
            param_kinds: Default::default(),
            param_checks: Default::default(),
            param_rest: Default::default(),
            param_defaults: Default::default(),
            captures: Default::default(),
            return_kind: LoweredReturnKind::Plain(LoweredType::Unit),
            slot_count: view.slot_count(),
            body: Vec::new(),
            has_defers: false,
        };
        let flow = match view.tag() {
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
                let value_span =
                    indexed_decode::<Span>(&mut payload, &execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let mut value =
                    match self.eval_indexed_expr(&execution, value, &mut slots, call_span)? {
                        ControlFlow::Continue(value) => value.into_value(),
                        ControlFlow::Break(value) => {
                            return Ok(Some(self.question_flow(
                                value.into_value(),
                                call_span,
                            )));
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
                            return Ok(Some(self.question_flow(
                                value.into_value(),
                                call_span,
                            )));
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
                self.assign(&target, value, span)?;
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
                let flow = self.eval_indexed_stmt(
                    &execution,
                    statement,
                    &header,
                    &mut slots,
                    call_span,
                )?;
                for slot in &top_level_slots {
                    if slot.mutable {
                        self.assign(
                            &slot.name,
                            slots[slot.slot].clone().into_value(),
                            call_span,
                        )?;
                    }
                }
                match flow {
                    LoweredStmtFlow::Propagate(value) => {
                        self.question_flow(value.into_value(), call_span)
                    }
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
                            return Ok(Some(self.question_flow(
                                value.into_value(),
                                call_span,
                            )));
                        }
                    };
                if matches!(value, Value::Result(_)) {
                    self.question_flow(value, call_span)
                } else {
                    Flow::Continue(value)
                }
            }
            _ => unreachable!("direct driver tag checked before evaluation"),
        };
        Ok(Some(flow))
    }

    fn indexed_direct_tag(tag: FullTag) -> bool {
        Self::indexed_direct_expr_tag(tag)
            || matches!(
                tag,
                FullTag::IntInt
                    | FullTag::IntSlot
                    | FullTag::IntBinary
                    | FullTag::IntStrByteLenSlot
                    | FullTag::IntStrCountLinesSlot
                    | FullTag::IntStrByteAtSlot
                    | FullTag::BoolBool
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
                    | FullTag::StmtLet
                    | FullTag::StmtLetInt
                    | FullTag::StmtLetBool
                    | FullTag::StmtAssign
                    | FullTag::StmtAssignInt
                    | FullTag::StmtAssignBool
                    | FullTag::StmtExpr
                    | FullTag::StmtIf
                    | FullTag::StmtIfBool
                    | FullTag::StmtWhile
                    | FullTag::StmtWhileBool
                    | FullTag::StmtFor
                    | FullTag::StmtPrint
                    | FullTag::StmtRun
                    | FullTag::StmtLoop
                    | FullTag::StmtReturn
                    | FullTag::StmtYield
                    | FullTag::StmtBreak
                    | FullTag::StmtBreakValue
                    | FullTag::StmtContinue
            )
    }

    pub(super) fn call_indexed_direct(
        &mut self,
        function: LoweredFunctionKey,
        kind: LoweredFunctionKind,
        args: &[Value],
        call_span: Span,
    ) -> Option<Result<Value, RuntimeError>> {
        let program = Arc::clone(self.indexed_program.as_ref()?);
        let view = match program.function_view(function, kind) {
            Ok(Some(view)) => view,
            Ok(None) => return None,
            Err(error) => return Some(Err(indexed_error(error, call_span))),
        };
        let tags = match view.instruction_tags() {
            Ok(tags) => tags,
            Err(error) => return Some(Err(indexed_error(error, call_span))),
        };
        if view.has_defers() || !tags.iter().copied().all(Self::indexed_direct_tag) {
            return None;
        }
        let stage_tags = match view.pipeline_stage_tags() {
            Ok(tags) => tags,
            Err(error) => return Some(Err(indexed_error(error, call_span))),
        };
        if !stage_tags
            .iter()
            .copied()
            .all(Self::indexed_direct_stage_tag)
        {
            return None;
        }
        let header = match view.header() {
            Ok(header) => header,
            Err(error) => return Some(Err(indexed_error(error, call_span))),
        };
        let mut slots = self.try_bind_lowered_runtime_args(&header, args)?;
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
        header: &LoweredPureFunction,
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
        let result = with_lowered_eval_depth(call_span, || {
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
        direct_allowed: bool,
    ) -> Result<LoweredValue, RuntimeError> {
        let kind = if self.contains_indexed_function(function, LoweredFunctionKind::Pure) {
            LoweredFunctionKind::Pure
        } else if self.contains_indexed_function(function, LoweredFunctionKind::Proc) {
            LoweredFunctionKind::Proc
        } else {
            return Err(RuntimeError::new(
                "unresolved-lowered-call",
                function.display_name(),
            )
            .with_span(call_span));
        };
        let program = Arc::clone(
            self.indexed_program
                .as_ref()
                .expect("indexed caller retains its indexed program"),
        );
        let view = program
            .function_view(function, kind)
            .map_err(|error| indexed_error(error, call_span))?
            .ok_or_else(|| {
                RuntimeError::new("unresolved-lowered-call", function.display_name())
                    .with_span(call_span)
            })?;
        let direct = !view.has_defers()
            && view
                .instruction_tags()
                .map_err(|error| indexed_error(error, call_span))?
                .iter()
                .copied()
                .all(Self::indexed_direct_tag)
            && view
                .pipeline_stage_tags()
                .map_err(|error| indexed_error(error, call_span))?
                .iter()
                .copied()
                .all(Self::indexed_direct_stage_tag);
        if direct && direct_allowed {
            let header = view
                .header()
                .map_err(|error| indexed_error(error, call_span))?;
            let mut next_slots = self.bind_lowered_values(&header, values, call_span)?;
            let result = self
                .eval_indexed_call_frame(
                    function,
                    kind,
                    view,
                    &header,
                    &mut next_slots,
                    call_span,
                )
                .and_then(|value| lowered_return_value(header.return_kind, value, call_span));
            self.recycle_lowered_slots(next_slots);
            return result;
        }
        let callee = self.lowered_function(function).ok_or_else(|| {
            RuntimeError::new("unresolved-lowered-call", function.display_name())
                .with_span(call_span)
        })?;
        let mut next_slots = self.bind_lowered_values(&callee, values, call_span)?;
        let result = self
            .eval_lowered_call_with_frame(function, &callee, &mut next_slots, call_span)
            .and_then(|value| lowered_return_value(callee.return_kind, value, call_span));
        self.recycle_lowered_slots(next_slots);
        result
    }

    fn eval_indexed_function(
        &mut self,
        view: FullFunctionView<'_>,
        header: &LoweredPureFunction,
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
                LoweredStmtFlow::None => Ok(LoweredValue::Stream(Box::new(
                    StreamValue::from_values(items),
                ))),
                LoweredStmtFlow::Return(value) if matches!(value, LoweredValue::Stream(_)) => {
                    Ok(value)
                }
                LoweredStmtFlow::Return(LoweredValue::Unit) => Ok(LoweredValue::Stream(Box::new(
                    StreamValue::from_values(items),
                ))),
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
        let result = self.eval_indexed_stmts(&execution, body, header, slots, call_span);
        let write_back = self.write_back_lowered_captures(header, slots, call_span);
        let flow = result?;
        write_back?;
        match flow {
            LoweredStmtFlow::Return(value) | LoweredStmtFlow::Propagate(value) => Ok(value),
            LoweredStmtFlow::None => {
                Err(RuntimeError::new("return", "lowered function did not return")
                    .with_span(call_span))
            }
            LoweredStmtFlow::Continue => {
                Err(RuntimeError::new("control-flow", "continue outside loop")
                    .with_span(call_span))
            }
            LoweredStmtFlow::Break(_) => Err(
                RuntimeError::new("control-flow", "break outside loop").with_span(call_span),
            ),
        }
    }

    pub(super) fn indexed_direct_expr_tag(tag: FullTag) -> bool {
        matches!(
            tag,
            FullTag::ExprNull
                | FullTag::ExprUnit
                | FullTag::ExprInt
                | FullTag::ExprFloat
                | FullTag::ExprDuration
                | FullTag::ExprBool
                | FullTag::ExprStr
                | FullTag::ExprBytes
                | FullTag::ExprPath
                | FullTag::ExprFunctionRef
                | FullTag::ExprPathFrom
                | FullTag::ExprParam
                | FullTag::ExprBinary
                | FullTag::ExprIf
                | FullTag::ExprResultFallback
                | FullTag::ExprFmtString
                | FullTag::ExprPathFmtString
                | FullTag::ExprGlob
                | FullTag::ExprLastStatus
                | FullTag::ExprRecord
                | FullTag::ExprList
                | FullTag::ExprEmptyMap
                | FullTag::ExprBytesConcat
                | FullTag::ExprRange
                | FullTag::ExprTag
                | FullTag::ExprPipeline
                | FullTag::ExprField
                | FullTag::ExprIndex
                | FullTag::ExprSlice
                | FullTag::ExprMethod
                | FullTag::ExprStrByteLen
                | FullTag::ExprStrByteAt
                | FullTag::ExprStrPredicate
                | FullTag::ExprContains
                | FullTag::ExprRegexCompile
                | FullTag::ExprModuleCall
                | FullTag::ExprOk
                | FullTag::ExprErr
                | FullTag::ExprTry
                | FullTag::ExprCall
                | FullTag::ExprSelfCall
        )
    }

    fn indexed_direct_stage_tag(tag: FullStageTag) -> bool {
        matches!(
            tag,
            FullStageTag::TextLines
                | FullStageTag::JsonLines
                | FullStageTag::Where
                | FullStageTag::Map
                | FullStageTag::Enumerate
                | FullStageTag::Zip
                | FullStageTag::Sort
                | FullStageTag::SortBy
                | FullStageTag::GroupBy
                | FullStageTag::CountBy
                | FullStageTag::Any
                | FullStageTag::All
                | FullStageTag::UniqueBy
                | FullStageTag::Count
                | FullStageTag::Sum
                | FullStageTag::Collect
                | FullStageTag::First
                | FullStageTag::Last
                | FullStageTag::Min
                | FullStageTag::Max
                | FullStageTag::Take
                | FullStageTag::Drop
                | FullStageTag::Repeat
                | FullStageTag::Range
        )
    }

    fn indexed_stage_name(tag: FullStageTag) -> &'static str {
        match tag {
            FullStageTag::TextLines => "text.lines",
            FullStageTag::JsonLines => "json.lines",
            FullStageTag::Where => "where",
            FullStageTag::Map | FullStageTag::MapBlock => "map",
            FullStageTag::FlatMap | FullStageTag::FlatMapBlock => "flat-map",
            FullStageTag::BytesChunks => "bytes.chunks",
            FullStageTag::BatchCount
            | FullStageTag::BatchMaxArgv
            | FullStageTag::BatchMaxBytes => "batch",
            FullStageTag::Shuffle => "shuffle",
            FullStageTag::Fold => "fold",
            FullStageTag::ReduceBy => "reduce-by",
            FullStageTag::ParMap | FullStageTag::ParMapBlock => "par-map",
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
            ControlFlow::Break(value) => {
                Err(runtime_error_from_value(value.into_value(), span))
            }
        }
    }

    pub(super) fn eval_indexed_expr(
        &mut self,
        execution: &FullExecution<'_>,
        instruction: u32,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let (tag, mut payload) =
            indexed_value(execution.instruction_id(instruction), call_span)?;
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
                let value =
                    indexed_decode::<DurationValue>(&mut payload, execution, call_span)?;
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
                let function =
                    indexed_decode::<FunctionName>(&mut payload, execution, call_span)?;
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
                let left = match self.eval_indexed_expr(execution, left, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let right = match self.eval_indexed_expr(execution, right, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                ControlFlow::Continue(lowered_binary_value(op, left, right, span)?)
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
                    Some(indexed_decode::<Span>(
                        &mut payload,
                        execution,
                        call_span,
                    )?)
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
                            let span =
                                indexed_decode::<Span>(&mut parts, execution, call_span)?;
                            let spec = indexed_decode::<Option<FormatSpec>>(
                                &mut parts,
                                execution,
                                call_span,
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
                let matches = crate::runtime::eval::expand_glob_pattern(
                    &self.cwd,
                    &pattern,
                    span,
                )?;
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
                            let name =
                                indexed_decode::<Name>(&mut entries, execution, call_span)?;
                            let expr = indexed_raw(&mut entries, call_span)?;
                            let value =
                                match self.eval_indexed_expr(execution, expr, slots, call_span)? {
                                    ControlFlow::Continue(value) => value,
                                    ControlFlow::Break(value) => {
                                        return Ok(ControlFlow::Break(value));
                                    }
                                };
                            lowered_record_vec_append_or_replace_unsorted(
                                &mut record,
                                name,
                                value,
                            );
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
                                    for (key, value) in lowered_inline_stats_to_record_vec(
                                        blanks, code, comments,
                                    ) {
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
                                            (
                                                Arc::from("index"),
                                                LoweredValue::Int(index as i64),
                                            ),
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
                            let right = self.lowered_list_items(other, span, "zip expected List")?;
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
                                execution,
                                descending,
                                slots,
                                span,
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
                            let descending = indexed_optional_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let items = self.lowered_pipeline_input_items(current, span)?;
                            let mut keyed = Vec::with_capacity(items.len());
                            for item in items {
                                slots[slot] = item;
                                let key =
                                    match self.eval_indexed_expr(execution, key, slots, span)? {
                                        ControlFlow::Continue(value) => value,
                                        ControlFlow::Break(value) => {
                                            return Ok(ControlFlow::Break(value));
                                        }
                                    };
                                let item =
                                    std::mem::replace(&mut slots[slot], LoweredValue::Unit);
                                keyed.push((key, item));
                            }
                            keyed.sort_unstable_by(|(left, _), (right, _)| {
                                compare_lowered_sort_keys(left, right)
                            });
                            if self.eval_indexed_pipeline_descending(
                                execution,
                                descending,
                                slots,
                                span,
                            )? {
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
                            let items = self.lowered_pipeline_input_items(current, span)?;
                            let mut groups: Vec<(LoweredValue, Vec<LoweredValue>)> = Vec::new();
                            for item in items {
                                slots[slot] = item;
                                let key =
                                    match self.eval_indexed_expr(execution, key, slots, span)? {
                                        ControlFlow::Continue(value) => value,
                                        ControlFlow::Break(value) => {
                                            return Ok(ControlFlow::Break(value));
                                        }
                                    };
                                let item =
                                    std::mem::replace(&mut slots[slot], LoweredValue::Unit);
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
                            let slot = indexed_decode::<usize>(
                                &mut stage_payload,
                                execution,
                                span,
                            )?;
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
                            let slot = indexed_decode::<usize>(
                                &mut stage_payload,
                                execution,
                                span,
                            )?;
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
                                let item =
                                    std::mem::replace(&mut slots[slot], LoweredValue::Unit);
                                if !seen.iter().any(|existing| existing == &key) {
                                    seen.push(key);
                                    unique.push(item);
                                }
                            }
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
                            let mut filtered = Vec::new();
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
                        FullStageTag::Map => {
                            let slot = indexed_decode::<usize>(
                                &mut stage_payload,
                                execution,
                                span,
                            )?;
                            let value = indexed_raw(&mut stage_payload, span)?;
                            indexed_finish(stage_payload, span)?;
                            let items = self.lowered_pipeline_input_items(current, span)?;
                            let mut mapped = Vec::with_capacity(items.len());
                            for (index, item) in items.into_iter().enumerate() {
                                slots[slot] = item;
                                let value =
                                    match self.eval_indexed_expr(execution, value, slots, span) {
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
                            let end =
                                match self.eval_indexed_expr(execution, end, slots, span)? {
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
                                (end + 1..=start)
                                    .rev()
                                    .map(LoweredValue::Int)
                                    .collect()
                            })
                        }
                        _ => unreachable!("direct pipeline stage checked before evaluation"),
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
                let name =
                    indexed_decode::<&'static str>(&mut payload, execution, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
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
                let name =
                    indexed_decode::<&'static str>(&mut payload, execution, call_span)?;
                let (_, mut args) = execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, call_span))?;
                let len = indexed_raw(&mut args, call_span)? as usize;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let receiver =
                    match self.eval_indexed_expr(execution, receiver, slots, span)? {
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
                    return self.eval_lowered_method_dispatch(
                        receiver,
                        name,
                        values,
                        &span,
                    );
                }
                let trace_name = format!("{}.{}", receiver.type_name(), name);
                self.trace_enter(
                    TraceKind::MethodCall,
                    Some(span),
                    Some(&trace_name),
                    TracePayload::None,
                );
                let result =
                    self.eval_lowered_method_dispatch(receiver, name, values, &span);
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
                        return Err(RuntimeError::new("type-error", "byte_at expected Int")
                            .with_span(span));
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
                    return Err(RuntimeError::new(
                        "type-error",
                        "regex.compile expected Str",
                    )
                    .with_span(span));
                };
                ControlFlow::Continue(match crate::modules::regex::compile(pattern, span) {
                    Ok(regex) => LoweredValue::ResultOk(Box::new(LoweredValue::Regex(Box::new(
                        RegexValue {
                            pattern: pattern.to_string(),
                            regex: Arc::new(regex),
                        },
                    )))),
                    Err(error) => {
                        LoweredValue::ResultErr(Box::new(Value::Error(Box::new(error))))
                    }
                })
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
                let function = indexed_decode::<LoweredFunctionKey>(
                    &mut payload,
                    execution,
                    call_span,
                )?;
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
                let direct_allowed = execution
                    .function_identity()
                    .is_ok_and(|(caller, _)| caller != function);
                return self
                    .eval_indexed_named_call(function, &values, span, direct_allowed)
                    .map(ControlFlow::Continue);
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
                    .eval_indexed_named_call(function, &values, span, false)
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

    fn eval_indexed_stmts(
        &mut self,
        execution: &FullExecution<'_>,
        mut statements: FullPayload<'_>,
        header: &LoweredPureFunction,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<LoweredStmtFlow, RuntimeError> {
        let len = indexed_raw(&mut statements, call_span)? as usize;
        for _ in 0..len {
            let statement = indexed_raw(&mut statements, call_span)?;
            match self.eval_indexed_stmt(execution, statement, header, slots, call_span)? {
                LoweredStmtFlow::None => {}
                flow @ (LoweredStmtFlow::Return(_)
                | LoweredStmtFlow::Propagate(_)
                | LoweredStmtFlow::Break(_)
                | LoweredStmtFlow::Continue) => return Ok(flow),
            }
        }
        indexed_finish(statements, call_span)?;
        Ok(LoweredStmtFlow::None)
    }

    fn eval_indexed_statement_block(
        &mut self,
        execution: &FullExecution<'_>,
        block: u32,
        header: &LoweredPureFunction,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<LoweredStmtFlow, RuntimeError> {
        let (_, statements) = execution
            .block_id(block, BLOCK_STATEMENTS)
            .map_err(|error| indexed_error(error, call_span))?;
        self.eval_indexed_stmts(execution, statements, header, slots, call_span)
    }

    fn eval_indexed_optional_statement_block(
        &mut self,
        execution: &FullExecution<'_>,
        payload: &mut FullPayload<'_>,
        header: &LoweredPureFunction,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<Option<LoweredStmtFlow>, RuntimeError> {
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
        header: &LoweredPureFunction,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<LoweredStmtFlow, RuntimeError> {
        let (tag, mut payload) =
            indexed_value(execution.instruction_id(instruction), call_span)?;
        match tag {
            FullTag::StmtLet => {
                let slot = indexed_decode::<usize>(&mut payload, execution, call_span)?;
                let value = indexed_raw(&mut payload, call_span)?;
                indexed_finish(payload, call_span)?;
                match self.eval_indexed_expr(execution, value, slots, call_span)? {
                    ControlFlow::Continue(value) => slots[slot] = value,
                    ControlFlow::Break(value) => return Ok(LoweredStmtFlow::Return(value)),
                }
                Ok(LoweredStmtFlow::None)
            }
            FullTag::StmtLetInt => {
                let slot = indexed_decode::<usize>(&mut payload, execution, call_span)?;
                let value = indexed_raw(&mut payload, call_span)?;
                indexed_finish(payload, call_span)?;
                match self.eval_indexed_typed_int(execution, value, slots, call_span)? {
                    ControlFlow::Continue(value) => slots[slot] = LoweredValue::Int(value),
                    ControlFlow::Break(value) => return Ok(LoweredStmtFlow::Return(value)),
                }
                Ok(LoweredStmtFlow::None)
            }
            FullTag::StmtLetBool => {
                let slot = indexed_decode::<usize>(&mut payload, execution, call_span)?;
                let value = indexed_raw(&mut payload, call_span)?;
                indexed_finish(payload, call_span)?;
                match self.eval_indexed_typed_bool(execution, value, slots, call_span)? {
                    ControlFlow::Continue(value) => slots[slot] = LoweredValue::Bool(value),
                    ControlFlow::Break(value) => return Ok(LoweredStmtFlow::Return(value)),
                }
                Ok(LoweredStmtFlow::None)
            }
            FullTag::StmtAssign => {
                let slot = indexed_decode::<usize>(&mut payload, execution, call_span)?;
                let op = indexed_decode::<AssignOp>(&mut payload, execution, call_span)?;
                let value = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let value = match self.eval_indexed_expr(execution, value, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(LoweredStmtFlow::Return(value)),
                };
                slots[slot] = lowered_assign_value(op, slots[slot].clone(), value, span)?;
                Ok(LoweredStmtFlow::None)
            }
            FullTag::StmtAssignInt => {
                let slot = indexed_decode::<usize>(&mut payload, execution, call_span)?;
                let op = indexed_decode::<AssignOp>(&mut payload, execution, call_span)?;
                let value = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let value =
                    match self.eval_indexed_typed_int(execution, value, slots, call_span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(LoweredStmtFlow::Return(value)),
                    };
                if op == AssignOp::Set {
                    slots[slot] = LoweredValue::Int(value);
                    return Ok(LoweredStmtFlow::None);
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
                Ok(LoweredStmtFlow::None)
            }
            FullTag::StmtAssignBool => {
                let slot = indexed_decode::<usize>(&mut payload, execution, call_span)?;
                let value = indexed_raw(&mut payload, call_span)?;
                indexed_finish(payload, call_span)?;
                match self.eval_indexed_typed_bool(execution, value, slots, call_span)? {
                    ControlFlow::Continue(value) => slots[slot] = LoweredValue::Bool(value),
                    ControlFlow::Break(value) => return Ok(LoweredStmtFlow::Return(value)),
                }
                Ok(LoweredStmtFlow::None)
            }
            FullTag::StmtExpr => {
                let value = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                match self.eval_indexed_expr(execution, value, slots, span)? {
                    ControlFlow::Continue(value @ LoweredValue::ResultErr(_)) => {
                        let value = self.lowered_question_propagation_value(value, span)?;
                        Ok(LoweredStmtFlow::Propagate(value))
                    }
                    ControlFlow::Continue(_) => Ok(LoweredStmtFlow::None),
                    ControlFlow::Break(value) => Ok(LoweredStmtFlow::Propagate(value)),
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
                        match self.eval_indexed_typed_bool(
                            execution, condition, slots, call_span,
                        )? {
                            ControlFlow::Continue(value) => value,
                            ControlFlow::Break(value) => {
                                return Ok(LoweredStmtFlow::Return(value));
                            }
                        }
                    } else {
                        match self.eval_indexed_bool(execution, condition, slots, call_span)? {
                            ControlFlow::Continue(value) => value,
                            ControlFlow::Break(value) => {
                                return Ok(LoweredStmtFlow::Return(value));
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
                Ok(flow.unwrap_or(LoweredStmtFlow::None))
            }
            FullTag::StmtWhile | FullTag::StmtWhileBool => {
                let typed = tag == FullTag::StmtWhileBool;
                let condition = indexed_raw(&mut payload, call_span)?;
                let body = indexed_raw(&mut payload, call_span)?;
                indexed_finish(payload, call_span)?;
                loop {
                    self.service_pending_signal(call_span)?;
                    if self.signal_state.shutdown_complete {
                        return Ok(LoweredStmtFlow::None);
                    }
                    let condition = if typed {
                        match self.eval_indexed_typed_bool(
                            execution, condition, slots, call_span,
                        )? {
                            ControlFlow::Continue(value) => value,
                            ControlFlow::Break(value) => {
                                return Ok(LoweredStmtFlow::Return(value));
                            }
                        }
                    } else {
                        match self.eval_indexed_bool(execution, condition, slots, call_span)? {
                            ControlFlow::Continue(value) => value,
                            ControlFlow::Break(value) => {
                                return Ok(LoweredStmtFlow::Return(value));
                            }
                        }
                    };
                    if !condition {
                        break;
                    }
                    match self.eval_indexed_statement_block(
                        execution, body, header, slots, call_span,
                    )? {
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
            FullTag::StmtFor => {
                let slot = indexed_decode::<usize>(&mut payload, execution, call_span)?;
                let iter = indexed_raw(&mut payload, call_span)?;
                let body = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let iter = match self.eval_indexed_expr(execution, iter, slots, call_span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(LoweredStmtFlow::Return(value)),
                };
                let items = self.lowered_list_items(iter, span, "lowered for expected List")?;
                for item in items {
                    self.service_pending_signal(span)?;
                    if self.signal_state.shutdown_complete {
                        return Ok(LoweredStmtFlow::None);
                    }
                    slots[slot] = item;
                    match self.eval_indexed_statement_block(
                        execution, body, header, slots, call_span,
                    )? {
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
            FullTag::StmtPrint => {
                let (_, mut args) = execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, call_span))?;
                let len = indexed_raw(&mut args, call_span)? as usize;
                let stderr = indexed_decode::<bool>(&mut payload, execution, call_span)?;
                let flush = indexed_decode::<bool>(&mut payload, execution, call_span)?;
                let propagate_result =
                    indexed_decode::<bool>(&mut payload, execution, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let mut line = String::new();
                let mut argv = Vec::with_capacity(len);
                for index in 0..len {
                    let arg = indexed_raw(&mut args, span)?;
                    let value = match self.eval_indexed_expr(
                        execution, arg, slots, call_span,
                    )? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => {
                            return Ok(LoweredStmtFlow::Return(value));
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
                        Some(0) | None => Ok(LoweredStmtFlow::None),
                        Some(code) => Ok(LoweredStmtFlow::Propagate(LoweredValue::Int(
                            i64::from(code),
                        ))),
                    }
                } else {
                    Ok(LoweredStmtFlow::None)
                }
            }
            FullTag::StmtRun => {
                let value = indexed_raw(&mut payload, call_span)?;
                let propagate_result =
                    indexed_decode::<bool>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                match self.eval_indexed_expr(execution, value, slots, call_span)? {
                    ControlFlow::Continue(value) => {
                        if propagate_result {
                            match value {
                                LoweredValue::ResultOk(_) => Ok(LoweredStmtFlow::None),
                                value @ LoweredValue::ResultErr(_) => {
                                    let value = self
                                        .lowered_question_propagation_value(value, call_span)?;
                                    Ok(LoweredStmtFlow::Propagate(value))
                                }
                                other => Err(RuntimeError::new(
                                    "type-error",
                                    format!(
                                        "`?` expected Result, found {}",
                                        other.type_name()
                                    ),
                                )
                                .with_span(call_span)),
                            }
                        } else {
                            Ok(LoweredStmtFlow::None)
                        }
                    }
                    ControlFlow::Break(value) => Ok(LoweredStmtFlow::Propagate(value)),
                }
            }
            FullTag::StmtLoop => {
                let body = indexed_raw(&mut payload, call_span)?;
                indexed_finish(payload, call_span)?;
                loop {
                    match self.eval_indexed_statement_block(
                        execution, body, header, slots, call_span,
                    )? {
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
            FullTag::StmtReturn => {
                let value = indexed_raw(&mut payload, call_span)?;
                indexed_finish(payload, call_span)?;
                let value = match self.eval_indexed_expr(execution, value, slots, call_span)? {
                    ControlFlow::Continue(value) | ControlFlow::Break(value) => value,
                };
                Ok(LoweredStmtFlow::Return(value))
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
                Ok(LoweredStmtFlow::None)
            }
            FullTag::StmtBreak => {
                indexed_finish(payload, call_span)?;
                Ok(LoweredStmtFlow::Break(None))
            }
            FullTag::StmtBreakValue => {
                let value = indexed_raw(&mut payload, call_span)?;
                indexed_finish(payload, call_span)?;
                match self.eval_indexed_expr(execution, value, slots, call_span)? {
                    ControlFlow::Continue(value) => Ok(LoweredStmtFlow::Break(Some(value))),
                    ControlFlow::Break(value) => Ok(LoweredStmtFlow::Propagate(value)),
                }
            }
            FullTag::StmtContinue => {
                indexed_finish(payload, call_span)?;
                Ok(LoweredStmtFlow::Continue)
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
        if let Some(value) = lowered_record_field_value(&base, name) {
            return Ok(value);
        }
        match base {
            LoweredValue::FsEntry(entry) => {
                let value = entry
                    .field_value(name)
                    .ok_or_else(|| RuntimeError::new("missing-field", name).with_span(span))?
                    .map_err(|error| error.with_span(span))?;
                lowered_value_from_runtime_any(&value).ok_or_else(|| {
                    RuntimeError::new(
                        "type-error",
                        format!("fs entry field produced unsupported {}", value.type_name()),
                    )
                    .with_span(span)
                })
            }
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
                    handle
                        .argv
                        .iter()
                        .cloned()
                        .map(LoweredValue::Str)
                        .collect(),
                )),
                "detached" => Ok(LoweredValue::Bool(handle.detached)),
                _ => Err(RuntimeError::new("missing-field", name).with_span(span)),
            },
            LoweredValue::Path(path) => lowered_path_method_value(path, name, Vec::new(), span),
            _ => Err(RuntimeError::new("missing-field", name).with_span(span)),
        }
    }

    fn eval_indexed_typed_int(
        &mut self,
        execution: &FullExecution<'_>,
        instruction: u32,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<ControlFlow<LoweredValue, i64>, RuntimeError> {
        let (tag, mut payload) =
            indexed_value(execution.instruction_id(instruction), call_span)?;
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
                let left = match self.eval_indexed_typed_int(
                    execution, left, slots, call_span,
                )? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let right = match self.eval_indexed_typed_int(
                    execution, right, slots, call_span,
                )? {
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
                        return Err(RuntimeError::new(
                            "division-by-zero",
                            "division by zero",
                        )
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
                let index = match self.eval_indexed_typed_int(
                    execution, index, slots, call_span,
                )? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let default = match default {
                    Some(default) => match self.eval_indexed_typed_int(
                        execution, default, slots, call_span,
                    )? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    },
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
        let (tag, mut payload) =
            indexed_value(execution.instruction_id(instruction), call_span)?;
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
                let left =
                    match self.eval_indexed_typed_bool(execution, left, slots, call_span)? {
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
                let left =
                    match self.eval_indexed_typed_int(execution, left, slots, call_span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    };
                let right =
                    match self.eval_indexed_typed_int(execution, right, slots, call_span)? {
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
                let needle = indexed_decode::<Arc<[u8]>>(
                    &mut payload,
                    execution,
                    call_span,
                )?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                lowered_str_predicate_text(&slots[slot], predicate, &needle, span)?
            }
            FullTag::BoolContainsSlot => {
                let slot = indexed_decode::<usize>(&mut payload, execution, call_span)?;
                let needle =
                    indexed_decode::<LoweredValue>(&mut payload, execution, call_span)?;
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
                let needle = indexed_decode::<Arc<[u8]>>(
                    &mut payload,
                    execution,
                    call_span,
                )?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                lowered_trim_str_predicate_value(&slots[slot], predicate, &needle, span)?
            }
            FullTag::BoolLiteralCompareSlot => {
                let op = indexed_decode::<BinaryOp>(&mut payload, execution, call_span)?;
                let slot = indexed_decode::<usize>(&mut payload, execution, call_span)?;
                let value =
                    indexed_decode::<LoweredValue>(&mut payload, execution, call_span)?;
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
            ControlFlow::Continue(LoweredValue::Bool(value)) => {
                Ok(ControlFlow::Continue(value))
            }
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

        let result = evaluator
            .call_indexed_direct(
                LoweredFunctionKey::Name(Name::intern("direct_limit")),
                LoweredFunctionKind::Pure,
                &[Value::Int(4)],
                Span::new(source_id, 0, 0),
            )
            .expect("function uses only direct indexed opcodes")
            .unwrap();

        assert_eq!(result, Value::Int(8));
        let recursive = evaluator
            .call_indexed_direct(
                LoweredFunctionKey::Name(Name::intern("countdown")),
                LoweredFunctionKind::Pure,
                &[Value::Int(4)],
                Span::new(source_id, 0, 0),
            )
            .expect("self-recursive function uses only direct indexed opcodes")
            .unwrap();
        assert_eq!(recursive, Value::Int(4));
        let piped = evaluator
            .call_indexed_direct(
                LoweredFunctionKey::Name(Name::intern("pipeline")),
                LoweredFunctionKind::Pure,
                &[Value::List(vec![Value::Int(3), Value::Int(1), Value::Int(2)])],
                Span::new(source_id, 0, 0),
            )
            .expect("collection pipeline uses only direct indexed opcodes")
            .unwrap();
        assert_eq!(
            piped,
            Value::List(vec![Value::Int(4), Value::Int(6)])
        );
    }
}
