//! Descriptor replacement and explicit process credential transitions.

use crate::modules::RuntimeOp;
use crate::modules::process::{Args, host_error, name_error};
use crate::runtime::value::{RuntimeError, Value};
use crate::source::Span;
use std::fs::OpenOptions;
use std::io;
use std::os::fd::{AsRawFd, IntoRawFd};
use std::os::unix::fs::OpenOptionsExt;

pub(crate) fn handles(op: RuntimeOp) -> bool {
    matches!(op, RuntimeOp::UnixRedirectFd | RuntimeOp::UnixDupFd
        | RuntimeOp::UnixSetGroups | RuntimeOp::UnixSetCredentials
        | RuntimeOp::UnixSetUid | RuntimeOp::UnixSetGid)
}

pub(crate) fn call(op: RuntimeOp, args: &Args<'_>) -> Result<Value, RuntimeError> {
    match op {
        RuntimeOp::UnixRedirectFd => redirect_fd(args),
        RuntimeOp::UnixDupFd => dup_fd(args),
        RuntimeOp::UnixSetUid => set_identity(args, true),
        RuntimeOp::UnixSetGid => set_identity(args, false),
        RuntimeOp::UnixSetGroups => set_groups(args),
        RuntimeOp::UnixSetCredentials => set_credentials(args),
        _ => unreachable!("Unix process primitive expected"),
    }
}

fn invalid(kind: &str, message: &str, span: Span) -> RuntimeError {
    RuntimeError::new(kind, message).with_span(span)
}

fn descriptor(fd: i64, kind: &str, span: Span) -> Result<libc::c_int, RuntimeError> {
    libc::c_int::try_from(fd).ok().filter(|fd| *fd >= 0)
        .ok_or_else(|| invalid(kind, "fd must be between 0 and 2147483647", span))
}

/// Descriptor replacement must survive exec, including when open allocated
/// the target descriptor itself or source and target were already equal.
fn duplicate(source: libc::c_int, target: libc::c_int, kind: &str, span: Span) -> Result<(), RuntimeError> {
    if unsafe { libc::dup2(source, target) } == -1 {
        return Err(host_error(kind, io::Error::last_os_error(), span));
    }
    if source == target {
        let flags = unsafe { libc::fcntl(target, libc::F_GETFD) };
        if flags == -1 || unsafe { libc::fcntl(target, libc::F_SETFD, flags & !libc::FD_CLOEXEC) } == -1 {
            return Err(host_error(kind, io::Error::last_os_error(), span));
        }
    }
    Ok(())
}

fn redirect_fd(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let kind = "unix-redirect-fd";
    let span = args.span();
    let target = descriptor(args.int(0)?, kind, span)?;
    let path = args.path(1)?;
    let write = args.bool_or(2, false)?;
    let append = args.bool_or(3, false)?;
    let mode = args.int_or(4, 438)?;
    if append && !write {
        return Err(invalid(kind, "append requires write: true", span));
    }
    if !(0..=4095).contains(&mode) {
        return Err(invalid(kind, "mode must be between 0 and 4095", span));
    }
    let mut options = OpenOptions::new();
    options.mode(mode as u32);
    if write {
        options.write(true).create(true).append(append).truncate(!append);
    } else {
        options.read(true);
    }
    let file = options.open(&path)
        .map_err(|error| name_error(&path.display().to_string(), host_error(kind, error, span)))?;
    duplicate(file.as_raw_fd(), target, kind, span)?;
    if file.as_raw_fd() == target {
        let _ = file.into_raw_fd();
    }
    Ok(Value::ok(Value::Unit))
}

fn dup_fd(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let kind = "unix-dup-fd";
    let span = args.span();
    let source = descriptor(args.int(0)?, kind, span)?;
    let target = descriptor(args.int(1)?, kind, span)?;
    duplicate(source, target, kind, span)?;
    Ok(Value::ok(Value::Unit))
}

// The all-ones UID/GID is the keep-current sentinel in credential syscalls;
// an explicit identity must never silently mean "leave it unchanged".
fn identity(id: i64, kind: &str, span: Span) -> Result<u32, RuntimeError> {
    u32::try_from(id).ok().filter(|id| *id != u32::MAX)
        .ok_or_else(|| invalid(kind, "identity must be between 0 and 4294967294", span))
}

fn group_ids(args: &Args<'_>, index: usize, kind: &str) -> Result<Vec<libc::gid_t>, RuntimeError> {
    args.ints(index)?.into_iter()
        .map(|id| identity(id, kind, args.span()))
        .collect()
}

fn replace_groups(groups: &[libc::gid_t], kind: &str, span: Span) -> Result<(), RuntimeError> {
    // libc coordinates credential changes with other process threads on Linux.
    if unsafe { libc::setgroups(groups.len() as _, groups.as_ptr()) } == -1 {
        return Err(host_error(kind, io::Error::last_os_error(), span));
    }
    Ok(())
}

fn set_groups(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let kind = "unix-set-groups";
    let groups = group_ids(args, 0, kind)?;
    replace_groups(&groups, kind, args.span())?;
    Ok(Value::ok(Value::Unit))
}

/// Validate the whole request before touching credentials. Supplementary
/// groups and the primary GID must change before dropping UID privileges;
/// syscall failures cannot roll back completed credential changes.
fn set_credentials(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let kind = "unix-set-credentials";
    let span = args.span();
    let uid = identity(args.int(0)?, kind, span)?;
    let gid = identity(args.int(1)?, kind, span)?;
    let groups = group_ids(args, 2, kind)?;
    replace_groups(&groups, kind, span)?;
    if unsafe { libc::setgid(gid) } == -1 {
        return Err(host_error(kind, io::Error::last_os_error(), span));
    }
    if unsafe { libc::setuid(uid) } == -1 {
        return Err(host_error(kind, io::Error::last_os_error(), span));
    }
    Ok(Value::ok(Value::Unit))
}

fn set_identity(args: &Args<'_>, user: bool) -> Result<Value, RuntimeError> {
    let kind = if user { "unix-set-uid" } else { "unix-set-gid" };
    let id = identity(args.int(0)?, kind, args.span())?;
    let result = unsafe {
        if user { libc::setuid(id) } else { libc::setgid(id) }
    };
    if result == -1 {
        return Err(host_error(kind, io::Error::last_os_error(), args.span()));
    }
    Ok(Value::ok(Value::Unit))
}
