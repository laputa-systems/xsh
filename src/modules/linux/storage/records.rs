//! Record values returned by the storage primitives.

use super::ata::{AtaOutcome, SmartStatus};
use super::transport::{NvmeResponse, SgResponse};
use crate::runtime::value::{RecordMap, Value};
use std::sync::Arc;

fn int(value: impl Into<i64>) -> Value {
    Value::Int(value.into())
}

fn record(fields: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    Value::Record(
        fields
            .into_iter()
            .map(|(name, value)| (Arc::from(name), value))
            .collect::<RecordMap>(),
    )
}

pub(super) fn sg_io(response: SgResponse) -> Value {
    record([
        ("status", int(response.status)),
        ("host_status", int(response.host_status)),
        ("driver_status", int(response.driver_status)),
        ("sense", Value::Bytes(response.sense)),
        ("data", Value::Bytes(response.data)),
        ("resid", int(response.resid)),
    ])
}

pub(super) fn ata_result(outcome: AtaOutcome) -> Value {
    let registers = outcome.registers;
    record([
        ("data", Value::Bytes(outcome.data)),
        ("status", int(registers.status)),
        ("error", int(registers.error)),
        ("sector_count", int(registers.sector_count)),
        ("lba_low", int(registers.lba_low)),
        ("lba_mid", int(registers.lba_mid)),
        ("lba_high", int(registers.lba_high)),
        ("device", int(registers.device)),
        ("sense", Value::Bytes(outcome.sense)),
    ])
}

pub(super) fn smart_status(status: SmartStatus) -> Value {
    record([
        ("passed", Value::Bool(status.passed)),
        ("status", int(status.registers.status)),
        ("error", int(status.registers.error)),
        ("lba_mid", int(status.registers.lba_mid)),
        ("lba_high", int(status.registers.lba_high)),
    ])
}

pub(super) fn nvme(response: NvmeResponse) -> Value {
    record([
        ("status", int(response.status)),
        ("result", int(response.result)),
        ("data", Value::Bytes(response.data)),
    ])
}
