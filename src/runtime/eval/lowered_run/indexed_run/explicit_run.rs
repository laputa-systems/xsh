use super::serial_pipeline::{
    IndexedLiveSerialStage, IndexedSerialPipeline, indexed_for_pipeline_input,
};
use super::{
    Arc, AssignOp, BLOCK_LIST, BLOCK_STATEMENTS, BTreeMap, BinaryOp, ControlFlow, Evaluator,
    FormatSpec, FullExecution, FullFunctionView, FullPayload, FullProgram, FullTag, FunctionHeader,
    IndexedAssignStep, IndexedCompQualifier, IndexedFmt, IndexedFmtPart, IndexedOperands,
    IndexedRecordEntry, LoweredFunctionKey, LoweredFunctionKind, LoweredReturnKind, LoweredType,
    LoweredTypeCheck, LoweredValue, Name, ResolvedAssignStep, RuntimeError, Span, StmtFlow,
    StreamValue, TraceKind, TracePayload, TracebackFrame, TracebackFrameKind, append_call_argument,
    append_lowered_list_element, append_lowered_map_literal, apply_indexed_assignment,
    apply_indexed_path_assignment, assign_lowered_bytes_view, assign_lowered_str_view,
    bind_lowered_comp_target, capture_checked_error, checked_indexed_assignment,
    comparison_link_holds, decode_assign_path, decode_comp_qualifiers, decode_comparison_chain,
    decode_match_expr, decode_module_call, decode_record_updates, finish_record_entries,
    fmt_operands, indexed_assignment_operand, indexed_callable_identity, indexed_decode,
    indexed_error, indexed_finish, indexed_optional_raw, indexed_raw, indexed_string,
    indexed_value, lowered_binary_value, lowered_bytes_parts, lowered_comp_iterable,
    lowered_condition_bool, lowered_err_with_cause, lowered_fallback_value,
    lowered_freeze_large_slot_list, lowered_map_literal_key, lowered_match_no_arm,
    lowered_result_err_value, lowered_result_ok, lowered_str_parts, lowered_value_from_runtime_any,
    resolve_assign_index,
};
use super::{LoweredMapCursor, LoweredScalarCursor};
use crate::map_key::MapKey;
use crate::runtime::eval::LoweredCompTarget;
use crate::runtime::eval::lowered_ops::lowered_record_update_batch;
use crate::runtime::eval::lowered_run::validate_parameter_default;

enum FrameValue {
    Value(LoweredValue),
    Break(LoweredValue),
}

// Pending projections and the work stack share producer ownership so error and
// return unwinding can cancel every active producer from innermost to outermost.
// The mutex keeps frame state movable between evaluator threads; only the
// active evaluator pulls the streams.
type CompStreams = Arc<std::sync::Mutex<Vec<Option<(StreamValue, Span)>>>>;

enum CompIterator {
    Scalars {
        cursor: LoweredScalarCursor,
        clause: usize,
    },
    Map {
        cursor: LoweredMapCursor,
        clause: usize,
    },
    Items {
        items: std::vec::IntoIter<LoweredValue>,
        clause: usize,
    },
    Stream {
        stream: usize,
        clause: usize,
    },
}

struct ListCompState {
    map: bool,
    key: Option<u32>,
    value: u32,
    qualifiers: Vec<IndexedCompQualifier>,
    cursor: usize,
    iterators: Vec<CompIterator>,
    streams: CompStreams,
    values: Vec<LoweredValue>,
    map_values: BTreeMap<MapKey, LoweredValue>,
    span: Span,
}

struct FmtState {
    parts: Vec<IndexedFmtPart>,
    index: usize,
    fmt: IndexedFmt,
}

struct AssignPathState {
    slot: usize,
    path: Vec<IndexedAssignStep>,
    selectors: Vec<ResolvedAssignStep>,
    position: usize,
    op: AssignOp,
    value: u32,
    singleton: bool,
    check: Option<LoweredTypeCheck>,
    span: Span,
}

enum FrameContinuation {
    WithBinding {
        bindings: Vec<(usize, u32)>,
        position: usize,
        body: u32,
        else_param_slot: Option<usize>,
        else_body: u32,
        span: Span,
    },
    GuardInput {
        target: LoweredCompTarget,
        else_param_slot: Option<usize>,
        else_body: u32,
        span: Span,
    },
    Store(usize),
    StoreTarget {
        target: LoweredCompTarget,
        span: Span,
    },
    ParameterDefault {
        slot: usize,
        kind: LoweredType,
        check: Option<LoweredTypeCheck>,
        span: Span,
    },
    Assign {
        slot: usize,
        op: AssignOp,
        singleton: bool,
        check: Option<LoweredTypeCheck>,
        span: Span,
    },
    AssignSelector(AssignPathState),
    AssignPath(AssignPathState),
    Return,
    BlockValue,
    ErrorContextEntry {
        body: u32,
        span: Span,
        next: Box<FrameContinuation>,
    },
    ContextScopeEntry {
        kind: crate::syntax::arena::ContextScopeKind,
        body: u32,
        span: Span,
        next: Box<FrameContinuation>,
    },
    Discard(Span),
    ComparisonLeft {
        pairs: Vec<(BinaryOp, u32, Span)>,
        next: Box<FrameContinuation>,
    },
    ComparisonRight {
        left: LoweredValue,
        pairs: Vec<(BinaryOp, u32, Span)>,
        position: usize,
        next: Box<FrameContinuation>,
    },
    Field {
        name: String,
        span: Span,
        next: Box<FrameContinuation>,
    },
    IndexBase {
        instruction: u32,
        span: Span,
        next: Box<FrameContinuation>,
    },
    IndexValue {
        base: LoweredValue,
        span: Span,
        next: Box<FrameContinuation>,
    },
    ModuleArguments {
        op: super::RuntimeOp,
        cli_plan: Option<Arc<crate::modules::cli::CliDescriptorPlan>>,
        args: Vec<Option<u32>>,
        position: usize,
        values: Vec<Option<LoweredValue>>,
        span: Span,
        next: Box<FrameContinuation>,
    },
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
    PatternIf {
        branches: Vec<(u32, u32, Vec<usize>)>,
        index: usize,
        else_body: Option<u32>,
        span: Span,
    },
    PatternWhile {
        body: u32,
        span: Span,
    },
    ForItems {
        target: LoweredCompTarget,
        body: u32,
        span: Span,
    },
    ForPipelineInput {
        slot: usize,
        body: u32,
        span: Span,
        stages: smallvec::SmallVec<[IndexedLiveSerialStage; 4]>,
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
    /// A `yield` statement's value: the frame suspends here and hands the value
    /// to whoever pulled the producer.
    Yield,
    YieldDelegate {
        span: Span,
    },
    DynamicCallee {
        args: Vec<(u32, u32)>,
        span: Span,
        next: Box<FrameContinuation>,
    },
    DynamicArguments {
        callee: LoweredValue,
        args: Vec<(u32, u32)>,
        argument: usize,
        values: Vec<LoweredValue>,
        span: Span,
        next: Box<FrameContinuation>,
    },
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
    WrapErr {
        cause: Option<u32>,
        span: Span,
        next: Box<FrameContinuation>,
    },
    AttachErrCause {
        error: crate::runtime::value::Value,
        span: Span,
        next: Box<FrameContinuation>,
    },
    Try {
        span: Span,
        next: Box<FrameContinuation>,
    },
    CheckedValue {
        check: LoweredTypeCheck,
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
        items: Vec<(u32, bool, Span)>,
        index: usize,
        values: Vec<LoweredValue>,
        next: Box<FrameContinuation>,
    },
    MapLiteralItems {
        entries: Vec<(Option<u32>, u32, Span)>,
        index: usize,
        fields: BTreeMap<MapKey, LoweredValue>,
        key: Option<MapKey>,
        reading_key: bool,
        next: Box<FrameContinuation>,
    },
    RecordUpdateBase {
        updates: Vec<(Vec<Name>, u32, Span)>,
        span: Span,
        next: Box<FrameContinuation>,
    },
    RecordUpdateItems {
        base: LoweredValue,
        updates: Vec<(Vec<Name>, u32, Span)>,
        index: usize,
        values: Vec<(Vec<Name>, LoweredValue, Span)>,
        span: Span,
        next: Box<FrameContinuation>,
    },
    RecordItems {
        entries: Vec<IndexedRecordEntry>,
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
        key: Option<MapKey>,
        next: Box<FrameContinuation>,
    },
}

// Propagated initializer errors stop compound evaluation at the existing With
// handler. Explicit returns and loop transfers retain their separate targets.
fn with_initializer_handler(
    mut continuation: &FrameContinuation,
) -> Option<(Option<usize>, u32, Span)> {
    loop {
        continuation = match continuation {
            FrameContinuation::WithBinding {
                else_param_slot,
                else_body,
                span,
                ..
            } => return Some((*else_param_slot, *else_body, *span)),
            FrameContinuation::Field { next, .. }
            | FrameContinuation::IndexBase { next, .. }
            | FrameContinuation::IndexValue { next, .. }
            | FrameContinuation::ModuleArguments { next, .. }
            | FrameContinuation::ComparisonLeft { next, .. }
            | FrameContinuation::ComparisonRight { next, .. }
            | FrameContinuation::BinaryLeft { next, .. }
            | FrameContinuation::BinaryRight { next, .. }
            | FrameContinuation::BoolBinaryRight { next, .. }
            | FrameContinuation::If { next, .. }
            | FrameContinuation::MatchExprValue { next, .. }
            | FrameContinuation::MatchExprGuard { next, .. }
            | FrameContinuation::CallArguments { next, .. }
            | FrameContinuation::DynamicCallee { next, .. }
            | FrameContinuation::DynamicArguments { next, .. }
            | FrameContinuation::Try { next, .. }
            | FrameContinuation::Require { next, .. }
            | FrameContinuation::CheckedValue { next, .. }
            | FrameContinuation::MethodReceiver { next, .. }
            | FrameContinuation::MethodArg { next, .. }
            | FrameContinuation::FmtValue { next, .. }
            | FrameContinuation::ResultFallback { next, .. }
            | FrameContinuation::ListItems { next, .. }
            | FrameContinuation::RecordItems { next, .. }
            | FrameContinuation::ListCompIter { next, .. }
            | FrameContinuation::ListCompCondition { next, .. }
            | FrameContinuation::ListCompKey { next, .. }
            | FrameContinuation::ListCompValue { next, .. }
            | FrameContinuation::WrapErr { next, .. }
            | FrameContinuation::AttachErrCause { next, .. }
            | FrameContinuation::WrapOk(next) => next,
            _ => return None,
        };
    }
}

// Expression boundaries own completion and failure routing within one lexical
// call frame, so producers can suspend without creating recursive evaluators.
enum ExpressionBoundaryPolicy {
    Value,
    Context(crate::runtime::value::ErrorContext),
    Capture,
    Scope(super::ContextScopeRestore),
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum CleanupFailureResources {
    Release,
    Retain,
    RejectContextEscape,
}

enum FrameWork {
    ClearSlots(Vec<usize>),
    GuardFailureEnd(Span),
    // A lexical expression retains its destination across producer suspension.
    // Its statement scope owns defers; this boundary consumes only the body value.
    ExpressionBoundary {
        policy: ExpressionBoundaryPolicy,
        next: FrameContinuation,
    },
    CompCleanup(CompStreams),
    Statements {
        statements: Vec<u32>,
        complete_call: bool,
        scope_id: Option<u64>,
    },
    Expr {
        instruction: u32,
        span: Span,
        next: FrameContinuation,
    },
    Value {
        value: FrameValue,
        next: FrameContinuation,
    },
    ForScalars {
        target: LoweredCompTarget,
        cursor: LoweredScalarCursor,
        body: u32,
        span: Span,
    },
    ForMap {
        target: LoweredCompTarget,
        cursor: LoweredMapCursor,
        body: u32,
        span: Span,
    },
    ForItems {
        target: LoweredCompTarget,
        items: Vec<LoweredValue>,
        index: usize,
        body: u32,
        span: Span,
    },
    /// A loop over a script producer: each step pulls one item and re-arms
    /// itself, so the loop never holds the whole stream.
    ForStream {
        target: LoweredCompTarget,
        stream: StreamValue,
        body: u32,
        span: Span,
    },
    ForPipeline {
        slot: usize,
        pipeline: Box<IndexedSerialPipeline>,
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
    Loop {
        body: u32,
        span: Span,
    },
    PatternWhile {
        condition: u32,
        body: u32,
        captures: Vec<usize>,
        span: Span,
    },
    Finish(StmtFlow),
    FinishError,
}

/// Whose statements a frame runs. A function frame returns a checked value to
/// its caller. A block frame runs statements for the recursive evaluator
/// against slots it lends, and hands the statement flow back when it finishes.
#[derive(Clone, Copy)]
pub(super) enum FrameOwner {
    Function(LoweredFunctionKey, LoweredFunctionKind),
    /// A top-level statement does not own a scope: it runs in the script's.
    /// Slots the block has not declared belong to `outer_scope`.
    /// `context_depth` is set when a frame inside a context scope lent the
    /// slots: see `LentContextSlots`.
    Block {
        owns_scope: bool,
        outer_scope: u64,
        context_depth: Option<usize>,
    },
}

/// Slot ownership a frame inside a context scope lends to the recursive
/// evaluator, which runs nested bodies (stage blocks, retry bodies) and field
/// assignments on the same slots in frames of their own. Scopes at or above
/// `depth` on the evaluator's scope stack belong to the context body, so a live
/// value may be stored only in a slot one of those scopes owns.
pub(in crate::runtime::eval) struct LentContextSlots {
    slots: usize,
    depth: usize,
    scopes: Vec<u64>,
}

/// A function frame owns its slots. A block frame borrows the recursive
/// evaluator's, so their contents and address never move, and state keyed by
/// that address (root publication, context-scope locals) stays valid.
pub(super) enum FrameSlots<'p> {
    Owned(Vec<LoweredValue>),
    Lent(&'p mut [LoweredValue]),
}

impl std::ops::Deref for FrameSlots<'_> {
    type Target = [LoweredValue];

    fn deref(&self) -> &[LoweredValue] {
        match self {
            Self::Owned(slots) => slots,
            Self::Lent(slots) => slots,
        }
    }
}

impl std::ops::DerefMut for FrameSlots<'_> {
    fn deref_mut(&mut self) -> &mut [LoweredValue] {
        match self {
            Self::Owned(slots) => slots,
            Self::Lent(slots) => slots,
        }
    }
}

pub(super) struct CallFrame<'p> {
    pub(super) owner: FrameOwner,
    /// Whether this frame is a stream producer, whose body ends by falling off
    /// the end of its statements rather than by returning.
    pub(super) producer: bool,
    /// Whether the body was discarded for cancellation. A cancelled frame
    /// replays only cleanup: retained expression boundaries restore their
    /// context scopes and deliver no value to the continuations, which are
    /// body computation rather than cleanup.
    cancelled: bool,
    pub(super) scope_id: u64,
    pub(super) execution: FullExecution<'p>,
    pub(super) slots: FrameSlots<'p>,
    pub(super) slot_scopes: Vec<u64>,
    pub(super) call_span: Span,
    pub(super) definition_span: Span,
    work: Vec<FrameWork>,
    pub(super) defers: Vec<u32>,
    pub(super) block_scopes: Vec<u64>,
    block_defer_offsets: Vec<usize>,
    return_to: Option<FrameContinuation>,
}

enum ProducerSuspension {
    Yielded(LoweredValue),
    Delegated { value: LoweredValue, span: Span },
}

pub(super) struct ExplicitFrames<'a, 'p> {
    evaluator: &'a mut Evaluator,
    program: &'p FullProgram,
    /// Inline room for one frame: a block run for the recursive evaluator
    /// usually never calls, so it does not allocate a call stack.
    calls: smallvec::SmallVec<[CallFrame<'p>; 1]>,
    result: Option<Result<LoweredValue, RuntimeError>>,
    pending_error: Option<RuntimeError>,
    /// The yielded item or delegation source, with the remaining work saved
    /// on the frame's stack until its consumer requests another item.
    suspended: Option<ProducerSuspension>,
    /// A finished block frame's statement flow.
    block_flow: Option<StmtFlow>,
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

    /// Runs a statement block for the recursive evaluator, in a scope of its own.
    pub(super) fn eval_indexed_statement_block(
        &mut self,
        execution: &FullExecution<'_>,
        block: u32,
        slots: &mut [LoweredValue],
        span: Span,
    ) -> Result<StmtFlow, RuntimeError> {
        let mut statements = self.frame_scratch.take_statements();
        decode_statement_block_into(execution, block, span, &mut statements)?;
        let work = FrameWork::Statements {
            statements,
            complete_call: true,
            scope_id: None,
        };
        self.eval_indexed_work_with_frames(execution, work, true, slots, span)
    }

    /// Runs one top-level statement in the script's scope.
    pub(super) fn eval_indexed_top_level_statement(
        &mut self,
        execution: &FullExecution<'_>,
        statement: u32,
        slots: &mut [LoweredValue],
        span: Span,
    ) -> Result<StmtFlow, RuntimeError> {
        let mut statements = self.frame_scratch.take_statements();
        statements.push(statement);
        let work = FrameWork::Statements {
            statements,
            complete_call: true,
            scope_id: None,
        };
        self.eval_indexed_work_with_frames(execution, work, false, slots, span)
    }

    /// Evaluates one expression for the recursive evaluator, in its scope.
    pub(super) fn eval_indexed_expr_with_frames(
        &mut self,
        execution: &FullExecution<'_>,
        instruction: u32,
        slots: &mut [LoweredValue],
        span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let work = FrameWork::Expr {
            instruction,
            span,
            next: FrameContinuation::BlockValue,
        };
        Ok(
            match self.eval_indexed_work_with_frames(execution, work, false, slots, span)? {
                StmtFlow::Value(value) => ControlFlow::Continue(value),
                flow => self.preserve_lexical_expression_flow(flow),
            },
        )
    }

    /// Rejects a live value stored in a slot whose owner scope is below the
    /// context body's `depth` on the scope stack. Owners no longer on the
    /// stack belong to finished bodies and are not outer bindings.
    fn check_context_slot_owner(
        &self,
        owner: u64,
        depth: usize,
        span: Span,
    ) -> Result<(), RuntimeError> {
        match self.scope_ids.iter().rposition(|scope| *scope == owner) {
            Some(position) if position < depth => Err(context_assignment_escape(span)),
            _ => Ok(()),
        }
    }

    /// The context check for an assignment the recursive evaluator performs
    /// on slots a context-scoped frame lent it.
    pub(super) fn check_lent_context_assignment(
        &self,
        slots: &[LoweredValue],
        slot: usize,
        value: &LoweredValue,
        span: Span,
    ) -> Result<(), RuntimeError> {
        match &self.lent_context_slots {
            Some(lent)
                if lent.slots == slots.as_ptr() as usize
                    && Self::context_scope_value_escapes(value) =>
            {
                self.check_context_slot_owner(lent.scopes[slot], lent.depth, span)
            }
            _ => Ok(()),
        }
    }

    /// Statements have one implementation, on the frames, whoever runs them.
    fn eval_indexed_work_with_frames(
        &mut self,
        execution: &FullExecution<'_>,
        first: FrameWork,
        owns_scope: bool,
        slots: &mut [LoweredValue],
        span: Span,
    ) -> Result<StmtFlow, RuntimeError> {
        let Some(program) = self.indexed_program.clone() else {
            return Err(
                RuntimeError::new("indexed-ir", "statement block has no indexed program")
                    .with_span(span),
            );
        };
        let outer_scope = self.current_scope_id();
        let scope_id = if owns_scope {
            self.enter_owned_host_scope()
        } else {
            outer_scope
        };
        let (slot_scopes, context_depth) = match &self.lent_context_slots {
            Some(lent) if lent.slots == slots.as_ptr() as usize => {
                (lent.scopes.clone(), Some(lent.depth))
            }
            _ => (self.frame_scratch.take_slot_scopes(0, outer_scope), None),
        };
        let mut work = self.frame_scratch.take_work();
        work.push(first);
        let mut frames = ExplicitFrames::new(self, &program);
        frames.calls.push(CallFrame {
            owner: FrameOwner::Block {
                owns_scope,
                outer_scope,
                context_depth,
            },
            producer: false,
            cancelled: false,
            scope_id,
            execution: execution.thread_local(),
            slots: FrameSlots::Lent(slots),
            slot_scopes,
            call_span: span,
            definition_span: span,
            work,
            defers: Vec::new(),
            block_scopes: Vec::new(),
            block_defer_offsets: Vec::new(),
            return_to: None,
        });
        let result = frames.run();
        let flow = frames.block_flow.take();
        result.map(|_| flow.expect("a finished block frame reports its flow"))
    }
}

fn context_assignment_escape(span: Span) -> RuntimeError {
    RuntimeError::new(
        "context-scope-escape",
        "a live producer or host handle cannot escape through an outer assignment",
    )
    .with_span(span)
}

/// The slot a receiver instruction reads, when it reads exactly one.
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
    /// The parent pauses while this source supplies elements to its consumer.
    Delegated {
        value: LoweredValue,
        span: Span,
        state: ProducerFrameState,
    },
    /// The body ended, propagated an error, or returned a stream.
    Finished(Result<LoweredValue, RuntimeError>),
}

/// Reusable vectors for frame work and decoded operands.
///
/// Calls and conditional branches repeatedly decode short lists into vectors.
/// Bounded pools reuse their storage after evaluation, along with work stacks,
/// slot scopes, and statement lists, the same way `lowered_slot_pool` does for
/// slots.
#[derive(Default)]
pub(in crate::runtime::eval) struct FrameScratch {
    work: Vec<Vec<FrameWork>>,
    slot_scopes: Vec<Vec<u64>>,
    statements: Vec<Vec<u32>>,
    call_args: Vec<Vec<(u32, u32)>>,
    if_branches: Vec<Vec<(u32, u32)>>,
    /// A frame's block scopes and their defer offsets, taken when the frame
    /// opens its first block.
    block_stacks: Vec<(Vec<u64>, Vec<usize>)>,
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

    fn take_block_stacks(&mut self) -> (Vec<u64>, Vec<usize>) {
        self.block_stacks.pop().unwrap_or_default()
    }

    pub(super) fn take_call_args(&mut self) -> Vec<(u32, u32)> {
        self.call_args.pop().unwrap_or_default()
    }

    pub(super) fn recycle_call_args(&mut self, mut args: Vec<(u32, u32)>) {
        if args.capacity() > 32 || self.call_args.len() >= Self::POOL_CAP {
            return;
        }
        args.clear();
        self.call_args.push(args);
    }

    fn take_if_branches(&mut self) -> Vec<(u32, u32)> {
        self.if_branches.pop().unwrap_or_default()
    }

    fn recycle_if_branches(&mut self, mut branches: Vec<(u32, u32)>) {
        if branches.capacity() > 32 || self.if_branches.len() >= Self::POOL_CAP {
            return;
        }
        branches.clear();
        self.if_branches.push(branches);
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
            let FrameWork::Statements { mut statements, .. } = work else {
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

    /// Returns a frame's block stacks once its block scopes have exited.
    fn recycle_block_stacks(&mut self, call: &mut CallFrame<'_>) {
        if call.block_scopes.capacity() == 0 || self.block_stacks.len() >= Self::POOL_CAP {
            return;
        }
        call.block_scopes.clear();
        call.block_defer_offsets.clear();
        self.block_stacks.push((
            std::mem::take(&mut call.block_scopes),
            std::mem::take(&mut call.block_defer_offsets),
        ));
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
    block_defer_offsets: Vec<usize>,
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
            block_defer_offsets: Vec::new(),
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

    pub(super) fn has_context_scope(&self) -> bool {
        self.work.iter().any(|work| {
            matches!(
                work,
                FrameWork::ExpressionBoundary {
                    policy: ExpressionBoundaryPolicy::Scope(_),
                    ..
                }
            )
        })
    }

    /// Enters the body's call scope, so its slots belong to a live scope.
    pub(super) fn start(&mut self, scope_id: u64) {
        self.scope_id = scope_id;
        self.slot_scopes = vec![scope_id; self.slots.len()];
    }
}

impl<'p> CallFrame<'p> {
    fn owns_scope(&self) -> bool {
        !matches!(
            self.owner,
            FrameOwner::Block {
                owns_scope: false,
                ..
            }
        )
    }

    /// The scope that owns a slot's value. A block frame records only the
    /// slots it declares; the rest belong to the scope that lent them.
    fn slot_scope(&self, slot: usize) -> u64 {
        match (self.slot_scopes.get(slot), self.owner) {
            (Some(scope), _) => *scope,
            (None, FrameOwner::Block { outer_scope, .. }) => outer_scope,
            (None, FrameOwner::Function(..)) => self.scope_id,
        }
    }

    /// Drops the body's remaining work, keeping its registered defers.
    pub(super) fn discard_body(&mut self) {
        self.cancelled = true;
        self.work.retain(|work| {
            matches!(
                work,
                FrameWork::Statements {
                    scope_id: Some(_),
                    ..
                } | FrameWork::ExpressionBoundary { .. }
            )
        });
        for work in &mut self.work {
            if let FrameWork::Statements { statements, .. } = work {
                statements.clear();
            }
        }
        self.work.insert(
            0,
            FrameWork::Statements {
                statements: Vec::new(),
                complete_call: true,
                scope_id: None,
            },
        );
    }

    pub(super) fn into_state(self) -> ProducerFrameState {
        ProducerFrameState {
            work: self.work,
            slots: match self.slots {
                FrameSlots::Owned(slots) => slots,
                FrameSlots::Lent(_) => unreachable!("a producer frame owns its slots"),
            },
            slot_scopes: self.slot_scopes,
            defers: self.defers,
            block_scopes: self.block_scopes,
            block_defer_offsets: self.block_defer_offsets,
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
            owner: FrameOwner::Function(function, kind),
            producer: true,
            cancelled: false,
            scope_id: state.scope_id,
            execution,
            slots: FrameSlots::Owned(state.slots),
            slot_scopes: state.slot_scopes,
            call_span,
            definition_span,
            work: state.work,
            defers: state.defers,
            block_scopes: state.block_scopes,
            block_defer_offsets: state.block_defer_offsets,
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
            calls: smallvec::SmallVec::new(),
            result: None,
            pending_error: None,
            suspended: None,
            block_flow: None,
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
                return Err(
                    RuntimeError::new("control-flow", "yield outside stream producer")
                        .with_span(span),
                );
            }
            let index = self
                .calls
                .len()
                .checked_sub(1)
                .expect("active indexed frame");
            if let Err(error) = self.step(index) {
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
            if let Err(error) = self.step(index) {
                self.begin_error_unwind(error);
            }
        }
        if let Some(suspension) = self.suspended.take() {
            let state = self
                .calls
                .pop()
                .expect("suspended producer frame")
                .into_state();
            return match suspension {
                ProducerSuspension::Yielded(value) => ProducerStep::Yielded { value, state },
                ProducerSuspension::Delegated { value, span } => {
                    ProducerStep::Delegated { value, span, state }
                }
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
        if self.pending_error.is_some() {
            self.evaluator.report_cleanup_error(
                &error,
                self.calls
                    .last()
                    .map(|call| call.call_span)
                    .unwrap_or_else(crate::runtime::eval::zero_span),
            );
        } else {
            self.pending_error = Some(error);
        }
        let Some(index) = self.calls.len().checked_sub(1) else {
            self.result = Some(Err(self
                .pending_error
                .take()
                .expect("pending indexed frame error")));
            return;
        };
        if let Err(error) = self.unwind_error_frame(index) {
            self.begin_error_unwind(error);
        }
    }

    fn capture_boundary(&self, index: usize) -> Option<usize> {
        self.calls[index].work.iter().rposition(|work| {
            matches!(
                work,
                FrameWork::ExpressionBoundary {
                    policy: ExpressionBoundaryPolicy::Capture,
                    ..
                }
            )
        })
    }

    fn boundary_survivor_scope(&self, index: usize, boundary: usize) -> u64 {
        self.calls[index].work[..boundary]
            .iter()
            .rev()
            .find_map(|work| match work {
                FrameWork::Statements {
                    scope_id: Some(scope),
                    ..
                } => Some(*scope),
                _ => None,
            })
            .unwrap_or(self.calls[index].scope_id)
    }

    fn discarded_statement_scopes(&self, index: usize, keep: usize) -> Vec<u64> {
        self.calls[index].work[keep..]
            .iter()
            .filter_map(|work| match work {
                FrameWork::Statements {
                    scope_id: Some(scope),
                    ..
                } => Some(*scope),
                _ => None,
            })
            .collect()
    }

    fn crosses_context_scope(&self, index: usize, keep: usize) -> bool {
        self.calls[index].work[keep..].iter().any(|work| {
            matches!(
                work,
                FrameWork::ExpressionBoundary {
                    policy: ExpressionBoundaryPolicy::Scope(_),
                    ..
                }
            )
        })
    }

    // Callee cleanup completes before searching its caller for a local capture.
    // The selected boundary remains live while its inner lexical scopes unwind.
    fn unwind_error_frame(&mut self, index: usize) -> Result<(), RuntimeError> {
        let mut boundary = self
            .pending_error
            .as_ref()
            .filter(|error| error.abort.is_none() && error.propagated)
            .and_then(|_| self.capture_boundary(index));
        let mut keep = boundary.map_or(0, |boundary| boundary + 1);
        if self.crosses_context_scope(index, keep)
            && self
                .pending_error
                .as_ref()
                .is_some_and(Evaluator::context_scope_runtime_error_escapes)
        {
            self.evaluator.pending_traceback = None;
            self.pending_error = Some(
                RuntimeError::new(
                    "context-scope-escape",
                    "a live producer or host handle cannot escape a restored context",
                )
                .with_span(self.calls[index].call_span),
            );
            boundary = None;
            keep = 0;
        }
        let survivor = boundary.map_or(self.calls[index].scope_id, |boundary| {
            self.boundary_survivor_scope(index, boundary)
        });
        let sources = self.discarded_statement_scopes(index, keep);
        if let Some(error) = self.pending_error.as_ref()
            && error.abort.is_none()
            && error.propagated
        {
            for source in sources {
                self.evaluator
                    .transfer_owned_host_resources_in_runtime_error(error, source, survivor);
            }
        }
        if let Err(error) = self.discard_work_from(index, keep) {
            if error.abort.as_ref().is_some_and(|signal| signal.force) {
                return Err(error);
            }
            self.evaluator
                .report_cleanup_error(&error, self.calls[index].call_span);
        }
        if boundary.is_some() {
            let Some(FrameWork::ExpressionBoundary { next, .. }) = self.calls[index].work.pop()
            else {
                unreachable!()
            };
            let error = self
                .pending_error
                .take()
                .expect("checked indexed frame failure");
            let value = capture_checked_error(error)?;
            self.evaluator.pending_traceback = None;
            self.push_value(index, FrameValue::Value(value), next);
        } else {
            self.calls[index].work.push(FrameWork::FinishError);
        }
        Ok(())
    }

    fn discard_calls(&mut self) {
        while let Some(mut call) = self.calls.pop() {
            if let FrameOwner::Function(function, kind) = call.owner
                && let Ok(header) = self.function_header(function, kind, call.call_span)
            {
                let _ = self.evaluator.write_back_lowered_captures(
                    &header,
                    &call.slots,
                    call.call_span,
                );
            }
            // Forced discard skips user defers but still closes inner owned
            // scopes before restoring each evaluator context in lexical order.
            for work in call.work.drain(..).rev() {
                match work {
                    FrameWork::Statements {
                        scope_id: Some(scope_id),
                        ..
                    } => {
                        if call.block_scopes.last() == Some(&scope_id) {
                            call.block_scopes.pop();
                            let _ = self.evaluator.exit_owned_host_scope(scope_id);
                        }
                    }
                    FrameWork::ExpressionBoundary {
                        policy: ExpressionBoundaryPolicy::Scope(restore),
                        ..
                    } => self.evaluator.restore_indexed_context_scope(restore),
                    _ => {}
                }
            }
            let _ = cleanup_call_scopes(self.evaluator, &mut call);
            let FrameOwner::Function(function, kind) = call.owner else {
                continue;
            };
            self.release_slots(call.slots);
            self.evaluator.call_stack.pop();
            let exit_kind = match kind {
                LoweredFunctionKind::Pure => TraceKind::PureExit,
                LoweredFunctionKind::Proc => TraceKind::ProcExit,
            };
            let name = function.display_name();
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

    /// Walks a named call's arguments from `argument`, filling the arguments it
    /// omits from the callee's prepared defaults, then resolves the call.
    fn push_static_arguments(
        &mut self,
        index: usize,
        function: LoweredFunctionKey,
        kind: LoweredFunctionKind,
        args: Vec<(u32, u32)>,
        mut argument: usize,
        mut values: Vec<LoweredValue>,
        span: Span,
        next: FrameContinuation,
    ) -> Result<(), RuntimeError> {
        while let Some((arg_kind, instruction)) = args.get(argument).copied() {
            if arg_kind == 2 {
                values.push(self.evaluator.indexed_argument_default_for(
                    function,
                    kind,
                    instruction as usize,
                    span,
                )?);
                argument += 1;
            } else {
                self.push_expr(
                    index,
                    instruction,
                    span,
                    FrameContinuation::CallArguments {
                        function,
                        kind,
                        args,
                        index: argument,
                        values,
                        span,
                        next: Box::new(next),
                    },
                );
                return Ok(());
            }
        }
        self.evaluator.frame_scratch.recycle_call_args(args);
        self.push_resolved_call(index, function, kind, values, span, next)
    }

    fn push_dynamic_arguments(
        &mut self,
        index: usize,
        callee: LoweredValue,
        args: Vec<(u32, u32)>,
        mut argument: usize,
        mut values: Vec<LoweredValue>,
        span: Span,
        next: FrameContinuation,
    ) -> Result<(), RuntimeError> {
        while let Some((kind, instruction)) = args.get(argument).copied() {
            if kind == 2 {
                values.push(self.evaluator.indexed_argument_default(
                    &callee,
                    instruction as usize,
                    span,
                )?);
                argument += 1;
            } else {
                self.push_expr(
                    index,
                    instruction,
                    span,
                    FrameContinuation::DynamicArguments {
                        callee,
                        args,
                        argument,
                        values,
                        span,
                        next: Box::new(next),
                    },
                );
                return Ok(());
            }
        }
        self.evaluator.frame_scratch.recycle_call_args(args);
        let (function, kind) = indexed_callable_identity(&callee, span)?;
        if self
            .program
            .function_view(function, kind)
            .map_err(|error| indexed_error(error, span))?
            .is_some()
        {
            self.push_resolved_call(index, function, kind, values, span, next)
        } else if let LoweredFunctionKey::Qualified(qualified) = function {
            let value = self
                .evaluator
                .eval_indexed_external_call(qualified, &values, span)?;
            self.push_value(index, FrameValue::Value(value), next);
            Ok(())
        } else {
            Err(
                RuntimeError::new("unresolved-lowered-call", function.display_name())
                    .with_span(span),
            )
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
        let header = view
            .header()
            .map_err(|error| indexed_error(error, call_span))?;
        let slots = self
            .evaluator
            .bind_lowered_values_owned(&header, values, call_span)?;
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
        // A call starts only once any earlier `Err` was handled.
        self.evaluator.pending_traceback = None;
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
            owner: FrameOwner::Function(function, kind),
            producer: false,
            cancelled: false,
            scope_id,
            execution,
            slots: FrameSlots::Owned(slots),
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
            block_defer_offsets: Vec::new(),
            return_to,
        });
        Ok(())
    }

    fn step(&mut self, index: usize) -> Result<(), RuntimeError> {
        if let Some(FrameWork::Statements { .. }) = self.calls[index].work.last() {
            return self.step_statements(index);
        }
        let work = self.calls[index].work.pop().expect("indexed frame work");
        match work {
            FrameWork::ClearSlots(slots) => {
                for slot in slots {
                    self.calls[index].slots[slot] = LoweredValue::Unit;
                }
                Ok(())
            }
            FrameWork::GuardFailureEnd(span) => {
                Err(RuntimeError::new("guard", "guard else block must diverge").with_span(span))
            }
            FrameWork::ExpressionBoundary { policy, next } => {
                if self.calls[index].cancelled {
                    // A cancelled body replays only cleanup. Restore the scope
                    // this boundary opened and skip its continuation: feeding a
                    // placeholder value into body computation (selecting match
                    // arms, for one) can fail on a value the body never produced.
                    if let ExpressionBoundaryPolicy::Scope(restore) = policy {
                        self.evaluator.restore_indexed_context_scope(restore);
                    }
                    return Ok(());
                }
                let value = match policy {
                    ExpressionBoundaryPolicy::Capture => {
                        LoweredValue::ResultOk(Box::new(LoweredValue::Unit))
                    }
                    ExpressionBoundaryPolicy::Scope(restore) => {
                        self.evaluator.restore_indexed_context_scope(restore);
                        lowered_result_ok(LoweredValue::Unit)
                    }
                    _ => LoweredValue::Unit,
                };
                self.push_value(index, FrameValue::Value(value), next);
                Ok(())
            }
            FrameWork::CompCleanup(streams) => self.cleanup_comp_streams(streams),
            FrameWork::Statements { .. } => unreachable!("statement lists step in place"),
            FrameWork::Expr {
                instruction,
                span,
                next,
            } => self.eval_expr(index, instruction, span, next),
            FrameWork::Value { value, next } => self.continue_value(index, value, next),
            FrameWork::ForMap {
                target,
                mut cursor,
                body,
                span,
            } => {
                self.evaluator.service_pending_signal(span)?;
                if self.evaluator.signal_state.shutdown_complete {
                    return Ok(());
                }
                if let Some(item) = cursor.next() {
                    bind_lowered_comp_target(&target, item, &mut self.calls[index].slots, span)?;
                    self.calls[index].work.push(FrameWork::ForMap {
                        target,
                        cursor,
                        body,
                        span,
                    });
                    self.push_statement_block(index, body, span)?;
                }
                Ok(())
            }
            FrameWork::ForScalars {
                target,
                mut cursor,
                body,
                span,
            } => {
                self.evaluator.service_pending_signal(span)?;
                if self.evaluator.signal_state.shutdown_complete {
                    return Ok(());
                }
                if let Some(item) = cursor.next() {
                    bind_lowered_comp_target(&target, item, &mut self.calls[index].slots, span)?;
                    self.calls[index].work.push(FrameWork::ForScalars {
                        target,
                        cursor,
                        body,
                        span,
                    });
                    self.push_statement_block(index, body, span)?;
                }
                Ok(())
            }
            FrameWork::ForItems {
                target,
                items,
                index: item_index,
                body,
                span,
            } => self.step_for_items(index, target, items, item_index, body, span),
            FrameWork::ForStream {
                target,
                stream,
                body,
                span,
            } => self.step_for_stream(index, target, stream, body, span),
            FrameWork::ForPipeline {
                slot,
                pipeline,
                body,
                span,
            } => self.step_for_pipeline(index, slot, pipeline, body, span),
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
            FrameWork::Loop { body, span } => {
                self.calls[index].work.push(FrameWork::Loop { body, span });
                self.push_statement_block(index, body, span)
            }
            FrameWork::PatternWhile {
                condition,
                body,
                captures,
                span,
            } => {
                self.evaluator.service_pending_signal(span)?;
                if self.evaluator.shutting_down() {
                    return Ok(());
                }
                self.calls[index].work.push(FrameWork::PatternWhile {
                    condition,
                    body,
                    captures: captures.clone(),
                    span,
                });
                self.open_pattern_scope(index, captures);
                self.push_expr(
                    index,
                    condition,
                    span,
                    FrameContinuation::PatternWhile { body, span },
                );
                Ok(())
            }
            FrameWork::Finish(flow) => self.finish_deferred_call(index, flow),
            FrameWork::FinishError => self.finish_error_deferred_call(index),
        }
    }

    /// Runs the next statement of the list on top of the work stack. The list
    /// stays in place and comes off the stack once it is exhausted.
    fn step_statements(&mut self, index: usize) -> Result<(), RuntimeError> {
        let signal = self
            .evaluator
            .service_pending_signal(self.calls[index].call_span);
        if signal.is_err() || self.evaluator.shutting_down() {
            // The block stays on the work stack so error unwinding closes its
            // owned scope before running function defers.
            signal?;
            return Err(RuntimeError::abort(
                self.evaluator.signal_state.shutdown_status.unwrap_or(3),
                self.evaluator.signal_state.shutdown_force,
            )
            .with_span(self.calls[index].call_span));
        }
        let Some(FrameWork::Statements { statements, .. }) = self.calls[index].work.last_mut()
        else {
            unreachable!("step_statements runs a statement list");
        };
        let Some(statement) = statements.pop() else {
            let Some(FrameWork::Statements {
                statements,
                complete_call,
                scope_id,
            }) = self.calls[index].work.pop()
            else {
                unreachable!("step_statements runs a statement list");
            };
            // A list that ran to its end goes back to the pool here: its
            // entries are done with, and a loop body would otherwise allocate a
            // fresh list on every iteration.
            self.evaluator.frame_scratch.recycle_statements(statements);
            return if complete_call {
                self.complete_call(index, StmtFlow::None)
            } else {
                if let Some(scope_id) = scope_id {
                    self.exit_block_scope(index, scope_id, true, CleanupFailureResources::Retain)?;
                }
                Ok(())
            };
        };
        // A block frame running top-level statements publishes script
        // bindings between them, as the statements' own reads expect.
        if let FrameOwner::Block { .. } = self.calls[index].owner {
            let call = &mut self.calls[index];
            self.evaluator
                .sync_indexed_root_slots(&mut call.slots, call.call_span)?;
        }
        self.eval_statement(index, statement)
    }

    fn eval_statement(&mut self, index: usize, instruction: u32) -> Result<(), RuntimeError> {
        // A statement starts only once any earlier `Err` was handled.
        self.evaluator.pending_traceback = None;
        let span = self.calls[index].call_span;
        let (tag, mut payload) = indexed_value(
            self.calls[index].execution.instruction_id(instruction),
            span,
        )?;
        match tag {
            FullTag::StmtDefaultParameter => {
                let slot =
                    indexed_decode::<usize>(&mut payload, &self.calls[index].execution, span)?;
                let value = indexed_raw(&mut payload, span)?;
                let kind = indexed_decode::<LoweredType>(
                    &mut payload,
                    &self.calls[index].execution,
                    span,
                )?;
                let check = indexed_decode::<Option<LoweredTypeCheck>>(
                    &mut payload,
                    &self.calls[index].execution,
                    span,
                )?;
                let span =
                    indexed_decode::<Span>(&mut payload, &self.calls[index].execution, span)?;
                indexed_finish(payload, span)?;
                if matches!(self.calls[index].slots[slot], LoweredValue::OmittedArgument) {
                    self.push_expr(
                        index,
                        value,
                        span,
                        FrameContinuation::ParameterDefault {
                            slot,
                            kind,
                            check,
                            span,
                        },
                    );
                }
                Ok(())
            }
            FullTag::StmtLet => {
                let slot: usize = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                let value = indexed_raw(&mut payload, span)?;
                indexed_finish(payload, span)?;
                self.push_expr(index, value, span, FrameContinuation::Store(slot));
                Ok(())
            }
            FullTag::StmtLetRecord => {
                let source = indexed_raw(&mut payload, span)?;
                let target = indexed_decode::<LoweredCompTarget>(
                    &mut payload,
                    &self.calls[index].execution,
                    span,
                )?;
                let value_span = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                indexed_finish(payload, span)?;
                self.declare_target(index, &target);
                self.push_expr(
                    index,
                    source,
                    span,
                    FrameContinuation::StoreTarget {
                        target,
                        span: value_span,
                    },
                );
                Ok(())
            }
            FullTag::StmtWith => {
                let (_, mut binding_words) = self.calls[index]
                    .execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, span))?;
                let count = indexed_raw(&mut binding_words, span)? as usize;
                let mut bindings = Vec::with_capacity(count);
                for _ in 0..count {
                    bindings.push((
                        indexed_decode::<usize>(
                            &mut binding_words,
                            &self.calls[index].execution,
                            span,
                        )?,
                        indexed_raw(&mut binding_words, span)?,
                    ));
                }
                indexed_finish(binding_words, span)?;
                let body = indexed_raw(&mut payload, span)?;
                let else_param_slot = indexed_decode::<Option<usize>>(
                    &mut payload,
                    &self.calls[index].execution,
                    span,
                )?;
                let else_body = indexed_raw(&mut payload, span)?;
                let captures =
                    indexed_decode::<Vec<usize>>(&mut payload, &self.calls[index].execution, span)?;
                let span =
                    indexed_decode::<Span>(&mut payload, &self.calls[index].execution, span)?;
                indexed_finish(payload, span)?;
                let scope_id = self.evaluator.enter_owned_host_scope();
                let defer_offset = self.calls[index].defers.len();
                self.calls[index].block_scopes.push(scope_id);
                self.calls[index].block_defer_offsets.push(defer_offset);
                self.calls[index].work.push(FrameWork::Statements {
                    statements: Vec::new(),
                    complete_call: false,
                    scope_id: Some(scope_id),
                });
                self.calls[index].work.push(FrameWork::ClearSlots(captures));
                if let Some((_, value)) = bindings.first().copied() {
                    self.push_expr(
                        index,
                        value,
                        span,
                        FrameContinuation::WithBinding {
                            bindings,
                            position: 0,
                            body,
                            else_param_slot,
                            else_body,
                            span,
                        },
                    );
                    Ok(())
                } else {
                    self.push_statement_block(index, body, span)
                }
            }
            FullTag::StmtGuard => {
                let target = indexed_decode::<LoweredCompTarget>(
                    &mut payload,
                    &self.calls[index].execution,
                    span,
                )?;
                let value = indexed_raw(&mut payload, span)?;
                let else_param_slot = indexed_decode::<Option<usize>>(
                    &mut payload,
                    &self.calls[index].execution,
                    span,
                )?;
                let else_body = indexed_raw(&mut payload, span)?;
                let span =
                    indexed_decode::<Span>(&mut payload, &self.calls[index].execution, span)?;
                indexed_finish(payload, span)?;
                self.push_expr(
                    index,
                    value,
                    span,
                    FrameContinuation::GuardInput {
                        target,
                        else_param_slot,
                        else_body,
                        span,
                    },
                );
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
                        return self.complete_expression_escape(index, value);
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
                        return self.complete_expression_escape(index, value);
                    }
                }
                Ok(())
            }
            FullTag::StmtAssign => {
                let slot: usize = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                let op = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                let value = indexed_raw(&mut payload, span)?;
                let check = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                let value_span = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                indexed_finish(payload, span)?;
                let (value, singleton) = indexed_assignment_operand(
                    &self.calls[index].execution,
                    value,
                    op,
                    value_span,
                )?;
                self.push_expr(
                    index,
                    value,
                    value_span,
                    FrameContinuation::Assign {
                        slot,
                        op,
                        singleton,
                        check,
                        span: value_span,
                    },
                );
                Ok(())
            }
            FullTag::StmtAssignPath => {
                let slot = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                let path = decode_assign_path(&self.calls[index].execution, &mut payload, span)?;
                let op = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                let value = indexed_raw(&mut payload, span)?;
                let check = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                let span = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                indexed_finish(payload, span)?;
                let (value, singleton) =
                    indexed_assignment_operand(&self.calls[index].execution, value, op, span)?;
                self.advance_assign_path(
                    index,
                    AssignPathState {
                        slot,
                        path,
                        selectors: Vec::new(),
                        position: 0,
                        op,
                        value,
                        singleton,
                        check,
                        span,
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
                let mut values = self.evaluator.frame_scratch.take_if_branches();
                values.reserve(len);
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
                    } else {
                        self.evaluator.frame_scratch.recycle_if_branches(values);
                        if let Some(body) = else_body {
                            self.push_statement_block(index, body, span)?;
                        }
                    }
                    return Ok(());
                }
                let mut selected = None;
                for (condition, body) in values.iter().copied() {
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
                            return self.complete_expression_escape(index, value);
                        }
                    }
                }
                self.evaluator.frame_scratch.recycle_if_branches(values);
                if let Some(body) = selected.or(else_body) {
                    self.push_statement_block(index, body, span)?;
                }
                Ok(())
            }
            // Pattern conditionals run on the frame, like `if`, so a `yield`
            // in a branch suspends the producer that lexically contains it.
            FullTag::StmtPatternIf => {
                let (_, mut words) = self.calls[index]
                    .execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, span))?;
                let count = indexed_raw(&mut words, span)? as usize;
                let mut branches = Vec::with_capacity(count);
                for _ in 0..count {
                    let condition = indexed_raw(&mut words, span)?;
                    let body = indexed_raw(&mut words, span)?;
                    let captures = indexed_decode::<Vec<usize>>(
                        &mut words,
                        &self.calls[index].execution,
                        span,
                    )?;
                    branches.push((condition, body, captures));
                }
                indexed_finish(words, span)?;
                let else_body = indexed_optional_raw(&mut payload, span)?;
                let span =
                    indexed_decode::<Span>(&mut payload, &self.calls[index].execution, span)?;
                indexed_finish(payload, span)?;
                self.start_pattern_branch(index, branches, 0, else_body, span)
            }
            FullTag::StmtPatternWhile => {
                let condition = indexed_raw(&mut payload, span)?;
                let body = indexed_raw(&mut payload, span)?;
                let captures =
                    indexed_decode::<Vec<usize>>(&mut payload, &self.calls[index].execution, span)?;
                let span =
                    indexed_decode::<Span>(&mut payload, &self.calls[index].execution, span)?;
                indexed_finish(payload, span)?;
                self.calls[index].work.push(FrameWork::PatternWhile {
                    condition,
                    body,
                    captures,
                    span,
                });
                Ok(())
            }
            FullTag::StmtFor | FullTag::StmtForRecord => {
                let target = if tag == FullTag::StmtForRecord {
                    indexed_decode::<LoweredCompTarget>(
                        &mut payload,
                        &self.calls[index].execution,
                        span,
                    )?
                } else {
                    LoweredCompTarget::Slot(indexed_decode(
                        &mut payload,
                        &self.calls[index].execution,
                        span,
                    )?)
                };
                let iter = indexed_raw(&mut payload, span)?;
                let body = indexed_raw(&mut payload, span)?;
                let value_span = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                indexed_finish(payload, span)?;
                self.declare_target(index, &target);
                // Only a single loop variable pulls a serial pipeline row by row.
                if let LoweredCompTarget::Slot(slot) = target
                    && let Some((input, pipeline_span, stages)) =
                        indexed_for_pipeline_input(&self.calls[index].execution, iter, value_span)?
                {
                    self.push_expr(
                        index,
                        input,
                        pipeline_span,
                        FrameContinuation::ForPipelineInput {
                            slot,
                            body,
                            span: pipeline_span,
                            stages,
                        },
                    );
                    return Ok(());
                }
                self.push_expr(
                    index,
                    iter,
                    value_span,
                    FrameContinuation::ForItems {
                        target,
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
            FullTag::StmtLoop => {
                let body = indexed_raw(&mut payload, span)?;
                indexed_finish(payload, span)?;
                self.calls[index].work.push(FrameWork::Loop { body, span });
                Ok(())
            }
            FullTag::StmtBreak => {
                indexed_finish(payload, span)?;
                self.break_loop(index, None)
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
            FullTag::StmtYield | FullTag::StmtYieldDelegate => {
                let value = indexed_raw(&mut payload, span)?;
                let span = if tag == FullTag::StmtYieldDelegate {
                    indexed_decode(&mut payload, &self.calls[index].execution, span)?
                } else {
                    span
                };
                indexed_finish(payload, span)?;
                if !self.calls[index].producer {
                    return Err(
                        RuntimeError::new("control-flow", "yield outside stream producer")
                            .with_span(span),
                    );
                }
                let continuation = if tag == FullTag::StmtYieldDelegate {
                    FrameContinuation::YieldDelegate { span }
                } else {
                    FrameContinuation::Yield
                };
                self.push_expr(index, value, span, continuation);
                Ok(())
            }
            FullTag::StmtDefer => {
                let value = indexed_raw(&mut payload, span)?;
                indexed_finish(payload, span)?;
                self.calls[index].defers.push(value);
                Ok(())
            }
            FullTag::StmtValue => {
                let value = indexed_raw(&mut payload, span)?;
                indexed_finish(payload, span)?;
                self.push_expr(index, value, span, FrameContinuation::BlockValue);
                Ok(())
            }
            FullTag::StmtReturn => {
                let value = indexed_raw(&mut payload, span)?;
                indexed_finish(payload, span)?;
                self.push_expr(index, value, span, FrameContinuation::Return);
                Ok(())
            }
            _ => {
                let flow = self.with_lent_context(index, |evaluator, execution, slots| {
                    evaluator.eval_indexed_stmt(execution, instruction, slots, span)
                })?;
                match flow {
                    StmtFlow::None => Ok(()),
                    StmtFlow::Value(value) => self.complete_expression_value(index, value),
                    StmtFlow::Break(value) => self.break_loop(index, value),
                    StmtFlow::Continue => self.continue_loop(index),
                    flow => self.complete_call(index, flow),
                }
            }
        }
    }

    /// A slot read or literal, which evaluates without scheduling any work.
    fn leaf_operand(
        &mut self,
        index: usize,
        instruction: u32,
        span: Span,
    ) -> Result<Option<LoweredValue>, RuntimeError> {
        let (tag, mut payload) = indexed_value(
            self.calls[index].execution.instruction_id(instruction),
            span,
        )?;
        let execution = &self.calls[index].execution;
        let value = match tag {
            FullTag::ExprNull => LoweredValue::Null,
            FullTag::ExprUnit => LoweredValue::Unit,
            FullTag::ExprInt => LoweredValue::Int(indexed_decode(&mut payload, execution, span)?),
            FullTag::ExprBool => LoweredValue::Bool(indexed_decode(&mut payload, execution, span)?),
            FullTag::ExprStr => LoweredValue::Str(indexed_decode(&mut payload, execution, span)?),
            FullTag::ExprParam => {
                let slot: usize = indexed_decode(&mut payload, execution, span)?;
                indexed_finish(payload, span)?;
                let slot = &mut self.calls[index].slots[slot];
                lowered_freeze_large_slot_list(slot);
                return Ok(Some(slot.clone()));
            }
            _ => return Ok(None),
        };
        indexed_finish(payload, span)?;
        Ok(Some(value))
    }

    fn eval_expr(
        &mut self,
        index: usize,
        instruction: u32,
        span: Span,
        next: FrameContinuation,
    ) -> Result<(), RuntimeError> {
        if let Some(value) = self.leaf_operand(index, instruction, span)? {
            self.push_value(index, FrameValue::Value(value), next);
            return Ok(());
        }
        let (tag, mut payload) = indexed_value(
            self.calls[index].execution.instruction_id(instruction),
            span,
        )?;
        match tag {
            FullTag::ExprValueBlock => {
                let body = indexed_raw(&mut payload, span)?;
                let span = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                indexed_finish(payload, span)?;
                self.calls[index].work.push(FrameWork::ExpressionBoundary {
                    policy: ExpressionBoundaryPolicy::Value,
                    next,
                });
                self.push_statement_block(index, body, span)?;
            }
            FullTag::ExprCapture => {
                let body = indexed_raw(&mut payload, span)?;
                let span = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                indexed_finish(payload, span)?;
                self.calls[index].work.push(FrameWork::ExpressionBoundary {
                    policy: ExpressionBoundaryPolicy::Capture,
                    next,
                });
                self.push_statement_block(index, body, span)?;
            }
            FullTag::ExprContextScope => {
                let kind = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                let input = indexed_raw(&mut payload, span)?;
                let body = indexed_raw(&mut payload, span)?;
                let span = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                indexed_finish(payload, span)?;
                self.push_expr(
                    index,
                    input,
                    span,
                    FrameContinuation::ContextScopeEntry {
                        kind,
                        body,
                        span,
                        next: Box::new(next),
                    },
                );
            }
            FullTag::ExprErrorContext => {
                let message = indexed_raw(&mut payload, span)?;
                let body = indexed_raw(&mut payload, span)?;
                let span = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                indexed_finish(payload, span)?;
                self.push_expr(
                    index,
                    message,
                    span,
                    FrameContinuation::ErrorContextEntry {
                        body,
                        span,
                        next: Box::new(next),
                    },
                );
            }
            FullTag::ExprComparisonChain => {
                let (first, pairs) =
                    decode_comparison_chain(&self.calls[index].execution, &mut payload, span)?;
                indexed_finish(payload, span)?;
                self.push_expr(
                    index,
                    first,
                    span,
                    FrameContinuation::ComparisonLeft {
                        pairs,
                        next: Box::new(next),
                    },
                );
            }
            FullTag::ExprBinary => {
                let op = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                let left = indexed_raw(&mut payload, span)?;
                let right = indexed_raw(&mut payload, span)?;
                let value_span = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                indexed_finish(payload, span)?;
                // Slot and literal operands are read in place rather than
                // scheduled; `and`/`or` keep their short circuit.
                if !matches!(op, BinaryOp::And | BinaryOp::Or)
                    && let Some(left) = self.leaf_operand(index, left, value_span)?
                {
                    match self.leaf_operand(index, right, value_span)? {
                        Some(right) => self.push_value(
                            index,
                            FrameValue::Value(lowered_binary_value(op, left, right, value_span)?),
                            next,
                        ),
                        None => self.push_expr(
                            index,
                            right,
                            value_span,
                            FrameContinuation::BinaryRight {
                                op,
                                left,
                                span: value_span,
                                next: Box::new(next),
                            },
                        ),
                    }
                    return Ok(());
                }
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
                let mut values = self.evaluator.frame_scratch.take_if_branches();
                values.reserve(len);
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
                    self.evaluator.frame_scratch.recycle_if_branches(values);
                    self.push_expr(index, else_value, value_span, next);
                }
            }
            FullTag::ExprMatch => {
                let (value, decoded_arms, value_span) =
                    decode_match_expr(&self.calls[index].execution, &mut payload, span)?;
                indexed_finish(payload, span)?;
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
            // Compound expressions and formatted strings must stay in the active heap-backed frame machine.
            // Falling back to the recursive evaluator here would nest another explicit runner for each Result
            // call in the expression and consume native stack even when the outer loop itself is iterative.
            FullTag::ExprList | FullTag::ExprListBuild => {
                let items = IndexedOperands::<(u32, bool, Span)>::literal(
                    &self.calls[index].execution,
                    payload,
                    tag == FullTag::ExprListBuild,
                    span,
                )?
                .into_vec()?;
                if let Some(&(instruction, _, item_span)) = items.first() {
                    let values = Vec::with_capacity(items.len());
                    self.push_expr(
                        index,
                        instruction,
                        item_span,
                        FrameContinuation::ListItems {
                            items,
                            index: 0,
                            values,
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
            FullTag::ExprMapLiteral => {
                let entries = IndexedOperands::<(Option<u32>, u32, Span)>::literal(
                    &self.calls[index].execution,
                    payload,
                    false,
                    span,
                )?
                .into_vec()?;
                if let Some(&(key, value, entry_span)) = entries.first() {
                    self.push_expr(
                        index,
                        key.unwrap_or(value),
                        entry_span,
                        FrameContinuation::MapLiteralItems {
                            entries,
                            index: 0,
                            fields: BTreeMap::new(),
                            key: None,
                            reading_key: key.is_some(),
                            next: Box::new(next),
                        },
                    );
                } else {
                    self.push_value(
                        index,
                        FrameValue::Value(LoweredValue::Map(Arc::new(BTreeMap::new()))),
                        next,
                    );
                }
            }
            FullTag::ExprRecordUpdate => {
                let base = indexed_raw(&mut payload, span)?;
                let updates =
                    decode_record_updates(&self.calls[index].execution, &mut payload, span)?;
                let update_span =
                    indexed_decode::<Span>(&mut payload, &self.calls[index].execution, span)?;
                indexed_finish(payload, span)?;
                self.push_expr(
                    index,
                    base,
                    update_span,
                    FrameContinuation::RecordUpdateBase {
                        updates,
                        span: update_span,
                        next: Box::new(next),
                    },
                );
            }
            FullTag::ExprRecord => {
                let entries = IndexedOperands::<IndexedRecordEntry>::literal(
                    &self.calls[index].execution,
                    payload,
                    false,
                    span,
                )?
                .into_vec()?;
                if let Some(entry) = entries.first() {
                    let instruction = entry.instruction();
                    self.push_expr(
                        index,
                        instruction,
                        span,
                        FrameContinuation::RecordItems {
                            entries,
                            index: 0,
                            fields: Vec::new(),
                            span,
                            next: Box::new(next),
                        },
                    );
                } else {
                    self.push_value(
                        index,
                        FrameValue::Value(finish_record_entries(Vec::new())),
                        next,
                    );
                }
            }
            FullTag::ExprListComp | FullTag::ExprMapComp => {
                let map = tag == FullTag::ExprMapComp;
                let key = map.then(|| indexed_raw(&mut payload, span)).transpose()?;
                let value = indexed_raw(&mut payload, span)?;
                let qualifiers =
                    decode_comp_qualifiers(&self.calls[index].execution, &mut payload, span)?;
                let value_span = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                indexed_finish(payload, span)?;
                let streams = Arc::new(std::sync::Mutex::new(Vec::new()));
                self.calls[index]
                    .work
                    .push(FrameWork::CompCleanup(streams.clone()));
                let state = ListCompState {
                    map,
                    key,
                    value,
                    qualifiers,
                    cursor: 0,
                    iterators: Vec::new(),
                    streams,
                    values: Vec::new(),
                    map_values: BTreeMap::new(),
                    span: value_span,
                };
                self.step_comp_qualifier(index, state, next)?;
            }
            FullTag::ExprFmtString | FullTag::ExprPathFmtString => {
                let (parts, fmt) = fmt_operands(
                    &self.calls[index].execution,
                    payload,
                    tag == FullTag::ExprPathFmtString,
                    span,
                )?;
                let parts = parts.into_vec()?;
                self.step_fmt(
                    index,
                    FmtState {
                        parts,
                        index: 0,
                        fmt,
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
                let cause = indexed_optional_raw(&mut payload, span)?;
                indexed_finish(payload, span)?;
                self.push_expr(
                    index,
                    value,
                    span,
                    FrameContinuation::WrapErr {
                        cause,
                        span,
                        next: Box::new(next),
                    },
                );
            }
            FullTag::ExprTry => {
                let value = indexed_raw(&mut payload, span)?;
                indexed_finish(payload, span)?;
                // The `?` reuses only a traceback that its operand records.
                self.evaluator.pending_traceback = None;
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
            FullTag::ExprCheckedValue => {
                let value = indexed_raw(&mut payload, span)?;
                let check = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                let span = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                indexed_finish(payload, span)?;
                self.push_expr(
                    index,
                    value,
                    span,
                    FrameContinuation::CheckedValue {
                        check,
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
                let name: Arc<str> = Arc::from(indexed_string(
                    &mut payload,
                    &self.calls[index].execution,
                    span,
                )?);
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
                    FrameContinuation::Assign {
                        slot, op, check, ..
                    } if *op == AssignOp::Set
                        && check.is_none()
                        && !self
                            .evaluator
                            .indexed_root_binds(&self.calls[index].slots, *slot)
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
                if decoded_args.is_empty()
                    && let Some(receiver) = self.leaf_operand(index, receiver, value_span)?
                {
                    let receiver =
                        take_consumed_receiver(&mut self.calls[index].slots, consume, receiver);
                    return self.push_method_result(
                        index,
                        receiver,
                        name,
                        Vec::new(),
                        value_span,
                        next,
                    );
                }
                self.push_expr(
                    index,
                    receiver,
                    value_span,
                    FrameContinuation::MethodReceiver {
                        name,
                        args: decoded_args,
                        span: value_span,
                        consume,
                        next: Box::new(next),
                    },
                );
            }
            FullTag::ExprDynamicCall => {
                let callee = indexed_raw(&mut payload, span)?;
                let mut args = self.evaluator.frame_scratch.take_call_args();
                decode_call_args_into(&self.calls[index].execution, &mut payload, span, &mut args)?;
                let value_span = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                indexed_finish(payload, span)?;
                self.push_expr(
                    index,
                    callee,
                    value_span,
                    FrameContinuation::DynamicCallee {
                        args,
                        span: value_span,
                        next: Box::new(next),
                    },
                );
            }
            FullTag::ExprField => {
                let base = indexed_raw(&mut payload, span)?;
                let name =
                    indexed_string(&mut payload, &self.calls[index].execution, span)?.to_string();
                let value_span = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                indexed_finish(payload, span)?;
                if let Some(base) = Evaluator::indexed_field_chain_ref(
                    &self.calls[index].execution,
                    base,
                    &self.calls[index].slots,
                    value_span,
                )? && let Some(value) = super::lowered_record_field_value(base, &name)
                {
                    self.push_value(index, FrameValue::Value(value), next);
                    return Ok(());
                }
                self.push_expr(
                    index,
                    base,
                    value_span,
                    FrameContinuation::Field {
                        name,
                        span: value_span,
                        next: Box::new(next),
                    },
                );
            }
            FullTag::ExprIndex => {
                let base = indexed_raw(&mut payload, span)?;
                let instruction = indexed_raw(&mut payload, span)?;
                let value_span = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                indexed_finish(payload, span)?;
                self.push_expr(
                    index,
                    base,
                    value_span,
                    FrameContinuation::IndexBase {
                        instruction,
                        span: value_span,
                        next: Box::new(next),
                    },
                );
            }
            FullTag::ExprModuleCall => {
                let (op, cli_plan, args, value_span) =
                    decode_module_call(&self.calls[index].execution, &mut payload, span)?;
                indexed_finish(payload, span)?;
                self.step_module_arguments(
                    index,
                    op,
                    cli_plan,
                    args,
                    0,
                    Vec::new(),
                    value_span,
                    next,
                )?;
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
                let mut args = self.evaluator.frame_scratch.take_call_args();
                decode_call_args_into(&self.calls[index].execution, &mut payload, span, &mut args)?;
                let value_span = indexed_decode(&mut payload, &self.calls[index].execution, span)?;
                indexed_finish(payload, span)?;
                self.push_static_arguments(
                    index,
                    function,
                    kind,
                    args,
                    0,
                    Vec::new(),
                    value_span,
                    next,
                )?;
            }
            _ => {
                let flow = self.with_lent_context(index, |evaluator, execution, slots| {
                    evaluator.eval_indexed_expr(execution, instruction, slots, span)
                })?;
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
        if matches!(value, FrameValue::Break(LoweredValue::ResultErr(_)))
            && matches!(
                self.evaluator.pending_value_block_flow,
                None | Some(StmtFlow::Propagate(_))
            )
            && let Some((else_param_slot, else_body, span)) = with_initializer_handler(&next)
        {
            let FrameValue::Break(LoweredValue::ResultErr(error)) = value else {
                unreachable!("checked propagated Result error")
            };
            self.evaluator.pending_value_block_flow = None;
            self.evaluator.pending_traceback = None;
            if let Some(slot) = else_param_slot {
                self.calls[index].slots[slot] = LoweredValue::Error(error);
            }
            return self.push_statement_block(index, else_body, span);
        }
        if let Some(flow) = self.evaluator.pending_value_block_flow.take() {
            return match flow {
                StmtFlow::Break(value) => self.break_loop(index, value),
                StmtFlow::Continue => self.continue_loop(index),
                flow => self.complete_call(index, flow),
            };
        }
        match next {
            FrameContinuation::Field { name, span, next } => match value {
                FrameValue::Value(base) => {
                    let value = self.evaluator.indexed_field_value(base, &name, span)?;
                    self.push_value(index, FrameValue::Value(value), *next);
                }
                FrameValue::Break(value) => self.push_value(index, FrameValue::Break(value), *next),
            },
            FrameContinuation::IndexBase {
                instruction,
                span,
                next,
            } => match value {
                FrameValue::Value(base) => self.push_expr(
                    index,
                    instruction,
                    span,
                    FrameContinuation::IndexValue { base, span, next },
                ),
                FrameValue::Break(value) => self.push_value(index, FrameValue::Break(value), *next),
            },
            FrameContinuation::IndexValue { base, span, next } => match value {
                FrameValue::Value(value) => {
                    let value = super::lowered_index_value(base, value, span)?;
                    self.push_value(index, FrameValue::Value(value), *next);
                }
                FrameValue::Break(value) => self.push_value(index, FrameValue::Break(value), *next),
            },
            FrameContinuation::ModuleArguments {
                op,
                cli_plan,
                args,
                position,
                mut values,
                span,
                next,
            } => match value {
                FrameValue::Value(value) => {
                    values.push(Some(value));
                    self.step_module_arguments(
                        index,
                        op,
                        cli_plan,
                        args,
                        position + 1,
                        values,
                        span,
                        *next,
                    )?;
                }
                FrameValue::Break(value) => self.push_value(index, FrameValue::Break(value), *next),
            },
            FrameContinuation::WithBinding {
                bindings,
                position,
                body,
                else_param_slot,
                else_body,
                span,
            } => {
                let value = match value {
                    FrameValue::Value(value) => value,
                    FrameValue::Break(value) => {
                        return self.complete_call(index, StmtFlow::Propagate(value));
                    }
                };
                let value = match value {
                    LoweredValue::ResultErr(error) => {
                        self.evaluator.pending_traceback = None;
                        if let Some(slot) = else_param_slot {
                            self.calls[index].slots[slot] = LoweredValue::Error(error);
                        }
                        return self.push_statement_block(index, else_body, span);
                    }
                    LoweredValue::ResultOk(value) => *value,
                    value => value,
                };
                let slot = bindings[position].0;
                self.calls[index].slots[slot] = value;
                self.declare_slot(index, slot);
                if let Some((_, value)) = bindings.get(position + 1).copied() {
                    self.push_expr(
                        index,
                        value,
                        span,
                        FrameContinuation::WithBinding {
                            bindings,
                            position: position + 1,
                            body,
                            else_param_slot,
                            else_body,
                            span,
                        },
                    );
                } else {
                    self.push_statement_block(index, body, span)?;
                }
            }
            FrameContinuation::GuardInput {
                target,
                else_param_slot,
                else_body,
                span,
            } => match value {
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
                FrameValue::Value(LoweredValue::ResultOk(value)) => {
                    self.declare_target(index, &target);
                    bind_lowered_comp_target(&target, *value, &mut self.calls[index].slots, span)?;
                }
                FrameValue::Value(LoweredValue::ResultErr(error)) => {
                    if let Some(slot) = else_param_slot {
                        self.calls[index].slots[slot] = LoweredValue::Error(error);
                    }
                    self.calls[index]
                        .work
                        .push(FrameWork::GuardFailureEnd(span));
                    self.calls[index]
                        .work
                        .push(FrameWork::ClearSlots(else_param_slot.into_iter().collect()));
                    self.push_statement_block(index, else_body, span)?;
                }
                FrameValue::Value(other) => {
                    return Err(RuntimeError::new(
                        "type-error",
                        format!("guard expected Result, found {}", other.type_name()),
                    )
                    .with_span(span));
                }
            },
            FrameContinuation::ParameterDefault {
                slot,
                kind,
                check,
                span,
            } => match value {
                FrameValue::Value(value) => {
                    validate_parameter_default(&value, kind, check.as_ref(), span)?;
                    self.calls[index].slots[slot] = value;
                    self.declare_slot(index, slot);
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::Store(slot) => match value {
                FrameValue::Value(value) => {
                    self.calls[index].slots[slot] = value;
                    self.declare_slot(index, slot);
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::StoreTarget { target, span } => match value {
                FrameValue::Value(value) => {
                    bind_lowered_comp_target(&target, value, &mut self.calls[index].slots, span)?
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::Assign {
                slot,
                op,
                singleton,
                check,
                span,
            } => match value {
                FrameValue::Value(value) => {
                    self.check_context_assignment(index, slot, &value, span)?;
                    let owner_scope = self.calls[index].slot_scope(slot);
                    let source_scope = self.evaluator.current_scope_id();
                    // The target's existing contents already belong to its
                    // scope; only the right side can bring resources in, so a
                    // compound assignment transfers that rather than the whole
                    // accumulated value.
                    let incoming =
                        (op != AssignOp::Set && owner_scope != source_scope).then(|| value.clone());
                    let value = if let Some(check) = check.as_ref() {
                        checked_indexed_assignment(
                            &self.calls[index].slots[slot],
                            op,
                            value,
                            singleton,
                            check,
                            span,
                        )?
                    } else {
                        apply_indexed_assignment(
                            &mut self.calls[index].slots[slot],
                            op,
                            value,
                            singleton,
                            span,
                        )?
                    };
                    self.evaluator
                        .transfer_owned_host_resources_in_lowered_value(
                            incoming.as_ref().unwrap_or(&value),
                            source_scope,
                            owner_scope,
                        );
                    self.calls[index].slots[slot] = value;
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::AssignSelector(mut state) => match value {
                FrameValue::Value(value) => {
                    state
                        .selectors
                        .push(resolve_assign_index(value, state.span)?);
                    self.advance_assign_path(index, state);
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::AssignPath(state) => match value {
                FrameValue::Value(value) => {
                    self.check_context_assignment(index, state.slot, &value, state.span)?;
                    let (source_scope, owner_scope) = (
                        self.evaluator.current_scope_id(),
                        self.calls[index].slot_scope(state.slot),
                    );
                    let incoming = (owner_scope != source_scope).then(|| value.clone());
                    apply_indexed_path_assignment(
                        &mut self.calls[index].slots[state.slot],
                        &state.selectors,
                        state.op,
                        value,
                        state.singleton,
                        state.check.as_ref(),
                        state.span,
                    )?;
                    if let Some(incoming) = incoming {
                        self.evaluator
                            .transfer_owned_host_resources_in_lowered_value(
                                &incoming,
                                source_scope,
                                owner_scope,
                            );
                    }
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::Return => {
                return self.complete_call(
                    index,
                    match value {
                        FrameValue::Value(value) => StmtFlow::Return(value),
                        FrameValue::Break(value) => StmtFlow::Propagate(value),
                    },
                );
            }
            FrameContinuation::BlockValue => {
                return match value {
                    FrameValue::Value(value) => self.complete_expression_value(index, value),
                    FrameValue::Break(value) => {
                        self.complete_call(index, StmtFlow::Propagate(value))
                    }
                };
            }
            FrameContinuation::ContextScopeEntry {
                kind,
                body,
                span,
                next,
            } => match value {
                FrameValue::Value(value) => match self
                    .evaluator
                    .enter_indexed_context_scope(kind, value, span)
                {
                    Ok(restore) => {
                        self.calls[index].work.push(FrameWork::ExpressionBoundary {
                            policy: ExpressionBoundaryPolicy::Scope(restore),
                            next: *next,
                        });
                        self.push_statement_block(index, body, span)?;
                    }
                    Err(error) => self.push_value(
                        index,
                        FrameValue::Value(lowered_result_err_value(error)),
                        *next,
                    ),
                },
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::ErrorContextEntry { body, span, next } => match value {
                FrameValue::Value(value) => {
                    let description =
                        super::lowered_str_arg_owned(Some(value), "", "ctx description", span)?;
                    let context = crate::runtime::value::ErrorContext {
                        kind: "ctx".to_string(),
                        message: Some(description),
                        span: Some(span),
                    };
                    self.calls[index].work.push(FrameWork::ExpressionBoundary {
                        policy: ExpressionBoundaryPolicy::Context(context),
                        next: *next,
                    });
                    self.push_statement_block(index, body, span)?;
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
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
            FrameContinuation::ComparisonLeft { pairs, next } => match value {
                FrameValue::Value(left) => {
                    let (_, right, span) = pairs[0];
                    self.push_expr(
                        index,
                        right,
                        span,
                        FrameContinuation::ComparisonRight {
                            left,
                            pairs,
                            position: 0,
                            next,
                        },
                    );
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::ComparisonRight {
                left,
                pairs,
                position,
                next,
            } => match value {
                FrameValue::Value(right) => {
                    let (op, _, span) = pairs[position];
                    let holds = comparison_link_holds(op, &left, &right, span)?;
                    if !holds || position + 1 == pairs.len() {
                        self.push_value(index, FrameValue::Value(LoweredValue::Bool(holds)), *next);
                    } else {
                        let position = position + 1;
                        let (_, operand, span) = pairs[position];
                        self.push_expr(
                            index,
                            operand,
                            span,
                            FrameContinuation::ComparisonRight {
                                left: right,
                                pairs,
                                position,
                                next,
                            },
                        );
                    }
                }
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
                    let left = lowered_condition_bool(left, span)?;
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
                    return self.complete_call(index, StmtFlow::Propagate(value));
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
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::BoolBinaryRight { next, span } => match value {
                FrameValue::Value(value) => self.push_value(
                    index,
                    FrameValue::Value(LoweredValue::Bool(lowered_condition_bool(value, span)?)),
                    *next,
                ),
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
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
                    if lowered_condition_bool(value, span)? {
                        let value = branches[branch].1;
                        self.evaluator.frame_scratch.recycle_if_branches(branches);
                        self.push_expr(index, value, span, *next);
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
                            self.evaluator.frame_scratch.recycle_if_branches(branches);
                            self.push_expr(index, else_value, span, *next);
                        }
                    }
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::StatementIf {
                branches,
                index: branch,
                else_body,
                span,
            } => match value {
                FrameValue::Value(value) => {
                    if lowered_condition_bool(value, span)? {
                        let body = branches[branch].1;
                        self.evaluator.frame_scratch.recycle_if_branches(branches);
                        self.push_statement_block(index, body, span)?;
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
                            self.evaluator.frame_scratch.recycle_if_branches(branches);
                            self.push_statement_block(index, body, span)?;
                        } else {
                            self.evaluator.frame_scratch.recycle_if_branches(branches);
                        }
                    }
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::PatternIf {
                branches,
                index: branch,
                else_body,
                span,
            } => match value {
                FrameValue::Value(value) => {
                    if lowered_condition_bool(value, span)? {
                        self.push_statement_block(index, branches[branch].1, span)?;
                    } else {
                        self.close_pattern_scope(index)?;
                        self.start_pattern_branch(index, branches, branch + 1, else_body, span)?;
                    }
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::PatternWhile { body, span } => match value {
                FrameValue::Value(value) => {
                    if lowered_condition_bool(value, span)? {
                        self.push_statement_block(index, body, span)?;
                    } else {
                        self.close_pattern_scope(index)?;
                        let rearmed = self.calls[index].work.pop();
                        debug_assert!(matches!(rearmed, Some(FrameWork::PatternWhile { .. })));
                    }
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::ForItems { target, body, span } => match value {
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
                            target,
                            stream: *stream,
                            body,
                            span,
                        });
                        return Ok(());
                    }
                    let value = match LoweredScalarCursor::try_new(value) {
                        Ok(cursor) => {
                            self.calls[index].work.push(FrameWork::ForScalars {
                                target,
                                cursor,
                                body,
                                span,
                            });
                            return Ok(());
                        }
                        Err(value) => value,
                    };
                    if let LoweredValue::Map(entries) = value {
                        self.calls[index].work.push(FrameWork::ForMap {
                            target,
                            cursor: LoweredMapCursor::new(entries),
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
                        target,
                        items,
                        index: 0,
                        body,
                        span,
                    });
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::ForPipelineInput {
                slot,
                body,
                span,
                stages,
            } => match value {
                FrameValue::Value(input) => {
                    let pipeline = {
                        let call = &mut self.calls[index];
                        IndexedSerialPipeline::new(
                            self.evaluator,
                            &call.execution,
                            input,
                            stages,
                            &mut call.slots,
                            span,
                            call.call_span,
                        )?
                    };
                    match pipeline {
                        ControlFlow::Continue(pipeline) => {
                            self.calls[index].work.push(FrameWork::ForPipeline {
                                slot,
                                pipeline: Box::new(pipeline),
                                body,
                                span,
                            });
                        }
                        ControlFlow::Break(value) => {
                            return self.complete_expression_escape(index, value);
                        }
                    }
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
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
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::While {
                condition,
                body,
                span,
            } => match value {
                FrameValue::Value(value) => {
                    if lowered_condition_bool(value, span)? {
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
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::MatchValue { arms, span } => match value {
                FrameValue::Value(value) => self.select_match_arm(index, arms, 0, value, span)?,
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::MatchGuard {
                arms,
                index: arm_index,
                value: match_value,
                span,
            } => match value {
                FrameValue::Value(value) => {
                    if lowered_condition_bool(value, span)? {
                        self.push_statement_block(index, arms[arm_index].2, span)?;
                    } else {
                        self.select_match_arm(index, arms, arm_index + 1, match_value, span)?;
                    }
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::MatchExprValue {
                arms, next, span, ..
            } => match value {
                FrameValue::Value(value) => {
                    self.select_expr_match_arm(index, arms, 0, value, span, *next)?;
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
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
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::BreakLoop => match value {
                FrameValue::Value(value) => return self.break_loop(index, Some(value)),
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::DynamicCallee { args, span, next } => match value {
                FrameValue::Value(callee) => {
                    self.push_dynamic_arguments(index, callee, args, 0, Vec::new(), span, *next)?
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::DynamicArguments {
                callee,
                args,
                argument,
                mut values,
                span,
                next,
            } => match value {
                FrameValue::Value(value) => {
                    append_call_argument(&mut values, args[argument].0, value, span)?;
                    self.push_dynamic_arguments(
                        index,
                        callee,
                        args,
                        argument + 1,
                        values,
                        span,
                        *next,
                    )?;
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
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
                    append_call_argument(&mut values, args[argument].0, value, span)?;
                    self.push_static_arguments(
                        index,
                        function,
                        kind,
                        args,
                        argument + 1,
                        values,
                        span,
                        *next,
                    )?;
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::WrapOk(next) => match value {
                FrameValue::Value(value) => self.push_value(
                    index,
                    FrameValue::Value(LoweredValue::ResultOk(Box::new(value))),
                    *next,
                ),
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::WrapErr { cause, span, next } => match value {
                FrameValue::Value(value) => {
                    let error = value.into_value();
                    if let Some(cause) = cause {
                        self.push_expr(
                            index,
                            cause,
                            span,
                            FrameContinuation::AttachErrCause { error, span, next },
                        );
                    } else {
                        self.push_value(
                            index,
                            FrameValue::Value(lowered_err_with_cause(error, None, span)?),
                            *next,
                        );
                    }
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::AttachErrCause { error, span, next } => match value {
                FrameValue::Value(cause) => self.push_value(
                    index,
                    FrameValue::Value(lowered_err_with_cause(error, Some(cause), span)?),
                    *next,
                ),
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::Try { span, next } => match value {
                FrameValue::Value(value) => {
                    match self.evaluator.indexed_question_value(value, span)? {
                        Ok(value) => self.push_value(index, FrameValue::Value(value), *next),
                        Err(value) => self.push_value(index, FrameValue::Break(value), *next),
                    }
                }
                FrameValue::Break(value) => self.push_value(index, FrameValue::Break(value), *next),
            },
            FrameContinuation::CheckedValue { check, span, next } => match value {
                FrameValue::Value(value) => {
                    super::super::checked_unsigned_value(&value, &check, span)?;
                    self.push_value(index, FrameValue::Value(value), *next);
                }
                FrameValue::Break(value) => self.push_value(index, FrameValue::Break(value), *next),
            },
            FrameContinuation::Require { check, span, next } => match value {
                FrameValue::Value(value) => {
                    let value = super::super::super::require::require_value(
                        self.evaluator,
                        value,
                        &check,
                        span,
                    );
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
                    return self.complete_call(index, StmtFlow::Propagate(value));
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
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::FmtValue {
                mut state,
                span,
                spec,
                next,
            } => match value {
                FrameValue::Value(value) => {
                    state.fmt.push_value(&value, span, spec.as_ref())?;
                    self.step_fmt(index, state, *next)?;
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::ResultFallback { right, span, next } => match value {
                FrameValue::Value(value) => match lowered_fallback_value(value) {
                    Some(value) => self.push_value(index, FrameValue::Value(value), *next),
                    None => self.push_expr(index, right, span, *next),
                },
                FrameValue::Break(value) => self.push_value(index, FrameValue::Break(value), *next),
            },
            FrameContinuation::ListItems {
                items,
                index: item_index,
                mut values,
                next,
            } => match value {
                FrameValue::Value(value) => {
                    let (_, splice, item_span) = items[item_index];
                    append_lowered_list_element(&mut values, value, splice, item_span)?;
                    if let Some(&(instruction, _, span)) = items.get(item_index + 1) {
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
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::MapLiteralItems {
                entries,
                index: entry_index,
                mut fields,
                key,
                reading_key,
                next,
            } => match value {
                FrameValue::Value(value) => {
                    let (_, value_instruction, span) = entries[entry_index];
                    if reading_key {
                        let key = Some(lowered_map_literal_key(&value, span)?);
                        self.push_expr(
                            index,
                            value_instruction,
                            span,
                            FrameContinuation::MapLiteralItems {
                                entries,
                                index: entry_index,
                                fields,
                                key,
                                reading_key: false,
                                next,
                            },
                        );
                    } else {
                        append_lowered_map_literal(&mut fields, key, value, span)?;
                        if let Some(&(key, value, span)) = entries.get(entry_index + 1) {
                            self.push_expr(
                                index,
                                key.unwrap_or(value),
                                span,
                                FrameContinuation::MapLiteralItems {
                                    entries,
                                    index: entry_index + 1,
                                    fields,
                                    key: None,
                                    reading_key: key.is_some(),
                                    next,
                                },
                            );
                        } else {
                            self.push_value(
                                index,
                                FrameValue::Value(LoweredValue::Map(Arc::new(fields))),
                                *next,
                            );
                        }
                    }
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::RecordUpdateBase {
                updates,
                span,
                next,
            } => match value {
                FrameValue::Value(base) => {
                    if let Some((_, instruction, field_span)) = updates.first() {
                        self.push_expr(
                            index,
                            *instruction,
                            *field_span,
                            FrameContinuation::RecordUpdateItems {
                                base,
                                updates,
                                index: 0,
                                values: Vec::new(),
                                span,
                                next,
                            },
                        );
                    } else {
                        self.push_value(index, FrameValue::Value(base), *next);
                    }
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::RecordUpdateItems {
                base,
                updates,
                index: item_index,
                mut values,
                span,
                next,
            } => match value {
                FrameValue::Value(value) => {
                    let (path, _, field_span) = &updates[item_index];
                    values.push((path.clone(), value, *field_span));
                    if let Some((_, instruction, field_span)) = updates.get(item_index + 1) {
                        self.push_expr(
                            index,
                            *instruction,
                            *field_span,
                            FrameContinuation::RecordUpdateItems {
                                base,
                                updates,
                                index: item_index + 1,
                                values,
                                span,
                                next,
                            },
                        );
                    } else {
                        self.push_value(
                            index,
                            FrameValue::Value(lowered_record_update_batch(base, values, span)?),
                            *next,
                        );
                    }
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
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
                    entries[entry_index].append(&mut fields, value, span)?;
                    if let Some(entry) = entries.get(entry_index + 1) {
                        let instruction = entry.instruction();
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
                        self.push_value(
                            index,
                            FrameValue::Value(finish_record_entries(fields)),
                            *next,
                        );
                    }
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::ListCompIter { mut state, next } => match value {
                FrameValue::Value(value) => {
                    let iterable = lowered_comp_iterable(value, state.span)?;
                    let clause = state.cursor;
                    let iterable = match LoweredScalarCursor::try_new(iterable) {
                        Ok(cursor) => {
                            state
                                .iterators
                                .push(CompIterator::Scalars { cursor, clause });
                            return self.step_list_comp(index, *state, *next);
                        }
                        Err(iterable) => iterable,
                    };
                    if let LoweredValue::Stream(stream) = iterable {
                        let stream_index = {
                            let mut streams = state
                                .streams
                                .lock()
                                .expect("comprehension stream state poisoned");
                            let slot = streams
                                .iter()
                                .position(Option::is_none)
                                .unwrap_or(streams.len());
                            if slot == streams.len() {
                                streams.push(None);
                            }
                            streams[slot] = Some((*stream, state.qualifiers[clause].span()));
                            slot
                        };
                        state.iterators.push(CompIterator::Stream {
                            stream: stream_index,
                            clause,
                        });
                    } else if let LoweredValue::Map(entries) = iterable {
                        state.iterators.push(CompIterator::Map {
                            cursor: LoweredMapCursor::new(entries),
                            clause,
                        });
                    } else {
                        let items = self.evaluator.lowered_list_items(
                            iterable,
                            state.qualifiers[clause].span(),
                            "comprehension expected List or Stream",
                        )?;
                        state.iterators.push(CompIterator::Items {
                            items: items.into_iter(),
                            clause,
                        });
                    }
                    self.step_list_comp(index, *state, *next)?;
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::ListCompCondition { mut state, next } => match value {
                FrameValue::Value(value) => {
                    if lowered_condition_bool(value, state.qualifiers[state.cursor].span())? {
                        state.cursor += 1;
                        self.step_comp_qualifier(index, *state, *next)?;
                    } else {
                        self.step_list_comp(index, *state, *next)?;
                    }
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::ListCompKey { state, next } => match value {
                FrameValue::Value(value) => {
                    let key = lowered_map_literal_key(&value, state.span)?;
                    self.push_expr(
                        index,
                        state.value,
                        state.span,
                        FrameContinuation::ListCompValue {
                            state,
                            key: Some(key),
                            next,
                        },
                    );
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::YieldDelegate { span } => match value {
                FrameValue::Value(value) => {
                    self.suspended = Some(ProducerSuspension::Delegated { value, span });
                    return Ok(());
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::Yield => match value {
                FrameValue::Value(value) => {
                    if self.calls[index].work.iter().any(|work| {
                        matches!(
                            work,
                            FrameWork::ExpressionBoundary {
                                policy: ExpressionBoundaryPolicy::Scope(_),
                                ..
                            }
                        )
                    }) && Evaluator::context_scope_value_escapes(&value)
                    {
                        return Err(RuntimeError::new(
                            "context-scope-escape",
                            "a live producer or host handle cannot escape through yield",
                        )
                        .with_span(self.calls[index].call_span));
                    }
                    // The frame keeps everything after this statement on its
                    // work stack; the puller receives the value.
                    self.suspended = Some(ProducerSuspension::Yielded(value));
                    return Ok(());
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
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
                    self.step_list_comp(index, *state, *next)?;
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
        }
        Ok(())
    }

    fn complete_expression_escape(
        &mut self,
        index: usize,
        value: LoweredValue,
    ) -> Result<(), RuntimeError> {
        match self
            .evaluator
            .pending_value_block_flow
            .take()
            .unwrap_or(StmtFlow::Propagate(value))
        {
            StmtFlow::Value(value) => self.complete_expression_value(index, value),
            StmtFlow::Break(value) => self.break_loop(index, value),
            StmtFlow::Continue => self.continue_loop(index),
            flow => self.complete_call(index, flow),
        }
    }

    fn check_context_assignment(
        &self,
        index: usize,
        slot: usize,
        value: &LoweredValue,
        span: Span,
    ) -> Result<(), RuntimeError> {
        if !Evaluator::context_scope_value_escapes(value) {
            return Ok(());
        }
        let call = &self.calls[index];
        let owner = call.slot_scope(slot);
        let Some(boundary) = call.work.iter().rposition(|work| {
            matches!(
                work,
                FrameWork::ExpressionBoundary {
                    policy: ExpressionBoundaryPolicy::Scope(_),
                    ..
                }
            )
        }) else {
            return match call.owner {
                FrameOwner::Block {
                    context_depth: Some(depth),
                    ..
                } => self.evaluator.check_context_slot_owner(owner, depth, span),
                _ => Ok(()),
            };
        };
        if call.work[boundary + 1..].iter().any(|work| {
            matches!(work,
            FrameWork::Statements { scope_id: Some(scope), .. } if *scope == owner)
        }) {
            return Ok(());
        }
        Err(context_assignment_escape(span))
    }

    /// The slot ownership a delegated evaluation needs when this frame is
    /// inside a context scope, whether its own or the one that lent its slots.
    fn lent_context_slots(&self, index: usize) -> Option<LentContextSlots> {
        let call = &self.calls[index];
        let depth = match call.work.iter().rposition(|work| {
            matches!(
                work,
                FrameWork::ExpressionBoundary {
                    policy: ExpressionBoundaryPolicy::Scope(_),
                    ..
                }
            )
        }) {
            Some(boundary) => {
                let scope_ids = &self.evaluator.scope_ids;
                call.work[boundary + 1..]
                    .iter()
                    .find_map(|work| match work {
                        FrameWork::Statements {
                            scope_id: Some(scope),
                            ..
                        } => scope_ids.iter().rposition(|id| id == scope),
                        _ => None,
                    })
                    .unwrap_or(scope_ids.len())
            }
            None => match call.owner {
                FrameOwner::Block {
                    context_depth: Some(depth),
                    ..
                } => depth,
                _ => return None,
            },
        };
        Some(LentContextSlots {
            slots: call.slots.as_ptr() as usize,
            depth,
            scopes: (0..call.slots.len())
                .map(|slot| call.slot_scope(slot))
                .collect(),
        })
    }

    /// Runs `delegate` on the recursive evaluator with this frame's slots,
    /// lending it the frame's context-scope slot ownership.
    fn with_lent_context<T>(
        &mut self,
        index: usize,
        delegate: impl FnOnce(&mut Evaluator, &FullExecution<'p>, &mut [LoweredValue]) -> T,
    ) -> T {
        let lent = self.lent_context_slots(index);
        let previous = lent
            .is_some()
            .then(|| std::mem::replace(&mut self.evaluator.lent_context_slots, lent));
        let call = &mut self.calls[index];
        let result = delegate(self.evaluator, &call.execution, &mut call.slots);
        if let Some(previous) = previous {
            self.evaluator.lent_context_slots = previous;
        }
        result
    }

    /// Records a slot declared in the current scope.
    fn declare_slot(&mut self, index: usize, slot: usize) {
        let scope = self.evaluator.current_scope_id();
        let call = &mut self.calls[index];
        if slot >= call.slot_scopes.len() {
            let fill = call.slot_scope(slot);
            call.slot_scopes.resize(slot + 1, fill);
        }
        call.slot_scopes[slot] = scope;
    }

    fn declare_target(&mut self, index: usize, target: &LoweredCompTarget) {
        match target {
            LoweredCompTarget::Slot(slot) => self.declare_slot(index, *slot),
            LoweredCompTarget::Record { fields } => {
                for (_, target, _) in fields {
                    self.declare_target(index, target);
                }
            }
            LoweredCompTarget::Discard => {}
        }
    }

    fn complete_expression_value(
        &mut self,
        index: usize,
        value: LoweredValue,
    ) -> Result<(), RuntimeError> {
        let Some(boundary) = self.calls[index]
            .work
            .iter()
            .rposition(|work| matches!(work, FrameWork::ExpressionBoundary { .. }))
        else {
            return self.complete_call(index, StmtFlow::Value(value));
        };
        if matches!(
            &self.calls[index].work[boundary],
            FrameWork::ExpressionBoundary {
                policy: ExpressionBoundaryPolicy::Scope(_),
                ..
            }
        ) && Evaluator::context_scope_value_escapes(&value)
        {
            return Err(RuntimeError::new(
                "context-scope-escape",
                "a live producer or host handle cannot escape a restored context",
            )
            .with_span(self.calls[index].call_span));
        }
        self.evaluator
            .transfer_owned_host_resources_in_lowered_value(
                &value,
                self.evaluator.current_scope_id(),
                self.evaluator.parent_owned_host_scope(),
            );
        self.discard_work_from(index, boundary + 1)?;
        let Some(FrameWork::ExpressionBoundary { policy, next }) = self.calls[index].work.pop()
        else {
            unreachable!()
        };
        let value = match policy {
            ExpressionBoundaryPolicy::Capture => LoweredValue::ResultOk(Box::new(value)),
            ExpressionBoundaryPolicy::Scope(restore) => {
                self.evaluator.restore_indexed_context_scope(restore);
                lowered_result_ok(value)
            }
            _ => value,
        };
        self.push_value(index, FrameValue::Value(value), next);
        Ok(())
    }

    fn complete_call(&mut self, index: usize, mut flow: StmtFlow) -> Result<(), RuntimeError> {
        // A body that ran to its end, or returned with nothing left on its work
        // stack, has no boundaries, contexts, or block scopes to unwind.
        if self.calls[index].work.is_empty() && !matches!(flow, StmtFlow::Propagate(_)) {
            if self.calls[index].defers.is_empty() {
                return self.finish_call(index, flow);
            }
            self.calls[index].work.push(FrameWork::Finish(flow));
            return Ok(());
        }
        if let StmtFlow::Propagate(value) = &flow
            && let Some(boundary) = self.capture_boundary(index)
        {
            if self.crosses_context_scope(index, boundary + 1)
                && Evaluator::context_scope_value_escapes(value)
            {
                self.evaluator.pending_traceback = None;
                return Err(RuntimeError::new(
                    "context-scope-escape",
                    "a live producer or host handle cannot escape a restored context",
                )
                .with_span(self.calls[index].call_span));
            }
            let survivor = self.boundary_survivor_scope(index, boundary);
            for source in self.discarded_statement_scopes(index, boundary + 1) {
                self.evaluator
                    .transfer_owned_host_resources_in_lowered_value(value, source, survivor);
            }
            let contexts = self.calls[index].work[boundary + 1..]
                .iter()
                .rev()
                .filter_map(|work| match work {
                    FrameWork::ExpressionBoundary {
                        policy: ExpressionBoundaryPolicy::Context(context),
                        ..
                    } => Some(context.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let cleanup = self.discard_work_from_with_primary(index, boundary + 1, true);
            if let Err(error) = cleanup {
                if error.abort.as_ref().is_some_and(|signal| signal.force) {
                    return Err(error);
                }
                self.evaluator
                    .report_cleanup_error(&error, self.calls[index].call_span);
            }
            let Some(FrameWork::ExpressionBoundary { next, .. }) = self.calls[index].work.pop()
            else {
                unreachable!()
            };
            let value = contexts
                .into_iter()
                .fold(value.clone(), contextualize_propagation);
            self.evaluator.pending_traceback = None;
            self.push_value(index, FrameValue::Value(value), next);
            return Ok(());
        }
        let contexts: Vec<_> = self.calls[index]
            .work
            .iter()
            .rev()
            .filter_map(|work| match work {
                FrameWork::ExpressionBoundary {
                    policy: ExpressionBoundaryPolicy::Context(context),
                    ..
                } => Some(context.clone()),
                _ => None,
            })
            .collect();
        if let StmtFlow::Value(value)
        | StmtFlow::Return(value)
        | StmtFlow::Propagate(value)
        | StmtFlow::Break(Some(value)) = &flow
        {
            if self.calls[index].work.iter().any(|work| {
                matches!(
                    work,
                    FrameWork::ExpressionBoundary {
                        policy: ExpressionBoundaryPolicy::Scope(_),
                        ..
                    }
                )
            }) && Evaluator::context_scope_value_escapes(value)
            {
                self.evaluator.pending_traceback = None;
                return Err(RuntimeError::new(
                    "context-scope-escape",
                    "a live producer or host handle cannot escape a restored context",
                )
                .with_span(self.calls[index].call_span));
            }
            let function_scope = self.calls[index].scope_id;
            for source in self.discarded_statement_scopes(index, 0) {
                self.evaluator
                    .transfer_owned_host_resources_in_lowered_value(value, source, function_scope);
            }
        }
        // Lexical exits and checked failures retain resources from every
        // discarded block before cleanup runs in the registering scopes.
        let cleanup = self.discard_work_from_with_primary(
            index,
            0,
            matches!(
                flow,
                StmtFlow::Propagate(_) | StmtFlow::Return(LoweredValue::ResultErr(_))
            ),
        );
        if cleanup
            .as_ref()
            .err()
            .is_some_and(|error| error.abort.as_ref().is_some_and(|signal| signal.force))
        {
            return Err(cleanup.expect_err("forced cleanup abort"));
        }
        if matches!(
            flow,
            StmtFlow::Propagate(_) | StmtFlow::Return(LoweredValue::ResultErr(_))
        ) {
            if let Err(error) = cleanup {
                self.evaluator
                    .report_cleanup_error(&error, self.calls[index].call_span);
            }
        } else {
            cleanup?;
        }
        if let StmtFlow::Propagate(value) = flow {
            let value = contexts.into_iter().fold(value, contextualize_propagation);
            update_context_traceback(self.evaluator, &value);
            flow = StmtFlow::Propagate(value);
        }
        if self.calls[index].defers.is_empty() {
            self.finish_call(index, flow)
        } else {
            self.calls[index].work.push(FrameWork::Finish(flow));
            Ok(())
        }
    }

    fn cleanup_comp_streams(&mut self, streams: CompStreams) -> Result<(), RuntimeError> {
        let mut first_error = None;
        for stream in streams
            .lock()
            .expect("comprehension stream state poisoned")
            .iter_mut()
            .rev()
        {
            if let Some((mut stream, span)) = stream.take()
                && let Err(error) = self.evaluator.stream_cancel(&mut stream, span)
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    fn step_comp_qualifier(
        &mut self,
        index: usize,
        state: ListCompState,
        next: FrameContinuation,
    ) -> Result<(), RuntimeError> {
        match state.qualifiers.get(state.cursor).cloned() {
            Some(IndexedCompQualifier::For { iter, span, .. }) => self.push_expr(
                index,
                iter,
                span,
                FrameContinuation::ListCompIter {
                    state: Box::new(state),
                    next: Box::new(next),
                },
            ),
            Some(IndexedCompQualifier::If { condition, span }) => self.push_expr(
                index,
                condition,
                span,
                FrameContinuation::ListCompCondition {
                    state: Box::new(state),
                    next: Box::new(next),
                },
            ),
            None => return self.push_list_comp_projection(index, state, next),
        }
        Ok(())
    }

    fn step_list_comp(
        &mut self,
        index: usize,
        mut state: ListCompState,
        next: FrameContinuation,
    ) -> Result<(), RuntimeError> {
        while let Some(iterator) = state.iterators.last_mut() {
            let (item, clause) = match iterator {
                CompIterator::Items { items, clause } => (items.next(), *clause),
                CompIterator::Map { cursor, clause } => (cursor.next(), *clause),
                CompIterator::Scalars { cursor, clause } => {
                    self.evaluator
                        .service_pending_signal(state.qualifiers[*clause].span())?;
                    if self.evaluator.signal_state.shutdown_complete {
                        (None, *clause)
                    } else {
                        (cursor.next(), *clause)
                    }
                }
                CompIterator::Stream { stream, clause } => {
                    self.evaluator
                        .service_pending_signal(state.qualifiers[*clause].span())?;
                    let mut streams = state
                        .streams
                        .lock()
                        .expect("comprehension stream state poisoned");
                    let (producer, span) = streams[*stream]
                        .as_mut()
                        .expect("active comprehension stream");
                    let item = self.evaluator.stream_next(producer, *span)?;
                    let item = item
                        .map(|item| {
                            lowered_value_from_runtime_any(&item).ok_or_else(|| {
                                RuntimeError::new(
                                    "type-error",
                                    "stream produced unsupported comprehension item",
                                )
                                .with_span(*span)
                            })
                        })
                        .transpose()?;
                    if item.is_none() {
                        streams[*stream] = None;
                    }
                    (item, *clause)
                }
            };
            if let Some(item) = item {
                let IndexedCompQualifier::For { target, span, .. } = &state.qualifiers[clause]
                else {
                    unreachable!("iterator belongs to for clause")
                };
                bind_lowered_comp_target(target, item, &mut self.calls[index].slots, *span)?;
                state.cursor = clause + 1;
                return self.step_comp_qualifier(index, state, next);
            }
            state.iterators.pop();
        }
        // The comprehension's cleanup sits directly below its own work, so it
        // retires here instead of lingering under the consumer's
        // continuation, where it would break work-stack shapes that pattern
        // conditions and other consumers pop back to.
        let Some(FrameWork::CompCleanup(streams)) = self.calls[index].work.pop() else {
            unreachable!("finished comprehension owns its stream cleanup");
        };
        debug_assert!(Arc::ptr_eq(&streams, &state.streams));
        self.cleanup_comp_streams(streams)?;
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

    // Argument holes preserve omitted native defaults; present operands run once
    // in source order without retaining recursive evaluator frames across calls.
    fn step_module_arguments(
        &mut self,
        index: usize,
        op: super::RuntimeOp,
        cli_plan: Option<Arc<crate::modules::cli::CliDescriptorPlan>>,
        args: Vec<Option<u32>>,
        mut position: usize,
        mut values: Vec<Option<LoweredValue>>,
        span: Span,
        next: FrameContinuation,
    ) -> Result<(), RuntimeError> {
        while let Some(argument) = args.get(position).copied() {
            if let Some(instruction) = argument {
                self.push_expr(
                    index,
                    instruction,
                    span,
                    FrameContinuation::ModuleArguments {
                        op,
                        cli_plan,
                        args,
                        position,
                        values,
                        span,
                        next: Box::new(next),
                    },
                );
                return Ok(());
            }
            values.push(None);
            position += 1;
        }
        let flow = self.evaluator.eval_indexed_module_call_values(
            op,
            super::super::NativeArgumentValues::new(values),
            span,
            cli_plan.as_deref(),
        )?;
        self.push_value(
            index,
            match flow {
                ControlFlow::Continue(value) => FrameValue::Value(value),
                ControlFlow::Break(value) => FrameValue::Break(value),
            },
            next,
        );
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
        let result = match self.evaluator.eval_indexed_method_dispatch(
            receiver,
            name.as_ref(),
            values,
            span,
        )? {
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
        while let Some(part) = state.parts.get(state.index).cloned() {
            state.index += 1;
            match part {
                IndexedFmtPart::Text(text) => state.fmt.push_text(&text),
                IndexedFmtPart::Expr(instruction, span, spec) => {
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
        self.push_value(index, FrameValue::Value(state.fmt.finish()?), next);
        Ok(())
    }

    fn finish_deferred_call(&mut self, index: usize, flow: StmtFlow) -> Result<(), RuntimeError> {
        let previous_contexts = self.install_cleanup_contexts();
        let defers = std::mem::take(&mut self.calls[index].defers);
        let call = &mut self.calls[index];
        let cleanup = self.evaluator.run_indexed_defers(
            &call.execution,
            &defers,
            &mut call.slots,
            call.call_span,
        );
        let cleanup = if cleanup
            .as_ref()
            .err()
            .is_some_and(|error| error.abort.as_ref().is_some_and(|signal| signal.force))
        {
            cleanup
        } else if matches!(
            flow,
            StmtFlow::Propagate(_) | StmtFlow::Return(LoweredValue::ResultErr(_))
        ) {
            if let Err(error) = cleanup {
                self.evaluator
                    .report_cleanup_error(&error, self.calls[index].call_span);
            }
            Ok(())
        } else {
            cleanup
        };
        self.evaluator.cleanup_error_contexts = previous_contexts;
        cleanup?;
        self.finish_call(index, flow)
    }

    fn finish_error_deferred_call(&mut self, index: usize) -> Result<(), RuntimeError> {
        let previous_contexts = self.install_cleanup_contexts();
        let defers = std::mem::take(&mut self.calls[index].defers);
        let call = &mut self.calls[index];
        let cleanup = self.evaluator.run_indexed_defers(
            &call.execution,
            &defers,
            &mut call.slots,
            call.call_span,
        );
        let cleanup = match cleanup {
            Err(error) if error.abort.as_ref().is_some_and(|signal| signal.force) => Err(error),
            Err(error) => {
                self.evaluator
                    .report_cleanup_error(&error, self.calls[index].call_span);
                Ok(())
            }
            Ok(()) => Ok(()),
        };
        self.evaluator.cleanup_error_contexts = previous_contexts;
        cleanup?;
        self.finish_error_call(index)
    }

    fn finish_error_call(&mut self, index: usize) -> Result<(), RuntimeError> {
        debug_assert_eq!(index, self.calls.len() - 1);
        let mut call = self.calls.pop().expect("active indexed frame");
        if let FrameOwner::Function(function, kind) = call.owner
            && let Ok(header) = self.function_header(function, kind, call.call_span)
        {
            let _ =
                self.evaluator
                    .write_back_lowered_captures(&header, &call.slots, call.call_span);
        }
        if let Some(error) = self.pending_error.as_ref()
            && error.abort.is_none()
            && error.propagated
            && call.owns_scope()
        {
            let parent = self.evaluator.parent_owned_host_scope();
            self.evaluator
                .transfer_owned_host_resources_in_runtime_error(error, call.scope_id, parent);
        }
        // An active error remains primary; cleanup failure is intentionally
        // secondary, but the scope still must release its owned resources.
        let _ = cleanup_call_scopes(self.evaluator, &mut call);
        if let FrameOwner::Function(function, kind) = call.owner {
            self.release_slots(call.slots);
            self.evaluator.call_stack.pop();
            let exit_kind = match kind {
                LoweredFunctionKind::Pure => TraceKind::PureExit,
                LoweredFunctionKind::Proc => TraceKind::ProcExit,
            };
            if self.evaluator.trace_enabled {
                let name = function.display_name();
                self.evaluator.trace_exit_with_definition(
                    exit_kind,
                    Some(call.call_span),
                    Some(call.definition_span),
                    Some(&name),
                    TracePayload::None,
                );
            }
        }
        if let Some(parent) = self.calls.len().checked_sub(1) {
            self.unwind_error_frame(parent)?;
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
        let FrameOwner::Function(function, kind) = self.calls[index].owner else {
            return self.finish_block(index, flow);
        };
        let mut call = self.calls.pop().expect("active indexed frame");
        // The frame's own vectors are done with; the returned value has already
        // been taken out of `slots`.
        self.evaluator.frame_scratch.recycle(&mut call);
        let header = self.function_header(function, kind, call.call_span)?;
        // Producer failures leave the stream boundary as runtime errors. Keep
        // the original propagation location when converting the Result value.
        let return_span = if call.producer {
            self.evaluator
                .pending_traceback
                .as_ref()
                .and_then(|traceback| traceback.failing_span)
                .unwrap_or(call.call_span)
        } else {
            call.call_span
        };
        let value = match flow {
            StmtFlow::Return(LoweredValue::Unit) if call.producer => Ok(LoweredValue::Unit),
            StmtFlow::Value(value) | StmtFlow::Return(value) | StmtFlow::Propagate(value) => {
                super::super::checked_lowered_return_value(&header, value, return_span)
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
        if let Err(error) = &value
            && error.abort.is_none()
            && error.propagated
        {
            let parent = self.evaluator.parent_owned_host_scope();
            self.evaluator
                .transfer_owned_host_resources_in_runtime_error(error, call.scope_id, parent);
        }
        let cleanup = cleanup_call_scopes(self.evaluator, &mut call);
        self.evaluator.frame_scratch.recycle_block_stacks(&mut call);
        self.release_slots(call.slots);
        let exit_kind = match kind {
            LoweredFunctionKind::Pure => TraceKind::PureExit,
            LoweredFunctionKind::Proc => TraceKind::ProcExit,
        };
        self.evaluator.call_stack.pop();
        if self.evaluator.trace_enabled {
            let name = function.display_name();
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
                // A callee may have written a script binding back; a block
                // frame running top-level statements reads it from its slots.
                if let FrameOwner::Block { .. } = self.calls[parent].owner {
                    let call = &mut self.calls[parent];
                    self.evaluator
                        .sync_indexed_root_slots(&mut call.slots, call.call_span)?;
                }
                self.push_value(parent, FrameValue::Value(value), next);
            }
            (Some(_), Err(error)) | (None, Err(error)) => self.begin_error_unwind(error),
            (None, Ok(value)) => self.result = Some(Ok(value)),
        }
        Ok(())
    }

    fn release_slots(&mut self, slots: FrameSlots<'p>) {
        if let FrameSlots::Owned(slots) = slots {
            self.evaluator.recycle_lowered_slots(slots);
        }
    }

    fn function_header(
        &self,
        function: LoweredFunctionKey,
        kind: LoweredFunctionKind,
        span: Span,
    ) -> Result<Arc<FunctionHeader>, RuntimeError> {
        self.program
            .function_view(function, kind)
            .map_err(|error| indexed_error(error, span))?
            .ok_or_else(|| {
                RuntimeError::new("unresolved-lowered-call", function.display_name())
                    .with_span(span)
            })?
            .header()
            .map_err(|error| indexed_error(error, span))
    }

    /// Hands a finished block's flow back to the recursive evaluator. Outgoing
    /// values and checked failures keep their resources in the parent scope;
    /// a cleanup failure after a checked failure is reported, not raised.
    /// The frame is finished where it lies on the stack and dropped there.
    fn finish_block(&mut self, index: usize, flow: StmtFlow) -> Result<(), RuntimeError> {
        let call = &mut self.calls[index];
        self.evaluator.frame_scratch.recycle(call);
        if call.owns_scope()
            && let StmtFlow::Value(value)
            | StmtFlow::Return(value)
            | StmtFlow::Propagate(value)
            | StmtFlow::Break(Some(value)) = &flow
        {
            let parent = self.evaluator.parent_owned_host_scope();
            self.evaluator
                .transfer_owned_host_resources_in_lowered_value(value, call.scope_id, parent);
        }
        let cleanup = cleanup_call_scopes(self.evaluator, call);
        self.evaluator.frame_scratch.recycle_block_stacks(call);
        let call_span = call.call_span;
        self.calls.truncate(index);
        self.result = Some(match cleanup {
            Err(error) if error.abort.as_ref().is_some_and(|signal| signal.force) => Err(error),
            Err(error)
                if matches!(
                    flow,
                    StmtFlow::Propagate(_) | StmtFlow::Return(LoweredValue::ResultErr(_))
                ) =>
            {
                self.evaluator.report_cleanup_error(&error, call_span);
                Ok(LoweredValue::Unit)
            }
            Err(error) => Err(error),
            Ok(()) => Ok(LoweredValue::Unit),
        });
        self.block_flow = Some(flow);
        Ok(())
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
        target: LoweredCompTarget,
        items: Vec<LoweredValue>,
        item_index: usize,
        body: u32,
        span: Span,
    ) -> Result<(), RuntimeError> {
        self.evaluator.service_pending_signal(span)?;
        if self.evaluator.signal_state.shutdown_complete || item_index == items.len() {
            return Ok(());
        }
        bind_lowered_comp_target(
            &target,
            items[item_index].clone(),
            &mut self.calls[index].slots,
            span,
        )?;
        self.calls[index].work.push(FrameWork::ForItems {
            target,
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
                ControlFlow::Break(value) => self.complete_expression_escape(index, value),
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

    /// The innermost loop in the frame. A block frame without one hands the
    /// loop control back to the recursive evaluator that entered it.
    fn innermost_loop(&self, index: usize) -> Option<usize> {
        self.calls[index].work.iter().rposition(|work| {
            matches!(
                work,
                FrameWork::ForScalars { .. }
                    | FrameWork::ForMap { .. }
                    | FrameWork::ForItems { .. }
                    | FrameWork::ForStream { .. }
                    | FrameWork::ForPipeline { .. }
                    | FrameWork::ForStrLines { .. }
                    | FrameWork::While { .. }
                    | FrameWork::Loop { .. }
                    | FrameWork::PatternWhile { .. }
            )
        })
    }

    fn break_loop(
        &mut self,
        index: usize,
        value: Option<LoweredValue>,
    ) -> Result<(), RuntimeError> {
        match self.innermost_loop(index) {
            Some(loop_index) => self.discard_work_from(index, loop_index),
            None if matches!(self.calls[index].owner, FrameOwner::Block { .. }) => {
                self.complete_call(index, StmtFlow::Break(value))
            }
            None => Err(RuntimeError::new("control-flow", "break outside loop")
                .with_span(self.calls[index].call_span)),
        }
    }

    fn step_for_stream(
        &mut self,
        index: usize,
        target: LoweredCompTarget,
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
        if let Err(error) =
            bind_lowered_comp_target(&target, item, &mut self.calls[index].slots, span)
        {
            self.evaluator.stream_cancel(&mut stream, span)?;
            return Err(error);
        }
        // Re-arm before running the body, so a `continue` reaches the next item
        // and a `break` discards this item and stops the producer.
        self.calls[index].work.push(FrameWork::ForStream {
            target,
            stream,
            body,
            span,
        });
        self.push_statement_block(index, body, span)?;
        Ok(())
    }

    fn step_for_pipeline(
        &mut self,
        index: usize,
        slot: usize,
        mut pipeline: Box<IndexedSerialPipeline>,
        body: u32,
        span: Span,
    ) -> Result<(), RuntimeError> {
        if let Err(error) = self.evaluator.service_pending_signal(span) {
            let _ = pipeline.finish(self.evaluator, Some(&error));
            return Err(error);
        }
        if self.evaluator.shutting_down() {
            return pipeline.finish(self.evaluator, None);
        }
        let pulled = {
            let call = &mut self.calls[index];
            pipeline.next(self.evaluator, &call.execution, &mut call.slots)
        };
        match pulled {
            Ok(ControlFlow::Continue(Some(item))) => {
                self.calls[index].slots[slot] = item;
                self.calls[index].work.push(FrameWork::ForPipeline {
                    slot,
                    pipeline,
                    body,
                    span,
                });
                self.push_statement_block(index, body, span)
            }
            Ok(ControlFlow::Continue(None)) => pipeline.finish(self.evaluator, None),
            Ok(ControlFlow::Break(value)) => {
                pipeline.finish(self.evaluator, None)?;
                self.complete_call(index, StmtFlow::Return(value))
            }
            Err(error) => {
                let _ = pipeline.finish(self.evaluator, Some(&error));
                Err(error)
            }
        }
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
        match self.innermost_loop(index) {
            Some(loop_index) => self.discard_work_from(index, loop_index + 1),
            None if matches!(self.calls[index].owner, FrameOwner::Block { .. }) => {
                self.complete_call(index, StmtFlow::Continue)
            }
            None => Err(RuntimeError::new("control-flow", "continue outside loop")
                .with_span(self.calls[index].call_span)),
        }
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

    fn advance_assign_path(&mut self, index: usize, mut state: AssignPathState) {
        while state.position < state.path.len() {
            let step = state.path[state.position].clone();
            state.position += 1;
            match step {
                IndexedAssignStep::Field(name) => {
                    state.selectors.push(ResolvedAssignStep::Field(name))
                }
                IndexedAssignStep::Index(expr) => {
                    self.push_expr(
                        index,
                        expr,
                        state.span,
                        FrameContinuation::AssignSelector(state),
                    );
                    return;
                }
            }
        }
        self.push_expr(
            index,
            state.value,
            state.span,
            FrameContinuation::AssignPath(state),
        );
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

    /// Opens the owned scope of one pattern condition and its body: the
    /// captures clear and the scope closes when the work above them finishes
    /// or unwinds.
    fn open_pattern_scope(&mut self, index: usize, captures: Vec<usize>) {
        let scope_id = self.evaluator.enter_owned_host_scope();
        let defer_offset = self.calls[index].defers.len();
        self.calls[index].block_scopes.push(scope_id);
        self.calls[index].block_defer_offsets.push(defer_offset);
        self.calls[index].work.push(FrameWork::Statements {
            statements: Vec::new(),
            complete_call: false,
            scope_id: Some(scope_id),
        });
        self.calls[index].work.push(FrameWork::ClearSlots(captures));
    }

    /// Closes the scope `open_pattern_scope` left on top of the work stack
    /// after a condition that did not match.
    fn close_pattern_scope(&mut self, index: usize) -> Result<(), RuntimeError> {
        let Some(FrameWork::ClearSlots(captures)) = self.calls[index].work.pop() else {
            unreachable!("pattern condition owns its capture cleanup");
        };
        for slot in captures {
            self.calls[index].slots[slot] = LoweredValue::Unit;
        }
        let Some(FrameWork::Statements {
            statements,
            scope_id: Some(scope_id),
            ..
        }) = self.calls[index].work.pop()
        else {
            unreachable!("pattern condition owns its scope");
        };
        self.evaluator.frame_scratch.recycle_statements(statements);
        self.exit_block_scope(index, scope_id, true, CleanupFailureResources::Retain)
    }

    fn start_pattern_branch(
        &mut self,
        index: usize,
        mut branches: Vec<(u32, u32, Vec<usize>)>,
        branch: usize,
        else_body: Option<u32>,
        span: Span,
    ) -> Result<(), RuntimeError> {
        let Some((condition, _, captures)) = branches.get_mut(branch) else {
            return match else_body {
                Some(body) => self.push_statement_block(index, body, span),
                None => Ok(()),
            };
        };
        let (condition, captures) = (*condition, std::mem::take(captures));
        self.open_pattern_scope(index, captures);
        self.push_expr(
            index,
            condition,
            span,
            FrameContinuation::PatternIf {
                branches,
                index: branch,
                else_body,
                span,
            },
        );
        Ok(())
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
        let defer_offset = self.calls[index].defers.len();
        if self.calls[index].block_scopes.capacity() == 0 {
            let call = &mut self.calls[index];
            (call.block_scopes, call.block_defer_offsets) =
                self.evaluator.frame_scratch.take_block_stacks();
        }
        self.calls[index].block_scopes.push(scope_id);
        self.calls[index].block_defer_offsets.push(defer_offset);
        self.calls[index].work.push(FrameWork::Statements {
            statements,
            complete_call: false,
            scope_id: Some(scope_id),
        });
        Ok(())
    }

    fn exit_block_scope(
        &mut self,
        index: usize,
        scope_id: u64,
        include_work_contexts: bool,
        mut disposition: CleanupFailureResources,
    ) -> Result<(), RuntimeError> {
        let defer_offset = self.calls[index]
            .block_defer_offsets
            .pop()
            .expect("live block owns a defer boundary");
        if self.calls[index].defers.len() == defer_offset {
            let popped = self.calls[index].block_scopes.pop();
            debug_assert_eq!(popped, Some(scope_id));
            return self.evaluator.exit_owned_host_scope(scope_id);
        }
        let defers = self.calls[index].defers.split_off(defer_offset);
        let previous_contexts =
            (include_work_contexts && !defers.is_empty()).then(|| self.install_cleanup_contexts());
        let call = &mut self.calls[index];
        let mut cleanup = self.evaluator.run_indexed_defers(
            &call.execution,
            &defers,
            &mut call.slots,
            call.call_span,
        );
        if include_work_contexts
            && disposition == CleanupFailureResources::Retain
            && cleanup.is_err()
        {
            let keep = self
                .capture_boundary(index)
                .map_or(0, |boundary| boundary + 1);
            if self.crosses_context_scope(index, keep) {
                disposition = CleanupFailureResources::RejectContextEscape;
            }
        }
        if let Err(error) = &cleanup
            && error.abort.is_none()
            && error.propagated
        {
            if disposition == CleanupFailureResources::RejectContextEscape
                && Evaluator::context_scope_runtime_error_escapes(error)
            {
                self.evaluator.pending_traceback = None;
                cleanup = Err(RuntimeError::new(
                    "context-scope-escape",
                    "a live producer or host handle cannot escape a restored context",
                )
                .with_span(self.calls[index].call_span));
            } else if disposition == CleanupFailureResources::Retain {
                let parent = self.evaluator.parent_owned_host_scope();
                self.evaluator
                    .transfer_owned_host_resources_in_runtime_error(error, scope_id, parent);
            }
        }
        let popped = self.calls[index].block_scopes.pop();
        debug_assert_eq!(popped, Some(scope_id));
        let host_cleanup = self.evaluator.exit_owned_host_scope(scope_id);
        let result = match (cleanup, host_cleanup) {
            (Err(error), Err(secondary)) => {
                self.evaluator
                    .report_cleanup_error(&secondary, self.calls[index].call_span);
                Err(error)
            }
            (Err(error), _) | (_, Err(error)) => Err(error),
            _ => Ok(()),
        };
        if let Some(previous) = previous_contexts {
            self.evaluator.cleanup_error_contexts = previous;
        }
        result
    }

    /// Abandoned work unwinds every lexical cleanup action from innermost to outermost.
    fn install_cleanup_contexts(&mut self) -> Vec<crate::runtime::value::ErrorContext> {
        let previous = self.evaluator.cleanup_error_contexts.clone();
        self.evaluator.cleanup_error_contexts.extend(
            self.calls
                .iter()
                .flat_map(|call| call.work.iter())
                .filter_map(|work| match work {
                    FrameWork::ExpressionBoundary {
                        policy: ExpressionBoundaryPolicy::Context(context),
                        ..
                    } => Some(context.clone()),
                    _ => None,
                }),
        );
        previous
    }

    fn discard_work_from(&mut self, index: usize, keep: usize) -> Result<(), RuntimeError> {
        self.discard_work_from_with_primary(index, keep, self.pending_error.is_some())
    }

    fn discard_work_from_with_primary(
        &mut self,
        index: usize,
        keep: usize,
        primary_failed: bool,
    ) -> Result<(), RuntimeError> {
        if self.calls[index].work.len() <= keep {
            return Ok(());
        }
        let previous_contexts = self.install_cleanup_contexts();
        let mut context_scopes = self.calls[index].work[keep..]
            .iter()
            .filter(|work| {
                matches!(
                    work,
                    FrameWork::ExpressionBoundary {
                        policy: ExpressionBoundaryPolicy::Scope(_),
                        ..
                    }
                )
            })
            .count();
        let mut first_error: Option<RuntimeError> = None;
        // Discarded work is popped in place, innermost first; nothing it runs
        // reads this frame's work stack, and the stack keeps its capacity.
        while self.calls[index].work.len() > keep {
            let work = self.calls[index].work.pop().expect("discarded frame work");
            if !primary_failed
                && context_scopes > 0
                && first_error
                    .as_ref()
                    .is_some_and(Evaluator::context_scope_runtime_error_escapes)
            {
                self.evaluator.pending_traceback = None;
                first_error = Some(
                    RuntimeError::new(
                        "context-scope-escape",
                        "a live producer or host handle cannot escape a restored context",
                    )
                    .with_span(self.calls[index].call_span),
                );
            }
            let result = match work {
                FrameWork::ClearSlots(slots) => {
                    for slot in slots {
                        self.calls[index].slots[slot] = LoweredValue::Unit;
                    }
                    Ok(())
                }
                FrameWork::ExpressionBoundary {
                    policy: ExpressionBoundaryPolicy::Context(context),
                    ..
                } => {
                    self.evaluator.cleanup_error_contexts.pop();
                    if let Some(error) = self.pending_error.take() {
                        self.pending_error =
                            Some(contextualize_runtime_error(error, context.clone()));
                    }
                    if let Some(error) = first_error.take() {
                        first_error = Some(contextualize_runtime_error(error, context));
                    }
                    if let Some(error) = &self.pending_error
                        && let Some(traceback) = &mut self.evaluator.pending_traceback
                    {
                        traceback.error = crate::trace::TraceError::from_runtime_error(error);
                    }
                    Ok(())
                }
                FrameWork::ExpressionBoundary {
                    policy: ExpressionBoundaryPolicy::Scope(restore),
                    ..
                } => {
                    context_scopes -= 1;
                    self.evaluator.restore_indexed_context_scope(restore);
                    Ok(())
                }
                FrameWork::CompCleanup(streams) => self.cleanup_comp_streams(streams),
                FrameWork::Statements {
                    statements,
                    scope_id,
                    ..
                } => {
                    self.evaluator.frame_scratch.recycle_statements(statements);
                    let Some(scope_id) = scope_id else { continue };
                    if !primary_failed
                        && let Some(error) = &first_error
                        && error.abort.is_none()
                        && error.propagated
                    {
                        let parent = self.evaluator.parent_owned_host_scope();
                        self.evaluator
                            .transfer_owned_host_resources_in_runtime_error(
                                error, scope_id, parent,
                            );
                    }
                    let disposition = if primary_failed || first_error.is_some() {
                        CleanupFailureResources::Release
                    } else if context_scopes > 0 {
                        CleanupFailureResources::RejectContextEscape
                    } else {
                        CleanupFailureResources::Retain
                    };
                    self.exit_block_scope(index, scope_id, false, disposition)
                }
                FrameWork::ForStream {
                    mut stream, span, ..
                } => self.evaluator.stream_cancel(&mut stream, span),
                FrameWork::ForPipeline { mut pipeline, .. } => {
                    pipeline.finish(self.evaluator, self.pending_error.as_ref())
                }
                _ => Ok(()),
            };
            if let Err(error) = result {
                if error.abort.as_ref().is_some_and(|signal| signal.force) {
                    while self.calls[index].work.len() > keep {
                        match self.calls[index].work.pop().expect("discarded frame work") {
                            FrameWork::Statements {
                                scope_id: Some(scope_id),
                                ..
                            } => {
                                if self.calls[index].block_scopes.last() == Some(&scope_id) {
                                    self.calls[index].block_scopes.pop();
                                    self.calls[index].block_defer_offsets.pop();
                                    let _ = self.evaluator.exit_owned_host_scope(scope_id);
                                }
                            }
                            FrameWork::ExpressionBoundary {
                                policy: ExpressionBoundaryPolicy::Scope(restore),
                                ..
                            } => self.evaluator.restore_indexed_context_scope(restore),
                            _ => {}
                        }
                    }
                    self.evaluator.cleanup_error_contexts = previous_contexts;
                    return Err(error);
                }
                if first_error.is_none() {
                    first_error = Some(error);
                } else {
                    self.evaluator
                        .report_cleanup_error(&error, self.calls[index].call_span);
                }
            }
        }
        self.evaluator.cleanup_error_contexts = previous_contexts;
        first_error.map_or(Ok(()), Err)
    }
}

fn cleanup_call_scopes(
    evaluator: &mut Evaluator,
    call: &mut CallFrame<'_>,
) -> Result<(), RuntimeError> {
    let mut first_error = None;
    while let Some(scope_id) = call.block_scopes.pop() {
        if let Err(error) = evaluator.exit_owned_host_scope(scope_id)
            && first_error.is_none()
        {
            first_error = Some(error);
        }
    }
    if call.owns_scope()
        && let Err(error) = evaluator.exit_owned_host_scope(call.scope_id)
        && first_error.is_none()
    {
        first_error = Some(error);
    }
    first_error.map_or(Ok(()), Err)
}

fn contextualize_propagation(
    value: LoweredValue,
    context: crate::runtime::value::ErrorContext,
) -> LoweredValue {
    match value {
        LoweredValue::ResultErr(error) => LoweredValue::ResultErr(Box::new(
            crate::runtime::eval::add_error_context(*error, context),
        )),
        other => LoweredValue::Error(Box::new(crate::runtime::eval::add_error_context(
            other.into_value(),
            context,
        ))),
    }
}

fn contextualize_runtime_error(
    error: RuntimeError,
    context: crate::runtime::value::ErrorContext,
) -> RuntimeError {
    if error.abort.is_some() {
        return error;
    }
    let crate::runtime::value::Value::Error(error) = crate::runtime::eval::add_error_context(
        crate::runtime::value::Value::Error(Box::new(error)),
        context,
    ) else {
        unreachable!()
    };
    *error
}

fn update_context_traceback(evaluator: &mut Evaluator, value: &LoweredValue) {
    if let Some(traceback) = &mut evaluator.pending_traceback {
        let error = match value {
            LoweredValue::ResultErr(error) | LoweredValue::Error(error) => error.as_ref(),
            _ => return,
        };
        traceback.error = crate::trace::TraceError::from_value(error);
    }
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

pub(super) fn decode_call_args_into<'a>(
    execution: &FullExecution<'a>,
    payload: &mut FullPayload<'a>,
    span: Span,
    values: &mut Vec<(u32, u32)>,
) -> Result<(), RuntimeError> {
    let (_, mut args) = execution
        .block(payload, BLOCK_LIST)
        .map_err(|error| indexed_error(error, span))?;
    let len = indexed_raw(&mut args, span)? as usize;
    values.clear();
    values.reserve(len);
    for _ in 0..len {
        values.push((indexed_raw(&mut args, span)?, indexed_raw(&mut args, span)?));
    }
    indexed_finish(args, span)?;
    Ok(())
}
