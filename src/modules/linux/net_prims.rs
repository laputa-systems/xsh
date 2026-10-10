//! Generic Linux network boundary: sockets, netlink, and ioctl.
//!
//! These primitives move bytes and addresses between a script and the kernel
//! and nothing more. Packet and message layouts (ICMP, DNS, netlink payloads,
//! ethtool commands) stay with the script, which reads and writes them with
//! the `bytes` module. Every failure is a host error that carries its errno,
//! and every descriptor a primitive creates is returned to the script (which
//! closes it with `unix.close_fd`) or closed before the primitive returns.

// Only the Linux implementation consumes the argument and record helpers.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use crate::modules::RuntimeOp;
use crate::runtime::value::{RecordMap, RuntimeError, Value};
use crate::source::Span;
use std::sync::Arc;

#[cfg(target_os = "linux")]
mod address;
#[cfg(target_os = "linux")]
mod fake;
#[cfg(target_os = "linux")]
mod ioctl;
#[cfg(target_os = "linux")]
mod netlink;
#[cfg(target_os = "linux")]
mod sockets;

/// Whether `op` is one of the socket, netlink, or ioctl primitives.
pub(crate) fn handles(op: RuntimeOp) -> bool {
    operation_kind(op).is_some()
}

/// Whether a native-test fake with a netlink fixture may answer `op`. The
/// fake answers only for descriptors it opened itself; every other descriptor
/// stays on the real primitive.
pub(crate) fn fakeable(op: RuntimeOp) -> bool {
    matches!(
        op,
        RuntimeOp::LinuxNetlinkOpen
            | RuntimeOp::LinuxNetlinkRequest
            | RuntimeOp::LinuxGenlFamilyId
            | RuntimeOp::LinuxRecvfrom
            | RuntimeOp::LinuxSetsockoptInt
            | RuntimeOp::LinuxSetSocketTimeout
    )
}

/// The error kind of an operation, which is also the label in argument errors.
fn operation_kind(op: RuntimeOp) -> Option<&'static str> {
    Some(match op {
        RuntimeOp::LinuxNetConstants => "linux-net-constants",
        RuntimeOp::LinuxSocket => "linux-socket",
        RuntimeOp::LinuxConnect => "linux-connect",
        RuntimeOp::LinuxBind => "linux-bind",
        RuntimeOp::LinuxListen => "linux-listen",
        RuntimeOp::LinuxAccept => "linux-accept",
        RuntimeOp::LinuxSendto => "linux-sendto",
        RuntimeOp::LinuxRecvfrom => "linux-recvfrom",
        RuntimeOp::LinuxShutdown => "linux-shutdown",
        RuntimeOp::LinuxGetsockname => "linux-getsockname",
        RuntimeOp::LinuxGetpeername => "linux-getpeername",
        RuntimeOp::LinuxSetsockoptInt => "linux-setsockopt",
        RuntimeOp::LinuxGetsockoptInt => "linux-getsockopt",
        RuntimeOp::LinuxSetsockoptBytes => "linux-setsockopt",
        RuntimeOp::LinuxGetsockoptBytes => "linux-getsockopt",
        RuntimeOp::LinuxSetSocketTimeout => "linux-set-socket-timeout",
        RuntimeOp::LinuxNetlinkOpen => "linux-netlink-open",
        RuntimeOp::LinuxNetlinkRequest => "linux-netlink-request",
        RuntimeOp::LinuxGenlFamilyId => "linux-genl-family-id",
        RuntimeOp::LinuxIoctl => "linux-ioctl",
        _ => return None,
    })
}

/// The arguments of one native call. A slot is `None` when the caller left
/// the parameter to its default.
pub(super) struct Args<'a> {
    kind: &'static str,
    values: &'a [Option<Value>],
    span: Span,
}

impl<'a> Args<'a> {
    fn get(&self, index: usize) -> Option<&'a Value> {
        match self.values.get(index) {
            Some(Some(Value::Null)) | Some(None) | None => None,
            Some(Some(value)) => Some(value),
        }
    }

    fn type_error(&self, expected: &str, found: &Value) -> RuntimeError {
        RuntimeError::new(
            "type-error",
            format!("{} expected {expected}, found {}", self.kind, found.type_name()),
        )
        .with_span(self.span)
    }

    pub(super) fn invalid(&self, message: impl Into<String>) -> RuntimeError {
        RuntimeError::new("invalid-argument", message.into()).with_span(self.span)
    }

    /// A failed system call, keeping its errno.
    pub(super) fn os(&self, errno: rustix::io::Errno) -> RuntimeError {
        self.errno(errno.into())
    }

    pub(super) fn errno(&self, error: std::io::Error) -> RuntimeError {
        RuntimeError::host(self.kind, &error).with_span(self.span)
    }

    fn missing(&self) -> RuntimeError {
        RuntimeError::new("arity", format!("{} expected an argument", self.kind)).with_span(self.span)
    }

    pub(super) fn int(&self, index: usize) -> Result<i64, RuntimeError> {
        match self.get(index) {
            Some(Value::Int(value)) => Ok(*value),
            Some(other) => Err(self.type_error("Int", other)),
            None => Err(self.missing()),
        }
    }

    pub(super) fn int_or(&self, index: usize, default: i64) -> Result<i64, RuntimeError> {
        match self.get(index) {
            Some(Value::Int(value)) => Ok(*value),
            Some(other) => Err(self.type_error("Int", other)),
            None => Ok(default),
        }
    }

    pub(super) fn bool_or(&self, index: usize, default: bool) -> Result<bool, RuntimeError> {
        match self.get(index) {
            Some(Value::Bool(value)) => Ok(*value),
            Some(other) => Err(self.type_error("Bool", other)),
            None => Ok(default),
        }
    }

    pub(super) fn str(&self, index: usize) -> Result<&'a str, RuntimeError> {
        match self.get(index) {
            Some(Value::Str(value)) => Ok(value),
            Some(other) => Err(self.type_error("Str", other)),
            None => Err(self.missing()),
        }
    }

    pub(super) fn bytes(&self, index: usize) -> Result<&'a [u8], RuntimeError> {
        match self.get(index) {
            Some(Value::Bytes(value)) => Ok(value),
            Some(other) => Err(self.type_error("Bytes", other)),
            None => Err(self.missing()),
        }
    }

    /// An int that must fit the C type the kernel takes.
    pub(super) fn int_in<T: TryFrom<i64>>(&self, index: usize, what: &str) -> Result<T, RuntimeError> {
        let value = self.int(index)?;
        T::try_from(value).map_err(|_| self.invalid(format!("{what} {value} is out of range")))
    }

    pub(super) fn int_in_or<T: TryFrom<i64>>(
        &self,
        index: usize,
        default: i64,
        what: &str,
    ) -> Result<T, RuntimeError> {
        let value = self.int_or(index, default)?;
        T::try_from(value).map_err(|_| self.invalid(format!("{what} {value} is out of range")))
    }

    pub(super) fn fd(&self, index: usize) -> Result<libc::c_int, RuntimeError> {
        match libc::c_int::try_from(self.int(index)?) {
            Ok(fd) if fd >= 0 => Ok(fd),
            _ => Err(self.invalid("fd must be a non-negative descriptor number")),
        }
    }
}

/// A named constant of the shared table; the names used here are static, so a
/// missing one is a programming error.
pub(super) fn constant(name: &str) -> i64 {
    xsh_registry::records::NET_CONSTANTS
        .iter()
        .find_map(|(constant, value)| (*constant == name).then_some(*value))
        .unwrap_or_else(|| panic!("{name} is not in the network constant table"))
}

pub(super) fn key(name: &str) -> Arc<str> {
    Arc::from(name)
}

pub(super) fn record<const N: usize>(fields: [(&str, Value); N]) -> Value {
    Value::Record(RecordMap::from(fields.map(|(name, value)| (key(name), value))))
}

pub(super) fn ok(value: Value) -> Result<Value, RuntimeError> {
    Ok(Value::ok(value))
}

#[cfg(target_os = "linux")]
pub(crate) fn call(
    op: RuntimeOp,
    values: &[Option<Value>],
    span: Span,
) -> Result<Value, RuntimeError> {
    let kind = operation_kind(op).expect("socket primitive expected");
    let args = Args { kind, values, span };
    match op {
        RuntimeOp::LinuxNetConstants => Ok(constants()),
        RuntimeOp::LinuxIoctl => ioctl::ioctl(&args),
        RuntimeOp::LinuxNetlinkOpen
        | RuntimeOp::LinuxNetlinkRequest
        | RuntimeOp::LinuxGenlFamilyId => netlink::call(op, &args),
        _ => sockets::call(op, &args),
    }
}

/// Answers `op` from the recorded netlink fixture at `fixture`, or returns
/// `None` when the descriptor is not one the fake opened.
#[cfg(target_os = "linux")]
pub(crate) fn fake_call(
    op: RuntimeOp,
    values: &[Option<Value>],
    fixture: &std::path::Path,
    log: &mut fake::Log<'_>,
    span: Span,
) -> Option<Result<Value, RuntimeError>> {
    let kind = operation_kind(op)?;
    let args = Args { kind, values, span };
    fake::call(op, &args, fixture, log)
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn fake_call(
    _op: RuntimeOp,
    _values: &[Option<Value>],
    _fixture: &std::path::Path,
    _log: &mut dyn FnMut(&str, &[(&str, String)]) -> Result<(), RuntimeError>,
    _span: Span,
) -> Option<Result<Value, RuntimeError>> {
    None
}

fn constants() -> Value {
    Value::Record(
        xsh_registry::records::NET_CONSTANTS
            .iter()
            .map(|(name, value)| (key(name), Value::Int(*value)))
            .collect(),
    )
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn call(
    op: RuntimeOp,
    _values: &[Option<Value>],
    span: Span,
) -> Result<Value, RuntimeError> {
    if op == RuntimeOp::LinuxNetConstants {
        return Ok(constants());
    }
    Err(RuntimeError::new(
        "unsupported",
        format!("{} requires Linux", operation_kind(op).expect("socket primitive expected")),
    )
    .with_span(span))
}
