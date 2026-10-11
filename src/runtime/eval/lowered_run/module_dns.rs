use super::{
    Evaluator, Arc, ControlFlow, Duration, LoweredValue, NativeArgumentValues, RecordMap,
    RuntimeError, Span, Value, dns_module, intercept_test_host_call, lowered_duration_arg,
    lowered_result_err_value, lowered_runtime_list_result, lowered_runtime_value,
    lowered_str_arg_owned,
};

impl Evaluator {
    pub(super) fn eval_lowered_dns_lookup_values(
        &mut self, values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let timeout = match values.get(3).cloned() {
                Some(value) => Duration::from_millis(
                    lowered_duration_arg(Some(value), "dns.lookup", span)?.millis,
                ),
                None => Duration::from_millis(5_000),
            };
            let server = lowered_str_arg_owned(values.get(2).cloned(), "", "dns.lookup", span)?;
            let record =
                lowered_str_arg_owned(values.get(1).cloned(), "A", "dns.lookup", span)?;
            let name = lowered_str_arg_owned(values.first().cloned(), "", "dns.lookup", span)?;
            let args = RecordMap::from([
                (Arc::from("name"), Value::Str(name.as_str().into())),
                (Arc::from("record"), Value::Str(record.as_str().into())),
                (Arc::from("server"), Value::Str(server.as_str().into())),
                (
                    Arc::from("timeout_ms"),
                    Value::Int(timeout.as_millis() as i64),
                ),
            ]);
            if let Some(value) = intercept_test_host_call(self, "dns.lookup", args, span) {
                lowered_runtime_value(value, span)?
            } else {
                lowered_runtime_list_result(
                    dns_module::lookup(&name, &record, &server, timeout, span),
                    span,
                )?
            }
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_dns_resolve_host_values(
        &mut self, values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let family =
                lowered_str_arg_owned(values.get(1).cloned(), "any", "dns.resolve_host", span)?;
            let name =
                lowered_str_arg_owned(values.first().cloned(), "", "dns.resolve_host", span)?;
            let args = RecordMap::from([
                (Arc::from("name"), Value::Str(name.as_str().into())),
                (Arc::from("family"), Value::Str(family.as_str().into())),
            ]);
            if let Some(value) = intercept_test_host_call(self, "dns.resolve_host", args, span)
            {
                lowered_runtime_value(value, span)?
            } else {
                match dns_module::AddressFamily::from_name(&family)
                    .map_err(|error| {
                        RuntimeError::new(error.kind, error.message).with_span(span)
                    })
                    .and_then(|family| dns_module::resolve_host(&name, family, span))
                {
                    Ok(records) => {
                        lowered_runtime_value(Value::ok(Value::List(records)), span)?
                    }
                    Err(error) => lowered_result_err_value(error),
                }
            }
        };
        Ok(ControlFlow::Continue(value))
    }
}
