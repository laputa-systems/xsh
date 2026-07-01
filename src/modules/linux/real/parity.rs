use crate::modules::linux::api;
use crate::runtime::value::{RecordMap, RuntimeError, Value};
use crate::source::Span;
use std::path::Path;

pub(crate) fn blkid(device: &Path, span: Span) -> Result<Value, RuntimeError> {
    api::blkid(device, span)
}

pub(crate) fn modinfo(name: &str, span: Span) -> Result<Value, RuntimeError> {
    api::modinfo(name, span)
}

pub(crate) fn modprobe(name: &str, params: &str, span: Span) -> Result<Value, RuntimeError> {
    api::modprobe(name, params, span)
}

pub(crate) fn depmod(version: &str, span: Span) -> Result<Value, RuntimeError> {
    api::depmod(version, span)
}

pub(crate) fn open_files(pid: Option<i64>, span: Span) -> Result<Value, RuntimeError> {
    api::open_files(pid, span)
}

pub(crate) fn block_devices(span: Span) -> Result<Value, RuntimeError> {
    api::block_devices(span)
}

pub(crate) fn partition_table(device: &Path, span: Span) -> Result<Value, RuntimeError> {
    api::partition_table(device, span)
}

pub(crate) fn write_partition_table(
    device: &Path,
    table: &RecordMap,
    span: Span,
) -> Result<Value, RuntimeError> {
    api::write_partition_table(device, table, span)
}

pub(crate) fn fsck(
    device: &Path,
    fstype: &str,
    repair: bool,
    span: Span,
) -> Result<Value, RuntimeError> {
    api::fsck(device, fstype, repair, span)
}
