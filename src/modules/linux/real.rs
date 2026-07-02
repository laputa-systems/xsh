use crate::runtime::value::{LiveStream, RuntimeError, Value};
use crate::source::Span;
use common::parse_uevent_message;
use rustix::net::netlink::{self, SocketAddrNetlink};
use rustix::net::{AddressFamily, RecvFlags, SocketFlags, SocketType, bind, recv, socket_with};
use std::io;
use std::os::fd::OwnedFd;

const PROC_MEMINFO: &str = "/proc/meminfo";
const PROC_MODULES: &str = "/proc/modules";
const DEV_KMSG: &str = "/dev/kmsg";
const SYSLOG_ACTION_READ_ALL: libc::c_int = 3;
const SYSLOG_ACTION_SIZE_BUFFER: libc::c_int = 10;
const FS_SECRM_FL: u32 = 0x0000_0001;
const FS_UNRM_FL: u32 = 0x0000_0002;
const FS_COMPR_FL: u32 = 0x0000_0004;
const FS_SYNC_FL: u32 = 0x0000_0008;
const FS_IMMUTABLE_FL: u32 = 0x0000_0010;
const FS_APPEND_FL: u32 = 0x0000_0020;
const FS_NODUMP_FL: u32 = 0x0000_0040;
const FS_NOATIME_FL: u32 = 0x0000_0080;
const FS_INDEX_FL: u32 = 0x0000_1000;
const FS_JOURNAL_DATA_FL: u32 = 0x0000_4000;
const FS_NOTAIL_FL: u32 = 0x0000_8000;
const FS_DIRSYNC_FL: u32 = 0x0001_0000;
const FS_TOPDIR_FL: u32 = 0x0002_0000;
const RTC_RD_TIME: libc::c_ulong = 0x8024_7009;
const RTC_SET_TIME: libc::c_ulong = 0x4024_700a;
const SWAP_MAGIC: &[u8; 10] = b"SWAPSPACE2";
const SWAP_HEADER_OFFSET: u64 = 1024;
const SWAP_HEADER_SIZE: usize = 129 * 4;
const SWAP_UUID_OFFSET: usize = 12;
const LOOP_SET_FD: libc::c_int = 0x4C00;
const LOOP_CLR_FD: libc::c_int = 0x4C01;
const LOOP_SET_STATUS64: libc::c_int = 0x4C04;
const LOOP_GET_STATUS64: libc::c_int = 0x4C05;
const LOOP_CTL_GET_FREE: libc::c_int = 0x4C82;
const LO_NAME_SIZE: usize = 64;
const LO_KEY_SIZE: usize = 32;
const SWAP_FLAG_PREFER: libc::c_int = 0x8000;
const SWAP_FLAG_PRIO_SHIFT: libc::c_int = 0;
const UEVENT_GROUP_KERNEL: u32 = 1;
const UEVENT_BUFFER_SIZE: usize = 64 * 1024;

#[derive(Clone, Debug)]
struct FstabEntry {
    spec: String,
    file: String,
    vfstype: String,
    mntops: Vec<String>,
}

#[derive(Clone, Debug)]
struct MountEntry {
    source: String,
    target: String,
    fstype: String,
}

struct UeventStream {
    fd: OwnedFd,
    buffer: Vec<u8>,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct LinuxRtcTime {
    tm_sec: libc::c_int,
    tm_min: libc::c_int,
    tm_hour: libc::c_int,
    tm_mday: libc::c_int,
    tm_mon: libc::c_int,
    tm_year: libc::c_int,
    tm_wday: libc::c_int,
    tm_yday: libc::c_int,
    tm_isdst: libc::c_int,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct LoopInfo64 {
    lo_device: u64,
    lo_inode: u64,
    lo_rdevice: u64,
    lo_offset: u64,
    lo_sizelimit: u64,
    lo_number: u32,
    lo_encrypt_type: u32,
    lo_encrypt_key_size: u32,
    lo_flags: u32,
    lo_file_name: [u8; LO_NAME_SIZE],
    lo_crypt_name: [u8; LO_NAME_SIZE],
    lo_encrypt_key: [u8; LO_KEY_SIZE],
    lo_init: [u64; 2],
}

impl Default for LoopInfo64 {
    fn default() -> Self {
        Self {
            lo_device: 0,
            lo_inode: 0,
            lo_rdevice: 0,
            lo_offset: 0,
            lo_sizelimit: 0,
            lo_number: 0,
            lo_encrypt_type: 0,
            lo_encrypt_key_size: 0,
            lo_flags: 0,
            lo_file_name: [0; LO_NAME_SIZE],
            lo_crypt_name: [0; LO_NAME_SIZE],
            lo_encrypt_key: [0; LO_KEY_SIZE],
            lo_init: [0; 2],
        }
    }
}

impl UeventStream {
    fn open(span: Span) -> Result<Self, RuntimeError> {
        let fd = socket_with(
            AddressFamily::NETLINK,
            SocketType::DGRAM,
            SocketFlags::CLOEXEC,
            Some(netlink::KOBJECT_UEVENT),
        )
        .map_err(|error| {
            RuntimeError::new("linux-uevent", io::Error::from(error).to_string()).with_span(span)
        })?;

        let addr = SocketAddrNetlink::new(0, UEVENT_GROUP_KERNEL);
        bind(&fd, &addr).map_err(|error| {
            RuntimeError::new("linux-uevent", io::Error::from(error).to_string()).with_span(span)
        })?;

        Ok(Self {
            fd,
            buffer: vec![0; UEVENT_BUFFER_SIZE],
        })
    }
}

impl LiveStream for UeventStream {
    fn next(&mut self, span: Span) -> Result<Option<Value>, RuntimeError> {
        loop {
            let len = match recv(&self.fd, &mut self.buffer[..], RecvFlags::empty()) {
                Ok((len, _)) => len,
                Err(error) => {
                    let error = io::Error::from(error);
                    if error.kind() == io::ErrorKind::Interrupted {
                        continue;
                    }
                    return Err(
                        RuntimeError::new("linux-uevent", error.to_string()).with_span(span)
                    );
                }
            };
            if len == 0 {
                return Ok(None);
            }
            return parse_uevent_message(&self.buffer[..len], span).map(Some);
        }
    }
}

mod boot;
mod common;
mod device;
mod fs;
mod kernel;
mod mount;
mod net;
mod parity;

pub(crate) use boot::{
    chroot, halt, hwclock, insmod, kill_all, mknod, pivot_root, poweroff, reboot_system,
    rfkill_list, rfkill_set, rmmod, set_hwclock, set_system_clock, switch_root,
};
pub(crate) use device::{
    loop_attach, loop_detach, loop_list, mkswap, read_device, swapoff, swapon, uevent_stream,
    write_device,
};
pub(crate) use fs::{
    disk_usage, file_attrs, file_version, is_mountpoint, set_file_attrs, set_file_version,
    sysctl_get, sysctl_load_dirs, sysctl_set,
};
pub(crate) use kernel::{dmesg, meminfo, modules};
pub(crate) use mount::{mount, mount_all, root_device, swapoff_all, swapon_all, umount_all};
pub(crate) use net::{
    add_default_ipv4_route, del_default_ipv4_route, dhcp_close, dhcp_recv, dhcp_send,
    dhcp_send_release, dhcp_socket, flush_ipv4_addresses, interfaces, link_down, link_up, routes,
    set_ipv4_address,
};
pub(crate) use parity::{
    blkid, block_devices, depmod, fsck, modinfo, modprobe, open_files, partition_table,
    write_partition_table,
};
