use crate::runtime::value::{RecordMap, RuntimeError, Value};
use crate::source::Span;
use rustix::net::netlink::SocketAddrNetlink;
use rustix::net::{
    AddressFamily, RecvFlags, SendFlags, SocketFlags, SocketType, bind, recvfrom, sendto,
    socket_with,
};
use std::io;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::os::fd::OwnedFd;
use std::sync::Arc;
use std::time::{Duration, Instant};

const NLMSG_HEADER_LEN: usize = 16;
const NLA_HEADER_LEN: usize = 4;
const NLMSG_NOOP: u16 = 1;
const NLMSG_ERROR: u16 = 2;
const NLMSG_DONE: u16 = 3;
const NLMSG_OVERRUN: u16 = 4;
const NLM_F_REQUEST: u16 = 0x0001;
const NLM_F_MULTI: u16 = 0x0002;
const NLM_F_ACK: u16 = 0x0004;
const NLM_F_DUMP_INTR: u16 = 0x0010;
const NLM_F_DUMP: u16 = 0x0300;
const RTM_NEWLINK: u16 = 16;
const RTM_GETLINK: u16 = 18;
const RTM_NEWADDR: u16 = 20;
const RTM_GETADDR: u16 = 22;
const RTM_NEWROUTE: u16 = 24;
const RTM_GETROUTE: u16 = 26;
const RTM_NEWRULE: u16 = 32;
const RTM_GETRULE: u16 = 34;
const MAX_DATAGRAM_BYTES: usize = 1024 * 1024;
const MAX_DUMP_BYTES: usize = 8 * 1024 * 1024;
const MAX_DUMP_MESSAGES: usize = 65_536;
const MAX_DUMP_DATAGRAMS: usize = 4096;
const DUMP_DEADLINE: Duration = Duration::from_secs(3);
const DUMP_ATTEMPTS: usize = 2;
const MAX_JSON_SAFE_INT: u64 = 9_007_199_254_740_991;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DumpKind {
    Links,
    Addresses,
    Routes,
    Rules,
}

impl DumpKind {
    const fn request_type(self) -> u16 {
        match self {
            Self::Links => RTM_GETLINK,
            Self::Addresses => RTM_GETADDR,
            Self::Routes => RTM_GETROUTE,
            Self::Rules => RTM_GETRULE,
        }
    }

    const fn response_type(self) -> u16 {
        match self {
            Self::Links => RTM_NEWLINK,
            Self::Addresses => RTM_NEWADDR,
            Self::Routes => RTM_NEWROUTE,
            Self::Rules => RTM_NEWRULE,
        }
    }

    const fn payload_len(self) -> usize {
        match self {
            Self::Links => 16,
            Self::Addresses => 8,
            Self::Routes | Self::Rules => 12,
        }
    }
}

#[derive(Clone, Debug)]
struct NetlinkMessage {
    payload: Vec<u8>,
}

#[derive(Debug)]
struct DumpAccumulator {
    sequence: u32,
    local_port: u32,
    request_type: u16,
    expected_type: u16,
    byte_count: usize,
    message_count: usize,
    datagram_count: usize,
    complete: bool,
    interrupted: bool,
    messages: Vec<NetlinkMessage>,
}

impl DumpAccumulator {
    fn new(sequence: u32, local_port: u32, request_type: u16, expected_type: u16) -> Self {
        Self {
            sequence,
            local_port,
            request_type,
            expected_type,
            byte_count: 0,
            message_count: 0,
            datagram_count: 0,
            complete: false,
            interrupted: false,
            messages: Vec::new(),
        }
    }

    fn push_datagram(&mut self, bytes: &[u8]) -> Result<(), DumpError> {
        if self.complete {
            return Err(DumpError::Malformed(
                "data arrived after the dump terminator",
            ));
        }
        self.datagram_count += 1;
        self.byte_count = self
            .byte_count
            .checked_add(bytes.len())
            .ok_or(DumpError::Limit("netlink dump byte count overflowed"))?;
        if self.datagram_count > MAX_DUMP_DATAGRAMS || self.byte_count > MAX_DUMP_BYTES {
            return Err(DumpError::Limit("netlink dump exceeded its size limit"));
        }

        let mut cursor = 0;
        while cursor < bytes.len() {
            let remaining = &bytes[cursor..];
            if self.complete {
                if remaining.iter().all(|byte| *byte == 0) {
                    break;
                }
                return Err(DumpError::Malformed(
                    "data followed the netlink dump terminator",
                ));
            }
            if remaining.len() < NLMSG_HEADER_LEN {
                if remaining.iter().all(|byte| *byte == 0) {
                    break;
                }
                return Err(DumpError::Malformed("netlink message header is truncated"));
            }
            let length = read_u32(remaining, 0)
                .ok_or(DumpError::Malformed("netlink message length is missing"))?
                as usize;
            if length < NLMSG_HEADER_LEN || length > remaining.len() {
                return Err(DumpError::Malformed("netlink message length is invalid"));
            }
            let message_type = read_u16(remaining, 4)
                .ok_or(DumpError::Malformed("netlink message type is missing"))?;
            let flags = read_u16(remaining, 6)
                .ok_or(DumpError::Malformed("netlink message flags are missing"))?;
            let sequence = read_u32(remaining, 8)
                .ok_or(DumpError::Malformed("netlink sequence is missing"))?;
            let port = read_u32(remaining, 12)
                .ok_or(DumpError::Malformed("netlink port id is missing"))?;
            if sequence != self.sequence {
                return Err(DumpError::Malformed(
                    "netlink response sequence does not match",
                ));
            }
            if port != self.local_port {
                return Err(DumpError::Malformed(
                    "netlink response port id does not match",
                ));
            }
            if flags & NLM_F_DUMP_INTR != 0 {
                self.interrupted = true;
            }

            self.message_count += 1;
            if self.message_count > MAX_DUMP_MESSAGES {
                return Err(DumpError::Limit("netlink dump exceeded its message limit"));
            }
            let payload = &remaining[NLMSG_HEADER_LEN..length];
            match message_type {
                NLMSG_NOOP => {}
                NLMSG_OVERRUN => {
                    return Err(DumpError::Truncated(
                        "kernel reported a netlink receive overrun",
                    ));
                }
                NLMSG_ERROR => {
                    if payload.len() < 20 {
                        return Err(DumpError::Malformed(
                            "netlink error acknowledgment is truncated",
                        ));
                    }
                    let error = read_i32(payload, 0)
                        .ok_or(DumpError::Malformed("netlink error payload is truncated"))?;
                    let request_type = read_u16(payload, 8)
                        .ok_or(DumpError::Malformed("acknowledged request type is missing"))?;
                    let request_sequence = read_u32(payload, 12).ok_or(DumpError::Malformed(
                        "acknowledged request sequence is missing",
                    ))?;
                    let request_port = read_u32(payload, 16)
                        .ok_or(DumpError::Malformed("acknowledged request port is missing"))?;
                    if request_type != self.request_type
                        || request_sequence != self.sequence
                        || request_port != self.local_port
                    {
                        return Err(DumpError::Malformed(
                            "netlink acknowledgment does not match its request",
                        ));
                    }
                    if error > 0 {
                        return Err(DumpError::Malformed("netlink error code is positive"));
                    }
                    if error != 0 {
                        let errno = error.checked_neg().ok_or(DumpError::Malformed(
                            "netlink error code is outside the supported range",
                        ))?;
                        return Err(DumpError::Kernel(io::Error::from_raw_os_error(errno)));
                    }
                }
                NLMSG_DONE => {
                    if !payload.is_empty() {
                        if payload.len() < 4 {
                            return Err(DumpError::Malformed("netlink dump status is truncated"));
                        }
                        let error = read_i32(payload, 0)
                            .ok_or(DumpError::Malformed("netlink dump status is truncated"))?;
                        if error > 0 {
                            return Err(DumpError::Malformed("netlink dump status is positive"));
                        }
                        if error != 0 {
                            let errno = error.checked_neg().ok_or(DumpError::Malformed(
                                "netlink dump status is outside the supported range",
                            ))?;
                            return Err(DumpError::Kernel(io::Error::from_raw_os_error(errno)));
                        }
                    }
                    self.complete = true;
                }
                kind if kind == self.expected_type => {
                    if flags & NLM_F_MULTI == 0 {
                        return Err(DumpError::Malformed(
                            "netlink dump item is missing the multipart flag",
                        ));
                    }
                    self.messages.push(NetlinkMessage {
                        payload: payload.to_vec(),
                    });
                }
                _ => {
                    return Err(DumpError::Malformed(
                        "unexpected message type in netlink dump",
                    ));
                }
            }

            let aligned = align4(length)
                .ok_or(DumpError::Malformed("netlink message alignment overflowed"))?;
            if aligned > remaining.len() {
                if length == remaining.len() {
                    cursor = bytes.len();
                } else {
                    return Err(DumpError::Malformed("netlink message padding is truncated"));
                }
            } else {
                cursor += aligned;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct Attribute {
    raw_kind: u16,
    kind: u16,
    data: Vec<u8>,
}

#[derive(Clone, Debug, Default)]
struct Link {
    ifindex: i32,
    name: Option<Vec<u8>>,
    hardware_type: u16,
    flags: u32,
    mtu: Option<u32>,
    address: Option<Vec<u8>>,
    broadcast: Option<Vec<u8>>,
    master_ifindex: Option<u32>,
    lower_ifindex: Option<u32>,
    operstate: Option<u8>,
    kind: Option<Vec<u8>>,
    rx_bytes: Option<u64>,
    tx_bytes: Option<u64>,
    attributes: Vec<Attribute>,
}

#[derive(Clone, Debug, Default)]
struct Address {
    ifindex: u32,
    family: u8,
    prefix_length: u8,
    scope: u8,
    flags: u32,
    address: Option<Vec<u8>>,
    local: Option<Vec<u8>>,
    broadcast: Option<Vec<u8>>,
    label: Option<Vec<u8>>,
    preferred_lifetime: Option<u32>,
    valid_lifetime: Option<u32>,
    attributes: Vec<Attribute>,
}

#[derive(Clone, Debug, Default)]
struct Nexthop {
    ifindex: i32,
    flags: u8,
    hops: u8,
    gateway: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Default)]
struct Route {
    family: u8,
    destination_length: u8,
    source_length: u8,
    table: u32,
    protocol: u8,
    scope: u8,
    route_type: u8,
    flags: u32,
    destination: Option<Vec<u8>>,
    source: Option<Vec<u8>>,
    gateway: Option<Vec<u8>>,
    preferred_source: Option<Vec<u8>>,
    output_ifindex: Option<u32>,
    input_ifindex: Option<u32>,
    priority: Option<u32>,
    nexthops: Vec<Nexthop>,
    attributes: Vec<Attribute>,
}

#[derive(Clone, Debug, Default)]
struct Rule {
    family: u8,
    destination_length: u8,
    source_length: u8,
    table: u32,
    action: u8,
    flags: u32,
    destination: Option<Vec<u8>>,
    source: Option<Vec<u8>>,
    input_name: Option<Vec<u8>>,
    output_name: Option<Vec<u8>>,
    priority: Option<u32>,
    fwmark: Option<u32>,
    fwmask: Option<u32>,
    attributes: Vec<Attribute>,
}

#[derive(Clone, Debug, Default)]
struct Snapshot {
    links: Vec<Link>,
    addresses: Vec<Address>,
    routes: Vec<Route>,
    rules: Vec<Rule>,
    issues: Vec<NetworkIssue>,
    successful_dumps: usize,
    entity_decode_failed: bool,
}

#[derive(Clone, Debug)]
struct NetworkIssue {
    object: String,
    message: String,
    state: String,
    errno: Option<i32>,
    error_kind: String,
}

#[derive(Debug)]
enum DumpError {
    Malformed(&'static str),
    Limit(&'static str),
    Truncated(&'static str),
    Interrupted,
    Kernel(io::Error),
    Io(io::Error),
}

impl std::fmt::Display for DumpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed(message) | Self::Limit(message) | Self::Truncated(message) => {
                f.write_str(message)
            }
            Self::Interrupted => f.write_str("kernel interrupted the netlink dump"),
            Self::Kernel(error) | Self::Io(error) => write!(f, "{error}"),
        }
    }
}

impl DumpError {
    fn state(&self) -> &'static str {
        match self {
            Self::Malformed(_) => "malformed",
            Self::Limit(_) => "limited",
            Self::Truncated(_) => "truncated",
            Self::Interrupted => "interrupted",
            Self::Kernel(error) | Self::Io(error)
                if error.kind() == io::ErrorKind::PermissionDenied =>
            {
                "permission_denied"
            }
            Self::Kernel(error) | Self::Io(error)
                if matches!(
                    error.raw_os_error(),
                    Some(libc::EAFNOSUPPORT)
                        | Some(libc::EPROTONOSUPPORT)
                        | Some(libc::ENOSYS)
                        | Some(libc::EOPNOTSUPP)
                ) =>
            {
                "unsupported"
            }
            Self::Kernel(error) | Self::Io(error) if error.kind() == io::ErrorKind::TimedOut => {
                "limited"
            }
            Self::Kernel(_) => "kernel_error",
            Self::Io(_) => "io_error",
        }
    }

    fn error_kind(&self) -> &'static str {
        match self {
            Self::Malformed(_) => "malformed",
            Self::Limit(_) => "limit",
            Self::Truncated(_) => "truncated",
            Self::Interrupted => "interrupted",
            Self::Kernel(_) => "kernel",
            Self::Io(_) => "io",
        }
    }

    fn errno(&self) -> Option<i32> {
        match self {
            Self::Kernel(error) | Self::Io(error) => error.raw_os_error(),
            Self::Malformed(_) | Self::Limit(_) | Self::Truncated(_) | Self::Interrupted => None,
        }
    }
}

impl Snapshot {
    fn failed(error: DumpError, object: &str) -> Self {
        let mut snapshot = Self::default();
        snapshot.add_error(object, error);
        snapshot
    }

    fn add_error(&mut self, object: &str, error: DumpError) {
        self.issues.push(NetworkIssue {
            object: object.to_owned(),
            message: error.to_string(),
            state: error.state().to_owned(),
            errno: error.errno(),
            error_kind: error.error_kind().to_owned(),
        });
    }

    fn state(&self) -> &'static str {
        if self.issues.is_empty() && self.successful_dumps == 4 {
            "complete"
        } else if self.successful_dumps > 0 {
            "partial"
        } else {
            self.issues
                .first()
                .map_or("failed", |issue| match issue.state.as_str() {
                    "permission_denied" => "permission_denied",
                    "unsupported" => "unsupported",
                    "malformed" => "malformed",
                    "truncated" => "truncated",
                    "limited" => "limited",
                    "interrupted" => "interrupted",
                    _ => "failed",
                })
        }
    }

    fn add_messages(&mut self, kind: DumpKind, messages: &[NetlinkMessage]) {
        for message in messages {
            let parsed = match kind {
                DumpKind::Links => parse_link(&message.payload).map(|value| {
                    for (field, counter) in
                        [("rx_bytes", value.rx_bytes), ("tx_bytes", value.tx_bytes)]
                    {
                        if counter.is_some_and(|counter| counter > MAX_JSON_SAFE_INT) {
                            self.issues.push(NetworkIssue {
                                object: format!("links.{}.{}", value.ifindex, field),
                                message: "network counter exceeds the exact JSON integer range"
                                    .to_owned(),
                                state: "range_failure".to_owned(),
                                errno: None,
                                error_kind: "integer_out_of_range".to_owned(),
                            });
                        }
                    }
                    self.links.push(value);
                }),
                DumpKind::Addresses => {
                    parse_address(&message.payload).map(|value| self.addresses.push(value))
                }
                DumpKind::Routes => {
                    parse_route(&message.payload).map(|value| self.routes.push(value))
                }
                DumpKind::Rules => parse_rule(&message.payload).map(|value| self.rules.push(value)),
            };
            if let Err(message) = parsed {
                self.entity_decode_failed = true;
                self.issues.push(NetworkIssue {
                    object: kind.name().to_owned(),
                    message: message.to_owned(),
                    state: "malformed".to_owned(),
                    errno: None,
                    error_kind: "malformed".to_owned(),
                });
            }
        }
    }
}

impl DumpKind {
    const fn name(self) -> &'static str {
        match self {
            Self::Links => "link",
            Self::Addresses => "address",
            Self::Routes => "route",
            Self::Rules => "rule",
        }
    }
}

pub(crate) fn network_dump(_span: Span) -> Result<Value, RuntimeError> {
    Ok(Value::ok(snapshot_value(collect_snapshot())))
}

fn collect_snapshot() -> Snapshot {
    let socket = match socket_with(
        AddressFamily::NETLINK,
        SocketType::RAW,
        SocketFlags::CLOEXEC,
        None,
    ) {
        Ok(socket) => socket,
        Err(error) => return Snapshot::failed(DumpError::Io(io::Error::from(error)), "socket"),
    };
    if let Err(error) = bind(&socket, &SocketAddrNetlink::new(0, 0)) {
        return Snapshot::failed(DumpError::Io(io::Error::from(error)), "socket");
    }
    let local = match rustix::net::getsockname(&socket) {
        Ok(local) => local,
        Err(error) => return Snapshot::failed(DumpError::Io(io::Error::from(error)), "socket"),
    };
    let local = match SocketAddrNetlink::try_from(local) {
        Ok(local) => local,
        Err(error) => return Snapshot::failed(DumpError::Io(io::Error::from(error)), "socket"),
    };
    let mut snapshot = Snapshot::default();
    for (sequence, kind) in [
        DumpKind::Links,
        DumpKind::Addresses,
        DumpKind::Routes,
        DumpKind::Rules,
    ]
    .into_iter()
    .enumerate()
    {
        let sequence = u32::try_from(sequence + 1).expect("four netlink requests fit in u32");
        match collect_dump(&socket, sequence, local.pid(), kind) {
            Ok(messages) => {
                snapshot.successful_dumps += 1;
                snapshot.add_messages(kind, &messages);
            }
            Err(error) => snapshot.add_error(kind.name(), error),
        }
    }
    snapshot
}

fn collect_dump(
    socket: &OwnedFd,
    sequence: u32,
    local_port: u32,
    kind: DumpKind,
) -> Result<Vec<NetlinkMessage>, DumpError> {
    for attempt in 0..DUMP_ATTEMPTS {
        match collect_dump_attempt(socket, sequence, local_port, kind) {
            Err(DumpError::Interrupted) if attempt + 1 < DUMP_ATTEMPTS => continue,
            result => return result,
        }
    }
    Err(DumpError::Interrupted)
}

fn collect_dump_attempt(
    socket: &OwnedFd,
    sequence: u32,
    local_port: u32,
    kind: DumpKind,
) -> Result<Vec<NetlinkMessage>, DumpError> {
    collect_dump_attempt_recorded(socket, sequence, local_port, kind, &mut |_| {})
}

fn collect_dump_attempt_recorded(
    socket: &OwnedFd,
    sequence: u32,
    local_port: u32,
    kind: DumpKind,
    record: &mut impl FnMut(&[u8]),
) -> Result<Vec<NetlinkMessage>, DumpError> {
    let mut request = Vec::with_capacity(NLMSG_HEADER_LEN + kind.payload_len());
    request.extend_from_slice(
        &u32::try_from(NLMSG_HEADER_LEN + kind.payload_len())
            .expect("netlink request length fits u32")
            .to_ne_bytes(),
    );
    request.extend_from_slice(&kind.request_type().to_ne_bytes());
    request.extend_from_slice(&(NLM_F_REQUEST | NLM_F_ACK | NLM_F_DUMP).to_ne_bytes());
    request.extend_from_slice(&sequence.to_ne_bytes());
    request.extend_from_slice(&local_port.to_ne_bytes());
    request.resize(NLMSG_HEADER_LEN + kind.payload_len(), 0);
    let sent = sendto(
        socket,
        &request,
        SendFlags::empty(),
        &SocketAddrNetlink::new(0, 0),
    )
    .map_err(|error| DumpError::Io(io::Error::from(error)))?;
    if sent != request.len() {
        return Err(DumpError::Io(io::Error::new(
            io::ErrorKind::WriteZero,
            "netlink request was only partially sent",
        )));
    }

    let deadline = Instant::now() + DUMP_DEADLINE;
    let mut accumulator = DumpAccumulator::new(
        sequence,
        local_port,
        kind.request_type(),
        kind.response_type(),
    );
    let mut buffer = vec![0_u8; MAX_DATAGRAM_BYTES];
    while !accumulator.complete {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(DumpError::Io(io::Error::new(
                io::ErrorKind::TimedOut,
                "netlink dump exceeded its three-second deadline",
            )));
        }
        rustix::net::sockopt::set_socket_timeout(
            socket,
            rustix::net::sockopt::Timeout::Recv,
            Some(remaining),
        )
        .map_err(|error| DumpError::Io(io::Error::from(error)))?;
        let (received, datagram_len, sender) = recvfrom(socket, &mut buffer[..], RecvFlags::TRUNC)
            .map_err(|error| DumpError::Io(io::Error::from(error)))?;
        if datagram_len > buffer.len() {
            return Err(DumpError::Truncated(
                "kernel netlink datagram was truncated",
            ));
        }
        let sender = sender.ok_or(DumpError::Malformed("netlink sender address is missing"))?;
        let sender = SocketAddrNetlink::try_from(sender)
            .map_err(|_| DumpError::Malformed("netlink sender address has the wrong family"))?;
        if sender.pid() != 0 || sender.groups() != 0 {
            return Err(DumpError::Malformed(
                "netlink response sender is not the kernel",
            ));
        }
        accumulator.push_datagram(&buffer[..received])?;
        record(&buffer[..received]);
    }
    if accumulator.interrupted {
        return Err(DumpError::Interrupted);
    }
    Ok(accumulator.messages)
}

fn parse_link(payload: &[u8]) -> Result<Link, &'static str> {
    if payload.len() < 16 {
        return Err("link message header is truncated");
    }
    let mut link = Link {
        hardware_type: read_u16(payload, 2).ok_or("link hardware type is missing")?,
        ifindex: read_i32(payload, 4).ok_or("link index is missing")?,
        flags: read_u32(payload, 8).ok_or("link flags are missing")?,
        attributes: parse_attributes(payload, 16)?,
        ..Link::default()
    };
    for attribute in &link.attributes {
        match attribute.kind {
            1 => link.address = Some(attribute.data.clone()),
            2 => link.broadcast = Some(attribute.data.clone()),
            3 => {
                if attribute.data.is_empty() {
                    return Err("interface name attribute is empty");
                }
                link.name = Some(attribute.data.clone());
            }
            4 => link.mtu = Some(attr_u32(attribute)?),
            5 => link.lower_ifindex = Some(attr_u32(attribute)?),
            10 => link.master_ifindex = Some(attr_u32(attribute)?),
            16 => {
                if attribute.data.len() != 1 {
                    return Err("interface operational-state attribute has an invalid size");
                }
                link.operstate = attribute.data.first().copied();
            }
            18 => {
                let nested = parse_attributes(&attribute.data, 0)?;
                if let Some(kind) = nested.iter().find(|value| value.kind == 1) {
                    link.kind = Some(kind.data.clone());
                }
            }
            23 => {
                if attribute.data.len() < 32 {
                    return Err("interface statistics attribute is truncated");
                }
                link.rx_bytes = read_u64(&attribute.data, 16);
                link.tx_bytes = read_u64(&attribute.data, 24);
            }
            _ => {}
        }
    }
    Ok(link)
}

fn parse_address(payload: &[u8]) -> Result<Address, &'static str> {
    if payload.len() < 8 {
        return Err("address message header is truncated");
    }
    let mut address = Address {
        family: payload[0],
        prefix_length: payload[1],
        flags: u32::from(payload[2]),
        scope: payload[3],
        ifindex: read_u32(payload, 4).ok_or("address interface index is missing")?,
        attributes: parse_attributes(payload, 8)?,
        ..Address::default()
    };
    ensure_prefix(address.family, address.prefix_length)?;
    for attribute in &address.attributes {
        match attribute.kind {
            1 => {
                ensure_ip_size(address.family, &attribute.data)?;
                address.address = Some(attribute.data.clone());
            }
            2 => {
                ensure_ip_size(address.family, &attribute.data)?;
                address.local = Some(attribute.data.clone());
            }
            3 => address.label = Some(attribute.data.clone()),
            4 => {
                ensure_ip_size(address.family, &attribute.data)?;
                address.broadcast = Some(attribute.data.clone());
            }
            6 => {
                if attribute.data.len() != 16 {
                    return Err("address cache-information attribute has an invalid size");
                }
                address.preferred_lifetime = read_u32(&attribute.data, 0);
                address.valid_lifetime = read_u32(&attribute.data, 4);
            }
            8 => address.flags = attr_u32(attribute)?,
            _ => {}
        }
    }
    Ok(address)
}

fn parse_route(payload: &[u8]) -> Result<Route, &'static str> {
    if payload.len() < 12 {
        return Err("route message header is truncated");
    }
    let mut route = Route {
        family: payload[0],
        destination_length: payload[1],
        source_length: payload[2],
        table: u32::from(payload[4]),
        protocol: payload[5],
        scope: payload[6],
        route_type: payload[7],
        flags: read_u32(payload, 8).ok_or("route flags are missing")?,
        attributes: parse_attributes(payload, 12)?,
        ..Route::default()
    };
    ensure_prefix(route.family, route.destination_length)?;
    ensure_prefix(route.family, route.source_length)?;
    for attribute in &route.attributes {
        match attribute.kind {
            1 => {
                ensure_ip_size(route.family, &attribute.data)?;
                route.destination = Some(attribute.data.clone());
            }
            2 => {
                ensure_ip_size(route.family, &attribute.data)?;
                route.source = Some(attribute.data.clone());
            }
            3 => route.input_ifindex = Some(attr_u32(attribute)?),
            4 => route.output_ifindex = Some(attr_u32(attribute)?),
            5 => {
                ensure_ip_size(route.family, &attribute.data)?;
                route.gateway = Some(attribute.data.clone());
            }
            6 => route.priority = Some(attr_u32(attribute)?),
            7 => {
                ensure_ip_size(route.family, &attribute.data)?;
                route.preferred_source = Some(attribute.data.clone());
            }
            9 => route.nexthops = parse_nexthops(route.family, &attribute.data)?,
            15 => route.table = attr_u32(attribute)?,
            _ => {}
        }
    }
    Ok(route)
}

fn parse_rule(payload: &[u8]) -> Result<Rule, &'static str> {
    if payload.len() < 12 {
        return Err("rule message header is truncated");
    }
    let mut rule = Rule {
        family: payload[0],
        destination_length: payload[1],
        source_length: payload[2],
        table: u32::from(payload[4]),
        action: payload[7],
        flags: read_u32(payload, 8).ok_or("rule flags are missing")?,
        attributes: parse_attributes(payload, 12)?,
        ..Rule::default()
    };
    ensure_prefix(rule.family, rule.destination_length)?;
    ensure_prefix(rule.family, rule.source_length)?;
    for attribute in &rule.attributes {
        match attribute.kind {
            1 => rule.destination = Some(attribute.data.clone()),
            2 => rule.source = Some(attribute.data.clone()),
            3 => rule.input_name = Some(attribute.data.clone()),
            6 => rule.priority = Some(attr_u32(attribute)?),
            10 => rule.fwmark = Some(attr_u32(attribute)?),
            15 => rule.table = attr_u32(attribute)?,
            16 => rule.fwmask = Some(attr_u32(attribute)?),
            17 => rule.output_name = Some(attribute.data.clone()),
            _ => {}
        }
    }
    Ok(rule)
}

fn parse_nexthops(family: u8, bytes: &[u8]) -> Result<Vec<Nexthop>, &'static str> {
    let mut output = Vec::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        let remaining = &bytes[cursor..];
        if remaining.len() < 8 {
            if remaining.iter().all(|byte| *byte == 0) {
                break;
            }
            return Err("multipath nexthop header is truncated");
        }
        let length = usize::from(read_u16(remaining, 0).ok_or("nexthop length is missing")?);
        if length < 8 || length > remaining.len() {
            return Err("multipath nexthop length is invalid");
        }
        let attrs = parse_attributes(&remaining[..length], 8)?;
        let mut nexthop = Nexthop {
            ifindex: read_i32(remaining, 4).ok_or("nexthop interface is missing")?,
            flags: remaining[2],
            hops: remaining[3],
            ..Nexthop::default()
        };
        if let Some(gateway) = attrs.iter().find(|value| value.kind == 5) {
            ensure_ip_size(family, &gateway.data)?;
            nexthop.gateway = Some(gateway.data.clone());
        }
        output.push(nexthop);
        let aligned = align4(length).ok_or("multipath nexthop alignment overflowed")?;
        if aligned > remaining.len() {
            if length == remaining.len() {
                cursor = bytes.len();
            } else {
                return Err("multipath nexthop padding is truncated");
            }
        } else {
            cursor += aligned;
        }
    }
    Ok(output)
}

fn parse_attributes(payload: &[u8], start: usize) -> Result<Vec<Attribute>, &'static str> {
    if start > payload.len() {
        return Err("attribute offset is outside its message");
    }
    let mut attributes = Vec::new();
    let mut cursor = start;
    while cursor < payload.len() {
        let remaining = &payload[cursor..];
        if remaining.len() < NLA_HEADER_LEN {
            if remaining.iter().all(|byte| *byte == 0) {
                break;
            }
            return Err("netlink attribute header is truncated");
        }
        let length = usize::from(read_u16(remaining, 0).ok_or("attribute length is missing")?);
        if length < NLA_HEADER_LEN || length > remaining.len() {
            return Err("netlink attribute length is invalid");
        }
        let raw_kind = read_u16(remaining, 2).ok_or("attribute type is missing")?;
        attributes.push(Attribute {
            raw_kind,
            kind: raw_kind & 0x3fff,
            data: remaining[NLA_HEADER_LEN..length].to_vec(),
        });
        let aligned = align4(length).ok_or("netlink attribute alignment overflowed")?;
        if aligned > remaining.len() {
            if length == remaining.len() {
                cursor = payload.len();
            } else {
                return Err("netlink attribute padding is truncated");
            }
        } else {
            cursor += aligned;
        }
    }
    Ok(attributes)
}

fn snapshot_value(snapshot: Snapshot) -> Value {
    let state = snapshot.state();
    let enumeration_succeeded = snapshot.successful_dumps == 4 && !snapshot.entity_decode_failed;
    let links = snapshot.links.into_iter().map(link_value).collect();
    let addresses = snapshot.addresses.into_iter().map(address_value).collect();
    let routes = snapshot.routes.into_iter().map(route_value).collect();
    let rules = snapshot.rules.into_iter().map(rule_value).collect();
    let issues = snapshot
        .issues
        .into_iter()
        .map(|issue| {
            record([
                ("object", str_value(issue.object)),
                ("message", str_value(issue.message)),
                ("state", str_value(issue.state)),
                ("errno", optional_int(issue.errno.map(i64::from))),
                ("error_kind", str_value(issue.error_kind)),
            ])
        })
        .collect();
    record([
        ("state", str_value(state)),
        ("enumeration_succeeded", Value::Bool(enumeration_succeeded)),
        ("links", Value::List(links)),
        ("addresses", Value::List(addresses)),
        ("routes", Value::List(routes)),
        ("rules", Value::List(rules)),
        ("issues", Value::List(issues)),
    ])
}

fn link_value(link: Link) -> Value {
    record([
        ("ifindex", Value::Int(i64::from(link.ifindex))),
        ("name", optional_text(link.name.as_deref())),
        ("name_bytes", optional_bytes(link.name)),
        ("hardware_type", Value::Int(i64::from(link.hardware_type))),
        ("flags", Value::Int(i64::from(link.flags))),
        ("mtu", optional_int(link.mtu.map(i64::from))),
        ("address", optional_bytes(link.address)),
        ("broadcast", optional_bytes(link.broadcast)),
        (
            "master_ifindex",
            optional_int(link.master_ifindex.map(i64::from)),
        ),
        (
            "lower_ifindex",
            optional_int(link.lower_ifindex.map(i64::from)),
        ),
        ("operstate", optional_int(link.operstate.map(i64::from))),
        ("kind", optional_text(link.kind.as_deref())),
        (
            "rx_bytes",
            optional_int(
                link.rx_bytes
                    .filter(|n| *n <= MAX_JSON_SAFE_INT)
                    .and_then(|n| i64::try_from(n).ok()),
            ),
        ),
        (
            "tx_bytes",
            optional_int(
                link.tx_bytes
                    .filter(|n| *n <= MAX_JSON_SAFE_INT)
                    .and_then(|n| i64::try_from(n).ok()),
            ),
        ),
        ("attributes", attributes_value(link.attributes)),
    ])
}

fn address_value(address: Address) -> Value {
    record([
        ("ifindex", Value::Int(i64::from(address.ifindex))),
        ("family", str_value(family_name(address.family))),
        (
            "prefix_length",
            Value::Int(i64::from(address.prefix_length)),
        ),
        ("scope", Value::Int(i64::from(address.scope))),
        ("flags", Value::Int(i64::from(address.flags))),
        ("address", optional_ip(address.family, address.address)),
        ("local", optional_ip(address.family, address.local)),
        ("broadcast", optional_ip(address.family, address.broadcast)),
        ("label", optional_text(address.label.as_deref())),
        (
            "preferred_lifetime_seconds",
            optional_int(address.preferred_lifetime.map(i64::from)),
        ),
        (
            "valid_lifetime_seconds",
            optional_int(address.valid_lifetime.map(i64::from)),
        ),
        ("attributes", attributes_value(address.attributes)),
    ])
}

fn route_value(route: Route) -> Value {
    let family = route.family;
    let nexthops = route
        .nexthops
        .into_iter()
        .map(|value| {
            record([
                ("ifindex", Value::Int(i64::from(value.ifindex))),
                ("flags", Value::Int(i64::from(value.flags))),
                ("hops", Value::Int(i64::from(value.hops))),
                ("gateway", optional_ip(family, value.gateway)),
            ])
        })
        .collect();
    record([
        ("family", str_value(family_name(route.family))),
        (
            "destination_prefix_length",
            Value::Int(i64::from(route.destination_length)),
        ),
        (
            "source_prefix_length",
            Value::Int(i64::from(route.source_length)),
        ),
        ("destination", optional_ip(route.family, route.destination)),
        ("source", optional_ip(route.family, route.source)),
        ("gateway", optional_ip(route.family, route.gateway)),
        (
            "preferred_source",
            optional_ip(route.family, route.preferred_source),
        ),
        (
            "output_ifindex",
            optional_int(route.output_ifindex.map(i64::from)),
        ),
        (
            "input_ifindex",
            optional_int(route.input_ifindex.map(i64::from)),
        ),
        ("table", Value::Int(i64::from(route.table))),
        ("priority", optional_int(route.priority.map(i64::from))),
        ("route_type", Value::Int(i64::from(route.route_type))),
        ("protocol", Value::Int(i64::from(route.protocol))),
        ("scope", Value::Int(i64::from(route.scope))),
        ("flags", Value::Int(i64::from(route.flags))),
        ("nexthops", Value::List(nexthops)),
        ("attributes", attributes_value(route.attributes)),
    ])
}

fn rule_value(rule: Rule) -> Value {
    record([
        ("family", str_value(family_name(rule.family))),
        (
            "destination_prefix_length",
            Value::Int(i64::from(rule.destination_length)),
        ),
        (
            "source_prefix_length",
            Value::Int(i64::from(rule.source_length)),
        ),
        ("destination", optional_ip(rule.family, rule.destination)),
        ("source", optional_ip(rule.family, rule.source)),
        ("input_name", optional_text(rule.input_name.as_deref())),
        ("output_name", optional_text(rule.output_name.as_deref())),
        ("priority", optional_int(rule.priority.map(i64::from))),
        ("table", Value::Int(i64::from(rule.table))),
        ("fwmark", optional_int(rule.fwmark.map(i64::from))),
        ("fwmask", optional_int(rule.fwmask.map(i64::from))),
        ("action", Value::Int(i64::from(rule.action))),
        ("flags", Value::Int(i64::from(rule.flags))),
        ("attributes", attributes_value(rule.attributes)),
    ])
}

fn attributes_value(attributes: Vec<Attribute>) -> Value {
    Value::List(
        attributes
            .into_iter()
            .map(|attribute| {
                record([
                    ("kind", Value::Int(i64::from(attribute.raw_kind))),
                    ("data", Value::Bytes(attribute.data)),
                ])
            })
            .collect(),
    )
}

fn record<const N: usize>(fields: [(&str, Value); N]) -> Value {
    Value::Record(
        fields
            .into_iter()
            .map(|(name, value)| (Arc::from(name), value))
            .collect::<RecordMap>(),
    )
}

fn optional_int(value: Option<i64>) -> Value {
    value.map_or(Value::Null, Value::Int)
}

fn optional_text(value: Option<&[u8]>) -> Value {
    value
        .and_then(|bytes| text_bytes(bytes).ok())
        .map_or(Value::Null, |value| Value::Str(Arc::from(value)))
}

fn optional_bytes(value: Option<Vec<u8>>) -> Value {
    value.map_or(Value::Null, Value::Bytes)
}

fn optional_ip(family: u8, value: Option<Vec<u8>>) -> Value {
    value
        .and_then(|bytes| ip_text(family, &bytes))
        .map_or(Value::Null, |value| Value::Str(Arc::from(value)))
}

fn str_value(value: impl Into<Arc<str>>) -> Value {
    Value::Str(value.into())
}

fn text_bytes(bytes: &[u8]) -> Result<String, std::str::Utf8Error> {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    std::str::from_utf8(&bytes[..end]).map(str::to_owned)
}

fn family_name(value: u8) -> String {
    if value == libc::AF_INET as u8 {
        "inet".to_owned()
    } else if value == libc::AF_INET6 as u8 {
        "inet6".to_owned()
    } else {
        format!("af_{value}")
    }
}

fn ip_text(family: u8, bytes: &[u8]) -> Option<String> {
    match family as i32 {
        libc::AF_INET if bytes.len() == 4 => {
            Some(Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3]).to_string())
        }
        libc::AF_INET6 if bytes.len() == 16 => {
            let raw: [u8; 16] = bytes.try_into().ok()?;
            Some(Ipv6Addr::from(raw).to_string())
        }
        _ => None,
    }
}

fn attr_u32(attribute: &Attribute) -> Result<u32, &'static str> {
    if attribute.data.len() != 4 {
        return Err("integer netlink attribute has an invalid size");
    }
    read_u32(&attribute.data, 0).ok_or("integer netlink attribute is truncated")
}

fn ensure_ip_size(family: u8, data: &[u8]) -> Result<(), &'static str> {
    match family as i32 {
        libc::AF_INET if data.len() != 4 => Err("IPv4 netlink address has an invalid size"),
        libc::AF_INET6 if data.len() != 16 => Err("IPv6 netlink address has an invalid size"),
        _ => Ok(()),
    }
}

fn ensure_prefix(family: u8, prefix_length: u8) -> Result<(), &'static str> {
    match family as i32 {
        libc::AF_INET if prefix_length > 32 => Err("IPv4 prefix length exceeds 32 bits"),
        libc::AF_INET6 if prefix_length > 128 => Err("IPv6 prefix length exceeds 128 bits"),
        _ => Ok(()),
    }
}

fn align4(value: usize) -> Option<usize> {
    value.checked_add(3).map(|value| value & !3)
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_ne_bytes(
        bytes.get(offset..offset.checked_add(2)?)?.try_into().ok()?,
    ))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_ne_bytes(
        bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn read_i32(bytes: &[u8], offset: usize) -> Option<i32> {
    Some(i32::from_ne_bytes(
        bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_ne_bytes(
        bytes.get(offset..offset.checked_add(8)?)?.try_into().ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;
    use std::io::{Read, Write};
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};

    const RAW_REPLY_MAGIC: &[u8; 8] = b"XSHNL001";
    const RAW_REPLY_MAX_BYTES: usize = 4 * MAX_DUMP_BYTES + 4 * MAX_DUMP_DATAGRAMS * 4 + 256;

    #[derive(Debug)]
    struct RawReplyReplay {
        origin: &'static str,
        captured_unix_ms: u64,
        snapshot: Snapshot,
    }

    struct RawDump {
        kind: DumpKind,
        sequence: u32,
        local_port: u32,
        datagrams: Vec<Vec<u8>>,
    }

    fn append_xsh_bytes(bytes: &[u8], source: &mut String) {
        source.push_str("bytes.from_ints([");
        for (index, byte) in bytes.iter().enumerate() {
            if index != 0 {
                source.push(',');
            }
            write!(source, "{byte}").expect("write byte literal");
        }
        source.push_str("])?");
    }

    fn append_xsh_value(value: &Value, source: &mut String) {
        match value {
            Value::Null => source.push_str("null"),
            Value::Bool(value) => source.push_str(if *value { "true" } else { "false" }),
            Value::Int(value) => write!(source, "{value}").expect("write integer literal"),
            Value::Str(value) => {
                append_xsh_bytes(value.as_bytes(), source);
                source.push_str(".utf8()?");
            }
            Value::Bytes(value) => append_xsh_bytes(value, source),
            Value::List(values) => {
                source.push('[');
                for (index, value) in values.iter().enumerate() {
                    if index != 0 {
                        source.push(',');
                    }
                    append_xsh_value(value, source);
                }
                source.push(']');
            }
            Value::Record(fields) => {
                source.push('{');
                for (index, (key, value)) in fields.iter().enumerate() {
                    assert!(
                        key.starts_with(|character: char| character.is_ascii_alphabetic())
                            && key
                                .chars()
                                .all(|character| character.is_ascii_alphanumeric()
                                    || character == '_'),
                        "netlink record key is not an XSH identifier"
                    );
                    if index != 0 {
                        source.push(',');
                    }
                    source.push_str(key);
                    source.push(':');
                    append_xsh_value(value, source);
                }
                source.push('}');
            }
            other => panic!(
                "netlink snapshot contains unsupported XSH value: {}",
                other.type_name()
            ),
        }
    }

    fn verify_xsh_network_assembly(snapshot: Snapshot) {
        let Some(binary) = std::env::var_os("XSH_NETLINK_XSH_BIN") else {
            return;
        };
        let expected_links = snapshot.links.len();
        let expected_routes = snapshot.routes.len();
        let expected_rules = snapshot.rules.len();
        let mut expected_output =
            format!("true {expected_links} {expected_routes} {expected_rules}\n");
        for link in &snapshot.links {
            let address_count = snapshot
                .addresses
                .iter()
                .filter(|address| Some(address.ifindex) == u32::try_from(link.ifindex).ok())
                .count();
            writeln!(expected_output, "{} {address_count}", link.ifindex)
                .expect("write expected link identity");
        }
        let mut source = String::from(
            "use core.lib.system_report as report_model\n\
type NetworkCollection = {status: report_model.SectionStatus, links: List[report_model.NetworkLink], routes: List[report_model.NetworkRoute], rules: List[report_model.NetworkRule], issues: List[report_model.CollectionIssue]}\n\
type NetworkAssembler = module { export pure assemble_network_dump(value: LinuxNetworkDump) -> NetworkCollection }\n\
proc main() [fs, error, io] {\n\
  let collector = module.load(p\"core/lib/system_report_live.xsh\")?.require(NetworkAssembler)?\n\
  let dump: LinuxNetworkDump = ",
        );
        append_xsh_value(&snapshot_value(snapshot), &mut source);
        source.push_str(
            "\n  let result = collector.assemble_network_dump(dump)\n\
  print f\"{result.status.enumeration_succeeded} {result.links.len()} {result.routes.len()} {result.rules.len()}\"\n\
  for link in result.links {\n\
    print f\"{link.ifindex} {link.addresses.len()}\"\n\
  }\n\
}\nmain()?\n",
        );
        let directory = tempfile::tempdir().expect("private XSH replay directory");
        let script = directory.path().join("network-assembly.xsh");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&script)
            .expect("create private XSH replay script");
        file.write_all(source.as_bytes())
            .expect("write XSH replay script");
        let output = std::process::Command::new(binary)
            .arg(&script)
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .env("XSH_MODULE_PATH", env!("CARGO_MANIFEST_DIR"))
            .output()
            .expect("run XSH network assembly");
        assert!(
            output.status.success(),
            "XSH network assembly failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8(output.stdout).expect("UTF-8 XSH assembly output"),
            expected_output,
            "XSH network assembly changed saved link identities or entity counts"
        );
    }

    fn dump_kinds() -> [DumpKind; 4] {
        [
            DumpKind::Links,
            DumpKind::Addresses,
            DumpKind::Routes,
            DumpKind::Rules,
        ]
    }

    fn take_bytes<'a>(bytes: &'a [u8], cursor: &mut usize, len: usize) -> Result<&'a [u8], String> {
        let end = (*cursor)
            .checked_add(len)
            .ok_or("raw reply offset overflowed")?;
        let item = bytes
            .get(*cursor..end)
            .ok_or("raw reply bundle is truncated")?;
        *cursor = end;
        Ok(item)
    }

    fn take_u32(bytes: &[u8], cursor: &mut usize) -> Result<u32, String> {
        let raw: [u8; 4] = take_bytes(bytes, cursor, 4)?
            .try_into()
            .expect("four bytes");
        Ok(u32::from_le_bytes(raw))
    }

    fn encode_raw_replies(dumps: &[RawDump], origin: &str, captured_unix_ms: u64) -> Vec<u8> {
        assert_eq!(dumps.len(), 4);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(RAW_REPLY_MAGIC);
        bytes.push(if cfg!(target_endian = "little") {
            b'L'
        } else {
            b'B'
        });
        bytes.push(match origin {
            "synthetic" => 0,
            "container_live" => 1,
            "physical_live" => 2,
            _ => panic!("unknown raw reply origin"),
        });
        bytes.extend_from_slice(&captured_unix_ms.to_le_bytes());
        let architecture = std::env::consts::ARCH.as_bytes();
        bytes.push(u8::try_from(architecture.len()).expect("architecture name fits in one byte"));
        bytes.extend_from_slice(architecture);
        for (index, dump) in dumps.iter().enumerate() {
            assert_eq!(dump.kind, dump_kinds()[index]);
            bytes.push(u8::try_from(index).expect("four dump kinds"));
            bytes.extend_from_slice(&dump.sequence.to_le_bytes());
            bytes.extend_from_slice(&dump.local_port.to_le_bytes());
            bytes.extend_from_slice(
                &u32::try_from(dump.datagrams.len())
                    .expect("bounded datagrams")
                    .to_le_bytes(),
            );
            for datagram in &dump.datagrams {
                bytes.extend_from_slice(
                    &u32::try_from(datagram.len())
                        .expect("bounded datagram length")
                        .to_le_bytes(),
                );
                bytes.extend_from_slice(datagram);
            }
        }
        assert!(
            bytes.len() <= RAW_REPLY_MAX_BYTES - 32,
            "raw reply bundle exceeds its size limit"
        );
        let digest = Sha256::digest(&bytes);
        bytes.extend_from_slice(&digest);
        bytes
    }

    fn replay_raw_replies(bytes: &[u8]) -> Result<RawReplyReplay, String> {
        if bytes.len() > RAW_REPLY_MAX_BYTES || bytes.len() < 32 {
            return Err("raw reply bundle size is invalid".to_owned());
        }
        let (body, saved_digest) = bytes.split_at(bytes.len() - 32);
        if Sha256::digest(body).as_slice() != saved_digest {
            return Err("raw reply bundle digest does not match".to_owned());
        }
        let mut cursor = 0;
        if take_bytes(body, &mut cursor, 8)? != RAW_REPLY_MAGIC.as_slice() {
            return Err("raw reply bundle version is unsupported".to_owned());
        }
        let endian = if cfg!(target_endian = "little") {
            b'L'
        } else {
            b'B'
        };
        if take_bytes(body, &mut cursor, 1)? != &[endian][..] {
            return Err("raw reply bundle has a different native byte order".to_owned());
        }
        let origin = match take_bytes(body, &mut cursor, 1)?[0] {
            0 => "synthetic",
            1 => "container_live",
            2 => "physical_live",
            _ => return Err("raw reply origin is invalid".to_owned()),
        };
        let captured_unix_ms = u64::from_le_bytes(
            take_bytes(body, &mut cursor, 8)?
                .try_into()
                .expect("eight bytes"),
        );
        let architecture_len = usize::from(take_bytes(body, &mut cursor, 1)?[0]);
        if take_bytes(body, &mut cursor, architecture_len)? != std::env::consts::ARCH.as_bytes() {
            return Err("raw reply bundle has a different architecture".to_owned());
        }
        let mut snapshot = Snapshot::default();
        for (index, kind) in dump_kinds().into_iter().enumerate() {
            if take_bytes(body, &mut cursor, 1)? != &[index as u8][..] {
                return Err("raw reply dump order is invalid".to_owned());
            }
            let sequence = take_u32(body, &mut cursor)?;
            let local_port = take_u32(body, &mut cursor)?;
            let count =
                usize::try_from(take_u32(body, &mut cursor)?).map_err(|error| error.to_string())?;
            if count == 0 || count > MAX_DUMP_DATAGRAMS {
                return Err("raw reply datagram count is invalid".to_owned());
            }
            let mut accumulator = DumpAccumulator::new(
                sequence,
                local_port,
                kind.request_type(),
                kind.response_type(),
            );
            for _ in 0..count {
                let length = usize::try_from(take_u32(body, &mut cursor)?)
                    .map_err(|error| error.to_string())?;
                if length == 0 || length > MAX_DATAGRAM_BYTES {
                    return Err("raw reply datagram length is invalid".to_owned());
                }
                let datagram = take_bytes(body, &mut cursor, length)?;
                accumulator
                    .push_datagram(datagram)
                    .map_err(|error| error.to_string())?;
            }
            if !accumulator.complete || accumulator.interrupted {
                return Err("raw reply dump did not finish cleanly".to_owned());
            }
            snapshot.successful_dumps += 1;
            snapshot.add_messages(kind, &accumulator.messages);
        }
        if cursor != body.len() {
            return Err("raw reply bundle has trailing bytes".to_owned());
        }
        Ok(RawReplyReplay {
            origin,
            captured_unix_ms,
            snapshot,
        })
    }

    fn append_message(
        output: &mut Vec<u8>,
        message_type: u16,
        flags: u16,
        sequence: u32,
        port: u32,
        payload: &[u8],
    ) {
        let length = NLMSG_HEADER_LEN + payload.len();
        output.extend_from_slice(&(length as u32).to_ne_bytes());
        output.extend_from_slice(&message_type.to_ne_bytes());
        output.extend_from_slice(&flags.to_ne_bytes());
        output.extend_from_slice(&sequence.to_ne_bytes());
        output.extend_from_slice(&port.to_ne_bytes());
        output.extend_from_slice(payload);
        output.resize(output.len().next_multiple_of(4), 0);
    }

    fn append_attribute(output: &mut Vec<u8>, kind: u16, data: &[u8]) {
        let length = NLA_HEADER_LEN + data.len();
        output.extend_from_slice(&(length as u16).to_ne_bytes());
        output.extend_from_slice(&kind.to_ne_bytes());
        output.extend_from_slice(data);
        output.resize(output.len().next_multiple_of(4), 0);
    }

    fn accumulator() -> DumpAccumulator {
        DumpAccumulator::new(7, 91, RTM_GETLINK, RTM_NEWLINK)
    }

    fn error_payload(error: i32, request_type: u16, sequence: u32, port: u32) -> Vec<u8> {
        let mut payload = Vec::with_capacity(20);
        payload.extend_from_slice(&error.to_ne_bytes());
        payload.extend_from_slice(&32_u32.to_ne_bytes());
        payload.extend_from_slice(&request_type.to_ne_bytes());
        payload.extend_from_slice(&(NLM_F_REQUEST | NLM_F_ACK | NLM_F_DUMP).to_ne_bytes());
        payload.extend_from_slice(&sequence.to_ne_bytes());
        payload.extend_from_slice(&port.to_ne_bytes());
        payload
    }

    #[test]
    fn raw_reply_bundle_replays_validated_datagrams_and_rejects_tampering() {
        let mut dumps = Vec::new();
        for (index, kind) in dump_kinds().into_iter().enumerate() {
            let sequence = u32::try_from(index + 1).expect("four requests");
            let mut item = Vec::new();
            let mut payload = vec![0_u8; kind.payload_len()];
            if kind == DumpKind::Links {
                payload[4..8].copy_from_slice(&9_i32.to_ne_bytes());
                append_attribute(&mut payload, 3, b"eth9\0");
            } else {
                payload[0] = libc::AF_INET as u8;
            }
            append_message(
                &mut item,
                kind.response_type(),
                NLM_F_MULTI,
                sequence,
                91,
                &payload,
            );
            let mut done = Vec::new();
            append_message(&mut done, NLMSG_DONE, 0, sequence, 91, &0_i32.to_ne_bytes());
            dumps.push(RawDump {
                kind,
                sequence,
                local_port: 91,
                datagrams: vec![item, done],
            });
        }
        let mut encoded = encode_raw_replies(&dumps, "synthetic", 0);
        let replay = replay_raw_replies(&encoded).expect("raw reply replay");
        assert_eq!(replay.origin, "synthetic");
        assert_eq!(replay.captured_unix_ms, 0);
        let snapshot = replay.snapshot;
        assert_eq!(snapshot.successful_dumps, 4);
        assert_eq!(snapshot.links[0].ifindex, 9);
        assert_eq!(snapshot.addresses.len(), 1);
        assert_eq!(snapshot.routes.len(), 1);
        assert_eq!(snapshot.rules.len(), 1);

        let mut reframed = encoded.clone();
        let first_datagram = 8 + 1 + 1 + 8 + 1 + std::env::consts::ARCH.len() + 1 + 4 + 4 + 4 + 4;
        reframed[first_datagram + 8] ^= 1;
        let digest_offset = reframed.len() - 32;
        let digest = Sha256::digest(&reframed[..digest_offset]);
        reframed[digest_offset..].copy_from_slice(&digest);
        assert!(
            replay_raw_replies(&reframed)
                .unwrap_err()
                .contains("sequence")
        );

        let tamper_offset = encoded.len() - 33;
        encoded[tamper_offset] ^= 1;
        assert!(replay_raw_replies(&encoded).unwrap_err().contains("digest"));
    }

    #[test]
    #[ignore = "requires XSH_NETLINK_CAPTURE_DIR or XSH_NETLINK_REPLAY_DIR in the pinned Linux test image"]
    fn live_raw_reply_capture_or_replay_uses_the_network_decoder() {
        let capture = std::env::var_os("XSH_NETLINK_CAPTURE_DIR");
        let replay = std::env::var_os("XSH_NETLINK_REPLAY_DIR");
        assert!(
            capture.is_some() ^ replay.is_some(),
            "set exactly one raw reply bundle directory"
        );
        if let Some(directory) = replay {
            let file = std::fs::File::open(std::path::Path::new(&directory).join("replies.bin"))
                .expect("open saved raw replies");
            let mut bytes = Vec::new();
            file.take(u64::try_from(RAW_REPLY_MAX_BYTES + 1).expect("bounded raw reply size"))
                .read_to_end(&mut bytes)
                .expect("read bounded raw replies");
            let replay = replay_raw_replies(&bytes).expect("replay saved raw replies");
            assert_eq!(replay.snapshot.successful_dumps, 4);
            println!(
                "raw netlink replay: origin={}, captured={} ms, state={}, links={}, addresses={}, routes={}, rules={}",
                replay.origin,
                replay.captured_unix_ms,
                replay.snapshot.state(),
                replay.snapshot.links.len(),
                replay.snapshot.addresses.len(),
                replay.snapshot.routes.len(),
                replay.snapshot.rules.len()
            );
            verify_xsh_network_assembly(replay.snapshot);
            return;
        }

        let origin = std::env::var("XSH_NETLINK_CAPTURE_ORIGIN")
            .unwrap_or_else(|_| "container_live".to_owned());
        assert!(
            matches!(origin.as_str(), "container_live" | "physical_live"),
            "live capture origin is invalid"
        );

        let socket = socket_with(
            AddressFamily::NETLINK,
            SocketType::RAW,
            SocketFlags::CLOEXEC,
            None,
        )
        .expect("open route netlink socket");
        bind(&socket, &SocketAddrNetlink::new(0, 0)).expect("bind route netlink socket");
        let local = rustix::net::getsockname(&socket).expect("read local netlink port");
        let local = SocketAddrNetlink::try_from(local).expect("local route netlink address");
        let mut dumps = Vec::new();
        let mut live = Snapshot::default();
        for (index, kind) in dump_kinds().into_iter().enumerate() {
            let sequence = u32::try_from(index + 1).expect("four requests");
            let mut datagrams = Vec::new();
            let mut messages = None;
            for attempt in 0..DUMP_ATTEMPTS {
                datagrams.clear();
                match collect_dump_attempt_recorded(
                    &socket,
                    sequence,
                    local.pid(),
                    kind,
                    &mut |bytes| datagrams.push(bytes.to_vec()),
                ) {
                    Err(DumpError::Interrupted) if attempt + 1 < DUMP_ATTEMPTS => continue,
                    result => {
                        messages = Some(result.expect("capture a complete route netlink dump"));
                        break;
                    }
                }
            }
            let messages = messages.expect("at least one netlink dump attempt");
            live.successful_dumps += 1;
            live.add_messages(kind, &messages);
            dumps.push(RawDump {
                kind,
                sequence,
                local_port: local.pid(),
                datagrams,
            });
        }
        let captured_unix_ms = u64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("capture clock predates Unix epoch")
                .as_millis(),
        )
        .expect("capture timestamp fits u64");
        let bytes = encode_raw_replies(&dumps, &origin, captured_unix_ms);
        let decoded = replay_raw_replies(&bytes).expect("replay captured raw replies");
        assert_eq!(format!("{live:?}"), format!("{:?}", decoded.snapshot));

        let directory = std::path::PathBuf::from(capture.expect("capture directory"));
        let mut builder = std::fs::DirBuilder::new();
        builder.mode(0o700);
        builder
            .create(&directory)
            .expect("create new private raw reply directory");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(directory.join("replies.bin"))
            .expect("create private raw reply file");
        file.write_all(&bytes).expect("save bounded raw replies");
        file.sync_all().expect("sync raw reply file");
        println!(
            "raw netlink capture: origin={}, captured={} ms, state={}, links={}, addresses={}, routes={}, rules={}",
            decoded.origin,
            decoded.captured_unix_ms,
            live.state(),
            live.links.len(),
            live.addresses.len(),
            live.routes.len(),
            live.rules.len()
        );
        verify_xsh_network_assembly(decoded.snapshot);
    }

    #[test]
    fn dump_accumulator_reads_multipart_messages_across_datagrams() {
        let mut state = accumulator();
        let mut first = Vec::new();
        append_message(&mut first, RTM_NEWLINK, NLM_F_MULTI, 7, 91, &[0; 16]);
        state.push_datagram(&first).expect("first part");
        assert_eq!(state.messages.len(), 1);
        assert!(!state.complete);

        let mut last = Vec::new();
        append_message(&mut last, NLMSG_DONE, 0, 7, 91, &[]);
        state.push_datagram(&last).expect("terminator");
        assert!(state.complete);
    }

    #[test]
    fn raw_multipart_dumps_replay_through_the_network_snapshot_decoder() {
        let mut link = vec![0_u8; 16];
        link[4..8].copy_from_slice(&4_i32.to_ne_bytes());
        link[8..12].copy_from_slice(&1_u32.to_ne_bytes());
        append_attribute(&mut link, 3, b"eth0\0");
        append_attribute(&mut link, 4, &1500_u32.to_ne_bytes());

        let mut address = vec![libc::AF_INET as u8, 24, 0, 0];
        address.extend_from_slice(&4_u32.to_ne_bytes());
        append_attribute(&mut address, 1, &[192, 0, 2, 10]);

        let mut route = vec![0_u8; 12];
        route[0] = libc::AF_INET as u8;
        route[1] = 24;
        route[4] = 254;
        route[5] = 4;
        route[7] = 1;
        append_attribute(&mut route, 1, &[192, 0, 2, 0]);
        append_attribute(&mut route, 4, &4_u32.to_ne_bytes());

        let mut rule = vec![0_u8; 12];
        rule[0] = libc::AF_INET as u8;
        rule[4] = 254;
        rule[7] = 1;
        append_attribute(&mut rule, 6, &100_u32.to_ne_bytes());
        append_attribute(&mut rule, 10, &3_u32.to_ne_bytes());

        let mut snapshot = Snapshot::default();
        for (index, (kind, payload)) in [
            (DumpKind::Links, link),
            (DumpKind::Addresses, address),
            (DumpKind::Routes, route),
            (DumpKind::Rules, rule),
        ]
        .into_iter()
        .enumerate()
        {
            let sequence = (index + 1) as u32;
            let mut item = Vec::new();
            append_message(
                &mut item,
                kind.response_type(),
                NLM_F_MULTI,
                sequence,
                91,
                &payload,
            );
            let mut done = Vec::new();
            append_message(&mut done, NLMSG_DONE, 0, sequence, 91, &0_i32.to_ne_bytes());
            let mut accumulator =
                DumpAccumulator::new(sequence, 91, kind.request_type(), kind.response_type());
            accumulator.push_datagram(&item).expect("multipart item");
            accumulator
                .push_datagram(&done)
                .expect("multipart terminator");
            assert!(accumulator.complete);
            assert!(!accumulator.interrupted);
            snapshot.successful_dumps += 1;
            snapshot.add_messages(kind, &accumulator.messages);
        }

        assert!(snapshot.issues.is_empty());
        assert_eq!(snapshot.links[0].ifindex, 4);
        assert_eq!(snapshot.links[0].mtu, Some(1500));
        assert_eq!(
            snapshot.addresses[0].address.as_deref(),
            Some(&[192, 0, 2, 10][..])
        );
        assert_eq!(
            snapshot.routes[0].destination.as_deref(),
            Some(&[192, 0, 2, 0][..])
        );
        assert_eq!(snapshot.routes[0].output_ifindex, Some(4));
        assert_eq!(snapshot.rules[0].priority, Some(100));
        assert_eq!(snapshot.rules[0].fwmark, Some(3));
        let Value::Record(value) = snapshot_value(snapshot) else {
            panic!("network dump must be a record");
        };
        assert!(
            matches!(value.get("state"), Some(Value::Str(state)) if state.as_ref() == "complete")
        );
        assert!(matches!(
            value.get("enumeration_succeeded"),
            Some(Value::Bool(true))
        ));
        for field in ["links", "addresses", "routes", "rules"] {
            assert!(matches!(value.get(field), Some(Value::List(rows)) if rows.len() == 1));
        }
    }

    #[test]
    fn dump_accumulator_rejects_bad_sequence_port_and_lengths() {
        for (sequence, port, payload, expected) in [
            (
                8,
                91,
                &[0_u8; 16][..],
                "netlink response sequence does not match",
            ),
            (
                7,
                92,
                &[0_u8; 16][..],
                "netlink response port id does not match",
            ),
        ] {
            let mut state = accumulator();
            let mut datagram = Vec::new();
            append_message(
                &mut datagram,
                RTM_NEWLINK,
                NLM_F_MULTI,
                sequence,
                port,
                payload,
            );
            assert_eq!(
                state.push_datagram(&datagram).unwrap_err().to_string(),
                expected
            );
        }
        let mut state = accumulator();
        assert!(state.push_datagram(&[8, 0, 0]).is_err());
    }

    #[test]
    fn dump_accumulator_reports_kernel_errors_and_interrupted_dumps() {
        let mut state = accumulator();
        let mut error = Vec::new();
        append_message(
            &mut error,
            NLMSG_ERROR,
            0,
            7,
            91,
            &error_payload(-libc::EPERM, RTM_GETLINK, 7, 91),
        );
        assert!(matches!(
            state.push_datagram(&error),
            Err(DumpError::Kernel(_))
        ));

        let mut state = accumulator();
        let mut ack = Vec::new();
        append_message(
            &mut ack,
            NLMSG_ERROR,
            0,
            7,
            91,
            &error_payload(0, RTM_GETLINK, 7, 91),
        );
        state.push_datagram(&ack).expect("success acknowledgment");
        assert!(!state.complete);

        let mut state = accumulator();
        let mut mismatched_ack = Vec::new();
        append_message(
            &mut mismatched_ack,
            NLMSG_ERROR,
            0,
            7,
            91,
            &error_payload(0, RTM_GETADDR, 7, 91),
        );
        assert!(state.push_datagram(&mismatched_ack).is_err());

        let mut state = accumulator();
        let mut interrupted = Vec::new();
        append_message(
            &mut interrupted,
            RTM_NEWLINK,
            NLM_F_MULTI | NLM_F_DUMP_INTR,
            7,
            91,
            &[0; 16],
        );
        state
            .push_datagram(&interrupted)
            .expect("parse interrupted item");
        assert!(state.interrupted);
    }

    #[test]
    fn dump_accumulator_rejects_truncated_status_and_positive_error_codes() {
        let mut state = accumulator();
        let mut partial_done = Vec::new();
        append_message(&mut partial_done, NLMSG_DONE, 0, 7, 91, &[0; 3]);
        assert!(matches!(
            state.push_datagram(&partial_done),
            Err(DumpError::Malformed(_))
        ));

        let mut state = accumulator();
        let mut positive_error = Vec::new();
        append_message(
            &mut positive_error,
            NLMSG_ERROR,
            0,
            7,
            91,
            &error_payload(1, RTM_GETLINK, 7, 91),
        );
        assert!(matches!(
            state.push_datagram(&positive_error),
            Err(DumpError::Malformed(_))
        ));

        let mut state = accumulator();
        let mut positive_done = Vec::new();
        append_message(
            &mut positive_done,
            NLMSG_DONE,
            0,
            7,
            91,
            &1_i32.to_ne_bytes(),
        );
        assert!(matches!(
            state.push_datagram(&positive_done),
            Err(DumpError::Malformed(_))
        ));
    }

    #[test]
    fn attributes_reject_malformed_lengths_and_retain_unknown_kinds() {
        let attributes = [7_u8, 0, 0x34, 0x80, b'x', b'y', b'z', 0];
        let parsed = parse_attributes(&attributes, 0).expect("attribute");
        assert_eq!(parsed[0].raw_kind, 0x8034);
        assert_eq!(parsed[0].kind, 0x34);
        assert_eq!(parsed[0].data, b"xyz");
        assert!(parse_attributes(&[3, 0, 1, 0], 0).is_err());
    }

    #[test]
    fn malformed_one_link_does_not_discard_sibling_records() {
        let mut valid = vec![0_u8; 16];
        valid[4..8].copy_from_slice(&4_i32.to_ne_bytes());
        let mut dump = Snapshot::default();
        dump.add_messages(
            DumpKind::Links,
            &[
                NetlinkMessage {
                    payload: vec![0; 2],
                },
                NetlinkMessage { payload: valid },
            ],
        );
        assert_eq!(dump.links.len(), 1);
        assert_eq!(dump.issues.len(), 1);
        assert_eq!(dump.links[0].ifindex, 4);
    }

    #[test]
    fn truncated_address_attribute_does_not_discard_a_valid_neighbor() {
        let mut invalid = vec![libc::AF_INET as u8, 24, 0, 0];
        invalid.extend_from_slice(&2_u32.to_ne_bytes());
        invalid.extend_from_slice(&12_u16.to_ne_bytes());
        invalid.extend_from_slice(&1_u16.to_ne_bytes());
        invalid.extend_from_slice(&[192, 0, 2, 1]);

        let mut valid = vec![libc::AF_INET as u8, 24, 0, 0];
        valid.extend_from_slice(&3_u32.to_ne_bytes());
        valid.extend_from_slice(&8_u16.to_ne_bytes());
        valid.extend_from_slice(&1_u16.to_ne_bytes());
        valid.extend_from_slice(&[192, 0, 2, 2]);

        let mut snapshot = Snapshot::default();
        snapshot.add_messages(
            DumpKind::Addresses,
            &[
                NetlinkMessage { payload: invalid },
                NetlinkMessage { payload: valid },
            ],
        );
        assert_eq!(snapshot.addresses.len(), 1);
        assert_eq!(snapshot.addresses[0].ifindex, 3);
        assert_eq!(
            snapshot.addresses[0].address.as_deref(),
            Some(&[192, 0, 2, 2][..])
        );
        assert_eq!(snapshot.issues.len(), 1);
        assert_eq!(snapshot.issues[0].object, "address");
        assert_eq!(snapshot.issues[0].state, "malformed");
    }

    #[test]
    fn oversized_link_counters_keep_the_link_and_report_field_failures() {
        let link_message = |ifindex: i32, rx_bytes: u64, tx_bytes: u64| {
            let mut payload = vec![0_u8; 16];
            payload[4..8].copy_from_slice(&ifindex.to_ne_bytes());
            let mut stats = vec![0_u8; 32];
            stats[16..24].copy_from_slice(&rx_bytes.to_ne_bytes());
            stats[24..32].copy_from_slice(&tx_bytes.to_ne_bytes());
            payload.extend_from_slice(&36_u16.to_ne_bytes());
            payload.extend_from_slice(&23_u16.to_ne_bytes());
            payload.extend_from_slice(&stats);
            NetlinkMessage { payload }
        };

        let mut snapshot = Snapshot::default();
        snapshot.add_messages(
            DumpKind::Links,
            &[
                link_message(4, MAX_JSON_SAFE_INT + 1, 4096),
                link_message(5, 0, u64::MAX),
            ],
        );
        assert_eq!(snapshot.links.len(), 2);
        assert_eq!(snapshot.issues.len(), 2);
        assert_eq!(snapshot.issues[0].object, "links.4.rx_bytes");
        assert_eq!(snapshot.issues[0].state, "range_failure");
        assert_eq!(snapshot.issues[1].object, "links.5.tx_bytes");
        let Value::Record(link) = link_value(snapshot.links.remove(0)) else {
            panic!("link output must be a record");
        };
        assert!(matches!(link.get("rx_bytes"), Some(Value::Null)));
        assert!(matches!(link.get("tx_bytes"), Some(Value::Int(4096))));
        let Value::Record(link) = link_value(snapshot.links.remove(0)) else {
            panic!("link output must be a record");
        };
        assert!(matches!(link.get("rx_bytes"), Some(Value::Int(0))));
        assert!(matches!(link.get("tx_bytes"), Some(Value::Null)));
    }

    #[test]
    fn network_dump_keeps_enumeration_success_separate_from_field_issues() {
        let mut snapshot = Snapshot {
            successful_dumps: 4,
            ..Snapshot::default()
        };
        snapshot.issues.push(NetworkIssue {
            object: "links.4.rx_bytes".to_owned(),
            message: "counter out of range".to_owned(),
            state: "range_failure".to_owned(),
            errno: None,
            error_kind: "integer_out_of_range".to_owned(),
        });
        let Value::Record(value) = snapshot_value(snapshot) else {
            panic!("network dump must be a record");
        };
        assert!(matches!(
            value.get("state"),
            Some(Value::Str(state)) if state.as_ref() == "partial"
        ));
        assert!(matches!(
            value.get("enumeration_succeeded"),
            Some(Value::Bool(true))
        ));

        let Value::Record(value) = snapshot_value(Snapshot {
            successful_dumps: 3,
            ..Snapshot::default()
        }) else {
            panic!("network dump must be a record");
        };
        assert!(matches!(
            value.get("enumeration_succeeded"),
            Some(Value::Bool(false))
        ));
    }

    #[test]
    fn malformed_entity_prevents_successful_enumeration_after_all_dumps_finish() {
        let mut snapshot = Snapshot {
            successful_dumps: 4,
            ..Snapshot::default()
        };
        snapshot.add_messages(
            DumpKind::Addresses,
            &[NetlinkMessage {
                payload: vec![0_u8; 3],
            }],
        );
        let Value::Record(value) = snapshot_value(snapshot) else {
            panic!("network dump must be a record");
        };
        assert!(
            matches!(value.get("state"), Some(Value::Str(state)) if state.as_ref() == "partial")
        );
        assert!(matches!(
            value.get("enumeration_succeeded"),
            Some(Value::Bool(false))
        ));
    }
}
