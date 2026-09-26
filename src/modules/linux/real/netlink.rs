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
            return Err(DumpError::Malformed("data arrived after the dump terminator"));
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
                return Err(DumpError::Malformed("data followed the netlink dump terminator"));
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
                return Err(DumpError::Malformed("netlink response sequence does not match"));
            }
            if port != self.local_port {
                return Err(DumpError::Malformed("netlink response port id does not match"));
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
                    return Err(DumpError::Truncated("kernel reported a netlink receive overrun"));
                }
                NLMSG_ERROR => {
                    if payload.len() < 20 {
                        return Err(DumpError::Malformed("netlink error acknowledgment is truncated"));
                    }
                    let error = read_i32(payload, 0)
                        .ok_or(DumpError::Malformed("netlink error payload is truncated"))?;
                    let request_type = read_u16(payload, 4)
                        .ok_or(DumpError::Malformed("acknowledged request type is missing"))?;
                    let request_sequence = read_u32(payload, 12)
                        .ok_or(DumpError::Malformed("acknowledged request sequence is missing"))?;
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
                    if error != 0 {
                        let errno = error.checked_neg().ok_or(DumpError::Malformed(
                            "netlink error code is outside the supported range",
                        ))?;
                        return Err(DumpError::Kernel(io::Error::from_raw_os_error(errno)));
                    }
                }
                NLMSG_DONE => {
                    if payload.len() >= 4 {
                        let error = read_i32(payload, 0)
                            .ok_or(DumpError::Malformed("netlink dump status is truncated"))?;
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
                _ => return Err(DumpError::Malformed("unexpected message type in netlink dump")),
            }

            let aligned = align4(length).ok_or(DumpError::Malformed(
                "netlink message alignment overflowed",
            ))?;
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
            Self::Kernel(error) | Self::Io(error) if error.kind() == io::ErrorKind::PermissionDenied => {
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
            self.issues.first().map_or("failed", |issue| match issue.state.as_str() {
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
                DumpKind::Links => parse_link(&message.payload).map(|value| self.links.push(value)),
                DumpKind::Addresses => {
                    parse_address(&message.payload).map(|value| self.addresses.push(value))
                }
                DumpKind::Routes => {
                    parse_route(&message.payload).map(|value| self.routes.push(value))
                }
                DumpKind::Rules => parse_rule(&message.payload).map(|value| self.rules.push(value)),
            };
            if let Err(message) = parsed {
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
            return Err(DumpError::Truncated("kernel netlink datagram was truncated"));
        }
        let sender = sender.ok_or(DumpError::Malformed("netlink sender address is missing"))?;
        let sender = SocketAddrNetlink::try_from(sender)
            .map_err(|_| DumpError::Malformed("netlink sender address has the wrong family"))?;
        if sender.pid() != 0 || sender.groups() != 0 {
            return Err(DumpError::Malformed("netlink response sender is not the kernel"));
        }
        accumulator.push_datagram(&buffer[..received])?;
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
        ("master_ifindex", optional_int(link.master_ifindex.map(i64::from))),
        ("lower_ifindex", optional_int(link.lower_ifindex.map(i64::from))),
        ("operstate", optional_int(link.operstate.map(i64::from))),
        ("kind", optional_text(link.kind.as_deref())),
        ("rx_bytes", optional_int(link.rx_bytes.and_then(|n| i64::try_from(n).ok()))),
        ("tx_bytes", optional_int(link.tx_bytes.and_then(|n| i64::try_from(n).ok()))),
        ("attributes", attributes_value(link.attributes)),
    ])
}

fn address_value(address: Address) -> Value {
    record([
        ("ifindex", Value::Int(i64::from(address.ifindex))),
        ("family", str_value(family_name(address.family))),
        ("prefix_length", Value::Int(i64::from(address.prefix_length))),
        ("scope", Value::Int(i64::from(address.scope))),
        ("flags", Value::Int(i64::from(address.flags))),
        ("address", optional_ip(address.family, address.address)),
        ("local", optional_ip(address.family, address.local)),
        ("broadcast", optional_ip(address.family, address.broadcast)),
        ("label", optional_text(address.label.as_deref())),
        ("preferred_lifetime_seconds", optional_int(address.preferred_lifetime.map(i64::from))),
        ("valid_lifetime_seconds", optional_int(address.valid_lifetime.map(i64::from))),
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
        ("destination_prefix_length", Value::Int(i64::from(route.destination_length))),
        ("source_prefix_length", Value::Int(i64::from(route.source_length))),
        ("destination", optional_ip(route.family, route.destination)),
        ("source", optional_ip(route.family, route.source)),
        ("gateway", optional_ip(route.family, route.gateway)),
        ("preferred_source", optional_ip(route.family, route.preferred_source)),
        ("output_ifindex", optional_int(route.output_ifindex.map(i64::from))),
        ("input_ifindex", optional_int(route.input_ifindex.map(i64::from))),
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
        ("destination_prefix_length", Value::Int(i64::from(rule.destination_length))),
        ("source_prefix_length", Value::Int(i64::from(rule.source_length))),
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
    let end = bytes.iter().position(|byte| *byte == 0).unwrap_or(bytes.len());
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
        libc::AF_INET if bytes.len() == 4 => Some(Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3]).to_string()),
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
    Some(u16::from_ne_bytes(bytes.get(offset..offset.checked_add(2)?)?.try_into().ok()?))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_ne_bytes(bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?))
}

fn read_i32(bytes: &[u8], offset: usize) -> Option<i32> {
    Some(i32::from_ne_bytes(bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?))
}

fn read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_ne_bytes(bytes.get(offset..offset.checked_add(8)?)?.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn dump_accumulator_rejects_bad_sequence_port_and_lengths() {
        for (sequence, port, payload, expected) in [
            (8, 91, &[0_u8; 16][..], "netlink response sequence does not match"),
            (7, 92, &[0_u8; 16][..], "netlink response port id does not match"),
        ] {
            let mut state = accumulator();
            let mut datagram = Vec::new();
            append_message(&mut datagram, RTM_NEWLINK, NLM_F_MULTI, sequence, port, payload);
            assert_eq!(state.push_datagram(&datagram).unwrap_err().to_string(), expected);
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
        assert!(matches!(state.push_datagram(&error), Err(DumpError::Kernel(_))));

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
        state.push_datagram(&interrupted).expect("parse interrupted item");
        assert!(state.interrupted);
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
                NetlinkMessage {
                    payload: valid,
                },
            ],
        );
        assert_eq!(dump.links.len(), 1);
        assert_eq!(dump.issues.len(), 1);
        assert_eq!(dump.links[0].ifindex, 4);
    }
}
