use super::{
    Evaluator, ControlFlow, LoweredValue, NativeArgumentValues, RuntimeError, Span, Value,
    fs_module, ini_module, lowered_bool_arg_or, lowered_path_arg, lowered_result_err_value,
    lowered_unit_result,
};

impl Evaluator {
    pub(super) fn eval_lowered_ini_write_values(
        &mut self, mut values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let overwrite =
                lowered_bool_arg_or(values.get(2).cloned(), true, "ini.write", span)?;
            let value = values.remove(1).into_value();
            let Value::Record(record) = value else {
                return Err(RuntimeError::new("type-error", "ini.write expected Record")
                    .with_span(span));
            };
            let path = lowered_path_arg(values.remove(0), "ini.write", span)?;
            let text = match ini_module::encode(&record, span) {
                Ok(text) => text,
                Err(error) => {
                    return Ok(ControlFlow::Continue(lowered_result_err_value(error)));
                }
            };
            let host_path = self.host_path(&path);
            if !overwrite {
                match fs_module::exists(host_path.clone(), span) {
                    Ok(true) => {
                        return Ok(ControlFlow::Continue(lowered_result_err_value(
                            RuntimeError::new("ini-write", "destination exists")
                                .with_span(span),
                        )));
                    }
                    Ok(false) => {}
                    Err(error) => {
                        return Ok(ControlFlow::Continue(lowered_result_err_value(error)));
                    }
                }
            }
            lowered_unit_result(fs_module::write_path(host_path, text.as_bytes(), span))
        };
        Ok(ControlFlow::Continue(value))
    }
}
