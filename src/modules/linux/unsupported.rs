#![allow(clippy::single_call_fn)]

use crate::runtime::value::{RecordMap, RuntimeError, Value};
use crate::source::Span;
use std::path::{Path, PathBuf};

pub(crate) fn write_device(
    _device: &Path,
    _source: &Path,
    span: Span,
) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn read_device(
    _device: &Path,
    _dest: &Path,
    _bytes: i64,
    span: Span,
) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn uevent_stream(span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn mount(
    _source: &str,
    _target: &Path,
    _fstype: &str,
    _options: &[String],
    span: Span,
) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn mount_all(span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn umount_all(_types: &[String], span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn swapon_all(span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn swapoff_all(span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn root_device(span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn link_up(_interface: &str, span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn link_down(_interface: &str, span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn set_ipv4_address(
    _interface: &str,
    _address: &str,
    _netmask: &str,
    span: Span,
) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn add_default_ipv4_route(
    _gateway: &str,
    _interface: &str,
    span: Span,
) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn flush_ipv4_addresses(_interface: &str, span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn del_default_ipv4_route(
    _gateway: &str,
    _interface: &str,
    span: Span,
) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn dhcp_socket(_interface: &str, span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn dhcp_send(_fd: i64, _payload: &[u8], span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn dhcp_recv(_fd: i64, _timeout_ms: i64, span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn dhcp_close(_fd: i64, span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn dhcp_send_release(
    _interface: &str,
    _address: &str,
    _server_id: &str,
    span: Span,
) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn interfaces(span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn routes(span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn meminfo(span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn modules(span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn dmesg(span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn is_mountpoint(_path: &Path, span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn disk_usage(_path: Option<&Path>, span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn sysctl_get(_key: &str, span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn sysctl_set(_key: &str, _value: &str, span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn file_attrs(_path: &Path, span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn set_file_attrs(_path: &Path, _flags: i64, span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn file_version(_path: &Path, span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn set_file_version(
    _path: &Path,
    _version: i64,
    span: Span,
) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn sysctl_load_dirs(
    _dirs: &[PathBuf],
    _fallback: Option<&Path>,
    span: Span,
) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn kill_all(
    _signal: i32,
    _except_pid1: bool,
    span: Span,
) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn chroot(_path: &Path, span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn mknod(
    _path: &Path,
    _kind: &str,
    _major: i64,
    _minor: i64,
    span: Span,
) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn insmod(_path: &Path, _params: &str, span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn rmmod(_name: &str, _force: bool, span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn pivot_root(
    _new_root: &Path,
    _put_old: &Path,
    span: Span,
) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn switch_root(
    _new_root: &Path,
    _init: &Path,
    span: Span,
) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn hwclock(span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn set_hwclock(_epoch_ms: i64, span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn set_system_clock(_epoch_ms: i64, span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn rfkill_list(span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn rfkill_set(_id: i64, _blocked: bool, span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn loop_attach(
    _file: &Path,
    _device: Option<&Path>,
    span: Span,
) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn loop_detach(_device: &Path, span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn loop_list(span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn mkswap(_device: &Path, span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn swapon(_device: &Path, _priority: i64, span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn swapoff(_device: &Path, span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn blkid(_device: &Path, span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn modinfo(_name: &str, span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn modprobe(_name: &str, _params: &str, span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn depmod(_version: &str, span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn open_files(_pid: Option<i64>, span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn block_devices(span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn partition_table(_device: &Path, span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn write_partition_table(
    _device: &Path,
    _table: &RecordMap,
    span: Span,
) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn fsck(
    _device: &Path,
    _fstype: &str,
    _repair: bool,
    span: Span,
) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn halt(span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn poweroff(span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

pub(crate) fn reboot_system(span: Span) -> Result<Value, RuntimeError> {
    Ok(unsupported(span))
}

fn unsupported(span: Span) -> Value {
    Value::err(Value::Error(Box::new(
        RuntimeError::new(
            "linux-unsupported",
            "real linux.* primitives are only available on Linux",
        )
        .with_span(span),
    )))
}
