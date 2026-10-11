use super::{
    Arc, Evaluator, FullFunctionView, FullProgram, FunctionHeader, IrVerifyError,
    LoweredFunctionKey, LoweredFunctionKind, LoweredValue, QualifiedName, RuntimeError, Span,
    StreamValue, TraceKind, TracePayload, TracebackFrame, TracebackFrameKind, Value, indexed_error,
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
    pure: bool,
    span: Span,
) -> Result<(), RuntimeError> {
    let expected = if pure { "Pure" } else { "Proc" };
    match (callee, pure) {
        (LoweredValue::Pure(_), true) | (LoweredValue::Proc(_), false) => Ok(()),
        (other, _) => Err(RuntimeError::new(
            "type-error",
            format!(
                "typed call expected a {expected} of its callable type, found {}",
                other.type_name()
            ),
        )
        .with_span(span)),
    }
}

pub(super) fn indexed_callable_identity(
    callee: &LoweredValue,
    span: Span,
) -> Result<(LoweredFunctionKey, LoweredFunctionKind), RuntimeError> {
    let (function, kind) = match callee {
        LoweredValue::Pure(function) => (function, LoweredFunctionKind::Pure),
        LoweredValue::Proc(function) => (function, LoweredFunctionKind::Proc),
        other => {
            return Err(RuntimeError::new(
                "type-error",
                format!(
                    "dynamic call expected Pure or Proc, found {}",
                    other.type_name()
                ),
            )
            .with_span(span));
        }
    };
    let key = function
        .as_name()
        .map(LoweredFunctionKey::Name)
        .or_else(|| function.as_qualified().map(LoweredFunctionKey::Qualified))
        .expect("callable identity is interned");
    Ok((key, kind))
}

impl Evaluator {
    /// The index `function`/`kind` resolves to inside `program`.
    ///
    /// The program is part of the cache key because one evaluator resolves the
    /// same qualified key against more than one program: a dynamically loaded
    /// module links its standard calls to the loading program's prepared
    /// implementations, so `<xsh-stdlib:hash> verify_file` names a function in
    /// both. The entry keeps the program alive, so its identity cannot be
    /// reused by a later allocation while the entry is cached.
    pub(super) fn indexed_function_index(
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

    pub(super) fn indexed_argument_default(
        &self,
        callee: &LoweredValue,
        slot: usize,
        span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        let (function, kind) = match callee {
            LoweredValue::Pure(function) => (function, LoweredFunctionKind::Pure),
            LoweredValue::Proc(function) => (function, LoweredFunctionKind::Proc),
            _ => {
                return Err(RuntimeError::new(
                    "type-error",
                    "argument default requires a prepared callable",
                )
                .with_span(span));
            }
        };
        let key = function
            .as_name()
            .map(LoweredFunctionKey::Name)
            .or_else(|| function.as_qualified().map(LoweredFunctionKey::Qualified))
            .expect("callable identity is interned");
        self.indexed_argument_default_for(key, kind, slot, span)
    }

    pub(super) fn indexed_argument_default_for(
        &self,
        key: LoweredFunctionKey,
        kind: LoweredFunctionKind,
        slot: usize,
        span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        let program = self
            .indexed_program
            .as_ref()
            .expect("indexed call retains its program");
        let view = if let Some(view) = program
            .function_view(key, kind)
            .map_err(|error| indexed_error(error, span))?
        {
            view
        } else {
            let LoweredFunctionKey::Qualified(qualified) = key else {
                return Err(RuntimeError::new(
                    "unresolved-call",
                    "argument default callable is not prepared",
                )
                .with_span(span));
            };
            let dynamic = self
                .indexed_dynamic_functions
                .get(&qualified)
                .ok_or_else(|| {
                    RuntimeError::new(
                        "unresolved-call",
                        "argument default callable is not prepared",
                    )
                    .with_span(span)
                })?;
            dynamic
                .program
                .function_view(dynamic.function, dynamic.kind)
                .map_err(|error| indexed_error(error, span))?
                .ok_or_else(|| {
                    RuntimeError::new(
                        "unresolved-call",
                        "argument default callable is not prepared",
                    )
                    .with_span(span)
                })?
        };
        view.header()
            .map_err(|error| indexed_error(error, span))?
            .param_defaults
            .get(slot)
            .and_then(Clone::clone)
            .ok_or_else(|| {
                RuntimeError::new(
                    "indexed-ir",
                    "checked omitted argument has no prepared default",
                )
                .with_span(span)
            })
    }

    pub(in crate::runtime::eval) fn call_indexed_direct(
        &mut self,
        function: LoweredFunctionKey,
        kind: LoweredFunctionKind,
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
        kind: LoweredFunctionKind,
        args: &[Value],
        omitted: &[usize],
        call_span: Span,
    ) -> Option<Result<Value, RuntimeError>> {
        let program = Arc::clone(self.indexed_program.as_ref()?);
        let _symbols = program.symbol_owner().enter();
        self.call_indexed_direct_in_program(program, function, kind, args, omitted, call_span)
    }

    pub(super) fn call_indexed_direct_in_program(
        &mut self,
        program: Arc<FullProgram>,
        function: LoweredFunctionKey,
        kind: LoweredFunctionKind,
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
        if let Err(error) = super::super::validate_unsigned_runtime_args(&header, args, omitted, call_span)
        {
            return Some(Err(error));
        }
        let slots = self.try_bind_lowered_runtime_args(&header, args, omitted)?;
        let frame_support = match self.indexed_frames_supported(view, call_span) {
            Ok(supported) => supported,
            Err(error) => return Some(Err(error)),
        };
        if frame_support {
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
        let result = self
            .eval_indexed_call_frame(function, kind, view, &header, &mut slots, call_span)
            .and_then(|value| super::super::checked_lowered_return_value(&header, value, call_span))
            .map(LoweredValue::into_value);
        self.recycle_lowered_slots(slots);
        Some(result)
    }

    pub(super) fn eval_indexed_call_frame(
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
        let result = self.start_indexed_producer(view, header, slots, call_span);
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

    pub(super) fn eval_indexed_named_call(
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
        self.eval_indexed_call_at(&program, function, kind, index, values, call_span)
    }

    /// Runs a resolved callee: an ordinary body on the heap-backed frames, and
    /// a stream producer as a suspended continuation.
    pub(super) fn eval_indexed_call_at(
        &mut self,
        program: &Arc<FullProgram>,
        function: LoweredFunctionKey,
        kind: LoweredFunctionKind,
        index: usize,
        values: &[LoweredValue],
        call_span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        let view = program
            .function_view_at(index)
            .expect("cached lowered function index is valid");
        if self.indexed_frames_supported(view, call_span)? {
            return self.eval_indexed_with_frames(
                program.as_ref(),
                function,
                kind,
                values,
                call_span,
            );
        }
        let header = view
            .header()
            .map_err(|error| indexed_error(error, call_span))?;
        let mut next_slots = self.bind_lowered_values(&header, values, call_span)?;
        let result = self
            .eval_indexed_call_frame(function, kind, view, &header, &mut next_slots, call_span)
            .and_then(|value| super::super::checked_lowered_return_value(&header, value, call_span));
        self.recycle_lowered_slots(next_slots);
        result
    }

    /// Call an implementation a loading program published.
    ///
    /// A dynamically loaded module can name a standard-library implementation
    /// its loader already prepared. That function has no identity in the loaded
    /// module's own store, so the call resolves through the dynamic function
    /// table and executes inside the program that prepared it.
    pub(super) fn eval_indexed_external_call(
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

    /// A stream producer's call does not run the body: the bound slots become a
    /// suspended continuation that consuming the stream resumes, one `yield` at
    /// a time.
    pub(super) fn start_indexed_producer(
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
        let (function, kind) = execution
            .function_identity()
            .map_err(|error| indexed_error(error, call_span))?;
        let state = self.start_script_producer(function, kind, view, slots.to_vec(), call_span)?;
        Ok(LoweredValue::Stream(Box::new(StreamValue::from_script(
            state,
        ))))
    }
}
