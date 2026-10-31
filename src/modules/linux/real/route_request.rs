use super::netlink::{
    DumpError, NLM_F_ACK, NLM_F_ACK_TLVS, NLM_F_APPEND, NLM_F_CREATE, NLM_F_DUMP, NLM_F_EXCL,
    NLM_F_MULTI, NLM_F_REPLACE, NLM_F_REQUEST, NLMSG_DONE, NLMSG_ERROR, NLMSG_HEADER_LEN,
    RTM_BASE, encode_request, open_route_socket, parse_error_payload, read_u16, read_u32,
    receive_datagram, remaining_time, send_request,
};
use std::io;
use std::os::fd::OwnedFd;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

pub(crate) const MAX_REQUEST_BYTES: usize = 16 * 1024;
const ERROR_CODE_LEN: usize = 4;
const MAX_REPLY_BYTES: usize = NLMSG_HEADER_LEN + ERROR_CODE_LEN + MAX_REQUEST_BYTES;
const CALLER_FLAGS: u16 =
    NLM_F_REQUEST | NLM_F_ACK | NLM_F_REPLACE | NLM_F_EXCL | NLM_F_CREATE | NLM_F_APPEND;

static NEXT_SEQUENCE: AtomicU32 = AtomicU32::new(1);

/// One route-netlink request whose header the caller does not control.
#[derive(Debug)]
pub(crate) struct RouteRequest<'a> {
    kind: u16,
    flags: u16,
    payload: &'a [u8],
}

impl<'a> RouteRequest<'a> {
    pub(crate) fn new(kind: u16, flags: u16, payload: &'a [u8]) -> Result<Self, String> {
        if kind < RTM_BASE {
            return Err(format!(
                "message_type {kind} is a netlink control message, not a route-netlink request"
            ));
        }
        if flags & !CALLER_FLAGS != 0 {
            return Err(format!(
                "flags 0x{flags:x} may only add NLM_F_REPLACE, NLM_F_EXCL, NLM_F_CREATE, or NLM_F_APPEND"
            ));
        }
        if flags & NLM_F_DUMP == NLM_F_DUMP {
            return Err(
                "NLM_F_REPLACE with NLM_F_EXCL selects a dump, whose multipart reply this call rejects; use linux.network_dump"
                    .to_string(),
            );
        }
        if NLMSG_HEADER_LEN + payload.len() > MAX_REQUEST_BYTES {
            return Err(format!("a route-netlink request is limited to {MAX_REQUEST_BYTES} bytes"));
        }
        Ok(Self {
            kind,
            flags: flags | NLM_F_REQUEST | NLM_F_ACK,
            payload,
        })
    }

    pub(crate) fn kind(&self) -> u16 {
        self.kind
    }

    pub(crate) fn flags(&self) -> u16 {
        self.flags
    }

    pub(crate) fn payload(&self) -> &[u8] {
        self.payload
    }

    pub(crate) fn message(&self, sequence: u32, port_id: u32) -> Vec<u8> {
        encode_request(self.kind, self.flags, sequence, port_id, self.payload)
    }
}

/// The kernel's `NLMSG_ERROR` answer to one request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Acknowledgement {
    pub(crate) errno: i32,
    pub(crate) request_type: u16,
    pub(crate) sequence: u32,
    pub(crate) port_id: u32,
}

#[derive(Debug)]
pub(crate) enum RouteRequestError {
    Io(io::Error),
    Malformed(&'static str),
    Multipart(&'static str),
    WrongSequence,
    WrongPort,
    Oversized,
}

impl From<DumpError> for RouteRequestError {
    fn from(error: DumpError) -> Self {
        match error {
            DumpError::Io(error) => Self::Io(error),
            DumpError::Truncated(_) => Self::Oversized,
            DumpError::Malformed(message) | DumpError::Limit(message) => Self::Malformed(message),
            DumpError::Kernel(error) => Self::Io(error),
            DumpError::Interrupted => Self::Malformed("kernel interrupted the netlink reply"),
        }
    }
}

fn timed_out() -> RouteRequestError {
    RouteRequestError::Io(io::Error::from_raw_os_error(libc::ETIMEDOUT))
}

fn next_sequence() -> u32 {
    NEXT_SEQUENCE.fetch_add(1, Ordering::Relaxed).max(1)
}

/// Sends `request` on a fresh route-netlink socket and returns the kernel's
/// acknowledgement. The socket is closed on every path, and `timeout` bounds
/// the whole exchange.
pub(crate) fn route_request(
    request: &RouteRequest<'_>,
    timeout: Duration,
) -> Result<Acknowledgement, RouteRequestError> {
    let deadline = Instant::now().checked_add(timeout).unwrap_or_else(Instant::now);
    let (socket, port_id) = open_route_socket()?;
    let sequence = next_sequence();
    send_request(&socket, &request.message(sequence, port_id))?;
    let mut buffer = vec![0_u8; MAX_REPLY_BYTES];
    acknowledge(request, sequence, port_id, || {
        receive_before(&socket, &mut buffer, deadline)
    })
}

fn receive_before(
    socket: &OwnedFd,
    buffer: &mut [u8],
    deadline: Instant,
) -> Result<Vec<u8>, RouteRequestError> {
    loop {
        let remaining = remaining_time(deadline);
        if remaining.is_zero() {
            return Err(timed_out());
        }
        match receive_datagram(socket, buffer, remaining) {
            Ok(received) => return Ok(buffer[..received].to_vec()),
            Err(DumpError::Io(error)) if error.kind() == io::ErrorKind::WouldBlock => {
                return Err(timed_out());
            }
            Err(DumpError::Io(error)) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error.into()),
        }
    }
}

/// Reads one reply datagram from `next` and validates it as the
/// acknowledgement of `request` sent with `sequence` from `port_id`.
pub(crate) fn acknowledge(
    request: &RouteRequest<'_>,
    sequence: u32,
    port_id: u32,
    next: impl FnOnce() -> Result<Vec<u8>, RouteRequestError>,
) -> Result<Acknowledgement, RouteRequestError> {
    let datagram = next()?;
    if datagram.len() > MAX_REPLY_BYTES {
        return Err(RouteRequestError::Oversized);
    }
    if datagram.len() < NLMSG_HEADER_LEN {
        return Err(RouteRequestError::Malformed("netlink reply header is truncated"));
    }
    let word = |offset| read_u32(&datagram, offset).expect("header bytes were length-checked");
    let length = word(0) as usize;
    if length < NLMSG_HEADER_LEN || length > datagram.len() {
        return Err(RouteRequestError::Malformed("netlink reply length is invalid"));
    }
    let kind = read_u16(&datagram, 4).expect("header bytes were length-checked");
    let flags = read_u16(&datagram, 6).expect("header bytes were length-checked");
    if word(8) != sequence {
        return Err(RouteRequestError::WrongSequence);
    }
    if word(12) != port_id {
        return Err(RouteRequestError::WrongPort);
    }
    if flags & NLM_F_MULTI != 0 {
        return Err(RouteRequestError::Multipart("netlink reply is flagged as multipart"));
    }
    match kind {
        NLMSG_ERROR => {}
        NLMSG_DONE => {
            return Err(RouteRequestError::Multipart(
                "netlink reply is a multipart terminator",
            ));
        }
        _ => {
            return Err(RouteRequestError::Malformed(
                "netlink reply is not an acknowledgement",
            ));
        }
    }
    let message_end = length.next_multiple_of(4).min(datagram.len());
    let trailing = &datagram[message_end..];
    if trailing.iter().any(|byte| *byte != 0) {
        return Err(if trailing.len() >= NLMSG_HEADER_LEN {
            RouteRequestError::Multipart("netlink reply holds more than one message")
        } else {
            RouteRequestError::Malformed("netlink reply is followed by stray bytes")
        });
    }
    if flags & NLM_F_ACK_TLVS != 0 {
        return Err(RouteRequestError::Malformed(
            "netlink reply carries extended acknowledgement that was not requested",
        ));
    }
    let acknowledged = parse_error_payload(&datagram[NLMSG_HEADER_LEN..length])
        .map_err(RouteRequestError::Malformed)?;
    if acknowledged.request_sequence != sequence {
        return Err(RouteRequestError::WrongSequence);
    }
    if acknowledged.request_port != port_id {
        return Err(RouteRequestError::WrongPort);
    }
    if acknowledged.request_type != request.kind() {
        return Err(RouteRequestError::Malformed(
            "netlink acknowledgment names a different request type",
        ));
    }
    Ok(Acknowledgement {
        errno: acknowledged.errno().map_err(RouteRequestError::Malformed)?,
        request_type: acknowledged.request_type,
        sequence,
        port_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustix::net::{AddressFamily, SendFlags, SocketFlags, SocketType, send, socketpair};

    #[test]
    fn a_quiet_socket_runs_into_the_deadline_with_etimedout() {
        let (socket, _) = open_route_socket().expect("route socket");
        let mut buffer = vec![0_u8; MAX_REPLY_BYTES];
        let started = Instant::now();
        let deadline = started + Duration::from_millis(60);
        let error = receive_before(&socket, &mut buffer, deadline).expect_err("nothing was sent");
        let RouteRequestError::Io(error) = error else {
            panic!("a deadline is an operating-system timeout, got {error:?}");
        };
        assert_eq!(error.raw_os_error(), Some(libc::ETIMEDOUT));
        assert!(started.elapsed() >= Duration::from_millis(50));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn a_datagram_beyond_the_reply_limit_is_oversized_before_it_is_parsed() {
        let (receiver, sender) = socketpair(
            AddressFamily::UNIX,
            SocketType::DGRAM,
            SocketFlags::CLOEXEC,
            None,
        )
        .expect("datagram pair");
        send(&sender, &vec![0_u8; MAX_REPLY_BYTES + 1], SendFlags::empty()).expect("send");
        let mut buffer = vec![0_u8; MAX_REPLY_BYTES];
        let deadline = Instant::now() + Duration::from_secs(5);
        let error = receive_before(&receiver, &mut buffer, deadline).expect_err("oversized");
        assert!(matches!(error, RouteRequestError::Oversized), "{error:?}");
    }
}
