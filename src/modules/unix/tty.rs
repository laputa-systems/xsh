//! Terminal primitives: tty identity, descriptor opening, pseudo-terminals,
//! window size, foreground process group, the termios flag and control
//! character tables, and the `raw`, `cooked`, `sane`, and `cbreak` modes.
//! Every failure is a host error that carries its errno.

use crate::modules::RuntimeOp;
use crate::modules::process::{Args, host_error, key, name_error, positive_pid};
use crate::runtime::value::{RecordMap, RuntimeError, Value};
use crate::source::Span;
use rustix::fd::BorrowedFd;
use rustix::process::Pid;
use rustix::termios;
use std::ffi::CStr;
use std::io;
use std::os::fd::IntoRawFd;

pub(crate) fn handles(op: RuntimeOp) -> bool {
    matches!(
        op,
        RuntimeOp::UnixIsatty
            | RuntimeOp::UnixTtyname
            | RuntimeOp::UnixControllingTty
            | RuntimeOp::UnixOpenFd
            | RuntimeOp::UnixCloseFd
            | RuntimeOp::UnixOpenPty
            | RuntimeOp::UnixTtyTable
            | RuntimeOp::UnixTtyMode
            | RuntimeOp::UnixWindowSize
            | RuntimeOp::UnixSetWindowSize
            | RuntimeOp::UnixForegroundGroup
            | RuntimeOp::UnixSetForegroundGroup
            | RuntimeOp::UnixTtySession
    )
}

pub(crate) fn call(op: RuntimeOp, args: &Args<'_>) -> Result<Value, RuntimeError> {
    match op {
        RuntimeOp::UnixIsatty => isatty(args),
        RuntimeOp::UnixTtyname => ttyname(args),
        RuntimeOp::UnixControllingTty => controlling_tty(args),
        RuntimeOp::UnixOpenFd => open_fd(args),
        RuntimeOp::UnixCloseFd => close_fd(args),
        RuntimeOp::UnixOpenPty => open_pty(args),
        RuntimeOp::UnixTtyTable => Ok(tty_table()),
        RuntimeOp::UnixTtyMode => tty_mode(args),
        RuntimeOp::UnixWindowSize => window_size(args),
        RuntimeOp::UnixSetWindowSize => set_window_size(args),
        RuntimeOp::UnixForegroundGroup => foreground_group(args),
        RuntimeOp::UnixSetForegroundGroup => set_foreground_group(args),
        RuntimeOp::UnixTtySession => tty_session(args),
        _ => unreachable!("tty primitive expected"),
    }
}

fn ok(value: Value) -> Result<Value, RuntimeError> {
    Ok(Value::ok(value))
}

fn invalid(message: impl Into<String>, span: Span) -> RuntimeError {
    RuntimeError::new("invalid-argument", message.into()).with_span(span)
}

fn fd_arg(args: &Args<'_>, index: usize, default: i64) -> Result<i32, RuntimeError> {
    let fd = args.int_or(index, default)?;
    let fd = i32::try_from(fd)
        .ok()
        .filter(|fd| *fd >= 0)
        .ok_or_else(|| invalid("fd must be a non-negative descriptor number", args.span()))?;
    crate::startup_stdio::check(fd).map_err(|error| host_error("unix-fd", error, args.span()))?;
    Ok(fd)
}

/// Borrows a descriptor the caller names by number. The kernel validates it on
/// use, so a closed or foreign number fails with EBADF instead of misbehaving.
fn borrow(fd: i32) -> BorrowedFd<'static> {
    // SAFETY: the descriptor is only passed to system calls that report EBADF.
    unsafe { BorrowedFd::borrow_raw(fd) }
}

/// A descriptor number that cannot name an open descriptor is not a terminal.
fn isatty(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let fd = args.int_or(0, 0)?;
    Ok(Value::Bool(
        i32::try_from(fd)
            .ok()
            .filter(|fd| *fd >= 0)
            .is_some_and(|fd| crate::startup_stdio::check(fd).is_ok() && termios::isatty(borrow(fd))),
    ))
}

fn name_of(name: &CStr) -> Value {
    Value::Str(name.to_string_lossy().as_ref().into())
}

fn ttyname(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let fd = borrow(fd_arg(args, 0, 0)?);
    match termios::ttyname(fd, Vec::new()) {
        Ok(name) if !name.as_bytes().is_empty() => ok(name_of(&name)),
        Ok(_) => Err(host_error(
            "unix-ttyname",
            io::Error::from_raw_os_error(libc::ENOTTY),
            args.span(),
        )),
        Err(error) => Err(host_error("unix-ttyname", error, args.span())),
    }
}

/// The name of the controlling terminal, found by opening `/dev/tty`; ENXIO
/// when the process has none.
fn controlling_tty(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let span = args.span();
    let fd = rustix::fs::open(
        "/dev/tty",
        rustix::fs::OFlags::RDWR | rustix::fs::OFlags::NOCTTY | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )
    .map_err(|error| host_error("unix-controlling-tty", error, span))?;
    match termios::ttyname(&fd, Vec::new()) {
        Ok(name) if !name.as_bytes().is_empty() => ok(name_of(&name)),
        Ok(_) => Err(host_error(
            "unix-controlling-tty",
            io::Error::from_raw_os_error(libc::ENOTTY),
            span,
        )),
        Err(error) => Err(host_error("unix-controlling-tty", error, span)),
    }
}

/// Opens a path as a bare descriptor number: it never becomes the controlling
/// terminal and is closed on exec. Non-blocking by default so that opening a
/// modem line does not wait for carrier. `flags` adds open(2) flags by name;
/// the set is closed so a script cannot ask for a flag the platform lacks.
fn open_fd(args: &Args<'_>) -> Result<Value, RuntimeError> {
    use rustix::fs::{Mode, OFlags};
    let span = args.span();
    let path = args.path(0)?;
    let mut flags = OFlags::NOCTTY | OFlags::CLOEXEC;
    flags |= if args.bool_or(1, false)? {
        OFlags::RDWR
    } else {
        OFlags::RDONLY
    };
    if args.bool_or(2, true)? {
        flags |= OFlags::NONBLOCK;
    }
    for name in args.strs(3)? {
        flags |= open_flag(&name, span)?;
    }
    let shown = path.display().to_string();
    match rustix::fs::open(&path, flags, Mode::empty()) {
        Ok(fd) => ok(Value::Int(i64::from(fd.into_raw_fd()))),
        Err(error) => Err(name_error(&shown, host_error("unix-open-fd", error, span))),
    }
}

fn open_flag(name: &str, span: Span) -> Result<rustix::fs::OFlags, RuntimeError> {
    use rustix::fs::OFlags;
    Ok(match name {
        #[cfg(any(target_os = "linux", target_os = "android"))]
        "direct" => OFlags::DIRECT,
        #[cfg(any(target_os = "linux", target_os = "android"))]
        "noatime" => OFlags::NOATIME,
        "nofollow" => OFlags::NOFOLLOW,
        "directory" => OFlags::DIRECTORY,
        "dsync" => OFlags::DSYNC,
        "sync" => OFlags::SYNC,
        "append" => OFlags::APPEND,
        "nonblock" => OFlags::NONBLOCK,
        "noctty" => OFlags::NOCTTY,
        other => {
            return Err(invalid(
                format!(
                    "open flag must be one of direct, noatime, nofollow, directory, dsync, sync, append, nonblock or noctty on this platform, found `{other}`"
                ),
                span,
            ));
        }
    })
}

/// Closes a descriptor opened by `unix.open_fd` or `unix.open_pty`. The
/// standard streams are not the script's to close.
fn close_fd(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let span = args.span();
    let fd = fd_arg(args, 0, 0)?;
    if fd <= libc::STDERR_FILENO {
        return Err(invalid("refusing to close a standard stream", span));
    }
    // The kernel reports EBADF for a number that is not open; close(2) itself
    // has no failure rustix would surface.
    rustix::io::fcntl_getfd(borrow(fd))
        .map_err(|error| host_error("unix-close-fd", error, span))?;
    // SAFETY: the caller names a descriptor it owns, and it was just found open.
    unsafe { rustix::io::close(fd) };
    ok(Value::Unit)
}

/// A new pseudo-terminal pair: the controller (master) and the replica
/// (slave) as descriptor numbers, and the replica's device path.
fn open_pty(args: &Args<'_>) -> Result<Value, RuntimeError> {
    use rustix::fs::{Mode, OFlags};
    use rustix::io::{FdFlags, fcntl_setfd};
    use rustix::pty::{OpenptFlags, grantpt, openpt, ptsname, unlockpt};
    let span = args.span();
    let fail = |error: rustix::io::Errno| host_error("unix-open-pty", error, span);
    let master = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY).map_err(fail)?;
    fcntl_setfd(&master, FdFlags::CLOEXEC).map_err(fail)?;
    grantpt(&master).map_err(fail)?;
    unlockpt(&master).map_err(fail)?;
    let name = ptsname(&master, Vec::new()).map_err(fail)?;
    let replica = rustix::fs::open(
        name.as_c_str(),
        OFlags::RDWR | OFlags::NOCTTY | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(fail)?;
    ok(Value::Record(RecordMap::from([
        (key("master"), Value::Int(i64::from(master.into_raw_fd()))),
        (key("replica"), Value::Int(i64::from(replica.into_raw_fd()))),
        (key("name"), name_of(&name)),
    ])))
}

fn window_size(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let fd = borrow(fd_arg(args, 0, 0)?);
    match termios::tcgetwinsize(fd) {
        Ok(size) => ok(Value::Record(RecordMap::from([
            (key("rows"), Value::Int(i64::from(size.ws_row))),
            (key("cols"), Value::Int(i64::from(size.ws_col))),
            (key("xpixel"), Value::Int(i64::from(size.ws_xpixel))),
            (key("ypixel"), Value::Int(i64::from(size.ws_ypixel))),
        ]))),
        Err(error) => Err(host_error("unix-window-size", error, args.span())),
    }
}

fn dimension(args: &Args<'_>, index: usize, name: &str) -> Result<u16, RuntimeError> {
    let value = args.int_or(index, 0)?;
    u16::try_from(value)
        .map_err(|_| invalid(format!("{name} must be between 0 and 65535"), args.span()))
}

fn set_window_size(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let size = termios::Winsize {
        ws_row: dimension(args, 0, "rows")?,
        ws_col: dimension(args, 1, "cols")?,
        ws_xpixel: dimension(args, 2, "xpixel")?,
        ws_ypixel: dimension(args, 3, "ypixel")?,
    };
    let fd = borrow(fd_arg(args, 4, 0)?);
    match termios::tcsetwinsize(fd, size) {
        Ok(()) => ok(Value::Unit),
        Err(error) => Err(host_error("unix-set-window-size", error, args.span())),
    }
}

fn pid_value(pid: Pid) -> Value {
    Value::Int(i64::from(pid.as_raw_nonzero().get()))
}

fn foreground_group(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let fd = borrow(fd_arg(args, 0, 0)?);
    match termios::tcgetpgrp(fd) {
        Ok(pgid) => ok(pid_value(pgid)),
        Err(error) => Err(host_error("unix-foreground-group", error, args.span())),
    }
}

/// `tcsetpgrp(3)`: a background process group that calls it receives `SIGTTOU`.
fn set_foreground_group(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let span = args.span();
    let pgid = args.int(0)?;
    let Some(pgid) = positive_pid(pgid) else {
        return Err(invalid("process group id must be positive", span));
    };
    let fd = borrow(fd_arg(args, 1, 0)?);
    match termios::tcsetpgrp(fd, pgid) {
        Ok(()) => ok(Value::Unit),
        Err(error) => Err(host_error("unix-set-foreground-group", error, span)),
    }
}

fn tty_session(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let fd = borrow(fd_arg(args, 0, 0)?);
    match termios::tcgetsid(fd) {
        Ok(sid) => ok(pid_value(sid)),
        Err(error) => Err(host_error("unix-tty-session", error, args.span())),
    }
}

#[derive(Clone, Copy)]
struct Flag {
    name: &'static str,
    field: &'static str,
    mask: u64,
    value: u64,
    sane: bool,
}

const fn bit(name: &'static str, field: &'static str, bits: libc::tcflag_t, sane: bool) -> Flag {
    Flag {
        name,
        field,
        mask: bits as u64,
        value: bits as u64,
        sane,
    }
}

const fn group(
    name: &'static str,
    field: &'static str,
    mask: libc::tcflag_t,
    value: libc::tcflag_t,
    sane: bool,
) -> Flag {
    Flag {
        name,
        field,
        mask: mask as u64,
        value: value as u64,
        sane,
    }
}

// x86_64 musl declares some delay values (`CR1`, `TAB1`, `BS1`, ...) as `c_int`
// where other targets use `tcflag_t`, so those are cast; the cast is a no-op
// elsewhere.
#[allow(clippy::unnecessary_cast)]
const FLAGS: &[Flag] = &[
    bit("ignbrk", "iflag", libc::IGNBRK, false),
    bit("brkint", "iflag", libc::BRKINT, true),
    bit("ignpar", "iflag", libc::IGNPAR, false),
    bit("parmrk", "iflag", libc::PARMRK, false),
    bit("inpck", "iflag", libc::INPCK, false),
    bit("istrip", "iflag", libc::ISTRIP, false),
    bit("inlcr", "iflag", libc::INLCR, false),
    bit("igncr", "iflag", libc::IGNCR, false),
    bit("icrnl", "iflag", libc::ICRNL, true),
    #[cfg(any(target_os = "linux", target_os = "android"))]
    bit("iuclc", "iflag", libc::IUCLC, false),
    bit("ixon", "iflag", libc::IXON, false),
    bit("ixany", "iflag", libc::IXANY, false),
    bit("ixoff", "iflag", libc::IXOFF, false),
    bit("imaxbel", "iflag", libc::IMAXBEL, true),
    #[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
    bit("iutf8", "iflag", libc::IUTF8, false),
    bit("opost", "oflag", libc::OPOST, true),
    #[cfg(any(target_os = "linux", target_os = "android"))]
    bit("olcuc", "oflag", libc::OLCUC, false),
    bit("ocrnl", "oflag", libc::OCRNL, false),
    bit("onlcr", "oflag", libc::ONLCR, true),
    bit("onocr", "oflag", libc::ONOCR, false),
    bit("onlret", "oflag", libc::ONLRET, false),
    bit("ofill", "oflag", libc::OFILL, false),
    bit("ofdel", "oflag", libc::OFDEL, false),
    group("nl0", "oflag", libc::NLDLY, libc::NL0, true),
    group("nl1", "oflag", libc::NLDLY, libc::NL1, false),
    group("cr0", "oflag", libc::CRDLY, libc::CR0, true),
    group(
        "cr1",
        "oflag",
        libc::CRDLY,
        libc::CR1 as libc::tcflag_t,
        false,
    ),
    group(
        "cr2",
        "oflag",
        libc::CRDLY,
        libc::CR2 as libc::tcflag_t,
        false,
    ),
    group(
        "cr3",
        "oflag",
        libc::CRDLY,
        libc::CR3 as libc::tcflag_t,
        false,
    ),
    group("tab0", "oflag", libc::TABDLY, libc::TAB0, true),
    group(
        "tab1",
        "oflag",
        libc::TABDLY,
        libc::TAB1 as libc::tcflag_t,
        false,
    ),
    group(
        "tab2",
        "oflag",
        libc::TABDLY,
        libc::TAB2 as libc::tcflag_t,
        false,
    ),
    group(
        "tab3",
        "oflag",
        libc::TABDLY,
        libc::TAB3 as libc::tcflag_t,
        false,
    ),
    group("bs0", "oflag", libc::BSDLY, libc::BS0, true),
    group(
        "bs1",
        "oflag",
        libc::BSDLY,
        libc::BS1 as libc::tcflag_t,
        false,
    ),
    group("vt0", "oflag", libc::VTDLY, libc::VT0, true),
    group(
        "vt1",
        "oflag",
        libc::VTDLY,
        libc::VT1 as libc::tcflag_t,
        false,
    ),
    group("ff0", "oflag", libc::FFDLY, libc::FF0, true),
    group(
        "ff1",
        "oflag",
        libc::FFDLY,
        libc::FF1 as libc::tcflag_t,
        false,
    ),
    bit("parenb", "cflag", libc::PARENB, false),
    bit("parodd", "cflag", libc::PARODD, false),
    #[cfg(any(target_os = "linux", target_os = "android"))]
    bit("cmspar", "cflag", libc::CMSPAR, false),
    group("cs5", "cflag", libc::CSIZE, libc::CS5, false),
    group("cs6", "cflag", libc::CSIZE, libc::CS6, false),
    group("cs7", "cflag", libc::CSIZE, libc::CS7, false),
    group("cs8", "cflag", libc::CSIZE, libc::CS8, true),
    bit("hupcl", "cflag", libc::HUPCL, false),
    bit("cstopb", "cflag", libc::CSTOPB, false),
    bit("cread", "cflag", libc::CREAD, true),
    bit("clocal", "cflag", libc::CLOCAL, false),
    bit("crtscts", "cflag", libc::CRTSCTS, false),
    bit("isig", "lflag", libc::ISIG, true),
    bit("icanon", "lflag", libc::ICANON, true),
    bit("iexten", "lflag", libc::IEXTEN, true),
    bit("echo", "lflag", libc::ECHO, true),
    bit("echoe", "lflag", libc::ECHOE, true),
    bit("echok", "lflag", libc::ECHOK, true),
    bit("echonl", "lflag", libc::ECHONL, false),
    bit("noflsh", "lflag", libc::NOFLSH, false),
    #[cfg(any(target_os = "linux", target_os = "android"))]
    bit("xcase", "lflag", libc::XCASE, false),
    bit("tostop", "lflag", libc::TOSTOP, false),
    bit("echoprt", "lflag", libc::ECHOPRT, false),
    bit("echoctl", "lflag", libc::ECHOCTL, true),
    bit("echoke", "lflag", libc::ECHOKE, true),
    bit("flusho", "lflag", libc::FLUSHO, false),
    bit("extproc", "lflag", libc::EXTPROC, false),
];

struct Char {
    name: &'static str,
    index: usize,
    sane: u8,
}

const CHARS: &[Char] = &[
    Char {
        name: "intr",
        index: libc::VINTR,
        sane: 3,
    },
    Char {
        name: "quit",
        index: libc::VQUIT,
        sane: 28,
    },
    Char {
        name: "erase",
        index: libc::VERASE,
        sane: 127,
    },
    Char {
        name: "kill",
        index: libc::VKILL,
        sane: 21,
    },
    Char {
        name: "eof",
        index: libc::VEOF,
        sane: 4,
    },
    Char {
        name: "eol",
        index: libc::VEOL,
        sane: 0,
    },
    Char {
        name: "eol2",
        index: libc::VEOL2,
        sane: 0,
    },
    #[cfg(any(target_os = "linux", target_os = "android"))]
    Char {
        name: "swtch",
        index: libc::VSWTC,
        sane: 0,
    },
    Char {
        name: "start",
        index: libc::VSTART,
        sane: 17,
    },
    Char {
        name: "stop",
        index: libc::VSTOP,
        sane: 19,
    },
    Char {
        name: "susp",
        index: libc::VSUSP,
        sane: 26,
    },
    Char {
        name: "rprnt",
        index: libc::VREPRINT,
        sane: 18,
    },
    Char {
        name: "werase",
        index: libc::VWERASE,
        sane: 23,
    },
    Char {
        name: "lnext",
        index: libc::VLNEXT,
        sane: 22,
    },
    Char {
        name: "discard",
        index: libc::VDISCARD,
        sane: 15,
    },
    Char {
        name: "min",
        index: libc::VMIN,
        sane: 1,
    },
    Char {
        name: "time",
        index: libc::VTIME,
        sane: 0,
    },
];

/// The baud rates `stty` accepts by name; any of them can be a terminal speed.
const SPEEDS: &[i64] = &[
    0, 50, 75, 110, 134, 150, 200, 300, 600, 1200, 1800, 2400, 4800, 9600, 19200, 38400, 57600,
    115200, 230400, 460800, 500000, 576000, 921600, 1000000, 1152000, 1500000, 2000000, 2500000,
    3000000, 3500000, 4000000,
];

fn tty_table() -> Value {
    let flags = FLAGS
        .iter()
        .map(|flag| {
            Value::Record(RecordMap::from([
                (key("name"), Value::Str(flag.name.into())),
                (key("field"), Value::Str(flag.field.into())),
                (key("mask"), Value::Int(flag.mask as i64)),
                (key("value"), Value::Int(flag.value as i64)),
                (key("sane"), Value::Bool(flag.sane)),
            ]))
        })
        .collect();
    let chars = CHARS
        .iter()
        .map(|char| {
            Value::Record(RecordMap::from([
                (key("name"), Value::Str(char.name.into())),
                (key("index"), Value::Int(char.index as i64)),
                (key("sane"), Value::Int(i64::from(char.sane))),
            ]))
        })
        .collect();
    Value::Record(RecordMap::from([
        (key("flags"), Value::List(flags)),
        (key("chars"), Value::List(chars)),
        (
            key("speeds"),
            Value::List(SPEEDS.iter().map(|s| Value::Int(*s)).collect()),
        ),
    ]))
}

/// The four flag words and the control characters of a `UnixTtyAttrs` record,
/// edited by flag and character name.
struct Modes {
    iflag: u64,
    oflag: u64,
    cflag: u64,
    lflag: u64,
    cc: Vec<i64>,
}

impl Modes {
    fn field(&mut self, field: &str) -> &mut u64 {
        match field {
            "iflag" => &mut self.iflag,
            "oflag" => &mut self.oflag,
            "cflag" => &mut self.cflag,
            _ => &mut self.lflag,
        }
    }

    /// `-name` clears a flag (or restores a group to 0), `name` sets it.
    fn flag(&mut self, spec: &str) {
        let (name, on) = spec
            .strip_prefix('-')
            .map_or((spec, true), |name| (name, false));
        let Some(flag) = FLAGS.iter().find(|flag| flag.name == name) else {
            return;
        };
        let word = self.field(flag.field);
        if on {
            *word = (*word & !flag.mask) | flag.value;
        } else {
            *word &= !flag.value;
        }
    }

    fn char(&mut self, name: &str, value: i64) {
        if let Some(char) = CHARS.iter().find(|char| char.name == name)
            && let Some(slot) = self.cc.get_mut(char.index)
        {
            *slot = value;
        }
    }
}

fn record_flag(record: &RecordMap, name: &str, span: Span) -> Result<u64, RuntimeError> {
    match record.get(name) {
        Some(Value::Int(value)) if *value >= 0 => Ok(*value as u64),
        Some(_) => Err(invalid(format!("{name} must be a non-negative Int"), span)),
        None => Err(invalid(format!("missing `{name}` field"), span)),
    }
}

const COMBINATIONS: &[(&str, &[&str], &[(&str, i64)])] = &[
    (
        "raw",
        &[
            "-ignbrk", "-brkint", "-ignpar", "-parmrk", "-inpck", "-istrip", "-inlcr", "-igncr",
            "-icrnl", "-ixon", "-ixoff", "-icanon", "-opost", "-isig", "-iuclc", "-xcase",
            "-ixany", "-imaxbel",
        ],
        &[("min", 1), ("time", 0)],
    ),
    (
        "cooked",
        &[
            "brkint", "ignpar", "istrip", "icrnl", "ixon", "opost", "isig", "icanon",
        ],
        &[("eof", 4), ("eol", 0)],
    ),
    ("cbreak", &["-icanon"], &[]),
    (
        "sane",
        &[
            "cread", "-ignbrk", "brkint", "-inlcr", "-igncr", "icrnl", "icanon", "iexten", "echo",
            "echoe", "echok", "-echonl", "-noflsh", "-ixoff", "-iutf8", "-iuclc", "-xcase",
            "-ixany", "imaxbel", "-olcuc", "-ocrnl", "opost", "-ofill", "onlcr", "-onocr",
            "-onlret", "nl0", "cr0", "tab0", "bs0", "vt0", "ff0", "isig", "-tostop", "-ofdel",
            "-echoprt", "echoctl", "echoke", "-extproc", "-flusho",
        ],
        &[
            ("intr", 3),
            ("quit", 28),
            ("erase", 127),
            ("kill", 21),
            ("eof", 4),
            ("eol", 0),
            ("eol2", 0),
            ("swtch", 0),
            ("start", 17),
            ("stop", 19),
            ("susp", 26),
            ("rprnt", 18),
            ("werase", 23),
            ("lnext", 22),
            ("discard", 15),
        ],
    ),
];

/// Applies `stty`'s `raw`, `cooked`, `cbreak`, or `sane` to a terminal
/// attributes record and returns the changed record; the terminal itself is
/// untouched until `unix.set_tty_attrs`.
fn tty_mode(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let span = args.span();
    let record = args.record(0)?;
    let mode = args.str(1)?;
    let Some((_, flags, chars)) = COMBINATIONS.iter().find(|(name, _, _)| *name == mode) else {
        return Err(invalid(
            format!(
                "mode must be one of {}, found `{mode}`",
                COMBINATIONS
                    .iter()
                    .map(|(name, _, _)| *name)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            span,
        ));
    };
    let cc = match record.get("control_chars") {
        Some(Value::List(items)) => items
            .iter()
            .map(|item| match item {
                Value::Int(value) => Ok(*value),
                _ => Err(invalid("control_chars must be a List[Int]", span)),
            })
            .collect::<Result<Vec<_>, _>>()?,
        _ => return Err(invalid("missing `control_chars` field", span)),
    };
    let mut modes = Modes {
        iflag: record_flag(record, "iflag", span)?,
        oflag: record_flag(record, "oflag", span)?,
        cflag: record_flag(record, "cflag", span)?,
        lflag: record_flag(record, "lflag", span)?,
        cc,
    };
    for flag in *flags {
        modes.flag(flag);
    }
    for (name, value) in *chars {
        modes.char(name, *value);
    }
    let mut fields = record.clone();
    fields.insert(key("iflag"), Value::Int(modes.iflag as i64));
    fields.insert(key("oflag"), Value::Int(modes.oflag as i64));
    fields.insert(key("cflag"), Value::Int(modes.cflag as i64));
    fields.insert(key("lflag"), Value::Int(modes.lflag as i64));
    fields.insert(
        key("control_chars"),
        Value::List(modes.cc.into_iter().map(Value::Int).collect()),
    );
    derive(&mut fields, modes.iflag, modes.lflag);
    ok(Value::Record(fields))
}

/// Recomputes the convenience booleans of a `UnixTtyAttrs` record from its
/// flag words, the way `unix.tty_attrs` reports them.
pub(crate) fn derive(fields: &mut RecordMap, iflag: u64, lflag: u64) {
    let local = |bits: libc::tcflag_t| lflag & u64::from(bits) != 0;
    let input = |bits: libc::tcflag_t| iflag & u64::from(bits) != 0;
    fields.insert(key("echo"), Value::Bool(local(libc::ECHO)));
    fields.insert(
        key("raw"),
        Value::Bool(
            !(local(libc::ICANON) || local(libc::ECHO) || local(libc::ISIG))
                && !(input(libc::ICRNL) || input(libc::IXON)),
        ),
    );
    fields.insert(key("crnl"), Value::Bool(input(libc::ICRNL)));
}
