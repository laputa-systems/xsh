use super::{
    Evaluator, Arc, BTreeMap, ControlFlow, LoweredValue, NativeArgumentValues, OsString, PathValue,
    RuntimeError, Span, lowered_bool_arg_or, lowered_env_key_arg, lowered_int_arg,
    lowered_result_err_value, lowered_result_ok, path_value_from_pathbuf,
};
use std::os::unix::ffi::OsStringExt;

impl Evaluator {
    pub(super) fn eval_lowered_env_get_values(
        &mut self, mut values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let key = match lowered_env_key_arg(values.pop(), span)? {
                Ok(key) => key,
                Err(error) => return Ok(ControlFlow::Continue(error)),
            };
            let value = match self.env.get_owned(key.as_bytes()) {
                Some(value) => value,
                None => {
                    return Ok(ControlFlow::Continue(lowered_result_err_value(
                        RuntimeError::new("env-missing", "environment value is unset")
                            .with_span(span),
                    )));
                }
            };
            match String::from_utf8(value) {
                Ok(text) => lowered_result_ok(LoweredValue::Str(text.into())),
                Err(_) => lowered_result_err_value(
                    RuntimeError::new("invalid-utf8", "environment value is not valid UTF-8")
                        .with_span(span),
                ),
            }
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_env_bool_values(
        &mut self, values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let fallback =
                lowered_bool_arg_or(values.get(1).cloned(), false, "env.bool", span)?;
            let key = match lowered_env_key_arg(values.first().cloned(), span)? {
                Ok(key) => key,
                Err(error) => return Ok(ControlFlow::Continue(error)),
            };
            let Some(value) = self.env.get_owned(key.as_bytes()) else {
                return Ok(ControlFlow::Continue(lowered_result_ok(
                    LoweredValue::Bool(fallback),
                )));
            };
            let text = match String::from_utf8(value) {
                Ok(text) => text.trim().to_ascii_lowercase(),
                Err(_) => {
                    return Ok(ControlFlow::Continue(lowered_result_err_value(
                        RuntimeError::new(
                            "invalid-utf8",
                            "environment value is not valid UTF-8",
                        )
                        .with_span(span),
                    )));
                }
            };
            lowered_result_ok(LoweredValue::Bool(matches!(
                text.as_str(),
                "1" | "true" | "yes" | "on"
            )))
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_env_path_values(
        &mut self, values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let fallback = match values.get(1).cloned() {
                Some(LoweredValue::Path(path)) => path,
                Some(other) => {
                    return Err(RuntimeError::new(
                        "type-error",
                        format!(
                            "env.path expected Path fallback, found {}",
                            other.type_name()
                        ),
                    )
                    .with_span(span));
                }
                None => PathValue::from_text("").map_err(|error| error.with_span(span))?,
            };
            let key = match lowered_env_key_arg(values.first().cloned(), span)? {
                Ok(key) => key,
                Err(error) => return Ok(ControlFlow::Continue(error)),
            };
            let Some(value) = self.env.get_owned(key.as_bytes()) else {
                return Ok(ControlFlow::Continue(lowered_result_ok(
                    LoweredValue::Path(fallback),
                )));
            };
            match PathValue::new(value).map_err(|error| error.with_span(span)) {
                Ok(path) => lowered_result_ok(LoweredValue::Path(path)),
                Err(error) => lowered_result_err_value(error),
            }
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_env_int_values(
        &mut self, values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let fallback = match values.get(1).cloned() {
                Some(value) => lowered_int_arg(Some(value), "env.int", span)?,
                None => 0,
            };
            let key = match lowered_env_key_arg(values.first().cloned(), span)? {
                Ok(key) => key,
                Err(error) => return Ok(ControlFlow::Continue(error)),
            };
            let Some(value) = self.env.get_owned(key.as_bytes()) else {
                return Ok(ControlFlow::Continue(lowered_result_ok(LoweredValue::Int(
                    fallback,
                ))));
            };
            let text = match String::from_utf8(value) {
                Ok(text) => text,
                Err(_) => {
                    return Ok(ControlFlow::Continue(lowered_result_err_value(
                        RuntimeError::new(
                            "invalid-utf8",
                            "environment value is not valid UTF-8",
                        )
                        .with_span(span),
                    )));
                }
            };
            match text.trim().parse::<i64>() {
                Ok(value) => lowered_result_ok(LoweredValue::Int(value)),
                Err(_) => lowered_result_err_value(
                    RuntimeError::new("env-int", "environment value is not an integer")
                        .with_span(span),
                ),
            }
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_env_list_values(
        &mut self, _values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let mut items = Vec::new();
            for (name, value) in self.env.snapshot() {
                let name = match String::from_utf8(name.clone()) {
                    Ok(text) => text,
                    Err(_) => {
                        return Ok(ControlFlow::Continue(lowered_result_err_value(
                            RuntimeError::new(
                                "invalid-utf8",
                                "environment name is not valid UTF-8",
                            )
                            .with_span(span),
                        )));
                    }
                };
                let value = match String::from_utf8(value.clone()) {
                    Ok(text) => text,
                    Err(_) => {
                        return Ok(ControlFlow::Continue(lowered_result_err_value(
                            RuntimeError::new(
                                "invalid-utf8",
                                "environment value is not valid UTF-8",
                            )
                            .with_span(span),
                        )));
                    }
                };
                items.push(LoweredValue::Record(Arc::new(BTreeMap::from([
                    (Arc::from("name"), LoweredValue::Str(name.into())),
                    (Arc::from("value"), LoweredValue::Str(value.into())),
                ]))));
            }
            lowered_result_ok(LoweredValue::List(items))
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_env_path_list_values(
        &mut self, mut values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let key = match lowered_env_key_arg(values.pop(), span)? {
                Ok(key) => key,
                Err(error) => return Ok(ControlFlow::Continue(error)),
            };
            let value = match self.env.get_owned(key.as_bytes()) {
                Some(value) => value,
                None => {
                    return Ok(ControlFlow::Continue(lowered_result_err_value(
                        RuntimeError::new("env-missing", "environment value is unset")
                            .with_span(span),
                    )));
                }
            };
            let mut paths = Vec::new();
            let value = OsString::from_vec(value);
            for path in std::env::split_paths(&value) {
                paths.push(LoweredValue::Path(path_value_from_pathbuf(path)?));
            }
            lowered_result_ok(LoweredValue::List(paths))
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_env_path_entries_values(
        &mut self, mut values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let key = match lowered_env_key_arg(values.pop(), span)? {
                Ok(key) => key,
                Err(error) => return Ok(ControlFlow::Continue(error)),
            };
            let value = match self.env.get_owned(key.as_bytes()) {
                Some(value) => value,
                None => {
                    return Ok(ControlFlow::Continue(lowered_result_err_value(
                        RuntimeError::new("env-missing", "environment value is unset")
                            .with_span(span),
                    )));
                }
            };
            let text = match String::from_utf8(value) {
                Ok(text) => text,
                Err(_) => {
                    return Ok(ControlFlow::Continue(lowered_result_err_value(
                        RuntimeError::new(
                            "invalid-utf8",
                            "environment value is not valid UTF-8",
                        )
                        .with_span(span),
                    )));
                }
            };
            let mut entries = Vec::new();
            for (index, raw) in text.split(':').enumerate() {
                let path = if raw.is_empty() {
                    PathValue::from_text(".").map_err(|error| error.with_span(span))?
                } else {
                    PathValue::from_text(raw).map_err(|error| error.with_span(span))?
                };
                entries.push(LoweredValue::Record(Arc::new(BTreeMap::from([
                    (Arc::from("index"), LoweredValue::Int(index as i64)),
                    (Arc::from("raw"), LoweredValue::Str(raw.into())),
                    (Arc::from("path"), LoweredValue::Path(path)),
                    (Arc::from("empty"), LoweredValue::Bool(raw.is_empty())),
                ]))));
            }
            lowered_result_ok(LoweredValue::List(entries))
        };
        Ok(ControlFlow::Continue(value))
    }
}
