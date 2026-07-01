#![allow(clippy::single_call_fn)]

#[cfg(feature = "net")]
use crate::runtime::value::RecordMap;
use crate::runtime::value::{RuntimeError, Value};
use crate::source::Span;
#[cfg(feature = "net")]
use std::sync::Arc;
use std::time::Duration;

#[cfg(feature = "net")]
pub(crate) use xsh_net::AddressFamily;

#[cfg(not(feature = "net"))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AddressFamily {
    Any,
    V4,
    V6,
}

#[cfg(not(feature = "net"))]
impl AddressFamily {
    pub(crate) fn from_name(name: &str) -> Result<Self, DnsNameError> {
        match name {
            "any" | "Any" | "ANY" => Ok(Self::Any),
            "4" | "v4" | "V4" | "ipv4" | "IPv4" | "IPV4" => Ok(Self::V4),
            "6" | "v6" | "V6" | "ipv6" | "IPv6" | "IPV6" => Ok(Self::V6),
            _ => Err(DnsNameError {
                kind: "dns-family".to_string(),
                message: "family must be `any`, `ipv4`, or `ipv6`".to_string(),
            }),
        }
    }
}

#[cfg(not(feature = "net"))]
pub(crate) struct DnsNameError {
    pub(crate) kind: String,
    pub(crate) message: String,
}

#[cfg(feature = "net")]
pub(crate) fn lookup(
    name: &str,
    record: &str,
    server: &str,
    timeout: Duration,
    span: Span,
) -> Result<Vec<Value>, RuntimeError> {
    xsh_net::lookup(name, record, server, timeout)
        .map(|records| records.into_iter().map(dns_record_value).collect())
        .map_err(|error| runtime_error(error, span))
}

#[cfg(not(feature = "net"))]
pub(crate) fn lookup(
    _name: &str,
    _record: &str,
    _server: &str,
    _timeout: Duration,
    span: Span,
) -> Result<Vec<Value>, RuntimeError> {
    Err(net_disabled(span))
}

#[cfg(feature = "net")]
pub(crate) fn resolve_host(
    name: &str,
    family: AddressFamily,
    span: Span,
) -> Result<Vec<Value>, RuntimeError> {
    xsh_net::resolve_host(name, family)
        .map(|records| records.into_iter().map(host_address_value).collect())
        .map_err(|error| runtime_error(error, span))
}

#[cfg(not(feature = "net"))]
pub(crate) fn resolve_host(
    _name: &str,
    _family: AddressFamily,
    span: Span,
) -> Result<Vec<Value>, RuntimeError> {
    Err(net_disabled(span))
}

#[cfg(feature = "net")]
pub(crate) fn reverse(addr: &str, span: Span) -> Result<Vec<Value>, RuntimeError> {
    xsh_net::reverse(addr)
        .map(|names| {
            names
                .into_iter()
                .map(|name| Value::Str(name.into()))
                .collect()
        })
        .map_err(|error| runtime_error(error, span))
}

#[cfg(not(feature = "net"))]
pub(crate) fn reverse(_addr: &str, span: Span) -> Result<Vec<Value>, RuntimeError> {
    Err(net_disabled(span))
}

#[cfg(feature = "net")]
pub(crate) fn nameservers(span: Span) -> Result<Vec<Value>, RuntimeError> {
    xsh_net::nameservers()
        .map(|names| {
            names
                .into_iter()
                .map(|name| Value::Str(name.into()))
                .collect()
        })
        .map_err(|error| runtime_error(error, span))
}

#[cfg(not(feature = "net"))]
pub(crate) fn nameservers(span: Span) -> Result<Vec<Value>, RuntimeError> {
    Err(net_disabled(span))
}

#[cfg(feature = "net")]
fn dns_record_value(record: xsh_net::DnsRecord) -> Value {
    Value::Record(RecordMap::from([
        (Arc::from("name"), Value::Str(record.name.into())),
        (Arc::from("record"), Value::Str(record.record.into())),
        (Arc::from("value"), Value::Str(record.value.into())),
        (Arc::from("ttl"), Value::Int(record.ttl)),
    ]))
}

#[cfg(feature = "net")]
fn host_address_value(record: xsh_net::HostAddress) -> Value {
    Value::Record(RecordMap::from([
        (Arc::from("name"), Value::Str(record.name.into())),
        (Arc::from("family"), Value::Str(record.family.into())),
        (Arc::from("addr"), Value::Str(record.addr.into())),
    ]))
}

#[cfg(feature = "net")]
fn runtime_error(error: xsh_net::NetError, span: Span) -> RuntimeError {
    RuntimeError::new(error.kind, error.message).with_span(span)
}

#[cfg(not(feature = "net"))]
fn net_disabled(span: Span) -> RuntimeError {
    RuntimeError::new("net-disabled", "net feature is disabled").with_span(span)
}
