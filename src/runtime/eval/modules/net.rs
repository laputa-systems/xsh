use super::{
    Evaluator, net_body_from_record, record_bool, record_duration, record_headers,
    record_nonnegative_usize, record_optional_positive_u64, record_path, record_positive_u64,
    record_str, value_to_path,
};
use crate::modules::net::{
    self, NetAgentKey, NetCallOptions, NetDownload, NetOperation, NetProtocol, NetRequest,
    NetUpload,
};
use crate::runtime::value::{RecordMap, RuntimeError};
use crate::source::Span;
use std::collections::VecDeque;
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;

impl Evaluator {
    pub(in crate::runtime::eval) fn net_agent(
        &mut self,
        options: &NetCallOptions,
        span: Span,
    ) -> Result<net::NetAgent, RuntimeError> {
        let pool = self
            .net_pool_options
            .get(&options.pool)
            .cloned()
            .unwrap_or_default();
        let key = NetAgentKey {
            pool: options.pool.clone(),
            tls_verify: options.tls_verify,
            ca_certificate: options.ca_certificate.clone(),
            max_idle_per_host: pool.max_idle_per_host,
            idle_timeout: pool.idle_timeout,
        };
        if let Some(agent) = self.net_agents.get(&key) {
            return Ok(agent.clone());
        }
        let runtime = match self.net_runtime.as_mut() {
            Some(runtime) => runtime,
            None => {
                let runtime = net::NetRuntimeOwner::new().map_err(|error| {
                    RuntimeError::new("net-runtime", error.to_string()).with_span(span)
                })?;
                self.net_runtime.insert(runtime)
            }
        };
        let agent = net::make_agent(&key, runtime, span)?;
        self.net_agents.insert(key, agent.clone());
        Ok(agent)
    }

    pub(in crate::runtime::eval) fn net_call_options(
        &mut self,
        record: &RecordMap,
        span: Span,
    ) -> Result<NetCallOptions, RuntimeError> {
        let pool = record_str(record, "pool", Some("default"), span)?;
        let tls_verify = record_bool(record, "tls_verify", true, span)?;
        let ca_certificate = match record.get("ca_certificate") {
            Some(value) => Some(self.host_path(&value_to_path(value, "ca_certificate", span)?)),
            None => self.ssl_cert_file_from_env(),
        };
        Ok(NetCallOptions {
            pool,
            tls_verify,
            ca_certificate,
        })
    }

    pub(in crate::runtime::eval) fn net_request_from_record(
        &mut self,
        record: RecordMap,
        span: Span,
    ) -> Result<NetRequest, RuntimeError> {
        let body = net_body_from_record(self, &record, span)?;
        Ok(NetRequest {
            method: record_str(&record, "method", None, span)?,
            url: record_str(&record, "url", None, span)?,
            headers: record_headers(&record, span)?,
            body,
            timeout: record_duration(&record, "timeout", span)?,
            dns_timeout: record_duration(&record, "dns_timeout", span)?,
            connect_timeout: record_duration(&record, "connect_timeout", span)?,
            tls_timeout: record_duration(&record, "tls_timeout", span)?,
            headers_timeout: record_duration(&record, "headers_timeout", span)?,
            body_idle_timeout: record_duration(&record, "body_idle_timeout", span)?,
            redirects: record_nonnegative_usize(&record, "redirects", 3, span)?,
            fail_status: record_bool(&record, "fail_status", false, span)?,
            max_body_bytes: record_positive_u64(&record, "max_body_bytes", 10 * 1024 * 1024, span)?,
        })
    }

    pub(in crate::runtime::eval) fn net_download_from_record(
        &mut self,
        record: RecordMap,
        span: Span,
    ) -> Result<NetDownload, RuntimeError> {
        let dest = record_path(&record, "dest", span)?;
        Ok(NetDownload {
            url: record_str(&record, "url", None, span)?,
            dest: self.host_path(&dest),
            headers: record_headers(&record, span)?,
            timeout: record_duration(&record, "timeout", span)?,
            dns_timeout: record_duration(&record, "dns_timeout", span)?,
            connect_timeout: record_duration(&record, "connect_timeout", span)?,
            tls_timeout: record_duration(&record, "tls_timeout", span)?,
            headers_timeout: record_duration(&record, "headers_timeout", span)?,
            body_idle_timeout: record_duration(&record, "body_idle_timeout", span)?,
            redirects: record_nonnegative_usize(&record, "redirects", 3, span)?,
            fail_status: record_bool(&record, "fail_status", false, span)?,
            max_body_bytes: record_optional_positive_u64(&record, "max_body_bytes", span)?,
            atomic: record_bool(&record, "atomic", true, span)?,
            overwrite: record_bool(&record, "overwrite", false, span)?,
        })
    }

    pub(in crate::runtime::eval) fn net_upload_from_record(
        &mut self,
        record: RecordMap,
        span: Span,
    ) -> Result<NetUpload, RuntimeError> {
        let source = record_path(&record, "source", span)?;
        Ok(NetUpload {
            method: record_str(&record, "method", Some("PUT"), span)?,
            url: record_str(&record, "url", None, span)?,
            source: self.host_path(&source),
            headers: record_headers(&record, span)?,
            timeout: record_duration(&record, "timeout", span)?,
            dns_timeout: record_duration(&record, "dns_timeout", span)?,
            connect_timeout: record_duration(&record, "connect_timeout", span)?,
            tls_timeout: record_duration(&record, "tls_timeout", span)?,
            headers_timeout: record_duration(&record, "headers_timeout", span)?,
            body_idle_timeout: record_duration(&record, "body_idle_timeout", span)?,
            redirects: record_nonnegative_usize(&record, "redirects", 3, span)?,
            fail_status: record_bool(&record, "fail_status", false, span)?,
            max_body_bytes: record_positive_u64(&record, "max_body_bytes", 10 * 1024 * 1024, span)?,
        })
    }

    pub(in crate::runtime::eval) fn ssl_cert_file_from_env(&self) -> Option<std::path::PathBuf> {
        self.env
            .get_owned(b"SSL_CERT_FILE".as_slice())
            .map(|value| std::path::PathBuf::from(OsString::from_vec(value)))
    }

    /// Schedules a request batch by completion rather than input partition.
    ///
    /// The evaluator submits only the current concurrency window. Each terminal
    /// completion immediately admits the next input, while `results` preserves
    /// the caller's original order. All work uses the evaluator's persistent
    /// `NetRuntimeOwner` and the pool's reusable HTTP/2-capable agent.
    pub(in crate::runtime::eval) fn request_many_with_runtime(
        &mut self,
        agent: net::NetAgent,
        requests: Vec<NetRequest>,
        concurrency: usize,
        span: Span,
    ) -> Result<crate::runtime::value::Value, RuntimeError> {
        let mut pending = requests.into_iter().enumerate().collect::<VecDeque<_>>();
        let mut active = Vec::<(usize, NetOperation)>::new();
        let mut results = (0..pending.len()).map(|_| None).collect::<Vec<_>>();

        while !pending.is_empty() || !active.is_empty() {
            while active.len() < concurrency {
                let Some((index, request)) = pending.pop_front() else {
                    break;
                };
                let submitted = {
                    let runtime = self
                        .net_runtime
                        .as_mut()
                        .expect("network agent initializes runtime");
                    net::submit_request(
                        runtime,
                        agent.clone(),
                        request.clone(),
                        NetProtocol::Auto,
                        span,
                    )
                };
                match submitted {
                    Ok(operation) => active.push((index, operation)),
                    Err(error) if error.kind == "net-overload" && !active.is_empty() => {
                        // An unrelated live job consumed the global admission
                        // window. Wait for this batch's next completion, then
                        // retry this input without losing its position.
                        pending.push_front((index, request));
                        break;
                    }
                    Err(error) => {
                        self.cancel_net_batch_operations(
                            active.into_iter().map(|(_, operation)| operation).collect(),
                            span,
                        );
                        return Err(error);
                    }
                }
            }

            if active.is_empty() {
                // A batch cannot make progress if another job owns the entire
                // global window; report that structured admission failure to
                // its caller instead of spinning a host thread.
                return Err(RuntimeError::new(
                    "net-overload",
                    "network operation admission is full",
                )
                .with_span(span));
            }

            let operations = active
                .iter()
                .map(|(_, operation)| operation)
                .collect::<Vec<_>>();
            let completed = self.wait_for_net_batch_completion(&operations, span);
            let (active_index, result) = match completed {
                Ok(completed) => completed,
                Err(error) => {
                    self.cancel_net_batch_operations(
                        active.into_iter().map(|(_, operation)| operation).collect(),
                        span,
                    );
                    return Err(error);
                }
            };
            let (input_index, _) = active.remove(active_index);
            if result
                .as_ref()
                .is_err_and(|error| error.kind == "net-runtime")
            {
                self.cancel_net_batch_operations(
                    active.into_iter().map(|(_, operation)| operation).collect(),
                    span,
                );
                return Err(result.expect_err("network runtime result was checked"));
            }
            results[input_index] = Some(result);
        }

        Ok(batch_results_value(results, span))
    }

    /// The download counterpart to `request_many_with_runtime`; it shares the
    /// completion-driven scheduler while keeping download file handling inside
    /// `xsh-net`.
    pub(in crate::runtime::eval) fn download_many_with_runtime(
        &mut self,
        agent: net::NetAgent,
        downloads: Vec<NetDownload>,
        concurrency: usize,
        span: Span,
    ) -> Result<crate::runtime::value::Value, RuntimeError> {
        let mut pending = downloads.into_iter().enumerate().collect::<VecDeque<_>>();
        let mut active = Vec::<(usize, NetOperation)>::new();
        let mut results = (0..pending.len()).map(|_| None).collect::<Vec<_>>();

        while !pending.is_empty() || !active.is_empty() {
            while active.len() < concurrency {
                let Some((index, download)) = pending.pop_front() else {
                    break;
                };
                let submitted = {
                    let runtime = self
                        .net_runtime
                        .as_mut()
                        .expect("network agent initializes runtime");
                    net::submit_download(
                        runtime,
                        agent.clone(),
                        download.clone(),
                        NetProtocol::Auto,
                        span,
                    )
                };
                match submitted {
                    Ok(operation) => active.push((index, operation)),
                    Err(error) if error.kind == "net-overload" && !active.is_empty() => {
                        pending.push_front((index, download));
                        break;
                    }
                    Err(error) => {
                        self.cancel_net_batch_operations(
                            active.into_iter().map(|(_, operation)| operation).collect(),
                            span,
                        );
                        return Err(error);
                    }
                }
            }

            if active.is_empty() {
                return Err(RuntimeError::new(
                    "net-overload",
                    "network operation admission is full",
                )
                .with_span(span));
            }

            let operations = active
                .iter()
                .map(|(_, operation)| operation)
                .collect::<Vec<_>>();
            let completed = self.wait_for_net_batch_completion(&operations, span);
            let (active_index, result) = match completed {
                Ok(completed) => completed,
                Err(error) => {
                    self.cancel_net_batch_operations(
                        active.into_iter().map(|(_, operation)| operation).collect(),
                        span,
                    );
                    return Err(error);
                }
            };
            let (input_index, _) = active.remove(active_index);
            if result
                .as_ref()
                .is_err_and(|error| error.kind == "net-runtime")
            {
                self.cancel_net_batch_operations(
                    active.into_iter().map(|(_, operation)| operation).collect(),
                    span,
                );
                return Err(result.expect_err("network runtime result was checked"));
            }
            results[input_index] = Some(result);
        }

        Ok(batch_results_value(results, span))
    }
}

fn batch_results_value(
    results: Vec<Option<Result<crate::runtime::value::Value, RuntimeError>>>,
    span: Span,
) -> crate::runtime::value::Value {
    use crate::runtime::value::Value;

    Value::ok(Value::List(
        results
            .into_iter()
            .map(
                |result| match result.expect("network batch result must be terminal") {
                    Ok(response) => Value::ok(response),
                    Err(error) => Value::err(Value::Error(Box::new(error.with_span(span)))),
                },
            )
            .collect(),
    ))
}
