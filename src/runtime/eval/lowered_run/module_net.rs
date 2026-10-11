use super::{
    Evaluator, Arc, BTreeMap, ControlFlow, Duration, DurationValue, LoweredValue,
    NativeArgumentValues, NetJobTask, RuntimeError, Span, Value, intercept_test_host_call,
    lowered_duration_arg, lowered_int_arg_or, lowered_record_arg, lowered_result_err_value,
    lowered_result_ok, lowered_runtime_value, lowered_str_arg_owned, net_module,
    runtime_error_from_value,
};

impl Evaluator {
    pub(super) fn eval_lowered_net_pool_values(
        &mut self, values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let name =
                lowered_str_arg_owned(values.first().cloned(), "default", "net.pool", span)?;
            let max_idle_per_host =
                lowered_int_arg_or(values.get(1).cloned(), 8, "net.pool", span)?;
            if max_idle_per_host < 0 {
                lowered_result_err_value(
                    RuntimeError::new("net-pool", "max_idle_per_host cannot be negative")
                        .with_span(span),
                )
            } else {
                let idle_timeout = match values.get(2).cloned() {
                    Some(value) => lowered_duration_arg(Some(value), "net.pool", span)?,
                    None => DurationValue { millis: 90_000 },
                };
                self.net_pool_options.insert(
                    name.to_string(),
                    net_module::NetPoolOptions {
                        max_idle_per_host: max_idle_per_host as usize,
                        idle_timeout: Duration::from_millis(idle_timeout.millis),
                    },
                );
                self.net_agents.retain(|key, _| key.pool != name);
                lowered_result_ok(LoweredValue::Record(Arc::new(BTreeMap::from([
                    (Arc::from("name"), LoweredValue::Str(name.into())),
                    (
                        Arc::from("max_idle_per_host"),
                        LoweredValue::Int(max_idle_per_host),
                    ),
                    (
                        Arc::from("idle_timeout_ms"),
                        LoweredValue::Int(idle_timeout.millis as i64),
                    ),
                ]))))
            }
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_net_start_values(
        &mut self, mut values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let record = lowered_record_arg(values.pop(), "net.start", span)?;
            let request = self.net_request_from_record(record.clone(), span)?;
            if let Err(error) = net_module::validate_request(&request, span) {
                return Ok(ControlFlow::Continue(lowered_result_err_value(error)));
            }
            let reservation = request.max_body_bytes;
            if let Some(value) =
                intercept_test_host_call(self, "net.start", record.clone(), span)
            {
                match value {
                    Value::Result(crate::runtime::value::ResultValue::Ok(response)) => {
                        match self.admit_net_job(reservation, span) {
                            Ok(()) => {
                                let handle = self.net_job_value(
                                    NetJobTask::Completed(Ok(*response)),
                                    span,
                                    reservation,
                                );
                                lowered_result_ok(LoweredValue::NetJob(Box::new(handle)))
                            }
                            Err(error) => lowered_result_err_value(error),
                        }
                    }
                    Value::Result(crate::runtime::value::ResultValue::Err(error)) => {
                        lowered_result_err_value(runtime_error_from_value(*error, span))
                    }
                    _ => lowered_result_err_value(
                        RuntimeError::new(
                            "test-mock",
                            "net.start mock must return Result[NetResponse]",
                        )
                        .with_span(span),
                    ),
                }
            } else {
                #[cfg(feature = "net")]
                {
                    let options = self.net_call_options(&record, span)?;
                    match self.net_agent(&options, span) {
                        Ok(agent) => {
                            if let Err(error) = self.admit_net_job(reservation, span) {
                                return Ok(ControlFlow::Continue(lowered_result_err_value(
                                    error,
                                )));
                            }
                            let runtime = self
                                .net_runtime
                                .as_mut()
                                .expect("net agent initializes runtime");
                            match net_module::submit_request(
                                runtime,
                                agent,
                                request,
                                net_module::NetProtocol::Auto,
                                span,
                            ) {
                                Ok(operation) => {
                                    let handle = self.net_job_value(
                                        NetJobTask::Transport(operation),
                                        span,
                                        reservation,
                                    );
                                    lowered_result_ok(LoweredValue::NetJob(Box::new(handle)))
                                }
                                Err(error) => {
                                    self.release_net_job_admission(reservation);
                                    lowered_result_err_value(error)
                                }
                            }
                        }
                        Err(error) => lowered_result_err_value(error),
                    }
                }
                #[cfg(not(feature = "net"))]
                {
                    lowered_result_err_value(
                        RuntimeError::new("net-disabled", "net feature is disabled")
                            .with_span(span),
                    )
                }
            }
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_net_request_values(
        &mut self, mut values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let record = lowered_record_arg(values.pop(), "net.request", span)?;
            if let Some(value) =
                intercept_test_host_call(self, "net.request", record.clone(), span)
            {
                lowered_runtime_value(value, span)?
            } else {
                let options = self.net_call_options(&record, span)?;
                let request = self.net_request_from_record(record, span)?;
                if let Err(error) = net_module::validate_request(&request, span) {
                    return Ok(ControlFlow::Continue(lowered_result_err_value(error)));
                }
                match self.net_agent(&options, span) {
                    Ok(agent) => {
                        let runtime = self
                            .net_runtime
                            .as_mut()
                            .expect("net agent initializes runtime");
                        match net_module::submit_request(
                            runtime,
                            agent,
                            request,
                            net_module::NetProtocol::Http1,
                            span,
                        ) {
                            Ok(operation) => match self
                                .wait_transport_operation(&operation, span, span, true)
                            {
                                Ok(response) => {
                                    lowered_runtime_value(Value::ok(response), span)?
                                }
                                Err(error) => lowered_result_err_value(error),
                            },
                            Err(error) => lowered_result_err_value(error),
                        }
                    }
                    Err(error) => lowered_result_err_value(error),
                }
            }
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_net_request_many_values(
        &mut self, mut values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let batch = lowered_record_arg(values.pop(), "net.request_many", span)?;
            if let Some(value) =
                intercept_test_host_call(self, "net.request_many", batch.clone(), span)
            {
                lowered_runtime_value(value, span)?
            } else {
                let Some(values) = batch.get("requests") else {
                    return Ok(ControlFlow::Continue(lowered_result_err_value(
                        RuntimeError::new("net-request-many", "requests is required")
                            .with_span(span),
                    )));
                };
                let Value::List(records) = values else {
                    return Ok(ControlFlow::Continue(lowered_result_err_value(
                        RuntimeError::new("type-error", "requests must be List[Record]")
                            .with_span(span),
                    )));
                };
                let requests = records
                    .iter()
                    .map(|value| match value {
                        Value::Record(record) => {
                            self.net_request_from_record(record.clone(), span)
                        }
                        _ => Err(RuntimeError::new(
                            "type-error",
                            "requests must be List[Record]",
                        )
                        .with_span(span)),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let concurrency = match batch.get("concurrency") {
                    Some(Value::Int(value)) if *value >= 0 => *value as usize,
                    Some(Value::Int(_)) => {
                        return Ok(ControlFlow::Continue(lowered_result_err_value(
                            RuntimeError::new(
                                "net-concurrency",
                                "concurrency must be at least one",
                            )
                            .with_span(span),
                        )));
                    }
                    Some(_) => {
                        return Ok(ControlFlow::Continue(lowered_result_err_value(
                            RuntimeError::new("type-error", "concurrency must be Int")
                                .with_span(span),
                        )));
                    }
                    None => 16,
                };
                if concurrency == 0 {
                    return Ok(ControlFlow::Continue(lowered_result_err_value(
                        RuntimeError::new(
                            "net-concurrency",
                            "concurrency must be at least one",
                        )
                        .with_span(span),
                    )));
                }
                let options = self.net_call_options(&batch, span)?;
                match self.net_agent(&options, span) {
                    Ok(agent) => {
                        match self.request_many_with_runtime(agent, requests, concurrency, span)
                        {
                            Ok(value) => lowered_runtime_value(value, span)?,
                            Err(error) => lowered_result_err_value(error),
                        }
                    }
                    Err(error) => lowered_result_err_value(error),
                }
            }
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_net_download_many_values(
        &mut self, mut values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let batch = lowered_record_arg(values.pop(), "net.download_many", span)?;
            if let Some(value) =
                intercept_test_host_call(self, "net.download_many", batch.clone(), span)
            {
                lowered_runtime_value(value, span)?
            } else {
                let Some(values) = batch.get("downloads") else {
                    return Ok(ControlFlow::Continue(lowered_result_err_value(
                        RuntimeError::new("net-download-many", "downloads is required")
                            .with_span(span),
                    )));
                };
                let Value::List(records) = values else {
                    return Ok(ControlFlow::Continue(lowered_result_err_value(
                        RuntimeError::new("type-error", "downloads must be List[Record]")
                            .with_span(span),
                    )));
                };
                let downloads = records
                    .iter()
                    .map(|value| match value {
                        Value::Record(record) => {
                            self.net_download_from_record(record.clone(), span)
                        }
                        _ => Err(RuntimeError::new(
                            "type-error",
                            "downloads must be List[Record]",
                        )
                        .with_span(span)),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let concurrency = match batch.get("concurrency") {
                    Some(Value::Int(value)) if *value >= 0 => *value as usize,
                    Some(Value::Int(_)) => {
                        return Ok(ControlFlow::Continue(lowered_result_err_value(
                            RuntimeError::new(
                                "net-concurrency",
                                "concurrency must be at least one",
                            )
                            .with_span(span),
                        )));
                    }
                    Some(_) => {
                        return Ok(ControlFlow::Continue(lowered_result_err_value(
                            RuntimeError::new("type-error", "concurrency must be Int")
                                .with_span(span),
                        )));
                    }
                    None => 16,
                };
                if concurrency == 0 {
                    return Ok(ControlFlow::Continue(lowered_result_err_value(
                        RuntimeError::new(
                            "net-concurrency",
                            "concurrency must be at least one",
                        )
                        .with_span(span),
                    )));
                }
                let options = self.net_call_options(&batch, span)?;
                match self.net_agent(&options, span) {
                    Ok(agent) => match self.download_many_with_runtime(
                        agent,
                        downloads,
                        concurrency,
                        span,
                    ) {
                        Ok(value) => lowered_runtime_value(value, span)?,
                        Err(error) => lowered_result_err_value(error),
                    },
                    Err(error) => lowered_result_err_value(error),
                }
            }
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_net_download_values(
        &mut self, mut values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let record = lowered_record_arg(values.pop(), "net.download", span)?;
            if let Some(value) =
                intercept_test_host_call(self, "net.download", record.clone(), span)
            {
                lowered_runtime_value(value, span)?
            } else {
                let options = self.net_call_options(&record, span)?;
                let download = self.net_download_from_record(record, span)?;
                if let Err(error) = net_module::validate_download(&download, span) {
                    return Ok(ControlFlow::Continue(lowered_result_err_value(error)));
                }
                match self.net_agent(&options, span) {
                    Ok(agent) => {
                        let runtime = self
                            .net_runtime
                            .as_mut()
                            .expect("net agent initializes runtime");
                        match net_module::submit_download(
                            runtime,
                            agent,
                            download,
                            net_module::NetProtocol::Http1,
                            span,
                        ) {
                            Ok(operation) => match self
                                .wait_transport_operation(&operation, span, span, true)
                            {
                                Ok(response) => {
                                    lowered_runtime_value(Value::ok(response), span)?
                                }
                                Err(error) => lowered_result_err_value(error),
                            },
                            Err(error) => lowered_result_err_value(error),
                        }
                    }
                    Err(error) => lowered_result_err_value(error),
                }
            }
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_net_upload_values(
        &mut self, mut values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let record = lowered_record_arg(values.pop(), "net.upload", span)?;
            if let Some(value) =
                intercept_test_host_call(self, "net.upload", record.clone(), span)
            {
                lowered_runtime_value(value, span)?
            } else {
                let options = self.net_call_options(&record, span)?;
                let upload = self.net_upload_from_record(record, span)?;
                if let Err(error) = net_module::validate_upload(&upload, span) {
                    return Ok(ControlFlow::Continue(lowered_result_err_value(error)));
                }
                match self.net_agent(&options, span) {
                    Ok(agent) => {
                        let runtime = self
                            .net_runtime
                            .as_mut()
                            .expect("net agent initializes runtime");
                        match net_module::submit_upload(runtime, agent, upload, span) {
                            Ok(operation) => match self
                                .wait_transport_operation(&operation, span, span, true)
                            {
                                Ok(response) => {
                                    lowered_runtime_value(Value::ok(response), span)?
                                }
                                Err(error) => lowered_result_err_value(error),
                            },
                            Err(error) => lowered_result_err_value(error),
                        }
                    }
                    Err(error) => lowered_result_err_value(error),
                }
            }
        };
        Ok(ControlFlow::Continue(value))
    }
}
