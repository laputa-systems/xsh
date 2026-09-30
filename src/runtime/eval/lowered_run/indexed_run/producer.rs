//! Resumable script producers.
//!
//! A `stream` function's body is a continuation: everything it still has to do
//! lives on its frame's work stack, and a `yield` is the only place it stops.
//! `ScriptProducer` owns that state between pulls — the prepared code identity
//! and the program that owns it, the bound arguments and local slots, the
//! lexical scopes the body opened, and the defer bodies it registered — and each
//! pull resumes it against the evaluator that is consuming the stream.
//!
//! Nothing here evaluates a body with a second interpreter or on a worker
//! thread: a pull runs the *existing* frame engine over the stored frame, for
//! exactly the work between two yields, and puts the frame back.

use super::explicit_run::{
    CallFrame, ExplicitFrames, ProducerFrameState, ProducerStep, decode_statements,
};
use super::{
    Arc, Evaluator, LoweredFunctionKey, LoweredFunctionKind, LoweredValue, RuntimeError, Span,
    StreamValue, TraceKind, TracePayload, TracebackFrame, TracebackFrameKind,
};
use crate::runtime::value::{ScriptStreamState, ScriptStreamStep};

/// A producer suspended between pulls.
pub(super) struct ScriptProducer {
    /// The prepared program the body's code identity belongs to. Holding it
    /// keeps the identity valid for the producer's whole life.
    program: Arc<super::FullProgram>,
    function: LoweredFunctionKey,
    kind: LoweredFunctionKind,
    call_span: Span,
    definition_span: Span,
    /// The body's remaining work, its slots, its registered defers, and the
    /// lexical scopes it opened. `None` only while a step owns it.
    frame: Option<ProducerFrameState>,
    /// Whether the body has started. A producer whose body never started has no
    /// defers to run and no scopes to close.
    started: bool,
    finished: bool,
    /// A delegated child retains its one-shot cursor while this frame suspends.
    /// A proc that returns a stream uses the same cursor after its frame ends.
    delegated: Option<DelegatedSource>,
    context: Option<crate::runtime::eval::ScopedProducerContext>,
}

/// A single active source; List and Stream cursors cannot coexist.
enum DelegatedSource {
    List(DelegatedList),
    Stream {
        value: Box<StreamValue>,
        prefix: std::collections::VecDeque<super::Value>,
        span: Span,
    },
}

impl DelegatedSource {
    fn new(value: LoweredValue, span: Span) -> Result<Self, RuntimeError> {
        match value {
            LoweredValue::List(items) => Ok(Self::List(DelegatedList::Owned(items.into_iter()))),
            LoweredValue::SharedList(items) => Ok(Self::List(DelegatedList::Shared { items, next: 0 })),
            LoweredValue::Stream(mut value) => {
                let prefix = std::mem::take(&mut value.items).into_iter().map(|item| item.value).collect();
                Ok(Self::Stream { value, prefix, span })
            }
            _ => Err(RuntimeError::new("type-error", "yield delegation requires List or Stream").with_span(span)),
        }
    }
}

/// List delegation retains shared storage and clones only the current item.
enum DelegatedList {
    Owned(std::vec::IntoIter<LoweredValue>),
    Shared { items: Arc<Vec<LoweredValue>>, next: usize },
}

impl DelegatedList {
    fn next(&mut self) -> Option<LoweredValue> {
        match self {
            Self::Owned(items) => items.next(),
            Self::Shared { items, next } => {
                let value = items.get(*next)?.clone();
                *next += 1;
                Some(value)
            }
        }
    }
}

/// The stream value's view of a producer: it can resume it and stop it, and the
/// evaluator doing the consuming is the one the pull runs against.
impl ScriptProducer {
    /// Whether the body has ended or been stopped.
    fn is_finished(&self) -> bool {
        self.finished && self.delegated.is_none()
    }
}

impl crate::runtime::value::ScriptStream for ScriptProducer {
    fn finished(&self) -> bool {
        self.is_finished()
    }

    fn poll(
        &mut self,
        evaluator: &mut Evaluator,
        span: Span,
    ) -> Result<ScriptStreamStep, RuntimeError> {
        evaluator.pull_script_producer(self, span)
    }

    fn delegated_finished(&mut self) {
        self.delegated = None;
    }

    fn take_delegated(&mut self) -> (Option<ScriptStreamState>, Vec<u64>, Option<crate::runtime::eval::ScopedProducerContext>) {
        let child = self.delegated.take().and_then(|source| match source {
            DelegatedSource::Stream { value, .. } => value.script().cloned(),
            DelegatedSource::List(_) => None,
        });
        let scopes = if self.started && !self.finished {
            self.frame.as_ref().expect("suspended producer frame").open_scopes()
        } else { Vec::new() };
        (child, scopes, self.context.clone())
    }

    fn cancel(&mut self, evaluator: &mut Evaluator, span: Span) -> Result<(), RuntimeError> {
        evaluator.cancel_script_producer(self, span)
    }
}

impl Evaluator {
    /// Start a producer without running its body.
    ///
    /// The arguments are bound and the captures hydrated now — those are part of
    /// the call, not of the body — and the body's statements are left on the
    /// frame's work stack for the first pull to execute.
    pub(super) fn start_script_producer(
        &mut self,
        function: LoweredFunctionKey,
        kind: LoweredFunctionKind,
        view: super::FullFunctionView<'_>,
        slots: Vec<LoweredValue>,
        call_span: Span,
    ) -> Result<ScriptStreamState, RuntimeError> {
        let program = Arc::clone(
            self.indexed_program
                .as_ref()
                .expect("indexed caller retains its indexed program"),
        );
        let execution = view
            .execution()
            .map_err(|error| super::indexed_error(error, call_span))?;
        let (_, body) = view
            .body(&execution)
            .map_err(|error| super::indexed_error(error, call_span))?;
        let statements = decode_statements(body, call_span)?;
        let definition_span = view
            .definition_span()
            .map_err(|error| super::indexed_error(error, call_span))?;
        // The call scope is entered on the first pull, so a producer whose body
        // never starts never owns one.
        let frame = ProducerFrameState::begin_body(statements, slots);
        let state = ScriptStreamState::new(ScriptProducer {
            program,
            function,
            kind,
            call_span,
            definition_span,
            frame: Some(frame),
            started: false,
            finished: false,
            delegated: None,
            context: None,
        });
        // The evaluator keeps a handle so a producer the program can no longer
        // reach can still be stopped, which is what runs its defers.
        self.script_producers.push(state.clone());
        Ok(state)
    }

    /// Stops unreachable producers at a point with no statement span at hand.
    pub(in crate::runtime::eval) fn sweep_script_producers_at_rest(&mut self) {
        let span = Span::new(crate::source::SourceId::new(0), 0, 0);
        let _ = self.sweep_script_producers(span);
    }

    /// Stops the producers the program can no longer reach.
    ///
    /// A producer is reachable while a slot, argument, container, or the pull in
    /// progress holds a handle to it; once only this registry holds one, nothing
    /// can ever resume the body, so its `defer` bodies and host scopes are
    /// released here. Finished producers are simply forgotten.
    pub(in crate::runtime::eval) fn sweep_script_producers(
        &mut self,
        span: Span,
    ) -> Result<(), RuntimeError> {
        if self.script_producers.is_empty() {
            return Ok(());
        }
        let pending = std::mem::take(&mut self.script_producers);
        let mut kept = Vec::with_capacity(pending.len());
        let mut first_error = None;
        for state in pending {
            match state.try_finished() {
                // A pull owns the state: it is running, not abandoned.
                None => {
                    kept.push(state);
                    continue;
                }
                Some(true) => continue,
                Some(false) => {}
            }
            if !state.only_registry_holds() {
                kept.push(state);
                continue;
            }
            if let Err(error) = self.cancel_script_state(state.clone(), span) {
                first_error.get_or_insert(error);
            }
        }
        self.script_producers = kept;
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    /// Resume the frame or hand a retained child to the pull driver.
    pub(super) fn pull_script_producer(
        &mut self, producer: &mut ScriptProducer, span: Span,
    ) -> Result<ScriptStreamStep, RuntimeError> {
        let consumer = self.producer_context();
        let resumed_context = producer.context.take();
        let isolated = resumed_context.is_some();
        if let Some(context) = resumed_context { self.swap_producer_context(context); }
        let result = self.pull_script_producer_active(producer, span);
        let suspended_scope = producer.frame.as_ref().is_some_and(ProducerFrameState::has_context_scope);
        if suspended_scope { producer.context = Some(self.producer_context()); }
        if isolated || suspended_scope { self.swap_producer_context(consumer); }
        result
    }

    fn pull_script_producer_active(
        &mut self,
        producer: &mut ScriptProducer,
        span: Span,
    ) -> Result<ScriptStreamStep, RuntimeError> {
        loop {
            if let Some(step) = self.poll_delegated(producer)? { return Ok(step); }
            if producer.finished { return Ok(ScriptStreamStep::Finished); }
            let mut just_started = false;
            if !producer.started {
                producer.started = true;
                just_started = true;
                let scope_id = self.enter_owned_host_scope();
                producer
                    .frame
                    .as_mut()
                    .expect("a suspended producer owns its frame")
                    .start(scope_id);
                let (frame_kind, enter_kind) = match producer.kind {
                    LoweredFunctionKind::Pure => (TracebackFrameKind::Pure, TraceKind::PureEnter),
                    LoweredFunctionKind::Proc => (TracebackFrameKind::Proc, TraceKind::ProcEnter),
                };
                if self.trace_enabled {
                    let name = producer.function.display_name();
                    self.trace_enter_with_definition(
                        enter_kind,
                        Some(producer.call_span),
                        Some(producer.definition_span),
                        Some(&name),
                        TracePayload::None,
                    );
                }
                self.call_stack.push(TracebackFrame {
                    kind: frame_kind,
                    name: producer.function.traceback_name(),
                    definition_span: Some(producer.definition_span),
                    call_span: Some(producer.call_span),
                });
            }
            // The consumer may be inside scopes of its own, so the body's scopes are
            // reattached for this pull and detached again if it suspends. A finished
            // or stopped body has already closed them.
            let scopes = producer
                .frame
                .as_ref()
                .expect("a suspended producer owns its frame")
                .open_scopes();
            if !just_started {
                self.reattach_owned_host_scopes(&scopes);
            }
            let program = Arc::clone(&producer.program);
            // The body's nested calls resolve against the program the body belongs
            // to, which is not necessarily the one the consumer is running.
            let previous_program = self.indexed_program.replace(Arc::clone(&program));
            let call = CallFrame::from_state(
                program.as_ref(),
                producer.function,
                producer.kind,
                producer.call_span,
                producer.definition_span,
                producer
                    .frame
                    .take()
                    .expect("a suspended producer owns its frame"),
            )
            .map_err(|error| super::indexed_error(error, span))?
            .ok_or_else(|| {
                RuntimeError::new("unresolved-lowered-call", "a suspended producer's function")
                    .with_span(span)
            })?;
            let mut frames = ExplicitFrames::new(self, program.as_ref());
            let step = frames.run_producer(call);
            self.indexed_program = previous_program;
            match step {
                ProducerStep::Yielded { value, state } => {
                    // The suspension point decides which scopes the body has open,
                    // which may be more than it had at the start of this pull.
                    let open = state.open_scopes().len();
                    self.detach_owned_host_scopes(open);
                    producer.frame = Some(state);
                    return Ok(ScriptStreamStep::Yielded(value.into_value()));
                }
                ProducerStep::Delegated { value, span, state } => {
                    self.detach_owned_host_scopes(state.open_scopes().len());
                    producer.frame = Some(state);
                    match DelegatedSource::new(value, span) {
                        Ok(source) => producer.delegated = Some(source),
                        Err(error) => {
                            let _ = self.cancel_script_producer(producer, span);
                            return Err(error);
                        }
                    }
                }
                ProducerStep::Finished(result) => {
                    // `finish_call` closed the scope on the way out.
                    producer.finished = true;
                    match result {
                        // A body that returns a stream hands the rest of its output
                        // to that stream.
                        Ok(value @ LoweredValue::Stream(_)) => {
                            producer.delegated = Some(DelegatedSource::new(value, span)?);
                        }
                        Ok(_) => return Ok(ScriptStreamStep::Finished),
                        Err(error) => return Err(error),
                    }
                }
            }
        }
    }

    /// Stop a producer early: run the defers its body registered and close the
    /// scopes it opened, exactly once, without running the rest of the body.
    pub(super) fn cancel_script_producer(
        &mut self, producer: &mut ScriptProducer, span: Span,
    ) -> Result<(), RuntimeError> {
        let context = producer.context.take();
        let consumer = context.map(|context| self.swap_producer_context(context));
        let result = self.cancel_script_producer_active(producer, span);
        if let Some(consumer) = consumer { self.swap_producer_context(consumer); }
        result
    }

    fn cancel_script_producer_active(
        &mut self,
        producer: &mut ScriptProducer,
        span: Span,
    ) -> Result<(), RuntimeError> {
        if producer.finished {
            return Ok(());
        }
        producer.finished = true;
        if !producer.started {
            // The body never started, so it registered no defers and the call
            // opened no scope.
            return Ok(());
        }
        let scopes = producer
            .frame
            .as_ref()
            .expect("a started producer owns its frame")
            .open_scopes();
        self.reattach_owned_host_scopes(&scopes);
        let program = Arc::clone(&producer.program);
        let call = CallFrame::from_state(
            program.as_ref(),
            producer.function,
            producer.kind,
            producer.call_span,
            producer.definition_span,
            producer
                .frame
                .take()
                .expect("a started producer owns its frame"),
        )
        .map_err(|error| super::indexed_error(error, span))?
        .ok_or_else(|| {
            RuntimeError::new("unresolved-lowered-call", "a suspended producer's function")
                .with_span(span)
        })?;
        let previous_program = self.indexed_program.replace(Arc::clone(&program));
        let mut frames = ExplicitFrames::new(self, program.as_ref());
        // The cancelled body's `finish_call` closes the scope.
        let outcome = frames.run_cancelled_producer(call, span);
        self.indexed_program = previous_program;
        outcome
    }

    /// Poll the retained source without recursively entering another producer.
    fn poll_delegated(&mut self, producer: &mut ScriptProducer) -> Result<Option<ScriptStreamStep>, RuntimeError> {
        let Some(source) = producer.delegated.as_mut() else { return Ok(None); };
        let (next, span) = match source {
            DelegatedSource::List(items) => (Ok(items.next().map(LoweredValue::into_value)), producer.call_span),
            DelegatedSource::Stream { value, prefix, span } => {
                if let Some(item) = prefix.pop_front() {
                    return Ok(Some(ScriptStreamStep::Yielded(item)));
                }
                if let Some(child) = value.script() {
                    let scopes = if producer.finished { Vec::new() } else {
                        producer.frame.as_ref().expect("suspended producer frame").open_scopes()
                    };
                    return Ok(Some(ScriptStreamStep::Delegate { child: child.clone(), span: *span, scopes, context: producer.frame.as_ref().is_some_and(ProducerFrameState::has_context_scope).then(|| self.producer_context()) }));
                }
                (value.next_live(*span), *span)
            }
        };
        match next {
            Ok(Some(value)) => Ok(Some(ScriptStreamStep::Yielded(value))),
            Ok(None) => { producer.delegated = None; Ok(None) }
            Err(error) => {
                producer.delegated = None;
                let _ = self.cancel_script_producer(producer, span);
                Err(error)
            }
        }
    }

}

/// Runs a producer frame to its next yield, and a cancelled frame to its end.
impl<'a, 'p> ExplicitFrames<'a, 'p> {
    /// Cancels a started producer: nothing left to run but its defers.
    pub(super) fn run_cancelled_producer(
        &mut self,
        mut call: CallFrame<'p>,
        span: Span,
    ) -> Result<(), RuntimeError> {
        call.discard_body();
        match self.run_producer(call) {
            ProducerStep::Finished(Ok(_)) => Ok(()),
            ProducerStep::Finished(Err(error)) => Err(error),
            // The frame's remaining work was discarded, so its body cannot
            // reach another `yield`.
            ProducerStep::Yielded { .. } | ProducerStep::Delegated { .. } => Err(RuntimeError::new(
                "control-flow",
                "a cancelled producer yielded",
            )
            .with_span(span)),
        }
    }
}

/// Process output uses the evaluator's stream cursor so completion checks can
/// fail after yielded rows and cancellation retains the child owner.
pub(super) struct ProcessProducer {
    process: crate::runtime::process::ProcessStream,
    text: bool,
    pending: Vec<u8>,
    completion_error: Option<super::RunError>,
    finished: bool,
    trace: Option<crate::runtime::eval::TraceFrame>,
    span: Span,
}

impl ProcessProducer {
    fn decoded_line(&mut self, bytes: Vec<u8>, evaluator: &mut Evaluator, span: Span) -> Result<ScriptStreamStep, RuntimeError> {
        match String::from_utf8(bytes) {
            Ok(line) => Ok(ScriptStreamStep::Yielded(super::Value::Str(line.into()))),
            Err(_) => {
                let error = super::RunError::new("invalid-utf8", "streamed stdout was not valid UTF-8").with_span(self.span);
                self.process.cancel();
                self.finish(evaluator, Some(error.clone()));
                Err(super::runtime_error_from_value(super::Value::RunError(Box::new(error)), span))
            }
        }
    }

    fn finish(&mut self, evaluator: &mut Evaluator, error: Option<super::RunError>) {
        if !self.finished {
            evaluator.live_process_streams -= 1;
        }
        self.finished = true;
        evaluator.untrack_process_group(self.process.process_group());
        let end = self.process.end(error);
        if let Some(status) = &end.status { evaluator.last_status = Some(status.clone()); }
        if let Some(trace) = self.trace.take() {
            evaluator.event_stack.push(trace);
            evaluator.trace_process_run_end(self.span, &end);
        }
    }
}

impl crate::runtime::value::ScriptStream for ProcessProducer {
    fn finished(&self) -> bool { self.finished }
    fn delegated_finished(&mut self) {}
    fn take_delegated(&mut self) -> (Option<ScriptStreamState>, Vec<u64>, Option<crate::runtime::eval::ScopedProducerContext>) { (None, Vec::new(), None) }
    fn cancel(&mut self, evaluator: &mut Evaluator, _span: Span) -> Result<(), RuntimeError> {
        self.process.cancel();
        self.finish(evaluator, Some(super::RunError::new("canceled", "process stream canceled")));
        Ok(())
    }
    fn poll(&mut self, evaluator: &mut Evaluator, span: Span) -> Result<ScriptStreamStep, RuntimeError> {
        if self.finished { return Ok(ScriptStreamStep::Finished); }
        if let Some(error) = self.completion_error.take() {
            self.process.cancel(); self.finish(evaluator, Some(error.clone()));
            return Err(super::runtime_error_from_value(super::Value::RunError(Box::new(error)), span));
        }
        loop {
            if self.text && let Some(index) = self.pending.iter().position(|byte| *byte == b'\n') {
                let mut line = self.pending.drain(..=index).collect::<Vec<_>>(); line.pop();
                if line.last() == Some(&b'\r') { line.pop(); }
                return self.decoded_line(line, evaluator, span);
            }
            match self.process.next(evaluator) {
                Ok(Some(bytes)) => {
                    if self.text { self.pending.extend(bytes); }
                    else { return Ok(ScriptStreamStep::Yielded(super::Value::Bytes(bytes))); }
                }
                Ok(None) => {
                    let final_row = if self.text && !self.pending.is_empty() {
                        let bytes = std::mem::take(&mut self.pending);
                        Some(self.decoded_line(bytes, evaluator, span)?)
                    } else { None };
                    self.finish(evaluator, None);
                    return Ok(final_row.unwrap_or(ScriptStreamStep::Finished));
                }
                Err(error) => {
                    let error = error.with_span(self.span);
                    if self.text && !self.pending.is_empty() && error.status.is_some() {
                        let bytes = std::mem::take(&mut self.pending);
                        let row = self.decoded_line(bytes, evaluator, span)?;
                        self.completion_error = Some(error);
                        return Ok(row);
                    }
                    self.process.cancel(); self.finish(evaluator, Some(error.clone()));
                    return Err(super::runtime_error_from_value(super::Value::RunError(Box::new(error)), span));
                }
            }
        }
    }
}

impl Evaluator {
    pub(super) fn start_policy_process_stream(&mut self, invocation: &super::ProcessInvocation, text: bool, span: Span) -> Result<LoweredValue, RuntimeError> {
        self.trace_process_run_start(span, invocation);
        let trace = if self.trace_enabled { self.event_stack.pop() } else { None };
        let process = match crate::runtime::process::ProcessStream::start(invocation) {
            Ok(process) => process,
            Err(error) => {
                if let Some(trace) = trace { self.event_stack.push(trace); }
                self.trace_process_run_end(span, &super::ProcessEnd { pid: None, status: error.status.as_deref().cloned(), error: Some(error.clone()) });
                return Ok(super::lowered_process_run_error(error.with_span(span)));
            }
        };
        self.track_process_group(process.process_group());
        self.live_process_streams += 1;
        let state = ScriptStreamState::new(ProcessProducer { process, text, pending: Vec::new(), completion_error: None, finished: false, trace, span });
        self.script_producers.push(state.clone());
        Ok(LoweredValue::ResultOk(Box::new(LoweredValue::Stream(Box::new(StreamValue::from_script(state))))))
    }
}
