//! Netlink sockets: open, one request with its multipart reply, and
//! generic-netlink family resolution. Message payloads are returned as bytes;
//! decoding attributes and structures belongs to the script.

use super::address::SockAddr;
use super::sockets::borrow;
use super::{Args, ok, record};
use crate::modules::RuntimeOp;
use crate::runtime::value::{RuntimeError, Value};
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::fd::{AsFd, BorrowedFd, IntoRawFd, OwnedFd};
use rustix::io::Errno;
use rustix::net::{self, AddressFamily, Protocol, RecvFlags, SendFlags, SocketFlags, SocketType};
use std::io;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

const HEADER_LEN: usize = 16;
const NLMSG_NOOP: u16 = 1;
const NLMSG_ERROR: u16 = 2;
const NLMSG_DONE: u16 = 3;
const NLM_F_MULTI: u16 = 0x2;
const NLM_F_ACK: u16 = 0x4;
const NETLINK_GENERIC: u32 = 16;
const GENL_ID_CTRL: u16 = 0x10;
const CTRL_CMD_GETFAMILY: u8 = 3;
const CTRL_ATTR_FAMILY_ID: u16 = 1;
const CTRL_ATTR_FAMILY_NAME: u16 = 2;
/// Request payload ceiling; netlink requests are small structures.
const MAX_PAYLOAD: usize = 1024 * 1024;
const DEFAULT_TIMEOUT_MS: i64 = 5000;

static NEXT_SEQUENCE: AtomicU32 = AtomicU32::new(1);

/// One parsed netlink message: the header fields and the bytes after them.
struct Message {
    kind: u16,
    flags: u16,
    seq: u32,
    pid: u32,
    payload: Vec<u8>,
}

pub(super) fn call(op: RuntimeOp, args: &Args<'_>) -> Result<Value, RuntimeError> {
    match op {
        RuntimeOp::LinuxNetlinkOpen => open(args),
        RuntimeOp::LinuxNetlinkRequest => request(args),
        RuntimeOp::LinuxGenlFamilyId => family_id(args),
        _ => unreachable!("netlink primitive expected"),
    }
}

fn open_socket(args: &Args<'_>, protocol: u32, groups: u32) -> Result<OwnedFd, RuntimeError> {
    let protocol = NonZeroU32::new(protocol).map(Protocol::from_raw);
    let socket = net::socket_with(
        AddressFamily::NETLINK,
        SocketType::RAW,
        SocketFlags::CLOEXEC,
        protocol,
    )
    .map_err(|error| args.os(error))?;
    // The socket is closed by dropping it on this error path.
    net::bind(&socket, &SockAddr::netlink(0, groups)).map_err(|error| args.os(error))?;
    Ok(socket)
}

fn open(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let protocol: u32 = args.int_in(0, "protocol")?;
    let groups: u32 = args.int_in_or(1, 0, "groups")?;
    let socket = open_socket(args, protocol, groups)?;
    ok(Value::Int(i64::from(socket.into_raw_fd())))
}

fn request(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let fd = borrow(args.fd(0)?);
    let kind: u16 = args.int_in(1, "message_type")?;
    let flags: u16 = args.int_in(2, "flags")?;
    let payload = args.bytes(3)?;
    let seq: u32 = args.int_in_or(4, 0, "seq")?;
    let timeout = args.int_or(5, DEFAULT_TIMEOUT_MS)?;
    if timeout < 0 {
        return Err(args.invalid("timeout_ms must be non-negative"));
    }
    let messages = exchange(args, fd, kind, flags, payload, seq, Duration::from_millis(timeout as u64))?;
    ok(Value::List(
        messages
            .into_iter()
            .map(|message| {
                record([
                    ("type", Value::Int(i64::from(message.kind))),
                    ("flags", Value::Int(i64::from(message.flags))),
                    ("seq", Value::Int(i64::from(message.seq))),
                    ("pid", Value::Int(i64::from(message.pid))),
                    ("payload", Value::Bytes(message.payload)),
                ])
            })
            .collect(),
    ))
}

/// Sends one request and reads until the reply is complete: NLMSG_DONE, an
/// acknowledgement, or (when no acknowledgement was asked for) a message that
/// is not part of a multipart reply. A negative NLMSG_ERROR code is the
/// request's errno.
fn exchange(
    args: &Args<'_>,
    fd: BorrowedFd<'_>,
    kind: u16,
    flags: u16,
    payload: &[u8],
    seq: u32,
    timeout: Duration,
) -> Result<Vec<Message>, RuntimeError> {
    if payload.len() > MAX_PAYLOAD {
        return Err(args.invalid(format!("netlink payload is limited to {MAX_PAYLOAD} bytes")));
    }
    let seq = if seq == 0 {
        // Wraps past zero, which means "assign one".
        NEXT_SEQUENCE.fetch_add(1, Ordering::Relaxed).max(1)
    } else {
        seq
    };
    let mut request = Vec::with_capacity(HEADER_LEN + payload.len());
    request.extend_from_slice(&((HEADER_LEN + payload.len()) as u32).to_ne_bytes());
    request.extend_from_slice(&kind.to_ne_bytes());
    request.extend_from_slice(&flags.to_ne_bytes());
    request.extend_from_slice(&seq.to_ne_bytes());
    request.extend_from_slice(&0_u32.to_ne_bytes());
    request.extend_from_slice(payload);
    let kernel = SockAddr::netlink(0, 0);
    loop {
        match net::sendto(fd, &request, SendFlags::empty(), &kernel) {
            Ok(_) => break,
            Err(Errno::INTR) => {}
            Err(error) => return Err(args.os(error)),
        }
    }
    let deadline = Instant::now().checked_add(timeout).unwrap_or_else(Instant::now);
    let single_reply_ends = flags & NLM_F_ACK == 0;
    let mut replies = Vec::new();
    loop {
        wait_readable(args, fd, deadline)?;
        let datagram = receive_datagram(args, fd)?;
        let mut offset = 0;
        let mut finished = false;
        while offset + HEADER_LEN <= datagram.len() {
            let length = u32::from_ne_bytes(datagram[offset..offset + 4].try_into().unwrap()) as usize;
            if length < HEADER_LEN || offset + length > datagram.len() {
                return Err(args.errno(io::Error::from_raw_os_error(libc::EBADMSG)));
            }
            let field16 = |at: usize| u16::from_ne_bytes(datagram[offset + at..offset + at + 2].try_into().unwrap());
            let field32 = |at: usize| u32::from_ne_bytes(datagram[offset + at..offset + at + 4].try_into().unwrap());
            let message = Message {
                kind: field16(4),
                flags: field16(6),
                seq: field32(8),
                pid: field32(12),
                payload: datagram[offset + HEADER_LEN..offset + length].to_vec(),
            };
            offset += length.next_multiple_of(4);
            // A socket subscribed to multicast groups also receives events;
            // only this request's sequence belongs to its reply.
            if message.seq != seq {
                continue;
            }
            match message.kind {
                NLMSG_NOOP => {}
                NLMSG_DONE => finished = true,
                NLMSG_ERROR => {
                    let code = message
                        .payload
                        .get(..4)
                        .map(|bytes| i32::from_ne_bytes(bytes.try_into().unwrap()))
                        .ok_or_else(|| args.errno(io::Error::from_raw_os_error(libc::EBADMSG)))?;
                    if code < 0 {
                        return Err(args.errno(io::Error::from_raw_os_error(-code)));
                    }
                    finished = true;
                }
                _ => {
                    finished |= message.flags & NLM_F_MULTI == 0 && single_reply_ends;
                    replies.push(message);
                }
            }
        }
        if finished {
            return Ok(replies);
        }
    }
}

fn wait_readable(args: &Args<'_>, fd: BorrowedFd<'_>, deadline: Instant) -> Result<(), RuntimeError> {
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let timeout = Timespec {
            tv_sec: remaining.as_secs() as _,
            tv_nsec: remaining.subsec_nanos() as _,
        };
        let mut descriptors = [PollFd::new(&fd, PollFlags::IN)];
        match poll(&mut descriptors, Some(&timeout)) {
            Ok(0) => return Err(args.errno(io::Error::from_raw_os_error(libc::ETIMEDOUT))),
            Ok(_) => return Ok(()),
            Err(Errno::INTR) => {}
            Err(error) => return Err(args.os(error)),
        }
    }
}

/// Reads one whole datagram: the length is probed first so a large dump is
/// never truncated to a fixed buffer.
fn receive_datagram(args: &Args<'_>, fd: BorrowedFd<'_>) -> Result<Vec<u8>, RuntimeError> {
    let length = loop {
        let probe = RecvFlags::PEEK | RecvFlags::TRUNC | RecvFlags::DONTWAIT;
        match net::recv(fd, &mut [] as &mut [u8], probe) {
            Ok((_, length)) => break length,
            Err(Errno::INTR) => {}
            Err(error) => return Err(args.os(error)),
        }
    };
    let mut buffer = vec![0_u8; length.max(HEADER_LEN)];
    loop {
        match net::recv(fd, &mut buffer[..], RecvFlags::empty()) {
            Ok((_, received)) => {
                buffer.truncate(received.min(buffer.len()));
                return Ok(buffer);
            }
            Err(Errno::INTR) => {}
            Err(error) => return Err(args.os(error)),
        }
    }
}

fn family_id(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let name = args.str(0)?;
    if name.is_empty() || name.len() >= 16 || name.as_bytes().contains(&0) {
        return Err(args.invalid("family name must be 1 to 15 bytes without NUL"));
    }
    let socket = open_socket(args, NETLINK_GENERIC, 0)?;
    // genlmsghdr {cmd, version, reserved} then CTRL_ATTR_FAMILY_NAME.
    let attribute_len = 4 + name.len() + 1;
    let mut payload = vec![CTRL_CMD_GETFAMILY, 1, 0, 0];
    payload.extend_from_slice(&(attribute_len as u16).to_ne_bytes());
    payload.extend_from_slice(&CTRL_ATTR_FAMILY_NAME.to_ne_bytes());
    payload.extend_from_slice(name.as_bytes());
    payload.push(0);
    payload.resize(payload.len().next_multiple_of(4), 0);
    let replies = exchange(
        args,
        socket.as_fd(),
        GENL_ID_CTRL,
        0x1,
        &payload,
        0,
        Duration::from_millis(DEFAULT_TIMEOUT_MS as u64),
    )?;
    for reply in replies {
        let mut attributes = reply.payload.get(4..).unwrap_or_default();
        while attributes.len() >= 4 {
            let length = u16::from_ne_bytes([attributes[0], attributes[1]]) as usize;
            let kind = u16::from_ne_bytes([attributes[2], attributes[3]]) & 0x3fff;
            if length < 4 || length > attributes.len() {
                break;
            }
            if kind == CTRL_ATTR_FAMILY_ID && length >= 6 {
                return ok(Value::Int(i64::from(u16::from_ne_bytes([attributes[4], attributes[5]]))));
            }
            attributes = &attributes[length.next_multiple_of(4).min(attributes.len())..];
        }
    }
    Err(args.errno(io::Error::from_raw_os_error(libc::ENOENT)))
}
