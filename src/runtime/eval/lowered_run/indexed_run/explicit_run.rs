use super::{
    Arc, AssignOp, BLOCK_LIST, BLOCK_STATEMENTS, BTreeMap, BinaryOp, ControlFlow, Evaluator,
    FormatSpec, FullExecution, FullFunctionView, FullPayload, FullProgram, FullTag, FunctionHeader,
    LoweredCompTarget, LoweredFunctionKey, LoweredFunctionKind, LoweredReturnKind, LoweredType,
    LoweredTypeCheck, LoweredValue, Name, PathValue, RuntimeError, Span, StmtFlow, TraceKind,
    TracePayload, TracebackFrame, TracebackFrameKind, assign_lowered_bytes_view,
    assign_lowered_str_view, bind_lowered_comp_target, indexed_decode, indexed_error,
    indexed_finish, indexed_optional_raw, indexed_raw, indexed_string, indexed_value,
    lowered_assign_value, lowered_binary_value, lowered_bytes_parts,
    lowered_freeze_large_slot_list, lowered_match_no_arm,
    lowered_record_vec_append_or_replace_unsorted, lowered_record_vec_or_stats,
    lowered_result_err_value, lowered_result_ok, lowered_return_value, lowered_splice_arg_items,
    lowered_str_parts, lowered_value_from_runtime_any, lowered_value_satisfies_require,
    push_lowered_fmt_value, StreamValue,
};

enum FrameValue {
    Value(LoweredValue),
    Break(LoweredValue),
}

// Compound expressions and formatted strings must stay in the active heap-backed frame machine.
// Falling back to the recursive evaluator here would nest another explicit runner for each Result
// call in the expression and consume native stack even when the outer loop itself is iterative.
enum FrameRecordEntry {
    Field { name: Name, instruction: u32 },
    Spread(u32),
}

struct ListCompState {
    map: bool,
    key: Option<u32>,
    value: u32,
    target: LoweredCompTarget,
    condition: Option<u32>,
    items: Vec<LoweredValue>,
    index: usize,
    values: Vec<LoweredValue>,
    map_values: BTreeMap<String, LoweredValue>,
    span: Span,
}

struct FmtState {
    parts: Vec<FmtPart>,
    index: usize,
    text: String,
    path_span: Option<Span>,
}

#[derive(Clone)]
enum FmtPart {
    Text(Arc<str>),
    Expr(u32, Span, Option<FormatSpec>),
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
    /// A `yield` statement's value: the frame suspends here and hands the value
    /// to whoever pulled the producer.
    Yield,
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
        // The slot this call's result overwrites, when the call is the value of
        // a plain assignment to the same slot the receiver is read from. The
        // call may then take the value out of that slot instead of copying it.
        consume: Option<usize>,
        next: Box<FrameContinuation>,
    },
    MethodArg {
        name: Arc<str>,
        args: Vec<u32>,
        receiver: LoweredValue,
        consume: Option<usize>,
        index: usize,
        values: Vec<LoweredValue>,
        span: Span,
        next: Box<FrameContinuation>,
    },
    FmtValue {
        state: FmtState,
        span: Span,
        spec: Option<FormatSpec>,
        next: Box<FrameContinuation>,
    },
    ResultFallback {
        right: u32,
        span: Span,
        next: Box<FrameContinuation>,
    },
    ListItems {
        items: Vec<u32>,
        index: usize,
        values: Vec<LoweredValue>,
        next: Box<FrameContinuation>,
    },
    RecordItems {
        entries: Vec<FrameRecordEntry>,
        index: usize,
        fields: Vec<(Name, LoweredValue)>,
        span: Span,
        next: Box<FrameContinuation>,
    },
    ListCompIter {
        state: Box<ListCompState>,
        next: Box<FrameContinuation>,
    },
    ListCompCondition {
        state: Box<ListCompState>,
        next: Box<FrameContinuation>,
    },
    ListCompKey {
        state: Box<ListCompState>,
        next: Box<FrameContinuation>,
    },
    ListCompValue {
        state: Box<ListCompState>,
        key: Option<String>,
        next: Box<FrameContinuation>,
    },
}

enum FrameWork {
    Statements {
        statements: Vec<u32>,
        complete_call: bool,
        scope_id: Option<u64>,
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
    /// A loop over a script producer: each step pulls one item and re-arms
    /// itself, so the loop never holds the whole stream.
    ForStream {
        slot: usize,
        stream: StreamValue,
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

pub(super) struct CallFrame<'p> {
    pub(super) function: LoweredFunctionKey,
    pub(super) kind: LoweredFunctionKind,
    /// Whether this frame is a stream producer, whose body ends by falling off
    /// the end of its statements rather than by returning.
    pub(super) producer: bool,
    pub(super) scope_id: u64,
    pub(super) execution: FullExecution<'p>,
    pub(super) slots: Vec<LoweredValue>,
    pub(super) slot_scopes: Vec<u64>,
    pub(super) call_span: Span,
    pub(super) definition_span: Span,
    work: Vec<FrameWork>,
    pub(super) defers: Vec<u32>,
    pub(super) block_scopes: Vec<u64>,
    return_to: Option<FrameContinuation>,
}

pub(super) struct ExplicitFrames<'a, 'p> {
    evaluator: &'a mut Evaluator,
    program: &'p FullProgram,
    calls: Vec<CallFrame<'p>>,
    result: Option<Result<LoweredValue, RuntimeError>>,
    pending_error: Option<RuntimeError>,
    /// Set when a producer frame executes a `yield`: the value the puller
    /// receives, with the frame's remaining work left on its stack.
    suspended: Option<LoweredValue>,
}

impl Evaluator {
    pub(super) fn indexed_frames_supported(
        &self,
        view: FullFunctionView<'_>,
        span: Span,
    ) -> Result<bool, RuntimeError> {
        let header = view.header().map_err(|error| indexed_error(error, span))?;
        Ok(!matches!(
            header.return_kind,
            LoweredReturnKind::Plain(LoweredType::Stream)
        ))
    }

    pub(super) fn eval_indexed_with_frames(
        &mut self,
        program: &FullProgram,
        function: LoweredFunctionKey,
        kind: LoweredFunctionKind,
        values: &[LoweredValue],
        call_span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        let mut frames = ExplicitFrames::new(self, program);
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
        let mut frames = ExplicitFrames::new(self, program);
        frames.push_call_with_slots(function, kind, slots, call_span, None)?;
        frames.run()
    }
}

/// The slot a receiver instruction reads, when it reads exactly one.
fn indexed_slot_read(
    execution: &FullExecution<'_>,
    instruction: u32,
    span: Span,
) -> Result<Option<usize>, RuntimeError> {
    let (tag, mut payload) = indexed_value(execution.instruction_id(instruction), span)?;
    match tag {
        FullTag::ExprParam => Ok(Some(indexed_decode::<usize>(&mut payload, execution, span)?)),
        _ => Ok(None),
    }
}

/// Take a receiver out of the slot it was read from, when that slot still holds
/// the same container and nothing else has replaced it.
///
/// The receiver was evaluated before the arguments, and an argument may have
/// assigned to that slot in the meantime; the slot's current value is what the
/// statement stores, so the take only happens while the slot still holds the
/// very value the receiver came from.
fn take_consumed_receiver(
    slots: &mut [LoweredValue],
    consume: Option<usize>,
    receiver: LoweredValue,
) -> LoweredValue {
    let Some(slot) = consume else {
        return receiver;
    };
    let Some(current) = slots.get_mut(slot) else {
        return receiver;
    };
    if super::lowered_shares_backing(current, &receiver) {
        std::mem::replace(current, LoweredValue::Unit)
    } else {
        receiver
    }
}

/// What one pull of a producer did.
pub(super) enum ProducerStep {
    /// The body reached a `yield`; the value is the pulled item and the frame
    /// state is the continuation.
    Yielded {
        value: LoweredValue,
        state: ProducerFrameState,
    },
    /// The body ended, propagated an error, or returned a stream.
    Finished(Result<LoweredValue, RuntimeError>),
}

/// Pools of the vectors a frame allocates on every call.
///
/// Every call allocates its work stack, its slot-scope list, and the statement
/// list of the body it runs; every return frees them. A bounded pool turns
/// those allocations into reuse, the same way `lowered_slot_pool` does for
/// slots.
#[derive(Default)]
pub(in crate::runtime::eval) struct FrameScratch {
    work: Vec<Vec<FrameWork>>,
    slot_scopes: Vec<Vec<u64>>,
    statements: Vec<Vec<u32>>,
    /// How many statement lists were handed out fresh, and how many came back
    /// from the pool. A loop that reuses its body's list moves only the second:
    /// that is the claim `loop_iterations_reuse_their_statement_list` reads, and
    /// it is not visible from a run's values.
    pub(in crate::runtime::eval) fresh_statements: usize,
    pub(in crate::runtime::eval) reused_statements: usize,
}

impl FrameScratch {
    const POOL_CAP: usize = 32;

    fn take_work(&mut self) -> Vec<FrameWork> {
        self.work.pop().unwrap_or_default()
    }

    fn take_slot_scopes(&mut self, len: usize, fill: u64) -> Vec<u64> {
        let mut scopes = self.slot_scopes.pop().unwrap_or_default();
        scopes.clear();
        scopes.resize(len, fill);
        scopes
    }

    fn take_statements(&mut self) -> Vec<u32> {
        match self.statements.pop() {
            Some(statements) => {
                self.reused_statements += 1;
                statements
            }
            None => {
                self.fresh_statements += 1;
                Vec::new()
            }
        }
    }

    /// Returns a statement list whose entries have all run.
    ///
    /// A `Statements` work item owns its list, so the list is dropped when the
    /// item is exhausted — on every loop iteration, unless it comes back here.
    fn recycle_statements(&mut self, mut statements: Vec<u32>) {
        if self.statements.len() >= Self::POOL_CAP {
            return;
        }
        statements.clear();
        self.statements.push(statements);
    }

    /// Returns a finished frame's vectors to the pools, cleared for reuse.
    pub(super) fn recycle(&mut self, call: &mut CallFrame<'_>) {
        for work in call.work.drain(..) {
            let FrameWork::Statements {
                mut statements, ..
            } = work
            else {
                continue;
            };
            statements.clear();
            if self.statements.len() < Self::POOL_CAP {
                self.statements.push(statements);
            }
        }
        call.work.clear();
        if self.work.len() < Self::POOL_CAP {
            self.work.push(std::mem::take(&mut call.work));
        }
        call.slot_scopes.clear();
        if self.slot_scopes.len() < Self::POOL_CAP {
            self.slot_scopes.push(std::mem::take(&mut call.slot_scopes));
        }
    }
}

/// A suspended producer frame, without the borrow that ties it to a program.
///
/// The fields stay private to the frame engine: the producer module starts a
/// body's scope, discards a body's remaining work, and moves the state between
/// pulls, but never reaches into the machine's own bookkeeping.
pub(super) struct ProducerFrameState {
    work: Vec<FrameWork>,
    slots: Vec<LoweredValue>,
    slot_scopes: Vec<u64>,
    defers: Vec<u32>,
    block_scopes: Vec<u64>,
    scope_id: u64,
}

impl ProducerFrameState {
    /// The state of a producer whose body has not run yet.
    pub(super) fn begin_body(statements: Vec<u32>, slots: Vec<LoweredValue>) -> Self {
        Self {
            work: vec![FrameWork::Statements {
                statements,
                complete_call: true,
                scope_id: None,
            }],
            slots,
            slot_scopes: Vec::new(),
            defers: Vec::new(),
            block_scopes: Vec::new(),
            scope_id: 0,
        }
    }

    /// The scopes the body has open: its call scope, then its live blocks.
    ///
    /// A suspended producer's scopes are detached from the evaluator's stack
    /// while the consumer runs and reattached for each pull, which keeps the
    /// stack in the order the frame engine expects: a block may only be closed
    /// while it is innermost.
    pub(super) fn open_scopes(&self) -> Vec<u64> {
        let mut scopes = Vec::with_capacity(1 + self.block_scopes.len());
        scopes.push(self.scope_id);
        scopes.extend(self.block_scopes.iter().copied());
        scopes
    }

    /// Enters the body's call scope, so its slots belong to a live scope.
    pub(super) fn start(&mut self, scope_id: u64) {
        self.scope_id = scope_id;
        self.slot_scopes = vec![scope_id; self.slots.len()];
    }

}

impl<'p> CallFrame<'p> {
    /// Drops the body's remaining work, keeping its registered defers.
    pub(super) fn discard_body(&mut self) {
        self.work.clear();
        self.work.push(FrameWork::Statements {
            statements: Vec::new(),
            complete_call: true,
            scope_id: None,
        });
    }

    pub(super) fn into_state(self) -> ProducerFrameState {
        ProducerFrameState {
            work: self.work,
            slots: self.slots,
            slot_scopes: self.slot_scopes,
            defers: self.defers,
            block_scopes: self.block_scopes,
            scope_id: self.scope_id,
        }
    }
}

/// The frame state a producer resumes from.
impl<'p> CallFrame<'p> {
    /// Rebuilds a frame from the state a previous pull suspended.
    pub(super) fn from_state(
        program: &'p super::FullProgram,
        function: LoweredFunctionKey,
        kind: LoweredFunctionKind,
        call_span: Span,
        definition_span: Span,
        state: ProducerFrameState,
    ) -> Result<Option<Self>, super::IrVerifyError> {
        let Some(view) = program.function_view(function, kind)? else {
            return Ok(None);
        };
        let execution = view.execution()?;
        Ok(Some(CallFrame {
            function,
            kind,
            producer: true,
            scope_id: state.scope_id,
            execution,
            slots: state.slots,
            slot_scopes: state.slot_scopes,
            call_span,
            definition_span,
            work: state.work,
            defers: state.defers,
            block_scopes: state.block_scopes,
            return_to: None,
        }))
    }
}


impl<'a, 'p> ExplicitFrames<'a, 'p> {
    /// A machine over one program, with no frames of its own yet.
    pub(super) fn new(evaluator: &'a mut Evaluator, program: &'p FullProgram) -> Self {
        Self {
            evaluator,
            program,
            calls: Vec::new(),
            result: None,
            pending_error: None,
            suspended: None,
        }
    }

    fn run(&mut self) -> Result<LoweredValue, RuntimeError> {
        while self.result.is_none() {
            if self.suspended.take().is_some() {
                // Only a producer frame may suspend, and producers run through
                // `run_producer`; reaching this in an ordinary call means a
                // `yield` executed outside a producer.
                let span = self
                    .calls
                    .last()
                    .map(|call| call.call_span)
                    .unwrap_or_else(crate::runtime::eval::zero_span);
                return Err(RuntimeError::new(
                    "control-flow",
                    "yield outside stream producer",
                )
                .with_span(span));
            }
            let index = self
                .calls
                .len()
                .checked_sub(1)
                .expect("active indexed frame");
            let work = self.calls[index].work.pop().expect("indexed frame work");
            if let Err(error) = self.step(index, work)
                && self.pending_error.is_none()
            {
                self.begin_error_unwind(error);
            }
        }
        self.result.take().expect("indexed frame result")
    }

    /// Runs a producer frame until it yields or finishes.
    pub(super) fn run_producer(&mut self, call: CallFrame<'p>) -> ProducerStep {
        self.calls.push(call);
        while self.result.is_none() && self.suspended.is_none() {
            let index = self
                .calls
                .len()
                .checked_sub(1)
                .expect("active indexed frame");
            let work = self.calls[index].work.pop().expect("indexed frame work");
            if let Err(error) = self.step(index, work)
                && self.pending_error.is_none()
            {
                self.begin_error_unwind(error);
            }
        }
        if let Some(value) = self.suspended.take() {
            let frame = self.calls.pop().expect("suspended producer frame");
            return ProducerStep::Yielded {
                value,
                state: frame.into_state(),
            };
        }
        ProducerStep::Finished(self.result.take().expect("indexed frame result"))
    }

    fn begin_error_unwind(&mut self, error: RuntimeError) {
        if error.abort.as_ref().is_some_and(|signal| signal.force) {
            self.discard_calls();
            self.result = Some(Err(error));
            return;
        }
        self.pending_error = Some(error);
        let Some(index) = self.calls.len().checked_sub(1) else {
            self.result = Some(Err(self
                .pending_error
                .take()
                .expect("pending indexed frame error")));
            return;
        };
        // An error abandons the active lexical blocks before it enters this
        // function's defers. Their owned processes and NetJobs must observe
        // the same lexical cleanup boundary as they do on normal completion.
        let _ = self.discard_work_from(index, 0);
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
                .and_then(|view| {
                    view.header()
                        .map_err(|error| indexed_error(error, call.call_span))
                })
            {
                let _ = self.evaluator.write_back_lowered_captures(
                    &header,
                    &call.slots,
                    call.call_span,
                );
            }
            let _ = self.cleanup_call_scopes(&mut call);
            self.evaluator.recycle_lowered_slots(call.slots);
            self.evaluator.call_stack.pop();
            let exit_kind = match call.kind {
                LoweredFunctionKind::Pure => TraceKind::PureExit,
                LoweredFunctionKind::Proc => TraceKind::ProcExit,
            };
            let name = call.function.display_name();
            self.evaluator.trace_exit_with_definition(
                exit_kind,
                Some(call.call_span),
                Some(call.definition_span),
                Some(&name),
                TracePayload::None,
            );
        }
    }

    /// Resolves a fully evaluated call: a stream producer runs to completion
    /// and hands its stream back, and every other callee becomes a frame.
    ///
    /// The frame engine reaches this decision from its argument-walking
    /// continuation and, for a call with no arguments, directly; both go
    /// through here so a producer is never pushed as an ordinary frame, which
    /// would run its body with nowhere for `yield` to report and then fail the
    /// call as a function that did not return.
    fn push_resolved_call(
        &mut self,
        index: usize,
        function: LoweredFunctionKey,
        kind: LoweredFunctionKind,
        values: Vec<LoweredValue>,
        span: Span,
        next: FrameContinuation,
    ) -> Result<(), RuntimeError> {
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
            self.push_value(index, FrameValue::Value(value), next);
            return Ok(());
        }
        self.push_call(function, kind, values, span, Some(next))
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
        let header = view
            .header()
            .map_err(|error| indexed_error(error, call_span))?;
        let slots = self
            .evaluator
            .bind_lowered_values(&header, &values, call_span)?;
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
        let header = view
            .header()
            .map_err(|error| indexed_error(error, call_span))?;
        self.push_call_with_header(function, kind, view, header, slots, call_span, return_to)
    }

    fn push_call_with_header(
        &mut self,
        function: LoweredFunctionKey,
        kind: LoweredFunctionKind,
        view: FullFunctionView<'p>,
        header: Arc<FunctionHeader>,
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
        let mut statements = self.evaluator.frame_scratch.take_statements();
        decode_statements_into(body, call_span, &mut statements)?;
        let (frame_kind, enter_kind) = match kind {
            LoweredFunctionKind::Pure => (TracebackFrameKind::Pure, TraceKind::PureEnter),
            LoweredFunctionKind::Proc => (TracebackFrameKind::Proc, TraceKind::ProcEnter),
        };
        let definition_span = view
            .definition_span()
            .map_err(|error| indexed_error(error, call_span))?;
        // Tracing is off unless a tool turned it on, and rendering a function's
        // display name allocates; only do it when an event will use it.
        if self.evaluator.trace_enabled {
            let name = function.display_name();
            self.evaluator.trace_enter_with_definition(
                enter_kind,
                Some(call_span),
                Some(definition_span),
                Some(&name),
                TracePayload::None,
            );
        }
        self.evaluator.call_stack.push(TracebackFrame {
            kind: frame_kind,
            name: function.traceback_name(),
            definition_span: Some(definition_span),
            call_span: Some(call_span),
        });
        let scope_id = self.evaluator.enter_owned_host_scope();
        let slot_scopes = self
            .evaluator
            .frame_scratch
            .take_slot_scopes(slots.len(), scope_id);
        self.calls.push(CallFrame {
            function,
            kind,
            producer: false,
            scope_id,
            execution,
            slots,
            slot_scopes,
            call_span,
            definition_span,
            work: {
                let mut work = self.evaluator.frame_scratch.take_work();
                work.push(FrameWork::Statements {
                    statements,
                    complete_call: true,
                    scope_id: None,
                });
                work
            },
            defers: Vec::new(),
            block_scopes: Vec::new(),
            return_to,
        });
        Ok(())
    }

    fn step(&mut self, index: usize, work: FrameWork) -> Result<(), RuntimeError> {
        match work {
            FrameWork::Statements {
                mut statements,
                complete_call,
                scope_id,
            } => {
                let Some(statement) = statements.pop() else {
                    // A list that ran to its end goes back to the pool here:
                    // its entries are done with, and a loop body would
                    // otherwise allocate a fresh list on every iteration.
                    self.evaluator.frame_scratch.recycle_statements(statements);
                    return if complete_call {
                        self.complete_call(index, StmtFlow::None)
                    } else {
                        if let Some(scope_id) = scope_id {
                            self.exit_block_scope(index, scope_id)?;
                        }
                        Ok(())
                    };
                };
                self.calls[index].work.push(FrameWork::Statements {
                    statements,
                    complete_call,
                    scope_id,
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
            FrameWork::ForStream {
                slot,
                stream,
                body,
                span,
            } => self.step_for_stream(index, slot, stream, body, span),
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
                    ControlFlow::Continue(value) => {
                        self.calls[index].slots[slot] = LoweredValue::Int(value)
                    }
                    ControlFlow::Break(value) => {
                        return self.complete_call(index, StmtFlow::Return(value));
                    }
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
                    ControlFlow::Continue(value) => {
                        self.calls[index].slots[slot] = LoweredValue::Bool(value)
                    }
                    ControlFlow::Break(value) => {
                        return self.complete_call(index, StmtFlow::Return(value));
                    }
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
                self.push_expr(
                    index,
                    value,
                    value_span,
                    FrameContinuation::Discard(value_span),
                );
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
            FullTag::StmtYield => {
                let value = indexed_raw(&mut payload, span)?;
                indexed_finish(payload, span)?;
                if !self.calls[index].producer {
                    return Err(
                        RuntimeError::new("control-flow", "yield outside stream producer")
                            .with_span(span),
                    );
                }
                self.push_expr(index, value, span, FrameContinuation::Yield);
                Ok(())
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
                    StmtFlow::Return(value) => self.complete_call(index, StmtFlow::Return(value)),
                    StmtFlow::Propagate(value) => {
                        self.complete_call(index, StmtFlow::Propagate(value))
                    }
                    StmtFlow::Break(_) => {
                        Err(RuntimeError::new("control-flow", "break outside loop").with_span(span))
                    }
                    StmtFlow::Continue => {
                        Err(RuntimeError::new("control-flow", "continue outside loop")
                            .with_span(span))
                    }
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
                self.push_value(
                    index,
                    FrameValue::Value(self.calls[index].slots[slot].clone()),
                    next,
                );
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
                    values.push((
                        indexed_raw(&mut branches, span)?,
                        indexed_raw(&mut branches, span)?,
                    ));
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
            FullTag::ExprList => {
                let (_, mut values) = self.calls[index]
                    .execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, span))?;
                let len = indexed_raw(&mut values, span)? as usize;
                let mut items = Vec::with_capacity(len);
                for _ in 0..len {
                    items.push(indexed_raw(&mut values, span)?);
                }
                indexed_finish(values, span)?;
                indexed_finish(payload, span)?;
                if let Some(&instruction) = items.first() {
                    self.push_expr(
                        index,
                        instruction,
                        span,
                        FrameContinuation::ListItems {
                            items,
                            index: 0,
                            values: Vec::with_capacity(len),
                            next: Box::new(next),
                        },
                    );
                } else {
                    self.push_value(
                        index,
                        FrameValue::Value(LoweredValue::List(Vec::new())),
                        next,
                    );
                }
            }
            FullTag::ExprRecord => {
                let (_, mut entries) = self.calls[index]
                    .execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, span))?;
                let len = indexed_raw(&mut entries, span)? as usize;
                let mut decoded_entries = Vec::with_capacity(len);
                for _ in 0..len {
                    match indexed_raw(&mut entries, span)? {
                        0 => decoded_entries.push(FrameRecordEntry::Field {
                            name: indexed_decode(&mut entries, &self.calls[index].execution, span)?,
                            instruction: indexed_raw(&mut entries, span)?,
                        }),
                        1 => decoded_entries
                            .push(FrameRecordEntry::Spread(indexed_raw(&mut entries, span)?)),
                        _ => {
                            return Err(RuntimeError::new(
                                "indexed-ir",
                                "invalid indexed record entry",
                            )
                            .with_span(span));
                        }
                    }
                }
                indexed_finish(entries, span)?;
                indexed_finish(payload, span)?;
                if let Some(entry) = decoded_entries.first() {
                    let instruction = match entry {
                        FrameRecordEntry::Field { instruction, .. }
                        | FrameRecordEntry::Spread(instruction) => *instruction,
                    };
                    self.push_expr(
                        index,
                        instruction,
                        span,
                        FrameContinuation::RecordItems {
                            entries: decoded_entries,
                            index: 0,
                            fields: Vec::new(),
                            span,
                            next: Box::new(next),
                        },
                    );
                } else {
                    self.push_value(
                        index,
                        FrameValue::Value(lowered_record_vec_or_stats(Vec::new())),
                        next,
                    );
                }
            }
            FullTag::ExprListComp | FullTag::ExprMapComp => {
                let map = tag == FullTag::ExprMapComp;
                let key = map.then(|| indexed_raw(&mut payload, span)).transpose()?;
                let value = indexed_raw(&mut payload, span)?;
                let target = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                let iter = indexed_raw(&mut payload, span)?;
                let condition = indexed_optional_raw(&mut payload, span)?;
                let value_span = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                indexed_finish(payload, span)?;
                let state = ListCompState {
                    map,
                    key,
                    value,
                    target,
                    condition,
                    items: Vec::new(),
                    index: 0,
                    values: Vec::new(),
                    map_values: BTreeMap::new(),
                    span: value_span,
                };
                self.push_expr(
                    index,
                    iter,
                    value_span,
                    FrameContinuation::ListCompIter {
                        state: Box::new(state),
                        next: Box::new(next),
                    },
                );
            }
            FullTag::ExprFmtString | FullTag::ExprPathFmtString => {
                let path = tag == FullTag::ExprPathFmtString;
                let (_, mut encoded_parts) = self.calls[index]
                    .execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, span))?;
                let len = indexed_raw(&mut encoded_parts, span)? as usize;
                let mut parts = Vec::with_capacity(len);
                for _ in 0..len {
                    match indexed_raw(&mut encoded_parts, span)? {
                        0 => parts.push(FmtPart::Text(indexed_decode(
                            &mut encoded_parts,
                            &self.calls[index].execution,
                            span,
                        )?)),
                        1 => parts.push(FmtPart::Expr(
                            indexed_raw(&mut encoded_parts, span)?,
                            indexed_decode(&mut encoded_parts, &self.calls[index].execution, span)?,
                            indexed_decode(&mut encoded_parts, &self.calls[index].execution, span)?,
                        )),
                        _ => {
                            return Err(RuntimeError::new(
                                "indexed-ir",
                                "invalid indexed format part",
                            )
                            .with_span(span));
                        }
                    }
                }
                indexed_finish(encoded_parts, span)?;
                let path_span = if path {
                    Some(indexed_decode(
                        &mut payload,
                        &self.calls[index].execution,
                        span,
                    )?)
                } else {
                    None
                };
                indexed_finish(payload, span)?;
                self.step_fmt(
                    index,
                    FmtState {
                        parts,
                        index: 0,
                        text: String::new(),
                        path_span,
                    },
                    next,
                )?;
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
                self.push_expr(
                    index,
                    value,
                    span,
                    FrameContinuation::WrapOk(Box::new(next)),
                );
            }
            FullTag::ExprErr => {
                let value = indexed_raw(&mut payload, span)?;
                indexed_finish(payload, span)?;
                self.push_expr(
                    index,
                    value,
                    span,
                    FrameContinuation::WrapErr(Box::new(next)),
                );
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
                // `x = x.set(..)` and its shape: the call's result overwrites
                // the very slot the receiver reads, so the receiver can be
                // taken out of the slot rather than copied. Anything else — a
                // nested call, a different destination — keeps the plain copy.
                let consume = match &next {
                    FrameContinuation::Assign { slot, op, .. }
                        if *op == AssignOp::Set
                            && indexed_slot_read(
                                &self.calls[index].execution,
                                receiver,
                                value_span,
                            )? == Some(*slot) =>
                    {
                        Some(*slot)
                    }
                    _ => None,
                };
                self.push_expr(
                    index,
                    receiver,
                    value_span,
                    FrameContinuation::MethodReceiver {
                        name: Arc::from(name),
                        args: decoded_args,
                        span: value_span,
                        consume,
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
                    // A zero-argument call has no argument list to walk, so it
                    // reaches the call decision here instead of through
                    // `FrameContinuation::CallArguments`. Both paths resolve
                    // the callee the same way.
                    self.push_resolved_call(index, function, kind, Vec::new(), value_span, next)?;
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
                FrameValue::Value(value) => {
                    self.calls[index].slots[slot] = value;
                    self.calls[index].slot_scopes[slot] = self.evaluator.current_scope_id();
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Return(value));
                }
            },
            FrameContinuation::Assign { slot, op, span } => match value {
                FrameValue::Value(value) => {
                    let current = self.calls[index].slots[slot].clone();
                    let value = lowered_assign_value(op, current, value, span)?;
                    let owner_scope = self.calls[index].slot_scopes[slot];
                    let source_scope = self.evaluator.current_scope_id();
                    self.evaluator
                        .transfer_owned_host_resources_in_lowered_value(
                            &value,
                            source_scope,
                            owner_scope,
                        );
                    self.calls[index].slots[slot] = value;
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Return(value));
                }
            },
            FrameContinuation::Return => {
                let value = match value {
                    FrameValue::Value(value) | FrameValue::Break(value) => value,
                };
                return self.complete_call(index, StmtFlow::Return(value));
            }
            FrameContinuation::Discard(span) => match value {
                FrameValue::Value(value @ LoweredValue::ResultErr(_)) => {
                    let value = self
                        .evaluator
                        .lowered_question_propagation_value(value, span)?;
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
                FrameValue::Value(_) => {}
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::BinaryLeft {
                op,
                right,
                span,
                next,
            } => match value {
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
                    FrameContinuation::BinaryRight {
                        op,
                        left,
                        span,
                        next,
                    },
                ),
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Return(value));
                }
            },
            FrameContinuation::BinaryRight {
                op,
                left,
                span,
                next,
            } => match value {
                FrameValue::Value(right) => self.push_value(
                    index,
                    FrameValue::Value(lowered_binary_value(op, left, right, span)?),
                    *next,
                ),
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Return(value));
                }
            },
            FrameContinuation::BoolBinaryRight { next, span } => match value {
                FrameValue::Value(value) => self.push_value(
                    index,
                    FrameValue::Value(LoweredValue::Bool(frame_condition_bool(value, span)?)),
                    *next,
                ),
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Return(value));
                }
            },
            FrameContinuation::If {
                branches,
                index: branch,
                else_value,
                span,
                next,
            } => match value {
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
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Return(value));
                }
            },
            FrameContinuation::StatementIf {
                branches,
                index: branch,
                else_body,
                span,
            } => match value {
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
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Return(value));
                }
            },
            FrameContinuation::ForItems { slot, body, span } => match value {
                FrameValue::Value(value) => {
                    let script_stream = match &value {
                        LoweredValue::Stream(stream) => stream.script().is_some(),
                        _ => false,
                    };
                    if script_stream {
                        let LoweredValue::Stream(stream) = value else {
                            unreachable!("checked above")
                        };
                        self.calls[index].work.push(FrameWork::ForStream {
                            slot,
                            stream: *stream,
                            body,
                            span,
                        });
                        return Ok(());
                    }
                    let items = self.evaluator.lowered_list_items(
                        value,
                        span,
                        "lowered for expected List",
                    )?;
                    self.calls[index].work.push(FrameWork::ForItems {
                        slot,
                        items,
                        index: 0,
                        body,
                        span,
                    });
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Return(value));
                }
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
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Return(value));
                }
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
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Return(value));
                }
            },
            FrameContinuation::MatchValue { arms, span } => match value {
                FrameValue::Value(value) => self.select_match_arm(index, arms, 0, value, span)?,
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Return(value));
                }
            },
            FrameContinuation::MatchGuard {
                arms,
                index: arm_index,
                value: match_value,
                span,
            } => match value {
                FrameValue::Value(value) => {
                    if frame_condition_bool(value, span)? {
                        self.push_statement_block(index, arms[arm_index].2, span)?;
                    } else {
                        self.select_match_arm(index, arms, arm_index + 1, match_value, span)?;
                    }
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Return(value));
                }
            },
            FrameContinuation::MatchExprValue {
                arms, next, span, ..
            } => match value {
                FrameValue::Value(value) => {
                    self.select_expr_match_arm(index, arms, 0, value, span, *next)?;
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Return(value));
                }
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
                        self.select_expr_match_arm(
                            index,
                            arms,
                            arm_index + 1,
                            match_value,
                            span,
                            *next,
                        )?;
                    }
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Return(value));
                }
            },
            FrameContinuation::BreakLoop => match value {
                FrameValue::Value(_) => return self.break_loop(index),
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Return(value));
                }
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
                        _ => {
                            return Err(RuntimeError::new(
                                "indexed-ir",
                                "invalid call argument kind",
                            )
                            .with_span(span));
                        }
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
                        self.push_resolved_call(index, function, kind, values, span, *next)?;
                    }
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Return(value));
                }
            },
            FrameContinuation::WrapOk(next) => match value {
                FrameValue::Value(value) => self.push_value(
                    index,
                    FrameValue::Value(LoweredValue::ResultOk(Box::new(value))),
                    *next,
                ),
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Return(value));
                }
            },
            FrameContinuation::WrapErr(next) => match value {
                FrameValue::Value(value) => self.push_value(
                    index,
                    FrameValue::Value(LoweredValue::ResultErr(Box::new(value.into_value()))),
                    *next,
                ),
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Return(value));
                }
            },
            FrameContinuation::Try { span, next } => match value {
                FrameValue::Value(LoweredValue::ResultOk(value)) => {
                    self.push_value(index, FrameValue::Value(*value), *next)
                }
                FrameValue::Value(LoweredValue::ResultErr(error)) => {
                    let value = self
                        .evaluator
                        .lowered_question_propagation_value(LoweredValue::ResultErr(error), span)?;
                    self.push_value(index, FrameValue::Break(value), *next);
                }
                FrameValue::Value(_) => {
                    return Err(
                        RuntimeError::new("type-error", "lowered `?` expected Result")
                            .with_span(span),
                    );
                }
                FrameValue::Break(value) => self.push_value(index, FrameValue::Break(value), *next),
            },
            FrameContinuation::Require { check, span, next } => match value {
                FrameValue::Value(value) => {
                    let value =
                        if lowered_value_satisfies_require(self.evaluator, &value, &check.ty) {
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
                consume,
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
                                consume,
                                index: 0,
                                values: Vec::new(),
                                span,
                                next,
                            },
                        );
                    } else {
                        let receiver =
                            take_consumed_receiver(&mut self.calls[index].slots, consume, receiver);
                        self.push_method_result(index, receiver, name, Vec::new(), span, *next)?;
                    }
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Return(value));
                }
            },
            FrameContinuation::MethodArg {
                name,
                args,
                receiver,
                consume,
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
                                consume,
                                index: next_index,
                                values,
                                span,
                                next,
                            },
                        );
                    } else {
                        let receiver =
                            take_consumed_receiver(&mut self.calls[index].slots, consume, receiver);
                        self.push_method_result(index, receiver, name, values, span, *next)?;
                    }
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Return(value));
                }
            },
            FrameContinuation::FmtValue {
                mut state,
                span,
                spec,
                next,
            } => match value {
                FrameValue::Value(value) => {
                    push_lowered_fmt_value(&mut state.text, &value, span, spec.as_ref())?;
                    self.step_fmt(index, state, *next)?;
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Return(value));
                }
            },
            FrameContinuation::ResultFallback { right, span, next } => match value {
                FrameValue::Value(LoweredValue::ResultOk(value)) => {
                    self.push_value(index, FrameValue::Value(*value), *next)
                }
                FrameValue::Value(LoweredValue::ResultErr(_) | LoweredValue::Null) => {
                    self.push_expr(index, right, span, *next)
                }
                FrameValue::Value(value) => self.push_value(index, FrameValue::Value(value), *next),
                FrameValue::Break(value) => self.push_value(index, FrameValue::Break(value), *next),
            },
            FrameContinuation::ListItems {
                items,
                index: item_index,
                mut values,
                next,
            } => match value {
                FrameValue::Value(value) => {
                    values.push(value);
                    if let Some(&instruction) = items.get(item_index + 1) {
                        let span = self.calls[index].call_span;
                        self.push_expr(
                            index,
                            instruction,
                            span,
                            FrameContinuation::ListItems {
                                items,
                                index: item_index + 1,
                                values,
                                next,
                            },
                        );
                    } else {
                        self.push_value(
                            index,
                            FrameValue::Value(LoweredValue::List(values)),
                            *next,
                        );
                    }
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Return(value));
                }
            },
            FrameContinuation::RecordItems {
                entries,
                index: entry_index,
                mut fields,
                span,
                next,
            } => match value {
                FrameValue::Value(value) => {
                    append_record_entry(&mut fields, &entries[entry_index], value, span)?;
                    if let Some(entry) = entries.get(entry_index + 1) {
                        let instruction = match entry {
                            FrameRecordEntry::Field { instruction, .. }
                            | FrameRecordEntry::Spread(instruction) => *instruction,
                        };
                        self.push_expr(
                            index,
                            instruction,
                            span,
                            FrameContinuation::RecordItems {
                                entries,
                                index: entry_index + 1,
                                fields,
                                span,
                                next,
                            },
                        );
                    } else {
                        fields.sort_unstable_by_key(|(name, _)| *name);
                        self.push_value(
                            index,
                            FrameValue::Value(lowered_record_vec_or_stats(fields)),
                            *next,
                        );
                    }
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Return(value));
                }
            },
            FrameContinuation::ListCompIter { mut state, next } => match value {
                FrameValue::Value(value) => {
                    state.items = self.evaluator.lowered_list_items(
                        value,
                        state.span,
                        if state.map {
                            "map comprehension expected List"
                        } else {
                            "list comprehension expected List"
                        },
                    )?;
                    self.step_list_comp(index, *state, *next)?;
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Return(value));
                }
            },
            FrameContinuation::ListCompCondition { state, next } => match value {
                FrameValue::Value(value) => {
                    if frame_condition_bool(value, state.span)? {
                        self.push_list_comp_projection(index, *state, *next)?;
                    } else {
                        let mut state = *state;
                        state.index += 1;
                        self.step_list_comp(index, state, *next)?;
                    }
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Return(value));
                }
            },
            FrameContinuation::ListCompKey { state, next } => match value {
                FrameValue::Value(LoweredValue::Str(key)) => {
                    self.push_expr(
                        index,
                        state.value,
                        state.span,
                        FrameContinuation::ListCompValue {
                            state,
                            key: Some(key.to_string()),
                            next,
                        },
                    );
                }
                FrameValue::Value(value) => {
                    return Err(RuntimeError::new(
                        "type-error",
                        format!(
                            "map comprehension key expected Str, found {}",
                            value.type_name()
                        ),
                    )
                    .with_span(state.span));
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Return(value));
                }
            },
            FrameContinuation::Yield => match value {
                FrameValue::Value(value) => {
                    // The frame keeps everything after this statement on its
                    // work stack; the puller receives the value.
                    self.suspended = Some(value);
                    return Ok(());
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Return(value));
                }
            },
            FrameContinuation::ListCompValue {
                mut state,
                key,
                next,
            } => match value {
                FrameValue::Value(value) => {
                    if state.map {
                        state
                            .map_values
                            .insert(key.expect("map comprehension key was evaluated"), value);
                    } else {
                        state.values.push(value);
                    }
                    state.index += 1;
                    self.step_list_comp(index, *state, *next)?;
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Return(value));
                }
            },
        }
        Ok(())
    }

    fn complete_call(&mut self, index: usize, flow: StmtFlow) -> Result<(), RuntimeError> {
        if let StmtFlow::Return(value) | StmtFlow::Break(Some(value)) = &flow {
            let function_scope = self.calls[index].scope_id;
            let current_scope = self.evaluator.current_scope_id();
            if current_scope != function_scope {
                self.evaluator
                    .transfer_owned_host_resources_in_lowered_value(
                        value,
                        current_scope,
                        function_scope,
                    );
            }
        }
        // A return may leave nested statement blocks. Transfer an escaping
        // resource above, then close those blocks before running this
        // function's defers.
        self.discard_work_from(index, 0)?;
        if self.calls[index].defers.is_empty() {
            self.finish_call(index, flow)
        } else {
            self.calls[index].work.push(FrameWork::Finish(flow));
            Ok(())
        }
    }

    fn cleanup_call_scopes(&mut self, call: &mut CallFrame<'p>) -> Result<(), RuntimeError> {
        let mut first_error = None;
        while let Some(scope_id) = call.block_scopes.pop() {
            if let Err(error) = self.evaluator.exit_owned_host_scope(scope_id)
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        if let Err(error) = self.evaluator.exit_owned_host_scope(call.scope_id)
            && first_error.is_none()
        {
            first_error = Some(error);
        }
        first_error.map_or(Ok(()), Err)
    }

    fn step_list_comp(
        &mut self,
        index: usize,
        state: ListCompState,
        next: FrameContinuation,
    ) -> Result<(), RuntimeError> {
        if state.index < state.items.len() {
            let item = state.items[state.index].clone();
            bind_lowered_comp_target(
                &state.target,
                item,
                &mut self.calls[index].slots,
                state.span,
            )?;
            if let Some(condition) = state.condition {
                self.push_expr(
                    index,
                    condition,
                    state.span,
                    FrameContinuation::ListCompCondition {
                        state: Box::new(state),
                        next: Box::new(next),
                    },
                );
                return Ok(());
            }
            return self.push_list_comp_projection(index, state, next);
        }
        let value = if state.map {
            LoweredValue::Map(Arc::new(state.map_values))
        } else {
            LoweredValue::List(state.values)
        };
        self.push_value(index, FrameValue::Value(value), next);
        Ok(())
    }

    fn push_list_comp_projection(
        &mut self,
        index: usize,
        state: ListCompState,
        next: FrameContinuation,
    ) -> Result<(), RuntimeError> {
        if state.map {
            self.push_expr(
                index,
                state.key.expect("map comprehension key"),
                state.span,
                FrameContinuation::ListCompKey {
                    state: Box::new(state),
                    next: Box::new(next),
                },
            );
        } else {
            self.push_expr(
                index,
                state.value,
                state.span,
                FrameContinuation::ListCompValue {
                    state: Box::new(state),
                    key: None,
                    next: Box::new(next),
                },
            );
        }
        Ok(())
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
            let result =
                self.evaluator
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

    fn step_fmt(
        &mut self,
        index: usize,
        mut state: FmtState,
        next: FrameContinuation,
    ) -> Result<(), RuntimeError> {
        loop {
            let Some(part) = state.parts.get(state.index).cloned() else {
                let value = if let Some(span) = state.path_span {
                    LoweredValue::Path(
                        PathValue::from_text(state.text).map_err(|error| error.with_span(span))?,
                    )
                } else {
                    LoweredValue::Str(state.text.into())
                };
                self.push_value(index, FrameValue::Value(value), next);
                return Ok(());
            };
            state.index += 1;
            match part {
                FmtPart::Text(text) => state.text.push_str(&text),
                FmtPart::Expr(instruction, span, spec) => {
                    self.push_expr(
                        index,
                        instruction,
                        span,
                        FrameContinuation::FmtValue {
                            state,
                            span,
                            spec,
                            next: Box::new(next),
                        },
                    );
                    return Ok(());
                }
            }
        }
    }

    fn finish_deferred_call(&mut self, index: usize, flow: StmtFlow) -> Result<(), RuntimeError> {
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
            .and_then(|view| {
                view.header()
                    .map_err(|error| indexed_error(error, call.call_span))
            })
        {
            let _ =
                self.evaluator
                    .write_back_lowered_captures(&header, &call.slots, call.call_span);
        }
        // An active error remains primary; cleanup failure is intentionally
        // secondary, but the scope still must release its owned resources.
        let _ = self.cleanup_call_scopes(&mut call);
        self.evaluator.recycle_lowered_slots(call.slots);
        self.evaluator.call_stack.pop();
        let exit_kind = match call.kind {
            LoweredFunctionKind::Pure => TraceKind::PureExit,
            LoweredFunctionKind::Proc => TraceKind::ProcExit,
        };
        if self.evaluator.trace_enabled {
            let name = call.function.display_name();
            self.evaluator.trace_exit_with_definition(
                exit_kind,
                Some(call.call_span),
                Some(call.definition_span),
                Some(&name),
                TracePayload::None,
            );
        }
        if let Some(parent) = self.calls.len().checked_sub(1) {
            let _ = self.discard_work_from(parent, 0);
            self.calls[parent].work.push(FrameWork::FinishError);
        } else {
            self.result = Some(Err(self
                .pending_error
                .take()
                .expect("pending indexed frame error")));
        }
        Ok(())
    }

    fn finish_call(&mut self, index: usize, flow: StmtFlow) -> Result<(), RuntimeError> {
        debug_assert_eq!(index, self.calls.len() - 1);
        let mut call = self.calls.pop().expect("active indexed frame");
        // The frame's own vectors are done with; the returned value has already
        // been taken out of `slots`.
        self.evaluator.frame_scratch.recycle(&mut call);
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
                lowered_return_value(header.return_kind, value, call.call_span)
            }
            // A producer ends by running out of statements; that is the end of
            // the stream, not a function that failed to return.
            StmtFlow::None if call.producer => Ok(LoweredValue::Unit),
            StmtFlow::None => Err(
                RuntimeError::new("return", "lowered function did not return")
                    .with_span(call.call_span),
            ),
            StmtFlow::Break(_) => {
                Err(RuntimeError::new("control-flow", "break outside loop")
                    .with_span(call.call_span))
            }
            StmtFlow::Continue => Err(RuntimeError::new("control-flow", "continue outside loop")
                .with_span(call.call_span)),
        };
        let write_back =
            self.evaluator
                .write_back_lowered_captures(&header, &call.slots, call.call_span);
        if let Ok(value) = &value {
            let parent_scope = self.evaluator.parent_owned_host_scope();
            self.evaluator.transfer_owned_host_resources_in_value(
                &value.clone().into_value(),
                call.scope_id,
                parent_scope,
            );
        }
        let cleanup = self.cleanup_call_scopes(&mut call);
        self.evaluator.recycle_lowered_slots(call.slots);
        let exit_kind = match call.kind {
            LoweredFunctionKind::Pure => TraceKind::PureExit,
            LoweredFunctionKind::Proc => TraceKind::ProcExit,
        };
        self.evaluator.call_stack.pop();
        if self.evaluator.trace_enabled {
            let name = call.function.display_name();
            self.evaluator.trace_exit(
                exit_kind,
                Some(call.call_span),
                Some(&name),
                TracePayload::None,
            );
        }
        let value = value.and_then(|value| {
            write_back?;
            cleanup?;
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

    fn call_header(&self, index: usize) -> Result<Arc<FunctionHeader>, RuntimeError> {
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
            return Err(
                RuntimeError::new("type-error", "lowered for lines expected Str or Bytes")
                    .with_span(span),
            );
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
        assign_lowered_str_view(
            &mut self.calls[index].slots[slot],
            &text_value,
            cursor,
            view_end,
        );
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
                ControlFlow::Break(value) => self.complete_call(index, StmtFlow::Return(value)),
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
                    | FrameWork::ForStream { .. }
                    | FrameWork::ForStrLines { .. }
                    | FrameWork::While { .. }
            )
        }) else {
            return Err(RuntimeError::new("control-flow", "break outside loop")
                .with_span(self.calls[index].call_span));
        };
        self.discard_work_from(index, loop_index)
    }

    fn step_for_stream(
        &mut self,
        index: usize,
        slot: usize,
        mut stream: StreamValue,
        body: u32,
        span: Span,
    ) -> Result<(), RuntimeError> {
        self.evaluator.service_pending_signal(span)?;
        if self.evaluator.shutting_down() {
            self.evaluator.stream_cancel(&mut stream, span)?;
            return Ok(());
        }
        let Some(value) = self.evaluator.stream_next(&mut stream, span)? else {
            return Ok(());
        };
        let Some(item) = lowered_value_from_runtime_any(&value) else {
            return Err(RuntimeError::new(
                "type-error",
                format!("stream produced unsupported {}", value.type_name()),
            )
            .with_span(span));
        };
        self.calls[index].slots[slot] = item;
        // Re-arm before running the body, so a `continue` reaches the next item
        // and a `break` discards this item and stops the producer.
        self.calls[index].work.push(FrameWork::ForStream {
            slot,
            stream,
            body,
            span,
        });
        self.push_statement_block(index, body, span)?;
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
        self.discard_work_from(index, loop_index + 1)
    }

    fn function_kind(
        &self,
        function: LoweredFunctionKey,
        span: Span,
    ) -> Result<LoweredFunctionKind, RuntimeError> {
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
            Err(
                RuntimeError::new("unresolved-lowered-call", function.display_name())
                    .with_span(span),
            )
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
        self.calls[index]
            .work
            .push(FrameWork::Value { value, next });
    }

    fn push_statement_block(
        &mut self,
        index: usize,
        body: u32,
        span: Span,
    ) -> Result<(), RuntimeError> {
        let mut statements = self.evaluator.frame_scratch.take_statements();
        decode_statement_block_into(&self.calls[index].execution, body, span, &mut statements)?;
        let scope_id = self.evaluator.enter_owned_host_scope();
        self.calls[index].block_scopes.push(scope_id);
        self.calls[index].work.push(FrameWork::Statements {
            statements,
            complete_call: false,
            scope_id: Some(scope_id),
        });
        Ok(())
    }

    fn exit_block_scope(&mut self, index: usize, scope_id: u64) -> Result<(), RuntimeError> {
        let popped = self.calls[index].block_scopes.pop();
        debug_assert_eq!(popped, Some(scope_id));
        self.evaluator.exit_owned_host_scope(scope_id)
    }

    /// Drop work that cannot execute (return, error, break, or continue) and
    /// close each lexical statement scope it carried from innermost to outer.
    fn discard_work_from(&mut self, index: usize, keep: usize) -> Result<(), RuntimeError> {
        let discarded = self.calls[index].work.split_off(keep);
        for work in discarded.into_iter().rev() {
            match work {
                FrameWork::Statements {
                    scope_id: Some(scope_id),
                    ..
                } => self.exit_block_scope(index, scope_id)?,
                // A loop that is being discarded holds a producer nothing will
                // pull again: stopping it runs its defers.
                FrameWork::ForStream {
                    mut stream, span, ..
                } => self.evaluator.stream_cancel(&mut stream, span)?,
                _ => {}
            }
        }
        Ok(())
    }
}

fn frame_condition_bool(value: LoweredValue, span: Span) -> Result<bool, RuntimeError> {
    match value {
        LoweredValue::Bool(value) => Ok(value),
        LoweredValue::Status(status) => Ok(status.success),
        _ => {
            Err(RuntimeError::new("type-error", "lowered expression expected Bool").with_span(span))
        }
    }
}

fn append_record_entry(
    fields: &mut Vec<(Name, LoweredValue)>,
    entry: &FrameRecordEntry,
    value: LoweredValue,
    span: Span,
) -> Result<(), RuntimeError> {
    match entry {
        FrameRecordEntry::Field { name, .. } => {
            lowered_record_vec_append_or_replace_unsorted(fields, *name, value);
        }
        FrameRecordEntry::Spread(_) => match value {
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
                for (key, value) in
                    super::lowered_inline_stats_to_record_vec(blanks, code, comments)
                {
                    lowered_record_vec_append_or_replace_unsorted(fields, key, value.clone());
                }
            }
            LoweredValue::StatsBlob(stats) => {
                for (key, value) in stats.to_record_vec() {
                    lowered_record_vec_append_or_replace_unsorted(fields, key, value.clone());
                }
            }
            value => {
                return Err(RuntimeError::new(
                    "type-error",
                    format!("record spread expected Record, found {}", value.type_name()),
                )
                .with_span(span));
            }
        },
    }
    Ok(())
}

/// The instructions of a statement block, reversed so a frame can pop them.
///
/// The payload spells the block's statements in the order they run, and a frame
/// pops from the end of this list, so the list is reversed here: the last
/// element is the block's first statement.
pub(super) fn decode_statements(
    payload: FullPayload<'_>,
    span: Span,
) -> Result<Vec<u32>, RuntimeError> {
    let mut statements = Vec::new();
    decode_statements_into(payload, span, &mut statements)?;
    Ok(statements)
}

/// Fills `statements` with a block's instructions, reversed so a frame pops
/// them in the order they run.
///
/// Taking the vector from a pool is what keeps a loop iteration from allocating
/// one; the caller hands it back through `FrameScratch::recycle`.
pub(super) fn decode_statements_into(
    mut payload: FullPayload<'_>,
    span: Span,
    statements: &mut Vec<u32>,
) -> Result<(), RuntimeError> {
    let len = indexed_raw(&mut payload, span)? as usize;
    statements.clear();
    statements.reserve(len);
    for _ in 0..len {
        statements.push(indexed_raw(&mut payload, span)?);
    }
    indexed_finish(payload, span)?;
    statements.reverse();
    Ok(())
}

fn decode_statement_block_into(
    execution: &FullExecution<'_>,
    block: u32,
    span: Span,
    statements: &mut Vec<u32>,
) -> Result<(), RuntimeError> {
    let (_, payload) = execution
        .block_id(block, BLOCK_STATEMENTS)
        .map_err(|error| indexed_error(error, span))?;
    decode_statements_into(payload, span, statements)
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
