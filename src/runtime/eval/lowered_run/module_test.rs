use super::{
    Evaluator, Arc, ControlFlow, LoweredValue, NativeArgumentValues, RecordMap, RuntimeError,
    RuntimeOp, Span, TestMock, Value, lowered_bytes_arg_or_empty, lowered_int_arg,
    lowered_int_arg_or, lowered_optional_argv_words, lowered_optional_env_record,
    lowered_optional_str_list, lowered_record_arg, lowered_result_err_value, lowered_result_ok,
    lowered_runtime_value, lowered_str_arg_owned, script_expectation_failure, test_error_kind,
    test_failure, test_mock_expected_return_type, ValueView, value_matches_static_type,
};

impl Evaluator {
    pub(super) fn eval_lowered_test_error_kind_values(
        &mut self, values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let value = values[0].clone().into_value();
            let expected =
                lowered_str_arg_owned(values.get(1).cloned(), "", "test.error_kind", span)?;
            let message =
                lowered_str_arg_owned(values.get(2).cloned(), "", "test.error_kind", span)?;
            let actual = test_error_kind(&value);
            if actual.as_deref() == Some(expected.as_str()) {
                lowered_result_ok(LoweredValue::Unit)
            } else {
                let detail = if message.is_empty() {
                    format!(
                        "expected error kind `{expected}`, found `{}`",
                        actual.unwrap_or_else(|| "none".to_string())
                    )
                } else {
                    message
                };
                lowered_runtime_value(test_failure(detail), span)?
            }
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_test_mock_values(
        &mut self, _op: RuntimeOp, values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let _ctx = lowered_record_arg(values.first().cloned(), "test.mock", span)?;
            let op = lowered_str_arg_owned(values.get(1).cloned(), "", "test.mock", span)?;
            let matcher = lowered_record_arg(values.get(2).cloned(), "test.mock", span)?;
            let result = values[3].clone().into_value();
            let times = lowered_int_arg_or(values.get(4).cloned(), 1, "test.mock", span)?;
            if times < 1 {
                lowered_result_err_value(
                    RuntimeError::new("test-mock", "times must be at least 1").with_span(span),
                )
            } else if let Some(expected) = test_mock_expected_return_type(&op)
                && !value_matches_static_type(ValueView::Runtime(&result), &expected)
            {
                lowered_result_err_value(
                    RuntimeError::new(
                        "test-mock",
                        format!("mock result for `{op}` must match {expected}"),
                    )
                    .with_span(span),
                )
            } else {
                self.test_mocks
                    .entry(op.to_string())
                    .or_default()
                    .push(TestMock {
                        matcher,
                        result,
                        remaining: times,
                    });
                lowered_result_ok(LoweredValue::Unit)
            }
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_test_linux_fake_values(
        &mut self, op: RuntimeOp, values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let linux = op == RuntimeOp::TestLinuxFake;
            let (module, api, kind) = if linux {
                ("linux", "test.linux_fake", "test-linux-fake")
            } else {
                ("unix", "test.unix_fake", "test-unix-fake")
            };
            let _ctx = lowered_record_arg(values.first().cloned(), api, span)?;
            let settings = match values.get(1).cloned() {
                Some(value) => lowered_record_arg(Some(value), api, span)?,
                None => RecordMap::new(),
            };
            let mut linux_fake = super::super::LinuxFake::default();
            let mut unix_fake = super::super::UnixFake::default();
            let mut failure = None;
            for (key, value) in settings.iter() {
                let text = match value {
                    Value::Str(text) => text.to_string(),
                    Value::Int(number) => number.to_string(),
                    Value::Path(path) => path.display(),
                    other => {
                        failure = Some(format!(
                            "{module} fake setting `{key}` must be Str, Int, or Path, found {}",
                            other.type_name()
                        ));
                        break;
                    }
                };
                let set = if linux {
                    linux_fake.set(key, text)
                } else {
                    unix_fake.set(key, text)
                };
                if let Err(message) = set {
                    failure = Some(message);
                    break;
                }
            }
            match failure {
                Some(message) => {
                    lowered_result_err_value(RuntimeError::new(kind, message).with_span(span))
                }
                None => {
                    if linux {
                        self.linux_fake = Some(Arc::new(linux_fake));
                    } else {
                        self.unix_fake = Some(Arc::new(unix_fake));
                    }
                    lowered_result_ok(LoweredValue::Unit)
                }
            }
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_test_run_script_values(
        &mut self, values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let ctx = lowered_record_arg(values.first().cloned(), "test.run_script", span)?;
            let source =
                lowered_str_arg_owned(values.get(1).cloned(), "", "test.run_script", span)?;
            let args =
                lowered_optional_argv_words(values.get(2).cloned(), "test.run_script", span)?;
            let env =
                lowered_optional_env_record(values.get(3).cloned(), "test.run_script", span)?;
            let stdin =
                lowered_bytes_arg_or_empty(values.get(4).cloned(), "test.run_script", span)?;
            let name = lowered_str_arg_owned(
                values.get(5).cloned(),
                "script.xsh",
                "test.run_script",
                span,
            )?;
            match self.lowered_test_run_script(&ctx, &source, &args, &env, &stdin, &name, span)
            {
                Ok(record) => lowered_result_ok(LoweredValue::Record(Arc::new(record))),
                Err(error) => lowered_result_err_value(error),
            }
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_test_run_xsh_values(
        &mut self, values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let ctx = lowered_record_arg(values.first().cloned(), "test.run_xsh", span)?;
            let source =
                lowered_str_arg_owned(values.get(1).cloned(), "", "test.run_xsh", span)?;
            let xsh_args =
                lowered_optional_str_list(values.get(2).cloned(), "test.run_xsh", span)?;
            let script_args =
                lowered_optional_argv_words(values.get(3).cloned(), "test.run_xsh", span)?;
            let env =
                lowered_optional_env_record(values.get(4).cloned(), "test.run_xsh", span)?;
            let stdin =
                lowered_bytes_arg_or_empty(values.get(5).cloned(), "test.run_xsh", span)?;
            let name = lowered_str_arg_owned(
                values.get(6).cloned(),
                "script.xsh",
                "test.run_xsh",
                span,
            )?;
            match self.lowered_test_run_xsh(
                &ctx,
                &source,
                &xsh_args,
                &script_args,
                &env,
                &stdin,
                &name,
                span,
            ) {
                Ok(record) => lowered_result_ok(LoweredValue::Record(Arc::new(record))),
                Err(error) => lowered_result_err_value(error),
            }
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_test_run_xsht_trace_values(
        &mut self, values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let ctx = lowered_record_arg(values.first().cloned(), "test.run_xsht_trace", span)?;
            let source =
                lowered_str_arg_owned(values.get(1).cloned(), "", "test.run_xsht_trace", span)?;
            let trace_args =
                lowered_optional_str_list(values.get(2).cloned(), "test.run_xsht_trace", span)?;
            let script_args = lowered_optional_argv_words(
                values.get(3).cloned(),
                "test.run_xsht_trace",
                span,
            )?;
            let env = lowered_optional_env_record(
                values.get(4).cloned(),
                "test.run_xsht_trace",
                span,
            )?;
            let stdin = lowered_bytes_arg_or_empty(
                values.get(5).cloned(),
                "test.run_xsht_trace",
                span,
            )?;
            let name = lowered_str_arg_owned(
                values.get(6).cloned(),
                "script.xsh",
                "test.run_xsht_trace",
                span,
            )?;
            match self.lowered_test_run_xsht_trace(
                &ctx,
                &source,
                &trace_args,
                &script_args,
                &env,
                &stdin,
                &name,
                span,
            ) {
                Ok(record) => lowered_result_ok(LoweredValue::Record(Arc::new(record))),
                Err(error) => lowered_result_err_value(error),
            }
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_test_expect_values(
        &mut self, values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let ctx = lowered_record_arg(values.first().cloned(), "test.expect", span)?;
            let source =
                lowered_str_arg_owned(values.get(1).cloned(), "", "test.expect", span)?;
            let status = lowered_int_arg(values.get(2).cloned(), "test.expect", span)?;
            let stderr =
                lowered_optional_str_list(values.get(3).cloned(), "test.expect", span)?;
            let stdout =
                lowered_optional_str_list(values.get(4).cloned(), "test.expect", span)?;
            let args =
                lowered_optional_argv_words(values.get(5).cloned(), "test.expect", span)?;
            let env = lowered_optional_env_record(values.get(6).cloned(), "test.expect", span)?;
            let stdin =
                lowered_bytes_arg_or_empty(values.get(7).cloned(), "test.expect", span)?;
            let name = lowered_str_arg_owned(
                values.get(8).cloned(),
                "script.xsh",
                "test.expect",
                span,
            )?;
            match self.lowered_test_run_script(&ctx, &source, &args, &env, &stdin, &name, span)
            {
                Ok(record) => {
                    match script_expectation_failure(&record, status, &stderr, &stdout) {
                        Some(message) => lowered_runtime_value(test_failure(message), span)?,
                        None => lowered_result_ok(LoweredValue::Record(Arc::new(record))),
                    }
                }
                Err(error) => lowered_result_err_value(error),
            }
        };
        Ok(ControlFlow::Continue(value))
    }
}
