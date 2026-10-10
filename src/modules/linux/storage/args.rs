//! Positional argument access for the storage primitives. An omitted
//! parameter is `None`, which is distinct from a supplied `null`.

use super::transport::{MAX_TRANSFER, Transfer};
use crate::runtime::value::{RuntimeError, Value};
use crate::source::Span;
use std::path::PathBuf;

pub(super) struct StorageArgs<'a> {
    name: &'static str,
    values: &'a [Option<Value>],
    span: Span,
}

impl<'a> StorageArgs<'a> {
    pub(super) fn new(name: &'static str, values: &'a [Option<Value>], span: Span) -> Self {
        Self { name, values, span }
    }

    pub(super) fn span(&self) -> Span {
        self.span
    }

    fn get(&self, index: usize) -> Option<&'a Value> {
        self.values.get(index).and_then(Option::as_ref)
    }

    fn type_error(&self, expected: &str, found: &Value) -> RuntimeError {
        RuntimeError::new(
            "type-error",
            format!("linux.{} expected {expected}, found {}", self.name, found.type_name()),
        )
        .with_span(self.span)
    }

    fn missing(&self) -> RuntimeError {
        RuntimeError::new("arity", format!("linux.{} expected an argument", self.name))
            .with_span(self.span)
    }

    fn range_error(&self, parameter: &str, detail: &str) -> RuntimeError {
        RuntimeError::new(
            "invalid-argument",
            format!("linux.{}: {parameter} {detail}", self.name),
        )
        .with_span(self.span)
    }

    fn int_or(&self, index: usize, default: i64) -> Result<i64, RuntimeError> {
        match self.get(index) {
            Some(Value::Int(value)) => Ok(*value),
            Some(other) => Err(self.type_error("Int", other)),
            None => Ok(default),
        }
    }

    /// A descriptor number. The kernel validates it on use, so a closed or
    /// foreign number fails with EBADF.
    pub(super) fn fd(&self, index: usize) -> Result<i32, RuntimeError> {
        let value = match self.get(index) {
            Some(Value::Int(value)) => *value,
            Some(other) => return Err(self.type_error("Int", other)),
            None => return Err(self.missing()),
        };
        i32::try_from(value)
            .ok()
            .filter(|fd| *fd >= 0)
            .ok_or_else(|| self.range_error("fd", "must be a non-negative descriptor number"))
    }

    /// An unsigned integer bounded by `max`, or `default` when omitted.
    pub(super) fn uint_or(
        &self,
        index: usize,
        parameter: &str,
        default: u64,
        max: u64,
    ) -> Result<u64, RuntimeError> {
        let value = self.int_or(index, i64::try_from(default).unwrap_or(i64::MAX))?;
        u64::try_from(value)
            .ok()
            .filter(|value| *value <= max)
            .ok_or_else(|| self.range_error(parameter, &format!("must be between 0 and {max}")))
    }

    pub(super) fn uint(
        &self,
        index: usize,
        parameter: &str,
        max: u64,
    ) -> Result<u64, RuntimeError> {
        if self.get(index).is_none() {
            return Err(self.missing());
        }
        self.uint_or(index, parameter, 0, max)
    }

    pub(super) fn timeout_ms(&self, index: usize, default: u32) -> Result<u32, RuntimeError> {
        let value = self.uint_or(index, "timeout_ms", u64::from(default), u64::from(u32::MAX))?;
        Ok(value as u32)
    }

    pub(super) fn bytes(&self, index: usize) -> Result<&'a [u8], RuntimeError> {
        match self.get(index) {
            Some(Value::Bytes(bytes)) => Ok(bytes),
            Some(other) => Err(self.type_error("Bytes", other)),
            None => Err(self.missing()),
        }
    }

    /// The data phase of a command: a length asks for a device-to-host
    /// transfer (zero or omitted means none) and Bytes, only where `write`
    /// allows, supplies a host-to-device payload.
    pub(super) fn transfer(&self, index: usize, write: bool) -> Result<Transfer<'a>, RuntimeError> {
        match self.get(index) {
            None => Ok(Transfer::None),
            Some(Value::Int(length)) => {
                let length = usize::try_from(*length)
                    .ok()
                    .filter(|length| *length <= MAX_TRANSFER)
                    .ok_or_else(|| {
                        self.range_error(
                            "data_len",
                            &format!("must be between 0 and {MAX_TRANSFER}"),
                        )
                    })?;
                Ok(if length == 0 { Transfer::None } else { Transfer::FromDevice(length) })
            }
            Some(Value::Bytes(bytes)) if write => {
                if bytes.is_empty() || bytes.len() > MAX_TRANSFER {
                    return Err(self.range_error(
                        "data",
                        &format!("must hold 1 to {MAX_TRANSFER} bytes"),
                    ));
                }
                Ok(Transfer::ToDevice(bytes))
            }
            Some(other) => Err(self.type_error(if write { "Int or Bytes" } else { "Int" }, other)),
        }
    }

    pub(super) fn str(&self, index: usize) -> Result<String, RuntimeError> {
        match self.get(index) {
            Some(Value::Str(value)) => Ok(value.to_string()),
            Some(other) => Err(self.type_error("Str", other)),
            None => Err(self.missing()),
        }
    }

    pub(super) fn bool_or(&self, index: usize, default: bool) -> Result<bool, RuntimeError> {
        match self.get(index) {
            Some(Value::Bool(value)) => Ok(*value),
            Some(other) => Err(self.type_error("Bool", other)),
            None => Ok(default),
        }
    }

    /// An optional path; omitted and `null` both mean absent.
    pub(super) fn path_opt(&self, index: usize) -> Result<Option<PathBuf>, RuntimeError> {
        use std::os::unix::ffi::OsStringExt;
        match self.get(index) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::Path(path)) => Ok(Some(PathBuf::from(std::ffi::OsString::from_vec(
                path.bytes.clone(),
            )))),
            Some(other) => Err(self.type_error("Path", other)),
        }
    }
}
