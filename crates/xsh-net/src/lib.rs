#![allow(clippy::single_call_fn)]

use async_executor::Executor;
use async_io::Async;
use crossbeam_channel::RecvTimeoutError;
use h12tiny_client::{
    Client as H12Client, Connector as H12Connector, Error as H12Error, ErrorKind as H12ErrorKind,
    TcpConnected, TcpDialFuture, TcpDialer,
};
use http::{Request, Response};
use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Incoming};
use rustc_hash::FxHashSet;
use rustix::fd::OwnedFd;
use rustix::net as rnet;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme};
use rustls_graviola::default_provider;
#[cfg(target_os = "macos")]
use rustls_platform_verifier::BuilderVerifierExt;
use std::ffi::{CStr, CString};
use std::fs::{self, File, OpenOptions};
use std::future::Future;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{self, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs, UdpSocket};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NetError {
    pub kind: String,
    pub message: String,
}

impl NetError {
    fn new(kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
        }
    }

    fn from_io(prefix: &'static str, error: io::Error) -> Self {
        Self::new(prefix, error.to_string())
    }
}

impl std::fmt::Display for NetError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.kind, self.message)
    }
}

impl std::error::Error for NetError {}

pub type NetResult<T> = Result<T, NetError>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AddressFamily {
    Any,
    V4,
    V6,
}

impl AddressFamily {
    pub fn from_name(name: &str) -> NetResult<Self> {
        match name {
            "any" | "Any" | "ANY" => Ok(Self::Any),
            "4" | "v4" | "V4" | "ipv4" | "IPv4" | "IPV4" => Ok(Self::V4),
            "6" | "v6" | "V6" | "ipv6" | "IPv6" | "IPV6" => Ok(Self::V6),
            _ => Err(NetError::new(
                "dns-family",
                "family must be `any`, `ipv4`, or `ipv6`",
            )),
        }
    }

    fn keeps(self, addr: IpAddr) -> bool {
        matches!(
            (self, addr),
            (Self::Any, _) | (Self::V4, IpAddr::V4(_)) | (Self::V6, IpAddr::V6(_))
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DnsRecord {
    pub name: String,
    pub record: String,
    pub value: String,
    pub ttl: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostAddress {
    pub name: String,
    pub family: String,
    pub addr: String,
}

pub fn lookup(
    name: &str,
    record: &str,
    server: &str,
    timeout: Duration,
) -> NetResult<Vec<DnsRecord>> {
    validate_name(name)?;
    let record = DnsRecordKind::from_name(record)?;
    if !server.is_empty() {
        return lookup_dns_server(name, record, server, timeout);
    }
    let family = record.family();
    let addrs = resolve_socket_addrs(name, 0, family, Some(timeout))?;
    let mut seen = FxHashSet::default();
    let mut records = Vec::new();
    for addr in addrs {
        let ip = addr.ip();
        if seen.insert(ip) {
            records.push(dns_record(name, record.name(), &ip.to_string(), 0));
        }
    }
    if records.is_empty() {
        Err(NetError::new("dns-not-found", "no records found"))
    } else {
        Ok(records)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DnsRecordKind {
    A,
    Aaaa,
}

impl DnsRecordKind {
    fn from_name(name: &str) -> NetResult<Self> {
        match name.to_ascii_uppercase().as_str() {
            "A" => Ok(Self::A),
            "AAAA" => Ok(Self::Aaaa),
            _ => Err(NetError::new("dns-record", "record must be `A` or `AAAA`")),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::A => "A",
            Self::Aaaa => "AAAA",
        }
    }

    fn family(self) -> AddressFamily {
        match self {
            Self::A => AddressFamily::V4,
            Self::Aaaa => AddressFamily::V6,
        }
    }

    fn qtype(self) -> u16 {
        match self {
            Self::A => 1,
            Self::Aaaa => 28,
        }
    }
}

fn lookup_dns_server(
    name: &str,
    record: DnsRecordKind,
    server: &str,
    timeout: Duration,
) -> NetResult<Vec<DnsRecord>> {
    let server = parse_dns_server(server)?;
    let mut query = Vec::with_capacity(512);
    let id = dns_query_id(name, record, server);
    query.extend_from_slice(&id.to_be_bytes());
    query.extend_from_slice(&0x0100_u16.to_be_bytes());
    query.extend_from_slice(&1_u16.to_be_bytes());
    query.extend_from_slice(&0_u16.to_be_bytes());
    query.extend_from_slice(&0_u16.to_be_bytes());
    query.extend_from_slice(&0_u16.to_be_bytes());
    write_dns_name(name, &mut query)?;
    query.extend_from_slice(&record.qtype().to_be_bytes());
    query.extend_from_slice(&1_u16.to_be_bytes());
    if query.len() > 512 {
        return Err(NetError::new("dns-name", "DNS query name is too long"));
    }

    let bind_addr = if server.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    };
    let socket =
        UdpSocket::bind(bind_addr).map_err(|error| NetError::from_io("dns-server", error))?;
    socket
        .set_read_timeout(Some(timeout))
        .map_err(|error| NetError::from_io("dns-server", error))?;
    socket
        .set_write_timeout(Some(timeout))
        .map_err(|error| NetError::from_io("dns-server", error))?;
    socket.send_to(&query, server).map_err(dns_io_error)?;

    let mut response = [0_u8; 1232];
    let (len, peer) = socket.recv_from(&mut response).map_err(dns_io_error)?;
    if peer != server {
        return Err(NetError::new(
            "dns-response",
            "DNS response came from a different server",
        ));
    }
    parse_dns_response(&response[..len], id, name, record)
}

fn parse_dns_server(server: &str) -> NetResult<SocketAddr> {
    validate_name(server)?;
    if let Ok(addr) = server.parse::<SocketAddr>() {
        return Ok(addr);
    }
    if let Ok(ip) = server.parse::<IpAddr>() {
        return Ok(SocketAddr::new(ip, 53));
    }
    let has_port = server
        .rsplit_once(':')
        .and_then(|(_, port)| port.parse::<u16>().ok())
        .is_some();
    let target = if has_port {
        server.to_string()
    } else {
        format!("{server}:53")
    };
    target
        .to_socket_addrs()
        .map_err(|error| NetError::from_io("dns-server", error))?
        .next()
        .ok_or_else(|| NetError::new("dns-server", "no server addresses found"))
}

fn dns_query_id(name: &str, record: DnsRecordKind, server: SocketAddr) -> u16 {
    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    record.qtype().hash(&mut hasher);
    server.hash(&mut hasher);
    std::process::id().hash(&mut hasher);
    if let Ok(now) = SystemTime::now().duration_since(UNIX_EPOCH) {
        now.as_nanos().hash(&mut hasher);
    }
    hasher.finish() as u16
}

fn write_dns_name(name: &str, out: &mut Vec<u8>) -> NetResult<()> {
    let name = name.trim_end_matches('.');
    if name.is_empty() {
        return Err(NetError::new("dns-name", "name cannot be empty"));
    }
    if name.len() > 253 {
        return Err(NetError::new("dns-name", "DNS name is too long"));
    }
    for label in name.split('.') {
        if label.is_empty() {
            return Err(NetError::new("dns-name", "DNS label cannot be empty"));
        }
        if label.len() > 63 {
            return Err(NetError::new("dns-name", "DNS label is too long"));
        }
        out.push(label.len() as u8);
        out.extend_from_slice(label.as_bytes());
    }
    out.push(0);
    Ok(())
}

fn parse_dns_response(
    response: &[u8],
    id: u16,
    requested_name: &str,
    record: DnsRecordKind,
) -> NetResult<Vec<DnsRecord>> {
    if response.len() < 12 {
        return Err(NetError::new("dns-response", "short DNS response"));
    }
    if read_u16(response, 0)? != id {
        return Err(NetError::new("dns-response", "DNS response ID mismatch"));
    }
    let flags = read_u16(response, 2)?;
    if flags & 0x8000 == 0 {
        return Err(NetError::new(
            "dns-response",
            "DNS response is not a response",
        ));
    }
    if flags & 0x0200 != 0 {
        return Err(NetError::new("dns-truncated", "truncated DNS response"));
    }
    match flags & 0x000f {
        0 => {}
        3 => return Err(NetError::new("dns-not-found", "no records found")),
        2 => return Err(NetError::new("dns-server", "DNS server failure")),
        code => {
            return Err(NetError::new(
                "dns-response",
                format!("DNS response code {code}"),
            ));
        }
    }

    let qdcount = read_u16(response, 4)? as usize;
    let ancount = read_u16(response, 6)? as usize;
    let mut offset = 12;
    for _ in 0..qdcount {
        let (_, next) = read_dns_name(response, offset)?;
        offset = next
            .checked_add(4)
            .ok_or_else(|| NetError::new("dns-response", "invalid DNS question"))?;
        if offset > response.len() {
            return Err(NetError::new("dns-response", "short DNS question"));
        }
    }

    let mut records = Vec::new();
    for _ in 0..ancount {
        let (name, next) = read_dns_name(response, offset)?;
        offset = next;
        if offset + 10 > response.len() {
            return Err(NetError::new("dns-response", "short DNS answer"));
        }
        let qtype = read_u16(response, offset)?;
        let class = read_u16(response, offset + 2)?;
        let ttl = read_u32(response, offset + 4)? as i64;
        let rdlen = read_u16(response, offset + 8)? as usize;
        offset += 10;
        if offset + rdlen > response.len() {
            return Err(NetError::new("dns-response", "short DNS record data"));
        }
        let data = &response[offset..offset + rdlen];
        offset += rdlen;
        if class != 1 || qtype != record.qtype() {
            continue;
        }
        let value = match record {
            DnsRecordKind::A if data.len() == 4 => {
                IpAddr::V4(Ipv4Addr::new(data[0], data[1], data[2], data[3])).to_string()
            }
            DnsRecordKind::Aaaa if data.len() == 16 => {
                let mut octets = [0_u8; 16];
                octets.copy_from_slice(data);
                IpAddr::V6(Ipv6Addr::from(octets)).to_string()
            }
            _ => continue,
        };
        let name = if name.is_empty() {
            requested_name
        } else {
            &name
        };
        records.push(dns_record(name, record.name(), &value, ttl));
    }

    if records.is_empty() {
        Err(NetError::new("dns-not-found", "no records found"))
    } else {
        Ok(records)
    }
}

fn read_dns_name(message: &[u8], offset: usize) -> NetResult<(String, usize)> {
    let mut labels = Vec::new();
    let mut pos = offset;
    let mut next_offset = offset;
    let mut jumped = false;
    let mut jumps = 0;
    loop {
        if pos >= message.len() {
            return Err(NetError::new("dns-response", "short DNS name"));
        }
        let len = message[pos];
        if len & 0xc0 == 0xc0 {
            if pos + 1 >= message.len() {
                return Err(NetError::new(
                    "dns-response",
                    "short DNS compression pointer",
                ));
            }
            let pointer = (((len & 0x3f) as usize) << 8) | message[pos + 1] as usize;
            if pointer >= message.len() {
                return Err(NetError::new(
                    "dns-response",
                    "DNS compression pointer is out of range",
                ));
            }
            if !jumped {
                next_offset = pos + 2;
            }
            jumped = true;
            pos = pointer;
            jumps += 1;
            if jumps > message.len() {
                return Err(NetError::new(
                    "dns-response",
                    "DNS compression pointer loop",
                ));
            }
            continue;
        }
        if len & 0xc0 != 0 {
            return Err(NetError::new(
                "dns-response",
                "unsupported DNS label encoding",
            ));
        }
        pos += 1;
        if len == 0 {
            if !jumped {
                next_offset = pos;
            }
            break;
        }
        let len = len as usize;
        if pos + len > message.len() {
            return Err(NetError::new("dns-response", "short DNS label"));
        }
        labels.push(String::from_utf8_lossy(&message[pos..pos + len]).into_owned());
        pos += len;
    }
    Ok((labels.join("."), next_offset))
}

fn read_u16(message: &[u8], offset: usize) -> NetResult<u16> {
    if offset + 2 > message.len() {
        return Err(NetError::new("dns-response", "short DNS integer"));
    }
    Ok(u16::from_be_bytes([message[offset], message[offset + 1]]))
}

fn read_u32(message: &[u8], offset: usize) -> NetResult<u32> {
    if offset + 4 > message.len() {
        return Err(NetError::new("dns-response", "short DNS integer"));
    }
    Ok(u32::from_be_bytes([
        message[offset],
        message[offset + 1],
        message[offset + 2],
        message[offset + 3],
    ]))
}

pub fn resolve_host(name: &str, family: AddressFamily) -> NetResult<Vec<HostAddress>> {
    validate_name(name)?;
    let addrs = resolve_socket_addrs(name, 0, family, None)?;
    let mut seen = FxHashSet::default();
    let mut records = Vec::new();
    for addr in addrs {
        let ip = addr.ip();
        if seen.insert(ip) {
            let family = if ip.is_ipv4() { "ipv4" } else { "ipv6" };
            records.push(HostAddress {
                name: name.to_string(),
                family: family.to_string(),
                addr: ip.to_string(),
            });
        }
    }
    if records.is_empty() {
        Err(NetError::new("dns-not-found", "no addresses found"))
    } else {
        Ok(records)
    }
}

pub fn reverse(addr: &str) -> NetResult<Vec<String>> {
    let ip = addr
        .parse::<IpAddr>()
        .map_err(|_| NetError::new("dns-address", "invalid IP address"))?;
    let name = reverse_lookup(ip)?;
    Ok(vec![name])
}

pub fn nameservers() -> NetResult<Vec<String>> {
    let text = std::fs::read_to_string("/etc/resolv.conf")
        .map_err(|error| NetError::from_io("dns-nameservers", error))?;
    Ok(text
        .lines()
        .filter_map(|line| {
            let line = line.split('#').next().unwrap_or("").trim();
            let mut parts = line.split_whitespace();
            if parts.next()? != "nameserver" {
                return None;
            }
            parts.next().map(str::to_string)
        })
        .collect())
}

pub fn resolve_socket_addrs(
    host: &str,
    port: u16,
    family: AddressFamily,
    timeout: Option<Duration>,
) -> NetResult<Vec<SocketAddr>> {
    validate_name(host)?;
    if let Ok(ip) = host.parse::<IpAddr>() {
        return if family.keeps(ip) {
            Ok(vec![SocketAddr::new(ip, port)])
        } else {
            Err(NetError::new("dns-not-found", "no addresses found"))
        };
    }

    let resolved = match timeout {
        Some(timeout) => resolve_with_timeout(host.to_string(), port, timeout)?,
        None => (host, port)
            .to_socket_addrs()
            .map_err(dns_io_error)?
            .collect(),
    };
    let addrs = resolved
        .into_iter()
        .filter(|addr| family.keeps(addr.ip()))
        .collect::<Vec<_>>();
    if addrs.is_empty() {
        Err(NetError::new("dns-not-found", "no addresses found"))
    } else {
        Ok(addrs)
    }
}

fn resolve_with_timeout(host: String, port: u16, timeout: Duration) -> NetResult<Vec<SocketAddr>> {
    let (tx, rx) = crossbeam_channel::bounded(1);
    thread::spawn(move || {
        let result = (host.as_str(), port)
            .to_socket_addrs()
            .map(|iter| iter.collect::<Vec<_>>());
        let _ = tx.send(result);
    });
    match rx.recv_timeout(timeout) {
        Ok(Ok(addrs)) => Ok(addrs),
        Ok(Err(error)) => Err(dns_io_error(error)),
        Err(RecvTimeoutError::Timeout) => Err(NetError::new("dns-timeout", "DNS lookup timed out")),
        Err(RecvTimeoutError::Disconnected) => {
            Err(NetError::new("dns", "DNS lookup worker stopped"))
        }
    }
}

async fn async_resolve_socket_addrs(
    host: String,
    port: u16,
    family: AddressFamily,
) -> NetResult<Vec<SocketAddr>> {
    validate_name(&host)?;
    if let Ok(ip) = host.parse::<IpAddr>() {
        return if family.keeps(ip) {
            Ok(vec![SocketAddr::new(ip, port)])
        } else {
            Err(NetError::new("dns-not-found", "no addresses found"))
        };
    }

    let resolved = async_net::resolve((host, port))
        .await
        .map_err(dns_io_error)?;
    let addrs = resolved
        .into_iter()
        .filter(|addr| family.keeps(addr.ip()))
        .collect::<Vec<_>>();
    if addrs.is_empty() {
        Err(NetError::new("dns-not-found", "no addresses found"))
    } else {
        Ok(addrs)
    }
}

fn reverse_lookup(ip: IpAddr) -> NetResult<String> {
    let mut host = vec![0 as libc::c_char; 1025];
    let flags = libc::NI_NAMEREQD;
    let result = match ip {
        IpAddr::V4(ip) => {
            let mut sockaddr: libc::sockaddr_in = unsafe { std::mem::zeroed() };
            #[cfg(any(
                target_os = "macos",
                target_os = "ios",
                target_os = "freebsd",
                target_os = "openbsd",
                target_os = "netbsd"
            ))]
            {
                sockaddr.sin_len = std::mem::size_of::<libc::sockaddr_in>() as u8;
            };
            sockaddr.sin_family = libc::AF_INET as libc::sa_family_t;
            sockaddr.sin_port = 0;
            sockaddr.sin_addr = libc::in_addr {
                s_addr: u32::from_be_bytes(ip.octets()).to_be(),
            };
            unsafe {
                libc::getnameinfo(
                    (&sockaddr as *const libc::sockaddr_in).cast::<libc::sockaddr>(),
                    std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t,
                    host.as_mut_ptr(),
                    host.len() as libc::socklen_t,
                    std::ptr::null_mut(),
                    0,
                    flags,
                )
            }
        }
        IpAddr::V6(ip) => {
            let mut sockaddr: libc::sockaddr_in6 = unsafe { std::mem::zeroed() };
            #[cfg(any(
                target_os = "macos",
                target_os = "ios",
                target_os = "freebsd",
                target_os = "openbsd",
                target_os = "netbsd"
            ))]
            {
                sockaddr.sin6_len = std::mem::size_of::<libc::sockaddr_in6>() as u8;
            };
            sockaddr.sin6_family = libc::AF_INET6 as libc::sa_family_t;
            sockaddr.sin6_port = 0;
            sockaddr.sin6_flowinfo = 0;
            sockaddr.sin6_addr = libc::in6_addr {
                s6_addr: ip.octets(),
            };
            sockaddr.sin6_scope_id = 0;
            unsafe {
                libc::getnameinfo(
                    (&sockaddr as *const libc::sockaddr_in6).cast::<libc::sockaddr>(),
                    std::mem::size_of::<libc::sockaddr_in6>() as libc::socklen_t,
                    host.as_mut_ptr(),
                    host.len() as libc::socklen_t,
                    std::ptr::null_mut(),
                    0,
                    flags,
                )
            }
        }
    };
    if result != 0 {
        let message = unsafe { CStr::from_ptr(libc::gai_strerror(result)) }
            .to_string_lossy()
            .into_owned();
        return Err(NetError::new("dns-reverse", message));
    }
    let name = unsafe { CStr::from_ptr(host.as_ptr()) }
        .to_string_lossy()
        .into_owned();
    Ok(name)
}

fn validate_name(name: &str) -> NetResult<()> {
    if name.is_empty() {
        return Err(NetError::new("dns-name", "name cannot be empty"));
    }
    CString::new(name)
        .map(|_| ())
        .map_err(|_| NetError::new("dns-name", "name cannot contain NUL"))
}

fn dns_record(name: &str, record: &str, value: &str, ttl: i64) -> DnsRecord {
    DnsRecord {
        name: name.to_string(),
        record: record.to_string(),
        value: value.to_string(),
        ttl,
    }
}

fn dns_io_error(error: io::Error) -> NetError {
    let kind = match error.kind() {
        io::ErrorKind::TimedOut => "dns-timeout",
        io::ErrorKind::NotFound => "dns-not-found",
        _ => "dns-lookup",
    };
    NetError::new(kind, error.to_string())
}

#[derive(Clone)]
pub struct NetAgent {
    tls_config: Arc<ClientConfig>,
    max_idle_per_host: usize,
    idle_timeout: Duration,
    executor: Arc<Executor<'static>>,
    h1_client: H12Client<Full<Bytes>>,
}

impl std::fmt::Debug for NetAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetAgent").finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct NetAgentKey {
    pub pool: String,
    pub tls_verify: bool,
    pub ca_certificate: Option<PathBuf>,
    pub max_idle_per_host: usize,
    pub idle_timeout: Duration,
}

#[derive(Clone, Debug)]
pub struct NetPoolOptions {
    pub max_idle_per_host: usize,
    pub idle_timeout: Duration,
}

impl Default for NetPoolOptions {
    fn default() -> Self {
        Self {
            max_idle_per_host: 8,
            idle_timeout: Duration::from_secs(90),
        }
    }
}

#[derive(Clone, Debug)]
pub struct NetRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<NetHeader>,
    pub body: NetBody,
    pub timeout: Option<Duration>,
    pub connect_timeout: Option<Duration>,
    pub redirects: usize,
    pub fail_status: bool,
    pub max_body_bytes: u64,
}

#[derive(Clone, Debug)]
pub struct NetDownload {
    pub url: String,
    pub dest: PathBuf,
    pub headers: Vec<NetHeader>,
    pub timeout: Option<Duration>,
    pub connect_timeout: Option<Duration>,
    pub redirects: usize,
    pub fail_status: bool,
    pub max_body_bytes: Option<u64>,
    pub atomic: bool,
    pub overwrite: bool,
}

#[derive(Clone, Debug)]
pub struct NetUpload {
    pub method: String,
    pub url: String,
    pub source: PathBuf,
    pub headers: Vec<NetHeader>,
    pub timeout: Option<Duration>,
    pub connect_timeout: Option<Duration>,
    pub redirects: usize,
    pub fail_status: bool,
    pub max_body_bytes: u64,
}

#[derive(Clone, Debug)]
pub struct NetHeader {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug)]
pub enum NetBody {
    Empty,
    Bytes(Vec<u8>),
    File(PathBuf),
}

#[derive(Clone, Debug)]
pub struct NetCallOptions {
    pub pool: String,
    pub tls_verify: bool,
    pub ca_certificate: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct NetResponse {
    pub status: i64,
    pub reason: String,
    pub bytes: i64,
    pub headers: Vec<NetHeader>,
    pub url: String,
    pub body: Option<Vec<u8>>,
}

pub fn make_agent(key: &NetAgentKey) -> NetResult<NetAgent> {
    let tls_config = tls_config_for_key(key)?;
    let executor = Arc::new(Executor::new());
    let h1_client = h12_client(
        Arc::clone(&executor),
        tls_config.clone(),
        key.max_idle_per_host,
        key.idle_timeout,
        true,
    );
    Ok(NetAgent {
        tls_config: Arc::new(tls_config),
        max_idle_per_host: key.max_idle_per_host,
        idle_timeout: key.idle_timeout,
        executor,
        h1_client,
    })
}

fn h12_client(
    executor: Arc<Executor<'static>>,
    mut tls_config: ClientConfig,
    max_idle_per_host: usize,
    idle_timeout: Duration,
    http1_only: bool,
) -> H12Client<Full<Bytes>> {
    if !http1_only {
        tls_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    }
    let connector = H12Connector::builder()
        .tcp_dialer(ResolvedTcpDialer)
        .tls_config(tls_config)
        .build();
    let mut builder = H12Client::builder(h12_executor(executor));
    builder
        .connector(connector)
        .pool_max_idle_per_host(max_idle_per_host)
        .pool_idle_timeout(idle_timeout);
    if http1_only {
        builder.http1_only();
    }
    builder.build()
}

type H12Task = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

#[derive(Clone)]
struct H12Executor(Arc<Executor<'static>>);

impl hyper::rt::Executor<H12Task> for H12Executor {
    fn execute(&self, future: H12Task) {
        self.0.spawn(future).detach();
    }
}

fn h12_executor(executor: Arc<Executor<'static>>) -> H12Executor {
    H12Executor(executor)
}

struct ResolvedTcpDialer;

impl TcpDialer for ResolvedTcpDialer {
    fn connect(&self, origin: http::Uri) -> TcpDialFuture {
        Box::pin(async move {
            let host = origin
                .host()
                .ok_or_else(|| -> h12tiny_client::DialError {
                    Box::new(NetError::new("net-url", "URL must include a host"))
                })?
                .strip_prefix('[')
                .and_then(|host| host.strip_suffix(']'))
                .unwrap_or_else(|| origin.host().expect("host was checked above"))
                .to_string();
            let port = match origin.scheme_str() {
                Some("http") => origin.port_u16().unwrap_or(80),
                Some("https") => origin.port_u16().unwrap_or(443),
                Some(_) => {
                    return Err(Box::new(NetError::new(
                        "net-scheme",
                        "URL scheme must be http or https",
                    )) as _);
                }
                None => {
                    return Err(
                        Box::new(NetError::new("net-url", "URL must include a scheme")) as _,
                    );
                }
            };
            let addresses = async_resolve_socket_addrs(host, port, AddressFamily::Any)
                .await
                .map_err(|error| -> h12tiny_client::DialError { Box::new(error) })?;
            let stream = async_connect_resolved_tcp(addresses).await.map_err(
                |error| -> h12tiny_client::DialError { Box::new(net_transport_error(error)) },
            )?;
            let local_addr = stream.get_ref().local_addr().ok();
            let peer_addr = stream.get_ref().peer_addr().ok();
            Ok(TcpConnected::new(stream).with_addresses(local_addr, peer_addr))
        })
    }
}

async fn async_connect_resolved_tcp(
    addrs: Vec<SocketAddr>,
) -> io::Result<Async<std::net::TcpStream>> {
    let mut last_error = None;
    for addr in addrs {
        let family = match addr {
            SocketAddr::V4(_) => rnet::AddressFamily::INET,
            SocketAddr::V6(_) => rnet::AddressFamily::INET6,
        };
        let socket = rnet::socket_with(family, rnet::SocketType::STREAM, tcp_socket_flags(), None)?;
        configure_tcp_socket(&socket)?;
        match rnet::connect(&socket, &addr) {
            Ok(()) => match Async::new(std::net::TcpStream::from(socket)) {
                Ok(stream) => return Ok(stream),
                Err(error) => last_error = Some(error),
            },
            Err(error) if connect_is_pending(&error.into()) => {
                let stream = match Async::new(std::net::TcpStream::from(socket)) {
                    Ok(stream) => stream,
                    Err(error) => {
                        last_error = Some(error);
                        continue;
                    }
                };
                stream.writable().await?;
                match stream.get_ref().take_error()? {
                    Some(error) => last_error = Some(error),
                    None => return Ok(stream),
                }
            }
            Err(error) => last_error = Some(error.into()),
        }
    }
    Err(last_error.unwrap_or_else(|| io::Error::other("dns-not-found: no addresses found")))
}

#[cfg(not(target_vendor = "apple"))]
fn tcp_socket_flags() -> rnet::SocketFlags {
    rnet::SocketFlags::CLOEXEC | rnet::SocketFlags::NONBLOCK
}

#[cfg(target_vendor = "apple")]
fn tcp_socket_flags() -> rnet::SocketFlags {
    rnet::SocketFlags::empty()
}

#[cfg(not(target_vendor = "apple"))]
fn configure_tcp_socket(_socket: &OwnedFd) -> io::Result<()> {
    Ok(())
}

#[cfg(target_vendor = "apple")]
fn configure_tcp_socket(socket: &OwnedFd) -> io::Result<()> {
    rustix::io::ioctl_fioclex(socket)?;
    rustix::io::ioctl_fionbio(socket, true)?;
    Ok(())
}

fn connect_is_pending(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::WouldBlock
        || matches!(
            error.raw_os_error(),
            Some(libc::EINPROGRESS | libc::EALREADY | libc::EWOULDBLOCK)
        )
}

fn h12_block_on<T>(agent: &NetAgent, future: impl Future<Output = NetResult<T>>) -> NetResult<T> {
    futures_lite::future::block_on(agent.executor.run(future))
}

fn h12_error(error: H12Error) -> NetError {
    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(&error);
    while let Some(error) = source {
        if let Some(error) = error.downcast_ref::<NetError>() {
            return error.clone();
        }
        source = error.source();
    }
    let kind = match error.kind() {
        H12ErrorKind::UnsupportedScheme => "net-scheme",
        H12ErrorKind::AbsoluteUriRequired => "net-url",
        H12ErrorKind::Tls | H12ErrorKind::Alpn => "net-tls",
        H12ErrorKind::Connect
        | H12ErrorKind::Handshake
        | H12ErrorKind::SendRequest
        | H12ErrorKind::Canceled => "net-io",
        H12ErrorKind::UnsupportedMethod | H12ErrorKind::UnsupportedVersion => "net-request",
        H12ErrorKind::ProtocolUnavailable => "net-protocol",
        _ => "net-io",
    };
    NetError::new(kind, error.to_string())
}

fn h12_request(request: &HttpRequest) -> NetResult<Request<Full<Bytes>>> {
    let mut builder = Request::builder()
        .method(request.method.as_str())
        .uri(request.url.absolute());
    let mut has_content_length = false;
    for header in &request.headers {
        if header.name.eq_ignore_ascii_case("content-length") {
            has_content_length = true;
        }
        builder = builder.header(&header.name, &header.value);
    }
    if !request.body.is_empty() && !has_content_length {
        builder = builder.header("Content-Length", request.body.len());
    }
    builder
        .body(Full::new(Bytes::copy_from_slice(&request.body)))
        .map_err(|error| NetError::new("net-request", error.to_string()))
}

async fn h12_send(
    client: &H12Client<Full<Bytes>>,
    request: &HttpRequest,
) -> NetResult<Response<Incoming>> {
    client
        .request(h12_request(request)?)
        .await
        .map_err(h12_error)
}

async fn h12_collect_response(
    response: Response<Incoming>,
    max_body_bytes: u64,
) -> NetResult<HttpResponse> {
    let status = response.status().as_u16();
    let reason = response
        .status()
        .canonical_reason()
        .unwrap_or("")
        .to_string();
    let headers = response
        .headers()
        .iter()
        .map(|(name, value)| NetHeader {
            name: response_header_name(name.as_str()),
            value: value.to_str().unwrap_or_default().to_string(),
        })
        .collect();
    let mut body = response.into_body();
    let mut bytes = Vec::new();
    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(|error| NetError::new("net-io", error.to_string()))?;
        if let Ok(data) = frame.into_data() {
            let total = (bytes.len() as u64).saturating_add(data.len() as u64);
            if total > max_body_bytes {
                return Err(NetError::new(
                    "net-body-limit",
                    format!("response body exceeds {max_body_bytes} bytes"),
                ));
            }
            bytes.extend_from_slice(&data);
        }
    }
    Ok(HttpResponse {
        status,
        reason,
        headers,
        body: bytes,
    })
}

async fn h12_discard_response(response: Response<Incoming>) -> NetResult<()> {
    let mut body = response.into_body();
    while let Some(frame) = body.frame().await {
        frame.map_err(|error| NetError::new("net-io", error.to_string()))?;
    }
    Ok(())
}

fn response_header_name(name: &str) -> String {
    name.split('-')
        .map(|part| {
            let mut characters = part.chars();
            let Some(first) = characters.next() else {
                return String::new();
            };
            format!("{}{}", first.to_ascii_uppercase(), characters.as_str())
        })
        .collect::<Vec<_>>()
        .join("-")
}

async fn h12_request_with_redirects(
    client: &H12Client<Full<Bytes>>,
    mut request: HttpRequest,
    config: &RequestConfig,
    max_body_bytes: u64,
) -> NetResult<HttpResponse> {
    for _ in 0..=config.redirects {
        let response = h12_send(client, &request).await?;
        let status = response.status().as_u16();
        if !is_redirect(status) {
            let response = h12_collect_response(response, max_body_bytes).await?;
            if config.fail_status && !(200..300).contains(&response.status) {
                return Err(NetError::new(
                    "net-status",
                    format!("HTTP status {}", response.status),
                ));
            }
            return Ok(response);
        }
        let location = response
            .headers()
            .get("location")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let Some(location) = location else {
            return Err(NetError::new(
                "net-redirect",
                "redirect missing Location header",
            ));
        };
        h12_discard_response(response).await?;
        request.url = redirect_url(&request.url, &location)?;
    }
    Err(NetError::new("net-redirect", "too many redirects"))
}

async fn h12_response_record(
    client: &H12Client<Full<Bytes>>,
    request: HttpRequest,
    config: &RequestConfig,
    max_body_bytes: u64,
    url: &str,
    include_body: bool,
) -> NetResult<NetResponse> {
    let response = h12_request_with_redirects(client, request, config, max_body_bytes).await?;
    response_record(response, max_body_bytes, url, include_body)
}

async fn h12_write_download_response(
    response: Response<Incoming>,
    path: &Path,
    limit: u64,
    overwrite: bool,
) -> NetResult<NetResponse> {
    let status = response.status().as_u16() as i64;
    let reason = response
        .status()
        .canonical_reason()
        .unwrap_or("")
        .to_string();
    let headers = response
        .headers()
        .iter()
        .map(|(name, value)| NetHeader {
            name: response_header_name(name.as_str()),
            value: value.to_str().unwrap_or_default().to_string(),
        })
        .collect();
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .create_new(!overwrite)
        .truncate(overwrite)
        .open(path)
        .map_err(|error| NetError::from_io("net-write", error))?;
    let mut body = response.into_body();
    let mut bytes = 0_u64;
    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(|error| NetError::new("net-io", error.to_string()))?;
        if let Ok(data) = frame.into_data() {
            bytes = bytes.saturating_add(data.len() as u64);
            if bytes > limit {
                return Err(NetError::new(
                    "net-body-limit",
                    format!("response body exceeds {limit} bytes"),
                ));
            }
            file.write_all(&data)
                .map_err(|error| NetError::from_io("net-write", error))?;
        }
    }
    Ok(NetResponse {
        status,
        reason,
        bytes: bytes as i64,
        headers,
        url: String::new(),
        body: None,
    })
}

async fn h12_download_with_redirects(
    client: &H12Client<Full<Bytes>>,
    request: HttpRequest,
    config: &RequestConfig,
    download: &NetDownload,
) -> NetResult<NetResponse> {
    if !download.overwrite && download.dest.exists() {
        return Err(NetError::new("net-dest", "destination exists"));
    }
    if let Some(parent) = download.dest.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|error| NetError::from_io("net-dest", error))?;
    }
    let output = if download.atomic {
        temp_download_path(&download.dest)
    } else {
        download.dest.clone()
    };
    let mut request = request;
    for _ in 0..=config.redirects {
        let response = h12_send(client, &request).await?;
        let status = response.status().as_u16();
        if is_redirect(status) {
            let location = response
                .headers()
                .get("location")
                .and_then(|value| value.to_str().ok())
                .map(str::to_string);
            let Some(location) = location else {
                let _ = fs::remove_file(&output);
                return Err(NetError::new(
                    "net-redirect",
                    "redirect missing Location header",
                ));
            };
            h12_discard_response(response).await?;
            request.url = redirect_url(&request.url, &location)?;
            continue;
        }
        if config.fail_status && !(200..300).contains(&status) {
            h12_discard_response(response).await?;
            return Err(NetError::new("net-status", format!("HTTP status {status}")));
        }
        let result = h12_write_download_response(
            response,
            &output,
            download.max_body_bytes.unwrap_or(u64::MAX),
            download.overwrite,
        )
        .await;
        let response = match result {
            Ok(response) => response,
            Err(error) => {
                let _ = fs::remove_file(&output);
                return Err(error);
            }
        };
        if download.atomic {
            if download.overwrite && download.dest.exists() {
                fs::remove_file(&download.dest).map_err(|error| {
                    let _ = fs::remove_file(&output);
                    NetError::from_io("net-rename", error)
                })?;
            }
            fs::rename(&output, &download.dest).map_err(|error| {
                let _ = fs::remove_file(&output);
                NetError::from_io("net-rename", error)
            })?;
        }
        return Ok(NetResponse {
            url: download.url.clone(),
            ..response
        });
    }
    let _ = fs::remove_file(&output);
    Err(NetError::new("net-redirect", "too many redirects"))
}

async fn h12_request_worker(
    client: H12Client<Full<Bytes>>,
    requests: Vec<(usize, NetRequest)>,
) -> Vec<(usize, NetResult<NetResponse>)> {
    let mut responses = Vec::with_capacity(requests.len());
    for (index, request) in requests {
        let config = request_config(&request);
        let max_body_bytes = request.max_body_bytes;
        let url = request.url.clone();
        let result = async_with_timeout(config.timeout.or(config.connect_timeout), async {
            let request = request_builder(&request)?;
            h12_response_record(&client, request, &config, max_body_bytes, &url, true).await
        })
        .await;
        responses.push((index, result));
    }
    responses
}

async fn h12_download_worker(
    client: H12Client<Full<Bytes>>,
    downloads: Vec<(usize, NetDownload)>,
) -> Vec<(usize, NetResult<NetResponse>)> {
    let mut responses = Vec::with_capacity(downloads.len());
    for (index, download) in downloads {
        let request = NetRequest {
            method: "GET".to_string(),
            url: download.url.clone(),
            headers: download.headers.clone(),
            body: NetBody::Empty,
            timeout: download.timeout,
            connect_timeout: download.connect_timeout,
            redirects: download.redirects,
            fail_status: download.fail_status,
            max_body_bytes: download.max_body_bytes.unwrap_or(u64::MAX),
        };
        let config = request_config(&request);
        let result = async_with_timeout(config.timeout.or(config.connect_timeout), async {
            let request = request_builder(&request)?;
            h12_download_with_redirects(&client, request, &config, &download).await
        })
        .await;
        responses.push((index, result));
    }
    responses
}

async fn async_with_timeout<T>(
    timeout: Option<Duration>,
    request: impl Future<Output = NetResult<T>>,
) -> NetResult<T> {
    match timeout {
        Some(timeout) => {
            futures_lite::future::race(request, async move {
                async_io::Timer::after(timeout).await;
                Err(NetError::new("net-timeout", "request timed out"))
            })
            .await
        }
        None => request.await,
    }
}

pub fn request(agent: &NetAgent, request: NetRequest) -> NetResult<NetResponse> {
    validate_url(&request.url)?;
    let config = request_config(&request);
    let max_body_bytes = request.max_body_bytes;
    let url = request.url.clone();
    let http_request = request_builder(&request)?;
    h12_block_on(
        agent,
        async_with_timeout(
            config.timeout.or(config.connect_timeout),
            h12_response_record(
                &agent.h1_client,
                http_request,
                &config,
                max_body_bytes,
                &url,
                true,
            ),
        ),
    )
}

pub fn request_many(
    agent: &NetAgent,
    requests: Vec<NetRequest>,
    concurrency: usize,
) -> NetResult<Vec<NetResult<NetResponse>>> {
    if concurrency == 0 {
        return Err(NetError::new(
            "net-concurrency",
            "concurrency must be at least one",
        ));
    }
    let request_count = requests.len();
    let mut partitions: Vec<Vec<(usize, NetRequest)>> = (0..concurrency.min(request_count).max(1))
        .map(|_| Vec::new())
        .collect();
    let worker_count = partitions.len();
    for (index, request) in requests.into_iter().enumerate() {
        partitions[index % worker_count].push((index, request));
    }
    let client = h12_client(
        Arc::clone(&agent.executor),
        agent.tls_config.as_ref().clone(),
        agent.max_idle_per_host,
        agent.idle_timeout,
        false,
    );
    let executor = Arc::clone(&agent.executor);
    h12_block_on(agent, async move {
        let mut tasks = Vec::with_capacity(partitions.len());
        for partition in partitions {
            let task_executor = Arc::clone(&executor);
            let client = client.clone();
            tasks.push(task_executor.spawn(h12_request_worker(client, partition)));
        }
        let mut ordered: Vec<Option<NetResult<NetResponse>>> =
            (0..request_count).map(|_| None).collect();
        for task in tasks {
            for (index, response) in task.await {
                if index >= ordered.len() {
                    ordered.resize_with(index + 1, || None);
                }
                ordered[index] = Some(response);
            }
        }
        ordered
            .into_iter()
            .map(|response| Ok(response.expect("async request result missing")))
            .collect()
    })
}

pub fn download_many(
    agent: &NetAgent,
    downloads: Vec<NetDownload>,
    concurrency: usize,
) -> NetResult<Vec<NetResult<NetResponse>>> {
    if concurrency == 0 {
        return Err(NetError::new(
            "net-concurrency",
            "concurrency must be at least one",
        ));
    }
    let download_count = downloads.len();
    let mut partitions: Vec<Vec<(usize, NetDownload)>> =
        (0..concurrency.min(download_count).max(1))
            .map(|_| Vec::new())
            .collect();
    let worker_count = partitions.len();
    for (index, download) in downloads.into_iter().enumerate() {
        partitions[index % worker_count].push((index, download));
    }
    let client = h12_client(
        Arc::clone(&agent.executor),
        agent.tls_config.as_ref().clone(),
        agent.max_idle_per_host,
        agent.idle_timeout,
        false,
    );
    let executor = Arc::clone(&agent.executor);
    h12_block_on(agent, async move {
        let mut tasks = Vec::with_capacity(partitions.len());
        for partition in partitions {
            let task_executor = Arc::clone(&executor);
            let client = client.clone();
            tasks.push(task_executor.spawn(h12_download_worker(client, partition)));
        }
        let mut ordered: Vec<Option<NetResult<NetResponse>>> =
            (0..download_count).map(|_| None).collect();
        for task in tasks {
            for (index, response) in task.await {
                ordered[index] = Some(response);
            }
        }
        ordered
            .into_iter()
            .map(|response| Ok(response.expect("async download result missing")))
            .collect()
    })
}

pub fn download(agent: &NetAgent, download: NetDownload) -> NetResult<NetResponse> {
    validate_url(&download.url)?;
    let request = NetRequest {
        method: "GET".to_string(),
        url: download.url.clone(),
        headers: download.headers.clone(),
        body: NetBody::Empty,
        timeout: download.timeout,
        connect_timeout: download.connect_timeout,
        redirects: download.redirects,
        fail_status: download.fail_status,
        max_body_bytes: download.max_body_bytes.unwrap_or(u64::MAX),
    };
    let config = request_config(&request);
    h12_block_on(
        agent,
        async_with_timeout(config.timeout.or(config.connect_timeout), async {
            let request = request_builder(&request)?;
            h12_download_with_redirects(&agent.h1_client, request, &config, &download).await
        }),
    )
}

pub fn upload(agent: &NetAgent, upload: NetUpload) -> NetResult<NetResponse> {
    validate_url(&upload.url)?;
    validate_method(&upload.method, true)?;
    let file =
        File::open(&upload.source).map_err(|error| NetError::from_io("net-source", error))?;
    let mut request = NetRequest {
        method: upload.method,
        url: upload.url.clone(),
        headers: upload.headers,
        body: NetBody::File(upload.source.clone()),
        timeout: upload.timeout,
        connect_timeout: upload.connect_timeout,
        redirects: upload.redirects,
        fail_status: upload.fail_status,
        max_body_bytes: upload.max_body_bytes,
    };
    if !request
        .headers
        .iter()
        .any(|header| header.name.eq_ignore_ascii_case("content-length"))
        && let Ok(metadata) = upload.source.metadata()
    {
        request.headers.push(NetHeader {
            name: "Content-Length".to_string(),
            value: metadata.len().to_string(),
        });
    }
    drop(file);
    let config = request_config(&request);
    let max_body_bytes = request.max_body_bytes;
    let url = request.url.clone();
    h12_block_on(
        agent,
        async_with_timeout(config.timeout.or(config.connect_timeout), async {
            let request = request_builder(&request)?;
            h12_response_record(
                &agent.h1_client,
                request,
                &config,
                max_body_bytes,
                &url,
                false,
            )
            .await
        }),
    )
}

fn request_builder(request: &NetRequest) -> NetResult<HttpRequest> {
    validate_method(&request.method, false)?;
    for header in &request.headers {
        if header.name.trim().is_empty() {
            return Err(NetError::new("net-header", "header name cannot be empty"));
        }
    }
    Ok(HttpRequest {
        method: request.method.clone(),
        url: UrlParts::parse(&request.url)?,
        headers: request.headers.clone(),
        body: request_body_bytes(&request.body)?,
    })
}

fn request_body_bytes(body: &NetBody) -> NetResult<Vec<u8>> {
    match body {
        NetBody::Empty => Ok(Vec::new()),
        NetBody::Bytes(bytes) => Ok(bytes.clone()),
        NetBody::File(path) => {
            let mut file =
                File::open(path).map_err(|error| NetError::from_io("net-body-file", error))?;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)
                .map_err(|error| NetError::from_io("net-body-file", error))?;
            Ok(bytes)
        }
    }
}

fn request_config(request: &NetRequest) -> RequestConfig {
    RequestConfig {
        timeout: request.timeout,
        connect_timeout: request.connect_timeout,
        redirects: request.redirects,
        fail_status: request.fail_status,
    }
}

fn response_record(
    response: HttpResponse,
    max_body_bytes: u64,
    url: &str,
    include_body: bool,
) -> NetResult<NetResponse> {
    let status = response.status as i64;
    let reason = response.reason;
    let headers = response.headers;
    let body = limited_body(response.body, max_body_bytes)?;
    let bytes = body.len() as i64;
    Ok(NetResponse {
        status,
        reason,
        bytes,
        headers,
        url: url.to_string(),
        body: include_body.then_some(body),
    })
}

fn limited_body(body: Vec<u8>, limit: u64) -> NetResult<Vec<u8>> {
    if body.len() as u64 > limit {
        return Err(NetError::new(
            "net-body-limit",
            "response body exceeds limit",
        ));
    }
    Ok(body)
}

fn validate_url(url: &str) -> NetResult<()> {
    UrlParts::parse(url).map(|_| ())
}

fn validate_method(method: &str, upload: bool) -> NetResult<()> {
    let allowed = if upload {
        matches!(method, "POST" | "PUT" | "PATCH" | "DELETE")
    } else {
        matches!(method, "GET" | "HEAD" | "POST" | "PUT" | "PATCH" | "DELETE")
    };
    if allowed {
        Ok(())
    } else {
        Err(NetError::new("net-method", "unsupported HTTP method"))
    }
}

fn temp_download_path(dest: &Path) -> PathBuf {
    let mut name = dest
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_else(|| "download".into());
    name.push(format!(".tmp.{}", std::process::id()));
    dest.with_file_name(name)
}

fn load_ca_certs(path: &Path) -> NetResult<Vec<CertificateDer<'static>>> {
    let pem = fs::read(path).map_err(|error| NetError::from_io("net-ca-certificate", error))?;
    let mut reader = io::Cursor::new(pem);
    let certs = rustls_pemfile::certs(&mut reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| NetError::new("net-ca-certificate", error.to_string()))?;
    if certs.is_empty() {
        return Err(NetError::new("net-ca-certificate", "no certificates found"));
    }
    Ok(certs)
}

// These are standard Linux CA locations. Keeping the list here avoids a TLS
// backend dependency while retaining distribution coverage.
#[cfg(not(target_os = "macos"))]
const SYSTEM_CERTIFICATE_FILES: &[&str] = &[
    "/etc/ssl/certs/ca-certificates.crt",
    "/etc/pki/ca-trust/extracted/pem/tls-ca-bundle.pem",
    "/etc/pki/tls/certs/ca-bundle.crt",
    "/etc/ssl/ca-bundle.pem",
    "/etc/pki/tls/cacert.pem",
    "/etc/ssl/cert.pem",
    "/opt/etc/ssl/certs/ca-certificates.crt",
    "/etc/ssl/certs/cacert.pem",
];

#[cfg(not(target_os = "macos"))]
const SYSTEM_CERTIFICATE_DIRECTORIES: &[&str] = &[
    "/etc/ssl/certs",
    "/etc/pki/tls/certs",
    "/etc/security/certificates",
];

#[cfg(not(target_os = "macos"))]
fn system_root_certificates() -> NetResult<RootCertStore> {
    let certificate_file = std::env::var_os("SSL_CERT_FILE").map(PathBuf::from);
    let certificate_directories = std::env::var_os("SSL_CERT_DIR")
        .map(|directories| std::env::split_paths(&directories).collect::<Vec<_>>())
        .unwrap_or_default();
    let (certificate_file, certificate_directories) =
        if certificate_file.is_some() || !certificate_directories.is_empty() {
            (certificate_file, certificate_directories)
        } else {
            (
                SYSTEM_CERTIFICATE_FILES
                    .iter()
                    .map(PathBuf::from)
                    .find(|path| path.is_file()),
                SYSTEM_CERTIFICATE_DIRECTORIES
                    .iter()
                    .map(PathBuf::from)
                    .filter(|path| path.is_dir())
                    .collect(),
            )
        };

    let mut certificates = Vec::new();
    if let Some(path) = certificate_file {
        collect_system_certificates(&path, &mut certificates);
    }
    for directory in certificate_directories {
        collect_system_certificates_from_dir(&directory, &mut certificates);
    }
    certificates.sort_unstable_by(|left, right| left.as_ref().cmp(right.as_ref()));
    certificates.dedup_by(|left, right| left.as_ref() == right.as_ref());

    let mut roots = RootCertStore::empty();
    roots.add_parsable_certificates(certificates);
    if roots.is_empty() {
        return Err(NetError::new(
            "net-tls",
            "no CA certificates were loaded from the system",
        ));
    }
    Ok(roots)
}

#[cfg(not(target_os = "macos"))]
fn collect_system_certificates(path: &Path, certificates: &mut Vec<CertificateDer<'static>>) {
    let Ok(pem) = fs::read(path) else {
        return;
    };
    let mut reader = io::Cursor::new(pem);
    certificates.extend(rustls_pemfile::certs(&mut reader).filter_map(Result::ok));
}

#[cfg(not(target_os = "macos"))]
fn collect_system_certificates_from_dir(
    directory: &Path,
    certificates: &mut Vec<CertificateDer<'static>>,
) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            collect_system_certificates(&path, certificates);
        }
    }
}

fn net_transport_error(error: io::Error) -> NetError {
    let message = error.to_string();
    let kind = if matches!(
        error.kind(),
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
    ) {
        "net-timeout"
    } else if message.contains("dns-not-found") {
        "dns-not-found"
    } else if message.contains("dns-timeout") {
        "dns-timeout"
    } else if message.contains("dns-") {
        "dns-lookup"
    } else if message.contains("certificate")
        || message.contains("invalid peer certificate")
        || message.contains("tls")
    {
        "net-tls"
    } else if message.contains("invalid URL") || message.contains("invalid uri") {
        "net-url"
    } else {
        "net-io"
    };
    NetError::new(kind, message)
}

fn tls_config_for_key(key: &NetAgentKey) -> NetResult<ClientConfig> {
    let provider = Arc::new(default_provider());
    let builder = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|error| NetError::new("net-tls", error.to_string()))?;
    let config = if !key.tls_verify {
        builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(DisabledVerifier))
            .with_no_client_auth()
    } else if let Some(path) = &key.ca_certificate {
        let mut roots = RootCertStore::empty();
        for cert in load_ca_certs(path)? {
            roots
                .add(cert)
                .map_err(|error| NetError::new("net-ca-certificate", error.to_string()))?;
        }
        builder.with_root_certificates(roots).with_no_client_auth()
    } else {
        default_tls_client_config(builder)?
    };
    Ok(config)
}

fn default_tls_client_config(
    builder: rustls::ConfigBuilder<ClientConfig, rustls::WantsVerifier>,
) -> NetResult<ClientConfig> {
    #[cfg(target_os = "macos")]
    {
        return builder
            .with_platform_verifier()
            .map_err(|error| NetError::new("net-tls", error.to_string()))
            .map(|builder| builder.with_no_client_auth());
    }

    #[cfg(not(target_os = "macos"))]
    Ok(builder
        .with_root_certificates(system_root_certificates()?)
        .with_no_client_auth())
}

#[derive(Clone, Debug)]
struct RequestConfig {
    timeout: Option<Duration>,
    connect_timeout: Option<Duration>,
    redirects: usize,
    fail_status: bool,
}

#[derive(Clone, Debug)]
struct HttpRequest {
    method: String,
    url: UrlParts,
    headers: Vec<NetHeader>,
    body: Vec<u8>,
}

#[derive(Clone, Debug)]
struct HttpResponse {
    status: u16,
    reason: String,
    headers: Vec<NetHeader>,
    body: Vec<u8>,
}

#[derive(Clone, Debug)]
struct UrlParts {
    scheme: String,
    host: String,
    port: u16,
    path: String,
}

impl UrlParts {
    fn parse(url: &str) -> NetResult<Self> {
        let (scheme, rest) = url
            .split_once("://")
            .ok_or_else(|| NetError::new("net-url", "URL must include a scheme"))?;
        if !matches!(scheme, "http" | "https") {
            return Err(NetError::new(
                "net-scheme",
                "URL scheme must be http or https",
            ));
        }
        let (authority, path) = match rest.find(['/', '?', '#']) {
            Some(index) => (&rest[..index], &rest[index..]),
            None => (rest, "/"),
        };
        if authority.is_empty() {
            return Err(NetError::new("net-url", "URL must include a host"));
        }
        let default_port = if scheme == "https" { 443 } else { 80 };
        let (host, port) = parse_authority(authority, default_port)?;
        Ok(Self {
            scheme: scheme.to_string(),
            host,
            port,
            path: path.to_string(),
        })
    }

    fn authority(&self) -> String {
        let default_port = if self.scheme == "https" { 443 } else { 80 };
        let host = if self.host.contains(':') && !self.host.starts_with('[') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        if self.port == default_port {
            host
        } else {
            format!("{host}:{}", self.port)
        }
    }

    fn absolute(&self) -> String {
        format!("{}://{}{}", self.scheme, self.authority(), self.path)
    }
}

fn parse_authority(authority: &str, default_port: u16) -> NetResult<(String, u16)> {
    if let Some(rest) = authority.strip_prefix('[') {
        let (host, rest) = rest
            .split_once(']')
            .ok_or_else(|| NetError::new("net-url", "invalid IPv6 host"))?;
        let port = if let Some(port) = rest.strip_prefix(':') {
            port.parse::<u16>()
                .map_err(|error| NetError::new("net-url", error.to_string()))?
        } else if rest.is_empty() {
            default_port
        } else {
            return Err(NetError::new("net-url", "invalid URL authority"));
        };
        return Ok((host.to_string(), port));
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') => {
            let port = port
                .parse::<u16>()
                .map_err(|error| NetError::new("net-url", error.to_string()))?;
            (host, port)
        }
        _ => (authority, default_port),
    };
    if host.is_empty() {
        return Err(NetError::new("net-url", "URL must include a host"));
    }
    Ok((host.to_string(), port))
}

fn is_redirect(status: u16) -> bool {
    matches!(status, 301 | 302 | 303 | 307 | 308)
}

fn redirect_url(base: &UrlParts, location: &str) -> NetResult<UrlParts> {
    if location.contains("://") {
        return UrlParts::parse(location);
    }
    let path = if location.starts_with('/') {
        location.to_string()
    } else {
        let parent = base
            .path
            .rsplit_once('/')
            .map(|(parent, _)| parent)
            .unwrap_or("");
        format!("{parent}/{location}")
    };
    UrlParts::parse(&format!("{}://{}{}", base.scheme, base.authority(), path))
}

#[derive(Debug)]
struct DisabledVerifier;

impl ServerCertVerifier for DisabledVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA1,
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ECDSA_NISTP521_SHA512,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::ED25519,
            SignatureScheme::ED448,
        ]
    }
}
