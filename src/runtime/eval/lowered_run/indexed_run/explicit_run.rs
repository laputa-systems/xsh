use super::*;

enum FrameValue {
    Value(LoweredValue),
    Break(LoweredValue),
}

enum FrameContinuation {
    Store(usize),
    Assign {
        slot: usize,
        op: AssignOp,
        span: Span,
    },
    Return,
    Discard(Span),
    BinaryLeft {
        op: BinaryOp,
        right: u32,
        span: Span,
        next: Box<FrameContinuation>,
    },
    BinaryRight {
        op: BinaryOp,
        left: LoweredValue,
        span: Span,
        next: Box<FrameContinuation>,
    },
    BoolBinaryRight {
        next: Box<FrameContinuation>,
        span: Span,
    },
    If {
        branches: Vec<(u32, u32)>,
        index: usize,
        else_value: u32,
        span: Span,
        next: Box<FrameContinuation>,
    },
    StatementIf {
        branches: Vec<(u32, u32)>,
        index: usize,
        else_body: Option<u32>,
        span: Span,
    },
    ForItems {
        slot: usize,
        body: u32,
        span: Span,
    },
    ForStrLines {
        slot: usize,
        body: u32,
        span: Span,
    },
    While {
        condition: u32,
        body: u32,
        span: Span,
    },
    MatchValue {
        arms: Vec<(u32, Option<u32>, u32)>,
        span: Span,
    },
    MatchGuard {
        arms: Vec<(u32, Option<u32>, u32)>,
        index: usize,
        value: LoweredValue,
        span: Span,
    },
    MatchExprValue {
        arms: Vec<(u32, Option<u32>, u32)>,
        span: Span,
        next: Box<FrameContinuation>,
    },
    MatchExprGuard {
        arms: Vec<(u32, Option<u32>, u32)>,
        index: usize,
        value: LoweredValue,
        span: Span,
        next: Box<FrameContinuation>,
    },
    BreakLoop,
    Defer,
    CallArguments {
        function: LoweredFunctionKey,
        kind: LoweredFunctionKind,
        args: Vec<(u32, u32)>,
        index: usize,
        values: Vec<LoweredValue>,
        span: Span,
        next: Box<FrameContinuation>,
    },
    WrapOk(Box<FrameContinuation>),
    WrapErr(Box<FrameContinuation>),
    Try {
        span: Span,
        next: Box<FrameContinuation>,
    },
    Require {
        check: LoweredTypeCheck,
        span: Span,
        next: Box<FrameContinuation>,
    },
    MethodReceiver {
        name: Arc<str>,
        args: Vec<u32>,
        span: Span,
        next: Box<FrameContinuation>,
    },
    MethodArg {
        name: Arc<str>,
        args: Vec<u32>,
        receiver: LoweredValue,
        index: usize,
        values: Vec<LoweredValue>,
        span: Span,
        next: Box<FrameContinuation>,
    },
    ResultFallback {
        right: u32,
        span: Span,
        next: Box<FrameContinuation>,
    },
}

enum FrameWork {
    Statements {
        statements: Vec<u32>,
        complete_call: bool,
    },
    Statement(u32),
    Expr {
        instruction: u32,
        span: Span,
        next: FrameContinuation,
    },
    Value {
        value: FrameValue,
        next: FrameContinuation,
    },
    ForItems {
        slot: usize,
        items: Vec<LoweredValue>,
        index: usize,
        body: u32,
        span: Span,
    },
    ForStrLines {
        slot: usize,
        text: LoweredValue,
        cursor: usize,
        line_count: u32,
        body: u32,
        span: Span,
    },
    While {
        condition: u32,
        body: u32,
        typed: bool,
        span: Span,
    },
    Finish(StmtFlow),
    FinishError,
}

struct CallFrame<'p> {
    function: LoweredFunctionKey,
    kind: LoweredFunctionKind,
    execution: FullExecution<'p>,
    slots: Vec<LoweredValue>,
    call_span: Span,
    name: String,
    work: Vec<FrameWork>,
    defers: Vec<u32>,
    return_to: Option<FrameContinuation>,
}

struct ExplicitFrames<'a, 'p> {
    evaluator: &'a mut Evaluator,
    program: &'p FullProgram,
    calls: Vec<CallFrame<'p>>,
    result: Option<Result<LoweredValue, RuntimeError>>,
    pending_error: Option<RuntimeError>,
}

impl Evaluator {
    pub(super) fn indexed_frames_supported(
        &self,
        view: FullFunctionView<'_>,
        span: Span,
    ) -> Result<bool, RuntimeError> {
        let header = view.header().map_err(|error| indexed_error(error, span))?;
        if matches!(header.return_kind, LoweredReturnKind::Plain(LoweredType::Stream)) {
            return Ok(false);
        }
        Ok(true)
    }

    pub(super) fn eval_indexed_with_frames(
        &mut self,
        program: &FullProgram,
        function: LoweredFunctionKey,
        kind: LoweredFunctionKind,
        values: &[LoweredValue],
        call_span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        let mut frames = ExplicitFrames {
            evaluator: self,
            program,
            calls: Vec::new(),
            result: None,
            pending_error: None,
        };
        frames.push_call(function, kind, values.to_vec(), call_span, None)?;
        frames.run()
    }

    pub(super) fn eval_indexed_with_frame_slots(
        &mut self,
        program: &FullProgram,
        function: LoweredFunctionKey,
        kind: LoweredFunctionKind,
        slots: Vec<LoweredValue>,
        call_span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        let mut frames = ExplicitFrames {
            evaluator: self,
            program,
            calls: Vec::new(),
            result: None,
            pending_error: None,
        };
        frames.push_call_with_slots(function, kind, slots, call_span, None)?;
        frames.run()
    }
}

impl<'a, 'p> ExplicitFrames<'a, 'p> {
    fn run(&mut self) -> Result<LoweredValue, RuntimeError> {
        while self.result.is_none() {
            let index = self.calls.len().checked_sub(1).expect("active indexed frame");
            let work = self.calls[index].work.pop().expect("indexed frame work");
            if let Err(error) = self.step(index, work) {
                if self.pending_error.is_none() {
                    self.begin_error_unwind(error);
                }
            }
        }
        self.result.take().expect("indexed frame result")
    }

    fn begin_error_unwind(&mut self, error: RuntimeError) {
        if error.abort.as_ref().is_some_and(|signal| signal.force) {
            self.discard_calls();
            self.result = Some(Err(error));
            return;
        }
        self.pending_error = Some(error);
        let Some(index) = self.calls.len().checked_sub(1) else {
            self.result = Some(Err(
                self.pending_error
                    .take()
                    .expect("pending indexed frame error"),
            ));
            return;
        };
        self.calls[index].work.clear();
        self.calls[index].work.push(FrameWork::FinishError);
    }

    fn discard_calls(&mut self) {
        while let Some(mut call) = self.calls.pop() {
            if let Ok(header) = self
                .program
                .function_view(call.function, call.kind)
                .map_err(|error| indexed_error(error, call.call_span))
                .and_then(|view| {
                    view.ok_or_else(|| {
                        RuntimeError::new("unresolved-lowered-call", call.function.display_name())
                            .with_span(call.call_span)
                    })
                })
                .and_then(|view| view.header().map_err(|error| indexed_error(error, call.call_span)))
            {
                let _ = self.evaluator.write_back_lowered_captures(
                    &header,
                    &mut call.slots,
                    call.call_span,
                );
            }
            self.evaluator.recycle_lowered_slots(call.slots);
            self.evaluator.call_stack.pop();
            let exit_kind = match call.kind {
                LoweredFunctionKind::Pure => TraceKind::PureExit,
                LoweredFunctionKind::Proc => TraceKind::ProcExit,
            };
            self.evaluator.trace_exit(
                exit_kind,
                Some(call.call_span),
                Some(&call.name),
                TracePayload::None,
            );
        }
    }

    fn push_call(
        &mut self,
        function: LoweredFunctionKey,
        kind: LoweredFunctionKind,
        values: Vec<LoweredValue>,
        call_span: Span,
        return_to: Option<FrameContinuation>,
    ) -> Result<(), RuntimeError> {
        let view = self
            .program
            .function_view(function, kind)
            .map_err(|error| indexed_error(error, call_span))?
            .ok_or_else(|| {
                RuntimeError::new("unresolved-lowered-call", function.display_name())
                    .with_span(call_span)
            })?;
        let header = view.header().map_err(|error| indexed_error(error, call_span))?;
        let slots = self.evaluator.bind_lowered_values(&header, &values, call_span)?;
        self.push_call_with_header(function, kind, view, header, slots, call_span, return_to)
    }

    fn push_call_with_slots(
        &mut self,
        function: LoweredFunctionKey,
        kind: LoweredFunctionKind,
        slots: Vec<LoweredValue>,
        call_span: Span,
        return_to: Option<FrameContinuation>,
    ) -> Result<(), RuntimeError> {
        let view = self
            .program
            .function_view(function, kind)
            .map_err(|error| indexed_error(error, call_span))?
            .ok_or_else(|| {
                RuntimeError::new("unresolved-lowered-call", function.display_name())
                    .with_span(call_span)
            })?;
        let header = view.header().map_err(|error| indexed_error(error, call_span))?;
        self.push_call_with_header(function, kind, view, header, slots, call_span, return_to)
    }

    fn push_call_with_header(
        &mut self,
        function: LoweredFunctionKey,
        kind: LoweredFunctionKind,
        view: FullFunctionView<'p>,
        header: FunctionHeader,
        mut slots: Vec<LoweredValue>,
        call_span: Span,
        return_to: Option<FrameContinuation>,
    ) -> Result<(), RuntimeError> {
        self.evaluator
            .hydrate_lowered_captures(&header, &mut slots, call_span)?;
        let execution = view
            .execution()
            .map_err(|error| indexed_error(error, call_span))?;
        let (_, body) = view
            .body(&execution)
            .map_err(|error| indexed_error(error, call_span))?;
        let statements = decode_statements(body, call_span)?;
        let (frame_kind, enter_kind) = match kind {
            LoweredFunctionKind::Pure => (TracebackFrameKind::Pure, TraceKind::PureEnter),
            LoweredFunctionKind::Proc => (TracebackFrameKind::Proc, TraceKind::ProcEnter),
        };
        let name = function.display_name();
        self.evaluator
            .trace_enter(enter_kind, Some(call_span), Some(&name), TracePayload::None);
        self.evaluator.call_stack.push(TracebackFrame {
            kind: frame_kind,
            name: name.clone(),
            definition_span: None,
            call_span: Some(call_span),
        });
        self.calls.push(CallFrame {
            function,
            kind,
            execution,
            slots,
            call_span,
            name,
            work: vec![FrameWork::Statements {
                statements,
                complete_call: true,
            }],
            defers: Vec::new(),
            return_to,
        });
        Ok(())
    }

    fn step(&mut self, index: usize, work: FrameWork) -> Result<(), RuntimeError> {
        match work {
            FrameWork::Statements {
                mut statements,
                complete_call,
            } => {
                let Some(statement) = statements.pop() else {
                    return if complete_call {
                        self.complete_call(index, StmtFlow::None)
                    } else {
                        Ok(())
                    };
                };
                self.calls[index].work.push(FrameWork::Statements {
                    statements,
                    complete_call,
                });
                self.calls[index].work.push(FrameWork::Statement(statement));
                Ok(())
            }
            FrameWork::Statement(instruction) => self.eval_statement(index, instruction),
            FrameWork::Expr {
                instruction,
                span,
                next,
            } => self.eval_expr(index, instruction, span, next),
            FrameWork::Value { value, next } => self.continue_value(index, value, next),
            FrameWork::ForItems {
                slot,
                items,
                index: item_index,
                body,
                span,
            } => self.step_for_items(index, slot, items, item_index, body, span),
            FrameWork::ForStrLines {
                slot,
                text,
                cursor,
                line_count,
                body,
                span,
            } => self.step_for_str_lines(index, slot, text, cursor, line_count, body, span),
            FrameWork::While {
                condition,
                body,
                typed,
                span,
            } => self.step_while(index, condition, body, typed, span),
            FrameWork::Finish(flow) => self.finish_deferred_call(index, flow),
            FrameWork::FinishError => self.finish_error_deferred_call(index),
        }
    }

    fn eval_statement(&mut self, index: usize, instruction: u32) -> Result<(), RuntimeError> {
        let span = self.calls[index].call_span;
        let (tag, mut payload) = indexed_value(
            self.calls[index].execution.instruction_id(instruction),
            span,
        )?;
        match tag {
            FullTag::StmtLet => {
                let slot: usize = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                let value = indexed_raw(&mut payload, span)?;
                indexed_finish(payload, span)?;
                self.push_expr(index, value, span, FrameContinuation::Store(slot));
                Ok(())
            }
            FullTag::StmtLetInt => {
                let slot: usize = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                let value = indexed_raw(&mut payload, span)?;
                indexed_finish(payload, span)?;
                let flow = {
                    let call = &mut self.calls[index];
                    self.evaluator.eval_indexed_typed_int(
                        &call.execution,
                        value,
                        &mut call.slots,
                        span,
                    )?
                };
                match flow {
                    ControlFlow::Continue(value) => self.calls[index].slots[slot] = LoweredValue::Int(value),
                    ControlFlow::Break(value) => return self.complete_call(index, StmtFlow::Return(value)),
                }
                Ok(())
            }
            FullTag::StmtLetBool => {
                let slot: usize = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                let value = indexed_raw(&mut payload, span)?;
                indexed_finish(payload, span)?;
                let flow = {
                    let call = &mut self.calls[index];
                    self.evaluator.eval_indexed_typed_bool(
                        &call.execution,
                        value,
                        &mut call.slots,
                        span,
                    )?
                };
                match flow {
                    ControlFlow::Continue(value) => self.calls[index].slots[slot] = LoweredValue::Bool(value),
                    ControlFlow::Break(value) => return self.complete_call(index, StmtFlow::Return(value)),
                }
                Ok(())
            }
            FullTag::StmtAssign => {
                let slot: usize = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                let op = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                let value = indexed_raw(&mut payload, span)?;
                let value_span = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                indexed_finish(payload, span)?;
                self.push_expr(
                    index,
                    value,
                    value_span,
                    FrameContinuation::Assign {
                        slot,
                        op,
                        span: value_span,
                    },
                );
                Ok(())
            }
            FullTag::StmtExpr => {
                let value = indexed_raw(&mut payload, span)?;
                let value_span = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                indexed_finish(payload, span)?;
                self.push_expr(index, value, value_span, FrameContinuation::Discard(value_span));
                Ok(())
            }
            FullTag::StmtIf | FullTag::StmtIfBool => {
                let (_, mut branches) = self.calls[index]
                    .execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, span))?;
                let len = indexed_raw(&mut branches, span)? as usize;
                let mut values = Vec::with_capacity(len);
                for _ in 0..len {
                    values.push((
                        indexed_raw(&mut branches, span)?,
                        indexed_raw(&mut branches, span)?,
                    ));
                }
                indexed_finish(branches, span)?;
                let else_body = indexed_optional_raw(&mut payload, span)?;
                indexed_finish(payload, span)?;
                if tag == FullTag::StmtIf {
                    if let Some((condition, _)) = values.first().copied() {
                        self.push_expr(
                            index,
                            condition,
                            span,
                            FrameContinuation::StatementIf {
                                branches: values,
                                index: 0,
                                else_body,
                                span,
                            },
                        );
                    } else if let Some(body) = else_body {
                        self.push_statement_block(index, body, span)?;
                    }
                    return Ok(());
                }
                let mut selected = None;
                for (condition, body) in values {
                    let flow = {
                        let call = &mut self.calls[index];
                        self.evaluator.eval_indexed_typed_bool(
                            &call.execution,
                            condition,
                            &mut call.slots,
                            span,
                        )?
                    };
                    match flow {
                        ControlFlow::Continue(true) => {
                            selected = Some(body);
                            break;
                        }
                        ControlFlow::Continue(false) => {}
                        ControlFlow::Break(value) => {
                            return self.complete_call(index, StmtFlow::Return(value));
                        }
                    }
                }
                if let Some(body) = selected.or(else_body) {
                    self.push_statement_block(index, body, span)?;
                }
                Ok(())
            }
            FullTag::StmtFor => {
                let slot: usize = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                let iter = indexed_raw(&mut payload, span)?;
                let body = indexed_raw(&mut payload, span)?;
                let value_span = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                indexed_finish(payload, span)?;
                self.push_expr(
                    index,
                    iter,
                    value_span,
                    FrameContinuation::ForItems {
                        slot,
                        body,
                        span: value_span,
                    },
                );
                Ok(())
            }
            FullTag::StmtForStrLines => {
                let slot: usize = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                let text = indexed_raw(&mut payload, span)?;
                let body = indexed_raw(&mut payload, span)?;
                let value_span = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                indexed_finish(payload, span)?;
                self.push_expr(
                    index,
                    text,
                    value_span,
                    FrameContinuation::ForStrLines {
                        slot,
                        body,
                        span: value_span,
                    },
                );
                Ok(())
            }
            FullTag::StmtWhile | FullTag::StmtWhileBool => {
                let condition = indexed_raw(&mut payload, span)?;
                let body = indexed_raw(&mut payload, span)?;
                indexed_finish(payload, span)?;
                self.calls[index].work.push(FrameWork::While {
                    condition,
                    body,
                    typed: tag == FullTag::StmtWhileBool,
                    span,
                });
                Ok(())
            }
            FullTag::StmtMatch => {
                let value = indexed_raw(&mut payload, span)?;
                let (_, mut arms) = self.calls[index]
                    .execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, span))?;
                let arm_count = indexed_raw(&mut arms, span)? as usize;
                let value_span = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                indexed_finish(payload, span)?;
                let mut decoded_arms = Vec::with_capacity(arm_count);
                for _ in 0..arm_count {
                    decoded_arms.push((
                        indexed_raw(&mut arms, value_span)?,
                        indexed_optional_raw(&mut arms, value_span)?,
                        indexed_raw(&mut arms, value_span)?,
                    ));
                }
                indexed_finish(arms, value_span)?;
                self.push_expr(
                    index,
                    value,
                    value_span,
                    FrameContinuation::MatchValue {
                        arms: decoded_arms,
                        span: value_span,
                    },
                );
                Ok(())
            }
            FullTag::StmtBreak => {
                indexed_finish(payload, span)?;
                self.break_loop(index)
            }
            FullTag::StmtBreakValue => {
                let value = indexed_raw(&mut payload, span)?;
                indexed_finish(payload, span)?;
                self.push_expr(index, value, span, FrameContinuation::BreakLoop);
                Ok(())
            }
            FullTag::StmtContinue => {
                indexed_finish(payload, span)?;
                self.continue_loop(index)
            }
            FullTag::StmtDefer => {
                let value = indexed_raw(&mut payload, span)?;
                indexed_finish(payload, span)?;
                self.calls[index].defers.push(value);
                Ok(())
            }
            FullTag::StmtReturn => {
                let value = indexed_raw(&mut payload, span)?;
                indexed_finish(payload, span)?;
                self.push_expr(index, value, span, FrameContinuation::Return);
                Ok(())
            }
            _ => {
                let header = self.call_header(index)?;
                let flow = {
                    let call = &mut self.calls[index];
                    self.evaluator.eval_indexed_stmt(
                        &call.execution,
                        instruction,
                        &header,
                        &mut call.slots,
                        span,
                    )?
                };
                match flow {
                    StmtFlow::None => Ok(()),
                    StmtFlow::Return(value) => {
                        self.complete_call(index, StmtFlow::Return(value))
                    }
                    StmtFlow::Propagate(value) => {
                        self.complete_call(index, StmtFlow::Propagate(value))
                    }
                    StmtFlow::Break(_) => Err(RuntimeError::new(
                        "control-flow",
                        "break outside loop",
                    )
                    .with_span(span)),
                    StmtFlow::Continue => Err(RuntimeError::new(
                        "control-flow",
                        "continue outside loop",
                    )
                    .with_span(span)),
                }
            }
        }
    }

    fn eval_expr(
        &mut self,
        index: usize,
        instruction: u32,
        span: Span,
        next: FrameContinuation,
    ) -> Result<(), RuntimeError> {
        let (tag, mut payload) = indexed_value(
            self.calls[index].execution.instruction_id(instruction),
            span,
        )?;
        match tag {
            FullTag::ExprNull => {
                indexed_finish(payload, span)?;
                self.push_value(index, FrameValue::Value(LoweredValue::Null), next);
            }
            FullTag::ExprUnit => {
                indexed_finish(payload, span)?;
                self.push_value(index, FrameValue::Value(LoweredValue::Unit), next);
            }
            FullTag::ExprInt => {
                let value = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                indexed_finish(payload, span)?;
                self.push_value(index, FrameValue::Value(LoweredValue::Int(value)), next);
            }
            FullTag::ExprBool => {
                let value = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                indexed_finish(payload, span)?;
                self.push_value(index, FrameValue::Value(LoweredValue::Bool(value)), next);
            }
            FullTag::ExprStr => {
                let value = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                indexed_finish(payload, span)?;
                self.push_value(index, FrameValue::Value(LoweredValue::Str(value)), next);
            }
            FullTag::ExprParam => {
                let slot: usize = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                indexed_finish(payload, span)?;
                lowered_freeze_large_slot_list(&mut self.calls[index].slots[slot]);
                self.push_value(index, FrameValue::Value(self.calls[index].slots[slot].clone()), next);
            }
            FullTag::ExprBinary => {
                let op = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                let left = indexed_raw(&mut payload, span)?;
                let right = indexed_raw(&mut payload, span)?;
                let value_span = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                indexed_finish(payload, span)?;
                self.push_expr(
                    index,
                    left,
                    value_span,
                    FrameContinuation::BinaryLeft {
                        op,
                        right,
                        span: value_span,
                        next: Box::new(next),
                    },
                );
            }
            FullTag::ExprIf => {
                let (_, mut branches) = self.calls[index]
                    .execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, span))?;
                let len = indexed_raw(&mut branches, span)? as usize;
                let mut values = Vec::with_capacity(len);
                for _ in 0..len {
                    values.push((indexed_raw(&mut branches, span)?, indexed_raw(&mut branches, span)?));
                }
                indexed_finish(branches, span)?;
                let else_value = indexed_raw(&mut payload, span)?;
                let value_span = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                indexed_finish(payload, span)?;
                if let Some((condition, _)) = values.first().copied() {
                    self.push_expr(
                        index,
                        condition,
                        value_span,
                        FrameContinuation::If {
                            branches: values,
                            index: 0,
                            else_value,
                            span: value_span,
                            next: Box::new(next),
                        },
                    );
                } else {
                    self.push_expr(index, else_value, value_span, next);
                }
            }
            FullTag::ExprMatch => {
                let value = indexed_raw(&mut payload, span)?;
                let (_, mut arms) = self.calls[index]
                    .execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, span))?;
                let arm_count = indexed_raw(&mut arms, span)? as usize;
                let value_span = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                indexed_finish(payload, span)?;
                let mut decoded_arms = Vec::with_capacity(arm_count);
                for _ in 0..arm_count {
                    decoded_arms.push((
                        indexed_raw(&mut arms, value_span)?,
                        indexed_optional_raw(&mut arms, value_span)?,
                        indexed_raw(&mut arms, value_span)?,
                    ));
                }
                indexed_finish(arms, value_span)?;
                self.push_expr(
                    index,
                    value,
                    value_span,
                    FrameContinuation::MatchExprValue {
                        arms: decoded_arms,
                        span: value_span,
                        next: Box::new(next),
                    },
                );
            }
            FullTag::ExprResultFallback => {
                let left = indexed_raw(&mut payload, span)?;
                let right = indexed_raw(&mut payload, span)?;
                indexed_finish(payload, span)?;
                self.push_expr(
                    index,
                    left,
                    span,
                    FrameContinuation::ResultFallback {
                        right,
                        span,
                        next: Box::new(next),
                    },
                );
            }
            FullTag::ExprOk => {
                let value = indexed_raw(&mut payload, span)?;
                indexed_finish(payload, span)?;
                self.push_expr(index, value, span, FrameContinuation::WrapOk(Box::new(next)));
            }
            FullTag::ExprErr => {
                let value = indexed_raw(&mut payload, span)?;
                indexed_finish(payload, span)?;
                self.push_expr(index, value, span, FrameContinuation::WrapErr(Box::new(next)));
            }
            FullTag::ExprTry => {
                let value = indexed_raw(&mut payload, span)?;
                indexed_finish(payload, span)?;
                self.push_expr(
                    index,
                    value,
                    span,
                    FrameContinuation::Try {
                        span,
                        next: Box::new(next),
                    },
                );
            }
            FullTag::ExprRequire => {
                let value = indexed_raw(&mut payload, span)?;
                let check = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                let value_span = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                indexed_finish(payload, span)?;
                self.push_expr(
                    index,
                    value,
                    value_span,
                    FrameContinuation::Require {
                        check,
                        span: value_span,
                        next: Box::new(next),
                    },
                );
            }
            FullTag::ExprMethod => {
                let receiver = indexed_raw(&mut payload, span)?;
                let name = indexed_string(&mut payload, &self.calls[index].execution, span)?;
                let (_, mut args) = self.calls[index]
                    .execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, span))?;
                let len = indexed_raw(&mut args, span)? as usize;
                let value_span = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                indexed_finish(payload, span)?;
                let mut decoded_args = Vec::with_capacity(len);
                for _ in 0..len {
                    decoded_args.push(indexed_raw(&mut args, value_span)?);
                }
                indexed_finish(args, value_span)?;
                self.push_expr(
                    index,
                    receiver,
                    value_span,
                    FrameContinuation::MethodReceiver {
                        name: Arc::from(name),
                        args: decoded_args,
                        span: value_span,
                        next: Box::new(next),
                    },
                );
            }
            FullTag::ExprCall | FullTag::ExprSelfCall | FullTag::ExprDirectPureCall => {
                let function = if matches!(tag, FullTag::ExprCall | FullTag::ExprDirectPureCall) {
                    indexed_decode(&mut payload, &self.calls[index].execution, span)?
                } else {
                    self.calls[index]
                        .execution
                        .function_identity()
                        .map_err(|error| indexed_error(error, span))?
                        .0
                };
                let kind = self.function_kind(function, span)?;
                let args = decode_call_args(&self.calls[index].execution, &mut payload, span)?;
                let value_span = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                indexed_finish(payload, span)?;
                if let Some((_, value)) = args.first().copied() {
                    self.push_expr(
                        index,
                        value,
                        value_span,
                        FrameContinuation::CallArguments {
                            function,
                            kind,
                            args,
                            index: 0,
                            values: Vec::new(),
                            span: value_span,
                            next: Box::new(next),
                        },
                    );
                } else {
                    self.push_call(function, kind, Vec::new(), value_span, Some(next))?;
                }
            }
            _ => {
                let flow = {
                    let call = &mut self.calls[index];
                    self.evaluator.eval_indexed_expr(
                        &call.execution,
                        instruction,
                        &mut call.slots,
                        span,
                    )?
                };
                let value = match flow {
                    ControlFlow::Continue(value) => FrameValue::Value(value),
                    ControlFlow::Break(value) => FrameValue::Break(value),
                };
                self.push_value(index, value, next);
            }
        }
        Ok(())
    }

    fn continue_value(
        &mut self,
        index: usize,
        value: FrameValue,
        next: FrameContinuation,
    ) -> Result<(), RuntimeError> {
        match next {
            FrameContinuation::Store(slot) => match value {
                FrameValue::Value(value) => self.calls[index].slots[slot] = value,
                FrameValue::Break(value) => return self.complete_call(index, StmtFlow::Return(value)),
            },
            FrameContinuation::Assign { slot, op, span } => match value {
                FrameValue::Value(value) => {
                    let current = self.calls[index].slots[slot].clone();
                    self.calls[index].slots[slot] = lowered_assign_value(op, current, value, span)?;
                }
                FrameValue::Break(value) => return self.complete_call(index, StmtFlow::Return(value)),
            },
            FrameContinuation::Return => {
                let value = match value {
                    FrameValue::Value(value) | FrameValue::Break(value) => value,
                };
                return self.complete_call(index, StmtFlow::Return(value));
            }
            FrameContinuation::Discard(span) => match value {
                FrameValue::Value(value @ LoweredValue::ResultErr(_)) => {
                    let value = self.evaluator.lowered_question_propagation_value(value, span)?;
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
                FrameValue::Value(_) => {}
                FrameValue::Break(value) => return self.complete_call(index, StmtFlow::Propagate(value)),
            },
            FrameContinuation::BinaryLeft { op, right, span, next } => match value {
                FrameValue::Value(left) if op == BinaryOp::And || op == BinaryOp::Or => {
                    let left = frame_condition_bool(left, span)?;
                    if (op == BinaryOp::And && !left) || (op == BinaryOp::Or && left) {
                        self.push_value(index, FrameValue::Value(LoweredValue::Bool(left)), *next);
                    } else {
                        self.push_expr(
                            index,
                            right,
                            span,
                            FrameContinuation::BoolBinaryRight { next, span },
                        );
                    }
                }
                FrameValue::Value(left) => self.push_expr(
                    index,
                    right,
                    span,
                    FrameContinuation::BinaryRight { op, left, span, next },
                ),
                FrameValue::Break(value) => return self.complete_call(index, StmtFlow::Return(value)),
            },
            FrameContinuation::BinaryRight { op, left, span, next } => match value {
                FrameValue::Value(right) => self.push_value(
                    index,
                    FrameValue::Value(lowered_binary_value(op, left, right, span)?),
                    *next,
                ),
                FrameValue::Break(value) => return self.complete_call(index, StmtFlow::Return(value)),
            },
            FrameContinuation::BoolBinaryRight { next, span } => match value {
                FrameValue::Value(value) => self.push_value(
                    index,
                    FrameValue::Value(LoweredValue::Bool(frame_condition_bool(value, span)?)),
                    *next,
                ),
                FrameValue::Break(value) => return self.complete_call(index, StmtFlow::Return(value)),
            },
            FrameContinuation::If { branches, index: branch, else_value, span, next } => match value {
                FrameValue::Value(value) => {
                    if frame_condition_bool(value, span)? {
                        self.push_expr(index, branches[branch].1, span, *next);
                    } else {
                        let next_index = branch + 1;
                        if let Some((condition, _)) = branches.get(next_index).copied() {
                            self.push_expr(
                                index,
                                condition,
                                span,
                                FrameContinuation::If {
                                    branches,
                                    index: next_index,
                                    else_value,
                                    span,
                                    next,
                                },
                            );
                        } else {
                            self.push_expr(index, else_value, span, *next);
                        }
                    }
                }
                FrameValue::Break(value) => return self.complete_call(index, StmtFlow::Return(value)),
            },
            FrameContinuation::StatementIf { branches, index: branch, else_body, span } => match value {
                FrameValue::Value(value) => {
                    if frame_condition_bool(value, span)? {
                        self.push_statement_block(index, branches[branch].1, span)?;
                    } else {
                        let next_index = branch + 1;
                        if let Some((condition, _)) = branches.get(next_index).copied() {
                            self.push_expr(
                                index,
                                condition,
                                span,
                                FrameContinuation::StatementIf {
                                    branches,
                                    index: next_index,
                                    else_body,
                                    span,
                                },
                            );
                        } else if let Some(body) = else_body {
                            self.push_statement_block(index, body, span)?;
                        }
                    }
                }
                FrameValue::Break(value) => return self.complete_call(index, StmtFlow::Return(value)),
            },
            FrameContinuation::ForItems { slot, body, span } => match value {
                FrameValue::Value(value) => {
                    let items = self
                        .evaluator
                        .lowered_list_items(value, span, "lowered for expected List")?;
                    self.calls[index].work.push(FrameWork::ForItems {
                        slot,
                        items,
                        index: 0,
                        body,
                        span,
                    });
                }
                FrameValue::Break(value) => return self.complete_call(index, StmtFlow::Return(value)),
            },
            FrameContinuation::ForStrLines { slot, body, span } => match value {
                FrameValue::Value(value) => {
                    let start = if let Some((_, start, _)) = lowered_str_parts(&value) {
                        start
                    } else if let Some((_, start, _)) = lowered_bytes_parts(&value) {
                        start
                    } else {
                        return Err(RuntimeError::new(
                            "type-error",
                            "lowered for lines expected Str or Bytes",
                        )
                        .with_span(span));
                    };
                    self.calls[index].work.push(FrameWork::ForStrLines {
                        slot,
                        text: value,
                        cursor: start,
                        line_count: 0,
                        body,
                        span,
                    });
                }
                FrameValue::Break(value) => return self.complete_call(index, StmtFlow::Return(value)),
            },
            FrameContinuation::While {
                condition,
                body,
                span,
            } => match value {
                FrameValue::Value(value) => {
                    if frame_condition_bool(value, span)? {
                        self.calls[index].work.push(FrameWork::While {
                            condition,
                            body,
                            typed: false,
                            span,
                        });
                        self.push_statement_block(index, body, span)?;
                    }
                }
                FrameValue::Break(value) => return self.complete_call(index, StmtFlow::Return(value)),
            },
            FrameContinuation::MatchValue { arms, span } => match value {
                FrameValue::Value(value) => self.select_match_arm(index, arms, 0, value, span)?,
                FrameValue::Break(value) => return self.complete_call(index, StmtFlow::Return(value)),
            },
            FrameContinuation::MatchGuard { arms, index: arm_index, value: match_value, span } => match value {
                FrameValue::Value(value) => {
                    if frame_condition_bool(value, span)? {
                        self.push_statement_block(index, arms[arm_index].2, span)?;
                    } else {
                        self.select_match_arm(index, arms, arm_index + 1, match_value, span)?;
                    }
                }
                FrameValue::Break(value) => return self.complete_call(index, StmtFlow::Return(value)),
            },
            FrameContinuation::MatchExprValue { arms, next, span, .. } => match value {
                FrameValue::Value(value) => {
                    self.select_expr_match_arm(index, arms, 0, value, span, *next)?;
                }
                FrameValue::Break(value) => return self.complete_call(index, StmtFlow::Return(value)),
            },
            FrameContinuation::MatchExprGuard {
                arms,
                index: arm_index,
                value: match_value,
                span,
                next,
            } => match value {
                FrameValue::Value(value) => {
                    if matches!(value, LoweredValue::Bool(true)) {
                        self.push_expr(index, arms[arm_index].2, span, *next);
                    } else {
                        self.select_expr_match_arm(index, arms, arm_index + 1, match_value, span, *next)?;
                    }
                }
                FrameValue::Break(value) => return self.complete_call(index, StmtFlow::Return(value)),
            },
            FrameContinuation::BreakLoop => match value {
                FrameValue::Value(_) => return self.break_loop(index),
                FrameValue::Break(value) => return self.complete_call(index, StmtFlow::Return(value)),
            },
            FrameContinuation::Defer => match value {
                FrameValue::Value(_) => {}
                FrameValue::Break(_) => {
                    return Err(RuntimeError::new(
                        "defer-control-flow",
                        "deferred expression produced invalid control flow",
                    )
                    .with_span(self.calls[index].call_span));
                }
            },
            FrameContinuation::CallArguments {
                function,
                kind,
                args,
                index: argument,
                mut values,
                span,
                next,
            } => match value {
                FrameValue::Value(value) => {
                    match args[argument].0 {
                        0 => values.push(value),
                        1 => values.extend(lowered_splice_arg_items(value, span)?),
                        _ => return Err(RuntimeError::new("indexed-ir", "invalid call argument kind").with_span(span)),
                    }
                    let next_index = argument + 1;
                    if let Some((_, instruction)) = args.get(next_index).copied() {
                        self.push_expr(
                            index,
                            instruction,
                            span,
                            FrameContinuation::CallArguments {
                                function,
                                kind,
                                args,
                                index: next_index,
                                values,
                                span,
                                next,
                            },
                        );
                    } else {
                        let stream_call = match self
                            .program
                            .function_view(function, kind)
                            .map_err(|error| indexed_error(error, span))?
                        {
                            Some(view) => matches!(
                                view.header()
                                    .map_err(|error| indexed_error(error, span))?
                                    .return_kind,
                                LoweredReturnKind::Plain(LoweredType::Stream)
                            ),
                            None => false,
                        };
                        if stream_call {
                            let value = self
                                .evaluator
                                .eval_indexed_named_call(function, &values, span)?;
                            self.push_value(index, FrameValue::Value(value), *next);
                        } else {
                            self.push_call(function, kind, values, span, Some(*next))?;
                        }
                    }
                }
                FrameValue::Break(value) => return self.complete_call(index, StmtFlow::Return(value)),
            },
            FrameContinuation::WrapOk(next) => match value {
                FrameValue::Value(value) => self.push_value(index, FrameValue::Value(LoweredValue::ResultOk(Box::new(value))), *next),
                FrameValue::Break(value) => return self.complete_call(index, StmtFlow::Return(value)),
            },
            FrameContinuation::WrapErr(next) => match value {
                FrameValue::Value(value) => self.push_value(index, FrameValue::Value(LoweredValue::ResultErr(Box::new(value.into_value()))), *next),
                FrameValue::Break(value) => return self.complete_call(index, StmtFlow::Return(value)),
            },
            FrameContinuation::Try { span, next } => match value {
                FrameValue::Value(LoweredValue::ResultOk(value)) => self.push_value(index, FrameValue::Value(*value), *next),
                FrameValue::Value(LoweredValue::ResultErr(error)) => {
                    let value = self.evaluator.lowered_question_propagation_value(LoweredValue::ResultErr(error), span)?;
                    self.push_value(index, FrameValue::Break(value), *next);
                }
                FrameValue::Value(_) => return Err(RuntimeError::new("type-error", "lowered `?` expected Result").with_span(span)),
                FrameValue::Break(value) => self.push_value(index, FrameValue::Break(value), *next),
            },
            FrameContinuation::Require { check, span, next } => match value {
                FrameValue::Value(value) => {
                    let value = if lowered_value_satisfies_require(self.evaluator, &value, &check.ty) {
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
                    };
                    self.push_value(index, FrameValue::Value(value), *next);
                }
                FrameValue::Break(value) => self.push_value(index, FrameValue::Break(value), *next),
            },
            FrameContinuation::MethodReceiver {
                name,
                args,
                span,
                next,
                ..
            } => match value {
                FrameValue::Value(receiver) => {
                    if let Some(argument) = args.first().copied() {
                        self.push_expr(
                            index,
                            argument,
                            span,
                            FrameContinuation::MethodArg {
                                name,
                                args,
                                receiver,
                                index: 0,
                                values: Vec::new(),
                                span,
                                next,
                            },
                        );
                    } else {
                        self.push_method_result(index, receiver, name, Vec::new(), span, *next)?;
                    }
                }
                FrameValue::Break(value) => return self.complete_call(index, StmtFlow::Return(value)),
            },
            FrameContinuation::MethodArg {
                name,
                args,
                receiver,
                index: argument,
                mut values,
                span,
                next,
            } => match value {
                FrameValue::Value(value) => {
                    values.push(value);
                    let next_index = argument + 1;
                    if let Some(instruction) = args.get(next_index).copied() {
                        self.push_expr(
                            index,
                            instruction,
                            span,
                            FrameContinuation::MethodArg {
                                name,
                                args,
                                receiver,
                                index: next_index,
                                values,
                                span,
                                next,
                            },
                        );
                    } else {
                        self.push_method_result(index, receiver, name, values, span, *next)?;
                    }
                }
                FrameValue::Break(value) => return self.complete_call(index, StmtFlow::Return(value)),
            },
            FrameContinuation::ResultFallback { right, span, next } => match value {
                FrameValue::Value(LoweredValue::ResultOk(value)) => self.push_value(index, FrameValue::Value(*value), *next),
                FrameValue::Value(LoweredValue::ResultErr(_) | LoweredValue::Null) => self.push_expr(index, right, span, *next),
                FrameValue::Value(value) => self.push_value(index, FrameValue::Value(value), *next),
                FrameValue::Break(value) => self.push_value(index, FrameValue::Break(value), *next),
            },
        }
        Ok(())
    }

    fn complete_call(&mut self, index: usize, flow: StmtFlow) -> Result<(), RuntimeError> {
        self.calls[index].work.clear();
        if self.calls[index].defers.is_empty() {
            self.finish_call(index, flow)
        } else {
            self.calls[index].work.push(FrameWork::Finish(flow));
            Ok(())
        }
    }

    fn push_method_result(
        &mut self,
        index: usize,
        receiver: LoweredValue,
        name: Arc<str>,
        values: Vec<LoweredValue>,
        span: Span,
        next: FrameContinuation,
    ) -> Result<(), RuntimeError> {
        let result = if !self.evaluator.trace_enabled {
            self.evaluator
                .eval_lowered_method_dispatch(receiver, name.as_ref(), values, &span)?
        } else {
            let trace_name = format!("{}.{}", receiver.type_name(), name);
            self.evaluator.trace_enter(
                TraceKind::MethodCall,
                Some(span),
                Some(&trace_name),
                TracePayload::None,
            );
            let result = self
                .evaluator
                .eval_lowered_method_dispatch(receiver, name.as_ref(), values, &span);
            self.evaluator.trace_exit(
                TraceKind::MethodResult,
                Some(span),
                Some(&trace_name),
                TracePayload::None,
            );
            result?
        };
        let result = match result {
            ControlFlow::Continue(value) => FrameValue::Value(value),
            ControlFlow::Break(value) => FrameValue::Break(value),
        };
        self.push_value(index, result, next);
        Ok(())
    }

    fn finish_deferred_call(
        &mut self,
        index: usize,
        flow: StmtFlow,
    ) -> Result<(), RuntimeError> {
        let Some(value) = self.calls[index].defers.pop() else {
            return self.finish_call(index, flow);
        };
        let span = self.calls[index].call_span;
        self.calls[index].work.push(FrameWork::Finish(flow));
        self.push_expr(index, value, span, FrameContinuation::Defer);
        Ok(())
    }

    fn finish_error_deferred_call(&mut self, index: usize) -> Result<(), RuntimeError> {
        let Some(value) = self.calls[index].defers.pop() else {
            return self.finish_error_call(index);
        };
        let span = self.calls[index].call_span;
        self.calls[index].work.push(FrameWork::FinishError);
        self.push_expr(index, value, span, FrameContinuation::Defer);
        Ok(())
    }

    fn finish_error_call(&mut self, index: usize) -> Result<(), RuntimeError> {
        debug_assert_eq!(index, self.calls.len() - 1);
        let mut call = self.calls.pop().expect("active indexed frame");
        if let Ok(header) = self
            .program
            .function_view(call.function, call.kind)
            .map_err(|error| indexed_error(error, call.call_span))
            .and_then(|view| {
                view.ok_or_else(|| {
                    RuntimeError::new("unresolved-lowered-call", call.function.display_name())
                        .with_span(call.call_span)
                })
            })
            .and_then(|view| view.header().map_err(|error| indexed_error(error, call.call_span)))
        {
            let _ = self.evaluator.write_back_lowered_captures(
                &header,
                &mut call.slots,
                call.call_span,
            );
        }
        self.evaluator.recycle_lowered_slots(call.slots);
        self.evaluator.call_stack.pop();
        let exit_kind = match call.kind {
            LoweredFunctionKind::Pure => TraceKind::PureExit,
            LoweredFunctionKind::Proc => TraceKind::ProcExit,
        };
        self.evaluator.trace_exit(
            exit_kind,
            Some(call.call_span),
            Some(&call.name),
            TracePayload::None,
        );
        if let Some(parent) = self.calls.len().checked_sub(1) {
            self.calls[parent].work.clear();
            self.calls[parent].work.push(FrameWork::FinishError);
        } else {
            self.result = Some(Err(
                self.pending_error
                    .take()
                    .expect("pending indexed frame error"),
            ));
        }
        Ok(())
    }

    fn finish_call(&mut self, index: usize, flow: StmtFlow) -> Result<(), RuntimeError> {
        debug_assert_eq!(index, self.calls.len() - 1);
        let mut call = self.calls.pop().expect("active indexed frame");
        let view = self
            .program
            .function_view(call.function, call.kind)
            .map_err(|error| indexed_error(error, call.call_span))?
            .expect("active indexed frame function");
        let header = view
            .header()
            .map_err(|error| indexed_error(error, call.call_span))?;
        let value = match flow {
            StmtFlow::Return(value) | StmtFlow::Propagate(value) => {
                lowered_return_value(header.return_kind.clone(), value, call.call_span)
            }
            StmtFlow::None => Err(RuntimeError::new("return", "lowered function did not return").with_span(call.call_span)),
            StmtFlow::Break(_) => Err(RuntimeError::new("control-flow", "break outside loop").with_span(call.call_span)),
            StmtFlow::Continue => Err(RuntimeError::new("control-flow", "continue outside loop").with_span(call.call_span)),
        };
        let write_back = self.evaluator.write_back_lowered_captures(
            &header,
            &mut call.slots,
            call.call_span,
        );
        self.evaluator.recycle_lowered_slots(call.slots);
        let exit_kind = match call.kind {
            LoweredFunctionKind::Pure => TraceKind::PureExit,
            LoweredFunctionKind::Proc => TraceKind::ProcExit,
        };
        self.evaluator.call_stack.pop();
        self.evaluator.trace_exit(
            exit_kind,
            Some(call.call_span),
            Some(&call.name),
            TracePayload::None,
        );
        let value = value.and_then(|value| {
            write_back?;
            Ok(value)
        });
        match (call.return_to, value) {
            (Some(next), Ok(value)) => {
                let parent = self.calls.len() - 1;
                self.push_value(parent, FrameValue::Value(value), next);
            }
            (Some(_), Err(error)) | (None, Err(error)) => self.begin_error_unwind(error),
            (None, Ok(value)) => self.result = Some(Ok(value)),
        }
        Ok(())
    }

    fn call_header(&self, index: usize) -> Result<FunctionHeader, RuntimeError> {
        let call = &self.calls[index];
        self.program
            .function_view(call.function, call.kind)
            .map_err(|error| indexed_error(error, call.call_span))?
            .expect("active indexed frame function")
            .header()
            .map_err(|error| indexed_error(error, call.call_span))
    }

    fn select_match_arm(
        &mut self,
        index: usize,
        arms: Vec<(u32, Option<u32>, u32)>,
        start: usize,
        value: LoweredValue,
        span: Span,
    ) -> Result<(), RuntimeError> {
        for arm_index in start..arms.len() {
            let (pattern, guard, body) = arms[arm_index];
            let matches = {
                let call = &mut self.calls[index];
                Evaluator::indexed_pattern_matches(
                    &call.execution,
                    pattern,
                    &value,
                    &mut call.slots,
                    span,
                )?
            };
            if !matches {
                continue;
            }
            if let Some(guard) = guard {
                self.push_expr(
                    index,
                    guard,
                    span,
                    FrameContinuation::MatchGuard {
                        arms,
                        index: arm_index,
                        value,
                        span,
                    },
                );
            } else {
                self.push_statement_block(index, body, span)?;
            }
            return Ok(());
        }
        Err(lowered_match_no_arm(span))
    }

    fn step_for_items(
        &mut self,
        index: usize,
        slot: usize,
        items: Vec<LoweredValue>,
        item_index: usize,
        body: u32,
        span: Span,
    ) -> Result<(), RuntimeError> {
        self.evaluator.service_pending_signal(span)?;
        if self.evaluator.signal_state.shutdown_complete || item_index == items.len() {
            return Ok(());
        }
        self.calls[index].slots[slot] = items[item_index].clone();
        self.calls[index].work.push(FrameWork::ForItems {
            slot,
            items,
            index: item_index + 1,
            body,
            span,
        });
        self.push_statement_block(index, body, span)
    }

    fn step_for_str_lines(
        &mut self,
        index: usize,
        slot: usize,
        text: LoweredValue,
        cursor: usize,
        line_count: u32,
        body: u32,
        span: Span,
    ) -> Result<(), RuntimeError> {
        if let Some((bytes, _, end)) = lowered_bytes_parts(&text) {
            if cursor >= end {
                return Ok(());
            }
            let newline = memchr::memchr(b'\n', &bytes[cursor..end]).map(|offset| cursor + offset);
            let line_end = newline.unwrap_or(end);
            let view_end = if line_end > cursor && bytes[line_end - 1] == b'\r' {
                line_end - 1
            } else {
                line_end
            };
            let line_count = line_count.wrapping_add(1);
            if line_count & 63 == 0 {
                self.evaluator.service_pending_signal(span)?;
                if self.evaluator.signal_state.shutdown_complete {
                    return Ok(());
                }
            }
            assign_lowered_bytes_view(&mut self.calls[index].slots[slot], &bytes, cursor, view_end);
            self.calls[index].work.push(FrameWork::ForStrLines {
                slot,
                text,
                cursor: newline.map_or(end, |offset| offset + 1),
                line_count,
                body,
                span,
            });
            return self.push_statement_block(index, body, span);
        }
        let Some((text_value, _, end)) = lowered_str_parts(&text) else {
            return Err(RuntimeError::new(
                "type-error",
                "lowered for lines expected Str or Bytes",
            )
            .with_span(span));
        };
        if cursor >= end {
            return Ok(());
        }
        let bytes = text_value.as_bytes();
        let newline = memchr::memchr(b'\n', &bytes[cursor..end]).map(|offset| cursor + offset);
        let line_end = newline.unwrap_or(end);
        let view_end = if line_end > cursor && bytes[line_end - 1] == b'\r' {
            line_end - 1
        } else {
            line_end
        };
        let line_count = line_count.wrapping_add(1);
        if line_count & 63 == 0 {
            self.evaluator.service_pending_signal(span)?;
            if self.evaluator.signal_state.shutdown_complete {
                return Ok(());
            }
        }
        assign_lowered_str_view(&mut self.calls[index].slots[slot], &text_value, cursor, view_end);
        self.calls[index].work.push(FrameWork::ForStrLines {
            slot,
            text,
            cursor: newline.map_or(end, |offset| offset + 1),
            line_count,
            body,
            span,
        });
        self.push_statement_block(index, body, span)
    }

    fn step_while(
        &mut self,
        index: usize,
        condition: u32,
        body: u32,
        typed: bool,
        span: Span,
    ) -> Result<(), RuntimeError> {
        self.evaluator.service_pending_signal(span)?;
        if self.evaluator.signal_state.shutdown_complete {
            return Ok(());
        }
        if typed {
            let value = {
                let call = &mut self.calls[index];
                self.evaluator.eval_indexed_typed_bool(
                    &call.execution,
                    condition,
                    &mut call.slots,
                    span,
                )?
            };
            match value {
                ControlFlow::Continue(true) => {
                    self.calls[index].work.push(FrameWork::While {
                        condition,
                        body,
                        typed,
                        span,
                    });
                    self.push_statement_block(index, body, span)
                }
                ControlFlow::Continue(false) => Ok(()),
                ControlFlow::Break(value) => {
                    self.complete_call(index, StmtFlow::Return(value))
                }
            }
        } else {
            self.push_expr(
                index,
                condition,
                span,
                FrameContinuation::While {
                    condition,
                    body,
                    span,
                },
            );
            Ok(())
        }
    }

    fn break_loop(&mut self, index: usize) -> Result<(), RuntimeError> {
        let Some(loop_index) = self.calls[index].work.iter().rposition(|work| {
            matches!(
                work,
                FrameWork::ForItems { .. }
                    | FrameWork::ForStrLines { .. }
                    | FrameWork::While { .. }
            )
        }) else {
            return Err(RuntimeError::new("control-flow", "break outside loop")
                .with_span(self.calls[index].call_span));
        };
        self.calls[index].work.truncate(loop_index);
        Ok(())
    }

    fn select_expr_match_arm(
        &mut self,
        index: usize,
        arms: Vec<(u32, Option<u32>, u32)>,
        start: usize,
        value: LoweredValue,
        span: Span,
        next: FrameContinuation,
    ) -> Result<(), RuntimeError> {
        for arm_index in start..arms.len() {
            let (pattern, guard, body) = arms[arm_index];
            let matches = {
                let call = &mut self.calls[index];
                Evaluator::indexed_pattern_matches(
                    &call.execution,
                    pattern,
                    &value,
                    &mut call.slots,
                    span,
                )?
            };
            if !matches {
                continue;
            }
            if let Some(guard) = guard {
                self.push_expr(
                    index,
                    guard,
                    span,
                    FrameContinuation::MatchExprGuard {
                        arms,
                        index: arm_index,
                        value,
                        span,
                        next: Box::new(next),
                    },
                );
            } else {
                self.push_expr(index, body, span, next);
            }
            return Ok(());
        }
        Err(lowered_match_no_arm(span))
    }

    fn continue_loop(&mut self, index: usize) -> Result<(), RuntimeError> {
        let Some(loop_index) = self.calls[index].work.iter().rposition(|work| {
            matches!(
                work,
                FrameWork::ForItems { .. }
                    | FrameWork::ForStrLines { .. }
                    | FrameWork::While { .. }
            )
        }) else {
            return Err(RuntimeError::new("control-flow", "continue outside loop")
                .with_span(self.calls[index].call_span));
        };
        self.calls[index].work.truncate(loop_index + 1);
        Ok(())
    }

    fn function_kind(&self, function: LoweredFunctionKey, span: Span) -> Result<LoweredFunctionKind, RuntimeError> {
        if self
            .program
            .function_view(function, LoweredFunctionKind::Pure)
            .map_err(|error| indexed_error(error, span))?
            .is_some()
        {
            Ok(LoweredFunctionKind::Pure)
        } else if self
            .program
            .function_view(function, LoweredFunctionKind::Proc)
            .map_err(|error| indexed_error(error, span))?
            .is_some()
        {
            Ok(LoweredFunctionKind::Proc)
        } else {
            Err(RuntimeError::new("unresolved-lowered-call", function.display_name()).with_span(span))
        }
    }

    fn push_expr(&mut self, index: usize, instruction: u32, span: Span, next: FrameContinuation) {
        self.calls[index].work.push(FrameWork::Expr {
            instruction,
            span,
            next,
        });
    }

    fn push_value(&mut self, index: usize, value: FrameValue, next: FrameContinuation) {
        self.calls[index].work.push(FrameWork::Value { value, next });
    }

    fn push_statement_block(&mut self, index: usize, body: u32, span: Span) -> Result<(), RuntimeError> {
        let statements = decode_statement_block(&self.calls[index].execution, body, span)?;
        self.calls[index].work.push(FrameWork::Statements {
            statements,
            complete_call: false,
        });
        Ok(())
    }
}

fn frame_condition_bool(value: LoweredValue, span: Span) -> Result<bool, RuntimeError> {
    match value {
        LoweredValue::Bool(value) => Ok(value),
        LoweredValue::Status(status) => Ok(status.success),
        _ => Err(RuntimeError::new("type-error", "lowered expression expected Bool").with_span(span)),
    }
}

fn decode_statements(mut payload: FullPayload<'_>, span: Span) -> Result<Vec<u32>, RuntimeError> {
    let len = indexed_raw(&mut payload, span)? as usize;
    let mut statements = Vec::with_capacity(len);
    for _ in 0..len {
        statements.push(indexed_raw(&mut payload, span)?);
    }
    indexed_finish(payload, span)?;
    statements.reverse();
    Ok(statements)
}

fn decode_statement_block(
    execution: &FullExecution<'_>,
    block: u32,
    span: Span,
) -> Result<Vec<u32>, RuntimeError> {
    let (_, payload) = execution
        .block_id(block, BLOCK_STATEMENTS)
        .map_err(|error| indexed_error(error, span))?;
    decode_statements(payload, span)
}

fn decode_call_args<'a>(
    execution: &FullExecution<'a>,
    payload: &mut FullPayload<'a>,
    span: Span,
) -> Result<Vec<(u32, u32)>, RuntimeError> {
    let (_, mut args) = execution
        .block(payload, BLOCK_LIST)
        .map_err(|error| indexed_error(error, span))?;
    let len = indexed_raw(&mut args, span)? as usize;
    let mut values = Vec::with_capacity(len);
    for _ in 0..len {
        values.push((indexed_raw(&mut args, span)?, indexed_raw(&mut args, span)?));
    }
    indexed_finish(args, span)?;
    Ok(values)
}
