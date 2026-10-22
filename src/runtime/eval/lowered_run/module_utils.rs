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
        LoweredValue::Callable(function) => function.identity(),
        other => return Err(RuntimeError::new("type-error", format!("utils.cache expected a callable, found {}", other.type_name())).with_span(span)),
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

struct ActualCallableTarget<'a> {
    program: &'a FullProgram,
    function: usize,
    invocation: Option<&'a PreparedInvocationBindings>,
}

impl Evaluator {
    fn actual_callable_target<'a>(
        &'a self, context: Option<&'a FullProgram>, value: super::ValueView<'a>, span: Span,
    ) -> Result<Option<ActualCallableTarget<'a>>, RuntimeError> {
        let callable = match value {
            super::ValueView::Runtime(Value::Callable(callable)) | super::ValueView::Lowered(LoweredValue::Callable(callable)) => callable,
            _ => return Ok(None),
        };
        let kind = callable.kind();
        let identity = callable.identity();
        let mut function = identity.as_name().map(LoweredFunctionKey::Name)
            .or_else(|| identity.as_qualified().map(LoweredFunctionKey::Qualified))
            .ok_or_else(|| RuntimeError::new("indexed-ir", "callable identity is not interned").with_span(span))?;
        let (program, invocation) = if let Some(instance) = callable.instance() {
            if instance.kind != kind || instance.function != function {
                return Err(RuntimeError::new("indexed-ir", "prepared callable identity disagrees with its owner").with_span(span));
            }
            (instance.program.as_ref(), Some(&instance.applications))
        } else {
            let mut program = context;
            let present = match program {
                Some(program) => program.function_view(function, kind).map_err(|error| super::indexed_run::indexed_error(error, span))?.is_some(),
                None => false,
            };
            if !present && let LoweredFunctionKey::Qualified(name) = function
                && let Some(dynamic) = self.indexed_dynamic_functions.get(&name)
            {
                if dynamic.kind != kind { return Err(RuntimeError::new("indexed-ir", "callable kind differs from its dynamic target").with_span(span)); }
                program = Some(&dynamic.program);
                function = dynamic.function;
            }
            (program.ok_or_else(|| RuntimeError::new("indexed-ir", "callable has no owning program").with_span(span))?, None)
        };
        let function = program.function_view(function, kind).map_err(|error| super::indexed_run::indexed_error(error, span))?
            .ok_or_else(|| RuntimeError::new("indexed-ir", "callable signature target is absent").with_span(span))?.index();
        Ok(Some(ActualCallableTarget { program, function, invocation }))
    }

    pub(in crate::runtime::eval) fn retained_callable_type(
        &self, context: Option<&FullProgram>, value: super::ValueView<'_>, span: Span,
    ) -> Result<Option<crate::sema::types::TypedCallable>, RuntimeError> {
        let Some(target) = self.actual_callable_target(context, value, span)? else { return Ok(None) };
        if target.invocation.is_none() && target.program.function_requires_invocation(target.function).map_err(|error| super::indexed_run::indexed_error(error, span))? {
            return Err(RuntimeError::new("indexed-ir", "generic callable has no checked invocation instance").with_span(span));
        }
        let empty = PreparedInvocationBindings::default();
        target.program.invocation_callable_type(target.function, target.invocation.unwrap_or(&empty))
            .map(Some).map_err(|error| super::indexed_run::indexed_error(error, span))
    }

    pub(in crate::runtime::eval) fn retained_generic_callable_matches(
        &self, context: Option<&FullProgram>, value: super::ValueView<'_>, kind: crate::sema::types::CallableKind, span: Span,
    ) -> Result<bool, RuntimeError> {
        if value.scalar_type() != Some(Type::Function(kind)) { return Ok(false); }
        let Some(target) = self.actual_callable_target(context, value, span)? else { return Ok(false) };
        if target.invocation.is_some() { return Ok(false); }
        target.program.function_has_quantified_contract(target.function).map_err(|error| super::indexed_run::indexed_error(error, span))
    }

    fn prepared_callable_matches(
        &self, context: &FullProgram, value: super::ValueView<'_>,
        expected: crate::runtime::eval::type_view::CallableContractView<'_>, span: Span,
    ) -> Result<bool, RuntimeError> {
        if value.scalar_type() != Some(Type::Function(expected.kind())) { return Ok(false); }
        Ok(self.retained_callable_type(Some(context), value, span)?.is_some_and(|actual| expected.accepts(&actual)))
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
                    crate::runtime::eval::type_view::RetainedValueContract::GenericCallable(kind) => self.retained_generic_callable_matches(Some(program), value, kind, span),
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
                        crate::runtime::eval::type_view::RetainedValueContract::GenericCallable(kind) => self.retained_generic_callable_matches(Some(program), value, kind, span),
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

    fn resolve_callable_invocation(
        &self, callee: &LoweredValue, span: Span,
    ) -> Result<(Arc<FullProgram>, LoweredFunctionKey, crate::sema::types::CallableKind, usize, PreparedInvocationBindings), RuntimeError> {
        let (mut function, mut kind) = super::indexed_run::indexed_callable_identity(callee, span)?;
        let callable = match callee { LoweredValue::Callable(value) => value, _ => unreachable!() };
        let (program, invocation) = if let Some(instance) = callable.instance() {
            if instance.kind != kind || instance.function != function {
                return Err(RuntimeError::new("indexed-ir", "prepared callable identity disagrees with its owner").with_span(span));
            }
            (Arc::clone(&instance.program), instance.applications.clone())
        } else {
            let mut program = self.indexed_program.clone().ok_or_else(|| RuntimeError::new("indexed-ir", "callable target has no executable program").with_span(span))?;
            if program.function_view(function, kind).map_err(|error| super::indexed_run::indexed_error(error, span))?.is_none()
                && let LoweredFunctionKey::Qualified(name) = function
                && let Some(dynamic) = self.indexed_dynamic_functions.get(&name)
            {
                if dynamic.kind != kind {
                    return Err(RuntimeError::new("indexed-ir", "callable kind differs from its dynamic target").with_span(span));
                }
                (program, function, kind) = (Arc::clone(&dynamic.program), dynamic.function, dynamic.kind);
            }
            (program, PreparedInvocationBindings::default())
        };
        let target = program.function_view(function, kind).map_err(|error| super::indexed_run::indexed_error(error, span))?
            .ok_or_else(|| RuntimeError::new("unresolved-call", "callable target is absent").with_span(span))?.index();
        if callable.instance().is_none() && program.function_requires_invocation(target).map_err(|error| super::indexed_run::indexed_error(error, span))? {
            return Err(RuntimeError::new("indexed-ir", "generic callable has no checked invocation instance").with_span(span));
        }
        Ok((program, function, kind, target, invocation))
    }

    pub(super) fn eval_lowered_callable_values(
        &mut self, callee: &LoweredValue, args: Vec<LoweredValue>, span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        let (program, function, kind, target, invocation) = self.resolve_callable_invocation(callee, span)?;
        self.validate_prepared_arguments(&program, target, &invocation, &args, span)?;
        self.eval_indexed_with_invocation(&program, function, kind, args, invocation, span)
    }

    pub(super) fn eval_lowered_utils_cache_values(
        &mut self, values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let (callee, args, encoded) = utils_cache_arguments(values, span)?;
        let (program, function, kind, target, invocation) = self.resolve_callable_invocation(&callee, span)?;
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

#[cfg(test)]
mod tests {
    use super::super::{Evaluator, LoweredFunctionKey, NativeArgumentValues};
    use crate::runtime::eval::FullBuilder;
    use crate::runtime::value::{ResultValue, Value};
    use crate::sema::check::Checker;
    use crate::sema::types::CallableKind;
    use crate::source::{SourceMap, Span};
    use crate::symbol::Name;
    use crate::syntax::parser::Parser;
    use std::sync::Arc;

    #[test]
    fn prepared_callable_host_adapters_preserve_dynamic_export_targets() {
        std::thread::Builder::new().stack_size(32 * 1024 * 1024).spawn(|| {
            let work = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".work/tmp");
            std::fs::create_dir_all(&work).unwrap();
            let directory = tempfile::tempdir_in(work).unwrap();
            let module = directory.path().join("dynamic-callable.xsh");
            std::fs::write(&module, "##! Dynamic callable target.\n## Returns the next value.\nexport pure identity(value: Int) -> Int { value + 1 }\n").unwrap();
            let source = format!("type Plugin = module {{ export pure identity(value: Int) -> Int }}\npure direct(handle: Pure) -> Any {{ handle.call(7) }}\nproc cached(handle: Pure) -> Any {{ utils.cache(handle, [7]) }}\nproc load_handle() [fs, error] -> Result[Pure] {{ let plugin = module.load(p{module:?})?.require(Plugin)?; Ok(plugin.identity) }}\n");
            let mut sources = SourceMap::default();
            let source_id = sources.add_file("dynamic-callable-adapters.xsh", source.clone());
            let parsed = Parser::parse_source_arena_only(source_id, &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let program = Arc::new(FullBuilder::build_compact(&parsed.arena, &checked, &source, Arc::new(sources.clone()), source_id).unwrap());
            drop(checked);
            drop(parsed);
            for force_frames in [false, true] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
                evaluator.force_frames = force_frames;
                evaluator.indexed_program = Some(Arc::clone(&program));
                let span = Span::at(source_id, 0);
                let (load, direct, cached) = program.symbol_owner().with_current(|| (LoweredFunctionKey::Name(Name::intern("load_handle")), LoweredFunctionKey::Name(Name::intern("direct")), LoweredFunctionKey::Name(Name::intern("cached"))));
                let Value::Result(ResultValue::Ok(handle)) = evaluator.call_indexed_direct(load, CallableKind::Proc, &[], span).unwrap().unwrap() else { panic!("the actual dynamic module exports its callable") };
                assert!(matches!(&*handle, Value::Callable(callable) if callable.kind() == CallableKind::Pure && callable.instance().is_none()));
                evaluator.trace_enabled = true;
                evaluator.trace_events.clear();
                assert_eq!(evaluator.call_indexed_direct(direct, CallableKind::Pure, &[(*handle).clone()], span).unwrap().unwrap(), Value::Int(8), "frames={force_frames}");
                for _ in 0..2 { assert_eq!(evaluator.call_indexed_direct(cached, CallableKind::Proc, &[(*handle).clone()], span).unwrap().unwrap(), Value::Int(8), "frames={force_frames}"); }
                assert_eq!(evaluator.trace_events.iter().filter(|event| event.kind == crate::trace::TraceKind::PureEnter && event.name.as_deref() == Some("identity")).count(), 2);
                let Value::Callable(callable) = &*handle else { unreachable!() };
                let lowered = crate::runtime::eval::LoweredValue::Callable(callable.clone());
                let signature = evaluator.module_export_signatures.get(&callable.identity()).unwrap();
                assert_eq!(signature.kind, CallableKind::Pure);
                assert!(evaluator.prepared_callable_matches(&program, crate::runtime::eval::ValueView::Runtime(&handle), crate::runtime::eval::type_view::CallableContractView::Export { kind: CallableKind::Pure, sig: &signature.sig }, span).unwrap());
                let wrong_kind = crate::runtime::eval::LoweredValue::Callable(crate::runtime::value::CallableValue::named(CallableKind::Proc, callable.identity()));
                let error = evaluator.eval_lowered_callable_values(&wrong_kind, vec![crate::runtime::eval::LoweredValue::Int(7)], span).unwrap_err();
                assert_eq!(error.kind, "indexed-ir");
                assert_eq!(error.message, "callable kind differs from its dynamic target");
                evaluator.utils_cache.clear();
                evaluator.trace_events.clear();
                assert!(matches!(evaluator.eval_lowered_callable_values(&lowered, vec![crate::runtime::eval::LoweredValue::Int(7)], span).unwrap(), crate::runtime::eval::LoweredValue::Int(8)));
                for _ in 0..2 {
                    let values = NativeArgumentValues::new(vec![Some(lowered.clone()), Some(crate::runtime::eval::LoweredValue::List(vec![crate::runtime::eval::LoweredValue::Int(7)]))]);
                    assert!(matches!(evaluator.eval_lowered_utils_cache_values(values, span).unwrap(), std::ops::ControlFlow::Continue(crate::runtime::eval::LoweredValue::Int(8))));
                }
                assert_eq!(evaluator.trace_events.iter().filter(|event| event.kind == crate::trace::TraceKind::PureEnter && event.name.as_deref() == Some("identity")).count(), 2);
            }
        }).unwrap().join().unwrap();
    }

}
