//! The `linux` storage transport functions: SCSI generic, ATA pass-through,
//! NVMe admin, and candidate discovery. Reads and mutations are separate
//! names; the read forms refuse commands that can change a device.

use super::{ModuleFnSig, ParamSig, RuntimeOp, Type, default_param, param, result, sig};
use crate::records::{
    linux_ata_result_type, linux_ata_smart_status_type, linux_nvme_command_type,
    linux_sg_io_type, linux_storage_candidate_type,
};

pub(super) fn entries() -> Vec<(&'static str, ModuleFnSig)> {
    let fd = || param("fd", Type::Int);
    let cdb_size = || default_param("cdb_size", Type::UInt);
    let timeout = || default_param("timeout_ms", Type::UInt);
    let ata = |params, ret, op| sig(params, result(ret), false, op);
    // nvme_admin family: after the opcode and data phase come the namespace
    // id, the six command dwords, and the timeout.
    let nvme_tail = || {
        let mut tail = vec![default_param("nsid", Type::UInt)];
        for name in ["cdw10", "cdw11", "cdw12", "cdw13", "cdw14", "cdw15"] {
            tail.push(default_param(name, Type::UInt));
        }
        tail.push(default_param("timeout_ms", Type::UInt));
        tail
    };
    let nvme_params = |data: ParamSig| {
        let mut params = vec![fd(), param("opcode", Type::UInt), data];
        params.extend(nvme_tail());
        params
    };
    let nvme = |params, op| sig(params, result(linux_nvme_command_type()), false, op);
    vec![
        (
            "sg_io",
            sig(
                vec![fd(), param("cdb", Type::Bytes), default_param("data_len", Type::UInt), timeout()],
                result(linux_sg_io_type()),
                false,
                RuntimeOp::LinuxSgIo,
            ),
        ),
        (
            "sg_io_command",
            sig(
                vec![fd(), param("cdb", Type::Bytes), param("data_len", Type::UInt), timeout()],
                result(linux_sg_io_type()),
                false,
                RuntimeOp::LinuxSgIoCommand,
            ),
        ),
        (
            "sg_io_command",
            sig(
                vec![fd(), param("cdb", Type::Bytes), param("data", Type::Bytes), timeout()],
                result(linux_sg_io_type()),
                false,
                RuntimeOp::LinuxSgIoCommand,
            ),
        ),
        ("ata_identify", ata(vec![fd(), cdb_size()], linux_ata_result_type(), RuntimeOp::LinuxAtaIdentify)),
        (
            "ata_smart_read_data",
            ata(vec![fd(), cdb_size()], linux_ata_result_type(), RuntimeOp::LinuxAtaSmartReadData),
        ),
        (
            "ata_smart_read_thresholds",
            ata(vec![fd(), cdb_size()], linux_ata_result_type(), RuntimeOp::LinuxAtaSmartReadThresholds),
        ),
        (
            "ata_smart_read_log",
            ata(
                vec![fd(), param("log_address", Type::UInt), default_param("sectors", Type::UInt), cdb_size()],
                linux_ata_result_type(),
                RuntimeOp::LinuxAtaSmartReadLog,
            ),
        ),
        (
            "ata_smart_status",
            ata(vec![fd(), cdb_size()], linux_ata_smart_status_type(), RuntimeOp::LinuxAtaSmartStatus),
        ),
        (
            "ata_smart_enable",
            ata(vec![fd(), cdb_size()], linux_ata_result_type(), RuntimeOp::LinuxAtaSmartEnable),
        ),
        (
            "ata_smart_disable",
            ata(vec![fd(), cdb_size()], linux_ata_result_type(), RuntimeOp::LinuxAtaSmartDisable),
        ),
        (
            "ata_smart_start_self_test",
            ata(
                vec![fd(), param("kind", Type::Str), cdb_size()],
                linux_ata_result_type(),
                RuntimeOp::LinuxAtaSmartStartSelfTest,
            ),
        ),
        (
            "nvme_namespace_id",
            sig(vec![fd()], result(Type::UInt), false, RuntimeOp::LinuxNvmeNamespaceId),
        ),
        (
            "nvme_admin",
            nvme(nvme_params(default_param("data_len", Type::UInt)), RuntimeOp::LinuxNvmeAdmin),
        ),
        (
            "nvme_admin_command",
            nvme(nvme_params(param("data_len", Type::UInt)), RuntimeOp::LinuxNvmeAdminCommand),
        ),
        (
            "nvme_admin_command",
            nvme(nvme_params(param("data", Type::Bytes)), RuntimeOp::LinuxNvmeAdminCommand),
        ),
        (
            "nvme_identify_controller",
            nvme(vec![fd()], RuntimeOp::LinuxNvmeIdentifyController),
        ),
        (
            "nvme_identify_namespace",
            nvme(vec![fd(), param("nsid", Type::UInt)], RuntimeOp::LinuxNvmeIdentifyNamespace),
        ),
        (
            "nvme_log_page",
            nvme(
                vec![
                    fd(),
                    param("log_id", Type::UInt),
                    default_param("data_len", Type::UInt),
                    default_param("nsid", Type::UInt),
                ],
                RuntimeOp::LinuxNvmeLogPage,
            ),
        ),
        (
            "nvme_get_feature",
            nvme(
                vec![
                    fd(),
                    param("feature_id", Type::UInt),
                    default_param("nsid", Type::UInt),
                    default_param("select", Type::UInt),
                    default_param("cdw11", Type::UInt),
                    default_param("data_len", Type::UInt),
                ],
                RuntimeOp::LinuxNvmeGetFeature,
            ),
        ),
        (
            "nvme_set_feature",
            nvme(
                vec![
                    fd(),
                    param("feature_id", Type::UInt),
                    param("value", Type::UInt),
                    default_param("nsid", Type::UInt),
                    default_param("save", Type::Bool),
                ],
                RuntimeOp::LinuxNvmeSetFeature,
            ),
        ),
        (
            "storage_candidates",
            sig(
                vec![
                    default_param("sys_root", Type::Optional(Box::new(Type::Path))),
                    default_param("dev_root", Type::Optional(Box::new(Type::Path))),
                ],
                result(Type::List(Box::new(linux_storage_candidate_type()))),
                false,
                RuntimeOp::LinuxStorageCandidates,
            ),
        ),
    ]
}

type Doc = (&'static str, &'static str, &'static [&'static str]);

pub(super) fn function_doc(name: &str) -> Option<Doc> {
    Some(match name {
        "sg_io" => (
            "Sends a read-only SCSI command through SG_IO and returns the raw outcome.",
            "fd is an open descriptor (unix.open_fd) on a SCSI generic, block, or bsg node. data_len bytes are read from the device; 0 sends no data phase. Only commands known not to change the device are accepted: TEST UNIT READY, REQUEST SENSE, INQUIRY, MODE SENSE, LOG SENSE, READ CAPACITY, REPORT LUNS, and the READ commands; ATA pass-through and everything else need sg_io_command. status, host_status and driver_status are the device's own values and are data: a CHECK CONDITION returns normally with its sense bytes. data holds only the bytes transferred (data_len minus resid). A failing ioctl fails with its errno. timeout_ms defaults to 60000. Under a native-test linux fake the call is answered from the storage_fixture file and never reaches fd.",
            &["linux", "storage", "scsi", "read-only"],
        ),
        "sg_io_command" => (
            "Sends any SCSI command through SG_IO, reading data_len bytes or writing data.",
            "Mutating or unrestricted: the CDB is sent as given, so START STOP UNIT, WRITE, ATA pass-through and vendor commands all reach the device. An Int fourth argument is a device-to-host transfer length (0 for none); Bytes is a host-to-device payload. The result record is the same as sg_io. Failures carry the kernel errno; device status is data. Prefer sg_io and the typed ata_* helpers for reads.",
            &["linux", "storage", "scsi", "mutating", "privileged"],
        ),
        "ata_identify" => (
            "Runs ATA IDENTIFY DEVICE through ATA PASS-THROUGH and returns the 512-byte page.",
            "cdb_size selects the SAT pass-through form, 12 or 16 (default 16). The record carries the raw page in data and the ATA status, error and LBA registers from the device's status return descriptor. A device that sets ERR or DF, a transport failure, or a missing register descriptor is an error that includes the SCSI, host and ATA status.",
            &["linux", "storage", "ata", "read-only"],
        ),
        "ata_smart_read_data" => (
            "Runs ATA SMART READ DATA and returns the 512-byte attribute page.",
            "Same result and failure contract as ata_identify. The page holds the attribute table from offset 2 in 12-byte entries; decoding is left to the caller.",
            &["linux", "storage", "ata", "smart", "read-only"],
        ),
        "ata_smart_read_thresholds" => (
            "Runs ATA SMART READ THRESHOLDS and returns the 512-byte threshold page.",
            "Same result and failure contract as ata_identify. The page parallels the attribute table of ata_smart_read_data.",
            &["linux", "storage", "ata", "smart", "read-only"],
        ),
        "ata_smart_read_log" => (
            "Runs ATA SMART READ LOG for one log address and returns sectors * 512 bytes.",
            "log_address is 0 to 255 and sectors is 1 to 255 (default 1). Same result and failure contract as ata_identify.",
            &["linux", "storage", "ata", "smart", "read-only"],
        ),
        "ata_smart_status" => (
            "Runs ATA SMART RETURN STATUS and reports whether the drive predicts failure.",
            "passed is true for the healthy signature in LBA mid/high (0x4f, 0xc2) and false for the threshold-exceeded signature (0xf4, 0x2c); any other pair is an error. status, error, lba_mid and lba_high are the returned registers.",
            &["linux", "storage", "ata", "smart", "read-only"],
        ),
        "ata_smart_enable" => (
            "Enables SMART operations on an ATA drive.",
            "Mutating: ATA SMART ENABLE OPERATIONS changes drive state until disabled or power-cycled. Same result and failure contract as ata_identify; data is empty.",
            &["linux", "storage", "ata", "smart", "mutating", "privileged"],
        ),
        "ata_smart_disable" => (
            "Disables SMART operations on an ATA drive.",
            "Mutating: ATA SMART DISABLE OPERATIONS. Same result and failure contract as ata_identify; data is empty.",
            &["linux", "storage", "ata", "smart", "mutating", "privileged"],
        ),
        "ata_smart_start_self_test" => (
            "Starts or aborts an ATA SMART self-test with EXECUTE OFF-LINE IMMEDIATE.",
            "Mutating. kind is offline, short, extended, conveyance, or abort. The test runs in the background; captive and selective tests are not offered. Same result and failure contract as ata_identify; data is empty.",
            &["linux", "storage", "ata", "smart", "mutating", "privileged"],
        ),
        "nvme_namespace_id" => (
            "Returns the namespace id of an NVMe namespace node (NVME_IOCTL_ID).",
            "fd must name a namespace block device, not a controller character device. A failing ioctl fails with its errno.",
            &["linux", "storage", "nvme", "read-only"],
        ),
        "nvme_admin" => (
            "Sends a read-only NVMe admin command and returns status, result and data.",
            "Only Get Log Page (0x02), Identify (0x06) and Get Features (0x0a) are accepted; other opcodes need nvme_admin_command. data_len bytes are read from the device (0 for none); nsid and cdw10 to cdw15 default to 0, timeout_ms 0 selects the kernel default. status is the controller status the kernel returned (0 on success) and is data; a failing ioctl fails with its errno. Under a native-test linux fake the call is answered from the storage_fixture file.",
            &["linux", "storage", "nvme", "read-only"],
        ),
        "nvme_admin_command" => (
            "Sends any NVMe admin command, reading data_len bytes or writing data.",
            "Mutating or unrestricted: Format NVM, Sanitize, Firmware Commit and vendor opcodes all reach the controller. An Int third argument is a device-to-host length (0 for none); Bytes is a host-to-device payload. The kernel derives the transfer direction from the opcode. Same record and failure contract as nvme_admin.",
            &["linux", "storage", "nvme", "mutating", "privileged"],
        ),
        "nvme_identify_controller" => (
            "Reads the 4096-byte Identify Controller structure.",
            "A nonzero controller status is an error that names the status type and code; a failing ioctl fails with its errno. The record's data is the raw structure.",
            &["linux", "storage", "nvme", "read-only"],
        ),
        "nvme_identify_namespace" => (
            "Reads the 4096-byte Identify Namespace structure for nsid.",
            "Same failure contract as nvme_identify_controller.",
            &["linux", "storage", "nvme", "read-only"],
        ),
        "nvme_log_page" => (
            "Reads one NVMe log page.",
            "log_id is 0 to 255, data_len a positive multiple of 4 (default 512), nsid defaults to 0xffffffff for controller-wide pages. Same failure contract as nvme_identify_controller.",
            &["linux", "storage", "nvme", "read-only"],
        ),
        "nvme_get_feature" => (
            "Reads one NVMe feature with Get Features.",
            "select is the value selector 0 to 7 (0 current), cdw11 carries feature-specific input, and data_len is the buffer for features that return data (default 0). The feature value is the record's result. Same failure contract as nvme_identify_controller.",
            &["linux", "storage", "nvme", "read-only"],
        ),
        "nvme_set_feature" => (
            "Sets one NVMe feature with Set Features.",
            "Mutating. value is command dword 11; save asks the controller to persist the value across resets. Same failure contract as nvme_identify_controller.",
            &["linux", "storage", "nvme", "mutating", "privileged"],
        ),
        "storage_candidates" => (
            "Lists SCSI disks, NVMe namespaces and NVMe controllers that smartctl-style tools can address.",
            "Reads sysfs only and never opens a device node. sys_root and dev_root default to /sys and /dev. Each record names its kind (scsi_disk, nvme_namespace, nvme_controller), protocol, device path, and the model, serial, firmware, size and flags the kernel exposes, null where it exposes none. Partitions and NVMe multipath path nodes are not candidates.",
            &["linux", "storage", "discovery", "read-only"],
        ),
        _ => return None,
    })
}

pub(super) fn record_doc(name: &str) -> Doc {
    match name {
        "LinuxSgIo" => (
            "Reports one SG_IO command outcome.",
            "status is the SCSI status byte, host_status and driver_status the transport values; sense holds the returned sense bytes and data the bytes transferred. A nonzero status is data, not an error.",
            &["linux", "storage", "scsi"],
        ),
        "LinuxAtaResult" => (
            "Reports one ATA command accepted by the device.",
            "data is the raw page (empty for non-data commands); status, error and the LBA registers come from the ATA status return descriptor in sense, which is kept raw in sense.",
            &["linux", "storage", "ata"],
        ),
        "LinuxAtaSmartStatus" => (
            "Reports the SMART RETURN STATUS verdict.",
            "passed is false when the drive predicts failure; the registers are those the device returned.",
            &["linux", "storage", "ata", "smart"],
        ),
        "LinuxNvmeCommand" => (
            "Reports one NVMe admin command outcome.",
            "status is the kernel's return value: 0 on success, otherwise the controller status with the more and do-not-retry bits. result is completion dword 0.",
            &["linux", "storage", "nvme"],
        ),
        "LinuxStorageCandidate" => (
            "Describes one device that storage tools can address.",
            "Fields come from sysfs at query time; model, serial, firmware, size, rotational and removable are null where the kernel exposes none, and controller is set only for NVMe namespaces.",
            &["linux", "storage", "discovery"],
        ),
        other => panic!("missing record documentation for '{other}'"),
    }
}
