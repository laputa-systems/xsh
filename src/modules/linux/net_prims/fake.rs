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
//! `netlink_route_request` lines: request `type`, `flags` (as sent, with
//! `NLM_F_REQUEST` and `NLM_F_ACK` added), and `payload` (exact bytes), each
//! optional and matched only when present; response `datagrams`, the base64
//! reply datagrams the kernel would send, of which the first is read, or
//! `errno` to fail the call with that operating-system error. A line with no
//! datagrams leaves the call waiting until its deadline, so it fails with
//! `ETIMEDOUT`. The request is sent with sequence 1 from port id 4242, so a
//! reply is valid only when it carries those numbers, and no socket is opened.
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
use super::netlink::{acknowledgement_value, route_request_arguments};
use super::{Args, ok, record};
use crate::modules::RuntimeOp;
use crate::modules::bytes::{base64_decode, base64_encode};
use crate::modules::linux::real::{self, RouteRequestError};

use crate::runtime::value::{RuntimeError, Value};
use xsh::host::json::Value as JsonValue;
use rustix::fs::{MemfdFlags, memfd_create};
use rustix::fd::IntoRawFd;
use std::collections::BTreeMap;
use std::io;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

const KIND: &str = "linux-netlink-fake";
const MEMFD_NAME: &str = "xsh-netlink-fake";
const HEADER_LEN: usize = 16;
const SOL_NETLINK: i64 = 270;
const NETLINK_ADD_MEMBERSHIP: i64 = 1;
const NETLINK_DROP_MEMBERSHIP: i64 = 2;
const MSG_TRUNC: i64 = 0x20;
const MAX_RECEIVE: i64 = 64 * 1024 * 1024;
const ROUTE_REQUEST_SEQUENCE: u32 = 1;
const ROUTE_REQUEST_PORT_ID: u32 = 4242;

const FAMILY_KEYS: &[&str] = &["op", "name", "id", "errno"];
const REQUEST_KEYS: &[&str] = &["op", "protocol", "type", "flags", "cmd", "payload", "replies", "errno"];
const ROUTE_REQUEST_KEYS: &[&str] = &["op", "type", "flags", "payload", "datagrams", "errno"];
const NETWORK_DUMP_KEYS: &[&str] = &["op", "links", "addresses", "routes", "rules"];
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
        self.value.get("op").and_then(|value| value.as_str()).unwrap_or("")
    }

    fn uint(&self, key: &str) -> Option<u64> {
        self.value.get(key).and_then(|value| value.as_u64())
    }

    fn text(&self, key: &str) -> Option<&str> {
        self.value.get(key).and_then(|value| value.as_str())
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
        let value = xsh::host::json::parse(text).map_err(|error| fault(path, line, error, args))?;
        let Some(object) = value.as_map() else {
            return Err(fault(path, line, "each line must be a JSON object", args));
        };
        let op = value.get("op")
            .and_then(|value| value.as_str())
            .ok_or_else(|| fault(path, line, "missing `op`", args))?;
        let allowed = match op {
            "genl_family" => FAMILY_KEYS,
            "netlink_request" => REQUEST_KEYS,
            "netlink_route_request" => ROUTE_REQUEST_KEYS,
            "netlink_event" => EVENT_KEYS,
            "network_dump" => NETWORK_DUMP_KEYS,
            other => return Err(fault(path, line, format!("unknown op `{other}`"), args)),
        };
        for (key, field) in object.iter() {
            let key = key.as_str().expect("JSON object keys are strings");
            if !allowed.contains(&key) {
                return Err(fault(path, line, format!("unknown field `{key}` for {op}"), args));
            }
            let binary = key == "payload";
            if binary && !(field).as_str().is_some_and(|text| base64_decode(text).is_ok()) {
                return Err(fault(path, line, "`payload` must be base64 text", args));
            }
        }
        if let Some(datagrams) = value.get("datagrams") {
            let valid = datagrams.as_seq().is_some_and(|items| {
                items
                    .iter()
                    .all(|item| item.as_str().is_some_and(|text| base64_decode(text).is_ok()))
            });
            if !valid {
                return Err(fault(path, line, "`datagrams` must be a list of base64 text", args));
            }
        }
        if op == "network_dump" {
            for key in &NETWORK_DUMP_KEYS[1..] {
                let valid = match value.get(key) {
                    Some(reply) => {
                        reply.as_u64().is_some()
                            || reply.as_seq().is_some_and(|items| {
                                items.iter().all(|item| {
                                    item.as_str().is_some_and(|text| base64_decode(text).is_ok())
                                })
                            })
                    }
                    None => false,
                };
                if !valid {
                    return Err(fault(
                        path,
                        line,
                        format!("`{key}` must be an errno or a list of base64 text"),
                        args,
                    ));
                }
            }
        }
        if let Some(replies) = value.get("replies").and_then(|value| value.as_seq()) {
            for reply in replies.iter() {
                let Some(fields) = (reply).as_map() else {
                    return Err(fault(path, line, "each reply must be a JSON object", args));
                };
                for (key, field) in fields.iter() {
                    let key = key.as_str().expect("JSON object keys are strings");
                    if !REPLY_KEYS.contains(&key) {
                        return Err(fault(path, line, format!("unknown reply field `{key}`"), args));
                    }
                    if key == "payload"
                        && !(field).as_str().is_some_and(|text| base64_decode(text).is_ok())
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
        RuntimeOp::LinuxNetlinkRouteRequest => Some(route_request(args, fixture, log)),
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
    if let Some(replies) = entry.value.get("replies").and_then(|value| value.as_seq()) {
        for reply in replies.iter() {
            let field = |key: &str| (reply).get(key);
            let number = |key: &str| field(key).and_then(|value| value.as_u64()).unwrap_or(0) as i64;
            let data = field("payload")
                .and_then(|value| value.as_str())
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

fn route_request(args: &Args<'_>, fixture: &Path, log: &mut Log<'_>) -> Result<Value, RuntimeError> {
    let request = logged_route_request(args, log)?;
    let entry = load(fixture, args)?
        .into_iter()
        .find(|entry| {
            entry.op() == "netlink_route_request"
                && entry.uint("type").is_none_or(|value| value == u64::from(request.kind()))
                && entry.uint("flags").is_none_or(|value| value == u64::from(request.flags()))
                && entry.bytes("payload").is_none_or(|value| value == request.payload())
        })
        .ok_or_else(|| {
            no_match(
                fixture,
                format!(
                    "route-netlink request type {} flags {}",
                    request.kind(),
                    request.flags()
                ),
                args,
            )
        })?;
    route_reply(&entry, &request, args)
}

fn logged_route_request<'a>(
    args: &Args<'a>,
    log: &mut Log<'_>,
) -> Result<real::RouteRequest<'a>, RuntimeError> {
    let (request, _) = route_request_arguments(args)?;
    log(
        "netlink_route_request",
        &[
            ("type", request.kind().to_string()),
            ("flags", request.flags().to_string()),
            ("payload", base64_encode(request.payload())),
            ("sequence", ROUTE_REQUEST_SEQUENCE.to_string()),
            ("port_id", ROUTE_REQUEST_PORT_ID.to_string()),
        ],
    )?;
    Ok(request)
}

fn route_reply(
    entry: &Entry,
    request: &real::RouteRequest<'_>,
    args: &Args<'_>,
) -> Result<Value, RuntimeError> {
    if let Some(errno) = entry.errno() {
        return Err(os_error(args, errno));
    }
    let mut datagrams = entry.value
        .get("datagrams")
        .and_then(|value| value.as_seq())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().and_then(|text| base64_decode(text).ok()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
        .into_iter();
    let result = real::acknowledge(request, ROUTE_REQUEST_SEQUENCE, ROUTE_REQUEST_PORT_ID, || {
        datagrams
            .next()
            .ok_or_else(|| RouteRequestError::Io(io::Error::from_raw_os_error(libc::ETIMEDOUT)))
    });
    acknowledgement_value(args, result)
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
    let size = datagram.len();
    let mut received_flags = 0;
    if datagram.len() > max as usize {
        datagram.truncate(max as usize);
        received_flags = MSG_TRUNC;
    }
    ok(record([
        ("data", Value::Bytes(datagram)),
        ("size", Value::Int(size as i64)),
        ("address", SockAddr::netlink(0, 0).to_value()),
        ("flags", Value::Int(received_flags)),
        ("control", Value::List(Vec::new())),
        ("rights_closed", Value::Int(0)),
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

const REPLAY_KIND: &str = "linux-netlink-replay";

fn replay_fault(args: &Args<'_>, message: String) -> RuntimeError {
    RuntimeError::new(REPLAY_KIND, message).with_span(args.span)
}

fn next_observation(
    fixture: &Path,
    next: &AtomicUsize,
    observed: &str,
    args: &Args<'_>,
) -> Result<Entry, RuntimeError> {
    let entries = load(fixture, args)?;
    let total = entries.len();
    let position = next.fetch_add(1, Ordering::SeqCst);
    let entry = entries.into_iter().nth(position).ok_or_else(|| {
        replay_fault(
            args,
            format!("`{observed}` after all {total} replayed observations were consumed"),
        )
    })?;
    if entry.op() != observed {
        return Err(replay_fault(
            args,
            format!(
                "replay observation {}: expected `{}`, observed `{observed}`",
                position + 1,
                entry.op()
            ),
        ));
    }
    Ok(entry)
}

pub(super) fn replay_call(
    op: RuntimeOp,
    args: &Args<'_>,
    fixture: &Path,
    next: &AtomicUsize,
    log: &mut Log<'_>,
) -> Result<Value, RuntimeError> {
    if op != RuntimeOp::LinuxNetlinkRouteRequest {
        return Err(replay_fault(
            args,
            format!("{} would reach the host; a network replay covers only netlink_route_request and network_dump", args.kind),
        ));
    }
    let request = logged_route_request(args, log)?;
    let position = next.load(Ordering::SeqCst) + 1;
    let entry = next_observation(fixture, next, "netlink_route_request", args)?;
    let matches = entry.uint("type").is_none_or(|value| value == u64::from(request.kind()))
        && entry.uint("flags").is_none_or(|value| value == u64::from(request.flags()))
        && entry.bytes("payload").is_none_or(|value| value == request.payload());
    if !matches {
        return Err(replay_fault(
            args,
            format!(
                "replay observation {position}: request type {} flags {} does not match the recorded request",
                request.kind(),
                request.flags()
            ),
        ));
    }
    route_reply(&entry, &request, args)
}

pub(super) fn replay_network_dump(
    args: &Args<'_>,
    fixture: &Path,
    next: &AtomicUsize,
    log: &mut Log<'_>,
) -> Result<Value, RuntimeError> {
    log("network_dump", &[])?;
    let entry = next_observation(fixture, next, "network_dump", args)?;
    let reply = |key: &str| match entry.uint(key) {
        Some(errno) => real::ReplayedDump::Errno(errno as i32),
        None => real::ReplayedDump::Datagrams(
            entry
                .value
                .get(key)
                .and_then(|value| value.as_seq())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str().and_then(|text| base64_decode(text).ok()))
                        .collect()
                })
                .unwrap_or_default(),
        ),
    };
    Ok(real::replayed_network_dump(
        [reply("links"), reply("addresses"), reply("routes"), reply("rules")],
        ROUTE_REQUEST_PORT_ID,
    ))
}

pub(crate) fn unconsumed(fixture: &Path, consumed: usize) -> Result<(), String> {
    let text = std::fs::read_to_string(fixture).map_err(|error| format!("{}: {error}", fixture.display()))?;
    let total = text.lines().filter(|line| !line.trim().is_empty()).count();
    if consumed < total {
        return Err(format!(
            "replay incomplete: {consumed} of {total} observations were consumed"
        ));
    }
    Ok(())
}
