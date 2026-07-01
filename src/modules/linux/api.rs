use super::block::{
    blkid_info, blkid_record, fsck_impl, io_error, partition_table_impl, write_partition_table_impl,
};
use super::kernel::{depmod_impl, modinfo_impl, modprobe_impl};
use super::process::open_files_impl;
use crate::runtime::value::{RecordMap, RuntimeError, Value};
use crate::source::Span;
use std::path::Path;

pub(crate) fn blkid(device: &Path, span: Span) -> Result<Value, RuntimeError> {
    match blkid_info(device) {
        Ok(info) => Ok(Value::ok(blkid_record(info))),
        Err(error) => Ok(io_error("linux-blkid", error, span)),
    }
}

#[allow(clippy::single_call_fn)]
pub(crate) fn modinfo(name: &str, span: Span) -> Result<Value, RuntimeError> {
    match modinfo_impl(name, span) {
        Ok(record) => Ok(Value::ok(record)),
        Err(error) => Ok(Value::err(Value::Error(Box::new(error)))),
    }
}

pub(crate) fn modprobe(name: &str, params: &str, span: Span) -> Result<Value, RuntimeError> {
    match modprobe_impl(name, params, span) {
        Ok(()) => Ok(Value::ok(Value::Unit)),
        Err(error) => Ok(Value::err(Value::Error(Box::new(error)))),
    }
}

pub(crate) fn depmod(version: &str, span: Span) -> Result<Value, RuntimeError> {
    match depmod_impl(version, span) {
        Ok(()) => Ok(Value::ok(Value::Unit)),
        Err(error) => Ok(Value::err(Value::Error(Box::new(error)))),
    }
}

pub(crate) fn open_files(pid: Option<i64>, span: Span) -> Result<Value, RuntimeError> {
    match open_files_impl(pid, span) {
        Ok(records) => Ok(Value::ok(Value::List(records))),
        Err(error) => Ok(Value::err(Value::Error(Box::new(error)))),
    }
}

#[allow(clippy::single_call_fn)]
pub(crate) fn block_devices(span: Span) -> Result<Value, RuntimeError> {
    match super::block::block_devices_impl(span) {
        Ok(records) => Ok(Value::ok(Value::List(records))),
        Err(error) => Ok(Value::err(Value::Error(Box::new(error)))),
    }
}

pub(crate) fn partition_table(device: &Path, span: Span) -> Result<Value, RuntimeError> {
    match partition_table_impl(device, span) {
        Ok(record) => Ok(Value::ok(record)),
        Err(error) => Ok(Value::err(Value::Error(Box::new(error)))),
    }
}

pub(crate) fn write_partition_table(
    device: &Path,
    table: &RecordMap,
    span: Span,
) -> Result<Value, RuntimeError> {
    match write_partition_table_impl(device, table, span) {
        Ok(()) => Ok(Value::ok(Value::Unit)),
        Err(error) => Ok(Value::err(Value::Error(Box::new(error)))),
    }
}

pub(crate) fn fsck(
    device: &Path,
    fstype: &str,
    repair: bool,
    span: Span,
) -> Result<Value, RuntimeError> {
    match fsck_impl(device, fstype, repair, span) {
        Ok(record) => Ok(Value::ok(record)),
        Err(error) => Ok(Value::err(Value::Error(Box::new(error)))),
    }
}
