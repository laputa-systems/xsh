use super::common::{cstring_text, error_value, io_error, ok_unit};
use super::mount::read_mounts;
use super::{LinuxRtcTime, RTC_RD_TIME, RTC_SET_TIME};
use crate::modules::linux::str_value;
use crate::runtime::value::{LiveStream, RuntimeError, StreamValue, Value};
use crate::source::Span;
use rustix::mount::{UnmountFlags, unmount};
use rustix::{fs as rfs, process as rprocess};
use std::fs::{self, File};
use std::io;
use std::num::NonZeroI32;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub(crate) fn kill_all(signal: i32, except_pid1: bool, span: Span) -> Result<Value, RuntimeError> {
    let self_pid = rprocess::getpid();
    let current_sid = match rprocess::getsid(None) {
        Ok(sid) => sid,
        Err(error) => return Ok(io_error("linux-kill-all", io::Error::from(error), span)),
    };
    let signal = match signal_from_i32(signal) {
        Some(signal) => signal,
        None => return Ok(error_value("invalid-signal", "invalid signal", span)),
    };
    let entries = match fs::read_dir("/proc") {
        Ok(entries) => entries,
        Err(error) => return Ok(io_error("linux-kill-all", error, span)),
    };
    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<i32>().ok())
            .and_then(rprocess::Pid::from_raw)
        else {
            continue;
        };
        if pid == self_pid || (except_pid1 && pid == rprocess::Pid::INIT) {
            continue;
        }
        let process_sid = match rprocess::getsid(Some(pid)) {
            Ok(sid) => sid,
            Err(_) => continue,
        };
        if process_sid == current_sid {
            continue;
        }
        if let Err(error) = rprocess::kill_process(pid, signal) {
            let error = io::Error::from(error);
            if !matches!(error.raw_os_error(), Some(libc::ESRCH | libc::EPERM)) {
                return Ok(io_error("linux-kill-all", error, span));
            }
        }
    }
    Ok(ok_unit())
}

pub(crate) fn halt(span: Span) -> Result<Value, RuntimeError> {
    reboot(rustix::system::RebootCommand::Halt, "linux-halt", span)
}

pub(crate) fn poweroff(span: Span) -> Result<Value, RuntimeError> {
    reboot(
        rustix::system::RebootCommand::PowerOff,
        "linux-poweroff",
        span,
    )
}

pub(crate) fn reboot_system(span: Span) -> Result<Value, RuntimeError> {
    reboot(rustix::system::RebootCommand::Restart, "linux-reboot", span)
}

pub(crate) fn chroot(path: &Path, span: Span) -> Result<Value, RuntimeError> {
    match rprocess::chroot(path) {
        Ok(()) => Ok(ok_unit()),
        Err(e) => Ok(io_error("linux-chroot", io::Error::from(e), span)),
    }
}

pub(crate) fn mknod(
    path: &Path,
    kind: &str,
    major: i64,
    minor: i64,
    span: Span,
) -> Result<Value, RuntimeError> {
    if !(0..=u32::MAX as i64).contains(&major) || !(0..=u32::MAX as i64).contains(&minor) {
        return Ok(error_value(
            "linux-mknod",
            "major and minor must be between 0 and 4294967295",
            span,
        ));
    }
    let (file_type, dev) = match kind {
        "block" => (
            rfs::FileType::BlockDevice,
            rfs::makedev(major as u32, minor as u32),
        ),
        "char" => (
            rfs::FileType::CharacterDevice,
            rfs::makedev(major as u32, minor as u32),
        ),
        "fifo" => (rfs::FileType::Fifo, 0),
        _ => {
            return Ok(error_value(
                "linux-mknod",
                "kind must be `block`, `char`, or `fifo`",
                span,
            ));
        }
    };
    match rfs::mknodat(
        rfs::CWD,
        path,
        file_type,
        rfs::Mode::from_raw_mode(0o666),
        dev,
    ) {
        Ok(()) => Ok(ok_unit()),
        Err(e) => Ok(io_error("linux-mknod", io::Error::from(e), span)),
    }
}

pub(crate) fn insmod(path: &Path, params: &str, span: Span) -> Result<Value, RuntimeError> {
    let params = cstring_text(params, "linux-insmod", span)?;
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) => return Ok(io_error("linux-insmod", error, span)),
    };
    let rc = unsafe { libc::syscall(libc::SYS_finit_module, file.as_raw_fd(), params.as_ptr(), 0) };
    if rc == 0 {
        return Ok(ok_unit());
    }
    let finit_error = io::Error::last_os_error();
    if !matches!(
        finit_error.raw_os_error(),
        Some(libc::ENOSYS | libc::EINVAL)
    ) {
        return Ok(io_error("linux-insmod", finit_error, span));
    }
    let image = match fs::read(path) {
        Ok(image) => image,
        Err(error) => return Ok(io_error("linux-insmod", error, span)),
    };
    let rc = unsafe {
        libc::syscall(
            libc::SYS_init_module,
            image.as_ptr(),
            image.len(),
            params.as_ptr(),
        )
    };
    if rc == 0 {
        Ok(ok_unit())
    } else {
        Ok(io_error("linux-insmod", io::Error::last_os_error(), span))
    }
}

pub(crate) fn rmmod(name: &str, force: bool, span: Span) -> Result<Value, RuntimeError> {
    let name = cstring_text(name, "linux-rmmod", span)?;
    let mut flags = libc::O_NONBLOCK;
    if force {
        flags |= libc::O_TRUNC;
    }
    let rc = unsafe { libc::syscall(libc::SYS_delete_module, name.as_ptr(), flags) };
    if rc == 0 {
        Ok(ok_unit())
    } else {
        Ok(io_error("linux-rmmod", io::Error::last_os_error(), span))
    }
}

pub(crate) fn pivot_root(
    new_root: &Path,
    put_old: &Path,
    span: Span,
) -> Result<Value, RuntimeError> {
    match rprocess::pivot_root(new_root, put_old) {
        Ok(()) => Ok(ok_unit()),
        Err(error) => Ok(io_error("linux-pivot-root", io::Error::from(error), span)),
    }
}

fn signal_from_i32(signal: i32) -> Option<rprocess::Signal> {
    NonZeroI32::new(signal)
        .map(|signal| unsafe { rprocess::Signal::from_raw_nonzero_unchecked(signal) })
}

pub(crate) fn switch_root(new_root: &Path, init: &Path, span: Span) -> Result<Value, RuntimeError> {
    let mounts = match read_mounts("/proc/mounts") {
        Ok(mounts) => mounts,
        Err(error) => return Ok(io_error("linux-switch-root", error, span)),
    };
    for mount in mounts.into_iter().rev() {
        if mount.target == "/" || Path::new(&mount.target).starts_with(new_root) {
            continue;
        }
        let _ = unmount(mount.target.as_str(), UnmountFlags::DETACH);
    }
    if let Err(e) = rprocess::chroot(new_root).and_then(|()| rprocess::chdir(".")) {
        return Ok(io_error("linux-switch-root", io::Error::from(e), span));
    }
    let error = std::process::Command::new(init).exec();
    Ok(io_error("linux-switch-root", error, span))
}

pub(crate) fn hwclock(span: Span) -> Result<Value, RuntimeError> {
    let fd = match open_rtc(false) {
        Ok(fd) => fd,
        Err(error) => return Ok(io_error("linux-hwclock", error, span)),
    };
    let mut rtc = LinuxRtcTime::default();
    let rc = unsafe { libc::ioctl(fd.as_raw_fd(), RTC_RD_TIME as _, &mut rtc) };
    let error = io::Error::last_os_error();
    if rc != 0 {
        return Ok(io_error("linux-hwclock", error, span));
    }
    match rtc_to_epoch_ms(rtc) {
        Ok(epoch_ms) => Ok(Value::ok(Value::Int(epoch_ms))),
        Err(error) => Ok(Value::err(Value::Error(Box::new(error.with_span(span))))),
    }
}

pub(crate) fn set_hwclock(epoch_ms: i64, span: Span) -> Result<Value, RuntimeError> {
    let fd = match open_rtc(true) {
        Ok(fd) => fd,
        Err(error) => return Ok(io_error("linux-hwclock", error, span)),
    };
    let rtc = match epoch_ms_to_rtc(epoch_ms) {
        Ok(rtc) => rtc,
        Err(error) => {
            return Ok(Value::err(Value::Error(Box::new(error.with_span(span)))));
        }
    };
    let rc = unsafe { libc::ioctl(fd.as_raw_fd(), RTC_SET_TIME as _, &rtc) };
    let error = io::Error::last_os_error();
    if rc == 0 {
        Ok(ok_unit())
    } else {
        Ok(io_error("linux-hwclock", error, span))
    }
}
pub(crate) fn set_system_clock(epoch_ms: i64, span: Span) -> Result<Value, RuntimeError> {
    if epoch_ms < 0 {
        return Ok(Value::err(Value::Error(Box::new(
            RuntimeError::new("linux-set-system-clock", "epoch_ms cannot be negative")
                .with_span(span),
        ))));
    }

    let timespec = rustix::time::Timespec {
        tv_sec: epoch_ms / 1000,
        tv_nsec: (epoch_ms % 1000) * 1_000_000,
    };
    match rustix::time::clock_settime(rustix::time::ClockId::Realtime, timespec) {
        Ok(()) => Ok(ok_unit()),
        Err(e) => Ok(io_error("linux-set-system-clock", io::Error::from(e), span)),
    }
}

pub(crate) fn rfkill_list(span: Span) -> Result<Value, RuntimeError> {
    match fs::read_dir("/sys/class/rfkill") {
        Ok(entries) => {
            let mut paths = Vec::new();
            for entry in entries {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(error) => return Ok(io_error("linux-rfkill", error, span)),
                };
                let name = entry.file_name().to_string_lossy().into_owned();
                let Some(id) = name
                    .strip_prefix("rfkill")
                    .and_then(|value| value.parse::<i64>().ok())
                else {
                    continue;
                };
                paths.push((id, entry.path()));
            }
            paths.sort_unstable_by_key(|(id, _)| *id);
            Ok(Value::ok(Value::stream(StreamValue::from_live(
                "linux.rfkill_list",
                RfkillStream {
                    entries: paths.into_iter(),
                },
            ))))
        }
        Err(error) => Ok(io_error("linux-rfkill", error, span)),
    }
}

struct RfkillStream {
    entries: std::vec::IntoIter<(i64, PathBuf)>,
}

impl LiveStream for RfkillStream {
    fn next(&mut self, span: Span) -> Result<Option<Value>, RuntimeError> {
        let Some((id, path)) = self.entries.next() else {
            return Ok(None);
        };
        let record = crate::runtime::value::RecordMap::from([
            (Arc::from("id"), Value::Int(id)),
            (
                Arc::from("name"),
                str_value(read_trimmed(path.join("name")).map_err(|error| {
                    RuntimeError::new("linux-rfkill", error.to_string()).with_span(span)
                })?),
            ),
            (
                Arc::from("type"),
                str_value(read_trimmed(path.join("type")).map_err(|error| {
                    RuntimeError::new("linux-rfkill", error.to_string()).with_span(span)
                })?),
            ),
            (
                Arc::from("soft_blocked"),
                Value::Bool(
                    read_trimmed(path.join("soft")).map_err(|error| {
                        RuntimeError::new("linux-rfkill", error.to_string()).with_span(span)
                    })? == "1",
                ),
            ),
            (
                Arc::from("hard_blocked"),
                Value::Bool(
                    read_trimmed(path.join("hard")).map_err(|error| {
                        RuntimeError::new("linux-rfkill", error.to_string()).with_span(span)
                    })? == "1",
                ),
            ),
        ]);
        Ok(Some(Value::Record(record)))
    }
}

pub(crate) fn rfkill_set(id: i64, blocked: bool, span: Span) -> Result<Value, RuntimeError> {
    if id < 0 {
        return Ok(error_value("linux-rfkill", "id cannot be negative", span));
    }
    let path = Path::new("/sys/class/rfkill")
        .join(format!("rfkill{id}"))
        .join("soft");
    match fs::write(path, if blocked { "1" } else { "0" }) {
        Ok(()) => Ok(ok_unit()),
        Err(error) => Ok(io_error("linux-rfkill", error, span)),
    }
}

fn reboot(
    command: rustix::system::RebootCommand,
    kind: &str,
    span: Span,
) -> Result<Value, RuntimeError> {
    rfs::sync();
    match rustix::system::reboot(command) {
        Ok(()) => Ok(ok_unit()),
        Err(e) => Ok(io_error(kind, io::Error::from(e), span)),
    }
}

fn open_rtc(write: bool) -> io::Result<OwnedFd> {
    let candidates = ["/dev/rtc", "/dev/rtc0", "/dev/misc/rtc"];
    let mut last_error = None;
    let flags = if write {
        rfs::OFlags::RDWR
    } else {
        rfs::OFlags::RDONLY
    } | rfs::OFlags::CLOEXEC;
    for candidate in candidates {
        match rfs::open(candidate, flags, rfs::Mode::empty()) {
            Ok(fd) => return Ok(fd),
            Err(error) => last_error = Some(io::Error::from(error)),
        }
    }
    Err(last_error.unwrap_or_else(|| io::Error::from_raw_os_error(libc::ENOENT)))
}

fn rtc_to_epoch_ms(rtc: LinuxRtcTime) -> Result<i64, RuntimeError> {
    if !(0..=59).contains(&rtc.tm_sec)
        || !(0..=59).contains(&rtc.tm_min)
        || !(0..=23).contains(&rtc.tm_hour)
        || !(1..=31).contains(&rtc.tm_mday)
        || !(0..=11).contains(&rtc.tm_mon)
    {
        return Err(RuntimeError::new(
            "linux-hwclock",
            "RTC returned an invalid time",
        ));
    }
    let days = days_from_civil(
        i64::from(rtc.tm_year) + 1900,
        i64::from(rtc.tm_mon) + 1,
        i64::from(rtc.tm_mday),
    );
    let seconds = days
        .saturating_mul(86_400)
        .saturating_add(i64::from(rtc.tm_hour).saturating_mul(3_600))
        .saturating_add(i64::from(rtc.tm_min).saturating_mul(60))
        .saturating_add(i64::from(rtc.tm_sec));
    Ok(seconds.saturating_mul(1000))
}

fn days_from_civil(mut year: i64, month: i64, day: i64) -> i64 {
    year -= i64::from(month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_index = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[allow(deprecated)]
fn epoch_ms_to_rtc(epoch_ms: i64) -> Result<LinuxRtcTime, RuntimeError> {
    let seconds = (epoch_ms / 1000) as libc::time_t;
    let mut tm = libc::tm {
        tm_sec: 0,
        tm_min: 0,
        tm_hour: 0,
        tm_mday: 0,
        tm_mon: 0,
        tm_year: 0,
        tm_wday: 0,
        tm_yday: 0,
        tm_isdst: 0,
        tm_gmtoff: 0,
        tm_zone: std::ptr::null(),
    };
    let rc = unsafe { libc::gmtime_r(&seconds, &mut tm) };
    if rc.is_null() {
        return Err(RuntimeError::new(
            "linux-hwclock",
            "converting epoch time failed",
        ));
    }
    Ok(LinuxRtcTime {
        tm_sec: tm.tm_sec,
        tm_min: tm.tm_min,
        tm_hour: tm.tm_hour,
        tm_mday: tm.tm_mday,
        tm_mon: tm.tm_mon,
        tm_year: tm.tm_year,
        tm_wday: tm.tm_wday,
        tm_yday: tm.tm_yday,
        tm_isdst: 0,
    })
}

fn read_trimmed(path: PathBuf) -> io::Result<String> {
    fs::read_to_string(path).map(|value| value.trim().to_string())
}
