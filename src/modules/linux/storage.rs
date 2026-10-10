//! Storage transport primitives: SCSI generic (`SG_IO`), ATA pass-through
//! helpers on top of it, NVMe admin commands, and device discovery.
//!
//! Every primitive that talks to a device goes through one `Backend`. The
//! kernel backend issues the ioctl; the fixture backend replays recorded
//! command/response pairs and never touches a descriptor, so the same typed
//! helpers are exercised end to end without hardware.
//!
//! Reads and writes are separate functions. The default functions refuse any
//! command that is not known to leave the device unchanged, and the mutating
//! commands (SMART enable/disable, self-test start, feature set, raw
//! host-to-device or unrestricted commands) have their own names.

mod args;
mod ata;
mod discovery;
mod fixture;
mod kernel;
mod nvme;
mod records;
mod transport;

use crate::modules::RuntimeOp;
use crate::runtime::value::{RuntimeError, Value};
use crate::source::Span;
use args::StorageArgs;
pub(crate) use transport::Backend;
use transport::DEFAULT_SG_TIMEOUT_MS;

/// Whether `op` is one of the storage transport primitives.
pub(crate) fn handles(op: RuntimeOp) -> bool {
    label(op).is_some()
}

/// The XSH name of a storage primitive, as written after `linux.`.
pub(crate) fn label(op: RuntimeOp) -> Option<&'static str> {
    Some(match op {
        RuntimeOp::LinuxSgIo => "sg_io",
        RuntimeOp::LinuxSgIoCommand => "sg_io_command",
        RuntimeOp::LinuxAtaIdentify => "ata_identify",
        RuntimeOp::LinuxAtaSmartReadData => "ata_smart_read_data",
        RuntimeOp::LinuxAtaSmartReadThresholds => "ata_smart_read_thresholds",
        RuntimeOp::LinuxAtaSmartReadLog => "ata_smart_read_log",
        RuntimeOp::LinuxAtaSmartStatus => "ata_smart_status",
        RuntimeOp::LinuxAtaSmartEnable => "ata_smart_enable",
        RuntimeOp::LinuxAtaSmartDisable => "ata_smart_disable",
        RuntimeOp::LinuxAtaSmartStartSelfTest => "ata_smart_start_self_test",
        RuntimeOp::LinuxNvmeNamespaceId => "nvme_namespace_id",
        RuntimeOp::LinuxNvmeAdmin => "nvme_admin",
        RuntimeOp::LinuxNvmeAdminCommand => "nvme_admin_command",
        RuntimeOp::LinuxNvmeIdentifyController => "nvme_identify_controller",
        RuntimeOp::LinuxNvmeIdentifyNamespace => "nvme_identify_namespace",
        RuntimeOp::LinuxNvmeLogPage => "nvme_log_page",
        RuntimeOp::LinuxNvmeGetFeature => "nvme_get_feature",
        RuntimeOp::LinuxNvmeSetFeature => "nvme_set_feature",
        RuntimeOp::LinuxStorageCandidates => "storage_candidates",
        _ => return None,
    })
}

/// Runs one storage primitive and wraps its record in `Ok`. Failures are
/// returned as errors so the caller turns them into `Err` values.
pub(crate) fn call(
    op: RuntimeOp,
    values: &[Option<Value>],
    backend: &Backend,
    span: Span,
) -> Result<Value, RuntimeError> {
    let name = label(op).expect("storage primitive expected");
    let args = StorageArgs::new(name, values, span);
    let record = match op {
        RuntimeOp::LinuxSgIo => sg_io(&args, backend, false),
        RuntimeOp::LinuxSgIoCommand => sg_io(&args, backend, true),
        RuntimeOp::LinuxAtaIdentify
        | RuntimeOp::LinuxAtaSmartReadData
        | RuntimeOp::LinuxAtaSmartReadThresholds
        | RuntimeOp::LinuxAtaSmartReadLog
        | RuntimeOp::LinuxAtaSmartStatus
        | RuntimeOp::LinuxAtaSmartEnable
        | RuntimeOp::LinuxAtaSmartDisable
        | RuntimeOp::LinuxAtaSmartStartSelfTest => ata::call(op, &args, backend),
        RuntimeOp::LinuxNvmeNamespaceId
        | RuntimeOp::LinuxNvmeAdmin
        | RuntimeOp::LinuxNvmeAdminCommand
        | RuntimeOp::LinuxNvmeIdentifyController
        | RuntimeOp::LinuxNvmeIdentifyNamespace
        | RuntimeOp::LinuxNvmeLogPage
        | RuntimeOp::LinuxNvmeGetFeature
        | RuntimeOp::LinuxNvmeSetFeature => nvme::call(op, &args, backend),
        RuntimeOp::LinuxStorageCandidates => discovery::candidates(&args),
        _ => unreachable!("storage primitive expected"),
    }?;
    Ok(Value::ok(record))
}

/// `linux.sg_io` and `linux.sg_io_command`. The read form refuses every
/// operation code that is not on the read-only list; the command form sends
/// whatever CDB it is given, device to host or host to device.
fn sg_io(args: &StorageArgs<'_>, backend: &Backend, command: bool) -> Result<Value, RuntimeError> {
    let kind = "linux-sg-io";
    let fd = args.fd(0)?;
    let cdb = args.bytes(1)?;
    let transfer = args.transfer(2, command)?;
    if cdb.is_empty() || cdb.len() > transport::MAX_CDB {
        return Err(RuntimeError::new(
            kind,
            format!("cdb must be 1 to {} bytes", transport::MAX_CDB),
        )
        .with_span(args.span()));
    }
    if !command && !transport::scsi_read_only(cdb) {
        return Err(RuntimeError::new(
            kind,
            format!(
                "operation code 0x{:02x} may change device state; use linux.sg_io_command",
                cdb[0]
            ),
        )
        .with_span(args.span()));
    }
    let timeout_ms = args.timeout_ms(3, DEFAULT_SG_TIMEOUT_MS)?;
    let request = transport::SgRequest { cdb, transfer, timeout_ms };
    let response = backend.sg_io(fd, &request, args.span())?;
    Ok(records::sg_io(response))
}
