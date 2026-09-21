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
use crate::runtime::value::ScriptStreamState;
use super::{
    Arc, Evaluator, LoweredFunctionKey, LoweredFunctionKind, LoweredValue, RuntimeError, Span,
    StreamValue, TraceKind, TracePayload, TracebackFrame, TracebackFrameKind,
};

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
    /// A body that ends by returning another stream hands its remaining items
    /// over to that stream; the producer is finished and drains it instead.
    delegated: Option<Box<StreamValue>>,
}

/// The stream value's view of a producer: it can resume it and stop it, and the
/// evaluator doing the consuming is the one the pull runs against.
impl ScriptProducer {
    /// Whether the body has ended or been stopped.
    fn is_finished(&self) -> bool {
        self.finished
    }
}

impl crate::runtime::value::ScriptStream for ScriptProducer {
    fn finished(&self) -> bool {
        self.is_finished()
    }

    fn pull(
        &mut self,
        evaluator: &mut Evaluator,
        span: Span,
    ) -> Result<Option<super::Value>, RuntimeError> {
        Ok(evaluator
            .pull_script_producer(self, span)?
            .map(super::LoweredValue::into_value))
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
            if let Err(error) = state.lock(span).and_then(|mut p| p.cancel(self, span)) {
                first_error.get_or_insert(error);
            }
        }
        self.script_producers = kept;
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    /// Pull the next item, or `None` when the body has ended.
    pub(super) fn pull_script_producer(
        &mut self,
        producer: &mut ScriptProducer,
        span: Span,
    ) -> Result<Option<LoweredValue>, RuntimeError> {
        if producer.finished {
            return self.pull_delegated(producer, span);
        }
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
                Ok(Some(value))
            }
            ProducerStep::Finished(result) => {
                // `finish_call` closed the scope on the way out.
                producer.finished = true;
                match result {
                    // A body that returns a stream hands the rest of its output
                    // to that stream.
                    Ok(LoweredValue::Stream(stream)) => {
                        producer.delegated = Some(stream);
                        self.pull_delegated(producer, span)
                    }
                    Ok(_) => Ok(None),
                    Err(error) => Err(error),
                }
            }
        }
    }

    /// Stop a producer early: run the defers its body registered and close the
    /// scopes it opened, exactly once, without running the rest of the body.
    pub(super) fn cancel_script_producer(
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

    /// Hand out items from a stream a finished body delegated to.
    fn pull_delegated(
        &mut self,
        producer: &mut ScriptProducer,
        span: Span,
    ) -> Result<Option<LoweredValue>, RuntimeError> {
        let Some(stream) = producer.delegated.as_mut() else {
            return Ok(None);
        };
        if let Some(item) = stream.items.pop() {
            return Ok(super::lowered_value_from_runtime_any(&item.value));
        }
        match stream.next_live(span)? {
            Some(value) => Ok(super::lowered_value_from_runtime_any(&value)),
            None => {
                producer.delegated = None;
                Ok(None)
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
            ProducerStep::Yielded { .. } => Err(RuntimeError::new(
                "control-flow",
                "a cancelled producer yielded",
            )
            .with_span(span)),
        }
    }
}
