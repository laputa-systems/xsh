use super::{
    Evaluator, ControlFlow, LoweredFunctionKey, LoweredValue,
    NativeArgumentValues, RuntimeError, RuntimeOp, Span, TraceKind, TracePayload,
    lowered_value_from_runtime_any, utils_cache_key,
};
use crate::runtime::value::Value;
use crate::sema::types::Type;

use crate::runtime::eval::indexed::full::{FullProgram, PreparedInvocationBindings};
use std::sync::Arc;
use std::hash::{Hash, Hasher};

#[derive(Clone)]
pub(in crate::runtime::eval) struct UtilsCacheKey {
    program: Arc<FullProgram>,
    target: usize,
    invocation: PreparedInvocationBindings,
    arguments: String,
}

impl PartialEq for UtilsCacheKey {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.program, &other.program) && self.target == other.target
            && self.invocation == other.invocation && self.arguments == other.arguments
    }
}
impl Eq for UtilsCacheKey {}
impl Hash for UtilsCacheKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.program).hash(state);
        self.target.hash(state);
        self.arguments.hash(state);
    }
}

impl UtilsCacheKey {
    pub(in crate::runtime::eval) fn new(
        program: Arc<FullProgram>, target: usize, invocation: PreparedInvocationBindings, arguments: String,
    ) -> Self { Self { program, target, invocation, arguments } }
}

pub(super) fn utils_cache_arguments(
    mut values: NativeArgumentValues, span: Span,
) -> Result<(LoweredValue, Vec<LoweredValue>, String), RuntimeError> {
    let callee = values.remove(0);
    let function = match &callee {
        LoweredValue::Pure(function) | LoweredValue::Proc(function) => function.identity(),
        other => return Err(RuntimeError::new("type-error", format!("utils.cache expected Pure or Proc, found {}", other.type_name())).with_span(span)),
    };
    let args = if values.is_empty() { Vec::new() } else {
        let LoweredValue::List(args) = values.remove(0) else {
            return Err(RuntimeError::new("type-error", "utils.cache expected List args").with_span(span));
        };
        args
    };
    let runtime = args.iter().cloned().map(LoweredValue::into_value).collect::<Vec<_>>();
    let key = utils_cache_key(&function.display_name(), &runtime).map_err(|bad_type| {
        RuntimeError::new("cache-key-error", format!("args contains a {bad_type}, which cannot be used as a cache key")).with_span(span)
    })?;
    Ok((callee, args, key))
}

impl Evaluator {
    fn prepared_callable_matches(
        &self, context: &FullProgram, value: super::ValueView<'_>,
        expected: crate::runtime::eval::type_view::CallableContractView<'_>, span: Span,
    ) -> Result<bool, RuntimeError> {
        let (callable, kind) = match value {
            super::ValueView::Runtime(Value::Pure(callable)) | super::ValueView::Lowered(LoweredValue::Pure(callable)) => (callable, super::LoweredFunctionKind::Pure),
            super::ValueView::Runtime(Value::Proc(callable)) | super::ValueView::Lowered(LoweredValue::Proc(callable)) => (callable, super::LoweredFunctionKind::Proc),
            _ => return Ok(false),
        };
        if expected.pure() != (kind == super::LoweredFunctionKind::Pure) { return Ok(false); }
        let identity = callable.identity();
        let function = identity.as_name().map(LoweredFunctionKey::Name)
            .or_else(|| identity.as_qualified().map(LoweredFunctionKey::Qualified))
            .ok_or_else(|| RuntimeError::new("indexed-ir", "callable identity is not interned").with_span(span))?;
        let empty = PreparedInvocationBindings::default();
        let (program, invocation) = if let Some(instance) = callable.instance() {
            if instance.kind != kind || instance.function != function {
                return Err(RuntimeError::new("indexed-ir", "prepared callable identity disagrees with its owner").with_span(span));
            }
            (instance.program.as_ref(), &instance.applications)
        } else {
            let mut program = context;
            if program.function_view(function, kind).map_err(|error| super::indexed_run::indexed_error(error, span))?.is_none()
                && let LoweredFunctionKey::Qualified(name) = function
                && let Some(dynamic) = self.indexed_dynamic_functions.get(&name)
            {
                if dynamic.kind != kind { return Ok(false); }
                program = &dynamic.program;
            }
            (program, &empty)
        };
        let target = program.function_view(function, kind).map_err(|error| super::indexed_run::indexed_error(error, span))?
            .ok_or_else(|| RuntimeError::new("indexed-ir", "callable signature target is absent").with_span(span))?.index();
        if callable.instance().is_none() && program.function_requires_invocation(target).map_err(|error| super::indexed_run::indexed_error(error, span))? {
            return Err(RuntimeError::new("indexed-ir", "generic callable has no checked invocation instance").with_span(span));
        }
        let actual = program.invocation_callable_type(target, invocation).map_err(|error| super::indexed_run::indexed_error(error, span))?;
        Ok(expected.accepts(&actual))
    }

    fn prepared_stream_matches(
        &self, program: &FullProgram, value: super::ValueView<'_>, expected: &Type, span: Span,
    ) -> Result<bool, RuntimeError> {
        let Some(stream) = value.stream() else { return Ok(false); };
        if let Some(contract) = &stream.contract {
            return contract.matches(expected).map_err(|error| super::indexed_run::indexed_error(error, span));
        }
        if stream.script.is_some() || stream.source.is_some() {
            return Err(RuntimeError::new("indexed-ir", "live stream has no retained item contract").with_span(span));
        }
        for item in &stream.items {
            let mut proof_error = None;
            let matches = crate::runtime::eval::type_view::value_matches_type_with_contracts(super::ValueView::Runtime(&item.value), expected, &mut |value, contract| {
                let result = match contract {
                    crate::runtime::eval::type_view::RetainedValueContract::Callable(callable) => self.prepared_callable_matches(program, value, callable, span),
                    crate::runtime::eval::type_view::RetainedValueContract::Stream(item) => self.prepared_stream_matches(program, value, item, span),
                };
                match result { Ok(matches) => matches, Err(error) => { proof_error = Some(error); false } }
            });
            if !matches { return proof_error.map_or(Ok(false), Err); }
        }
        Ok(true)
    }

    pub(super) fn validate_prepared_arguments(
        &mut self, program: &FullProgram, target: usize, invocation: &PreparedInvocationBindings,
        values: &[LoweredValue], span: Span,
    ) -> Result<(), RuntimeError> {
        let Some(types) = program.invocation_parameter_types(target, invocation)
            .map_err(|error| super::indexed_run::indexed_error(error, span))? else { return Ok(()); };
        let header = program.function_view_at(target).ok_or_else(|| RuntimeError::new("indexed-ir", "prepared target is absent").with_span(span))?
            .header().map_err(|error| super::indexed_run::indexed_error(error, span))?;
        let slots = self.bind_lowered_values(&header, values, span)?;
        let result = types.iter().zip(&slots).enumerate().try_for_each(|(slot, (ty, value))| {
            let mut proof_error = None;
            let matches = matches!(value, LoweredValue::OmittedArgument)
                || crate::runtime::eval::type_view::value_matches_type_with_contracts(super::ValueView::Lowered(value), ty, &mut |value, expected| {
                    let result = match expected {
                        crate::runtime::eval::type_view::RetainedValueContract::Callable(expected) => self.prepared_callable_matches(program, value, expected, span),
                        crate::runtime::eval::type_view::RetainedValueContract::Stream(item) => self.prepared_stream_matches(program, value, item, span),
                    };
                    match result {
                        Ok(matches) => matches,
                        Err(error) => { proof_error = Some(error); false }
                    }
                });
            if matches { Ok(()) }
            else if let Some(error) = proof_error.take() { Err(error) }
            else { Err(RuntimeError::new("type-error", format!("argument {} violates its prepared callable type", slot + 1)).with_span(span)) }
        });
        self.recycle_lowered_slots(slots);
        result
    }

    pub(super) fn eval_lowered_utils_cache_values(
        &mut self, values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let (callee, args, encoded) = utils_cache_arguments(values, span)?;
        let (function, kind) = super::indexed_run::indexed_callable_identity(&callee, span)?;
        let callable = match &callee { LoweredValue::Pure(value) | LoweredValue::Proc(value) => value, _ => unreachable!() };
        let (program, invocation) = if let Some(instance) = callable.instance() {
            if instance.kind != kind || instance.function != function {
                return Err(RuntimeError::new("indexed-ir", "prepared callable identity disagrees with its owner").with_span(span));
            }
            (Arc::clone(&instance.program), instance.applications.clone())
        } else {
            let mut program = self.indexed_program.clone().ok_or_else(|| RuntimeError::new("indexed-ir", "cache target has no executable program").with_span(span))?;
            if program.function_view(function, kind).map_err(|error| super::indexed_run::indexed_error(error, span))?.is_none()
                && let LoweredFunctionKey::Qualified(name) = function
                && let Some(dynamic) = self.indexed_dynamic_functions.get(&name)
            { program = Arc::clone(&dynamic.program); }
            (program, PreparedInvocationBindings::default())
        };
        let target = program.function_view(function, kind).map_err(|error| super::indexed_run::indexed_error(error, span))?
            .ok_or_else(|| RuntimeError::new("unresolved-call", "cache target is absent").with_span(span))?.index();
        if callable.instance().is_none() && program.function_requires_invocation(target).map_err(|error| super::indexed_run::indexed_error(error, span))? {
            return Err(RuntimeError::new("indexed-ir", "generic callable has no checked invocation instance").with_span(span));
        }
        self.validate_prepared_arguments(&program, target, &invocation, &args, span)?;
        let key = UtilsCacheKey::new(Arc::clone(&program), target, invocation.clone(), encoded);
        if let Some(value) = self.utils_cache.get(&key) { return Self::lowered_utils_cache_value(value, span).map(ControlFlow::Continue); }
        let value = self.eval_indexed_with_invocation(&program, function, kind, args, invocation, span)?;
        self.finish_lowered_utils_cache_frame(key, value, span).map(ControlFlow::Continue)
    }

    pub(super) fn finish_lowered_utils_cache_frame(
        &mut self, key: UtilsCacheKey, mut value: LoweredValue, span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        self.register_returned_lowered_stream(&mut value, span)?;
        let value = value.into_value();
        self.utils_cache.insert(key, value.clone());
        Self::lowered_utils_cache_value(&value, span)
    }

    pub(super) fn trace_lowered_utils_cache_call(&mut self, span: Span) {
        if !self.trace_enabled {
            return;
        }
        let trace_name = crate::modules::signature::api_spec()
            .op_trace_name(RuntimeOp::UtilsCache)
            .map(str::to_string);
        self.trace_enter(
            TraceKind::ModuleCall,
            Some(span),
            trace_name.as_deref(),
            TracePayload::None,
        );
    }

    pub(super) fn trace_lowered_utils_cache_result(&mut self, span: Span) {
        if !self.trace_enabled {
            return;
        }
        let trace_name = crate::modules::signature::api_spec()
            .op_trace_name(RuntimeOp::UtilsCache)
            .map(str::to_string);
        self.trace_exit(
            TraceKind::ModuleResult,
            Some(span),
            trace_name.as_deref(),
            TracePayload::None,
        );
    }
}

impl Evaluator {
    pub(super) fn lowered_utils_cache_value(value: &Value, span: Span) -> Result<LoweredValue, RuntimeError> {
        lowered_value_from_runtime_any(value).ok_or_else(|| {
            RuntimeError::new(
                "type-error",
                format!("utils.cache returned unsupported {}", value.type_name()),
            )
            .with_span(span)
        })
    }
}
