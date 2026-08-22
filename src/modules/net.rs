#![allow(clippy::single_call_fn)]

#[cfg(feature = "net")]
use crate::runtime::value::RecordMap;
use crate::runtime::value::{RuntimeError, Value};
use crate::source::Span;
#[cfg(not(feature = "net"))]
use std::path::PathBuf;
#[cfg(feature = "net")]
use std::sync::Arc;
use std::time::Duration;

#[cfg(feature = "net")]
pub(crate) use xsh_net::{
    NetAgent, NetAgentKey, NetBody, NetCallOptions, NetDownload, NetHeader, NetOperation,
    NetOperationMetrics, NetPoolOptions, NetProtocol, NetRequest, NetRuntimeOwner, NetUpload,
};

#[cfg(not(feature = "net"))]
#[derive(Clone, Debug)]
pub(crate) struct NetAgent;

#[cfg(not(feature = "net"))]
#[derive(Debug)]
pub(crate) struct NetRuntimeOwner;

#[cfg(not(feature = "net"))]
impl NetRuntimeOwner {
    pub(crate) fn new() -> Result<Self, &'static str> {
        Err("net feature is disabled")
    }

    pub(crate) fn shutdown(&mut self) {}
}

#[cfg(not(feature = "net"))]
#[derive(Debug)]
pub(crate) struct NetOperation;

#[cfg(not(feature = "net"))]
#[derive(Clone, Copy, Debug)]
pub(crate) enum NetProtocol {
    Http1,
    Auto,
}

#[cfg(not(feature = "net"))]
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub(crate) struct NetAgentKey {
    pub(crate) pool: String,
    pub(crate) tls_verify: bool,
    pub(crate) ca_certificate: Option<PathBuf>,
    pub(crate) max_idle_per_host: usize,
    pub(crate) idle_timeout: Duration,
}

#[cfg(not(feature = "net"))]
#[derive(Clone, Debug)]
pub(crate) struct NetPoolOptions {
    pub(crate) max_idle_per_host: usize,
    pub(crate) idle_timeout: Duration,
}

#[cfg(not(feature = "net"))]
impl Default for NetPoolOptions {
    fn default() -> Self {
        Self {
            max_idle_per_host: 8,
            idle_timeout: Duration::from_secs(90),
        }
    }
}

#[cfg(not(feature = "net"))]
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) struct NetRequest {
    pub(crate) method: String,
    pub(crate) url: String,
    pub(crate) headers: Vec<NetHeader>,
    pub(crate) body: NetBody,
    pub(crate) timeout: Option<Duration>,
    pub(crate) dns_timeout: Option<Duration>,
    pub(crate) connect_timeout: Option<Duration>,
    pub(crate) tls_timeout: Option<Duration>,
    pub(crate) headers_timeout: Option<Duration>,
    pub(crate) body_idle_timeout: Option<Duration>,
    pub(crate) redirects: usize,
    pub(crate) fail_status: bool,
    pub(crate) max_body_bytes: u64,
}

#[cfg(not(feature = "net"))]
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) struct NetDownload {
    pub(crate) url: String,
    pub(crate) dest: PathBuf,
    pub(crate) headers: Vec<NetHeader>,
    pub(crate) timeout: Option<Duration>,
    pub(crate) dns_timeout: Option<Duration>,
    pub(crate) connect_timeout: Option<Duration>,
    pub(crate) tls_timeout: Option<Duration>,
    pub(crate) headers_timeout: Option<Duration>,
    pub(crate) body_idle_timeout: Option<Duration>,
    pub(crate) redirects: usize,
    pub(crate) fail_status: bool,
    pub(crate) max_body_bytes: Option<u64>,
    pub(crate) atomic: bool,
    pub(crate) overwrite: bool,
}

#[cfg(not(feature = "net"))]
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) struct NetUpload {
    pub(crate) method: String,
    pub(crate) url: String,
    pub(crate) source: PathBuf,
    pub(crate) headers: Vec<NetHeader>,
    pub(crate) timeout: Option<Duration>,
    pub(crate) dns_timeout: Option<Duration>,
    pub(crate) connect_timeout: Option<Duration>,
    pub(crate) tls_timeout: Option<Duration>,
    pub(crate) headers_timeout: Option<Duration>,
    pub(crate) body_idle_timeout: Option<Duration>,
    pub(crate) redirects: usize,
    pub(crate) fail_status: bool,
    pub(crate) max_body_bytes: u64,
}

#[cfg(not(feature = "net"))]
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) struct NetHeader {
    pub(crate) name: String,
    pub(crate) value: String,
}

#[cfg(not(feature = "net"))]
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) enum NetBody {
    Empty,
    Bytes(Vec<u8>),
    File(PathBuf),
}

#[cfg(not(feature = "net"))]
#[derive(Clone, Debug)]
pub(crate) struct NetCallOptions {
    pub(crate) pool: String,
    pub(crate) tls_verify: bool,
    pub(crate) ca_certificate: Option<PathBuf>,
}

#[cfg(feature = "net")]
pub(crate) fn make_agent(
    key: &NetAgentKey,
    runtime: &NetRuntimeOwner,
    span: Span,
) -> Result<NetAgent, RuntimeError> {
    xsh_net::make_agent(key, runtime.executor()).map_err(|error| runtime_error(error, span))
}

#[cfg(feature = "net")]
pub(crate) fn validate_request(request: &NetRequest, span: Span) -> Result<(), RuntimeError> {
    xsh_net::validate_request(request).map_err(|error| runtime_error(error, span))
}

#[cfg(feature = "net")]
pub(crate) fn validate_download(download: &NetDownload, span: Span) -> Result<(), RuntimeError> {
    xsh_net::validate_download(download).map_err(|error| runtime_error(error, span))
}

#[cfg(feature = "net")]
pub(crate) fn validate_upload(upload: &NetUpload, span: Span) -> Result<(), RuntimeError> {
    xsh_net::validate_upload(upload).map_err(|error| runtime_error(error, span))
}

#[cfg(not(feature = "net"))]
pub(crate) fn validate_request(_request: &NetRequest, span: Span) -> Result<(), RuntimeError> {
    Err(net_disabled(span))
}

#[cfg(not(feature = "net"))]
pub(crate) fn validate_download(_download: &NetDownload, span: Span) -> Result<(), RuntimeError> {
    Err(net_disabled(span))
}

#[cfg(not(feature = "net"))]
pub(crate) fn validate_upload(_upload: &NetUpload, span: Span) -> Result<(), RuntimeError> {
    Err(net_disabled(span))
}

#[cfg(not(feature = "net"))]
pub(crate) fn make_agent(
    _key: &NetAgentKey,
    _runtime: &NetRuntimeOwner,
    span: Span,
) -> Result<NetAgent, RuntimeError> {
    Err(net_disabled(span))
}

#[cfg(feature = "net")]
pub(crate) fn submit_request(
    runtime: &mut NetRuntimeOwner,
    agent: NetAgent,
    request: NetRequest,
    protocol: NetProtocol,
    span: Span,
) -> Result<NetOperation, RuntimeError> {
    runtime
        .submit_request(agent, request, protocol)
        .map_err(|error| runtime_error(error, span))
}

#[cfg(feature = "net")]
pub(crate) fn submit_download(
    runtime: &mut NetRuntimeOwner,
    agent: NetAgent,
    download: NetDownload,
    protocol: NetProtocol,
    span: Span,
) -> Result<NetOperation, RuntimeError> {
    runtime
        .submit_download(agent, download, protocol)
        .map_err(|error| runtime_error(error, span))
}

#[cfg(feature = "net")]
pub(crate) fn submit_upload(
    runtime: &mut NetRuntimeOwner,
    agent: NetAgent,
    upload: NetUpload,
    span: Span,
) -> Result<NetOperation, RuntimeError> {
    runtime
        .submit_upload(agent, upload)
        .map_err(|error| runtime_error(error, span))
}

#[cfg(not(feature = "net"))]
pub(crate) fn submit_request(
    _runtime: &mut NetRuntimeOwner,
    _agent: NetAgent,
    _request: NetRequest,
    _protocol: NetProtocol,
    span: Span,
) -> Result<NetOperation, RuntimeError> {
    Err(net_disabled(span))
}

#[cfg(not(feature = "net"))]
pub(crate) fn submit_download(
    _runtime: &mut NetRuntimeOwner,
    _agent: NetAgent,
    _download: NetDownload,
    _protocol: NetProtocol,
    span: Span,
) -> Result<NetOperation, RuntimeError> {
    Err(net_disabled(span))
}

#[cfg(not(feature = "net"))]
pub(crate) fn submit_upload(
    _runtime: &mut NetRuntimeOwner,
    _agent: NetAgent,
    _upload: NetUpload,
    span: Span,
) -> Result<NetOperation, RuntimeError> {
    Err(net_disabled(span))
}

#[cfg(feature = "net")]
pub(crate) fn receive_any(
    operations: &[&NetOperation],
    timeout: Duration,
    span: Span,
) -> Result<Option<(usize, Result<Value, RuntimeError>)>, RuntimeError> {
    xsh_net::receive_any(operations, timeout)
        .map(|completion| {
            completion.map(|(index, result)| {
                (
                    index,
                    result
                        .map(response_value)
                        .map_err(|error| runtime_error(error, span)),
                )
            })
        })
        .map_err(|error| runtime_error(error, span))
}

#[cfg(not(feature = "net"))]
#[allow(dead_code)]
pub(crate) fn receive_any(
    _operations: &[&NetOperation],
    _timeout: Duration,
    span: Span,
) -> Result<Option<(usize, Result<Value, RuntimeError>)>, RuntimeError> {
    Err(net_disabled(span))
}

#[cfg(feature = "net")]
pub(crate) fn response_value(response: xsh_net::NetResponse) -> Value {
    let headers = response
        .headers
        .into_iter()
        .map(|header| {
            Value::Record(RecordMap::from([
                (Arc::from("name"), Value::Str(header.name.into())),
                (Arc::from("value"), Value::Str(header.value.into())),
            ]))
        })
        .collect();
    let mut fields = std::collections::BTreeMap::from([
        (Arc::from("status"), Value::Int(response.status)),
        (Arc::from("reason"), Value::Str(response.reason.into())),
        (Arc::from("bytes"), Value::Int(response.bytes)),
        (Arc::from("headers"), Value::List(headers)),
        (Arc::from("url"), Value::Str(response.url.into())),
    ]);
    if let Some(body) = response.body {
        fields.insert(Arc::from("body"), Value::Bytes(body));
    }
    Value::Record(RecordMap::from(fields))
}

#[cfg(feature = "net")]
fn runtime_error(error: xsh_net::NetError, span: Span) -> RuntimeError {
    RuntimeError::new(error.kind, error.message).with_span(span)
}

#[cfg(not(feature = "net"))]
fn net_disabled(span: Span) -> RuntimeError {
    RuntimeError::new("net-disabled", "net feature is disabled").with_span(span)
}
