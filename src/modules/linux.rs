#[cfg(any(target_os = "linux", test))]
use crate::runtime::value::Value;
#[cfg(any(target_os = "linux", test))]
use rustc_hash::FxHashMap;
#[cfg(any(target_os = "linux", test))]
use std::fs::File;
#[cfg(any(target_os = "linux", test))]
use std::path::PathBuf;
#[cfg(any(target_os = "linux", test))]
use std::sync::Arc;

#[cfg(any(target_os = "linux", test))]
const MODULE_EXTENSIONS: &[&str] = &[".ko", ".ko.gz", ".ko.xz", ".ko.bz2"];
#[cfg(any(target_os = "linux", test))]
const EXT_SUPER_OFFSET: u64 = 1024;
#[cfg(any(target_os = "linux", test))]
const EXT_SUPER_SIZE: usize = 0x200;
#[cfg(any(target_os = "linux", test))]
const EXT_MAGIC_OFFSET: usize = 0x38;
#[cfg(any(target_os = "linux", test))]
const EXT_FEATURE_COMPAT_OFFSET: usize = 0x5c;
#[cfg(any(target_os = "linux", test))]
const EXT_FEATURE_INCOMPAT_OFFSET: usize = 0x60;
#[cfg(any(target_os = "linux", test))]
const EXT_FEATURE_RO_COMPAT_OFFSET: usize = 0x64;
#[cfg(any(target_os = "linux", test))]
const EXT_UUID_OFFSET: usize = 0x68;
#[cfg(any(target_os = "linux", test))]
const EXT_LABEL_OFFSET: usize = 0x78;
#[cfg(any(target_os = "linux", test))]
const EXT3_FEATURE_HAS_JOURNAL: u32 = 0x0004;
#[cfg(any(target_os = "linux", test))]
const EXT4_FEATURE_RO_COMPAT_HUGE_FILE: u32 = 0x0008;
#[cfg(any(target_os = "linux", test))]
const EXT4_FEATURE_RO_COMPAT_DIR_NLINK: u32 = 0x0020;
#[cfg(any(target_os = "linux", test))]
const EXT4_FEATURE_INCOMPAT_EXTENTS: u32 = 0x0040;
#[cfg(any(target_os = "linux", test))]
const EXT4_FEATURE_INCOMPAT_64BIT: u32 = 0x0080;
#[cfg(any(target_os = "linux", test))]
const XFS_SUPER_SIZE: usize = 0x200;
#[cfg(any(target_os = "linux", test))]
const XFS_UUID_OFFSET: usize = 0x20;
#[cfg(any(target_os = "linux", test))]
const XFS_LABEL_OFFSET: usize = 0x6c;
#[cfg(any(target_os = "linux", test))]
const BTRFS_SUPER_OFFSET: u64 = 64 * 1024;
#[cfg(any(target_os = "linux", test))]
const BTRFS_SUPER_SIZE: usize = 0x200;
#[cfg(any(target_os = "linux", test))]
const BTRFS_FSID_OFFSET: usize = 0x20;
#[cfg(any(target_os = "linux", test))]
const BTRFS_MAGIC_OFFSET: usize = 0x40;
#[cfg(any(target_os = "linux", test))]
const ISO9660_PVD_OFFSET: u64 = 16 * 2048;
#[cfg(any(target_os = "linux", test))]
const ISO9660_PVD_SIZE: usize = 2048;
#[cfg(any(target_os = "linux", test))]
const ISO9660_LABEL_OFFSET: usize = 40;
#[cfg(any(target_os = "linux", test))]
const ISO9660_LABEL_SIZE: usize = 32;
#[cfg(any(target_os = "linux", test))]
const VFAT_BOOT_SIZE: usize = 512;
#[cfg(any(target_os = "linux", test))]
const VFAT_SERIAL16_OFFSET: usize = 39;
#[cfg(any(target_os = "linux", test))]
const VFAT_LABEL16_OFFSET: usize = 43;
#[cfg(any(target_os = "linux", test))]
const VFAT_SERIAL32_OFFSET: usize = 67;
#[cfg(any(target_os = "linux", test))]
const VFAT_LABEL32_OFFSET: usize = 71;
#[cfg(any(target_os = "linux", test))]
const MBR_SIGNATURE_OFFSET: usize = 510;
#[cfg(any(target_os = "linux", test))]
const MBR_PARTITION_OFFSET: usize = 446;
#[cfg(any(target_os = "linux", test))]
const GPT_HEADER_SIZE: usize = 92;
#[cfg(any(target_os = "linux", test))]
const GPT_ENTRY_SIZE: usize = 128;
#[cfg(any(target_os = "linux", test))]
const BLKSSZGET: libc::c_ulong = 0x1268;
#[cfg(any(target_os = "linux", test))]
const BLKGETSIZE64: libc::c_ulong = 0x8008_1272;
#[cfg(any(target_os = "linux", test))]
const BLKRRPART: libc::c_ulong = 0x125f;

#[cfg(any(target_os = "linux", test))]
#[derive(Clone, Debug, Default)]
struct BlkidInfo {
    fstype: String,
    uuid: String,
    label: String,
    part_table_type: String,
    part_entry_uuid: String,
}

#[cfg(any(target_os = "linux", test))]
#[derive(Clone, Debug)]
struct ModuleEntry {
    name: String,
    relative_path: String,
    path: PathBuf,
    metadata: ModuleMetadata,
}

#[cfg(any(target_os = "linux", test))]
#[derive(Clone, Debug, Default)]
struct ModuleMetadata {
    fields: Vec<(String, String)>,
}

#[cfg(any(target_os = "linux", test))]
#[derive(Clone, Debug, Default)]
struct ModuleIndex {
    entries: Vec<ModuleEntry>,
    by_name: FxHashMap<String, usize>,
}

#[cfg(any(target_os = "linux", test))]
#[derive(Debug)]
struct Device {
    file: File,
    sector_size: u64,
    total_bytes: u64,
    is_block: bool,
}

#[cfg(any(target_os = "linux", test))]
#[derive(Clone, Debug)]
struct PartitionRecord {
    index: i64,
    start: i64,
    end: i64,
    size: i64,
    kind: String,
    uuid: String,
    name: String,
}

#[cfg(any(target_os = "linux", test))]
fn str_value(value: impl Into<Arc<str>>) -> Value {
    Value::Str(value.into())
}

#[cfg(any(target_os = "linux", test))]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod api;
#[cfg(any(target_os = "linux", test))]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod block;
#[cfg(any(target_os = "linux", test))]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod kernel;
#[cfg(any(target_os = "linux", test))]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod process;
#[cfg(target_os = "linux")]
mod real;
#[cfg(test)]
mod tests;
#[cfg(target_os = "linux")]
use real as imp;

#[cfg(not(target_os = "linux"))]
mod unsupported;
#[cfg(not(target_os = "linux"))]
use unsupported as imp;

pub(crate) use imp::{
    add_default_ipv4_route, blkid, block_devices, chroot, del_default_ipv4_route, depmod,
    dhcp_close, dhcp_recv, dhcp_send, dhcp_send_release, dhcp_socket, disk_usage, dmesg,
    file_attrs, file_version, flush_ipv4_addresses, fsck, halt, hwclock, insmod, interfaces,
    is_mountpoint, kill_all, link_down, link_up, loop_attach, loop_detach, loop_list, meminfo,
    mknod, mkswap, modinfo, modprobe, modules, mount, mount_all, open_files, partition_table,
    pivot_root, poweroff, read_device, reboot_system, rfkill_list, rfkill_set, rmmod, root_device,
    routes, set_file_attrs, set_file_version, set_hwclock, set_ipv4_address, set_system_clock,
    swapoff, swapoff_all, swapon, swapon_all, switch_root, sysctl_get, sysctl_load_dirs,
    sysctl_set, uevent_stream, umount_all, write_device, write_partition_table,
};
