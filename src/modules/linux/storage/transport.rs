//! The command and response shapes shared by the kernel and fixture
//! backends, and the choice between them.

use super::{fixture, kernel};
use crate::runtime::value::RuntimeError;
use crate::source::Span;
use std::path::PathBuf;

/// Upper bound on one data phase. The kernel enforces its own, smaller
/// per-device limit; this only stops a script from asking for a huge buffer.
pub(super) const MAX_TRANSFER: usize = 1 << 20;
/// SCSI CDBs this module will send: the 6 to 16 byte fixed forms and the
/// 32-byte variable-length form.
pub(super) const MAX_CDB: usize = 32;
/// The timeout smartmontools uses for pass-through commands.
pub(super) const DEFAULT_SG_TIMEOUT_MS: u32 = 60_000;

/// The data phase of one command.
#[derive(Clone, Copy, Debug)]
pub(super) enum Transfer<'a> {
    None,
    FromDevice(usize),
    ToDevice(&'a [u8]),
}

impl Transfer<'_> {
    pub(super) fn direction(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::FromDevice(_) => "from_device",
            Self::ToDevice(_) => "to_device",
        }
    }
}

#[derive(Debug)]
pub(super) struct SgRequest<'a> {
    pub(super) cdb: &'a [u8],
    pub(super) transfer: Transfer<'a>,
    pub(super) timeout_ms: u32,
}

/// What `SG_IO` reported. `data` holds only the bytes the device actually
/// returned, that is the requested length minus `resid`.
#[derive(Debug, Default)]
pub(super) struct SgResponse {
    pub(super) status: u8,
    pub(super) host_status: u16,
    pub(super) driver_status: u16,
    pub(super) sense: Vec<u8>,
    pub(super) data: Vec<u8>,
    pub(super) resid: u32,
}

#[derive(Debug)]
pub(super) struct NvmeRequest<'a> {
    pub(super) opcode: u8,
    pub(super) nsid: u32,
    /// Command dwords 10 through 15.
    pub(super) cdw: [u32; 6],
    pub(super) transfer: Transfer<'a>,
    pub(super) timeout_ms: u32,
}

/// What the NVMe admin ioctl reported: `status` is the kernel's return
/// value (0 for success, otherwise the controller status with its more and
/// do-not-retry bits), `result` is the completion queue entry's dword 0.
#[derive(Debug, Default)]
pub(super) struct NvmeResponse {
    pub(super) status: u32,
    pub(super) result: u32,
    pub(super) data: Vec<u8>,
}

/// Where a command goes. Only the native-test harness can select the
/// fixture backends, through the `storage_fixture` setting of its Linux fake.
#[derive(Clone, Debug)]
pub(crate) enum Backend {
    /// Issue the ioctl on the descriptor.
    Kernel,
    /// Answer from the recorded command/response pairs in this file.
    Fixture(PathBuf),
    /// A fake is installed without a fixture: every device command fails
    /// before reaching a descriptor.
    FixtureMissing,
}

impl Backend {
    pub(super) fn sg_io(
        &self,
        fd: i32,
        request: &SgRequest<'_>,
        span: Span,
    ) -> Result<SgResponse, RuntimeError> {
        match self {
            Self::Kernel => kernel::sg_io(fd, request, span),
            Self::Fixture(path) => fixture::sg_io(path, request, span),
            Self::FixtureMissing => Err(missing_fixture(span)),
        }
    }

    pub(super) fn nvme_admin(
        &self,
        fd: i32,
        request: &NvmeRequest<'_>,
        span: Span,
    ) -> Result<NvmeResponse, RuntimeError> {
        match self {
            Self::Kernel => kernel::nvme_admin(fd, request, span),
            Self::Fixture(path) => fixture::nvme_admin(path, request, span),
            Self::FixtureMissing => Err(missing_fixture(span)),
        }
    }

    pub(super) fn nvme_namespace_id(&self, fd: i32, span: Span) -> Result<u32, RuntimeError> {
        match self {
            Self::Kernel => kernel::nvme_namespace_id(fd, span),
            Self::Fixture(path) => fixture::nvme_namespace_id(path, span),
            Self::FixtureMissing => Err(missing_fixture(span)),
        }
    }
}

fn missing_fixture(span: Span) -> RuntimeError {
    RuntimeError::new(
        "linux-storage-fake",
        "the linux fake has no storage_fixture; storage commands never reach a descriptor under a fake",
    )
    .with_span(span)
}

/// Whether a SCSI command is known to leave the device unchanged. ATA
/// pass-through is deliberately absent: it can carry any ATA command, so it
/// is only issued through the typed ATA helpers.
pub(super) fn scsi_read_only(cdb: &[u8]) -> bool {
    let Some(&opcode) = cdb.first() else {
        return false;
    };
    match opcode {
        // TEST UNIT READY, REQUEST SENSE, READ(6), INQUIRY, MODE SENSE(6),
        // READ CAPACITY(10), READ(10), LOG SENSE, MODE SENSE(10), READ(16),
        // REPORT LUNS, READ(12).
        0x00 | 0x03 | 0x08 | 0x12 | 0x1a | 0x25 | 0x28 | 0x4d | 0x5a | 0x88 | 0xa0 | 0xa8 => true,
        // SERVICE ACTION IN(16): only READ CAPACITY(16) and GET LBA STATUS
        // read; the other service actions include writes of device state.
        0x9e => cdb.get(1).is_some_and(|action| matches!(action & 0x1f, 0x10 | 0x12)),
        _ => false,
    }
}

/// A byte string as spaced lowercase hex, for diagnostics.
pub(super) fn hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 3);
    for (index, byte) in bytes.iter().enumerate() {
        if index > 0 {
            text.push(' ');
        }
        text.push_str(&format!("{byte:02x}"));
    }
    text
}
