//! Recorded netlink exchanges for native tests.
//!
//! Wireless hardware (and most kernel state a netlink client reads) is not
//! available where tests run, so a test fake can name a JSON Lines file of
//! recorded exchanges. Under such a fake `netlink_open`, `netlink_request`,
//! `genl_family_id`, `recvfrom`, and the socket options on a descriptor the
//! fake opened are answered from the file and never reach the kernel. The
//! script under test is the same code that runs against the real kernel; only
//! the byte source differs. Netlink payloads are bytes in the fixture, so
//! message layout stays with the script.
//!
//! The file is re-read on every call, so a test can swap responses between
//! calls. A request that matches no line fails loudly with
//! `linux-netlink-fake`, so a test cannot reach the kernel by accident. When
//! several lines match, the first wins. Binary fields are base64 text.
//!
//! `genl_family` lines: request `name`; response `id`, or `errno`.
//!
//! `netlink_request` lines: request `protocol`, `type`, `flags`, `cmd` (the
//! first payload byte, the generic-netlink command) and `payload` (exact
//! bytes), each optional and matched only when present; response `replies`,
//! a list of `{type, flags, payload}` data messages (the kernel's DONE and
//! acknowledgement framing is added implicitly), or `errno` to fail the
//! request as a negative NLMSG_ERROR would.
//!
//! `netlink_event` lines: `type`, `flags` (default 0), `payload`, and an
//! optional multicast `group` id. The events of a file are delivered in order
//! by `recvfrom` to a fake socket that joined their group (lines without a
//! group go to every socket). A line with `errno` instead of `payload` makes
//! the receive at that position fail, and once every event is consumed a
//! receive fails with EAGAIN, as a socket with a receive timeout does.
//!
//! A fake socket is a descriptor of an anonymous in-memory file, so
//! `unix.close_fd` releases it like any other. The descriptor's identity is
//! checked on every call, which keeps a recycled descriptor number that now
//! names a real socket on the real path.

use super::address::SockAddr;
use super::{Args, ok, record};
use crate::modules::RuntimeOp;
use crate::modules::bytes::{base64_decode, base64_encode};
use crate::modules::json::{parse_raw_json, raw_json_as_str, raw_json_as_u64, raw_json_get};
use crate::runtime::value::{RuntimeError, Value};
use miniserde::json::Value as JsonValue;
use rustix::fs::{MemfdFlags, memfd_create};
use rustix::fd::IntoRawFd;
use std::collections::BTreeMap;
use std::io;
use std::path::Path;
use std::sync::Mutex;

const KIND: &str = "linux-netlink-fake";
const MEMFD_NAME: &str = "xsh-netlink-fake";
const HEADER_LEN: usize = 16;
const SOL_NETLINK: i64 = 270;
const NETLINK_ADD_MEMBERSHIP: i64 = 1;
const NETLINK_DROP_MEMBERSHIP: i64 = 2;
const MSG_TRUNC: i64 = 0x20;
const MAX_RECEIVE: i64 = 64 * 1024 * 1024;

const FAMILY_KEYS: &[&str] = &["op", "name", "id", "errno"];
const REQUEST_KEYS: &[&str] = &["op", "protocol", "type", "flags", "cmd", "payload", "replies", "errno"];
const EVENT_KEYS: &[&str] = &["op", "type", "flags", "payload", "group", "errno"];
const REPLY_KEYS: &[&str] = &["type", "flags", "payload"];

/// Appends one call to the fake's log; supplied by the evaluator.
pub(crate) type Log<'a> = dyn FnMut(&str, &[(&str, String)]) -> Result<(), RuntimeError> + 'a;

/// What the script did to one fake socket, which the fixture lines select on.
struct Socket {
    protocol: u32,
    groups: Vec<u32>,
    delivered: usize,
}

/// Fake sockets by descriptor number. Entries are validated against the
/// descriptor's identity before use, so a stale one is never trusted, and a
/// recycled number gets a fresh entry when `netlink_open` returns it again.
static SOCKETS: Mutex<BTreeMap<i32, Socket>> = Mutex::new(BTreeMap::new());

/// Whether `fd` is a descriptor `netlink_open` returned under the fake.
fn is_fake(fd: i32) -> bool {
    let named = std::fs::read_link(format!("/proc/self/fd/{fd}"))
        .is_ok_and(|target| target.to_string_lossy().starts_with(&format!("/memfd:{MEMFD_NAME}")));
    named && SOCKETS.lock().expect("fake socket table").contains_key(&fd)
}

fn with_socket<T>(fd: i32, change: impl FnOnce(&mut Socket) -> T) -> T {
    let mut sockets = SOCKETS.lock().expect("fake socket table");
    change(sockets.get_mut(&fd).expect("fake socket was checked"))
}

fn fault(path: &Path, line: usize, message: impl std::fmt::Display, args: &Args<'_>) -> RuntimeError {
    RuntimeError::new(KIND, format!("{}:{line}: {message}", path.display())).with_span(args.span)
}

struct Entry {
    value: JsonValue,
}

impl Entry {
    fn op(&self) -> &str {
        raw_json_get(&self.value, "op").and_then(raw_json_as_str).unwrap_or("")
    }

    fn uint(&self, key: &str) -> Option<u64> {
        raw_json_get(&self.value, key).and_then(raw_json_as_u64)
    }

    fn text(&self, key: &str) -> Option<&str> {
        raw_json_get(&self.value, key).and_then(raw_json_as_str)
    }

    fn bytes(&self, key: &str) -> Option<Vec<u8>> {
        self.text(key).map(|text| base64_decode(text).unwrap_or_default())
    }

    fn errno(&self) -> Option<i32> {
        self.uint("errno").map(|errno| errno as i32)
    }
}

fn load(path: &Path, args: &Args<'_>) -> Result<Vec<Entry>, RuntimeError> {
    let text = std::fs::read_to_string(path).map_err(|error| {
        RuntimeError::new(KIND, format!("{}: {error}", path.display())).with_span(args.span)
    })?;
    let mut entries = Vec::new();
    for (index, text) in text.lines().enumerate() {
        if text.trim().is_empty() {
            continue;
        }
        let line = index + 1;
        let value = parse_raw_json(text).map_err(|error| fault(path, line, error, args))?;
        let JsonValue::Object(object) = &value else {
            return Err(fault(path, line, "each line must be a JSON object", args));
        };
        let op = raw_json_get(&value, "op")
            .and_then(raw_json_as_str)
            .ok_or_else(|| fault(path, line, "missing `op`", args))?;
        let allowed = match op {
            "genl_family" => FAMILY_KEYS,
            "netlink_request" => REQUEST_KEYS,
            "netlink_event" => EVENT_KEYS,
            other => return Err(fault(path, line, format!("unknown op `{other}`"), args)),
        };
        for (key, field) in object.iter() {
            if !allowed.contains(&key.as_str()) {
                return Err(fault(path, line, format!("unknown field `{key}` for {op}"), args));
            }
            let binary = key == "payload";
            if binary && !raw_json_as_str(field).is_some_and(|text| base64_decode(text).is_ok()) {
                return Err(fault(path, line, "`payload` must be base64 text", args));
            }
        }
        if let Some(JsonValue::Array(replies)) = raw_json_get(&value, "replies") {
            for reply in replies.iter() {
                let JsonValue::Object(fields) = reply else {
                    return Err(fault(path, line, "each reply must be a JSON object", args));
                };
                for (key, field) in fields.iter() {
                    if !REPLY_KEYS.contains(&key.as_str()) {
                        return Err(fault(path, line, format!("unknown reply field `{key}`"), args));
                    }
                    if key == "payload"
                        && !raw_json_as_str(field).is_some_and(|text| base64_decode(text).is_ok())
                    {
                        return Err(fault(path, line, "reply `payload` must be base64 text", args));
                    }
                }
            }
        }
        entries.push(Entry { value });
    }
    Ok(entries)
}

fn no_match(path: &Path, what: String, args: &Args<'_>) -> RuntimeError {
    RuntimeError::new(KIND, format!("{}: no recorded response for {what}", path.display()))
        .with_span(args.span)
}

fn os_error(args: &Args<'_>, errno: i32) -> RuntimeError {
    args.errno(io::Error::from_raw_os_error(errno))
}

/// Runs `op` against the fixture, or `None` when it names a descriptor the
/// fake did not open, which the real primitive then handles.
pub(super) fn call(
    op: RuntimeOp,
    args: &Args<'_>,
    fixture: &Path,
    log: &mut Log<'_>,
) -> Option<Result<Value, RuntimeError>> {
    match op {
        RuntimeOp::LinuxNetlinkOpen => Some(open(args, log)),
        RuntimeOp::LinuxGenlFamilyId => Some(family_id(args, fixture, log)),
        RuntimeOp::LinuxNetlinkRequest
        | RuntimeOp::LinuxRecvfrom
        | RuntimeOp::LinuxSetsockoptInt
        | RuntimeOp::LinuxSetSocketTimeout => {
            let fd = args.fd(0).ok()?;
            if !is_fake(fd) {
                return None;
            }
            Some(match op {
                RuntimeOp::LinuxNetlinkRequest => request(args, fd, fixture, log),
                RuntimeOp::LinuxRecvfrom => receive(args, fd, fixture, log),
                RuntimeOp::LinuxSetsockoptInt => option(args, fd, log),
                _ => timeout(args, fd, log),
            })
        }
        _ => None,
    }
}

fn open(args: &Args<'_>, log: &mut Log<'_>) -> Result<Value, RuntimeError> {
    let protocol: u32 = args.int_in(0, "protocol")?;
    let groups: u32 = args.int_in_or(1, 0, "groups")?;
    let file = memfd_create(MEMFD_NAME, MemfdFlags::CLOEXEC).map_err(|error| args.os(error))?;
    let fd = file.into_raw_fd();
    log("netlink_open", &[("protocol", protocol.to_string()), ("groups", groups.to_string())])?;
    // Bit n of the legacy group mask is multicast group n + 1.
    let joined = (0..32).filter(|bit| groups & (1 << bit) != 0).map(|bit| bit + 1).collect();
    SOCKETS
        .lock()
        .expect("fake socket table")
        .insert(fd, Socket { protocol, groups: joined, delivered: 0 });
    ok(Value::Int(i64::from(fd)))
}

fn family_id(args: &Args<'_>, fixture: &Path, log: &mut Log<'_>) -> Result<Value, RuntimeError> {
    let name = args.str(0)?;
    if name.is_empty() || name.len() >= 16 || name.as_bytes().contains(&0) {
        return Err(args.invalid("family name must be 1 to 15 bytes without NUL"));
    }
    log("genl_family_id", &[("name", name.to_string())])?;
    let entry = load(fixture, args)?
        .into_iter()
        .find(|entry| entry.op() == "genl_family" && entry.text("name") == Some(name))
        .ok_or_else(|| no_match(fixture, format!("generic-netlink family `{name}`"), args))?;
    if let Some(errno) = entry.errno() {
        return Err(os_error(args, errno));
    }
    match entry.uint("id") {
        Some(id) => ok(Value::Int(id as i64)),
        None => Err(RuntimeError::new(
            KIND,
            format!("{}: genl_family line for `{name}` has neither `id` nor `errno`", fixture.display()),
        )
        .with_span(args.span)),
    }
}

fn request(args: &Args<'_>, fd: i32, fixture: &Path, log: &mut Log<'_>) -> Result<Value, RuntimeError> {
    let kind: u16 = args.int_in(1, "message_type")?;
    let flags: u16 = args.int_in(2, "flags")?;
    let payload = args.bytes(3)?;
    let seq: u32 = args.int_in_or(4, 0, "seq")?;
    if args.int_or(5, 0)? < 0 {
        return Err(args.invalid("timeout_ms must be non-negative"));
    }
    log(
        "netlink_request",
        &[
            ("fd", fd.to_string()),
            ("type", kind.to_string()),
            ("flags", flags.to_string()),
            ("payload", base64_encode(payload)),
        ],
    )?;
    let protocol = with_socket(fd, |socket| socket.protocol);
    let command = payload.first().copied();
    let entry = load(fixture, args)?
        .into_iter()
        .find(|entry| {
            entry.op() == "netlink_request"
                && entry.uint("protocol").is_none_or(|value| value == u64::from(protocol))
                && entry.uint("type").is_none_or(|value| value == u64::from(kind))
                && entry.uint("flags").is_none_or(|value| value == u64::from(flags))
                && entry.uint("cmd").is_none_or(|value| Some(value) == command.map(u64::from))
                && entry.bytes("payload").is_none_or(|value| value == payload)
        })
        .ok_or_else(|| {
            no_match(
                fixture,
                format!(
                    "netlink request protocol {protocol} type {kind} flags {flags} cmd {}",
                    command.map_or_else(|| "none".to_string(), |byte| byte.to_string())
                ),
                args,
            )
        })?;
    if let Some(errno) = entry.errno() {
        return Err(os_error(args, errno));
    }
    let reply_seq = if seq == 0 { 1 } else { seq };
    let mut messages = Vec::new();
    if let Some(JsonValue::Array(replies)) = raw_json_get(&entry.value, "replies") {
        for reply in replies.iter() {
            let field = |key: &str| raw_json_get(reply, key);
            let number = |key: &str| field(key).and_then(raw_json_as_u64).unwrap_or(0) as i64;
            let data = field("payload")
                .and_then(raw_json_as_str)
                .map(|text| base64_decode(text).unwrap_or_default())
                .unwrap_or_default();
            messages.push(record([
                ("type", Value::Int(number("type"))),
                ("flags", Value::Int(number("flags"))),
                ("seq", Value::Int(i64::from(reply_seq))),
                ("pid", Value::Int(0)),
                ("payload", Value::Bytes(data)),
            ]));
        }
    }
    ok(Value::List(messages))
}

fn receive(args: &Args<'_>, fd: i32, fixture: &Path, log: &mut Log<'_>) -> Result<Value, RuntimeError> {
    let max = args.int(1)?;
    if !(1..=MAX_RECEIVE).contains(&max) {
        return Err(args.invalid(format!("max_bytes must be between 1 and {MAX_RECEIVE}")));
    }
    log("recvfrom", &[("fd", fd.to_string())])?;
    let (groups, delivered) = with_socket(fd, |socket| (socket.groups.clone(), socket.delivered));
    let events: Vec<Entry> = load(fixture, args)?
        .into_iter()
        .filter(|entry| entry.op() == "netlink_event")
        .filter(|entry| entry.uint("group").is_none_or(|group| groups.contains(&(group as u32))))
        .collect();
    let Some(entry) = events.get(delivered) else {
        return Err(os_error(args, libc::EAGAIN));
    };
    with_socket(fd, |socket| socket.delivered += 1);
    if let Some(errno) = entry.errno() {
        return Err(os_error(args, errno));
    }
    let body = entry.bytes("payload").unwrap_or_default();
    let kind = entry.uint("type").unwrap_or(0) as u16;
    let flags = entry.uint("flags").unwrap_or(0) as u16;
    let mut datagram = Vec::with_capacity(HEADER_LEN + body.len());
    datagram.extend_from_slice(&((HEADER_LEN + body.len()) as u32).to_ne_bytes());
    datagram.extend_from_slice(&kind.to_ne_bytes());
    datagram.extend_from_slice(&flags.to_ne_bytes());
    datagram.extend_from_slice(&0_u32.to_ne_bytes());
    datagram.extend_from_slice(&0_u32.to_ne_bytes());
    datagram.extend_from_slice(&body);
    let mut received_flags = 0;
    if datagram.len() > max as usize {
        datagram.truncate(max as usize);
        received_flags = MSG_TRUNC;
    }
    ok(record([
        ("data", Value::Bytes(datagram)),
        ("address", SockAddr::netlink(0, 0).to_value()),
        ("flags", Value::Int(received_flags)),
        ("control", Value::List(Vec::new())),
    ]))
}

fn option(args: &Args<'_>, fd: i32, log: &mut Log<'_>) -> Result<Value, RuntimeError> {
    let level = args.int(1)?;
    let name = args.int(2)?;
    let value = args.int(3)?;
    log(
        "setsockopt",
        &[
            ("fd", fd.to_string()),
            ("level", level.to_string()),
            ("option", name.to_string()),
            ("value", value.to_string()),
        ],
    )?;
    if level == SOL_NETLINK && (name == NETLINK_ADD_MEMBERSHIP || name == NETLINK_DROP_MEMBERSHIP) {
        let group = u32::try_from(value).map_err(|_| args.invalid("group must be a positive id"))?;
        with_socket(fd, |socket| {
            socket.groups.retain(|joined| *joined != group);
            if name == NETLINK_ADD_MEMBERSHIP {
                socket.groups.push(group);
            }
        });
    }
    ok(Value::Unit)
}

fn timeout(args: &Args<'_>, fd: i32, log: &mut Log<'_>) -> Result<Value, RuntimeError> {
    let milliseconds = args.int(2)?;
    log(
        "set_socket_timeout",
        &[("fd", fd.to_string()), ("timeout_ms", milliseconds.to_string())],
    )?;
    ok(Value::Unit)
}
