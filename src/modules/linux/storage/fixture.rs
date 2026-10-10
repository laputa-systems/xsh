//! The fixture backend: recorded command/response pairs read from a
//! JSON Lines file, one object per line.
//!
//! Request fields select a line; response fields are what the device
//! "returned". Binary fields are base64 text. A request that matches no line
//! fails loudly, so a test cannot reach a device by accident. When several
//! lines match, the first wins; the file is re-read on every command, so a
//! test can swap responses between calls.
//!
//! `sg_io` lines: request `cdb`, `direction` (`none`, `from_device`,
//! `to_device`; default `none`), `data_len` (from_device) or `data_out`
//! (to_device); response `status`, `host_status`, `driver_status`, `sense`,
//! `data`, `resid`, all defaulting to zero or empty, or `errno` to fail the
//! ioctl.
//!
//! `nvme_admin` lines: request `opcode`, `nsid`, `cdw10` to `cdw15`
//! (default 0), `direction`, `data_len` or `data_out`; response `status`,
//! `result`, `data`, or `errno`.
//!
//! `nvme_namespace_id` lines: response `nsid`, or `errno`.

use super::transport::{
    NvmeRequest, NvmeResponse, SgRequest, SgResponse, Transfer, hex,
};
use crate::modules::bytes::base64_decode;
use crate::modules::json::{parse_raw_json, raw_json_as_str, raw_json_as_u64, raw_json_get};
use crate::runtime::value::RuntimeError;
use crate::source::Span;
use miniserde::json::Value as JsonValue;
use std::io;
use std::path::Path;

const KIND: &str = "linux-storage-fake";

const SG_KEYS: &[&str] = &[
    "op", "cdb", "direction", "data_len", "data_out", "status", "host_status", "driver_status",
    "sense", "data", "resid", "errno",
];
const NVME_KEYS: &[&str] = &[
    "op", "opcode", "nsid", "cdw10", "cdw11", "cdw12", "cdw13", "cdw14", "cdw15", "direction",
    "data_len", "data_out", "status", "result", "data", "errno",
];
const NSID_KEYS: &[&str] = &["op", "nsid", "errno"];

struct Entry {
    value: JsonValue,
}

impl Entry {
    fn op(&self) -> &str {
        raw_json_get(&self.value, "op").and_then(raw_json_as_str).unwrap_or("")
    }

    fn uint(&self, key: &str, default: u64) -> u64 {
        raw_json_get(&self.value, key).and_then(raw_json_as_u64).unwrap_or(default)
    }

    fn bytes(&self, key: &str) -> Vec<u8> {
        raw_json_get(&self.value, key)
            .and_then(raw_json_as_str)
            .map(|text| base64_decode(text).unwrap_or_default())
            .unwrap_or_default()
    }

    fn errno(&self) -> Option<i32> {
        raw_json_get(&self.value, "errno")
            .and_then(raw_json_as_u64)
            .map(|errno| errno as i32)
    }

    fn matches_transfer(&self, transfer: Transfer<'_>) -> bool {
        let direction = raw_json_get(&self.value, "direction")
            .and_then(raw_json_as_str)
            .unwrap_or("none");
        if direction != transfer.direction() {
            return false;
        }
        match transfer {
            Transfer::None => true,
            Transfer::FromDevice(length) => self.uint("data_len", 0) == length as u64,
            Transfer::ToDevice(bytes) => self.bytes("data_out") == bytes,
        }
    }

    fn failure(&self, kind: &'static str, span: Span) -> Option<RuntimeError> {
        self.errno().map(|errno| {
            RuntimeError::host(kind, &io::Error::from_raw_os_error(errno)).with_span(span)
        })
    }
}

fn fault(path: &Path, line: usize, message: impl std::fmt::Display, span: Span) -> RuntimeError {
    RuntimeError::new(KIND, format!("{}:{line}: {message}", path.display())).with_span(span)
}

fn load(path: &Path, span: Span) -> Result<Vec<Entry>, RuntimeError> {
    let text = std::fs::read_to_string(path).map_err(|error| {
        RuntimeError::new(KIND, format!("{}: {error}", path.display())).with_span(span)
    })?;
    let mut entries = Vec::new();
    for (index, text) in text.lines().enumerate() {
        if text.trim().is_empty() {
            continue;
        }
        let line = index + 1;
        let value = parse_raw_json(text).map_err(|error| fault(path, line, error, span))?;
        let JsonValue::Object(object) = &value else {
            return Err(fault(path, line, "each line must be a JSON object", span));
        };
        let op = raw_json_get(&value, "op")
            .and_then(raw_json_as_str)
            .ok_or_else(|| fault(path, line, "missing `op`", span))?;
        let allowed = match op {
            "sg_io" => SG_KEYS,
            "nvme_admin" => NVME_KEYS,
            "nvme_namespace_id" => NSID_KEYS,
            other => return Err(fault(path, line, format!("unknown op `{other}`"), span)),
        };
        for (key, field) in object.iter() {
            if !allowed.contains(&key.as_str()) {
                return Err(fault(path, line, format!("unknown field `{key}` for {op}"), span));
            }
            let binary = matches!(key.as_str(), "cdb" | "data_out" | "sense" | "data");
            if binary
                && !raw_json_as_str(field).is_some_and(|text| base64_decode(text).is_ok())
            {
                return Err(fault(path, line, format!("`{key}` must be base64 text"), span));
            }
        }
        entries.push(Entry { value });
    }
    Ok(entries)
}

fn no_match(path: &Path, what: String, span: Span) -> RuntimeError {
    RuntimeError::new(
        KIND,
        format!("{}: no recorded response for {what}", path.display()),
    )
    .with_span(span)
}

pub(super) fn sg_io(
    path: &Path,
    request: &SgRequest<'_>,
    span: Span,
) -> Result<SgResponse, RuntimeError> {
    let entry = load(path, span)?
        .into_iter()
        .find(|entry| {
            entry.op() == "sg_io"
                && entry.bytes("cdb") == request.cdb
                && entry.matches_transfer(request.transfer)
        })
        .ok_or_else(|| {
            no_match(
                path,
                format!("sg_io cdb [{}] direction {}", hex(request.cdb), request.transfer.direction()),
                span,
            )
        })?;
    if let Some(error) = entry.failure("linux-sg-io", span) {
        return Err(error);
    }
    let data = entry.bytes("data");
    let default_resid = match request.transfer {
        Transfer::FromDevice(length) => length.saturating_sub(data.len()) as u64,
        _ => 0,
    };
    Ok(SgResponse {
        status: entry.uint("status", 0) as u8,
        host_status: entry.uint("host_status", 0) as u16,
        driver_status: entry.uint("driver_status", 0) as u16,
        sense: entry.bytes("sense"),
        data,
        resid: entry.uint("resid", default_resid) as u32,
    })
}

pub(super) fn nvme_admin(
    path: &Path,
    request: &NvmeRequest<'_>,
    span: Span,
) -> Result<NvmeResponse, RuntimeError> {
    let words = ["cdw10", "cdw11", "cdw12", "cdw13", "cdw14", "cdw15"];
    let entry = load(path, span)?
        .into_iter()
        .find(|entry| {
            entry.op() == "nvme_admin"
                && entry.uint("opcode", u64::MAX) == u64::from(request.opcode)
                && entry.uint("nsid", 0) == u64::from(request.nsid)
                && words
                    .iter()
                    .zip(request.cdw)
                    .all(|(key, value)| entry.uint(key, 0) == u64::from(value))
                && entry.matches_transfer(request.transfer)
        })
        .ok_or_else(|| {
            no_match(
                path,
                format!(
                    "nvme_admin opcode 0x{:02x} nsid {} cdw10..15 {:?} direction {}",
                    request.opcode,
                    request.nsid,
                    request.cdw,
                    request.transfer.direction()
                ),
                span,
            )
        })?;
    if let Some(error) = entry.failure("linux-nvme", span) {
        return Err(error);
    }
    Ok(NvmeResponse {
        status: entry.uint("status", 0) as u32,
        result: entry.uint("result", 0) as u32,
        data: entry.bytes("data"),
    })
}

pub(super) fn nvme_namespace_id(path: &Path, span: Span) -> Result<u32, RuntimeError> {
    let entry = load(path, span)?
        .into_iter()
        .find(|entry| entry.op() == "nvme_namespace_id")
        .ok_or_else(|| no_match(path, "nvme_namespace_id".to_string(), span))?;
    if let Some(error) = entry.failure("linux-nvme", span) {
        return Err(error);
    }
    Ok(entry.uint("nsid", 0) as u32)
}
