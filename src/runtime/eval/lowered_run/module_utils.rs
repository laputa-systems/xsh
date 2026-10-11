use super::{
    Evaluator, ControlFlow, LoweredFunctionKey, LoweredFunctionKind, LoweredValue,
    NativeArgumentValues, RuntimeError, RuntimeOp, Span, TraceKind, TracePayload,
    lowered_value_from_runtime_any, utils_cache_key,
};
use crate::runtime::value::Value;

pub(super) struct LoweredUtilsCacheMiss {
    pub(super) key: String,
    pub(super) function: LoweredFunctionKey,
    pub(super) kind: LoweredFunctionKind,
    pub(super) args: Vec<Value>,
}

pub(super) enum LoweredUtilsCacheLookup {
    Hit(LoweredValue),
    Miss(LoweredUtilsCacheMiss),
}

impl Evaluator {
    pub(super) fn eval_lowered_utils_cache_values(
        &mut self,
        values: NativeArgumentValues,
        span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        match self.prepare_lowered_utils_cache_values(values, span)? {
            LoweredUtilsCacheLookup::Hit(value) => Ok(ControlFlow::Continue(value)),
            LoweredUtilsCacheLookup::Miss(miss) => self
                .call_lowered_utils_cache_miss(miss, span)
                .map(ControlFlow::Continue),
        }
    }

    pub(super) fn prepare_lowered_utils_cache_values(
        &mut self,
        mut values: NativeArgumentValues,
        span: Span,
    ) -> Result<LoweredUtilsCacheLookup, RuntimeError> {
        let callee = values.remove(0);
        let call_args = if values.is_empty() {
            Vec::new()
        } else {
            let args = values.remove(0);
            let LoweredValue::List(args) = args else {
                return Err(RuntimeError::new(
                    "type-error",
                    "utils.cache expected List args",
                )
                .with_span(span));
            };
            args.into_iter()
                .map(LoweredValue::into_value)
                .collect::<Vec<_>>()
        };
        let (function, kind) = match callee {
            LoweredValue::Pure(function) => (function, LoweredFunctionKind::Pure),
            LoweredValue::Proc(function) => (function, LoweredFunctionKind::Proc),
            other => {
                return Err(RuntimeError::new(
                    "type-error",
                    format!(
                        "utils.cache expected Pure or Proc, found {}",
                        other.type_name()
                    ),
                )
                .with_span(span));
            }
        };
        let display_name = function.display_name();
        let key = utils_cache_key(&display_name, &call_args).map_err(|bad_type| {
            RuntimeError::new(
                "cache-key-error",
                format!("args contains a {bad_type}, which cannot be used as a cache key"),
            )
            .with_span(span)
        })?;
        if let Some(cached) = self.utils_cache.get(&key).cloned() {
            return Self::lowered_utils_cache_value(&cached, span)
                .map(LoweredUtilsCacheLookup::Hit);
        }
        let function = function
            .as_name()
            .map(LoweredFunctionKey::Name)
            .or_else(|| function.as_qualified().map(LoweredFunctionKey::Qualified))
            .expect("function identity is interned");
        Ok(LoweredUtilsCacheLookup::Miss(LoweredUtilsCacheMiss {
            key,
            function,
            kind,
            args: call_args,
        }))
    }

    pub(super) fn call_lowered_utils_cache_miss(
        &mut self,
        miss: LoweredUtilsCacheMiss,
        span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        let display_name = miss.function.display_name();
        let result = self
            .call_indexed_direct(miss.function, miss.kind, &miss.args, span)
            .ok_or_else(|| {
                RuntimeError::new(
                    "unresolved-call",
                    format!("utils.cache target {display_name} could not be lowered"),
                )
                .with_span(span)
            })??;
        self.utils_cache.insert(miss.key, result.clone());
        Self::lowered_utils_cache_value(&result, span)
    }

    pub(super) fn finish_lowered_utils_cache_frame(
        &mut self,
        key: String,
        mut value: LoweredValue,
        span: Span,
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
    fn lowered_utils_cache_value(value: &Value, span: Span) -> Result<LoweredValue, RuntimeError> {
        lowered_value_from_runtime_any(value).ok_or_else(|| {
            RuntimeError::new(
                "type-error",
                format!("utils.cache returned unsupported {}", value.type_name()),
            )
            .with_span(span)
        })
    }
}
