use super::common::{error_value, io_error, ok_unit, record_str};
use crate::modules::linux::str_value;
use crate::runtime::value::{RuntimeError, Value};
use crate::source::Span;
use rustix::net::{AddressFamily, SocketFlags, SocketType, socket_with};
use std::ffi::CStr;
use std::ffi::CString;
use std::fs;
use std::io;
use std::os::fd::{AsRawFd, BorrowedFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// AF_INET datagram socket used purely as an ioctl handle for interface config.
fn inet_dgram_socket() -> io::Result<OwnedFd> {
    socket_with(
        AddressFamily::INET,
        SocketType::DGRAM,
        SocketFlags::CLOEXEC,
        None,
    )
    .map_err(io::Error::from)
}

pub(crate) fn link_up(interface: &str, span: Span) -> Result<Value, RuntimeError> {
    if let Err(error) = validate_interface(interface) {
        return Ok(error_value("linux-link-up", error.to_string(), span));
    }
    let fd = match inet_dgram_socket() {
        Ok(fd) => fd,
        Err(error) => return Ok(io_error("linux-link-up", error, span)),
    };
    let result = link_up_with_socket(fd.as_raw_fd(), interface);
    match result {
        Ok(()) => Ok(ok_unit()),
        Err(error) => Ok(io_error("linux-link-up", error, span)),
    }
}

pub(crate) fn link_down(interface: &str, span: Span) -> Result<Value, RuntimeError> {
    if let Err(error) = validate_interface(interface) {
        return Ok(error_value("linux-link-down", error.to_string(), span));
    }
    let fd = match inet_dgram_socket() {
        Ok(fd) => fd,
        Err(error) => return Ok(io_error("linux-link-down", error, span)),
    };
    let result = link_down_with_socket(fd.as_raw_fd(), interface);
    match result {
        Ok(()) => Ok(ok_unit()),
        Err(error) => Ok(io_error("linux-link-down", error, span)),
    }
}

pub(crate) fn set_ipv4_address(
    interface: &str,
    address: &str,
    netmask: &str,
    span: Span,
) -> Result<Value, RuntimeError> {
    if let Err(error) = validate_interface(interface) {
        return Ok(error_value(
            "linux-set-ipv4-address",
            error.to_string(),
            span,
        ));
    }
    let address = match parse_ipv4(address, "address") {
        Ok(address) => address,
        Err(error) => {
            return Ok(error_value(
                "linux-set-ipv4-address",
                error.to_string(),
                span,
            ));
        }
    };
    let netmask = match parse_ipv4(netmask, "netmask") {
        Ok(netmask) => netmask,
        Err(error) => {
            return Ok(error_value(
                "linux-set-ipv4-address",
                error.to_string(),
                span,
            ));
        }
    };
    let fd = match inet_dgram_socket() {
        Ok(fd) => fd,
        Err(error) => return Ok(io_error("linux-set-ipv4-address", error, span)),
    };
    let result = set_ipv4_address_with_socket(fd.as_raw_fd(), interface, address, netmask);
    match result {
        Ok(()) => Ok(ok_unit()),
        Err(error) => Ok(io_error("linux-set-ipv4-address", error, span)),
    }
}

pub(crate) fn add_default_ipv4_route(
    gateway: &str,
    interface: &str,
    span: Span,
) -> Result<Value, RuntimeError> {
    if !interface.is_empty()
        && let Err(error) = validate_interface(interface)
    {
        return Ok(error_value(
            "linux-add-default-ipv4-route",
            error.to_string(),
            span,
        ));
    }
    let gateway = match parse_ipv4(gateway, "gateway") {
        Ok(gateway) => gateway,
        Err(error) => {
            return Ok(error_value(
                "linux-add-default-ipv4-route",
                error.to_string(),
                span,
            ));
        }
    };
    let fd = match inet_dgram_socket() {
        Ok(fd) => fd,
        Err(error) => return Ok(io_error("linux-add-default-ipv4-route", error, span)),
    };
    let result = add_default_ipv4_route_with_socket(fd.as_raw_fd(), gateway, interface);
    match result {
        Ok(()) => Ok(ok_unit()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(ok_unit()),
        Err(error) => Ok(io_error("linux-add-default-ipv4-route", error, span)),
    }
}

pub(crate) fn flush_ipv4_addresses(interface: &str, span: Span) -> Result<Value, RuntimeError> {
    if let Err(error) = validate_interface(interface) {
        return Ok(error_value(
            "linux-flush-ipv4-addresses",
            error.to_string(),
            span,
        ));
    }
    let fd = match inet_dgram_socket() {
        Ok(fd) => fd,
        Err(error) => return Ok(io_error("linux-flush-ipv4-addresses", error, span)),
    };
    let result = flush_ipv4_addresses_with_socket(fd.as_raw_fd(), interface);
    match result {
        Ok(()) => Ok(ok_unit()),
        Err(error) if error.kind() == io::ErrorKind::AddrNotAvailable => Ok(ok_unit()),
        Err(error) => Ok(io_error("linux-flush-ipv4-addresses", error, span)),
    }
}

pub(crate) fn del_default_ipv4_route(
    gateway: &str,
    interface: &str,
    span: Span,
) -> Result<Value, RuntimeError> {
    if let Err(error) = validate_interface(interface) {
        return Ok(error_value(
            "linux-del-default-ipv4-route",
            error.to_string(),
            span,
        ));
    }
    let gateway = match parse_ipv4(gateway, "gateway") {
        Ok(gateway) => gateway,
        Err(error) => {
            return Ok(error_value(
                "linux-del-default-ipv4-route",
                error.to_string(),
                span,
            ));
        }
    };
    let fd = match inet_dgram_socket() {
        Ok(fd) => fd,
        Err(error) => return Ok(io_error("linux-del-default-ipv4-route", error, span)),
    };
    let result = del_default_ipv4_route_with_socket(fd.as_raw_fd(), gateway, interface);
    match result {
        Ok(()) => Ok(ok_unit()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(ok_unit()),
        Err(error) => Ok(io_error("linux-del-default-ipv4-route", error, span)),
    }
}

// DHCP uses the well-known BOOTP client/server UDP ports. The protocol itself
// is driven from core/ifup.xsh; these primitives only provide the broadcast UDP
// socket it needs before the interface has an address. IPv4 only.
const DHCP_CLIENT_PORT: u16 = 68;
const DHCP_SERVER_PORT: u16 = 67;
const DHCP_RECV_BUF: usize = 1500;

/// Open a UDP socket bound to `0.0.0.0:68` on `interface`, with broadcast
/// enabled, suitable for a DHCP client. Returns the raw fd as an Int; the caller
/// must release it with `dhcp_close`.
pub(crate) fn dhcp_socket(interface: &str, span: Span) -> Result<Value, RuntimeError> {
    if let Err(error) = validate_interface(interface) {
        return Ok(error_value("linux-dhcp-socket", error.to_string(), span));
    }
    match open_dhcp_socket(interface) {
        Ok(fd) => Ok(Value::ok(Value::Int(fd as i64))),
        Err(error) => Ok(io_error("linux-dhcp-socket", error, span)),
    }
}

fn open_dhcp_socket(interface: &str) -> io::Result<RawFd> {
    let fd = socket_with(
        AddressFamily::INET,
        SocketType::DGRAM,
        SocketFlags::CLOEXEC,
        None,
    )?;
    rustix::net::sockopt::set_socket_reuseaddr(&fd, true)?;
    rustix::net::sockopt::set_socket_broadcast(&fd, true)?;
    // SO_BINDTODEVICE keeps the unconfigured-interface broadcast on the right
    // link; rustix 1.1 has no helper for it, so use libc as elsewhere here.
    bind_to_device(fd.as_raw_fd(), interface)?;
    rustix::net::bind(
        &fd,
        &rustix::net::SocketAddrV4::new(rustix::net::Ipv4Addr::UNSPECIFIED, DHCP_CLIENT_PORT),
    )?;
    Ok(fd.into_raw_fd())
}

/// Broadcast `payload` to `255.255.255.255:67` on the DHCP client socket `fd`.
pub(crate) fn dhcp_send(fd: i64, payload: &[u8], span: Span) -> Result<Value, RuntimeError> {
    let socket = unsafe { BorrowedFd::borrow_raw(fd as RawFd) };
    let destination =
        rustix::net::SocketAddrV4::new(rustix::net::Ipv4Addr::BROADCAST, DHCP_SERVER_PORT);
    match rustix::net::sendto(
        socket,
        payload,
        rustix::net::SendFlags::empty(),
        &destination,
    ) {
        Ok(_) => Ok(ok_unit()),
        Err(error) => Ok(io_error("linux-dhcp-send", io::Error::from(error), span)),
    }
}

/// Receive one datagram from the DHCP client socket `fd`, waiting up to
/// `timeout_ms` (0 or negative blocks). Returns the bytes, or empty bytes on
/// timeout so the caller can retransmit.
pub(crate) fn dhcp_recv(fd: i64, timeout_ms: i64, span: Span) -> Result<Value, RuntimeError> {
    let socket = unsafe { BorrowedFd::borrow_raw(fd as RawFd) };
    let timeout = if timeout_ms <= 0 {
        None
    } else {
        Some(Duration::from_millis(timeout_ms as u64))
    };
    if let Err(error) = rustix::net::sockopt::set_socket_timeout(
        socket,
        rustix::net::sockopt::Timeout::Recv,
        timeout,
    ) {
        return Ok(io_error("linux-dhcp-recv", io::Error::from(error), span));
    }
    let mut buffer = vec![0u8; DHCP_RECV_BUF];
    let received = match rustix::net::recv(socket, &mut buffer[..], rustix::net::RecvFlags::empty())
    {
        Ok((_, len)) => len,
        Err(error) if is_recv_timeout(error) => return Ok(Value::ok(Value::Bytes(Vec::new()))),
        Err(error) => return Ok(io_error("linux-dhcp-recv", io::Error::from(error), span)),
    };
    buffer.truncate(received);
    Ok(Value::ok(Value::Bytes(buffer)))
}

/// Close a DHCP client socket previously returned by `dhcp_socket`.
pub(crate) fn dhcp_close(fd: i64, _span: Span) -> Result<Value, RuntimeError> {
    drop(unsafe { OwnedFd::from_raw_fd(fd as RawFd) });
    Ok(ok_unit())
}

/// Send a DHCP RELEASE (type 7) for `address` to `server_id` on `interface`.
/// Opens its own socket so the caller doesn't need to manage one.
pub(crate) fn dhcp_send_release(
    interface: &str,
    address: &str,
    server_id: &str,
    span: Span,
) -> Result<Value, RuntimeError> {
    if let Err(error) = validate_interface(interface) {
        return Ok(error_value(
            "linux-dhcp-send-release",
            error.to_string(),
            span,
        ));
    }
    let mac = match interface_mac(interface) {
        Ok(mac) => mac,
        Err(error) => return Ok(io_error("linux-dhcp-send-release", error, span)),
    };
    let addr = match parse_ipv4_bytes(address, "address") {
        Ok(a) => a,
        Err(error) => return Ok(io_error("linux-dhcp-send-release", error, span)),
    };
    let sid = match parse_ipv4_bytes(server_id, "server_id") {
        Ok(s) => s,
        Err(error) => return Ok(io_error("linux-dhcp-send-release", error, span)),
    };
    let xid = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))
        .map(|d| d.as_secs() as u32)
        .unwrap_or(0);
    let packet = build_dhcp_release(xid, &mac, &addr, &sid);
    let destination =
        rustix::net::SocketAddrV4::new(rustix::net::Ipv4Addr::BROADCAST, DHCP_SERVER_PORT);
    let socket = match open_dhcp_owned_socket(interface) {
        Ok(s) => s,
        Err(error) => return Ok(io_error("linux-dhcp-send-release", error, span)),
    };
    match rustix::net::sendto(
        &socket,
        &packet,
        rustix::net::SendFlags::empty(),
        &destination,
    ) {
        Ok(_) => Ok(ok_unit()),
        Err(error) => Ok(io_error(
            "linux-dhcp-send-release",
            io::Error::from(error),
            span,
        )),
    }
}

fn open_dhcp_owned_socket(interface: &str) -> io::Result<OwnedFd> {
    let fd = socket_with(
        AddressFamily::INET,
        SocketType::DGRAM,
        SocketFlags::CLOEXEC,
        None,
    )?;
    rustix::net::sockopt::set_socket_reuseaddr(&fd, true)?;
    rustix::net::sockopt::set_socket_broadcast(&fd, true)?;
    bind_to_device(fd.as_raw_fd(), interface)?;
    rustix::net::bind(
        &fd,
        &rustix::net::SocketAddrV4::new(rustix::net::Ipv4Addr::UNSPECIFIED, DHCP_CLIENT_PORT),
    )?;
    Ok(fd)
}

fn is_recv_timeout(error: rustix::io::Errno) -> bool {
    error == rustix::io::Errno::AGAIN
        || error == rustix::io::Errno::WOULDBLOCK
        || error == rustix::io::Errno::INTR
        || error == rustix::io::Errno::TIMEDOUT
}

fn bind_to_device(fd: RawFd, interface: &str) -> io::Result<()> {
    let name = CString::new(interface)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "interface name contains NUL"))?;
    let bytes = name.as_bytes_with_nul();
    let rc = unsafe {
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_BINDTODEVICE,
            bytes.as_ptr() as *const libc::c_void,
            bytes.len() as libc::socklen_t,
        )
    };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub(crate) fn interfaces(span: Span) -> Result<Value, RuntimeError> {
    let mut records = Vec::new();
    let entries = match fs::read_dir("/sys/class/net") {
        Ok(entries) => entries,
        Err(error) => return Ok(io_error("linux-interfaces", error, span)),
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => return Ok(io_error("linux-interfaces", error, span)),
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        let flags = read_sys_u32(&path.join("flags")).unwrap_or(0);
        let mtu = match interface_mtu(&name) {
            Ok(mtu) => mtu,
            Err(_) => read_sys_u32(&path.join("mtu")).unwrap_or(0),
        };
        let mac = fs::read_to_string(path.join("address"))
            .map(|value| value.trim().to_string())
            .unwrap_or_default();
        records.push(Value::Record(crate::runtime::value::RecordMap::from([
            (Arc::from("name"), str_value(name.clone())),
            (
                Arc::from("flags"),
                Value::List(
                    interface_flag_names(flags)
                        .into_iter()
                        .map(|flag| str_value(flag.to_string()))
                        .collect(),
                ),
            ),
            (Arc::from("mtu"), Value::Int(mtu as i64)),
            (Arc::from("mac"), str_value(mac)),
            (
                Arc::from("addresses"),
                Value::List(interface_addresses(&name).unwrap_or_default()),
            ),
        ])));
    }
    records.sort_unstable_by_key(|left| record_str(left, "name"));
    Ok(Value::ok(Value::List(records)))
}

pub(crate) fn routes(span: Span) -> Result<Value, RuntimeError> {
    let mut records = Vec::new();
    match fs::read_to_string("/proc/net/route") {
        Ok(text) => records.extend(parse_ipv4_routes(&text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Ok(io_error("linux-routes", error, span)),
    }
    match fs::read_to_string("/proc/net/ipv6_route") {
        Ok(text) => records.extend(parse_ipv6_routes(&text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Ok(io_error("linux-routes", error, span)),
    }
    Ok(Value::ok(Value::List(records)))
}

fn parse_ipv4_routes(text: &str) -> Vec<Value> {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            if fields.len() < 11 {
                return None;
            }
            let destination = parse_ipv4_hex(fields[1])?;
            let gateway = parse_ipv4_hex(fields[2])?;
            let flags = u16::from_str_radix(fields[3], 16).ok()?;
            let metric = fields[6].parse::<i64>().unwrap_or(0);
            let mask = parse_ipv4_hex(fields[7])?;
            let prefix_len = u32::from(mask).count_ones();
            Some(route_record(
                "inet",
                route_dst(destination.to_string(), prefix_len),
                prefix_len as i64,
                gateway.to_string(),
                fields[0].to_string(),
                metric,
                route_flags(flags),
            ))
        })
        .collect()
}

fn parse_ipv6_routes(text: &str) -> Vec<Value> {
    text.lines()
        .filter_map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            if fields.len() < 10 {
                return None;
            }
            let destination = parse_ipv6_hex(fields[0])?;
            let prefix_len = u8::from_str_radix(fields[1], 16).ok()? as i64;
            let gateway = parse_ipv6_hex(fields[4])?;
            let metric = i64::from_str_radix(fields[5], 16).unwrap_or(0);
            let flags = u16::from_str_radix(fields[8], 16).unwrap_or(0);
            Some(route_record(
                "inet6",
                route_dst(destination.to_string(), prefix_len as u32),
                prefix_len,
                gateway.to_string(),
                fields[9].to_string(),
                metric,
                route_flags(flags),
            ))
        })
        .collect()
}

fn parse_ipv4_hex(value: &str) -> Option<std::net::Ipv4Addr> {
    let raw = u32::from_str_radix(value, 16).ok()?;
    Some(std::net::Ipv4Addr::from(u32::from_le(raw)))
}

fn parse_ipv6_hex(value: &str) -> Option<std::net::Ipv6Addr> {
    if value.len() != 32 {
        return None;
    }
    let mut bytes = [0_u8; 16];
    for index in 0..16 {
        bytes[index] = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(std::net::Ipv6Addr::from(bytes))
}

fn route_dst(addr: String, prefix_len: u32) -> String {
    if prefix_len == 0 {
        "default".to_string()
    } else {
        format!("{addr}/{prefix_len}")
    }
}

fn route_flags(flags: u16) -> Vec<String> {
    let mut names = Vec::new();
    if flags & 0x1 != 0 {
        names.push("UP".to_string());
    }
    if flags & 0x2 != 0 {
        names.push("GATEWAY".to_string());
    }
    if flags & 0x4 != 0 {
        names.push("HOST".to_string());
    }
    if flags & 0x10 != 0 {
        names.push("REJECT".to_string());
    }
    names
}

fn route_record(
    family: &str,
    dst: String,
    prefix_len: i64,
    gateway: String,
    dev: String,
    metric: i64,
    flags: Vec<String>,
) -> Value {
    Value::Record(crate::runtime::value::RecordMap::from([
        (Arc::from("family"), str_value(family)),
        (Arc::from("dst"), str_value(dst)),
        (Arc::from("prefix_len"), Value::Int(prefix_len)),
        (Arc::from("gateway"), str_value(gateway)),
        (Arc::from("dev"), str_value(dev)),
        (Arc::from("metric"), Value::Int(metric)),
        (
            Arc::from("flags"),
            Value::List(flags.into_iter().map(str_value).collect()),
        ),
    ]))
}

fn link_up_with_socket(fd: RawFd, interface: &str) -> io::Result<()> {
    let mut request: libc::ifreq = unsafe { std::mem::zeroed() };
    fill_ifreq_name(&mut request, interface);
    if unsafe { libc::ioctl(fd, libc::SIOCGIFFLAGS as _, &mut request) } != 0 {
        return Err(io::Error::last_os_error());
    }
    unsafe {
        request.ifr_ifru.ifru_flags |= libc::IFF_UP as libc::c_short;
    }
    if unsafe { libc::ioctl(fd, libc::SIOCSIFFLAGS as _, &request) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn set_ipv4_address_with_socket(
    fd: RawFd,
    interface: &str,
    address: std::net::Ipv4Addr,
    netmask: std::net::Ipv4Addr,
) -> io::Result<()> {
    set_ifreq_sockaddr(fd, interface, libc::SIOCSIFADDR, address)?;
    set_ifreq_sockaddr(fd, interface, libc::SIOCSIFNETMASK, netmask)
}

fn set_ifreq_sockaddr(
    fd: RawFd,
    interface: &str,
    request_code: libc::c_ulong,
    address: std::net::Ipv4Addr,
) -> io::Result<()> {
    let mut request: libc::ifreq = unsafe { std::mem::zeroed() };
    fill_ifreq_name(&mut request, interface);
    request.ifr_ifru.ifru_addr = ipv4_sockaddr(address);
    if unsafe { libc::ioctl(fd, request_code as _, &request) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn add_default_ipv4_route_with_socket(
    fd: RawFd,
    gateway: std::net::Ipv4Addr,
    interface: &str,
) -> io::Result<()> {
    let interface = if interface.is_empty() {
        None
    } else {
        Some(CString::new(interface).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "interface name contains NUL")
        })?)
    };
    let mut route: libc::rtentry = unsafe { std::mem::zeroed() };
    route.rt_gateway = unsafe { std::mem::transmute(ipv4_sockaddr(gateway)) };
    route.rt_dst = unsafe { std::mem::transmute(ipv4_sockaddr(std::net::Ipv4Addr::UNSPECIFIED)) };
    route.rt_genmask =
        unsafe { std::mem::transmute(ipv4_sockaddr(std::net::Ipv4Addr::UNSPECIFIED)) };
    route.rt_flags = (libc::RTF_UP | libc::RTF_GATEWAY) as libc::c_ushort;
    if let Some(interface) = interface.as_ref() {
        route.rt_dev = interface.as_ptr() as *mut libc::c_char;
    }
    if unsafe { libc::ioctl(fd, libc::SIOCADDRT as _, &route) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn link_down_with_socket(fd: RawFd, interface: &str) -> io::Result<()> {
    let mut request: libc::ifreq = unsafe { std::mem::zeroed() };
    fill_ifreq_name(&mut request, interface);
    if unsafe { libc::ioctl(fd, libc::SIOCGIFFLAGS as _, &mut request) } != 0 {
        return Err(io::Error::last_os_error());
    }
    unsafe {
        request.ifr_ifru.ifru_flags &= !(libc::IFF_UP as libc::c_short);
    }
    if unsafe { libc::ioctl(fd, libc::SIOCSIFFLAGS as _, &request) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn flush_ipv4_addresses_with_socket(fd: RawFd, interface: &str) -> io::Result<()> {
    set_ifreq_sockaddr(
        fd,
        interface,
        libc::SIOCDIFADDR,
        std::net::Ipv4Addr::UNSPECIFIED,
    )
}

fn del_default_ipv4_route_with_socket(
    fd: RawFd,
    gateway: std::net::Ipv4Addr,
    interface: &str,
) -> io::Result<()> {
    let interface_c = CString::new(interface)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "interface name contains NUL"))?;
    let mut route: libc::rtentry = unsafe { std::mem::zeroed() };
    route.rt_gateway = unsafe { std::mem::transmute(ipv4_sockaddr(gateway)) };
    route.rt_dst = unsafe { std::mem::transmute(ipv4_sockaddr(std::net::Ipv4Addr::UNSPECIFIED)) };
    route.rt_genmask =
        unsafe { std::mem::transmute(ipv4_sockaddr(std::net::Ipv4Addr::UNSPECIFIED)) };
    route.rt_flags = (libc::RTF_UP | libc::RTF_GATEWAY) as libc::c_ushort;
    route.rt_dev = interface_c.as_ptr() as *mut libc::c_char;
    if unsafe { libc::ioctl(fd, libc::SIOCDELRT as _, &route) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Read the MAC address for `interface` from sysfs.
fn interface_mac(interface: &str) -> io::Result<[u8; 6]> {
    let path = format!("/sys/class/net/{interface}/address");
    let raw = std::fs::read_to_string(&path).map_err(|_| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("no address file at {path}"),
        )
    })?;
    let raw = raw.trim();
    let mut mac: [u8; 6] = [0; 6];
    for (i, byte) in raw.split(':').take(6).enumerate() {
        mac[i] = u8::from_str_radix(byte, 16).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, format!("invalid MAC: {raw}"))
        })?;
    }
    Ok(mac)
}

fn parse_ipv4_bytes(s: &str, label: &str) -> io::Result<[u8; 4]> {
    let addr: std::net::Ipv4Addr = s
        .parse()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, format!("invalid {label}")))?;
    let octets = addr.octets();
    Ok(octets)
}

fn build_dhcp_release(xid: u32, mac: &[u8; 6], address: &[u8; 4], server_id: &[u8; 4]) -> Vec<u8> {
    let mut packet = vec![0u8; 240];
    packet[0] = 1; // op: BOOTREQUEST
    packet[1] = 1; // htype: ethernet
    packet[2] = 6; // hlen
    packet[3] = 0; // hops
    packet[4..8].copy_from_slice(&xid.to_be_bytes());
    // secs, flags, ciaddr (leave ciaddr as zeros for RELEASE per RFC 2131)
    packet[16..20].copy_from_slice(address);
    packet[28..34].copy_from_slice(mac);
    // sname, file: zeros
    // DHCP magic cookie
    packet[236..240].copy_from_slice(&[99, 130, 83, 99]);
    // Option 53: DHCP RELEASE (7), then end
    let options: [u8; 19] = [
        53,
        1,
        7, // DHCP message type 7 (RELEASE)
        54,
        4,
        server_id[0],
        server_id[1],
        server_id[2],
        server_id[3], // server identifier
        61,
        7,
        1,
        mac[0],
        mac[1],
        mac[2],
        mac[3],
        mac[4],
        mac[5], // client identifier
        255,    // end
    ];
    packet.extend_from_slice(&options);
    packet
}

fn ipv4_sockaddr(address: std::net::Ipv4Addr) -> libc::sockaddr {
    let mut addr: libc::sockaddr_in = unsafe { std::mem::zeroed() };
    addr.sin_family = libc::AF_INET as libc::sa_family_t;
    addr.sin_addr = libc::in_addr {
        s_addr: u32::from(address).to_be(),
    };
    unsafe { std::mem::transmute(addr) }
}

fn fill_ifreq_name(request: &mut libc::ifreq, interface: &str) {
    for (index, byte) in interface.as_bytes().iter().copied().enumerate() {
        request.ifr_name[index] = byte as libc::c_char;
    }
}

fn validate_interface(interface: &str) -> io::Result<()> {
    if interface.is_empty() || interface.len() >= libc::IFNAMSIZ {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "interface name is empty or too long",
        ));
    }
    Ok(())
}

fn parse_ipv4(value: &str, label: &str) -> io::Result<std::net::Ipv4Addr> {
    value.parse().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid IPv4 {label}: {value}"),
        )
    })
}

fn interface_mtu(interface: &str) -> io::Result<u32> {
    validate_interface(interface)?;
    let fd = inet_dgram_socket()?;
    let mut request: libc::ifreq = unsafe { std::mem::zeroed() };
    fill_ifreq_name(&mut request, interface);
    let rc = unsafe { libc::ioctl(fd.as_raw_fd(), libc::SIOCGIFMTU as _, &mut request) };
    let error = io::Error::last_os_error();
    if rc == 0 {
        Ok(unsafe { request.ifr_ifru.ifru_mtu as u32 })
    } else {
        Err(error)
    }
}

fn interface_addresses(name: &str) -> io::Result<Vec<Value>> {
    let mut out = Vec::new();
    let mut addrs = std::ptr::null_mut::<libc::ifaddrs>();
    if unsafe { libc::getifaddrs(&mut addrs) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let mut cursor = addrs;
    while !cursor.is_null() {
        let ifa = unsafe { &*cursor };
        if !ifa.ifa_addr.is_null() {
            let current = unsafe { CStr::from_ptr(ifa.ifa_name) }.to_string_lossy();
            if current == name
                && let Some(record) = sockaddr_record(ifa.ifa_addr, ifa.ifa_netmask)
            {
                out.push(record);
            }
        }
        cursor = unsafe { (*cursor).ifa_next };
    }
    unsafe {
        libc::freeifaddrs(addrs);
    }
    Ok(out)
}

fn sockaddr_record(addr: *const libc::sockaddr, netmask: *const libc::sockaddr) -> Option<Value> {
    let family = unsafe { (*addr).sa_family as i32 };
    match family {
        libc::AF_INET => {
            let addr = ipv4_addr(addr)?;
            let prefix_len = ipv4_prefix_len(netmask)?;
            Some(Value::Record(crate::runtime::value::RecordMap::from([
                (Arc::from("family"), str_value("inet")),
                (Arc::from("addr"), str_value(addr)),
                (Arc::from("prefix_len"), Value::Int(prefix_len as i64)),
            ])))
        }
        libc::AF_INET6 => {
            let addr = ipv6_addr(addr)?;
            let prefix_len = ipv6_prefix_len(netmask)?;
            Some(Value::Record(crate::runtime::value::RecordMap::from([
                (Arc::from("family"), str_value("inet6")),
                (Arc::from("addr"), str_value(addr)),
                (Arc::from("prefix_len"), Value::Int(prefix_len as i64)),
            ])))
        }
        _ => None,
    }
}

fn ipv4_addr(addr: *const libc::sockaddr) -> Option<String> {
    if addr.is_null() {
        return None;
    }
    let addr = unsafe { &*(addr as *const libc::sockaddr_in) };
    Some(std::net::Ipv4Addr::from(u32::from_be(addr.sin_addr.s_addr)).to_string())
}

fn ipv6_addr(addr: *const libc::sockaddr) -> Option<String> {
    if addr.is_null() {
        return None;
    }
    let addr = unsafe { &*(addr as *const libc::sockaddr_in6) };
    Some(std::net::Ipv6Addr::from(addr.sin6_addr.s6_addr).to_string())
}

fn ipv4_prefix_len(mask: *const libc::sockaddr) -> Option<u32> {
    let mask = unsafe { &*(mask as *const libc::sockaddr_in) };
    Some(u32::from_be(mask.sin_addr.s_addr).count_ones())
}

fn ipv6_prefix_len(mask: *const libc::sockaddr) -> Option<u32> {
    let mask = unsafe { &*(mask as *const libc::sockaddr_in6) };
    Some(
        mask.sin6_addr
            .s6_addr
            .iter()
            .map(|byte| byte.count_ones())
            .sum(),
    )
}

fn interface_flag_names(flags: u32) -> Vec<&'static str> {
    let mut names = Vec::new();
    if flags & libc::IFF_UP as u32 != 0 {
        names.push("UP");
    }
    if flags & libc::IFF_BROADCAST as u32 != 0 {
        names.push("BROADCAST");
    }
    if flags & libc::IFF_LOOPBACK as u32 != 0 {
        names.push("LOOPBACK");
    }
    if flags & libc::IFF_RUNNING as u32 != 0 {
        names.push("RUNNING");
    }
    if flags & libc::IFF_MULTICAST as u32 != 0 {
        names.push("MULTICAST");
    }
    if flags & libc::IFF_PROMISC as u32 != 0 {
        names.push("PROMISC");
    }
    if flags & libc::IFF_NOARP as u32 != 0 {
        names.push("NOARP");
    }
    if flags & libc::IFF_POINTOPOINT as u32 != 0 {
        names.push("POINTOPOINT");
    }
    names
}

fn read_sys_u32(path: &Path) -> Option<u32> {
    let value = fs::read_to_string(path).ok()?;
    let trimmed = value.trim();
    trimmed.strip_prefix("0x").map_or_else(
        || trimmed.parse().ok(),
        |hex| u32::from_str_radix(hex, 16).ok(),
    )
}
