#![allow(clippy::single_call_fn)]

#[cfg(feature = "net")]
use crate::runtime::value::RecordMap;
use crate::runtime::value::{RuntimeError, Value};
use crate::source::Span;
#[cfg(not(feature = "net"))]
use std::path::PathBuf;
#[cfg(feature = "net")]
use std::sync::Arc;
#[cfg(not(feature = "net"))]
use std::time::Duration;

#[cfg(feature = "net")]
pub(crate) use xsh_net::{
    NetAgent, NetAgentKey, NetBody, NetCallOptions, NetDownload, NetHeader, NetPoolOptions,
    NetRequest, NetUpload,
};

#[cfg(not(feature = "net"))]
#[derive(Clone, Debug)]
pub(crate) struct NetAgent;

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
    pub(crate) connect_timeout: Option<Duration>,
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
    pub(crate) connect_timeout: Option<Duration>,
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
    pub(crate) connect_timeout: Option<Duration>,
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
pub(crate) fn make_agent(key: &NetAgentKey, span: Span) -> Result<NetAgent, RuntimeError> {
    xsh_net::make_agent(key).map_err(|error| runtime_error(error, span))
}

#[cfg(not(feature = "net"))]
pub(crate) fn make_agent(_key: &NetAgentKey, span: Span) -> Result<NetAgent, RuntimeError> {
    Err(net_disabled(span))
}

#[cfg(feature = "net")]
pub(crate) fn request(
    agent: &NetAgent,
    request: NetRequest,
    span: Span,
) -> Result<Value, RuntimeError> {
    xsh_net::request(agent, request)
        .map(response_record)
        .map_err(|error| runtime_error(error, span))
}

#[cfg(not(feature = "net"))]
pub(crate) fn request(
    _agent: &NetAgent,
    _request: NetRequest,
    span: Span,
) -> Result<Value, RuntimeError> {
    Err(net_disabled(span))
}

#[cfg(feature = "net")]
pub(crate) fn download(
    agent: &NetAgent,
    download: NetDownload,
    span: Span,
) -> Result<Value, RuntimeError> {
    xsh_net::download(agent, download)
        .map(response_record)
        .map_err(|error| runtime_error(error, span))
}

#[cfg(not(feature = "net"))]
pub(crate) fn download(
    _agent: &NetAgent,
    _download: NetDownload,
    span: Span,
) -> Result<Value, RuntimeError> {
    Err(net_disabled(span))
}

#[cfg(feature = "net")]
pub(crate) fn upload(
    agent: &NetAgent,
    upload: NetUpload,
    span: Span,
) -> Result<Value, RuntimeError> {
    xsh_net::upload(agent, upload)
        .map(response_record)
        .map_err(|error| runtime_error(error, span))
}

#[cfg(not(feature = "net"))]
pub(crate) fn upload(
    _agent: &NetAgent,
    _upload: NetUpload,
    span: Span,
) -> Result<Value, RuntimeError> {
    Err(net_disabled(span))
}

#[cfg(feature = "net")]
fn response_record(response: xsh_net::NetResponse) -> Value {
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
    Value::ok(Value::Record(RecordMap::from(fields)))
}

#[cfg(feature = "net")]
fn runtime_error(error: xsh_net::NetError, span: Span) -> RuntimeError {
    RuntimeError::new(error.kind, error.message).with_span(span)
}

#[cfg(not(feature = "net"))]
fn net_disabled(span: Span) -> RuntimeError {
    RuntimeError::new("net-disabled", "net feature is disabled").with_span(span)
}
