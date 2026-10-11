use super::{
    Evaluator, ControlFlow, LoweredFunctionKey, LoweredFunctionKind, LoweredValue,
    NativeArgumentValues, RuntimeError, Span, lowered_value_from_runtime_any, utils_cache_key,
};

impl Evaluator {
    pub(super) fn eval_lowered_utils_cache_values(
        &mut self, mut values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
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
            let (function, pure) = match callee {
                LoweredValue::Pure(function) => (function, true),
                LoweredValue::Proc(function) => (function, false),
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
            let result = if let Some(cached) = self.utils_cache.get(&key).cloned() {
                cached
            } else {
                let function_key = function
                    .as_name()
                    .map(LoweredFunctionKey::Name)
                    .or_else(|| function.as_qualified().map(LoweredFunctionKey::Qualified))
                    .expect("function identity is interned");
                let result = self
                    .call_indexed_direct(
                        function_key,
                        if pure {
                            LoweredFunctionKind::Pure
                        } else {
                            LoweredFunctionKind::Proc
                        },
                        &call_args,
                        span,
                    )
                    .ok_or_else(|| {
                        RuntimeError::new(
                            "unresolved-call",
                            format!(
                                "utils.cache target {} could not be lowered",
                                function.display_name()
                            ),
                        )
                        .with_span(span)
                    })??;
                self.utils_cache.insert(key, result.clone());
                result
            };
            lowered_value_from_runtime_any(&result).ok_or_else(|| {
                RuntimeError::new(
                    "type-error",
                    format!("utils.cache returned unsupported {}", result.type_name()),
                )
                .with_span(span)
            })?
        };
        Ok(ControlFlow::Continue(value))
    }
}
