//! NVMe admin commands over the Linux passthrough ioctl.
//!
//! The device-reported status is data on the raw functions and an error on
//! the typed helpers, so a script can inspect a failing command or rely on a
//! returned value being a completed one.

use super::args::StorageArgs;
use super::records;
use super::transport::{Backend, MAX_TRANSFER, NvmeRequest, NvmeResponse, Transfer};
use crate::modules::RuntimeOp;
use crate::runtime::value::{RuntimeError, Value};
use crate::source::Span;

const KIND: &str = "linux-nvme";

const OPCODE_GET_LOG_PAGE: u8 = 0x02;
const OPCODE_IDENTIFY: u8 = 0x06;
const OPCODE_SET_FEATURES: u8 = 0x09;
const OPCODE_GET_FEATURES: u8 = 0x0a;
/// Identify and log data are returned in 4 KiB units at most per page.
const IDENTIFY_BYTES: usize = 4096;
const CNS_NAMESPACE: u32 = 0;
const CNS_CONTROLLER: u32 = 1;
/// The broadcast namespace id, used by log pages that are not per namespace.
const ALL_NAMESPACES: u64 = 0xffff_ffff;

/// Admin opcodes that read state and change nothing: Get Log Page,
/// Identify, and Get Features.
fn read_only(opcode: u8) -> bool {
    matches!(opcode, OPCODE_GET_LOG_PAGE | OPCODE_IDENTIFY | OPCODE_GET_FEATURES)
}

fn status_text(status: u32) -> String {
    let code = status & 0xff;
    let kind = (status >> 8) & 0x7;
    let mut text = format!("status 0x{status:04x} (type {kind}, code 0x{code:02x}");
    if status & 0x2000 != 0 {
        text.push_str(", more");
    }
    if status & 0x4000 != 0 {
        text.push_str(", do not retry");
    }
    text.push(')');
    text
}

fn issue(
    backend: &Backend,
    fd: i32,
    request: NvmeRequest<'_>,
    span: Span,
) -> Result<NvmeResponse, RuntimeError> {
    backend.nvme_admin(fd, &request, span)
}

/// A typed helper's command: any nonzero device status is an error.
fn complete(
    backend: &Backend,
    fd: i32,
    request: NvmeRequest<'_>,
    span: Span,
) -> Result<Value, RuntimeError> {
    let opcode = request.opcode;
    let expected = match request.transfer {
        Transfer::FromDevice(length) => length,
        _ => 0,
    };
    let response = issue(backend, fd, request, span)?;
    if response.status != 0 {
        return Err(RuntimeError::new(
            KIND,
            format!("admin command 0x{opcode:02x} failed: {}", status_text(response.status)),
        )
        .with_span(span));
    }
    if response.data.len() != expected {
        return Err(RuntimeError::new(
            KIND,
            format!(
                "admin command 0x{opcode:02x} returned {} of {expected} data bytes",
                response.data.len()
            ),
        )
        .with_span(span));
    }
    Ok(records::nvme(response))
}

pub(super) fn call(
    op: RuntimeOp,
    args: &StorageArgs<'_>,
    backend: &Backend,
) -> Result<Value, RuntimeError> {
    let span = args.span();
    let fd = args.fd(0)?;
    match op {
        RuntimeOp::LinuxNvmeNamespaceId => {
            return Ok(Value::Int(i64::from(backend.nvme_namespace_id(fd, span)?)));
        }
        RuntimeOp::LinuxNvmeAdmin | RuntimeOp::LinuxNvmeAdminCommand => {
            let command = op == RuntimeOp::LinuxNvmeAdminCommand;
            let opcode = args.uint(1, "opcode", 255)? as u8;
            if !command && !read_only(opcode) {
                return Err(RuntimeError::new(
                    KIND,
                    format!(
                        "admin opcode 0x{opcode:02x} may change device state; use linux.nvme_admin_command"
                    ),
                )
                .with_span(span));
            }
            let mut cdw = [0u32; 6];
            for (index, word) in cdw.iter_mut().enumerate() {
                *word = args.uint_or(4 + index, "cdw", 0, u64::from(u32::MAX))? as u32;
            }
            let request = NvmeRequest {
                opcode,
                nsid: args.uint_or(3, "nsid", 0, u64::from(u32::MAX))? as u32,
                cdw,
                transfer: args.transfer(2, command)?,
                timeout_ms: args.uint_or(10, "timeout_ms", 0, u64::from(u32::MAX))? as u32,
            };
            return Ok(records::nvme(issue(backend, fd, request, span)?));
        }
        _ => {}
    }
    let request = match op {
        RuntimeOp::LinuxNvmeIdentifyController => NvmeRequest {
            opcode: OPCODE_IDENTIFY,
            nsid: 0,
            cdw: [CNS_CONTROLLER, 0, 0, 0, 0, 0],
            transfer: Transfer::FromDevice(IDENTIFY_BYTES),
            timeout_ms: 0,
        },
        RuntimeOp::LinuxNvmeIdentifyNamespace => NvmeRequest {
            opcode: OPCODE_IDENTIFY,
            nsid: args.uint(1, "nsid", u64::from(u32::MAX))? as u32,
            cdw: [CNS_NAMESPACE, 0, 0, 0, 0, 0],
            transfer: Transfer::FromDevice(IDENTIFY_BYTES),
            timeout_ms: 0,
        },
        RuntimeOp::LinuxNvmeLogPage => {
            let log_id = args.uint(1, "log_id", 255)? as u32;
            let length = args.uint_or(2, "data_len", 512, MAX_TRANSFER as u64)? as usize;
            if length < 4 || length % 4 != 0 {
                return Err(RuntimeError::new(
                    "invalid-argument",
                    "data_len must be a positive multiple of 4",
                )
                .with_span(span));
            }
            // NUMD is a zero-based dword count split across two command dwords.
            let numd = (length / 4 - 1) as u32;
            NvmeRequest {
                opcode: OPCODE_GET_LOG_PAGE,
                nsid: args.uint_or(3, "nsid", ALL_NAMESPACES, u64::from(u32::MAX))? as u32,
                cdw: [((numd & 0xffff) << 16) | log_id, numd >> 16, 0, 0, 0, 0],
                transfer: Transfer::FromDevice(length),
                timeout_ms: 0,
            }
        }
        RuntimeOp::LinuxNvmeGetFeature => {
            let feature = args.uint(1, "feature_id", 255)? as u32;
            let select = args.uint_or(3, "select", 0, 7)? as u32;
            let length = args.uint_or(5, "data_len", 0, MAX_TRANSFER as u64)? as usize;
            NvmeRequest {
                opcode: OPCODE_GET_FEATURES,
                nsid: args.uint_or(2, "nsid", 0, u64::from(u32::MAX))? as u32,
                cdw: [
                    (select << 8) | feature,
                    args.uint_or(4, "cdw11", 0, u64::from(u32::MAX))? as u32,
                    0,
                    0,
                    0,
                    0,
                ],
                transfer: if length == 0 { Transfer::None } else { Transfer::FromDevice(length) },
                timeout_ms: 0,
            }
        }
        RuntimeOp::LinuxNvmeSetFeature => {
            let feature = args.uint(1, "feature_id", 255)? as u32;
            let value = args.uint(2, "value", u64::from(u32::MAX))? as u32;
            let save = args.bool_or(4, false)?;
            NvmeRequest {
                opcode: OPCODE_SET_FEATURES,
                nsid: args.uint_or(3, "nsid", 0, u64::from(u32::MAX))? as u32,
                cdw: [(u32::from(save) << 31) | feature, value, 0, 0, 0, 0],
                transfer: Transfer::None,
                timeout_ms: 0,
            }
        }
        _ => unreachable!("NVMe primitive expected"),
    };
    complete(backend, fd, request, span)
}
