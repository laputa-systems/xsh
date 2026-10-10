//! CPU affinity, scheduler policy, and I/O priority of this process or of
//! another one. A pid of 0 names the calling thread, which `unix.exec` carries
//! into the new program. Every failure is a host error that carries its errno.

use super::prims::{Args, Slot, Which, host_error, invalid, key, ok, user_id, which};
use rustix::process::{Pid, getpriority_process};
use crate::modules::RuntimeOp;
use crate::runtime::value::{RecordMap, RuntimeError, Value};
use crate::source::Span;
use std::io;

/// Scheduler policies by the name the typed API uses, in kernel number order.
const POLICIES: &[(&str, i32)] = &[
    ("other", 0),
    ("fifo", 1),
    ("rr", 2),
    ("batch", 3),
    ("idle", 5),
    ("deadline", 6),
    ("ext", 7),
];

const IOPRIO_CLASSES: &[&str] = &["none", "realtime", "best-effort", "idle"];

/// The kernel allocates the low 13 bits of an I/O priority value to the level
/// and the bits above them to the class.
const IOPRIO_CLASS_SHIFT: u32 = 13;
const IOPRIO_LEVEL_LIMIT: i64 = 1 << IOPRIO_CLASS_SHIFT;

/// Affinity masks beyond this many CPUs are far past any kernel's
/// configuration limit; refusing them keeps a typo from allocating gigabytes.
const AFFINITY_CPU_LIMIT: i64 = 1 << 20;

/// Whether `op` is one of the primitives in this file.
pub(super) fn handles(op: RuntimeOp) -> bool {
    matches!(
        op,
        RuntimeOp::ProcessAffinity
            | RuntimeOp::ProcessSetAffinity
            | RuntimeOp::ProcessScheduler
            | RuntimeOp::ProcessSetScheduler
            | RuntimeOp::ProcessSchedulerPriorities
            | RuntimeOp::ProcessIoPriority
            | RuntimeOp::ProcessSetIoPriority
    )
}

pub(super) fn call(op: RuntimeOp, args: &Args<'_>) -> Result<Value, RuntimeError> {
    match op {
        RuntimeOp::ProcessAffinity => affinity(args),
        RuntimeOp::ProcessSetAffinity => set_affinity(args),
        RuntimeOp::ProcessScheduler => scheduler(args),
        RuntimeOp::ProcessSetScheduler => set_scheduler(args),
        RuntimeOp::ProcessSchedulerPriorities => scheduler_priorities(args),
        RuntimeOp::ProcessIoPriority => io_priority(args),
        RuntimeOp::ProcessSetIoPriority => set_io_priority(args),
        _ => unreachable!("scheduling primitive expected"),
    }
}

/// The raw pid of a thread-or-process argument; omitted and 0 name the caller.
/// A negative value is passed on, since each system call answers it its own
/// way (ESRCH or EINVAL) and callers report that answer.
fn target(args: &Args<'_>, index: usize, operation: &str) -> Result<i32, RuntimeError> {
    let pid = args.int_or(index, 0)?;
    i32::try_from(pid).map_err(|_| {
        invalid(
            "pid-range",
            format!("{operation}: pid is out of range"),
            args.span(),
        )
    })
}

/// `struct sched_attr` at the layout that carries the deadline and
/// utilisation-clamp fields.
#[repr(C)]
#[derive(Clone, Copy, Default)]
#[cfg_attr(not(any(target_os = "linux", target_os = "android")), allow(dead_code))]
struct SchedAttr {
    size: u32,
    policy: u32,
    flags: u64,
    nice: i32,
    priority: u32,
    runtime: u64,
    deadline: u64,
    period: u64,
    util_min: u32,
    util_max: u32,
}

const FLAG_RESET_ON_FORK: u64 = 0x01;
const POLICY_RESET_ON_FORK: i32 = 0x4000_0000;
const POLICY_DEADLINE: i32 = 6;

/// The Linux system calls; the structs they exchange are defined above.
#[cfg(any(target_os = "linux", target_os = "android"))]
mod host {
    use super::SchedAttr;
    use std::io;

    fn check(status: libc::c_long) -> io::Result<libc::c_long> {
        if status < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(status)
        }
    }

    pub(super) fn affinity(pid: i32) -> io::Result<Vec<libc::c_ulong>> {
        let mut words = 16usize;
        loop {
            let mut mask = vec![0 as libc::c_ulong; words];
            let bytes = words * std::mem::size_of::<libc::c_ulong>();
            // SAFETY: `mask` holds `bytes` writable bytes.
            let status = unsafe {
                libc::syscall(
                    libc::SYS_sched_getaffinity,
                    libc::c_long::from(pid),
                    bytes as libc::c_long,
                    mask.as_mut_ptr(),
                )
            };
            match check(status) {
                Ok(_) => return Ok(mask),
                // The kernel rejects a buffer smaller than its CPU mask.
                Err(error) if error.raw_os_error() == Some(libc::EINVAL) && words < 1 << 14 => {
                    words *= 2;
                }
                Err(error) => return Err(error),
            }
        }
    }

    pub(super) fn set_affinity(pid: i32, mask: &[libc::c_ulong]) -> io::Result<()> {
        let bytes = std::mem::size_of_val(mask);
        // SAFETY: `mask` holds `bytes` readable bytes.
        let status = unsafe {
            libc::syscall(
                libc::SYS_sched_setaffinity,
                libc::c_long::from(pid),
                bytes as libc::c_long,
                mask.as_ptr(),
            )
        };
        check(status).map(drop)
    }

    pub(super) fn get_attr(pid: i32) -> io::Result<SchedAttr> {
        let mut attr = SchedAttr::default();
        // SAFETY: `attr` is a live struct of the size passed.
        let status = unsafe {
            libc::syscall(
                libc::SYS_sched_getattr,
                libc::c_long::from(pid),
                std::ptr::from_mut(&mut attr),
                std::mem::size_of::<SchedAttr>() as libc::c_long,
                0 as libc::c_long,
            )
        };
        check(status).map(|_| attr)
    }

    pub(super) fn set_attr(pid: i32, attr: &SchedAttr) -> io::Result<()> {
        // SAFETY: `attr` is a live struct of the size it declares.
        let status = unsafe {
            libc::syscall(
                libc::SYS_sched_setattr,
                libc::c_long::from(pid),
                std::ptr::from_ref(attr),
                0 as libc::c_long,
            )
        };
        check(status).map(drop)
    }

    /// The raw call, because musl's `sched_setscheduler` is a stub that
    /// always fails.
    pub(super) fn set_scheduler(pid: i32, policy: i32, priority: i32) -> io::Result<()> {
        // The kernel's struct sched_param is the priority alone.
        // SAFETY: `priority` outlives the call and is the whole struct.
        let status = unsafe {
            libc::syscall(
                libc::SYS_sched_setscheduler,
                libc::c_long::from(pid),
                libc::c_long::from(policy),
                std::ptr::from_ref(&priority),
            )
        };
        check(status).map(drop)
    }

    pub(super) fn priority_range(policy: i32) -> io::Result<(i32, i32)> {
        // SAFETY: the calls take and return plain integers.
        let (min, max) = unsafe {
            (
                libc::sched_get_priority_min(policy),
                libc::sched_get_priority_max(policy),
            )
        };
        if min < 0 || max < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok((min, max))
    }

    pub(super) fn ioprio_get(which: i32, who: i32) -> io::Result<i32> {
        // SAFETY: the call takes plain integers.
        let status = unsafe {
            libc::syscall(
                libc::SYS_ioprio_get,
                libc::c_long::from(which),
                libc::c_long::from(who),
            )
        };
        check(status).map(|value| value as i32)
    }

    pub(super) fn ioprio_set(which: i32, who: i32, value: i32) -> io::Result<()> {
        // SAFETY: the call takes plain integers.
        let status = unsafe {
            libc::syscall(
                libc::SYS_ioprio_set,
                libc::c_long::from(which),
                libc::c_long::from(who),
                libc::c_long::from(value),
            )
        };
        check(status).map(drop)
    }
}

fn policy_number(name: &str, span: Span) -> Result<i32, RuntimeError> {
    POLICIES
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map(|(_, number)| *number)
        .ok_or_else(|| {
            invalid(
                "invalid-argument",
                format!(
                    "policy must be one of {}, found `{name}`",
                    POLICIES
                        .iter()
                        .map(|(name, _)| *name)
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                span,
            )
        })
}

fn policy_name(number: i32) -> &'static str {
    POLICIES
        .iter()
        .find(|(_, candidate)| *candidate == number)
        .map_or("unknown", |(name, _)| name)
}

/// The sorted CPU numbers of a kernel affinity mask.
fn mask_cpus(mask: &[libc::c_ulong]) -> Vec<Value> {
    let bits = libc::c_ulong::BITS as usize;
    mask.iter()
        .enumerate()
        .flat_map(|(word_index, word)| {
            (0..bits)
                .filter(move |bit| word >> bit & 1 == 1)
                .map(move |bit| Value::Int((word_index * bits + bit) as i64))
        })
        .collect()
}

fn affinity(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let pid = target(args, 0, "process.affinity")?;
    match host::affinity(pid) {
        Ok(mask) => ok(Value::List(mask_cpus(&mask))),
        Err(error) => Err(host_error("process-affinity", error, args.span())),
    }
}

fn set_affinity(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let span = args.span();
    let pid = target(args, 0, "process.set_affinity")?;
    let cpus = args.ints(1)?;
    let bits = libc::c_ulong::BITS as usize;
    let mut words = vec![0 as libc::c_ulong; 1];
    for cpu in cpus {
        if !(0..AFFINITY_CPU_LIMIT).contains(&cpu) {
            return Err(invalid(
                "invalid-argument",
                format!("cpu {cpu} is outside 0..{AFFINITY_CPU_LIMIT}"),
                span,
            ));
        }
        let cpu = cpu as usize;
        if cpu / bits >= words.len() {
            words.resize(cpu / bits + 1, 0);
        }
        words[cpu / bits] |= 1 << (cpu % bits);
    }
    match host::set_affinity(pid, &words) {
        Ok(()) => ok(Value::Unit),
        Err(error) => Err(host_error("process-set-affinity", error, span)),
    }
}

fn scheduler(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let pid = target(args, 0, "process.scheduler")?;
    let attr = host::get_attr(pid)
        .map_err(|error| host_error("process-scheduler", error, args.span()))?;
    let nanoseconds = |value: u64| Value::Int(i64::try_from(value).unwrap_or(i64::MAX));
    ok(Value::Record(RecordMap::from([
        (key("policy"), Value::Str(policy_name(attr.policy as i32).into())),
        (key("priority"), Value::Int(i64::from(attr.priority))),
        (
            key("reset_on_fork"),
            Value::Bool(attr.flags & FLAG_RESET_ON_FORK != 0),
        ),
        (key("runtime_ns"), nanoseconds(attr.runtime)),
        (key("deadline_ns"), nanoseconds(attr.deadline)),
        (key("period_ns"), nanoseconds(attr.period)),
    ])))
}

/// A parameter in nanoseconds that was left out, or `null`, is `None`.
fn nanoseconds(args: &Args<'_>, index: usize, span: Span) -> Result<Option<u64>, RuntimeError> {
    match args.slot(index)? {
        Slot::Omitted | Slot::Null => Ok(None),
        Slot::Value(value) => u64::try_from(value)
            .map(Some)
            .map_err(|_| invalid("invalid-argument", "a time parameter cannot be negative", span)),
    }
}

/// Changes the policy of `pid`. The deadline policy and any time parameter go
/// through `sched_setattr`, which is the only call that carries them; a plain
/// policy change keeps the niceness the way `sched_setscheduler` does.
fn set_scheduler(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let span = args.span();
    let pid = target(args, 0, "process.set_scheduler")?;
    let policy = policy_number(&args.str(1)?, span)?;
    let priority = i32::try_from(args.int_or(2, 0)?)
        .map_err(|_| invalid("invalid-argument", "priority is out of range", span))?;
    let reset_on_fork = args.bool_or(3, false)?;
    let runtime = nanoseconds(args, 4, span)?;
    let deadline = nanoseconds(args, 5, span)?;
    let period = nanoseconds(args, 6, span)?;
    let timed = runtime.is_some() || deadline.is_some() || period.is_some();
    let result = if timed || policy == POLICY_DEADLINE {
        let nice = getpriority_process(Pid::from_raw(pid))
            .map_err(|error| host_error("process-set-scheduler", error, span))?;
        host::set_attr(
            pid,
            &SchedAttr {
                size: std::mem::size_of::<SchedAttr>() as u32,
                policy: policy as u32,
                flags: if reset_on_fork {
                    FLAG_RESET_ON_FORK
                } else {
                    0
                },
                nice,
                priority: priority as u32,
                runtime: runtime.unwrap_or(0),
                deadline: deadline.unwrap_or(0),
                period: period.unwrap_or(0),
                ..SchedAttr::default()
            },
        )
    } else {
        let flag = if reset_on_fork {
            POLICY_RESET_ON_FORK
        } else {
            0
        };
        host::set_scheduler(pid, policy | flag, priority)
    };
    match result {
        Ok(()) => ok(Value::Unit),
        Err(error) => Err(host_error("process-set-scheduler", error, span)),
    }
}

fn scheduler_priorities(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let span = args.span();
    let policy = policy_number(&args.str(0)?, span)?;
    match host::priority_range(policy) {
        Ok((min, max)) => ok(Value::Record(RecordMap::from([
            (key("min"), Value::Int(i64::from(min))),
            (key("max"), Value::Int(i64::from(max))),
        ]))),
        Err(error) => Err(host_error("process-scheduler-priorities", error, span)),
    }
}

/// `ioprio_get(2)` and `ioprio_set(2)` name their target by a kind and an id,
/// where 0 is the caller's process, group, or real user.
fn io_target(
    args: &Args<'_>,
    which_index: usize,
    operation: &str,
) -> Result<(i32, i32), RuntimeError> {
    match which(args, which_index)? {
        Which::Process => Ok((1, target(args, 0, operation)?)),
        Which::Group => Ok((2, target(args, 0, operation)?)),
        Which::User => {
            let uid = user_id(args.int_or(0, 0)?, args.span())?;
            Ok((3, uid.as_raw() as i32))
        }
    }
}

fn io_priority(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let (which, who) = io_target(args, 1, "process.io_priority")?;
    let value = host::ioprio_get(which, who)
        .map_err(|error| host_error("process-io-priority", error, args.span()))?;
    let class = (value >> IOPRIO_CLASS_SHIFT) as usize;
    ok(Value::Record(RecordMap::from([
        (
            key("class"),
            Value::Str(IOPRIO_CLASSES.get(class).copied().unwrap_or("unknown").into()),
        ),
        (key("level"), Value::Int(i64::from(value & (IOPRIO_LEVEL_LIMIT as i32 - 1)))),
    ])))
}

fn set_io_priority(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let span = args.span();
    let (which, who) = io_target(args, 3, "process.set_io_priority")?;
    let class_name = args.str(1)?;
    let Some(class) = IOPRIO_CLASSES.iter().position(|name| *name == class_name) else {
        return Err(invalid(
            "invalid-argument",
            format!(
                "class must be one of {}, found `{class_name}`",
                IOPRIO_CLASSES.join(", ")
            ),
            span,
        ));
    };
    let level = args.int_or(2, 0)?;
    // A level the 13-bit field cannot hold would spill into the class, so it
    // is refused the way the kernel refuses an invalid priority.
    if !(0..IOPRIO_LEVEL_LIMIT).contains(&level) {
        return Err(host_error(
            "process-set-io-priority",
            io::Error::from_raw_os_error(libc::EINVAL),
            span,
        ));
    }
    let value = ((class as i32) << IOPRIO_CLASS_SHIFT) | level as i32;
    match host::ioprio_set(which, who, value) {
        Ok(()) => ok(Value::Unit),
        Err(error) => Err(host_error("process-set-io-priority", error, span)),
    }
}

/// Hosts without these Linux interfaces report ENOSYS from each call.
#[cfg(not(any(target_os = "linux", target_os = "android")))]
mod host {
    use super::SchedAttr;
    use std::io;

    fn unsupported<T>() -> io::Result<T> {
        Err(io::Error::from_raw_os_error(libc::ENOSYS))
    }

    pub(super) fn affinity(_pid: i32) -> io::Result<Vec<libc::c_ulong>> {
        unsupported()
    }

    pub(super) fn set_affinity(_pid: i32, _mask: &[libc::c_ulong]) -> io::Result<()> {
        unsupported()
    }

    pub(super) fn get_attr(_pid: i32) -> io::Result<SchedAttr> {
        unsupported()
    }

    pub(super) fn set_attr(_pid: i32, _attr: &SchedAttr) -> io::Result<()> {
        unsupported()
    }

    pub(super) fn set_scheduler(_pid: i32, _policy: i32, _priority: i32) -> io::Result<()> {
        unsupported()
    }

    pub(super) fn priority_range(_policy: i32) -> io::Result<(i32, i32)> {
        unsupported()
    }

    pub(super) fn ioprio_get(_which: i32, _who: i32) -> io::Result<i32> {
        unsupported()
    }

    pub(super) fn ioprio_set(_which: i32, _who: i32, _value: i32) -> io::Result<()> {
        unsupported()
    }
}
