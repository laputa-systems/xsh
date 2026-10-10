//! ATA commands carried in SCSI ATA PASS-THROUGH (12) and (16) CDBs, as
//! defined by SAT, with the status registers read back from the ATA Status
//! Return sense descriptor.
//!
//! Every command sets CK_COND so the device reports its registers even on
//! success. The helpers fail when the device sets ERR or DF, so a returned
//! value always describes a command the device accepted.

use super::args::StorageArgs;
use super::records;
use super::transport::{Backend, DEFAULT_SG_TIMEOUT_MS, SgRequest, Transfer, hex};
use crate::modules::RuntimeOp;
use crate::runtime::value::{RuntimeError, Value};
use crate::source::Span;

const KIND: &str = "linux-ata";
const SECTOR: usize = 512;

const ATA_PASS_THROUGH_12: u8 = 0xa1;
const ATA_PASS_THROUGH_16: u8 = 0x85;
/// SAT protocol field values.
const PROTOCOL_NON_DATA: u8 = 3;
const PROTOCOL_PIO_DATA_IN: u8 = 4;
/// CK_COND: return the ATA registers in sense data on completion.
const CK_COND: u8 = 0x20;
/// T_DIR to host, BYTE_BLOCK in sectors, T_LENGTH from the sector count.
const DATA_IN_FLAGS: u8 = 0x08 | 0x04 | 0x02;

const IDENTIFY_DEVICE: u8 = 0xec;
const SMART: u8 = 0xb0;
const SMART_KEY_MID: u8 = 0x4f;
const SMART_KEY_HIGH: u8 = 0xc2;
const SMART_READ_DATA: u8 = 0xd0;
const SMART_READ_THRESHOLDS: u8 = 0xd1;
const SMART_EXECUTE_OFFLINE: u8 = 0xd4;
const SMART_READ_LOG: u8 = 0xd5;
const SMART_ENABLE: u8 = 0xd8;
const SMART_DISABLE: u8 = 0xd9;
const SMART_RETURN_STATUS: u8 = 0xda;
/// RETURN STATUS reports a failing attribute with these register values.
const SMART_FAILED_MID: u8 = 0xf4;
const SMART_FAILED_HIGH: u8 = 0x2c;

const STATUS_ERR: u8 = 0x01;
const STATUS_DF: u8 = 0x20;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Registers {
    pub(super) error: u8,
    pub(super) sector_count: u8,
    pub(super) lba_low: u8,
    pub(super) lba_mid: u8,
    pub(super) lba_high: u8,
    pub(super) device: u8,
    pub(super) status: u8,
}

pub(super) struct AtaOutcome {
    pub(super) data: Vec<u8>,
    pub(super) registers: Registers,
    pub(super) sense: Vec<u8>,
}

pub(super) struct SmartStatus {
    pub(super) passed: bool,
    pub(super) registers: Registers,
}

#[derive(Clone, Copy)]
enum CdbSize {
    Twelve,
    Sixteen,
}

struct AtaCommand {
    features: u8,
    count: u8,
    lba_low: u8,
    lba_mid: u8,
    lba_high: u8,
    command: u8,
    /// Sectors the device returns; zero for a non-data command.
    data_in_sectors: usize,
}

impl AtaCommand {
    fn smart(features: u8) -> Self {
        Self {
            features,
            count: 0,
            lba_low: 0,
            lba_mid: SMART_KEY_MID,
            lba_high: SMART_KEY_HIGH,
            command: SMART,
            data_in_sectors: 0,
        }
    }

    fn cdb(&self, size: CdbSize) -> Vec<u8> {
        let data_in = self.data_in_sectors > 0;
        let protocol = if data_in { PROTOCOL_PIO_DATA_IN } else { PROTOCOL_NON_DATA };
        let flags = CK_COND | if data_in { DATA_IN_FLAGS } else { 0 };
        // The device register stays zero, as the SAT translators select the
        // drive themselves and smartmontools sends zero.
        match size {
            CdbSize::Twelve => vec![
                ATA_PASS_THROUGH_12,
                protocol << 1,
                flags,
                self.features,
                self.count,
                self.lba_low,
                self.lba_mid,
                self.lba_high,
                0,
                self.command,
                0,
                0,
            ],
            CdbSize::Sixteen => vec![
                ATA_PASS_THROUGH_16,
                protocol << 1,
                flags,
                0,
                self.features,
                0,
                self.count,
                0,
                self.lba_low,
                0,
                self.lba_mid,
                0,
                self.lba_high,
                0,
                self.command,
                0,
            ],
        }
    }
}

/// Reads the ATA Status Return descriptor (code 0x09) from descriptor-format
/// sense data.
fn registers_from_sense(sense: &[u8]) -> Option<Registers> {
    // Response codes 0x72 (current) and 0x73 (deferred) are descriptor format.
    if !matches!(sense.first().map(|code| code & 0x7f), Some(0x72 | 0x73)) {
        return None;
    }
    let total = 8 + usize::from(*sense.get(7)?);
    let mut offset = 8;
    while offset + 2 <= sense.len().min(total) {
        let code = sense[offset];
        let length = usize::from(sense[offset + 1]);
        let body = sense.get(offset + 2..offset + 2 + length)?;
        if code == 0x09 && length >= 0x0c {
            return Some(Registers {
                error: body[1],
                sector_count: body[3],
                lba_low: body[5],
                lba_mid: body[7],
                lba_high: body[9],
                device: body[10],
                status: body[11],
            });
        }
        offset += 2 + length;
    }
    None
}

fn fail(span: Span, message: String) -> RuntimeError {
    RuntimeError::new(KIND, message).with_span(span)
}

fn execute(
    backend: &Backend,
    fd: i32,
    size: CdbSize,
    command: &AtaCommand,
    span: Span,
) -> Result<AtaOutcome, RuntimeError> {
    let cdb = command.cdb(size);
    let expected = command.data_in_sectors * SECTOR;
    let request = SgRequest {
        cdb: &cdb,
        transfer: if expected == 0 { Transfer::None } else { Transfer::FromDevice(expected) },
        timeout_ms: DEFAULT_SG_TIMEOUT_MS,
    };
    let response = backend.sg_io(fd, &request, span)?;
    let context = format!(
        "ATA command 0x{:02x}: scsi status 0x{:02x}, host status 0x{:04x}, driver status 0x{:04x}",
        command.command, response.status, response.host_status, response.driver_status
    );
    if response.host_status != 0 {
        return Err(fail(span, format!("{context}: transport failure, sense [{}]", hex(&response.sense))));
    }
    // GOOD, or CHECK CONDITION carrying the registers that CK_COND asked for.
    if response.status != 0 && response.status != 0x02 {
        return Err(fail(span, format!("{context}: unexpected SCSI status, sense [{}]", hex(&response.sense))));
    }
    let Some(registers) = registers_from_sense(&response.sense) else {
        return Err(fail(
            span,
            format!(
                "{context}: no ATA status return descriptor in sense data [{}]",
                hex(&response.sense)
            ),
        ));
    };
    if registers.status & (STATUS_ERR | STATUS_DF) != 0 {
        return Err(fail(
            span,
            format!(
                "{context}: device reported ATA status 0x{:02x}, error 0x{:02x}",
                registers.status, registers.error
            ),
        ));
    }
    if response.data.len() != expected {
        return Err(fail(
            span,
            format!(
                "{context}: device returned {} of {expected} data bytes",
                response.data.len()
            ),
        ));
    }
    Ok(AtaOutcome { data: response.data, registers, sense: response.sense })
}

fn cdb_size(args: &StorageArgs<'_>, index: usize) -> Result<CdbSize, RuntimeError> {
    match args.uint_or(index, "cdb_size", 16, 255)? {
        12 => Ok(CdbSize::Twelve),
        16 => Ok(CdbSize::Sixteen),
        _ => Err(RuntimeError::new("invalid-argument", "cdb_size must be 12 or 16")
            .with_span(args.span())),
    }
}

/// Self-test subcommand numbers for EXECUTE OFF-LINE IMMEDIATE. Selective
/// and captive tests are not offered: the first needs a span table the
/// device interprets, the second blocks the device until it finishes.
fn self_test_subcommand(kind: &str, span: Span) -> Result<u8, RuntimeError> {
    match kind {
        "offline" => Ok(0),
        "short" => Ok(1),
        "extended" => Ok(2),
        "conveyance" => Ok(3),
        "abort" => Ok(127),
        other => Err(RuntimeError::new(
            "invalid-argument",
            format!("unknown self-test kind `{other}`; expected offline, short, extended, conveyance, or abort"),
        )
        .with_span(span)),
    }
}

pub(super) fn call(
    op: RuntimeOp,
    args: &StorageArgs<'_>,
    backend: &Backend,
) -> Result<Value, RuntimeError> {
    let span = args.span();
    let fd = args.fd(0)?;
    // The CDB size parameter is the last one of every helper.
    let (command, size_index) = match op {
        RuntimeOp::LinuxAtaIdentify => (
            AtaCommand {
                features: 0,
                count: 1,
                lba_low: 0,
                lba_mid: 0,
                lba_high: 0,
                command: IDENTIFY_DEVICE,
                data_in_sectors: 1,
            },
            1,
        ),
        RuntimeOp::LinuxAtaSmartReadData => (
            AtaCommand { count: 1, data_in_sectors: 1, ..AtaCommand::smart(SMART_READ_DATA) },
            1,
        ),
        RuntimeOp::LinuxAtaSmartReadThresholds => (
            AtaCommand { count: 1, data_in_sectors: 1, ..AtaCommand::smart(SMART_READ_THRESHOLDS) },
            1,
        ),
        RuntimeOp::LinuxAtaSmartReadLog => {
            let log_address = args.uint(1, "log_address", 255)? as u8;
            let sectors = args.uint_or(2, "sectors", 1, 255)? as usize;
            if sectors == 0 {
                return Err(RuntimeError::new("invalid-argument", "sectors must be at least 1")
                    .with_span(span));
            }
            (
                AtaCommand {
                    count: sectors as u8,
                    lba_low: log_address,
                    data_in_sectors: sectors,
                    ..AtaCommand::smart(SMART_READ_LOG)
                },
                3,
            )
        }
        RuntimeOp::LinuxAtaSmartStatus => (AtaCommand::smart(SMART_RETURN_STATUS), 1),
        RuntimeOp::LinuxAtaSmartEnable => (AtaCommand::smart(SMART_ENABLE), 1),
        RuntimeOp::LinuxAtaSmartDisable => (AtaCommand::smart(SMART_DISABLE), 1),
        RuntimeOp::LinuxAtaSmartStartSelfTest => {
            let kind = args.str(1)?;
            let subcommand = self_test_subcommand(&kind, span)?;
            (
                AtaCommand { lba_low: subcommand, ..AtaCommand::smart(SMART_EXECUTE_OFFLINE) },
                2,
            )
        }
        _ => unreachable!("ATA primitive expected"),
    };
    let outcome = execute(backend, fd, cdb_size(args, size_index)?, &command, span)?;
    if op == RuntimeOp::LinuxAtaSmartStatus {
        let registers = outcome.registers;
        let passed = match (registers.lba_mid, registers.lba_high) {
            (SMART_KEY_MID, SMART_KEY_HIGH) => true,
            (SMART_FAILED_MID, SMART_FAILED_HIGH) => false,
            (mid, high) => {
                return Err(fail(
                    span,
                    format!(
                        "SMART RETURN STATUS reported an unknown signature: lba_mid 0x{mid:02x}, lba_high 0x{high:02x}"
                    ),
                ));
            }
        };
        return Ok(records::smart_status(SmartStatus { passed, registers }));
    }
    Ok(records::ata_result(outcome))
}
