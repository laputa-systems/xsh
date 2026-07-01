#![allow(clippy::single_call_fn)]

use bytes::Bytes;
use cap_net_ext::{Blocking, PoolExt, TcpListenerExt};
use crossbeam_channel::RecvTimeoutError;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::{Request, Response, Uri};
use hyper_rustls::ConfigBuilderExt;
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::{TokioExecutor, TokioIo};
use rustc_hash::FxHashSet;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme};
use std::ffi::{CStr, CString};
use std::fs::{self, File, OpenOptions};
use std::future::Future;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{self, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs, UdpSocket};
#[cfg(not(windows))]
use std::os::fd::{FromRawFd, IntoRawFd};
#[cfg(windows)]
use std::os::windows::io::{FromRawSocket, IntoRawSocket};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tower_service::Service;

type HyperClient = Client<HttpsConnector<CapNetConnector>, Full<Bytes>>;

type ConnectorFuture = Pin<
    Box<dyn Future<Output = Result<TokioIo<tokio::net::TcpStream>, io::Error>> + Send + 'static>,
>;

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
    client: HyperClient,
    runtime: Arc<tokio::runtime::Runtime>,
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
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_io()
        .enable_time()
        .build()
        .map_err(|error| NetError::new("net-runtime", error.to_string()))?;
    let tls_config = tls_config_for_key(key)?;
    let https = hyper_rustls::HttpsConnectorBuilder::new()
        .with_tls_config(tls_config)
        .https_or_http()
        .enable_http1()
        .wrap_connector(CapNetConnector);
    let client = Client::builder(TokioExecutor::new())
        .pool_idle_timeout(key.idle_timeout)
        .pool_max_idle_per_host(key.max_idle_per_host)
        .build(https);
    Ok(NetAgent {
        client,
        runtime: Arc::new(runtime),
    })
}

pub fn request(agent: &NetAgent, request: NetRequest) -> NetResult<NetResponse> {
    validate_url(&request.url)?;
    let config = request_config(&request);
    let max_body_bytes = request.max_body_bytes;
    let url = request.url.clone();
    let http_request = request_builder(&request)?;
    let response = run_request(agent, http_request, &config)?;
    response_record(agent, response, max_body_bytes, &url, true)
}

pub fn download(agent: &NetAgent, download: NetDownload) -> NetResult<NetResponse> {
    validate_url(&download.url)?;
    if !download.overwrite && download.dest.exists() {
        return Err(NetError::new("net-dest", "destination exists"));
    }
    if let Some(parent) = download.dest.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|error| NetError::from_io("net-dest", error))?;
    }
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
    let http_request = request_builder(&request)?;
    let response = run_request(agent, http_request, &request_config(&request))?;
    let status = response.status().as_u16() as i64;
    let reason = response
        .status()
        .canonical_reason()
        .unwrap_or("")
        .to_string();
    let headers = response_headers(response.headers());
    let output = if download.atomic {
        temp_download_path(&download.dest)
    } else {
        download.dest.clone()
    };
    let result = write_response_body(
        agent,
        response,
        &output,
        request.max_body_bytes,
        download.overwrite,
    );
    if let Err(error) = result {
        let _ = fs::remove_file(&output);
        return Err(error);
    }
    let bytes = result.unwrap();
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
    Ok(NetResponse {
        status,
        reason,
        bytes,
        headers,
        url: download.url,
        body: None,
    })
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
    let http_request = request_builder(&request)?;
    let response = run_request(agent, http_request, &request_config(&request))?;
    response_record(agent, response, request.max_body_bytes, &request.url, false)
}

fn request_builder(request: &NetRequest) -> NetResult<Request<Full<Bytes>>> {
    validate_method(&request.method, false)?;
    let mut builder = Request::builder()
        .method(request.method.as_str())
        .uri(request.url.as_str());
    for header in &request.headers {
        if header.name.trim().is_empty() {
            return Err(NetError::new("net-header", "header name cannot be empty"));
        }
        builder = builder.header(header.name.as_str(), header.value.as_str());
    }
    builder
        .body(Full::new(request_body_bytes(&request.body)?))
        .map_err(|error| NetError::new("net-request", error.to_string()))
}

fn request_body_bytes(body: &NetBody) -> NetResult<Bytes> {
    match body {
        NetBody::Empty => Ok(Bytes::new()),
        NetBody::Bytes(bytes) => Ok(Bytes::from(bytes.clone())),
        NetBody::File(path) => {
            let mut file =
                File::open(path).map_err(|error| NetError::from_io("net-body-file", error))?;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)
                .map_err(|error| NetError::from_io("net-body-file", error))?;
            Ok(Bytes::from(bytes))
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

fn run_request(
    agent: &NetAgent,
    request: Request<Full<Bytes>>,
    config: &RequestConfig,
) -> NetResult<Response<Incoming>> {
    agent
        .runtime
        .block_on(run_request_async(agent.client.clone(), request, config))
}

async fn run_request_async(
    client: HyperClient,
    request: Request<Full<Bytes>>,
    config: &RequestConfig,
) -> NetResult<Response<Incoming>> {
    let response = request_with_redirects(client, request, config).await?;
    if config.fail_status && !response.status().is_success() {
        return Err(NetError::new(
            "net-status",
            format!("HTTP status {}", response.status().as_u16()),
        ));
    }
    Ok(response)
}

async fn request_with_redirects(
    client: HyperClient,
    mut request: Request<Full<Bytes>>,
    config: &RequestConfig,
) -> NetResult<Response<Incoming>> {
    for _ in 0..=config.redirects {
        let response = send_once(client.clone(), request.clone(), config).await?;
        if !is_redirect(response.status()) {
            return Ok(response);
        }
        let Some(location) = response.headers().get(hyper::header::LOCATION).cloned() else {
            return Err(NetError::new(
                "net-redirect",
                "redirect missing Location header",
            ));
        };
        let location = location
            .to_str()
            .map_err(|error| NetError::new("net-redirect", error.to_string()))?;
        let uri = redirect_uri(request.uri(), location)?;
        let (mut parts, body) = request.into_parts();
        parts.uri = uri;
        request = Request::from_parts(parts, body);
    }
    Err(NetError::new("net-redirect", "too many redirects"))
}

async fn send_once(
    client: HyperClient,
    request: Request<Full<Bytes>>,
    config: &RequestConfig,
) -> NetResult<Response<Incoming>> {
    let future = client.request(request);
    if let Some(timeout) = config.timeout.or(config.connect_timeout) {
        tokio::time::timeout(timeout, future)
            .await
            .map_err(|_| NetError::new("net-timeout", "request timed out"))?
            .map_err(hyper_error)
    } else {
        future.await.map_err(hyper_error)
    }
}

fn response_record(
    agent: &NetAgent,
    response: Response<Incoming>,
    max_body_bytes: u64,
    url: &str,
    include_body: bool,
) -> NetResult<NetResponse> {
    let status = response.status().as_u16() as i64;
    let reason = response
        .status()
        .canonical_reason()
        .unwrap_or("")
        .to_string();
    let headers = response_headers(response.headers());
    let body = collect_body(agent, response.into_body(), max_body_bytes)?;
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

fn write_response_body(
    agent: &NetAgent,
    response: Response<Incoming>,
    path: &Path,
    limit: u64,
    overwrite: bool,
) -> NetResult<i64> {
    let body = response.into_body();
    let bytes = collect_body(agent, body, limit)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .create_new(!overwrite)
        .truncate(overwrite)
        .open(path)
        .map_err(|error| NetError::from_io("net-write", error))?;
    file.write_all(&bytes)
        .map_err(|error| NetError::from_io("net-write", error))?;
    Ok(bytes.len() as i64)
}

fn collect_body(agent: &NetAgent, body: Incoming, limit: u64) -> NetResult<Vec<u8>> {
    agent.runtime.block_on(async move {
        let collected = body.collect().await.map_err(hyper_error)?;
        let bytes = collected.to_bytes();
        if bytes.len() as u64 > limit {
            return Err(NetError::new(
                "net-body-limit",
                "response body exceeds limit",
            ));
        }
        Ok(bytes.to_vec())
    })
}

fn response_headers(headers: &hyper::HeaderMap) -> Vec<NetHeader> {
    headers
        .iter()
        .map(|(name, value)| NetHeader {
            name: name.as_str().to_string(),
            value: String::from_utf8_lossy(value.as_bytes()).into_owned(),
        })
        .collect()
}

fn validate_url(url: &str) -> NetResult<()> {
    let uri = url
        .parse::<Uri>()
        .map_err(|error| NetError::new("net-url", error.to_string()))?;
    match uri.scheme_str() {
        Some("http" | "https") => Ok(()),
        Some(_) => Err(NetError::new(
            "net-scheme",
            "URL scheme must be http or https",
        )),
        None => Err(NetError::new("net-url", "URL must include a scheme")),
    }
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

fn hyper_error(error: impl std::error::Error) -> NetError {
    let message = error.to_string();
    let kind = if message.contains("dns-not-found") {
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
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
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
        builder
            .try_with_platform_verifier()
            .map_err(|error| NetError::new("net-tls", error.to_string()))?
            .with_no_client_auth()
    };
    Ok(config)
}

#[derive(Clone, Debug)]
struct RequestConfig {
    timeout: Option<Duration>,
    connect_timeout: Option<Duration>,
    redirects: usize,
    fail_status: bool,
}

#[derive(Clone, Copy, Debug)]
struct CapNetConnector;

impl Service<Uri> for CapNetConnector {
    type Response = TokioIo<tokio::net::TcpStream>;
    type Error = io::Error;
    type Future = ConnectorFuture;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, uri: Uri) -> Self::Future {
        Box::pin(async move {
            let addrs = resolve_uri_socket_addrs(&uri)?;
            let stream = tokio::task::spawn_blocking(move || connect_cap_tcp_stream(addrs))
                .await
                .map_err(|error| io::Error::other(format!("net-connect: {error}")))??;
            Ok(TokioIo::new(tokio::net::TcpStream::from_std(stream)?))
        })
    }
}

fn resolve_uri_socket_addrs(uri: &Uri) -> io::Result<Vec<SocketAddr>> {
    let host = uri
        .host()
        .ok_or_else(|| io::Error::other("net-url: URL must include a host"))?;
    let port = uri
        .port_u16()
        .or_else(|| match uri.scheme_str() {
            Some("http") => Some(80),
            Some("https") => Some(443),
            _ => None,
        })
        .ok_or_else(|| io::Error::other("net-url: URL must include a port"))?;
    let addrs =
        resolve_socket_addrs(host, port, AddressFamily::Any, None).map_err(net_error_to_io)?;
    let addrs = addrs.into_iter().take(16).collect::<Vec<_>>();
    if addrs.is_empty() {
        Err(io::Error::other("dns-not-found: no records found"))
    } else {
        Ok(addrs)
    }
}

fn connect_cap_tcp_stream(addrs: Vec<SocketAddr>) -> io::Result<std::net::TcpStream> {
    let mut last_err = None;
    for addr in addrs {
        match connect_cap_tcp_addr(addr) {
            Ok(stream) => return Ok(stream),
            Err(error) => last_err = Some(error),
        }
    }
    Err(last_err.unwrap_or_else(|| io::Error::other("dns-not-found: no records found")))
}

fn connect_cap_tcp_addr(addr: SocketAddr) -> io::Result<std::net::TcpStream> {
    let family = match addr {
        SocketAddr::V4(_) => cap_net_ext::AddressFamily::Ipv4,
        SocketAddr::V6(_) => cap_net_ext::AddressFamily::Ipv6,
    };
    let socket = cap_std::net::TcpListener::new(family, Blocking::Yes)?;
    let mut pool = cap_std::net::Pool::new();
    pool.insert_socket_addr(addr, cap_std::ambient_authority());
    let stream = pool.connect_into_tcp_stream(socket, addr)?;
    stream.set_nonblocking(true)?;
    Ok(cap_tcp_stream_into_std(stream))
}

#[cfg(not(windows))]
fn cap_tcp_stream_into_std(stream: cap_std::net::TcpStream) -> std::net::TcpStream {
    let fd = stream.into_raw_fd();
    // `into_raw_fd` transfers ownership of the socket to the returned std stream.
    unsafe { std::net::TcpStream::from_raw_fd(fd) }
}

#[cfg(windows)]
fn cap_tcp_stream_into_std(stream: cap_std::net::TcpStream) -> std::net::TcpStream {
    let socket = stream.into_raw_socket();
    // `into_raw_socket` transfers ownership of the socket to the returned std stream.
    unsafe { std::net::TcpStream::from_raw_socket(socket) }
}

fn net_error_to_io(error: NetError) -> io::Error {
    io::Error::other(format!("{}: {}", error.kind, error.message))
}

fn is_redirect(status: hyper::StatusCode) -> bool {
    matches!(status.as_u16(), 301 | 302 | 303 | 307 | 308)
}

fn redirect_uri(base: &Uri, location: &str) -> NetResult<Uri> {
    if let Ok(uri) = location.parse::<Uri>()
        && uri.scheme().is_some()
    {
        validate_url(location)?;
        return Ok(uri);
    }
    let scheme = base
        .scheme_str()
        .ok_or_else(|| NetError::new("net-redirect", "base URL missing scheme"))?;
    let authority = base
        .authority()
        .ok_or_else(|| NetError::new("net-redirect", "base URL missing authority"))?;
    let path = if location.starts_with('/') {
        location.to_string()
    } else {
        let base_path = base.path();
        let parent = base_path
            .rsplit_once('/')
            .map(|(parent, _)| parent)
            .unwrap_or("");
        format!("{parent}/{location}")
    };
    format!("{scheme}://{authority}{path}")
        .parse::<Uri>()
        .map_err(|error| NetError::new("net-redirect", error.to_string()))
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
