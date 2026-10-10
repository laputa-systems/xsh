//! The guarded ioctl: only fixed-layout interface and ARP request structures
//! from a closed catalogue, each copied into a buffer of exactly the size the
//! kernel reads and writes for that request.

use super::{Args, ok};
use crate::runtime::value::{RuntimeError, Value};
use xsh_registry::records::NET_CONSTANTS;

const IFNAMSIZ: usize = 16;
const IFREQ_SIZE: usize = if cfg!(target_pointer_width = "64") { 40 } else { 32 };
const ARPREQ_SIZE: usize = 68;
const IWREQ_SIZE: usize = 32;
const INT_SIZE: usize = 4;
/// Ethtool command structures are at most a few pages (string tables, stats).
const MAX_PAYLOAD: usize = 64 * 1024;

/// How a request's argument is laid out.
#[derive(Clone, Copy)]
enum Shape {
    /// A `struct ifreq` of the given total size.
    Fixed(usize),
    /// A `struct ifreq` whose data pointer addresses a separate buffer; the
    /// input is the interface name followed by that buffer.
    NamedPayload,
}

/// Request names by argument shape. Numbers come from the shared constant
/// table so the names a script reads and the requests the guard admits agree.
const CATALOGUE: &[(&str, Shape)] = &[
    ("SIOCGIFNAME", Shape::Fixed(IFREQ_SIZE)),
    ("SIOCGIFFLAGS", Shape::Fixed(IFREQ_SIZE)),
    ("SIOCSIFFLAGS", Shape::Fixed(IFREQ_SIZE)),
    ("SIOCGIFADDR", Shape::Fixed(IFREQ_SIZE)),
    ("SIOCSIFADDR", Shape::Fixed(IFREQ_SIZE)),
    ("SIOCGIFDSTADDR", Shape::Fixed(IFREQ_SIZE)),
    ("SIOCSIFDSTADDR", Shape::Fixed(IFREQ_SIZE)),
    ("SIOCGIFBRDADDR", Shape::Fixed(IFREQ_SIZE)),
    ("SIOCSIFBRDADDR", Shape::Fixed(IFREQ_SIZE)),
    ("SIOCGIFNETMASK", Shape::Fixed(IFREQ_SIZE)),
    ("SIOCSIFNETMASK", Shape::Fixed(IFREQ_SIZE)),
    ("SIOCGIFMETRIC", Shape::Fixed(IFREQ_SIZE)),
    ("SIOCSIFMETRIC", Shape::Fixed(IFREQ_SIZE)),
    ("SIOCGIFMTU", Shape::Fixed(IFREQ_SIZE)),
    ("SIOCSIFMTU", Shape::Fixed(IFREQ_SIZE)),
    ("SIOCSIFNAME", Shape::Fixed(IFREQ_SIZE)),
    ("SIOCGIFHWADDR", Shape::Fixed(IFREQ_SIZE)),
    ("SIOCSIFHWADDR", Shape::Fixed(IFREQ_SIZE)),
    ("SIOCGIFTXQLEN", Shape::Fixed(IFREQ_SIZE)),
    ("SIOCSIFTXQLEN", Shape::Fixed(IFREQ_SIZE)),
    ("SIOCGIFINDEX", Shape::Fixed(IFREQ_SIZE)),
    ("SIOCGIFMAP", Shape::Fixed(IFREQ_SIZE)),
    ("SIOCGMIIPHY", Shape::Fixed(IFREQ_SIZE)),
    ("SIOCGMIIREG", Shape::Fixed(IFREQ_SIZE)),
    ("SIOCGARP", Shape::Fixed(ARPREQ_SIZE)),
    ("SIOCSARP", Shape::Fixed(ARPREQ_SIZE)),
    ("SIOCDARP", Shape::Fixed(ARPREQ_SIZE)),
    ("SIOCGIWNAME", Shape::Fixed(IWREQ_SIZE)),
    ("SIOCINQ", Shape::Fixed(INT_SIZE)),
    ("SIOCOUTQ", Shape::Fixed(INT_SIZE)),
    ("SIOCETHTOOL", Shape::NamedPayload),
];

fn shape_of(request: i64) -> Option<Shape> {
    CATALOGUE.iter().find_map(|(name, shape)| {
        NET_CONSTANTS
            .iter()
            .any(|(constant, value)| constant == name && *value == request)
            .then_some(*shape)
    })
}

pub(super) fn ioctl(args: &Args<'_>) -> Result<Value, RuntimeError> {
    let fd = args.fd(0)?;
    let request = args.int(1)?;
    let input = args.bytes(2)?;
    let out_len = usize::try_from(args.int(3)?).map_err(|_| args.invalid("out_len must be non-negative"))?;
    let shape = shape_of(request).ok_or_else(|| {
        args.invalid(format!("ioctl request {request:#x} is not in the guarded catalogue"))
    })?;
    match shape {
        Shape::Fixed(size) => {
            if input.len() > size || out_len > size {
                return Err(args.invalid(format!(
                    "this request's structure is {size} bytes; input and out_len must not exceed it"
                )));
            }
            // u64 elements keep the structure aligned for its widest member.
            let mut words = vec![0_u64; size.div_ceil(8)];
            let buffer = bytes_of(&mut words);
            buffer[..input.len()].copy_from_slice(input);
            // SAFETY: the buffer is at least the size the request reads and writes.
            if unsafe { libc::ioctl(fd, request as _, buffer.as_mut_ptr()) } < 0 {
                return Err(args.errno(std::io::Error::last_os_error()));
            }
            ok(Value::Bytes(buffer[..out_len].to_vec()))
        }
        Shape::NamedPayload => {
            if input.len() <= IFNAMSIZ || input.len() - IFNAMSIZ > MAX_PAYLOAD {
                return Err(args.invalid(format!(
                    "input is a {IFNAMSIZ}-byte interface name followed by 1 to {MAX_PAYLOAD} payload bytes"
                )));
            }
            let payload_len = input.len() - IFNAMSIZ;
            if out_len > payload_len {
                return Err(args.invalid("out_len must not exceed the payload"));
            }
            let mut payload_words = vec![0_u64; payload_len.div_ceil(8)];
            let payload = bytes_of(&mut payload_words);
            payload[..payload_len].copy_from_slice(&input[IFNAMSIZ..]);
            let mut request_words = vec![0_u64; IFREQ_SIZE.div_ceil(8)];
            let ifreq = bytes_of(&mut request_words);
            ifreq[..IFNAMSIZ].copy_from_slice(&input[..IFNAMSIZ]);
            let pointer = (payload.as_mut_ptr() as usize).to_ne_bytes();
            ifreq[IFNAMSIZ..IFNAMSIZ + pointer.len()].copy_from_slice(&pointer);
            // SAFETY: the data pointer addresses `payload`, which outlives the
            // call and is as long as the caller said the structure is.
            if unsafe { libc::ioctl(fd, request as _, ifreq.as_mut_ptr()) } < 0 {
                return Err(args.errno(std::io::Error::last_os_error()));
            }
            ok(Value::Bytes(payload[..out_len].to_vec()))
        }
    }
}

fn bytes_of(words: &mut [u64]) -> &mut [u8] {
    // SAFETY: u64 storage reinterpreted as the same number of bytes.
    unsafe { std::slice::from_raw_parts_mut(words.as_mut_ptr().cast(), std::mem::size_of_val(words)) }
}
