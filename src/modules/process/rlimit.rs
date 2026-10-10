//! Resource limits of this process or of another one. Every failure is a host
//! error that carries its errno.

use super::prims::{Args, Slot, host_error, invalid, key, ok, optional_int, pid_arg};
use crate::modules::RuntimeOp;
use crate::runtime::value::{RecordMap, RuntimeError, Value};
use rustix::process::Resource;
#[cfg(not(any(target_os = "linux", target_os = "android")))]
use rustix::process::{self as rprocess, Rlimit};
use std::io;

/// The limit the host reports for a resource; `None` is unlimited.
#[derive(Clone, Copy)]
struct Limit {
    soft: Option<u64>,
    hard: Option<u64>,
}

/// The value prlimit(2) uses for no limit.
#[cfg(any(target_os = "linux", target_os = "android"))]
const INFINITY: u64 = u64::MAX;

const RLIMIT_NAMES: &[(&str, Resource)] = &[
    ("cpu", Resource::Cpu),
    ("fsize", Resource::Fsize),
    ("data", Resource::Data),
    ("stack", Resource::Stack),
    ("core", Resource::Core),
    ("rss", Resource::Rss),
    ("nproc", Resource::Nproc),
    ("nofile", Resource::Nofile),
    ("memlock", Resource::Memlock),
    ("as", Resource::As),
    #[cfg(any(target_os = "linux", target_os = "android"))]
    ("locks", Resource::Locks),
    #[cfg(any(target_os = "linux", target_os = "android"))]
    ("sigpending", Resource::Sigpending),
    #[cfg(any(target_os = "linux", target_os = "android"))]
    ("msgqueue", Resource::Msgqueue),
    #[cfg(any(target_os = "linux", target_os = "android"))]
    ("nice", Resource::Nice),
    #[cfg(any(target_os = "linux", target_os = "android"))]
    ("rtprio", Resource::Rtprio),
    #[cfg(any(target_os = "linux", target_os = "android"))]
    ("rttime", Resource::Rttime),
];

/// Whether `op` is one of the resource-limit primitives.
pub(super) fn handles(op: RuntimeOp) -> bool {
    matches!(
        op,
        RuntimeOp::ProcessRlimit | RuntimeOp::ProcessRlimits | RuntimeOp::ProcessSetRlimit
    )
}

pub(super) fn call(op: RuntimeOp, args: &Args<'_>) -> Result<Value, RuntimeError> {
    match op {
        RuntimeOp::ProcessRlimit => rlimit(args),
        RuntimeOp::ProcessRlimits => rlimits(args),
        RuntimeOp::ProcessSetRlimit => set_rlimit(args),
        _ => unreachable!("resource-limit primitive expected"),
    }
}

/// Reads the limit of `resource` for `pid` (0 is this process) and, when `new`
/// is given, replaces it, returning the limit as it was before.
#[cfg(any(target_os = "linux", target_os = "android"))]
fn prlimit(pid: i32, resource: Resource, new: Option<Limit>) -> io::Result<Limit> {
    // rustix's prlimit always writes a new value, so reading another process
    // needs the system call itself.
    #[repr(C)]
    struct Raw {
        current: u64,
        maximum: u64,
    }
    let raw = |limit: Option<u64>| limit.unwrap_or(INFINITY);
    let kept = |value: u64| (value != INFINITY).then_some(value);
    let replacement = new.map(|limit| Raw {
        current: raw(limit.soft),
        maximum: raw(limit.hard),
    });
    let mut old = Raw {
        current: 0,
        maximum: 0,
    };
    // SAFETY: both pointers are null or point at live `Raw` values, which
    // match the kernel's struct rlimit64.
    let status = unsafe {
        libc::syscall(
            libc::SYS_prlimit64,
            libc::c_long::from(pid),
            libc::c_long::from(resource as u32),
            replacement
                .as_ref()
                .map_or(std::ptr::null(), std::ptr::from_ref),
            std::ptr::from_mut(&mut old),
        )
    };
    if status != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(Limit {
        soft: kept(old.current),
        hard: kept(old.maximum),
    })
}

/// The hosts without prlimit(2) can only reach the calling process.
#[cfg(not(any(target_os = "linux", target_os = "android")))]
fn prlimit(pid: i32, resource: Resource, new: Option<Limit>) -> io::Result<Limit> {
    if pid != 0 {
        return Err(io::Error::from_raw_os_error(libc::ENOSYS));
    }
    let old = rprocess::getrlimit(resource);
    if let Some(limit) = new {
        rprocess::setrlimit(
            resource,
            Rlimit {
                current: limit.soft,
                maximum: limit.hard,
            },
        )?;
    }
    Ok(Limit {
        soft: old.current,
        hard: old.maximum,
    })
}

fn target(args: &Args<'_>, index: usize, operation: &str) -> Result<i32, RuntimeError> {
    let pid = pid_arg(args.int_or(index, 0)?, operation, args.span())?;
    Ok(pid.map_or(0, |pid| pid.as_raw_nonzero().get()))
}

fn resource_arg(args: &Args<'_>, index: usize) -> Result<(&'static str, Resource), RuntimeError> {
    let name = args.str(index)?;
    RLIMIT_NAMES
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .copied()
        .ok_or_else(|| {
            invalid(
                "invalid-argument",
                format!(
                    "unknown resource `{name}`; expected one of {}",
                    RLIMIT_NAMES
                        .iter()
                        .map(|(name, _)| *name)
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                args.span(),
            )
        })
}

/// `None` is unlimited; a finite value beyond `Int` saturates.
fn limit_value(limit: Option<u64>) -> Value {
    optional_int(limit.map(|limit| i64::try_from(limit).unwrap_or(i64::MAX)))
}

fn rlimit_record(name: &str, limit: Limit) -> Value {
    Value::Record(RecordMap::from([
        (key("resource"), Value::Str(name.into())),
        (key("soft"), limit_value(limit.soft)),
        (key("hard"), limit_value(limit.hard)),
    ]))
}

fn rlimit(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let (name, resource) = resource_arg(args, 0)?;
    let pid = target(args, 1, "process.rlimit")?;
    match prlimit(pid, resource, None) {
        Ok(limit) => ok(rlimit_record(name, limit)),
        Err(error) => Err(host_error("process-rlimit", error, args.span())),
    }
}

fn rlimits(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let pid = target(args, 0, "process.rlimits")?;
    let mut records = Vec::with_capacity(RLIMIT_NAMES.len());
    for (name, resource) in RLIMIT_NAMES {
        match prlimit(pid, *resource, None) {
            Ok(limit) => records.push(rlimit_record(name, limit)),
            Err(error) => return Err(host_error("process-rlimit", error, args.span())),
        }
    }
    ok(Value::List(records))
}

/// Sets the soft and hard limits named by non-omitted arguments; `null` is
/// unlimited. Lowering the hard limit below an omitted soft one lowers the
/// soft one with it. The read and the write are separate calls, so a limit
/// another process changes in between is overwritten with the old value of
/// the bound the caller left omitted.
fn set_rlimit(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let span = args.span();
    let (_, resource) = resource_arg(args, 0)?;
    let pid = target(args, 3, "process.set_rlimit")?;
    let mut limit = prlimit(pid, resource, None)
        .map_err(|error| host_error("process-set-rlimit", error, span))?;
    let apply = |slot: Slot<i64>, target: &mut Option<u64>| match slot {
        Slot::Omitted => Ok(()),
        Slot::Null => {
            *target = None;
            Ok(())
        }
        Slot::Value(value) => u64::try_from(value)
            .map(|value| *target = Some(value))
            .map_err(|_| invalid("invalid-argument", "limit cannot be negative", span)),
    };
    let soft = args.slot(1)?;
    let hard = args.slot(2)?;
    apply(soft, &mut limit.soft)?;
    apply(hard, &mut limit.hard)?;
    if soft == Slot::Omitted
        && let Some(maximum) = limit.hard
        && limit.soft.is_none_or(|current| current > maximum)
    {
        limit.soft = Some(maximum);
    }
    match prlimit(pid, resource, Some(limit)) {
        Ok(_) => ok(Value::Unit),
        Err(error) => Err(host_error("process-set-rlimit", error, span)),
    }
}
