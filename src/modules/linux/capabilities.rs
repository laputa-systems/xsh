//! Privilege state of the calling process: capability sets, securebits,
//! no_new_privs, the parent-death signal and the ptracer.
//!
//! Capability sets, securebits and the parent-death signal are per-thread
//! kernel state, so these calls must run on the thread that later calls
//! exec; the evaluator is single-threaded for ordinary scripts. Every failure
//! is a host error that carries its errno.

use crate::modules::RuntimeOp;
use crate::modules::process::{Args, host_error};
use crate::runtime::value::{RuntimeError, Value};

pub(crate) fn handles(op: RuntimeOp) -> bool {
    matches!(
        op,
        RuntimeOp::LinuxPrivileges
            | RuntimeOp::LinuxSetCapabilities
            | RuntimeOp::LinuxDropBoundingCapability
            | RuntimeOp::LinuxSetAmbientCapability
            | RuntimeOp::LinuxSetSecurebits
            | RuntimeOp::LinuxSetNoNewPrivs
            | RuntimeOp::LinuxSetKeepCapabilities
            | RuntimeOp::LinuxSetParentDeathSignal
            | RuntimeOp::LinuxSetPtracer
    )
}

#[cfg(target_os = "linux")]
pub(crate) fn call(op: RuntimeOp, args: &Args<'_>) -> Result<Value, RuntimeError> {
    native::call(op, args)
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn call(op: RuntimeOp, args: &Args<'_>) -> Result<Value, RuntimeError> {
    let _ = op;
    Err(host_error(
        "linux-privileges",
        std::io::Error::from_raw_os_error(libc::ENOSYS),
        args.span(),
    ))
}

#[cfg(target_os = "linux")]
mod native {
    use super::*;
    use crate::runtime::value::RecordMap;
    use crate::source::Span;
    use std::io;
    use std::sync::Arc;

    fn invalid(message: impl Into<String>, span: Span) -> RuntimeError {
        RuntimeError::new("invalid-argument", message.into()).with_span(span)
    }

    fn key(name: &str) -> Arc<str> {
        Arc::from(name)
    }

    const PR_SET_PDEATHSIG: libc::c_int = 1;
    const PR_GET_PDEATHSIG: libc::c_int = 2;
    const PR_SET_KEEPCAPS: libc::c_int = 8;
    const PR_CAPBSET_READ: libc::c_int = 23;
    const PR_CAPBSET_DROP: libc::c_int = 24;
    const PR_GET_SECUREBITS: libc::c_int = 27;
    const PR_SET_SECUREBITS: libc::c_int = 28;
    const PR_SET_NO_NEW_PRIVS: libc::c_int = 38;
    const PR_GET_NO_NEW_PRIVS: libc::c_int = 39;
    const PR_CAP_AMBIENT: libc::c_int = 47;
    const PR_CAP_AMBIENT_IS_SET: libc::c_ulong = 1;
    const PR_CAP_AMBIENT_RAISE: libc::c_ulong = 2;
    const PR_CAP_AMBIENT_LOWER: libc::c_ulong = 3;
    const PR_SET_PTRACER: libc::c_int = 0x5961_6d61;
    // PR_SET_PTRACER_ANY is the all-ones unsigned long.
    const PR_SET_PTRACER_ANY: libc::c_ulong = libc::c_ulong::MAX;

    const LINUX_CAPABILITY_VERSION_3: u32 = 0x2008_0522;
    // Capability bits live in a 64-bit mask in the version-3 ABI.
    const CAPABILITY_BITS: i64 = 64;

    /// Securebit names in bit order. `keep_caps` is the PR_SET_KEEPCAPS flag,
    /// which the kernel clears on exec.
    const SECUREBITS: [(&str, u32); 8] = [
        ("noroot", 1 << 0),
        ("noroot_locked", 1 << 1),
        ("no_setuid_fixup", 1 << 2),
        ("no_setuid_fixup_locked", 1 << 3),
        ("keep_caps", 1 << 4),
        ("keep_caps_locked", 1 << 5),
        ("no_cap_ambient_raise", 1 << 6),
        ("no_cap_ambient_raise_locked", 1 << 7),
    ];

    #[repr(C)]
    struct CapHeader {
        version: u32,
        pid: i32,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct CapData {
        effective: u32,
        permitted: u32,
        inheritable: u32,
    }

    pub(super) fn call(op: RuntimeOp, args: &Args<'_>) -> Result<Value, RuntimeError> {
        let span = args.span();
        let outcome = match op {
            RuntimeOp::LinuxPrivileges => privileges(span),
            RuntimeOp::LinuxSetCapabilities => set_capabilities(args),
            RuntimeOp::LinuxDropBoundingCapability => drop_bounding(args),
            RuntimeOp::LinuxSetAmbientCapability => set_ambient(args),
            RuntimeOp::LinuxSetSecurebits => set_securebits(args),
            RuntimeOp::LinuxSetNoNewPrivs => {
                prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0)
                    .map(|_| Value::Unit)
                    .map_err(|error| host_error("linux-set-no-new-privs", error, span))
            }
            RuntimeOp::LinuxSetKeepCapabilities => {
                let enabled = args.bool_or(0, false)?;
                prctl(PR_SET_KEEPCAPS, libc::c_ulong::from(enabled), 0, 0, 0)
                    .map(|_| Value::Unit)
                    .map_err(|error| host_error("linux-set-keep-capabilities", error, span))
            }
            RuntimeOp::LinuxSetParentDeathSignal => set_parent_death_signal(args),
            RuntimeOp::LinuxSetPtracer => set_ptracer(args),
            _ => unreachable!("capability primitive expected"),
        };
        outcome.map(Value::ok)
    }

    fn prctl(
        option: libc::c_int,
        a2: libc::c_ulong,
        a3: libc::c_ulong,
        a4: libc::c_ulong,
        a5: libc::c_ulong,
    ) -> io::Result<libc::c_int> {
        let status = unsafe { libc::prctl(option, a2, a3, a4, a5) };
        if status == -1 {
            return Err(io::Error::last_os_error());
        }
        Ok(status)
    }

    fn capability_number(value: i64, span: Span) -> Result<libc::c_ulong, RuntimeError> {
        if !(0..CAPABILITY_BITS).contains(&value) {
            return Err(invalid("capability must be between 0 and 63", span));
        }
        Ok(value as libc::c_ulong)
    }

    /// The highest capability number this kernel defines. PR_CAPBSET_READ
    /// fails with EINVAL for a number the kernel does not know.
    fn last_capability() -> io::Result<i64> {
        let mut cap: i64 = 0;
        while cap < CAPABILITY_BITS {
            match prctl(PR_CAPBSET_READ, cap as libc::c_ulong, 0, 0, 0) {
                Ok(_) => cap += 1,
                Err(error) if error.raw_os_error() == Some(libc::EINVAL) => break,
                Err(error) => return Err(error),
            }
        }
        Ok(cap - 1)
    }

    fn capget() -> io::Result<[CapData; 2]> {
        let mut header = CapHeader { version: LINUX_CAPABILITY_VERSION_3, pid: 0 };
        let mut data = [CapData::default(); 2];
        let status = unsafe { libc::syscall(libc::SYS_capget, &mut header, data.as_mut_ptr()) };
        if status == -1 {
            return Err(io::Error::last_os_error());
        }
        Ok(data)
    }

    fn capset(data: &[CapData; 2]) -> io::Result<()> {
        let header = CapHeader { version: LINUX_CAPABILITY_VERSION_3, pid: 0 };
        let status = unsafe { libc::syscall(libc::SYS_capset, &header, data.as_ptr()) };
        if status == -1 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    fn list_value(numbers: impl IntoIterator<Item = i64>) -> Value {
        Value::List(numbers.into_iter().map(Value::Int).collect::<Vec<_>>())
    }

    fn mask_list(low: u32, high: u32) -> Value {
        let mask = u64::from(high) << 32 | u64::from(low);
        list_value((0..CAPABILITY_BITS).filter(|bit| mask >> bit & 1 == 1))
    }

    fn mask_of(numbers: &[i64], span: Span) -> Result<u64, RuntimeError> {
        let mut mask = 0u64;
        for number in numbers {
            mask |= 1 << capability_number(*number, span)?;
        }
        Ok(mask)
    }

    fn privileges(span: Span) -> Result<Value, RuntimeError> {
        let fail = |kind: &str, error: io::Error| host_error(kind, error, span);
        let data = capget().map_err(|error| fail("linux-capabilities", error))?;
        let last = last_capability().map_err(|error| fail("linux-capabilities", error))?;
        let mut bounding = Vec::new();
        let mut ambient = Vec::new();
        for cap in 0..=last {
            let number = cap as libc::c_ulong;
            if prctl(PR_CAPBSET_READ, number, 0, 0, 0)
                .map_err(|error| fail("linux-capabilities", error))?
                == 1
            {
                bounding.push(cap);
            }
            if prctl(PR_CAP_AMBIENT, PR_CAP_AMBIENT_IS_SET, number, 0, 0)
                .map_err(|error| fail("linux-capabilities", error))?
                == 1
            {
                ambient.push(cap);
            }
        }
        let bits = prctl(PR_GET_SECUREBITS, 0, 0, 0, 0)
            .map_err(|error| fail("linux-securebits", error))? as u32;
        let securebits = SECUREBITS
            .iter()
            .filter(|(_, bit)| bits & bit != 0)
            .map(|(name, _)| Value::Str(Arc::from(*name)))
            .collect::<Vec<_>>();
        let no_new_privs = prctl(PR_GET_NO_NEW_PRIVS, 0, 0, 0, 0)
            .map_err(|error| fail("linux-no-new-privs", error))?
            == 1;
        let mut signal: libc::c_int = 0;
        prctl(PR_GET_PDEATHSIG, std::ptr::addr_of_mut!(signal) as libc::c_ulong, 0, 0, 0)
            .map_err(|error| fail("linux-parent-death-signal", error))?;
        Ok(Value::Record(RecordMap::from([
            (key("effective"), mask_list(data[0].effective, data[1].effective)),
            (key("permitted"), mask_list(data[0].permitted, data[1].permitted)),
            (key("inheritable"), mask_list(data[0].inheritable, data[1].inheritable)),
            (key("bounding"), list_value(bounding)),
            (key("ambient"), list_value(ambient)),
            (key("securebits"), Value::List(securebits)),
            (key("no_new_privs"), Value::Bool(no_new_privs)),
            (key("parent_death_signal"), Value::Int(i64::from(signal))),
            (key("last_capability"), Value::Int(last)),
        ])))
    }

    fn set_capabilities(args: &Args<'_>) -> Result<Value, RuntimeError> {
        let span = args.span();
        let effective = mask_of(&args.ints(0)?, span)?;
        let permitted = mask_of(&args.ints(1)?, span)?;
        let inheritable = mask_of(&args.ints(2)?, span)?;
        let word = |mask: u64, high: bool| if high { (mask >> 32) as u32 } else { mask as u32 };
        let mut data = [CapData::default(); 2];
        for (index, entry) in data.iter_mut().enumerate() {
            let high = index == 1;
            *entry = CapData {
                effective: word(effective, high),
                permitted: word(permitted, high),
                inheritable: word(inheritable, high),
            };
        }
        capset(&data).map_err(|error| host_error("linux-set-capabilities", error, span))?;
        Ok(Value::Unit)
    }

    fn drop_bounding(args: &Args<'_>) -> Result<Value, RuntimeError> {
        let span = args.span();
        let cap = capability_number(args.int(0)?, span)?;
        prctl(PR_CAPBSET_DROP, cap, 0, 0, 0)
            .map_err(|error| host_error("linux-drop-bounding-capability", error, span))?;
        Ok(Value::Unit)
    }

    fn set_ambient(args: &Args<'_>) -> Result<Value, RuntimeError> {
        let span = args.span();
        let cap = capability_number(args.int(0)?, span)?;
        let enabled = args.bool_or(1, false)?;
        let action = if enabled { PR_CAP_AMBIENT_RAISE } else { PR_CAP_AMBIENT_LOWER };
        prctl(PR_CAP_AMBIENT, action, cap, 0, 0)
            .map_err(|error| host_error("linux-set-ambient-capability", error, span))?;
        Ok(Value::Unit)
    }

    fn set_securebits(args: &Args<'_>) -> Result<Value, RuntimeError> {
        let span = args.span();
        let mut bits: u32 = 0;
        for name in args.strs(0)? {
            let Some((_, bit)) = SECUREBITS.iter().find(|(known, _)| *known == name) else {
                return Err(invalid(format!("unknown securebit `{name}`"), span));
            };
            bits |= bit;
        }
        prctl(PR_SET_SECUREBITS, libc::c_ulong::from(bits), 0, 0, 0)
            .map_err(|error| host_error("linux-set-securebits", error, span))?;
        Ok(Value::Unit)
    }

    fn set_parent_death_signal(args: &Args<'_>) -> Result<Value, RuntimeError> {
        let span = args.span();
        let signal = args.int(0)?;
        if signal < 0 {
            return Err(invalid("signal must not be negative", span));
        }
        prctl(PR_SET_PDEATHSIG, signal as libc::c_ulong, 0, 0, 0)
            .map_err(|error| host_error("linux-set-parent-death-signal", error, span))?;
        Ok(Value::Unit)
    }

    fn set_ptracer(args: &Args<'_>) -> Result<Value, RuntimeError> {
        let span = args.span();
        let pid = args.int(0)?;
        let target = match pid {
            -1 => PR_SET_PTRACER_ANY,
            0.. => pid as libc::c_ulong,
            _ => return Err(invalid("pid must be -1 (any), 0 (none), or a process ID", span)),
        };
        prctl(PR_SET_PTRACER, target, 0, 0, 0)
            .map_err(|error| host_error("linux-set-ptracer", error, span))?;
        Ok(Value::Unit)
    }
}
