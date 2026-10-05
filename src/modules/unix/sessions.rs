//! Login session records and the load average, for `who`, `users`, `pinky`,
//! and `uptime`. Every failure is a host error that carries its errno.

use crate::modules::RuntimeOp;
use crate::modules::process::{Args, host_error, key, name_error};
use crate::runtime::value::{FloatValue, RecordMap, RuntimeError, Value};
use std::io;

pub(crate) fn handles(op: RuntimeOp) -> bool {
    matches!(op, RuntimeOp::UnixReadUtmp | RuntimeOp::UnixLoadAverage)
}

pub(crate) fn call(op: RuntimeOp, args: &Args<'_>) -> Result<Value, RuntimeError> {
    match op {
        RuntimeOp::UnixReadUtmp => read_utmp(args),
        RuntimeOp::UnixLoadAverage => load_average(args),
        _ => unreachable!("session primitive expected"),
    }
}

fn load_average(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let mut load = [0f64; 3];
    // SAFETY: `load` has room for the three samples requested.
    if unsafe { libc::getloadavg(load.as_mut_ptr(), 3) } != 3 {
        return Err(host_error(
            "unix-load-average",
            io::Error::from_raw_os_error(libc::ENOSYS),
            args.span(),
        ));
    }
    Ok(Value::ok(Value::Record(RecordMap::from([
        (key("one"), Value::Float(FloatValue::new(load[0]))),
        (key("five"), Value::Float(FloatValue::new(load[1]))),
        (key("fifteen"), Value::Float(FloatValue::new(load[2]))),
    ]))))
}

const DEFAULT_UTMP: &str = "/var/run/utmp";

/// Reads a utmp-format file. A trailing partial record is ignored, so a file
/// that holds no records at all reads as an empty list; a missing or
/// unreadable file is an error.
#[cfg(any(target_os = "linux", target_os = "android"))]
fn read_utmp(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let span = args.span();
    let path = args
        .path_opt(0)?
        .unwrap_or_else(|| std::path::PathBuf::from(DEFAULT_UTMP));
    let shown = path.display().to_string();
    let bytes = std::fs::read(&path)
        .map_err(|error| name_error(&shown, host_error("unix-read-utmp", error, span)))?;
    Ok(Value::ok(Value::List(
        bytes.chunks_exact(RECORD_SIZE).map(utmp_record).collect(),
    )))
}

#[cfg(not(any(target_os = "linux", target_os = "android")))]
fn read_utmp(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let _ = (DEFAULT_UTMP, name_error, host_error);
    Err(RuntimeError::new(
        "unsupported",
        "utmp records are only readable on Linux",
    )
    .with_span(args.span()))
}

/// `struct utmp` as glibc and musl lay it out on 64-bit Linux, in native byte
/// order: type, pid, line, id, user, host, exit status, session, time, address.
#[cfg(any(target_os = "linux", target_os = "android"))]
const RECORD_SIZE: usize = 384;

#[cfg(any(target_os = "linux", target_os = "android"))]
fn utmp_record(raw: &[u8]) -> Value {
    let int16 = |at: usize| i64::from(i16::from_ne_bytes([raw[at], raw[at + 1]]));
    let int32 = |at: usize| {
        i64::from(i32::from_ne_bytes([
            raw[at],
            raw[at + 1],
            raw[at + 2],
            raw[at + 3],
        ]))
    };
    let text = |from: usize, to: usize| {
        let field = &raw[from..to];
        let end = field.iter().position(|byte| *byte == 0).unwrap_or(field.len());
        Value::Str(String::from_utf8_lossy(&field[..end]).as_ref().into())
    };
    let kind = int16(0);
    Value::Record(RecordMap::from([
        (key("type"), Value::Int(kind)),
        (key("kind"), Value::Str(kind_name(kind).into())),
        (key("pid"), Value::Int(int32(4))),
        (key("line"), text(8, 40)),
        (key("id"), text(40, 44)),
        (key("user"), text(44, 76)),
        (key("host"), text(76, 332)),
        (key("termination"), Value::Int(int16(332))),
        (key("exit_status"), Value::Int(int16(334))),
        (key("session"), Value::Int(int32(336))),
        (key("time_sec"), Value::Int(int32(340))),
        (key("time_usec"), Value::Int(int32(344))),
        (key("addr"), Value::Str(address(&raw[348..364]).into())),
    ]))
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn kind_name(kind: i64) -> &'static str {
    match kind {
        0 => "empty",
        1 => "run_level",
        2 => "boot_time",
        3 => "new_time",
        4 => "old_time",
        5 => "init_process",
        6 => "login_process",
        7 => "user_process",
        8 => "dead_process",
        9 => "accounting",
        _ => "unknown",
    }
}

/// The remote address of a session: dotted quad when only the first word is
/// set, IPv6 text when more is, and empty when there is none.
#[cfg(any(target_os = "linux", target_os = "android"))]
fn address(raw: &[u8]) -> String {
    if raw.iter().all(|byte| *byte == 0) {
        String::new()
    } else if raw[4..].iter().all(|byte| *byte == 0) {
        std::net::Ipv4Addr::new(raw[0], raw[1], raw[2], raw[3]).to_string()
    } else {
        let mut octets = [0u8; 16];
        octets.copy_from_slice(raw);
        std::net::Ipv6Addr::from(octets).to_string()
    }
}
