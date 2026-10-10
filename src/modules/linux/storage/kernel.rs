//! The kernel backend: `SG_IO` on SCSI generic, block, and bsg nodes, and
//! the NVMe admin passthrough ioctl on controller and namespace nodes.
//! Pointer fix-ups live here so scripts only handle plain bytes.

use super::transport::{NvmeRequest, NvmeResponse, SgRequest, SgResponse, Transfer};
use crate::runtime::value::RuntimeError;
use crate::source::Span;

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use std::ffi::c_void;
    use std::io;

    const SG_IO: libc::c_ulong = 0x2285;
    const SG_INTERFACE_ID: libc::c_int = b'S' as libc::c_int;
    const SG_DXFER_NONE: libc::c_int = -1;
    const SG_DXFER_TO_DEV: libc::c_int = -2;
    const SG_DXFER_FROM_DEV: libc::c_int = -3;
    /// Room for the largest fixed-format and descriptor sense this module
    /// parses. A device that returns more is truncated by the kernel.
    const SENSE_LEN: usize = 64;
    const NVME_IOCTL_ID: libc::c_ulong = 0x4e40;
    // _IOWR('N', 0x41, struct nvme_admin_cmd), a 72-byte structure.
    const NVME_IOCTL_ADMIN_CMD: libc::c_ulong = 0xc048_4e41;

    /// `struct sg_io_hdr` from `scsi/sg.h`.
    #[repr(C)]
    struct SgIoHdr {
        interface_id: libc::c_int,
        dxfer_direction: libc::c_int,
        cmd_len: u8,
        mx_sb_len: u8,
        iovec_count: u16,
        dxfer_len: u32,
        dxferp: *mut c_void,
        cmdp: *mut u8,
        sbp: *mut u8,
        timeout: u32,
        flags: u32,
        pack_id: libc::c_int,
        usr_ptr: *mut c_void,
        status: u8,
        masked_status: u8,
        msg_status: u8,
        sb_len_wr: u8,
        host_status: u16,
        driver_status: u16,
        resid: libc::c_int,
        duration: u32,
        info: u32,
    }

    /// `struct nvme_admin_cmd` (`struct nvme_passthru_cmd`) from
    /// `linux/nvme_ioctl.h`.
    #[repr(C)]
    struct NvmeAdminCmd {
        opcode: u8,
        flags: u8,
        rsvd1: u16,
        nsid: u32,
        cdw2: u32,
        cdw3: u32,
        metadata: u64,
        addr: u64,
        metadata_len: u32,
        data_len: u32,
        cdw10: u32,
        cdw11: u32,
        cdw12: u32,
        cdw13: u32,
        cdw14: u32,
        cdw15: u32,
        timeout_ms: u32,
        result: u32,
    }

    const _: () = assert!(std::mem::size_of::<NvmeAdminCmd>() == 72);
    #[cfg(target_pointer_width = "64")]
    const _: () = assert!(std::mem::size_of::<SgIoHdr>() == 88);

    fn host_error(kind: &'static str, span: Span) -> RuntimeError {
        RuntimeError::host(kind, &io::Error::last_os_error()).with_span(span)
    }

    fn data_buffer(transfer: Transfer<'_>) -> (Vec<u8>, usize) {
        match transfer {
            Transfer::None => (Vec::new(), 0),
            Transfer::FromDevice(length) => (vec![0; length], length),
            Transfer::ToDevice(bytes) => (bytes.to_vec(), bytes.len()),
        }
    }

    pub(in super::super) fn sg_io(
        fd: i32,
        request: &SgRequest<'_>,
        span: Span,
    ) -> Result<SgResponse, RuntimeError> {
        let mut cdb = request.cdb.to_vec();
        let (mut data, data_len) = data_buffer(request.transfer);
        let mut sense = vec![0u8; SENSE_LEN];
        let mut header = SgIoHdr {
            interface_id: SG_INTERFACE_ID,
            dxfer_direction: match request.transfer {
                Transfer::None => SG_DXFER_NONE,
                Transfer::FromDevice(_) => SG_DXFER_FROM_DEV,
                Transfer::ToDevice(_) => SG_DXFER_TO_DEV,
            },
            cmd_len: cdb.len() as u8,
            mx_sb_len: SENSE_LEN as u8,
            iovec_count: 0,
            dxfer_len: data_len as u32,
            dxferp: if data.is_empty() {
                std::ptr::null_mut()
            } else {
                data.as_mut_ptr().cast()
            },
            cmdp: cdb.as_mut_ptr(),
            sbp: sense.as_mut_ptr(),
            timeout: request.timeout_ms,
            flags: 0,
            pack_id: 0,
            usr_ptr: std::ptr::null_mut(),
            status: 0,
            masked_status: 0,
            msg_status: 0,
            sb_len_wr: 0,
            host_status: 0,
            driver_status: 0,
            resid: 0,
            duration: 0,
            info: 0,
        };
        // SAFETY: `header` matches the kernel's sg_io_hdr layout and its
        // pointers address buffers (`cdb`, `data`, `sense`) that outlive the
        // call and are at least as long as the lengths recorded beside them.
        let result = unsafe { libc::ioctl(fd, SG_IO as _, &mut header as *mut SgIoHdr) };
        if result < 0 {
            return Err(host_error("linux-sg-io", span));
        }
        sense.truncate(usize::from(header.sb_len_wr).min(SENSE_LEN));
        let resid = u32::try_from(header.resid).unwrap_or(0).min(data_len as u32);
        if matches!(request.transfer, Transfer::FromDevice(_)) {
            data.truncate(data_len - resid as usize);
        } else {
            data.clear();
        }
        Ok(SgResponse {
            status: header.status,
            host_status: header.host_status,
            driver_status: header.driver_status,
            sense,
            data,
            resid,
        })
    }

    pub(in super::super) fn nvme_admin(
        fd: i32,
        request: &NvmeRequest<'_>,
        span: Span,
    ) -> Result<NvmeResponse, RuntimeError> {
        let (mut data, data_len) = data_buffer(request.transfer);
        let mut command = NvmeAdminCmd {
            opcode: request.opcode,
            flags: 0,
            rsvd1: 0,
            nsid: request.nsid,
            cdw2: 0,
            cdw3: 0,
            metadata: 0,
            addr: if data.is_empty() { 0 } else { data.as_mut_ptr() as u64 },
            metadata_len: 0,
            data_len: data_len as u32,
            cdw10: request.cdw[0],
            cdw11: request.cdw[1],
            cdw12: request.cdw[2],
            cdw13: request.cdw[3],
            cdw14: request.cdw[4],
            cdw15: request.cdw[5],
            timeout_ms: request.timeout_ms,
            result: 0,
        };
        // SAFETY: `command` matches the kernel's nvme_admin_cmd layout and
        // `addr` addresses `data`, which outlives the call and holds
        // `data_len` bytes.
        let status =
            unsafe { libc::ioctl(fd, NVME_IOCTL_ADMIN_CMD as _, &mut command as *mut NvmeAdminCmd) };
        if status < 0 {
            return Err(host_error("linux-nvme", span));
        }
        if !matches!(request.transfer, Transfer::FromDevice(_)) {
            data.clear();
        }
        Ok(NvmeResponse { status: status as u32, result: command.result, data })
    }

    pub(in super::super) fn nvme_namespace_id(fd: i32, span: Span) -> Result<u32, RuntimeError> {
        // SAFETY: NVME_IOCTL_ID takes no argument and returns the id.
        let nsid = unsafe { libc::ioctl(fd, NVME_IOCTL_ID as _) };
        if nsid < 0 {
            return Err(host_error("linux-nvme", span));
        }
        Ok(nsid as u32)
    }
}

#[cfg(target_os = "linux")]
pub(super) use imp::{nvme_admin, nvme_namespace_id, sg_io};

#[cfg(not(target_os = "linux"))]
mod imp {
    use super::*;

    fn unsupported(span: Span) -> RuntimeError {
        RuntimeError::new(
            "linux-unsupported",
            "real linux.* primitives are only available on Linux",
        )
        .with_span(span)
    }

    pub(in super::super) fn sg_io(
        _fd: i32,
        _request: &SgRequest<'_>,
        span: Span,
    ) -> Result<SgResponse, RuntimeError> {
        Err(unsupported(span))
    }

    pub(in super::super) fn nvme_admin(
        _fd: i32,
        _request: &NvmeRequest<'_>,
        span: Span,
    ) -> Result<NvmeResponse, RuntimeError> {
        Err(unsupported(span))
    }

    pub(in super::super) fn nvme_namespace_id(_fd: i32, span: Span) -> Result<u32, RuntimeError> {
        Err(unsupported(span))
    }
}

#[cfg(not(target_os = "linux"))]
pub(super) use imp::{nvme_admin, nvme_namespace_id, sg_io};
