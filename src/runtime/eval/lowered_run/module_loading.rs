use super::{
    Evaluator, ControlFlow, LoweredValue, NativeArgumentValues, RuntimeError, Span, Value,
    lowered_path_arg, lowered_result_err_value, lowered_result_ok, lowered_value_from_runtime_any,
};

impl Evaluator {
    pub(super) fn eval_lowered_module_load_values(
        &mut self, mut values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let path = lowered_path_arg(
                values.pop().expect("checked value length"),
                "module.load",
                span,
            )?;
            match self.load_dynamic_module(path, span) {
                Ok(record) => {
                    let Some(module) = lowered_value_from_runtime_any(&Value::Module(record))
                    else {
                        return Err(RuntimeError::new(
                            "type-error",
                            "module.load returned unsupported Module",
                        )
                        .with_span(span));
                    };
                    lowered_result_ok(module)
                }
                Err(error) => lowered_result_err_value(error),
            }
        };
        Ok(ControlFlow::Continue(value))
    }
}
