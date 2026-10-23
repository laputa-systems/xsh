use crate::runtime::eval::indexed::full::PreparedInvocationBindings;
use super::{
    Arc, Evaluator, FullFunctionView, FullProgram, FunctionHeader,
    LoweredFunctionKey, CallableKind, LoweredValue, RuntimeError, Span,
    StreamValue, Value, indexed_error,
    lowered_splice_arg_items,
};

/// Adds an evaluated argument as a single value or, spliced, as its items.
#[inline]
pub(super) fn append_call_argument(
    values: &mut Vec<LoweredValue>,
    kind: u32,
    value: LoweredValue,
    span: Span,
) -> Result<(), RuntimeError> {
    match kind {
        0 => values.push(value),
        1 => values.extend(lowered_splice_arg_items(value, span)?),
        _ => {
            return Err(
                RuntimeError::new("indexed-ir", "invalid call argument kind").with_span(span),
            );
        }
    }
    Ok(())
}

/// The checker gives a callable type only to functions of its kind, so a
/// typed call that receives anything else was reached through a path the
/// checker did not see. The call stops here instead of running it.
pub(super) fn indexed_typed_callee_kind(
    callee: &LoweredValue,
    kind: CallableKind,
    span: Span,
) -> Result<(), RuntimeError> {
    let expected = kind.value_type_name();
    match callee {
        LoweredValue::Callable(callable) if callable.kind() == kind => Ok(()),
        other => Err(RuntimeError::new(
            "type-error",
            format!(
                "typed call expected a {expected} of its callable type, found {}",
                other.type_name()
            ),
        )
        .with_span(span)),
    }
}

pub(in crate::runtime::eval) fn indexed_callable_identity(
    callee: &LoweredValue,
    span: Span,
) -> Result<(LoweredFunctionKey, CallableKind), RuntimeError> {
    let (function, kind) = match callee {
        LoweredValue::Callable(function) => (function, function.kind()),
        other => {
            return Err(RuntimeError::new(
                "type-error",
                format!(
                    "dynamic call expected a callable, found {}",
                    other.type_name()
                ),
            )
            .with_span(span));
        }
    };
    let function = function.identity();
    let key = function
        .as_name()
        .map(LoweredFunctionKey::Name)
        .or_else(|| function.as_qualified().map(LoweredFunctionKey::Qualified))
        .expect("callable identity is interned");
    Ok((key, kind))
}

impl Evaluator {
    pub(in crate::runtime::eval) fn call_indexed_direct(
        &mut self,
        function: LoweredFunctionKey,
        kind: CallableKind,
        args: &[Value],
        call_span: Span,
    ) -> Option<Result<Value, RuntimeError>> {
        self.call_indexed_direct_omitting(function, kind, args, &[], call_span)
    }

    /// A direct call in which the arguments at the `omitted` positions were
    /// not supplied: the callee takes its own default for each, evaluating
    /// one that is not a constant. The values at those positions are unused.
    pub(in crate::runtime::eval) fn call_indexed_direct_omitting(
        &mut self,
        function: LoweredFunctionKey,
        kind: CallableKind,
        args: &[Value],
        omitted: &[usize],
        call_span: Span,
    ) -> Option<Result<Value, RuntimeError>> {
        let program = Arc::clone(self.indexed_program.as_ref()?);
        let _symbols = program.symbol_owner().enter();
        self.call_indexed_direct_in_program(program, function, kind, args, omitted, call_span)
            .map(|result| {
                result.and_then(|mut value| {
                    self.register_returned_runtime_streams(&mut value, call_span)?;
                    Ok(value)
                })
            })
    }

    pub(super) fn call_indexed_direct_in_program(
        &mut self,
        program: Arc<FullProgram>,
        function: LoweredFunctionKey,
        kind: CallableKind,
        args: &[Value],
        omitted: &[usize],
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
                return self.call_indexed_direct_in_program(
                    Arc::clone(&dynamic.program),
                    dynamic.function,
                    dynamic.kind,
                    args,
                    omitted,
                    call_span,
                );
            }
            Err(error) => return Some(Err(indexed_error(error, call_span))),
        };
        let header = match view.header() {
            Ok(header) => header,
            Err(error) => return Some(Err(indexed_error(error, call_span))),
        };
        if let Err(error) = super::super::validate_unsigned_runtime_args(&header, args, omitted, call_span)
        {
            return Some(Err(error));
        }
        let slots = self.try_bind_lowered_runtime_args(&header, args, omitted)?;
        let frame_support = match self.indexed_frames_supported(view, call_span) {
            Ok(supported) => supported,
            Err(error) => return Some(Err(error)),
        };
        if kind != CallableKind::Stream && frame_support {
            return Some(
                self.eval_indexed_with_frame_slots(
                    program.as_ref(),
                    function,
                    kind,
                    slots,
                    call_span,
                )
                .map(LoweredValue::into_value),
            );
        }
        let mut slots = slots;
        let invocation = PreparedInvocationBindings::default();
        let normalize_error_payload = match program.normalizes_result_error_payload(view.index(), &invocation) {
            Ok(normalize) => normalize,
            Err(error) => return Some(Err(indexed_error(error, call_span))),
        };
        let result = self.start_indexed_producer(
            Arc::clone(&program),
            view,
            &header,
            &mut slots,
            invocation,
            call_span,
        );
        let result = result
            .and_then(|value| {
                super::super::checked_lowered_return_value(
                    &header,
                    value,
                    call_span,
                    &mut self.pending_traceback,
                    normalize_error_payload,
                )
            })
            .map(LoweredValue::into_value);
        self.recycle_lowered_slots(slots);
        Some(result)
    }

    pub(super) fn eval_indexed_call_at(
        &mut self,
        program: &Arc<FullProgram>,
        function: LoweredFunctionKey,
        kind: CallableKind,
        index: usize,
        values: &[LoweredValue],
        invocation: PreparedInvocationBindings,
        call_span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        let view = program.function_view_at(index).ok_or_else(|| RuntimeError::new("indexed-ir", "call target is absent").with_span(call_span))?;
        if view.callable_kind().map_err(|error| indexed_error(error, call_span))? != kind {
            return Err(RuntimeError::new("indexed-ir", "call target kind disagrees with its executable owner").with_span(call_span));
        }
        if kind != CallableKind::Stream {
            return self.eval_indexed_with_invocation(program, function, kind, values.to_vec(), invocation, call_span);
        }
        let header = view.header().map_err(|error| indexed_error(error, call_span))?;
        let mut slots = self.bind_lowered_values(&header, values, call_span)?;
        let result = self.start_indexed_producer(
            Arc::clone(program),
            view,
            &header,
            &mut slots,
            invocation,
            call_span,
        );
        let result = result.and_then(|value| {
            super::super::checked_lowered_return_value(
                &header,
                value,
                call_span,
                &mut self.pending_traceback,
                false,
            )
        });
        self.recycle_lowered_slots(slots);
        result
    }

    /// A stream producer's call does not run the body: the bound slots become a
    /// suspended continuation that consuming the stream resumes, one `yield` at
    /// a time.
    pub(super) fn start_indexed_producer(
        &mut self,
        program: Arc<FullProgram>,
        view: FullFunctionView<'_>,
        header: &FunctionHeader,
        slots: &mut [LoweredValue],
        invocation: PreparedInvocationBindings,
        call_span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        self.hydrate_lowered_captures(header, slots, call_span)?;
        let execution = view
            .execution()
            .map_err(|error| indexed_error(error, call_span))?;
        let (function, kind) = execution
            .function_identity()
            .map_err(|error| indexed_error(error, call_span))?;
        let contract = program.stream_function_contract(view.index(), &invocation)
            .map_err(|error| indexed_error(error, call_span))?;
        let state = self.start_script_producer(program, function, kind, view, slots.to_vec(), invocation, call_span)?;
        let mut stream = StreamValue::from_script(state);
        stream.contract = Some(contract);
        self.register_returned_stream(&mut stream, call_span)?;
        Ok(LoweredValue::Stream(Box::new(stream)))
    }
}
