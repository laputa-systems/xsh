//! Socket addresses: the typed record form and the raw `sockaddr` bytes.
//!
//! An address is held as the exact bytes the kernel reads or wrote, so any
//! family round-trips unchanged and the typed fields are decoded on demand.

use super::{Args, record};
use crate::runtime::value::{RecordMap, RuntimeError, Value};
use rustix::net::SocketAddrAny;
use rustix::net::addr::{SocketAddrArg, SocketAddrLen, SocketAddrOpaque};
use std::net::{Ipv4Addr, Ipv6Addr};

const AF_UNSPEC: u16 = 0;
const AF_UNIX: u16 = 1;
const AF_INET: u16 = 2;
const AF_INET6: u16 = 10;
const AF_NETLINK: u16 = 16;
const AF_PACKET: u16 = 17;
/// `sizeof(struct sockaddr_storage)`: the largest address any family needs.
const STORAGE_LEN: usize = 128;
/// `sizeof(sun_path)`.
const UNIX_PATH_LEN: usize = 108;

pub(super) struct SockAddr {
    bytes: Vec<u8>,
}

// SAFETY: `with_sockaddr` passes a pointer to bytes this value owns together
// with their exact length, which is what the kernel reads for any family.
unsafe impl SocketAddrArg for SockAddr {
    unsafe fn with_sockaddr<R>(
        &self,
        f: impl FnOnce(*const SocketAddrOpaque, SocketAddrLen) -> R,
    ) -> R {
        f(self.bytes.as_ptr().cast(), self.bytes.len() as SocketAddrLen)
    }
}

fn u16_at(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_ne_bytes(bytes.get(offset..offset + 2)?.try_into().ok()?))
}

fn u32_at(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_ne_bytes(bytes.get(offset..offset + 4)?.try_into().ok()?))
}

impl SockAddr {
    /// The netlink address of a port id and multicast group mask.
    pub(super) fn netlink(pid: u32, groups: u32) -> Self {
        let mut bytes = Vec::with_capacity(12);
        bytes.extend_from_slice(&AF_NETLINK.to_ne_bytes());
        bytes.extend_from_slice(&[0, 0]);
        bytes.extend_from_slice(&pid.to_ne_bytes());
        bytes.extend_from_slice(&groups.to_ne_bytes());
        Self { bytes }
    }

    /// The address a kernel call filled in.
    pub(super) fn from_kernel(address: &SocketAddrAny) -> Self {
        // SAFETY: `as_ptr` and `addr_len` describe the initialised prefix of
        // the storage the kernel wrote.
        let bytes = unsafe {
            std::slice::from_raw_parts(address.as_ptr().cast::<u8>(), address.addr_len() as usize)
        };
        Self { bytes: bytes.to_vec() }
    }

    /// No address at all, as a connected stream socket reports its sender.
    pub(super) fn unspecified() -> Self {
        Self { bytes: Vec::new() }
    }

    /// Reads a script address: a record `{family, address, port, scope_id}`
    /// for inet, inet6, unix, and netlink, or raw `sockaddr` bytes.
    pub(super) fn from_value(args: &Args<'_>, index: usize) -> Result<Self, RuntimeError> {
        match args.get(index) {
            Some(Value::Record(fields)) => Self::from_record(args, fields),
            Some(Value::Bytes(bytes)) => {
                if bytes.len() < 2 || bytes.len() > STORAGE_LEN {
                    return Err(args.invalid(format!(
                        "a raw socket address must be 2 to {STORAGE_LEN} bytes, not {}",
                        bytes.len()
                    )));
                }
                Ok(Self { bytes: bytes.clone() })
            }
            Some(other) => Err(args.type_error("a socket address record or Bytes", other)),
            None => Err(args.missing()),
        }
    }

    fn from_record(args: &Args<'_>, fields: &RecordMap) -> Result<Self, RuntimeError> {
        let text = |name: &str| match fields.get(name) {
            Some(Value::Str(value)) => Ok(Some(value.to_string())),
            None | Some(Value::Null) => Ok(None),
            Some(other) => Err(args.type_error(&format!("Str for {name}"), other)),
        };
        let number = |name: &str, max: i64| match fields.get(name) {
            Some(Value::Int(value)) if (0..=max).contains(value) => Ok(*value),
            Some(Value::Int(value)) => {
                Err(args.invalid(format!("address field {name} {value} is out of range")))
            }
            None | Some(Value::Null) => Ok(0),
            Some(other) => Err(args.type_error(&format!("Int for {name}"), other)),
        };
        let family = text("family")?.ok_or_else(|| args.invalid("address needs a family field"))?;
        let host = text("address")?.unwrap_or_default();
        let mut bytes = Vec::new();
        match family.as_str() {
            "inet" => {
                let ip: Ipv4Addr = host
                    .parse()
                    .map_err(|_| args.invalid(format!("{host:?} is not an IPv4 address")))?;
                let port = number("port", i64::from(u16::MAX))? as u16;
                bytes.extend_from_slice(&AF_INET.to_ne_bytes());
                bytes.extend_from_slice(&port.to_be_bytes());
                bytes.extend_from_slice(&ip.octets());
                bytes.extend_from_slice(&[0; 8]);
            }
            "inet6" => {
                let ip: Ipv6Addr = host
                    .parse()
                    .map_err(|_| args.invalid(format!("{host:?} is not an IPv6 address")))?;
                let port = number("port", i64::from(u16::MAX))? as u16;
                let scope = number("scope_id", i64::from(u32::MAX))? as u32;
                bytes.extend_from_slice(&AF_INET6.to_ne_bytes());
                bytes.extend_from_slice(&port.to_be_bytes());
                bytes.extend_from_slice(&[0; 4]);
                bytes.extend_from_slice(&ip.octets());
                bytes.extend_from_slice(&scope.to_ne_bytes());
            }
            "netlink" => {
                let pid = number("port", i64::from(u32::MAX))? as u32;
                let groups = number("scope_id", i64::from(u32::MAX))? as u32;
                return Ok(Self::netlink(pid, groups));
            }
            "unix" => {
                // A path needs room for its NUL terminator and an abstract
                // name for its leading NUL, so both leave one byte.
                let (name, abstract_socket) = match host.strip_prefix('@') {
                    Some(name) => (name, true),
                    None => (host.as_str(), false),
                };
                if name.len() >= UNIX_PATH_LEN || name.as_bytes().contains(&0) {
                    return Err(args.invalid("unix socket name is too long or contains NUL"));
                }
                bytes.extend_from_slice(&AF_UNIX.to_ne_bytes());
                if abstract_socket {
                    bytes.push(0);
                    bytes.extend_from_slice(name.as_bytes());
                } else {
                    bytes.extend_from_slice(name.as_bytes());
                    bytes.push(0);
                }
            }
            other => {
                return Err(args.invalid(format!(
                    "unsupported address family {other:?}; use inet, inet6, unix, netlink, or raw sockaddr bytes"
                )));
            }
        }
        Ok(Self { bytes })
    }

    /// The script-visible record: decoded fields plus the raw bytes.
    pub(super) fn to_value(&self) -> Value {
        let bytes = &self.bytes;
        let family = u16_at(bytes, 0).unwrap_or(AF_UNSPEC);
        let (name, address, port, scope_id) = match family {
            AF_INET if bytes.len() >= 8 => {
                let octets: [u8; 4] = bytes[4..8].try_into().expect("slice of four");
                let port = u16::from_be_bytes([bytes[2], bytes[3]]);
                ("inet".to_string(), Ipv4Addr::from(octets).to_string(), i64::from(port), 0)
            }
            AF_INET6 if bytes.len() >= 24 => {
                let octets: [u8; 16] = bytes[8..24].try_into().expect("slice of sixteen");
                let port = u16::from_be_bytes([bytes[2], bytes[3]]);
                let scope = u32_at(bytes, 24).unwrap_or(0);
                (
                    "inet6".to_string(),
                    Ipv6Addr::from(octets).to_string(),
                    i64::from(port),
                    i64::from(scope),
                )
            }
            AF_NETLINK if bytes.len() >= 12 => (
                "netlink".to_string(),
                String::new(),
                i64::from(u32_at(bytes, 4).unwrap_or(0)),
                i64::from(u32_at(bytes, 8).unwrap_or(0)),
            ),
            AF_UNIX => {
                let path = bytes.get(2..).unwrap_or_default();
                let text = match path.first() {
                    None => String::new(),
                    Some(0) => format!("@{}", String::from_utf8_lossy(&path[1..])),
                    Some(_) => {
                        let end = path.iter().position(|byte| *byte == 0).unwrap_or(path.len());
                        String::from_utf8_lossy(&path[..end]).into_owned()
                    }
                };
                ("unix".to_string(), text, 0, 0)
            }
            AF_PACKET => ("packet".to_string(), String::new(), 0, 0),
            AF_UNSPEC => ("unspec".to_string(), String::new(), 0, 0),
            other => (other.to_string(), String::new(), 0, 0),
        };
        record([
            ("family", Value::Str(name.into())),
            ("address", Value::Str(address.into())),
            ("port", Value::Int(port)),
            ("scope_id", Value::Int(scope_id)),
            ("raw", Value::Bytes(self.bytes.clone())),
        ])
    }
}
