//! Socket creation, connection, transfer, and option primitives.

use super::address::SockAddr;
use super::{Args, constant, ok, record};
use crate::modules::RuntimeOp;
use crate::runtime::value::{RuntimeError, Value};
use rustix::fd::{BorrowedFd, IntoRawFd};
use rustix::io::Errno;
use rustix::net::sockopt::{self, Timeout};
use rustix::net::{
    self, AddressFamily, Protocol, RecvAncillaryBuffer, RecvFlags, SendFlags, Shutdown,
    SocketFlags, SocketType,
};
use std::io::{self, IoSliceMut};
use std::mem::{MaybeUninit, size_of};
use std::num::NonZeroU32;
use std::time::Duration;

/// Receive buffer ceiling, so a typo cannot ask for gigabytes.
const MAX_RECEIVE: i64 = 64 * 1024 * 1024;
/// Ancillary data room per receive: IP_RECVERR, TTL/hop limit, and TOS fit in
/// well under this.
const CONTROL_BYTES: usize = 1024;
/// Largest option value the byte forms of setsockopt and getsockopt move.
const MAX_OPTION_BYTES: usize = 64 * 1024;

pub(super) fn call(op: RuntimeOp, args: &Args<'_>) -> Result<Value, RuntimeError> {
    match op {
        RuntimeOp::LinuxSocket => socket(args),
        RuntimeOp::LinuxConnect => connect_or_bind(args, true),
        RuntimeOp::LinuxBind => connect_or_bind(args, false),
        RuntimeOp::LinuxListen => listen(args),
        RuntimeOp::LinuxAccept => accept(args),
        RuntimeOp::LinuxSendto => sendto(args),
        RuntimeOp::LinuxRecvfrom => recvfrom(args),
        RuntimeOp::LinuxShutdown => shutdown(args),
        RuntimeOp::LinuxGetsockname => name(args, false),
        RuntimeOp::LinuxGetpeername => name(args, true),
        RuntimeOp::LinuxSetsockoptInt => setsockopt_int(args),
        RuntimeOp::LinuxGetsockoptInt => getsockopt_int(args),
        RuntimeOp::LinuxSetsockoptBytes => setsockopt_bytes(args),
        RuntimeOp::LinuxGetsockoptBytes => getsockopt_bytes(args),
        RuntimeOp::LinuxSetSocketTimeout => set_timeout(args),
        _ => unreachable!("socket primitive expected"),
    }
}

/// Borrows a descriptor the script names by number. The kernel validates it
/// on use, so a closed or foreign number fails with EBADF.
pub(super) fn borrow(fd: libc::c_int) -> BorrowedFd<'static> {
    // SAFETY: the descriptor is only passed to system calls that report EBADF.
    unsafe { BorrowedFd::borrow_raw(fd) }
}

fn socket(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let family: u16 = args.int_in(0, "address family")?;
    let kind: u32 = args.int_in(1, "socket type")?;
    let protocol: u32 = args.int_in_or(2, 0, "protocol")?;
    let nonblock = constant("SOCK_NONBLOCK") as u32;
    let cloexec = constant("SOCK_CLOEXEC") as u32;
    // Close-on-exec always: a descriptor a script opens must not leak into
    // the commands it later spawns.
    let mut flags = SocketFlags::CLOEXEC;
    if kind & nonblock != 0 {
        flags |= SocketFlags::NONBLOCK;
    }
    let kind = SocketType::from_raw(kind & !(nonblock | cloexec));
    let protocol = NonZeroU32::new(protocol).map(Protocol::from_raw);
    let fd = net::socket_with(AddressFamily::from_raw(family), kind, flags, protocol)
        .map_err(|error| args.os(error))?;
    ok(Value::Int(i64::from(fd.into_raw_fd())))
}

// connect is not retried on EINTR: the kernel keeps connecting in the
// background, so a second call would report EALREADY or EISCONN.
fn connect_or_bind(args: &Args<'_>, connect: bool) -> Result<Value, RuntimeError> {
    let fd = borrow(args.fd(0)?);
    let address = SockAddr::from_value(args, 1)?;
    let status = if connect {
        net::connect(fd, &address)
    } else {
        net::bind(fd, &address)
    };
    status.map_err(|error| args.os(error))?;
    ok(Value::Unit)
}

fn listen(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let fd = borrow(args.fd(0)?);
    let backlog: i32 = args.int_in_or(1, 128, "backlog")?;
    net::listen(fd, backlog).map_err(|error| args.os(error))?;
    ok(Value::Unit)
}

fn accept(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let fd = borrow(args.fd(0)?);
    let mut flags = SocketFlags::CLOEXEC;
    if args.bool_or(1, false)? {
        flags |= SocketFlags::NONBLOCK;
    }
    loop {
        match net::acceptfrom_with(fd, flags) {
            Ok((accepted, peer)) => {
                let peer = peer.as_ref().map_or_else(SockAddr::unspecified, SockAddr::from_kernel);
                return ok(record([
                    ("fd", Value::Int(i64::from(accepted.into_raw_fd()))),
                    ("peer", peer.to_value()),
                ]));
            }
            Err(Errno::INTR) => {}
            Err(error) => return Err(args.os(error)),
        }
    }
}

fn sendto(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let fd = borrow(args.fd(0)?);
    let data = args.bytes(1)?;
    let address = match args.get(2) {
        Some(_) => Some(SockAddr::from_value(args, 2)?),
        None => None,
    };
    let flags: u32 = args.int_in_or(3, 0, "flags")?;
    // NOSIGNAL: a vanished peer is an EPIPE the script handles, not a signal
    // that ends the process.
    let flags = SendFlags::from_bits_retain(flags) | SendFlags::NOSIGNAL;
    loop {
        let sent = match &address {
            Some(address) => net::sendto(fd, data, flags, address),
            None => net::send(fd, data, flags),
        };
        match sent {
            Ok(sent) => return ok(Value::Int(sent as i64)),
            Err(Errno::INTR) => {}
            Err(error) => return Err(args.os(error)),
        }
    }
}

fn recvfrom(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let fd = borrow(args.fd(0)?);
    let max = args.int(1)?;
    if !(1..=MAX_RECEIVE).contains(&max) {
        return Err(args.invalid(format!("max_bytes must be between 1 and {MAX_RECEIVE}")));
    }
    let flags: u32 = args.int_in_or(2, 0, "flags")?;
    let flags = RecvFlags::from_bits_retain(flags) | RecvFlags::CMSG_CLOEXEC;
    let mut data = vec![0_u8; max as usize];
    // Zeroed u64 words give the buffer cmsghdr alignment, and the zeros mark
    // where the kernel's messages end, since rustix does not expose arbitrary
    // control messages and only the kernel's own framing is read back here.
    let mut control = [0_u64; CONTROL_BYTES / size_of::<u64>()];
    let message = loop {
        // SAFETY: the zeroed words are valid initialised bytes of the same length.
        let space = unsafe {
            std::slice::from_raw_parts_mut(
                control.as_mut_ptr().cast::<MaybeUninit<u8>>(),
                CONTROL_BYTES,
            )
        };
        let mut ancillary = RecvAncillaryBuffer::new(space);
        let mut vectors = [IoSliceMut::new(&mut data)];
        let received = net::recvmsg(fd, &mut vectors, &mut ancillary, flags);
        // Dropping the buffer would close descriptors passed with SCM_RIGHTS;
        // they are close-on-exec and belong to the script, which sees their
        // numbers in the control message and closes them.
        std::mem::forget(ancillary);
        match received {
            Ok(message) => break message,
            Err(Errno::INTR) => {}
            Err(error) => return Err(args.os(error)),
        }
    };
    // A datagram longer than the buffer reports its full size under MSG_TRUNC.
    data.truncate(message.bytes.min(data.len()));
    let source = message
        .address
        .as_ref()
        .map_or_else(SockAddr::unspecified, SockAddr::from_kernel);
    // SAFETY: reading back the words just viewed as bytes.
    let control_bytes = unsafe {
        std::slice::from_raw_parts(control.as_ptr().cast::<u8>(), CONTROL_BYTES)
    };
    ok(record([
        ("data", Value::Bytes(data)),
        ("address", source.to_value()),
        ("flags", Value::Int(i64::from(message.flags.bits()))),
        ("control", Value::List(control_messages(control_bytes))),
    ]))
}

/// Walks the kernel's `cmsghdr` sequence: a native-width length covering the
/// header and data, the level and type as ints, then the data, with each
/// message padded to the native word. The buffer was zeroed, so a zero length
/// ends the sequence.
fn control_messages(buffer: &[u8]) -> Vec<Value> {
    const WORD: usize = size_of::<usize>();
    const HEADER: usize = WORD + 2 * size_of::<i32>();
    let mut messages = Vec::new();
    let mut offset = 0;
    while offset + HEADER <= buffer.len() {
        let length = usize::from_ne_bytes(buffer[offset..offset + WORD].try_into().unwrap());
        if length < HEADER || offset + length > buffer.len() {
            break;
        }
        let level = i32::from_ne_bytes(buffer[offset + WORD..offset + WORD + 4].try_into().unwrap());
        let kind =
            i32::from_ne_bytes(buffer[offset + WORD + 4..offset + HEADER].try_into().unwrap());
        messages.push(record([
            ("level", Value::Int(i64::from(level))),
            ("type", Value::Int(i64::from(kind))),
            ("data", Value::Bytes(buffer[offset + HEADER..offset + length].to_vec())),
        ]));
        offset += length.next_multiple_of(WORD);
    }
    messages
}

fn shutdown(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let fd = borrow(args.fd(0)?);
    let how = match args.int(1)? {
        0 => Shutdown::Read,
        1 => Shutdown::Write,
        2 => Shutdown::Both,
        other => return Err(args.invalid(format!("how {other} is not SHUT_RD, SHUT_WR, or SHUT_RDWR"))),
    };
    net::shutdown(fd, how).map_err(|error| args.os(error))?;
    ok(Value::Unit)
}

fn name(args: &Args<'_>, peer: bool) -> Result<Value, RuntimeError> {
    let fd = borrow(args.fd(0)?);
    let address = if peer {
        net::getpeername(fd).map_err(|error| args.os(error))?
    } else {
        Some(net::getsockname(fd).map_err(|error| args.os(error))?)
    };
    let address = address
        .as_ref()
        .map_or_else(SockAddr::unspecified, SockAddr::from_kernel);
    ok(address.to_value())
}

// rustix types the options it knows; a script names any level and option by
// number, so the generic forms use the raw calls with sized buffers.
fn setsockopt_int(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let fd = args.fd(0)?;
    let level: libc::c_int = args.int_in(1, "level")?;
    let option: libc::c_int = args.int_in(2, "option")?;
    let value: libc::c_int = args.int_in(3, "value")?;
    set_option(args, fd, level, option, &value.to_ne_bytes())
}

fn setsockopt_bytes(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let fd = args.fd(0)?;
    let level: libc::c_int = args.int_in(1, "level")?;
    let option: libc::c_int = args.int_in(2, "option")?;
    let value = args.bytes(3)?;
    if value.len() > MAX_OPTION_BYTES {
        return Err(args.invalid(format!("option value is limited to {MAX_OPTION_BYTES} bytes")));
    }
    set_option(args, fd, level, option, value)
}

fn set_option(
    args: &Args<'_>,
    fd: libc::c_int,
    level: libc::c_int,
    option: libc::c_int,
    value: &[u8],
) -> Result<Value, RuntimeError> {
    // SAFETY: `value` is readable for its length for the duration of the call.
    let status = unsafe {
        libc::setsockopt(fd, level, option, value.as_ptr().cast(), value.len() as libc::socklen_t)
    };
    if status < 0 {
        return Err(args.errno(io::Error::last_os_error()));
    }
    ok(Value::Unit)
}

fn get_option(
    args: &Args<'_>,
    fd: libc::c_int,
    level: libc::c_int,
    option: libc::c_int,
    buffer: &mut [u8],
) -> Result<usize, RuntimeError> {
    let mut length = buffer.len() as libc::socklen_t;
    // SAFETY: `buffer` is writable for `length` bytes; the kernel shrinks it.
    let status =
        unsafe { libc::getsockopt(fd, level, option, buffer.as_mut_ptr().cast(), &mut length) };
    if status < 0 {
        return Err(args.errno(io::Error::last_os_error()));
    }
    Ok((length as usize).min(buffer.len()))
}

fn getsockopt_int(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let fd = args.fd(0)?;
    let level: libc::c_int = args.int_in(1, "level")?;
    let option: libc::c_int = args.int_in(2, "option")?;
    let mut buffer = [0_u8; size_of::<libc::c_int>()];
    let length = get_option(args, fd, level, option, &mut buffer)?;
    if length != buffer.len() {
        return Err(args.errno(io::Error::from_raw_os_error(libc::EINVAL)));
    }
    ok(Value::Int(i64::from(libc::c_int::from_ne_bytes(buffer))))
}

fn getsockopt_bytes(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let fd = args.fd(0)?;
    let level: libc::c_int = args.int_in(1, "level")?;
    let option: libc::c_int = args.int_in(2, "option")?;
    let size = args.int(3)?;
    if !(1..=MAX_OPTION_BYTES as i64).contains(&size) {
        return Err(args.invalid(format!("length must be between 1 and {MAX_OPTION_BYTES}")));
    }
    let mut buffer = vec![0_u8; size as usize];
    let length = get_option(args, fd, level, option, &mut buffer)?;
    buffer.truncate(length);
    ok(Value::Bytes(buffer))
}

// The struct timeval width follows the host's time_t, which a script cannot
// know, so the timeout is built by rustix rather than packed as bytes.
fn set_timeout(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let fd = borrow(args.fd(0)?);
    let option = args.int(1)?;
    let which = if option == constant("SO_RCVTIMEO") {
        Timeout::Recv
    } else if option == constant("SO_SNDTIMEO") {
        Timeout::Send
    } else {
        return Err(args.invalid("option must be SO_RCVTIMEO or SO_SNDTIMEO"));
    };
    let millis = args.int(2)?;
    if millis < 0 {
        return Err(args.invalid("timeout_ms must be non-negative"));
    }
    // Zero disables the timeout; the kernel treats a zero timeval the same.
    let timeout = (millis > 0).then(|| Duration::from_millis(millis as u64));
    sockopt::set_socket_timeout(fd, which, timeout).map_err(|error| args.os(error))?;
    ok(Value::Unit)
}
